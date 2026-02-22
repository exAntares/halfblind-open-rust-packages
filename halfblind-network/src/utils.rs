use crate::ConnectionContext;
use axum::body::Bytes;
use halfblind_protobuf::pack_any;
use halfblind_protobuf_network::*;
use prost::Message;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

pub fn get_now() -> u64 {
    chrono::Utc::now().naive_utc().and_utc().timestamp_millis() as u64
}

pub fn encode_ok<T: Message>(
    message_id: u64,
    response: T,
) -> Result<ProtoResponse, Box<dyn Error + Send + Sync>> {
    let mut encoded = Vec::new();
    response.encode(&mut encoded)?;
    let message = ProtoResponse {
        server_now: get_now(),
        message_id,
        any_payload: Some(pack_any(&response)),
        error: ErrorCode::Ok as i32,
    };
    Ok(message)
}

pub fn encode_ok_ref<T: Message>(
    message_id: u64,
    response: &T,
) -> Result<ProtoResponse, Box<dyn Error + Send + Sync>> {
    let mut encoded = Vec::new();
    response.encode(&mut encoded)?;
    let message = ProtoResponse {
        server_now: get_now(),
        message_id,
        any_payload: Some(pack_any(response)),
        error: ErrorCode::Ok as i32,
    };
    Ok(message)
}

pub fn encode_proto_response(
    response: ProtoResponse,
) -> Result<axum::extract::ws::Message, Box<dyn Error + Send + Sync>> {
    let mut out = Vec::new();
    match response.encode(&mut out) {
        Ok(_) => Ok(axum::extract::ws::Message::Binary(Bytes::from(out))),
        Err(_) => Err("Failed to encode response".into()),
    }
}

pub fn encode_message<T: Message>(
    message_id: u64,
    response: T,
) -> Result<axum::extract::ws::Message, Box<dyn Error + Send + Sync>> {
    let mut encoded = Vec::new();
    response.encode(&mut encoded)?;
    let message = ProtoResponse {
        server_now: get_now(),
        message_id,
        any_payload: Some(pack_any(&response)),
        error: ErrorCode::Ok as i32,
    };
    encode_proto_response(message)
}

pub fn build_error_response(message_id: u64, code: i32, msg: &str) -> ProtoResponse {
    eprintln!("Error [{}]: {}", code, msg);
    ProtoResponse {
        server_now: get_now(),
        message_id,
        any_payload: None,
        error: code,
    }
}

pub fn validate_player_context(
    ctx: &Arc<ConnectionContext>,
    message_id: u64,
) -> Result<Uuid, ProtoResponse> {
    // Get and validate player UUID
    let player_uuid = match ctx.get_player_uuid() {
        Some(id) => id,
        None => {
            return Err(build_error_response(
                message_id,
                ErrorCode::AuthenticationFailed as i32,
                "User not logged in trying to do something",
            ));
        }
    };
    Ok(player_uuid)
}
