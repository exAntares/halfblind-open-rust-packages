use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::handlers::utils;
use crate::services::services::Services;
use halfblind_network::*;
use halfblind_protobuf_network::{ErrorCode, ProtoResponse};
use proto_gen::QuestStatus;
use proto_gen::{GameErrorCode, StartQuestRequest, StartQuestResponse};
use std::collections::HashMap;
use std::sync::Arc;

request_handler!(StartQuestRequest => StartQuestRequestHandler, Services);

async fn handle(
    _message_timestamp: u64,
    req: StartQuestRequest,
    ctx: Arc<ConnectionContext>,
    systems: Arc<Services>,
) -> Result<ProtoResponse, ProtoResponse> {
    let character_uuid_str = req.character_uuid;
    let (player_uuid, character_uuid) =
        match utils::validate_character_and_player_uuid(&ctx, systems.clone(), character_uuid_str).await {
            Ok(result) => result,
            Err(response) => return Ok(response),
        };

    let quest_definition_id = req.quest_definition_id;
    let inventory_lock = match systems
        .inventory_service
        .get_inventory(player_uuid, character_uuid)
        .await {
        Ok(x) => x,
        Err(e) => return Err(build_error_response(ErrorCode::UnknownError.into(), &format!("Failed to get inventory: {}", e)))
    };
    let mut inventory = inventory_lock.write().await;
    let inventory_hashmap_int_int: HashMap<u64, i64> = inventory
        .iter()
        .map(|item| (item.item_definition_id, item.amount as i64))
        .collect();
    let quest_status = match inventory_hashmap_int_int.get(&quest_definition_id) {
        None => {
            return Ok(build_error_response(
                GameErrorCode::QuestIsNotAvailable.into(),
                "Quest is not available",
            ));
        }
        Some(res) => res,
    };

    if *quest_status != (QuestStatus::Available as i64) {
        return Ok(build_error_response(
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
    encode_ok(&response)
}