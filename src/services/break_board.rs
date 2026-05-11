use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serenity::{
    all::{
        ActionRowComponent, ComponentInteraction, Context, CreateActionRow, CreateInputText,
        CreateInteractionResponse, InputTextStyle, ModalInteraction, RoleId,
    },
    builder::CreateModal,
    prelude::Mentionable,
};

use crate::{
    errors::{AppError, AppResult},
    models::{
        BreakBoardMessageState, BreakBoardRequestState, BreakBoardRequestsState,
        DiscordConfigBundle, ResolvedChannelTarget, ResolvedRoleTarget,
    },
    services::{
        BREAK_BOARD_CHANNEL_NAME, BREAK_BOARD_MODAL_CUSTOM_ID_PREFIX,
        BREAK_BOARD_OPEN_CUSTOM_ID_PREFIX, BREAK_BOARD_PREF_CUSTOM_ID_PREFIX,
        BREAK_BOARD_REQUEST_CLAIM_CUSTOM_ID_PREFIX, BREAK_BOARD_REQUEST_COMPLETE_CUSTOM_ID_PREFIX,
        BREAK_BOARD_REQUEST_DELETE_CUSTOM_ID_PREFIX, break_board_label,
        break_board_modal_custom_id, ephemeral_interaction_response,
    },
    state::AppState,
};

const BREAK_BOARD_CLEANUP_INTERVAL_SECS: u64 = 30;
const BREAK_BOARD_CLAIM_TIMEOUT_MINUTES: i64 = 5;
const BREAK_BOARD_REQUEST_TIMEOUT_GRACE_MINUTES: i64 = 5;
const BREAK_BOARD_MODAL_TITLE_PREFIX: &str = "Break Request: ";
const BREAK_BOARD_MODAL_TITLE_MAX_LEN: usize = 45;

const MODAL_TIME_BEFORE_CLOSE_ID: &str = "time_before_close";
const MODAL_POSITION_ID: &str = "position";
const MODAL_NOTES_ID: &str = "notes";

pub async fn ensure_break_board_messages(state: &AppState) -> AppResult<()> {
    let bundle = current_bundle(state).await?;
    let (target, roles) = resolve_break_board_targets(&bundle)?;
    tracing::info!(
        guild_id = target.guild_id,
        channel_id = target.channel_id,
        roles = roles.len(),
        "ensuring break board messages"
    );
    let saved = load_break_board_message_state(state).await?;
    let target_state = saved
        .filter(|saved| saved.guild_id == target.guild_id && saved.channel_id == target.channel_id);

    let next_state = if let Some(saved) = target_state {
        if state.delivery.break_board_messages_exist(&saved).await? {
            tracing::info!(
                guild_id = saved.guild_id,
                channel_id = saved.channel_id,
                "refreshing persisted break board messages"
            );
            state
                .delivery
                .refresh_break_board_messages(&saved, &roles)
                .await?;
            saved
        } else {
            tracing::info!(
                guild_id = target.guild_id,
                channel_id = target.channel_id,
                "persisted break board messages missing; recreating"
            );
            state
                .delivery
                .create_break_board_messages(&target, &roles)
                .await?
        }
    } else {
        tracing::info!(
            guild_id = target.guild_id,
            channel_id = target.channel_id,
            "creating break board messages"
        );
        state
            .delivery
            .create_break_board_messages(&target, &roles)
            .await?
    };

    save_break_board_message_state(state, &next_state).await?;
    Ok(())
}

pub async fn load_break_board_requests_into_runtime(state: &AppState) -> AppResult<()> {
    let requests = load_break_board_requests_file(state).await?;
    tracing::info!(
        requests = requests.items.len(),
        "loaded break board requests into runtime"
    );
    let mut guard = state.runtime.break_board_requests.lock().await;
    *guard = requests;
    Ok(())
}

pub async fn start_break_board_cleanup_worker(state: AppState) {
    tracing::info!(
        interval_secs = BREAK_BOARD_CLEANUP_INTERVAL_SECS,
        "starting break board cleanup worker"
    );
    tokio::spawn(async move {
        loop {
            if let Err(error) = cleanup_expired_break_board_requests(&state).await {
                tracing::error!(?error, "break board cleanup cycle failed");
            }

            tokio::time::sleep(std::time::Duration::from_secs(
                BREAK_BOARD_CLEANUP_INTERVAL_SECS,
            ))
            .await;
        }
    });
}

