use crate::db::db::sqlx_error_to_proto_error;
use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::item_definitions::ItemDefinitionLookupServiceImpl;
use crate::services::services::Services;
use crate::transactions::postgress_delayed_rewards_inserter::PostgresDelayedRewardsInserter;
use halfblind_network::*;
use halfblind_protobuf_network::{ErrorCode, ProtoResponse};
use proto_gen::{TransactionRequest, TransactionResponse};
use ::protobuf_itemdefinition::*;
use std::sync::Arc;
use uuid::Uuid;

request_handler!(TransactionRequest => TransactionResponse, Services);

async fn handle(
    _message_timestamp: u64,
    req: TransactionRequest,
    ctx: Arc<ConnectionContext>,
    systems: Arc<Services>,
    ) -> Result<TransactionResponse, ProtoResponse> {
    let player_uuid = validate_player_context(&ctx)?;
    if let Err(error_code) = get_transaction_definition(systems.item_definition_lookup_service.clone(), req.transaction_id).await {
        return Err(build_error_response(
            ItemsErrorCode::TransactionInvalid.into(),
                      "Transaction definition not found.",
                  ));
    };

    let secondary_key_uuid = match Uuid::parse_str(&req.inventory_source_uuid) {
        Ok(x) => x,
        Err(e) => return Err(build_error_response(ErrorCode::UnknownError.into(), &format!("failed to parse inventory_source_uuid: {}", e))),
    };
    let inventory_arc = match systems.inventory_service.get_inventory(player_uuid, secondary_key_uuid).await {
        Ok(inventory) => inventory,
        Err(e) => {
            eprintln!("error trying to get items from player {}", e);
            return Err(build_error_response(ErrorCode::UnknownError.into(), &format!("failed to get inventory: {}", e)));
        }
    };
    let mut inventory_rwlock = inventory_arc.write().await;
    let mut db_transaction = systems.database_service.clone()
        .get_db_pool()
        .begin()
        .await.map_err(sqlx_error_to_proto_error)?;
    // Process the transaction
    let result = {
        let mut delayed_items_inserter = PostgresDelayedRewardsInserter { connection: &mut db_transaction };
        match systems.transaction_service.process_inventory_transaction_id(
            systems.random_service.clone(),
            &mut delayed_items_inserter,
            &mut inventory_rwlock,
            player_uuid,
            req.transaction_id,
        )
            .await
        {
            Ok(result) => result,
            Err(error_code) => {
                return Err(build_error_response(
                    error_code.into(),
                    "Transaction failed.",
                ));
            }
        }
    };
    db_transaction.commit().await.map_err(sqlx_error_to_proto_error)?;
    let response = TransactionResponse {
        transaction_instance_id: result.delayed_items,
        inventory: result.inventory,
        rewarded: result.rewarded,
    };
    Ok(response)
}

pub async fn get_transaction_definition(
    item_definition_lookup_service: Arc<ItemDefinitionLookupServiceImpl>,
    transaction_id: u64,
) -> Result<Arc<TransactionComponent>, ItemsErrorCode> {
    let transaction_component = match item_definition_lookup_service.transaction_component(&transaction_id) {
        None => return Err(ItemsErrorCode::InvalidItemDefinition),
        Some(transaction_component) => transaction_component,
    };
    Ok(transaction_component.clone())
}
