use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};

use crate::server::FluteServer;
use crate::tools::{Empty, Id, flute_err_to_result, value_to_result};

#[tool_router(router = terminals_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "List POS terminals. Safe to retry.")]
    pub async fn terminals_list(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["terminals".into(), "list".into()]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Get a terminal's status by id. A terminal must be SemiIntegrated + Online to accept POS transactions. Safe to retry.")]
    pub async fn terminals_status(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["terminals".into(), "status".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }
}
