use crate::TransactionResult;
use async_trait::async_trait;
use halfblind_random::RandomService;
use protobuf_itemdefinition::{TransactionItem, TransactionReward};
use sqlx::PgConnection;
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
        inventory: &mut Vec<T>,
        rewards: &Vec<TransactionReward>,
        random_service: Arc<dyn RandomService + Send + Sync>,
    );
    
    async fn process_inventory_transaction(
        &self,
        random_service: Arc<dyn RandomService + Send + Sync>,
        db_connection: &mut PgConnection,
        player_inventory: &mut Vec<T>,
        player_uuid: Uuid,
        required: Option<Vec<TransactionItem>>,
        required_negative: Option<Vec<TransactionItem>>,
        consumed: Option<Vec<TransactionItem>>,
        rewarded: Option<Vec<TransactionReward>>,
        rewards_random: Option<Vec<protobuf_itemdefinition::PoolWeightedItemsComponent>>,
    ) -> Result<TransactionResult<T>, i32>;
    
    async fn process_inventory_transaction_id(
        &self,
        random_service: Arc<dyn RandomService + Send + Sync>,
        db_connection: &mut PgConnection,
        player_inventory: &mut Vec<T>,
        player_uuid: Uuid,
        transaction_id: u64,
    ) -> Result<TransactionResult<T>, i32>;
}
