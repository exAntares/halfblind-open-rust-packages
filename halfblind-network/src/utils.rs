use crate::ConnectionContext;
use axum::body::Bytes;
use halfblind_protobuf::pack_any;
use halfblind_protobuf_network::*;
use prost::Message;
use std::sync::Arc;
use uuid::Uuid;

pub fn get_now() -> u64 {
    chrono::Utc::now().naive_utc().and_utc().timestamp_millis() as u64
}

pub fn encode_ok<T: Message>(response: &T) -> Result<ProtoResponse, ProtoResponse> {
    let message = ProtoResponse {
        server_now: get_now(),
        message_id: 0, // Assigned before sending back to the player
        any_payload: Some(pack_any(response)),
        error: ErrorCode::Ok as i32,
    };
    Ok(message)
}


pub fn encode_proto_response(response: ProtoResponse) -> Result<axum::extract::ws::Message, ProtoResponse> {
    let mut out = Vec::new();
    match response.encode(&mut out) {
        Ok(_) => Ok(axum::extract::ws::Message::Binary(Bytes::from(out))),
        Err(e) => Err(build_error_response(ErrorCode::UnknownError.into(),&format!("Failed to encode response: {}", e))),
    }
}

pub fn encode_message<T: prost::Message + Default>(response: T) -> Result<axum::extract::ws::Message, ProtoResponse> {
    let message = ProtoResponse {
        server_now: get_now(),
        message_id: 0, // Assigned before sending back to the player
        any_payload: Some(pack_any(&response)),
        error: ErrorCode::Ok as i32,
    };
    encode_proto_response(message)
}

pub fn build_error_response(
    code: i32,
    msg: &str
) -> ProtoResponse {
    eprintln!("Error [{}]: {}", code, msg);
    ProtoResponse {
        server_now: get_now(),
        message_id: 0, // Assigned before sending back to the player
        any_payload: None,
        error: code,
    }
}

pub fn validate_player_context(
    ctx: &Arc<ConnectionContext>,
) -> Result<Uuid, ProtoResponse> {
    // Get and validate player UUID
    let player_uuid = match ctx.get_player_uuid() {
        Some(id) => id,
        None => {
            return Err(build_error_response(
                ErrorCode::AuthenticationFailed.into(),
                "User not logged in trying to do something",
            ));
        }
    };
    Ok(player_uuid)
}

pub fn decode_or_error<T: prost::Message + Default>(data: &[u8]) -> Result<T, ProtoResponse> {
    T::decode(data)
        .map_err(|e| build_error_response(
            ErrorCode::InvalidRequest.into(),
            &format!("Failed to decode: {}", e)
        ))
}