pub async fn handle_break_board_component_interaction(
    ctx: &Context,
    state: &AppState,
    interaction: &ComponentInteraction,
) -> AppResult<bool> {
    if let Some((custom_guild_id, role_id)) =
        parse_break_board_pref_custom_id(&interaction.data.custom_id)
    {
        return handle_break_board_preference_toggle(
            ctx,
            state,
            interaction,
            custom_guild_id,
            role_id,
        )
        .await;
    }

    if let Some((custom_guild_id, role_id)) =
        parse_break_board_open_custom_id(&interaction.data.custom_id)
    {
        return handle_break_board_request_open(ctx, state, interaction, custom_guild_id, role_id)
            .await;
    }

    if let Some(request_message_id) = parse_break_board_claim_custom_id(&interaction.data.custom_id)
    {
        return handle_break_board_claim(ctx, state, interaction, request_message_id).await;
    }

    if let Some(request_message_id) =
        parse_break_board_delete_custom_id(&interaction.data.custom_id)
    {
        return handle_break_board_delete(ctx, state, interaction, request_message_id).await;
    }

    if let Some(request_message_id) =
        parse_break_board_complete_custom_id(&interaction.data.custom_id)
    {
        return handle_break_board_complete(ctx, state, interaction, request_message_id).await;
    }

    Ok(false)
}

pub async fn handle_break_board_modal_interaction(
    ctx: &Context,
    state: &AppState,
    interaction: &ModalInteraction,
) -> AppResult<bool> {
    let Some((custom_guild_id, role_id)) =
        parse_break_board_modal_custom_id(&interaction.data.custom_id)
    else {
        return Ok(false);
    };
    tracing::info!(
        interaction_type = "modal",
        custom_id = %interaction.data.custom_id,
        guild_id = interaction.guild_id.map(|id| id.get()),
        channel_id = interaction.channel_id.get(),
        user_id = interaction.user.id.get(),
        role_id,
        "handling break board modal interaction"
    );

    let Some(interaction_guild_id) = interaction.guild_id else {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "This break board only works inside a Discord server.",
                )),
            )
            .await?;
        return Ok(true);
    };

    let bundle = current_bundle(state).await?;
    let (target, roles) = resolve_break_board_targets(&bundle)?;
    let Some(role) = roles.iter().find(|role| {
        role.guild_id == custom_guild_id
            && role.guild_id == interaction_guild_id.get()
            && role.role_id == role_id
    }) else {
        tracing::warn!(
            guild_id = interaction_guild_id.get(),
            role_id,
            user_id = interaction.user.id.get(),
            "received stale break board modal option"
        );
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "That break board option is no longer valid.",
                )),
            )
            .await?;
        return Ok(true);
    };

    let time_raw = modal_value(interaction, MODAL_TIME_BEFORE_CLOSE_ID)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    let minutes_before_close = match parse_duration_minutes(&time_raw) {
        Ok(minutes) => minutes,
        Err(message) => {
            interaction
                .create_response(
                    ctx,
                    CreateInteractionResponse::Message(ephemeral_interaction_response(message)),
                )
                .await?;
            return Ok(true);
        }
    };

    let position = modal_value(interaction, MODAL_POSITION_ID)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let notes = modal_value(interaction, MODAL_NOTES_ID)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let created_at = Utc::now();
    let request = BreakBoardRequestState {
        request_message_id: 0,
        guild_id: target.guild_id,
        channel_id: target.channel_id,
        requester_user_id: interaction.user.id.get(),
        requester_mention: interaction.user.mention().to_string(),
        role_id: role.role_id,
        role_name: role.name.clone(),
        audience_label: break_board_label(&role.name),
        display_position: position.unwrap_or_else(|| break_board_label(&role.name)),
        minutes_before_close,
        notes,
        created_at,
        expires_at: request_expires_at(created_at, minutes_before_close),
        claimed_by_user_id: None,
        claimed_by_mention: None,
        claimed_at: None,
        claimed_message_id: None,
        claimed_expires_at: None,
    };

    let mut requests_guard = state.runtime.break_board_requests.lock().await;
    let message_id = state
        .delivery
        .create_break_board_request_message(&request)
        .await?;
    let mut request = request;
    request.request_message_id = message_id;
    requests_guard.items.push(request.clone());

    if let Err(error) =
        save_break_board_requests_to_path(&state.config.break_board_requests_path, &requests_guard)
            .await
    {
        tracing::error!(
            ?error,
            guild_id = request.guild_id,
            channel_id = request.channel_id,
            role_id = request.role_id,
            user_id = request.requester_user_id,
            request_message_id = message_id,
            "failed to persist break board request state"
        );
        requests_guard
            .items
            .retain(|item| item.request_message_id != message_id);
        let _ = state
            .delivery
            .delete_break_board_message(target.channel_id, message_id)
            .await;
        return Err(error);
    }

    drop(requests_guard);
    tracing::info!(
        guild_id = request.guild_id,
        channel_id = request.channel_id,
        role_id = request.role_id,
        user_id = request.requester_user_id,
        request_message_id = request.request_message_id,
        "created break board request"
    );

    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Message(ephemeral_interaction_response(
                "Break request posted.",
            )),
        )
        .await?;
    Ok(true)
}

