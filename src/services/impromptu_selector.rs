use std::path::{Path, PathBuf};

use serenity::all::{ComponentInteraction, Context, CreateInteractionResponse, RoleId};

use crate::{
    errors::{AppError, AppResult},
    models::{
        DiscordConfigBundle, ImpromptuSelectorMessageState, ResolvedChannelTarget,
        ResolvedRoleTarget,
    },
    services::{
        IMPROMPTU_SELECTOR_CHANNEL_NAME, IMPROMPTU_SELECTOR_ROLE_PREFIX,
        ephemeral_interaction_response, impromptu_selector_label,
        parse_impromptu_selector_custom_id,
    },
    state::AppState,
};

pub async fn ensure_impromptu_selector_message(state: &AppState) -> AppResult<()> {
    let bundle = current_bundle(state).await?;
    let (target, roles) = resolve_selector_targets(&bundle)?;
    tracing::info!(
        guild_id = target.guild_id,
        channel_id = target.channel_id,
        roles = roles.len(),
        "ensuring impromptu selector message"
    );
    let saved = load_selector_state(state).await?;
    let target_state = saved
        .filter(|saved| saved.guild_id == target.guild_id && saved.channel_id == target.channel_id);

    let next_state = if let Some(saved) = target_state {
        if state
            .delivery
            .impromptu_selector_message_exists(&saved)
            .await?
        {
            tracing::info!(
                guild_id = saved.guild_id,
                channel_id = saved.channel_id,
                request_message_id = saved.message_id,
                "refreshing persisted impromptu selector message"
            );
            state
                .delivery
                .refresh_impromptu_selector_message(&saved, &roles)
                .await?;
            saved
        } else {
            tracing::info!(
                guild_id = target.guild_id,
                channel_id = target.channel_id,
                "persisted impromptu selector message missing; recreating"
            );
            state
                .delivery
                .create_impromptu_selector_message(&target, &roles)
                .await?
        }
    } else {
        tracing::info!(
            guild_id = target.guild_id,
            channel_id = target.channel_id,
            "creating impromptu selector message"
        );
        state
            .delivery
            .create_impromptu_selector_message(&target, &roles)
            .await?
    };

    save_selector_state(state, &next_state).await?;
    Ok(())
}

pub async fn handle_impromptu_selector_interaction(
    ctx: &Context,
    state: &AppState,
    interaction: &ComponentInteraction,
) -> AppResult<bool> {
    let Some((custom_guild_id, role_id)) =
        parse_impromptu_selector_custom_id(&interaction.data.custom_id)
    else {
        return Ok(false);
    };
    tracing::info!(
        interaction_type = "component",
        custom_id = %interaction.data.custom_id,
        guild_id = interaction.guild_id.map(|id| id.get()),
        channel_id = interaction.channel_id.get(),
        user_id = interaction.user.id.get(),
        role_id,
        "handling impromptu selector interaction"
    );

    let Some(interaction_guild_id) = interaction.guild_id else {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "This selector only works inside a Discord server.",
                )),
            )
            .await?;
        return Ok(true);
    };

    let Some(member) = interaction.member.as_ref() else {
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "Your member record was not available for this role toggle.",
                )),
            )
            .await?;
        return Ok(true);
    };

    let bundle = current_bundle(state).await?;
    let (_, roles) = resolve_selector_targets(&bundle)?;
    let Some(role) = roles.iter().find(|role| {
        role.guild_id == custom_guild_id
            && role.guild_id == interaction_guild_id.get()
            && role.role_id == role_id
    }) else {
        tracing::warn!(
            guild_id = interaction_guild_id.get(),
            role_id,
            user_id = interaction.user.id.get(),
            "received stale impromptu selector option"
        );
        interaction
            .create_response(
                ctx,
                CreateInteractionResponse::Message(ephemeral_interaction_response(
                    "That selector option is no longer valid.",
                )),
            )
            .await?;
        return Ok(true);
    };

    let role_id = RoleId::new(role.role_id);
    let label = impromptu_selector_label(&role.name);
    let response = if member.roles.contains(&role_id) {
        member.remove_role(&ctx.http, role_id).await?;
        tracing::info!(
            role_id = role.role_id,
            user_id = interaction.user.id.get(),
            action = "remove",
            "removed impromptu selector role"
        );
        format!("Removed {label}")
    } else {
        member.add_role(&ctx.http, role_id).await?;
        tracing::info!(
            role_id = role.role_id,
            user_id = interaction.user.id.get(),
            action = "add",
            "added impromptu selector role"
        );
        format!("Added {label}")
    };

    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Message(ephemeral_interaction_response(response)),
        )
        .await?;

    Ok(true)
}

