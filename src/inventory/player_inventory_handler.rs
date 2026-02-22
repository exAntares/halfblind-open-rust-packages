use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use protobuf_itemdefinition::PlayerInventoryResponse;
use halfblind_protobuf_network::ProtoResponse;
use std::error::Error;
use std::sync::Arc;

#[derive(Default)]
pub struct PlayerInventoryHandler;

#[async_trait]
impl RequestHandler for PlayerInventoryHandler {
    async fn handle(
        &self,
        message_id: u64,
        _message_timestamp: u64,
        payload: &[u8],
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, Box<dyn Error + Send + Sync>> {
        // Ensure player is authenticated
        let player_uuid = match validate_player_context(&ctx, message_id) {
            Ok(player_uuid) => player_uuid,
            Err(response) => return Ok(response),
        };

        let result = match SYSTEMS.inventory_service.get_player_inventory(player_uuid).await {
            Ok(inventory) => inventory,
            Err(_) => {
                return Ok(build_error_response(
                    message_id,
                    halfblind_protobuf_network::ErrorCode::UnknownError.into(),
                    "Inventory does not exist",
                ));
            }
        };
        let player_inventory = result.read().await.clone();
        let response = PlayerInventoryResponse {
            inventory: player_inventory,
        };
        encode_ok(message_id, response)
    }
}
