use std::path::Path;

use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};

use crate::errors::AppResult;

const DEFAULT_FILTER: &str = "info,serenity=info,hyper=warn";
const LOG_DIR: &str = "logs";
const LOG_FILE_PREFIX: &str = "vzdc-discord-bot.log";

pub struct LoggingGuard {
    _console_guard: tracing_appender::non_blocking::WorkerGuard,
    _file_guard: tracing_appender::non_blocking::WorkerGuard,
}

pub fn init() -> AppResult<LoggingGuard> {
    std::fs::create_dir_all(Path::new(LOG_DIR))?;

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| DEFAULT_FILTER.into());

    let (console_writer, console_guard) = tracing_appender::non_blocking(std::io::stdout());
    let file_appender = tracing_appender::rolling::daily(LOG_DIR, LOG_FILE_PREFIX);
    let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);

    let console_layer = tracing_subscriber::fmt::layer()
        .with_writer(console_writer)
        .with_target(false)
        .with_filter(filter.clone());
    let file_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .with_writer(file_writer)
        .with_filter(filter);

    tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer)
        .try_init()
        .map_err(|error| {
            crate::errors::AppError::Config(format!("failed to initialize logging: {error}"))
        })?;

    Ok(LoggingGuard {
        _console_guard: console_guard,
        _file_guard: file_guard,
    })
}
