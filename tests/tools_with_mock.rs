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
