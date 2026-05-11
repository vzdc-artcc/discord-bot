pub mod app;
pub mod commands;
pub mod config;
pub mod discord;
pub mod errors;
pub mod http;
pub mod logging;
pub mod models;
pub mod osmium;
pub mod services;
pub mod state;

pub async fn run() -> Result<(), errors::AppError> {
    app::run().await
}
