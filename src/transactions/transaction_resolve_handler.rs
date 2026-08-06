use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::services::services::Services;
use halfblind_network::*;
use halfblind_protobuf_network::*;
use halfblind_transactions::resolve_expired_transaction;
use proto_gen::{TransactionResolveRequest, TransactionResolveResponse};
use std::sync::Arc;
use uuid::Uuid;

request_handler!(TransactionResolveRequest => TransactionResolveResponse, Services);

async fn handle(
    message_timestamp: u64,
    req: TransactionResolveRequest,
    ctx: Arc<ConnectionContext>,
    systems: Arc<Services>,
    ) -> Result<TransactionResolveResponse, ProtoResponse> {
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

    let db_pool = systems.database_service.get_db_pool();
    let result = resolve_expired_transaction(
        &transaction_id,
        player_uuid,
        db_pool.as_ref(),
    )
        .await;
    match result {
        Err((error_code, _)) => {
            return Err(build_error_response(error_code.into(), ""));
        }
        _ => {}
    };
    let inventory = match systems
        .inventory_service
        .get_player_inventory(player_uuid)
        .await {
        Ok(x) => x,
        Err(e) => return Err(build_error_response(ErrorCode::UnknownError.into(), &format!("Failed to get player inventory: {}", e))),
    };
    let response = TransactionResolveResponse {
        inventory: inventory.read().await.clone(),
    };
    Ok(response)
}
