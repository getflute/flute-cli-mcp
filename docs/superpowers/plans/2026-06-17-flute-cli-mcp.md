# flute-cli-mcp Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an MCP (Model Context Protocol) stdio server in Rust that drives the `flute` payments CLI, exposing 47 tools (full non-interactive surface) with a production write guard.

**Architecture:** A thin `rmcp` stdio server. Each tool spawns `flute --profile <pinned> --output json <group> <action> …` once via `tokio::process`, parses stdout JSON, and surfaces success or the CLI's `{kind,message,status?,correlation_id?}` error envelope through MCP. Profile is pinned at startup (one instance per environment). Tools are split into per-group modules whose `ToolRouter`s combine into one router on the server struct. Mirrors the sibling `flute-webhooks-mcp` project.

**Tech Stack:** Rust 2024, `rmcp` 1.7 (server, transport-io, macros, schemars), `tokio`, `serde`/`serde_json`, `schemars` 1, `thiserror` 2, `which`, `clap`, `async-trait`, `tracing`. Distribution via cargo-dist.

---

## Reference material

- Pattern to mirror: `/Users/chad.lung/Rust-Projects/flute-webhooks-mcp` (`src/{config,error,runner,server}.rs`, `tests/`, `dist-workspace.toml`, `readme.md`).
- CLI contract: `/Users/chad.lung/aurora-payments/flute-cli/agents.md` — authoritative for every subcommand's flags. The argv each tool builds is taken from this file.
- Design spec: `docs/superpowers/specs/2026-06-17-flute-cli-mcp-design.md`.

## Verified API facts (do not re-derive)

- `flute`'s `--profile`, `--output`, `--merchant-id` are all `global = true` → they may precede the subcommand. Base args are `["--profile", <p>, "--output", "json"]`.
- `flute auth status --output json` exits 0 regardless of credential state and prints an envelope whose `data.has_credentials` is a bool (covers keychain + env-var creds). Never prints the JWT. This is the `auth_status` probe.
- `rmcp` 1.7: `#[tool_router(router = NAME, vis = "pub(crate)")]` on an `impl` block generates `pub(crate) fn NAME() -> ToolRouter<Self>`. `ToolRouter<S>` implements `std::ops::Add`. `#[tool_handler(router = self.tool_router)]` makes the `ServerHandler` use a `tool_router` field (built once). `CallToolResult::success(Vec<Content>)` / `::error(Vec<Content>)` set `is_error` to `Some(false)`/`Some(true)`. `Content::json(v)` returns `Result<Content, ErrorData>`.

## File structure

```
flute-cli-mcp/
├── Cargo.toml                       Task 1
├── dist-workspace.toml              Task 16
├── readme.md                        Task 15
├── .github/workflows/release.yml    Task 16 (generated)
├── src/
│   ├── main.rs                      Task 13   clap args, tracing, Config, serve
│   ├── lib.rs                       Task 1+   module declarations (grows per task)
│   ├── error.rs                     Task 2    FluteError + envelope parser
│   ├── config.rs                    Task 3    Config + Profile + env loading
│   ├── runner.rs                    Task 4    CliRunner + ProcessRunner + MockRunner
│   ├── server.rs                    Task 5    FluteServer, base_args, run_cli, guard, helpers, ServerHandler
│   └── tools/
│       ├── mod.rs                   Task 5    shared param structs + helpers
│       ├── util.rs                  Task 5    ping, version, auth_status
│       ├── transactions.rs          Task 6
│       ├── ach.rs                   Task 7
│       ├── customers.rs             Task 8
│       ├── terminals.rs             Task 9
│       ├── devices.rs               Task 9
│       ├── pos.rs                   Task 10
│       ├── settlements.rs           Task 11
│       ├── subscriptions.rs         Task 11
│       └── tokens.rs                Task 12
└── tests/
    ├── runner_unit.rs               Task 4    ProcessRunner vs fake binary
    ├── tools_with_mock.rs           Task 5+   argv shape + guard (grows per task)
    └── e2e_stdio.rs                 Task 14   real stdio MCP frames
```

Each tool-group task (6–12) follows the identical shape: create `tools/<group>.rs`, add `pub mod <group>;` to `tools/mod.rs`, extend the `+` chain in `FluteServer::new`, add argv tests, run, commit.

---

### Task 1: Project manifest + lib skeleton

**Files:**
- Modify: `Cargo.toml`
- Create: `src/lib.rs`

- [ ] **Step 1: Replace `Cargo.toml`**

```toml
[package]
name = "flute-cli-mcp"
version = "0.1.0"
edition = "2024"
description = "MCP server that drives the flute payments CLI"
license = "MIT"
repository = "https://github.com/getflute/flute-cli-mcp"
homepage = "https://github.com/getflute/flute-cli-mcp"

[[bin]]
name = "flute-cli-mcp"
path = "src/main.rs"

[lib]
name = "flute_cli_mcp"
path = "src/lib.rs"

[dependencies]
rmcp = { version = "1.7", features = ["server", "transport-io", "macros", "schemars"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "process", "time", "signal", "io-util"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
schemars = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
which = "8"
clap = { version = "4", features = ["derive", "env"] }
async-trait = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread", "process", "time", "test-util"] }
pretty_assertions = "1"
tempfile = "3"
assert_cmd = "2"
serde_json = "1"

[profile.dist]
inherits = "release"
lto = "thin"
```

- [ ] **Step 2: Create `src/lib.rs`** (modules are added as later tasks create them)

```rust
pub mod error;
```

- [ ] **Step 3: Create a placeholder `src/error.rs`** so the crate compiles

```rust
// Replaced in Task 2.
```

- [ ] **Step 4: Verify it builds**

Run: `cargo build`
Expected: compiles (a stub binary `src/main.rs` already exists; leave it for now).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/lib.rs src/error.rs
git commit -m "chore: set up flute-cli-mcp crate manifest and lib skeleton"
```

---

### Task 2: `FluteError` + envelope parser

This is ported verbatim from the reference (`flute-webhooks-mcp/src/error.rs`); only wording mentions `flute`.

**Files:**
- Modify: `src/error.rs`

- [ ] **Step 1: Write `src/error.rs`**

```rust
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FluteError {
    #[error("api error (status {status}): {message}")]
    Api {
        status: u16,
        message: String,
        correlation_id: Option<String>,
    },
    #[error("transport error: {message}")]
    Transport { message: String },
    #[error("auth error: {message} — run `flute auth login`")]
    Auth { message: String },
    #[error("decode error: {message}")]
    Decode { message: String },
    #[error("cli usage error: {message}")]
    Client { message: String },
    #[error("could not spawn flute: {0}")]
    Spawn(String),
    #[error("flute timed out after {secs}s")]
    Timeout { secs: u64 },
    #[error("flute produced unparseable output (exit {exit_code})")]
    BadOutput {
        exit_code: i32,
        stdout: String,
        stderr: String,
    },
}

#[derive(Debug, Deserialize)]
struct Envelope {
    kind: String,
    message: String,
    #[serde(default)]
    status: Option<u16>,
    #[serde(default)]
    correlation_id: Option<String>,
}

