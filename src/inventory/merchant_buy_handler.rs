use crate::handlers::utils;
use crate::item_definitions::MerchantAvailableItemsComponentLookup;
use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use prost::Message;
use proto_gen::{MerchantBuyItemRequest, MerchantBuyItemResponse};
use protobuf_itemdefinition::ItemsErrorCode;
use std::error::Error;
use std::sync::Arc;

#[derive(Default)]
pub struct MerchantBuyItemHandler;

#[async_trait]
impl RequestHandler for MerchantBuyItemHandler {
    async fn handle(
        &self,
        message_id: u64,
        _message_timestamp: u64,
        payload: &[u8],
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, Box<dyn Error + Send + Sync>> {
        // Decode request
        let req = MerchantBuyItemRequest::decode(payload)?;

        // Ensure player is authenticated
        let (player_uuid, character_uuid) = match utils::validate_character_and_player_uuid(&ctx, SYSTEMS.clone(), message_id, req.character_uuid).await {
            Ok(x) => x,
            Err(response) => return Ok(response),
        };

        match MerchantAvailableItemsComponentLookup.get(&req.merchant_definition_id) {
            None => {
                Ok(build_error_response(
                    message_id,
                    ItemsErrorCode::InvalidItemDefinition.into(),
                    "Merchant does not exist",
                ))
            }
            Some(merchant_component) => {
                if merchant_component.available_items.iter().len() <= req.item_index as usize{
                    Ok(build_error_response(
                        message_id,
                        halfblind_protobuf_network::ErrorCode::InvalidRequest.into(),
                        "Item index is out of bounds",
                    ))
                } else {
                    let transaction = merchant_component.available_items[req.item_index as usize].clone();
                    let result = match SYSTEMS.transaction_service.process_player_transaction(
                        SYSTEMS.inventory_service.clone(),
                        SYSTEMS.database_service.clone(),
                        SYSTEMS.random_service.clone(),
                        player_uuid,
                        character_uuid,
                        &transaction,
                    )
                        .await
                    {
                        Ok(result) => result,
                        Err(error_code) => {
                            return Ok(build_error_response(
                                message_id,
                                error_code.into(),
                                "Merchant buy failed.",
                            ));
                        }
                    };

                    let response = MerchantBuyItemResponse {
                        inventory: result.inventory,
                    };
                    Ok(encode_ok(message_id, response)?)
                }
            }
        }
    }
}