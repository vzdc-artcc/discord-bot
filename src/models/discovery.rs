use serde::Serialize;

/// A guild the bot is a member of, surfaced for configuration UIs.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiscoveredGuild {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GuildListResponse {
    pub guilds: Vec<DiscoveredGuild>,
}

/// A selectable channel within a guild.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiscoveredChannel {
    pub id: String,
    pub name: String,
    /// Human-readable channel kind, e.g. `text`, `voice`, `forum`.
    pub kind: String,
    /// Parent category id, when the channel is nested under one.
    pub parent_category_id: Option<String>,
}

/// A category (channel group) within a guild.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiscoveredCategory {
    pub id: String,
    pub name: String,
}

/// A selectable role within a guild.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiscoveredRole {
    pub id: String,
    pub name: String,
}

/// Full discovery snapshot for a single guild, used to populate the
/// website's Discord integration configuration dropdowns.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GuildDiscoveryResponse {
    pub guild_id: String,
    pub channels: Vec<DiscoveredChannel>,
    pub categories: Vec<DiscoveredCategory>,
    pub roles: Vec<DiscoveredRole>,
}
