use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use flute_cli_mcp::config::{Config, Profile};
use flute_cli_mcp::runner::MockRunner;
use flute_cli_mcp::server::FluteServer;
use flute_cli_mcp::tools::Empty;
use pretty_assertions::assert_eq;
use rmcp::handler::server::wrapper::Parameters;
use serde_json::json;

pub fn cfg(profile: Profile, allow_prod_writes: bool, merchant_id: Option<&str>) -> Arc<Config> {
    Arc::new(Config {
        profile,
        binary: PathBuf::from("/bin/false"),
        merchant_id: merchant_id.map(String::from),
        timeout: Duration::from_secs(30),
        debug: false,
        allow_prod_writes,
    })
}

pub fn svec<const N: usize>(a: [&str; N]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// The tool's JSON payload, decoded out of the MCP content envelope.
pub fn body(res: &rmcp::model::CallToolResult) -> serde_json::Value {
    let v = serde_json::to_value(res).unwrap();
    serde_json::from_str(v["content"][0]["text"].as_str().unwrap()).unwrap()
}

/// A sandbox server wired to a MockRunner seeded with `n` empty-object OK responses.
pub fn sandbox(n: usize) -> (FluteServer, Arc<MockRunner>) {
    let mock = MockRunner::new((0..n).map(|_| Ok(json!({"object": "x"}))).collect());
    (
        FluteServer::new(cfg(Profile::Sandbox, false, None), mock.clone()),
        mock,
    )
}

#[tokio::test]
async fn util_tools_build_expected_argv() {
    let (srv, mock) = sandbox(3);
    srv.ping(Parameters(Empty {})).await.unwrap();
    srv.version(Parameters(Empty {})).await.unwrap();
    let calls = mock.calls();
    assert_eq!(
        calls[0],
        svec(["--profile", "sandbox", "--output", "json", "ping"])
    );
    assert_eq!(
        calls[1],
        svec(["--profile", "sandbox", "--output", "json", "version"])
    );
}

/// ARISE-4706 replaced the offline `has_credentials` flag with a live
/// `authenticated` check plus `client_id`/`merchant_id`.
#[tokio::test]
async fn auth_status_maps_live_authenticated_shape() {
    let mock = MockRunner::new(vec![Ok(json!({
        "object": "auth_status",
        "data": {
            "profile": "sandbox",
            "api_base_url": "https://sandbox.api.flute.com",
            "authenticated": true,
            "client_id": "cid-server",
            "merchant_id": "m-42",
        }
    }))]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock.clone());
    let res = srv.auth_status(Parameters(Empty {})).await.unwrap();
    assert_eq!(res.is_error, Some(false));
    let v = body(&res);
    assert_eq!(v["authenticated"], true);
    assert_eq!(v["profile"], "sandbox");
    assert_eq!(v["api_base_url"], "https://sandbox.api.flute.com");
    assert_eq!(v["client_id"], "cid-server");
    assert_eq!(v["merchant_id"], "m-42");
    assert_eq!(
        mock.calls()[0],
        svec(["--profile", "sandbox", "--output", "json", "auth", "status"])
    );
}

/// A failed live check still reports the client id, with `authenticated:false`.
/// `merchant_id` is null when the principal isn't merchant-bound and must be
/// omitted rather than forwarded as a JSON null.
#[tokio::test]
async fn auth_status_reports_failed_live_check_without_null_merchant() {
    let mock = MockRunner::new(vec![Ok(json!({
        "object": "auth_status",
        "data": { "authenticated": false, "client_id": "cid-stored", "merchant_id": null }
    }))]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock.clone());
    let v = body(&srv.auth_status(Parameters(Empty {})).await.unwrap());
    assert_eq!(v["authenticated"], false);
    assert_eq!(v["client_id"], "cid-stored");
    assert!(
        v.get("merchant_id").is_none(),
        "a null merchant_id must be omitted, got {v}"
    );
}

/// Backward compatibility: against a pre-v1.1.0 CLI the payload still carries
/// `has_credentials`, and reporting `authenticated:false` there would be wrong.
#[tokio::test]
async fn auth_status_falls_back_to_legacy_has_credentials() {
    let mock = MockRunner::new(vec![Ok(
        json!({"object":"auth_status","data":{"has_credentials":true}}),
    )]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock.clone());
    let v = body(&srv.auth_status(Parameters(Empty {})).await.unwrap());
    assert_eq!(v["authenticated"], true);
    assert!(v.get("client_id").is_none());
}

use flute_cli_mcp::tools::transactions::{SaleArgs, Settle, TransactionsList, TxnRef};

#[tokio::test]
async fn transactions_read_argv() {
    let (srv, mock) = sandbox(3);
    srv.transactions_list(Parameters(TransactionsList {
        limit: Some(25),
        unsettled: Some(true),
        ..Default::default()
    }))
    .await
    .unwrap();
    srv.transactions_get(Parameters(flute_cli_mcp::tools::Id { id: "t1".into() }))
        .await
        .unwrap();
    srv.transactions_inspect(Parameters(flute_cli_mcp::tools::Id { id: "t2".into() }))
        .await
        .unwrap();
    let c = mock.calls();
    assert_eq!(
        c[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "list",
            "--limit",
            "25",
            "--unsettled"
        ])
    );
    assert_eq!(
        c[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "get",
            "t1"
        ])
    );
    assert_eq!(
        c[2],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "inspect",
            "t2"
        ])
    );
}

