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

/// A sandbox server wired to a MockRunner seeded with `n` empty-object OK responses.
pub fn sandbox(n: usize) -> (FluteServer, Arc<MockRunner>) {
    let mock = MockRunner::new((0..n).map(|_| Ok(json!({"object": "x"}))).collect());
    (FluteServer::new(cfg(Profile::Sandbox, false, None), mock.clone()), mock)
}

#[tokio::test]
async fn util_tools_build_expected_argv() {
    let (srv, mock) = sandbox(3);
    srv.ping(Parameters(Empty {})).await.unwrap();
    srv.version(Parameters(Empty {})).await.unwrap();
    let calls = mock.calls();
    assert_eq!(calls[0], svec(["--profile", "sandbox", "--output", "json", "ping"]));
    assert_eq!(calls[1], svec(["--profile", "sandbox", "--output", "json", "version"]));
}

#[tokio::test]
async fn auth_status_maps_has_credentials() {
    let mock = MockRunner::new(vec![Ok(json!({"object":"auth_status","data":{"has_credentials":true}}))]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock.clone());
    let res = srv.auth_status(Parameters(Empty {})).await.unwrap();
    assert_eq!(res.is_error, Some(false));
    assert_eq!(mock.calls()[0], svec(["--profile", "sandbox", "--output", "json", "auth", "status"]));
}

use flute_cli_mcp::tools::transactions::{SaleArgs, Settle, TransactionsList, TxnRef};

#[tokio::test]
async fn transactions_read_argv() {
    let (srv, mock) = sandbox(3);
    srv.transactions_list(Parameters(TransactionsList { limit: Some(25), unsettled: Some(true), ..Default::default() })).await.unwrap();
    srv.transactions_get(Parameters(flute_cli_mcp::tools::Id { id: "t1".into() })).await.unwrap();
    srv.transactions_inspect(Parameters(flute_cli_mcp::tools::Id { id: "t2".into() })).await.unwrap();
    let c = mock.calls();
    assert_eq!(c[0], svec(["--profile","sandbox","--output","json","transactions","list","--limit","25","--unsettled"]));
    assert_eq!(c[1], svec(["--profile","sandbox","--output","json","transactions","get","t1"]));
    assert_eq!(c[2], svec(["--profile","sandbox","--output","json","transactions","inspect","t2"]));
}

#[tokio::test]
async fn transactions_sale_argv() {
    let (srv, mock) = sandbox(1);
    srv.transactions_sale(Parameters(SaleArgs {
        amount: "10.00".into(), card: Some("4111111111111111".into()),
        exp: Some("12/27".into()), cvv: Some("123".into()), ..Default::default()
    })).await.unwrap();
    assert_eq!(mock.calls()[0], svec([
        "--profile","sandbox","--output","json","transactions","sale",
        "--amount","10.00","--card","4111111111111111","--exp","12/27","--cvv","123",
    ]));
}

#[tokio::test]
async fn transactions_refund_argv() {
    let (srv, mock) = sandbox(1);
    srv.transactions_refund(Parameters(TxnRef { transaction_id: "t9".into(), amount: Some("5.00".into()) })).await.unwrap();
    assert_eq!(mock.calls()[0], svec([
        "--profile","sandbox","--output","json","transactions","refund","--transaction-id","t9","--amount","5.00",
    ]));
}

#[tokio::test]
async fn transactions_settle_argv() {
    let (srv, mock) = sandbox(1);
    srv.transactions_settle(Parameters(Settle { payment_processor_id: "pp1".into() })).await.unwrap();
    assert_eq!(mock.calls()[0], svec(["--profile","sandbox","--output","json","transactions","settle","--payment-processor-id","pp1"]));
}

#[tokio::test]
async fn prod_blocks_writes_without_override() {
    let mock = MockRunner::new(vec![]); // must never be called
    let srv = FluteServer::new(cfg(Profile::Production, false, None), mock.clone());
    let res = srv.transactions_sale(Parameters(SaleArgs { amount: "10.00".into(), ..Default::default() })).await.unwrap();
    assert_eq!(res.is_error, Some(true));
    assert!(mock.calls().is_empty());
}

