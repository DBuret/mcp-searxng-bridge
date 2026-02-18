mod error;
mod mcp;
mod state;
mod handlers;

use axum::{
    extract::State,
    http::{StatusCode, HeaderMap},
    response::{sse::{Event, KeepAlive, Sse}, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use std::{sync::Arc, net::SocketAddr, time::Duration};
use tokio::sync::broadcast;
use futures::stream::{self, Stream};
use tracing::{info, warn, error};
use tower_http::trace::TraceLayer;
use serde_json::{json, Value};

use crate::state::AppState;
use crate::mcp::{McpRequest, McpResponse};
use crate::handlers::messages::{call_searxng, fetch_url};

#[tokio::main]
async fn main() {
    let log_level = std::env::var("MCP_SX_LOG").unwrap_or_else(|_| "info".into());
    tracing_subscriber::fmt().with_env_filter(log_level).init();

    let (tx, _) = broadcast::channel(100);
    let state = Arc::new(AppState::new(tx));

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        // LMStudio exige le POST sur /sse pour l'initialisation
        .route("/sse", get(sse_handler).post(messages_handler))
        .route("/messages", post(messages_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("🚀 MCP SearXNG Bridge started on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn sse_handler(State(state): State<Arc<AppState>>) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = state.tx.subscribe();
    let stream = stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Ok(msg) => Some((Ok(Event::default().data(msg)), rx)),
            Err(_) => None,
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::new())
}

async fn messages_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<McpRequest>,
) -> impl IntoResponse {
    let tx = state.tx.clone();
    let method = payload.method.clone();
    let request_id = payload.id.clone().unwrap_or(Value::Null);

    // --- STRATÉGIE HYBRIDE POUR LMSTUDIO ---
    // Si c'est une initialisation, on répond DIRECTEMENT en HTTP 200.
    // Cela évite d'attendre l'ouverture du tunnel SSE qui arrive trop tard chez LMStudio.
    if method == "initialize" {
        info!("Handling 'initialize' via direct HTTP response");
        let result = handle_initialize();
        let response = McpResponse { jsonrpc: "2.0".into(), id: request_id, result };
        return (StatusCode::OK, Json(response)).into_response();
    }

    // Pour les outils (search/fetch), on utilise le spawn asynchrone + SSE
    tokio::spawn(async move {
        if request_id.is_null() && method != "notifications/initialized" { return; }

        let result = match method.as_str() {
            "tools/list" => handle_list_tools(),
            "tools/call" => {
                let tool_name = payload.params.as_ref().and_then(|p| p.get("name")?.as_str()).unwrap_or("");
                let args = payload.params.as_ref().and_then(|p| p.get("arguments"));
                
                let res = match tool_name {
                    "search" => {
                        let query = args.and_then(|a| a.get("query")?.as_str()).unwrap_or("");
                        call_searxng(&state, query).await
                    },
                    "fetch_page" => {
                        let url = args.and_then(|a| a.get("url")?.as_str()).unwrap_or("");
                        fetch_url(&state, url).await
                    },
                    _ => Err(crate::error::BridgeError::Api(format!("Unknown tool: {}", tool_name))),
                };

                match res {
                    Ok(t) => json!({ "content": [{ "type": "text", "text": t }] }),
                    Err(e) => {
                        error!(error = %e, "Tool call failed");
                        json!({ "isError": true, "content": [{ "type": "text", "text": e.to_string() }] })
                    }
                }
            },
            "notifications/initialized" => return,
            _ => json_error(&format!("Method {} not supported", method)),
        };

        let response = McpResponse { jsonrpc: "2.0".into(), id: request_id, result };
        if let Ok(json_msg) = serde_json::to_string(&response) {
            // Tentative d'envoi via SSE avec un petit retry si le tunnel s'ouvre juste après
            let mut delivered = false;
            for _ in 0..3 {
                if tx.send(json_msg.clone()).is_ok() {
                    delivered = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            if !delivered {
                warn!("Could not deliver {} via SSE (no client connected)", method);
            }
        }
    });

    StatusCode::ACCEPTED.into_response()
}

fn handle_initialize() -> Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "mcp-searxng-bridge", "version": "1.0.0" }
    })
}

fn handle_list_tools() -> Value {
    json!({
        "tools": [
            {
                "name": "search",
                "description": "Search the web via SearXNG",
                "inputSchema": {
                    "type": "object",
                    "properties": { "query": { "type": "string" } },
                    "required": ["query"]
                }
            },
            {
                "name": "fetch_page",
                "description": "Get the content of a web page as Markdown",
                "inputSchema": {
                    "type": "object",
                    "properties": { "url": { "type": "string" } },
                    "required": ["url"]
                }
            }
        ]
    })
}

fn json_error(msg: &str) -> Value {
    json!({ "isError": true, "content": [{ "type": "text", "text": msg }] })
}