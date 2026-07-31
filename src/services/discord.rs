use std::sync::Arc;

use async_trait::async_trait;
use reqwest::StatusCode;
use serenity::{
    Error as SerenityError,
    all::{
        ButtonStyle, ChannelId, CreateActionRow, CreateAllowedMentions, CreateButton, CreateEmbed,
        CreateEmbedFooter, CreateMessage, EditMessage, GuildId, Http, MessageId, RoleId,
    },
    builder::{CreateInteractionResponseMessage, EditInteractionResponse},
    model::Timestamp,
};

use crate::{
    commands,
    errors::{AppError, AppResult},
    models::{
        AnnouncementPayload, AuditEvent, BreakBoardMessageState, BreakBoardRequestState,
        DiscordConfigBundle, Event, EventPosition, EventPositionPostingPayload,
        GuildValidationEntry, GuildValidationReport, ImpromptuSelectorMessageState,
        ResolvedChannelTarget, ResolvedRoleTarget, StaffupOfflineEmbed, StaffupOnlineEmbed,
    },
};

pub const IMPROMPTU_SELECTOR_CHANNEL_NAME: &str = "impromptu_training";
pub const IMPROMPTU_SELECTOR_ROLE_PREFIX: &str = "impromptu_";
pub const IMPROMPTU_SELECTOR_CUSTOM_ID_PREFIX: &str = "impromptu-role";
pub const BREAK_BOARD_CHANNEL_NAME: &str = "break_board";
pub const BREAK_BOARD_ROLE_PREFIX: &str = "break_board_";
pub const BREAK_BOARD_PREF_CUSTOM_ID_PREFIX: &str = "break-pref-role";
pub const BREAK_BOARD_OPEN_CUSTOM_ID_PREFIX: &str = "break-request-open";
pub const BREAK_BOARD_MODAL_CUSTOM_ID_PREFIX: &str = "break-request-modal";
pub const BREAK_BOARD_REQUEST_CLAIM_CUSTOM_ID_PREFIX: &str = "break-request-claim";
pub const BREAK_BOARD_REQUEST_DELETE_CUSTOM_ID_PREFIX: &str = "break-request-delete";
pub const BREAK_BOARD_REQUEST_COMPLETE_CUSTOM_ID_PREFIX: &str = "break-request-complete";
const BREAK_BOARD_EMBED_COLOR: u32 = 0x2D6CDF;

#[async_trait]
pub trait DiscordDelivery: Send + Sync {
    async fn send_announcement(
        &self,
        bundle: &DiscordConfigBundle,
        payload: &AnnouncementPayload,
    ) -> AppResult<usize>;

    async fn send_event_position_posting(
        &self,
        bundle: &DiscordConfigBundle,
        payload: &EventPositionPostingPayload,
        event: &Event,
        positions: &[EventPosition],
    ) -> AppResult<usize>;

    async fn register_commands(&self, guild_ids: &[GuildId]) -> AppResult<usize>;

    async fn validate_guilds(
        &self,
        bundle: &DiscordConfigBundle,
    ) -> AppResult<GuildValidationReport>;

    async fn send_staffup_online(
        &self,
        bundle: &DiscordConfigBundle,
        payload: &StaffupOnlineEmbed,
    ) -> AppResult<usize>;

    async fn send_staffup_offline(
        &self,
        bundle: &DiscordConfigBundle,
        payload: &StaffupOfflineEmbed,
    ) -> AppResult<usize>;

    async fn send_audit_event(
        &self,
        bundle: &DiscordConfigBundle,
        event: &AuditEvent,
    ) -> AppResult<usize>;

    async fn impromptu_selector_message_exists(
        &self,
        state: &ImpromptuSelectorMessageState,
    ) -> AppResult<bool>;

    async fn create_impromptu_selector_message(
        &self,
        target: &ResolvedChannelTarget,
        roles: &[ResolvedRoleTarget],
    ) -> AppResult<ImpromptuSelectorMessageState>;

    async fn refresh_impromptu_selector_message(
        &self,
        state: &ImpromptuSelectorMessageState,
        roles: &[ResolvedRoleTarget],
    ) -> AppResult<()>;

    async fn break_board_messages_exist(&self, state: &BreakBoardMessageState) -> AppResult<bool>;

    async fn create_break_board_messages(
        &self,
        target: &ResolvedChannelTarget,
        roles: &[ResolvedRoleTarget],
    ) -> AppResult<BreakBoardMessageState>;

    async fn refresh_break_board_messages(
        &self,
        state: &BreakBoardMessageState,
        roles: &[ResolvedRoleTarget],
    ) -> AppResult<()>;

    async fn create_break_board_request_message(
        &self,
        request: &BreakBoardRequestState,
    ) -> AppResult<u64>;

    async fn mark_break_board_request_claimed(
        &self,
        request: &BreakBoardRequestState,
    ) -> AppResult<()>;

    async fn create_break_board_claim_message(
        &self,
        request: &BreakBoardRequestState,
    ) -> AppResult<u64>;

    async fn delete_break_board_message(&self, channel_id: u64, message_id: u64) -> AppResult<()>;
}

#[derive(Clone)]
pub struct SerenityDiscordService {
    http: Arc<Http>,
    embed_footer_icon_url: String,
}

impl SerenityDiscordService {
    pub fn new(http: Arc<Http>, embed_footer_icon_url: String) -> Self {
        Self {
            http,
            embed_footer_icon_url,
        }
    }

    fn apply_standard_footer(&self, embed: CreateEmbed) -> CreateEmbed {
        embed
            .footer(CreateEmbedFooter::new("vZDC").icon_url(self.embed_footer_icon_url.clone()))
            .timestamp(Timestamp::now())
    }

    pub fn preview_announcement_embed(&self, payload: &AnnouncementPayload) -> CreateEmbed {
        let mut embed = CreateEmbed::new()
            .title(payload.title.clone())
            .description(payload.body_markdown.clone())
            .field(
                "Requested By CID",
                payload.requested_by_cid.to_string(),
                true,
            );

        if let Some(url) = payload.details_url.as_deref() {
            embed = embed.field("Details", url, false);
        }

        self.apply_standard_footer(embed)
    }

    pub fn preview_event_embed(
        &self,
        _payload: &EventPositionPostingPayload,
        event: &Event,
        positions: &[EventPosition],
    ) -> CreateEmbed {
        let mut embed = CreateEmbed::new()
            .color(0x2E86DE)
            .title(event.title.clone());

        // Link back to the event signup page at the top: make the title clickable
        // and lead the description with a prominent link.
        let signup_url = event_signup_url(event);
        if let Some(url) = &signup_url {
            embed = embed.url(url.clone());
        }

        // Discord caps an embed at 6000 chars across title+description+fields+
        // footer; keep a running budget so a long description plus many position
        // groups can't blow past it and get the whole message rejected.
        let mut budget = 6000usize.saturating_sub(event.title.chars().count() + 64);

        let mut description_parts = Vec::new();
        if let Some(url) = &signup_url {
            description_parts.push(format!("**[📋 Sign up / view event »]({url})**"));
        }
        if let Some(description) = event
            .description
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            description_parts.push(description.to_string());
        }
        if !description_parts.is_empty() {
            let value = truncate_chars(&description_parts.join("\n\n"), budget.min(4000));
            budget = budget.saturating_sub(value.chars().count());
            embed = embed.description(value);
        }

