use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use flute_cli_mcp::error::FluteError;
use flute_cli_mcp::runner::{CliRunner, ProcessRunner};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::RwLock;

/// Keeps a fake-binary write from overlapping a spawn in a parallel test.
/// On Linux a child forked by another test inherits this process's open file
/// descriptors until its `exec` closes them, and exec'ing a script that any
/// process still holds open for writing fails with ETXTBSY ("Text file busy").
/// Writes take the lock exclusively and runs take it shared, so no fork can
/// happen while a script is open for writing.
static SPAWN_LOCK: RwLock<()> = RwLock::const_new(());

async fn fake(dir: &TempDir, body: &str) -> PathBuf {
    let _guard = SPAWN_LOCK.write().await;
    let path = dir.path().join("flute");
    fs::write(&path, body).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

async fn run(runner: &ProcessRunner, args: &[String]) -> Result<Value, FluteError> {
    let _guard = SPAWN_LOCK.read().await;
    runner.run(args).await
}

fn runner(bin: PathBuf, secs: u64) -> ProcessRunner {
    ProcessRunner {
        binary: bin,
        timeout: Duration::from_secs(secs),
        debug: false,
    }
}

#[tokio::test]
async fn parses_success_json() {
    let dir = TempDir::new().unwrap();
    let bin = fake(&dir, "#!/bin/sh\nprintf '{\"object\":\"ping\"}'\nexit 0\n").await;
    let out = run(&runner(bin, 5), &["ping".into()]).await.unwrap();
    assert_eq!(out, json!({"object": "ping"}));
}

#[tokio::test]
async fn empty_stdout_on_success_is_null() {
    let dir = TempDir::new().unwrap();
    let bin = fake(&dir, "#!/bin/sh\nexit 0\n").await;
    let out = run(&runner(bin, 5), &["x".into()]).await.unwrap();
    assert_eq!(out, json!(null));
}

#[tokio::test]
async fn api_envelope_on_nonzero_exit() {
    let dir = TempDir::new().unwrap();
    let bin = fake(
        &dir,
        "#!/bin/sh\nprintf '{\"kind\":\"api\",\"message\":\"nope\",\"status\":404}'\nexit 4\n",
    )
    .await;
    match run(&runner(bin, 5), &["x".into()]).await {
        Err(FluteError::Api {
            status, message, ..
        }) => {
            assert_eq!(status, 404);
            assert_eq!(message, "nope");
        }
        other => panic!("expected Api, got {other:?}"),
    }
}

#[tokio::test]
async fn unparseable_nonzero_is_bad_output() {
    let dir = TempDir::new().unwrap();
    let bin = fake(&dir, "#!/bin/sh\nprintf 'boom'\nexit 1\n").await;
    assert!(matches!(
        run(&runner(bin, 5), &["x".into()]).await,
        Err(FluteError::BadOutput { exit_code: 1, .. })
    ));
}

#[tokio::test]
async fn missing_binary_is_spawn_error() {
    let r = runner(PathBuf::from("/no/such/flute"), 5);
    assert!(matches!(
        run(&r, &["x".into()]).await,
        Err(FluteError::Spawn(_))
    ));
}

#[tokio::test]
async fn hung_child_times_out() {
    let dir = TempDir::new().unwrap();
    let bin = fake(&dir, "#!/bin/sh\nsleep 5\n").await;
    let r = ProcessRunner {
        binary: bin,
        timeout: Duration::from_millis(200),
        debug: false,
    };
    assert!(matches!(
        run(&r, &["x".into()]).await,
        Err(FluteError::Timeout { .. })
    ));
}
