use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};

use serenity::all::Http;
use tokio::sync::RwLock;

use crate::{
    config::Config,
    models::{BreakBoardRequestsState, DiscordConfigBundle, ReadinessSnapshot, StaffupCursorState},
    osmium::OsmiumClient,
    services::DiscordDelivery,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub osmium: OsmiumClient,
    pub runtime: Arc<RuntimeState>,
    pub discord_http: Arc<Http>,
    pub delivery: Arc<dyn DiscordDelivery>,
}

pub struct RuntimeState {
    pub readiness: RwLock<ReadinessSnapshot>,
    pub config_bundle: RwLock<Option<DiscordConfigBundle>>,
    pub feature_flags: RwLock<HashMap<String, bool>>,
    pub staffup_cursor: RwLock<StaffupCursorState>,
    pub break_board_requests: tokio::sync::Mutex<BreakBoardRequestsState>,
    delivery_keys: tokio::sync::Mutex<DeliveryDeduper>,
    recent_messages: tokio::sync::Mutex<RecentMessageCache>,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeState {
    pub fn new() -> Self {
        Self {
            readiness: RwLock::new(ReadinessSnapshot::default()),
            config_bundle: RwLock::new(None),
            feature_flags: RwLock::new(HashMap::new()),
            staffup_cursor: RwLock::new(StaffupCursorState::default()),
            break_board_requests: tokio::sync::Mutex::new(BreakBoardRequestsState::default()),
            delivery_keys: tokio::sync::Mutex::new(DeliveryDeduper::default()),
            recent_messages: tokio::sync::Mutex::new(RecentMessageCache::default()),
        }
    }

    pub async fn mark_delivery_seen(&self, key: String) -> bool {
        let mut deduper = self.delivery_keys.lock().await;
        deduper.mark_seen(key)
    }

    /// Whether a bot feature/segment is enabled. Defaults to `true` when the flag
    /// hasn't been synced yet or is unknown, so features work before the first
    /// config sync and no feature is silently off due to a missing flag.
    pub async fn feature_enabled(&self, key: &str) -> bool {
        self.feature_flags
            .read()
            .await
            .get(key)
            .copied()
            .unwrap_or(true)
    }

    pub async fn store_message_snapshot(&self, snapshot: MessageSnapshot) {
        let mut cache = self.recent_messages.lock().await;
        cache.put(snapshot);
    }

    pub async fn recent_message_snapshot(
        &self,
        channel_id: u64,
        message_id: u64,
    ) -> Option<MessageSnapshot> {
        let cache = self.recent_messages.lock().await;
        cache.get(channel_id, message_id).cloned()
    }

    pub async fn remove_message_snapshot(
        &self,
        channel_id: u64,
        message_id: u64,
    ) -> Option<MessageSnapshot> {
        let mut cache = self.recent_messages.lock().await;
        cache.remove(channel_id, message_id)
    }
}

#[derive(Debug, Clone)]
pub struct MessageSnapshot {
    pub channel_id: u64,
    pub message_id: u64,
    pub author_id: u64,
    pub author_label: String,
    pub author_is_bot: bool,
    pub content: String,
}

#[derive(Default)]
struct DeliveryDeduper {
    keys: HashSet<String>,
    order: VecDeque<String>,
}

impl DeliveryDeduper {
    const CAPACITY: usize = 512;

    fn mark_seen(&mut self, key: String) -> bool {
        if self.keys.contains(&key) {
            return true;
        }

        self.keys.insert(key.clone());
        self.order.push_back(key);

        while self.order.len() > Self::CAPACITY {
            if let Some(old) = self.order.pop_front() {
                self.keys.remove(&old);
            }
        }

        false
    }
}

#[derive(Default)]
struct RecentMessageCache {
    items: HashMap<(u64, u64), MessageSnapshot>,
    order: VecDeque<(u64, u64)>,
}

impl RecentMessageCache {
    const CAPACITY: usize = 2_048;

    fn put(&mut self, snapshot: MessageSnapshot) {
        let key = (snapshot.channel_id, snapshot.message_id);
        if !self.items.contains_key(&key) {
            self.order.push_back(key);
        }
        self.items.insert(key, snapshot);

        while self.order.len() > Self::CAPACITY {
            if let Some(old) = self.order.pop_front() {
                self.items.remove(&old);
            }
        }
    }

    fn get(&self, channel_id: u64, message_id: u64) -> Option<&MessageSnapshot> {
        self.items.get(&(channel_id, message_id))
    }

    fn remove(&mut self, channel_id: u64, message_id: u64) -> Option<MessageSnapshot> {
        self.items.remove(&(channel_id, message_id))
    }
}
