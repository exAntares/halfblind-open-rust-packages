use crate::DatabaseService;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

pub struct DatabaseServiceImpl {
    db_pool: Arc<Pool<Postgres>>
}

impl DatabaseServiceImpl {
    pub fn new(pool : Arc<Pool<Postgres>>) -> Self {
        Self {
            db_pool: pool
        }
    }
}

impl DatabaseService for DatabaseServiceImpl {
    fn get_db_pool(&self) -> Arc<Pool<Postgres>> {
        self.db_pool.clone()
    }
}
