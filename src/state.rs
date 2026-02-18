use std::env;
use tokio::sync::broadcast;

pub struct AppState {
    pub searxng_url: String,
    pub client: reqwest::Client,
    pub tx: broadcast::Sender<String>,
}

impl AppState {
    pub fn new(tx: broadcast::Sender<String>) -> Self {
        // Préfixe unique pour tes variables d'environnement
        let prefix = "MCP_SX";

        let searxng_url = env::var(format!("{}_URL", prefix))
            .unwrap_or_else(|_| "http://172.17.0.1:18080".into());

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent("MCP-SearXNG-Rust-Bridge/1.0")
            .build()
            .expect("Failed to create reqwest client");

        Self {
            searxng_url,
            client,
            tx,
        }
    }
}