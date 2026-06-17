use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use crate::error::FluteError;

pub mod util;
pub mod transactions;
pub mod ach;
pub mod customers;
pub mod terminals;
pub mod devices;
pub mod pos;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

/// A bare resource id, used by the many `get`/`status`/positional tools.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Id {
    /// The resource id.
    pub id: String,
}

pub(crate) fn value_to_result(value: Value) -> CallToolResult {
    CallToolResult::success(vec![
        Content::json(value).expect("serde_json::Value is always JSON-serializable"),
    ])
}

pub(crate) fn flute_err_to_result(err: FluteError) -> CallToolResult {
    let payload = match &err {
        FluteError::Api { status, message, correlation_id } => serde_json::json!({
            "kind": "api", "status": status, "message": message, "correlation_id": correlation_id,
        }),
        FluteError::Transport { message } => serde_json::json!({ "kind": "transport", "message": message }),
        FluteError::Auth { message } => serde_json::json!({
            "kind": "auth", "message": format!("{message} — run `flute auth login`"),
        }),
        FluteError::Decode { message } => serde_json::json!({ "kind": "decode", "message": message }),
        FluteError::Client { message } => serde_json::json!({ "kind": "client", "message": message }),
        FluteError::Spawn(msg) => serde_json::json!({
            "kind": "spawn",
            "message": format!("could not spawn flute — set FLUTE_BIN or install the CLI ({msg})"),
        }),
        FluteError::Timeout { secs } => serde_json::json!({
            "kind": "timeout", "message": format!("flute timed out after {secs}s"),
        }),
        FluteError::BadOutput { exit_code, stdout, stderr } => {
            let stderr_trunc = if stderr.len() > 4096 {
                &stderr[..stderr.floor_char_boundary(4096)]
            } else {
                stderr.as_str()
            };
            serde_json::json!({
                "kind": "bad_output", "exit_code": exit_code, "stdout": stdout, "stderr": stderr_trunc,
            })
        }
    };
    CallToolResult::error(vec![
        Content::json(payload).expect("serde_json::Value is always JSON-serializable"),
    ])
}
