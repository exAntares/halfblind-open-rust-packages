// src/context.rs
use futures_util::stream::SplitSink;
use std::sync::Arc;
use std::sync::Mutex;
use uuid::Uuid;

pub struct ConnectionContext {
    pub player_uuid: Mutex<Option<Uuid>>,
    pub is_player_connected: Mutex<bool>,
    pub ws_writer: Arc<
        tokio::sync::Mutex<SplitSink<axum::extract::ws::WebSocket, axum::extract::ws::Message>>,
    >,
}

impl ConnectionContext {
    pub fn set_user(&self, id: Uuid) {
        let mut guard = self.player_uuid.lock().unwrap();
        *guard = Some(id);
        *self.is_player_connected.lock().unwrap() = true;
    }

    pub fn get_player_uuid(&self) -> Option<Uuid> {
        let lock = self.player_uuid.lock();
        let guard = lock.unwrap();
        guard.clone()
    }

    pub fn set_is_player_connected(&self, is_connected: bool) {
        *self.is_player_connected.lock().unwrap() = is_connected;
    }

    pub fn is_player_connected(&self) -> bool {
        self.is_player_connected.lock().unwrap().clone()
    }
}
