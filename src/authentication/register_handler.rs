use crate::db;
use crate::inventory::inventory_item_utils;
use crate::systems::systems::{Systems, SYSTEMS};
use halfblind_network::*;
use halfblind_protobuf_network::*;
use sqlx::Row;
use std::error::Error;
use std::sync::Arc;
use uuid::Uuid;

request_handler!(RegisterRequest => RegisterHandler);

async fn handle(
        _message_timestamp: u64,
        req: RegisterRequest,
        _ctx: Arc<ConnectionContext>,
    ) -> Result<ProtoResponse, ProtoResponse> {
    let db_pool = SYSTEMS.database_service.get_db_pool();
    let player_uuid = match Uuid::parse_str(&req.player_uuid) {
        Ok(player_uuid) => player_uuid,
        Err(_) => {
            return Ok(build_error_response(
                ErrorCode::InvalidRequest as i32,
                &format!("Register is not a valid UUID {}", req.player_uuid),
            ));
        }
    };

    let player_exists = match sqlx::query("SELECT EXISTS(SELECT 1 FROM players WHERE uuid = $1)")
        .bind(player_uuid)
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
        return Ok(build_error_response(
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
    let _ = match match db::db::create_player_or_not(&db_pool, player_uuid, password).await {
        Ok(_) => Ok(true),
        Err(e) if e.to_string().contains("duplicate key") => Ok(false),
        Err(e) => Err(e),
    } {
        Ok(x) => x,
        Err(e) => {
            return Err(build_error_response(
                ErrorCode::UnknownError as i32,
                &format!("Failed to create player: {}", e),
            ));
        }
    };

    if let Err(e) = add_default_inventory_to_player(player_uuid, SYSTEMS.clone()).await {
        return Err(build_error_response(
            ErrorCode::UnknownError as i32,
            &format!("Failed to add default inventory: {}", e),
        ));
    }

    let response = RegisterResponse {
        player_uuid: player_uuid.to_string(),
        token: password.to_string(),
    };
    encode_ok(&response)
}

#[derive(Debug)]
struct TempInventoryItem {
    item_id: i64,
    quantity: i64,
}

pub async fn add_default_inventory_to_player(
    player_uuid: Uuid,
    systems: Arc<Systems>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Convert to InventoryItem protobuf messages using generate_inventory_item_for_player
    let mut inventory_items = Vec::new();
    for (item_id, component) in SYSTEMS.item_definition_lookup_service.inventory_initial_value_component_all() {
        let generated_item = inventory_item_utils::generate_inventory_item_for_player(
            systems.items_definitions_service.clone(),
            systems.random_service.clone(),
            player_uuid,
            *item_id,
            component.value as u64,
            0.0, // Players don't have luck
        );

        inventory_items.push(generated_item);
    }

    // Save using inventory_service if we have any items
    if !inventory_items.is_empty() {
        systems
            .inventory_service
            .aggregate_inventories(player_uuid, player_uuid, inventory_items)
            .await?;
        systems
            .inventory_service
            .save_character_inventory(player_uuid, player_uuid)
            .await?;
    }

    Ok(())
}
