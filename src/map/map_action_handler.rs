use crate::db::db::sqlx_error_to_proto_error;
use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::inventory::inventory_item_utils::filter_visible_inventory;
use crate::map::models::MapAction::{AddStatsToCharacter, MoveTo, PickupItem, SpawnSkill};
use crate::map::models::MapActionTimed;
use crate::services::services::Services;
use crate::transactions::postgress_delayed_rewards_inserter::PostgresDelayedRewardsInserter;
use halfblind_network::*;
use halfblind_protobuf_network::{ErrorCode, ProtoResponse};
use proto_gen::map_action_request::MapAction;
use proto_gen::{CharacterStat, InventoryItem};
use proto_gen::{GameErrorCode, MapActionRequest, MapActionResponse};
use protobuf_itemdefinition::ItemsErrorCode;
use std::sync::Arc;
use uuid::Uuid;

request_handler!(MapActionRequest => MapActionResponse, Services);

async fn handle(
    message_timestamp: u64,
    req: MapActionRequest,
    ctx: Arc<ConnectionContext>,
    systems: Arc<Services>,
) -> Result<MapActionResponse, ProtoResponse> {
    let player_uuid = validate_player_context(&ctx)?;
    let character_uuid_str = req.character_uuid;
    let character_uuid = match Uuid::parse_str(&character_uuid_str) {
        Ok(c) => c,
        Err(_) => {
            return Err(build_error_response(
                GameErrorCode::InvalidCharacter.into(),
                "Invalid character UUID",
            ));
        }
    };

    // Get the player's current map
    let game_map= match systems.maps_service.get_player_map(&player_uuid) {
        None => {
            return Err(build_error_response(
                GameErrorCode::PlayerIsNotInAnyMap.into(),
                "Player is not on any map!",
            ));
        }
        Some(game_map) => game_map,
    };

    // Check if the character is owned by the player requesting the action.
    match game_map.player_by_character.get(&character_uuid) {
        None => {
            return Err(build_error_response(
                GameErrorCode::InvalidCharacter.into(),
                "There is no player for requested character on this map.",
            ));
        }
        Some(player_by_character) => {
            if player_by_character.value().clone() != player_uuid.clone() {
                return Err(build_error_response(
                    GameErrorCode::InvalidCharacter.into(),
                    "Player is requesting action for another player!",
                ));
            }
        }
    }

    let action = match req.map_action {
        None => {
            return Err(build_error_response(
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
            Ok(response)
        }
        MapAction::UsableSkill(skill_request) => {
            if let Some(skill_comp) =
                systems.item_definition_lookup_service.skill_component(&skill_request.skill_definition_id)
            {
                let character_inventory_guard = match systems.inventory_service.get_inventory(player_uuid, character_uuid).await {
                    Ok(x) => {x}
                    Err(e) => {
                        return Err(build_error_response(halfblind_protobuf_network::ErrorCode::UnknownError.into(), format!("Failed to get character inventory: {}", e).as_str()));
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
                    return Err(build_error_response(
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
                Ok(response)
            } else {
                Err(build_error_response(
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
            match systems.inventory_service.get_inventory(player_uuid, character_uuid).await {
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
                    Ok(response)
                }
                Err(_) => {
                    Err(build_error_response(ErrorCode::UnknownError.into(), "Failed to get character inventory"))
                }
            }
        }
        MapAction::UseTeleport(req) => {
            let index = req.teleport_index as usize;
            if game_map.map_component.teleporter.len() <= index {
                return Err(build_error_response(
                    ItemsErrorCode::InvalidItemDefinition.into(),
                    "Index does not exist on teleporters",
                ));
            }
            let teleport = game_map.map_component.teleporter[index];
            if systems.item_definition_lookup_service.transaction_component(&teleport.transaction_id).is_none() {
                return Err(build_error_response(
                    ItemsErrorCode::TransactionInvalid.into(),
                    "Transaction does not exist",
                ));
            };

            let inventory_lock = match systems
                .inventory_service
                .get_inventory(player_uuid, character_uuid)
                .await {
                Ok(x) => x,
                Err(e) => return Err(build_error_response(ErrorCode::UnknownError.into(), &format!("Failed to get character inventory {}", e))),
            };
            let mut inventory_rw_lock = inventory_lock.write().await;
            let mut db_transaction = systems.database_service.clone()
                .get_db_pool()
                .begin()
                .await.map_err(sqlx_error_to_proto_error)?;
            {
                let mut delayed_items_inserter = PostgresDelayedRewardsInserter { connection: &mut db_transaction };
                match systems.transaction_service.process_inventory_transaction_id(
                    systems.random_service.clone(),
                    &mut delayed_items_inserter,
                    &mut inventory_rw_lock,
                    player_uuid,
                    teleport.transaction_id,
                ).await {
                    Ok(_) => {}
                    Err(e) => {
                        return Err(build_error_response(e.into(), &"Failed transaction".to_string()))
                    }
                };
            }

            let visible_inventory: Vec<InventoryItem> = filter_visible_inventory(systems.item_definition_lookup_service.clone(), inventory_rw_lock.as_slice())
                .into_iter()
                .cloned()
                .collect();

            let map_id = teleport.connected_map_id;
            let map_service_clone = systems.maps_service.clone();
            match systems.maps_service
                .change_player_map(
                    map_service_clone,
                    ctx.clone(),
                    player_uuid,
                    character_uuid,
                    visible_inventory,
                    map_id,
                )
                .await
            {
                Ok(_) => {
                    db_transaction.commit().await.map_err(sqlx_error_to_proto_error)?;
                    let response = MapActionResponse {};
                    Ok(response)
                }
                Err(e) => Err(build_error_response(
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
                    return Err(build_error_response(
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
            Ok(response)
        }
    }
}
