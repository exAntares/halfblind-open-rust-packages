use crate::characters::models::DatabaseCharacter;
use async_trait::async_trait;
use proto_gen::{CharacterDefinitionComponent, InventoryItem};
use std::error::Error;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[async_trait]
pub trait CharactersService: Send + Sync {
    async fn add_new_character_into_db(
        &self,
        player_uuid: Uuid,
        character_definition_id: i64,
        character_definition: Arc<CharacterDefinitionComponent>,
        character_name: String,
    ) -> Result<Arc<RwLock<DatabaseCharacter>>, Box<dyn Error + Send + Sync>>;

    async fn has_character(
        &self,
        player_uuid: Uuid,
        character_uuid: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>>;

    async fn get_all_character_instances(
        &self,
        player_uuid: Uuid,
    ) -> Result<Vec<Arc<RwLock<DatabaseCharacter>>>, Box<dyn Error + Send + Sync>>;

    async fn get_character_instance(
        &self,
        player_uuid: Uuid,
        character_uuid: Uuid,
    ) -> Result<Arc<RwLock<DatabaseCharacter>>, Box<dyn Error + Send + Sync>>;

    async fn save_character_instance_to_db(
        &self,
        character_instance: &DatabaseCharacter,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;

    fn try_level_up_character(
        &self,
        inventory: &mut Vec<InventoryItem>,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;

    fn add_xp_to_inventory(
        &self,
        player_uuid: Uuid,
        inventory: &mut Vec<InventoryItem>,
        xp: u64,
    ) -> Result<(), Box<dyn Error + Send + Sync>>;
}
