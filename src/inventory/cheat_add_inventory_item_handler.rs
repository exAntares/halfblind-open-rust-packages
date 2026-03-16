use crate::systems::systems::SYSTEMS;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use proto_gen::{CheatAddInventoryItemRequest, CheatAddInventoryItemResponse};
use std::sync::Arc;
use uuid::Uuid;

request_handler!(CheatAddInventoryItemRequest => CheatAddInventoryItemHandler);

async fn handle(
    _message_timestamp: u64,
    req: CheatAddInventoryItemRequest,
    _ctx: Arc<ConnectionContext>,
) -> Result<ProtoResponse, ProtoResponse> {
    #[cfg(feature = "cheats")]
    {
        let inventory_service = SYSTEMS.inventory_service.clone();
        let player_uuid = match Uuid::parse_str(&req.player_uuid) {
            Ok(x) => x,
            Err(_) => return Err(build_error_response(halfblind_protobuf_network::ErrorCode::AuthenticationFailed.into(), "Invalid player UUID")),
        };
        let character_uuid = match Uuid::parse_str(&req.character_uuid) {
            Ok(x) => x,
            Err(_) => return Err(build_error_response(halfblind_protobuf_network::ErrorCode::AuthenticationFailed.into(), "Invalid character UUID")),
        };
        let inventory = match inventory_service.get_inventory(player_uuid, character_uuid).await {
            Ok(x) => x,
            Err(_) => return Err(build_error_response(halfblind_protobuf_network::ErrorCode::UnknownError.into(), "Failed to get inventory")),
        };

        match inventory_service.aggregate_inventories(player_uuid, character_uuid, req.item_def).await {
            Ok(overflow_items) => {
                let response = CheatAddInventoryItemResponse {
                    player_uuid: req.player_uuid,
                    character_uuid: req.character_uuid,
                    inventory: inventory.read().await.clone(),
                };
                return encode_ok(&response)
            }
            Err(e) => {
                return Err(build_error_response(halfblind_protobuf_network::ErrorCode::UnknownError.into(), &format!("Failed to add item to inventory: {}", e)));
            }
        }
    }
    Err(build_error_response(halfblind_protobuf_network::ErrorCode::UnknownError.into(), &"No cheats in production. Please enable the \"cheats\" feature.".to_string()))
}