use axum::{
    Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
};
use serenity::all::GuildId;

use crate::{
    errors::{AppError, AppResult},
    models::{
        AnnouncementPayload, DeliveryResponse, EventPositionPostingPayload, GuildDiscoveryResponse,
        GuildListResponse, HealthResponse, ImpromptuFinalizePayload, ImpromptuOfferPostPayload,
        ImpromptuOfferPostedResponse, ReadyResponse, ScheduledEventPayload,
    },
    services::{
        RoleSyncRequest, create_event_scheduled_events, discover_guild, finalize_impromptu_offer,
        format_role_sync_outcome, list_guilds, post_impromptu_offer, sync_roles_from_request,
    },
    state::AppState,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/announcement", post(announcement))
        .route("/event_position_posting", post(event_position_posting))
        .route("/scheduled_event", post(scheduled_event))
        .route("/impromptu_offer", post(impromptu_offer))
        .route("/impromptu_offer/finalize", post(impromptu_offer_finalize))
        .route("/role_sync", post(role_sync))
        .route("/guilds", get(guilds))
        .route("/guilds/{guild_id}/discovery", get(guild_discovery))
        .with_state(state)
}

async fn health() -> axum::Json<HealthResponse> {
    axum::Json(HealthResponse {
        service: "vzdc-discord-bot",
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn ready(State(state): State<AppState>) -> axum::Json<ReadyResponse> {
    let readiness = state.runtime.readiness.read().await.clone();

    axum::Json(ReadyResponse {
        ready: readiness.discord_connected
            && readiness.config_loaded
            && (!readiness.staffup.enabled
                || (readiness.staffup.channel_ready && readiness.staffup.last_error.is_none()))
            && (!readiness.audit.enabled
                || (readiness.audit.channel_ready && readiness.audit.last_error.is_none())),
        discord_connected: readiness.discord_connected,
        config_loaded: readiness.config_loaded,
        last_config_sync_at: readiness.last_config_sync_at,
        last_config_error: readiness.last_config_error,
        staffup: readiness.staffup,
        audit: readiness.audit,
    })
}

async fn announcement(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(payload): axum::Json<AnnouncementPayload>,
) -> AppResult<axum::Json<DeliveryResponse>> {
    let span = tracing::info_span!(
        "http_announcement",
        route = "/announcement",
        requested_by_cid = payload.requested_by_cid
    );
    let _entered = span.enter();
    tracing::info!("received announcement delivery request");

    authorize(&state, &headers, "/announcement")?;
    if let Some(disabled) = feature_disabled(&state, "announcements", "announcements").await {
        return Ok(disabled);
    }
    let body = serde_json::to_string(&payload)?;
    let deduplicated = dedupe(&state, "announcement", &body).await;
    tracing::info!(deduplicated, "announcement dedupe evaluated");
    if deduplicated {
        tracing::info!(deduplicated = true, "duplicate announcement skipped");
        return Ok(axum::Json(DeliveryResponse {
            ok: true,
            message: "duplicate announcement skipped".into(),
            deduplicated: true,
        }));
    }

    let bundle = current_bundle(&state).await?;
    tracing::info!("sending announcement delivery");
    let delivered = state.delivery.send_announcement(&bundle, &payload).await?;
    tracing::info!(
        delivered,
        requested_by_cid = payload.requested_by_cid,
        "announcement delivered"
    );

    Ok(axum::Json(DeliveryResponse {
        ok: true,
        message: format!("announcement delivered to {delivered} channel(s)"),
        deduplicated: false,
    }))
}

async fn event_position_posting(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(payload): axum::Json<EventPositionPostingPayload>,
) -> AppResult<axum::Json<DeliveryResponse>> {
    let span = tracing::info_span!(
        "http_event_position_posting",
        route = "/event_position_posting",
        event_id = %payload.event_id,
        requested_by_cid = payload.requested_by_cid
    );
    let _entered = span.enter();
    tracing::info!("received event position posting request");

    authorize(&state, &headers, "/event_position_posting")?;
    if let Some(disabled) = feature_disabled(&state, "event_postings", "event postings").await {
        return Ok(disabled);
    }
    let body = serde_json::to_string(&payload)?;
    let deduplicated = dedupe(&state, "event_position_posting", &body).await;
    tracing::info!(deduplicated, "event position posting dedupe evaluated");
    if deduplicated {
        tracing::info!(
            deduplicated = true,
            "duplicate event position posting skipped"
        );
        return Ok(axum::Json(DeliveryResponse {
            ok: true,
            message: "duplicate event position posting skipped".into(),
            deduplicated: true,
        }));
    }

    let bundle = current_bundle(&state).await?;
    tracing::info!("fetching event position posting dependencies");
    let event = state.osmium.fetch_event(&payload.event_id).await?;
    let positions = state
        .osmium
        .fetch_event_positions(&payload.event_id)
        .await?;
    tracing::info!(
        positions = positions.items.len(),
        "sending event position posting"
    );
    let delivered = state
        .delivery
        .send_event_position_posting(&bundle, &payload, &event, &positions.items)
        .await?;

    tracing::info!(
        delivered,
        event_id = %payload.event_id,
        requested_by_cid = payload.requested_by_cid,
        "event position posting delivered"
    );

    Ok(axum::Json(DeliveryResponse {
        ok: true,
        message: format!("event position posting delivered to {delivered} channel(s)"),
        deduplicated: false,
    }))
}

async fn scheduled_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(payload): axum::Json<ScheduledEventPayload>,
) -> AppResult<axum::Json<DeliveryResponse>> {
    let span = tracing::info_span!(
        "http_scheduled_event",
        route = "/scheduled_event",
        event_id = %payload.event_id,
        requested_by_cid = payload.requested_by_cid
    );
    let _entered = span.enter();
    tracing::info!("received discord scheduled event request");

    authorize(&state, &headers, "/scheduled_event")?;
    if let Some(disabled) = feature_disabled(&state, "scheduled_events", "scheduled events").await {
        return Ok(disabled);
    }
    let bundle = current_bundle(&state).await?;
    let event = state.osmium.fetch_event(&payload.event_id).await?;
    let location = payload
        .location
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("vatsim.net");

    let created =
        create_event_scheduled_events(&state.discord_http, &bundle, &event, location).await?;

    Ok(axum::Json(DeliveryResponse {
        ok: true,
        message: format!("discord scheduled event created in {created} guild(s)"),
        deduplicated: false,
    }))
}

async fn impromptu_offer(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(payload): axum::Json<ImpromptuOfferPostPayload>,
) -> AppResult<axum::Json<ImpromptuOfferPostedResponse>> {
    authorize(&state, &headers, "/impromptu_offer")?;
    if !state.runtime.feature_enabled("impromptu_offers").await {
        // Treat a disabled feature as delivered (200) so the outbound job is not
        // retried indefinitely, matching the other feature-gated routes.
        tracing::info!(
            feature = "impromptu_offers",
            "skipping request; feature disabled"
        );
        return Ok(axum::Json(ImpromptuOfferPostedResponse {
            channel_id: String::new(),
            message_id: String::new(),
        }));
    }
    let bundle = current_bundle(&state).await?;
    let posted = post_impromptu_offer(&state.discord_http, &bundle, &payload).await?;
    tracing::info!(offer_id = %payload.offer_id, "posted impromptu offer");
    Ok(axum::Json(posted))
}

async fn impromptu_offer_finalize(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(payload): axum::Json<ImpromptuFinalizePayload>,
) -> AppResult<axum::Json<DeliveryResponse>> {
    authorize(&state, &headers, "/impromptu_offer/finalize")?;
    finalize_impromptu_offer(&state.discord_http, &payload).await;
    Ok(axum::Json(DeliveryResponse {
        ok: true,
        message: "impromptu offer finalized".into(),
        deduplicated: false,
    }))
}

async fn role_sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(payload): axum::Json<RoleSyncRequest>,
) -> AppResult<axum::Json<DeliveryResponse>> {
    authorize(&state, &headers, "/role_sync")?;
    let outcome = sync_roles_from_request(&state, payload).await?;

    Ok(axum::Json(DeliveryResponse {
        ok: true,
        message: format_role_sync_outcome(&outcome),
        deduplicated: false,
    }))
}

async fn guilds(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<axum::Json<GuildListResponse>> {
    authorize(&state, &headers, "/guilds")?;
    let response = list_guilds(&state.discord_http).await?;
    tracing::info!(count = response.guilds.len(), "listed discoverable guilds");
    Ok(axum::Json(response))
}

async fn guild_discovery(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<u64>,
) -> AppResult<axum::Json<GuildDiscoveryResponse>> {
    authorize(&state, &headers, "/guilds/{guild_id}/discovery")?;
    let response = discover_guild(&state.discord_http, GuildId::new(guild_id)).await?;
    tracing::info!(
        guild_id,
        channels = response.channels.len(),
        categories = response.categories.len(),
        roles = response.roles.len(),
        "completed guild discovery"
    );
    Ok(axum::Json(response))
}

/// Returns a success "feature disabled" response (so the outbound job is treated
/// as delivered, not retried) when the given bot feature is toggled off.
async fn feature_disabled(
    state: &AppState,
    key: &str,
    label: &str,
) -> Option<axum::Json<DeliveryResponse>> {
    if state.runtime.feature_enabled(key).await {
        return None;
    }
    tracing::info!(feature = key, "skipping request; feature disabled");
    Some(axum::Json(DeliveryResponse {
        ok: true,
        message: format!("{label} feature is disabled"),
        deduplicated: false,
    }))
}

fn authorize(state: &AppState, headers: &HeaderMap, route: &str) -> AppResult<()> {
    let header_key = headers
        .get("X-API-Key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim);
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim);

    if header_key == Some(state.config.bot_api_shared_key.as_str())
        || bearer == Some(state.config.bot_api_shared_key.as_str())
    {
        tracing::info!(
            route,
            auth_mode = if header_key.is_some() {
                "x-api-key"
            } else {
                "bearer"
            },
            "authorized http request"
        );
        Ok(())
    } else {
        tracing::warn!(
            route,
            has_api_key = header_key.is_some(),
            has_bearer = bearer.is_some(),
            "unauthorized http request"
        );
        Err(AppError::Unauthorized)
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

async fn dedupe(state: &AppState, route: &str, body: &str) -> bool {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(body.as_bytes());
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let key = format!("{route}:{hex}");
    state.runtime.mark_delivery_seen(key).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    use crate::{
        config::Config,
        models::{AnnouncementPayload, AuditEvent, DiscordConfigBundle},
        osmium::OsmiumClient,
        services::DiscordDelivery,
        state::{AppState, RuntimeState},
    };

    struct MockDelivery;

    #[async_trait]
    impl DiscordDelivery for MockDelivery {
        async fn send_announcement(
            &self,
            _bundle: &DiscordConfigBundle,
            _payload: &AnnouncementPayload,
        ) -> crate::errors::AppResult<usize> {
            Ok(1)
        }

        async fn send_event_position_posting(
            &self,
            _bundle: &DiscordConfigBundle,
            _payload: &crate::models::EventPositionPostingPayload,
            _event: &crate::models::Event,
            _positions: &[crate::models::EventPosition],
        ) -> crate::errors::AppResult<usize> {
            Ok(1)
        }

        async fn register_commands(
            &self,
            _guild_ids: &[serenity::all::GuildId],
        ) -> crate::errors::AppResult<usize> {
            Ok(0)
        }

        async fn validate_guilds(
            &self,
            _bundle: &DiscordConfigBundle,
        ) -> crate::errors::AppResult<crate::models::GuildValidationReport> {
            Ok(crate::models::GuildValidationReport {
                valid: true,
                entries: Vec::new(),
            })
        }

        async fn send_staffup_online(
            &self,
            _bundle: &DiscordConfigBundle,
            _payload: &crate::models::StaffupOnlineEmbed,
        ) -> crate::errors::AppResult<usize> {
            Ok(1)
        }

        async fn send_staffup_offline(
            &self,
            _bundle: &DiscordConfigBundle,
            _payload: &crate::models::StaffupOfflineEmbed,
        ) -> crate::errors::AppResult<usize> {
            Ok(1)
        }

        async fn send_audit_event(
            &self,
            _bundle: &DiscordConfigBundle,
            _event: &AuditEvent,
        ) -> crate::errors::AppResult<usize> {
            Ok(1)
        }

        async fn impromptu_selector_message_exists(
            &self,
            _state: &crate::models::ImpromptuSelectorMessageState,
        ) -> crate::errors::AppResult<bool> {
            Ok(false)
        }

        async fn create_impromptu_selector_message(
            &self,
            target: &crate::models::ResolvedChannelTarget,
            _roles: &[crate::models::ResolvedRoleTarget],
        ) -> crate::errors::AppResult<crate::models::ImpromptuSelectorMessageState> {
            Ok(crate::models::ImpromptuSelectorMessageState {
                guild_id: target.guild_id,
                channel_id: target.channel_id,
                message_id: 1,
            })
        }

        async fn refresh_impromptu_selector_message(
            &self,
            _state: &crate::models::ImpromptuSelectorMessageState,
            _roles: &[crate::models::ResolvedRoleTarget],
        ) -> crate::errors::AppResult<()> {
            Ok(())
        }

        async fn break_board_messages_exist(
            &self,
            _state: &crate::models::BreakBoardMessageState,
        ) -> crate::errors::AppResult<bool> {
            Ok(true)
        }

        async fn create_break_board_messages(
            &self,
            target: &crate::models::ResolvedChannelTarget,
            _roles: &[crate::models::ResolvedRoleTarget],
        ) -> crate::errors::AppResult<crate::models::BreakBoardMessageState> {
            Ok(crate::models::BreakBoardMessageState {
                guild_id: target.guild_id,
                channel_id: target.channel_id,
                preference_message_id: 1,
                request_message_id: 2,
            })
        }

        async fn refresh_break_board_messages(
            &self,
            _state: &crate::models::BreakBoardMessageState,
            _roles: &[crate::models::ResolvedRoleTarget],
        ) -> crate::errors::AppResult<()> {
            Ok(())
        }

        async fn create_break_board_request_message(
            &self,
            _request: &crate::models::BreakBoardRequestState,
        ) -> crate::errors::AppResult<u64> {
            Ok(10)
        }

        async fn mark_break_board_request_claimed(
            &self,
            _request: &crate::models::BreakBoardRequestState,
        ) -> crate::errors::AppResult<()> {
            Ok(())
        }

        async fn create_break_board_claim_message(
            &self,
            _request: &crate::models::BreakBoardRequestState,
        ) -> crate::errors::AppResult<u64> {
            Ok(11)
        }

        async fn delete_break_board_message(
            &self,
            _channel_id: u64,
            _message_id: u64,
        ) -> crate::errors::AppResult<()> {
            Ok(())
        }
    }

    async fn test_state() -> AppState {
        let runtime = Arc::new(RuntimeState::new());
        let bundle = DiscordConfigBundle::default();
        runtime.config_bundle.write().await.replace(bundle);
        runtime.readiness.write().await.config_loaded = true;

        AppState {
            config: Config {
                discord_token: "token".into(),
                discord_application_id: 1,
                bind_addr: "127.0.0.1:3010".parse().unwrap(),
                bot_api_shared_key: "secret".into(),
                osmium_base_url: "http://127.0.0.1:3000".into(),
                osmium_bearer_token: "bearer".into(),
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
                staffup_enabled: false,
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
            osmium: OsmiumClient::new("http://127.0.0.1:3000".into(), "bearer").unwrap(),
            runtime,
            discord_http: Arc::new(serenity::http::Http::new("token")),
            delivery: Arc::new(MockDelivery),
        }
    }

    #[tokio::test]
    async fn rejects_invalid_shared_key() {
        let response = super::router(test_state().await)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/announcement")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&AnnouncementPayload {
                            title: "x".into(),
                            body_markdown: "y".into(),
                            details_url: None,
                            requested_by_cid: 1,
                            channel: None,
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn accepts_valid_shared_key() {
        let response = super::router(test_state().await)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/announcement")
                    .header("content-type", "application/json")
                    .header("x-api-key", "secret")
                    .body(Body::from(
                        serde_json::to_vec(&AnnouncementPayload {
                            title: "x".into(),
                            body_markdown: "y".into(),
                            details_url: None,
                            requested_by_cid: 1,
                            channel: None,
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.status().is_success());
    }

    #[tokio::test]
    async fn ready_fails_when_audit_enabled_but_channel_missing() {
        let state = test_state().await;
        state.runtime.readiness.write().await.audit.enabled = true;
        state.runtime.readiness.write().await.audit.channel_ready = false;

        let response = super::router(state)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.status().is_success());
        let body = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["ready"], false);
    }
}
