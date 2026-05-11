use std::{net::SocketAddr, path::PathBuf};

use serenity::all::{GuildId, RoleId, UserId};

use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Config {
    pub discord_token: String,
    pub discord_application_id: u64,
    pub bind_addr: SocketAddr,
    pub bot_api_shared_key: String,
    pub osmium_base_url: String,
    pub osmium_bearer_token: String,
    pub command_guild_ids: Vec<GuildId>,
    pub operator_role_ids: Vec<RoleId>,
    pub operator_user_ids: Vec<UserId>,
    pub audit_logging_enabled: bool,
    pub audit_include_bot_events: bool,
    pub audit_fetch_audit_logs: bool,
    pub audit_max_field_chars: usize,
    pub staffup_enabled: bool,
    pub staffup_poll_interval_secs: u64,
    pub staffup_batch_size: u64,
    pub staffup_cursor_path: PathBuf,
    pub staffup_environment: String,
    pub staffup_artcc_id: String,
    pub impromptu_selector_state_path: PathBuf,
    pub break_board_state_path: PathBuf,
    pub break_board_requests_path: PathBuf,
}

impl Config {
    pub fn from_env() -> AppResult<Self> {
        let bind_addr = std::env::var("BOT_BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:3010".to_string())
            .parse()?;

        Ok(Self {
            discord_token: required_var("DISCORD_TOKEN")?,
            discord_application_id: parse_required_u64("DISCORD_APPLICATION_ID")?,
            bind_addr,
            bot_api_shared_key: required_var("BOT_API_SHARED_KEY")?,
            osmium_base_url: required_var("OSMIUM_BASE_URL")?
                .trim_end_matches('/')
                .to_string(),
            osmium_bearer_token: required_var("OSMIUM_BEARER_TOKEN")?,
            command_guild_ids: parse_id_list("BOT_COMMAND_GUILD_IDS", GuildId::new)?,
            operator_role_ids: parse_id_list("BOT_OPERATOR_ROLE_IDS", RoleId::new)?,
            operator_user_ids: parse_id_list("BOT_OPERATOR_USER_IDS", UserId::new)?,
            audit_logging_enabled: parse_bool("AUDIT_LOGGING_ENABLED", true),
            audit_include_bot_events: parse_bool("AUDIT_INCLUDE_BOT_EVENTS", false),
            audit_fetch_audit_logs: parse_bool("AUDIT_FETCH_AUDIT_LOGS", true),
            audit_max_field_chars: parse_usize_with_default("AUDIT_MAX_FIELD_CHARS", 900)?,
            staffup_enabled: parse_bool("STAFFUP_ENABLED", true),
            staffup_poll_interval_secs: parse_u64_with_default("STAFFUP_POLL_INTERVAL_SECS", 10)?,
            staffup_batch_size: parse_u64_with_default("STAFFUP_BATCH_SIZE", 100)?,
            staffup_cursor_path: std::env::var("STAFFUP_CURSOR_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("data/staffup_cursor.json")),
            staffup_environment: std::env::var("STAFFUP_ENVIRONMENT")
                .unwrap_or_else(|_| "live".to_string())
                .trim()
                .to_string(),
            staffup_artcc_id: std::env::var("STAFFUP_ARTCC_ID")
                .unwrap_or_else(|_| "ZDC".to_string())
                .trim()
                .to_string(),
            impromptu_selector_state_path: std::env::var("IMPROMPTU_SELECTOR_STATE_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("data/impromptu_selector_message.json")),
            break_board_state_path: std::env::var("BREAK_BOARD_STATE_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("data/break_board_messages.json")),
            break_board_requests_path: std::env::var("BREAK_BOARD_REQUESTS_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("data/break_board_requests.json")),
        })
    }
}

fn required_var(name: &str) -> AppResult<String> {
    std::env::var(name)
        .map(|value| value.trim().to_string())
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::Config(format!("missing required env var {name}")))
}

fn parse_required_u64(name: &str) -> AppResult<u64> {
    required_var(name)?
        .parse::<u64>()
        .map_err(|_| AppError::Config(format!("env var {name} must be an unsigned integer")))
}

fn parse_u64_with_default(name: &str, default: u64) -> AppResult<u64> {
    match std::env::var(name) {
        Ok(value) => value
            .trim()
            .parse::<u64>()
            .map_err(|_| AppError::Config(format!("env var {name} must be an unsigned integer"))),
        Err(_) => Ok(default),
    }
}

fn parse_usize_with_default(name: &str, default: usize) -> AppResult<usize> {
    match std::env::var(name) {
        Ok(value) => value
            .trim()
            .parse::<usize>()
            .map_err(|_| AppError::Config(format!("env var {name} must be an unsigned integer"))),
        Err(_) => Ok(default),
    }
}

fn parse_bool(name: &str, default: bool) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn parse_id_list<T>(name: &str, map: impl Fn(u64) -> T) -> AppResult<Vec<T>> {
    let Some(raw) = std::env::var(name).ok() else {
        return Ok(Vec::new());
    };

    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value.parse::<u64>().map(&map).map_err(|_| {
                AppError::Config(format!(
                    "env var {name} contains invalid numeric id `{value}`"
                ))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_bool, parse_id_list, parse_u64_with_default};
    use serenity::all::GuildId;

    #[test]
    fn parses_numeric_id_list() {
        unsafe {
            std::env::set_var("BOT_COMMAND_GUILD_IDS", "123, 456");
        }

        let values = parse_id_list("BOT_COMMAND_GUILD_IDS", GuildId::new).unwrap();
        assert_eq!(values, vec![GuildId::new(123), GuildId::new(456)]);

        unsafe {
            std::env::remove_var("BOT_COMMAND_GUILD_IDS");
        }
    }

    #[test]
    fn parses_staffup_defaults() {
        assert_eq!(
            parse_u64_with_default("STAFFUP_BATCH_SIZE_DOES_NOT_EXIST", 100).unwrap(),
            100
        );
        assert!(parse_bool("STAFFUP_ENABLED_DOES_NOT_EXIST", true));
    }
}
