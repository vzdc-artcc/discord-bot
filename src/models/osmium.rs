use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceAccountSession {
    pub id: String,
    pub key: String,
    pub name: String,
    pub roles: Vec<String>,
    pub permissions: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub id: String,
    pub title: String,
    pub event_type: Option<String>,
    pub host: Option<String>,
    pub description: Option<String>,
    pub status: String,
    pub published: bool,
    #[serde(default)]
    pub banner_asset_id: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventPosition {
    pub id: String,
    pub event_id: String,
    pub callsign: String,
    pub user_id: Option<String>,
    #[serde(default)]
    pub user_cid: Option<i64>,
    #[serde(default)]
    pub user_name: Option<String>,
    #[serde(default)]
    pub user_rating: Option<String>,
    #[serde(default)]
    pub user_discord_id: Option<String>,
    #[serde(default)]
    pub controlling_category: Option<String>,
    pub requested_slot: Option<i32>,
    pub assigned_slot: Option<i32>,
    #[serde(default)]
    pub final_position: Option<String>,
    pub published: bool,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventPositionListResponse {
    pub items: Vec<EventPosition>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControllerEventsResponse {
    pub environment: String,
    pub events: Vec<ControllerEventItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControllerEventItem {
    pub id: i64,
    pub environment: String,
    pub event_type: String,
    pub cid: i64,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub activation_id: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControllerPositionEventPayload {
    pub environment: String,
    pub artcc_id: String,
    pub cid: i64,
    pub user_id: Option<String>,
    pub session_id: String,
    pub activation_id: String,
    pub occurred_at: DateTime<Utc>,
    pub real_name: Option<String>,
    pub role: Option<String>,
    pub user_rating: Option<String>,
    pub requested_rating: Option<String>,
    pub position_id: String,
    pub facility_id: Option<String>,
    pub facility_name: Option<String>,
    pub position_name: Option<String>,
    pub position_type: Option<String>,
    pub radio_name: Option<String>,
    pub default_callsign: Option<String>,
    pub frequency: Option<f64>,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event_type", content = "data", rename_all = "snake_case")]
pub enum ControllerLifecyclePayload {
    ControllerLoggedOn(ControllerSessionEventPayload),
    ControllerLoggedOff(ControllerSessionEventPayload),
    PositionActivated(ControllerPositionEventPayload),
    PositionDeactivated(ControllerPositionEventPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControllerSessionEventPayload {
    pub environment: String,
    pub artcc_id: String,
    pub cid: i64,
    pub user_id: Option<String>,
    pub session_id: String,
    pub occurred_at: DateTime<Utc>,
    pub real_name: Option<String>,
    pub role: Option<String>,
    pub user_rating: Option<String>,
    pub requested_rating: Option<String>,
    pub primary_facility_id: Option<String>,
    pub primary_position_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct StaffupCursorState {
    pub last_event_id: i64,
    pub open_activations: std::collections::BTreeMap<String, OpenActivationState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenActivationState {
    pub session_id: String,
    pub callsign: String,
    pub real_name: Option<String>,
    pub user_rating: Option<String>,
    pub requested_rating: Option<String>,
    pub frequency: Option<f64>,
    pub logon_time: DateTime<Utc>,
    pub event_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscordUserLookupResponse {
    pub linked: bool,
    pub discord_id: Option<String>,
    pub discord_username: Option<String>,
    pub discord_global_name: Option<String>,
    pub cid: Option<i64>,
    pub user_id: Option<String>,
    pub name: Option<String>,
    pub rating: Option<String>,
    pub controller_status: Option<String>,
    pub membership_status: Option<String>,
    pub linked_at: Option<DateTime<Utc>>,
    pub link_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LinkedDiscordUserItem {
    pub cid: i64,
    pub name: String,
    pub discord_id: String,
    pub discord_username: Option<String>,
    pub linked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LinkedDiscordUsersResponse {
    pub items: Vec<LinkedDiscordUserItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub total_pages: i64,
    pub has_next: bool,
    pub has_prev: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordRoleMapping {
    pub id: String,
    pub discord_config_id: String,
    pub discord_role_id: String,
    pub rule_type: String,
    pub rule_value: String,
    pub priority: i32,
    pub is_additive: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordRoleMappingsResponse {
    pub items: Vec<DiscordRoleMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordComputedRolesResponse {
    pub cid: i64,
    pub roles_to_add: Vec<String>,
    pub roles_to_remove: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiMessageResponse {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImpromptuClaimRecordResponse {
    pub linked: bool,
}
