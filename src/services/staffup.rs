use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    errors::{AppError, AppResult},
    models::{
        ControllerEventItem, ControllerLifecyclePayload, ControllerPositionEventPayload,
        OpenActivationState, StaffupCursorState, StaffupOfflineEmbed, StaffupOnlineEmbed,
    },
    state::AppState,
};

const ERROR_BACKOFF_SECS: u64 = 30;
const ERROR_BACKOFF_MAX_SECS: u64 = 120;
const BOOTSTRAP_FETCH_LIMIT: u64 = 500;

pub async fn start_staffup_worker(state: AppState) {
    if !state.config.staffup_enabled {
        tracing::info!("staffup worker disabled by configuration");
        let mut readiness = state.runtime.readiness.write().await;
        readiness.staffup.enabled = false;
        return;
    }

    tracing::info!(
        poll_interval_secs = state.config.staffup_poll_interval_secs,
        batch_size = state.config.staffup_batch_size,
        environment = %state.config.staffup_environment,
        artcc_id = %state.config.staffup_artcc_id,
        "starting staffup worker"
    );

    if let Err(error) = load_cursor_state(&state).await {
        tracing::error!(?error, "failed to load staffup cursor state");
        let mut readiness = state.runtime.readiness.write().await;
        readiness.staffup.enabled = true;
        readiness.staffup.last_error = Some(error.to_string());
    }

    if let Err(error) = initialize_cursor_if_missing(&state).await {
        tracing::error!(?error, "failed to initialize staffup cursor state");
        let mut readiness = state.runtime.readiness.write().await;
        readiness.staffup.enabled = true;
        readiness.staffup.last_error = Some(error.to_string());
    }

    tokio::spawn(async move {
        let mut backoff = ERROR_BACKOFF_SECS;
        loop {
            let outcome = process_staffup_cycle(&state).await;
            match outcome {
                Ok(processed_any) => {
                    backoff = ERROR_BACKOFF_SECS;
                    if processed_any {
                        continue;
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(
                        state.config.staffup_poll_interval_secs,
                    ))
                    .await;
                }
                Err(error) => {
                    tracing::error!(?error, "staffup worker cycle failed");
                    let mut readiness = state.runtime.readiness.write().await;
                    readiness.staffup.enabled = true;
                    readiness.staffup.last_error = Some(error.to_string());
                    tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                    backoff = (backoff * 2).min(ERROR_BACKOFF_MAX_SECS);
                }
            }
        }
    });
}

async fn process_staffup_cycle(state: &AppState) -> AppResult<bool> {
    // When the staffup feature is disabled we still drain the controller-event
    // backlog below (advancing the cursor without posting) so re-enabling later
    // doesn't replay a burst of stale online/offline transitions.
    let enabled = state.runtime.feature_enabled("staffup").await;
    if enabled {
        let bundle = current_bundle(state).await?;
        let channel_ready = bundle.resolve_staffup_targets().is_ok();
        let mut readiness = state.runtime.readiness.write().await;
        readiness.staffup.enabled = true;
        readiness.staffup.channel_ready = channel_ready;
        readiness.staffup.last_polled_at = Some(Utc::now());
        if !channel_ready {
            readiness.staffup.last_error =
                Some("osmium discord config bundle does not define a `staffup` channel".into());
            return Ok(false);
        }
    }

    let after_id = state.runtime.staffup_cursor.read().await.last_event_id;
    tracing::info!(after_id, "polling staffup controller events");
    let response = state
        .osmium
        .fetch_controller_events(
            after_id,
            state.config.staffup_batch_size,
            &state.config.staffup_environment,
        )
        .await?;
    tracing::info!(
        after_id,
        fetched = response.events.len(),
        "staffup controller events fetched"
    );

    if response.events.is_empty() {
        let mut readiness = state.runtime.readiness.write().await;
        readiness.staffup.last_error = None;
        tracing::debug!(after_id, processed = false, "staffup cycle found no events");
        return Ok(false);
    }

    let bundle = current_bundle(state).await?;
    let mut cursor = state.runtime.staffup_cursor.read().await.clone();

    for event in &response.events {
        if enabled {
            process_event(state, &bundle, &mut cursor, event).await?;
        }
        cursor.last_event_id = event.id;
    }

    save_cursor_state(state, &cursor).await?;
    {
        let mut guard = state.runtime.staffup_cursor.write().await;
        *guard = cursor.clone();
    }
    let mut readiness = state.runtime.readiness.write().await;
    readiness.staffup.last_success_at = Some(Utc::now());
    readiness.staffup.last_event_id = Some(cursor.last_event_id);
    readiness.staffup.last_error = None;
    tracing::info!(
        after_id,
        processed = true,
        last_event_id = cursor.last_event_id,
        open_activations = cursor.open_activations.len(),
        "staffup cycle completed"
    );

    Ok(true)
}

