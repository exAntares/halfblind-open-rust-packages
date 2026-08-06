use crate::db::db::sqlx_error_to_proto_error;
use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::handlers::utils;
use crate::services::services::Services;
use crate::transactions::postgress_delayed_rewards_inserter::PostgresDelayedRewardsInserter;
use halfblind_network::*;
use halfblind_protobuf_network::{ErrorCode, ProtoResponse};
use proto_gen::{ClaimQuestRequest, QuestStatus};
use proto_gen::{ClaimQuestResponse, GameErrorCode};
use protobuf_itemdefinition::ItemsErrorCode;
use std::sync::Arc;

request_handler!(ClaimQuestRequest => ClaimQuestResponse, Services);

async fn handle(
    _message_timestamp: u64,
    req: ClaimQuestRequest,
    ctx: Arc<ConnectionContext>,
    systems: Arc<Services>,
) -> Result<ClaimQuestResponse, ProtoResponse> {
    let character_uuid_str = req.character_uuid;
    let (player_uuid, character_uuid) = utils::validate_character_and_player_uuid(&ctx, systems.clone(), character_uuid_str).await?;

    let quest_definition_id = req.quest_definition_id;
    if systems.item_definition_lookup_service.transaction_component(&quest_definition_id).is_none() {
        return Err(build_error_response(
            ItemsErrorCode::TransactionInvalid.into(),
            "Quest is not a transaction",
        ));
    };

    let inventory_lock = match systems.inventory_service.get_inventory(player_uuid, character_uuid).await {
        Ok(x) => x,
        Err(e) => return Err(build_error_response(ErrorCode::UnknownError.into(), &format!("Failed to get inventory: {}", e)))
    };
    let mut inventory_rw_lock = inventory_lock.write().await;
    let quest_inventory_item = inventory_rw_lock
        .iter()
        .find(|item| item.item_definition_id == quest_definition_id);
    let quest_status = match quest_inventory_item {
        None => return Err(build_error_response(GameErrorCode::QuestIsNotAvailable.into(), "Quest is not available")),
        Some(res) => res,
    };

    if quest_status.amount != (QuestStatus::InProgress as u64) {
        return Err(build_error_response(GameErrorCode::QuestIsNotAvailable.into(), "Quest is not in progress!!"));
    }
    let mut db_transaction = systems.database_service.clone()
        .get_db_pool()
        .begin()
        .await.map_err(sqlx_error_to_proto_error)?;
    // TODO: luis getting rewards could fail due not enough inventory space!! We should check it before and ignore the claim
    let mut delayed_items_inserter = PostgresDelayedRewardsInserter { connection: &mut db_transaction };
    match systems.transaction_service.process_inventory_transaction_id(
        systems.random_service.clone(),
        &mut delayed_items_inserter,
        &mut inventory_rw_lock,
        player_uuid,
        req.quest_definition_id,
    ).await {
        Ok(_) => {}
        Err(e) => return Err(build_error_response(e.into(), &"Failed transaction".to_string())),
    };
    db_transaction.commit().await.map_err(sqlx_error_to_proto_error)?;
    let response = ClaimQuestResponse {};
    Ok(response)
}