#[tokio::test]
async fn prod_allows_writes_with_override() {
    let mock = MockRunner::new(vec![Ok(json!({"object":"transaction"}))]);
    let srv = FluteServer::new(cfg(Profile::Production, true, None), mock.clone());
    srv.transactions_sale(Parameters(SaleArgs { amount: "10.00".into(), ..Default::default() })).await.unwrap();
    assert_eq!(mock.calls()[0], svec(["--profile","production","--output","json","transactions","sale","--amount","10.00"]));
}

#[tokio::test]
async fn prod_allows_reads() {
    let mock = MockRunner::new(vec![Ok(json!({"object":"transaction_list"}))]);
    let srv = FluteServer::new(cfg(Profile::Production, false, None), mock.clone());
    srv.transactions_list(Parameters(TransactionsList::default())).await.unwrap();
    assert_eq!(mock.calls().len(), 1);
}

use flute_cli_mcp::tools::customers::{AddCard, CustomerFields, CustomerUpdate, CustomersList, RemoveMethod};

#[tokio::test]
async fn customers_argv() {
    let (srv, mock) = sandbox(4);
    srv.customers_list(Parameters(CustomersList { search: Some("ann".into()), ..Default::default() })).await.unwrap();
    srv.customers_create(Parameters(CustomerFields { first_name: Some("Ann".into()), email: Some("a@b.com".into()), ..Default::default() })).await.unwrap();
    srv.customers_update(Parameters(CustomerUpdate { id: "c1".into(), fields: CustomerFields { mobile: Some("5551234".into()), ..Default::default() } })).await.unwrap();
    srv.customers_remove_method(Parameters(RemoveMethod { id: "c1".into(), method_id: "m9".into() })).await.unwrap();
    let c = mock.calls();
    assert_eq!(c[0], svec(["--profile","sandbox","--output","json","customers","list","--search","ann"]));
    assert_eq!(c[1], svec(["--profile","sandbox","--output","json","customers","create","--first-name","Ann","--email","a@b.com"]));
    assert_eq!(c[2], svec(["--profile","sandbox","--output","json","customers","update","c1","--mobile","5551234"]));
    assert_eq!(c[3], svec(["--profile","sandbox","--output","json","customers","remove-method","c1","m9","--yes"]));
    let _ = AddCard::default(); // keep import used if add-card test added later
}

use flute_cli_mcp::tools::devices::{DeviceId, DeviceRegister};

#[tokio::test]
async fn terminals_and_devices_argv() {
    let (srv, mock) = sandbox(4);
    srv.terminals_status(Parameters(flute_cli_mcp::tools::Id { id: "term1".into() })).await.unwrap();
    srv.devices_list(Parameters(flute_cli_mcp::tools::Empty {})).await.unwrap();
    srv.devices_ttp_jwt(Parameters(DeviceId { device_id: "d1".into() })).await.unwrap();
    srv.devices_register(Parameters(DeviceRegister { id: "d1".into(), name: Some("Lane 1".into()) })).await.unwrap();
    let c = mock.calls();
    assert_eq!(c[0], svec(["--profile","sandbox","--output","json","terminals","status","term1"]));
    assert_eq!(c[1], svec(["--profile","sandbox","--output","json","devices","list"]));
    assert_eq!(c[2], svec(["--profile","sandbox","--output","json","devices","ttp-jwt","--device-id","d1"]));
    assert_eq!(c[3], svec(["--profile","sandbox","--output","json","devices","register","d1","--name","Lane 1"]));
}

use flute_cli_mcp::tools::ach::AchMove;

#[tokio::test]
async fn ach_debit_argv() {
    let (srv, mock) = sandbox(1);
    srv.ach_debit(Parameters(AchMove {
        amount: "25.00".into(), payment_processor_id: "pp1".into(),
        routing: "021000021".into(), account: "123456789".into(),
        account_type: "checking".into(), account_holder_type: "personal".into(),
        billing_line1: "1 Main St".into(), billing_city: "Austin".into(),
        billing_state: "TX".into(), billing_state_id: 44, billing_postal_code: "78701".into(),
        contact_first_name: "A".into(), contact_last_name: "B".into(),
        contact_email: "a@b.com".into(), contact_phone: "5125551234".into(),
        ..Default::default()
    })).await.unwrap();
    assert_eq!(mock.calls()[0], svec([
        "--profile","sandbox","--output","json","ach","debit",
        "--amount","25.00","--payment-processor-id","pp1","--routing","021000021","--account","123456789",
        "--account-type","checking","--account-holder-type","personal",
        "--billing-line1","1 Main St","--billing-city","Austin","--billing-state","TX",
        "--billing-state-id","44","--billing-postal-code","78701","--billing-country-id","1",
        "--contact-first-name","A","--contact-last-name","B","--contact-email","a@b.com","--contact-phone","5125551234",
        "--sec-code","1","--requester-ip","127.0.0.1",
    ]));
}