#[tracing::instrument(
    skip(state, bundle, cursor, event),
    fields(
        event_id = event.id,
        event_type = %event.event_type,
        environment = %event.environment
    )
)]
async fn process_event(
    state: &AppState,
    bundle: &crate::models::DiscordConfigBundle,
    cursor: &mut StaffupCursorState,
    event: &ControllerEventItem,
) -> AppResult<()> {
    if event.environment != state.config.staffup_environment {
        tracing::debug!(
            configured_environment = %state.config.staffup_environment,
            processed = false,
            "skipping staffup event from different environment"
        );
        return Ok(());
    }

    if event.event_type != "position_activated" && event.event_type != "position_deactivated" {
        tracing::debug!(processed = false, "skipping non-position lifecycle event");
        return Ok(());
    }

    let payload = parse_position_payload(event)?;
    tracing::debug!(
        activation_id = %payload.activation_id,
        cid = payload.cid,
        role_id = payload.position_id.as_str(),
        "parsed staffup lifecycle payload"
    );

    if !qualifies_for_staffup(state, &payload) {
        tracing::debug!(
            activation_id = %payload.activation_id,
            cid = payload.cid,
            processed = false,
            "skipping event that does not qualify for staffup"
        );
        return Ok(());
    }

    match event.event_type.as_str() {
        "position_activated" => {
            let online = StaffupOnlineEmbed {
                callsign: select_callsign(&payload),
                name: display_name(&payload),
                frequency: format_frequency(payload.frequency),
                logon_unix: payload.occurred_at.timestamp(),
            };
            let delivered = state.delivery.send_staffup_online(bundle, &online).await?;
            cursor.open_activations.insert(
                payload.activation_id.clone(),
                OpenActivationState {
                    session_id: payload.session_id.clone(),
                    callsign: online.callsign,
                    real_name: payload.real_name.clone(),
                    user_rating: payload.user_rating.clone(),
                    requested_rating: payload.requested_rating.clone(),
                    frequency: payload.frequency,
                    logon_time: payload.occurred_at,
                    event_id: event.id,
                },
            );
            tracing::info!(
                activation_id = %payload.activation_id,
                cid = payload.cid,
                delivered,
                open_activations = cursor.open_activations.len(),
                processed = true,
                "staffup online notification delivered"
            );
        }
        "position_deactivated" => {
            let activation = cursor.open_activations.remove(&payload.activation_id);
            let logon_unix = activation.as_ref().map(|item| item.logon_time.timestamp());
            let duration = activation
                .as_ref()
                .map(|item| humanize_duration(item.logon_time, payload.occurred_at));
            let offline = StaffupOfflineEmbed {
                callsign: activation
                    .as_ref()
                    .map(|item| item.callsign.clone())
                    .unwrap_or_else(|| select_callsign(&payload)),
                name: activation
                    .as_ref()
                    .map(|item| {
                        display_name_from_parts(
                            item.real_name.as_deref(),
                            display_rating_from_parts(
                                item.user_rating.as_deref(),
                                item.requested_rating.as_deref(),
                            ),
                            payload.cid,
                        )
                    })
                    .unwrap_or_else(|| display_name(&payload)),
                frequency: activation
                    .as_ref()
                    .map(|item| format_frequency(item.frequency))
                    .unwrap_or_else(|| format_frequency(payload.frequency)),
                logon_unix,
                logoff_unix: payload.occurred_at.timestamp(),
                duration,
            };
            let delivered = state
                .delivery
                .send_staffup_offline(bundle, &offline)
                .await?;
            tracing::info!(
                activation_id = %payload.activation_id,
                cid = payload.cid,
                delivered,
                open_activations = cursor.open_activations.len(),
                processed = true,
                "staffup offline notification delivered"
            );
        }
        _ => {}
    }

    Ok(())
}

