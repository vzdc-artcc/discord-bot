use std::sync::Arc;

use serenity::{
    all::{Context, EventHandler, Interaction, Ready},
    async_trait,
};

use crate::{commands, services, state::AppState};

pub struct Handler {
    pub state: AppState,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        tracing::info!(user = %ready.user.name, "discord gateway ready");
        let mut readiness = self.state.runtime.readiness.write().await;
        readiness.discord_connected = true;
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Command(command) => {
                tracing::info!(
                    interaction_type = "command",
                    command = %command.data.name,
                    guild_id = command.guild_id.map(|id| id.get()),
                    channel_id = command.channel_id.get(),
                    user_id = command.user.id.get(),
                    "received discord interaction"
                );
                if let Err(error) = commands::handle_interaction(&ctx, &self.state, command).await {
                    tracing::error!(?error, "failed to handle slash command");
                }
            }
            Interaction::Component(component) => {
                tracing::info!(
                    interaction_type = "component",
                    custom_id = %component.data.custom_id,
                    guild_id = component.guild_id.map(|id| id.get()),
                    channel_id = component.channel_id.get(),
                    user_id = component.user.id.get(),
                    "received discord interaction"
                );
                let handled =
                    services::handle_impromptu_selector_interaction(&ctx, &self.state, &component)
                        .await;
                match handled {
                    Ok(true) => {}
                    Ok(false) => {
                        if let Err(error) = services::handle_break_board_component_interaction(
                            &ctx,
                            &self.state,
                            &component,
                        )
                        .await
                        {
                            tracing::error!(
                                ?error,
                                "failed to handle break board component interaction"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(?error, "failed to handle impromptu selector interaction");
                    }
                }
            }
            Interaction::Modal(modal) => {
                tracing::info!(
                    interaction_type = "modal",
                    custom_id = %modal.data.custom_id,
                    guild_id = modal.guild_id.map(|id| id.get()),
                    channel_id = modal.channel_id.get(),
                    user_id = modal.user.id.get(),
                    "received discord interaction"
                );
                if let Err(error) =
                    services::handle_break_board_modal_interaction(&ctx, &self.state, &modal).await
                {
                    tracing::error!(?error, "failed to handle break board modal interaction");
                }
            }
            _ => {}
        }
    }

    async fn resume(&self, _ctx: Context, _event: serenity::all::ResumedEvent) {
        let mut readiness = self.state.runtime.readiness.write().await;
        readiness.discord_connected = true;
    }

    async fn message_update(
        &self,
        ctx: Context,
        old_if_available: Option<serenity::all::Message>,
        new: Option<serenity::all::Message>,
        event: serenity::all::MessageUpdateEvent,
    ) {
        services::handle_message_update(&ctx, &self.state, old_if_available, new, event).await;
    }

    async fn message(&self, _ctx: Context, new_message: serenity::all::Message) {
        services::handle_message_create(&self.state, new_message).await;
    }

    async fn message_delete(
        &self,
        ctx: Context,
        channel_id: serenity::all::ChannelId,
        deleted_message_id: serenity::all::MessageId,
        guild_id: Option<serenity::all::GuildId>,
    ) {
        services::handle_message_delete(
            &ctx,
            &self.state,
            guild_id,
            channel_id,
            deleted_message_id,
        )
        .await;
    }

    async fn message_delete_bulk(
        &self,
        ctx: Context,
        channel_id: serenity::all::ChannelId,
        multiple_deleted_messages_ids: Vec<serenity::all::MessageId>,
        guild_id: Option<serenity::all::GuildId>,
    ) {
        services::handle_message_delete_bulk(
            &ctx,
            &self.state,
            guild_id,
            channel_id,
            multiple_deleted_messages_ids,
        )
        .await;
    }

    async fn channel_create(&self, ctx: Context, channel: serenity::all::GuildChannel) {
        services::handle_channel_create(&ctx, &self.state, channel).await;
    }

    async fn category_create(&self, ctx: Context, category: serenity::all::GuildChannel) {
        services::handle_channel_create(&ctx, &self.state, category).await;
    }

    async fn channel_update(
        &self,
        ctx: Context,
        old: Option<serenity::all::GuildChannel>,
        new: serenity::all::GuildChannel,
    ) {
        services::handle_channel_update(&ctx, &self.state, old, new).await;
    }

