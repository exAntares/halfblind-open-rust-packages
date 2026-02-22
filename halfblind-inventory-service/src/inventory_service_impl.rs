use crate::models::PlayerItem;
use crate::InventoryService;
use async_trait::async_trait;
use dashmap::DashMap;
use halfblind_database_service::DatabaseService;
use protobuf_itemdefinition::InventoryItem;
use protobuf_itemdefinition::InventoryItemAttribute;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[async_trait]
impl InventoryService for InventoryServiceImpl {
    async fn get_player_inventory(
        &self,
        player_uuid: Uuid,
    ) -> Result<Arc<RwLock<Vec<InventoryItem>>>, sqlx::Error> {
        self.get_inventory(player_uuid, player_uuid).await
    }

    async fn get_inventory(
        &self,
        player_uuid: Uuid,
        secondary_uuid: Uuid,
    ) -> Result<Arc<RwLock<Vec<InventoryItem>>>, sqlx::Error> {
        self.get_inventory(player_uuid, secondary_uuid).await
    }

    async fn get_definition_value_summed(
        &self,
        player_uuid: Uuid,
        owner_uuid: Uuid,
        item_definition_id: u64,
    ) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        let inventory_lock = self.get_inventory(player_uuid, owner_uuid).await?;
        let inventory = inventory_lock.read().await;
        let mut sum = 0i64;
        for item in inventory.iter() {
            if item.item_definition_id == item_definition_id {
                sum += item.amount as i64;
            }
        }
        Ok(sum)
    }

    async fn save_character_inventory(
        &self,
        player_uuid: Uuid,
        character_uuid: Uuid,
    ) -> Result<(), sqlx::Error> {
        let db_pool = self.database_service.get_db_pool();

        let mut transaction = db_pool.begin().await?;
        let inventory_items = match self.inventory_caches.get(&(player_uuid, character_uuid)) {
            None => return Ok(()),
            Some(inventory) => inventory.read().await.clone(),
        };

        for item in inventory_items {
            // Helper function which gives Some(key, value) if the index exists, None for both otherwise.
            let get_attr = |idx: usize| {
                item.attributes
                    .get(idx)
                    .map(|a| (a.attr_definition, a.attr_value as f64))
            };

            let item_instance_uuid = match Uuid::parse_str(&item.item_instance_id) {
                Ok(item_instance_uuid) => item_instance_uuid,
                Err(e) => {
                    eprintln!("Item instance id is not UUID! {}", e);
                    continue;
                }
            };
            if item.amount == 0 {
                sqlx::query("DELETE FROM player_inventory WHERE item_instance_id = $1")
                    .bind(item_instance_uuid)
                    .execute(&mut *transaction)
                    .await?;
            } else{
                sqlx::query(
                    "INSERT INTO player_inventory
                     (item_instance_id, player_uuid, owner_uuid, item_definition_id, quantity,
                      attr_enum_1, attr_val_1, attr_enum_2, attr_val_2,
                      attr_enum_3, attr_val_3, user_attr_enum_1, usr_attr_val_1, is_equipped)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                     ON CONFLICT (item_instance_id) DO UPDATE SET
                         item_definition_id = EXCLUDED.item_definition_id,
                         quantity = EXCLUDED.quantity,
                         attr_enum_1 = EXCLUDED.attr_enum_1,
                         attr_val_1 = EXCLUDED.attr_val_1,
                         attr_enum_2 = EXCLUDED.attr_enum_2,
                         attr_val_2 = EXCLUDED.attr_val_2,
                         attr_enum_3 = EXCLUDED.attr_enum_3,
                         attr_val_3 = EXCLUDED.attr_val_3,
                         user_attr_enum_1 = EXCLUDED.user_attr_enum_1,
                         usr_attr_val_1 = EXCLUDED.usr_attr_val_1,
                         is_equipped = EXCLUDED.is_equipped",
                )
                    .bind(item_instance_uuid)
                    .bind(player_uuid)
                    .bind(character_uuid)
                    .bind(item.item_definition_id as i64)
                    .bind(item.amount as i64)
                    .bind(get_attr(0).map(|(e, _)| e))
                    .bind(get_attr(0).map(|(_, v)| v))
                    .bind(get_attr(1).map(|(e, _)| e))
                    .bind(get_attr(1).map(|(_, v)| v))
                    .bind(get_attr(2).map(|(e, _)| e))
                    .bind(get_attr(2).map(|(_, v)| v))
                    .bind(get_attr(3).map(|(e, _)| e))
                    .bind(get_attr(3).map(|(_, v)| v))
                    .bind(item.is_equipped)
                    .execute(&mut *transaction)
                    .await?;
            }
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn aggregate_inventories(
        &self,
        player_uuid: Uuid,
        secondary_uuid: Uuid,
        inventory: Vec<InventoryItem>,
    ) -> Result<Vec<InventoryItem>, sqlx::Error> {
        let inventory_guard = self
            .get_inventory(player_uuid, secondary_uuid)
            .await?;
        let mut target_inventory = inventory_guard.write().await; // Then access the RwLock
        let unable_to_collect = self.try_aggregate_inventories(
            inventory,
            &mut target_inventory,
        );
        Ok(unable_to_collect)
    }

    fn try_aggregate_inventories(
        &self,
        source: Vec<InventoryItem>,
        target: &mut Vec<InventoryItem>,
    ) -> Vec<InventoryItem> {
        (self.try_aggregate_inventories_fn)(
            source,
            target,
        )
    }

    fn generate_inventory_item_for_player(
        &self,
        player_uuid: Uuid,
        definition_id: u64,
        amount: u64,
    ) -> InventoryItem {
        (self.generate_inventory_item_for_player_fn)(
            player_uuid,
            definition_id,
            amount,
        )
    }
}

pub struct InventoryServiceImpl {
    database_service: Arc<dyn DatabaseService + Send + Sync>,
    inventory_caches: DashMap<(Uuid, Uuid), Arc<RwLock<Vec<InventoryItem>>>>,
    try_aggregate_inventories_fn: Arc<dyn Fn(Vec<InventoryItem>, &mut Vec<InventoryItem>) -> Vec<InventoryItem> + Send + Sync>,
    generate_inventory_item_for_player_fn: Arc<dyn Fn(Uuid, u64, u64) -> InventoryItem + Send + Sync>,
}

impl InventoryServiceImpl {
    pub fn new(
        database_service: Arc<dyn DatabaseService + Send + Sync>,
        try_aggregate_inventories_fn: Arc<dyn Fn(Vec<InventoryItem>, &mut Vec<InventoryItem>) -> Vec<InventoryItem> + Send + Sync>,
        generate_inventory_item_for_player_fn: Arc<dyn Fn(Uuid, u64, u64) -> InventoryItem + Send + Sync>,
    ) -> Self {
        Self {
            database_service,
            inventory_caches: DashMap::new(),
            try_aggregate_inventories_fn,
            generate_inventory_item_for_player_fn,
        }
    }

    async fn get_inventory(
        &self,
        player_uuid: Uuid,
        secondary: Uuid,
    ) -> Result<Arc<RwLock<Vec<InventoryItem>>>, sqlx::Error> {
        match self.inventory_caches.get(&(player_uuid, secondary)) {
            None => {}
            Some(result) => return Ok(result.value().clone()),
        }

        let db_pool = self.database_service.get_db_pool();
        let items = sqlx::query_as::<_, PlayerItem>(
            "SELECT 
                item_instance_id,
                item_definition_id,
                quantity,
                is_equipped,
                attr_enum_1, attr_val_1,
                attr_enum_2, attr_val_2,
                attr_enum_3, attr_val_3,
                user_attr_enum_1, usr_attr_val_1
             FROM player_inventory
             WHERE player_uuid = $1 AND owner_uuid = $2",
        )
        .bind(player_uuid)
        .bind(secondary)
        .fetch_all(db_pool.as_ref())
        .await?;

        let result: Vec<InventoryItem> = items
            .into_iter()
            .map(|row| {
                let row = row.populate_attributes();
                let attrs = row
                    .rolled_attributes
                    .into_iter()
                    .chain(row.player_custom_attributes.into_iter())
                    .map(|a| InventoryItemAttribute {
                        attr_definition: a.attribute_enum,
                        attr_value: a.attribute_val as f32,
                    })
                    .collect();
                InventoryItem {
                    item_instance_id: row.item_instance_id.to_string(),
                    item_definition_id: row.item_definition_id as u64,
                    amount: row.quantity as u64,
                    is_equipped: row.is_equipped,
                    attributes: attrs,
                }
            })
            .collect();
        let key = (player_uuid, secondary);
        self.inventory_caches
            .insert(key, Arc::new(RwLock::new(result)));
        Ok(self.inventory_caches.get(&key).unwrap().value().clone())
    }
}
