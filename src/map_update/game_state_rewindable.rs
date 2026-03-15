use crate::behaviour_trees::behaviour_tree_decoder::get_behavior_tree_node;
use crate::behaviour_trees::behaviour_tree_map_context::BehaviourTreeMapContext;
use crate::behaviour_trees::utils::move_to_positions;
use crate::characters::characters_service::CharactersService;
use crate::combat::combat_utils::{get_skill_damage, CharacterDamageModifier};
use crate::inventory::inventory_item_utils;
use crate::map::game_map::GameMap;
use crate::map::models::MapAction::MoveTo;
use crate::map::models::MapEntities::{MobCharacter, PlayerCharacter, Skill};
use crate::map::models::{DamageEntity, GameSnapshot, GameState, MapAction, MapActionTimed, MapEntities, MapEntity, StatusInstance, TargetPositions};
use crate::systems::systems::SYSTEMS;
use dashmap::{DashMap, DashSet};
use glam::Vec2;
use halfblind_itemdefinitions_service::ItemDefinitionsService;
use halfblind_network::*;
use halfblind_random::RandomService;
use proto_gen::{CharacterStat, Position, SkillAoETargetType};
use proto_gen::{DamageType, MobComponent, SkillTargetMode};
use rayon::prelude::*;
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone)]
pub struct GameStateRewindable {
    pub state_snapshots: VecDeque<GameSnapshot>, // You should also consider using a VecDeque if you are frequently removing old states from the front

    character_service: Arc<dyn CharactersService + Send + Sync>,
    item_definition_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
    random_service: Arc<dyn RandomService + Send + Sync>,

    max_lag_ms: u64,
    current_state: GameState,
    game_map: Arc<GameMap>,
}

impl GameStateRewindable {
    pub fn new(
        character_service: Arc<dyn CharactersService + Send + Sync>,
        item_definition_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
        random_service: Arc<dyn RandomService + Send + Sync>,
        game_map: Arc<GameMap>,
        current_state: GameState,
    ) -> Self {
        let snapshot = GameSnapshot {
            state: current_state.clone(),
            timestamp: get_now(),
            pending_actions: Default::default(),
            dirty: true,
            rng: random_service.get_small_rng_clone()
        };
        Self {
            character_service,
            item_definition_service,
            game_map,
            random_service,
            max_lag_ms: 500,
            current_state,
            state_snapshots: VecDeque::from([snapshot]),
        }
    }

