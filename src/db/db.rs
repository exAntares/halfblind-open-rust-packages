use sqlx::PgPool;
use uuid::Uuid;

pub async fn create_player_or_not(
    pool: &PgPool,
    user_id: Uuid,
    auth_token: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO players (uuid, auth_token) 
         VALUES ($1, $2)
         ON CONFLICT (uuid) DO NOTHING",
        user_id,
        auth_token
    )
    .execute(pool)
    .await?;
    Ok(())
}
