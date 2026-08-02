use crate::db::db::sqlx_error_to_proto_error;
use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::inventory::inventory_item_utils::try_aggregate_inventories;
use crate::services::services::Services;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use proto_gen::{CheatAddInventoryItemRequest, CheatAddInventoryItemResponse};
use std::sync::Arc;
use uuid::Uuid;

request_handler!(CheatAddInventoryItemRequest => CheatAddInventoryItemHandler, Services);

async fn handle(
    _message_timestamp: u64,
    req: CheatAddInventoryItemRequest,
    _ctx: Arc<ConnectionContext>,
    systems: Arc<Services>,
) -> Result<ProtoResponse, ProtoResponse> {
    #[cfg(feature = "cheats")]
    {
        let inventory_service = systems.inventory_service.clone();
        let player_uuid = match Uuid::parse_str(&req.player_uuid) {
            Ok(x) => x,
            Err(_) => return Err(build_error_response(halfblind_protobuf_network::ErrorCode::AuthenticationFailed.into(), "Invalid player UUID")),
        };
        let character_uuid = match Uuid::parse_str(&req.character_uuid) {
            Ok(x) => x,
            Err(_) => return Err(build_error_response(halfblind_protobuf_network::ErrorCode::AuthenticationFailed.into(), "Invalid character UUID")),
        };
        let character_inventory_arc = inventory_service.get_inventory(player_uuid, character_uuid).await.map_err(sqlx_error_to_proto_error)?;
        let mut character_inventory_rw = character_inventory_arc.write().await;
        let leftovers = try_aggregate_inventories(
            systems.item_definition_lookup_service.clone(),
            &req.item_def,
            &mut character_inventory_rw);
        let mut db_connection = systems.database_service.get_db_pool().begin().await.map_err(sqlx_error_to_proto_error)?;
        inventory_service.save_inventory_to_db(player_uuid, character_uuid, &mut db_connection).await.map_err(sqlx_error_to_proto_error)?;
        db_connection.commit().await.map_err(sqlx_error_to_proto_error)?;
        let response = CheatAddInventoryItemResponse {
            player_uuid: req.player_uuid,
            character_uuid: req.character_uuid,
            inventory: character_inventory_rw.clone(),
        };
        return encode_ok(&response)
    }
    Err(build_error_response(halfblind_protobuf_network::ErrorCode::UnknownError.into(), &"No cheats in production. Please enable the \"cheats\" feature.".to_string()))
}