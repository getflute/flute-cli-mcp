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
pub struct TransactionsList {
    /// Page size (maps to the API `pageSize`).
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub limit: Option<u32>,
    /// Zero-based page index: 0 (or omit) is the first page, 1 the second, etc.
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub page: Option<u32>,
    /// Only unsettled transactions.
    #[serde(default)]
    pub unsettled: Option<bool>,
    /// Client-side status filter applied to the returned page.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
}

/// Shared input for `sale` and `auth` (identical flag set).
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaleArgs {
    /// Amount as a decimal string, e.g. "10.00" (≤2 decimal places).
    pub amount: String,
    #[serde(default)]
    pub card: Option<String>,
    /// Expiry as MM/YY or MM/YYYY.
    #[serde(default)]
    pub exp: Option<String>,
    #[serde(default)]
    pub cvv: Option<String>,
    #[serde(default)]
    pub tip_amount: Option<String>,
    #[serde(default)]
    pub customer_id: Option<String>,
    #[serde(default)]
    pub payment_method_id: Option<String>,
    /// Currency id; API default 1 = USD.
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub currency_id: Option<u32>,
    /// Card data source; CLI default 1 = Internet/ISV.
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub card_data_source: Option<u32>,
    #[serde(default)]
    pub reference_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TxnRef {
    pub transaction_id: String,
    /// Optional partial amount (capture/refund).
    #[serde(default)]
    pub amount: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TipAdjust {
    pub transaction_id: String,
    pub tip_amount: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Settle {
    pub payment_processor_id: String,
}

impl FluteServer {
    /// Shared argv builder for `sale`/`auth`.
    fn sale_like_args(&self, sub: &str, p: SaleArgs) -> Vec<String> {
        let mut args = self.base_args();
        args.extend([
            "transactions".into(),
            sub.into(),
            "--amount".into(),
            p.amount,
        ]);
        if let Some(v) = p.card {
            args.extend(["--card".into(), v]);
        }
        if let Some(v) = p.exp {
            args.extend(["--exp".into(), v]);
        }
        if let Some(v) = p.cvv {
            args.extend(["--cvv".into(), v]);
        }
        if let Some(v) = p.tip_amount {
            args.extend(["--tip-amount".into(), v]);
        }
        if let Some(v) = p.customer_id {
            args.extend(["--customer-id".into(), v]);
        }
        if let Some(v) = p.payment_method_id {
            args.extend(["--payment-method-id".into(), v]);
        }
        if let Some(v) = p.currency_id {
            args.extend(["--currency-id".into(), v.to_string()]);
        }
        if let Some(v) = p.card_data_source {
            args.extend(["--card-data-source".into(), v.to_string()]);
        }
        if let Some(v) = p.reference_id {
            args.extend(["--reference-id".into(), v]);
        }
        args
    }
}

#[tool_router(router = transactions_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(
        description = "List transactions (newest first). Filters --status/--from/--to are applied client-side to the returned page. Safe to retry."
    )]
    pub async fn transactions_list(
        &self,
        Parameters(p): Parameters<TransactionsList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["transactions".into(), "list".into()]);
        if let Some(v) = p.limit {
            args.extend(["--limit".into(), v.to_string()]);
        }
        if let Some(v) = p.page {
            args.extend(["--page".into(), v.to_string()]);
        }
        if p.unsettled == Some(true) {
            args.push("--unsettled".into());
        }
        if let Some(v) = p.status {
            args.extend(["--status".into(), v]);
        }
        if let Some(v) = p.from {
            args.extend(["--from".into(), v]);
        }
        if let Some(v) = p.to {
            args.extend(["--to".into(), v]);
        }
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "Get one transaction by id. Safe to retry.")]
    pub async fn transactions_get(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["transactions".into(), "get".into(), p.id]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Rich client-composed view of a transaction: its current `status` (e.g. \"Voided\") plus `availableOperations`. Note `transactionType` reflects the ORIGINAL type (e.g. \"Sale\") even after a void/refund — determine current state from `status`/`availableOperations`, not `transactionType`. Safe to retry."
    )]
    pub async fn transactions_inspect(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["transactions".into(), "inspect".into(), p.id]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Charge a card. NOT idempotent — each call moves money. Use a unique reference_id for server-side duplicate control; reconcile with transactions_list before retrying."
    )]
    pub async fn transactions_sale(
        &self,
        Parameters(p): Parameters<SaleArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_sale") {
            return Ok(blocked);
        }
        let args = self.sale_like_args("sale", p);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Authorize (hold) a card without capturing. NOT idempotent. Capture later with transactions_capture."
    )]
    pub async fn transactions_auth(
        &self,
        Parameters(p): Parameters<SaleArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_auth") {
            return Ok(blocked);
        }
        let args = self.sale_like_args("auth", p);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "Capture a prior authorization. NOT idempotent. Optional partial amount.")]
    pub async fn transactions_capture(
        &self,
        Parameters(p): Parameters<TxnRef>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_capture") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "transactions".into(),
            "capture".into(),
            "--transaction-id".into(),
            p.transaction_id,
        ]);
        if let Some(v) = p.amount {
            args.extend(["--amount".into(), v]);
        }
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Void a transaction. The void is recorded as a separate operation (response `type: \"Void\"`); the original transaction keeps its `transactionType` (e.g. \"Sale\") and its `status` becomes \"Voided\" — so a later inspect showing transactionType \"Sale\" with status \"Voided\" is correct, not a lost void. 404 on repeat = already voided (idempotent)."
    )]
    pub async fn transactions_void(
        &self,
        Parameters(p): Parameters<TxnRef>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_void") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "transactions".into(),
            "void".into(),
            "--transaction-id".into(),
            p.transaction_id,
        ]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Refund a transaction. NOT idempotent — moves money. Optional partial amount."
    )]
    pub async fn transactions_refund(
        &self,
        Parameters(p): Parameters<TxnRef>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_refund") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "transactions".into(),
            "refund".into(),
            "--transaction-id".into(),
            p.transaction_id,
        ]);
        if let Some(v) = p.amount {
            args.extend(["--amount".into(), v]);
        }
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Settle a payment processor's open batch (batch-level, NOT a single transaction). NOT idempotent."
    )]
    pub async fn transactions_settle(
        &self,
        Parameters(p): Parameters<Settle>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_settle") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "transactions".into(),
            "settle".into(),
            "--payment-processor-id".into(),
            p.payment_processor_id,
        ]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "Adjust the tip on a transaction. NOT idempotent.")]
    pub async fn transactions_tip_adjust(
        &self,
        Parameters(p): Parameters<TipAdjust>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_tip_adjust") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "transactions".into(),
            "tip-adjust".into(),
            "--transaction-id".into(),
            p.transaction_id,
            "--tip-amount".into(),
            p.tip_amount,
        ]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }
}
