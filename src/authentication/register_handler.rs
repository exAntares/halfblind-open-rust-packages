use crate::db;
use crate::db::db::sqlx_error_to_proto_error;
use crate::handlers::handler_registry::HandlerRegistration;
use crate::handlers::handler_registry::RequestHandler;
use crate::inventory::inventory_item_utils;
use crate::inventory::inventory_item_utils::try_aggregate_inventories;
use crate::item_definitions::ItemDefinitionLookupService;
use crate::services::services::Services;
use halfblind_network::*;
use halfblind_protobuf_network::*;
use proto_gen::InventoryItem;
use sqlx::Row;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

request_handler!(RegisterRequest => RegisterResponse, Services);

async fn handle(
    _message_timestamp: u64,
    req: RegisterRequest,
    _ctx: Arc<ConnectionContext>,
    systems: Arc<Services>,
    ) -> Result<RegisterResponse, ProtoResponse> {
    let db_pool = systems.database_service.get_db_pool();
    let device_uuid = match Uuid::parse_str(&req.device_id) {
        Ok(player_uuid) => player_uuid,
        Err(_) => {
            return Err(build_error_response(
                ErrorCode::InvalidRequest as i32,
                &format!("Register is not a valid UUID {}", req.device_id),
            ));
        }
    };

    let player_exists = match sqlx::query("SELECT EXISTS(SELECT 1 FROM players WHERE device_id = $1)")
        .bind(device_uuid)
        .fetch_one(db_pool.as_ref())
        .await {
        Ok(x) => x,
        Err(e) => return Err(build_error_response(
            ErrorCode::UnknownError as i32,
            &format!("Failed to check player existence: {}", e),
        )),
    }
        .get::<bool, _>(0);

    if player_exists {
        return Err(build_error_response(
            ErrorCode::UserAlreadyExists as i32,
            "",
        ));
    }
    let password;
    #[cfg(feature = "dev-password")]
    {
        // This code only compiles when the "dev-password" feature is enabled
        password = match Uuid::parse_str("12345678-1234-1234-1234-123456789012") {
            Ok(x) => x,
            Err(e) => return Err(build_error_response(ErrorCode::UnknownError as i32, &format!("failed to parse dev-password: {}", e))),
        };
    }
    #[cfg(not(feature = "dev-password"))]
    {
        // Generate new UUID token
        password = Uuid::new_v4();
    }
    let player_uuid = match db::db::try_create_player(&db_pool, device_uuid, password).await {
        Ok(player_uuid) => player_uuid,
        Err(e) if e.to_string().contains("duplicate key") => return Err(build_error_response(
            ErrorCode::UnknownError as i32,
            &format!("Failed to create player duplicate key: {}", e),
        )),
        Err(e) => return Err(build_error_response(
            ErrorCode::UnknownError as i32,
            &format!("Failed to create player: {}", e),
        )),
    };

    let player_inventory_arc = systems.inventory_service
        .get_inventory(player_uuid, player_uuid)
        .await
        .map_err(sqlx_error_to_proto_error)?;
    let mut player_inventory_rw_lock = player_inventory_arc.write().await;
    let mut player_inventory_clone = player_inventory_rw_lock.clone();

    add_default_inventory_to_player(
        systems.item_definition_lookup_service.clone(),
        player_uuid,
        systems.clone(),
        &mut player_inventory_clone,
    ).await.map_err(|e|build_error_response(ErrorCode::UnknownError as i32, &format!("Failed to add default inventory: {}", e)))?;

    *player_inventory_rw_lock = player_inventory_clone;
    let mut db_connection = systems.database_service.get_db_pool().begin().await.map_err(sqlx_error_to_proto_error)?;
    systems
        .inventory_service
        .save_inventory_to_db(player_uuid, player_uuid, &player_inventory_rw_lock.clone(), &mut db_connection)
        .await.map_err(sqlx_error_to_proto_error)?;
    db_connection.commit().await.map_err(sqlx_error_to_proto_error)?;
    let response = RegisterResponse {
        player_uuid: player_uuid.to_string(),
        token: password.to_string(),
    };
    Ok(response)
}

#[derive(Debug)]
struct TempInventoryItem {
    item_id: i64,
    quantity: i64,
}

pub async fn add_default_inventory_to_player(
    item_definition_lookup_service: Arc<dyn ItemDefinitionLookupService + Send + Sync>,
    player_uuid: Uuid,
    systems: Arc<Services>,
    player_inventory: &mut Vec<InventoryItem>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Convert to InventoryItem protobuf messages using generate_inventory_item_for_player
    let mut inventory_items_to_add = Vec::new();
    for (item_id, component) in systems.item_definition_lookup_service.inventory_initial_value_component_all() {
        let generated_item = inventory_item_utils::generate_inventory_item_for_player(
            systems.item_definition_lookup_service.clone(),
            *item_id,
            component.value as u64,  // Players don't have luck
        );

        inventory_items_to_add.push(generated_item);
    }

    // Save using inventory_service if we have any items
    if !inventory_items_to_add.is_empty() {
        try_aggregate_inventories(item_definition_lookup_service, &inventory_items_to_add, player_inventory);
    }
    Ok(())
}
