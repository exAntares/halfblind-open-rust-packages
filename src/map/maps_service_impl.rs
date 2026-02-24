use crate::characters::characters_service::CharactersService;
use crate::characters::models::Character;
use crate::item_definitions::CharacterDefinitionComponentLookup;
use crate::map::game_map::GameMap;
use crate::map::maps_service::MapsService;
use crate::map::models::{MapAction, MapActionTimed, MapEntities};
use crate::map_update::maps_update_service::MapsUpdateService;
use async_trait::async_trait;
use dashmap::DashMap;
use halfblind_inventory_service::InventoryService;
use halfblind_itemdefinitions_service::ItemDefinitionsService;
use halfblind_network::*;
use proto_gen::InventoryItem;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

#[async_trait]
impl MapsService for MapsServiceImpl {
    async fn change_player_map(
        &self,
        ctx: Arc<ConnectionContext>,
        player_uuid: Uuid,
        character_uuid: Uuid,
        visible_inventory: Vec<InventoryItem>,
        new_map_id: u64,
    ) -> Result<Arc<GameMap>, Box<dyn Error + Send + Sync>> {
        if let Some(_game_map) = self.get_player_map(&player_uuid) {
            self.remove_player_from_all_maps(&player_uuid).await;
        }
        let game_map = match self.get_map(new_map_id) {
            Ok(map) => map,
            Err(e) => return Err(format!("Invalid map {}", e).into()),
        };
        let character_instance = match self
            .character_service
            .get_character_instance(player_uuid, character_uuid)
            .await
        {
            Ok(character_instance) => character_instance,
            Err(_) => return Err("Could not get character instance".into()),
        };

        self.map_id_by_player.insert(player_uuid, game_map.map_id);
        let mut character_db;
        {
            character_db = character_instance.read().await.clone();
            if character_db.current_hp <= 0 {
                character_db.current_hp = 1;
            }
        }// Release lock

        let character_definition = match CharacterDefinitionComponentLookup
            .get(&(character_db.character_definition_id as u64))
        {
            Some(x) => x,
            None => {
                eprintln!("Error getting character definition");
                return Err("Error getting character definition".into());
            }
        };

        let character = Character {
            character_db,
            character_definition: character_definition.clone(),
        };
        game_map.push_action(MapActionTimed {
            timestamp: get_now(),
            action: MapAction::SpawnCharacter {
                character_instance: character,
                character_visible_inventory: visible_inventory,
            },
        });

        game_map
            .player_by_character
            .insert(character_uuid, player_uuid);

        game_map
            .characters_by_player
            .entry(player_uuid)
            .or_insert_with(Vec::new)
            .push(character_uuid);

        // Check if we are the first player in the map, if so, start the update loop
        if game_map.characters_by_player.iter().count() == 1 {
            println!("First Character joined map so we should start update loop");
            self.maps_update_service_impl
                .start_update_loop(game_map.clone());
        } else {
            println!("Map has {} players so we should not start update loop", game_map.characters_by_player.iter().count());
        }
        self.maps_update_service_impl.start_broadcast_loop(
            ctx.clone(),
            player_uuid,
            game_map.clone(),
        );
        Ok(game_map)
    }

    fn get_player_map(&self, player_id: &Uuid) -> Option<Arc<GameMap>> {
        self.map_id_by_player.get(player_id).and_then(|map_id| {
            self.all_maps_by_id
                .get(map_id.value())
                .map(|game_map| game_map.clone())
        })
    }

    async fn remove_player_from_all_maps(&self, player_uuid: &Uuid) {
        // First, get the map_id and remove the player from player_maps in one operation
        let map_id = match self.map_id_by_player.remove(player_uuid) {
            None => return, // There is no map with that player
            Some(x) => x.1,
        };
        let game_map = match self.all_maps_by_id.get(&map_id) {
            None => return, // There is no map with that ID!!
            Some(x) => x,
        };

        // Stop broadcasting this player the map updates.
        game_map.broadcast.remove(player_uuid);

        let (player_uuid, characters_uuid) = match game_map.characters_by_player.remove(player_uuid)
        {
            None => return, // Could not find the player in the map data
            Some(x) => x,
        };
        let game_state = game_map.last_known_game_state.read().await;
        for character_uuid in &characters_uuid {
            game_map.player_by_character.remove(&character_uuid);
            let character_instance = match game_state.entities.get(&character_uuid) {
                None => continue,
                Some(entity) => match &entity.entity_data {
                    MapEntities::PlayerCharacter {
                        character_instance, ..
                    } => character_instance.clone(),
                    _ => continue,
                },
            };
            match self
                .character_service
                .save_character_instance_to_db(&character_instance.character_db)
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Error saving character to db {}", e);
                }
            }

            match self
                .inventory_service
                .save_character_inventory(player_uuid, *character_uuid)
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Error saving character inventory to db {}", e);
                }
            }
        }

        game_map.push_action(MapActionTimed {
            timestamp: get_now(),
            action: MapAction::RemoveCharacters {
                player_uuid,
                characters_uuid,
            },
        })
    }
}

pub struct MapsServiceImpl {
    all_maps_by_id: DashMap<u64, Arc<GameMap>>,
    map_id_by_player: DashMap<Uuid, u64>,
    character_service: Arc<dyn CharactersService + Send + Sync>,
    inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
    item_definitions_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
    maps_update_service_impl: Arc<dyn MapsUpdateService + Send + Sync>,
}

impl MapsServiceImpl {
    pub fn new(
        character_service: Arc<dyn CharactersService + Send + Sync>,
        item_definitions_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
        inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
        maps_update_service_impl: Arc<dyn MapsUpdateService + Send + Sync>,
    ) -> Self {
        let all_maps_by_id = DashMap::new();
        Self {
            all_maps_by_id,
            map_id_by_player: DashMap::new(),
            character_service,
            inventory_service,
            item_definitions_service,
            maps_update_service_impl,
        }
    }

    fn get_map(&self, map_id: u64) -> Result<Arc<GameMap>, Box<dyn Error + Send + Sync>> {
        let actual_map_id = if map_id == 0u64 {
            return Err("Missing MapIsInitialMapComponent!!".into());
        } else {
            map_id
        };

        match self.all_maps_by_id.get(&actual_map_id) {
            None => Err("Missing MapIsInitialMapComponent in hash!!".into()),
            Some(initial_map) => Ok(initial_map.clone()),
        }
    }
}
