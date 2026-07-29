use chrono::{DateTime, Utc};
use serenity::{
    all::{
        ButtonStyle, CommandInteraction, CommandOptionType, ComponentInteraction, Context,
        CreateActionRow, CreateButton, CreateCommand, CreateCommandOption, CreateEmbed,
        ResolvedValue,
    },
    builder::{
        CreateInteractionResponse, CreateInteractionResponseMessage, EditInteractionResponse,
    },
};
use url::Url;

use crate::{
    errors::{AppError, AppResult},
    models::{
        AnnouncementPayload, AuditEvent, AuditKind, DiscordUserLookupResponse,
        EventPositionPostingPayload, LinkedDiscordUsersResponse, ReadyResponse,
    },
    services::{
        format_role_sync_outcome, interaction_response, publish_audit_event, sync_roles_for_cid,
    },
    state::AppState,
};

const MAX_TITLE_LEN: usize = 256;
const MAX_BODY_LEN: usize = 4000;
const MAX_EVENT_ID_LEN: usize = 128;
const USER_INFO_COLOR: u32 = 0x2ECC71;
const OPERATOR_INFO_COLOR: u32 = 0x5865F2;
const WARNING_COLOR: u32 = 0xF39C12;

pub fn definitions() -> Vec<CreateCommand> {
    vec![
        CreateCommand::new("ping").description("Check bot responsiveness"),
        CreateCommand::new("health").description("Show bot readiness and config sync state"),
        CreateCommand::new("whoami").description("Show your linked Osmium account status"),
        CreateCommand::new("link-status")
            .description("Show whether your Discord account is linked"),
        CreateCommand::new("profile")
            .description("Look up a public profile by CID or Discord mention")
            .add_option(CreateCommandOption::new(
                CommandOptionType::String,
                "cid_or_mention",
                "CID or Discord mention",
            )),
        CreateCommand::new("help").description("List available bot commands"),
        CreateCommand::new("lookup-discord")
            .description("Operator: look up a Discord link by Discord ID or mention")
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "discord_id_or_mention",
                    "Discord ID or mention",
                )
                .required(true),
            ),
        CreateCommand::new("lookup-cid")
            .description("Operator: look up a Discord link by CID")
            .add_option(
                CreateCommandOption::new(CommandOptionType::String, "cid", "VATSIM CID")
                    .required(true),
            ),
        CreateCommand::new("list-linked-users")
            .description("Operator: list linked Discord users with pagination"),
        CreateCommand::new("unlink-discord")
            .description("Operator: force-unlink a Discord account by CID")
            .add_option(
                CreateCommandOption::new(CommandOptionType::String, "cid", "VATSIM CID")
                    .required(true),
            ),
        CreateCommand::new("sync-roles")
            .description("Operator: manually sync mapped Discord roles for a CID or mention")
            .add_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "cid_or_mention",
                    "CID or Discord mention",
                )
                .required(true),
            ),
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

    let result = dispatch_command(ctx, state, &command).await;

    match result {
        Ok(response) => {
            command.edit_response(ctx, response).await?;
            tracing::info!("slash command completed successfully");
        }
        Err(CommandError::Unauthorized) => {
            tracing::warn!("unauthorized slash command attempt");
            command
                .edit_response(
                    ctx,
                    interaction_response("You are not authorized to use this command."),
                )
                .await?;
        }
        Err(CommandError::Validation(msg)) => {
            tracing::warn!(reason = %msg, "slash command validation failed");
            command
                .edit_response(ctx, interaction_response(msg))
                .await?;
        }
        Err(CommandError::App(error)) => {
            tracing::error!(?error, "slash command failed");
            command
                .edit_response(
                    ctx,
                    interaction_response("An internal error occurred. Check bot logs for details."),
                )
                .await?;
        }
    }

    Ok(())
}

