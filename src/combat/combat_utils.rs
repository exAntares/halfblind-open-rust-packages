use crate::characters::models::Character;
use crate::inventory::inventory_item_utils::{
    filter_equipped_items, sum_inventory_item_attributes_by_definition,
};
use crate::map::models::StatusInstance;
use proto_gen::{DamageOnHitComponent, HealOnHitComponent, InventoryItem, ItemAttributeDefinition, StatType, StatusType};
use rand::prelude::SmallRng;
use rand::Rng;

/// Calculates the damage of a skill based on the character's attributes, equipped items,
/// and skill-specific modifiers.
///
/// # Arguments
///
/// * `character_inventory` - A reference to the vector of `InventoryItem` objects representing
///   all items currently in the character's inventory.
/// * `character_instance` - A reference to the `Character` instance representing the character
///   using the skill. The `Character` contains base attribute values such as strength,
///   agility, and intelligence.
/// * `skill_component` - A reference to the `SkillComponent` instance that contains
///   the modifiers (e.g., agility, strength, and intelligence modifiers) associated with a skill.
///
/// # Returns
///
/// A `f32` value representing the total calculated damage of the skill.
pub fn get_skill_damage(
    equipped_inventory_items: &Vec<InventoryItem>,
    character_instance: &Character,
    damage_modifiers: &CharacterDamageModifier,
    statuses: &Vec<StatusInstance>,
    rng: &mut SmallRng,
) -> (f32, bool) {
    let stat_modifiers = statuses.iter().filter(|x| {
        let status_type = StatusType::try_from(x.status_component.effect_type).unwrap();
        return match status_type {
            StatusType::StatModifier => { true }
        }
    });
    let mut damage_modifiers_plain = damage_modifiers.base_damage;
    let mut damage_modifiers_percentage = 1.0; // Default to 100% damage
    let mut critical_rate_percentage = 0.0;
    let mut critical_damage_multiplier = 1.2;
    stat_modifiers.for_each(|x| {
            x.status_component.modifications.iter().for_each(|y| {
                match StatType::try_from(y.stat_type).unwrap() {
                    StatType::DamageStat => {
                        damage_modifiers_plain += y.plain;
                        damage_modifiers_percentage += (y.percentage / 100.0);
                    }
                    StatType::CriticalRate => {
                        critical_rate_percentage += (y.percentage / 100.0);
                    }
                    StatType::CriticalDamage => {
                        critical_damage_multiplier += (y.percentage / 100.0);
                    }
                };
            });
        }
    );
    let equipped_items = filter_equipped_items(equipped_inventory_items);
    let total_agility = get_total_stat_value(
        &equipped_items,
        character_instance,
        ItemAttributeDefinition::IncreaseAgility,
    );
    let total_strength = get_total_stat_value(
        &equipped_items,
        character_instance,
        ItemAttributeDefinition::IncreaseStrength,
    );
    let total_intelligence = get_total_stat_value(
        &equipped_items,
        character_instance,
        ItemAttributeDefinition::IncreaseIntelligence,
    );
    let item_weapon_damage = sum_inventory_item_attributes_by_definition(
        &equipped_items,
        ItemAttributeDefinition::IncreaseWeaponDamage,
    );
    let item_weapon_damage_percentage = sum_inventory_item_attributes_by_definition(
        &equipped_items,
        ItemAttributeDefinition::IncreaseWeaponDamagePercentage,
    );
    let weapon_damage_percentage = 1.0 + (item_weapon_damage_percentage / 100.0);
    let weapon_damage = (item_weapon_damage * weapon_damage_percentage).max(1.0);
    let stats_multiplier = (total_agility * damage_modifiers.agility_modifier)
        + (total_strength * damage_modifiers.strength_modifier)
        + (total_intelligence * damage_modifiers.intelligence_modifier);
    let damage = ((weapon_damage * stats_multiplier) + damage_modifiers_plain as f32)* damage_modifiers_percentage;
    let critical_roll = rng.random_range(0.0..1.0);
    if critical_roll < critical_rate_percentage {
        let result_critical_damage = damage * critical_damage_multiplier;
        (result_critical_damage, true)
    } else {
        (damage, false)
    }
}

pub fn get_total_stat_value(
    items: &[&InventoryItem],
    character_instance: &Character,
    attr_def: ItemAttributeDefinition,
) -> f32 {
    let item_stats = sum_inventory_item_attributes_by_definition(items, attr_def);
    let character_stats = match attr_def {
        ItemAttributeDefinition::IncreaseStrength => {
            character_instance.get_strength() as f32
        }
        ItemAttributeDefinition::IncreaseAgility => {
            character_instance.get_agility() as f32
        }
        ItemAttributeDefinition::IncreaseIntelligence => {
            character_instance.get_intelligence() as f32
        }
        _ => 0.0, // For non-stat attributes, return 0 for character base
    };
    item_stats + character_stats
}

pub struct CharacterDamageModifier {
    pub base_damage: i64,
    pub strength_modifier: f32,
    pub agility_modifier: f32,
    pub intelligence_modifier: f32,
}

impl CharacterDamageModifier {
    pub fn from_damage_component(comp: &DamageOnHitComponent) -> Self {
        Self {
            base_damage: comp.base_damage as i64,
            strength_modifier: comp.strength_modifier,
            agility_modifier: comp.agility_modifier,
            intelligence_modifier: comp.intelligence_modifier,
        }
    }

    pub fn from_heal_component(comp: &HealOnHitComponent) -> Self {
        Self {
            base_damage: comp.base_heal as i64,
            strength_modifier: comp.strength_modifier,
            agility_modifier: comp.agility_modifier,
            intelligence_modifier: comp.intelligence_modifier,
        }
    }
}