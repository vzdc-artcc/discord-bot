use serenity::all::{ChannelType, GuildId, Http};

use crate::{
    errors::AppResult,
    models::{
        DiscoveredCategory, DiscoveredChannel, DiscoveredGuild, DiscoveredRole,
        GuildDiscoveryResponse, GuildListResponse,
    },
};

/// List every guild the bot is currently a member of.
///
/// Used by the website's Discord configuration UI so operators can pick a
/// guild instead of pasting a snowflake id.
pub async fn list_guilds(http: &Http) -> AppResult<GuildListResponse> {
    let guilds = http.get_guilds(None, None).await?;

    let mut discovered: Vec<DiscoveredGuild> = guilds
        .into_iter()
        .map(|guild| DiscoveredGuild {
            id: guild.id.get().to_string(),
            name: guild.name,
        })
        .collect();
    discovered.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(GuildListResponse { guilds: discovered })
}

/// Enumerate the channels, categories, and roles of a single guild so the
/// website can populate channel/role selection dropdowns from live data.
pub async fn discover_guild(http: &Http, guild_id: GuildId) -> AppResult<GuildDiscoveryResponse> {
    let channels = guild_id.channels(http).await?;
    let roles = guild_id.roles(http).await?;

    let mut discovered_channels = Vec::new();
    let mut categories = Vec::new();

    for channel in channels.values() {
        if channel.kind == ChannelType::Category {
            categories.push(DiscoveredCategory {
                id: channel.id.get().to_string(),
                name: channel.name.clone(),
            });
        } else {
            discovered_channels.push(DiscoveredChannel {
                id: channel.id.get().to_string(),
                name: channel.name.clone(),
                kind: channel_kind_label(channel.kind),
                parent_category_id: channel.parent_id.map(|id| id.get().to_string()),
            });
        }
    }

    let mut discovered_roles: Vec<DiscoveredRole> = roles
        .values()
        // The implicit @everyone role shares the guild id and is not selectable.
        .filter(|role| role.id.get() != guild_id.get())
        .map(|role| DiscoveredRole {
            id: role.id.get().to_string(),
            name: role.name.clone(),
        })
        .collect();

    discovered_channels.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    categories.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    discovered_roles.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(GuildDiscoveryResponse {
        guild_id: guild_id.get().to_string(),
        channels: discovered_channels,
        categories,
        roles: discovered_roles,
    })
}

fn channel_kind_label(kind: ChannelType) -> String {
    match kind {
        ChannelType::Text => "text",
        ChannelType::Voice => "voice",
        ChannelType::Category => "category",
        ChannelType::News => "news",
        ChannelType::NewsThread => "news_thread",
        ChannelType::PublicThread => "public_thread",
        ChannelType::PrivateThread => "private_thread",
        ChannelType::Stage => "stage",
        ChannelType::Forum => "forum",
        ChannelType::Directory => "directory",
        _ => "unknown",
    }
    .to_string()
}
