use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordConfigItem {
    pub id: String,
    pub name: String,
    pub guild_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordChannelItem {
    pub id: String,
    pub discord_config_id: String,
    pub name: String,
    pub channel_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordRoleItem {
    pub id: String,
    pub discord_config_id: String,
    pub name: String,
    pub role_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordCategoryItem {
    pub id: String,
    pub discord_config_id: String,
    pub name: String,
    pub category_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DiscordConfigBundle {
    pub configs: Vec<DiscordConfigItem>,
    pub channels: Vec<DiscordChannelItem>,
    pub roles: Vec<DiscordRoleItem>,
    pub categories: Vec<DiscordCategoryItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigTargets {
    pub announcements: Vec<ResolvedChannelTarget>,
    pub event_postings: Vec<ResolvedChannelTarget>,
    pub staffup: Vec<ResolvedChannelTarget>,
    pub audit_log: Vec<ResolvedChannelTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedChannelTarget {
    pub config_name: String,
    pub guild_id: u64,
    pub channel_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRoleTarget {
    pub config_name: String,
    pub guild_id: u64,
    pub role_id: u64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImpromptuSelectorMessageState {
    pub guild_id: u64,
    pub channel_id: u64,
    pub message_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BreakBoardMessageState {
    pub guild_id: u64,
    pub channel_id: u64,
    pub preference_message_id: u64,
    pub request_message_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BreakBoardRequestState {
    pub request_message_id: u64,
    pub guild_id: u64,
    pub channel_id: u64,
    pub requester_user_id: u64,
    pub requester_mention: String,
    pub role_id: u64,
    pub role_name: String,
    pub audience_label: String,
    pub display_position: String,
    pub minutes_before_close: i64,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub claimed_by_user_id: Option<u64>,
    pub claimed_by_mention: Option<String>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claimed_message_id: Option<u64>,
    pub claimed_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BreakBoardRequestsState {
    pub items: Vec<BreakBoardRequestState>,
}

impl DiscordConfigBundle {
    pub fn resolve_targets(&self) -> AppResult<ConfigTargets> {
        Ok(ConfigTargets {
            announcements: self.resolve_named_channels("announcements")?,
            event_postings: self.resolve_named_channels("event_position_posting")?,
            staffup: self.resolve_named_channels("staffup")?,
            audit_log: self.resolve_named_channels("audit_log")?,
        })
    }

    pub fn resolve_announcement_targets(
        &self,
        channel: &str,
    ) -> AppResult<Vec<ResolvedChannelTarget>> {
        let targets = self.resolve_named_channels(channel)?;
        if targets.is_empty() {
            return Err(AppError::Config(format!(
                "osmium discord config bundle does not define a `{channel}` channel"
            )));
        }
        Ok(targets)
    }

    pub fn resolve_event_posting_targets(&self) -> AppResult<Vec<ResolvedChannelTarget>> {
        let targets = self.resolve_named_channels("event_position_posting")?;
        if targets.is_empty() {
            return Err(AppError::Config(
                "osmium discord config bundle does not define an `event_position_posting` channel"
                    .into(),
            ));
        }
        Ok(targets)
    }

    pub fn resolve_staffup_targets(&self) -> AppResult<Vec<ResolvedChannelTarget>> {
        let targets = self.resolve_named_channels("staffup")?;
        if targets.is_empty() {
            return Err(AppError::Config(
                "osmium discord config bundle does not define a `staffup` channel".into(),
            ));
        }
        Ok(targets)
    }

    pub fn resolve_audit_log_targets(&self) -> AppResult<Vec<ResolvedChannelTarget>> {
        let targets = self.resolve_named_channels("audit_log")?;
        if targets.is_empty() {
            return Err(AppError::Config(
                "osmium discord config bundle does not define an `audit_log` channel".into(),
            ));
        }
        Ok(targets)
    }

    pub fn resolve_role_prefix(&self, prefix: &str) -> AppResult<Vec<ResolvedRoleTarget>> {
        let mut resolved = Vec::new();

        for role in self
            .roles
            .iter()
            .filter(|item| item.name.starts_with(prefix))
        {
            let Some(config) = self
                .configs
                .iter()
                .find(|config| config.id == role.discord_config_id)
            else {
                continue;
            };
            let guild_id = parse_u64(
                config.guild_id.as_deref().ok_or_else(|| {
                    AppError::Config(format!(
                        "discord config `{}` is missing guild_id for role prefix `{prefix}`",
                        config.name
                    ))
                })?,
                "guild_id",
            )?;

            resolved.push(ResolvedRoleTarget {
                config_name: config.name.clone(),
                guild_id,
                role_id: parse_u64(&role.role_id, "role_id")?,
                name: role.name.clone(),
            });
        }

        resolved.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(resolved)
    }

    pub fn validate_shape(&self) -> AppResult<()> {
        for config in &self.configs {
            let guild_id = config.guild_id.as_deref().ok_or_else(|| {
                AppError::Config(format!(
                    "discord config `{}` is missing a guild_id",
                    config.name
                ))
            })?;
            parse_u64(guild_id, "guild_id")?;
        }

        for channel in &self.channels {
            parse_u64(&channel.channel_id, "channel_id")?;
        }

        for role in &self.roles {
            parse_u64(&role.role_id, "role_id")?;
        }

        for category in &self.categories {
            parse_u64(&category.category_id, "category_id")?;
        }

        Ok(())
    }

    pub fn resolve_named_channels(&self, name: &str) -> AppResult<Vec<ResolvedChannelTarget>> {
        let mut resolved = Vec::new();

        for channel in self.channels.iter().filter(|item| item.name == name) {
            let Some(config) = self
                .configs
                .iter()
                .find(|config| config.id == channel.discord_config_id)
            else {
                continue;
            };
            let guild_id = parse_u64(
                config.guild_id.as_deref().ok_or_else(|| {
                    AppError::Config(format!(
                        "discord config `{}` is missing guild_id for channel `{name}`",
                        config.name
                    ))
                })?,
                "guild_id",
            )?;

            resolved.push(ResolvedChannelTarget {
                config_name: config.name.clone(),
                guild_id,
                channel_id: parse_u64(&channel.channel_id, "channel_id")?,
            });
        }

        Ok(resolved)
    }
}

fn parse_u64(value: &str, field: &str) -> AppResult<u64> {
    value
        .parse::<u64>()
        .map_err(|_| AppError::Config(format!("invalid {field} `{value}` in osmium config")))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BotFeatureFlag {
    pub key: String,
    #[serde(default)]
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BotFeatureFlagsResponse {
    pub features: Vec<BotFeatureFlag>,
}

impl BotFeatureFlagsResponse {
    pub fn into_map(self) -> std::collections::HashMap<String, bool> {
        self.features
            .into_iter()
            .map(|flag| (flag.key, flag.enabled))
            .collect()
    }
}
