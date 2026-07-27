use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::services::services::Services;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use proto_gen::{PlayerInventoryRequest, PlayerInventoryResponse};
use std::sync::Arc;

request_handler!(PlayerInventoryRequest => PlayerInventoryRequestHandler, Services);

async fn handle(
    _message_timestamp: u64,
    _: PlayerInventoryRequest,
    ctx: Arc<ConnectionContext>,
    systems: Arc<Services>
) -> Result<ProtoResponse, ProtoResponse> {
    // Ensure player is authenticated
    let player_uuid = validate_player_context(&ctx)?;

    let result = match systems.inventory_service.get_player_inventory(player_uuid).await {
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
