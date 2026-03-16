use crate::systems::systems::SYSTEMS;
use halfblind_network::*;
use halfblind_protobuf_network::*;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

request_handler!(LoginRequest => LoginHandler);

async fn handle(
        _message_timestamp: u64,
        req: LoginRequest,
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, ProtoResponse> {
    let db_pool = SYSTEMS.database_service.get_db_pool();
    let player_uuid = match Uuid::parse_str(&req.player_uuid) {
        Ok(uuid) => uuid,
        Err(e) => {
            eprintln!("Invalid player UUID: {}", e);
            return Ok(build_error_response(
                ErrorCode::InvalidRequest.into(),
                "Invalid player UUID",
            ));
        }
    };

    let auth_token = match Uuid::parse_str(&req.token) {
        Ok(token) => token,
        Err(e) => {
            eprintln!("Invalid auth token: {}", e);
            return Ok(build_error_response(
                ErrorCode::InvalidRequest.into(),
                "Invalid auth token",
            ));
        }
    };

    let player_exists =
        match sqlx::query("SELECT EXISTS(SELECT 1 FROM players WHERE uuid = $1 AND auth_token = $2)")
            .bind(player_uuid)
            .bind(auth_token)
            .fetch_one(db_pool.as_ref())
            .await {
            Ok(x) => x,
            Err(e) => return Ok(build_error_response(
                ErrorCode::AuthenticationFailed.into(),
                &format!("Failed to check player existence: {}", e),
            )),
        }
            .get::<bool, _>(0);

    if !player_exists {
        return Ok(build_error_response(
            ErrorCode::AuthenticationFailed.into(),
            "Player not found or invalid token",
        ));
    }

    ctx.set_user(player_uuid); // Set user context
    let response = LoginResponse {
        player_uuid: player_uuid.to_string(),
    };
    encode_ok(&response)
}

