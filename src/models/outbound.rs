use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnouncementPayload {
    pub title: String,
    pub body_markdown: String,
    pub details_url: Option<String>,
    pub requested_by_cid: i64,
    /// Logical config channel name to post to; defaults to `announcements`.
    #[serde(default)]
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventPositionPostingPayload {
    pub event_id: String,
    pub ping_users: bool,
    pub requested_by_cid: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduledEventPayload {
    pub event_id: String,
    /// External event location text; defaults to `vatsim.net` when absent.
    #[serde(default)]
    pub location: Option<String>,
    pub requested_by_cid: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeliveryResponse {
    pub ok: bool,
    pub message: String,
    pub deduplicated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuildValidationReport {
    pub valid: bool,
    pub entries: Vec<GuildValidationEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuildValidationEntry {
    pub config_name: String,
    pub guild_id: String,
    pub channels_checked: usize,
    pub roles_checked: usize,
    pub categories_checked: usize,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StaffupOnlineEmbed {
    pub callsign: String,
    pub name: String,
    pub frequency: String,
    pub logon_unix: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StaffupOfflineEmbed {
    pub callsign: String,
    pub name: String,
    pub frequency: String,
    pub logon_unix: Option<i64>,
    pub logoff_unix: i64,
    pub duration: Option<String>,
}

// --- Impromptu session offers (instructor-initiated) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpromptuOfferPostPayload {
    pub offer_id: String,
    pub session_types: Vec<String>,
    pub mentor_name: String,
    #[serde(default)]
    pub available_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImpromptuOfferPostedResponse {
    pub channel_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ImpromptuParticipant {
    #[serde(default)]
    pub discord_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImpromptuFinalizePayload {
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub message_id: Option<String>,
    pub mentor_name: String,
    #[serde(default)]
    pub session_types: Vec<String>,
    #[serde(default)]
    pub accepted: Option<ImpromptuParticipant>,
    #[serde(default)]
    pub rejected: Vec<ImpromptuParticipant>,
}