pub async fn handle_component_interaction(
    ctx: &Context,
    state: &AppState,
    component: &ComponentInteraction,
) -> AppResult<bool> {
    let custom_id = component.data.custom_id.as_str();

    if let Some(page) = custom_id.strip_prefix("linked-users:") {
        ensure_operator_component(state, component)?;
        let page = parse_positive_i64(page, "page").map_err(command_error_to_app_error)?;
        let response = state.osmium.list_linked_discord_users(page).await?;
        component
            .create_response(
                ctx,
                CreateInteractionResponse::UpdateMessage(
                    CreateInteractionResponseMessage::new()
                        .embed(linked_users_embed(&response))
                        .components(linked_users_components(&response)),
                ),
            )
            .await?;
        return Ok(true);
    }

    if let Some(cid) = custom_id.strip_prefix("unlink-discord:confirm:") {
        ensure_operator_component(state, component)?;
        let cid = parse_positive_i64(cid, "cid").map_err(command_error_to_app_error)?;
        let api_response = state.osmium.unlink_discord(cid).await?;
        publish_audit_event(
            state,
            AuditEvent {
                kind: AuditKind::MemberUpdated,
                guild_id: component.guild_id.map(|id| id.get()).unwrap_or_default(),
                channel_id: Some(component.channel_id.get()),
                target_id: Some(component.user.id.get()),
                actor_user_id: Some(component.user.id.get()),
                actor_label: Some(user_label(&component.user.name, component.user.id.get())),
                subject_label: format!("CID {cid}"),
                summary: "An operator changed a Discord link.".to_string(),
                details: vec![
                    (
                        "Action".to_string(),
                        "Force unlink Discord account".to_string(),
                    ),
                    ("Result".to_string(), api_response.message.clone()),
                ],
                color: WARNING_COLOR,
                occurred_at: Utc::now(),
                dedupe_key: None,
            },
        )
        .await?;
        component
            .create_response(
                ctx,
                CreateInteractionResponse::UpdateMessage(
                    CreateInteractionResponseMessage::new()
                        .content(api_response.message)
                        .components(Vec::new()),
                ),
            )
            .await?;
        return Ok(true);
    }

    if custom_id.starts_with("unlink-discord:cancel:") {
        ensure_operator_component(state, component)?;
        component
            .create_response(
                ctx,
                CreateInteractionResponse::UpdateMessage(
                    CreateInteractionResponseMessage::new()
                        .content("Discord unlink cancelled.")
                        .components(Vec::new()),
                ),
            )
            .await?;
        return Ok(true);
    }

    Ok(false)
}

fn command_error_to_app_error(error: CommandError) -> AppError {
    match error {
        CommandError::Unauthorized => AppError::Unauthorized,
        CommandError::Validation(message) => AppError::BadRequest(message),
        CommandError::App(error) => error,
    }
}

enum CommandError {
    Unauthorized,
    Validation(String),
    App(AppError),
}

impl From<AppError> for CommandError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::Unauthorized => Self::Unauthorized,
            AppError::BadRequest(msg) => Self::Validation(msg),
            other => Self::App(other),
        }
    }
}

