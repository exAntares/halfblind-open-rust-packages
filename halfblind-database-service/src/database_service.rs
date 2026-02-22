use async_trait::async_trait;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

#[async_trait]
pub trait DatabaseService: Send + Sync {
    fn get_db_pool(&self) -> Arc<Pool<Postgres>>;
}
