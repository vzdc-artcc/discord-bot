use chrono::Utc;
use serenity::{
    all::{
        AuditLogEntry, ChannelId, Context, GuildChannel, GuildId, GuildMemberUpdateEvent, Member,
        Message, MessageId, MessageUpdateEvent, PartialGuildChannel, Role, RoleId, Sticker,
        StickerId, User,
    },
    model::guild::audit_log::{
        Action, ChannelAction, EmojiAction, MemberAction, RoleAction, StickerAction, ThreadAction,
    },
    prelude::Mentionable,
};
use std::collections::{HashMap, HashSet};

use crate::{
    errors::AppResult,
    models::{AuditEvent, AuditKind},
    state::{AppState, MessageSnapshot},
};

const CREATE_COLOR: u32 = 0x2ECC71;
const UPDATE_COLOR: u32 = 0xF39C12;
const DELETE_COLOR: u32 = 0xE74C3C;
const MODERATION_COLOR: u32 = 0x3498DB;
const AUDIT_LOOKUP_LIMIT: u8 = 6;
const AUDIT_LOOKUP_WINDOW_SECS: i64 = 15;

pub async fn track_message(state: &AppState, message: &Message) {
    if should_skip_user(state, Some(&message.author)) {
        return;
    }

    state
        .runtime
        .store_message_snapshot(MessageSnapshot {
            channel_id: message.channel_id.get(),
            message_id: message.id.get(),
            author_id: message.author.id.get(),
            author_label: user_label(&message.author),
            author_is_bot: message.author.bot,
            content: message.content.clone(),
        })
        .await;
}

pub async fn handle_message_create(state: &AppState, message: Message) {
    track_message(state, &message).await;
}

