use crate::characters::characters_service::CharactersService;
use crate::characters::models::DatabaseCharacter;
use crate::inventory::inventory_item_utils::filter_visible_inventory;
use crate::map::game_map::GameMap;
use crate::map::maps_service::MapsService;
use crate::map::models::MapAction::SpawnMob;
use crate::map::models::MapEntities::{LootableItem, PlayerCharacter};
use crate::map::models::{GameSnapshot, GameState, MapActionTimed, MapEntities, MapEntity};
use crate::map_update::game_state_rewindable::GameStateRewindable;
use crate::map_update::maps_update_service::MapsUpdateService;
use crate::systems::systems::SYSTEMS;
use futures_util::SinkExt;
use halfblind_inventory_service::InventoryService;
use halfblind_itemdefinitions_service::ItemDefinitionsService;
use halfblind_network::*;
use halfblind_random::RandomService;
use proto_gen::{entity_position, CharacterInstance, CharacterPrivateInstance, EntityPosition, InventoryItem, ItemInstance, MapState, MobInstance, SkillInstance, StatusInstance};
use proto_gen::{MapComponent, MapUpdateResponse};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use tokio::time;
use uuid::Uuid;

impl MapsUpdateService for MapsUpdateServiceImpl {
    fn start_update_loop(&self, map: Arc<GameMap>) -> JoinHandle<()> {
        Self::start_update_loop(
            self.character_service.clone(),
            self.item_definitions_service.clone(),
            self.inventory_service.clone(),
            self.random_service.clone(),
            SYSTEMS.maps_service.clone(),
            map,
        )
    }

    fn start_broadcast_loop(
        &self,
        ctx: Arc<ConnectionContext>,
        player_uuid: Uuid,
        map: Arc<GameMap>,
    ) {
        map.broadcast.insert(player_uuid, ctx.clone());
    }
}

pub struct MapsUpdateServiceImpl {
    character_service: Arc<dyn CharactersService + Send + Sync>,
    item_definitions_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
    inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
    random_service: Arc<dyn RandomService + Send + Sync>,
}

impl MapsUpdateServiceImpl {
    pub fn new(
        character_service: Arc<dyn CharactersService + Send + Sync>,
        item_definitions_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
        inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
        random_service: Arc<dyn RandomService + Send + Sync>,
    ) -> Self {
        Self {
            character_service,
            item_definitions_service,
            inventory_service,
            random_service,
        }
    }

