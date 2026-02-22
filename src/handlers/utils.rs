use crate::systems::systems::Systems;
use halfblind_network::*;
use halfblind_protobuf_network::ProtoResponse;
use proto_gen::GameErrorCode;
use std::sync::Arc;
use uuid::Uuid;

/// Validates that a character UUID string is valid and belongs to the authenticated player.
///
/// This function performs several validation steps:
/// 1. Verifies the player is authenticated (has a valid player UUID in the context)
/// 2. Parses the character UUID string to ensure it's a valid UUID format
/// 3. Checks that the character belongs to the authenticated player by querying the characters service
///
/// # Arguments
///
/// * `ctx` - The connection context containing the player's authentication state and system services
/// * `message_id` - The ID of the message being processed (used for error responses)
/// * `character_uuid_str` - The character UUID as a string to be validated
///
/// # Returns
///
/// * `Ok(Uuid)` - The validated character UUID if all checks pass
/// * `Err(ProtoResponse)` - An error response with appropriate error code if validation fails:
///   - `AuthenticationFailed` if the player is not logged in
///   - `InvalidCharacter` if the UUID format is invalid or the character doesn't belong to the player
///   - `UnknownError` if there's a database error while checking character ownership
///
/// # Example
///
/// ```rust
/// let character_uuid = match validate_character_uuid(&ctx, message_id, req.character_uuid).await {
///     Ok(uuid) => uuid,
///     Err(error_response) => return Ok(error_response),
/// };
/// ```
pub async fn validate_character_and_player_uuid(
    ctx: &Arc<ConnectionContext>,
    systems: Arc<Systems>,
    message_id: u64,
    character_uuid_str: String,
) -> Result<(Uuid, Uuid), ProtoResponse> {
    // Get and validate player UUID
    let player_uuid = match ctx.get_player_uuid() {
        Some(id) => id,
        None => {
            return Err(build_error_response(
                message_id,
                halfblind_protobuf_network::ErrorCode::AuthenticationFailed as i32,
                "User not logged in trying to do something",
            ));
        }
    };

    let character_uuid = match Uuid::parse_str(&character_uuid_str) {
        Ok(c) => c,
        Err(_) => {
            return Err(build_error_response(
                message_id,
                GameErrorCode::InvalidCharacter as i32,
                "Invalid character UUID",
            ));
        }
    };

    match systems
        .characters_service
        .has_character(player_uuid, character_uuid)
        .await
    {
        Ok(has_character) => {
            if !has_character {
                return Err(build_error_response(
                    message_id,
                    GameErrorCode::InvalidCharacter as i32,
                    "Player is requesting action for a character that is not owned!",
                ));
            }
        }
        Err(e) => {
            eprintln!("Error checking if character exists: {}", e);
            return Err(build_error_response(
                message_id,
                halfblind_protobuf_network::ErrorCode::UnknownError as i32,
                "Something happened while trying to check if character exists. Please try again later.",
            ));
        }
    }
    Ok((player_uuid, character_uuid))
}


