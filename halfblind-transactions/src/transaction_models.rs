use chrono::NaiveDateTime;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub struct TransactionRecord {
    pub id: Uuid,
    pub end_at: NaiveDateTime,
    pub item_id: i64,
    pub quantity: i64,
}
