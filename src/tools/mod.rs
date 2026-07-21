use rmcp::model::{CallToolResult, Content};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::error::FluteError;

/// Deserialize an optional `u32` that may arrive as a JSON number **or** a
/// numeric string. MCP clients (LLMs) frequently send integers as quoted
/// strings; without this, serde rejects them with
/// `invalid type: string "..", expected u32` (surfaced to the client as
/// JSON-RPC `-32602`). A non-numeric string is still an error, and native
/// numbers keep working. (ARISE-4505 BUG-03.)
pub(crate) fn de_flexible_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u32),
        Str(String),
    }
    match Option::<NumOrStr>::deserialize(deserializer)? {
        None => Ok(None),
        Some(NumOrStr::Num(n)) => Ok(Some(n)),
        Some(NumOrStr::Str(s)) => {
            let trimmed = s.trim();
            trimmed.parse::<u32>().map(Some).map_err(|_| {
                serde::de::Error::custom(format!("expected an integer, got string {trimmed:?}"))
            })
        }
    }
}

/// Required-field counterpart of [`de_flexible_u32`]: accepts a JSON number or a
/// numeric string but rejects `null`. Absent fields are still reported as
/// missing by the derived `Deserialize` (this runs only when the field is
/// present). (ARISE-4505 BUG-03.)
pub(crate) fn de_flexible_u32_req<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    de_flexible_u32(deserializer)?
        .ok_or_else(|| serde::de::Error::custom("expected an integer, got null"))
}

pub mod ach;
pub mod customers;
pub mod devices;
pub mod pos;
pub mod settlements;
pub mod subscriptions;
pub mod terminals;
pub mod tokens;
pub mod transactions;
pub mod util;

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

/// Build a synthetic success envelope for operations whose CLI command returns an empty
/// body (delete / remove-method / revoke). Without this, an empty-stdout success surfaces
/// to the client as a bare JSON `null`; this gives the same `{object, data, meta}` shape
/// the other tools return.
pub(crate) fn ack_envelope(object: &str, data: Value, environment: &str) -> Value {
    serde_json::json!({
        "object": object,
        "data": data,
        "meta": { "environment": environment },
    })
}

/// Truncate raw CLI output to at most 4 KiB on a UTF-8 char boundary before embedding
/// it in an error payload, so a non-JSON failure body can't leak large or sensitive
/// content back to the client or balloon the response.
fn truncate_4k(s: &str) -> &str {
    const MAX: usize = 4096;
    if s.len() > MAX {
        &s[..s.floor_char_boundary(MAX)]
    } else {
        s
    }
}

pub(crate) fn flute_err_to_result(err: FluteError) -> CallToolResult {
    let payload = match &err {
        FluteError::Api {
            status,
            message,
            correlation_id,
        } => serde_json::json!({
            "kind": "api", "status": status, "message": message, "correlation_id": correlation_id,
        }),
        FluteError::Transport { message } => {
            serde_json::json!({ "kind": "transport", "message": message })
        }
        FluteError::Auth { message } => serde_json::json!({
            "kind": "auth", "message": format!("{message} — run `flute auth login`"),
        }),
        FluteError::Decode { message } => {
            serde_json::json!({ "kind": "decode", "message": message })
        }
        FluteError::Client { message } => {
            serde_json::json!({ "kind": "client", "message": message })
        }
        FluteError::Spawn(msg) => serde_json::json!({
            "kind": "spawn",
            "message": format!("could not spawn flute — set FLUTE_BIN or install the CLI ({msg})"),
        }),
        FluteError::Timeout { secs } => serde_json::json!({
            "kind": "timeout", "message": format!("flute timed out after {secs}s"),
        }),
        FluteError::BadOutput {
            exit_code,
            stdout,
            stderr,
        } => serde_json::json!({
            "kind": "bad_output",
            "exit_code": exit_code,
            "stdout": truncate_4k(stdout),
            "stderr": truncate_4k(stderr),
        }),
    };
    CallToolResult::error(vec![
        Content::json(payload).expect("serde_json::Value is always JSON-serializable"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_4k_passes_short_input_through() {
        assert_eq!(truncate_4k("hello"), "hello");
        assert_eq!(truncate_4k(""), "");
    }

    #[test]
    fn truncate_4k_caps_long_ascii_at_4096() {
        let big = "x".repeat(5000);
        assert_eq!(truncate_4k(&big).len(), 4096);
    }

    #[test]
    fn truncate_4k_respects_utf8_boundary() {
        // 4095 ASCII bytes then a 2-byte char straddling index 4096:
        // must cut back to 4095 rather than split the char.
        let mut s = "a".repeat(4095);
        s.push('é');
        let out = truncate_4k(&s);
        assert_eq!(out.len(), 4095);
        assert!(s.is_char_boundary(out.len()));
    }

    #[test]
    fn bad_output_payload_is_truncated_and_marked_error() {
        let err = FluteError::BadOutput {
            exit_code: 1,
            stdout: "Z".repeat(10_000),
            stderr: "E".repeat(10_000),
        };
        let result = flute_err_to_result(err);
        assert_eq!(result.is_error, Some(true));
    }
}
