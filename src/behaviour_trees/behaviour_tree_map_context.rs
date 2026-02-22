use crate::map::game_map::GameMap;
use crate::map::models::{GameState, MapEntities};
use halfblind_random::RandomService;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// The Context passed to every node.
// It provides read access to the World and Write access to the specific Mob.
pub struct BehaviourTreeMapContext<'a> {
    pub entity_uuid: Uuid,
    pub entity: &'a mut MapEntities, // Mutable access to the mob itself
    pub delta_time: f32,
    pub random_service: Arc<dyn RandomService + Send + Sync>,
    pub game_map: Arc<GameMap>,
    pub game_state_readonly: &'a GameState,
    pub entities_to_add: &'a Mutex<Vec<MapEntities>>,
}