#[tokio::test]
async fn transactions_sale_argv() {
    let (srv, mock) = sandbox(1);
    srv.transactions_sale(Parameters(SaleArgs {
        amount: "10.00".into(),
        card: Some("4111111111111111".into()),
        exp: Some("12/27".into()),
        cvv: Some("123".into()),
        ..Default::default()
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "sale",
            "--amount",
            "10.00",
            "--card",
            "4111111111111111",
            "--exp",
            "12/27",
            "--cvv",
            "123",
        ])
    );
}

/// ARISE-4706: card sale/auth must forward the AVS billing address; dropping
/// its matched street or ZIP can cause an AVS-sensitive processor to decline.
#[tokio::test]
async fn transactions_sale_forwards_avs_billing_address() {
    let (srv, mock) = sandbox(1);
    srv.transactions_sale(Parameters(SaleArgs {
        amount: "10.00".into(),
        card: Some("4111111111111111".into()),
        billing_line1: Some("123 Test St".into()),
        billing_city: Some("Denver".into()),
        billing_state: Some("CO".into()),
        billing_state_id: Some(6),
        billing_postal_code: Some("80202".into()),
        billing_country_id: Some(1),
        ..Default::default()
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "sale",
            "--amount",
            "10.00",
            "--card",
            "4111111111111111",
            "--billing-line1",
            "123 Test St",
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
        ])
    );
}

/// `auth` shares the sale flag set, and an address-free call must stay
/// byte-identical to the pre-4706 argv (no empty `--billing-*` flags).
#[tokio::test]
async fn transactions_auth_omits_billing_flags_when_unset() {
    let (srv, mock) = sandbox(1);
    srv.transactions_auth(Parameters(SaleArgs {
        amount: "5.00".into(),
        billing_city: Some("Denver".into()),
        ..Default::default()
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "auth",
            "--amount",
            "5.00",
            "--billing-city",
            "Denver",
        ])
    );
}

#[tokio::test]
async fn transactions_refund_argv() {
    let (srv, mock) = sandbox(1);
    srv.transactions_refund(Parameters(TxnRef {
        transaction_id: "t9".into(),
        amount: Some("5.00".into()),
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "refund",
            "--transaction-id",
            "t9",
            "--amount",
            "5.00",
        ])
    );
}

#[tokio::test]
async fn transactions_settle_argv() {
    let (srv, mock) = sandbox(1);
    srv.transactions_settle(Parameters(Settle {
        payment_processor_id: "pp1".into(),
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "settle",
            "--payment-processor-id",
            "pp1"
        ])
    );
}

#[tokio::test]
async fn prod_blocks_writes_without_override() {
    let mock = MockRunner::new(vec![]); // must never be called
    let srv = FluteServer::new(cfg(Profile::Production, false, None), mock.clone());
    let res = srv
        .transactions_sale(Parameters(SaleArgs {
            amount: "10.00".into(),
            ..Default::default()
        }))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(true));
    assert!(mock.calls().is_empty());
}

#[tokio::test]
async fn prod_allows_writes_with_override() {
    let mock = MockRunner::new(vec![Ok(json!({"object":"transaction"}))]);
    let srv = FluteServer::new(cfg(Profile::Production, true, None), mock.clone());
    srv.transactions_sale(Parameters(SaleArgs {
        amount: "10.00".into(),
        ..Default::default()
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "production",
            "--output",
            "json",
            "transactions",
            "sale",
            "--amount",
            "10.00"
        ])
    );
}

