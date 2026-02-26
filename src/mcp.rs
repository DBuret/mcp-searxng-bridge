// mcp.rs
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Debug)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Serialize, Debug)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Value,
}