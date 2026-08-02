use halfblind_network::build_error_response;
use halfblind_protobuf_network::{ErrorCode, ProtoResponse};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub async fn try_create_player(
    pool: &PgPool,
    device_uuid: Uuid,
    auth_token: Uuid,
) -> Result<Uuid, sqlx::Error> {
    let row = sqlx::query(
        "INSERT INTO players (device_uuid, auth_token) 
         VALUES ($1, $2)
         ON CONFLICT (device_uuid) DO NOTHING
         RETURNING uuid"
    )
        .bind(device_uuid)
        .bind(auth_token)
        .fetch_one(pool)
        .await?;
    let player_uuid = row.get::<Uuid, _>("uuid");
    Ok(player_uuid)
}

pub fn sqlx_error_to_proto_error(e: sqlx::Error) -> ProtoResponse {
    build_error_response(ErrorCode::UnknownError.into(), &format!("Sqlx failure: {}", e))
}