use crate::systems::systems::SYSTEMS;
use halfblind_network::*;
use halfblind_protobuf_network::*;
use halfblind_transactions::resolve_expired_transaction;
use proto_gen::{TransactionResolveRequest, TransactionResolveResponse};
use std::sync::Arc;
use uuid::Uuid;

request_handler!(TransactionResolveRequest => TransactionResolveHandler);

async fn handle(
        message_timestamp: u64,
        req: TransactionResolveRequest,
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, ProtoResponse> {
    let player_uuid = validate_player_context(&ctx)?;
    let transaction_id = match Uuid::parse_str(&req.id) {
        Ok(id) => id,
        Err(_) => {
            return Err(build_error_response(
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
            return Ok(build_error_response(error_code.into(), ""));
        }
        _ => {}
    };
    let inventory = match SYSTEMS
        .inventory_service
        .get_player_inventory(player_uuid)
        .await {
        Ok(x) => x,
        Err(e) => return Err(build_error_response(ErrorCode::UnknownError.into(), &format!("Failed to get player inventory: {}", e))),
    };
    let response = TransactionResolveResponse {
        inventory: inventory.read().await.clone(),
    };
    encode_ok(&response)
}
