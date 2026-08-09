use crate::inventory::inventory_item_utils::{generate_inventory_item_for_player, try_aggregate_inventories};
use crate::item_definitions::ItemDefinitionLookupService;
use async_trait::async_trait;
use halfblind_protobuf_network::ErrorCode;
use halfblind_random::RandomService;
use halfblind_transactions::{DelayedReward, DelayedRewardsDatabaseInserter, TransactionResult, TransactionService, get_transaction_reward_random_value};
use proto_gen::InventoryItem;
use protobuf_itemdefinition::{ItemDefinitionRef, ItemsErrorCode, PoolWeightedItemsComponent, TransactionInstance, TransactionItem, TransactionReward, convert_transaction_consumed, convert_transaction_required_items, convert_transaction_required_not_items, convert_transaction_rewarded, convert_transaction_rewarded_random};
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

pub struct TransactionServiceImpl {
    item_definition_lookup_service: Arc<dyn ItemDefinitionLookupService + Send + Sync>,
}

impl TransactionServiceImpl {
    pub fn new(
        item_definition_lookup_service: Arc<dyn ItemDefinitionLookupService + Send + Sync>,
    ) -> Self {
        Self {
            item_definition_lookup_service 
        }
    }
}

#[async_trait]
impl TransactionService<InventoryItem> for TransactionServiceImpl {
    fn has_enough_item_definitions(
        &self,
        inventory: &Vec<InventoryItem>,
        required: &Vec<TransactionItem>,
    ) -> bool {
        for required in required {
            if required.item_instance_id != String::new() {
                if inventory
                    .iter()
                    .find(|x| x.item_instance_id == required.item_instance_id && x.amount >= required.value)
                    .is_none() {
                    return false;
                }
            } else {
                if inventory
                    .iter()
                    .find(|x| x.item_definition_id == required.id_ref.unwrap().id && x.amount >= required.value)
                    .is_none()
                {
                    return false;
                }
            }
        }
        true
    }

    fn has_any_item_definitions(
        &self,
        inventory: &Vec<InventoryItem>,
        any: &Vec<TransactionItem>,
    ) -> bool {
        for any_item in any {
            if any_item.item_instance_id != String::new() {
                if inventory
                    .iter()
                    .find(|x| x.item_instance_id == any_item.item_instance_id && x.amount >= any_item.value)
                    .is_some() {
                    return true;
                }
            } else {
                if inventory
                    .iter()
                    .find(|x| x.item_definition_id == any_item.id_ref.unwrap().id && x.amount >= any_item.value)
                    .is_some()
                {
                    return true;
                }
            }
        }
        false
    }

    fn get_instant_rewards_items_into_inventory(
        &self,
        inventory: &mut Vec<InventoryItem>,
        rewards: &Vec<TransactionReward>,
        random_service: Arc<dyn RandomService + Send + Sync>,
    ) {
        let mut new_items = Vec::new();
        for reward in rewards {
            if reward.duration > 0 {
                // TODO: Find a good way to give rewards over time too, since we can't be changing the database due lag compensation
                continue;
            }
            let new_item = generate_inventory_item_for_player(
                self.item_definition_lookup_service.clone(),
                reward.id_ref.unwrap_or_default().id,
                get_transaction_reward_random_value(random_service.clone(), reward),
            );
            new_items.push(new_item);
        }

        let unable_to_collect_items = try_aggregate_inventories(
            self.item_definition_lookup_service.clone(),
            &new_items,
            inventory,
        );
        // TODO: What to do when we can't collect items?
        eprintln!("Failed to collect items, they will disappear {:?}", unable_to_collect_items);
    }

