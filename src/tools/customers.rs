use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{Id, ack_envelope, address::BillingArgs, flute_err_to_result, value_to_result};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomersList {
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub limit: Option<u32>,
    /// Zero-based page index: 0 (or omit) is the first page, 1 the second, etc.
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub page: Option<u32>,
    #[serde(default)]
    pub search: Option<String>,
}

/// Used by `create` (no id) and `update` (id required → set via separate field).
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomerFields {
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub company: Option<String>,
    #[serde(default)]
    pub mobile: Option<String>,

    // AVS billing address (ARISE-4706) → customer `billingAddress`. On update,
    // supplying any billing_* field replaces the stored address wholesale;
    // omitting them all preserves the customer's current one.
    /// AVS billing street line 1.
    #[serde(default)]
    pub billing_line1: Option<String>,
    /// AVS billing street line 2.
    #[serde(default)]
    pub billing_line2: Option<String>,
    /// AVS billing city.
    #[serde(default)]
    pub billing_city: Option<String>,
    /// AVS billing state name, e.g. "CO".
    #[serde(default)]
    pub billing_state: Option<String>,
    /// AVS billing numeric state id.
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub billing_state_id: Option<u32>,
    /// AVS billing postal / ZIP code.
    #[serde(default)]
    pub billing_postal_code: Option<String>,
    /// AVS billing numeric country id; 1 = US.
    #[serde(default, deserialize_with = "crate::tools::de_flexible_u32")]
    pub billing_country_id: Option<u32>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomerUpdate {
    pub id: String,
    #[serde(flatten)]
    pub fields: CustomerFields,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddCard {
    pub id: String,
    pub card: String,
    pub exp: String,
    pub cvv: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddAch {
    pub id: String,
    pub routing: String,
    pub account: String,
    pub account_type: String,
    pub account_holder_type: String,
    #[serde(default)]
    pub tax_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveMethod {
    pub id: String,
    pub method_id: String,
}

impl FluteServer {
    fn customer_field_args(args: &mut Vec<String>, f: CustomerFields) {
        if let Some(v) = f.first_name {
            args.extend(["--first-name".into(), v]);
        }
        if let Some(v) = f.last_name {
            args.extend(["--last-name".into(), v]);
        }
        if let Some(v) = f.email {
            args.extend(["--email".into(), v]);
        }
        if let Some(v) = f.company {
            args.extend(["--company".into(), v]);
        }
        if let Some(v) = f.mobile {
            args.extend(["--mobile".into(), v]);
        }
        BillingArgs {
            line1: f.billing_line1,
            line2: f.billing_line2,
            city: f.billing_city,
            state: f.billing_state,
            state_id: f.billing_state_id,
            postal_code: f.billing_postal_code,
            country_id: f.billing_country_id,
        }
        .push(args);
    }
}

#[tool_router(router = customers_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "Get a customer by id. Safe to retry.")]
    pub async fn customers_get(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["customers".into(), "get".into(), p.id]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "List customers. --search is a real server param. Safe to retry.")]
    pub async fn customers_list(
        &self,
        Parameters(p): Parameters<CustomersList>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["customers".into(), "list".into()]);
        if let Some(v) = p.limit {
            args.extend(["--limit".into(), v.to_string()]);
        }
        if let Some(v) = p.page {
            args.extend(["--page".into(), v.to_string()]);
        }
        if let Some(v) = p.search {
            args.extend(["--search".into(), v]);
        }
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "List a customer's stored payment methods. Safe to retry.")]
    pub async fn customers_methods(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["customers".into(), "methods".into(), p.id]);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Create a customer. NOT idempotent — duplicates create a second record. Response is minimal; follow with customers_get."
    )]
    pub async fn customers_create(
        &self,
        Parameters(p): Parameters<CustomerFields>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_create") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend(["customers".into(), "create".into()]);
        Self::customer_field_args(&mut args, p);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Update a customer (GET-merge-PUT — omitted fields retain existing values). Note the billing address is replaced wholesale: supplying ANY billing_* field overwrites the stored address, so send the complete address, not just the part you are changing. Omitting them all preserves it. Safe to retry."
    )]
    pub async fn customers_update(
        &self,
        Parameters(p): Parameters<CustomerUpdate>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_update") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend(["customers".into(), "update".into(), p.id]);
        Self::customer_field_args(&mut args, p.fields);
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "Delete a customer. 404 on repeat = idempotent.")]
    pub async fn customers_delete(
        &self,
        Parameters(p): Parameters<Id>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_delete") {
            return Ok(blocked);
        }
        let id = p.id;
        let mut args = self.base_args();
        args.extend([
            "customers".into(),
            "delete".into(),
            id.clone(),
            "--yes".into(),
        ]);
        Ok(match self.run_cli(args).await {
            // The CLI returns no body on a successful delete (empty stdout -> Null).
            // Synthesize a structured success so clients don't get a bare `null`.
            Ok(v) if v.is_null() => value_to_result(ack_envelope(
                "customer_delete",
                serde_json::json!({ "id": id, "deleted": true }),
                self.config.profile.as_cli_str(),
            )),
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Vault a card on a customer. NOT idempotent. Response is minimal; follow with customers_methods."
    )]
    pub async fn customers_add_card(
        &self,
        Parameters(p): Parameters<AddCard>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_add_card") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "customers".into(),
            "add-card".into(),
            p.id,
            "--card".into(),
            p.card,
            "--exp".into(),
            p.exp,
            "--cvv".into(),
            p.cvv,
        ]);
        if let Some(v) = p.name {
            args.extend(["--name".into(), v]);
        }
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "Vault a bank account (ACH) on a customer. NOT idempotent.")]
    pub async fn customers_add_ach(
        &self,
        Parameters(p): Parameters<AddAch>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_add_ach") {
            return Ok(blocked);
        }
        let mut args = self.base_args();
        args.extend([
            "customers".into(),
            "add-ach".into(),
            p.id,
            "--routing".into(),
            p.routing,
            "--account".into(),
            p.account,
            "--account-type".into(),
            p.account_type,
            "--account-holder-type".into(),
            p.account_holder_type,
        ]);
        if let Some(v) = p.tax_id {
            args.extend(["--tax-id".into(), v]);
        }
        if let Some(v) = p.name {
            args.extend(["--name".into(), v]);
        }
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Remove a stored payment method from a customer. 404 on repeat = idempotent."
    )]
    pub async fn customers_remove_method(
        &self,
        Parameters(p): Parameters<RemoveMethod>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_remove_method") {
            return Ok(blocked);
        }
        let id = p.id;
        let method_id = p.method_id;
        let mut args = self.base_args();
        args.extend([
            "customers".into(),
            "remove-method".into(),
            id.clone(),
            method_id.clone(),
            "--yes".into(),
        ]);
        Ok(match self.run_cli(args).await {
            // No body on success (empty stdout -> Null); synthesize a structured result.
            Ok(v) if v.is_null() => value_to_result(ack_envelope(
                "payment_method_removed",
                serde_json::json!({ "id": id, "method_id": method_id, "removed": true }),
                self.config.profile.as_cli_str(),
            )),
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }
}
