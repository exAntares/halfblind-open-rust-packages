use crate::item_definitions::TransactionComponentLookup;
use crate::systems::systems::SYSTEMS;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use halfblind_transactions::process_player_transaction;
use ::protobuf_itemdefinition::*;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

request_handler!(TransactionRequest => TransactionHandler);

async fn handle(
        message_id: u64,
        _message_timestamp: u64,
        req: TransactionRequest,
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, Box<dyn Error + Send + Sync>> {
    let player_uuid = match validate_player_context(&ctx, message_id) {
        Ok(result) => result,
        Err(response) => return Ok(response),
    };
    let transaction = match get_transaction_definition(req.transaction_id).await {
        Ok(result) => match result.transaction {
            None => {
                return Ok(build_error_response(
                    message_id,
                    ItemsErrorCode::TransactionInvalid.into(),
                    &format!("Transaction definition not found for id {}.", req.transaction_id),
                ));
            }
            Some(x) => x,
        },
        Err(error_code) => {
            return Ok(build_error_response(
                message_id,
                error_code.into(),
                "Transaction definition not found.",
            ));
        }
    };

    let secondary_key_uuid = Uuid::parse_str(&req.inventory_source_uuid)?;

    // Process the transaction
    let result = match process_player_transaction(
        SYSTEMS.inventory_service.clone(),
        SYSTEMS.database_service.clone(),
        SYSTEMS.random_service.clone(),
        player_uuid,
        secondary_key_uuid,
        &transaction,
    )
        .await
    {
        Ok(result) => result,
        Err(error_code) => {
            return Ok(build_error_response(
                message_id,
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

    Ok(encode_ok(message_id, response)?)
}

pub async fn get_transaction_definition(
    transaction_id: u64,
) -> Result<TransactionComponent, ItemsErrorCode> {
    let transaction_component = match TransactionComponentLookup.get(&transaction_id) {
        None => return Err(ItemsErrorCode::InvalidItemDefinition),
        Some(transaction_component) => transaction_component,
    };
    Ok(transaction_component.clone())
}
