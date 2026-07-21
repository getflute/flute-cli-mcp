use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{Id, flute_err_to_result, value_to_result};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettlementsList {
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub limit: Option<u32>,
    /// Zero-based page index: 0 (or omit) is the first page, 1 the second, etc.
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub page: Option<u32>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    /// "open" | "settled".
    #[serde(default)]
    pub status: Option<String>,
}

#[tool_router(router = settlements_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "List settlement batches. Safe to retry.")]
    pub async fn settlements_list(
        &self,
        Parameters(p): Parameters<SettlementsList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["settlements".into(), "list".into()]);
        if let Some(v) = p.limit {
            args.extend(["--limit".into(), v.to_string()]);
        }
        if let Some(v) = p.page {
            args.extend(["--page".into(), v.to_string()]);
        }
        if let Some(v) = p.from {
            args.extend(["--from".into(), v]);
        }
        if let Some(v) = p.to {
            args.extend(["--to".into(), v]);
        }
        if let Some(v) = p.status {
            args.extend(["--status".into(), v]);
        }
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Get a settlement batch by id (client-side filter over the fetched page; page-bounded). Safe to retry."
    )]
    pub async fn settlements_get(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["settlements".into(), "get".into(), p.id]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }
}
