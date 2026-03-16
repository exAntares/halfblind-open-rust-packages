use crate::handlers::utils;
use crate::systems::systems::SYSTEMS;
use async_trait::async_trait;
use halfblind_network::*;
use halfblind_protobuf_network::{ErrorCode, ProtoResponse};
use proto_gen::{ClaimQuestResponse, GameErrorCode, StartQuestRequest};
use proto_gen::QuestStatus;
use protobuf_itemdefinition::ItemsErrorCode;
use std::sync::Arc;

#[derive(Default)]
pub struct ClaimQuestHandler {}

#[async_trait]
impl RequestHandler for ClaimQuestHandler {
    async fn handle(
        &self,
        message_timestamp: u64,
        payload: &[u8],
        ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, ProtoResponse> {
        let req = decode_or_error::<StartQuestRequest>(payload)?;
        let character_uuid_str = req.character_uuid;
        let (player_uuid, character_uuid) = utils::validate_character_and_player_uuid(&ctx, SYSTEMS.clone(), character_uuid_str).await?;

        let quest_definition_id = req.quest_definition_id;
        let inventory_lock = match SYSTEMS
            .inventory_service
            .get_inventory(player_uuid, character_uuid)
            .await {
            Ok(x) => x,
            Err(e) => return Err(build_error_response(ErrorCode::UnknownError.into(), &format!("Failed to get inventory: {}", e)))
        };
        let mut inventory = inventory_lock.write().await;
        let quest_inventory_item = inventory
            .iter()
            .find(|item| item.item_definition_id == quest_definition_id);
        let quest_status = match quest_inventory_item {
            None => {
                return Ok(build_error_response(
                    GameErrorCode::QuestIsNotAvailable.into(),
                    "Quest is not available",
                ));
            }
            Some(res) => res,
        };

        if quest_status.amount != (QuestStatus::InProgress as u64) {
            return Ok(build_error_response(
                GameErrorCode::QuestIsNotAvailable.into(),
                "Quest is not in progress!!",
            ));
        }

        if SYSTEMS.item_definition_lookup_service.transaction_component(&req.quest_definition_id).is_none() {
            return Ok(build_error_response(
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
                return Ok(build_error_response(e.into(), &"Failed transaction".to_string()))
            }
        };

        let response = ClaimQuestResponse {};
        encode_ok(&response)
    }
}
