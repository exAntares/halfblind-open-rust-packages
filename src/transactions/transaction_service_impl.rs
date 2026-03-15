use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use chrono::NaiveDateTime;
use halfblind_database_service::DatabaseService;
use halfblind_inventory_service::InventoryService;
use halfblind_protobuf_network::ErrorCode;
use halfblind_random::RandomService;
use halfblind_transactions::{get_transaction_reward_random_value, TransactionRecord, TransactionResult, TransactionService};
use proto_gen::InventoryItem;
use protobuf_itemdefinition::{convert_transaction_consumed, convert_transaction_required_items, convert_transaction_required_not_items, convert_transaction_rewarded, convert_transaction_rewarded_random, ItemDefinitionRef, ItemsErrorCode, PoolWeightedItemsComponent, TransactionInstance, TransactionItem, TransactionReward};
use sqlx::{Postgres, Transaction};
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Default)]
pub struct TransactionServiceImpl;

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
        player_uuid: Uuid,
        inventory: &mut Vec<InventoryItem>,
        rewards: &Vec<TransactionReward>,
        inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
        random_service: Arc<dyn RandomService + Send + Sync>,
    ) {
        let mut new_items = Vec::new();
        for reward in rewards {
            if reward.duration > 0 {
                // TODO: Find a good way to give rewards over time too, since we can't be changing the database due lag compensation
                continue;
            }
            let new_item = inventory_service.generate_inventory_item_for_player(
                player_uuid,
                reward.id_ref.unwrap().id,
                get_transaction_reward_random_value(random_service.clone(), reward),
            );
            new_items.push(new_item);
        }

        let unable_to_collect_items = inventory_service.try_aggregate_inventories(
            new_items.clone(),
            inventory,
        );
        // TODO: What to do when we can't collect items?
        eprintln!("Failed to collect items, they will disappear {:?}", unable_to_collect_items);
    }

    async fn process_player_transaction_id(
        &self,
        inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
        database_service: Arc<dyn DatabaseService + Send + Sync>,
        random_service: Arc<dyn RandomService + Send + Sync>,
        player_uuid: Uuid,
        secondary_uuid: Uuid,
        transaction_id: u64,
    ) -> Result<TransactionResult<InventoryItem>, i32> {
        let random_rewarded_items: Option<Vec<PoolWeightedItemsComponent>> = convert_transaction_rewarded_random(SYSTEMS.item_definition_lookup_service.transaction_rewarded_items_random_component(&transaction_id))
            .map(|value| value.into_iter().filter_map(|value| {
                let pool = SYSTEMS.item_definition_lookup_service.pool_weighted_items_component(&value.id)?;
                Some(pool.as_ref().clone())
            }).collect::<Vec<_>>());
        self.process_player_transaction(
            inventory_service,
            database_service,
            random_service,
            player_uuid,
            secondary_uuid,
            convert_transaction_required_items(SYSTEMS.item_definition_lookup_service.transaction_required_items_component(&transaction_id)),
            convert_transaction_required_not_items(SYSTEMS.item_definition_lookup_service.transaction_required_not_having_items_component(&transaction_id)),
            convert_transaction_consumed(SYSTEMS.item_definition_lookup_service.transaction_consumed_items_component(&transaction_id)),
            convert_transaction_rewarded(SYSTEMS.item_definition_lookup_service.transaction_rewarded_items_component(&transaction_id)),
            random_rewarded_items,
        ).await
    }

    /// Executes a `TransactionComponent` using the player's inventory
    async fn process_player_transaction(
        &self,
        inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
        database_service: Arc<dyn DatabaseService + Send + Sync>,
        random_service: Arc<dyn RandomService + Send + Sync>,
        player_uuid: Uuid,
        secondary_uuid: Uuid,
        required: Option<Vec<TransactionItem>>,
        required_negative: Option<Vec<TransactionItem>>,
        consumed: Option<Vec<TransactionItem>>,
        rewarded: Option<Vec<TransactionReward>>,
        rewards_random: Option<Vec<protobuf_itemdefinition::PoolWeightedItemsComponent>>,
    ) -> Result<TransactionResult<InventoryItem>, i32> {
        let inventory_arc = match inventory_service.get_inventory(player_uuid, secondary_uuid).await {
            Ok(inventory) => inventory,
            Err(e) => {
                eprintln!("error trying to get items from player {}", e);
                return Err(ErrorCode::UnknownError.into());
            }
        };

        { // Acquire read lock on inventory
            let player_inventory = inventory_arc.read().await;

            if let Some(required) = &required && !self.has_enough_item_definitions(&player_inventory, required) {
                return Err(ItemsErrorCode::TransactionRequirementsNotMet.into());
            }

            if let Some(required_negative) = required_negative && self.has_any_item_definitions(&player_inventory, &required_negative) {
                return Err(ItemsErrorCode::TransactionRequirementsNotMet.into());
            }

            if let Some(consumed) = &consumed && !self.has_enough_item_definitions(&player_inventory, consumed) {
                return Err(ItemsErrorCode::NotEnoughItems.into());
            }
        } // release read lock

        let mut rewarded_items = vec![];
        let mut transaction_instance_id = Vec::new();
        { // Acquire write lock on inventory
            let mut player_inventory = inventory_arc.write().await;
            // Process consumed items
            if let Some(consumed) = &consumed {
                if let Err(e) = consume_items_unchecked(&mut player_inventory, consumed).await {
                    eprintln!("error trying to consume items from player {}", e);
                    return Err(ItemsErrorCode::NotEnoughItems.into());
                }
            }

            let mut immediate_items= vec![];
            let mut delayed_items = vec![];
            if let Some(rewarded) = &rewarded {
                // For items that take 0 seconds (immediate production)
                immediate_items = rewarded
                    .iter()
                    .filter(|x| x.duration <= 0)
                    .collect::<Vec<_>>();
                delayed_items = rewarded
                    .iter()
                    .filter(|x| x.duration > 0)
                    .collect::<Vec<_>>();
            }
            let mut rewards_random_tmp = vec![];
            if let Some(rewards_random) = rewards_random {
                rewards_random_tmp = rewards_random.clone();
            }
            {
                rewarded_items = match process_rewarded_items_immediate(
                    inventory_service.clone(),
                    random_service.clone(),
                    &mut player_inventory,
                    player_uuid,
                    immediate_items,
                    &rewards_random_tmp,
                ).await {
                    Ok(x) => {x}
                    Err(e) => {
                        eprintln!("error trying to process_rewarded_items_immediate {}", e);
                        return Err(ErrorCode::UnknownError.into());
                    }
                };
            }

            // For items that take more than 0 seconds (delayed production)
            if !delayed_items.is_empty()
            {  // Acquire write lock on the database
                let mut tx = database_service.get_db_pool().begin().await.ok().unwrap();
                transaction_instance_id = match process_rewarded_items_delayed(random_service.clone(), &mut tx, player_uuid, delayed_items).await {
                    Ok(x) => {x}
                    Err(e) => {
                        eprintln!("error trying to process_rewarded_items_delayed {}", e);
                        return Err(ErrorCode::UnknownError.into());
                    }
                };
                // Commit transaction
                match tx.commit().await {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("error trying to get update items in database {}", e);
                        return Err(ErrorCode::UnknownError.into());
                    }
                };
            }// release database write lock
        } // release inventory write lock
        // Get updated inventory
        let updated_inventory = match inventory_service.get_inventory(player_uuid, secondary_uuid).await {
            Ok(inventory) => inventory,
            Err(e) => {
                eprintln!("error trying to get items from player db {}", e);
                return Err(ErrorCode::UnknownError.into());
            }
        };
        Ok(TransactionResult {
            transaction_instance_id,
            inventory: updated_inventory.read().await.clone(),
            rewarded: rewarded_items,
        })
    }
}


