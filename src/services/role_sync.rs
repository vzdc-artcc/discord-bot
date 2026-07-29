use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

use chrono::Utc;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serenity::{
    Error as SerenityError,
    all::{GuildId, Member, RoleId, UserId},
};

use crate::{
    errors::{AppError, AppResult},
    models::{AuditEvent, AuditKind},
    state::AppState,
};

const ROLE_SYNC_AUDIT_COLOR: u32 = 0x5865F2;

/// A guild's full role-id set, cached across a sync so a batch doesn't refetch
/// the (invariant) role list once per user.
type GuildRolesCache = HashMap<GuildId, BTreeSet<RoleId>>;

#[derive(Debug, Clone, Default)]
pub struct RoleSyncOutcome {
    pub linked: bool,
    pub cid: Option<i64>,
    pub discord_id: Option<u64>,
    pub guilds_checked: usize,
    pub guilds_updated: usize,
    pub roles_added: usize,
    pub roles_removed: usize,
}

#[derive(Debug, Clone, Default)]
pub struct RoleSyncBatchOutcome {
    pub users_processed: usize,
    pub users_linked: usize,
    pub guilds_updated: usize,
    pub roles_added: usize,
    pub roles_removed: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoleSyncRequest {
    pub cid: Option<i64>,
    pub discord_id: Option<String>,
    pub reason: Option<String>,
}

pub async fn start_role_sync_worker(state: AppState) {
    if !state.config.role_sync_periodic_enabled {
        return;
    }

    let interval = Duration::from_secs(state.config.role_sync_interval_mins.saturating_mul(60));
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.tick().await;

        loop {
            ticker.tick().await;
            match run_full_sync(&state).await {
                Ok(outcome) => {
                    tracing::info!(
                        users_processed = outcome.users_processed,
                        roles_added = outcome.roles_added,
                        roles_removed = outcome.roles_removed,
                        "completed periodic role sync"
                    );
                }
                Err(error) => {
                    tracing::error!(?error, "periodic role sync failed");
                }
            }
        }
    });
}

pub async fn sync_roles_for_discord_id(
    state: &AppState,
    discord_id: u64,
    reason: &str,
) -> AppResult<RoleSyncOutcome> {
    if !state.runtime.feature_enabled("role_sync").await {
        return Ok(RoleSyncOutcome {
            discord_id: Some(discord_id),
            ..RoleSyncOutcome::default()
        });
    }
    let lookup = state.osmium.lookup_discord_user(discord_id).await?;
    let Some(cid) = lookup.cid else {
        return Ok(RoleSyncOutcome {
            linked: false,
            discord_id: Some(discord_id),
            ..RoleSyncOutcome::default()
        });
    };

    let mut role_cache = GuildRolesCache::new();
    sync_roles_for_link(state, cid, discord_id, reason, &mut role_cache).await
}

pub async fn sync_roles_for_cid(
    state: &AppState,
    cid: i64,
    reason: &str,
) -> AppResult<RoleSyncOutcome> {
    let mut role_cache = GuildRolesCache::new();
    sync_roles_for_cid_cached(state, cid, reason, &mut role_cache).await
}

async fn sync_roles_for_cid_cached(
    state: &AppState,
    cid: i64,
    reason: &str,
    role_cache: &mut GuildRolesCache,
) -> AppResult<RoleSyncOutcome> {
    if !state.runtime.feature_enabled("role_sync").await {
        return Ok(RoleSyncOutcome {
            cid: Some(cid),
            ..RoleSyncOutcome::default()
        });
    }
    let lookup = state.osmium.admin_lookup_cid(cid).await?;
    let Some(discord_id) = lookup
        .discord_id
        .as_deref()
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return Ok(RoleSyncOutcome {
            linked: false,
            cid: Some(cid),
            ..RoleSyncOutcome::default()
        });
    };

    sync_roles_for_link(state, cid, discord_id, reason, role_cache).await
}