    fn start_update_loop(
        character_service: Arc<dyn CharactersService + Send + Sync>,
        item_definitions_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
        inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
        random_service: Arc<dyn RandomService + Send + Sync>,
        maps_service: Arc<dyn MapsService + Send + Sync>,
        map: Arc<GameMap>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let millis = 16;
            let frames_to_keep_history = 50;
            let tick_time = 16;
            let mut game_tick = time::interval(Duration::from_millis(tick_time));
            let mut broadcast_tick = time::interval(Duration::from_millis(100));
            let mut mobs_tick = time::interval(Duration::from_millis(5000));
            let mut state = GameStateRewindable::new(
                character_service.clone(),
                item_definitions_service.clone(),
                random_service.clone(),
                map.clone(),
                GameState {
                    entities: Default::default(),
                    entities_damage: Default::default(),
                    behaviour_tree_node_states_by_entity: Default::default(),
                },
            );
            let map_component = map.map_component.clone();
            let max_enemies = map_component.max_enemies;
            let random_service = random_service.clone();
            let maps_service = maps_service.clone();
            loop {
                let characters_count = map.characters_by_player.clone().len();
                if characters_count <= 0 {
                    println!("No characters in map, exiting map update loop");
                    while let Some(action) = map.actions_queue.pop() {
                        // clean up the actions_queue
                    }
                    break;
                }
                tokio::select! {
                    _ = game_tick.tick() => {
                        let tick_start = Instant::now(); // Start timing

                        let last_state = state.get_current_state_clone().clone();
                        let new_frame = GameSnapshot {
                            state: last_state.clone(),
                            timestamp:  get_now(),
                            pending_actions: Vec::new(),
                            dirty: true,
                            rng: random_service.get_small_rng_clone(),
                        };
                        // At the top of the update loop, before pushing a new state
                        if state.state_snapshots.len() > frames_to_keep_history {
                            // Keep 5 seconds worth of states
                            state.state_snapshots.pop_front(); // Remove oldest state
                        }
                        state.state_snapshots.push_back(new_frame);
                        // Insert all the pending actions in the right snapshots so we can execute them again
                        {
                            let mut pending_actions = Vec::new();
                            while let Some(action) = map.actions_queue.pop() {
                                pending_actions.push(action);
                            }
                            state.insert_pending_actions(&pending_actions);
                        } // Release the lock
                        let mut dirty_state_index = state
                            .find_first_dirty_state()
                            .unwrap_or(state.state_snapshots.len() - 1);
                        let mut current_state = state.state_snapshots[dirty_state_index].state.clone();
                        let delta_time = (millis as f32) / 1000.0; // 16 ms per tick @ 60fps
                        let rewinding = dirty_state_index < state.state_snapshots.len() - 1;
                        if rewinding {
                            // println!("[MapUpdateLoop]Rewind {dirty_state_index} => {}", state.state_history.len()-1);
                        }

                        while dirty_state_index < state.state_snapshots.len() {
                            state.apply_pending_actions(map.clone(), dirty_state_index, current_state);
                            let timestamp = state.state_snapshots[dirty_state_index].timestamp;
                            current_state = state.execute_game_logic(timestamp, delta_time).await;
                            dirty_state_index = dirty_state_index + 1;
                        };

                        let tick_duration = tick_start.elapsed(); // Measure elapsed time
                        if tick_duration > Duration::from_millis(tick_time) {
                            eprintln!("Warning: game_tick took longer than expected: {:?}", tick_duration);
                        }
                    }
                    _ = mobs_tick.tick() => {
                        Self::spawn_mobs_tick(
                            &state,
                            &map,
                            max_enemies,
                            &map_component,
                            random_service.clone(),
                        ).await;
                    }
                    _ = broadcast_tick.tick() => {
                        let tick_start = Instant::now(); // Start timing

                        // Broadcast world state with error handling
                        let current_state = state.get_current_state_clone();
                        {
                            let mut guard = map.last_known_game_state.write().await;
                            *guard = current_state.clone();
                        }// Release last_known_game_state lock

                        // Drop all states so we only have the last one
                        // We do this so we avoid inconsistencies with player's temporary inventories as we will merge them to their actual inventory
                        while state.state_snapshots.len() > 1 {
                            state.state_snapshots.pop_front();
                        }

                        Self::merge_rewindable_inventory_into_real_inventory(&current_state, inventory_service.clone(), character_service.clone()).await;
                        state.set_current_state(current_state.clone());
                        let map_update_response = Self::game_state_to_update_response(map.map_id, &current_state, inventory_service.clone()).await;
                        // Collect all broadcast connections first to avoid holding the lock during iteration
                        let broadcast_connections: Vec<_> = map.broadcast.iter().map(|entry| entry.value().clone()).collect();
                        for ctx in broadcast_connections {
                            Self::send_map_update(ctx, maps_service.clone(), &map_update_response).await;
                        }

                        let tick_duration = tick_start.elapsed(); // Measure elapsed time
                        if tick_duration > Duration::from_millis(tick_time) {
                            eprintln!("Warning: broadcast_tick took longer than expected: {:?}", tick_duration);
                        }
                    }
                }
            }
        })
    }

    async fn spawn_mobs_tick(
        state: &GameStateRewindable,
        map: &GameMap,
        max_enemies: u32,
        map_component: &MapComponent,
        random_service: Arc<dyn RandomService + Send + Sync>,
    ) {
        let current_mob_count = state.get_current_state_clone().entities.iter()
            .filter(|entity| matches!(entity.entity_data, MapEntities::MobCharacter { .. }))
            .count();

        if current_mob_count > 0 {
            return;
        }

        let mut missing_spawn_count = max_enemies.saturating_sub(current_mob_count as u32);
        let spawn_points_data: Vec<_> = map_component.spawn_points
            .iter()
            .filter_map(|spawn_point| {
                SYSTEMS.item_definition_lookup_service.mob_component(&spawn_point.enemy_definition_id)
                    .map(|mob_comp| (spawn_point.enemy_definition_id, spawn_point.position, mob_comp.clone()))
            }).collect();
        let spawn_points_count = spawn_points_data.len();
        while missing_spawn_count > 0 && spawn_points_count > 0 {
            let spawn_index = random_service.random_range_usize(0, &spawn_points_data.len()-1);
            let ( definition_id, position, mob_comp) = &spawn_points_data[spawn_index];
            let action = MapActionTimed {
                timestamp: get_now(),
                action: SpawnMob {
                    mob_definition_id: *definition_id,
                    mob_component: mob_comp.clone(),
                    target_position: position.unwrap_or_default(),
                },
            };
            map.push_action(action);
            missing_spawn_count -= 1;
        }
    }

    async fn try_level_up_character(
        character_instance: &mut DatabaseCharacter,
        inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
        characters_service: Arc<dyn CharactersService + Send + Sync>,
    ) {
        let character_inventory_lock = match inventory_service
            .get_inventory(
                character_instance.player_uuid,
                character_instance.character_uuid,
            )
            .await
        {
            Ok(inventory) => inventory,
            Err(e) => {
                eprintln!("Failed to get character inventory: {}", e);
                return;
            }
        };
        let mut character_inventory = character_inventory_lock.write().await;
        match characters_service
            .try_level_up_character(&mut character_inventory)
        {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Failed to check level up: {}", e);
            }
        };
    }

    async fn merge_rewindable_inventory_into_real_inventory(
        current_state: &GameState,
        inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
        characters_service: Arc<dyn CharactersService + Send + Sync>,
    ) {
        let mut unclaimed_drops: Vec<MapEntities> = Vec::new();
        for mut entity in current_state.entities.iter_mut() {
            if let PlayerCharacter {
                position,
                character_instance,
                rewindable_character_inventory,
                visible_inventory,
                ..
            } = &mut entity.entity_data
            {
                let unable_to_claim_inventory = match inventory_service
                    .aggregate_inventories(
                        character_instance.character_db.player_uuid,
                        character_instance.character_db.character_uuid,
                        rewindable_character_inventory.clone(),
                    )
                    .await
                {
                    Ok(unable_to_claim_inventory) => unable_to_claim_inventory,
                    Err(e) => {
                        eprintln!("Failed to merge inventory: {}", e);
                        continue;
                    }
                };
                rewindable_character_inventory.clear();
                for unclaimed_item in unable_to_claim_inventory {
                    unclaimed_drops.push(LootableItem {
                        owner_uuid: character_instance.character_db.character_uuid,
                        definition_id: unclaimed_item.item_definition_id,
                        position: position.clone(),
                        amount: unclaimed_item.amount,
                    });
                }
                // If we merged the inventory, check if the character should level up
                Self::try_level_up_character(
                    &mut character_instance.character_db,
                    inventory_service.clone(),
                    characters_service.clone(),
                )
                .await;
                // after level up we should update the visible inventory
                match inventory_service
                    .get_inventory(
                    character_instance.character_db.player_uuid,
                    character_instance.character_db.character_uuid)
                    .await {
                    Ok(character_inventory_lock) => {
                        let character_inventory = character_inventory_lock.read().await;
                        *visible_inventory = filter_visible_inventory(character_inventory.as_slice())
                            .into_iter()
                            .cloned()
                            .collect();
                    }
                    Err(_) => {}
                }
            }
        }
        for drop in unclaimed_drops {
            let uuid = Uuid::new_v4();
            current_state.entities.insert(
                uuid,
                MapEntity {
                    entity_uuid: uuid,
                    entity_data: drop,
                    creation_timestamp: get_now(),
                },
            );
        }
    }

    async fn send_map_update(
        ctx: Arc<ConnectionContext>,
        maps_service: Arc<dyn MapsService + Send + Sync>,
        update: &MapUpdateResponse,
    ) {
        let message = match encode_message(0, update.clone()) {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("Failed to encode message: {}", e);
                return;
            }
        };

        let player_uuid = match ctx.get_player_uuid() {
            None => {
                eprintln!("Unknown connection context player!!");
                return;
            }
            Some(x) => x,
        };

        if !ctx.is_player_connected() {
            maps_service.remove_player_from_all_maps(&player_uuid).await;
            return;
        }

        let mut is_player_lost_connection = false;
        // Scope the mutex lock to ensure it's released
        {
            match ctx.ws_writer.try_lock() {
                Ok(mut writer) => {
                    if let Err(e) = writer.send(message).await {
                        eprintln!(
                            "Failed to send map update, probably player disconnected: {}",
                            e
                        );
                        is_player_lost_connection = true;
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Failed to try_lock ws_writer someone else is using it\
                                Will try again later {}",
                        e
                    );
                    return;
                }
            }
        } // release lock

        if is_player_lost_connection {
            maps_service.remove_player_from_all_maps(&player_uuid).await;
        }
    }

    pub async fn game_state_to_update_response(
        map_definition_id: u64,
        game_state: &GameState,
        inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
    ) -> MapUpdateResponse {
        let mut map_state = MapState {
            map_definition_id,
            entities: vec![],
            damage_entities: vec![],
        };
        for entity in game_state.entities.iter() {
            let entities = &entity.entity_data;
            let creation_timestamp = entity.creation_timestamp;
            let entity_uuid = entity.entity_uuid;
            match entities {
                PlayerCharacter {
                    character_instance,
                    visible_inventory: equipped_inventory,
                    position,
                    statuses,
                    ..
                } => {
                    let character_full_inventory = match inventory_service
                        .get_inventory(
                            character_instance.character_db.player_uuid,
                            character_instance.character_db.character_uuid,
                        )
                        .await
                    {
                        Ok(character_inventory) => character_inventory.read().await.clone(),
                        Err(e) => {
                            eprintln!("Failed to get inventory: {}", e);
                            continue;
                        }
                    };
                    let character_definition = match SYSTEMS.item_definition_lookup_service.character_definition_component(&(character_instance.character_db.character_definition_id as u64)) {
                        None => {
                            eprintln!("Failed to get character definition: {}", character_instance.character_db.character_definition_id);
                            continue;
                        }
                        Some(x) => x,
                    };
                    let max_hp = character_definition.base_hp as u32; // TODO ADD VITALITY FROM EQUIPS
                    map_state.entities.push(EntityPosition {
                        creation_timestamp,
                        position: Some(*position),
                        entity: Some(entity_position::Entity::Player(CharacterInstance {
                            player_owner_uuid: character_instance
                                .character_db
                                .player_uuid
                                .to_string(),
                            character_uuid: character_instance
                                .character_db
                                .character_uuid
                                .to_string(),
                            character_definition_id: character_instance
                                .character_db
                                .character_definition_id
                                as u64,
                            current_max_hp: max_hp,
                            current_hp: character_instance.character_db.current_hp,
                            visible_inventory: equipped_inventory.clone(),
                            character_name: character_instance.character_db.character_name.clone(),
                            private_instance: Some(CharacterPrivateInstance {
                                full_inventory: character_full_inventory,
                                agi_spent: character_instance.character_db.agility as u32,
                                int_spent: character_instance.character_db.intelligence as u32,
                                str_spent: character_instance.character_db.strength as u32,
                                vit_spent: character_instance.character_db.vitality as u32,
                            }),
                            statuses: statuses.iter().map(|x| StatusInstance{
                                definition_id: x.definition_id,
                                remaining_lifetime: x.lifetime as f32,
                            }).collect()
                        })),
                    });
                }
                MapEntities::MobCharacter {
                    mob_definition_id,
                    mob_component,
                    position,
                    current_hp,
                    ..
                } => {
                    map_state.entities.push(EntityPosition {
                        creation_timestamp,
                        position: Some(*position),
                        entity: Some(entity_position::Entity::Mob(MobInstance {
                            definition_id: *mob_definition_id,
                            current_hp: *current_hp,
                            max_hp: mob_component.max_hp,
                            instance_id: entity_uuid.to_string(),
                        })),
                    });
                }
                MapEntities::Skill {
                    skill_definition_id,
                    position,
                    owner_uuid,
                    lifetime_milliseconds,
                    ..
                } => {
                    map_state.entities.push(EntityPosition {
                        creation_timestamp,
                        position: Some(*position),
                        entity: Some(entity_position::Entity::Skill(SkillInstance {
                            definition_id: *skill_definition_id,
                            instance_id: entity_uuid.to_string(),
                            owner_uuid: owner_uuid.to_string(),
                            remaining_lifetime: *lifetime_milliseconds,
                        })),
                    });
                }
                LootableItem {
                    owner_uuid,
                    definition_id,
                    position,
                    amount,
                    ..
                } => {
                    map_state.entities.push(EntityPosition {
                        creation_timestamp,
                        position: Some(*position),
                        entity: Some(entity_position::Entity::PickableItem(ItemInstance {
                            owner_uuid: owner_uuid.to_string(),
                            definition_id: *definition_id,
                            value: *amount,
                            instance_uuid: entity_uuid.to_string(),
                        })),
                    });
                }
            }
        }

        for entity in game_state.entities_damage.iter() {
            map_state.damage_entities.push(proto_gen::DamageInstance {
                instance_id: entity.entity_uuid.to_string(),
                creation_timestamp: entity.creation_timestamp,
                position: Some(entity.damage_target_position),
                damage_owner_uuid: entity.damage_owner_uuid.to_string(),
                damage_target_uuid: entity.damage_target_uuid.to_string(),
                damage_amount: entity.damage_amount,
                is_critical_hit: entity.is_critical_hit,
                damage_type: entity.damage_type.into(),
            });
        }

        let update_message = MapUpdateResponse {
            map_state: Some(map_state),
        };
        update_message
    }
}
