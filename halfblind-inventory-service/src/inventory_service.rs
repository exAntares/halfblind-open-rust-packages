use async_trait::async_trait;
use protobuf_itemdefinition::InventoryItem;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[async_trait]
pub trait InventoryService {
    async fn get_player_inventory(
        &self,
        player_uuid: Uuid,
    ) -> Result<Arc<RwLock<Vec<InventoryItem>>>, sqlx::Error>;

    async fn get_inventory(
        &self,
        player_uuid: Uuid,
        character_uuid: Uuid,
    ) -> Result<Arc<RwLock<Vec<InventoryItem>>>, sqlx::Error>;

    async fn get_definition_value_summed(
        &self,
        player_uuid: Uuid,
        owner_uuid: Uuid,
        item_definition_id: u64,
    ) -> Result<i64, Box<dyn Error + Send + Sync>>;

    async fn save_character_inventory(
        &self,
        player_uuid: Uuid,
        character_uuid: Uuid,
    ) -> Result<(), sqlx::Error>;

    ///
    /// Returns items that were unable to be aggregated due the inventory limits.
    ///
    async fn aggregate_inventories(
        &self,
        player_uuid: Uuid,
        secondary_uuid: Uuid,
        inventory: Vec<InventoryItem>,
    ) -> Result<Vec<InventoryItem>, sqlx::Error>;

    fn try_aggregate_inventories(
        &self,
        source: Vec<InventoryItem>,
        target: &mut Vec<InventoryItem>,
    ) -> Vec<InventoryItem>;

    fn generate_inventory_item_for_player(
        &self,
        player_uuid: Uuid,
        definition_id: u64,
        amount: u64,
    ) -> InventoryItem;
}
