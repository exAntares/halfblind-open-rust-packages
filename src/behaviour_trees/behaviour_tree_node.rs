use crate::behaviour_trees::behaviour_tree_map_context::BehaviourTreeMapContext;
use crate::behaviour_trees::behaviour_tree_status::BTStatus;
use crate::behaviour_trees::utils::move_to_positions;
use crate::map::models::{MapEntities, TargetPositions};
use crate::systems::systems::SYSTEMS;
use dashmap::DashMap;
use glam::Vec2;
use proto_gen::{BtRepeatMode, Position};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Clone)]
pub enum BehaviorTreeNode {
    // Control
    Sequence {
        uuid: Uuid,
        children : Vec<BehaviorTreeNode>,
    },
    Selector{
        uuid: Uuid,
        children : Vec<BehaviorTreeNode>,
    },
    // Decorators
    Repeat{
        uuid: Uuid,
        repeat_mode: BtRepeatMode,
        count_times: u64,
        children : Vec<BehaviorTreeNode>,
    },
    // Actions
    CalculateRandomPosition(),
    MoveToRandomPosition(),
    SpawnSkillOnTarget { 
        skill_definition_id: u64,
        damage: u64,
    },
    Wait(Uuid, f32),
    // Conditions
    IsCharacterInRange(f32),
}

/// Runtime execution state for behavior tree nodes.
/// Stores progress information to resume execution across game ticks.
#[derive(Clone, Copy, Debug)]
pub enum BehaviourTreeNodeState {
    /// Sequence node state: tracks current child index (0-based)
    Sequence(usize),
    /// Repeat node state: counts execution iterations
    Repeat(u64),
    /// Wait node state: tracks remaining time in milliseconds
    Wait(f32)
}

