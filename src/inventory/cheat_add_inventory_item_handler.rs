use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use protobuf_itemdefinition::{CheatAddInventoryItemRequest, CheatAddInventoryItemResponse};
use halfblind_protobuf_network::ProtoResponse;
use prost::Message;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Default)]
pub struct CheatAddInventoryItemHandler;

#[async_trait]
impl RequestHandler for CheatAddInventoryItemHandler {
    async fn handle(
        &self,
        message_id: u64,
        _message_timestamp: u64,
        payload: &[u8],
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, Box<dyn Error + Send + Sync>> {
        #[cfg(feature = "cheats")]
        {
            let req = CheatAddInventoryItemRequest::decode(payload)?;
            let inventory_service = SYSTEMS.inventory_service.clone();
            let player_uuid = Uuid::parse_str(&req.player_uuid)?;
            let character_uuid = Uuid::parse_str(&req.character_uuid)?;
            match inventory_service.aggregate_inventories(player_uuid, character_uuid, req.item_def).await {
                Ok(overflow_items) => {
                    let response = CheatAddInventoryItemResponse {
                        player_uuid: req.player_uuid,
                        character_uuid: req.character_uuid,
                        inventory: inventory_service.get_inventory(player_uuid, character_uuid).await?.read().await.clone()
                    };
                    return Ok(encode_ok(message_id, response)?)
                }
                Err(e) => {
                    return Ok(build_error_response(message_id, halfblind_protobuf_network::ErrorCode::UnknownError.into(), &format!("Failed to add item to inventory: {}", e)));
                }
            }
        }
        Ok(build_error_response(message_id, halfblind_protobuf_network::ErrorCode::UnknownError.into(), &"No cheats in production. Please enable the \"cheats\" feature.".to_string()))
    }
}