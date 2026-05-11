use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};

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
    pub delivery: Arc<dyn DiscordDelivery>,
}

pub struct RuntimeState {
    pub readiness: RwLock<ReadinessSnapshot>,
    pub config_bundle: RwLock<Option<DiscordConfigBundle>>,
    pub staffup_cursor: RwLock<StaffupCursorState>,
    pub break_board_requests: tokio::sync::Mutex<BreakBoardRequestsState>,
    delivery_keys: tokio::sync::Mutex<DeliveryDeduper>,
    recent_messages: tokio::sync::Mutex<RecentMessageCache>,
}

impl RuntimeState {
    pub fn new() -> Self {
        Self {
            readiness: RwLock::new(ReadinessSnapshot::default()),
            config_bundle: RwLock::new(None),
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