fn resolve_break_board_targets(
    bundle: &DiscordConfigBundle,
) -> AppResult<(ResolvedChannelTarget, Vec<ResolvedRoleTarget>)> {
    let channels = bundle.resolve_named_channels(BREAK_BOARD_CHANNEL_NAME)?;
    if channels.is_empty() {
        return Err(AppError::Config(
            "osmium discord config bundle does not define a `break_board` channel".into(),
        ));
    }
    if channels.len() > 1 {
        return Err(AppError::Config(
            "osmium discord config bundle defines multiple `break_board` channels".into(),
        ));
    }

    let target = channels
        .into_iter()
        .next()
        .expect("checked non-empty channels");
    let roles = bundle.resolve_role_prefix(crate::services::BREAK_BOARD_ROLE_PREFIX)?;
    if roles.is_empty() {
        return Err(AppError::Config(
            "osmium discord config bundle does not define any `break_board_` roles".into(),
        ));
    }
    if roles.iter().any(|role| role.guild_id != target.guild_id) {
        return Err(AppError::Config(
            "break board roles must belong to the same guild as the `break_board` channel".into(),
        ));
    }

    Ok((target, roles))
}

async fn handle_break_board_preference_toggle(
    ctx: &Context,
    state: &AppState,
    interaction: &ComponentInteraction,
    custom_guild_id: u64,
    role_id: u64,
) -> AppResult<bool> {
    tracing::info!(
        interaction_type = "component",
        custom_id = %interaction.data.custom_id,
        guild_id = interaction.guild_id.map(|id| id.get()),
        channel_id = interaction.channel_id.get(),
        user_id = interaction.user.id.get(),
        role_id,
        "handling break board preference interaction"
    );
    let Some(interaction_guild_id) = interaction.guild_id else {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "This break board only works inside a Discord server.",
                )),
            )
            .await?;
        return Ok(true);
    };

    let Some(member) = interaction.member.as_ref() else {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "Your member record was not available for this role toggle.",
                )),
            )
            .await?;
        return Ok(true);
    };

    let bundle = current_bundle(state).await?;
    let (_, roles) = resolve_break_board_targets(&bundle)?;
    let Some(role) = roles.iter().find(|role| {
        role.guild_id == custom_guild_id
            && role.guild_id == interaction_guild_id.get()
            && role.role_id == role_id
    }) else {
        tracing::warn!(
            guild_id = interaction_guild_id.get(),
            role_id,
            user_id = interaction.user.id.get(),
            "received stale break board preference option"
        );
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "That break board option is no longer valid.",
                )),
            )
            .await?;
        return Ok(true);
    };

    let discord_role_id = RoleId::new(role.role_id);
    let label = break_board_label(&role.name);
    let response = if member.roles.contains(&discord_role_id) {
        member.remove_role(&ctx.http, discord_role_id).await?;
        tracing::info!(
            role_id = role.role_id,
            user_id = interaction.user.id.get(),
            action = "remove",
            "removed break board preference role"
        );
        format!("Removed {label}")
    } else {
        member.add_role(&ctx.http, discord_role_id).await?;
        tracing::info!(
            role_id = role.role_id,
            user_id = interaction.user.id.get(),
            action = "add",
            "added break board preference role"
        );
        format!("Added {label}")
    };

    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Message(ephemeral_interaction_response(response)),
        )
        .await?;

    Ok(true)
}