pub async fn handle_message_update(
    ctx: &Context,
    state: &AppState,
    old_if_available: Option<Message>,
    new: Option<Message>,
    event: MessageUpdateEvent,
) {
    let Some(guild_id) = event.guild_id else {
        return;
    };
    let snapshot = state
        .runtime
        .recent_message_snapshot(event.channel_id.get(), event.id.get())
        .await;

    let author = new
        .as_ref()
        .map(|message| message.author.clone())
        .or_else(|| {
            old_if_available
                .as_ref()
                .map(|message| message.author.clone())
        })
        .or_else(|| event.author.clone());
    if should_skip_user(state, author.as_ref()) {
        return;
    }

    let before = old_if_available
        .as_ref()
        .map(|message| message.content.clone())
        .or_else(|| snapshot.as_ref().map(|item| item.content.clone()))
        .unwrap_or_else(|| "Not available".to_string());
    let after = new
        .as_ref()
        .map(|message| message.content.clone())
        .or_else(|| event.content.clone())
        .unwrap_or_else(|| "Not available".to_string());

    if before == after && old_if_available.is_some() {
        return;
    }

    let subject_label = author_label(author.as_ref(), "Unknown author");
    let jump_url = format!(
        "https://discord.com/channels/{}/{}/{}",
        guild_id.get(),
        event.channel_id.get(),
        event.id.get()
    );
    let audit = AuditEvent {
        kind: AuditKind::MessageUpdated,
        guild_id: guild_id.get(),
        channel_id: Some(event.channel_id.get()),
        target_id: Some(event.id.get()),
        actor_user_id: author.as_ref().map(|user| user.id.get()),
        actor_label: author.as_ref().map(user_label),
        subject_label,
        summary: "A message was edited.".to_string(),
        details: vec![
            (
                "Author".to_string(),
                author_label(author.as_ref(), "Unknown author"),
            ),
            ("Message".to_string(), truncate(state, &jump_url)),
            ("Before".to_string(), truncate(state, &before)),
            ("After".to_string(), truncate(state, &after)),
        ],
        color: UPDATE_COLOR,
        occurred_at: Utc::now(),
        dedupe_key: Some(format!("audit:message_update:{}", event.id.get())),
    };

    if let Some(new_message) = new.as_ref() {
        track_message(state, new_message).await;
    }

    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_message_delete(
    ctx: &Context,
    state: &AppState,
    guild_id: Option<GuildId>,
    channel_id: ChannelId,
    message_id: MessageId,
) {
    let Some(guild_id) = guild_id else {
        return;
    };

    let cached = ctx
        .cache
        .channel_messages(channel_id)
        .and_then(|messages| messages.get(&message_id).cloned());
    let snapshot = state
        .runtime
        .remove_message_snapshot(channel_id.get(), message_id.get())
        .await;

    if (!state.config.audit_include_bot_events)
        && (cached.as_ref().is_some_and(|message| message.author.bot)
            || snapshot.as_ref().is_some_and(|item| item.author_is_bot))
    {
        return;
    }

    let subject = cached
        .as_ref()
        .map(|message| author_label(Some(&message.author), "Unknown author"))
        .or_else(|| snapshot.as_ref().map(|item| item.author_label.clone()))
        .unwrap_or_else(|| format!("Message {}", message_id.get()));
    let content = cached
        .as_ref()
        .map(|message| truncate(state, &message.content))
        .or_else(|| snapshot.as_ref().map(|item| truncate(state, &item.content)))
        .unwrap_or_else(|| "Not available".to_string());
    let author = cached
        .as_ref()
        .map(|message| user_label(&message.author))
        .or_else(|| snapshot.as_ref().map(|item| item.author_label.clone()));
    let actor_user_id = cached
        .as_ref()
        .map(|message| message.author.id.get())
        .or_else(|| snapshot.as_ref().map(|item| item.author_id));

    let audit = AuditEvent {
        kind: AuditKind::MessageDeleted,
        guild_id: guild_id.get(),
        channel_id: Some(channel_id.get()),
        target_id: Some(message_id.get()),
        actor_user_id,
        actor_label: author.clone(),
        subject_label: subject,
        summary: "A message was deleted.".to_string(),
        details: vec![
            (
                "Author".to_string(),
                author.unwrap_or_else(|| "Unknown".to_string()),
            ),
            ("Message ID".to_string(), message_id.get().to_string()),
            ("Content".to_string(), content),
        ],
        color: DELETE_COLOR,
        occurred_at: Utc::now(),
        dedupe_key: Some(format!("audit:message_delete:{}", message_id.get())),
    };

    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_message_delete_bulk(
    ctx: &Context,
    state: &AppState,
    guild_id: Option<GuildId>,
    channel_id: ChannelId,
    message_ids: Vec<MessageId>,
) {
    let Some(guild_id) = guild_id else {
        return;
    };

    let audit = AuditEvent {
        kind: AuditKind::BulkMessageDeleted,
        guild_id: guild_id.get(),
        channel_id: Some(channel_id.get()),
        target_id: None,
        actor_user_id: None,
        actor_label: None,
        subject_label: format!("{} messages", message_ids.len()),
        summary: "Multiple messages were deleted.".to_string(),
        details: vec![
            ("Count".to_string(), message_ids.len().to_string()),
            (
                "Message IDs".to_string(),
                truncate(
                    state,
                    &message_ids
                        .iter()
                        .map(|id| id.get().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            ),
        ],
        color: DELETE_COLOR,
        occurred_at: Utc::now(),
        dedupe_key: Some(format!(
            "audit:message_delete_bulk:{}:{}",
            channel_id.get(),
            message_ids.len()
        )),
    };

    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_channel_create(ctx: &Context, state: &AppState, channel: GuildChannel) {
    let action = Action::Channel(ChannelAction::Create);
    let audit = build_channel_event(
        state,
        AuditKind::ChannelCreated,
        &channel,
        None,
        CREATE_COLOR,
    );
    let audit = enrich_actor(ctx, state, audit, action, Some(channel.id.get())).await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_channel_update(
    ctx: &Context,
    state: &AppState,
    old: Option<GuildChannel>,
    new: GuildChannel,
) {
    let details = diff_channel(state, old.as_ref(), &new);
    if details.is_empty() {
        return;
    }

    let mut audit = build_channel_event(
        state,
        AuditKind::ChannelUpdated,
        &new,
        Some(details),
        UPDATE_COLOR,
    );
    audit.summary = "A channel was updated.".to_string();
    let audit = enrich_actor(
        ctx,
        state,
        audit,
        Action::Channel(ChannelAction::Update),
        Some(new.id.get()),
    )
    .await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_channel_delete(ctx: &Context, state: &AppState, channel: GuildChannel) {
    let mut audit = build_channel_event(
        state,
        AuditKind::ChannelDeleted,
        &channel,
        None,
        DELETE_COLOR,
    );
    audit.summary = "A channel was deleted.".to_string();
    let audit = enrich_actor(
        ctx,
        state,
        audit,
        Action::Channel(ChannelAction::Delete),
        Some(channel.id.get()),
    )
    .await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_role_create(ctx: &Context, state: &AppState, role: Role) {
    let audit = enrich_actor(
        ctx,
        state,
        build_role_event(state, AuditKind::RoleCreated, &role, None, CREATE_COLOR),
        Action::Role(RoleAction::Create),
        Some(role.id.get()),
    )
    .await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_role_update(ctx: &Context, state: &AppState, old: Option<Role>, new: Role) {
    let details = diff_role(state, old.as_ref(), &new);
    if details.is_empty() {
        return;
    }

    let audit = enrich_actor(
        ctx,
        state,
        build_role_event(
            state,
            AuditKind::RoleUpdated,
            &new,
            Some(details),
            UPDATE_COLOR,
        ),
        Action::Role(RoleAction::Update),
        Some(new.id.get()),
    )
    .await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_role_delete(
    ctx: &Context,
    state: &AppState,
    guild_id: GuildId,
    role_id: RoleId,
    role: Option<Role>,
) {
    let subject_label = role
        .as_ref()
        .map(|item| format!("@{}", item.name))
        .unwrap_or_else(|| format!("Role {}", role_id.get()));
    let mut details = vec![("Role ID".to_string(), role_id.get().to_string())];
    if let Some(role) = role.as_ref() {
        details.push((
            "Permissions".to_string(),
            permissions_label(role.permissions),
        ));
    }

    let audit = enrich_actor(
        ctx,
        state,
        AuditEvent {
            kind: AuditKind::RoleDeleted,
            guild_id: guild_id.get(),
            channel_id: None,
            target_id: Some(role_id.get()),
            actor_user_id: None,
            actor_label: None,
            subject_label,
            summary: "A role was deleted.".to_string(),
            details,
            color: DELETE_COLOR,
            occurred_at: Utc::now(),
            dedupe_key: Some(format!("audit:role_delete:{}", role_id.get())),
        },
        Action::Role(RoleAction::Delete),
        Some(role_id.get()),
    )
    .await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_thread_create(ctx: &Context, state: &AppState, thread: GuildChannel) {
    let mut audit =
        build_channel_event(state, AuditKind::ThreadCreated, &thread, None, CREATE_COLOR);
    audit.summary = "A thread was created.".to_string();
    let audit = enrich_actor(
        ctx,
        state,
        audit,
        Action::Thread(ThreadAction::Create),
        Some(thread.id.get()),
    )
    .await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_thread_update(
    ctx: &Context,
    state: &AppState,
    old: Option<GuildChannel>,
    new: GuildChannel,
) {
    let details = diff_thread(state, old.as_ref(), &new);
    if details.is_empty() {
        return;
    }

    let mut audit = build_channel_event(
        state,
        AuditKind::ThreadUpdated,
        &new,
        Some(details),
        UPDATE_COLOR,
    );
    audit.summary = "A thread was updated.".to_string();
    let audit = enrich_actor(
        ctx,
        state,
        audit,
        Action::Thread(ThreadAction::Update),
        Some(new.id.get()),
    )
    .await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_thread_delete(
    ctx: &Context,
    state: &AppState,
    thread: PartialGuildChannel,
    full_thread_data: Option<GuildChannel>,
) {
    let subject_label = full_thread_data
        .as_ref()
        .map(|item| format!("#{}", item.name))
        .unwrap_or_else(|| format!("Thread {}", thread.id.get()));
    let mut details = vec![
        ("Thread ID".to_string(), thread.id.get().to_string()),
        ("Parent ID".to_string(), thread.parent_id.get().to_string()),
    ];
    if let Some(full) = full_thread_data.as_ref()
        && let Some(metadata) = full.thread_metadata.as_ref()
    {
        details.push(("Archived".to_string(), metadata.archived.to_string()));
        details.push(("Locked".to_string(), metadata.locked.to_string()));
    }

    let audit = enrich_actor(
        ctx,
        state,
        AuditEvent {
            kind: AuditKind::ThreadDeleted,
            guild_id: thread.guild_id.get(),
            channel_id: Some(thread.parent_id.get()),
            target_id: Some(thread.id.get()),
            actor_user_id: None,
            actor_label: None,
            subject_label,
            summary: "A thread was deleted.".to_string(),
            details,
            color: DELETE_COLOR,
            occurred_at: Utc::now(),
            dedupe_key: Some(format!("audit:thread_delete:{}", thread.id.get())),
        },
        Action::Thread(ThreadAction::Delete),
        Some(thread.id.get()),
    )
    .await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_member_addition(ctx: &Context, state: &AppState, member: Member) {
    if should_skip_user(state, Some(&member.user)) {
        return;
    }

    let audit = AuditEvent {
        kind: AuditKind::MemberJoined,
        guild_id: member.guild_id.get(),
        channel_id: None,
        target_id: Some(member.user.id.get()),
        actor_user_id: Some(member.user.id.get()),
        actor_label: Some(user_label(&member.user)),
        subject_label: user_label(&member.user),
        summary: "A member joined the guild.".to_string(),
        details: vec![
            ("User".to_string(), member.user.mention().to_string()),
            (
                "Account Created".to_string(),
                format!("<t:{}:F>", member.user.created_at().unix_timestamp()),
            ),
        ],
        color: CREATE_COLOR,
        occurred_at: Utc::now(),
        dedupe_key: Some(format!("audit:member_join:{}", member.user.id.get())),
    };
    let _ = deliver_audit_event(ctx, state, audit).await;

    if state.config.role_sync_on_join {
        match crate::services::sync_roles_for_discord_id(state, member.user.id.get(), "member_join")
            .await
        {
            Ok(outcome) => {
                if outcome.linked && (outcome.roles_added > 0 || outcome.roles_removed > 0) {
                    tracing::info!(
                        cid = outcome.cid,
                        discord_id = outcome.discord_id,
                        roles_added = outcome.roles_added,
                        roles_removed = outcome.roles_removed,
                        "applied role sync during member join"
                    );
                }
            }
            Err(error) => {
                tracing::error!(?error, "failed role sync during member join");
            }
        }
    }
}

pub async fn handle_member_removal(
    ctx: &Context,
    state: &AppState,
    guild_id: GuildId,
    user: User,
    member_data: Option<Member>,
) {
    if should_skip_user(state, Some(&user)) {
        return;
    }

    let mut details = vec![("User".to_string(), user_label(&user))];
    if let Some(member) = member_data
        && !member.roles.is_empty()
    {
        details.push((
            "Roles".to_string(),
            member
                .roles
                .iter()
                .map(|role_id| format!("<@&{}>", role_id.get()))
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }

    let audit = AuditEvent {
        kind: AuditKind::MemberLeft,
        guild_id: guild_id.get(),
        channel_id: None,
        target_id: Some(user.id.get()),
        actor_user_id: Some(user.id.get()),
        actor_label: Some(user_label(&user)),
        subject_label: user_label(&user),
        summary: "A member left the guild.".to_string(),
        details,
        color: DELETE_COLOR,
        occurred_at: Utc::now(),
        dedupe_key: Some(format!("audit:member_left:{}", user.id.get())),
    };
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_member_update(
    ctx: &Context,
    state: &AppState,
    old: Option<Member>,
    new: Option<Member>,
    event: GuildMemberUpdateEvent,
) {
    if should_skip_user(state, Some(&event.user)) {
        return;
    }

    let mut details = Vec::new();
    if let Some(old) = old.as_ref() {
        if old.nick != event.nick {
            details.push((
                "Nickname".to_string(),
                format!(
                    "{} -> {}",
                    old.nick.clone().unwrap_or_else(|| "None".to_string()),
                    event.nick.clone().unwrap_or_else(|| "None".to_string())
                ),
            ));
        }
        if old.pending != event.pending {
            details.push((
                "Pending".to_string(),
                format!("{} -> {}", old.pending, event.pending),
            ));
        }
        if old.communication_disabled_until != event.communication_disabled_until {
            details.push((
                "Timeout".to_string(),
                format!(
                    "{} -> {}",
                    timestamp_label(old.communication_disabled_until),
                    timestamp_label(event.communication_disabled_until)
                ),
            ));
        }
        let role_delta = role_delta(&old.roles, &event.roles);
        if !role_delta.is_empty() {
            details.push(("Role Changes".to_string(), role_delta));
        }
    } else {
        if let Some(member) = new.as_ref() {
            details.push((
                "Nickname".to_string(),
                member.nick.clone().unwrap_or_else(|| "None".to_string()),
            ));
            if !member.roles.is_empty() {
                details.push((
                    "Roles".to_string(),
                    member
                        .roles
                        .iter()
                        .map(|role_id| format!("<@&{}>", role_id.get()))
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
            }
        }
    }

    if details.is_empty() {
        return;
    }

    let audit = enrich_actor(
        ctx,
        state,
        AuditEvent {
            kind: AuditKind::MemberUpdated,
            guild_id: event.guild_id.get(),
            channel_id: None,
            target_id: Some(event.user.id.get()),
            actor_user_id: None,
            actor_label: None,
            subject_label: user_label(&event.user),
            summary: "A member profile changed.".to_string(),
            details,
            color: MODERATION_COLOR,
            occurred_at: Utc::now(),
            dedupe_key: Some(format!("audit:member_update:{}", event.user.id.get())),
        },
        Action::Member(MemberAction::Update),
        Some(event.user.id.get()),
    )
    .await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_ban_addition(ctx: &Context, state: &AppState, guild_id: GuildId, user: User) {
    if should_skip_user(state, Some(&user)) {
        return;
    }

    let audit = enrich_actor(
        ctx,
        state,
        AuditEvent {
            kind: AuditKind::BanAdded,
            guild_id: guild_id.get(),
            channel_id: None,
            target_id: Some(user.id.get()),
            actor_user_id: None,
            actor_label: None,
            subject_label: user_label(&user),
            summary: "A member was banned.".to_string(),
            details: vec![("User".to_string(), user_label(&user))],
            color: DELETE_COLOR,
            occurred_at: Utc::now(),
            dedupe_key: Some(format!("audit:ban_add:{}", user.id.get())),
        },
        Action::Member(MemberAction::BanAdd),
        Some(user.id.get()),
    )
    .await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_ban_removal(ctx: &Context, state: &AppState, guild_id: GuildId, user: User) {
    if should_skip_user(state, Some(&user)) {
        return;
    }

    let audit = enrich_actor(
        ctx,
        state,
        AuditEvent {
            kind: AuditKind::BanRemoved,
            guild_id: guild_id.get(),
            channel_id: None,
            target_id: Some(user.id.get()),
            actor_user_id: None,
            actor_label: None,
            subject_label: user_label(&user),
            summary: "A member was unbanned.".to_string(),
            details: vec![("User".to_string(), user_label(&user))],
            color: MODERATION_COLOR,
            occurred_at: Utc::now(),
            dedupe_key: Some(format!("audit:ban_remove:{}", user.id.get())),
        },
        Action::Member(MemberAction::BanRemove),
        Some(user.id.get()),
    )
    .await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_emojis_update(
    ctx: &Context,
    state: &AppState,
    guild_id: GuildId,
    current_state: HashMap<serenity::all::EmojiId, serenity::all::Emoji>,
) {
    let details = vec![
        ("Count".to_string(), current_state.len().to_string()),
        (
            "Names".to_string(),
            truncate(
                state,
                &current_state
                    .values()
                    .map(|emoji| emoji.name.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        ),
    ];
    let audit = enrich_actor(
        ctx,
        state,
        AuditEvent {
            kind: AuditKind::EmojiUpdated,
            guild_id: guild_id.get(),
            channel_id: None,
            target_id: None,
            actor_user_id: None,
            actor_label: None,
            subject_label: "Guild emojis".to_string(),
            summary: "The custom emoji set changed.".to_string(),
            details,
            color: UPDATE_COLOR,
            occurred_at: Utc::now(),
            dedupe_key: Some(format!("audit:emoji_update:{}", guild_id.get())),
        },
        Action::Emoji(EmojiAction::Update),
        None,
    )
    .await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn handle_stickers_update(
    ctx: &Context,
    state: &AppState,
    guild_id: GuildId,
    current_state: HashMap<StickerId, Sticker>,
) {
    let details = vec![
        ("Count".to_string(), current_state.len().to_string()),
        (
            "Names".to_string(),
            truncate(
                state,
                &current_state
                    .values()
                    .map(|sticker| sticker.name.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        ),
    ];
    let audit = enrich_actor(
        ctx,
        state,
        AuditEvent {
            kind: AuditKind::StickerUpdated,
            guild_id: guild_id.get(),
            channel_id: None,
            target_id: None,
            actor_user_id: None,
            actor_label: None,
            subject_label: "Guild stickers".to_string(),
            summary: "The sticker set changed.".to_string(),
            details,
            color: UPDATE_COLOR,
            occurred_at: Utc::now(),
            dedupe_key: Some(format!("audit:sticker_update:{}", guild_id.get())),
        },
        Action::Sticker(StickerAction::Update),
        None,
    )
    .await;
    let _ = deliver_audit_event(ctx, state, audit).await;
}

pub async fn publish_audit_event(state: &AppState, event: AuditEvent) -> AppResult<()> {
    if !state.config.audit_logging_enabled || !state.runtime.feature_enabled("audit_log").await {
        return Ok(());
    }

    {
        let mut readiness = state.runtime.readiness.write().await;
        readiness.audit.last_event_at = Some(event.occurred_at);
    }

    if let Some(dedupe_key) = event.dedupe_key.clone()
        && state.runtime.mark_delivery_seen(dedupe_key).await
    {
        return Ok(());
    }

    let bundle = match state.runtime.config_bundle.read().await.clone() {
        Some(bundle) => bundle,
        None => return Ok(()),
    };

    match state.delivery.send_audit_event(&bundle, &event).await {
        Ok(_) => {
            let mut readiness = state.runtime.readiness.write().await;
            readiness.audit.last_delivery_at = Some(Utc::now());
            readiness.audit.last_error = None;
            Ok(())
        }
        Err(error) => {
            tracing::error!(?error, kind = ?event.kind, "failed to deliver audit event");
            let mut readiness = state.runtime.readiness.write().await;
            readiness.audit.last_error = Some(error.to_string());
            Ok(())
        }
    }
}

async fn deliver_audit_event(_ctx: &Context, state: &AppState, event: AuditEvent) -> AppResult<()> {
    publish_audit_event(state, event).await
}

fn build_channel_event(
    state: &AppState,
    kind: AuditKind,
    channel: &GuildChannel,
    details: Option<Vec<(String, String)>>,
    color: u32,
) -> AuditEvent {
    let mut detail_rows = vec![
        ("Name".to_string(), truncate(state, &channel.name)),
        ("Type".to_string(), format!("{:?}", channel.kind)),
        ("Channel ID".to_string(), channel.id.get().to_string()),
    ];
    if let Some(parent_id) = channel.parent_id {
        detail_rows.push(("Parent".to_string(), format!("<#{}>", parent_id.get())));
    }
    if let Some(topic) = channel.topic.as_deref() {
        detail_rows.push(("Topic".to_string(), truncate(state, topic)));
    }
    if let Some(rate_limit) = channel.rate_limit_per_user {
        detail_rows.push(("Slowmode".to_string(), format!("{rate_limit}s")));
    }
    if let Some(extra) = details {
        detail_rows.extend(extra);
    }

    AuditEvent {
        kind,
        guild_id: channel.guild_id.get(),
        channel_id: Some(channel.id.get()),
        target_id: Some(channel.id.get()),
        actor_user_id: None,
        actor_label: None,
        subject_label: format!("#{}", channel.name),
        summary: "A guild channel changed.".to_string(),
        details: detail_rows,
        color,
        occurred_at: Utc::now(),
        dedupe_key: Some(format!("audit:{kind:?}:{}", channel.id.get())),
    }
}

fn build_role_event(
    _state: &AppState,
    kind: AuditKind,
    role: &Role,
    details: Option<Vec<(String, String)>>,
    color: u32,
) -> AuditEvent {
    let mut detail_rows = vec![
        ("Role".to_string(), format!("@{}", role.name)),
        ("Role ID".to_string(), role.id.get().to_string()),
        ("Color".to_string(), format!("#{:06X}", role.colour.0)),
        ("Hoist".to_string(), role.hoist.to_string()),
        ("Mentionable".to_string(), role.mentionable.to_string()),
        (
            "Permissions".to_string(),
            permissions_label(role.permissions),
        ),
    ];
    if let Some(extra) = details {
        detail_rows.extend(extra);
    }

    AuditEvent {
        kind,
        guild_id: role.guild_id.get(),
        channel_id: None,
        target_id: Some(role.id.get()),
        actor_user_id: None,
        actor_label: None,
        subject_label: format!("@{}", role.name),
        summary: "A role changed.".to_string(),
        details: detail_rows,
        color,
        occurred_at: Utc::now(),
        dedupe_key: Some(format!("audit:{kind:?}:{}", role.id.get())),
    }
}

async fn enrich_actor(
    ctx: &Context,
    state: &AppState,
    mut audit: AuditEvent,
    action: Action,
    target_id: Option<u64>,
) -> AuditEvent {
    if !state.config.audit_fetch_audit_logs {
        return audit;
    }

    match fetch_audit_actor(
        ctx,
        audit.guild_id,
        action,
        target_id,
        audit.occurred_at.timestamp(),
    )
    .await
    {
        Ok(Some((user_id, label))) => {
            audit.actor_user_id = Some(user_id);
            audit.actor_label = Some(label);
        }
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(?error, kind = ?audit.kind, "failed to enrich audit actor");
        }
    }

    audit
}

async fn fetch_audit_actor(
    ctx: &Context,
    guild_id: u64,
    action: Action,
    target_id: Option<u64>,
    occurred_at: i64,
) -> serenity::Result<Option<(u64, String)>> {
    let logs = GuildId::new(guild_id)
        .audit_logs(
            &ctx.http,
            Some(action),
            None,
            None,
            Some(AUDIT_LOOKUP_LIMIT),
        )
        .await?;
    for entry in logs.entries {
        if !matches_target(&entry, target_id) {
            continue;
        }
        let entry_time = entry.id.created_at().unix_timestamp();
        if (entry_time - occurred_at).abs() > AUDIT_LOOKUP_WINDOW_SECS {
            continue;
        }
        let label = logs
            .users
            .get(&entry.user_id)
            .map(user_label)
            .unwrap_or_else(|| format!("<@{}>", entry.user_id.get()));
        return Ok(Some((entry.user_id.get(), label)));
    }
    Ok(None)
}

fn matches_target(entry: &AuditLogEntry, target_id: Option<u64>) -> bool {
    match (entry.target_id, target_id) {
        (_, None) => true,
        (Some(id), Some(target_id)) => id.get() == target_id,
        (None, Some(_)) => false,
    }
}

fn diff_channel(
    state: &AppState,
    old: Option<&GuildChannel>,
    new: &GuildChannel,
) -> Vec<(String, String)> {
    let Some(old) = old else {
        return Vec::new();
    };

    let mut details = Vec::new();
    push_change(&mut details, "Name", &old.name, &new.name);
    push_optional_change(
        &mut details,
        "Topic",
        old.topic.as_deref(),
        new.topic.as_deref(),
    );
    if old.parent_id != new.parent_id {
        details.push((
            "Parent".to_string(),
            format!(
                "{} -> {}",
                old.parent_id
                    .map(|id| format!("<#{}>", id.get()))
                    .unwrap_or_else(|| "None".to_string()),
                new.parent_id
                    .map(|id| format!("<#{}>", id.get()))
                    .unwrap_or_else(|| "None".to_string())
            ),
        ));
    }
    if old.nsfw != new.nsfw {
        details.push(("NSFW".to_string(), format!("{} -> {}", old.nsfw, new.nsfw)));
    }
    if old.rate_limit_per_user != new.rate_limit_per_user {
        details.push((
            "Slowmode".to_string(),
            format!(
                "{} -> {}",
                old.rate_limit_per_user.unwrap_or(0),
                new.rate_limit_per_user.unwrap_or(0)
            ),
        ));
    }
    details
        .into_iter()
        .map(|(name, value)| (name, truncate(state, &value)))
        .collect()
}

fn diff_thread(
    state: &AppState,
    old: Option<&GuildChannel>,
    new: &GuildChannel,
) -> Vec<(String, String)> {
    let Some(old) = old else {
        return Vec::new();
    };

    let mut details = diff_channel(state, Some(old), new);
    let old_meta = old.thread_metadata.as_ref();
    let new_meta = new.thread_metadata.as_ref();
    if old_meta.map(|item| item.archived) != new_meta.map(|item| item.archived)
        || old_meta.map(|item| item.locked) != new_meta.map(|item| item.locked)
    {
        details.push((
            "Archived".to_string(),
            format!(
                "{} -> {}",
                old_meta.map(|item| item.archived).unwrap_or(false),
                new_meta.map(|item| item.archived).unwrap_or(false)
            ),
        ));
        details.push((
            "Locked".to_string(),
            format!(
                "{} -> {}",
                old_meta.map(|item| item.locked).unwrap_or(false),
                new_meta.map(|item| item.locked).unwrap_or(false)
            ),
        ));
    }
    details
}

fn diff_role(state: &AppState, old: Option<&Role>, new: &Role) -> Vec<(String, String)> {
    let Some(old) = old else {
        return Vec::new();
    };

    let mut details = Vec::new();
    push_change(&mut details, "Name", &old.name, &new.name);
    if old.colour != new.colour {
        details.push((
            "Color".to_string(),
            format!("#{:06X} -> #{:06X}", old.colour.0, new.colour.0),
        ));
    }
    if old.hoist != new.hoist {
        details.push((
            "Hoist".to_string(),
            format!("{} -> {}", old.hoist, new.hoist),
        ));
    }
    if old.mentionable != new.mentionable {
        details.push((
            "Mentionable".to_string(),
            format!("{} -> {}", old.mentionable, new.mentionable),
        ));
    }
    if old.permissions != new.permissions {
        details.push((
            "Permissions".to_string(),
            permissions_delta(old.permissions, new.permissions),
        ));
    }
    details
        .into_iter()
        .map(|(name, value)| (name, truncate(state, &value)))
        .collect()
}

fn permissions_delta(old: serenity::all::Permissions, new: serenity::all::Permissions) -> String {
    let added = new - old;
    let removed = old - new;
    let mut parts = Vec::new();
    if !added.is_empty() {
        parts.push(format!("added [{}]", permissions_label(added)));
    }
    if !removed.is_empty() {
        parts.push(format!("removed [{}]", permissions_label(removed)));
    }
    if parts.is_empty() {
        "No change".to_string()
    } else {
        parts.join("; ")
    }
}

fn permissions_label(permissions: serenity::all::Permissions) -> String {
    let names = permissions
        .get_permission_names()
        .into_iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

fn role_delta(old: &[RoleId], new: &[RoleId]) -> String {
    let old_set: HashSet<_> = old.iter().copied().collect();
    let new_set: HashSet<_> = new.iter().copied().collect();
    let added = new_set
        .difference(&old_set)
        .map(|id| format!("<@&{}>", id.get()))
        .collect::<Vec<_>>();
    let removed = old_set
        .difference(&new_set)
        .map(|id| format!("<@&{}>", id.get()))
        .collect::<Vec<_>>();
    let mut parts = Vec::new();
    if !added.is_empty() {
        parts.push(format!("added {}", added.join(", ")));
    }
    if !removed.is_empty() {
        parts.push(format!("removed {}", removed.join(", ")));
    }
    parts.join("; ")
}

fn user_label(user: &User) -> String {
    format!("{} (<@{}>)", user.tag(), user.id.get())
}

fn author_label(user: Option<&User>, fallback: &str) -> String {
    user.map(user_label).unwrap_or_else(|| fallback.to_string())
}

fn timestamp_label(value: Option<serenity::model::Timestamp>) -> String {
    value
        .map(|timestamp| format!("<t:{}:F>", timestamp.unix_timestamp()))
        .unwrap_or_else(|| "None".to_string())
}

fn truncate(state: &AppState, value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let max = state.config.audit_max_field_chars;
    if normalized.chars().count() <= max {
        normalized
    } else {
        normalized
            .chars()
            .take(max.saturating_sub(1))
            .collect::<String>()
            + "…"
    }
}

fn should_skip_user(state: &AppState, user: Option<&User>) -> bool {
    !state.config.audit_include_bot_events && user.is_some_and(|user| user.bot)
}

fn push_change(details: &mut Vec<(String, String)>, label: &str, old: &str, new: &str) {
    if old != new {
        details.push((label.to_string(), format!("{old} -> {new}")));
    }
}

fn push_optional_change(
    details: &mut Vec<(String, String)>,
    label: &str,
    old: Option<&str>,
    new: Option<&str>,
) {
    if old != new {
        details.push((
            label.to_string(),
            format!("{} -> {}", old.unwrap_or("None"), new.unwrap_or("None")),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{permissions_delta, role_delta};
    use serenity::all::{Permissions, RoleId};

    #[test]
    fn permissions_delta_is_readable() {
        let old = Permissions::VIEW_CHANNEL;
        let new = Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES;
        let delta = permissions_delta(old, new);
        assert!(delta.contains("added"));
        assert_ne!(delta, "No change");
    }

    #[test]
    fn role_delta_tracks_add_and_remove() {
        let delta = role_delta(
            &[RoleId::new(1), RoleId::new(2)],
            &[RoleId::new(2), RoleId::new(3)],
        );
        assert!(delta.contains("added"));
        assert!(delta.contains("removed"));
    }
}
