use crate::Services;
use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::handlers::utils;
use crate::inventory::inventory_item_utils::filter_visible_inventory;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use proto_gen::{GameErrorCode, InventoryItem, MapJoinRequest, MapJoinResponse};
use std::sync::Arc;

request_handler!(MapJoinRequest => MapJoinResponse, Services);

async fn handle(
    message_timestamp: u64,
    req: MapJoinRequest,
    ctx: std::sync::Arc<ConnectionContext>,
    systems: Arc<Services>,
) -> Result<MapJoinResponse, ProtoResponse> {
    let (player_uuid, character_uuid) = match utils::validate_character_and_player_uuid(
        &ctx,
        systems.clone(),
        req.character_uuid.clone(),
    )
        .await
    {
        Ok(character_uuid) => character_uuid,
        Err(response) => return Err(response),
    };

    // Check if the character already exists
    {
        match systems
            .characters_service
            .get_character_instance(player_uuid, character_uuid)
            .await
        {
            Ok(_) => {}
            Err(e) => {
                return Err(build_error_response(
                    GameErrorCode::InvalidCharacter.into(),
                    format!("Character ID does not exist in db {}", e).as_ref(),
                ));
            }
        };
    } // Release lock

    let character_inventory = systems
        .inventory_service
        .get_inventory(player_uuid, character_uuid)
        .await;
    let character_inventory = match character_inventory {
        Ok(inventory) => inventory,
        Err(_) => {
            return Err(build_error_response(
                GameErrorCode::InvalidCharacter.into(),
                "Failed to find character inventory",
            ));
        }
    };
    let maps_service = systems.maps_service.clone(); // Arc<RwLock<...>>
    let visible_inventory: Vec<InventoryItem>;
    {
        // Lock read inventory
        let guard = character_inventory.read().await;
        let inventory_slice = guard.as_slice();
        visible_inventory = filter_visible_inventory(systems.item_definition_lookup_service.clone(), inventory_slice)
            .into_iter()
            .cloned()
            .collect();
    } // Unlock read inventory

    let game_map;
    {
        // Acquire write lock only for the duration of map change
        game_map = {
            match maps_service
                .change_player_map(
                    systems.maps_service.clone(),
                    ctx.clone(),
                    player_uuid.clone(),
                    character_uuid.clone(),
                    visible_inventory,
                    req.map_uuid,
                )
                .await
            {
                Ok(map) => map,
                Err(e) => {
                    return Err(build_error_response(
                        GameErrorCode::InvalidMapId.into(),
                        format!("Map was invalid {}", e).as_ref(),
                    ));
                }
            }
        };
    }
    let response = MapJoinResponse {
        map_uuid: game_map.map_id,
        character_uuid: character_uuid.to_string(),
    };
    Ok(response)
}