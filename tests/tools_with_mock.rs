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
