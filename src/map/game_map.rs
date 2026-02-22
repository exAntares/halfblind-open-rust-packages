use crate::map::models::{GameState, MapActionTimed};
use crate::nav_mesh::navmesh::NavMesh;
use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use halfblind_network::*;
use proto_gen::MapComponent;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

// Structure to hold player positions for each map
pub struct GameMap {
    pub map_id: u64,
    pub map_component: MapComponent,
    pub actions_queue: Arc<SegQueue<MapActionTimed>>,
    pub characters_by_player: DashMap<Uuid, Vec<Uuid>>, // <player_uuid, Vec[character_uuid]>
    pub player_by_character: DashMap<Uuid, Uuid>,       // <character_uuid, player_uuid>
    pub broadcast: DashMap<Uuid, Arc<ConnectionContext>>,
    pub nav_mesh: NavMesh,
    pub last_known_game_state: Arc<RwLock<GameState>>,
}

impl GameMap {
    pub fn new(map_id: u64, map_component: MapComponent) -> GameMap {
        // Build read-only navmesh from provided map data
        let nav_mesh = match map_component.map_data.as_ref() {
            Some(md) => NavMesh::from_mesh_data(md),
            None => NavMesh::empty(),
        };
        GameMap {
            map_id,
            map_component,
            actions_queue: Arc::new(SegQueue::new()),
            characters_by_player: DashMap::new(),
            player_by_character: DashMap::new(),
            broadcast: DashMap::new(),
            last_known_game_state: Arc::new(RwLock::new(GameState {
                entities: Default::default(),
                entities_damage: Default::default(),
                behaviour_tree_node_states_by_entity: Default::default(),
            })),
            nav_mesh,
        }
    }

    pub fn push_action(&self, action: MapActionTimed) {
        self.actions_queue.push(action);
    }
}
