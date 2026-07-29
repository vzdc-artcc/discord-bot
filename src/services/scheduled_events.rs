use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serenity::all::{
    CreateAttachment, CreateScheduledEvent, GuildId, Http, ScheduledEventId, ScheduledEventType,
    Timestamp,
};

use crate::{
    errors::{AppError, AppResult},
    models::{DiscordConfigBundle, Event},
};

/// A Discord scheduled event the bot created for a website event, so a re-post
/// can delete the old one instead of stacking duplicates.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreatedScheduledEvent {
    guild_id: u64,
    scheduled_event_id: u64,
}

type ScheduledEventState = HashMap<String, Vec<CreatedScheduledEvent>>;

fn state_path() -> PathBuf {
    std::env::var("SCHEDULED_EVENT_STATE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data/scheduled_events.json"))
}

async fn load_state(path: &Path) -> ScheduledEventState {
    match tokio::fs::read(path).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => ScheduledEventState::default(),
    }
}

async fn save_state(path: &Path, state: &ScheduledEventState) -> AppResult<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp_path = path.with_extension("json.tmp");
    tokio::fs::write(&tmp_path, serde_json::to_vec_pretty(state)?).await?;
    tokio::fs::rename(&tmp_path, path).await?;
    Ok(())
}

fn configured_guild_ids(bundle: &DiscordConfigBundle) -> Vec<u64> {
    let mut ids: Vec<u64> = bundle
        .configs
        .iter()
        .filter_map(|config| config.guild_id.as_deref())
        .filter_map(|guild| guild.trim().parse::<u64>().ok())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Best-effort cover image, downloaded from the osmium CDN. Skipped (returns
/// `None`) if `EVENT_BANNER_BASE_URL` is unset, there's no banner, or the fetch
/// fails — the event is still created without an image.
async fn cover_attachment(http: &Http, event: &Event) -> Option<CreateAttachment> {
    let url = super::discord::event_banner_url(event)?;
    match CreateAttachment::url(http, &url).await {
        Ok(attachment) => Some(attachment),
        Err(error) => {
            tracing::warn!(
                ?error,
                "failed downloading event banner for scheduled event cover"
            );
            None
        }
    }
}

/// Create (or replace) a native Discord scheduled event in every configured
/// guild from a website event's details. External type, using `location`.
pub async fn create_event_scheduled_events(
    http: &Http,
    bundle: &DiscordConfigBundle,
    event: &Event,
    location: &str,
) -> AppResult<usize> {
    let guilds = configured_guild_ids(bundle);
    if guilds.is_empty() {
        return Err(AppError::Config(
            "osmium discord config bundle has no guild to create the scheduled event in".into(),
        ));
    }

    let path = state_path();
    let mut state = load_state(&path).await;

    // Replace any prior scheduled events for this website event.
    for old in state.remove(&event.id).unwrap_or_default() {
        if let Err(error) = GuildId::new(old.guild_id)
            .delete_scheduled_event(http, ScheduledEventId::new(old.scheduled_event_id))
            .await
        {
            tracing::warn!(
                ?error,
                guild_id = old.guild_id,
                scheduled_event_id = old.scheduled_event_id,
                "failed deleting previous scheduled event (already gone?)"
            );
        }
    }

    let start = Timestamp::from_unix_timestamp(event.starts_at.timestamp())
        .map_err(|_| AppError::Config("event has an invalid start time".into()))?;
    let end = Timestamp::from_unix_timestamp(event.ends_at.timestamp())
        .map_err(|_| AppError::Config("event has an invalid end time".into()))?;
    let description = event
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| super::discord::truncate_chars(value, 1000));
    let cover = cover_attachment(http, event).await;

    let mut created = Vec::new();
    for guild in guilds {
        // External events require an end time and a location.
        let mut builder =
            CreateScheduledEvent::new(ScheduledEventType::External, event.title.clone(), start)
                .end_time(end)
                .location(location.to_string());
        if let Some(description) = &description {
            builder = builder.description(description.clone());
        }
        if let Some(cover) = &cover {
            builder = builder.image(cover);
        }

        match GuildId::new(guild)
            .create_scheduled_event(http, builder)
            .await
        {
            Ok(scheduled) => {
                tracing::info!(
                    guild_id = guild,
                    scheduled_event_id = scheduled.id.get(),
                    event_id = %event.id,
                    "created discord scheduled event"
                );
                created.push(CreatedScheduledEvent {
                    guild_id: guild,
                    scheduled_event_id: scheduled.id.get(),
                });
            }
            Err(error) => {
                tracing::error!(?error, guild_id = guild, event_id = %event.id, "failed creating discord scheduled event");
            }
        }
    }

    if created.is_empty() {
        return Err(AppError::Discord(
            "failed to create any discord scheduled event".into(),
        ));
    }

    let count = created.len();
    state.insert(event.id.clone(), created);
    if let Err(error) = save_state(&path, &state).await {
        tracing::warn!(?error, "failed persisting scheduled event state");
    }
    Ok(count)
}
