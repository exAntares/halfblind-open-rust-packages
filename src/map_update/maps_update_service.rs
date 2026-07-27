use crate::map::game_map::GameMap;
use crate::map::maps_service::MapsService;
use halfblind_network::*;
use std::sync::Arc;
use tokio::task::JoinHandle;
use uuid::Uuid;

pub trait MapsUpdateService {
    fn start_update_loop(
        &self,
        maps_service: Arc<dyn MapsService + Send + Sync>,
        map: Arc<GameMap>
    ) -> JoinHandle<()>;

    fn start_broadcast_loop(
        &self,
        ctx: Arc<ConnectionContext>,
        player_uuid: Uuid,
        map: Arc<GameMap>,
    );
}
