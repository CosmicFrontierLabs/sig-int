use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;

use shared::protocol::{ClientMsg, ServerMsg};
use shared::temporal::ViewQuery;

use crate::AppState;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    tracing::info!("WebSocket client connected");

    while let Some(Ok(msg)) = socket.recv().await {
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };

        let client_msg: ClientMsg = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                let err = ServerMsg::Error {
                    message: format!("Invalid message: {}", e),
                };
                let _ = socket
                    .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                    .await;
                continue;
            }
        };

        let responses = handle_client_msg(client_msg, &state).await;

        for resp in responses {
            let json = serde_json::to_string(&resp).unwrap();
            if socket.send(Message::Text(json.into())).await.is_err() {
                return;
            }
        }
    }

    tracing::info!("WebSocket client disconnected");
}

async fn handle_client_msg(msg: ClientMsg, state: &AppState) -> Vec<ServerMsg> {
    match msg {
        ClientMsg::Ping => vec![ServerMsg::Heartbeat],

        ClientMsg::GetManifest => {
            let registry = state.registry.read().await;
            vec![ServerMsg::Manifest {
                manifest: registry.manifest(),
            }]
        }

        ClientMsg::Query {
            time_range,
            resolution_ns,
            generation,
        } => {
            let registry = state.registry.read().await;
            let query = ViewQuery {
                time_range,
                resolution_ns,
            };
            let results = registry.query(&query);
            results
                .into_iter()
                .map(|(path, view)| ServerMsg::Tile { path, view, generation })
                .collect()
        }

        ClientMsg::UpdateSource { text } => {
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(_) => {
                    vec![ServerMsg::Error {
                        message: "WaveJSON import not yet implemented — using built-in I2C example"
                            .to_string(),
                    }]
                }
                Err(e) => vec![ServerMsg::Error {
                    message: format!("Parse error: {}", e),
                }],
            }
        }
    }
}