        // Discord timestamps render in each viewer's local timezone.
        embed = embed
            .field(
                "Start",
                format!("<t:{}:F>", event.starts_at.timestamp()),
                true,
            )
            .field("End", format!("<t:{}:F>", event.ends_at.timestamp()), true);
        budget = budget.saturating_sub(48);

        let groups = group_positions_by_category(positions);
        if groups.is_empty() {
            embed = embed.field("Positions", "No positions are currently published.", false);
        } else {
            for (label, lines) in groups {
                let name = format!("{label} ({})", lines.len());
                let name_cost = name.chars().count();
                if budget <= name_cost + 8 {
                    break;
                }
                let value = truncate_chars(&lines.join("\n"), 1024.min(budget - name_cost));
                budget = budget.saturating_sub(name_cost + value.chars().count());
                embed = embed.field(name, value, false);
            }
        }

        if let Some(url) = event_banner_url(event) {
            embed = embed.image(url);
        }

        self.apply_standard_footer(embed)
    }

    pub fn staffup_online_embed(&self, payload: &StaffupOnlineEmbed) -> CreateEmbed {
        self.apply_standard_footer(
            CreateEmbed::new()
                .color(0x2ECC71)
                .title(format!("{} - Online", payload.callsign))
                .field("Name", payload.name.clone(), false)
                .field("Frequency", payload.frequency.clone(), true)
                .field("Logon Time", format!("<t:{}:t>", payload.logon_unix), true),
        )
    }

    pub fn staffup_offline_embed(&self, payload: &StaffupOfflineEmbed) -> CreateEmbed {
        let mut embed = CreateEmbed::new()
            .color(0xE74C3C)
            .title(format!("{} - Offline", payload.callsign))
            .field("Name", payload.name.clone(), false)
            .field("Frequency", payload.frequency.clone(), true);

        if let Some(logon_unix) = payload.logon_unix {
            embed = embed.field("Logon Time", format!("<t:{logon_unix}:t>"), true);
        }

        embed = embed.field(
            "Logoff Time",
            format!("<t:{}:t>", payload.logoff_unix),
            true,
        );

        if let Some(duration) = payload.duration.as_deref() {
            embed = embed.field("Duration", duration, true);
        }

        self.apply_standard_footer(embed)
    }

    pub fn impromptu_selector_embed(&self) -> CreateEmbed {
        self.apply_standard_footer(
            CreateEmbed::new()
                .title("Impromptu Selector")
                .description(
                    "As a perk of being a ZDC home controller, you may join a role that our training staff can use to alert you of an available impromptu session. These sessions will usually be the same day. Keep reading for instructions on how to do this.\n\nThis will add you to a role that the training staff can ping in this channel. If you get an alert that there is an open session, you may PM the instructor who posted. These sessions are first come – first serve. The instructor will delete the message or react to it to indicate it has been taken.\n\nClick the buttons below to opt in or out of receiving notifications for the training that you are seeking\n• If you have the role, clicking the button will remove it.\n• If you don't have the role, clicking the button will add it."
                ),
        )
    }

    pub fn impromptu_selector_components(
        &self,
        roles: &[ResolvedRoleTarget],
    ) -> Vec<CreateActionRow> {
        roles
            .chunks(5)
            .map(|chunk| {
                CreateActionRow::Buttons(
                    chunk
                        .iter()
                        .map(|role| {
                            CreateButton::new(impromptu_selector_custom_id(
                                role.guild_id,
                                role.role_id,
                            ))
                            .label(impromptu_selector_label(&role.name))
                            .style(ButtonStyle::Secondary)
                        })
                        .collect(),
                )
            })
            .collect()
    }

    pub fn break_board_preferences_embed(&self) -> CreateEmbed {
        self.apply_standard_footer(
            CreateEmbed::new()
                .color(BREAK_BOARD_EMBED_COLOR)
                .title("🔔 Controller Notification Preferences 🔔")
                .description(
                    "Click the buttons below to opt in or out of receiving notifications when controllers request a break for specific positions.\n\n• If you have the role, clicking the button will remove it.\n• If you don't have the role, clicking the button will add it.\nYour role preferences determine which break requests you see."
                ),
        )
    }

    pub fn break_board_request_embed(&self) -> CreateEmbed {
        self.apply_standard_footer(
            CreateEmbed::new()
                .color(BREAK_BOARD_EMBED_COLOR)
                .title("Controller Break Notification System")
                .description(
                    "Use the buttons below to request a break for specific positions.\n\nThe request form will ask how much time remains before you close, with optional fields for position and notes.\nThe posted message will include `Claim` and `Delete` controls. Once claimed, a `Complete / Delete` control is used to finish the handoff."
                ),
        )
    }

    pub fn break_board_preference_components(
        &self,
        roles: &[ResolvedRoleTarget],
    ) -> Vec<CreateActionRow> {
        break_board_role_rows(roles, |role| {
            break_board_pref_custom_id(role.guild_id, role.role_id)
        })
    }

    pub fn break_board_request_components(
        &self,
        roles: &[ResolvedRoleTarget],
    ) -> Vec<CreateActionRow> {
        break_board_role_rows(roles, |role| {
            break_board_open_custom_id(role.guild_id, role.role_id)
        })
    }

    pub fn break_board_request_post_embed(&self, request: &BreakBoardRequestState) -> CreateEmbed {
        let mut embed = CreateEmbed::new()
            .color(BREAK_BOARD_EMBED_COLOR)
            .title("Break Requested")
            .field("Requested By", request.requester_mention.clone(), true)
            .field("Position", request.display_position.clone(), true)
            .field(
                "Time Before Close",
                humanize_minutes(request.minutes_before_close),
                true,
            );

        if let Some(notes) = request.notes.as_deref() {
            embed = embed.field("Notes", notes, false);
        }

        if let Some(claimed_by) = request.claimed_by_mention.as_deref() {
            embed = embed.field("Claimed By", claimed_by, true);
        }

        self.apply_standard_footer(embed)
    }

    pub fn break_board_request_post_components(
        &self,
        request_message_id: u64,
    ) -> Vec<CreateActionRow> {
        vec![CreateActionRow::Buttons(vec![
            CreateButton::new(break_board_claim_custom_id(request_message_id))
                .label("Claim")
                .style(ButtonStyle::Primary),
            CreateButton::new(break_board_delete_custom_id(request_message_id))
                .label("Delete")
                .style(ButtonStyle::Primary),
        ])]
    }

    pub fn break_board_claim_embed(&self, request: &BreakBoardRequestState) -> CreateEmbed {
        let claimer = request
            .claimed_by_mention
            .as_deref()
            .unwrap_or("Unknown claimer");

        self.apply_standard_footer(
            CreateEmbed::new()
                .color(BREAK_BOARD_EMBED_COLOR)
                .title("Break Request Claimed")
                .description("The request has been claimed and handoff is in progress.")
                .field("Requested By", request.requester_mention.clone(), true)
                .field("Claimed By", claimer, true)
                .field("Position", request.display_position.clone(), true),
        )
    }

    pub fn break_board_claim_components(&self, request_message_id: u64) -> Vec<CreateActionRow> {
        vec![CreateActionRow::Buttons(vec![
            CreateButton::new(break_board_complete_custom_id(request_message_id))
                .label("Complete / Delete")
                .style(ButtonStyle::Primary),
        ])]
    }

    pub fn audit_embed(&self, event: &AuditEvent) -> CreateEmbed {
        let mut embed = CreateEmbed::new()
            .color(event.color)
            .title(event.kind.title())
            .description(event.summary.clone())
            .field("Subject", event.subject_label.clone(), false);

        if let Some(channel_id) = event.channel_id {
            embed = embed.field("Channel", format!("<#{channel_id}>"), true);
        }

        embed = embed.field(
            "Actor",
            event
                .actor_label
                .clone()
                .unwrap_or_else(|| "Unknown".to_string()),
            true,
        );

        for (name, value) in &event.details {
            embed = embed.field(name.clone(), value.clone(), false);
        }

        self.apply_standard_footer(embed)
    }
}