impl BehaviorTreeNode {
    pub fn tick(&self,
                node_state: &mut DashMap<Uuid, BehaviourTreeNodeState>,
                ctx: &mut BehaviourTreeMapContext,
    ) -> BTStatus {
        match self {
            BehaviorTreeNode::Sequence {
                uuid,
                children,
            } => {
                let mut index = 0;
                if let Some(state) = node_state.get(&uuid)  {
                    if let BehaviourTreeNodeState::Sequence(child_index) = *state {
                        index = child_index
                    }
                }
                if index >= children.len() {
                    // All children have been executed, so we are done
                    node_state.insert(*uuid, BehaviourTreeNodeState::Sequence(0));
                    return BTStatus::Success;
                }
                let status = children[index].tick(node_state, ctx);
                match status {
                    BTStatus::Success => {
                        node_state.insert(*uuid, BehaviourTreeNodeState::Sequence(index + 1));
                    }
                    BTStatus::Failure => {
                        node_state.insert(*uuid, BehaviourTreeNodeState::Sequence(0));
                    }
                    BTStatus::Running => {}
                }
                status
            }
            BehaviorTreeNode::Selector {
                children,
                ..
            } => {
                for child in children {
                    let status = child.tick(node_state, ctx);
                    match status {
                        BTStatus::Success => { return BTStatus::Success; }
                        BTStatus::Failure => {}
                        BTStatus::Running => {
                            return BTStatus::Running;
                        }
                    }
                }
                BTStatus::Failure
            }
            BehaviorTreeNode::Repeat {
                uuid,
                children,
                repeat_mode,
                count_times,
            } => {
                let mut repeat_count = 0;
                if let Some(state) = node_state.get(&uuid)  {
                    if let BehaviourTreeNodeState::Repeat(count) = *state {
                        repeat_count = count
                    }
                }
                match repeat_mode {
                    BtRepeatMode::Once => {
                        if repeat_count > 1 {
                            return BTStatus::Success;
                        }
                        let status = children[0].tick(node_state, ctx);
                        match status {
                            BTStatus::Running => {
                                BTStatus::Running
                            }
                            _ => {
                                node_state.insert(*uuid, BehaviourTreeNodeState::Repeat(repeat_count + 1));
                                BTStatus::Running
                            }
                        }
                    }
                    BtRepeatMode::Forever => {
                        children[0].tick(node_state, ctx);
                        BTStatus::Running
                    }
                    BtRepeatMode::XTimes => {
                        if repeat_count >= *count_times {
                           return BTStatus::Success;
                        }
                        let status = children[0].tick(node_state, ctx);
                        match status {
                            BTStatus::Success => {
                                node_state.insert(*uuid, BehaviourTreeNodeState::Repeat(repeat_count + 1));
                                BTStatus::Running
                            }
                            BTStatus::Failure => {
                                node_state.insert(*uuid, BehaviourTreeNodeState::Repeat(repeat_count + 1));
                                BTStatus::Running
                            }
                            BTStatus::Running => {
                                BTStatus::Running
                            }
                        }
                    }
                }
            }
            &BehaviorTreeNode::CalculateRandomPosition() => {
                match ctx.entity {
                    MapEntities::MobCharacter {
                        target_positions,
                        position,
                        ..
                    } => {
                        // Start from the current mob position
                        let start = Vec2::new(position.x, position.y);

                        // Pick a halfblind-random direction and a max travel distance
                        let angle: f32 = ctx.random_service.random_range_f32(0.0, std::f32::consts::TAU);
                        let dir = Vec2::new(angle.cos(), angle.sin());
                        let max_dist: f32 = ctx.random_service.random_range_f32(2.0, 25.0); // randomized max distance within [2, 25]
                        let desired_end = start + dir * max_dist;

                        // Query navmesh to get furthest reachable point along the ray
                        let target_vec2 = match ctx.game_map.nav_mesh.intersects_line(start, desired_end) {
                            Some(hit) => {
                                // Step slightly back inside the mesh from the boundary
                                let back_off = 0.05_f32;
                                hit - dir * back_off
                            }
                            None => {
                                // The entire segment is inside mesh
                                desired_end
                            }
                        };
                        let random_pos = Position { x: target_vec2.x, y: target_vec2.y };
                        *target_positions = TargetPositions {
                            positions: vec![random_pos],
                            current_index: 0
                        };
                        BTStatus::Success
                    }
                    _ => BTStatus::Failure,
                }
            }
            &BehaviorTreeNode::MoveToRandomPosition() => {
                match ctx.entity {
                    MapEntities::MobCharacter {
                        target_positions,
                        position,
                        mob_component,
                        ..
                    } => {
                        // Simple AI behaviour
                        if target_positions.positions.len() > 0 {
                            let speed = mob_component.movement_speed as f32;
                            if move_to_positions(
                                position,
                                target_positions,
                                speed,
                                ctx.delta_time,
                            ) {
                                return BTStatus::Success
                            }
                        } else {
                            return BTStatus::Success
                        }
                    }
                    _ => return BTStatus::Failure,
                }
                BTStatus::Running
            }
            &BehaviorTreeNode::SpawnSkillOnTarget {
                skill_definition_id,
                damage,
            } => {
                // TODO: find a way to communicate between nodes, a kind of blackboard
                // IsCharacterInRange should give the uuid of the character to attack, or at least the position
                let position = match ctx.entity {
                    MapEntities::MobCharacter {
                        position,
                        target_positions,
                        ..
                    } => {
                        if target_positions.positions.len() > 0 {
                            target_positions.positions[0].clone()
                        } else {
                            position.clone()
                        }
                    }
                    _ => return BTStatus::Failure,
                };
                let skill_component = match SYSTEMS.item_definition_lookup_service.skill_component(&skill_definition_id) {
                    None => {
                        return BTStatus::Failure;
                    }
                    Some(skill_component) => {skill_component}
                };
                ctx.entities_to_add.lock().unwrap().push(MapEntities::Skill {
                    owner_uuid: ctx.entity_uuid.clone(),
                    skill_definition_id,
                    skill_component: skill_component.clone(),
                    damage_heal_per_tick: damage as f64,
                    is_critical_hit: false,
                    position: position.clone(),
                    direction: Default::default(),
                    lifetime_milliseconds: skill_component.lifetime_millis as f32,
                    remaining_time_for_damage: skill_component.start_delay_millis
                        as f32,
                    already_affected_entities: HashSet::new(),
                });
                BTStatus::Success
            }
            &BehaviorTreeNode::IsCharacterInRange(range) => {
                match ctx.entity {
                    MapEntities::MobCharacter {
                        position,
                        target_positions,
                        ..
                    } => {
                        for iter in ctx.game_state_readonly.entities.iter() {
                            match &iter.entity_data {
                                MapEntities::PlayerCharacter {
                                    character_instance,
                                    position: player_position,
                                    ..
                                } => {
                                    if character_instance.character_db.current_hp <= 0 {
                                        // Dead players should not be considered in range
                                        continue;
                                    }
                                    let player_position = *player_position;
                                    let distance = (player_position - position.clone()).length();
                                    if distance < range {
                                        *target_positions = TargetPositions {
                                            positions: vec![player_position],
                                            current_index: 0
                                        };
                                        return BTStatus::Success;
                                    }
                                }
                                _ => continue,
                            }
                        }
                    }
                    _ => return BTStatus::Failure,
                }
                BTStatus::Failure
            },
            &BehaviorTreeNode::Wait(uuid, duration_milliseconds) => {
                let mut remaining_duration = duration_milliseconds;
                if let Some(state) = node_state.get(&uuid)  {
                    if let BehaviourTreeNodeState::Wait(remaining) = *state {
                        remaining_duration = remaining
                    }
                }
                if remaining_duration <= 0.0 {
                    // Reset wait time for next iterations
                    node_state.insert(uuid, BehaviourTreeNodeState::Wait(duration_milliseconds));
                    return BTStatus::Success;
                }
                let new_remaining_duration = remaining_duration - ctx.delta_time * 1000.0;
                node_state.insert(uuid, BehaviourTreeNodeState::Wait(new_remaining_duration));
                BTStatus::Running
            }
        }
    }
}