async fn handle_break_board_request_open(
    ctx: &Context,
    state: &AppState,
    interaction: &ComponentInteraction,
    custom_guild_id: u64,
    role_id: u64,
) -> AppResult<bool> {
    tracing::info!(
        interaction_type = "component",
        custom_id = %interaction.data.custom_id,
        guild_id = interaction.guild_id.map(|id| id.get()),
        channel_id = interaction.channel_id.get(),
        user_id = interaction.user.id.get(),
        role_id,
        "handling break board request open interaction"
    );
    let Some(interaction_guild_id) = interaction.guild_id else {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "This break board only works inside a Discord server.",
                )),
            )
            .await?;
        return Ok(true);
    };

    let bundle = current_bundle(state).await?;
    let (_, roles) = resolve_break_board_targets(&bundle)?;
    let Some(role) = roles.iter().find(|role| {
        role.guild_id == custom_guild_id
            && role.guild_id == interaction_guild_id.get()
            && role.role_id == role_id
    }) else {
        tracing::warn!(
            guild_id = interaction_guild_id.get(),
            role_id,
            user_id = interaction.user.id.get(),
            "received stale break board request option"
        );
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "That break board option is no longer valid.",
                )),
            )
            .await?;
        return Ok(true);
    };

    let modal = CreateModal::new(
        break_board_modal_custom_id(role.guild_id, role.role_id),
        break_board_modal_title(&break_board_label(&role.name)),
    )
    .components(vec![
        CreateActionRow::InputText(
            CreateInputText::new(
                InputTextStyle::Short,
                "How much time before you close?",
                MODAL_TIME_BEFORE_CLOSE_ID,
            )
            .required(true)
            .placeholder("Examples: 15m, 30, 1h, 1h 15m"),
        ),
        CreateActionRow::InputText(
            CreateInputText::new(
                InputTextStyle::Short,
                "Position to fill (optional)",
                MODAL_POSITION_ID,
            )
            .required(false)
            .placeholder("Defaults to the selected role"),
        ),
        CreateActionRow::InputText(
            CreateInputText::new(
                InputTextStyle::Paragraph,
                "Notes (optional)",
                MODAL_NOTES_ID,
            )
            .required(false),
        ),
    ]);

    interaction
        .create_response(ctx, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(true)
}

async fn handle_break_board_claim(
    ctx: &Context,
    state: &AppState,
    interaction: &ComponentInteraction,
    request_message_id: u64,
) -> AppResult<bool> {
    tracing::info!(
        interaction_type = "component",
        custom_id = %interaction.data.custom_id,
        channel_id = interaction.channel_id.get(),
        user_id = interaction.user.id.get(),
        request_message_id,
        "handling break board claim"
    );
    let mut requests_guard = state.runtime.break_board_requests.lock().await;
    let Some(index) = requests_guard
        .items
        .iter()
        .position(|item| item.request_message_id == request_message_id)
    else {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "That break request is no longer active.",
                )),
            )
            .await?;
        return Ok(true);
    };

    if requests_guard.items[index].claimed_by_user_id.is_some() {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "That break request has already been claimed.",
                )),
            )
            .await?;
        return Ok(true);
    }

    if requests_guard.items[index].requester_user_id == interaction.user.id.get() {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "You cannot claim your own break request.",
                )),
            )
            .await?;
        return Ok(true);
    }

    let claimed_at = Utc::now();
    requests_guard.items[index].claimed_by_user_id = Some(interaction.user.id.get());
    requests_guard.items[index].claimed_by_mention = Some(interaction.user.mention().to_string());
    requests_guard.items[index].claimed_at = Some(claimed_at);
    requests_guard.items[index].claimed_expires_at = Some(claim_expires_at(claimed_at));

    let request_snapshot = requests_guard.items[index].clone();
    state
        .delivery
        .mark_break_board_request_claimed(&request_snapshot)
        .await?;
    let claimed_message_id = state
        .delivery
        .create_break_board_claim_message(&request_snapshot)
        .await?;
    requests_guard.items[index].claimed_message_id = Some(claimed_message_id);

    save_break_board_requests_to_path(&state.config.break_board_requests_path, &requests_guard)
        .await?;
    drop(requests_guard);
    tracing::info!(
        request_message_id = request_snapshot.request_message_id,
        guild_id = request_snapshot.guild_id,
        channel_id = request_snapshot.channel_id,
        user_id = interaction.user.id.get(),
        "claimed break board request"
    );

    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Message(ephemeral_interaction_response(
                "Break request claimed.",
            )),
        )
        .await?;
    Ok(true)
}

