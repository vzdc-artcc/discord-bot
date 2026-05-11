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
    pub requested_slot: Option<i32>,
    pub assigned_slot: Option<i32>,
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
