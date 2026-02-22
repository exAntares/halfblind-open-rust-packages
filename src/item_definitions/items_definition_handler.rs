use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::*;
use std::error::Error;
use std::sync::Arc;

#[derive(Default)]
pub struct ItemDefinitionsHandler;

#[async_trait]
impl RequestHandler for ItemDefinitionsHandler {
    async fn handle(
        &self,
        message_id: u64,
        message_timestamp: u64,
        _: &[u8],
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, Box<dyn Error + Send + Sync>> {
        let player_uuid = match validate_player_context(&ctx, message_id) {
            Ok(result) => result,
            Err(response) => return Ok(response),
        };
        match SYSTEMS
            .items_definitions_service
            .get_item_definitions_response_for_player(player_uuid)
        {
            Ok(response) => Ok(encode_ok_ref(message_id, response)?),
            Err(e) => Ok(build_error_response(
                message_id,
                ErrorCode::UnknownError.into(),
                format!("{}", e).as_str(),
            )),
        }
    }
}
