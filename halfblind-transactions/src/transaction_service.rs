use crate::TransactionResult;
use async_trait::async_trait;
use halfblind_database_service::DatabaseService;
use halfblind_inventory_service::InventoryService;
use halfblind_random::RandomService;
use protobuf_itemdefinition::{TransactionItem, TransactionReward};
use std::sync::Arc;
use uuid::Uuid;

#[async_trait]
pub trait TransactionService<T> {
    fn has_enough_item_definitions(
        &self,
        inventory: &Vec<T>,
        required: &Vec<TransactionItem>,
    ) -> bool;

    fn has_any_item_definitions(
        &self,
        inventory: &Vec<T>,
        any: &Vec<TransactionItem>,
    ) -> bool;

    fn get_instant_rewards_items_into_inventory(
        &self,
        player_uuid: Uuid,
        inventory: &mut Vec<T>,
        rewards: &Vec<TransactionReward>,
        inventory_service: Arc<dyn InventoryService<T> + Send + Sync>,
        random_service: Arc<dyn RandomService + Send + Sync>,
    );

    async fn process_player_transaction(
        &self,
        inventory_service: Arc<dyn InventoryService<T> + Send + Sync>,
        database_service: Arc<dyn DatabaseService + Send + Sync>,
        random_service: Arc<dyn RandomService + Send + Sync>,
        player_uuid: Uuid,
        secondary_uuid: Uuid,
        transaction: &protobuf_itemdefinition::Transaction,
    ) -> Result<TransactionResult<T>, i32>;
}