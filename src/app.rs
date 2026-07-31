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
        start_role_sync_worker, start_staffup_worker,
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
    // Slash-command registration (via `delivery`, below) goes through this Http
    // instance, so it needs the application id set on it directly — the id passed
    // to `Client::builder().application_id()` only reaches the gateway client's
    // separate Http, not this one. Without this, registering commands fails with
    // `Http(ApplicationIdMissing)`.
    http_client.set_application_id(config.discord_application_id.into());
    let current_user = http_client.get_current_user().await?;
    let delivery = Arc::new(SerenityDiscordService::new(
        http_client.clone(),
        current_user.face(),
    ));

    let state = AppState {
        config: config.clone(),
        osmium,
        runtime,
        discord_http: http_client.clone(),
        delivery,
    };

    let session = state.osmium.verify_service_account().await?;
    tracing::info!(id = %session.id, key = %session.key, "verified osmium service account");

    sync_config(&state).await?;
    // These interactive-message bootstraps depend on the osmium Discord config
    // (impromptu_training / break_board channels + roles). The bot must still boot
    // when that config is absent or incomplete so operators can configure it live
    // via the website using the guild-discovery endpoints; treat failures as
    // non-fatal warnings, matching how the staffup/audit workers degrade.
    if let Err(error) = ensure_impromptu_selector_message(&state).await {
        tracing::warn!(
            ?error,
            "skipping impromptu selector bootstrap; config incomplete"
        );
    }
    if let Err(error) = ensure_break_board_messages(&state).await {
        tracing::warn!(?error, "skipping break board bootstrap; config incomplete");
    }
    load_break_board_requests_into_runtime(&state).await?;
    start_break_board_cleanup_worker(state.clone()).await;
    start_staffup_worker(state.clone()).await;
    start_role_sync_worker(state.clone()).await;
    start_config_sync_worker(state.clone());

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

    // Pull runtime feature toggles alongside the config. Non-fatal: keep the last
    // known flags if osmium hiccups, so a transient failure doesn't flip features.
    match state.osmium.fetch_bot_feature_flags().await {
        Ok(flags) => {
            *state.runtime.feature_flags.write().await = flags.into_map();
        }
        Err(error) => {
            tracing::warn!(?error, "failed syncing bot feature flags; keeping previous");
        }
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

/// Periodically re-pull the Discord config bundle from osmium so channel/role
/// changes made in the website take effect on the running bot without a restart.
///
/// The config is otherwise only loaded once at startup; gateway shard reconnects
/// do NOT re-run startup, so without this a newly-added channel (e.g.
/// `event_position_posting`) never reaches the running process.
fn start_config_sync_worker(state: AppState) {
    let interval_secs = std::env::var("CONFIG_SYNC_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .unwrap_or(60);

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        // The startup call in `run()` already synced once; skip the immediate tick.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match sync_config(&state).await {
                Ok(()) => {
                    // Interactive-message bootstraps only run at startup; re-run them
                    // (idempotent — they no-op if the message already exists) so a
                    // channel/role added after boot brings the feature online too.
                    if let Err(error) = ensure_impromptu_selector_message(&state).await {
                        tracing::debug!(?error, "impromptu selector not ready after resync");
                    }
                    if let Err(error) = ensure_break_board_messages(&state).await {
                        tracing::debug!(?error, "break board not ready after resync");
                    }
                }
                Err(error) => {
                    tracing::warn!(?error, "periodic config sync failed");
                    let mut readiness = state.runtime.readiness.write().await;
                    readiness.last_config_error = Some(error.to_string());
                }
            }
        }
    });
}
