use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{Id, flute_err_to_result, value_to_result};

/// Shared input for `ach debit` and `ach credit`. The API under-marks several
/// of these as optional; agents.md lists them all as required for a live call.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AchMove {
    /// Amount as a decimal string, e.g. "10.00".
    pub amount: String,
    pub payment_processor_id: String,
    pub routing: String,
    pub account: String,
    /// "checking" | "savings".
    pub account_type: String,
    /// "business" | "personal".
    pub account_holder_type: String,
    pub billing_line1: String,
    pub billing_city: String,
    pub billing_state: String,
    /// Numeric state id (free-text state alone is rejected).
    #[serde(deserialize_with = "crate::tools::de_flexible_u32_req")]
    pub billing_state_id: u32,
    pub billing_postal_code: String,
    /// Numeric country id; 1 = US.
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub billing_country_id: Option<u32>,
    pub contact_first_name: String,
    pub contact_last_name: String,
    pub contact_email: String,
    pub contact_phone: String,
    /// Contact company name. Maps to `contactInfo.companyName`; **required by
    /// the API when `account_holder_type` is "business".**
    #[serde(default)]
    pub contact_company: Option<String>,
    /// SEC code; CLI default 1 = Web.
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub sec_code: Option<u32>,
    /// Requester IP; CLI default 127.0.0.1.
    #[serde(default)]
    pub requester_ip: Option<String>,
}

impl FluteServer {
    fn ach_move_args(&self, sub: &str, p: AchMove) -> Vec<String> {
        let mut a = self.base_args();
        a.extend(["ach".into(), sub.into(), "--amount".into(), p.amount]);
        a.extend(["--payment-processor-id".into(), p.payment_processor_id]);
        a.extend(["--routing".into(), p.routing, "--account".into(), p.account]);
        a.extend(["--account-type".into(), p.account_type]);
        a.extend(["--account-holder-type".into(), p.account_holder_type]);
        a.extend(["--billing-line1".into(), p.billing_line1]);
        a.extend(["--billing-city".into(), p.billing_city]);
        a.extend(["--billing-state".into(), p.billing_state]);
        a.extend(["--billing-state-id".into(), p.billing_state_id.to_string()]);
        a.extend(["--billing-postal-code".into(), p.billing_postal_code]);
        a.extend([
            "--billing-country-id".into(),
            p.billing_country_id.unwrap_or(1).to_string(),
        ]);
        a.extend(["--contact-first-name".into(), p.contact_first_name]);
        a.extend(["--contact-last-name".into(), p.contact_last_name]);
        a.extend(["--contact-email".into(), p.contact_email]);
        a.extend(["--contact-phone".into(), p.contact_phone]);
        if let Some(v) = p.contact_company {
            a.extend(["--contact-company".into(), v]);
        }
        a.extend(["--sec-code".into(), p.sec_code.unwrap_or(1).to_string()]);
        a.extend([
            "--requester-ip".into(),
            p.requester_ip.unwrap_or_else(|| "127.0.0.1".into()),
        ]);
        a
    }
}

#[tool_router(router = ach_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(
        description = "ACH debit (pull funds). NOT idempotent — moves money. Requires a live payment_processor_id, billing address, and contact info."
    )]
    pub async fn ach_debit(
        &self,
        Parameters(p): Parameters<AchMove>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("ach_debit") {
            return Ok(blocked);
        }
        let args = self.ach_move_args("debit", p);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "ACH credit (push funds). NOT idempotent — moves money. Same required fields as ach_debit."
    )]
    pub async fn ach_credit(
        &self,
        Parameters(p): Parameters<AchMove>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("ach_credit") {
            return Ok(blocked);
        }
        let args = self.ach_move_args("credit", p);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Void an ACH transaction by id. NOT idempotent: a repeat surfaces the server error, so reconcile with transactions_get/list before retrying."
    )]
    pub async fn ach_void(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("ach_void") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend(["ach".into(), "void".into(), p.id]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "Refund an ACH transaction by id. NOT idempotent — moves money.")]
    pub async fn ach_refund(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("ach_refund") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend(["ach".into(), "refund".into(), p.id]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }
}
