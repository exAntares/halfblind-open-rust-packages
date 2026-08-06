use crate::db::db::sqlx_error_to_proto_error;
use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::handlers::utils;
use crate::services::services::Services;
use crate::transactions::postgress_delayed_rewards_inserter::PostgresDelayedRewardsInserter;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use proto_gen::{MerchantBuyItemRequest, MerchantBuyItemResponse};
use protobuf_itemdefinition::ItemsErrorCode;
use std::sync::Arc;

request_handler!(MerchantBuyItemRequest => MerchantBuyItemResponse, Services);

async fn handle(
    _message_timestamp: u64,
    req: MerchantBuyItemRequest,
    ctx: Arc<ConnectionContext>,
    systems: Arc<Services>,
) -> Result<MerchantBuyItemResponse, ProtoResponse> {
    // Ensure player is authenticated
    let (player_uuid, character_uuid) = match utils::validate_character_and_player_uuid(&ctx, systems.clone(), req.character_uuid).await {
        Ok(x) => x,
        Err(response) => return Err(response),
    };

    match systems.item_definition_lookup_service.merchant_available_items_component(&req.merchant_definition_id) {
        None => {
            Err(build_error_response(
                ItemsErrorCode::InvalidItemDefinition.into(),
                "Merchant does not exist",
            ))
        }
        Some(merchant_component) => {
            if merchant_component.available_transactions.iter().len() <= req.item_index as usize {
                Err(build_error_response(
                    halfblind_protobuf_network::ErrorCode::InvalidRequest.into(),
                    "Item index is out of bounds",
                ))
            } else {
                let transaction = merchant_component.available_transactions[req.item_index as usize].clone();
                let inventory_arc = match systems.inventory_service.get_inventory(player_uuid, character_uuid).await {
                    Ok(inventory_lock) => inventory_lock,
                    Err(e) => return Err(build_error_response(halfblind_protobuf_network::ErrorCode::UnknownError.into(), "Inventory does not exist")),
                };
                let mut inventory_rw_lock = inventory_arc.write().await;
                let mut db_transaction = systems.database_service.clone()
                    .get_db_pool()
                    .begin()
                    .await.map_err(sqlx_error_to_proto_error)?;
                let result = {
                    let mut delayed_items_inserter = PostgresDelayedRewardsInserter { connection: &mut db_transaction };
                    match systems.transaction_service.process_inventory_transaction_id(
                        systems.random_service.clone(),
                        &mut delayed_items_inserter,
                        &mut inventory_rw_lock,
                        player_uuid,
                        transaction.id,
                    )
                        .await
                    {
                        Ok(result) => result,
                        Err(error_code) => {
                            return Err(build_error_response(
                                error_code.into(),
                                "Merchant buy failed.",
                            ));
                        }
                    }
                };
                db_transaction.commit().await.map_err(sqlx_error_to_proto_error)?;
                let response = MerchantBuyItemResponse {
                    inventory: result.inventory,
                };
                Ok(response)
            }
        }
    }
}