#[tokio::test]
async fn prod_allows_reads() {
    let mock = MockRunner::new(vec![Ok(json!({"object":"transaction_list"}))]);
    let srv = FluteServer::new(cfg(Profile::Production, false, None), mock.clone());
    srv.transactions_list(Parameters(TransactionsList::default()))
        .await
        .unwrap();
    assert_eq!(mock.calls().len(), 1);
}

use flute_cli_mcp::tools::customers::{
    AddAch, AddCard, CustomerFields, CustomerUpdate, CustomersList, RemoveMethod,
};

#[tokio::test]
async fn customers_argv() {
    let (srv, mock) = sandbox(4);
    srv.customers_list(Parameters(CustomersList {
        search: Some("ann".into()),
        ..Default::default()
    }))
    .await
    .unwrap();
    srv.customers_create(Parameters(CustomerFields {
        first_name: Some("Ann".into()),
        email: Some("a@b.com".into()),
        ..Default::default()
    }))
    .await
    .unwrap();
    srv.customers_update(Parameters(CustomerUpdate {
        id: "c1".into(),
        fields: CustomerFields {
            mobile: Some("5551234".into()),
            ..Default::default()
        },
    }))
    .await
    .unwrap();
    srv.customers_remove_method(Parameters(RemoveMethod {
        id: "c1".into(),
        method_id: "m9".into(),
    }))
    .await
    .unwrap();
    let c = mock.calls();
    assert_eq!(
        c[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "list",
            "--search",
            "ann"
        ])
    );
    assert_eq!(
        c[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "create",
            "--first-name",
            "Ann",
            "--email",
            "a@b.com"
        ])
    );
    assert_eq!(
        c[2],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "update",
            "c1",
            "--mobile",
            "5551234"
        ])
    );
    assert_eq!(
        c[3],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "remove-method",
            "c1",
            "m9",
            "--yes"
        ])
    );
}

/// ARISE-4706: create/update carry the same billing vocabulary through to the
/// customer's `billingAddress`.
#[tokio::test]
async fn customers_create_and_update_forward_billing_address() {
    let (srv, mock) = sandbox(2);
    srv.customers_create(Parameters(CustomerFields {
        first_name: Some("Ann".into()),
        billing_line1: Some("1 Main St".into()),
        billing_line2: Some("Suite 2".into()),
        billing_city: Some("Denver".into()),
        billing_country_id: Some(1),
        ..Default::default()
    }))
    .await
    .unwrap();
    srv.customers_update(Parameters(CustomerUpdate {
        id: "c1".into(),
        fields: CustomerFields {
            billing_postal_code: Some("80202".into()),
            billing_state_id: Some(6),
            ..Default::default()
        },
    }))
    .await
    .unwrap();
    let c = mock.calls();
    assert_eq!(
        c[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "create",
            "--first-name",
            "Ann",
            "--billing-line1",
            "1 Main St",
            "--billing-line2",
            "Suite 2",
            "--billing-city",
            "Denver",
            "--billing-country-id",
            "1",
        ])
    );
    assert_eq!(
        c[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "update",
            "c1",
            "--billing-state-id",
            "6",
            "--billing-postal-code",
            "80202",
        ])
    );
}

#[tokio::test]
async fn customers_add_methods_argv() {
    let (srv, mock) = sandbox(2);
    srv.customers_add_card(Parameters(AddCard {
        id: "c1".into(),
        card: "4111111111111111".into(),
        exp: "12/27".into(),
        cvv: "123".into(),
        name: Some("Ann B".into()),
    }))
    .await
    .unwrap();
    srv.customers_add_ach(Parameters(AddAch {
        id: "c1".into(),
        routing: "021000021".into(),
        account: "123456789".into(),
        account_type: "checking".into(),
        account_holder_type: "personal".into(),
        ..Default::default()
    }))
    .await
    .unwrap();
    let c = mock.calls();
    assert_eq!(
        c[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "add-card",
            "c1",
            "--card",
            "4111111111111111",
            "--exp",
            "12/27",
            "--cvv",
            "123",
            "--name",
            "Ann B"
        ])
    );
    assert_eq!(
        c[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "add-ach",
            "c1",
            "--routing",
            "021000021",
            "--account",
            "123456789",
            "--account-type",
            "checking",
            "--account-holder-type",
            "personal"
        ])
    );
}

