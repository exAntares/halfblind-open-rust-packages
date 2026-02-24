use crate::inventory::inventory_item_utils::filter_visible_inventory;
use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::*;
use proto_gen::CharactersQueryResponse;
use proto_gen::{CharacterInstance, CharacterPrivateInstance, InventoryItem};
use std::error::Error;
use std::sync::Arc;

#[derive(Default)]
pub struct CharactersQueryHandler;

#[async_trait]
impl RequestHandler for CharactersQueryHandler {
    async fn handle(
        &self,
        message_id: u64,
        message_timestamp: u64,
        _payload: &[u8],
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, Box<dyn Error + Send + Sync>> {
        let player_uuid = match validate_player_context(&ctx, message_id) {
            Ok(result) => result,
            Err(response) => return Ok(response),
        };

        let characters;
        {
            characters = match SYSTEMS
                .characters_service
                .get_all_character_instances(player_uuid)
                .await
            {
                Ok(characters) => characters,
                Err(e) => {
                    return Ok(build_error_response(
                        message_id,
                        ErrorCode::UnknownError.into(),
                        format!("failed to get all characters {}", e).as_str(),
                    ));
                }
            };
        } // release characters service

        // Convert to protobuf response type
        let mut owned_characters: Vec<CharacterInstance> = Vec::new();
        for c in characters {
            let character = c.read().await;
            let inventory = match SYSTEMS
                .inventory_service
                .get_inventory(player_uuid, character.character_uuid)
                .await
            {
                Ok(inventory) => inventory,
                Err(e) => {
                    eprintln!(
                        "Failure to get character inventory, skipping this character for now"
                    );
                    continue;
                }
            };
            let full_inventory = inventory.read().await;
            let visible_inventory = filter_visible_inventory(&full_inventory)
                .into_iter()
                .cloned()
                .collect::<Vec<InventoryItem>>();
            owned_characters.push(CharacterInstance {
                player_owner_uuid: character.player_uuid.to_string(),
                character_uuid: character.character_uuid.to_string(),
                character_definition_id: character.character_definition_id as u64,
                current_max_hp: character.current_hp as u32,
                current_hp: character.current_hp,
                visible_inventory,
                character_name: character.character_name.clone(),
                private_instance: Some(CharacterPrivateInstance {
                    full_inventory: full_inventory.clone(),
                    agi_spent: character.agility as u32,
                    int_spent: character.intelligence as u32,
                    str_spent: character.strength as u32,
                    vit_spent: character.vitality as u32,
                }),
                statuses: Vec::new(),
            });
        }
        let response = CharactersQueryResponse { owned_characters };
        Ok(encode_ok(message_id, response)?)
    }
}
