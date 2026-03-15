use crate::item_definitions::INVENTORY_HIDDEN_ITEM_COMPONENT_LOOKUP;
use crate::systems::systems::SYSTEMS;
use halfblind_itemdefinitions_service::ItemDefinitionsService;
use halfblind_random::RandomService;
use once_cell::sync::Lazy;
use prost::Message;
use proto_gen::{InventoryItem, ItemAttributeDefinition};
use std::cmp::max;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

pub fn generate_inventory_item_for_player_default(
    player_uuid: Uuid,
    definition_id: u64,
    amount: u64,
) -> InventoryItem {
    let total_luck_percentage = 0.0;
    generate_inventory_item_for_player(
        SYSTEMS.items_definitions_service.clone(),
        SYSTEMS.random_service.clone(),
        player_uuid,
        definition_id,
        amount,
        total_luck_percentage)
}

// Utility to construct an InventoryItem for a pickup action.
// It generates inventory items using the luck accumulated by
// the character.
pub fn generate_inventory_item_for_player(
    item_definitions_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
    random_service: Arc<dyn RandomService + Send + Sync>,
    player_uuid: Uuid,
    definition_id: u64,
    amount: u64,
    total_luck_percentage: f32,
) -> InventoryItem {
    let _item_definition =
        item_definitions_service.get_item_definition_for_player(player_uuid, definition_id);
    if _item_definition.is_none() {
        // I honestly don't know what to do in this case :)
        return InventoryItem {
            item_instance_id: Uuid::new_v4().to_string(),
            item_definition_id: definition_id,
            amount,
            is_equipped: false,
            attributes: Vec::new(),
        };
    }

    let _item_definition = _item_definition.unwrap();

    if SYSTEMS.item_definition_lookup_service.is_stackable_component(&definition_id).is_some() {
        return InventoryItem {
            item_instance_id: Uuid::new_v4().to_string(),
            item_definition_id: definition_id,
            amount,
            is_equipped: false,
            attributes: Vec::new(),
        };
    }

    InventoryItem {
        item_instance_id: Uuid::new_v4().to_string(),
        item_definition_id: definition_id,
        amount,
        is_equipped: false,
        attributes: Vec::new(),
    }
}

/// Attempts to aggregate inventory items from a source collection into a target inventory.
///
/// This function processes items from the source inventory and tries to merge them into the
/// target inventory based on item stackability and available inventory space. Items that
/// cannot be collected are returned as a separate collection.
///
/// # Arguments
///
/// * `source` - Vector of inventory items to be added to the target inventory
/// * `target` - Mutable reference to the target inventory where items will be aggregated
///
/// # Returns
///
/// A `Vec<InventoryItem>` containing items that could not be collected due to inventory
/// constraints or lack of space.
///
/// # Behavior
///
/// For each item in the source:
/// - If the item is stackable and already exists in the target inventory, the amounts are combined
/// - If the item is stackable but doesn't exist in the target, it's added as a new entry
/// - If the item is not stackable, it checks if there's space available before adding
/// - Items that cannot be collected due to space constraints are added to the return collection
///
/// # Example
///
/// ```rust
/// let mut player_inventory = vec![/* existing items */];
/// let items_to_add = vec![/* new items */];
///
/// let uncollectable = try_aggregate_inventories(
///     items_to_add,
///     &mut player_inventory
/// );
///
/// // uncollectable contains items that couldn't be added due to space/constraints
/// ```
pub fn try_aggregate_inventories(
    source: Vec<InventoryItem>,
    target: &mut Vec<InventoryItem>,
) -> Vec<InventoryItem> {
    let mut unable_to_collect_items = Vec::new();
    for to_add in source {
        if let Some(_stackable) = SYSTEMS.item_definition_lookup_service.is_stackable_component(&to_add.item_definition_id) {
            if let Some(existing) = target
                .iter_mut()
                .find(|x| x.item_definition_id == to_add.item_definition_id)
            {
                existing.amount += to_add.amount;
            } else {
                // Otherwise, just add the new item to the inventory
                target.push(to_add);
            }
        } else {
            // The item is not stackable, or we didn't have it before, check for space
            if could_collect_item(
                to_add.item_definition_id,
                target,
            ) {
                target.push(to_add);
            } else {
                unable_to_collect_items.push(to_add);
            }
        }
    }
    unable_to_collect_items
}

/// Filters the provided inventory items, keeping only those whose item definition
/// contains a component that matches the desired trait/component type `M`.
/// Example usages:
///  - filter_inventory_items_by_component(&items, &IsStackableLookup);
///  - filter_inventory_items_by_component(&items, &IsEquippedLookup);
///  - filter_inventory_items_by_component(&items, &InventoryHiddenItemComponentLookup);
pub fn filter_inventory_items_by_component<M>(
    items: &[InventoryItem],
    lookup: &Lazy<HashMap<u64, Arc<M>>>,
) -> Vec<InventoryItem>
where
    M: Clone + Message + Default + Send + Sync + 'static,
{
    items
        .iter()
        .cloned()
        .filter(|item| lookup.get(&item.item_definition_id).is_some())
        .collect()
}