use flute_cli_mcp::tools::devices::{DeviceId, DeviceRegister};

#[tokio::test]
async fn terminals_and_devices_argv() {
    let (srv, mock) = sandbox(4);
    srv.terminals_status(Parameters(flute_cli_mcp::tools::Id { id: "term1".into() }))
        .await
        .unwrap();
    srv.devices_list(Parameters(flute_cli_mcp::tools::Empty {}))
        .await
        .unwrap();
    srv.devices_ttp_jwt(Parameters(DeviceId {
        device_id: "d1".into(),
    }))
    .await
    .unwrap();
    srv.devices_register(Parameters(DeviceRegister {
        id: "d1".into(),
        name: Some("Lane 1".into()),
    }))
    .await
    .unwrap();
    let c = mock.calls();
    assert_eq!(
        c[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "terminals",
            "status",
            "term1"
        ])
    );
    assert_eq!(
        c[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "devices",
            "list"
        ])
    );
    assert_eq!(
        c[2],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "devices",
            "ttp-jwt",
            "--device-id",
            "d1"
        ])
    );
    assert_eq!(
        c[3],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "devices",
            "register",
            "d1",
            "--name",
            "Lane 1"
        ])
    );
}

use flute_cli_mcp::tools::ach::AchMove;

#[tokio::test]
async fn ach_debit_argv() {
    let (srv, mock) = sandbox(1);
    srv.ach_debit(Parameters(AchMove {
        amount: "25.00".into(),
        payment_processor_id: "pp1".into(),
        routing: "021000021".into(),
        account: "123456789".into(),
        account_type: "checking".into(),
        account_holder_type: "personal".into(),
        billing_line1: "1 Main St".into(),
        billing_city: "Austin".into(),
        billing_state: "TX".into(),
        billing_state_id: 44,
        billing_postal_code: "78701".into(),
        contact_first_name: "A".into(),
        contact_last_name: "B".into(),
        contact_email: "a@b.com".into(),
        contact_phone: "5125551234".into(),
        ..Default::default()
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "ach",
            "debit",
            "--amount",
            "25.00",
            "--payment-processor-id",
            "pp1",
            "--routing",
            "021000021",
            "--account",
            "123456789",
            "--account-type",
            "checking",
            "--account-holder-type",
            "personal",
            "--billing-line1",
            "1 Main St",
            "--billing-city",
            "Austin",
            "--billing-state",
            "TX",
            "--billing-state-id",
            "44",
            "--billing-postal-code",
            "78701",
            "--billing-country-id",
            "1",
            "--contact-first-name",
            "A",
            "--contact-last-name",
            "B",
            "--contact-email",
            "a@b.com",
            "--contact-phone",
            "5125551234",
            "--sec-code",
            "1",
            "--requester-ip",
            "127.0.0.1",
        ])
    );
}

// ARISE-4505 BUG-06: business ACH needs a company name. The CLI supports
// `--contact-company`; the tool must expose `contact_company` and forward it.
#[tokio::test]
async fn ach_debit_argv_forwards_contact_company() {
    let (srv, mock) = sandbox(1);
    srv.ach_debit(Parameters(AchMove {
        amount: "25.00".into(),
        payment_processor_id: "pp1".into(),
        routing: "021000021".into(),
        account: "123456789".into(),
        account_type: "checking".into(),
        account_holder_type: "business".into(),
        billing_line1: "1 Main St".into(),
        billing_city: "Austin".into(),
        billing_state: "TX".into(),
        billing_state_id: 44,
        billing_postal_code: "78701".into(),
        contact_first_name: "A".into(),
        contact_last_name: "B".into(),
        contact_email: "a@b.com".into(),
        contact_phone: "5125551234".into(),
        contact_company: Some("Acme Corp".into()),
        ..Default::default()
    }))
    .await
    .unwrap();
    let argv = &mock.calls()[0];
    let pos = argv
        .iter()
        .position(|a| a == "--contact-company")
        .expect("argv must include --contact-company");
    assert_eq!(argv[pos + 1], "Acme Corp");
}

use flute_cli_mcp::tools::pos::{PosCreate, PosList};

