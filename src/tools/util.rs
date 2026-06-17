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
        description = "Check whether credentials are present for the active profile. Returns `{authenticated, profile}`; never returns the token. Safe to retry."
    )]
    pub async fn auth_status(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["auth".into(), "status".into()]);
        Ok(match self.run_cli(args).await {
            Ok(v) => {
                let authenticated = v
                    .get("data")
                    .and_then(|d| d.get("has_credentials"))
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                value_to_result(serde_json::json!({
                    "authenticated": authenticated,
                    "profile": self.config.profile.as_cli_str(),
                }))
            }
            Err(e) => flute_err_to_result(e),
        })
    }
}