use flute_cli_mcp::tools::pos::{PosCreate, PosList};

#[tokio::test]
async fn pos_argv_has_no_wait() {
    let (srv, mock) = sandbox(2);
    srv.pos_list(Parameters(PosList { terminal_id: Some("term1".into()), ..Default::default() })).await.unwrap();
    srv.pos_create(Parameters(PosCreate {
        terminal_id: "term1".into(), amount: "10.00".into(),
        pos_device_id: "dev1".into(), reference_id: "ref-1".into(), ..Default::default()
    })).await.unwrap();
    let c = mock.calls();
    assert_eq!(c[0], svec(["--profile","sandbox","--output","json","pos","list","--terminal-id","term1"]));
    assert_eq!(c[1], svec(["--profile","sandbox","--output","json","pos","create","--terminal-id","term1","--amount","10.00","--pos-device-id","dev1","--reference-id","ref-1"]));
    assert!(!c[1].iter().any(|a| a == "--wait"), "pos_create must not pass --wait");
}

use flute_cli_mcp::tools::settlements::SettlementsList;
use flute_cli_mcp::tools::subscriptions::SubscriptionCreate;

#[tokio::test]
async fn settlements_and_subscriptions_argv() {
    let (srv, mock) = sandbox(2);
    srv.settlements_list(Parameters(SettlementsList { status: Some("open".into()), ..Default::default() })).await.unwrap();
    srv.subscriptions_create(Parameters(SubscriptionCreate {
        customer_id: "c1".into(), payment_method_id: "pm1".into(),
        amount: "9.99".into(), number_of_payments: 12, ..Default::default()
    })).await.unwrap();
    let c = mock.calls();
    assert_eq!(c[0], svec(["--profile","sandbox","--output","json","settlements","list","--status","open"]));
    assert_eq!(c[1], svec(["--profile","sandbox","--output","json","subscriptions","create","--customer-id","c1","--payment-method-id","pm1","--amount","9.99","--number-of-payments","12"]));
}

use flute_cli_mcp::tools::tokens::{TokenCreate, TokenRevoke};

#[tokio::test]
async fn tokens_create_uses_per_call_merchant_id() {
    let (srv, mock) = sandbox(1);
    srv.tokens_create(Parameters(TokenCreate { name: "ci".into(), merchant_id: Some("m-1".into()) })).await.unwrap();
    assert_eq!(mock.calls()[0], svec(["--profile","sandbox","--output","json","tokens","create","--merchant-id","m-1","--name","ci"]));
}

#[tokio::test]
async fn tokens_create_falls_back_to_pinned_merchant_id() {
    let mock = MockRunner::new(vec![Ok(json!({"object":"api_token"}))]);
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, Some("m-pinned")), mock.clone());
    srv.tokens_create(Parameters(TokenCreate { name: "ci".into(), merchant_id: None })).await.unwrap();
    assert_eq!(mock.calls()[0], svec(["--profile","sandbox","--output","json","tokens","create","--merchant-id","m-pinned","--name","ci"]));
}

#[tokio::test]
async fn tokens_create_errors_without_any_merchant_id() {
    let mock = MockRunner::new(vec![]); // CLI must never be called
    let srv = FluteServer::new(cfg(Profile::Sandbox, false, None), mock.clone());
    let res = srv.tokens_create(Parameters(TokenCreate { name: "ci".into(), merchant_id: None })).await.unwrap();
    assert_eq!(res.is_error, Some(true));
    assert!(mock.calls().is_empty());
    let _ = TokenRevoke::default();
}