#[tokio::test]
async fn pos_argv_has_no_wait() {
    let (srv, mock) = sandbox(2);
    srv.pos_list(Parameters(PosList {
        terminal_id: Some("term1".into()),
        ..Default::default()
    }))
    .await
    .unwrap();
    srv.pos_create(Parameters(PosCreate {
        terminal_id: "term1".into(),
        amount: "10.00".into(),
        pos_device_id: "dev1".into(),
        reference_id: "ref-1".into(),
        ..Default::default()
    }))
    .await
    .unwrap();
    let c = mock.calls();
    assert_eq!(
        c[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "pos",
            "list",
            "--terminal-id",
            "term1"
        ])
    );
    assert_eq!(
        c[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "pos",
            "create",
            "--terminal-id",
            "term1",
            "--amount",
            "10.00",
            "--pos-device-id",
            "dev1",
            "--reference-id",
            "ref-1"
        ])
    );
    assert!(
        !c[1].iter().any(|a| a == "--wait"),
        "pos_create must not pass --wait"
    );
}

use flute_cli_mcp::tools::settlements::SettlementsList;
use flute_cli_mcp::tools::subscriptions::{SubscriptionCreate, SubscriptionsList};

#[tokio::test]
async fn settlements_and_subscriptions_argv() {
    let (srv, mock) = sandbox(3);
    srv.settlements_list(Parameters(SettlementsList {
        status: Some("open".into()),
        ..Default::default()
    }))
    .await
    .unwrap();
    srv.subscriptions_create(Parameters(SubscriptionCreate {
        customer_id: "c1".into(),
        payment_method_id: "pm1".into(),
        amount: "9.99".into(),
        number_of_payments: 12,
        payment_processor_id: "pp1".into(),
        ..Default::default()
    }))
    .await
    .unwrap();
    srv.subscriptions_list(Parameters(SubscriptionsList {
        customer_id: Some("c1".into()),
        status: Some("active".into()),
        ..Default::default()
    }))
    .await
    .unwrap();
    let c = mock.calls();
    assert_eq!(
        c[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "settlements",
            "list",
            "--status",
            "open"
        ])
    );
    assert_eq!(
        c[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "subscriptions",
            "create",
            "--customer-id",
            "c1",
            "--payment-method-id",
            "pm1",
            "--amount",
            "9.99",
            "--number-of-payments",
            "12",
            "--payment-processor-id",
            "pp1"
        ])
    );
    assert_eq!(
        c[2],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "subscriptions",
            "list",
            "--customer-id",
            "c1",
            "--status",
            "active"
        ])
    );
}

use flute_cli_mcp::tools::keys::{KeyCreate, KeyRevoke};

#[tokio::test]
async fn keys_create_uses_per_call_merchant_id() {
    let (srv, mock) = sandbox(1);
    srv.keys_create(Parameters(KeyCreate {
        name: "ci".into(),
        merchant_id: Some("m-1".into()),
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "keys",
            "create",
            "--merchant-id",
            "m-1",
            "--name",
            "ci"
        ])
    );
}

#[tokio::test]
async fn keys_create_falls_back_to_pinned_merchant_id() {
    let mock = MockRunner::new(vec![Ok(json!({"object":"api_token"}))]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, Some("m-pinned")), mock.clone());
    srv.keys_create(Parameters(KeyCreate {
        name: "ci".into(),
        merchant_id: None,
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "keys",
            "create",
            "--merchant-id",
            "m-pinned",
            "--name",
            "ci"
        ])
    );
}

#[tokio::test]
async fn keys_create_errors_without_any_merchant_id() {
    let mock = MockRunner::new(vec![]); // CLI must never be called
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock.clone());
    let res = srv
        .keys_create(Parameters(KeyCreate {
            name: "ci".into(),
            merchant_id: None,
        }))
        .await
        .unwrap();
    assert_eq!(res.is_error, Some(true));
    assert!(mock.calls().is_empty());
}

#[tokio::test]
async fn keys_revoke_argv() {
    let (srv, mock) = sandbox(1);
    srv.keys_revoke(Parameters(KeyRevoke {
        client_id: "cid-1".into(),
        merchant_id: Some("m-1".into()),
    }))
    .await
    .unwrap();
    assert_eq!(
        mock.calls()[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "keys",
            "revoke",
            "--client-id",
            "cid-1",
            "--merchant-id",
            "m-1",
            "--yes"
        ])
    );
}

