use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::services::services::Services;
use halfblind_network::*;
use halfblind_protobuf_network::*;
use proto_gen::{CharacterEquipRequest, CharacterEquipResponse};
use std::sync::Arc;

request_handler!(CharacterEquipRequest => CharacterEquipResponse, Services);

async fn handle(
    _message_timestamp: u64,
    req: CharacterEquipRequest,
    _ctx: Arc<ConnectionContext>,
    systems: Arc<Services>,
) -> Result<CharacterEquipResponse, ProtoResponse> {
    Err(build_error_response(ErrorCode::NotImplemented as i32, "NotImplemented"))
}