    /// Executes a `TransactionComponent` using the player's inventory
    async fn process_inventory_transaction(
        &self,
        random_service: Arc<dyn RandomService + Send + Sync>,
        repository: &mut dyn DelayedRewardsDatabaseInserter,
        player_inventory: &mut Vec<InventoryItem>,
        player_uuid: Uuid,
        required: Option<Vec<TransactionItem>>,
        required_negative: Option<Vec<TransactionItem>>,
        consumed: Option<Vec<TransactionItem>>,
        rewarded: Option<Vec<TransactionReward>>,
        rewards_random: Option<Vec<PoolWeightedItemsComponent>>,
    ) -> Result<TransactionResult<InventoryItem>, i32> {
        { // Acquire read lock on inventory
            if let Some(required) = &required && !self.has_enough_item_definitions(player_inventory, required) {
                return Err(ItemsErrorCode::TransactionRequirementsNotMet.into());
            }

            if let Some(required_negative) = required_negative && self.has_any_item_definitions(player_inventory, &required_negative) {
                return Err(ItemsErrorCode::TransactionRequirementsNotMet.into());
            }

            if let Some(consumed) = &consumed && !self.has_enough_item_definitions(player_inventory, consumed) {
                return Err(ItemsErrorCode::NotEnoughItems.into());
            }
        } // release read lock

        let mut rewarded_items = vec![];
        // Process consumed items
        if let Some(consumed) = &consumed {
            if let Err(e) = consume_items_unchecked(player_inventory, consumed).await {
                eprintln!("error trying to consume items from player {}", e);
                return Err(ItemsErrorCode::NotEnoughItems.into());
            }
        }

        let mut immediate_items: Vec<TransactionReward> = vec![];
        let mut delayed_rewards: Vec<TransactionReward> = vec![];
        if let Some(rewarded) = rewarded {
            // For items that take 0 seconds (immediate production)
            immediate_items = rewarded
                .iter()
                .filter(|x| x.duration <= 0)
                .cloned()
                .collect::<Vec<TransactionReward>>();
            delayed_rewards = rewarded
                .iter()
                .filter(|x| x.duration > 0)
                .cloned()
                .collect::<Vec<_>>();
        }
        let mut rewards_random_tmp = vec![];
        if let Some(rewards_random) = rewards_random {
            rewards_random_tmp = rewards_random.clone();
        }
        rewarded_items = process_rewarded_items_immediate(
            self.item_definition_lookup_service.clone(),
            random_service.clone(),
            player_inventory,
            immediate_items,
            &rewards_random_tmp,
        );

        // For items that take more than 0 seconds (delayed production)
        let mut delayed_item_transactions = Vec::new();
        if !delayed_rewards.is_empty() {
            delayed_item_transactions = process_rewarded_items_delayed(
                random_service.clone(),
                repository,
                player_uuid,
                delayed_rewards,
            ).await.map_err(|e| {
                eprintln!("Error trying to process rewarded items delayed {}", e);
                ErrorCode::UnknownError as i32
            })?;
        }
        Ok(TransactionResult {
            delayed_items: delayed_item_transactions,
            inventory: player_inventory.clone(),
            rewarded: rewarded_items,
        })
    }

    async fn process_inventory_transaction_id(
        &self,
        random_service: Arc<dyn RandomService + Send + Sync>,
        repository: &mut dyn DelayedRewardsDatabaseInserter,
        player_inventory: &mut Vec<InventoryItem>,
        player_uuid: Uuid,
        transaction_id: u64,
    ) -> Result<TransactionResult<InventoryItem>, i32> {
        let random_rewarded_items: Option<Vec<PoolWeightedItemsComponent>> = convert_transaction_rewarded_random(self.item_definition_lookup_service.transaction_rewarded_items_random_component(&transaction_id))
            .map(|value| value.into_iter().filter_map(|value| {
                let pool = self.item_definition_lookup_service.pool_weighted_items_component(&value.id)?;
                Some(pool.as_ref().clone())
            }).collect::<Vec<_>>());
        self.process_inventory_transaction(
            random_service,
            repository,
            player_inventory,
            player_uuid,
            convert_transaction_required_items(self.item_definition_lookup_service.transaction_required_items_component(&transaction_id)),
            convert_transaction_required_not_items(self.item_definition_lookup_service.transaction_required_not_having_items_component(&transaction_id)),
            convert_transaction_consumed(self.item_definition_lookup_service.transaction_consumed_items_component(&transaction_id)),
            convert_transaction_rewarded(self.item_definition_lookup_service.transaction_rewarded_items_component(&transaction_id)),
            random_rewarded_items,
        ).await
    }
}