/// The production write guard must block a representative write from EVERY tool group,
/// returning an error without spawning the CLI. The runner is seeded empty, so any
/// write that slips past the guard and reaches `run_cli` panics the test.
#[tokio::test]
async fn production_blocks_a_write_in_every_group() {
    let mock = MockRunner::new(vec![]);
    let srv = FluteServer::new(cfg(Profile::Production, false, None), mock.clone());

    let blocked = [
        srv.transactions_sale(Parameters(SaleArgs {
            amount: "1.00".into(),
            ..Default::default()
        }))
        .await
        .unwrap(),
        srv.ach_debit(Parameters(AchMove {
            amount: "1.00".into(),
            ..Default::default()
        }))
        .await
        .unwrap(),
        srv.customers_create(Parameters(CustomerFields::default()))
            .await
            .unwrap(),
        srv.devices_register(Parameters(DeviceRegister {
            id: "d1".into(),
            ..Default::default()
        }))
        .await
        .unwrap(),
        srv.pos_create(Parameters(PosCreate {
            terminal_id: "t1".into(),
            amount: "1.00".into(),
            pos_device_id: "dev1".into(),
            reference_id: "ref1".into(),
            ..Default::default()
        }))
        .await
        .unwrap(),
        srv.subscriptions_create(Parameters(SubscriptionCreate {
            customer_id: "c1".into(),
            payment_method_id: "pm1".into(),
            amount: "1.00".into(),
            number_of_payments: 1,
            ..Default::default()
        }))
        .await
        .unwrap(),
        srv.keys_create(Parameters(KeyCreate {
            name: "n".into(),
            merchant_id: Some("m1".into()),
        }))
        .await
        .unwrap(),
    ];

    for res in &blocked {
        assert_eq!(res.is_error, Some(true));
    }
    assert!(
        mock.calls().is_empty(),
        "no write should reach the CLI on a guarded production instance"
    );
}

/// `page` is forwarded to the CLI verbatim — the API is zero-based and the MCP applies
/// NO offset. This locks the documented contract: page 0 (or omitted) is the first page.
/// If anyone adds a 1-based normalization later, this test fails.
#[tokio::test]
async fn page_is_forwarded_verbatim_zero_based() {
    let (srv, mock) = sandbox(2);
    srv.customers_list(Parameters(CustomersList {
        page: Some(2),
        ..Default::default()
    }))
    .await
    .unwrap();
    srv.transactions_list(Parameters(TransactionsList {
        page: Some(0),
        ..Default::default()
    }))
    .await
    .unwrap();
    let c = mock.calls();
    assert_eq!(
        c[0],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "customers",
            "list",
            "--page",
            "2"
        ])
    );
    assert_eq!(
        c[1],
        svec([
            "--profile",
            "sandbox",
            "--output",
            "json",
            "transactions",
            "list",
            "--page",
            "0"
        ])
    );
}

/// delete / remove-method / revoke return empty stdout on success, which the runner maps
/// to `Value::Null`. The tools must synthesize a structured `{object, data, meta}` envelope
/// instead of handing back a bare `null`.
#[tokio::test]
async fn empty_success_synthesizes_structured_envelope() {
    let mock = MockRunner::new(vec![
        Ok(serde_json::Value::Null),
        Ok(serde_json::Value::Null),
        Ok(serde_json::Value::Null),
    ]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock.clone());

    let del = srv
        .customers_delete(Parameters(flute_cli_mcp::tools::Id { id: "c1".into() }))
        .await
        .unwrap();
    let rm = srv
        .customers_remove_method(Parameters(RemoveMethod {
            id: "c1".into(),
            method_id: "m9".into(),
        }))
        .await
        .unwrap();
    let rev = srv
        .keys_revoke(Parameters(KeyRevoke {
            client_id: "cid1".into(),
            merchant_id: Some("m1".into()),
        }))
        .await
        .unwrap();

    // customers_delete: full shape, not a bare null.
    assert_eq!(del.is_error, Some(false));
    let d = body(&del);
    assert_eq!(d["object"], "customer_delete");
    assert_eq!(d["data"]["id"], "c1");
    assert_eq!(d["data"]["deleted"], true);
    assert_eq!(d["meta"]["environment"], "sandbox");

    // remove-method and revoke: structured success with their own object + data.
    assert_eq!(rm.is_error, Some(false));
    let r = body(&rm);
    assert_eq!(r["object"], "payment_method_removed");
    assert_eq!(r["data"]["method_id"], "m9");
    assert_eq!(r["data"]["removed"], true);

    assert_eq!(rev.is_error, Some(false));
    let v = body(&rev);
    assert_eq!(v["object"], "api_token_revoked");
    assert_eq!(v["data"]["client_id"], "cid1");
    assert_eq!(v["data"]["revoked"], true);
}