impl FluteError {
    pub fn from_envelope_stdout(exit_code: i32, stdout: &str, stderr: &str) -> FluteError {
        let parsed: Result<Envelope, _> = serde_json::from_str(stdout.trim());
        let Ok(env) = parsed else {
            return FluteError::BadOutput {
                exit_code,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            };
        };
        match env.kind.as_str() {
            "api" => FluteError::Api {
                status: env.status.unwrap_or(0),
                message: env.message,
                correlation_id: env.correlation_id,
            },
            "transport" => FluteError::Transport { message: env.message },
            "auth" => FluteError::Auth { message: env.message },
            "decode" => FluteError::Decode { message: env.message },
            "client" => FluteError::Client { message: env.message },
            _ => FluteError::BadOutput {
                exit_code,
                stdout: stdout.to_string(),
                stderr: stderr.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_api_envelope_with_all_fields() {
        let body = r#"{"kind":"api","message":"validation failed","status":422,"correlation_id":"abc-123"}"#;
        match FluteError::from_envelope_stdout(1, body, "") {
            FluteError::Api { status, message, correlation_id } => {
                assert_eq!(status, 422);
                assert_eq!(message, "validation failed");
                assert_eq!(correlation_id.as_deref(), Some("abc-123"));
            }
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn parses_auth_envelope_without_status() {
        let body = r#"{"kind":"auth","message":"no credentials"}"#;
        assert!(matches!(
            FluteError::from_envelope_stdout(1, body, ""),
            FluteError::Auth { message } if message == "no credentials"
        ));
    }

    #[test]
    fn unparseable_stdout_becomes_bad_output() {
        match FluteError::from_envelope_stdout(2, "not json", "stderr text") {
            FluteError::BadOutput { exit_code, stdout, stderr } => {
                assert_eq!(exit_code, 2);
                assert_eq!(stdout, "not json");
                assert_eq!(stderr, "stderr text");
            }
            other => panic!("expected BadOutput, got {other:?}"),
        }
    }

    #[test]
    fn unknown_kind_becomes_bad_output() {
        let body = r#"{"kind":"martian","message":"x"}"#;
        assert!(matches!(
            FluteError::from_envelope_stdout(1, body, ""),
            FluteError::BadOutput { .. }
        ));
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test --lib error::`
Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/error.rs
git commit -m "feat: add FluteError envelope parser"
```

---

### Task 3: `Config` + `Profile` + env loading

Extends the reference's `config.rs` with `merchant_id`, `allow_prod_writes`, and the `flute` binary name. All fields are `pub` so tests construct `Config` directly.

**Files:**
- Create: `src/config.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add the module to `src/lib.rs`**

```rust
pub mod config;
pub mod error;
```

- [ ] **Step 2: Write `src/config.rs`**

```rust
use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Sandbox,
    Production,
}

impl Profile {
    pub fn as_cli_str(self) -> &'static str {
        match self {
            Profile::Sandbox => "sandbox",
            Profile::Production => "production",
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid FLUTE_PROFILE value `{0}` (expected `sandbox` or `production`)")]
    InvalidProfile(String),
    #[error("invalid FLUTE_MCP_TIMEOUT_SECS value `{0}` (expected positive integer)")]
    InvalidTimeout(String),
    #[error("could not locate `flute` on PATH and FLUTE_BIN is unset")]
    BinaryNotFound,
    #[error("FLUTE_BIN=`{0}` does not exist or is not executable")]
    BinaryUnusable(String),
}

#[derive(Debug, Clone)]
pub struct Config {
    pub profile: Profile,
    pub binary: PathBuf,
    pub merchant_id: Option<String>,
    pub timeout: Duration,
    pub debug: bool,
    pub allow_prod_writes: bool,
}

impl Config {
    /// Build a Config from a closure that returns env vars (so tests can inject).
    pub fn from_env<F>(getenv: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let profile = match getenv("FLUTE_PROFILE").as_deref() {
            None | Some("") | Some("sandbox") => Profile::Sandbox,
            Some("production") | Some("prod") => Profile::Production,
            Some(other) => return Err(ConfigError::InvalidProfile(other.to_string())),
        };

        let timeout = match getenv("FLUTE_MCP_TIMEOUT_SECS") {
            None => Duration::from_secs(30),
            Some(s) => {
                let n: u64 = s.parse().map_err(|_| ConfigError::InvalidTimeout(s.clone()))?;
                if n == 0 {
                    return Err(ConfigError::InvalidTimeout(s));
                }
                Duration::from_secs(n)
            }
        };

        let binary = match getenv("FLUTE_BIN") {
            Some(p) if !p.is_empty() => {
                let path = PathBuf::from(&p);
                if !path.is_file() {
                    return Err(ConfigError::BinaryUnusable(p));
                }
                path
            }
            _ => which::which("flute").map_err(|_| ConfigError::BinaryNotFound)?,
        };

        let merchant_id = getenv("FLUTE_MERCHANT_ID").filter(|s| !s.is_empty());
        let debug = matches!(getenv("FLUTE_MCP_DEBUG").as_deref(), Some(v) if !v.is_empty());
        let allow_prod_writes =
            matches!(getenv("FLUTE_MCP_ALLOW_PROD_WRITES").as_deref(), Some(v) if !v.is_empty());

        Ok(Self {
            profile,
            binary,
            merchant_id,
            timeout,
            debug,
            allow_prod_writes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn make_env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| map.get(k).cloned()
    }

    fn fake_binary(dir: &TempDir) -> PathBuf {
        let path = dir.path().join("flute");
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    #[test]
    fn defaults_to_sandbox_30s_no_prod_writes() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        let env = make_env(&[("FLUTE_BIN", bin.to_str().unwrap())]);
        let cfg = Config::from_env(env).unwrap();
        assert_eq!(cfg.profile, Profile::Sandbox);
        assert_eq!(cfg.timeout, Duration::from_secs(30));
        assert!(!cfg.debug);
        assert!(!cfg.allow_prod_writes);
        assert_eq!(cfg.merchant_id, None);
    }

    #[test]
    fn accepts_production_and_prod_alias() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        for value in ["production", "prod"] {
            let env = make_env(&[("FLUTE_BIN", bin.to_str().unwrap()), ("FLUTE_PROFILE", value)]);
            assert_eq!(Config::from_env(env).unwrap().profile, Profile::Production);
        }
    }

    #[test]
    fn reads_merchant_id_and_prod_write_override() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        let env = make_env(&[
            ("FLUTE_BIN", bin.to_str().unwrap()),
            ("FLUTE_MERCHANT_ID", "m-123"),
            ("FLUTE_MCP_ALLOW_PROD_WRITES", "1"),
        ]);
        let cfg = Config::from_env(env).unwrap();
        assert_eq!(cfg.merchant_id.as_deref(), Some("m-123"));
        assert!(cfg.allow_prod_writes);
    }

    #[test]
    fn rejects_unknown_profile() {
        let dir = TempDir::new().unwrap();
        let bin = fake_binary(&dir);
        let env = make_env(&[("FLUTE_BIN", bin.to_str().unwrap()), ("FLUTE_PROFILE", "staging")]);
        assert!(matches!(Config::from_env(env), Err(ConfigError::InvalidProfile(s)) if s == "staging"));
    }

    #[test]
    fn missing_binary_errors() {
        let env = make_env(&[("FLUTE_BIN", "/nope/does/not/exist")]);
        assert!(matches!(Config::from_env(env), Err(ConfigError::BinaryUnusable(_))));
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --lib config::`
Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/config.rs src/lib.rs
git commit -m "feat: add Config with profile, merchant id, and prod-write override"
```

---

### Task 4: `CliRunner` trait + `ProcessRunner` + `MockRunner`

Ported verbatim from the reference (`flute-webhooks-mcp/src/runner.rs`). Plus `tests/runner_unit.rs` exercising `ProcessRunner` against fake shell scripts.

**Files:**
- Create: `src/runner.rs`
- Create: `tests/runner_unit.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add the module to `src/lib.rs`**

```rust
pub mod config;
pub mod error;
pub mod runner;
```

- [ ] **Step 2: Write `src/runner.rs`**

```rust
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::timeout;

use crate::error::FluteError;

#[async_trait]
pub trait CliRunner: Send + Sync {
    /// Run the upstream CLI with these argv tokens (the binary path is owned by the
    /// runner). On success returns the parsed JSON stdout. On failure returns a
    /// FluteError carrying the structured envelope fields when available.
    async fn run(&self, args: &[String]) -> Result<Value, FluteError>;
}

/// Test double. Records every `run` call and returns canned responses in order.
pub struct MockRunner {
    pub calls: Mutex<Vec<Vec<String>>>,
    pub responses: Mutex<Vec<Result<Value, FluteError>>>,
}

impl MockRunner {
    pub fn new(responses: Vec<Result<Value, FluteError>>) -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(responses),
        })
    }

    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl CliRunner for MockRunner {
    async fn run(&self, args: &[String]) -> Result<Value, FluteError> {
        self.calls.lock().unwrap().push(args.to_vec());
        let next = self.responses.lock().unwrap().drain(..1).next();
        match next {
            Some(r) => r,
            None => panic!("MockRunner: no canned response for call {args:?}"),
        }
    }
}

pub struct ProcessRunner {
    pub binary: PathBuf,
    pub timeout: Duration,
    pub debug: bool,
}

#[async_trait]
impl CliRunner for ProcessRunner {
    async fn run(&self, args: &[String]) -> Result<Value, FluteError> {
        let mut cmd = Command::new(&self.binary);
        cmd.args(args)
            .env_remove("RUST_LOG")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let child = cmd
            .spawn()
            .map_err(|e| FluteError::Spawn(format!("{}: {}", self.binary.display(), e)))?;

        let output = match timeout(self.timeout, child.wait_with_output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Err(FluteError::Spawn(e.to_string())),
            Err(_) => return Err(FluteError::Timeout { secs: self.timeout.as_secs() }),
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if self.debug && !stderr.is_empty() {
            tracing::debug!(target: "flute_cli_mcp::runner", "flute stderr: {stderr}");
        }

        let exit_code = output.status.code().unwrap_or(-1);

        if output.status.success() {
            if stdout.trim().is_empty() {
                return Ok(Value::Null);
            }
            serde_json::from_str(&stdout).map_err(|e| FluteError::Decode {
                message: format!("could not parse stdout as JSON: {e}"),
            })
        } else {
            Err(FluteError::from_envelope_stdout(exit_code, &stdout, &stderr))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[tokio::test]
    async fn mock_records_calls_and_replays_responses() {
        let mock = MockRunner::new(vec![Ok(json!({"data": []}))]);
        let out = mock.run(&["a".into(), "b".into()]).await.unwrap();
        assert_eq!(out, json!({"data": []}));
        assert_eq!(mock.calls(), vec![vec!["a".to_string(), "b".to_string()]]);
    }
}
```

- [ ] **Step 3: Write `tests/runner_unit.rs`**

```rust
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use flute_cli_mcp::error::FluteError;
use flute_cli_mcp::runner::{CliRunner, ProcessRunner};
use serde_json::json;
use tempfile::TempDir;

fn fake(dir: &TempDir, body: &str) -> PathBuf {
    let path = dir.path().join("flute");
    fs::write(&path, body).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

fn runner(bin: PathBuf, secs: u64) -> ProcessRunner {
    ProcessRunner { binary: bin, timeout: Duration::from_secs(secs), debug: false }
}

#[tokio::test]
async fn parses_success_json() {
    let dir = TempDir::new().unwrap();
    let bin = fake(&dir, "#!/bin/sh\nprintf '{\"object\":\"ping\"}'\nexit 0\n");
    let out = runner(bin, 5).run(&["ping".into()]).await.unwrap();
    assert_eq!(out, json!({"object": "ping"}));
}

#[tokio::test]
async fn empty_stdout_on_success_is_null() {
    let dir = TempDir::new().unwrap();
    let bin = fake(&dir, "#!/bin/sh\nexit 0\n");
    let out = runner(bin, 5).run(&["x".into()]).await.unwrap();
    assert_eq!(out, json!(null));
}

#[tokio::test]
async fn api_envelope_on_nonzero_exit() {
    let dir = TempDir::new().unwrap();
    let bin = fake(
        &dir,
        "#!/bin/sh\nprintf '{\"kind\":\"api\",\"message\":\"nope\",\"status\":404}'\nexit 4\n",
    );
    match runner(bin, 5).run(&["x".into()]).await {
        Err(FluteError::Api { status, message, .. }) => {
            assert_eq!(status, 404);
            assert_eq!(message, "nope");
        }
        other => panic!("expected Api, got {other:?}"),
    }
}

#[tokio::test]
async fn unparseable_nonzero_is_bad_output() {
    let dir = TempDir::new().unwrap();
    let bin = fake(&dir, "#!/bin/sh\nprintf 'boom'\nexit 1\n");
    assert!(matches!(
        runner(bin, 5).run(&["x".into()]).await,
        Err(FluteError::BadOutput { exit_code: 1, .. })
    ));
}

#[tokio::test]
async fn missing_binary_is_spawn_error() {
    let r = runner(PathBuf::from("/no/such/flute"), 5);
    assert!(matches!(r.run(&["x".into()]).await, Err(FluteError::Spawn(_))));
}

#[tokio::test]
async fn hung_child_times_out() {
    let dir = TempDir::new().unwrap();
    let bin = fake(&dir, "#!/bin/sh\nsleep 5\n");
    let r = ProcessRunner {
        binary: bin,
        timeout: Duration::from_millis(200),
        debug: false,
    };
    assert!(matches!(r.run(&["x".into()]).await, Err(FluteError::Timeout { .. })));
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib runner:: ; cargo test --test runner_unit`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/runner.rs tests/runner_unit.rs src/lib.rs
git commit -m "feat: add CliRunner with ProcessRunner and MockRunner"
```

---

### Task 5: `FluteServer` core + shared tool helpers + `util` tools

This task makes the server compile end-to-end with the smallest tool group (`util`: `ping`, `version`, `auth_status`). Later tasks add more group modules and extend the router chain in `FluteServer::new`.

**Files:**
- Create: `src/server.rs`
- Create: `src/tools/mod.rs`
- Create: `src/tools/util.rs`
- Create: `tests/tools_with_mock.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Update `src/lib.rs`**

```rust
pub mod config;
pub mod error;
pub mod runner;
pub mod server;
pub mod tools;
```

- [ ] **Step 2: Write `src/tools/mod.rs`** (shared param structs + result helpers)

```rust
use rmcp::model::{CallToolResult, Content};
use serde::Deserialize;
use serde_json::Value;

use crate::error::FluteError;

pub mod util;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

/// A bare resource id, used by the many `get`/`status`/positional tools.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Id {
    /// The resource id.
    pub id: String,
}

pub(crate) fn value_to_result(value: Value) -> CallToolResult {
    CallToolResult::success(vec![
        Content::json(value).expect("serde_json::Value is always JSON-serializable"),
    ])
}

pub(crate) fn flute_err_to_result(err: FluteError) -> CallToolResult {
    let payload = match &err {
        FluteError::Api { status, message, correlation_id } => serde_json::json!({
            "kind": "api", "status": status, "message": message, "correlation_id": correlation_id,
        }),
        FluteError::Transport { message } => serde_json::json!({ "kind": "transport", "message": message }),
        FluteError::Auth { message } => serde_json::json!({
            "kind": "auth", "message": format!("{message} — run `flute auth login`"),
        }),
        FluteError::Decode { message } => serde_json::json!({ "kind": "decode", "message": message }),
        FluteError::Client { message } => serde_json::json!({ "kind": "client", "message": message }),
        FluteError::Spawn(msg) => serde_json::json!({
            "kind": "spawn",
            "message": format!("could not spawn flute — set FLUTE_BIN or install the CLI ({msg})"),
        }),
        FluteError::Timeout { secs } => serde_json::json!({
            "kind": "timeout", "message": format!("flute timed out after {secs}s"),
        }),
        FluteError::BadOutput { exit_code, stdout, stderr } => {
            let stderr_trunc = if stderr.len() > 4096 {
                &stderr[..stderr.floor_char_boundary(4096)]
            } else {
                stderr.as_str()
            };
            serde_json::json!({
                "kind": "bad_output", "exit_code": exit_code, "stdout": stdout, "stderr": stderr_trunc,
            })
        }
    };
    CallToolResult::error(vec![
        Content::json(payload).expect("serde_json::Value is always JSON-serializable"),
    ])
}
```

- [ ] **Step 3: Write `src/server.rs`**

```rust
use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::ServerInfo,
    tool_handler,
};
use serde_json::Value;

use crate::config::{Config, Profile};
use crate::error::FluteError;
use crate::runner::CliRunner;
use crate::tools::flute_err_to_result;

#[derive(Clone)]
pub struct FluteServer {
    pub(crate) config: Arc<Config>,
    pub(crate) runner: Arc<dyn CliRunner>,
    tool_router: ToolRouter<FluteServer>,
}

impl FluteServer {
    pub fn new(config: Arc<Config>, runner: Arc<dyn CliRunner>) -> Self {
        // Each group module contributes a router via `#[tool_router(router = …)]`.
        // Extend this chain as group modules are added (Tasks 6–12).
        let tool_router = Self::util_router();
        Self { config, runner, tool_router }
    }

    pub(crate) fn base_args(&self) -> Vec<String> {
        vec![
            "--profile".into(),
            self.config.profile.as_cli_str().into(),
            "--output".into(),
            "json".into(),
        ]
    }

    pub(crate) async fn run_cli(&self, args: Vec<String>) -> Result<Value, FluteError> {
        self.runner.run(&args).await
    }

    /// Returns `Some(error_result)` when a write must be refused on a guarded
    /// production instance; `None` when the write may proceed.
    pub(crate) fn guard_write(&self, tool: &str) -> Option<rmcp::model::CallToolResult> {
        if self.config.profile == Profile::Production && !self.config.allow_prod_writes {
            Some(flute_err_to_result(FluteError::Client {
                message: format!(
                    "refusing {tool} on production profile; set FLUTE_MCP_ALLOW_PROD_WRITES=1 to enable writes"
                ),
            }))
        } else {
            None
        }
    }

    /// Resolve the merchant id for token tools: per-call override, else the
    /// pinned `FLUTE_MERCHANT_ID`, else a `client` error.
    pub(crate) fn merchant_id_for(&self, override_id: Option<String>) -> Result<String, FluteError> {
        override_id
            .filter(|s| !s.is_empty())
            .or_else(|| self.config.merchant_id.clone())
            .ok_or_else(|| FluteError::Client {
                message: "merchant_id required: pass `merchant_id` or set FLUTE_MERCHANT_ID".into(),
            })
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for FluteServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default().with_instructions(
            "Drives the `flute` payments CLI. The active profile is pinned at server start; \
             launch one instance per environment (sandbox vs production). On a production \
             instance, write tools are refused unless FLUTE_MCP_ALLOW_PROD_WRITES=1. \
             Credentials come from the OS keychain (`flute auth login`) or FLUTE_CLIENT_ID/\
             FLUTE_CLIENT_SECRET in this server's environment.",
        )
    }
}
```

- [ ] **Step 4: Write `src/tools/util.rs`**

```rust
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};

use crate::server::FluteServer;
use crate::tools::{Empty, flute_err_to_result, value_to_result};

#[tool_router(router = util_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "API health check. Pure read; safe to retry.")]
    pub async fn ping(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.push("ping".into());
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(description = "Print the CLI version and active profile. Safe to retry.")]
    pub async fn version(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.push("version".into());
        Ok(match self.run_cli(args).await {
            Ok(v) => value_to_result(v),
            Err(e) => flute_err_to_result(e),
        })
    }

    #[tool(
        description = "Check whether credentials are present for the active profile. Returns `{authenticated, profile}`; never returns the token. Safe to retry."
    )]
    pub async fn auth_status(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["auth".into(), "status".into()]);
        Ok(match self.run_cli(args).await {
            Ok(v) => {
                let authenticated = v
                    .get("data")
                    .and_then(|d| d.get("has_credentials"))
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                value_to_result(serde_json::json!({
                    "authenticated": authenticated,
                    "profile": self.config.profile.as_cli_str(),
                }))
            }
            Err(e) => flute_err_to_result(e),
        })
    }
}
```

- [ ] **Step 5: Write `tests/tools_with_mock.rs`** (shared harness + util tests)

```rust
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
```

- [ ] **Step 6: Run everything**

Run: `cargo test`
Expected: lib + runner_unit + tools_with_mock all pass; crate builds (the stub `src/main.rs` still present).

- [ ] **Step 7: Commit**

```bash
git add src/server.rs src/tools/mod.rs src/tools/util.rs tests/tools_with_mock.rs src/lib.rs
git commit -m "feat: add FluteServer core, write guard, and util tools"
```

---

### Task 6: `transactions` tools

**Files:**
- Create: `src/tools/transactions.rs`
- Modify: `src/tools/mod.rs` (add `pub mod transactions;`)
- Modify: `src/server.rs` (extend the router chain)
- Modify: `tests/tools_with_mock.rs` (add argv + guard tests)

- [ ] **Step 1: Add `pub mod transactions;` to `src/tools/mod.rs`** (next to `pub mod util;`)

- [ ] **Step 2: Extend the chain in `src/server.rs` `FluteServer::new`**

```rust
let tool_router = Self::util_router() + Self::transactions_router();
```

- [ ] **Step 3: Write `src/tools/transactions.rs`**

```rust
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
pub struct TransactionsList {
    /// Page size (maps to the API `pageSize`).
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub page: Option<u32>,
    /// Only unsettled transactions.
    #[serde(default)]
    pub unsettled: Option<bool>,
    /// Client-side status filter applied to the returned page.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
}

/// Shared input for `sale` and `auth` (identical flag set).
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaleArgs {
    /// Amount as a decimal string, e.g. "10.00" (≤2 decimal places).
    pub amount: String,
    #[serde(default)]
    pub card: Option<String>,
    /// Expiry as MM/YY or MM/YYYY.
    #[serde(default)]
    pub exp: Option<String>,
    #[serde(default)]
    pub cvv: Option<String>,
    #[serde(default)]
    pub tip_amount: Option<String>,
    #[serde(default)]
    pub customer_id: Option<String>,
    #[serde(default)]
    pub payment_method_id: Option<String>,
    /// Currency id; API default 1 = USD.
    #[serde(default)]
    pub currency_id: Option<u32>,
    /// Card data source; CLI default 1 = Internet/ISV.
    #[serde(default)]
    pub card_data_source: Option<u32>,
    #[serde(default)]
    pub reference_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TxnRef {
    pub transaction_id: String,
    /// Optional partial amount (capture/refund).
    #[serde(default)]
    pub amount: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TipAdjust {
    pub transaction_id: String,
    pub tip_amount: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Settle {
    pub payment_processor_id: String,
}

impl FluteServer {
    /// Shared argv builder for `sale`/`auth`.
    fn sale_like_args(&self, sub: &str, p: SaleArgs) -> Vec<String> {
        let mut args = self.base_args();
        args.extend(["transactions".into(), sub.into(), "--amount".into(), p.amount]);
        if let Some(v) = p.card { args.extend(["--card".into(), v]); }
        if let Some(v) = p.exp { args.extend(["--exp".into(), v]); }
        if let Some(v) = p.cvv { args.extend(["--cvv".into(), v]); }
        if let Some(v) = p.tip_amount { args.extend(["--tip-amount".into(), v]); }
        if let Some(v) = p.customer_id { args.extend(["--customer-id".into(), v]); }
        if let Some(v) = p.payment_method_id { args.extend(["--payment-method-id".into(), v]); }
        if let Some(v) = p.currency_id { args.extend(["--currency-id".into(), v.to_string()]); }
        if let Some(v) = p.card_data_source { args.extend(["--card-data-source".into(), v.to_string()]); }
        if let Some(v) = p.reference_id { args.extend(["--reference-id".into(), v]); }
        args
    }
}

#[tool_router(router = transactions_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "List transactions (newest first). Filters --status/--from/--to are applied client-side to the returned page. Safe to retry.")]
    pub async fn transactions_list(&self, Parameters(p): Parameters<TransactionsList>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["transactions".into(), "list".into()]);
        if let Some(v) = p.limit { args.extend(["--limit".into(), v.to_string()]); }
        if let Some(v) = p.page { args.extend(["--page".into(), v.to_string()]); }
        if p.unsettled == Some(true) { args.push("--unsettled".into()); }
        if let Some(v) = p.status { args.extend(["--status".into(), v]); }
        if let Some(v) = p.from { args.extend(["--from".into(), v]); }
        if let Some(v) = p.to { args.extend(["--to".into(), v]); }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Get one transaction by id. Safe to retry.")]
    pub async fn transactions_get(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["transactions".into(), "get".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Rich client-composed view of a transaction, including available operations. Safe to retry.")]
    pub async fn transactions_inspect(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["transactions".into(), "inspect".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Charge a card. NOT idempotent — each call moves money. Use a unique reference_id for server-side duplicate control; reconcile with transactions_list before retrying.")]
    pub async fn transactions_sale(&self, Parameters(p): Parameters<SaleArgs>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_sale") { return Ok(blocked); }
        let args = self.sale_like_args("sale", p);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Authorize (hold) a card without capturing. NOT idempotent. Capture later with transactions_capture.")]
    pub async fn transactions_auth(&self, Parameters(p): Parameters<SaleArgs>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_auth") { return Ok(blocked); }
        let args = self.sale_like_args("auth", p);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Capture a prior authorization. NOT idempotent. Optional partial amount.")]
    pub async fn transactions_capture(&self, Parameters(p): Parameters<TxnRef>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_capture") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["transactions".into(), "capture".into(), "--transaction-id".into(), p.transaction_id]);
        if let Some(v) = p.amount { args.extend(["--amount".into(), v]); }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Void a transaction. 404 on repeat = already voided (idempotent).")]
    pub async fn transactions_void(&self, Parameters(p): Parameters<TxnRef>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_void") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["transactions".into(), "void".into(), "--transaction-id".into(), p.transaction_id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Refund a transaction. NOT idempotent — moves money. Optional partial amount.")]
    pub async fn transactions_refund(&self, Parameters(p): Parameters<TxnRef>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_refund") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["transactions".into(), "refund".into(), "--transaction-id".into(), p.transaction_id]);
        if let Some(v) = p.amount { args.extend(["--amount".into(), v]); }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Settle a payment processor's open batch (batch-level, NOT a single transaction). NOT idempotent.")]
    pub async fn transactions_settle(&self, Parameters(p): Parameters<Settle>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_settle") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["transactions".into(), "settle".into(), "--payment-processor-id".into(), p.payment_processor_id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Adjust the tip on a transaction. NOT idempotent.")]
    pub async fn transactions_tip_adjust(&self, Parameters(p): Parameters<TipAdjust>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("transactions_tip_adjust") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["transactions".into(), "tip-adjust".into(), "--transaction-id".into(), p.transaction_id, "--tip-amount".into(), p.tip_amount]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }
}
```

- [ ] **Step 4: Add tests to `tests/tools_with_mock.rs`**

```rust
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
```

- [ ] **Step 5: Run tests**

Run: `cargo test --test tools_with_mock`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src/tools/transactions.rs src/tools/mod.rs src/server.rs tests/tools_with_mock.rs
git commit -m "feat: add transactions tools and production write-guard tests"
```

---

### Task 7: `ach` tools

**Files:**
- Create: `src/tools/ach.rs`
- Modify: `src/tools/mod.rs`, `src/server.rs`, `tests/tools_with_mock.rs`

- [ ] **Step 1:** Add `pub mod ach;` to `src/tools/mod.rs`.

- [ ] **Step 2:** Extend chain in `src/server.rs`: `… + Self::ach_router();`

- [ ] **Step 3: Write `src/tools/ach.rs`**

```rust
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
    pub billing_state_id: u32,
    pub billing_postal_code: String,
    /// Numeric country id; 1 = US.
    #[serde(default)]
    pub billing_country_id: Option<u32>,
    pub contact_first_name: String,
    pub contact_last_name: String,
    pub contact_email: String,
    pub contact_phone: String,
    /// SEC code; CLI default 1 = Web.
    #[serde(default)]
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
        a.extend(["--billing-country-id".into(), p.billing_country_id.unwrap_or(1).to_string()]);
        a.extend(["--contact-first-name".into(), p.contact_first_name]);
        a.extend(["--contact-last-name".into(), p.contact_last_name]);
        a.extend(["--contact-email".into(), p.contact_email]);
        a.extend(["--contact-phone".into(), p.contact_phone]);
        a.extend(["--sec-code".into(), p.sec_code.unwrap_or(1).to_string()]);
        a.extend(["--requester-ip".into(), p.requester_ip.unwrap_or_else(|| "127.0.0.1".into())]);
        a
    }
}

#[tool_router(router = ach_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "ACH debit (pull funds). NOT idempotent — moves money. Requires a live payment_processor_id, billing address, and contact info.")]
    pub async fn ach_debit(&self, Parameters(p): Parameters<AchMove>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("ach_debit") { return Ok(blocked); }
        let args = self.ach_move_args("debit", p);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "ACH credit (push funds). NOT idempotent — moves money. Same required fields as ach_debit.")]
    pub async fn ach_credit(&self, Parameters(p): Parameters<AchMove>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("ach_credit") { return Ok(blocked); }
        let args = self.ach_move_args("credit", p);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Void an ACH transaction by id. 404 on repeat = idempotent.")]
    pub async fn ach_void(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("ach_void") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["ach".into(), "void".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Refund an ACH transaction by id. NOT idempotent — moves money.")]
    pub async fn ach_refund(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("ach_refund") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["ach".into(), "refund".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }
}
```

- [ ] **Step 4: Add a test to `tests/tools_with_mock.rs`**

```rust
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
```

- [ ] **Step 5: Run** `cargo test --test tools_with_mock` (all pass).
- [ ] **Step 6: Commit**

```bash
git add src/tools/ach.rs src/tools/mod.rs src/server.rs tests/tools_with_mock.rs
git commit -m "feat: add ach tools"
```

---

### Task 8: `customers` tools

**Files:**
- Create: `src/tools/customers.rs`
- Modify: `src/tools/mod.rs`, `src/server.rs`, `tests/tools_with_mock.rs`

- [ ] **Step 1:** Add `pub mod customers;` to `src/tools/mod.rs`.
- [ ] **Step 2:** Extend chain: `… + Self::customers_router();`
- [ ] **Step 3: Write `src/tools/customers.rs`**

```rust
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
pub struct CustomersList {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
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
        if let Some(v) = f.first_name { args.extend(["--first-name".into(), v]); }
        if let Some(v) = f.last_name { args.extend(["--last-name".into(), v]); }
        if let Some(v) = f.email { args.extend(["--email".into(), v]); }
        if let Some(v) = f.company { args.extend(["--company".into(), v]); }
        if let Some(v) = f.mobile { args.extend(["--mobile".into(), v]); }
    }
}

#[tool_router(router = customers_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "Get a customer by id. Safe to retry.")]
    pub async fn customers_get(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["customers".into(), "get".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "List customers. --search is a real server param. Safe to retry.")]
    pub async fn customers_list(&self, Parameters(p): Parameters<CustomersList>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["customers".into(), "list".into()]);
        if let Some(v) = p.limit { args.extend(["--limit".into(), v.to_string()]); }
        if let Some(v) = p.page { args.extend(["--page".into(), v.to_string()]); }
        if let Some(v) = p.search { args.extend(["--search".into(), v]); }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "List a customer's stored payment methods. Safe to retry.")]
    pub async fn customers_methods(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["customers".into(), "methods".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Create a customer. NOT idempotent — duplicates create a second record. Response is minimal; follow with customers_get.")]
    pub async fn customers_create(&self, Parameters(p): Parameters<CustomerFields>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_create") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["customers".into(), "create".into()]);
        Self::customer_field_args(&mut args, p);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Update a customer (GET-merge-PUT — omitted fields retain existing values). Safe to retry.")]
    pub async fn customers_update(&self, Parameters(p): Parameters<CustomerUpdate>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_update") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["customers".into(), "update".into(), p.id]);
        Self::customer_field_args(&mut args, p.fields);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Delete a customer. 404 on repeat = idempotent.")]
    pub async fn customers_delete(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_delete") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["customers".into(), "delete".into(), p.id, "--yes".into()]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Vault a card on a customer. NOT idempotent. Response is minimal; follow with customers_methods.")]
    pub async fn customers_add_card(&self, Parameters(p): Parameters<AddCard>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_add_card") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["customers".into(), "add-card".into(), p.id, "--card".into(), p.card, "--exp".into(), p.exp, "--cvv".into(), p.cvv]);
        if let Some(v) = p.name { args.extend(["--name".into(), v]); }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Vault a bank account (ACH) on a customer. NOT idempotent.")]
    pub async fn customers_add_ach(&self, Parameters(p): Parameters<AddAch>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_add_ach") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["customers".into(), "add-ach".into(), p.id, "--routing".into(), p.routing, "--account".into(), p.account, "--account-type".into(), p.account_type, "--account-holder-type".into(), p.account_holder_type]);
        if let Some(v) = p.tax_id { args.extend(["--tax-id".into(), v]); }
        if let Some(v) = p.name { args.extend(["--name".into(), v]); }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Remove a stored payment method from a customer. 404 on repeat = idempotent.")]
    pub async fn customers_remove_method(&self, Parameters(p): Parameters<RemoveMethod>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("customers_remove_method") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["customers".into(), "remove-method".into(), p.id, p.method_id, "--yes".into()]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }
}
```

- [ ] **Step 4: Add a test to `tests/tools_with_mock.rs`**

```rust
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
```

- [ ] **Step 5: Run** `cargo test --test tools_with_mock` (all pass).
- [ ] **Step 6: Commit**

```bash
git add src/tools/customers.rs src/tools/mod.rs src/server.rs tests/tools_with_mock.rs
git commit -m "feat: add customers tools"
```

---

### Task 9: `terminals` + `devices` tools

**Files:**
- Create: `src/tools/terminals.rs`, `src/tools/devices.rs`
- Modify: `src/tools/mod.rs`, `src/server.rs`, `tests/tools_with_mock.rs`

- [ ] **Step 1:** Add `pub mod terminals;` and `pub mod devices;` to `src/tools/mod.rs`.
- [ ] **Step 2:** Extend chain: `… + Self::terminals_router() + Self::devices_router();`
- [ ] **Step 3: Write `src/tools/terminals.rs`**

```rust
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};

use crate::server::FluteServer;
use crate::tools::{Empty, Id, flute_err_to_result, value_to_result};

#[tool_router(router = terminals_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "List POS terminals. Safe to retry.")]
    pub async fn terminals_list(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["terminals".into(), "list".into()]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Get a terminal's status by id. A terminal must be SemiIntegrated + Online to accept POS transactions. Safe to retry.")]
    pub async fn terminals_status(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["terminals".into(), "status".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }
}
```

- [ ] **Step 4: Write `src/tools/devices.rs`**

```rust
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{Empty, Id, flute_err_to_result, value_to_result};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceRegister {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceId {
    pub device_id: String,
}

#[tool_router(router = devices_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "List mobile payment devices. Records use `deviceId`, not `id`. Safe to retry.")]
    pub async fn devices_list(&self, _p: Parameters<Empty>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["devices".into(), "list".into()]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Get a device by id. Safe to retry.")]
    pub async fn devices_get(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["devices".into(), "get".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Mint a Tap-to-Pay JWT for a device. Returns a `tap_to_pay_jwt` envelope. Idempotent read.")]
    pub async fn devices_ttp_jwt(&self, Parameters(p): Parameters<DeviceId>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["devices".into(), "ttp-jwt".into(), "--device-id".into(), p.device_id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Register a device. NOT idempotent. Optional --name.")]
    pub async fn devices_register(&self, Parameters(p): Parameters<DeviceRegister>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("devices_register") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["devices".into(), "register".into(), p.id]);
        if let Some(v) = p.name { args.extend(["--name".into(), v]); }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Activate Tap-to-Pay on a device. NOT idempotent.")]
    pub async fn devices_ttp_activate(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("devices_ttp_activate") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["devices".into(), "ttp-activate".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }
}
```

- [ ] **Step 5: Add a test to `tests/tools_with_mock.rs`**

```rust
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
```

- [ ] **Step 6: Run** `cargo test --test tools_with_mock` (all pass).
- [ ] **Step 7: Commit**

```bash
git add src/tools/terminals.rs src/tools/devices.rs src/tools/mod.rs src/server.rs tests/tools_with_mock.rs
git commit -m "feat: add terminals and devices tools"
```

---

### Task 10: `pos` tools

`pos_create` deliberately omits `--wait`; the agent polls `pos_get`.

**Files:**
- Create: `src/tools/pos.rs`
- Modify: `src/tools/mod.rs`, `src/server.rs`, `tests/tools_with_mock.rs`

- [ ] **Step 1:** Add `pub mod pos;` to `src/tools/mod.rs`.
- [ ] **Step 2:** Extend chain: `… + Self::pos_router();`
- [ ] **Step 3: Write `src/tools/pos.rs`**

```rust
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
pub struct PosList {
    #[serde(default)]
    pub terminal_id: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub page: Option<u32>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PosCreate {
    pub terminal_id: String,
    /// Amount as a decimal string, e.g. "10.00".
    pub amount: String,
    pub pos_device_id: String,
    pub reference_id: String,
    /// Currency id; default 1.
    #[serde(default)]
    pub currency_id: Option<u32>,
    /// Transaction type; default 2 = Sale (1=Auth,3=Capture,4=Void,5=Refund).
    #[serde(default)]
    pub transaction_type: Option<u32>,
    #[serde(default)]
    pub tip_amount: Option<String>,
    #[serde(default)]
    pub tip_rate: Option<String>,
    #[serde(default)]
    pub customer_id: Option<String>,
    #[serde(default)]
    pub payment_processor_id: Option<String>,
    #[serde(default)]
    pub target_transaction_id: Option<String>,
    #[serde(default)]
    pub reading_method: Option<String>,
}

#[tool_router(router = pos_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "Get a POS transaction by id. get/list use `id` + `posTransactionStatus`. Safe to retry.")]
    pub async fn pos_get(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["pos".into(), "get".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "List POS transactions, optionally by terminal. Safe to retry.")]
    pub async fn pos_list(&self, Parameters(p): Parameters<PosList>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["pos".into(), "list".into()]);
        if let Some(v) = p.terminal_id { args.extend(["--terminal-id".into(), v]); }
        if let Some(v) = p.limit { args.extend(["--limit".into(), v.to_string()]); }
        if let Some(v) = p.page { args.extend(["--page".into(), v.to_string()]); }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Start a terminal (POS) transaction. NOT idempotent. Does not wait — poll pos_get until the response shows completion. A terminal allows only one in-progress transaction; cancel or complete before starting another. create/cancel responses use `posTransactionId` + `status`.")]
    pub async fn pos_create(&self, Parameters(p): Parameters<PosCreate>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("pos_create") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["pos".into(), "create".into(), "--terminal-id".into(), p.terminal_id, "--amount".into(), p.amount, "--pos-device-id".into(), p.pos_device_id, "--reference-id".into(), p.reference_id]);
        if let Some(v) = p.currency_id { args.extend(["--currency-id".into(), v.to_string()]); }
        if let Some(v) = p.transaction_type { args.extend(["--transaction-type".into(), v.to_string()]); }
        if let Some(v) = p.tip_amount { args.extend(["--tip-amount".into(), v]); }
        if let Some(v) = p.tip_rate { args.extend(["--tip-rate".into(), v]); }
        if let Some(v) = p.customer_id { args.extend(["--customer-id".into(), v]); }
        if let Some(v) = p.payment_processor_id { args.extend(["--payment-processor-id".into(), v]); }
        if let Some(v) = p.target_transaction_id { args.extend(["--target-transaction-id".into(), v]); }
        if let Some(v) = p.reading_method { args.extend(["--reading-method".into(), v]); }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Cancel an in-progress POS transaction by id. 404 on repeat = idempotent.")]
    pub async fn pos_cancel(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("pos_cancel") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["pos".into(), "cancel".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }
}
```

- [ ] **Step 4: Add a test to `tests/tools_with_mock.rs`**

```rust
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
```

- [ ] **Step 5: Run** `cargo test --test tools_with_mock` (all pass).
- [ ] **Step 6: Commit**

```bash
git add src/tools/pos.rs src/tools/mod.rs src/server.rs tests/tools_with_mock.rs
git commit -m "feat: add pos tools (no --wait; poll pos_get)"
```

---

### Task 11: `settlements` + `subscriptions` tools

**Files:**
- Create: `src/tools/settlements.rs`, `src/tools/subscriptions.rs`
- Modify: `src/tools/mod.rs`, `src/server.rs`, `tests/tools_with_mock.rs`

- [ ] **Step 1:** Add `pub mod settlements;` and `pub mod subscriptions;` to `src/tools/mod.rs`.
- [ ] **Step 2:** Extend chain: `… + Self::settlements_router() + Self::subscriptions_router();`
- [ ] **Step 3: Write `src/tools/settlements.rs`**

```rust
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
pub struct SettlementsList {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    /// "open" | "settled".
    #[serde(default)]
    pub status: Option<String>,
}

#[tool_router(router = settlements_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "List settlement batches. Safe to retry.")]
    pub async fn settlements_list(&self, Parameters(p): Parameters<SettlementsList>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["settlements".into(), "list".into()]);
        if let Some(v) = p.limit { args.extend(["--limit".into(), v.to_string()]); }
        if let Some(v) = p.page { args.extend(["--page".into(), v.to_string()]); }
        if let Some(v) = p.from { args.extend(["--from".into(), v]); }
        if let Some(v) = p.to { args.extend(["--to".into(), v]); }
        if let Some(v) = p.status { args.extend(["--status".into(), v]); }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Get a settlement batch by id (client-side filter over the fetched page; page-bounded). Safe to retry.")]
    pub async fn settlements_get(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["settlements".into(), "get".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }
}
```

- [ ] **Step 4: Write `src/tools/subscriptions.rs`**

```rust
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
    pub async fn subscriptions_get(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["subscriptions".into(), "get".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "List subscriptions. --status is client-side. Safe to retry.")]
    pub async fn subscriptions_list(&self, Parameters(p): Parameters<SubscriptionsList>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["subscriptions".into(), "list".into()]);
        if let Some(v) = p.limit { args.extend(["--limit".into(), v.to_string()]); }
        if let Some(v) = p.page { args.extend(["--page".into(), v.to_string()]); }
        if let Some(v) = p.search { args.extend(["--search".into(), v]); }
        if let Some(v) = p.customer_id { args.extend(["--customer-id".into(), v]); }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "List the payments for a subscription. Safe to retry.")]
    pub async fn subscriptions_payments(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["subscriptions".into(), "payments".into(), p.id]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Create a recurring subscription. NOT idempotent. payment_method_id must be vaulted + active.")]
    pub async fn subscriptions_create(&self, Parameters(p): Parameters<SubscriptionCreate>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("subscriptions_create") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["subscriptions".into(), "create".into(),
            "--customer-id".into(), p.customer_id,
            "--payment-method-id".into(), p.payment_method_id,
            "--amount".into(), p.amount,
            "--number-of-payments".into(), p.number_of_payments.to_string()]);
        if let Some(v) = p.interval { args.extend(["--interval".into(), v]); }
        if let Some(v) = p.payment_frequency { args.extend(["--payment-frequency".into(), v.to_string()]); }
        if let Some(v) = p.currency_id { args.extend(["--currency-id".into(), v.to_string()]); }
        if let Some(v) = p.transaction_type { args.extend(["--transaction-type".into(), v.to_string()]); }
        if let Some(v) = p.requester_ip { args.extend(["--requester-ip".into(), v]); }
        if let Some(v) = p.payment_processor_id { args.extend(["--payment-processor-id".into(), v]); }
        if let Some(v) = p.start_date { args.extend(["--start-date".into(), v]); }
        if let Some(v) = p.sec_code { args.extend(["--sec-code".into(), v.to_string()]); }
        if p.faster == Some(true) { args.push("--faster".into()); }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Terminate a subscription. 404 on repeat = idempotent.")]
    pub async fn subscriptions_terminate(&self, Parameters(p): Parameters<Id>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("subscriptions_terminate") { return Ok(blocked); }
        let mut args = self.base_args();
        args.extend(["subscriptions".into(), "terminate".into(), p.id, "--yes".into()]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }
}
```

- [ ] **Step 5: Add a test to `tests/tools_with_mock.rs`**

```rust
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
```

- [ ] **Step 6: Run** `cargo test --test tools_with_mock` (all pass).
- [ ] **Step 7: Commit**

```bash
git add src/tools/settlements.rs src/tools/subscriptions.rs src/tools/mod.rs src/server.rs tests/tools_with_mock.rs
git commit -m "feat: add settlements and subscriptions tools"
```

---

### Task 12: `tokens` tools (merchant-id resolution)

**Files:**
- Create: `src/tools/tokens.rs`
- Modify: `src/tools/mod.rs`, `src/server.rs`, `tests/tools_with_mock.rs`

- [ ] **Step 1:** Add `pub mod tokens;` to `src/tools/mod.rs`.
- [ ] **Step 2:** Extend chain: `… + Self::tokens_router();`
- [ ] **Step 3: Write `src/tools/tokens.rs`**

```rust
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ErrorData as McpError},
    tool, tool_router,
};
use serde::Deserialize;

use crate::server::FluteServer;
use crate::tools::{flute_err_to_result, value_to_result};

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TokensList {
    /// Overrides the pinned FLUTE_MERCHANT_ID; optional for list.
    #[serde(default)]
    pub merchant_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenCreate {
    pub name: String,
    /// Overrides the pinned FLUTE_MERCHANT_ID; required (here or via env).
    #[serde(default)]
    pub merchant_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenRevoke {
    pub client_id: String,
    #[serde(default)]
    pub merchant_id: Option<String>,
}

#[tool_router(router = tokens_router, vis = "pub(crate)")]
impl FluteServer {
    #[tool(description = "List ISV API tokens for a merchant. Uses merchant_id, else the pinned FLUTE_MERCHANT_ID, else lists without a merchant filter. Safe to retry.")]
    pub async fn tokens_list(&self, Parameters(p): Parameters<TokensList>) -> Result<CallToolResult, McpError> {
        let mut args = self.base_args();
        args.extend(["tokens".into(), "list".into()]);
        if let Some(m) = p.merchant_id.filter(|s| !s.is_empty()).or_else(|| self.config.merchant_id.clone()) {
            args.extend(["--merchant-id".into(), m]);
        }
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Create an ISV API token. NOT idempotent. The response includes `clientSecret` shown ONCE — store it; the API never returns it again. Requires merchant_id (here or via FLUTE_MERCHANT_ID).")]
    pub async fn tokens_create(&self, Parameters(p): Parameters<TokenCreate>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("tokens_create") { return Ok(blocked); }
        let merchant_id = match self.merchant_id_for(p.merchant_id) {
            Ok(m) => m,
            Err(e) => return Ok(flute_err_to_result(e)),
        };
        let mut args = self.base_args();
        args.extend(["tokens".into(), "create".into(), "--merchant-id".into(), merchant_id, "--name".into(), p.name]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }

    #[tool(description = "Revoke an ISV API token. --merchant-id is required (DELETE needs it). 404 on repeat = idempotent.")]
    pub async fn tokens_revoke(&self, Parameters(p): Parameters<TokenRevoke>) -> Result<CallToolResult, McpError> {
        if let Some(blocked) = self.guard_write("tokens_revoke") { return Ok(blocked); }
        let merchant_id = match self.merchant_id_for(p.merchant_id) {
            Ok(m) => m,
            Err(e) => return Ok(flute_err_to_result(e)),
        };
        let mut args = self.base_args();
        args.extend(["tokens".into(), "revoke".into(), "--client-id".into(), p.client_id, "--merchant-id".into(), merchant_id, "--yes".into()]);
        Ok(match self.run_cli(args).await { Ok(v) => value_to_result(v), Err(e) => flute_err_to_result(e) })
    }
}
```

- [ ] **Step 4: Add tests to `tests/tools_with_mock.rs`**

```rust
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
```

- [ ] **Step 5: Run** `cargo test --test tools_with_mock` (all pass).
- [ ] **Step 6: Commit**

```bash
git add src/tools/tokens.rs src/tools/mod.rs src/server.rs tests/tools_with_mock.rs
git commit -m "feat: add tokens tools with merchant-id resolution"
```

---

### Task 13: `main.rs` — startup wiring

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Replace `src/main.rs`**

```rust
use std::sync::Arc;

use clap::Parser;
use flute_cli_mcp::{
    config::{Config, ConfigError},
    runner::ProcessRunner,
    server::FluteServer,
};
use rmcp::{ServiceExt, transport::io::stdio};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "flute-cli-mcp", about = "MCP server for the flute payments CLI")]
struct Args {
    /// Override `FLUTE_PROFILE` (sandbox | production).
    #[arg(long, env = "FLUTE_PROFILE")]
    profile: Option<String>,
    /// Override `FLUTE_BIN` (path to the `flute` binary).
    #[arg(long, env = "FLUTE_BIN")]
    binary: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();
    let cfg = Config::from_env(|name| match name {
        "FLUTE_PROFILE" => args.profile.clone().or_else(|| std::env::var(name).ok()),
        "FLUTE_BIN" => args.binary.clone().or_else(|| std::env::var(name).ok()),
        other => std::env::var(other).ok(),
    });

    let cfg = match cfg {
        Ok(c) => c,
        Err(ConfigError::BinaryNotFound) => {
            eprintln!(
                "flute-cli-mcp: could not find `flute` on PATH. \
                 Install it from https://github.com/getflute/flute-cli or set FLUTE_BIN."
            );
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("flute-cli-mcp: configuration error: {e}");
            std::process::exit(2);
        }
    };

    tracing::info!(
        profile = %cfg.profile.as_cli_str(),
        binary = %cfg.binary.display(),
        allow_prod_writes = cfg.allow_prod_writes,
        "starting flute-cli-mcp"
    );

    let runner = Arc::new(ProcessRunner {
        binary: cfg.binary.clone(),
        timeout: cfg.timeout,
        debug: cfg.debug,
    });
    let server = FluteServer::new(Arc::new(cfg), runner);

    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

- [ ] **Step 2: Verify build**

Run: `cargo build`
Expected: the `flute-cli-mcp` binary compiles.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire up main.rs startup (clap, tracing, stdio serve)"
```

---

### Task 14: End-to-end stdio test

Spawns the compiled binary with `FLUTE_BIN` pointing at a fake `flute` shell script and drives real MCP frames. Adapted from `flute-webhooks-mcp/tests/e2e_stdio.rs`.

**Files:**
- Create: `tests/e2e_stdio.rs`

- [ ] **Step 1: Write `tests/e2e_stdio.rs`**

```rust
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use tempfile::TempDir;

fn write_fake_flute(dir: &TempDir, stdout: &str, exit_code: i32) -> std::path::PathBuf {
    let path = dir.path().join("flute");
    let script = format!(
        "#!/bin/sh\ncat >/dev/null\nprintf '%s' '{}'\nexit {}\n",
        stdout.replace('\'', "'\\''"),
        exit_code,
    );
    fs::write(&path, script).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

fn jsonrpc(id: u64, method: &str, params: Value) -> String {
    format!("{}\n", serde_json::to_string(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})).unwrap())
}

fn jsonrpc_notify(method: &str, params: Value) -> String {
    format!("{}\n", serde_json::to_string(&json!({"jsonrpc":"2.0","method":method,"params":params})).unwrap())
}

fn read_one_frame(reader: &mut BufReader<impl std::io::Read>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).expect("read frame");
    serde_json::from_str(line.trim()).expect("parse frame")
}

fn start(fake: &std::path::Path, profile: &str) -> std::process::Child {
    let bin = assert_cmd::cargo::cargo_bin("flute-cli-mcp");
    Command::new(bin)
        .env("FLUTE_BIN", fake)
        .env("FLUTE_PROFILE", profile)
        .env("RUST_LOG", "warn")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn server")
}

fn handshake(stdin: &mut impl Write, reader: &mut BufReader<impl std::io::Read>) {
    stdin.write_all(jsonrpc(1, "initialize", json!({
        "protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"e2e","version":"0"}
    })).as_bytes()).unwrap();
    let init = read_one_frame(reader);
    assert_eq!(init["id"], 1);
    assert!(init["result"].is_object());
    stdin.write_all(jsonrpc_notify("notifications/initialized", json!({})).as_bytes()).unwrap();
}

#[test]
fn lists_all_tools_and_calls_a_read() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute(&dir, r#"{"object":"ping","data":{"ok":true}}"#, 0);
    let mut child = start(&fake, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    handshake(&mut stdin, &mut reader);

    // tools/list — expect 47 tools.
    stdin.write_all(jsonrpc(2, "tools/list", json!({})).as_bytes()).unwrap();
    let listed = read_one_frame(&mut reader);
    let tools = listed["result"]["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 47, "expected 47 tools, got {}", tools.len());

    // tools/call ping — fake returns success JSON.
    stdin.write_all(jsonrpc(3, "tools/call", json!({"name":"ping","arguments":{}})).as_bytes()).unwrap();
    let called = read_one_frame(&mut reader);
    assert_eq!(called["id"], 3);
    assert_ne!(called["result"]["isError"], json!(true));

    drop(stdin);
    let _ = child.wait();
}

#[test]
fn surfaces_auth_error_kind() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake_flute(&dir, r#"{"kind":"auth","message":"no credentials"}"#, 2);
    let mut child = start(&fake, "sandbox");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    handshake(&mut stdin, &mut reader);

    stdin.write_all(jsonrpc(2, "tools/call", json!({"name":"transactions_list","arguments":{}})).as_bytes()).unwrap();
    let called = read_one_frame(&mut reader);
    assert_eq!(called["result"]["isError"], json!(true));
    let text = called["result"]["content"][0]["text"].as_str().expect("text content");
    let payload: Value = serde_json::from_str(text).expect("parse error payload");
    assert_eq!(payload["kind"], "auth");

    drop(stdin);
    let _ = child.wait();
}

#[test]
fn production_blocks_a_write_without_spawning() {
    let dir = TempDir::new().unwrap();
    // exit 1 with non-JSON: if the guard fails and the CLI is spawned, the result
    // would be a bad_output error, not the client guard message.
    let fake = write_fake_flute(&dir, "should-not-run", 1);
    let mut child = start(&fake, "production");
    let mut stdin = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());

    handshake(&mut stdin, &mut reader);

    stdin.write_all(jsonrpc(2, "tools/call", json!({"name":"transactions_sale","arguments":{"amount":"1.00"}})).as_bytes()).unwrap();
    let called = read_one_frame(&mut reader);
    assert_eq!(called["result"]["isError"], json!(true));
    let text = called["result"]["content"][0]["text"].as_str().expect("text content");
    let payload: Value = serde_json::from_str(text).expect("parse error payload");
    assert_eq!(payload["kind"], "client");
    assert!(payload["message"].as_str().unwrap().contains("FLUTE_MCP_ALLOW_PROD_WRITES"));

    drop(stdin);
    let _ = child.wait();
}
```

- [ ] **Step 2: Run**

Run: `cargo test --test e2e_stdio`
Expected: 3 tests pass. (If `tools/list` count fails, the assertion message prints the actual count — reconcile against the inventory; the number must be 47.)

- [ ] **Step 3: Commit**

```bash
git add tests/e2e_stdio.rs
git commit -m "test: add end-to-end stdio MCP tests (tools/list, auth error, prod guard)"
```

---

### Task 15: `readme.md`

**Files:**
- Create: `readme.md`

- [ ] **Step 1: Write `readme.md`**

````markdown
# flute-cli-mcp

An MCP (Model Context Protocol) server that lets an AI agent drive Aurora's `flute` payments CLI.

## How it works

The server spawns `flute --profile <pinned> --output json …` once per tool call, parses the structured stdout, and surfaces both successes and the CLI's `{kind, message, status?, correlation_id?}` error envelope through MCP. Credentials live in the OS keychain (`flute auth login`) or in `FLUTE_CLIENT_ID`/`FLUTE_CLIENT_SECRET` in this server's environment; the server never handles them directly.

The active profile is **pinned at startup** — run one instance per environment (sandbox vs production). On a `production` instance, write tools are refused unless `FLUTE_MCP_ALLOW_PROD_WRITES=1`.

## Install

```bash
# macOS / Linux (curl + sh)
curl -LsSf https://github.com/getflute/flute-cli-mcp/releases/latest/download/flute-cli-mcp-installer.sh | sh

# macOS / Linux (Homebrew)
brew install getflute/flute-cli-mcp/flute-cli-mcp

# Windows (PowerShell)
irm https://github.com/getflute/flute-cli-mcp/releases/latest/download/flute-cli-mcp-installer.ps1 | iex
```

Or build from source: `cargo install --path .`

Prereq: install `flute` first (see [getflute/flute-cli](https://github.com/getflute/flute-cli)) and configure credentials (`flute auth login`, or env vars).

## Run

```bash
flute-cli-mcp        # talks JSON-RPC over stdio
```

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `FLUTE_PROFILE` | `sandbox` | `sandbox` or `production` (alias `prod`). Pinned at startup. |
| `FLUTE_BIN` | resolved on `PATH` | Override the `flute` binary location. |
| `FLUTE_MERCHANT_ID` | unset | Pinned ISV merchant id (token tools; per-call `merchant_id` overrides). |
| `FLUTE_MCP_TIMEOUT_SECS` | `30` | Per-call timeout for the child process. |
| `FLUTE_MCP_DEBUG` | unset | Route `flute` stderr into this server's tracing. |
| `FLUTE_MCP_ALLOW_PROD_WRITES` | unset | Lift the production write guard. |
| `RUST_LOG` | `info` | tracing filter. Logs go to *stderr* only. |

## Claude Desktop config

```jsonc
{
  "mcpServers": {
    "flute-sandbox": {
      "command": "flute-cli-mcp",
      "env": { "FLUTE_PROFILE": "sandbox" }
    },
    "flute-prod-readonly": {
      "command": "flute-cli-mcp",
      "env": { "FLUTE_PROFILE": "production" }
    }
  }
}
```

The `flute-prod-readonly` instance serves reads; production writes are refused unless you add `"FLUTE_MCP_ALLOW_PROD_WRITES": "1"`.

## Tools

47 tools across: `transactions` (get/list/inspect/sale/auth/capture/void/refund/settle/tip_adjust), `ach` (debit/credit/void/refund), `customers` (get/list/methods/create/update/delete/add_card/add_ach/remove_method), `terminals` (list/status), `devices` (list/get/ttp_jwt/register/ttp_activate), `pos` (get/list/create/cancel), `settlements` (list/get), `subscriptions` (get/list/payments/create/terminate), `tokens` (list/create/revoke), plus `ping`, `version`, `auth_status`.

Reads are always allowed. On a guarded production instance, every write (anything that creates/moves money or mutates a resource) returns a `kind:"client"` error without spawning the CLI.

Excluded by design: `auth login/logout/switch` (interactive/local-state), `update` (operator-only), `completion` (shell-only), `pos create --wait` (poll `pos_get` instead).

## Errors

Every tool returns either a success result or `isError: true` with a structured JSON content item whose `kind` is one of `api`, `transport`, `auth`, `decode`, `client`, `spawn`, `timeout`, `bad_output`. Branch on `kind` first; `transport` and `api` with status ∈ {500,502,503,504} are safe to retry with backoff; `auth` means configure credentials on the operator's machine.

## License

MIT.
````

- [ ] **Step 2: Commit**

```bash
git add readme.md
git commit -m "docs: add readme"
```

---

### Task 16: Distribution (cargo-dist) + final verification

**Files:**
- Create: `dist-workspace.toml`
- Create: `.github/workflows/release.yml` (generated)

- [ ] **Step 1: Write `dist-workspace.toml`**

```toml
[workspace]
members = ["cargo:."]

# Config for 'dist'
[dist]
cargo-dist-version = "0.31.0"
ci = "github"
installers = ["shell", "powershell", "homebrew"]
targets = ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"]
install-path = "CARGO_HOME"
hosting = "github"
install-updater = false
pr-run-mode = "skip"
allow-dirty = []
```

- [ ] **Step 2: Generate the release workflow**

Run: `cargo install cargo-dist --version 0.31.0` (if `dist` is not already installed), then `dist generate`
Expected: `.github/workflows/release.yml` is created. If cargo-dist is unavailable in this environment, copy `/Users/chad.lung/Rust-Projects/flute-webhooks-mcp/.github/workflows/release.yml` and replace every `flute-webhooks-mcp` with `flute-cli-mcp`.

- [ ] **Step 3: Full verification gate**

Run each and confirm:
```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```
Expected: fmt clean; clippy zero warnings; all tests pass (lib units + `runner_unit` + `tools_with_mock` + `e2e_stdio`); `target/release/flute-cli-mcp` produced.

- [ ] **Step 4: Manual smoke test (optional but recommended)**

```bash
printf '%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | FLUTE_PROFILE=sandbox ./target/release/flute-cli-mcp
```
Expected: an `initialize` result then a `tools/list` result enumerating 47 tools (requires `flute` on PATH).

- [ ] **Step 5: Commit**

```bash
git add dist-workspace.toml .github/workflows/release.yml
git commit -m "ci: add cargo-dist release pipeline"
```

---

## Self-review

**Spec coverage:** Architecture (Task 5 `server.rs`); config + env vars incl. `FLUTE_MERCHANT_ID`/`FLUTE_MCP_ALLOW_PROD_WRITES` (Task 3); `FluteError` mapping (Tasks 2, 5 `flute_err_to_result`); production write guard (Task 5 `guard_write`, tested Tasks 6/14); all 47 tools across groups (Tasks 5–12); `pos_create` no `--wait` (Task 10, asserted); `auth_status` via `auth status` (Task 5); merchant-id resolution (Task 12); three test layers — unit (Tasks 2/3/4), mock argv (Tasks 5–12), e2e (Task 14); distribution mirror (Task 16); readme with env vars + sandbox/prod snippets + tool list (Task 15). Excluded commands documented (readme + spec). All spec success criteria map to Task 16's verification gate plus the e2e tests.

**Placeholder scan:** No "TODO/TBD/handle errors appropriately". Every code step contains complete code. The only intentional `…`-style note is the prose instruction in Tasks 7–12 Step 2 ("Extend chain: `… + Self::X_router();`") which shows the exact call to append to the chain established in Task 6.

**Type consistency:** `FluteServer::new(Arc<Config>, Arc<dyn CliRunner>)` used identically in `main.rs` and all tests (`MockRunner::new` → `Arc<MockRunner>` coerces to `Arc<dyn CliRunner>`). Helper names (`base_args`, `run_cli`, `guard_write`, `merchant_id_for`, `value_to_result`, `flute_err_to_result`) match across modules. Router fns named `<group>_router` consistently and all appended to the chain in `FluteServer::new` (final chain after Task 12: `util + transactions + ach + customers + terminals + devices + pos + settlements + subscriptions + tokens`). Shared `Id`/`Empty` structs live in `tools/mod.rs`; per-group structs live in their module and are imported by tests with full paths. `Config` fields (`profile, binary, merchant_id, timeout, debug, allow_prod_writes`) match the test constructor in `tools_with_mock.rs`.

**Note for the executor:** flag spellings come from `flute-cli/agents.md`. If `cargo test` reveals a CLI flag mismatch (e.g. a renamed flag), confirm against `flute <group> --help` and fix the argv in the affected tool plus its mock test together.
