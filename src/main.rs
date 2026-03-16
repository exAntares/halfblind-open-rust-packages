mod authentication;
mod characters;
mod combat;
mod db;
mod handlers;
mod inventory;
mod item_definitions;
mod map;
mod map_update;
mod nav_mesh;
mod quests;
mod systems;
mod transactions;
mod behaviour_trees;

use crate::handlers::HANDLER_REGISTRY_BY_ANY_TYPE;
use crate::systems::systems::{Systems, POOL, SYSTEMS};
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Router};
use dotenvy::dotenv;
use futures_util::{SinkExt, StreamExt};
use halfblind_network::*;
use halfblind_protobuf_network::*;
use prost::Message;
use sqlx::PgPool;
use std::env;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

fn get_database_url() -> String {
    // Load .env file if it exists
    dotenv().ok();

    // Try environment variable, fallback if missing
    env::var("DATABASE_URL").unwrap()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Database connecting...");
    let pool = PgPool::connect(get_database_url().as_str()).await?;
    POOL.set(Arc::new(pool)).unwrap();
    let systems = SYSTEMS.clone();
    println!("Create TcpListener...");
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .layer(Extension(systems.clone()));
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("Websocket Now Listening on {}", addr);
    println!("Game server ready!");
    if let Err(e) = axum::serve(listener, app).await {
        // accept() or hyper-level error
        println!("server error: {e}");
    }
    Ok(())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(systems): Extension<Arc<Systems>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, systems))
}

async fn handle_socket(socket: WebSocket, systems: Arc<Systems>) {
    let (ws_writer, mut ws_reader) = socket.split();

    let ctx = Arc::new(ConnectionContext {
        player_uuid: Mutex::new(None),
        ws_writer: Arc::new(tokio::sync::Mutex::new(ws_writer)),
        is_player_connected: Mutex::new(false),
    });
    while let Some(msg_result) = ws_reader.next().await {
        match msg_result {
            Ok(axum::extract::ws::Message::Binary(data)) => {
                match ProtoRequest::decode(&*data) {
                    Ok(request) => {
                        let message_id = request.message_id;
                        let mut response = handle_request(request, ctx.clone())
                            .await
                            .unwrap_or_else(|e| e);
                        response.message_id = message_id;
                        let message = encode_proto_response(response).unwrap_or_else(|mut error_response| {
                            error_response.message_id = message_id;
                            encode_proto_response(error_response).unwrap_or_else(|_| {
                                // If we somehow fail to encode the error response, return a close message as a last resort
                                axum::extract::ws::Message::Close(None)
                            })
                        });
                        let mut writer = ctx.ws_writer.lock().await;
                        if writer.send(message).await.is_err() {
                            eprintln!("Failed to send response");
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to decode RequestMessage: {}", e);
                    }
                }
            }
            Ok(axum::extract::ws::Message::Close(_)) => {
                println!("WebSocket closed by client");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                ctx.set_is_player_connected(false);
                eprintln!("WebSocket error: {}", e);
                break;
            }
        }
    }
}

async fn handle_request(
    request: ProtoRequest,
    ctx: Arc<ConnectionContext>,
) -> Result<ProtoResponse, ProtoResponse> {
    if let Some(any_payload) = request.any_payload {
        let type_url = any_payload.type_url.as_str();
        let handler = HANDLER_REGISTRY_BY_ANY_TYPE.get(type_url);
        if let Some(handler) = handler {
            return handler
                .handle(
                    request.message_timestamp,
                    &any_payload.value,
                    ctx,
                )
                .await;
        }
        return Err(build_error_response(
            ErrorCode::InvalidRequest.into(),
            "No handler found for this request type",
        ));
    }
    Err(build_error_response(
        ErrorCode::InvalidRequest.into(),
        "No any_payload found in request",
    ))
}