#[async_trait]
impl DiscordDelivery for SerenityDiscordService {
    async fn send_announcement(
        &self,
        bundle: &DiscordConfigBundle,
        payload: &AnnouncementPayload,
    ) -> AppResult<usize> {
        let channel = payload
            .channel
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("announcements");
        let targets = bundle.resolve_announcement_targets(channel)?;
        tracing::info!(
            channel,
            target_count = targets.len(),
            requested_by_cid = payload.requested_by_cid,
            "sending announcement embed to discord targets"
        );
        let embed = self.preview_announcement_embed(payload);
        send_embed_to_targets(&self.http, &targets, embed).await
    }

    async fn send_event_position_posting(
        &self,
        bundle: &DiscordConfigBundle,
        payload: &EventPositionPostingPayload,
        event: &Event,
        positions: &[EventPosition],
    ) -> AppResult<usize> {
        let targets = bundle.resolve_event_posting_targets()?;
        tracing::info!(
            target_count = targets.len(),
            event_id = %payload.event_id,
            requested_by_cid = payload.requested_by_cid,
            positions = positions.len(),
            "sending event position posting embed to discord targets"
        );
        let embed = self.preview_event_embed(payload, event, positions);
        let ping_ids = if payload.ping_users {
            event_ping_mentions(positions)
        } else {
            Vec::new()
        };
        send_event_posting_to_targets(&self.http, &event.id, &targets, embed, &ping_ids).await
    }

    async fn register_commands(&self, guild_ids: &[GuildId]) -> AppResult<usize> {
        let commands = commands::definitions();
        let mut updated = 0usize;
        tracing::info!(target_count = guild_ids.len(), "registering slash commands");

        for guild_id in guild_ids {
            tracing::debug!(
                guild_id = guild_id.get(),
                "registering slash commands for guild"
            );
            guild_id.set_commands(&self.http, commands.clone()).await?;
            updated += 1;
        }

        tracing::info!(delivered = updated, "registered slash commands");
        Ok(updated)
    }

    async fn validate_guilds(
        &self,
        bundle: &DiscordConfigBundle,
    ) -> AppResult<GuildValidationReport> {
        bundle.validate_shape()?;
        tracing::info!(
            configs = bundle.configs.len(),
            "validating configured guild resources"
        );

        let mut entries = Vec::new();

        for config in &bundle.configs {
            let guild_id = config
                .guild_id
                .as_deref()
                .ok_or_else(|| {
                    AppError::Config(format!("config `{}` missing guild id", config.name))
                })?
                .parse::<u64>()
                .map_err(|_| {
                    AppError::Config(format!("config `{}` has invalid guild id", config.name))
                })?;
            let guild_id = GuildId::new(guild_id);
            tracing::debug!(guild_id = guild_id.get(), config_name = %config.name, "validating guild config");
            let guild = guild_id.to_partial_guild(&self.http).await?;
            let channels = guild.channels(&self.http).await?;
            let roles = &guild.roles;

            let mut missing = Vec::new();
            let config_channels: Vec<_> = bundle
                .channels
                .iter()
                .filter(|channel| channel.discord_config_id == config.id)
                .collect();
            let config_roles: Vec<_> = bundle
                .roles
                .iter()
                .filter(|role| role.discord_config_id == config.id)
                .collect();
            let config_categories: Vec<_> = bundle
                .categories
                .iter()
                .filter(|category| category.discord_config_id == config.id)
                .collect();

            for channel in &config_channels {
                let channel_id = ChannelId::new(
                    channel
                        .channel_id
                        .parse::<u64>()
                        .map_err(|_| AppError::Config("invalid channel id".into()))?,
                );
                if !channels.contains_key(&channel_id) {
                    missing.push(format!(
                        "channel `{}` ({})",
                        channel.name, channel.channel_id
                    ));
                }
            }

            for role in &config_roles {
                let role_id = RoleId::new(
                    role.role_id
                        .parse::<u64>()
                        .map_err(|_| AppError::Config("invalid role id".into()))?,
                );
                if !roles.contains_key(&role_id) {
                    missing.push(format!("role `{}` ({})", role.name, role.role_id));
                }
            }

            for category in &config_categories {
                let category_id = ChannelId::new(
                    category
                        .category_id
                        .parse::<u64>()
                        .map_err(|_| AppError::Config("invalid category id".into()))?,
                );
                if !channels.contains_key(&category_id) {
                    missing.push(format!(
                        "category `{}` ({})",
                        category.name, category.category_id
                    ));
                }
            }

            entries.push(GuildValidationEntry {
                config_name: config.name.clone(),
                guild_id: guild.id.to_string(),
                channels_checked: config_channels.len(),
                roles_checked: config_roles.len(),
                categories_checked: config_categories.len(),
                missing,
            });
        }

        tracing::info!(
            valid = entries.iter().all(|entry| entry.missing.is_empty()),
            entries = entries.len(),
            "completed guild validation"
        );
        Ok(GuildValidationReport {
            valid: entries.iter().all(|entry| entry.missing.is_empty()),
            entries,
        })
    }

    async fn send_staffup_online(
        &self,
        bundle: &DiscordConfigBundle,
        payload: &StaffupOnlineEmbed,
    ) -> AppResult<usize> {
        let targets = bundle.resolve_staffup_targets()?;
        tracing::info!(
            target_count = targets.len(),
            "sending staffup online embed to discord targets"
        );
        send_embed_to_targets(&self.http, &targets, self.staffup_online_embed(payload)).await
    }

    async fn send_staffup_offline(
        &self,
        bundle: &DiscordConfigBundle,
        payload: &StaffupOfflineEmbed,
    ) -> AppResult<usize> {
        let targets = bundle.resolve_staffup_targets()?;
        tracing::info!(
            target_count = targets.len(),
            "sending staffup offline embed to discord targets"
        );
        send_embed_to_targets(&self.http, &targets, self.staffup_offline_embed(payload)).await
    }

