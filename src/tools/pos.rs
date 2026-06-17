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
pub struct PosList {
    #[serde(default)]
    pub terminal_id: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub page: Option<u32>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PosCreate {
    pub terminal_id: String,
    /// Amount as a decimal string, e.g. "10.00".
    pub amount: String,
    pub pos_device_id: String,
    pub reference_id: String,
    /// Currency id; default 1.
    #[serde(default)]
    pub currency_id: Option<u32>,
    /// Transaction type; default 2 = Sale (1=Auth,3=Capture,4=Void,5=Refund).
    #[serde(default)]
    pub transaction_type: Option<u32>,
    #[serde(default)]
    pub tip_amount: Option<String>,
    #[serde(default)]
    pub tip_rate: Option<String>,
    #[serde(default)]
    pub customer_id: Option<String>,
    #[serde(default)]
    pub payment_processor_id: Option<String>,
    #[serde(default)]
    pub target_transaction_id: Option<String>,
    #[serde(default)]
    pub reading_method: Option<String>,
}

#[tool_router(router = pos_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(
        description = "Get a POS transaction by id. get/list use `id` + `posTransactionStatus`. Safe to retry."
    )]
    pub async fn pos_get(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["pos".into(), "get".into(), p.id]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "List POS transactions, optionally by terminal. Safe to retry.")]
    pub async fn pos_list(
        &self,
        Parameters(p): Parameters<PosList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["pos".into(), "list".into()]);
        if let Some(v) = p.terminal_id {
            args.extend(["--terminal-id".into(), v]);
        }
        if let Some(v) = p.limit {
            args.extend(["--limit".into(), v.to_string()]);
        }
        if let Some(v) = p.page {
            args.extend(["--page".into(), v.to_string()]);
        }
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Start a terminal (POS) transaction. NOT idempotent. Does not wait — poll pos_get until the response shows completion. A terminal allows only one in-progress transaction; cancel or complete before starting another. create/cancel responses use `posTransactionId` + `status`."
    )]
    pub async fn pos_create(
        &self,
        Parameters(p): Parameters<PosCreate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("pos_create") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "pos".into(),
            "create".into(),
            "--terminal-id".into(),
            p.terminal_id,
            "--amount".into(),
            p.amount,
            "--pos-device-id".into(),
            p.pos_device_id,
            "--reference-id".into(),
            p.reference_id,
        ]);
        if let Some(v) = p.currency_id {
            args.extend(["--currency-id".into(), v.to_string()]);
        }
        if let Some(v) = p.transaction_type {
            args.extend(["--transaction-type".into(), v.to_string()]);
        }
        if let Some(v) = p.tip_amount {
            args.extend(["--tip-amount".into(), v]);
        }
        if let Some(v) = p.tip_rate {
            args.extend(["--tip-rate".into(), v]);
        }
        if let Some(v) = p.customer_id {
            args.extend(["--customer-id".into(), v]);
        }
        if let Some(v) = p.payment_processor_id {
            args.extend(["--payment-processor-id".into(), v]);
        }
        if let Some(v) = p.target_transaction_id {
            args.extend(["--target-transaction-id".into(), v]);
        }
        if let Some(v) = p.reading_method {
            args.extend(["--reading-method".into(), v]);
        }
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Cancel an in-progress POS transaction by id. 404 on repeat = idempotent."
    )]
    pub async fn pos_cancel(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("pos_cancel") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend(["pos".into(), "cancel".into(), p.id]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }
}
