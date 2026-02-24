use crate::characters::characters_service::CharactersService;
use crate::characters::models::DatabaseCharacter;
use crate::inventory::inventory_item_utils;
use crate::inventory::inventory_item_utils::try_aggregate_inventories;
use crate::item_definitions::IsCharacterLevelComponentLookup;
use crate::item_definitions::IsCharacterXpComponentLookup;
use crate::item_definitions::LevelRequiredExperienceComponentLookup;
use async_trait::async_trait;
use dashmap::DashMap;
use halfblind_database_service::DatabaseService;
use halfblind_itemdefinitions_service::ItemDefinitionsService;
use halfblind_random::RandomService;
use proto_gen::{CharacterDefinitionComponent, InventoryItem, LevelRequiredExperienceComponent};
use std::error::Error;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct CharactersServiceImpl {
    database_service: Arc<dyn DatabaseService + Send + Sync>,
    item_definition_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
    random_service: Arc<dyn RandomService + Send + Sync>,
    characters_cache: DashMap<Uuid, Vec<Arc<RwLock<DatabaseCharacter>>>>,
}

#[async_trait]
impl CharactersService for CharactersServiceImpl {
    async fn add_new_character_into_db(
        &self,
        player_uuid: Uuid,
        character_definition_id: i64,
        character_definition: CharacterDefinitionComponent,
        character_name: String,
    ) -> Result<Arc<RwLock<DatabaseCharacter>>, Box<dyn Error + Send + Sync>> {
        let db_pool = self.database_service.get_db_pool();
        let record = sqlx::query_as::<_, DatabaseCharacter>(r#"
        INSERT INTO player_characters (player_uuid, character_definition_id, strength, agility, intelligence, vitality, current_hp, character_name)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING
            player_uuid,
            character_uuid,
            character_definition_id,
            strength,
            agility,
            intelligence,
            vitality,
            current_hp,
            character_name
        "#,
        )
            .bind(player_uuid)
            .bind(character_definition_id)
            .bind(0)
            .bind(0)
            .bind(0)
            .bind(0)
            .bind(character_definition.base_hp)
            .bind(character_name.clone())
            .fetch_one(db_pool.as_ref())
            .await?;
        let arc_record = Arc::new(RwLock::new(record));
        match self.characters_cache.get_mut(&player_uuid) {
            None => {
                self.characters_cache
                    .insert(player_uuid, vec![arc_record.clone()]);
            }
            Some(mut vec) => {
                vec.push(arc_record.clone());
            }
        }
        Ok(arc_record)
    }

