use crate::behaviour_trees::behaviour_tree_node::BehaviorTreeNode;
use dashmap::DashMap;
use halfblind_protobuf::get_type_url;
use once_cell::sync::Lazy;
use prost::Message;
use proto_gen::{BehaviourTreeComponent, BtNode, BtNodeCalculateRandomPosition, BtNodeIsCharacterInRange, BtNodeMoveToRandomLocation, BtNodeRepeat, BtNodeSelector, BtNodeSequence, BtNodeSpawnSkillOnTarget, BtNodeWait, BtRepeatMode};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

static BEHAVIOUR_TREES_MAP : Lazy<DashMap<u64, Arc<BehaviorTreeNode>>> = Lazy::new(|| DashMap::new());

pub fn get_behavior_tree_node(definition_id: u64, component: &BehaviourTreeComponent) -> Option<Arc<BehaviorTreeNode>> {
    match BEHAVIOUR_TREES_MAP.get(&definition_id) {
        None => {
            match &component.start_node {
                None => {
                    None
                }
                Some(node) => {
                    let root_node = convert_proto_to_node(node.clone())?;
                    let arc = Arc::new(root_node);
                    BEHAVIOUR_TREES_MAP.insert(definition_id, arc.clone());
                    Some(arc.clone())
                }
            }
        }
        Some(x) => {
            Some(x.clone())
        }
    }
}

fn convert_proto_to_node(proto_node: BtNode) -> Option<BehaviorTreeNode> {
    let type_url = proto_node.any_node.clone()?.type_url;
    match DECODERS.get(&type_url.to_lowercase()) {
        None => {
            eprintln!("Unknown node type please add the decoder `behaviour_tree_decoder register_all`: {}", type_url);
            None
        }
        Some(decoder) => {
            decoder(proto_node.any_node.unwrap())
        }
    }
}

fn convert_children(
    children: Vec<BtNode>
) -> Vec<BehaviorTreeNode> {
    let x: Vec<BehaviorTreeNode> = children
        .into_iter()
        .map(convert_proto_to_node)
        .filter(|x|x.is_some())
        .map(|x|x.unwrap())
        .collect();
    x
}

type DecodeFn = fn(prost_types::Any) -> Option<BehaviorTreeNode>;

static DECODERS : Lazy<HashMap<String, DecodeFn>> = Lazy::new(||{
    let mut result = HashMap::new();
    register_all(&mut result);
    result
});

fn register_all(result: &mut HashMap<String, DecodeFn>) {
    result.insert(get_type_url::<BtNodeRepeat>().to_lowercase(), |any| {
        let root_node = BtNodeRepeat::decode(any.value.as_slice()).unwrap();
        let children = match convert_proto_to_node(root_node.child.unwrap()) {
            None => vec![],
            Some(child) => vec![child],
        };
        Some(BehaviorTreeNode::Repeat {
            uuid: Uuid::new_v4(),
            repeat_mode: BtRepeatMode::try_from(root_node.repeat).unwrap_or(BtRepeatMode::Forever),
            children,
            count_times: root_node.count_times as u64,
        })
    });

    result.insert(get_type_url::<BtNodeSequence>().to_lowercase(), |any| {
        let root_node = BtNodeSequence::decode(any.value.as_slice()).unwrap();
        let x: Vec<BehaviorTreeNode> = convert_children(root_node.children);
        Some(BehaviorTreeNode::Sequence {
            uuid: Uuid::new_v4(),
            children: x,
        })
    });

    result.insert(get_type_url::<BtNodeSelector>().to_lowercase(), |any| {
        let root_node = BtNodeSelector::decode(any.value.as_slice()).unwrap();
        let x: Vec<BehaviorTreeNode> = convert_children(root_node.children);
        Some(BehaviorTreeNode::Selector {
            uuid: Uuid::new_v4(),
            children: x,
        })
    });

    result.insert(get_type_url::<BtNodeMoveToRandomLocation>().to_lowercase(), |any| {
        Some(BehaviorTreeNode::MoveToRandomPosition())
    });

    result.insert(get_type_url::<BtNodeCalculateRandomPosition>().to_lowercase(), |any| {
        Some(BehaviorTreeNode::CalculateRandomPosition())
    });

    result.insert(get_type_url::<BtNodeSpawnSkillOnTarget>().to_lowercase(), |any| {
        let root_node = BtNodeSpawnSkillOnTarget::decode(any.value.as_slice()).unwrap();
        Some(BehaviorTreeNode::SpawnSkillOnTarget{
            skill_definition_id: root_node.skill_definition.unwrap().id,
            damage: root_node.override_damage,
        })
    });

    result.insert(get_type_url::<BtNodeIsCharacterInRange>().to_lowercase(), |any| {
        let root_node = BtNodeIsCharacterInRange::decode(any.value.as_slice()).unwrap();
        Some(BehaviorTreeNode::IsCharacterInRange(root_node.range))
    });

    result.insert(get_type_url::<BtNodeWait>().to_lowercase(), |any| {
        let root_node = BtNodeWait::decode(any.value.as_slice()).unwrap();
        Some(BehaviorTreeNode::Wait(Uuid::new_v4(), root_node.duration_milliseconds))
    });
}