fn resolve_selector_targets(
    bundle: &DiscordConfigBundle,
) -> AppResult<(ResolvedChannelTarget, Vec<ResolvedRoleTarget>)> {
    let channels = bundle.resolve_named_channels(IMPROMPTU_SELECTOR_CHANNEL_NAME)?;
    if channels.is_empty() {
        return Err(AppError::Config(
            "osmium discord config bundle does not define an `impromptu_training` channel".into(),
        ));
    }
    if channels.len() > 1 {
        return Err(AppError::Config(
            "osmium discord config bundle defines multiple `impromptu_training` channels".into(),
        ));
    }

    let target = channels
        .into_iter()
        .next()
        .expect("checked non-empty channels");
    let roles = bundle.resolve_role_prefix(IMPROMPTU_SELECTOR_ROLE_PREFIX)?;
    if roles.is_empty() {
        return Err(AppError::Config(
            "osmium discord config bundle does not define any `impromptu_` roles".into(),
        ));
    }
    if roles.iter().any(|role| role.guild_id != target.guild_id) {
        return Err(AppError::Config(
            "impromptu selector roles must belong to the same guild as the `impromptu_training` channel".into(),
        ));
    }

    Ok((target, roles))
}

async fn current_bundle(state: &AppState) -> AppResult<DiscordConfigBundle> {
    state
        .runtime
        .config_bundle
        .read()
        .await
        .clone()
        .ok_or_else(|| AppError::Config("discord config bundle is not loaded".into()))
}

async fn load_selector_state(state: &AppState) -> AppResult<Option<ImpromptuSelectorMessageState>> {
    let path = &state.config.impromptu_selector_state_path;
    let Some(parent) = path.parent() else {
        return Ok(None);
    };
    tokio::fs::create_dir_all(parent).await?;
    if !path.exists() {
        return Ok(None);
    }

    let bytes = tokio::fs::read(path).await?;
    let parsed = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::BadRequest(format!(
            "failed to parse impromptu selector message state: {error}"
        ))
    })?;
    Ok(Some(parsed))
}

async fn save_selector_state(
    state: &AppState,
    selector: &ImpromptuSelectorMessageState,
) -> AppResult<()> {
    let path = &state.config.impromptu_selector_state_path;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp_path = tmp_path(path);
    let body = serde_json::to_vec_pretty(selector)?;
    tokio::fs::write(&tmp_path, body).await?;
    tokio::fs::rename(&tmp_path, path).await?;
    Ok(())
}

fn tmp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("impromptu_selector_message.json");
    path.with_file_name(format!("{file_name}.tmp"))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::models::{
        DiscordCategoryItem, DiscordChannelItem, DiscordConfigBundle, DiscordConfigItem,
        DiscordRoleItem,
    };

    use super::resolve_selector_targets;

    fn bundle() -> DiscordConfigBundle {
        DiscordConfigBundle {
            configs: vec![DiscordConfigItem {
                id: "config-1".into(),
                name: "main".into(),
                guild_id: Some("123".into()),
                created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
                updated_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
            }],
            channels: vec![DiscordChannelItem {
                id: "channel-1".into(),
                discord_config_id: "config-1".into(),
                name: "impromptu_training".into(),
                channel_id: "456".into(),
                created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
            }],
            roles: vec![
                DiscordRoleItem {
                    id: "role-1".into(),
                    discord_config_id: "config-1".into(),
                    name: "impromptu_s1".into(),
                    role_id: "789".into(),
                    created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
                },
                DiscordRoleItem {
                    id: "role-2".into(),
                    discord_config_id: "config-1".into(),
                    name: "impromptu_s2".into(),
                    role_id: "790".into(),
                    created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
                },
            ],
            categories: vec![DiscordCategoryItem {
                id: "category-1".into(),
                discord_config_id: "config-1".into(),
                name: "general".into(),
                category_id: "111".into(),
                created_at: chrono::Utc.with_ymd_and_hms(2026, 5, 10, 0, 0, 0).unwrap(),
            }],
        }
    }

    #[test]
    fn resolves_selector_target_and_roles() {
        let (target, roles) = resolve_selector_targets(&bundle()).unwrap();
        assert_eq!(target.guild_id, 123);
        assert_eq!(target.channel_id, 456);
        assert_eq!(roles.len(), 2);
        assert_eq!(roles[0].name, "impromptu_s1");
        assert_eq!(roles[1].name, "impromptu_s2");
    }

    #[test]
    fn rejects_missing_selector_roles() {
        let mut bundle = bundle();
        bundle.roles.clear();
        assert!(resolve_selector_targets(&bundle).is_err());
    }
}
