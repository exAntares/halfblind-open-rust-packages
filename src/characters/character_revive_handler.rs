use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::services::services::Services;
use halfblind_network::*;
use halfblind_protobuf_network::*;
use proto_gen::{CharacterReviveRequest, CharacterReviveResponse};
use std::sync::Arc;

request_handler!(CharacterReviveRequest => CharacterReviveResponse, Services);

async fn handle(
    _message_timestamp: u64,
    _: CharacterReviveRequest,
    _ctx: Arc<ConnectionContext>,
    systems: Arc<Services>,
) -> Result<CharacterReviveResponse, ProtoResponse> {
    Err(build_error_response(ErrorCode::NotImplemented as i32, "CharacterReviveHandler NotImplemented"))
}