async fn process_rewarded_items_immediate(
    inventory_service: Arc<dyn InventoryService<InventoryItem> + Send + Sync>,
    random_service: Arc<dyn RandomService + Send + Sync>,
    inventory_items: &mut Vec<InventoryItem>,
    player_uuid: Uuid,
    rewards: Vec<&TransactionReward>,
    rewards_random: &Vec<PoolWeightedItemsComponent>,
) -> Result<Vec<InventoryItem>, Box<dyn Error + Send + Sync>> {
    let mut reward_inventory_items = rewards
        .iter()
        .map(|reward|{
            inventory_service.generate_inventory_item_for_player(
                player_uuid,
                reward.id_ref.unwrap().id,
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
                        inventory_service.generate_inventory_item_for_player(
                            player_uuid,
                            reward.id_ref.unwrap().id,
                            get_transaction_reward_random_value(random_service.clone(), reward),
                        )
                    );
                    break;
                }
            }
        }
    }
    let leftovers = inventory_service.try_aggregate_inventories(reward_inventory_items.clone(), inventory_items);
    Ok(reward_inventory_items)
}

async fn process_rewarded_items(
    random_service: Arc<dyn RandomService + Send + Sync>,
    tx: &mut Transaction<'_, Postgres>,
    player_uuid: Uuid,
    delayed_items: Vec<&TransactionReward>,
) -> Result<Vec<TransactionInstance>, Box<dyn Error + Send + Sync>> {
    let transactions_ids = process_rewarded_items_delayed(random_service, tx, player_uuid, delayed_items).await?;
    Ok(transactions_ids)
}

async fn process_rewarded_items_delayed(
    random_service: Arc<dyn RandomService + Send + Sync>,
    tx: &mut Transaction<'_, Postgres>,
    player_uuid: Uuid,
    rewards: Vec<&TransactionReward>,
) -> Result<Vec<TransactionInstance>, Box<dyn Error + Send + Sync>> {
    if rewards.is_empty() {
        return Ok(Vec::new());
    }
    let now = chrono::Utc::now().naive_utc();
    let end_times: Vec<NaiveDateTime> = rewards
        .iter()
        .map(|r| now + chrono::Duration::seconds(r.duration as i64))
        .collect();

    let transaction_records = sqlx::query_as::<_, TransactionRecord>(
        r#"
        WITH inserted AS (
            INSERT INTO player_transactions (player_uuid, end_at, item_id, quantity)
            SELECT $1,
                   unnest($2::timestamp[]),
                   unnest($3::bigint[]),
                   unnest($4::bigint[])
            RETURNING id, end_at, item_id, quantity
        )
        SELECT
            inserted.id,
            inserted.end_at,
            inserted.item_id,
            inserted.quantity
        FROM inserted
        "#,
    )
        .bind(player_uuid)
        .bind(&end_times)
        .bind(
            &rewards
                .iter()
                .map(|r| r.id_ref.unwrap().id as i64)
                .collect::<Vec<i64>>(),
        )
        .bind(&rewards.iter().map(|r| {
            get_transaction_reward_random_value(random_service.clone(), r) as i64
        }).collect::<Vec<i64>>())
        .fetch_all(&mut **tx)
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