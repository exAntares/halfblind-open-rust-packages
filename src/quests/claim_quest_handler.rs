use crate::handlers::utils;
use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use prost::Message;
use proto_gen::QuestStatus;
use proto_gen::{ClaimQuestResponse, GameErrorCode, StartQuestRequest};
use protobuf_itemdefinition::ItemsErrorCode;
use std::error::Error;
use std::sync::Arc;

#[derive(Default)]
pub struct ClaimQuestHandler {}

#[async_trait]
impl RequestHandler for ClaimQuestHandler {
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
        let quest_inventory_item = inventory
            .iter()
            .find(|item| item.item_definition_id == quest_definition_id);
        let quest_status = match quest_inventory_item {
            None => {
                return Ok(build_error_response(
                    message_id,
                    GameErrorCode::QuestIsNotAvailable.into(),
                    "Quest is not available",
                ));
            }
            Some(res) => res,
        };

        if quest_status.amount != (QuestStatus::InProgress as u64) {
            return Ok(build_error_response(
                message_id,
                GameErrorCode::QuestIsNotAvailable.into(),
                "Quest is not in progress!!",
            ));
        }

        if SYSTEMS.item_definition_lookup_service.transaction_component(&req.quest_definition_id).is_none() {
            return Ok(build_error_response(
                message_id,
                ItemsErrorCode::TransactionInvalid.into(),
                "Quest is not a transaction",
            ));
        };

        // TODO: luis getting rewards could fail due not enough inventory space!! We should check it before and ignore the claim
        match SYSTEMS.transaction_service.process_player_transaction_id(
            SYSTEMS.inventory_service.clone(),
            SYSTEMS.database_service.clone(),
            SYSTEMS.random_service.clone(),
            player_uuid,
            character_uuid,
            req.quest_definition_id,
        ).await {
            Ok(_) => {}
            Err(e) => {
                return Ok(build_error_response(message_id, e.into(), &"Failed transaction".to_string()))
            }
        };

        let response = ClaimQuestResponse {};
        Ok(encode_ok(message_id, response)?)
    }
}