    async fn send_audit_event(
        &self,
        bundle: &DiscordConfigBundle,
        event: &AuditEvent,
    ) -> AppResult<usize> {
        let targets = bundle.resolve_audit_log_targets()?;
        tracing::info!(
            kind = ?event.kind,
            guild_id = event.guild_id,
            target_count = targets.len(),
            "sending audit embed to discord targets"
        );
        send_embed_to_targets(&self.http, &targets, self.audit_embed(event)).await
    }

    async fn impromptu_selector_message_exists(
        &self,
        state: &ImpromptuSelectorMessageState,
    ) -> AppResult<bool> {
        let result = ChannelId::new(state.channel_id)
            .message(&self.http, MessageId::new(state.message_id))
            .await;

        match result {
            Ok(_) => Ok(true),
            Err(SerenityError::Http(error))
                if error.status_code() == Some(StatusCode::NOT_FOUND) =>
            {
                tracing::warn!(
                    guild_id = state.guild_id,
                    channel_id = state.channel_id,
                    request_message_id = state.message_id,
                    "impromptu selector message not found"
                );
                Ok(false)
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn create_impromptu_selector_message(
        &self,
        target: &ResolvedChannelTarget,
        roles: &[ResolvedRoleTarget],
    ) -> AppResult<ImpromptuSelectorMessageState> {
        tracing::info!(
            guild_id = target.guild_id,
            channel_id = target.channel_id,
            roles = roles.len(),
            "creating impromptu selector message"
        );
        let message = ChannelId::new(target.channel_id)
            .send_message(
                &self.http,
                CreateMessage::new()
                    .embed(self.impromptu_selector_embed())
                    .components(self.impromptu_selector_components(roles))
                    .allowed_mentions(suppress_mentions()),
            )
            .await
            .map_err(|error| {
                AppError::Discord(format!(
                    "failed posting impromptu selector to config `{}` channel {}: {error}",
                    target.config_name, target.channel_id
                ))
            })?;

        Ok(ImpromptuSelectorMessageState {
            guild_id: target.guild_id,
            channel_id: target.channel_id,
            message_id: message.id.get(),
        })
    }

    async fn refresh_impromptu_selector_message(
        &self,
        state: &ImpromptuSelectorMessageState,
        roles: &[ResolvedRoleTarget],
    ) -> AppResult<()> {
        tracing::info!(
            guild_id = state.guild_id,
            channel_id = state.channel_id,
            request_message_id = state.message_id,
            roles = roles.len(),
            "refreshing impromptu selector message"
        );
        ChannelId::new(state.channel_id)
            .edit_message(
                &self.http,
                MessageId::new(state.message_id),
                EditMessage::new()
                    .embed(self.impromptu_selector_embed())
                    .components(self.impromptu_selector_components(roles)),
            )
            .await
            .map_err(|error| {
                AppError::Discord(format!(
                    "failed refreshing impromptu selector message {} in channel {}: {error}",
                    state.message_id, state.channel_id
                ))
            })?;
        Ok(())
    }

    async fn break_board_messages_exist(&self, state: &BreakBoardMessageState) -> AppResult<bool> {
        let preference_exists =
            message_exists(&self.http, state.channel_id, state.preference_message_id).await?;
        if !preference_exists {
            tracing::warn!(
                guild_id = state.guild_id,
                channel_id = state.channel_id,
                request_message_id = state.preference_message_id,
                "break board preference message not found"
            );
            return Ok(false);
        }

        message_exists(&self.http, state.channel_id, state.request_message_id).await
    }

    async fn create_break_board_messages(
        &self,
        target: &ResolvedChannelTarget,
        roles: &[ResolvedRoleTarget],
    ) -> AppResult<BreakBoardMessageState> {
        tracing::info!(
            guild_id = target.guild_id,
            channel_id = target.channel_id,
            roles = roles.len(),
            "creating break board messages"
        );
        let preference_message = ChannelId::new(target.channel_id)
            .send_message(
                &self.http,
                CreateMessage::new()
                    .embed(self.break_board_preferences_embed())
                    .components(self.break_board_preference_components(roles))
                    .allowed_mentions(suppress_mentions()),
            )
            .await
            .map_err(|error| {
                AppError::Discord(format!(
                    "failed posting break board preference message to config `{}` channel {}: {error}",
                    target.config_name, target.channel_id
                ))
            })?;

        let request_message = ChannelId::new(target.channel_id)
            .send_message(
                &self.http,
                CreateMessage::new()
                    .embed(self.break_board_request_embed())
                    .components(self.break_board_request_components(roles))
                    .allowed_mentions(suppress_mentions()),
            )
            .await
            .map_err(|error| {
                AppError::Discord(format!(
                    "failed posting break board request message to config `{}` channel {}: {error}",
                    target.config_name, target.channel_id
                ))
            })?;

        Ok(BreakBoardMessageState {
            guild_id: target.guild_id,
            channel_id: target.channel_id,
            preference_message_id: preference_message.id.get(),
            request_message_id: request_message.id.get(),
        })
    }

    async fn refresh_break_board_messages(
        &self,
        state: &BreakBoardMessageState,
        roles: &[ResolvedRoleTarget],
    ) -> AppResult<()> {
        tracing::info!(
            guild_id = state.guild_id,
            channel_id = state.channel_id,
            roles = roles.len(),
            "refreshing break board messages"
        );
        ChannelId::new(state.channel_id)
            .edit_message(
                &self.http,
                MessageId::new(state.preference_message_id),
                EditMessage::new()
                    .embed(self.break_board_preferences_embed())
                    .components(self.break_board_preference_components(roles)),
            )
            .await
            .map_err(|error| {
                AppError::Discord(format!(
                    "failed refreshing break board preference message {} in channel {}: {error}",
                    state.preference_message_id, state.channel_id
                ))
            })?;

        ChannelId::new(state.channel_id)
            .edit_message(
                &self.http,
                MessageId::new(state.request_message_id),
                EditMessage::new()
                    .embed(self.break_board_request_embed())
                    .components(self.break_board_request_components(roles)),
            )
            .await
            .map_err(|error| {
                AppError::Discord(format!(
                    "failed refreshing break board request message {} in channel {}: {error}",
                    state.request_message_id, state.channel_id
                ))
            })?;

        Ok(())
    }

    async fn create_break_board_request_message(
        &self,
        request: &BreakBoardRequestState,
    ) -> AppResult<u64> {
        tracing::info!(
            guild_id = request.guild_id,
            channel_id = request.channel_id,
            role_id = request.role_id,
            user_id = request.requester_user_id,
            "creating break board request message"
        );
        let message = ChannelId::new(request.channel_id)
            .send_message(
                &self.http,
                CreateMessage::new()
                    .content(format!("<@&{}>", request.role_id))
                    .embed(self.break_board_request_post_embed(request))
                    .components(Vec::new())
                    .allowed_mentions(allow_role_mention(request.role_id)),
            )
            .await
            .map_err(|error| {
                AppError::Discord(format!(
                    "failed posting break request message to channel {}: {error}",
                    request.channel_id
                ))
            })?;

        if let Err(error) = ChannelId::new(request.channel_id)
            .edit_message(
                &self.http,
                message.id,
                EditMessage::new()
                    .components(self.break_board_request_post_components(message.id.get())),
            )
            .await
        {
            tracing::warn!(
                ?error,
                channel_id = request.channel_id,
                message_id = message.id.get(),
                "failed to update break request custom ids with final message id"
            );
        }

        Ok(message.id.get())
    }

    async fn mark_break_board_request_claimed(
        &self,
        request: &BreakBoardRequestState,
    ) -> AppResult<()> {
        tracing::info!(
            request_message_id = request.request_message_id,
            guild_id = request.guild_id,
            channel_id = request.channel_id,
            user_id = request.claimed_by_user_id,
            "marking break board request as claimed"
        );
        ChannelId::new(request.channel_id)
            .edit_message(
                &self.http,
                MessageId::new(request.request_message_id),
                EditMessage::new()
                    .content("")
                    .embed(self.break_board_request_post_embed(request))
                    .components(Vec::new()),
            )
            .await
            .map_err(|error| {
                AppError::Discord(format!(
                    "failed marking break request {} as claimed in channel {}: {error}",
                    request.request_message_id, request.channel_id
                ))
            })?;
        Ok(())
    }

    async fn create_break_board_claim_message(
        &self,
        request: &BreakBoardRequestState,
    ) -> AppResult<u64> {
        tracing::info!(
            request_message_id = request.request_message_id,
            guild_id = request.guild_id,
            channel_id = request.channel_id,
            user_id = request.claimed_by_user_id,
            "creating break board claim message"
        );
        let claimer = request.claimed_by_mention.as_deref().unwrap_or("Unknown");
        let mut ping_user_ids = vec![request.requester_user_id];
        if let Some(claimer_id) = request.claimed_by_user_id {
            ping_user_ids.push(claimer_id);
        }
        let message = ChannelId::new(request.channel_id)
            .send_message(
                &self.http,
                CreateMessage::new()
                    .content(format!("{} {}", request.requester_mention, claimer))
                    .embed(self.break_board_claim_embed(request))
                    .components(self.break_board_claim_components(request.request_message_id))
                    .allowed_mentions(allow_user_mentions(&ping_user_ids)),
            )
            .await
            .map_err(|error| {
                AppError::Discord(format!(
                    "failed posting break claim message for request {} in channel {}: {error}",
                    request.request_message_id, request.channel_id
                ))
            })?;
        Ok(message.id.get())
    }

    async fn delete_break_board_message(&self, channel_id: u64, message_id: u64) -> AppResult<()> {
        tracing::debug!(
            channel_id,
            request_message_id = message_id,
            "deleting break board message"
        );
        let result = ChannelId::new(channel_id)
            .delete_message(&self.http, MessageId::new(message_id))
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(SerenityError::Http(error))
                if error.status_code() == Some(StatusCode::NOT_FOUND) =>
            {
                tracing::warn!(
                    channel_id,
                    request_message_id = message_id,
                    "break board message already missing"
                );
                Ok(())
            }
            Err(error) => Err(error.into()),
        }
    }
}

fn suppress_mentions() -> CreateAllowedMentions {
    CreateAllowedMentions::new()
        .empty_roles()
        .empty_users()
        .all_users(false)
        .all_roles(false)
        .everyone(false)
}

fn allow_role_mention(role_id: u64) -> CreateAllowedMentions {
    CreateAllowedMentions::new()
        .roles([RoleId::new(role_id)])
        .empty_users()
        .all_users(false)
        .all_roles(false)
        .everyone(false)
}

fn allow_user_mentions(user_ids: &[u64]) -> CreateAllowedMentions {
    // `CreateAllowedMentions::users` replaces the list, so set every id in one
    // call; adding them one-by-one in a loop would keep only the last user.
    CreateAllowedMentions::new()
        .empty_roles()
        .users(user_ids.iter().map(|&id| serenity::all::UserId::new(id)))
        .all_users(false)
        .all_roles(false)
        .everyone(false)
}

/// Public base URL (reachable by Discord's servers) that serves osmium event
/// banners at `/cdn/{asset_id}`. Unset in local dev (localhost is unreachable by
/// Discord), so the banner is simply omitted there.
pub(crate) fn event_banner_url(event: &Event) -> Option<String> {
    let base = std::env::var("EVENT_BANNER_BASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    let asset = event
        .banner_asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(format!("{}/cdn/{}", base.trim_end_matches('/'), asset))
}

/// Public website base URL (e.g. `https://vzdc.org`) used to link an event
/// posting back to its signup page at `/events/{id}`. Omitted when unset.
fn event_signup_url(event: &Event) -> Option<String> {
    let base = std::env::var("EVENT_SIGNUP_BASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    Some(format!(
        "{}/events/{}",
        base.trim_end_matches('/'),
        event.id
    ))
}

/// A message the bot posted for an event, so a re-post can delete the old one.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PostedEventMessage {
    channel_id: u64,
    message_id: u64,
}

type EventPostings = std::collections::HashMap<String, Vec<PostedEventMessage>>;

/// Serializes the read-modify-write of the event-posting state file so two
/// concurrent postings can't clobber each other's tracked messages.
static EVENT_POSTING_STATE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn event_posting_state_path() -> std::path::PathBuf {
    std::env::var("EVENT_POSTING_STATE_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("data/event_postings.json"))
}

async fn load_event_postings(path: &std::path::Path) -> EventPostings {
    match tokio::fs::read(path).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => EventPostings::default(),
    }
}

async fn save_event_postings(path: &std::path::Path, postings: &EventPostings) -> AppResult<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp_path = path.with_extension("json.tmp");
    tokio::fs::write(&tmp_path, serde_json::to_vec_pretty(postings)?).await?;
    tokio::fs::rename(&tmp_path, path).await?;
    Ok(())
}

