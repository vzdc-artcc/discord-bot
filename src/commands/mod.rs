use serenity::{
    all::{
        CommandInteraction, CommandOptionType, Context, CreateCommand, CreateCommandOption,
        ResolvedValue,
    },
    builder::CreateInteractionResponse,
};

use crate::{
    errors::{AppError, AppResult},
    models::{AnnouncementPayload, EventPositionPostingPayload, ReadyResponse},
    services::interaction_response,
    state::AppState,
};

pub fn definitions() -> Vec<CreateCommand> {
    vec![
        CreateCommand::new("ping").description("Check bot responsiveness"),
        CreateCommand::new("health").description("Show bot readiness and config sync state"),
        CreateCommand::new("sync-config").description("Refresh Discord configuration from Osmium"),
        CreateCommand::new("validate-guild")
            .description("Validate configured guild, channels, roles, and categories"),
        CreateCommand::new("register-commands")
            .description("Re-register slash commands in configured guilds"),
        CreateCommand::new("post-announcement-preview")
            .description("Post an announcement preview to configured announcement channels")
            .add_option(
                CreateCommandOption::new(CommandOptionType::String, "title", "Announcement title")
                    .required(true),
            )
            .add_option(
                CreateCommandOption::new(CommandOptionType::String, "body", "Announcement body")
                    .required(true),
            )
            .add_option(CreateCommandOption::new(
                CommandOptionType::String,
                "details_url",
                "Optional details URL",
            )),
        CreateCommand::new("post-event-preview")
            .description("Post an event position preview using live Osmium event data")
            .add_option(
                CreateCommandOption::new(CommandOptionType::String, "event_id", "Osmium event id")
                    .required(true),
            )
            .add_option(CreateCommandOption::new(
                CommandOptionType::Boolean,
                "ping_users",
                "Whether the final publish would ping users",
            )),
    ]
}

pub async fn handle_interaction(
    ctx: &Context,
    state: &AppState,
    command: CommandInteraction,
) -> AppResult<()> {
    let command_name = command.data.name.clone();
    let guild_id = command.guild_id.map(|id| id.get());
    let channel_id = command.channel_id.get();
    let user_id = command.user.id.get();
    let operator_required =
        !state.config.operator_user_ids.is_empty() || !state.config.operator_role_ids.is_empty();
    let span = tracing::info_span!(
        "slash_command",
        command = %command_name,
        guild_id,
        channel_id,
        user_id,
        operator_required
    );
    let _entered = span.enter();
    tracing::info!("received slash command");

    command
        .create_response(ctx, CreateInteractionResponse::Defer(Default::default()))
        .await?;

    let response = match command.data.name.as_str() {
        "ping" => "pong".to_string(),
        "health" => format_health(state).await,
        "sync-config" => {
            ensure_operator(state, &command)?;
            crate::app::sync_config(state).await?;
            tracing::info!("slash command synced configuration");
            "configuration refreshed from Osmium".to_string()
        }
        "validate-guild" => {
            ensure_operator(state, &command)?;
            let bundle = current_bundle(state).await?;
            let report = state.delivery.validate_guilds(&bundle).await?;
            tracing::info!(
                valid = report.valid,
                entries = report.entries.len(),
                "slash command validated guild configuration"
            );
            serde_json::to_string_pretty(&report)?
        }
        "register-commands" => {
            ensure_operator(state, &command)?;
            let updated = state
                .delivery
                .register_commands(&state.config.command_guild_ids)
                .await?;
            tracing::info!(delivered = updated, "slash command re-registered commands");
            format!("registered commands in {updated} guild(s)")
        }
        "post-announcement-preview" => {
            ensure_operator(state, &command)?;
            let payload = AnnouncementPayload {
                title: required_string_option(&command, "title")?,
                body_markdown: required_string_option(&command, "body")?,
                details_url: optional_string_option(&command, "details_url"),
                requested_by_cid: 0,
            };
            let bundle = current_bundle(state).await?;
            let delivered = state.delivery.send_announcement(&bundle, &payload).await?;
            tracing::info!(
                delivered,
                requested_by_cid = payload.requested_by_cid,
                "announcement preview delivered"
            );
            format!("announcement preview delivered to {delivered} channel(s)")
        }
        "post-event-preview" => {
            ensure_operator(state, &command)?;
            let payload = EventPositionPostingPayload {
                event_id: required_string_option(&command, "event_id")?,
                ping_users: optional_bool_option(&command, "ping_users").unwrap_or(false),
                requested_by_cid: 0,
            };
            let bundle = current_bundle(state).await?;
            let event = state.osmium.fetch_event(&payload.event_id).await?;
            let positions = state
                .osmium
                .fetch_event_positions(&payload.event_id)
                .await?;
            let delivered = state
                .delivery
                .send_event_position_posting(&bundle, &payload, &event, &positions.items)
                .await?;
            tracing::info!(
                delivered,
                event_id = %payload.event_id,
                requested_by_cid = payload.requested_by_cid,
                "event preview delivered"
            );
            format!("event preview delivered to {delivered} channel(s)")
        }
        other => format!("unknown command `{other}`"),
    };

    command
        .edit_response(ctx, interaction_response(response))
        .await?;
    tracing::info!("slash command completed successfully");

    Ok(())
}

async fn format_health(state: &AppState) -> String {
    let readiness = state.runtime.readiness.read().await.clone();
    let response = ReadyResponse {
        ready: readiness.discord_connected && readiness.config_loaded,
        discord_connected: readiness.discord_connected,
        config_loaded: readiness.config_loaded,
        last_config_sync_at: readiness.last_config_sync_at,
        last_config_error: readiness.last_config_error,
        staffup: readiness.staffup,
        audit: readiness.audit,
    };

    serde_json::to_string_pretty(&response).unwrap_or_else(|_| "{\"ready\":false}".to_string())
}

fn ensure_operator(state: &AppState, command: &CommandInteraction) -> AppResult<()> {
    if state.config.operator_user_ids.is_empty() && state.config.operator_role_ids.is_empty() {
        return Ok(());
    }

    if state
        .config
        .operator_user_ids
        .iter()
        .any(|user_id| *user_id == command.user.id)
    {
        return Ok(());
    }

    if command
        .member
        .as_ref()
        .map(|member| {
            member.roles.iter().any(|role_id| {
                state
                    .config
                    .operator_role_ids
                    .iter()
                    .any(|allowed| allowed == role_id)
            })
        })
        .unwrap_or(false)
    {
        return Ok(());
    }

    tracing::warn!(
        command = %command.data.name,
        guild_id = command.guild_id.map(|id| id.get()),
        channel_id = command.channel_id.get(),
        user_id = command.user.id.get(),
        "unauthorized slash command attempt"
    );
    Err(AppError::Unauthorized)
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

fn required_string_option(command: &CommandInteraction, name: &str) -> AppResult<String> {
    optional_string_option(command, name)
        .ok_or_else(|| AppError::BadRequest(format!("missing required option `{name}`")))
}

fn optional_string_option(command: &CommandInteraction, name: &str) -> Option<String> {
    command.data.options().iter().find_map(|option| {
        if option.name == name {
            match &option.value {
                ResolvedValue::String(value) => Some((*value).to_string()),
                _ => None,
            }
        } else {
            None
        }
    })
}

fn optional_bool_option(command: &CommandInteraction, name: &str) -> Option<bool> {
    command.data.options().iter().find_map(|option| {
        if option.name == name {
            match option.value {
                ResolvedValue::Boolean(value) => Some(value),
                _ => None,
            }
        } else {
            None
        }
    })
}