async fn dispatch_command(
    _ctx: &Context,
    state: &AppState,
    command: &CommandInteraction,
) -> Result<EditInteractionResponse, CommandError> {
    match command.data.name.as_str() {
        "ping" => Ok(interaction_response("pong")),
        "health" => Ok(interaction_response(format_health(state).await)),
        "whoami" => {
            let lookup = state
                .osmium
                .lookup_discord_user(command.user.id.get())
                .await?;
            Ok(link_lookup_response(
                "Linked Account",
                "Your Discord link status.",
                &lookup,
                true,
                false,
            ))
        }
        "link-status" => {
            let lookup = state
                .osmium
                .lookup_discord_user(command.user.id.get())
                .await?;
            Ok(link_status_response(&lookup))
        }
        "profile" => {
            let lookup = match optional_string_option(command, "cid_or_mention") {
                Some(target) => lookup_profile_target(state, &target).await?,
                None => {
                    state
                        .osmium
                        .lookup_discord_user(command.user.id.get())
                        .await?
                }
            };
            Ok(link_lookup_response(
                "Profile Lookup",
                "Public profile information.",
                &lookup,
                false,
                true,
            ))
        }
        "help" => Ok(help_response()),
        "lookup-discord" => {
            ensure_operator_command(state, command)?;
            let target = required_string_option(command, "discord_id_or_mention")?;
            let discord_id = parse_discord_id_target(&target)?;
            let lookup = state.osmium.admin_lookup_discord_user(discord_id).await?;
            Ok(link_lookup_response(
                "Discord Link Lookup",
                "Operator lookup result.",
                &lookup,
                false,
                false,
            ))
        }
        "lookup-cid" => {
            ensure_operator_command(state, command)?;
            let cid = parse_positive_i64(&required_string_option(command, "cid")?, "cid")?;
            let lookup = state.osmium.admin_lookup_cid(cid).await?;
            Ok(link_lookup_response(
                "CID Link Lookup",
                "Operator lookup result.",
                &lookup,
                false,
                false,
            ))
        }
        "list-linked-users" => {
            ensure_operator_command(state, command)?;
            let response = state.osmium.list_linked_discord_users(1).await?;
            Ok(EditInteractionResponse::new()
                .embed(linked_users_embed(&response))
                .components(linked_users_components(&response)))
        }
        "unlink-discord" => {
            ensure_operator_command(state, command)?;
            let cid = parse_positive_i64(&required_string_option(command, "cid")?, "cid")?;
            Ok(EditInteractionResponse::new()
                .embed(
                    CreateEmbed::new()
                        .color(WARNING_COLOR)
                        .title("Confirm Discord Unlink")
                        .description(format!("Force-unlink the Discord account for CID `{cid}`?"))
                        .field("CID", cid.to_string(), true)
                        .field(
                            "Requested By",
                            format!("<@{}>", command.user.id.get()),
                            true,
                        ),
                )
                .components(vec![CreateActionRow::Buttons(vec![
                    CreateButton::new(format!("unlink-discord:confirm:{cid}"))
                        .label("Confirm unlink")
                        .style(ButtonStyle::Danger),
                    CreateButton::new(format!("unlink-discord:cancel:{cid}"))
                        .label("Cancel")
                        .style(ButtonStyle::Secondary),
                ])]))
        }
        "sync-roles" => {
            ensure_operator_command(state, command)?;
            let target = required_string_option(command, "cid_or_mention")?;
            let cid = resolve_cid_target(state, &target).await?;
            let outcome = sync_roles_for_cid(state, cid, "manual_command").await?;
            Ok(interaction_response(format_role_sync_outcome(&outcome)))
        }
        "sync-config" => {
            ensure_operator_command(state, command)?;
            crate::app::sync_config(state).await?;
            tracing::info!("slash command synced configuration");
            Ok(interaction_response("Configuration refreshed from Osmium."))
        }
        "validate-guild" => {
            ensure_operator_command(state, command)?;
            let bundle = current_bundle(state).await?;
            let report = state.delivery.validate_guilds(&bundle).await?;
            tracing::info!(
                valid = report.valid,
                entries = report.entries.len(),
                "slash command validated guild configuration"
            );
            Ok(interaction_response(format_guild_validation(&report)))
        }
        "register-commands" => {
            ensure_operator_command(state, command)?;
            let updated = state
                .delivery
                .register_commands(&state.config.command_guild_ids)
                .await?;
            tracing::info!(delivered = updated, "slash command re-registered commands");
            Ok(interaction_response(format!(
                "Registered commands in {updated} guild(s)."
            )))
        }
        "post-announcement-preview" => {
            ensure_operator_command(state, command)?;
            let title = validated_title(command)?;
            let body = validated_body(command)?;
            let details_url = validated_optional_url(command, "details_url")?;
            let payload = AnnouncementPayload {
                title,
                body_markdown: body,
                details_url,
                requested_by_cid: 0,
                channel: None,
            };
            let bundle = current_bundle(state).await?;
            let delivered = state.delivery.send_announcement(&bundle, &payload).await?;
            tracing::info!(
                delivered,
                requested_by_cid = payload.requested_by_cid,
                "announcement preview delivered"
            );
            Ok(interaction_response(format!(
                "Announcement preview delivered to {delivered} channel(s)."
            )))
        }
        "post-event-preview" => {
            ensure_operator_command(state, command)?;
            let event_id = validated_event_id(command)?;
            let payload = EventPositionPostingPayload {
                event_id,
                ping_users: optional_bool_option(command, "ping_users").unwrap_or(false),
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
            Ok(interaction_response(format!(
                "Event preview delivered to {delivered} channel(s)."
            )))
        }
        other => Ok(interaction_response(format!("Unknown command `{other}`."))),
    }
}

fn format_guild_validation(report: &crate::models::GuildValidationReport) -> String {
    let mut lines = Vec::new();
    let status = if report.valid { "Valid" } else { "Invalid" };
    lines.push(format!("**Guild Validation: {status}**"));

    for entry in &report.entries {
        let checked = entry.channels_checked + entry.roles_checked + entry.categories_checked;
        if entry.missing.is_empty() {
            lines.push(format!(
                "- `{}` ({}): {checked} resources checked, all present",
                entry.config_name, entry.guild_id
            ));
        } else {
            lines.push(format!(
                "- `{}` ({}): {checked} resources checked, {} missing",
                entry.config_name,
                entry.guild_id,
                entry.missing.len()
            ));
            for missing in &entry.missing {
                lines.push(format!("  - {missing}"));
            }
        }
    }

    lines.join("\n")
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

fn ensure_operator_command(
    state: &AppState,
    command: &CommandInteraction,
) -> Result<(), CommandError> {
    if is_operator(
        state,
        command.user.id.get(),
        command
            .member
            .as_ref()
            .map(|member| member.roles.as_slice()),
    ) {
        Ok(())
    } else {
        Err(CommandError::Unauthorized)
    }
}

fn ensure_operator_component(
    state: &AppState,
    component: &ComponentInteraction,
) -> Result<(), AppError> {
    if is_operator(
        state,
        component.user.id.get(),
        component
            .member
            .as_ref()
            .map(|member| member.roles.as_slice()),
    ) {
        Ok(())
    } else {
        Err(AppError::Unauthorized)
    }
}

fn is_operator(state: &AppState, user_id: u64, roles: Option<&[serenity::all::RoleId]>) -> bool {
    if state.config.operator_user_ids.is_empty() && state.config.operator_role_ids.is_empty() {
        return true;
    }

    if state
        .config
        .operator_user_ids
        .iter()
        .any(|operator_id| operator_id.get() == user_id)
    {
        return true;
    }

    roles.is_some_and(|roles| {
        roles
            .iter()
            .any(|role_id| state.config.operator_role_ids.contains(role_id))
    })
}

fn validated_title(command: &CommandInteraction) -> Result<String, CommandError> {
    let value = required_string_option(command, "title")?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::Validation(
            "Title cannot be empty or whitespace.".into(),
        ));
    }
    if trimmed.len() > MAX_TITLE_LEN {
        return Err(CommandError::Validation(format!(
            "Title exceeds maximum length of {MAX_TITLE_LEN} characters."
        )));
    }
    Ok(trimmed.to_string())
}

