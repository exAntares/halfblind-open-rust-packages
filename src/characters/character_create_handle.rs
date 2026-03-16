use crate::inventory::inventory_item_utils;
use crate::item_definitions::CHARACTER_DEFINITION_COMPONENT_LOOKUP;
use crate::systems::systems::{Systems, SYSTEMS};
use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use proto_gen::{CharacterCreateRequest, CharacterCreateResponse, GameErrorCode, InventoryItem};
use proto_gen::{CharacterInstance, CharacterPrivateInstance};
use protobuf_itemdefinition::ItemsErrorCode;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Default)]
pub struct CharacterCreateHandler;

#[async_trait]
impl RequestHandler for CharacterCreateHandler {
    async fn handle(
        &self,
        message_timestamp: u64,
        payload: &[u8],
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, ProtoResponse> {
        let player_uuid = validate_player_context(&ctx)?;
        let req = decode_or_error::<CharacterCreateRequest>(payload)?;
        let character_name = req.character_name.clone();
        let character_definition_id = req.character_definition_id;
        let character_slot_count_id = match SYSTEMS.item_definition_lookup_service.is_current_character_slot_count_component_all()
            .iter().last()
        {
            None => {
                return Ok(build_error_response(
                    ItemsErrorCode::InvalidItemDefinition as i32,
                    &"Character Slot id not found".to_string(),
                ));
            }
            Some(x) => x.0.clone(),
        };

        let slots_count = match SYSTEMS
            .inventory_service
            .get_definition_value_summed(player_uuid, player_uuid, character_slot_count_id)
            .await
        {
            Ok(slots_count) => slots_count,
            Err(_) => {
                return Ok(build_error_response(
                    GameErrorCode::NotEnoughCharacterSlots as i32,
                    "slot count not found".into(),
                ));
            }
        };
        let all_characters = SYSTEMS
            .characters_service
            .get_all_character_instances(player_uuid)
            .await
            .ok()
            .unwrap();
        if all_characters.iter().count() >= slots_count as usize {
            return Ok(build_error_response(
                GameErrorCode::NotEnoughCharacterSlots as i32,
                "Not enough character slots".into(),
            ));
        }

        let character_definition =
            match CHARACTER_DEFINITION_COMPONENT_LOOKUP.get(&character_definition_id) {
                None => {
                    return Ok(build_error_response(
                        ItemsErrorCode::InvalidItemDefinition as i32,
                        "Unknown character definition id",
                    ));
                }
                Some(x) => x,
            };

        let record = match SYSTEMS
            .characters_service
            .add_new_character_into_db(
                player_uuid,
                req.character_definition_id as i64,
                character_definition.clone(),
                character_name.clone(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(build_error_response(
                    halfblind_protobuf_network::ErrorCode::UnknownError as i32,
                    &format!("Failure creating character: {}", e),
                ));
            }
        };

        let initial_inventory: Vec<InventoryItem> = character_definition.initial_inventory
            .iter()
            .map(|x| InventoryItem{
                item_instance_id: "".to_string(),
                item_definition_id: x.item_ref.unwrap().id,
                amount: x.amount,
                is_equipped: false,
                attributes: vec![],
            }).collect::<Vec<_>>();
        let character_db = record.read().await;
        let new_inventory = match add_default_inventory_to_character(
            player_uuid,
            character_db.character_uuid,
            initial_inventory,
            SYSTEMS.clone(),
        )
        .await
        {
            Ok(e) => e,
            Err(e) => {
                return Ok(build_error_response(
                    halfblind_protobuf_network::ErrorCode::UnknownError as i32,
                    &format!("Failure creating initial character inventory: {}", e),
                ));
            }
        };

        let response = CharacterCreateResponse {
            character: Some(CharacterInstance {
                player_owner_uuid: player_uuid.to_string(),
                character_uuid: character_db.character_uuid.to_string(),
                character_definition_id,
                visible_inventory: new_inventory.clone(),
                current_max_hp: character_db.current_hp as u32,
                current_hp: character_db.current_hp,
                private_instance: Some(CharacterPrivateInstance {
                    agi_spent: 0,
                    int_spent: 0,
                    str_spent: 0,
                    vit_spent: 0,
                    full_inventory: new_inventory,
                }),
                character_name,
                statuses: Vec::new(),
            }),
        };
        encode_ok(&response)
    }
}

pub async fn add_default_inventory_to_character(
    player_uuid: Uuid,
    character_uuid: Uuid,
    initial_inventory_from_definition: Vec<InventoryItem>,
    systems: Arc<Systems>,
) -> Result<Vec<InventoryItem>, Box<dyn Error + Send + Sync>> {
    // Convert to InventoryItem protobuf messages using generate_inventory_item_for_player
    let mut inventory_items = Vec::new();
    for (item_id, component) in SYSTEMS.item_definition_lookup_service.inventory_initial_value_character_component_all() {
        let generated_item = inventory_item_utils::generate_inventory_item_for_player(
            systems.items_definitions_service.clone(),
            systems.random_service.clone(),
            player_uuid,
            *item_id,
            component.value as u64,
            0.0, // Empty luck for new players
        );

        inventory_items.push(generated_item);
    }

    for x in initial_inventory_from_definition {
        let generated_item = inventory_item_utils::generate_inventory_item_for_player(
            systems.items_definitions_service.clone(),
            systems.random_service.clone(),
            player_uuid,
            x.item_definition_id,
            x.amount,
            0.0, // Empty luck for new players
        );

        inventory_items.push(generated_item);
    }

    // Save using inventory_service if we have any items
    if !inventory_items.is_empty() {
        systems
            .inventory_service
            .aggregate_inventories(player_uuid, character_uuid, inventory_items.clone())
            .await?;
        systems
            .inventory_service
            .save_character_inventory(player_uuid, character_uuid)
            .await?;
    }

    Ok(inventory_items)
}