    async fn has_character(
        &self,
        player_uuid: Uuid,
        character_uuid: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        match self.characters_cache.get(&player_uuid) {
            None => Ok(false),
            Some(result) => {
                for x in result.value().iter() {
                    if x.read().await.character_uuid == character_uuid {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    async fn get_all_character_instances(
        &self,
        player_uuid: Uuid,
    ) -> Result<Vec<Arc<RwLock<DatabaseCharacter>>>, Box<dyn Error + Send + Sync>> {
        match self.characters_cache.get(&player_uuid) {
            None => {}
            Some(result) => {
                return Ok(result.value().clone());
            }
        }
        // Get database connection
        let db_pool = self.database_service.get_db_pool();

        // Query all matching character instances
        let characters: Vec<DatabaseCharacter> = sqlx::query_as(
            r#"
        SELECT player_uuid, character_uuid, character_definition_id, strength, agility, intelligence, vitality, current_hp, character_name
        FROM player_characters
        WHERE player_uuid = $1
        "#,
        )
            .bind(player_uuid)
            .fetch_all(db_pool.as_ref())
            .await?;
        let mut result = Vec::new();
        for db_character in characters {
            let arc_rwlock = Arc::new(RwLock::new(db_character));
            result.push(arc_rwlock);
        }
        self.characters_cache.insert(player_uuid, result.clone());
        // Wrap the results in Arc<RwLock>
        Ok(result)
    }

    async fn get_character_instance(
        &self,
        player_uuid: Uuid,
        character_uuid: Uuid,
    ) -> Result<Arc<RwLock<DatabaseCharacter>>, Box<dyn Error + Send + Sync>> {
        match self.characters_cache.get(&player_uuid) {
            None => {
                // Not found: load from DB
                return match self.get_all_character_instances(player_uuid).await {
                    Ok(characters) => {
                        for x in characters.iter() {
                            if x.read().await.character_uuid == character_uuid {
                                return Ok(x.clone());
                            }
                        }
                        Err("Character does not exist in DB".into())
                    }
                    Err(e) => Err(e),
                };
            }
            Some(result) => {
                for x in result.value().iter() {
                    if x.read().await.character_uuid == character_uuid {
                        return Ok(x.clone());
                    }
                }
                Err("Character does not exist in DB".into())
            }
        }
    }

    async fn save_character_instance_to_db(
        &self,
        character_instance: &DatabaseCharacter,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Try to get the character instance from the in-memory hash set
        // Get db pool
        let db_pool = self.database_service.get_db_pool();
        // Update the DB state
        sqlx::query(
            r#"
            UPDATE player_characters
            SET current_hp = $3, character_name = $4, strength = $5, agility = $6, intelligence = $7, vitality = $8
            WHERE player_uuid = $1 AND character_uuid = $2
            "#,
        )
            .bind(character_instance.player_uuid)
            .bind(character_instance.character_uuid)
            .bind(character_instance.current_hp)
            .bind(character_instance.character_name.clone())
            .bind(character_instance.strength)
            .bind(character_instance.agility)
            .bind(character_instance.intelligence)
            .bind(character_instance.vitality)
            .execute(db_pool.as_ref())
            .await?;

        // invalidate cache
        self.characters_cache
            .remove(&character_instance.player_uuid);
        Ok(())
    }

    fn try_level_up_character(
        &self,
        inventory: &mut Vec<InventoryItem>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let xp_item_definition_id = self.get_xp_item_definition_id();
        let xp_inventory_item_idx = match inventory.iter().position(|x| x.item_definition_id == xp_item_definition_id) {
            None => {
                // There is no xp so we don't need to level up.
                return Ok(());
            }
            Some(index) => {index}
        };
        let level_definition_id = IsCharacterLevelComponentLookup.iter().last().unwrap().0;
        let level_inventory_item_idx = match inventory.iter().position(|x| x.item_definition_id == *level_definition_id) {
            None => {
                return Err("Character does not have a level item".into());
            }
            Some(index) => {index}
        };
        
        let levels = self.get_xp_required();
        loop {
            let next_level = inventory[level_inventory_item_idx].amount + 1;
            let has_next_level =
                next_level < levels.required_exp_for_level.iter().count() as u64;
            if !has_next_level {
                return Ok(());
            }
            let next_level_required_exp =
                levels.required_exp_for_level[next_level as usize];
            if inventory[xp_inventory_item_idx].amount >= next_level_required_exp {
                inventory[xp_inventory_item_idx].amount -= next_level_required_exp;
                inventory[level_inventory_item_idx].amount += 1;
            } else {
                return Ok(());
            }
        }
    }

    fn add_xp_to_inventory(
        &self,
        player_uuid: Uuid,
        inventory: &mut Vec<InventoryItem>,
        xp: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let xp_item_definition_id = self.get_xp_item_definition_id();
        let new_item = inventory_item_utils::generate_inventory_item_for_player(
            self.item_definition_service.clone(),
            self.random_service.clone(),
            player_uuid,
            xp_item_definition_id,
            xp,
            0.0, // No luck needed for XP
        );
        try_aggregate_inventories(
            vec![new_item],
            inventory,
        );
        Ok(())
    }
}

impl CharactersServiceImpl {
    pub fn new(
        database_service: Arc<dyn DatabaseService + Send + Sync>,
        item_definition_service: Arc<dyn ItemDefinitionsService + Send + Sync>,
        random_service: Arc<dyn RandomService + Send + Sync>,
    ) -> Self {
        Self {
            database_service,
            item_definition_service,
            random_service,
            characters_cache: DashMap::new(),
        }
    }

    pub fn get_xp_item_definition_id(&self) -> u64 {
        IsCharacterXpComponentLookup
            .iter()
            .last()
            .unwrap()
            .0
            .clone()
    }

    pub fn get_xp_required(&self) -> LevelRequiredExperienceComponent {
        LevelRequiredExperienceComponentLookup
            .iter()
            .last()
            .unwrap()
            .1
            .clone()
    }
}