    pub async fn execute_game_logic(&mut self, timestamp: u64, delta_time: f32) -> GameState {
        let delta_time_milliseconds = delta_time * 1000.0;
        let entities_damaged = Mutex::new(Vec::new());
        let entities_affected_status = Mutex::new(Vec::new());
        let entities_to_remove = DashSet::new();
        let mut dmg_to_remove = Vec::new();
        let mut entities_to_add : Mutex<Vec<MapEntities>> = Mutex::new(Vec::new());
        let current_state_clone = self.current_state.clone();
        // First, collect all the mob positions we need to check
        let mob_positions: Vec<(Uuid, Position)> = self
            .current_state
            .entities
            .iter()
            .filter_map(|entity| {
                if let MobCharacter {position,..} = &entity.entity_data {
                    Some((entity.entity_uuid, position.clone()))
                } else {
                    None
                }
            })
            .collect();
        let characters_positions: Vec<(Uuid, Position)> = self
            .current_state
            .entities
            .iter()
            .filter_map(|entity| {
                if let PlayerCharacter {
                    position,
                    ..
                } = &entity.entity_data
                {
                    Some((entity.entity_uuid, position.clone()))
                } else {
                    None
                }
            })
            .collect();
        self.update_damage_entities(&mut dmg_to_remove, delta_time_milliseconds);
        self.current_state.entities
            .iter_mut()
            .par_bridge()
            .for_each(|mut entity| {
                let entity_uuid = entity.entity_uuid.clone();
                match &mut entity.entity_data {
                    PlayerCharacter {
                        character_instance: character,
                        position,
                        target_positions,
                        statuses,
                        ..
                    } => {
                        statuses.iter_mut().for_each(|x|x.lifetime -= delta_time_milliseconds as f64);
                        statuses.retain(|status| status.lifetime > 0.0);
                        if character.character_db.current_hp <= 0 {
                            // No update for dead players
                            return;
                        }
                        let character_definition_component = match SYSTEMS.item_definition_lookup_service.character_definition_component(&(character.character_db.character_definition_id as u64))
                        {
                            None => return,
                            Some(character_definition_component) => character_definition_component,
                        };
                        if target_positions.positions.len() > 0 {
                            let speed = character_definition_component.base_movement_speed as f32;
                            if move_to_positions(
                                position,
                                target_positions,
                                speed,
                                delta_time,
                            ) {
                                *target_positions = TargetPositions {
                                    positions: vec![],
                                    current_index: 0,
                                }
                            }
                        }
                    }
                    MobCharacter {
                        mob_definition_id,
                        ..
                    } => {
                        let mob_id = mob_definition_id.clone();
                        if let Some(behaviour_tree_comp) = SYSTEMS.item_definition_lookup_service.behaviour_tree_component(&mob_id) {
                            let mut blackboard = BehaviourTreeMapContext {
                                entity_uuid: entity_uuid.clone(),
                                entity: &mut entity.entity_data,
                                delta_time,
                                random_service: self.random_service.clone(),
                                game_map: self.game_map.clone(),
                                game_state_readonly: &current_state_clone,
                                entities_to_add: &entities_to_add,
                            };
                            if let Some(bt) = get_behavior_tree_node(mob_id, &behaviour_tree_comp) {
                                let mut nodes_states = match self.current_state.behaviour_tree_node_states_by_entity.get_mut(&entity_uuid) {
                                    None => {
                                        self.current_state.behaviour_tree_node_states_by_entity.insert(entity_uuid, DashMap::new());
                                        self.current_state.behaviour_tree_node_states_by_entity.get_mut(&entity_uuid).unwrap()
                                    }
                                    Some(x) => x
                                };
                                bt.tick(&mut nodes_states, &mut blackboard);
                            }
                        }
                    }
                    Skill {
                        owner_uuid,
                        skill_component,
                        damage_heal_per_tick,
                        position,
                        direction,
                        lifetime_milliseconds,
                        remaining_time_for_damage,
                        already_affected_entities,
                        is_critical_hit,
                        ..
                    } => {
                        if *lifetime_milliseconds <= 0.0 {
                            entities_to_remove.insert(entity_uuid);
                            return;
                        }
                        *lifetime_milliseconds -= delta_time_milliseconds;
                        // Enemies of mobs are characters, and enemies of characters are mobs
                        let target_type = SkillAoETargetType::try_from(skill_component.aoe_target_type)
                            .unwrap_or(SkillAoETargetType::Enemies);
                        let available_target_positions: &Vec<(Uuid, Position)> = match current_state_clone.entities.get(owner_uuid) {
                            None => { return;}
                            Some(owner_entity) => {
                                match owner_entity.entity_data {
                                    PlayerCharacter { .. } => {
                                        match target_type {
                                            SkillAoETargetType::Enemies => { &mob_positions }
                                            SkillAoETargetType::Allies => { &characters_positions }
                                        }
                                    }
                                    MobCharacter { .. } => {
                                        match target_type {
                                            SkillAoETargetType::Enemies => { &characters_positions }
                                            SkillAoETargetType::Allies => { &mob_positions }
                                        }
                                    }
                                    _ => {
                                        &vec![]
                                    }
                                }
                            }
                        };
                        Self::move_towards(
                            position,
                            direction.clone(),
                            skill_component.movement_speed,
                            delta_time,
                        );
                        *remaining_time_for_damage -= delta_time_milliseconds;
                        let total_lifetime = skill_component.lifetime_millis;
                        let skill_progress = (total_lifetime as f32 - lifetime_milliseconds.clone())
                            / total_lifetime as f32;
                        if *remaining_time_for_damage > 0.0 {
                            return;
                        }
                        let mut tick_total_duration = skill_component.trigger_interval_millis;
                        if tick_total_duration <= 0 {
                            // 0 means instant damage
                            tick_total_duration = 999999999; // Set the next tick way in the future so it does not happen
                        }
                        *remaining_time_for_damage = tick_total_duration as f32;
                        // Queue Damage
                        let skill_radius =
                            (1.0 - skill_progress)
                                * skill_component.radius_start
                                + skill_progress * skill_component.radius_end;
                        let enemies_in_range: Vec<(Uuid, Position)> = available_target_positions
                            .clone()
                            .into_iter()
                            .filter_map(|(uuid, entity_pos)| {
                                let dx = position.x - entity_pos.x;
                                let dy = position.y - entity_pos.y;
                                let distance = (dx * dx + dy * dy);
                                if (distance * distance) <= (skill_radius * skill_radius) {
                                    Some((uuid, entity_pos))
                                } else {
                                    None
                                }
                            })
                            .collect();

                        let skill_target_mode = SkillTargetMode::try_from(skill_component.skill_taget_mode)
                            .unwrap_or(SkillTargetMode::MultipleTargets);
                        let mut tick_targets = HashSet::new();
                        match skill_target_mode {
                            SkillTargetMode::MultipleTargets => {
                                for (enemy_uuid, _) in enemies_in_range {
                                    already_affected_entities.insert(enemy_uuid);
                                    tick_targets.insert(enemy_uuid);
                                }
                            }
                            SkillTargetMode::SingleTarget => {
                                if already_affected_entities.iter().count() <= 0 {
                                    for (enemy_uuid, _) in enemies_in_range {
                                        already_affected_entities.insert(enemy_uuid);
                                        tick_targets.insert(enemy_uuid);
                                        break;
                                    }
                                } else{
                                    for (enemy_uuid, _) in enemies_in_range {
                                        if already_affected_entities.contains(&enemy_uuid) {
                                            tick_targets.insert(enemy_uuid);
                                        }
                                    }
                                }
                            }
                            SkillTargetMode::Self_ => {
                                tick_targets.insert(owner_uuid.clone());
                            }
                        }

                        for target in tick_targets {
                            match skill_component.on_hit {
                                None => {}
                                Some(x) => {
                                    if let Some(status) = SYSTEMS.item_definition_lookup_service.status_on_hit_component(&x.id) {
                                        entities_affected_status.lock().unwrap().push((target, x.id, status))
                                    }
                                    let mut damage_type_opt = None;
                                    if let Some(_) = SYSTEMS.item_definition_lookup_service.damage_on_hit_component(&x.id) {
                                        damage_type_opt = Some(DamageType::Damage);
                                    }
                                    if let Some(_) = SYSTEMS.item_definition_lookup_service.heal_on_hit_component(&x.id) {
                                        damage_type_opt = Some(DamageType::Heal);
                                    }
                                    match damage_type_opt {
                                        None => {}
                                        Some(damage_type) => {
                                            entities_damaged.lock().unwrap().push((target, damage_heal_per_tick.clone(), owner_uuid.clone(), damage_type, is_critical_hit.clone()));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    MapEntities::LootableItem { .. } => {
                        // Nothing to do here loot does not have logic
                        // Perhaps delete loot over time, so it's not collected forever
                    }
                }
        });

        // Apply damage after iterating the entities as they are locked while iteration
        for (damaged_entity_uuid, damage, damage_source, damage_type, is_critical_hit) in entities_damaged.lock().unwrap().iter() {
            let (current_hp, position) = match self.apply_damage_or_heal(damaged_entity_uuid, *damage, damage_source, damage_type) {
                Ok(x) => x,
                Err(_) => continue,
            };
            let damage_uuid = Uuid::new_v4();
            self.current_state.entities_damage.insert(
                damage_uuid,
                DamageEntity {
                    entity_uuid: damage_uuid,
                    creation_timestamp: timestamp,
                    damage_owner_uuid: damage_source.clone(),
                    damage_target_uuid: damaged_entity_uuid.clone(),
                    damage_amount: damage.floor() as u64,
                    is_critical_hit: *is_critical_hit,
                    damage_target_position: position,
                    duration_millis: 2000.0, // damage feedback is alive for 2 seconds
                    damage_type: damage_type.clone(),
                },
            );
            if current_hp > 0 {
                continue;
            }
            // DEATH!!
            let mob_data_option = match self.current_state.entities.get(&damaged_entity_uuid) {
                None => continue,
                Some(damaged_entity) => {
                    match &damaged_entity.entity_data {
                        MobCharacter { mob_component, position, damage_dealers,
                            ..
                        } => { Some((mob_component.clone(), position.clone(), damage_dealers.clone())) }
                        _=> { None }
                    }
                },
            };

            if let Some((mob_component, position, damage_dealers)) = mob_data_option {
                // REMOVE MOB
                entities_to_remove.insert(damaged_entity_uuid.clone());

                // MOBS DROP LOOT
                self.drop_items(
                    damage_dealers.clone(),
                    &mob_component,
                    position.clone(),
                    &mut entities_to_add,
                );
                // GIVE XP
                for damager in damage_dealers.iter() {
                    let mut character = match self
                        .current_state
                        .entities
                        .get_mut(damager)
                    {
                        None => continue,
                        Some(x) => x,
                    };
                    match &mut character.entity_data {
                        PlayerCharacter {
                            character_instance,
                            rewindable_character_inventory,
                            ..
                        } => {
                            match self.character_service.add_xp_to_inventory(
                                character_instance.character_db.player_uuid,
                                rewindable_character_inventory,
                                mob_component.xp,
                            ) {
                                Ok(_) => {}
                                Err(_) => {
                                    eprintln!(
                                        "Failed to add xp to character {}",
                                        damage_source
                                    );
                                }
                            };
                        }
                        _ => continue,
                    }
                }
            }
        }

        // Apply status effects
        for (affected_entity_uuid, status_definition_id, status_comp) in entities_affected_status.lock().unwrap().iter() {
            match self.current_state.entities.get_mut(&affected_entity_uuid) {
                None => {}
                Some(mut entity) => {
                    match &mut entity.entity_data {
                        PlayerCharacter { statuses, .. } => {
                            statuses.push(StatusInstance {
                                definition_id: *status_definition_id,
                                status_component: status_comp.clone(),
                                lifetime: status_comp.lifetime_millis as f64,
                            })
                        }
                        MobCharacter { .. } => {

                        }
                        _ => {}
                    }
                }
            };
        }

        // cleanup dead entities
        for entity_uuid in entities_to_remove {
            self.current_state.entities.remove(&entity_uuid);
        }
        for entity in entities_to_add.lock().unwrap().iter() {
            let uuid = Uuid::new_v4();
            self.current_state.entities.insert(
                uuid,
                MapEntity {
                    entity_uuid: uuid,
                    entity_data: entity.clone(),
                    creation_timestamp: timestamp,
                },
            );
        }
        self.current_state.clone()
    }

    fn update_damage_entities(
        &mut self,
        dmg_to_remove: &mut Vec<Uuid>,
        delta_time_milliseconds: f32,
    ) {
        for mut dmg in self.current_state.entities_damage.iter_mut() {
            dmg.duration_millis -= delta_time_milliseconds;
            if dmg.duration_millis <= 0.0 {
                dmg_to_remove.push(dmg.entity_uuid);
            }
        }
        for uuid in dmg_to_remove {
            self.current_state.entities_damage.remove(&uuid);
        }
    }

    fn apply_damage_or_heal(
        &mut self,
        damaged_entity_uuid: &Uuid,
        damage: f64,
        damage_source: &Uuid,
        damage_type: &DamageType,
    ) -> Result<(u64, Position), Box<dyn std::error::Error + Send + Sync>> {
        let mut damaged_entity = match self.current_state.entities.get_mut(&damaged_entity_uuid) {
            None => return Err("Entity not found".into()),
            Some(x) => x,
        };
        let damage_floor = damage.floor().abs() as u64;
        match &mut damaged_entity.entity_data {
            MobCharacter {
                current_hp,
                position,
                damage_dealers,
                ..
            } => {
                match damage_type {
                    DamageType::Damage => {*current_hp = current_hp.saturating_sub(damage_floor);}
                    DamageType::Heal => {*current_hp = current_hp.saturating_add(damage_floor);}
                }
                damage_dealers.insert(damage_source.clone());
                Ok((current_hp.clone(), position.clone()))
            }
            PlayerCharacter {
                character_instance,
                position,
                ..
            } => {
                let max_hp = SYSTEMS.item_definition_lookup_service.character_definition_component(&(character_instance.character_db.character_definition_id as u64))
                    .unwrap()
                    .base_hp;
                match damage_type {
                    DamageType::Damage => {
                        character_instance.character_db.current_hp = character_instance
                            .character_db
                            .current_hp
                            .saturating_sub(damage_floor as i32)
                            .clamp(0, max_hp);
                    }
                    DamageType::Heal => {
                        character_instance.character_db.current_hp = character_instance
                            .character_db
                            .current_hp
                            .saturating_add(damage_floor as i32)
                            .clamp(0, max_hp);
                    }
                }
                Ok((character_instance.character_db.current_hp.clone() as u64, position.clone()))
            }
            _ => Err("Is not damageable entity".into()),
        }
    }

    fn drop_items(
        &self,
        damage_dealers: HashSet<Uuid>,
        mob_component: &MobComponent,
        position: Position,
        entities_to_add: &Mutex<Vec<MapEntities>>,
    ) {
        let loot_table = match mob_component.loot_table.as_ref() {
            // We can't drop anything if the mob does not have a loot table
            None => { return; }
            Some(x) => {x}
        };
        // Drop items per person that damaged the mob
        for damage_dealer in damage_dealers {
            let min = loot_table.drop_count_min as i32;
            let max = loot_table.drop_count_max as i32;
            let loot_count = self.random_service.random_range_i32(min, max);
            for _ in 0..loot_count {
                let new_position = self.find_nearby_valid_nav_point(&position, 2.5, 6)
                    .unwrap_or_else(|| Position {
                        x: position.x,
                        y: position.y,
                    });
                let weights: Vec<u32> = loot_table
                    .droppable_items
                    .iter()
                    .map(|d| d.weight)
                    .collect();
                if let Some(idx) = inventory_item_utils::select_weighted_index(
                    self.random_service.clone(),
                    &weights,
                    1.0, // TODO: use luck here?
                ) {
                    let selected_item = &loot_table.droppable_items[idx];
                    if SYSTEMS.item_definition_lookup_service.can_roll_attributes_component(&selected_item.id_ref.unwrap().id).is_some() {
                        // For the moment, we drop item definitions, we will roll the attributes on pick up
                        // with the character that picks up the item
                        entities_to_add.lock().unwrap().push(MapEntities::LootableItem {
                            owner_uuid: damage_dealer.clone(),
                            definition_id: selected_item.id_ref.unwrap().id,
                            position: new_position,
                            amount: 1,
                        });
                    } else if SYSTEMS.item_definition_lookup_service.is_stackable_component(&selected_item.id_ref.unwrap().id,).is_some() {
                        let amount = self.random_service.random_range_u32(selected_item.min_value, selected_item.max_value) as u64;
                        if amount <= 0 {
                            continue;
                        }
                        entities_to_add.lock().unwrap().push(MapEntities::LootableItem {
                            owner_uuid: damage_dealer.clone(),
                            definition_id: selected_item.id_ref.unwrap().id,
                            position: new_position,
                            amount,
                        });
                    } else {
                        entities_to_add.lock().unwrap().push(MapEntities::LootableItem {
                            owner_uuid: damage_dealer.clone(),
                            definition_id: selected_item.id_ref.unwrap().id,
                            position: new_position,
                            amount: 1,
                        });
                    }
                }
            }
        }
    }
    
    fn move_towards(position: &mut Position, direction: Position, speed: f32, delta_time: f32) {
        position.x += (direction.x * speed * delta_time);
        position.y += (direction.y * speed * delta_time);
    }

    pub fn get_current_state_clone(&self) -> GameState {
        self.current_state.clone()
    }

    pub fn set_current_state(&mut self, new_state: GameState) {
        self.current_state = new_state;
    }

    pub fn find_first_dirty_state(&self) -> Option<usize> {
        self.state_snapshots
            .iter()
            .position(|snapshot| snapshot.dirty)
    }

    pub fn insert_pending_actions(&mut self, pending_actions: &Vec<MapActionTimed>) {
        let last_timestamp = self.state_snapshots.back().unwrap().timestamp;
        for player_action in pending_actions.iter() {
            let action_timestamp = player_action.timestamp;
            if last_timestamp >= action_timestamp {
                let lag_ms = last_timestamp - action_timestamp;
                if lag_ms > self.max_lag_ms {
                    // Message way too old to be relevant
                    println!("{:?} lag is high {lag_ms}", player_action.action);
                }
            }
            // Using binary_search_by to find the closest snapshot
            let index = self
                .state_snapshots
                .binary_search_by(|snapshot| {
                    if snapshot.timestamp <= action_timestamp {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    }
                })
                .unwrap_or_else(|insert_pos| {
                    if insert_pos == 0 {
                        0 // If no lower timestamp found, use the earliest snapshot
                    } else {
                        insert_pos - 1 // Use the previous position to get the closest lower timestamp
                    }
                });
            if last_timestamp >= action_timestamp {
                let lag_ms = last_timestamp - action_timestamp;
            } else {
                let future_timestamp = action_timestamp - last_timestamp;
                println!(
                    "{:?} was requested {future_timestamp}ms the future!!",
                    player_action.action
                );
            }
            let game_snapshot = &mut self.state_snapshots[index];
            game_snapshot.pending_actions.push(player_action.clone());
            game_snapshot.dirty = true;
        }
    }

    pub fn apply_pending_actions(&mut self, map: Arc<GameMap>, index: usize, state: GameState) {
        self.set_current_state(state.clone());
        let game_snapshot: &mut GameSnapshot = &mut (self.state_snapshots[index]);
        game_snapshot.state = state;
        game_snapshot.dirty = false;
        let mut game_snapshot_rng = game_snapshot.rng.clone();
        game_snapshot.pending_actions.iter().for_each(|action| {
            match &action.action {
                MapAction::SpawnCharacter {
                    character_instance,
                    character_visible_inventory,
                } => {
                    // println!("Execute SpawnCharacter {player_uuid} {character_uuid}");
                    self.current_state.entities.insert(
                        character_instance.character_db.character_uuid,
                        MapEntity {
                            creation_timestamp: game_snapshot.timestamp,
                            entity_uuid: character_instance.character_db.character_uuid,
                            entity_data: PlayerCharacter {
                                character_instance: character_instance.clone(),
                                rewindable_character_inventory: Vec::new(),
                                visible_inventory: character_visible_inventory.clone(),
                                position: Default::default(),
                                target_positions: TargetPositions {
                                    positions: Vec::new(),
                                    current_index: 0,
                                },
                                statuses: Vec::new(),
                            },
                        },
                    );
                }
                MapAction::RemoveCharacters {
                    characters_uuid, ..
                } => {
                    // println!("Execute RemoveCharacter {player_uuid} {character_uuid}");
                    for character_uuid in characters_uuid {
                        self.current_state.entities.remove(&character_uuid);
                    }
                }
                MoveTo {
                    entity_uuid,
                    target_positions,
                } => {
                    // println!("Execute MoveTo {entity_uuid} {},{}", target_position.x, target_position.y);
                    let mut current_position = Vec2::new(0.0, 0.0);
                    match game_snapshot.state.entities.get(entity_uuid) {
                        None => {}
                        Some(entity_tuple) => match &entity_tuple.entity_data {
                            PlayerCharacter { character_instance, position, .. } => {
                                if character_instance.character_db.current_hp <= 0 {
                                    // Dead characters get moved into a new map from the update service
                                    return;
                                }
                                current_position = Vec2::new(position.x, position.y);
                            }
                            MobCharacter { position, .. } => {
                                current_position = Vec2::new(position.x, position.y);
                            }
                            _ => {}
                        },
                    }
                    let mut final_positions = Vec::new();
                    // Recalculate points do not trust the client
                    for target_position in target_positions {
                        let to_position = Vec2::new(target_position.x, target_position.y);
                        if let Some(final_position) =
                            map.nav_mesh.intersects_line(current_position, to_position)
                        {
                            final_positions.push(Position {
                                x: final_position.x,
                                y: final_position.y,
                            });
                        } else if map.nav_mesh.contains_point(to_position) {
                            final_positions.push(Position {
                                x: to_position.x,
                                y: to_position.y,
                            });
                        }
                        current_position = Vec2::new(target_position.x, target_position.y);
                    }
                    let new_target_positions = TargetPositions {
                        positions: final_positions,
                        current_index: 0,
                    };
                    match self.current_state.entities.get_mut(entity_uuid) {
                        None => {}
                        Some(mut entity_tuple) => match &mut entity_tuple.entity_data {
                            PlayerCharacter { target_positions, .. } => {
                                *target_positions = new_target_positions;
                            }
                            MobCharacter { target_positions, .. } => {
                                *target_positions = new_target_positions;
                            }
                            _ => {}
                        },
                    }
                }
                MapAction::SpawnMob {
                    mob_definition_id,
                    mob_component,
                    target_position,
                } => {
                    // println!("Execute SpawnMob {:?} {},{}", mob_component, target_position.x, target_position.y);
                    let mob_uuid = Uuid::new_v4();
                    self.current_state.entities.insert(
                        mob_uuid,
                        MapEntity {
                            creation_timestamp: game_snapshot.timestamp,
                            entity_uuid: mob_uuid,
                            entity_data: MobCharacter {
                                mob_definition_id: mob_definition_id.clone(),
                                mob_component: mob_component.clone(),
                                position: target_position.clone(),
                                current_hp: mob_component.max_hp,
                                damage_dealers: HashSet::new(),
                                target_positions: TargetPositions {
                                    positions: Vec::new(),
                                    current_index: 0,
                                }
                            },
                        },
                    );
                }
                MapAction::SpawnSkill {
                    character_owner_uuid,
                    skill_definition_id,
                    skill_component,
                    target_position,
                    direction,
                } => {
                    let skill_uuid = Uuid::new_v4();
                    let (character_instance, visible_inventory, statuses) =
                        match self.current_state.entities.get(character_owner_uuid) {
                            None => return,
                            Some(entity) => match &entity.entity_data {
                                PlayerCharacter {
                                    character_instance,
                                    visible_inventory,
                                    position,
                                    statuses,
                                    ..
                                } => (
                                    character_instance.clone(),
                                    visible_inventory.clone(),
                                    statuses.clone(),
                                ),
                                _ => return,
                            },
                        };
                    if character_instance.character_db.current_hp <= 0 {
                        // Dead characters can't spawn skills
                        return;
                    }
                    let mut damage_heal_per_tick = 0.0;
                    let mut is_critical_hit = false;
                    match SYSTEMS.item_definition_lookup_service.damage_on_hit_component(&skill_definition_id) {
                        None => {}
                        Some(comp) => {
                            (damage_heal_per_tick, is_critical_hit) = get_skill_damage(
                                &visible_inventory,
                                &character_instance,
                                &CharacterDamageModifier::from_damage_component(comp),
                                &statuses,
                                &mut game_snapshot_rng,
                            );
                        }
                    };
                    match SYSTEMS.item_definition_lookup_service.heal_on_hit_component(&skill_definition_id) {
                        None => {}
                        Some(comp) => {
                            (damage_heal_per_tick, is_critical_hit) = get_skill_damage(
                                &visible_inventory,
                                &character_instance,
                                &CharacterDamageModifier::from_heal_component(comp),
                                &statuses,
                                &mut game_snapshot_rng,
                            );
                        }
                    };
                    // Remove the target position from the list of targets, so the player stops moving while doing a skill
                    match self.current_state.entities.get_mut(&character_instance.character_db.character_uuid) {
                        None => {}
                        Some(mut x) => {
                            match &mut x.entity_data {
                                PlayerCharacter { target_positions, .. } => {
                                    *target_positions = TargetPositions {
                                        positions: vec![],
                                        current_index: 0,
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    let direction = direction.normalize();
                    self.current_state.entities.insert(
                        skill_uuid,
                        MapEntity {
                            creation_timestamp: game_snapshot.timestamp,
                            entity_uuid: skill_uuid,
                            entity_data: Skill {
                                owner_uuid: character_owner_uuid.clone(),
                                skill_definition_id: skill_definition_id.clone(),
                                skill_component: skill_component.clone(),
                                damage_heal_per_tick: damage_heal_per_tick as f64,
                                is_critical_hit,
                                position: target_position.clone(),
                                direction,
                                lifetime_milliseconds: skill_component.lifetime_millis as f32,
                                remaining_time_for_damage: skill_component.start_delay_millis
                                    as f32,
                                already_affected_entities: HashSet::new(),
                            },
                        },
                    );
                }
                MapAction::PickupItem {
                    picked_items_uuid,
                    character_uuid,
                    current_character_inventory_readonly,
                } => {
                    // Check HP
                    {
                        let character = match self.current_state.entities.get(character_uuid) {
                            None => return,
                            Some(character) => character,
                        };
                        // Extract both the player's UUID and a mutable reference to the character inventory
                        let current_hp =
                            match &character.entity_data {
                                PlayerCharacter {
                                    character_instance,
                                    ..
                                } => character_instance.character_db.current_hp,
                                _ => return,
                            };
                        if current_hp <= 0 {
                            // Dead characters can't pick up items
                            return;
                        }
                    }

                    // Extract both the player's UUID and a mutable reference to the character inventory
                    let (player_uuid, mut rewindable_character_inventory) = match self.current_state.entities.get(character_uuid) {
                        None => return,
                        Some(character) => {

                            match &character.entity_data {
                                PlayerCharacter {
                                    character_instance,
                                    rewindable_character_inventory,
                                    ..
                                } => (
                                    character_instance.character_db.player_uuid.clone(),
                                    rewindable_character_inventory.clone(),
                                ),
                                _ => return,
                            }
                        },
                    };

                    // Check if it can be looted
                    let mut looted_items = vec![];
                    for loot_uuid in picked_items_uuid {
                        let (definition_id, amount, loot_entity) = match self.current_state.entities
                            .remove_if(&loot_uuid, |_, loot_entity| {
                                match loot_entity.entity_data {
                                    MapEntities::LootableItem {
                                        definition_id,
                                        owner_uuid,
                                        ..
                                    } => {
                                        // Only the owner can pick up the item
                                        owner_uuid == *character_uuid
                                        && inventory_item_utils::could_collect_item(
                                            definition_id,
                                            current_character_inventory_readonly,
                                        )
                                    }
                                    _ => return false,
                                }
                            }) {
                            None => continue,
                            Some((_, loot_entity)) => {
                                match loot_entity.entity_data {
                                    MapEntities::LootableItem {
                                        definition_id,
                                        amount,
                                        ..
                                    } => (definition_id, amount, loot_entity),
                                    _ => continue,
                                }
                            }
                        };
                        looted_items.push((loot_uuid, definition_id, amount, loot_entity));
                    }

                    if looted_items.is_empty() {
                        // Nothing to loot
                        return;
                    }

                    for (loot_uuid, definition_id, amount, loot_entity) in looted_items {
                        // Use utility to construct the InventoryItem
                        let new_item = inventory_item_utils::generate_inventory_item_for_player(
                            self.item_definition_service.clone(),
                            self.random_service.clone(),
                            player_uuid,
                            definition_id,
                            amount,
                            0.0, // TODO: get luck from equipments
                        );

                        // If the item is stackable and we had it before, sum the values
                        if let Some(_stackable) = SYSTEMS.item_definition_lookup_service.is_stackable_component(&definition_id) {
                            if let Some(existing) = rewindable_character_inventory
                                .iter_mut()
                                .find(|x| x.item_definition_id == definition_id)
                            {
                                existing.amount += amount;
                            } else {
                                // Otherwise, just add the new item to the inventory
                                rewindable_character_inventory.push(new_item);
                            }
                        } else {
                            // The item is not stackable or we didn't have it before, check for space
                            if inventory_item_utils::could_collect_item(
                                definition_id,
                                &rewindable_character_inventory,
                            ) {
                                rewindable_character_inventory.push(new_item);
                            } else {
                                self.current_state
                                    .entities
                                    .insert(*loot_uuid, loot_entity);
                                continue;
                            }
                        }
                    }
                    // Assign back the inventory to the player
                    if let Some(mut character) = self.current_state.entities.get_mut(character_uuid) {
                        match &mut character.entity_data {
                            PlayerCharacter {
                                rewindable_character_inventory: character_inventory,
                                ..
                            } => *character_inventory = rewindable_character_inventory,
                            _ => return,
                        };
                    };
                }
                MapAction::AddStatsToCharacter {
                    character_uuid,
                    stats,
                } => {
                    let mut character = match self.current_state.entities.get_mut(character_uuid) {
                        None => return,
                        Some(character) => character,
                    };
                    let (character_instance, visible_inventory) = match &mut character.entity_data {
                        PlayerCharacter {
                            character_instance,
                            visible_inventory,
                            ..
                        } => (character_instance, visible_inventory),
                        _ => return,
                    };
                    if character_instance.character_db.current_hp <= 0 {
                        return;
                    }
                    let assigned_skill_points = character_instance.character_db.strength
                        + character_instance.character_db.agility
                        + character_instance.character_db.intelligence;
                    let result = SYSTEMS.item_definition_lookup_service.ability_points_per_level_singleton();
                    let level_item_definition_id = SYSTEMS.item_definition_lookup_service.is_character_level_component_all()
                        .iter().last().unwrap().0;
                    let level_item = match visible_inventory.iter().find(|x| x.item_definition_id == *level_item_definition_id) {
                        None => {
                            eprintln!("Failed to find level item, we can't calculate how many skill points we have");
                            return;
                        }
                        Some(x) => {x}
                    };
                    let available_skill_points = (level_item.amount as i32
                        * result.points_per_level as i32)
                        - assigned_skill_points;
                    if available_skill_points < stats.1 as i32 {
                        eprintln!("Not enough skill points to add stats to character");
                        return;
                    }
                    // Apply stats to character_db
                    match stats.0 {
                        CharacterStat::Str => {
                            character_instance.character_db.strength += stats.1 as i32;
                        }
                        CharacterStat::Agi => {
                            character_instance.character_db.agility += stats.1 as i32;
                        }
                        CharacterStat::Int => {
                            character_instance.character_db.intelligence += stats.1 as i32;
                        }
                        CharacterStat::Vit => {
                            character_instance.character_db.vitality += stats.1 as i32;
                        }
                    }
                }
                MapAction::SetCharacterVisibleInventory {
                    character_uuid,
                    character_visible_inventory,
                } => match self.current_state.entities.get_mut(character_uuid) {
                    None => {}
                    Some(mut entity) => match &mut entity.entity_data {
                        PlayerCharacter {
                            visible_inventory, ..
                        } => *visible_inventory = character_visible_inventory.clone(),
                        _ => {}
                    },
                },
            }
        });
    }

    fn find_nearby_valid_nav_point(
        &self,
        center: &Position,
        max_radius: f32,
        attempts: u32,
    ) -> Option<Position> {
        let center_vec = Vec2::new(center.x, center.y);
        if self.game_map.nav_mesh.contains_point(center_vec) {
            return Some(Position {
                x: center.x,
                y: center.y,
            });
        }
        for _ in 0..attempts {
            let angle: f32 = self.random_service.random_range_f32(0.0, std::f32::consts::PI * 2.0);
            let r: f32 = self.random_service.random_range_f32(0.0, max_radius);
            let nx = center.x + angle.cos() * r;
            let ny = center.y + angle.sin() * r;
            if self.game_map.nav_mesh.contains_point(Vec2::new(nx, ny)) {
                return Some(Position { x: nx, y: ny });
            }
        }
        None
    }
}