async fn handle_break_board_delete(
    ctx: &Context,
    state: &AppState,
    interaction: &ComponentInteraction,
    request_message_id: u64,
) -> AppResult<bool> {
    tracing::info!(
        interaction_type = "component",
        custom_id = %interaction.data.custom_id,
        channel_id = interaction.channel_id.get(),
        user_id = interaction.user.id.get(),
        request_message_id,
        "handling break board delete"
    );
    let mut requests_guard = state.runtime.break_board_requests.lock().await;
    let Some(index) = requests_guard
        .items
        .iter()
        .position(|item| item.request_message_id == request_message_id)
    else {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "That break request is no longer active.",
                )),
            )
            .await?;
        return Ok(true);
    };

    let request = requests_guard.items[index].clone();
    if request.requester_user_id != interaction.user.id.get() {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "Only the original requester can delete this break request.",
                )),
            )
            .await?;
        return Ok(true);
    }

    cleanup_request_messages(state, &request).await?;
    requests_guard.items.remove(index);
    save_break_board_requests_to_path(&state.config.break_board_requests_path, &requests_guard)
        .await?;
    drop(requests_guard);
    tracing::info!(
        request_message_id = request.request_message_id,
        guild_id = request.guild_id,
        channel_id = request.channel_id,
        user_id = interaction.user.id.get(),
        "deleted break board request"
    );

    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Message(ephemeral_interaction_response(
                "Break request deleted.",
            )),
        )
        .await?;
    Ok(true)
}

async fn handle_break_board_complete(
    ctx: &Context,
    state: &AppState,
    interaction: &ComponentInteraction,
    request_message_id: u64,
) -> AppResult<bool> {
    tracing::info!(
        interaction_type = "component",
        custom_id = %interaction.data.custom_id,
        channel_id = interaction.channel_id.get(),
        user_id = interaction.user.id.get(),
        request_message_id,
        "handling break board completion"
    );
    let mut requests_guard = state.runtime.break_board_requests.lock().await;
    let Some(index) = requests_guard
        .items
        .iter()
        .position(|item| item.request_message_id == request_message_id)
    else {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "That break request is no longer active.",
                )),
            )
            .await?;
        return Ok(true);
    };

    let request = requests_guard.items[index].clone();
    let authorized = request.requester_user_id == interaction.user.id.get()
        || request.claimed_by_user_id == Some(interaction.user.id.get());
    if !authorized {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "Only the requester or claimer can complete this break request.",
                )),
            )
            .await?;
        return Ok(true);
    }

    cleanup_request_messages(state, &request).await?;
    requests_guard.items.remove(index);
    save_break_board_requests_to_path(&state.config.break_board_requests_path, &requests_guard)
        .await?;
    drop(requests_guard);
    tracing::info!(
        request_message_id = request.request_message_id,
        guild_id = request.guild_id,
        channel_id = request.channel_id,
        user_id = interaction.user.id.get(),
        "completed break board request"
    );

    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Message(ephemeral_interaction_response(
                "Break request completed.",
            )),
        )
        .await?;
    Ok(true)
}

async fn cleanup_expired_break_board_requests(state: &AppState) -> AppResult<()> {
    let now = Utc::now();
    let mut requests_guard = state.runtime.break_board_requests.lock().await;
    let mut changed = false;
    let mut retained = Vec::with_capacity(requests_guard.items.len());

    for request in requests_guard.items.iter().cloned() {
        if should_delete_request(&request, now) {
            tracing::info!(
                request_message_id = request.request_message_id,
                guild_id = request.guild_id,
                channel_id = request.channel_id,
                "cleaning up expired break board request"
            );
            cleanup_request_messages(state, &request).await?;
            changed = true;
        } else {
            retained.push(request);
        }
    }

    if changed {
        requests_guard.items = retained;
        save_break_board_requests_to_path(&state.config.break_board_requests_path, &requests_guard)
            .await?;
    }

    tracing::debug!(
        changed,
        remaining = requests_guard.items.len(),
        "break board cleanup cycle finished"
    );

    Ok(())
}

fn should_delete_request(request: &BreakBoardRequestState, now: DateTime<Utc>) -> bool {
    if let Some(claimed_expires_at) = request.claimed_expires_at {
        now >= claimed_expires_at
    } else {
        now >= request.expires_at
    }
}

async fn cleanup_request_messages(
    state: &AppState,
    request: &BreakBoardRequestState,
) -> AppResult<()> {
    state
        .delivery
        .delete_break_board_message(request.channel_id, request.request_message_id)
        .await?;
    if let Some(claimed_message_id) = request.claimed_message_id {
        state
            .delivery
            .delete_break_board_message(request.channel_id, claimed_message_id)
            .await?;
    }
    Ok(())
}

async fn current_bundle(state: &AppState) -> AppResult<DiscordConfigBundle> {
    state
        .runtime
        .config_bundle
        .read()
        .await
        .clone()
        .ok_or_else(|| AppError::Config("discord config bundle is not loaded".into()))
}

