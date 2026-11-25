//! Oxide MCP - Model Context Protocol server exposing DAW actions to AI agents.

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct McpToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpToolResult {
    pub success: bool,
    pub data: serde_json::Value,
}

mod server;
pub use server::{serve, serve_editable};
