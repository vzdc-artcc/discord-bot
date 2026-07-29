use serenity::all::{
    ButtonStyle, ChannelId, ComponentInteraction, Context, CreateActionRow, CreateAllowedMentions,
    CreateButton, CreateEmbed, CreateInteractionResponse, CreateMessage, Http, MessageId, RoleId,
    UserId,
};

use crate::{
    errors::{AppError, AppResult},
    models::{
        DiscordConfigBundle, ImpromptuFinalizePayload, ImpromptuOfferPostPayload,
        ImpromptuOfferPostedResponse,
    },
    services::{IMPROMPTU_SELECTOR_CHANNEL_NAME, ephemeral_interaction_response},
    state::AppState,
};

pub const IMPROMPTU_OFFER_CLAIM_PREFIX: &str = "impromptu-offer-claim";

fn session_label(session_type: &str) -> String {
    match session_type.trim().to_lowercase().as_str() {
        "ground" => "Ground",
        "tower" => "Tower",
        "approach" => "Approach",
        "center" => "Center",
        other => return other.to_string(),
    }
    .to_string()
}

/// Role ids for the offered session types, mapped by the `impromptu_<type>`
/// naming convention (e.g. `ground` -> the `impromptu_ground` role).
fn offered_role_ids(bundle: &DiscordConfigBundle, session_types: &[String]) -> Vec<u64> {
    let mut ids = Vec::new();
    for session_type in session_types {
        let name = format!("impromptu_{}", session_type.trim().to_lowercase());
        if let Some(role) = bundle.roles.iter().find(|role| role.name == name)
            && let Ok(id) = role.role_id.parse::<u64>()
            && !ids.contains(&id)
        {
            ids.push(id);
        }
    }
    ids
}

pub async fn post_impromptu_offer(
    http: &Http,
    bundle: &DiscordConfigBundle,
    payload: &ImpromptuOfferPostPayload,
) -> AppResult<ImpromptuOfferPostedResponse> {
    let targets = bundle.resolve_named_channels(IMPROMPTU_SELECTOR_CHANNEL_NAME)?;
    let target = targets.first().ok_or_else(|| {
        AppError::Config(
            "osmium discord config bundle does not define an `impromptu_training` channel".into(),
        )
    })?;

    let positions = payload
        .session_types
        .iter()
        .map(|value| session_label(value))
        .collect::<Vec<_>>()
        .join(", ");
    let when = match payload.available_at {
        Some(at) => format!("<t:{}:F>", at.timestamp()),
        None => "Now".to_string(),
    };

    let mut embed = CreateEmbed::new()
        .color(0x9B59B6)
        .title("🎓 Impromptu Training Available")
        .field("Mentor / Instructor", payload.mentor_name.clone(), true)
        .field("Positions", positions, true)
        .field("Available", when, true);
    if let Some(notes) = payload
        .notes
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        embed = embed.description(notes.to_string());
    }
    embed = embed.field(
        "How to claim",
        "Press **Claim** below. Multiple people can claim — the mentor picks one.",
        false,
    );

    let claim_button = CreateButton::new(format!(
        "{IMPROMPTU_OFFER_CLAIM_PREFIX}:{}",
        payload.offer_id
    ))
    .label("Claim")
    .style(ButtonStyle::Success);

    let role_ids = offered_role_ids(bundle, &payload.session_types);
    let content = role_ids
        .iter()
        .map(|id| format!("<@&{id}>"))
        .collect::<Vec<_>>()
        .join(" ");
    let allowed = CreateAllowedMentions::new()
        .roles(role_ids.iter().map(|id| RoleId::new(*id)))
        .empty_users()
        .all_users(false);

    let message = ChannelId::new(target.channel_id)
        .send_message(
            http,
            CreateMessage::new()
                .content(content)
                .embed(embed)
                .components(vec![CreateActionRow::Buttons(vec![claim_button])])
                .allowed_mentions(allowed),
        )
        .await
        .map_err(|error| AppError::Discord(format!("failed posting impromptu offer: {error}")))?;

    Ok(ImpromptuOfferPostedResponse {
        channel_id: target.channel_id.to_string(),
        message_id: message.id.get().to_string(),
    })
}