    async fn channel_delete(
        &self,
        ctx: Context,
        channel: serenity::all::GuildChannel,
        _messages: Option<Vec<serenity::all::Message>>,
    ) {
        services::handle_channel_delete(&ctx, &self.state, channel).await;
    }

    async fn category_delete(&self, ctx: Context, category: serenity::all::GuildChannel) {
        services::handle_channel_delete(&ctx, &self.state, category).await;
    }

    async fn guild_role_create(&self, ctx: Context, new: serenity::all::Role) {
        services::handle_role_create(&ctx, &self.state, new).await;
    }

    async fn guild_role_update(
        &self,
        ctx: Context,
        old_data_if_available: Option<serenity::all::Role>,
        new: serenity::all::Role,
    ) {
        services::handle_role_update(&ctx, &self.state, old_data_if_available, new).await;
    }

    async fn guild_role_delete(
        &self,
        ctx: Context,
        guild_id: serenity::all::GuildId,
        removed_role_id: serenity::all::RoleId,
        removed_role_data_if_available: Option<serenity::all::Role>,
    ) {
        services::handle_role_delete(
            &ctx,
            &self.state,
            guild_id,
            removed_role_id,
            removed_role_data_if_available,
        )
        .await;
    }

    async fn thread_create(&self, ctx: Context, thread: serenity::all::GuildChannel) {
        services::handle_thread_create(&ctx, &self.state, thread).await;
    }

    async fn thread_update(
        &self,
        ctx: Context,
        old: Option<serenity::all::GuildChannel>,
        new: serenity::all::GuildChannel,
    ) {
        services::handle_thread_update(&ctx, &self.state, old, new).await;
    }

    async fn thread_delete(
        &self,
        ctx: Context,
        thread: serenity::all::PartialGuildChannel,
        full_thread_data: Option<serenity::all::GuildChannel>,
    ) {
        services::handle_thread_delete(&ctx, &self.state, thread, full_thread_data).await;
    }

    async fn guild_member_addition(&self, ctx: Context, new_member: serenity::all::Member) {
        services::handle_member_addition(&ctx, &self.state, new_member).await;
    }

    async fn guild_member_removal(
        &self,
        ctx: Context,
        guild_id: serenity::all::GuildId,
        user: serenity::all::User,
        member_data_if_available: Option<serenity::all::Member>,
    ) {
        services::handle_member_removal(
            &ctx,
            &self.state,
            guild_id,
            user,
            member_data_if_available,
        )
        .await;
    }

    async fn guild_member_update(
        &self,
        ctx: Context,
        old_if_available: Option<serenity::all::Member>,
        new: Option<serenity::all::Member>,
        event: serenity::all::GuildMemberUpdateEvent,
    ) {
        services::handle_member_update(&ctx, &self.state, old_if_available, new, event).await;
    }

    async fn guild_ban_addition(
        &self,
        ctx: Context,
        guild_id: serenity::all::GuildId,
        banned_user: serenity::all::User,
    ) {
        services::handle_ban_addition(&ctx, &self.state, guild_id, banned_user).await;
    }

    async fn guild_ban_removal(
        &self,
        ctx: Context,
        guild_id: serenity::all::GuildId,
        unbanned_user: serenity::all::User,
    ) {
        services::handle_ban_removal(&ctx, &self.state, guild_id, unbanned_user).await;
    }

    async fn guild_emojis_update(
        &self,
        ctx: Context,
        guild_id: serenity::all::GuildId,
        current_state: std::collections::HashMap<serenity::all::EmojiId, serenity::all::Emoji>,
    ) {
        services::handle_emojis_update(&ctx, &self.state, guild_id, current_state).await;
    }

    async fn guild_stickers_update(
        &self,
        ctx: Context,
        guild_id: serenity::all::GuildId,
        current_state: std::collections::HashMap<serenity::all::StickerId, serenity::all::Sticker>,
    ) {
        services::handle_stickers_update(&ctx, &self.state, guild_id, current_state).await;
    }
}

pub fn handler(state: AppState) -> Arc<Handler> {
    Arc::new(Handler { state })
}
