use async_trait::async_trait;
use sqlx::PgConnection;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[async_trait]
pub trait InventoryService<T> {
    async fn get_player_inventory(
        &self,
        player_uuid: Uuid,
    ) -> Result<Arc<RwLock<Vec<T>>>, sqlx::Error>;

    async fn get_inventory(
        &self,
        player_uuid: Uuid,
        secondary_uuid: Uuid,
    ) -> Result<Arc<RwLock<Vec<T>>>, sqlx::Error>;

    async fn get_definition_value_summed(
        &self,
        player_uuid: Uuid,
        owner_uuid: Uuid,
        item_definition_id: u64,
    ) -> Result<i64, sqlx::Error>;

    async fn save_inventory_to_db(
        &self,
        player_uuid: Uuid,
        secondary_uuid: Uuid,
        db_connection: &mut PgConnection, // The caller must do COMMIT
    ) -> Result<(), sqlx::Error>;
}
