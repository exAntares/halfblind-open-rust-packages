use crate::systems::systems::SYSTEMS;
use halfblind_network::*;
use halfblind_protobuf_network::{ErrorCode, ProtoResponse};
use halfblind_transactions::TransactionRecord;
use ::protobuf_itemdefinition::*;
use std::sync::Arc;

request_handler!(TransactionInstancesRequest => TransactionInstancesHandler);

async fn handle(
        _message_timestamp: u64,
        _: TransactionInstancesRequest,
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, ProtoResponse> {
    let player_uuid = validate_player_context(&ctx)?;
    let db_pool = SYSTEMS.database_service.get_db_pool();
    // Query pending transactions from the database
    let transactions = match sqlx::query_as::<_, TransactionRecord>(
        r#"
            SELECT
                id,
                player_uuid,
                end_at,
                item_id,
                quantity
            FROM player_transactions WHERE player_uuid = $1
            "#,
    )
        .bind(player_uuid)
        .fetch_all(db_pool.as_ref())
        .await {
        Ok(x) => x,
        Err(e) => return Err(build_error_response(ErrorCode::UnknownError.into(), &format!("Failed to query transactions: {}", e)))
    };

    let transactions_instances: Vec<TransactionInstance> = transactions
        .into_iter()
        .map(|t| TransactionInstance {
            id: t.id.to_string(),
            endtime: t.end_at.and_utc().timestamp() as u64,
            reward: Some(TransactionItem {
                id_ref: Some(ItemDefinitionRef {
                    id: t.item_id as u64,
                }),
                value: t.quantity as u64,
                item_instance_id: String::new(),
            }),
        })
        .collect();

    let response = TransactionInstancesResponse {
        transactions: transactions_instances,
    };
    encode_ok(&response)
}
