use crate::handlers::utils;
use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use proto_gen::{GameErrorCode, MerchantSellItemRequest, MerchantSellItemResponse};
use ::protobuf_itemdefinition::*;

#[derive(Default)]
pub struct MerchantSellItemHandler;

#[async_trait]
impl RequestHandler for MerchantSellItemHandler {
    async fn handle(
        &self,
        _message_timestamp: u64,
        payload: &[u8],
        ctx: std::sync::Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, ProtoResponse> {
        // Decode request
        let req = decode_or_error::<MerchantSellItemRequest>(payload)?;

        // Ensure player is authenticated
        let (player_uuid, character_uuid) = utils::validate_character_and_player_uuid(&ctx, SYSTEMS.clone(), req.character_uuid).await?;

        let merchant_comp = match SYSTEMS.item_definition_lookup_service.merchant_available_items_component(&req.merchant_definition_id) {
            None => {
                return Ok(build_error_response(
                    GameErrorCode::MerchantInvalid as i32,
                    "This merchant does not exist or is not available for sale.",
                ));
            }
            Some(merchant_comp) => {merchant_comp}
        };
        let to_sell = match req.item {
            None => {
                return Ok(build_error_response(
                    ItemsErrorCode::InvalidItemInstance.into(),
                    "Invalid item instance",
                ));
            }
            Some(to_sell) => {to_sell}
        };
        if to_sell.is_equipped {
            return Ok(build_error_response(
                GameErrorCode::UserCantSellItem.into(),
                "Cannot sell equipped items",
            ));
        }
        if let Some(hidden_comp) = SYSTEMS.item_definition_lookup_service.inventory_hidden_item_component(&to_sell.item_definition_id) {
            return Ok(build_error_response(
                GameErrorCode::UserCantSellItem.into(),
                "Cannot sell hidden items like xp or quest items",
            ));
        }

        // TODO: some merchants may buy items for a different value check that first
        let (to_sell, gains) = match SYSTEMS.item_definition_lookup_service.default_sell_value_component(&to_sell.item_definition_id) {
            None => {
                return Ok(build_error_response(
                    GameErrorCode::UserCantSellItem.into(),
                    "Cannot sell items without a sell value",
                ));
            }
            Some(gains) => {
                (to_sell, gains)
            }
        };
        let inventory_lock = match SYSTEMS
            .inventory_service
            .get_inventory(player_uuid, character_uuid)
            .await
        {
            Ok(inventory_lock) => inventory_lock,
            Err(_) => {
                return Ok(build_error_response(
                    halfblind_protobuf_network::ErrorCode::UnknownError.into(),
                    "Inventory does not exist",
                ));
            }
        };
        match SYSTEMS.item_definition_lookup_service.is_stackable_component(&to_sell.item_definition_id) {
            None => {
                // Non-stackable item check for the instance id
                match inventory_lock.read().await.iter().find(|x| x.item_instance_id == to_sell.item_instance_id) {
                    None => {
                        return Ok(build_error_response(
                            ItemsErrorCode::NotEnoughItems.into(),
                            "Cannot sell non-stackable items that are already in inventory",
                        ));
                    }
                    Some(item) => {
                        if item.amount < to_sell.amount {
                            return Ok(build_error_response(
                                ItemsErrorCode::NotEnoughItems.into(),
                                "Cannot sell more items than are present in inventory",
                            ));
                        }
                    }
                };
            }
            Some(_) => {
                match inventory_lock.read().await.iter().find(|x| x.item_definition_id == to_sell.item_definition_id) {
                    None => {
                        return Ok(build_error_response(
                            ItemsErrorCode::NotEnoughItems.into(),
                            "Cannot sell items that are not already in inventory",
                        ));
                    }
                    Some(item) => {
                        if item.amount < to_sell.amount {
                            return Ok(build_error_response(
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
        let transaction_result = match SYSTEMS.transaction_service.process_player_transaction(
            SYSTEMS.inventory_service.clone(),
            SYSTEMS.database_service.clone(),
            SYSTEMS.random_service.clone(),
            player_uuid,
            character_uuid,
            None,
            None,
            Some(consumed),
            Some(rewarded),
            None,
        ).await {
            Ok(x) => {x}
            Err(e) => {
                return Ok(build_error_response(e.into(), &"Failed sell transaction".to_string()))
            }
        };
        let result = MerchantSellItemResponse { inventory: transaction_result.inventory };
        encode_ok(&result)
    }
}
