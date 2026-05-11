use std::sync::Arc;

use serenity::{Client, all::GatewayIntents, http::Http};

use crate::{
    config::Config,
    discord,
    errors::AppResult,
    http,
    osmium::OsmiumClient,
    services::{
        SerenityDiscordService, ensure_break_board_messages, ensure_impromptu_selector_message,
        load_break_board_requests_into_runtime, start_break_board_cleanup_worker,
        start_staffup_worker,
    },
    state::{AppState, RuntimeState},
};

pub async fn run() -> AppResult<()> {
    let config = Config::from_env()?;
    tracing::info!(
        service = env!("CARGO_PKG_NAME"),
        version = env!("CARGO_PKG_VERSION"),
        addr = %config.bind_addr,
        staffup_enabled = config.staffup_enabled,
        command_guilds = config.command_guild_ids.len(),
        "starting discord bot"
    );

    let osmium = OsmiumClient::new(config.osmium_base_url.clone(), &config.osmium_bearer_token)?;
    let runtime = Arc::new(RuntimeState::new());
    let http_client = Arc::new(Http::new(&config.discord_token));
    let current_user = http_client.get_current_user().await?;
    let delivery = Arc::new(SerenityDiscordService::new(
        http_client.clone(),
        current_user.face(),
    ));

    let state = AppState {
        config: config.clone(),
        osmium,
        runtime,
        delivery,
    };

    let session = state.osmium.verify_service_account().await?;
    tracing::info!(id = %session.id, key = %session.key, "verified osmium service account");

    sync_config(&state).await?;
    ensure_impromptu_selector_message(&state).await?;
    ensure_break_board_messages(&state).await?;
    load_break_board_requests_into_runtime(&state).await?;
    start_break_board_cleanup_worker(state.clone()).await;
    start_staffup_worker(state.clone()).await;

    if !state.config.command_guild_ids.is_empty() {
        let count = state
            .delivery
            .register_commands(&state.config.command_guild_ids)
            .await?;
        tracing::info!(count, "registered configured slash command sets");
    }

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT
        | GatewayIntents::GUILD_MODERATION
        | GatewayIntents::GUILD_EMOJIS_AND_STICKERS;
    let handler = discord::Handler {
        state: state.clone(),
    };

    let mut client = Client::builder(&config.discord_token, intents)
        .application_id(config.discord_application_id.into())
        .event_handler(handler)
        .await?;

    let app = http::router(state.clone());
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "bot http listener ready");

    let http_task = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .map_err(crate::errors::AppError::from)
    });
    let discord_task =
        tokio::spawn(async move { client.start().await.map_err(crate::errors::AppError::from) });

    tokio::select! {
        result = http_task => {
            tracing::warn!("http task exited");
            result??
        },
        result = discord_task => {
            tracing::warn!("discord gateway task exited");
            result??
        },
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutdown signal received");
        }
    }

    tracing::info!("discord bot shutdown complete");
    Ok(())
}

#[tracing::instrument(skip(state))]
pub async fn sync_config(state: &AppState) -> AppResult<()> {
    tracing::info!("starting config sync from osmium");
    let bundle = state.osmium.fetch_discord_config_bundle().await?;
    bundle.validate_shape()?;

    {
        let mut guard = state.runtime.config_bundle.write().await;
        *guard = Some(bundle.clone());
    }

    let mut readiness = state.runtime.readiness.write().await;
    readiness.config_loaded = true;
    readiness.last_config_error = None;
    readiness.last_config_sync_at = Some(chrono::Utc::now());
    readiness.staffup.enabled = state.config.staffup_enabled;
    readiness.staffup.channel_ready = bundle.resolve_staffup_targets().is_ok();
    readiness.audit.enabled = state.config.audit_logging_enabled;
    readiness.audit.channel_ready =
        !state.config.audit_logging_enabled || bundle.resolve_audit_log_targets().is_ok();
    readiness.audit.last_error = if state.config.audit_logging_enabled {
        bundle
            .resolve_audit_log_targets()
            .err()
            .map(|error| error.to_string())
    } else {
        None
    };

    tracing::info!(
        configs = bundle.configs.len(),
        channels = bundle.channels.len(),
        roles = bundle.roles.len(),
        categories = bundle.categories.len(),
        staffup_channel_ready = readiness.staffup.channel_ready,
        audit_channel_ready = readiness.audit.channel_ready,
        "config sync completed"
    );

    Ok(())
}
