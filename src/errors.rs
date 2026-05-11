use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("address parse error: {0}")]
    AddrParse(#[from] std::net::AddrParseError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("http client error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("discord api error: {0}")]
    Serenity(#[from] serenity::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("task join error: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("osmium api error: {0}")]
    Osmium(String),
    #[error("discord delivery error: {0}")]
    Discord(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("bad request: {0}")]
    BadRequest(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Config(_) | Self::Discord(_) | Self::Osmium(_) => StatusCode::BAD_GATEWAY,
            Self::AddrParse(_)
            | Self::Io(_)
            | Self::Http(_)
            | Self::Serenity(_)
            | Self::Json(_)
            | Self::Join(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let body = serde_json::json!({ "error": self.to_string() });
        (status, axum::Json(body)).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