/// Group published positions by facility category (GND/TWR/APP/CTR/…), ordered
/// low-to-high, with each entry rendered as `mention (rating) — callsign`.
fn group_positions_by_category(positions: &[EventPosition]) -> Vec<(String, Vec<String>)> {
    use std::collections::BTreeMap;

    let mut groups: BTreeMap<String, Vec<&EventPosition>> = BTreeMap::new();
    for position in positions.iter().filter(|position| position.published) {
        groups
            .entry(position_category(position))
            .or_default()
            .push(position);
    }

    let mut ordered: Vec<(String, Vec<&EventPosition>)> = groups.into_iter().collect();
    ordered.sort_by(|(a, _), (b, _)| {
        category_rank(a)
            .cmp(&category_rank(b))
            .then_with(|| a.cmp(b))
    });

    ordered
        .into_iter()
        .map(|(label, mut items)| {
            items.sort_by(|a, b| position_callsign(a).cmp(position_callsign(b)));
            let lines = items
                .iter()
                .map(|item| format_position_line(item))
                .collect();
            (label, lines)
        })
        .collect()
}

fn position_callsign(position: &EventPosition) -> &str {
    position
        .final_position
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(position.callsign.as_str())
}

fn position_category(position: &EventPosition) -> String {
    // Prefer the callsign suffix (e.g. `HEF_GND` -> `GND`); it maps cleanly to a
    // facility group. `controlling_category` is a broad bucket (LOCAL/TERMINAL/
    // ENROUTE), only useful as a fallback for sector-named positions like `FLTRK`.
    let suffix = position_callsign(position)
        .rsplit('_')
        .next()
        .unwrap_or_default()
        .to_uppercase();
    match suffix.as_str() {
        "DEL" => return "DEL".to_string(),
        "GND" => return "GND".to_string(),
        "TWR" => return "TWR".to_string(),
        "APP" | "DEP" => return "APP".to_string(),
        "CTR" | "FSS" => return "CTR".to_string(),
        "TMU" => return "TMU".to_string(),
        _ => {}
    }

    match position
        .controlling_category
        .as_deref()
        .map(|value| value.trim().to_uppercase())
        .as_deref()
    {
        Some("LOCAL" | "TWR" | "TOWER") => "TWR",
        Some("TERMINAL" | "APP" | "APPROACH") => "APP",
        Some("ENROUTE" | "CTR" | "CENTER") => "CTR",
        Some("GND" | "GROUND") => "GND",
        Some("DEL" | "DELIVERY") => "DEL",
        _ => "OTHER",
    }
    .to_string()
}

