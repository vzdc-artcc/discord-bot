use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnouncementPayload {
    pub title: String,
    pub body_markdown: String,
    pub details_url: Option<String>,
    pub requested_by_cid: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventPositionPostingPayload {
    pub event_id: String,
    pub ping_users: bool,
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
