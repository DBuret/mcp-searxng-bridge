use crate::state::AppState;
use crate::error::BridgeError;
use tracing::instrument;
use std::time::Instant;
use scraper::{Html, Selector};
use html2md::parse_html;

#[instrument(skip(state), fields(query = %query))]
pub async fn call_searxng(state: &AppState, query: &str) -> Result<String, BridgeError> {
    if query.is_empty() { return Ok("Query is empty".into()); }

    let params = [("q", query), ("format", "json"), ("language", "en-US")];
    
    let resp = state.client
        .get(&format!("{}/search", state.searxng_url))
        .query(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(BridgeError::Api(format!("SearXNG error: HTTP {}", resp.status())));
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

#[instrument(skip(state), fields(url = %url))]
pub async fn fetch_url(state: &AppState, url: &str) -> Result<String, BridgeError> {
    if url.is_empty() { return Ok("URL is empty".into()); }

    let resp = state.client
        .get(url)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Ok(format!("Impossible de lire la page : Erreur HTTP {}", resp.status()));
    }

    let html_content = resp.text().await?;
    
    // Parsing et extraction du contenu principal
    let document = Html::parse_document(&html_content);
    
    // On cible les zones de texte probable pour éviter les menus/footers
    let selectors = ["article", "main", ".content", "#content", "body"];
    let mut best_fragment = String::new();

    for selector_str in selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(element) = document.select(&selector).next() {
                best_fragment = element.html();
                if selector_str == "article" || selector_str == "main" {
                    break;
                }
            }
        }
    }

    // Conversion en Markdown
    let markdown = if !best_fragment.is_empty() {
        parse_html(&best_fragment)
    } else {
        parse_html(&html_content)
    };

    let cleaned = markdown.trim();
    
    Ok(if cleaned.is_empty() {
        "La page a été chargée mais aucun contenu textuel n'a pu être extrait.".into()
    } else if cleaned.len() > 15000 {
        format!("{}...\n\n(Contenu tronqué car trop long)", &cleaned[..15000])
    } else {
        cleaned.to_string()
    })
}