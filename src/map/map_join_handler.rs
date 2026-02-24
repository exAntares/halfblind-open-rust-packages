use crate::handlers::utils;
use crate::inventory::inventory_item_utils::filter_visible_inventory;
use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use prost::Message;
use proto_gen::{GameErrorCode, InventoryItem, MapJoinRequest, MapJoinResponse};

#[derive(Default)]
pub struct MapJoinHandler;

#[async_trait]
impl RequestHandler for MapJoinHandler {
    async fn handle(
        &self,
        message_id: u64,
        message_timestamp: u64,
        payload: &[u8],
        ctx: std::sync::Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, Box<dyn std::error::Error + Send + Sync>> {
        let req = MapJoinRequest::decode(payload)?;
        let (player_uuid, character_uuid) = match utils::validate_character_and_player_uuid(
            &ctx,
            SYSTEMS.clone(),
            message_id,
            req.character_uuid.clone(),
        )
            .await
        {
            Ok(character_uuid) => character_uuid,
            Err(response) => return Ok(response),
        };

        // Check if the character already exists
        {
            match SYSTEMS
                .characters_service
                .get_character_instance(player_uuid, character_uuid)
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    return Ok(build_error_response(
                        message_id,
                        GameErrorCode::InvalidCharacter.into(),
                        format!("Character ID does not exist in db {}", e).as_ref(),
                    ));
                }
            };
        } // Release lock

        let character_inventory = SYSTEMS
            .inventory_service
            .get_inventory(player_uuid, character_uuid)
            .await;
        let character_inventory = match character_inventory {
            Ok(inventory) => inventory,
            Err(_) => {
                return Ok(build_error_response(
                    message_id,
                    GameErrorCode::InvalidCharacter.into(),
                    "Failed to find character inventory",
                ));
            }
        };
        let maps_service = SYSTEMS.maps_service.clone(); // Arc<RwLock<...>>
        let visible_inventory: Vec<InventoryItem>;
        {
            // Lock read inventory
            let guard = character_inventory.read().await;
            let inventory_slice = guard.as_slice();
            visible_inventory = filter_visible_inventory(inventory_slice)
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
                        return Ok(build_error_response(
                            message_id,
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
        Ok(encode_ok(message_id, response)?)
    }
}