pub async fn sync_roles_from_request(
    state: &AppState,
    request: RoleSyncRequest,
) -> AppResult<RoleSyncOutcome> {
    let reason = request.reason.as_deref().unwrap_or("webhook");

    if let Some(cid) = request.cid {
        return sync_roles_for_cid(state, cid, reason).await;
    }

    let discord_id = request
        .discord_id
        .as_deref()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| {
            AppError::BadRequest("role sync request requires `cid` or `discord_id`".into())
        })?;

    sync_roles_for_discord_id(state, discord_id, reason).await
}

pub async fn run_full_sync(state: &AppState) -> AppResult<RoleSyncBatchOutcome> {
    let mut page = 1i64;
    let mut outcome = RoleSyncBatchOutcome::default();
    // Guild role lists are invariant across the batch; fetch each once and reuse.
    let mut role_cache = GuildRolesCache::new();

    loop {
        let response = state.osmium.list_linked_discord_users(page).await?;
        if response.items.is_empty() {
            break;
        }

        for user in response.items {
            outcome.users_processed += 1;
            let sync =
                sync_roles_for_cid_cached(state, user.cid, "periodic_full_sync", &mut role_cache)
                    .await?;
            if sync.linked {
                outcome.users_linked += 1;
            }
            outcome.guilds_updated += sync.guilds_updated;
            outcome.roles_added += sync.roles_added;
            outcome.roles_removed += sync.roles_removed;
        }

        if !response.has_next {
            break;
        }
        page += 1;
    }

    if outcome.users_processed > 0 {
        crate::services::publish_audit_event(
            state,
            AuditEvent {
                kind: AuditKind::RoleSync,
                guild_id: primary_guild_id(state),
                channel_id: None,
                target_id: None,
                actor_user_id: None,
                actor_label: Some("Periodic Worker".to_string()),
                subject_label: "Periodic role sync".to_string(),
                summary: "Completed a periodic full role sync.".to_string(),
                details: vec![
                    (
                        "Users Processed".to_string(),
                        outcome.users_processed.to_string(),
                    ),
                    ("Users Linked".to_string(), outcome.users_linked.to_string()),
                    (
                        "Guilds Updated".to_string(),
                        outcome.guilds_updated.to_string(),
                    ),
                    ("Roles Added".to_string(), outcome.roles_added.to_string()),
                    (
                        "Roles Removed".to_string(),
                        outcome.roles_removed.to_string(),
                    ),
                ],
                color: ROLE_SYNC_AUDIT_COLOR,
                occurred_at: Utc::now(),
                dedupe_key: None,
            },
        )
        .await?;
    }

    Ok(outcome)
}

pub fn format_role_sync_outcome(outcome: &RoleSyncOutcome) -> String {
    if !outcome.linked {
        return "No linked Discord account was found for that target.".to_string();
    }

    format!(
        "Role sync complete for CID {}. Checked {} guild(s), updated {} guild(s), added {} role(s), removed {} role(s).",
        outcome.cid.unwrap_or_default(),
        outcome.guilds_checked,
        outcome.guilds_updated,
        outcome.roles_added,
        outcome.roles_removed
    )
}

