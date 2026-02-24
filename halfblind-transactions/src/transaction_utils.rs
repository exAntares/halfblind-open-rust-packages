use crate::transaction_models::TransactionRecord;
use halfblind_protobuf_network::*;
use halfblind_random::RandomService;
use protobuf_itemdefinition::{ItemsErrorCode, TransactionInstance, TransactionReward};
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

pub struct TransactionResult<T> {
    pub transaction_instance_id: Vec<TransactionInstance>,
    pub inventory: Vec<T>,
    pub rewarded: Vec<T>,
}

pub fn get_transaction_reward_random_value(
    random_service: Arc<dyn RandomService + Send + Sync>,
    transaction_reward: &TransactionReward,
) -> u64 {
    if transaction_reward.value > 0 {
        return transaction_reward.value;
    }
    let min = if transaction_reward.value_min <= transaction_reward.value_max { transaction_reward.value_min} else { transaction_reward.value_max};
    let max = if transaction_reward.value_max >= transaction_reward.value_min { transaction_reward.value_max} else { transaction_reward.value_min};
    random_service.random_range_u64(min, max)
}


pub async fn resolve_expired_transaction(
    transaction_id: &Uuid,
    player_uuid: Uuid,
    db_pool: &sqlx::PgPool,
) -> Result<(), (i32, Box<dyn Error + Send + Sync>)> {
    let transaction = sqlx::query_as::<_, TransactionRecord>(
        "SELECT * FROM player_transactions WHERE id = $1 AND player_uuid = $2",
    )
    .bind(transaction_id)
    .bind(player_uuid)
    .fetch_optional(db_pool)
    .await
    .map_err(|e| {
        (
            ItemsErrorCode::TransactionInvalid.into(),
            Box::new(e) as Box<dyn Error + Send + Sync>,
        )
    })?;

    let transaction = match transaction {
        Some(t) => t,
        None => {
            return Err((
                ItemsErrorCode::TransactionInvalid.into(),
                "Transaction not found".into(),
            ));
        }
    };

    // Check if the transaction is expired
    let now = chrono::Utc::now().naive_utc();
    if now < transaction.end_at {
        return Err((
            ItemsErrorCode::TransactionNotFinished.into(),
            "Transaction has not expired yet".into(),
        ));
    }

    // Start transaction
    let mut tx = db_pool.begin().await.map_err(|e| {
        (
            ErrorCode::UnknownError.into(),
            Box::new(e) as Box<dyn Error + Send + Sync>,
        )
    })?;

    // Update player inventory
    sqlx::query(
        r#"
        INSERT INTO player_inventory (player_uuid, owner_uuid, item_definition_id, quantity)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (player_uuid, owner_uuid, item_definition_id) DO UPDATE
        SET quantity = player_inventory.quantity + EXCLUDED.quantity
        "#,
    )
    .bind(player_uuid)
    .bind(player_uuid)
    .bind(transaction.item_id)
    .bind(transaction.quantity)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        (
            ErrorCode::UnknownError.into(),
            Box::new(e) as Box<dyn Error + Send + Sync>,
        )
    })?;

    // Delete the transaction
    sqlx::query!(
        "DELETE FROM player_transactions WHERE id = $1",
        transaction_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        (
            ErrorCode::UnknownError.into(),
            Box::new(e) as Box<dyn Error + Send + Sync>,
        )
    })?;

    // Commit transaction
    tx.commit().await.map_err(|e| {
        (
            ErrorCode::UnknownError.into(),
            Box::new(e) as Box<dyn Error + Send + Sync>,
        )
    })?;

    Ok(())
}
