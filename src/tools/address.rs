//! Shared AVS billing-address argv construction (ARISE-4706).
//!
//! Mirrors the CLI's `cli::address` module: one `--billing-*` vocabulary is
//! shared by the card `sale`/`auth` commands and the customer
//! `create`/`update` commands, and the CLI maps it onto each endpoint's
//! differing wire keys (transactions `line1`/`postalCode` vs customers
//! `addressLine1`/`zip`). The MCP only has to forward the flags.
//!
//! The seven fields are declared inline on each tool's params struct rather
//! than `#[serde(flatten)]`-ed in from here: `flatten` silently disables the
//! `deny_unknown_fields` those structs rely on, which would turn a misspelled
//! billing param into a dropped field — and a dropped billing address is
//! exactly what makes an AVS-sensitive processor decline. Only the argv
//! construction is shared.

/// The seven AVS billing values pulled off a tool's params.
#[derive(Debug, Default)]
pub(crate) struct BillingArgs {
    pub line1: Option<String>,
    pub line2: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub state_id: Option<u32>,
    pub postal_code: Option<String>,
    pub country_id: Option<u32>,
}

impl BillingArgs {
    /// Append the set `--billing-*` flags, in the CLI's declared order.
    ///
    /// Nothing is appended when every field is `None`; the CLI emits
    /// `billingAddress` only when at least one flag is present, so an empty
    /// address stays absent from the request body rather than being sent blank.
    pub(crate) fn push(self, args: &mut Vec<String>) {
        if let Some(v) = self.line1 {
            args.extend(["--billing-line1".into(), v]);
        }
        if let Some(v) = self.line2 {
            args.extend(["--billing-line2".into(), v]);
        }
        if let Some(v) = self.city {
            args.extend(["--billing-city".into(), v]);
        }
        if let Some(v) = self.state {
            args.extend(["--billing-state".into(), v]);
        }
        if let Some(v) = self.state_id {
            args.extend(["--billing-state-id".into(), v.to_string()]);
        }
        if let Some(v) = self.postal_code {
            args.extend(["--billing-postal-code".into(), v]);
        }
        if let Some(v) = self.country_id {
            args.extend(["--billing-country-id".into(), v.to_string()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_address_appends_nothing() {
        let mut args = vec!["transactions".to_string()];
        BillingArgs::default().push(&mut args);
        assert_eq!(args, vec!["transactions".to_string()]);
    }

    #[test]
    fn partial_address_appends_only_set_flags() {
        let mut args = Vec::new();
        BillingArgs {
            city: Some("Denver".into()),
            country_id: Some(1),
            ..Default::default()
        }
        .push(&mut args);
        assert_eq!(
            args,
            vec!["--billing-city", "Denver", "--billing-country-id", "1"]
        );
    }

    #[test]
    fn full_address_keeps_cli_flag_order() {
        let mut args = Vec::new();
        BillingArgs {
            line1: Some("123 Test St".into()),
            line2: Some("Apt 4".into()),
            city: Some("Denver".into()),
            state: Some("CO".into()),
            state_id: Some(6),
            postal_code: Some("80202".into()),
            country_id: Some(1),
        }
        .push(&mut args);
        assert_eq!(
            args,
            vec![
                "--billing-line1",
                "123 Test St",
                "--billing-line2",
                "Apt 4",
                "--billing-city",
                "Denver",
                "--billing-state",
                "CO",
                "--billing-state-id",
                "6",
                "--billing-postal-code",
                "80202",
                "--billing-country-id",
                "1",
            ]
        );
    }
}