fn process_rewarded_items_immediate(
    item_definition_lookup_service: Arc<dyn ItemDefinitionLookupService + Send + Sync>,
    random_service: Arc<dyn RandomService + Send + Sync>,
    inventory_items: &mut Vec<InventoryItem>,
    rewards: Vec<TransactionReward>,
    rewards_random: &Vec<PoolWeightedItemsComponent>,
) -> Vec<InventoryItem> {
    let mut reward_inventory_items = rewards
        .iter()
        .map(|reward|{
            generate_inventory_item_for_player(
                item_definition_lookup_service.clone(),
                reward.id_ref.unwrap_or_default().id,
                get_transaction_reward_random_value(random_service.clone(), reward),
            )
        })
        .collect::<Vec<InventoryItem>>();

    for loot_bag in rewards_random {
        let mut total_weight = 0;
        loot_bag
            .weighted_rewards
            .iter()
            .for_each(|x|{
                total_weight += x.weight
            });
        let index = random_service.random_range_u32(0u32, total_weight);
        let mut current_weight = 0;
        for reward_weighted in loot_bag.weighted_rewards.iter() {
            current_weight += reward_weighted.weight;
            if index <= current_weight {
                if let Some(reward) = &reward_weighted.reward {
                    reward_inventory_items.push(
                        generate_inventory_item_for_player(
                            item_definition_lookup_service.clone(),
                            reward.id_ref.unwrap().id,
                            get_transaction_reward_random_value(random_service.clone(), reward),
                        )
                    );
                    break;
                }
            }
        }
    }
    let leftovers = try_aggregate_inventories(item_definition_lookup_service.clone(), &reward_inventory_items, inventory_items);
    reward_inventory_items
}

async fn process_rewarded_items_delayed(
    random_service: Arc<dyn RandomService + Send + Sync>,
    delayed_items_inserter: &mut dyn DelayedRewardsDatabaseInserter,
    player_uuid: Uuid,
    rewards: Vec<TransactionReward>,
) -> Result<Vec<TransactionInstance>, sqlx::Error> {
    if rewards.is_empty() {
        return Ok(Vec::new());
    }
    let now = chrono::Utc::now().naive_utc();
    let delayed_rewards: Vec<DelayedReward> = rewards
        .iter()
        .map(|r| DelayedReward {
            end_at: now + chrono::Duration::seconds(r.duration as i64),
            item_id: r.id_ref.unwrap().id as i64,
            quantity: get_transaction_reward_random_value(random_service.clone(), r) as i64,
        })
        .collect();

    let transaction_records = delayed_items_inserter
        .insert_delayed_rewards_into_database(player_uuid, delayed_rewards)
        .await?;

    let result = transaction_records
        .into_iter()
        .map(|record| TransactionInstance {
            id: record.id.to_string(),
            endtime: record.end_at.and_utc().timestamp() as u64,
            reward: Some(TransactionItem {
                id_ref: Some(ItemDefinitionRef{
                    id: record.item_id as u64,
                }),
                value: record.quantity as u64,
                item_instance_id: String::new(),
            }),
        })
        .collect();
    Ok(result)
}

async fn consume_items_unchecked(
    inventory_items: &mut Vec<InventoryItem>,
    consumed: &Vec<TransactionItem>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if consumed.is_empty() {
        return Ok(());
    }
    consume_items_from_inventory(inventory_items, consumed);
    Ok(())
}

fn consume_items_from_inventory(
    inventory: &mut Vec<InventoryItem>,
    to_consume: &Vec<TransactionItem>,
) {
    for required in to_consume {
        if required.item_instance_id != String::new() {
            for item in inventory.iter_mut() {
                if item.item_instance_id == required.item_instance_id {
                    item.amount = item.amount.saturating_sub(required.value);
                    println!("consumed item {} {} {}", item.item_instance_id, item.item_definition_id, required.value);
                    break;
                }
            }
        } else {
            for item in inventory.iter_mut() {
                if item.item_definition_id == required.id_ref.unwrap().id {
                    item.amount = item.amount.saturating_sub(required.value);
                    println!("consumed item {} {} {}", item.item_instance_id, item.item_definition_id, required.value);
                    break;
                }
            }
        }
    }
}