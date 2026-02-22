use crate::map::game_map::GameMap;
use halfblind_network::*;
use std::sync::Arc;
use tokio::task::JoinHandle;
use uuid::Uuid;

pub trait MapsUpdateService {
    fn start_update_loop(&self, map: Arc<GameMap>) -> JoinHandle<()>;
    fn start_broadcast_loop(
        &self,
        ctx: Arc<ConnectionContext>,
        player_uuid: Uuid,
        map: Arc<GameMap>,
    );
}
