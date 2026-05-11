use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};

use crate::{
    errors::{AppError, AppResult},
    models::{
        ControllerEventsResponse, DiscordConfigBundle, Event, EventPositionListResponse,
        ServiceAccountSession,
    },
};

#[derive(Debug, Clone)]
pub struct OsmiumClient {
    base_url: String,
    client: reqwest::Client,
}

impl OsmiumClient {
    pub fn new(base_url: String, bearer_token: &str) -> AppResult<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {bearer_token}"))
                .map_err(|_| AppError::Config("invalid OSMIUM_BEARER_TOKEN".into()))?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self { base_url, client })
    }

    pub async fn verify_service_account(&self) -> AppResult<ServiceAccountSession> {
        self.get("/api/v1/auth/service-account/me").await
    }

    pub async fn fetch_discord_config_bundle(&self) -> AppResult<DiscordConfigBundle> {
        self.get("/api/v1/admin/integrations/discord/configs").await
    }

    pub async fn fetch_event(&self, event_id: &str) -> AppResult<Event> {
        self.get(&format!("/api/v1/events/{event_id}")).await
    }

    pub async fn fetch_event_positions(
        &self,
        event_id: &str,
    ) -> AppResult<EventPositionListResponse> {
        self.get(&format!(
            "/api/v1/events/{event_id}/positions?page=1&page_size=200"
        ))
        .await
    }

    pub async fn fetch_controller_events(
        &self,
        after_id: i64,
        limit: u64,
        environment: &str,
    ) -> AppResult<ControllerEventsResponse> {
        self.get(&format!(
            "/api/v1/stats/controller-events?environment={environment}&after_id={after_id}&limit={limit}"
        ))
        .await
    }

    async fn get<T>(&self, path: &str) -> AppResult<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let response = self
            .client
            .get(format!("{}{}", self.base_url, path))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error body".to_string());
            return Err(AppError::Osmium(format!(
                "{path} failed with {status}: {body}"
            )));
        }

        response.json::<T>().await.map_err(Into::into)
    }
}
