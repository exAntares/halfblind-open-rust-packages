use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::*;
use std::error::Error;
use std::sync::Arc;

#[derive(Default)]
pub struct CharacterEquipHandler;

#[async_trait]
impl RequestHandler for CharacterEquipHandler {
    async fn handle(
        &self,
        message_id: u64,
        message_timestamp: u64,
        payload: &[u8],
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, Box<dyn Error + Send + Sync>> {
        Ok(build_error_response(message_id, ErrorCode::NotImplemented as i32, "NotImplemented"))
    }
}
