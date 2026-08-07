use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};

use crate::server::FluteServer;
use crate::tools::{Empty, flute_err_to_result, value_to_result};

#[tool_router(router = util_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "API health check. Pure read; safe to retry.")]
    pub async fn ping(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.push("ping".into());
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "Print the CLI version and active profile. Safe to retry.")]
    pub async fn version(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.push("version".into());
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Live authentication check for the active profile: the CLI pings the API, so `authenticated` is true only when the stored credentials actually round-trip (not merely that credentials exist). Returns `{authenticated, profile}` plus `api_base_url`/`client_id`/`merchant_id` when known; never returns the secret. This one costs a network round-trip. Safe to retry."
    )]
    pub async fn auth_status(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["auth".into(), "status".into()]);
        Ok(match self.run_cli(args).await {
            Ok(v) => {
                let data = v.get("data");
                let field = |k: &str| data.and_then(|d| d.get(k));
                // ARISE-4706 replaced the offline `has_credentials` flag with a
                // live `authenticated` check. Fall back to the old key so the
                // tool keeps reporting correctly against a pre-v1.1.0 CLI.
                let authenticated = field("authenticated")
                    .or_else(|| field("has_credentials"))
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                let mut out = serde_json::json!({
                    "authenticated": authenticated,
                    "profile": self.config.profile.as_cli_str(),
                });
                // `api_base_url` names the environment actually being hit;
                // `client_id`/`merchant_id` are server-authoritative when the
                // ping succeeded. All three are absent on an older CLI and may
                // be null when unknown, so only forward real strings.
                for key in ["api_base_url", "client_id", "merchant_id"] {
                    if let Some(s) = field(key).and_then(|v| v.as_str()) {
                        out[key] = serde_json::Value::String(s.to_string());
                    }
                }
                value_to_result(out)
            }
            Err(e) => flute_err_to_result(e),
        })
    }
}
