use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use proto_gen::PlayerInventoryResponse;
use std::sync::Arc;

#[derive(Default)]
pub struct PlayerInventoryHandler;

#[async_trait]
impl RequestHandler for PlayerInventoryHandler {
    async fn handle(
        &self,
        _message_timestamp: u64,
        payload: &[u8],
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, ProtoResponse> {
        // Ensure player is authenticated
        let player_uuid = validate_player_context(&ctx)?;

        let result = match SYSTEMS.inventory_service.get_player_inventory(player_uuid).await {
            Ok(inventory) => inventory,
            Err(_) => {
                return Ok(build_error_response(
                    halfblind_protobuf_network::ErrorCode::UnknownError.into(),
                    "Inventory does not exist",
                ));
            }
        };
        let player_inventory = result.read().await.clone();
        let response = PlayerInventoryResponse {
            inventory: player_inventory,
        };
        encode_ok(&response)
    }
}