fn category_rank(label: &str) -> usize {
    match label {
        "DEL" => 0,
        "GND" => 1,
        "TWR" => 2,
        "APP" => 3,
        "CTR" => 4,
        "TMU" => 5,
        "OTHER" => 8,
        _ => 7,
    }
}

fn format_position_line(position: &EventPosition) -> String {
    let who = if let Some(id) = position
        .user_discord_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        format!("<@{id}>")
    } else if let Some(name) = position
        .user_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        name.to_string()
    } else {
        "*Open*".to_string()
    };

    let rating = position
        .user_rating
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!(" ({value})"))
        .unwrap_or_default();

    format!("{who}{rating} — {}", position_callsign(position))
}

/// Truncate to at most `max` characters, appending an ellipsis when clipped.
pub(crate) fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Collect unique linked-controller mentions for an event roster, for pinging.
fn event_ping_mentions(positions: &[EventPosition]) -> Vec<u64> {
    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();
    for position in positions.iter().filter(|position| position.published) {
        if let Some(id) = position
            .user_discord_id
            .as_deref()
            .and_then(|value| value.trim().parse::<u64>().ok())
            && seen.insert(id)
        {
            ids.push(id);
        }
    }
    ids
}

async fn send_event_posting_to_targets(
    http: &Arc<Http>,
    event_id: &str,
    targets: &[crate::models::ResolvedChannelTarget],
    embed: CreateEmbed,
    ping_ids: &[u64],
) -> AppResult<usize> {
    // Track the message we post per event+channel so a re-post replaces the old
    // one instead of stacking duplicates. Hold the lock across the whole
    // load-modify-write so concurrent postings don't clobber each other.
    let _state_guard = EVENT_POSTING_STATE_LOCK.lock().await;
    let state_path = event_posting_state_path();
    let mut postings = load_event_postings(&state_path).await;
    let previous = postings.remove(event_id).unwrap_or_default();
    let mut current: Vec<PostedEventMessage> = Vec::new();

    // Delete every previously-tracked posting for this event first (covers the
    // case where the posting channel was reconfigured between posts).
    for old in &previous {
        if let Err(error) = ChannelId::new(old.channel_id)
            .delete_message(http, MessageId::new(old.message_id))
            .await
        {
            tracing::warn!(
                ?error,
                channel_id = old.channel_id,
                message_id = old.message_id,
                "failed deleting previous event posting (already gone?)"
            );
        }
    }

    let mut delivered = 0usize;
    for target in targets {
        let channel = ChannelId::new(target.channel_id);

        // The posting itself is embed-only; user mentions render as name pills in
        // the embed but never notify, so the channel stays clean.
        let posted = match channel
            .send_message(
                http,
                CreateMessage::new()
                    .embed(embed.clone())
                    .allowed_mentions(suppress_mentions()),
            )
            .await
        {
            Ok(posted) => posted,
            Err(error) => {
                // Persist whatever we already posted so a later repost can delete
                // it instead of leaving orphaned duplicates behind.
                if !current.is_empty() {
                    postings.insert(event_id.to_string(), current);
                }
                if let Err(save_error) = save_event_postings(&state_path, &postings).await {
                    tracing::warn!(
                        ?save_error,
                        "failed persisting event posting state after send failure"
                    );
                }
                return Err(AppError::Discord(format!(
                    "failed posting to config `{}` channel {}: {error}",
                    target.config_name, target.channel_id
                )));
            }
        };
        current.push(PostedEventMessage {
            channel_id: target.channel_id,
            message_id: posted.id.get(),
        });
        delivered += 1;

        // Notify assigned controllers with a separate mention-only message, then
        // delete it immediately (a "ghost ping"): the notification fires, but the
        // posting embed is the only thing left in the channel.
        if !ping_ids.is_empty() {
            let content = ping_ids
                .iter()
                .map(|id| format!("<@{id}>"))
                .collect::<Vec<_>>()
                .join(" ");
            match channel
                .send_message(
                    http,
                    CreateMessage::new()
                        .content(content)
                        .allowed_mentions(allow_user_mentions(ping_ids)),
                )
                .await
            {
                Ok(ping_message) => {
                    if let Err(error) = channel.delete_message(http, ping_message.id).await {
                        tracing::warn!(
                            ?error,
                            channel_id = target.channel_id,
                            "failed deleting event ping message"
                        );
                    }
                }
                Err(error) => tracing::warn!(
                    ?error,
                    channel_id = target.channel_id,
                    "failed sending event ping message"
                ),
            }
        }
    }

    if !current.is_empty() {
        postings.insert(event_id.to_string(), current);
    }
    if let Err(error) = save_event_postings(&state_path, &postings).await {
        tracing::warn!(?error, "failed persisting event posting message state");
    }

    tracing::info!(delivered, "event position posting delivery completed");
    Ok(delivered)
}

async fn send_embed_to_targets(
    http: &Arc<Http>,
    targets: &[crate::models::ResolvedChannelTarget],
    embed: CreateEmbed,
) -> AppResult<usize> {
    let mut delivered = 0usize;

    for target in targets {
        tracing::debug!(
            guild_id = target.guild_id,
            channel_id = target.channel_id,
            config_name = %target.config_name,
            "sending embed to discord target"
        );
        ChannelId::new(target.channel_id)
            .send_message(
                http,
                CreateMessage::new()
                    .embed(embed.clone())
                    .allowed_mentions(suppress_mentions()),
            )
            .await
            .map_err(|error| {
                AppError::Discord(format!(
                    "failed posting to config `{}` channel {}: {error}",
                    target.config_name, target.channel_id
                ))
            })?;
        delivered += 1;
    }

    tracing::info!(delivered, "discord embed delivery completed");
    Ok(delivered)
}

pub fn interaction_response(content: impl Into<String>) -> EditInteractionResponse {
    EditInteractionResponse::new().content(content.into())
}

pub fn ephemeral_interaction_response(
    content: impl Into<String>,
) -> CreateInteractionResponseMessage {
    CreateInteractionResponseMessage::new()
        .content(content.into())
        .ephemeral(true)
}

