use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};

use crate::{
    errors::{AppError, AppResult},
    models::{
        ApiMessageResponse, ControllerEventsResponse, DiscordComputedRolesResponse,
        DiscordConfigBundle, DiscordRoleMappingsResponse, DiscordUserLookupResponse, Event,
        EventPositionListResponse, LinkedDiscordUsersResponse, ServiceAccountSession,
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

    pub async fn fetch_bot_feature_flags(
        &self,
    ) -> AppResult<crate::models::BotFeatureFlagsResponse> {
        self.get("/api/v1/admin/integrations/discord/features")
            .await
    }

    pub async fn record_impromptu_claim(
        &self,
        offer_id: &str,
        discord_id: u64,
    ) -> AppResult<crate::models::ImpromptuClaimRecordResponse> {
        self.post(
            "/api/v1/admin/integrations/discord/impromptu-claims",
            &serde_json::json!({ "offer_id": offer_id, "discord_id": discord_id.to_string() }),
        )
        .await
    }

    pub async fn lookup_discord_user(
        &self,
        discord_id: u64,
    ) -> AppResult<DiscordUserLookupResponse> {
        self.get(&format!(
            "/api/v1/discord/lookup/by-discord-id/{discord_id}"
        ))
        .await
    }

    pub async fn admin_lookup_discord_user(
        &self,
        discord_id: u64,
    ) -> AppResult<DiscordUserLookupResponse> {
        self.get(&format!(
            "/api/v1/admin/integrations/discord/lookup/by-external-id/{discord_id}"
        ))
        .await
    }

    pub async fn admin_lookup_cid(&self, cid: i64) -> AppResult<DiscordUserLookupResponse> {
        self.get(&format!(
            "/api/v1/admin/integrations/discord/lookup/by-cid/{cid}"
        ))
        .await
    }

    pub async fn list_linked_discord_users(
        &self,
        page: i64,
    ) -> AppResult<LinkedDiscordUsersResponse> {
        self.get(&format!(
            "/api/v1/admin/integrations/discord/linked-users?page={page}&page_size=25"
        ))
        .await
    }

    pub async fn unlink_discord(&self, cid: i64) -> AppResult<ApiMessageResponse> {
        self.delete(&format!("/api/v1/admin/integrations/discord/link/{cid}"))
            .await
    }

    pub async fn list_role_mappings(&self) -> AppResult<DiscordRoleMappingsResponse> {
        self.get("/api/v1/admin/integrations/discord/role-mappings")
            .await
    }

    pub async fn compute_roles(&self, cid: i64) -> AppResult<DiscordComputedRolesResponse> {
        self.get(&format!(
            "/api/v1/admin/integrations/discord/compute-roles/{cid}"
        ))
        .await
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

    async fn delete<T>(&self, path: &str) -> AppResult<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let response = self
            .client
            .delete(format!("{}{}", self.base_url, path))
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

    async fn post<B, T>(&self, path: &str, body: &B) -> AppResult<T>
    where
        B: serde::Serialize,
        T: serde::de::DeserializeOwned,
    {
        let response = self
            .client
            .post(format!("{}{}", self.base_url, path))
            .json(body)
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
