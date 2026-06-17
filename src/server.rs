use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::ServerInfo,
    tool_handler,
};
use serde_json::Value;

use crate::config::{Config, Profile};
use crate::error::FluteError;
use crate::runner::CliRunner;
use crate::tools::flute_err_to_result;

#[derive(Clone)]
pub struct FluteServer {
    pub(crate) config: Arc<Config>,
    pub(crate) runner: Arc<dyn CliRunner>,
    tool_router: ToolRouter<FluteServer>,
}

impl FluteServer {
    pub fn new(config: Arc<Config>, runner: Arc<dyn CliRunner>) -> Self {
        // Each group module contributes a router via `#[tool_router(router = …)]`.
        // Extend this chain as group modules are added (Tasks 6–12).
        let tool_router = Self::util_router() + Self::transactions_router();
        Self { config, runner, tool_router }
    }

    pub(crate) fn base_args(&self) -> Vec<String> {
        vec![
            "--profile".into(),
            self.config.profile.as_cli_str().into(),
            "--output".into(),
            "json".into(),
        ]
    }

    pub(crate) async fn run_cli(&self, args: Vec<String>) -> Result<Value, FluteError> {
        self.runner.run(&args).await
    }

    /// Returns `Some(error_result)` when a write must be refused on a guarded
    /// production instance; `None` when the write may proceed.
    pub(crate) fn guard_write(&self, tool: &str) -> Option<rmcp::model::CallToolResult> {
        if self.config.profile == Profile::Production && !self.config.allow_prod_writes {
            Some(flute_err_to_result(FluteError::Client {
                message: format!(
                    "refusing {tool} on production profile; set FLUTE_MCP_ALLOW_PROD_WRITES=1 to enable writes"
                ),
            }))
        } else {
            None
        }
    }

    /// Resolve the merchant id for token tools: per-call override, else the
    /// pinned `FLUTE_MERCHANT_ID`, else a `client` error.
    pub(crate) fn merchant_id_for(&self, override_id: Option<String>) -> Result<String, FluteError> {
        override_id
            .filter(|s| !s.is_empty())
            .or_else(|| self.config.merchant_id.clone())
            .ok_or_else(|| FluteError::Client {
                message: "merchant_id required: pass `merchant_id` or set FLUTE_MERCHANT_ID".into(),
            })
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for FluteServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default().with_instructions(
            "Drives the `flute` payments CLI. The active profile is pinned at server start; \
             launch one instance per environment (sandbox vs production). On a production \
             instance, write tools are refused unless FLUTE_MCP_ALLOW_PROD_WRITES=1. \
             Credentials come from the OS keychain (`flute auth login`) or FLUTE_CLIENT_ID/\
             FLUTE_CLIENT_SECRET in this server's environment.",
        )
    }
}
