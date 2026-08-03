use async_trait::async_trait;
use chrono::NaiveDateTime;
use halfblind_transactions::{DelayedReward, DelayedRewardsDatabaseInserter, TransactionRecord};
use sqlx::PgConnection;
use uuid::Uuid;

pub struct PostgresDelayedRewardsInserter<'a> {
    pub connection: &'a mut PgConnection,
}

#[async_trait]
impl DelayedRewardsDatabaseInserter for PostgresDelayedRewardsInserter<'_> {
    async fn insert_delayed_rewards_into_database(
        &mut self,
        player_uuid: Uuid,
        rewards: Vec<DelayedReward>,
    ) -> Result<Vec<TransactionRecord>, sqlx::Error> {
        if rewards.is_empty() {
            return Ok(Vec::new());
        }
        let end_times: Vec<NaiveDateTime> = rewards.iter().map(|r| r.end_at).collect();
        let item_ids: Vec<i64> = rewards.iter().map(|r| r.item_id).collect();
        let quantities: Vec<i64> = rewards.iter().map(|r| r.quantity).collect();

        sqlx::query_as::<_, TransactionRecord>(
            r#"
            WITH inserted AS (
                INSERT INTO player_transactions (player_uuid, end_at, item_id, quantity)
                SELECT $1,
                       unnest($2::timestamp[]),
                       unnest($3::bigint[]),
                       unnest($4::bigint[])
                RETURNING id, end_at, item_id, quantity
            )
            SELECT
                inserted.id,
                inserted.end_at,
                inserted.item_id,
                inserted.quantity
            FROM inserted
            "#,
        )
            .bind(player_uuid)
            .bind(&end_times)
            .bind(&item_ids)
            .bind(&quantities)
            .fetch_all(&mut *self.connection)
            .await
    }
}