/// DM the accepted + rejected students and delete the channel embed.
pub async fn finalize_impromptu_offer(http: &Http, payload: &ImpromptuFinalizePayload) {
    let session = if payload.session_types.is_empty() {
        String::new()
    } else {
        format!(
            " {}",
            payload
                .session_types
                .iter()
                .map(|value| session_label(value))
                .collect::<Vec<_>>()
                .join("/")
        )
    };

    if let Some(accepted) = &payload.accepted
        && let Some(id) = accepted
            .discord_id
            .as_deref()
            .and_then(|value| value.parse::<u64>().ok())
    {
        let message = format!(
            "✅ You've been selected for the impromptu{session} session with **{}**! They'll reach out shortly.",
            payload.mentor_name
        );
        dm_user(http, id, &message).await;
    }

    for rejected in &payload.rejected {
        if let Some(id) = rejected
            .discord_id
            .as_deref()
            .and_then(|value| value.parse::<u64>().ok())
        {
            let message = format!(
                "The impromptu{session} session with **{}** was claimed by someone else this time — thanks for jumping on it!",
                payload.mentor_name
            );
            dm_user(http, id, &message).await;
        }
    }

    if let (Some(channel_id), Some(message_id)) = (
        payload
            .channel_id
            .as_deref()
            .and_then(|value| value.parse::<u64>().ok()),
        payload
            .message_id
            .as_deref()
            .and_then(|value| value.parse::<u64>().ok()),
    ) && let Err(error) = ChannelId::new(channel_id)
        .delete_message(http, MessageId::new(message_id))
        .await
    {
        tracing::warn!(
            ?error,
            channel_id,
            message_id,
            "failed deleting impromptu offer message"
        );
    }
}

async fn dm_user(http: &Http, user_id: u64, content: &str) {
    match UserId::new(user_id).create_dm_channel(http).await {
        Ok(channel) => {
            if let Err(error) = channel
                .id
                .send_message(http, CreateMessage::new().content(content))
                .await
            {
                tracing::warn!(?error, user_id, "failed sending impromptu DM");
            }
        }
        Err(error) => tracing::warn!(?error, user_id, "failed opening DM channel"),
    }
}

/// Handle a Claim button press. Returns `Ok(true)` if this was an impromptu-offer
/// claim (handled), `Ok(false)` to let other component handlers try.
pub async fn handle_impromptu_offer_claim(
    ctx: &Context,
    state: &AppState,
    interaction: &ComponentInteraction,
) -> AppResult<bool> {
    let Some(offer_id) = interaction
        .data
        .custom_id
        .strip_prefix(&format!("{IMPROMPTU_OFFER_CLAIM_PREFIX}:"))
    else {
        return Ok(false);
    };

    if !state.runtime.feature_enabled("impromptu_offers").await {
        respond(ctx, interaction, "Impromptu offers are currently disabled.").await?;
        return Ok(true);
    }

    let discord_id = interaction.user.id.get();
    let response = match state
        .osmium
        .record_impromptu_claim(offer_id, discord_id)
        .await
    {
        Ok(result) if result.linked => {
            "You've claimed this session — the mentor will review and pick someone.".to_string()
        }
        Ok(_) => "Link your Discord account on the website first, then claim again.".to_string(),
        Err(error) => {
            tracing::warn!(?error, offer_id, "failed recording impromptu claim");
            "This offer is no longer open for claims.".to_string()
        }
    };
    respond(ctx, interaction, &response).await?;
    Ok(true)
}

async fn respond(
    ctx: &Context,
    interaction: &ComponentInteraction,
    content: &str,
) -> AppResult<()> {
    interaction
        .create_response(
            ctx,
            CreateInteractionResponse::Message(ephemeral_interaction_response(content)),
        )
        .await?;
    Ok(())
}