async fn load_break_board_message_state(
    state: &AppState,
) -> AppResult<Option<BreakBoardMessageState>> {
    let path = &state.config.break_board_state_path;
    ensure_parent_dir(path).await?;
    if !path.exists() {
        return Ok(None);
    }

    let bytes = tokio::fs::read(path).await?;
    let parsed = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::BadRequest(format!(
            "failed to parse break board message state: {error}"
        ))
    })?;
    Ok(Some(parsed))
}

async fn save_break_board_message_state(
    state: &AppState,
    board: &BreakBoardMessageState,
) -> AppResult<()> {
    save_json_to_path(&state.config.break_board_state_path, board).await
}

async fn load_break_board_requests_file(state: &AppState) -> AppResult<BreakBoardRequestsState> {
    let path = &state.config.break_board_requests_path;
    ensure_parent_dir(path).await?;
    if !path.exists() {
        return Ok(BreakBoardRequestsState::default());
    }

    let bytes = tokio::fs::read(path).await?;
    serde_json::from_slice(&bytes).map_err(|error| {
        AppError::BadRequest(format!(
            "failed to parse break board request state: {error}"
        ))
    })
}

async fn save_break_board_requests_to_path(
    path: &Path,
    requests: &BreakBoardRequestsState,
) -> AppResult<()> {
    save_json_to_path(path, requests).await
}

async fn save_json_to_path<T>(path: &Path, value: &T) -> AppResult<()>
where
    T: serde::Serialize,
{
    ensure_parent_dir(path).await?;
    let tmp_path = tmp_path(path);
    let body = serde_json::to_vec_pretty(value)?;
    tokio::fs::write(&tmp_path, body).await?;
    tokio::fs::rename(&tmp_path, path).await?;
    Ok(())
}

async fn ensure_parent_dir(path: &Path) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    Ok(())
}

fn tmp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("break_board_state.json");
    path.with_file_name(format!("{file_name}.tmp"))
}

fn modal_value<'a>(interaction: &'a ModalInteraction, custom_id: &str) -> Option<&'a str> {
    interaction.data.components.iter().find_map(|row| {
        row.components.iter().find_map(|component| match component {
            ActionRowComponent::InputText(input) if input.custom_id == custom_id => {
                input.value.as_deref()
            }
            _ => None,
        })
    })
}

fn parse_duration_minutes(raw: &str) -> Result<i64, &'static str> {
    let input = raw.trim().to_ascii_lowercase();
    if input.is_empty() {
        return Err("Time before close is required.");
    }
    if input.starts_with('-') {
        return Err("Time before close must be a positive duration.");
    }

    let chars: Vec<char> = input.chars().collect();
    let mut index = 0usize;
    let mut total = 0i64;
    let mut found_any = false;

    while index < chars.len() {
        while index < chars.len() && (chars[index].is_ascii_whitespace() || chars[index] == ',') {
            index += 1;
        }
        if index >= chars.len() {
            break;
        }
        if !chars[index].is_ascii_digit() {
            return Err("Time before close must look like `15m`, `30`, or `1h 15m`.");
        }

        let start = index;
        while index < chars.len() && chars[index].is_ascii_digit() {
            index += 1;
        }
        let value = input[start..index]
            .parse::<i64>()
            .map_err(|_| "Time before close must be a valid positive duration.")?;

        while index < chars.len() && chars[index].is_ascii_whitespace() {
            index += 1;
        }

        let unit_start = index;
        while index < chars.len() && chars[index].is_ascii_alphabetic() {
            index += 1;
        }
        let unit = input[unit_start..index].trim();

        let increment = if unit.is_empty() || unit.starts_with('m') {
            value
        } else if unit.starts_with('h') {
            value * 60
        } else {
            return Err("Time before close must use minutes or hours.");
        };

        total += increment;
        found_any = true;
    }

    if !found_any || total <= 0 {
        return Err("Time before close must be a positive duration.");
    }

    Ok(total)
}

fn break_board_modal_title(role_label: &str) -> String {
    let trimmed_label = role_label.trim();
    let full_title = format!("{BREAK_BOARD_MODAL_TITLE_PREFIX}{trimmed_label}");
    if full_title.chars().count() <= BREAK_BOARD_MODAL_TITLE_MAX_LEN {
        return full_title;
    }

    let allowed_label_chars = BREAK_BOARD_MODAL_TITLE_MAX_LEN
        .saturating_sub(BREAK_BOARD_MODAL_TITLE_PREFIX.chars().count());
    let shortened_label: String = trimmed_label.chars().take(allowed_label_chars).collect();
    format!(
        "{BREAK_BOARD_MODAL_TITLE_PREFIX}{}",
        shortened_label.trim_end()
    )
}