fn validated_body(command: &CommandInteraction) -> Result<String, CommandError> {
    let value = required_string_option(command, "body")?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::Validation(
            "Body cannot be empty or whitespace.".into(),
        ));
    }
    if trimmed.len() > MAX_BODY_LEN {
        return Err(CommandError::Validation(format!(
            "Body exceeds maximum length of {MAX_BODY_LEN} characters."
        )));
    }
    Ok(trimmed.to_string())
}

fn validated_event_id(command: &CommandInteraction) -> Result<String, CommandError> {
    let value = required_string_option(command, "event_id")?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::Validation(
            "Event ID cannot be empty or whitespace.".into(),
        ));
    }
    if trimmed.len() > MAX_EVENT_ID_LEN {
        return Err(CommandError::Validation(format!(
            "Event ID exceeds maximum length of {MAX_EVENT_ID_LEN} characters."
        )));
    }
    Ok(trimmed.to_string())
}

fn validated_optional_url(
    command: &CommandInteraction,
    name: &str,
) -> Result<Option<String>, CommandError> {
    let Some(value) = optional_string_option(command, name) else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed = Url::parse(trimmed)
        .map_err(|_| CommandError::Validation(format!("`{name}` is not a valid URL.")))?;
    match parsed.scheme() {
        "http" | "https" => Ok(Some(trimmed.to_string())),
        other => Err(CommandError::Validation(format!(
            "`{name}` must use http or https (found `{other}`)."
        ))),
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

fn required_string_option(
    command: &CommandInteraction,
    name: &str,
) -> Result<String, CommandError> {
    optional_string_option(command, name)
        .ok_or_else(|| CommandError::Validation(format!("Missing required option `{name}`.")))
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

fn help_response() -> EditInteractionResponse {
    EditInteractionResponse::new().embed(
        CreateEmbed::new()
            .color(OPERATOR_INFO_COLOR)
            .title("Discord Bot Commands")
            .field(
                "User Commands",
                "`/ping`, `/health`, `/whoami`, `/link-status`, `/profile`, `/help`",
                false,
            )
            .field(
                "Operator Commands",
                "`/lookup-discord`, `/lookup-cid`, `/list-linked-users`, `/unlink-discord`, `/sync-roles`, `/sync-config`, `/validate-guild`, `/register-commands`",
                false,
            )
            .field(
                "Preview Tools",
                "`/post-announcement-preview`, `/post-event-preview`",
                false,
            ),
    )
}

fn link_status_response(lookup: &DiscordUserLookupResponse) -> EditInteractionResponse {
    let mut embed = CreateEmbed::new()
        .color(if lookup.linked {
            USER_INFO_COLOR
        } else {
            WARNING_COLOR
        })
        .title("Discord Link Status")
        .field(
            "Status",
            if lookup.linked {
                "Linked"
            } else {
                "Not linked"
            },
            true,
        );

    if lookup.linked {
        if let Some(cid) = lookup.cid {
            embed = embed.field("CID", cid.to_string(), true);
        }
        if let Some(name) = lookup.name.as_deref() {
            embed = embed.field("Name", name, true);
        }
    } else if let Some(link_url) = lookup.link_url.as_deref() {
        embed = embed.field("Link URL", link_url, false);
    }

    EditInteractionResponse::new().embed(embed)
}

fn link_lookup_response(
    title: &str,
    description: &str,
    lookup: &DiscordUserLookupResponse,
    include_link_url: bool,
    public_only: bool,
) -> EditInteractionResponse {
    let mut embed = CreateEmbed::new()
        .color(if lookup.linked {
            USER_INFO_COLOR
        } else {
            WARNING_COLOR
        })
        .title(title)
        .description(description)
        .field("Linked", if lookup.linked { "Yes" } else { "No" }, true);

    if let Some(cid) = lookup.cid {
        embed = embed.field("CID", cid.to_string(), true);
    }
    if let Some(name) = lookup.name.as_deref() {
        embed = embed.field("Name", name, true);
    }
    if let Some(rating) = lookup.rating.as_deref() {
        embed = embed.field("Rating", rating, true);
    }
    if let Some(controller_status) = lookup.controller_status.as_deref() {
        embed = embed.field("Controller Status", controller_status, true);
    }
    if !public_only {
        if let Some(membership_status) = lookup.membership_status.as_deref() {
            embed = embed.field("Membership Status", membership_status, true);
        }
        if let Some(user_id) = lookup.user_id.as_deref() {
            embed = embed.field("Internal User ID", user_id, false);
        }
        if let Some(discord_id) = lookup.discord_id.as_deref() {
            embed = embed.field("Discord ID", discord_id, true);
        }
        if let Some(username) = lookup.discord_username.as_deref() {
            embed = embed.field("Discord Username", username, true);
        }
        if let Some(global_name) = lookup.discord_global_name.as_deref() {
            embed = embed.field("Discord Global Name", global_name, true);
        }
        if let Some(linked_at) = lookup.linked_at {
            embed = embed.field("Linked At", format_timestamp(linked_at), true);
        }
    }
    if include_link_url
        && !lookup.linked
        && let Some(link_url) = lookup.link_url.as_deref()
    {
        embed = embed.field("Link URL", link_url, false);
    }

    EditInteractionResponse::new().embed(embed)
}

fn linked_users_embed(response: &LinkedDiscordUsersResponse) -> CreateEmbed {
    let description = if response.items.is_empty() {
        "No linked users found.".to_string()
    } else {
        response
            .items
            .iter()
            .map(|item| {
                let linked_at = item
                    .linked_at
                    .map(format_timestamp)
                    .unwrap_or_else(|| "unknown".to_string());
                let username = item
                    .discord_username
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());
                format!(
                    "`{}` | {} | Discord `{}` | linked {}",
                    item.cid, item.name, username, linked_at
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    CreateEmbed::new()
        .color(OPERATOR_INFO_COLOR)
        .title("Linked Discord Users")
        .description(description)
        .field(
            "Page",
            format!("{} / {}", response.page, response.total_pages.max(1)),
            true,
        )
        .field("Total", response.total.to_string(), true)
}

fn linked_users_components(response: &LinkedDiscordUsersResponse) -> Vec<CreateActionRow> {
    vec![CreateActionRow::Buttons(vec![
        CreateButton::new(format!(
            "linked-users:{}",
            response.page.saturating_sub(1).max(1)
        ))
        .label("Previous")
        .style(ButtonStyle::Secondary)
        .disabled(!response.has_prev),
        CreateButton::new(format!("linked-users:{}", response.page + 1))
            .label("Next")
            .style(ButtonStyle::Primary)
            .disabled(!response.has_next),
    ])]
}

async fn lookup_profile_target(
    state: &AppState,
    target: &str,
) -> Result<DiscordUserLookupResponse, CommandError> {
    if let Some(discord_id) = parse_mention_or_discord_id(target) {
        return state
            .osmium
            .lookup_discord_user(discord_id)
            .await
            .map_err(Into::into);
    }

    let cid = parse_positive_i64(target, "cid_or_mention")?;
    state.osmium.admin_lookup_cid(cid).await.map_err(Into::into)
}

async fn resolve_cid_target(state: &AppState, target: &str) -> Result<i64, CommandError> {
    if let Some(discord_id) = parse_mention_or_discord_id(target) {
        let lookup = state.osmium.lookup_discord_user(discord_id).await?;
        return lookup.cid.ok_or_else(|| {
            CommandError::Validation("That Discord user does not have a linked CID.".to_string())
        });
    }

    parse_positive_i64(target, "cid_or_mention")
}

fn parse_discord_id_target(target: &str) -> Result<u64, CommandError> {
    parse_mention_or_discord_id(target).ok_or_else(|| {
        CommandError::Validation(
            "Expected a Discord mention or numeric Discord user ID.".to_string(),
        )
    })
}

fn parse_mention_or_discord_id(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if let Some(inner) = trimmed
        .strip_prefix("<@")
        .and_then(|value| value.strip_suffix('>'))
    {
        return inner.trim_start_matches('!').parse::<u64>().ok();
    }

    let numeric = trimmed.parse::<u64>().ok()?;
    if trimmed.len() >= 15 {
        Some(numeric)
    } else {
        None
    }
}

fn parse_positive_i64(value: &str, field: &str) -> Result<i64, CommandError> {
    value
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| CommandError::Validation(format!("`{field}` must be a positive integer.")))
}

fn format_timestamp(value: DateTime<Utc>) -> String {
    value.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn user_label(name: &str, user_id: u64) -> String {
    format!("{name} ({user_id})")
}