fn parse_position_payload(
    event: &ControllerEventItem,
) -> AppResult<ControllerPositionEventPayload> {
    let payload: ControllerLifecyclePayload = serde_json::from_value(event.payload.clone())
        .map_err(|error| {
            AppError::BadRequest(format!(
                "failed to parse controller position event payload for event {}: {error}",
                event.id
            ))
        })?;

    match payload {
        ControllerLifecyclePayload::PositionActivated(payload)
        | ControllerLifecyclePayload::PositionDeactivated(payload) => Ok(payload),
        _ => Err(AppError::BadRequest(format!(
            "event {} payload did not contain a position lifecycle payload",
            event.id
        ))),
    }
}

fn qualifies_for_staffup(state: &AppState, payload: &ControllerPositionEventPayload) -> bool {
    payload.environment == state.config.staffup_environment
        && payload.is_primary
        && payload.artcc_id == state.config.staffup_artcc_id
}

fn select_callsign(payload: &ControllerPositionEventPayload) -> String {
    payload
        .radio_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            payload
                .default_callsign
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| {
            payload
                .position_name
                .as_deref()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or("Unknown Position")
        .to_string()
}

fn display_name(payload: &ControllerPositionEventPayload) -> String {
    display_name_from_parts(
        payload.real_name.as_deref(),
        display_rating(payload),
        payload.cid,
    )
}

fn display_rating(payload: &ControllerPositionEventPayload) -> Option<&str> {
    display_rating_from_parts(
        payload.user_rating.as_deref(),
        payload.requested_rating.as_deref(),
    )
}

fn display_rating_from_parts<'a>(
    user_rating: Option<&'a str>,
    requested_rating: Option<&'a str>,
) -> Option<&'a str> {
    user_rating
        .filter(|value| !value.trim().is_empty())
        .or_else(|| requested_rating.filter(|value| !value.trim().is_empty()))
}

fn display_name_from_parts(real_name: Option<&str>, rating: Option<&str>, cid: i64) -> String {
    match (
        real_name.filter(|value| !value.trim().is_empty()),
        rating.filter(|value| !value.trim().is_empty()),
    ) {
        (Some(real_name), Some(rating)) => format!("{real_name} ({rating})"),
        (Some(real_name), None) => real_name.to_string(),
        (None, Some(rating)) => format!("CID {cid} ({rating})"),
        (None, None) => format!("CID {cid}"),
    }
}

fn format_frequency(value: Option<f64>) -> String {
    value
        .filter(|value| *value > 0.0)
        .map(normalize_frequency)
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "Unknown".to_string())
}

fn normalize_frequency(value: f64) -> f64 {
    if value >= 1_000_000.0 {
        value / 1_000_000.0
    } else {
        value
    }
}

fn humanize_duration(start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    let seconds = (end - start).num_seconds().max(0);
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;

    if hours == 0 {
        format!("{minutes}m")
    } else if remaining_minutes == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h {remaining_minutes}m")
    }
}

async fn current_bundle(state: &AppState) -> AppResult<crate::models::DiscordConfigBundle> {
    state
        .runtime
        .config_bundle
        .read()
        .await
        .clone()
        .ok_or_else(|| AppError::Config("discord config bundle is not loaded".into()))
}

async fn load_cursor_state(state: &AppState) -> AppResult<()> {
    let path = &state.config.staffup_cursor_path;
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    tokio::fs::create_dir_all(parent).await?;
    if !path.exists() {
        return Ok(());
    }

    let bytes = tokio::fs::read(path).await?;
    let cursor = deserialize_cursor_state(&bytes)?;
    {
        let mut guard = state.runtime.staffup_cursor.write().await;
        *guard = cursor.clone();
    }
    let mut readiness = state.runtime.readiness.write().await;
    readiness.staffup.last_event_id = Some(cursor.last_event_id);
    tracing::info!(
        last_event_id = cursor.last_event_id,
        open_activations = cursor.open_activations.len(),
        "loaded staffup cursor state"
    );
    Ok(())
}

