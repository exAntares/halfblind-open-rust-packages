use crate::behaviour_trees::behaviour_tree_node::BehaviourTreeNodeState;
use crate::characters::models::Character;
use dashmap::DashMap;
use proto_gen::{CharacterStat, DamageType, InventoryItem, Position};
use proto_gen::{MobComponent, SkillComponent};
use rand::rngs::SmallRng;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct GameState {
    pub entities: DashMap<Uuid, MapEntity>,
    pub entities_damage: DashMap<Uuid, DamageEntity>, // all the damages that happened
    pub behaviour_tree_node_states_by_entity: DashMap<Uuid, DashMap<Uuid, BehaviourTreeNodeState>>
}

#[derive(Clone, Debug, Default)]
pub struct TargetPositions {
    pub positions: Vec<Position>,
    pub current_index: usize,
}

#[derive(Clone)]
pub struct GameSnapshot {
    pub rng: SmallRng, // Snapshots require having their own RNG since the snapshot needs to be deterministic
    pub state: GameState,
    pub timestamp: u64,
    pub pending_actions: Vec<MapActionTimed>,
    pub dirty: bool,
}

#[derive(Clone, Debug)]
pub struct MapActionTimed {
    pub timestamp: u64,
    pub action: MapAction,
}

#[derive(Clone, Debug)]
pub enum MapAction {
    SpawnCharacter {
        character_instance: Character,
        character_visible_inventory: Vec<InventoryItem>,
    },
    RemoveCharacters {
        player_uuid: Uuid,
        characters_uuid: Vec<Uuid>,
    },
    MoveTo {
        entity_uuid: Uuid,
        target_positions: Vec<Position>,
    },
    SpawnMob {
        mob_definition_id: u64,
        mob_component: Arc<MobComponent>,
        target_position: Position,
    },
    SpawnSkill {
        character_owner_uuid: Uuid,
        skill_definition_id: u64,
        skill_component: Arc<SkillComponent>,
        target_position: Position,
        direction: Position,
    },
    PickupItem {
        picked_items_uuid: HashSet<Uuid>,
        character_uuid: Uuid,
        current_character_inventory_readonly: Vec<InventoryItem>,
    },
    AddStatsToCharacter {
        character_uuid: Uuid,
        stats: (CharacterStat, u32),
    },
    SetCharacterVisibleInventory {
        character_uuid: Uuid,
        character_visible_inventory: Vec<InventoryItem>,
    },
    // Add other actions as needed
}

#[derive(Clone, Debug)]
pub struct MapEntity {
    pub entity_uuid: Uuid,
    pub creation_timestamp: u64,
    pub entity_data: MapEntities,
}

#[derive(Clone, Debug)]
pub struct DamageEntity {
    pub entity_uuid: Uuid,
    pub creation_timestamp: u64,
    pub damage_owner_uuid: Uuid,  // Who made the damage
    pub damage_target_uuid: Uuid, // Who got the damage
    pub damage_amount: u64,
    pub is_critical_hit: bool,
    pub damage_target_position: Position,
    pub duration_millis: f32,
    pub damage_type: DamageType,
}

#[derive(Clone, Debug)]
pub struct StatusInstance {
    pub definition_id: u64,
    pub lifetime: f64,
    pub status_component: Arc<proto_gen::StatusOnHitComponent>,
}

#[derive(Clone, Debug)]
pub enum MapEntities {
    PlayerCharacter {
        character_instance: Character,
        rewindable_character_inventory: Vec<InventoryItem>, // Items received from the map, yet to be merged with the inventory of the player
        visible_inventory: Vec<InventoryItem>,
        position: Position,
        target_positions: TargetPositions,
        statuses: Vec<StatusInstance>,
    },
    MobCharacter {
        mob_definition_id: u64,
        mob_component: Arc<MobComponent>,
        position: Position,
        current_hp: u64,
        damage_dealers: HashSet<Uuid>,
        target_positions: TargetPositions,
    },
    Skill {
        owner_uuid: Uuid,
        skill_definition_id: u64,
        skill_component: Arc<SkillComponent>,
        damage_heal_per_tick: f64,
        is_critical_hit: bool,
        position: Position,
        direction: Position,
        lifetime_milliseconds: f32,
        remaining_time_for_damage: f32,
        already_affected_entities: HashSet<Uuid>,
    },
    LootableItem {
        owner_uuid: Uuid, // Who can pick up the item
        definition_id: u64,
        position: Position,
        amount: u64,
    },
    // Add others as needed
}
