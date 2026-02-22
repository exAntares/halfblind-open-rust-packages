use crate::inventory::inventory_item_utils::filter_visible_inventory;
use crate::item_definitions::{SkillComponentLookup, TransactionComponentLookup};
use crate::map::models::MapAction::{AddStatsToCharacter, MoveTo, PickupItem, SpawnSkill};
use crate::map::models::MapActionTimed;
use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::{ErrorCode, ProtoResponse};
use halfblind_transactions::process_player_transaction;
use prost::Message;
use proto_gen::map_action_request::MapAction;
use proto_gen::CharacterStat;
use proto_gen::{GameErrorCode, MapActionRequest, MapActionResponse};
use protobuf_itemdefinition::{InventoryItem, ItemsErrorCode};
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Default)]
pub struct MapActionHandler {}

#[async_trait]
impl RequestHandler for MapActionHandler {
    async fn handle(
        &self,
        message_id: u64,
        message_timestamp: u64,
        payload: &[u8],
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, Box<dyn Error + Send + Sync>> {
        let player_uuid = match validate_player_context(&ctx, message_id) {
            Ok(result) => result,
            Err(response) => return Ok(response),
        };
        let req = MapActionRequest::decode(payload)?;
        let character_uuid_str = req.character_uuid;
        let character_uuid = match Uuid::parse_str(&character_uuid_str) {
            Ok(c) => c,
            Err(_) => {
                return Ok(build_error_response(
                    message_id,
                    GameErrorCode::InvalidCharacter.into(),
                    "Invalid character UUID",
                ));
            }
        };

        // Get the player's current map
        let game_map= match SYSTEMS.maps_service.get_player_map(&player_uuid) {
            None => {
                return Ok(build_error_response(
                    message_id,
                    GameErrorCode::PlayerIsNotInAnyMap.into(),
                    "Player is not on any map!",
                ));
            }
            Some(game_map) => game_map,
        };

        // Check if the character is owned by the player requesting the action.
        match game_map.player_by_character.get(&character_uuid) {
            None => {
                return Ok(build_error_response(
                    message_id,
                    GameErrorCode::InvalidCharacter.into(),
                    "There is no player for requested character on this map.",
                ));
            }
            Some(player_by_character) => {
                if player_by_character.value().clone() != player_uuid.clone() {
                    return Ok(build_error_response(
                        message_id,
                        GameErrorCode::InvalidCharacter.into(),
                        "Player is requesting action for another player!",
                    ));
                }
            }
        }

        let action = match req.map_action {
            None => {
                return Ok(build_error_response(
                    message_id,
                    halfblind_protobuf_network::ErrorCode::UnknownError.into(),
                    "Invalid MapAction",
                ));
            }
            Some(x) => x,
        };
        match action {
            MapAction::MoveTo(move_to_req) => {
                game_map.push_action(MapActionTimed {
                    timestamp: message_timestamp,
                    action: MoveTo {
                        entity_uuid: character_uuid,
                        target_positions: move_to_req.target_positions,
                    },
                });
                let response = MapActionResponse {};
                Ok(encode_ok(message_id, response)?)
            }
            MapAction::UsableSkill(skill_request) => {
                if let Some(skill_comp) =
                    SkillComponentLookup.get(&skill_request.skill_definition_id)
                {
                    let character_inventory_guard = match SYSTEMS.inventory_service.get_inventory(player_uuid, character_uuid).await {
                        Ok(x) => {x}
                        Err(e) => {
                            return Ok(build_error_response(message_id, halfblind_protobuf_network::ErrorCode::UnknownError.into(), format!("Failed to get character inventory: {}", e).as_str()));
                        }
                    };
                    let character_inventory = character_inventory_guard.read().await;
                    let mut found = false;
                    for item in character_inventory.iter() {
                        if item.item_definition_id == skill_request.skill_definition_id {
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Ok(build_error_response(
                            message_id,
                            GameErrorCode::SkillNotOwned.into(),
                            &format!("Character does not have such Skill {}", skill_request.skill_definition_id)));
                    }
                    game_map.push_action(MapActionTimed {
                        timestamp: message_timestamp,
                        action: SpawnSkill {
                            character_owner_uuid: character_uuid,
                            skill_definition_id: skill_request.skill_definition_id,
                            skill_component: skill_comp.clone(),
                            target_position: skill_request.target_pos.unwrap_or_default(),
                            direction: skill_request.target_direction.unwrap_or_default(),
                        },
                    });
                    let response = MapActionResponse {};
                    Ok(encode_ok(message_id, response)?)
                } else {
                    Ok(build_error_response(
                        message_id,
                        ItemsErrorCode::InvalidItemDefinition.into(),
                        "Skill does not exist!",
                    ))
                }
            }
            MapAction::PickUpItem(req) => {
                let picked_items_uuid = req
                    .item_instance_id
                    .iter()
                    .filter_map(|x| Uuid::parse_str(x.as_ref()).ok())
                    .collect();
                match SYSTEMS.inventory_service.get_inventory(player_uuid, character_uuid).await {
                    Ok(character_inventory_guard) => {
                        let character_inventory_items = character_inventory_guard.read().await;
                        // We always let players attempt to pick up items, but they may be silently rejected
                        game_map.push_action(MapActionTimed {
                            timestamp: message_timestamp,
                            action: PickupItem {
                                picked_items_uuid,
                                character_uuid,
                                current_character_inventory_readonly: character_inventory_items.clone(),
                            },
                        });
                        let response = MapActionResponse {};
                        Ok(encode_ok(message_id, response)?)
                    }
                    Err(_) => {
                        Ok(build_error_response(message_id, ErrorCode::UnknownError.into(), "Failed to get character inventory"))
                    }
                }
            }
            MapAction::UseTeleport(req) => {
                let index = req.teleport_index as usize;
                if game_map.map_component.teleporter.len() <= index {
                    return Ok(build_error_response(
                        message_id,
                        ItemsErrorCode::InvalidItemDefinition.into(),
                        "Index does not exist on teleporters",
                    ));
                }
                let teleport = game_map.map_component.teleporter[index];
                let transaction = match TransactionComponentLookup.get(&teleport.transaction_id) {
                    None => {
                        return Ok(build_error_response(
                            message_id,
                            ItemsErrorCode::TransactionInvalid.into(),
                            "Transaction does not exist",
                        ));
                    }
                    Some(x) => match &x.transaction {
                        None => {
                            return Ok(build_error_response(
                                message_id,
                                ItemsErrorCode::TransactionInvalid.into(),
                                "Transaction does not exist",
                            ));
                        }
                        Some(x) => x,
                    },
                };
                match process_player_transaction(
                    SYSTEMS.inventory_service.clone(),
                    SYSTEMS.database_service.clone(),
                    SYSTEMS.random_service.clone(),
                    player_uuid,
                    character_uuid,
                    &transaction,
                ).await {
                    Ok(_) => {}
                    Err(e) => {
                        return Ok(build_error_response(message_id, e.into(), &"Failed transaction".to_string()))
                    }
                };

                let inventory_lock = SYSTEMS
                    .inventory_service
                    .get_inventory(player_uuid, character_uuid)
                    .await?;

                let visible_inventory: Vec<InventoryItem>;
                {
                    // inventory read lock
                    let inventory = inventory_lock.read().await;
                    visible_inventory = filter_visible_inventory(inventory.as_slice())
                        .into_iter()
                        .cloned()
                        .collect();
                } // inventory read lock release

                let map_id = teleport.connected_map_id;
                match SYSTEMS.maps_service
                    .change_player_map(
                        ctx.clone(),
                        player_uuid,
                        character_uuid,
                        visible_inventory,
                        map_id,
                    )
                    .await
                {
                    Ok(_) => {
                        let response = MapActionResponse {};
                        Ok(encode_ok(message_id, response)?)
                    }
                    Err(e) => Ok(build_error_response(
                        message_id,
                        ErrorCode::UnknownError.into(),
                        format!("Failed to change player to a new map {}", e).as_str(),
                    )),
                }
            }
            MapAction::UseAbilityPoint(req) => {
                let stat = match req.stat {
                    0 => CharacterStat::Agi,
                    1 => CharacterStat::Int,
                    2 => CharacterStat::Str,
                    _ => {
                        return Ok(build_error_response(
                            message_id,
                            GameErrorCode::InvalidCharacterStat.into(),
                            "Invalid stat type",
                        ));
                    }
                };
                game_map.push_action(MapActionTimed {
                    timestamp: message_timestamp,
                    action: AddStatsToCharacter {
                        character_uuid,
                        stats: (stat, req.amount),
                    },
                });
                let response = MapActionResponse {};
                Ok(encode_ok(message_id, response)?)
            }
        }
    }
}
