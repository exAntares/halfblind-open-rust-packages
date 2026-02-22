use crate::map::game_map::GameMap;
use async_trait::async_trait;
use halfblind_network::*;
use protobuf_itemdefinition::InventoryItem;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

#[async_trait]
pub trait MapsService: Send + Sync {
    async fn change_player_map(
        &self,
        ctx: Arc<ConnectionContext>,
        player_uuid: Uuid,
        character_uuid: Uuid,
        visible_inventory: Vec<InventoryItem>,
        new_map_id: u64,
    ) -> Result<Arc<GameMap>, Box<dyn Error + Send + Sync>>;
    fn get_player_map(&self, player_uuid: &Uuid) -> Option<Arc<GameMap>>;
    async fn remove_player_from_all_maps(&self, player_uuid: &Uuid);
}
