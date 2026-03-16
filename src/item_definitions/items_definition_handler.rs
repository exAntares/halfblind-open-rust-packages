use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::*;
use std::sync::Arc;

#[derive(Default)]
pub struct ItemDefinitionsHandler;

#[async_trait]
impl RequestHandler for ItemDefinitionsHandler {
    async fn handle(
        &self,
        message_timestamp: u64,
        _: &[u8],
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, ProtoResponse> {
        let player_uuid = validate_player_context(&ctx)?;
        match SYSTEMS
            .items_definitions_service
            .get_item_definitions_response_for_player(player_uuid)
        {
            Ok(response) => encode_ok(response),
            Err(e) => Ok(build_error_response(
                ErrorCode::UnknownError.into(),
                format!("{}", e).as_str(),
            )),
        }
    }
}