async fn sync_roles_for_link(
    state: &AppState,
    cid: i64,
    discord_id: u64,
    reason: &str,
    role_cache: &mut GuildRolesCache,
) -> AppResult<RoleSyncOutcome> {
    let computed = state.osmium.compute_roles(cid).await?;
    let guild_ids = configured_guild_ids(state).await?;
    let add_ids = parse_role_ids(&computed.roles_to_add);
    let remove_ids = parse_role_ids(&computed.roles_to_remove);
    let mut outcome = RoleSyncOutcome {
        linked: true,
        cid: Some(cid),
        discord_id: Some(discord_id),
        guilds_checked: guild_ids.len(),
        ..RoleSyncOutcome::default()
    };

    for guild_id in guild_ids {
        let Some(member) = fetch_member(state, guild_id, discord_id).await? else {
            continue;
        };
        let guild_roles: &BTreeSet<RoleId> = if role_cache.contains_key(&guild_id) {
            &role_cache[&guild_id]
        } else {
            let guild_role_ids = guild_id.roles(&state.discord_http).await?;
            role_cache
                .entry(guild_id)
                .or_insert_with(|| guild_role_ids.keys().copied().collect())
        };
        let add_for_guild = add_ids
            .iter()
            .copied()
            .filter(|role_id| guild_roles.contains(role_id))
            .collect::<Vec<_>>();
        let remove_for_guild = remove_ids
            .iter()
            .copied()
            .filter(|role_id| guild_roles.contains(role_id))
            .collect::<Vec<_>>();

        let changed = apply_role_diff(state, &member, &add_for_guild, &remove_for_guild).await?;
        if changed.0 > 0 || changed.1 > 0 {
            outcome.guilds_updated += 1;
        }
        outcome.roles_added += changed.0;
        outcome.roles_removed += changed.1;
    }

    // Only emit an audit event when roles actually changed; a no-op sync (common
    // on member join and during a periodic full sync) would otherwise flood the
    // audit channel and hit Discord rate limits.
    if outcome.roles_added > 0 || outcome.roles_removed > 0 {
        crate::services::publish_audit_event(
            state,
            AuditEvent {
                kind: AuditKind::RoleSync,
                guild_id: primary_guild_id(state),
                channel_id: None,
                target_id: Some(discord_id),
                actor_user_id: None,
                actor_label: Some("Automation".to_string()),
                subject_label: format!("CID {cid}"),
                summary: format!("Completed role sync ({reason})."),
                details: vec![
                    ("Discord ID".to_string(), discord_id.to_string()),
                    (
                        "Guilds Checked".to_string(),
                        outcome.guilds_checked.to_string(),
                    ),
                    (
                        "Guilds Updated".to_string(),
                        outcome.guilds_updated.to_string(),
                    ),
                    ("Roles Added".to_string(), outcome.roles_added.to_string()),
                    (
                        "Roles Removed".to_string(),
                        outcome.roles_removed.to_string(),
                    ),
                ],
                color: ROLE_SYNC_AUDIT_COLOR,
                occurred_at: Utc::now(),
                dedupe_key: None,
            },
        )
        .await?;
    }

    Ok(outcome)
}

async fn configured_guild_ids(state: &AppState) -> AppResult<Vec<GuildId>> {
    let bundle = state
        .runtime
        .config_bundle
        .read()
        .await
        .clone()
        .ok_or_else(|| AppError::Config("discord config bundle is not loaded".into()))?;
    let mut guild_ids = BTreeSet::new();
    for config in bundle.configs {
        let Some(guild_id) = config.guild_id else {
            continue;
        };
        let guild_id = guild_id.parse::<u64>().map_err(|_| {
            AppError::Config(format!("invalid guild id `{guild_id}` in config bundle"))
        })?;
        guild_ids.insert(GuildId::new(guild_id));
    }
    Ok(guild_ids.into_iter().collect())
}

async fn fetch_member(
    state: &AppState,
    guild_id: GuildId,
    discord_id: u64,
) -> AppResult<Option<Member>> {
    match guild_id
        .member(&state.discord_http, UserId::new(discord_id))
        .await
    {
        Ok(member) => Ok(Some(member)),
        Err(SerenityError::Http(error)) if error.status_code() == Some(StatusCode::NOT_FOUND) => {
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

async fn apply_role_diff(
    state: &AppState,
    member: &Member,
    add_ids: &[RoleId],
    remove_ids: &[RoleId],
) -> AppResult<(usize, usize)> {
    let mut added = 0usize;
    let mut removed = 0usize;

    for &role_id in remove_ids {
        if member.roles.contains(&role_id) {
            member.remove_role(&state.discord_http, role_id).await?;
            removed += 1;
        }
    }

    for &role_id in add_ids {
        if !member.roles.contains(&role_id) {
            member.add_role(&state.discord_http, role_id).await?;
            added += 1;
        }
    }

    Ok((added, removed))
}

fn parse_role_ids(values: &[String]) -> Vec<RoleId> {
    values
        .iter()
        .filter_map(|value| value.parse::<u64>().ok())
        .map(RoleId::new)
        .collect()
}

fn primary_guild_id(state: &AppState) -> u64 {
    state
        .config
        .command_guild_ids
        .first()
        .map(|id| id.get())
        .unwrap_or_default()
}