async fn initialize_cursor_if_missing(state: &AppState) -> AppResult<()> {
    let path = &state.config.staffup_cursor_path;
    if path.exists() {
        let cursor = state.runtime.staffup_cursor.read().await.clone();
        if cursor.last_event_id > 0 || !cursor.open_activations.is_empty() {
            tracing::debug!(
                last_event_id = cursor.last_event_id,
                open_activations = cursor.open_activations.len(),
                "staffup cursor already initialized"
            );
            return Ok(());
        }
    }

    let response = state
        .osmium
        .fetch_controller_events(0, BOOTSTRAP_FETCH_LIMIT, &state.config.staffup_environment)
        .await?;
    let last_event_id = response
        .events
        .iter()
        .map(|event| event.id)
        .max()
        .unwrap_or(0);
    let cursor = StaffupCursorState {
        last_event_id,
        open_activations: Default::default(),
    };
    save_cursor_state(state, &cursor).await?;
    {
        let mut guard = state.runtime.staffup_cursor.write().await;
        *guard = cursor.clone();
    }
    let mut readiness = state.runtime.readiness.write().await;
    readiness.staffup.last_event_id = Some(cursor.last_event_id);
    readiness.staffup.last_error = None;
    tracing::info!(
        last_event_id = cursor.last_event_id,
        bootstrap_events = response.events.len(),
        "initialized staffup cursor state"
    );
    Ok(())
}

async fn save_cursor_state(state: &AppState, cursor: &StaffupCursorState) -> AppResult<()> {
    let path = &state.config.staffup_cursor_path;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp_path = tmp_path(path);
    let body = serde_json::to_vec_pretty(cursor)?;
    tokio::fs::write(&tmp_path, body).await?;
    tokio::fs::rename(&tmp_path, path).await?;
    Ok(())
}

fn tmp_path(path: &Path) -> std::path::PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("staffup_cursor.json");
    path.with_file_name(format!("{file_name}.tmp"))
}

