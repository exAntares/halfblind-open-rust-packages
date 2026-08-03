use crate::{DelayedReward, TransactionRecord};
use async_trait::async_trait;
use uuid::Uuid;

///
/// We need to insert the delayed rewards into their own table.
/// We also want to abstract from the database, so we can create tests for the transaction service
/// without having to worry about the database classes
///
#[async_trait]
pub trait DelayedRewardsDatabaseInserter: Send + Sync {
    async fn insert_delayed_rewards_into_database(
        &mut self,
        player_uuid: Uuid,
        rewards: Vec<DelayedReward>,
    ) -> Result<Vec<TransactionRecord>, sqlx::Error>;
}