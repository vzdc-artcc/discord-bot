use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::models::AuditReadiness;

#[derive(Debug, Clone, Default)]
pub struct ReadinessSnapshot {
    pub discord_connected: bool,
    pub config_loaded: bool,
    pub last_config_sync_at: Option<DateTime<Utc>>,
    pub last_config_error: Option<String>,
    pub staffup: StaffupReadiness,
    pub audit: AuditReadiness,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StaffupReadiness {
    pub enabled: bool,
    pub channel_ready: bool,
    pub last_polled_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_event_id: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub service: &'static str,
    pub status: &'static str,
    pub version: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadyResponse {
    pub ready: bool,
    pub discord_connected: bool,
    pub config_loaded: bool,
    pub last_config_sync_at: Option<DateTime<Utc>>,
    pub last_config_error: Option<String>,
    pub staffup: StaffupReadiness,
    pub audit: AuditReadiness,
}