fn deserialize_cursor_state(bytes: &[u8]) -> AppResult<StaffupCursorState> {
    match serde_json::from_slice(bytes) {
        Ok(cursor) => Ok(cursor),
        Err(primary_error) => {
            let legacy: LegacyStaffupCursorState = serde_json::from_slice(bytes).map_err(|_| {
                AppError::BadRequest(format!(
                    "failed to parse staffup cursor state: {primary_error}"
                ))
            })?;
            Ok(legacy.into_current())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct LegacyStaffupCursorState {
    last_event_id: i64,
    #[serde(default)]
    open_activations: std::collections::BTreeMap<String, LegacyOpenActivationState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct LegacyOpenActivationState {
    session_id: String,
    callsign: String,
    real_name: Option<String>,
    role: Option<String>,
    frequency: Option<f64>,
    logon_time: DateTime<Utc>,
    event_id: i64,
}

impl LegacyStaffupCursorState {
    fn into_current(self) -> StaffupCursorState {
        StaffupCursorState {
            last_event_id: self.last_event_id,
            open_activations: self
                .open_activations
                .into_iter()
                .map(|(key, value)| {
                    (
                        key,
                        OpenActivationState {
                            session_id: value.session_id,
                            callsign: value.callsign,
                            real_name: value.real_name,
                            user_rating: value.role,
                            requested_rating: None,
                            frequency: value.frequency,
                            logon_time: value.logon_time,
                            event_id: value.event_id,
                        },
                    )
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::models::ControllerPositionEventPayload;

    use super::{
        display_name, display_rating, format_frequency, humanize_duration, normalize_frequency,
        qualifies_for_staffup, select_callsign,
    };

    fn sample_payload() -> ControllerPositionEventPayload {
        ControllerPositionEventPayload {
            environment: "live".into(),
            artcc_id: "ZDC".into(),
            cid: 1234567,
            user_id: Some("user-1".into()),
            session_id: "session-1".into(),
            activation_id: "activation-1".into(),
            occurred_at: chrono::Utc
                .with_ymd_and_hms(2026, 5, 10, 19, 39, 0)
                .unwrap(),
            real_name: Some("Aiden Huang".into()),
            role: Some("Controller".into()),
            user_rating: Some("Student1".into()),
            requested_rating: None,
            position_id: "position-1".into(),
            facility_id: Some("RDU".into()),
            facility_name: Some("RDU".into()),
            position_name: Some("RDU Ground".into()),
            position_type: Some("ground".into()),
            radio_name: Some("RDU_E1_GND".into()),
            default_callsign: Some("RDU_GND".into()),
            frequency: Some(121_900_000.0),
            is_primary: true,
        }
    }

    #[test]
    fn selects_callsign_in_priority_order() {
        let payload = sample_payload();
        assert_eq!(select_callsign(&payload), "RDU_E1_GND");
    }

    #[test]
    fn formats_frequency_to_three_decimals() {
        assert_eq!(normalize_frequency(121_900_000.0), 121.9);
        assert_eq!(format_frequency(Some(121_900_000.0)), "121.900");
        assert_eq!(format_frequency(Some(124_950_000.0)), "124.950");
        assert_eq!(format_frequency(Some(121.9)), "121.900");
        assert_eq!(format_frequency(None), "Unknown");
    }

    #[test]
    fn formats_duration_humanely() {
        let start = chrono::Utc
            .with_ymd_and_hms(2026, 5, 10, 19, 39, 0)
            .unwrap();
        let end = chrono::Utc
            .with_ymd_and_hms(2026, 5, 10, 20, 42, 0)
            .unwrap();
        assert_eq!(humanize_duration(start, end), "1h 3m");
    }

    #[test]
    fn formats_display_name() {
        let payload = sample_payload();
        assert_eq!(display_name(&payload), "Aiden Huang (Student1)");
    }

    #[test]
    fn prefers_user_rating_then_requested_rating() {
        let mut payload = sample_payload();
        assert_eq!(display_rating(&payload), Some("Student1"));
        payload.user_rating = None;
        payload.requested_rating = Some("Instructor1".into());
        assert_eq!(display_rating(&payload), Some("Instructor1"));
        payload.requested_rating = None;
        assert_eq!(display_rating(&payload), None);
    }

    #[test]
    fn filters_non_primary_or_wrong_artcc() {
        let mut payload = sample_payload();
        let state = crate::state::AppState {
            config: crate::config::Config {
                discord_token: "x".into(),
                discord_application_id: 1,
                bind_addr: "127.0.0.1:3010".parse().unwrap(),
                bot_api_shared_key: "x".into(),
                osmium_base_url: "http://127.0.0.1:3000".into(),
                osmium_bearer_token: "x".into(),
                command_guild_ids: Vec::new(),
                operator_role_ids: Vec::new(),
                operator_user_ids: Vec::new(),
                audit_logging_enabled: true,
                audit_include_bot_events: false,
                audit_fetch_audit_logs: true,
                audit_max_field_chars: 900,
                role_sync_on_join: true,
                role_sync_periodic_enabled: false,
                role_sync_interval_mins: 60,
                staffup_enabled: true,
                staffup_poll_interval_secs: 10,
                staffup_batch_size: 100,
                staffup_cursor_path: std::path::PathBuf::from("data/staffup_cursor.json"),
                staffup_environment: "live".into(),
                staffup_artcc_id: "ZDC".into(),
                impromptu_selector_state_path: std::path::PathBuf::from(
                    "data/impromptu_selector_message.json",
                ),
                break_board_state_path: std::path::PathBuf::from("data/break_board_messages.json"),
                break_board_requests_path: std::path::PathBuf::from(
                    "data/break_board_requests.json",
                ),
            },
            osmium: crate::osmium::OsmiumClient::new("http://127.0.0.1:3000".into(), "x").unwrap(),
            runtime: std::sync::Arc::new(crate::state::RuntimeState::new()),
            discord_http: std::sync::Arc::new(serenity::http::Http::new("token")),
            delivery: std::sync::Arc::new(crate::services::SerenityDiscordService::new(
                std::sync::Arc::new(serenity::http::Http::new("token")),
                "https://example.com/logo.png".into(),
            )),
        };
        assert!(qualifies_for_staffup(&state, &payload));
        payload.is_primary = false;
        assert!(!qualifies_for_staffup(&state, &payload));
        payload.is_primary = true;
        payload.artcc_id = "ZZZ".into();
        assert!(!qualifies_for_staffup(&state, &payload));
    }
}
