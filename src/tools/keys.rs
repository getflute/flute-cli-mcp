//! ISV API key tools — `flute keys …` (ARISE-4706).
//!
//! Renamed from `tokens` to track the CLI, which made `keys` the documented
//! command and left `tokens` only as a deprecated hidden alias. These tools
//! invoke `keys`, so they require flute >= v1.1.0.

use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{ack_envelope, flute_err_to_result, value_to_result};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeysList {
    /// Overrides the pinned FLUTE_MERCHANT_ID; optional for list.
    #[serde(default)]
    pub merchant_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyCreate {
    pub name: String,
    /// Overrides the pinned FLUTE_MERCHANT_ID; required (here or via env).
    #[serde(default)]
    pub merchant_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeyRevoke {
    pub client_id: String,
    #[serde(default)]
    pub merchant_id: Option<String>,
}

#[tool_router(router = keys_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(
        description = "List ISV API keys for a merchant. Uses merchant_id, else the pinned FLUTE_MERCHANT_ID, else lists without a merchant filter. Safe to retry."
    )]
    pub async fn keys_list(
        &self,
        Parameters(p): Parameters<KeysList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["keys".into(), "list".into()]);
        if let Some(m) = p
            .merchant_id
            .filter(|s| !s.is_empty())
            .or_else(|| self.config.merchant_id.clone())
        {
            args.extend(["--merchant-id".into(), m]);
        }
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Create an ISV API key. NOT idempotent. The response includes `clientSecret` shown ONCE — store it; the API never returns it again. Requires merchant_id (here or via FLUTE_MERCHANT_ID)."
    )]
    pub async fn keys_create(
        &self,
        Parameters(p): Parameters<KeyCreate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("keys_create") {
            return Ok(blocked);
        }
        let merchant_id = match self.merchant_id_for(p.merchant_id) {
            Ok(m) => m,
            Err(e) => return Ok(flute_err_to_result(e)),
        };
        let mut args = self.base_args();
        args.extend([
            "keys".into(),
            "create".into(),
            "--merchant-id".into(),
            merchant_id,
            "--name".into(),
            p.name,
        ]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Revoke an ISV API key. --merchant-id is required (DELETE needs it). 404 on repeat = idempotent."
    )]
    pub async fn keys_revoke(
        &self,
        Parameters(p): Parameters<KeyRevoke>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("keys_revoke") {
            return Ok(blocked);
        }
        let merchant_id = match self.merchant_id_for(p.merchant_id) {
            Ok(m) => m,
            Err(e) => return Ok(flute_err_to_result(e)),
        };
        let client_id = p.client_id;
        let mut args = self.base_args();
        args.extend([
            "keys".into(),
            "revoke".into(),
            "--client-id".into(),
            client_id.clone(),
            "--merchant-id".into(),
            merchant_id,
            "--yes".into(),
        ]);
        Ok(match self.run_cli(args).await {
            // No body on success (empty stdout -> Null); synthesize a structured result.
            // The envelope object stays `api_token_revoked`: the CLI renamed the
            // command, not the `api_token`/`api_token_list` envelope vocabulary.
            Ok(v) if v.is_null() => value_to_result(ack_envelope(
                "api_token_revoked",
                serde_json::json!({ "client_id": client_id, "revoked": true }),
                self.config.profile.as_cli_str(),
            )),
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }
}
