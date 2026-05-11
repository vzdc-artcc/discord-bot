use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditKind {
    MessageUpdated,
    MessageDeleted,
    BulkMessageDeleted,
    ChannelCreated,
    ChannelUpdated,
    ChannelDeleted,
    RoleCreated,
    RoleUpdated,
    RoleDeleted,
    ThreadCreated,
    ThreadUpdated,
    ThreadDeleted,
    MemberJoined,
    MemberLeft,
    MemberUpdated,
    BanAdded,
    BanRemoved,
    EmojiUpdated,
    StickerUpdated,
}

impl AuditKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::MessageUpdated => "Message Edited",
            Self::MessageDeleted => "Message Deleted",
            Self::BulkMessageDeleted => "Bulk Messages Deleted",
            Self::ChannelCreated => "Channel Created",
            Self::ChannelUpdated => "Channel Updated",
            Self::ChannelDeleted => "Channel Deleted",
            Self::RoleCreated => "Role Created",
            Self::RoleUpdated => "Role Updated",
            Self::RoleDeleted => "Role Deleted",
            Self::ThreadCreated => "Thread Created",
            Self::ThreadUpdated => "Thread Updated",
            Self::ThreadDeleted => "Thread Deleted",
            Self::MemberJoined => "Member Joined",
            Self::MemberLeft => "Member Left",
            Self::MemberUpdated => "Member Updated",
            Self::BanAdded => "Member Banned",
            Self::BanRemoved => "Member Unbanned",
            Self::EmojiUpdated => "Emoji Updated",
            Self::StickerUpdated => "Sticker Updated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    pub kind: AuditKind,
    pub guild_id: u64,
    pub channel_id: Option<u64>,
    pub target_id: Option<u64>,
    pub actor_user_id: Option<u64>,
    pub actor_label: Option<String>,
    pub subject_label: String,
    pub summary: String,
    pub details: Vec<(String, String)>,
    pub color: u32,
    pub occurred_at: DateTime<Utc>,
    pub dedupe_key: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AuditReadiness {
    pub enabled: bool,
    pub channel_ready: bool,
    pub last_error: Option<String>,
    pub last_event_at: Option<DateTime<Utc>>,
    pub last_delivery_at: Option<DateTime<Utc>>,
}
