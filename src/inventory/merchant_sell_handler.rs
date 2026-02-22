use crate::handlers::utils;
use crate::item_definitions::DefaultSellValueComponentLookup;
use crate::item_definitions::InventoryHiddenItemComponentLookup;
use crate::item_definitions::IsStackableComponentLookup;
use crate::item_definitions::MerchantAvailableItemsComponentLookup;
use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use halfblind_transactions::process_player_transaction;
use prost::Message;
use proto_gen::{GameErrorCode, MerchantSellItemRequest, MerchantSellItemResponse};
use ::protobuf_itemdefinition::*;

#[derive(Default)]
pub struct MerchantSellItemHandler;

#[async_trait]
impl RequestHandler for MerchantSellItemHandler {
    async fn handle(
        &self,
        message_id: u64,
        _message_timestamp: u64,
        payload: &[u8],
        ctx: std::sync::Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, Box<dyn std::error::Error + Send + Sync>> {
        // Decode request
        let req = match MerchantSellItemRequest::decode(payload) {
            Ok(r) => r,
            Err(_) => {
                return Ok(build_error_response(
                    message_id,
                    halfblind_protobuf_network::ErrorCode::InvalidRequest as i32,
                    "Failed to decode MerchantSellItemRequest",
                ));
            }
        };

        // Ensure player is authenticated
        let (player_uuid, character_uuid) = match utils::validate_character_and_player_uuid(&ctx, SYSTEMS.clone(), message_id, req.character_uuid).await {
            Ok(x) => x,
            Err(response) => return Ok(response),
        };

        let merchant_comp = match MerchantAvailableItemsComponentLookup.get(&req.merchant_definition_id) {
            None => {
                return Ok(build_error_response(
                    message_id,
                    GameErrorCode::MerchantInvalid as i32,
                    "This merchant does not exist or is not available for sale.",
                ));
            }
            Some(merchant_comp) => {merchant_comp}
        };
        let to_sell = match req.item {
            None => {
                return Ok(build_error_response(
                    message_id,
                    ItemsErrorCode::InvalidItemInstance.into(),
                    "Invalid item instance",
                ));
            }
            Some(to_sell) => {to_sell}
        };
        if to_sell.is_equipped {
            return Ok(build_error_response(
                message_id,
                GameErrorCode::UserCantSellItem.into(),
                "Cannot sell equipped items",
            ));
        }
        if let Some(hidden_comp) = InventoryHiddenItemComponentLookup.get(&to_sell.item_definition_id) {
            return Ok(build_error_response(
                message_id,
                GameErrorCode::UserCantSellItem.into(),
                "Cannot sell hidden items like xp or quest items",
            ));
        }

        // TODO: some merchants may buy items for a different value check that first
        let (to_sell, gains) = match DefaultSellValueComponentLookup.get(&to_sell.item_definition_id) {
            None => {
                return Ok(build_error_response(
                    message_id,
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
                    message_id,
                    halfblind_protobuf_network::ErrorCode::UnknownError.into(),
                    "Inventory does not exist",
                ));
            }
        };
        match IsStackableComponentLookup.get(&to_sell.item_definition_id) {
            None => {
                // Non-stackable item check for the instance id
                match inventory_lock.read().await.iter().find(|x| x.item_instance_id == to_sell.item_instance_id) {
                    None => {
                        return Ok(build_error_response(
                            message_id,
                            ItemsErrorCode::NotEnoughItems.into(),
                            "Cannot sell non-stackable items that are already in inventory",
                        ));
                    }
                    Some(item) => {
                        if item.amount < to_sell.amount {
                            return Ok(build_error_response(
                                message_id,
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
                            message_id,
                            ItemsErrorCode::NotEnoughItems.into(),
                            "Cannot sell items that are not already in inventory",
                        ));
                    }
                    Some(item) => {
                        if item.amount < to_sell.amount {
                            return Ok(build_error_response(
                                message_id,
                                ItemsErrorCode::NotEnoughItems.into(),
                                "Cannot sell more items than are present in inventory",
                            ));
                        }
                    }
                }
            }
        }
        let gains_amount = (gains.value.max(0) as u64).saturating_mul(to_sell.amount);
        let sell_transaction = Transaction {
            required: vec![],
            required_negative: vec![],
            consumed: vec![TransactionItem{
                item_instance_id: to_sell.item_instance_id.clone(),
                id_ref: Some(ItemDefinitionRef {
                    id: to_sell.item_definition_id,
                }),
                value: to_sell.amount,
            }],
            rewarded: vec![TransactionReward {
                id_ref: gains.item_id,
                value: gains_amount,
                value_min: 0,
                value_max: 0,
                duration: 0,
            }],
            rewards_random: vec![],
        };
        let transaction_result = match process_player_transaction(
            SYSTEMS.inventory_service.clone(),
            SYSTEMS.database_service.clone(),
            SYSTEMS.random_service.clone(),
            player_uuid,
            character_uuid,
            &sell_transaction,
        ).await {
            Ok(x) => {x}
            Err(e) => {
                return Ok(build_error_response(message_id, e.into(), &"Failed sell transaction".to_string()))
            }
        };
        let result = MerchantSellItemResponse { inventory: transaction_result.inventory };
        encode_ok(message_id, result)
    }
}
