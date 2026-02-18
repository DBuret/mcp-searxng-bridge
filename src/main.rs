mod error;
mod mcp;
mod state;
mod handlers;

use axum::{
    extract::{State}, 
    http::{StatusCode, HeaderMap}, 
    response::{sse::{Event, KeepAlive, Sse}, IntoResponse}, 
    routing::{get, post}, 
    Json, Router
};
use std::{sync::Arc, net::SocketAddr};
use tokio::sync::broadcast;
use futures::stream::{self, Stream};
use tracing::{info, warn};
use tower_http::trace::TraceLayer;

use crate::state::AppState;
use crate::mcp::{McpRequest, McpResponse};
use crate::handlers::messages::call_searxng;

#[tokio::main]
async fn main() {
    let log_level = std::env::var("MCP_SX_LOG").unwrap_or_else(|_| "info".into());
    tracing_subscriber::fmt().with_env_filter(log_level).init();

    let (tx, _) = broadcast::channel(100);
    let state = Arc::new(AppState::new(tx));

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/sse", get(sse_handler))
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
    let client_ip = headers.get("x-forwarded-for").and_then(|h| h.to_str().ok()).unwrap_or("unknown").to_string();

    tokio::spawn(async move {
        let method = payload.method.clone();
        let request_id = payload.id.clone().unwrap_or(serde_json::Value::Null);
        let mut backend_ms = 0;

        let result = match method.as_str() {
            "tools/call" => {
                let query = payload.params.as_ref()
                    .and_then(|p| p.get("arguments")?.get("query")?.as_str())
                    .unwrap_or("");
                let start = std::time::Instant::now();
                let res = call_searxng(&state, query).await;
                backend_ms = start.elapsed().as_millis();
                
                match res {
                    Ok(t) => serde_json::json!({ "content": [{ "type": "text", "text": t }] }),
                    Err(e) => serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": e.to_string() }] }),
                }
            },
            _ => serde_json::json!({ "isError": true, "content": [{ "type": "text", "text": "Unsupported method" }] }),
        };

        info!(target: "mcp_access_log", "src={} method=\"{}\" backend_ms={} status=processed", client_ip, method, backend_ms);

        let response = McpResponse { jsonrpc: "2.0".into(), id: request_id, result };
        if let Ok(json) = serde_json::to_string(&response) {
            if let Err(_) = tx.send(json) {
                warn!("No SSE client to receive response for {}", method);
            }
        }
    });

    StatusCode::ACCEPTED
}