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
pub struct SubscriptionsList {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub customer_id: Option<String>,
    /// Client-side status filter (e.g. "active", "paused", "terminated").
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionCreate {
    pub customer_id: String,
    /// Must be a vaulted + active payment method.
    pub payment_method_id: String,
    /// Amount as a decimal string, e.g. "10.00".
    pub amount: String,
    pub number_of_payments: u32,
    /// "day" | "week" | "month" (default month).
    #[serde(default)]
    pub interval: Option<String>,
    #[serde(default)]
    pub payment_frequency: Option<u32>,
    #[serde(default)]
    pub currency_id: Option<u32>,
    /// Default 2 = Sale; 11 = AchDebit.
    #[serde(default)]
    pub transaction_type: Option<u32>,
    #[serde(default)]
    pub requester_ip: Option<String>,
    #[serde(default)]
    pub payment_processor_id: Option<String>,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub sec_code: Option<u32>,
    #[serde(default)]
    pub faster: Option<bool>,
}

#[tool_router(router = subscriptions_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "Get a subscription by id. Safe to retry.")]
    pub async fn subscriptions_get(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["subscriptions".into(), "get".into(), p.id]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "List subscriptions. --status is client-side. Safe to retry.")]
    pub async fn subscriptions_list(
        &self,
        Parameters(p): Parameters<SubscriptionsList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["subscriptions".into(), "list".into()]);
        if let Some(v) = p.limit {
            args.extend(["--limit".into(), v.to_string()]);
        }
        if let Some(v) = p.page {
            args.extend(["--page".into(), v.to_string()]);
        }
        if let Some(v) = p.search {
            args.extend(["--search".into(), v]);
        }
        if let Some(v) = p.customer_id {
            args.extend(["--customer-id".into(), v]);
        }
        if let Some(v) = p.status {
            args.extend(["--status".into(), v]);
        }
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "List the payments for a subscription. Safe to retry.")]
    pub async fn subscriptions_payments(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["subscriptions".into(), "payments".into(), p.id]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Create a recurring subscription. NOT idempotent. payment_method_id must be vaulted + active."
    )]
    pub async fn subscriptions_create(
        &self,
        Parameters(p): Parameters<SubscriptionCreate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("subscriptions_create") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "subscriptions".into(),
            "create".into(),
            "--customer-id".into(),
            p.customer_id,
            "--payment-method-id".into(),
            p.payment_method_id,
            "--amount".into(),
            p.amount,
            "--number-of-payments".into(),
            p.number_of_payments.to_string(),
        ]);
        if let Some(v) = p.interval {
            args.extend(["--interval".into(), v]);
        }
        if let Some(v) = p.payment_frequency {
            args.extend(["--payment-frequency".into(), v.to_string()]);
        }
        if let Some(v) = p.currency_id {
            args.extend(["--currency-id".into(), v.to_string()]);
        }
        if let Some(v) = p.transaction_type {
            args.extend(["--transaction-type".into(), v.to_string()]);
        }
        if let Some(v) = p.requester_ip {
            args.extend(["--requester-ip".into(), v]);
        }
        if let Some(v) = p.payment_processor_id {
            args.extend(["--payment-processor-id".into(), v]);
        }
        if let Some(v) = p.start_date {
            args.extend(["--start-date".into(), v]);
        }
        if let Some(v) = p.sec_code {
            args.extend(["--sec-code".into(), v.to_string()]);
        }
        if p.faster == Some(true) {
            args.push("--faster".into());
        }
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "Terminate a subscription. 404 on repeat = idempotent.")]
    pub async fn subscriptions_terminate(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("subscriptions_terminate") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "subscriptions".into(),
            "terminate".into(),
            p.id,
            "--yes".into(),
        ]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }
}