fn request_expires_at(created_at: DateTime<Utc>, minutes_before_close: i64) -> DateTime<Utc> {
    created_at
        + Duration::minutes(minutes_before_close)
        + Duration::minutes(BREAK_BOARD_REQUEST_TIMEOUT_GRACE_MINUTES)
}

fn claim_expires_at(claimed_at: DateTime<Utc>) -> DateTime<Utc> {
    claimed_at + Duration::minutes(BREAK_BOARD_CLAIM_TIMEOUT_MINUTES)
}

pub fn parse_break_board_pref_custom_id(custom_id: &str) -> Option<(u64, u64)> {
    parse_guild_role_custom_id(custom_id, BREAK_BOARD_PREF_CUSTOM_ID_PREFIX)
}

pub fn parse_break_board_open_custom_id(custom_id: &str) -> Option<(u64, u64)> {
    parse_guild_role_custom_id(custom_id, BREAK_BOARD_OPEN_CUSTOM_ID_PREFIX)
}

pub fn parse_break_board_modal_custom_id(custom_id: &str) -> Option<(u64, u64)> {
    parse_guild_role_custom_id(custom_id, BREAK_BOARD_MODAL_CUSTOM_ID_PREFIX)
}

pub fn parse_break_board_claim_custom_id(custom_id: &str) -> Option<u64> {
    parse_message_custom_id(custom_id, BREAK_BOARD_REQUEST_CLAIM_CUSTOM_ID_PREFIX)
}

pub fn parse_break_board_delete_custom_id(custom_id: &str) -> Option<u64> {
    parse_message_custom_id(custom_id, BREAK_BOARD_REQUEST_DELETE_CUSTOM_ID_PREFIX)
}

pub fn parse_break_board_complete_custom_id(custom_id: &str) -> Option<u64> {
    parse_message_custom_id(custom_id, BREAK_BOARD_REQUEST_COMPLETE_CUSTOM_ID_PREFIX)
}

fn parse_guild_role_custom_id(custom_id: &str, prefix: &str) -> Option<(u64, u64)> {
    let mut parts = custom_id.split(':');
    let actual_prefix = parts.next()?;
    let guild_id = parts.next()?.parse::<u64>().ok()?;
    let role_id = parts.next()?.parse::<u64>().ok()?;
    if actual_prefix != prefix || parts.next().is_some() {
        return None;
    }
    Some((guild_id, role_id))
}

