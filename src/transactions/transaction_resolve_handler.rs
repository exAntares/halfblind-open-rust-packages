use crate::systems::systems::SYSTEMS;
use halfblind_network::*;
use protobuf_itemdefinition::{TransactionResolveRequest, TransactionResolveResponse};
use halfblind_protobuf_network::*;
use halfblind_transactions::resolve_expired_transaction;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

request_handler!(TransactionResolveRequest => TransactionResolveHandler);

async fn handle(
        message_id: u64,
        message_timestamp: u64,
        req: TransactionResolveRequest,
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, Box<dyn Error + Send + Sync>> {
    let player_uuid = match validate_player_context(&ctx, message_id) {
        Ok(result) => result,
        Err(response) => return Ok(response),
    };
    let transaction_id = match Uuid::parse_str(&req.id) {
        Ok(id) => id,
        Err(_) => {
            return Ok(build_error_response(
                message_id,
                ErrorCode::InvalidRequest.into(),
                "",
            ));
        }
    };

    let db_pool = SYSTEMS.database_service.get_db_pool();
    let result = resolve_expired_transaction(
        &transaction_id,
        player_uuid,
        db_pool.as_ref(),
    )
        .await;
    match result {
        Err((error_code, _)) => {
            return Ok(build_error_response(message_id, error_code.into(), ""));
        }
        _ => {}
    };
    let inventory = SYSTEMS
        .inventory_service
        .get_player_inventory(player_uuid)
        .await?;
    let response = TransactionResolveResponse {
        inventory: inventory.read().await.clone(),
    };
    Ok(encode_ok(message_id, response)?)
}
