use crate::systems::systems::SYSTEMS;
use halfblind_network::*;
use halfblind_protobuf_network::{ErrorCode, ProtoResponse};
use proto_gen::{TransactionRequest, TransactionResponse};
use ::protobuf_itemdefinition::*;
use std::sync::Arc;
use uuid::Uuid;

request_handler!(TransactionRequest => TransactionHandler);

async fn handle(
        _message_timestamp: u64,
        req: TransactionRequest,
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, ProtoResponse> {
    let player_uuid = validate_player_context(&ctx)?;
    if let Err(error_code) = get_transaction_definition(req.transaction_id).await {
        return Ok(build_error_response(
            ItemsErrorCode::TransactionInvalid.into(),
                      "Transaction definition not found.",
                  ));
    };

    let secondary_key_uuid = match Uuid::parse_str(&req.inventory_source_uuid) {
        Ok(x) => x,
        Err(e) => return Ok(build_error_response(ErrorCode::UnknownError.into(), &format!("failed to parse inventory_source_uuid: {}", e))),
    };
    // Process the transaction
    let result = match SYSTEMS.transaction_service.process_player_transaction_id(
        SYSTEMS.inventory_service.clone(),
        SYSTEMS.database_service.clone(),
        SYSTEMS.random_service.clone(),
        player_uuid,
        secondary_key_uuid,
        req.transaction_id,
    )
        .await
    {
        Ok(result) => result,
        Err(error_code) => {
            return Ok(build_error_response(
                error_code.into(),
                "Transaction failed.",
            ));
        }
    };

    let response = TransactionResponse {
        transaction_instance_id: result.transaction_instance_id,
        inventory: result.inventory,
        rewarded: result.rewarded,
    };

    encode_ok(&response)
}

pub async fn get_transaction_definition(
    transaction_id: u64,
) -> Result<Arc<TransactionComponent>, ItemsErrorCode> {
    let transaction_component = match SYSTEMS.item_definition_lookup_service.transaction_component(&transaction_id) {
        None => return Err(ItemsErrorCode::InvalidItemDefinition),
        Some(transaction_component) => transaction_component,
    };
    Ok(transaction_component.clone())
}