fn parse_message_custom_id(custom_id: &str, prefix: &str) -> Option<u64> {
    let mut parts = custom_id.split(':');
    let actual_prefix = parts.next()?;
    let message_id = parts.next()?.parse::<u64>().ok()?;
    if actual_prefix != prefix || parts.next().is_some() {
        return None;
    }
    Some(message_id)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use chrono::TimeZone;

    use crate::models::{
        DiscordCategoryItem, DiscordChannelItem, DiscordConfigBundle, DiscordConfigItem,
        DiscordRoleItem,
    };

    use super::{
        break_board_modal_title, claim_expires_at, parse_break_board_claim_custom_id,
        parse_break_board_complete_custom_id, parse_break_board_delete_custom_id,
        parse_break_board_modal_custom_id, parse_break_board_open_custom_id,
        parse_break_board_pref_custom_id, parse_duration_minutes, request_expires_at,
        resolve_break_board_targets, tmp_path,
    };

    fn bundle() -> DiscordConfigBundle {
        DiscordConfigBundle {
            configs: vec![DiscordConfigItem {
                id: "config-1".into(),
                name: "main".into(),
                guild_id: Some("123".into()),
                created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
                updated_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
            }],
            channels: vec![DiscordChannelItem {
                id: "channel-1".into(),
                discord_config_id: "config-1".into(),
                name: "break_board".into(),
                channel_id: "456".into(),
                created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
            }],
            roles: vec![
                DiscordRoleItem {
                    id: "role-1".into(),
                    discord_config_id: "config-1".into(),
                    name: "break_board_ground".into(),
                    role_id: "789".into(),
                    created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
                },
                DiscordRoleItem {
                    id: "role-2".into(),
                    discord_config_id: "config-1".into(),
                    name: "break_board_tower".into(),
                    role_id: "790".into(),
                    created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
                },
            ],
            categories: vec![DiscordCategoryItem {
                id: "category-1".into(),
                discord_config_id: "config-1".into(),
                name: "general".into(),
                category_id: "111".into(),
                created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
            }],
        }
    }

    #[test]
    fn resolves_break_board_target_and_roles() {
        let (target, roles) = resolve_break_board_targets(&bundle()).unwrap();
        assert_eq!(target.channel_id, 456);
        assert_eq!(roles.len(), 2);
        assert_eq!(roles[0].name, "break_board_ground");
    }

    #[test]
    fn rejects_missing_break_board_channel() {
        let mut bundle = bundle();
        bundle.channels.clear();
        assert!(resolve_break_board_targets(&bundle).is_err());
    }

    #[test]
    fn rejects_cross_guild_roles() {
        let mut bundle = bundle();
        bundle.configs.push(DiscordConfigItem {
            id: "config-2".into(),
            name: "other".into(),
            guild_id: Some("999".into()),
            created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
            updated_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
        });
        bundle.roles.push(DiscordRoleItem {
            id: "role-3".into(),
            discord_config_id: "config-2".into(),
            name: "break_board_center".into(),
            role_id: "791".into(),
            created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
        });
        assert!(resolve_break_board_targets(&bundle).is_err());
    }

    #[test]
    fn parses_duration_minutes_variants() {
        assert_eq!(parse_duration_minutes("5").unwrap(), 5);
        assert_eq!(parse_duration_minutes("15m").unwrap(), 15);
        assert_eq!(parse_duration_minutes("1h").unwrap(), 60);
        assert_eq!(parse_duration_minutes("1h 15m").unwrap(), 75);
        assert!(parse_duration_minutes("0").is_err());
        assert!(parse_duration_minutes("-5").is_err());
        assert!(parse_duration_minutes("tomorrow").is_err());
    }

    #[test]
    fn calculates_request_and_claim_expirations() {
        let created_at = chrono::Utc.with_ymd_and_hms(2026, 5, 10, 20, 0, 0).unwrap();
        assert_eq!(
            request_expires_at(created_at, 15),
            chrono::Utc
                .with_ymd_and_hms(2026, 5, 10, 20, 20, 0)
                .unwrap()
        );
        let claimed_at = chrono::Utc
            .with_ymd_and_hms(2026, 5, 10, 20, 10, 0)
            .unwrap();
        assert_eq!(
            claim_expires_at(claimed_at),
            chrono::Utc
                .with_ymd_and_hms(2026, 5, 10, 20, 15, 0)
                .unwrap()
        );
    }

    #[test]
    fn custom_ids_round_trip() {
        assert_eq!(
            parse_break_board_pref_custom_id(&crate::services::break_board_pref_custom_id(1, 2)),
            Some((1, 2))
        );
        assert_eq!(
            parse_break_board_open_custom_id(&crate::services::break_board_open_custom_id(1, 2)),
            Some((1, 2))
        );
        assert_eq!(
            parse_break_board_modal_custom_id(&crate::services::break_board_modal_custom_id(1, 2)),
            Some((1, 2))
        );
        assert_eq!(
            parse_break_board_claim_custom_id(&crate::services::break_board_claim_custom_id(3)),
            Some(3)
        );
        assert_eq!(
            parse_break_board_delete_custom_id(&crate::services::break_board_delete_custom_id(4)),
            Some(4)
        );
        assert_eq!(
            parse_break_board_complete_custom_id(&crate::services::break_board_complete_custom_id(
                5
            )),
            Some(5)
        );
    }

    #[test]
    fn tmp_path_uses_tmp_suffix() {
        assert_eq!(
            tmp_path(Path::new("data/break_board_requests.json")),
            PathBuf::from("data/break_board_requests.json.tmp")
        );
    }

    #[test]
    fn modal_title_keeps_short_labels_exact() {
        assert_eq!(
            break_board_modal_title("Center Relief"),
            "Break Request: Center Relief"
        );
    }

    #[test]
    fn modal_title_fits_long_known_labels() {
        let title = break_board_modal_title("Unrestricted Ground Relief");
        assert!(title.chars().count() <= 45);
        assert!(title.starts_with("Break Request: "));
        assert!(title.len() > "Break Request: ".len());
    }

    #[test]
    fn modal_title_truncates_extremely_long_labels_safely() {
        let title = break_board_modal_title(
            "This Is An Extremely Long Future Relief Label That Must Be Trimmed",
        );
        assert!(title.chars().count() <= 45);
        assert!(title.starts_with("Break Request: "));
    }
}
