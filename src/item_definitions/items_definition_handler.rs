use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::services::services::Services;
use halfblind_network::*;
use halfblind_protobuf_network::*;
use protobuf_itemdefinition::{ItemDefinitionsRequest, ItemDefinitionsResponse};
use std::sync::Arc;

request_handler!(ItemDefinitionsRequest => ItemDefinitionsResponse, Services);

async fn handle(
    message_timestamp: u64,
    _: ItemDefinitionsRequest,
    ctx: Arc<ConnectionContext>,
    systems: Arc<Services>,
) -> Result<ItemDefinitionsResponse, ProtoResponse> {
    let player_uuid = validate_player_context(&ctx)?;
    match systems
        .items_definitions_service
        .get_item_definitions_response_for_player(player_uuid)
    {
        Ok(response) => Ok(response.clone()),
        Err(e) => Err(build_error_response(
            ErrorCode::UnknownError.into(),
            format!("{}", e).as_str(),
        )),
    }
}