pub fn impromptu_selector_custom_id(guild_id: u64, role_id: u64) -> String {
    format!("{IMPROMPTU_SELECTOR_CUSTOM_ID_PREFIX}:{guild_id}:{role_id}")
}

pub fn parse_impromptu_selector_custom_id(custom_id: &str) -> Option<(u64, u64)> {
    let mut parts = custom_id.split(':');
    let prefix = parts.next()?;
    let guild_id = parts.next()?.parse::<u64>().ok()?;
    let role_id = parts.next()?.parse::<u64>().ok()?;

    if prefix != IMPROMPTU_SELECTOR_CUSTOM_ID_PREFIX || parts.next().is_some() {
        return None;
    }

    Some((guild_id, role_id))
}

pub fn impromptu_selector_label(role_name: &str) -> String {
    role_label_from_prefix(role_name, IMPROMPTU_SELECTOR_ROLE_PREFIX)
}

pub fn break_board_pref_custom_id(guild_id: u64, role_id: u64) -> String {
    format!("{BREAK_BOARD_PREF_CUSTOM_ID_PREFIX}:{guild_id}:{role_id}")
}

pub fn break_board_open_custom_id(guild_id: u64, role_id: u64) -> String {
    format!("{BREAK_BOARD_OPEN_CUSTOM_ID_PREFIX}:{guild_id}:{role_id}")
}

pub fn break_board_modal_custom_id(guild_id: u64, role_id: u64) -> String {
    format!("{BREAK_BOARD_MODAL_CUSTOM_ID_PREFIX}:{guild_id}:{role_id}")
}

pub fn break_board_claim_custom_id(request_message_id: u64) -> String {
    format!("{BREAK_BOARD_REQUEST_CLAIM_CUSTOM_ID_PREFIX}:{request_message_id}")
}

pub fn break_board_delete_custom_id(request_message_id: u64) -> String {
    format!("{BREAK_BOARD_REQUEST_DELETE_CUSTOM_ID_PREFIX}:{request_message_id}")
}

pub fn break_board_complete_custom_id(request_message_id: u64) -> String {
    format!("{BREAK_BOARD_REQUEST_COMPLETE_CUSTOM_ID_PREFIX}:{request_message_id}")
}

pub fn break_board_label(role_name: &str) -> String {
    role_label_from_prefix(role_name, BREAK_BOARD_ROLE_PREFIX)
}

async fn message_exists(http: &Arc<Http>, channel_id: u64, message_id: u64) -> AppResult<bool> {
    let result = ChannelId::new(channel_id)
        .message(http, MessageId::new(message_id))
        .await;

    match result {
        Ok(_) => Ok(true),
        Err(SerenityError::Http(error)) if error.status_code() == Some(StatusCode::NOT_FOUND) => {
            Ok(false)
        }
        Err(error) => Err(error.into()),
    }
}

fn break_board_role_rows(
    roles: &[ResolvedRoleTarget],
    custom_id: impl Fn(&ResolvedRoleTarget) -> String,
) -> Vec<CreateActionRow> {
    roles
        .chunks(5)
        .map(|chunk| {
            CreateActionRow::Buttons(
                chunk
                    .iter()
                    .map(|role| {
                        CreateButton::new(custom_id(role))
                            .label(break_board_label(&role.name))
                            .style(ButtonStyle::Primary)
                    })
                    .collect(),
            )
        })
        .collect()
}

fn humanize_minutes(minutes: i64) -> String {
    if minutes < 60 {
        return format!("{minutes}m");
    }

    let hours = minutes / 60;
    let remaining = minutes % 60;
    if remaining == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h {remaining}m")
    }
}

fn role_label_from_prefix(role_name: &str, prefix: &str) -> String {
    let trimmed = role_name
        .strip_prefix(prefix)
        .unwrap_or(role_name)
        .trim_matches('_');

    if trimmed.is_empty() {
        return role_name.to_string();
    }

    match trimmed.to_ascii_lowercase().as_str() {
        "c1" => return "Center".to_string(),
        "s3" => return "Approach".to_string(),
        "s2" => return "Tower".to_string(),
        "s1" => return "Ground".to_string(),
        _ => {}
    }

    trimmed
        .split('_')
        .filter(|segment| !segment.is_empty())
        .map(format_role_label_segment)
        .collect::<Vec<_>>()
        .join(" ")
}

/// ATC position abbreviations that read as acronyms, so a role-name segment like
/// `twr` renders "TWR" instead of "Twr" (e.g. break board `unrestricted_twr`).
const UPPERCASE_LABEL_ABBREVIATIONS: &[&str] =
    &["gnd", "twr", "app", "dep", "ctr", "del", "pct", "apr", "fss", "tmu"];

