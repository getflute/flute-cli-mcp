use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{flute_err_to_result, value_to_result};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TokensList {
    /// Overrides the pinned FLUTE_MERCHANT_ID; optional for list.
    #[serde(default)]
    pub merchant_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenCreate {
    pub name: String,
    /// Overrides the pinned FLUTE_MERCHANT_ID; required (here or via env).
    #[serde(default)]
    pub merchant_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenRevoke {
    pub client_id: String,
    #[serde(default)]
    pub merchant_id: Option<String>,
}

#[tool_router(router = tokens_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(
        description = "List ISV API tokens for a merchant. Uses merchant_id, else the pinned FLUTE_MERCHANT_ID, else lists without a merchant filter. Safe to retry."
    )]
    pub async fn tokens_list(
        &self,
        Parameters(p): Parameters<TokensList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["tokens".into(), "list".into()]);
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
        description = "Create an ISV API token. NOT idempotent. The response includes `clientSecret` shown ONCE — store it; the API never returns it again. Requires merchant_id (here or via FLUTE_MERCHANT_ID)."
    )]
    pub async fn tokens_create(
        &self,
        Parameters(p): Parameters<TokenCreate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("tokens_create") {
            return Ok(blocked);
        }
        let merchant_id = match self.merchant_id_for(p.merchant_id) {
            Ok(m) => m,
            Err(e) => return Ok(flute_err_to_result(e)),
        };
        let mut args = self.base_args();
        args.extend([
            "tokens".into(),
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
        description = "Revoke an ISV API token. --merchant-id is required (DELETE needs it). 404 on repeat = idempotent."
    )]
    pub async fn tokens_revoke(
        &self,
        Parameters(p): Parameters<TokenRevoke>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("tokens_revoke") {
            return Ok(blocked);
        }
        let merchant_id = match self.merchant_id_for(p.merchant_id) {
            Ok(m) => m,
            Err(e) => return Ok(flute_err_to_result(e)),
        };
        let mut args = self.base_args();
        args.extend([
            "tokens".into(),
            "revoke".into(),
            "--client-id".into(),
            p.client_id,
            "--merchant-id".into(),
            merchant_id,
            "--yes".into(),
        ]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }
}
