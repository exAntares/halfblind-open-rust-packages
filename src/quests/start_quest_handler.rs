use crate::handlers::utils;
use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use prost::Message;
use proto_gen::QuestStatus;
use proto_gen::{GameErrorCode, StartQuestRequest, StartQuestResponse};
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

#[derive(Default)]
pub struct StartQuestHandler {}

#[async_trait]
impl RequestHandler for StartQuestHandler {
    async fn handle(
        &self,
        message_id: u64,
        message_timestamp: u64,
        payload: &[u8],
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, Box<dyn Error + Send + Sync>> {
        let req = StartQuestRequest::decode(payload)?;
        let character_uuid_str = req.character_uuid;
        let (player_uuid, character_uuid) =
            match utils::validate_character_and_player_uuid(&ctx, SYSTEMS.clone(), message_id, character_uuid_str).await {
                Ok(result) => result,
                Err(response) => return Ok(response),
            };

        let quest_definition_id = req.quest_definition_id;
        let inventory_lock = SYSTEMS
            .inventory_service
            .get_inventory(player_uuid, character_uuid)
            .await?;
        let mut inventory = inventory_lock.write().await;
        let inventory_hashmap_int_int: HashMap<u64, i64> = inventory
            .iter()
            .map(|item| (item.item_definition_id, item.amount as i64))
            .collect();
        let quest_status = match inventory_hashmap_int_int.get(&quest_definition_id) {
            None => {
                return Ok(build_error_response(
                    message_id,
                    GameErrorCode::QuestIsNotAvailable.into(),
                    "Quest is not available",
                ));
            }
            Some(res) => res,
        };

        if *quest_status != (QuestStatus::Available as i64) {
            return Ok(build_error_response(
                message_id,
                GameErrorCode::QuestIsNotAvailable.into(),
                "Quest is not available",
            ));
        }

        if let Some(quest_item) = inventory
            .iter_mut()
            .find(|item| item.item_definition_id == quest_definition_id)
        {
            quest_item.amount = QuestStatus::InProgress as u64;
        }

        let response = StartQuestResponse {};
        Ok(encode_ok(message_id, response)?)
    }
}