fn format_role_label_segment(segment: &str) -> String {
    let lower = segment.to_ascii_lowercase();
    if UPPERCASE_LABEL_ABBREVIATIONS.contains(&lower.as_str()) {
        return lower.to_ascii_uppercase();
    }

    if segment
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
    {
        let uppercase = segment.to_ascii_uppercase();
        if uppercase.chars().any(|ch| ch.is_ascii_digit()) {
            return uppercase;
        }
    }

    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };

    let mut formatted = String::new();
    formatted.extend(first.to_uppercase());
    formatted.push_str(&chars.as_str().to_ascii_lowercase());
    formatted
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::TimeZone;
    use serenity::http::Http;

    use crate::models::{
        AnnouncementPayload, Event, EventPosition, EventPositionPostingPayload, ResolvedRoleTarget,
        StaffupOfflineEmbed, StaffupOnlineEmbed,
    };

    use super::{
        SerenityDiscordService, break_board_label, impromptu_selector_custom_id,
        impromptu_selector_label, parse_impromptu_selector_custom_id,
    };

    fn service() -> SerenityDiscordService {
        SerenityDiscordService::new(
            Arc::new(Http::new("token")),
            "https://example.com/logo.png".into(),
        )
    }

    #[test]
    fn announcement_embed_includes_standard_footer() {
        let embed = service().preview_announcement_embed(&AnnouncementPayload {
            title: "Notice".into(),
            body_markdown: "Body".into(),
            details_url: None,
            requested_by_cid: 1234567,
            channel: None,
        });
        let json = serde_json::to_value(embed).unwrap();
        assert_eq!(json["footer"]["text"], "vZDC");
        assert_eq!(json["footer"]["icon_url"], "https://example.com/logo.png");
        assert!(json.get("timestamp").is_some());
    }

    #[test]
    fn event_embed_includes_standard_footer() {
        let embed = service().preview_event_embed(
            &EventPositionPostingPayload {
                event_id: "event-1".into(),
                ping_users: true,
                requested_by_cid: 1234567,
            },
            &Event {
                id: "event-1".into(),
                title: "Open House".into(),
                event_type: None,
                host: None,
                description: Some("Desc".into()),
                status: "published".into(),
                published: true,
                banner_asset_id: None,
                starts_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 20, 0, 0).unwrap(),
                ends_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 22, 0, 0).unwrap(),
                created_by: "user".into(),
                created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 18, 0, 0).unwrap(),
                updated_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 18, 5, 0).unwrap(),
            },
            &[],
        );
        let json = serde_json::to_value(embed).unwrap();
        assert_eq!(json["footer"]["text"], "vZDC");
        assert_eq!(json["footer"]["icon_url"], "https://example.com/logo.png");
        assert!(json.get("timestamp").is_some());
    }

    fn position(
        callsign: &str,
        category: Option<&str>,
        name: Option<&str>,
        rating: Option<&str>,
        discord: Option<&str>,
    ) -> EventPosition {
        EventPosition {
            id: callsign.into(),
            event_id: "e".into(),
            callsign: callsign.into(),
            user_id: name.map(|_| "u".into()),
            user_cid: None,
            user_name: name.map(Into::into),
            user_rating: rating.map(Into::into),
            user_discord_id: discord.map(Into::into),
            controlling_category: category.map(Into::into),
            requested_slot: None,
            assigned_slot: None,
            final_position: None,
            published: true,
            status: "published".into(),
            created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 18, 0, 0).unwrap(),
            updated_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 18, 0, 0).unwrap(),
        }
    }

    #[test]
    fn groups_positions_by_category_in_order_with_mentions_and_ratings() {
        let positions = vec![
            position("DC_12_CTR", Some("CTR"), Some("Matthew"), Some("C1"), None),
            position("HEF_GND", Some("GND"), None, Some("S3"), Some("111")),
            position("DCA_TWR", None, Some("Aaron"), Some("C3"), Some("222")),
            position("FLTRK", Some("APP"), None, None, None),
        ];

        let groups = super::group_positions_by_category(&positions);
        let labels: Vec<&str> = groups.iter().map(|(label, _)| label.as_str()).collect();
        // GND < TWR (from callsign suffix) < APP < CTR
        assert_eq!(labels, vec!["GND", "TWR", "APP", "CTR"]);

        // GND: linked discord id -> mention, with rating.
        assert_eq!(groups[0].1[0], "<@111> (S3) — HEF_GND");
        // TWR: category inferred from `DCA_TWR` suffix; name fallback + rating.
        assert_eq!(groups[1].0, "TWR");
        assert_eq!(groups[1].1[0], "<@222> (C3) — DCA_TWR");
        // APP: unassigned position renders as open.
        assert_eq!(groups[2].1[0], "*Open* — FLTRK");
    }

    #[test]
    fn event_ping_mentions_are_unique_linked_ids() {
        let positions = vec![
            position("A_GND", None, Some("X"), None, Some("100")),
            position("B_TWR", None, Some("Y"), None, Some("100")),
            position("C_APP", None, Some("Z"), None, Some("200")),
            position("D_CTR", None, None, None, None),
        ];
        assert_eq!(super::event_ping_mentions(&positions), vec![100, 200]);
    }

    #[test]
    fn staffup_online_embed_includes_standard_footer() {
        let embed = service().staffup_online_embed(&StaffupOnlineEmbed {
            callsign: "RDU_E1_GND".into(),
            name: "Aiden Huang (Student1)".into(),
            frequency: "121.900".into(),
            logon_unix: 1_778_438_340,
        });
        let json = serde_json::to_value(embed).unwrap();
        assert_eq!(json["footer"]["text"], "vZDC");
        assert_eq!(json["footer"]["icon_url"], "https://example.com/logo.png");
        assert!(json.get("timestamp").is_some());
    }

    #[test]
    fn staffup_offline_embed_includes_standard_footer() {
        let embed = service().staffup_offline_embed(&StaffupOfflineEmbed {
            callsign: "RDU_E1_GND".into(),
            name: "Aiden Huang (Student1)".into(),
            frequency: "121.900".into(),
            logon_unix: Some(1_778_438_340),
            logoff_unix: 1_778_439_600,
            duration: Some("21m".into()),
        });
        let json = serde_json::to_value(embed).unwrap();
        assert_eq!(json["footer"]["text"], "vZDC");
        assert_eq!(json["footer"]["icon_url"], "https://example.com/logo.png");
        assert!(json.get("timestamp").is_some());
    }

    #[test]
    fn impromptu_selector_embed_includes_standard_footer() {
        let embed = service().impromptu_selector_embed();
        let json = serde_json::to_value(embed).unwrap();
        assert_eq!(json["title"], "Impromptu Selector");
        assert_eq!(json["footer"]["text"], "vZDC");
        assert_eq!(json["footer"]["icon_url"], "https://example.com/logo.png");
        assert!(json.get("timestamp").is_some());
    }

    #[test]
    fn impromptu_selector_components_pack_buttons() {
        let rows = service().impromptu_selector_components(&[
            ResolvedRoleTarget {
                config_name: "main".into(),
                guild_id: 1,
                role_id: 10,
                name: "impromptu_s1".into(),
            },
            ResolvedRoleTarget {
                config_name: "main".into(),
                guild_id: 1,
                role_id: 12,
                name: "impromptu_c1".into(),
            },
            ResolvedRoleTarget {
                config_name: "main".into(),
                guild_id: 1,
                role_id: 11,
                name: "impromptu_ground_training".into(),
            },
        ]);

        let json = serde_json::to_value(rows).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 1);
        let buttons = json[0]["components"].as_array().unwrap();
        assert_eq!(buttons.len(), 3);
        assert_eq!(buttons[0]["label"], "Ground");
        assert_eq!(buttons[1]["label"], "Center");
        assert_eq!(buttons[2]["label"], "Ground Training");
    }

    #[test]
    fn impromptu_selector_custom_id_round_trips() {
        let custom_id = impromptu_selector_custom_id(1, 2);
        assert_eq!(parse_impromptu_selector_custom_id(&custom_id), Some((1, 2)));
        assert_eq!(parse_impromptu_selector_custom_id("other:1:2"), None);
        assert_eq!(parse_impromptu_selector_custom_id("impromptu-role:1"), None);
    }

    #[test]
    fn impromptu_selector_labels_are_derived_from_role_names() {
        assert_eq!(impromptu_selector_label("impromptu_c1"), "Center");
        assert_eq!(impromptu_selector_label("impromptu_s3"), "Approach");
        assert_eq!(impromptu_selector_label("impromptu_s2"), "Tower");
        assert_eq!(impromptu_selector_label("impromptu_s1"), "Ground");
        assert_eq!(
            impromptu_selector_label("impromptu_ground_training"),
            "Ground Training"
        );
        assert_eq!(impromptu_selector_label("impromptu_"), "impromptu_");
    }

    #[test]
    fn break_board_labels_render_position_tiers() {
        assert_eq!(break_board_label("break_board_tier_1_gnd"), "Tier 1 GND");
        assert_eq!(break_board_label("break_board_tier_1_twr"), "Tier 1 TWR");
        assert_eq!(
            break_board_label("break_board_unrestricted_gnd"),
            "Unrestricted GND"
        );
        assert_eq!(
            break_board_label("break_board_unrestricted_twr"),
            "Unrestricted TWR"
        );
        assert_eq!(
            break_board_label("break_board_unrestricted_app"),
            "Unrestricted APP"
        );
        assert_eq!(break_board_label("break_board_center"), "Center");
        assert_eq!(break_board_label("break_board_pct"), "PCT");
    }
}
