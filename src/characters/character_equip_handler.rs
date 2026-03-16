use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::*;
use std::sync::Arc;

#[derive(Default)]
pub struct CharacterEquipHandler;

#[async_trait]
impl RequestHandler for CharacterEquipHandler {
    async fn handle(
        &self,
        _message_timestamp: u64,
        _payload: &[u8],
        _ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, ProtoResponse> {
        Err(build_error_response(ErrorCode::NotImplemented as i32, "NotImplemented"))
    }
}
