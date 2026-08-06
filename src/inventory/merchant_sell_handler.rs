use crate::db::db::sqlx_error_to_proto_error;
use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::handlers::utils;
use crate::services::services::Services;
use crate::transactions::postgress_delayed_rewards_inserter::PostgresDelayedRewardsInserter;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use proto_gen::{GameErrorCode, MerchantSellItemRequest, MerchantSellItemResponse};
use ::protobuf_itemdefinition::*;
use std::sync::Arc;

request_handler!(MerchantSellItemRequest => MerchantSellItemResponse, Services);

async fn handle(
    _message_timestamp: u64,
    req: MerchantSellItemRequest,
    ctx: std::sync::Arc<ConnectionContext>,
    systems: Arc<Services>,
) -> Result<MerchantSellItemResponse, ProtoResponse> {
    // Ensure player is authenticated
    let (player_uuid, character_uuid) = utils::validate_character_and_player_uuid(&ctx, systems.clone(), req.character_uuid).await?;

    let merchant_comp = match systems.item_definition_lookup_service.merchant_available_items_component(&req.merchant_definition_id) {
        None => {
            return Err(build_error_response(
                GameErrorCode::MerchantInvalid as i32,
                "This merchant does not exist or is not available for sale.",
            ));
        }
        Some(merchant_comp) => {merchant_comp}
    };
    let to_sell = match req.item {
        None => {
            return Err(build_error_response(
                ItemsErrorCode::InvalidItemInstance.into(),
                "Invalid item instance",
            ));
        }
        Some(to_sell) => {to_sell}
    };
    if to_sell.is_equipped {
        return Err(build_error_response(
            GameErrorCode::UserCantSellItem.into(),
            "Cannot sell equipped items",
        ));
    }
    if let Some(hidden_comp) = systems.item_definition_lookup_service.inventory_hidden_item_component(&to_sell.item_definition_id) {
        return Err(build_error_response(
            GameErrorCode::UserCantSellItem.into(),
            "Cannot sell hidden items like xp or quest items",
        ));
    }

    // TODO: some merchants may buy items for a different value check that first
    let (to_sell, gains) = match systems.item_definition_lookup_service.default_sell_value_component(&to_sell.item_definition_id) {
        None => {
            return Err(build_error_response(
                GameErrorCode::UserCantSellItem.into(),
                "Cannot sell items without a sell value",
            ));
        }
        Some(gains) => {
            (to_sell, gains)
        }
    };
    let inventory_arc = match systems
        .inventory_service
        .get_inventory(player_uuid, character_uuid)
        .await
    {
        Ok(inventory_lock) => inventory_lock,
        Err(_) => {
            return Err(build_error_response(
                halfblind_protobuf_network::ErrorCode::UnknownError.into(),
                "Inventory does not exist",
            ));
        }
    };
    let mut inventory_rw_lock = inventory_arc.write().await;
    match systems.item_definition_lookup_service.is_stackable_component(&to_sell.item_definition_id) {
        None => {
            // Non-stackable item check for the instance id
            match inventory_rw_lock.iter().find(|x| x.item_instance_id == to_sell.item_instance_id) {
                None => return Err(build_error_response(ItemsErrorCode::NotEnoughItems.into(), "Cannot sell non-stackable items that are already in inventory")),
                Some(item) => {
                    if item.amount < to_sell.amount {
                        return Err(build_error_response(
                            ItemsErrorCode::NotEnoughItems.into(),
                            "Cannot sell more items than are present in inventory",
                        ));
                    }
                }
            };
        }
        Some(_) => {
            match inventory_rw_lock.iter().find(|x| x.item_definition_id == to_sell.item_definition_id) {
                None => return Err(build_error_response(ItemsErrorCode::NotEnoughItems.into(), "Cannot sell items that are not already in inventory")),
                Some(item) => {
                    if item.amount < to_sell.amount {
                        return Err(build_error_response(
                            ItemsErrorCode::NotEnoughItems.into(),
                            "Cannot sell more items than are present in inventory",
                        ));
                    }
                }
            }
        }
    }
    let gains_amount = (gains.value.max(0) as u64).saturating_mul(to_sell.amount);
    let consumed = vec![TransactionItem{
        item_instance_id: to_sell.item_instance_id.clone(),
        id_ref: Some(ItemDefinitionRef {
            id: to_sell.item_definition_id,
        }),
        value: to_sell.amount,
    }];
    let rewarded = vec![TransactionReward {
        id_ref: gains.item_id,
        value: gains_amount,
        value_min: 0,
        value_max: 0,
        duration: 0,
    }];
    let mut db_transaction = systems.database_service.clone()
        .get_db_pool()
        .begin()
        .await.map_err(sqlx_error_to_proto_error)?;
    let transaction_result = {
        let mut delayed_items_inserter = PostgresDelayedRewardsInserter { connection: &mut db_transaction };
        systems.transaction_service.process_inventory_transaction(
            systems.random_service.clone(),
            &mut delayed_items_inserter,
            &mut inventory_rw_lock,
            player_uuid,
            None,
            None,
            Some(consumed),
            Some(rewarded),
            None,
        ).await.map_err(|e|build_error_response(e.into(), &"Failed sell transaction".to_string()))?
    };
    db_transaction.commit().await.map_err(sqlx_error_to_proto_error)?;
    let result = MerchantSellItemResponse { inventory: transaction_result.inventory };
    Ok(result)
}