/// Filters the inventory items to return only the visible items.
/// Which are the Equipped inventory items plus character level
pub fn filter_visible_inventory(
    items: &[InventoryItem]
) -> Vec<&InventoryItem> {
    let mut result = filter_equipped_or_unequipped_items(items, true);
    let level_item_definition_id = SYSTEMS.item_definition_lookup_service.is_character_level_component_all()
        .iter().last().unwrap().0;
    match items.iter().find(|x| x.item_definition_id == *level_item_definition_id) {
        None => {}
        Some(x) => {result.push(x);}
    };
    result
}

/// Filters the inventory items to return only the equipped items.
///
/// # Parameters
/// - `items`: A slice of `InventoryItem` that represents the player's inventory.
///
/// # Returns
/// - A `Vec<InventoryItem>` containing only the items that are equipped.
pub fn filter_equipped_items(items: &[InventoryItem]) -> Vec<&InventoryItem> {
    filter_equipped_or_unequipped_items(items, true)
}

/// Filters a list of inventory items based on their equipped status.
///
/// This function takes a slice of `InventoryItem` and a boolean flag
/// that indicates whether to filter for equipped or unequipped items.
/// It returns a new `Vec<InventoryItem>` containing only the items
/// that match the specified equipped status.
///
/// # Arguments
///
/// * `items` - A slice of `InventoryItem` representing the inventory to be filtered.
/// * `is_equipped` - A boolean flag indicating the filter condition:
///   * `true` to filter and return only equipped items.
///   * `false` to filter and return only unequipped items.
///
/// # Returns
///
/// A vector containing the filtered `InventoryItem` objects that match
/// the specified equipped status.
pub fn filter_equipped_or_unequipped_items(
    items: &[InventoryItem],
    is_equipped: bool,
) -> Vec<&InventoryItem> {
    items
        .iter()
        .filter(|item| item.is_equipped == is_equipped)
        .collect()
}

/// Sums the attribute values across the provided inventory items for a given attribute definition.
/// It inspects each InventoryItem.attributes and accumulates the attr_value where the
/// attr_definition matches the provided ItemAttributeDefinition.
pub fn sum_inventory_item_attributes_by_definition(
    items: &[&InventoryItem],
    attr_def: ItemAttributeDefinition,
) -> f32 {
    items
        .iter()
        .flat_map(|it| it.attributes.iter())
        .filter(|attr| attr.attr_definition == attr_def as i32)
        .map(|attr| attr.attr_value)
        .sum()
}

pub fn could_collect_item(
    item_definition_id: u64,
    character_inventory_items: &[InventoryItem],
) -> bool {
    // Check if the _item_to_collect_definition has a component of type InventoryHiddenItemComponent
    if SYSTEMS.item_definition_lookup_service.inventory_hidden_item_component(&item_definition_id)
        .is_some()
    {
        // Hidden items can always be collected (they don't count toward inventory limits)
        return true;
    }

    let max_visible_inventory_capacity = SYSTEMS.item_definition_lookup_service.maximum_visible_inventory_capacity_singleton().capacity;
    // Check if the item is stackable, if so, check if we already have it in the inventory
    if SYSTEMS.item_definition_lookup_service.is_stackable_component(&item_definition_id).is_some() {
        // Check if we have it in the inventory already
        for item in character_inventory_items {
            if item.item_definition_id == item_definition_id {
                return true;
            }
        }
    }

    let visible_item_count = count_visible_inventory_items(character_inventory_items.as_ref())
        - filter_equipped_or_unequipped_items(character_inventory_items, true).iter().count() as u32;
    (visible_item_count + 1u32) < max_visible_inventory_capacity
}

/// Counts the visible inventory items from the given list of items
pub fn count_visible_inventory_items(items: &[InventoryItem]) -> u32 {
    let item_count = items.len() as u32;
    let hidden_item_count =
        filter_inventory_items_by_component(&items, &INVENTORY_HIDDEN_ITEM_COMPONENT_LOOKUP).len() as u32;
    max(0u32, item_count - hidden_item_count)
}

/// Selects an index based on cumulative weights. The higher the adjusted RNG value,
/// the more likely later (more desirable) items are picked. The final roll is clamped
/// to the total weight sum boundary.
pub fn select_weighted_index(
    random_service: Arc<dyn RandomService + Send + Sync>,
    weights: &[u32],
    rng_multiplier: f32,
) -> Option<usize> {
    if weights.is_empty() {
        return None;
    }
    let total_weight: u32 = weights.iter().copied().sum();
    if total_weight == 0 {
        return None;
    }
    // Base roll in [0, total_weight)
    let base_roll: u32 = random_service.random_range_u32(0, total_weight);
    // Apply multiplier and clamp (favor higher rolls when multiplier > 1)
    let mut adjusted = (base_roll as f32 * rng_multiplier) as i64;
    let max_index = (total_weight - 1) as i64;
    if adjusted < 0 {
        adjusted = 0;
    }
    if adjusted > max_index {
        adjusted = max_index;
    }

    let mut remaining = adjusted as u32;
    for (idx, &w) in weights.iter().enumerate() {
        if remaining < w {
            return Some(idx);
        }
        remaining = remaining.saturating_sub(w);
    }
    None
}
