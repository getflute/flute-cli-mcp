//! ARISE-4505 BUG-03: MCP clients (LLMs) routinely send integer parameters as
//! JSON strings. Tool param structs must accept both a JSON number and a
//! numeric string for their `u32` fields, instead of failing deserialization
//! with `invalid type: string "..", expected u32` (`-32602`). Non-numeric
//! strings must still be rejected, and native numbers must keep working.

use flute_cli_mcp::tools::ach::AchMove;
use flute_cli_mcp::tools::subscriptions::SubscriptionCreate;
use flute_cli_mcp::tools::transactions::TransactionsList;
use serde_json::json;

#[test]
fn transactions_list_accepts_string_numerics() {
    let p: TransactionsList = serde_json::from_value(json!({ "limit": "100", "page": "2" }))
        .expect("stringified numerics must deserialize");
    assert_eq!(p.limit, Some(100));
    assert_eq!(p.page, Some(2));
}

#[test]
fn transactions_list_still_accepts_native_numerics() {
    let p: TransactionsList = serde_json::from_value(json!({ "limit": 100, "page": 2 }))
        .expect("native numerics must still deserialize");
    assert_eq!(p.limit, Some(100));
    assert_eq!(p.page, Some(2));
}

#[test]
fn transactions_list_rejects_non_numeric_string() {
    let r = serde_json::from_value::<TransactionsList>(json!({ "limit": "abc" }));
    assert!(r.is_err(), "a non-numeric string must still be rejected");
}

#[test]
fn subscription_create_accepts_string_required_and_optional_u32() {
    // `number_of_payments` is a REQUIRED u32; `payment_frequency` an OPTIONAL u32.
    let p: SubscriptionCreate = serde_json::from_value(json!({
        "customer_id": "c1",
        "payment_method_id": "pm1",
        "amount": "25.00",
        "number_of_payments": "12",
        "payment_frequency": "1",
        "payment_processor_id": "pp1"
    }))
    .expect("stringified numerics must deserialize");
    assert_eq!(p.number_of_payments, 12);
    assert_eq!(p.payment_frequency, Some(1));
}

// ARISE-4505 BUG-13: the API requires payment_processor_id for subscriptions,
// so the tool schema must mark it required (non-Option field).
#[test]
fn subscription_create_requires_payment_processor_id() {
    let r = serde_json::from_value::<SubscriptionCreate>(json!({
        "customer_id": "c1",
        "payment_method_id": "pm1",
        "amount": "9.99",
        "number_of_payments": 12
    }));
    assert!(
        r.is_err(),
        "payment_processor_id must be required (missing → deserialize error)"
    );
}

#[test]
fn ach_move_accepts_string_billing_state_id() {
    // `billing_state_id` is a REQUIRED u32.
    let p: AchMove = serde_json::from_value(json!({
        "amount": "25.00",
        "payment_processor_id": "pp1",
        "routing": "021000021",
        "account": "123456789",
        "account_type": "checking",
        "account_holder_type": "personal",
        "billing_line1": "123 Test St",
        "billing_city": "Denver",
        "billing_state": "CO",
        "billing_state_id": "6",
        "billing_postal_code": "80202",
        "billing_country_id": "1",
        "contact_first_name": "Ada",
        "contact_last_name": "Lovelace",
        "contact_email": "ada@example.com",
        "contact_phone": "+15035558842"
    }))
    .expect("stringified numerics must deserialize");
    assert_eq!(p.billing_state_id, 6);
    assert_eq!(p.billing_country_id, Some(1));
}
