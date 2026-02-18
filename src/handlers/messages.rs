use crate::state::AppState;
use crate::error::BridgeError;
use tracing::{instrument};
use std::time::Instant;

#[instrument(skip(state), fields(query = %query))]
pub async fn call_searxng(state: &AppState, query: &str) -> Result<String, BridgeError> {
    if query.is_empty() { return Ok("Query is empty".into()); }

    let params = [("q", query), ("format", "json"), ("language", "en-US")];
    let start = Instant::now();
    
    let resp = state.client
        .get(&format!("{}/search", state.searxng_url))
        .query(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(BridgeError::Api(format!("HTTP {}", resp.status())));
    }

    let json: serde_json::Value = resp.json().await?;
    let mut out = String::new();
    
    if let Some(results) = json.get("results").and_then(|r| r.as_array()) {
        for res in results.iter().take(5) {
            let title = res.get("title").and_then(|t| t.as_str()).unwrap_or("");
            let content = res.get("content").and_then(|c| c.as_str()).unwrap_or("");
            let url = res.get("url").and_then(|u| u.as_str()).unwrap_or("");
            out.push_str(&format!("### {}\n{}\nSource: {}\n\n", title, content, url));
        }
    }

    Ok(if out.is_empty() { "No results found".into() } else { out })
}