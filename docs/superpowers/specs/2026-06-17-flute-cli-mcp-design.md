# flute-cli-mcp — design

**Date:** 2026-06-17
**Status:** approved (brainstorm)
**Author:** chad.lung@risewithaurora.com (with Claude)

## Goal

Build a Model Context Protocol (MCP) server in Rust that lets an AI agent
operate Aurora's Flute payments platform by driving the existing `flute`
CLI ([getflute/flute-cli]). The CLI already exposes a machine-readable
contract (`--output json`, a structured success envelope, and a
`{kind,message,status?,correlation_id?}` error envelope) documented in its
[`agents.md`]; this project is the JSON-RPC façade in front of it.

It follows the established pattern of the sibling [getflute/flute-webhooks-mcp]
project: thin transport, profile pinned at startup, `CliRunner` abstraction,
typed `FluteError` envelope, and cargo-dist distribution.

[getflute/flute-cli]: https://github.com/getflute/flute-cli
[`agents.md`]: https://github.com/getflute/flute-cli/blob/main/agents.md
[getflute/flute-webhooks-mcp]: https://github.com/getflute/flute-webhooks-mcp

## Non-goals

- Re-implementing Flute's REST API in Rust. The upstream `flute` CLI owns
  HTTP, auth, retries, decimal handling, and DTOs; we shell out to it.
- Managing credentials. The operator runs `flute auth login` (keychain) or
  sets `FLUTE_CLIENT_ID`/`FLUTE_CLIENT_SECRET` in the server's environment;
  the MCP server never touches the keychain directly and surfaces
  `kind:"auth"` errors as structured tool errors.
- Exposing the interactive `auth login`/`logout`/`switch`, the `update`
  self-updater, or `completion`. Those are interactive, mutate local state,
  or are operator-only.
- Long-polling. `pos create --wait` is excluded; `pos_create` returns the
  in-progress transaction and the agent polls `pos_get`. This keeps every
  tool call inside the per-call timeout.
- Supporting transports other than stdio.
- Mixing sandbox and production in one process.

## Key decisions (from brainstorm)

| Decision | Choice |
|---|---|
| Tool scope | **Full surface** — every non-interactive command group, including money-moving operations. Excludes `auth login/logout/switch`, `update`, `completion`. |
| Production safety | **Block production writes by default.** On a `production` instance, every *write* tool returns a structured error without spawning the CLI unless `FLUTE_MCP_ALLOW_PROD_WRITES` is set. Reads always run. |
| Guard boundary | Read vs write — every state-mutating tool is blocked on guarded production, not only money-moving ones. Simplest, safest boundary. |
| Transport | stdio only. |
| Credentials | Assume operator configured creds (keychain via `flute auth login`, or inherited `FLUTE_CLIENT_ID`/`FLUTE_CLIENT_SECRET` env). Runner strips only `RUST_LOG`, so creds pass through. |
| Profile | Pinned at server start via `FLUTE_PROFILE` (default `sandbox`). One instance per environment. |
| Merchant context | `FLUTE_MERCHANT_ID` pins the ISV merchant; token tools accept a per-call `merchant_id` override. Only token tools pass `--merchant-id`. |
| CLI invocation | One `flute` child process per tool call via `tokio::process::Command`. |
| Tool layout | One module per command group under `src/tools/`, each its own `#[tool_router(router = …)]`, combined in `server.rs`. |

## Architecture

```
flute-cli-mcp  (single Rust binary, stdio MCP)

  ┌──────────┐    ┌──────────┐    ┌─────────────────────┐
  │  rmcp    │───►│  Tools   │───►│  CliRunner          │
  │  stdio   │    │  layer   │    │  (tokio::process)   │
  │  server  │◄───│ (+guard) │◄───│                     │
  └──────────┘    └──────────┘    └─────────┬───────────┘
                       │                    │
                       ▼                    ▼
                 ┌──────────┐         spawns
                 │  Config  │         `flute
                 │  (start- │          --profile X
                 │   time)  │          --output json …`
                 └──────────┘
```

- **`main.rs`** wires tracing → stderr, loads `Config` from env (with CLI
  flag overrides for `--profile`/`--binary`), builds `ProcessRunner`, hands
  to the rmcp stdio server, runs until stdin closes. Exits `2` on a config
  error (missing binary, bad profile).
- **`config.rs`** holds `Config { profile, binary, merchant_id, timeout, debug, allow_prod_writes }` built from a `getenv` closure (so tests inject env).
- **`error.rs`** — `FluteError` mirroring the upstream envelope
  (`Api`/`Transport`/`Auth`/`Decode`/`Client`) plus our own `Spawn`,
  `Timeout`, `BadOutput`. Verbatim from the reference, with `flute` wording.
- **`runner.rs`** — `trait CliRunner { async fn run(&self, args: &[String]) -> Result<Value, FluteError>; }`, a `ProcessRunner`, and a `MockRunner` test double. Verbatim from the reference.
- **`server.rs`** — `FluteServer { config: Arc<Config>, runner: Arc<dyn CliRunner>, tool_router: ToolRouter<FluteServer> }`. Holds `base_args()` (`--profile X --output json`), `run_cli()`, the write-guard helper, the success/error→`CallToolResult` converters, and the `#[tool_handler] ServerHandler` impl. `new()` builds `tool_router` by summing every group router.
- **`tools/`** — one module per command group. Each defines its input
  param structs (`#[serde(deny_unknown_fields)]` + `schemars::JsonSchema`)
  and a `#[tool_router(router = <group>_router)] impl FluteServer { … }`
  block. Each tool builds an argv, runs the write-guard (for writes), calls
  the runner, and returns a `Value` or `FluteError`.

`rmcp` 1.7 supports this split: `ToolRouter` implements `Add`/`merge`, so
`server.rs` combines `Self::transactions_router() + Self::ach_router() + …`
into the single router stored on the struct and consumed by `#[tool_handler]`.

The whole binary stays modest; every source file stays well under ~500 LoC.

## Config / environment variables

| Variable | Default | Purpose |
|---|---|---|
| `FLUTE_PROFILE` | `sandbox` | `sandbox` \| `production`/`prod`. Pinned at startup. Passed to `flute` as `--profile`. |
| `FLUTE_BIN` | `which("flute")` | Override the `flute` binary location. Hard error at startup if neither resolves. |
| `FLUTE_MERCHANT_ID` | unset | Pinned ISV merchant id; used by token tools (per-call `merchant_id` overrides). |
| `FLUTE_MCP_TIMEOUT_SECS` | `30` | Per-call child timeout. Positive integer. |
| `FLUTE_MCP_DEBUG` | unset | When set to any non-empty value, route `flute` stderr into this server's tracing layer. |
| `FLUTE_MCP_ALLOW_PROD_WRITES` | unset | When set to any non-empty value, lift the production write guard. |
| `RUST_LOG` | `info` | Standard tracing filter. Logs go to **stderr** only. |

Credentials (`FLUTE_CLIENT_ID`/`FLUTE_CLIENT_SECRET`) are not read by this
server; if present in its environment they are inherited by the spawned
`flute` child (the runner strips only `RUST_LOG`).

## Production write guard

When `config.profile == Production` and `allow_prod_writes == false`, every
tool classified as a **write** short-circuits before spawning the CLI and
returns:

```json
{ "kind": "client",
  "message": "refusing <tool> on production profile; set FLUTE_MCP_ALLOW_PROD_WRITES=1 to enable writes" }
```

surfaced as `isError: true`. Pure **read** tools always run. The
classification is a `const`/match in `server.rs` (or an enum the tool passes
to the guard helper), tested explicitly. Sandbox instances never block.

## Tool inventory

47 tools. Output on success is the upstream CLI's stdout JSON envelope,
unchanged. Output on failure is a structured error (see Error handling).
Idempotency / "NOT idempotent" hints from `agents.md` live in each tool's
`description`. **R** = read (always allowed); **W** = write (blocked on
guarded production).

### transactions — `flute transactions …`

| Tool | R/W | Wraps |
|---|---|---|
| `transactions_get` | R | `transactions get <id>` |
| `transactions_list` | R | `transactions list [--limit --page --unsettled --status --from --to]` |
| `transactions_inspect` | R | `transactions inspect <id>` |
| `transactions_sale` | W | `transactions sale --amount … [--card --exp --cvv --tip-amount --customer-id --payment-method-id --currency-id --card-data-source --reference-id …]` |
| `transactions_auth` | W | `transactions auth …` (same flags as sale) |
| `transactions_capture` | W | `transactions capture --transaction-id <id> [--amount]` |
| `transactions_void` | W | `transactions void --transaction-id <id>` |
| `transactions_refund` | W | `transactions refund --transaction-id <id> [--amount]` |
| `transactions_settle` | W | `transactions settle --payment-processor-id <id>` (batch-level) |
| `transactions_tip_adjust` | W | `transactions tip-adjust --transaction-id <id> --tip-amount <amt>` |

### ach — `flute ach …`

| Tool | R/W | Wraps |
|---|---|---|
| `ach_debit` | W | `ach debit --amount --payment-processor-id --routing --account --account-type --account-holder-type --billing-* --contact-* --sec-code --requester-ip` |
| `ach_credit` | W | `ach credit …` (same required fields as debit) |
| `ach_void` | W | `ach void <id>` |
| `ach_refund` | W | `ach refund <id>` |

### customers — `flute customers …`

| Tool | R/W | Wraps |
|---|---|---|
| `customers_get` | R | `customers get <id>` |
| `customers_list` | R | `customers list [--limit --page --search]` |
| `customers_methods` | R | `customers methods <id>` |
| `customers_create` | W | `customers create [--first-name --last-name --email --company --mobile]` |
| `customers_update` | W | `customers update <id> …` (GET-merge-PUT; omitted flags retained) |
| `customers_delete` | W | `customers delete <id> --yes` (404 idempotent) |
| `customers_add_card` | W | `customers add-card <id> --card --exp --cvv [--name]` |
| `customers_add_ach` | W | `customers add-ach <id> --routing --account --account-type --account-holder-type [--tax-id --name]` |
| `customers_remove_method` | W | `customers remove-method <id> <method-id> --yes` (404 idempotent) |

### terminals — `flute terminals …`

| Tool | R/W | Wraps |
|---|---|---|
| `terminals_list` | R | `terminals list` |
| `terminals_status` | R | `terminals status <id>` |

### devices — `flute devices …`

| Tool | R/W | Wraps |
|---|---|---|
| `devices_list` | R | `devices list` |
| `devices_get` | R | `devices get <id>` |
| `devices_ttp_jwt` | R | `devices ttp-jwt --device-id <id>` (returns a `tap_to_pay_jwt` envelope; idempotent read) |
| `devices_register` | W | `devices register <id> [--name]` |
| `devices_ttp_activate` | W | `devices ttp-activate <id>` |

### pos — `flute pos …`

| Tool | R/W | Wraps |
|---|---|---|
| `pos_get` | R | `pos get <id>` |
| `pos_list` | R | `pos list [--terminal-id --limit --page]` |
| `pos_create` | W | `pos create --terminal-id --amount --pos-device-id --reference-id [--currency-id --transaction-type --tip-amount --tip-rate --customer-id …]` — **no `--wait`**; poll `pos_get` |
| `pos_cancel` | W | `pos cancel <id>` |

### settlements — `flute settlements …`

| Tool | R/W | Wraps |
|---|---|---|
| `settlements_list` | R | `settlements list [--limit --page --from --to --status]` |
| `settlements_get` | R | `settlements get <id>` (client-side filter over the fetched page) |

### subscriptions — `flute subscriptions …`

| Tool | R/W | Wraps |
|---|---|---|
| `subscriptions_get` | R | `subscriptions get <id>` |
| `subscriptions_list` | R | `subscriptions list [--limit --page --search --customer-id]` |
| `subscriptions_payments` | R | `subscriptions payments <id>` |
| `subscriptions_create` | W | `subscriptions create --customer-id --payment-method-id --amount --number-of-payments [--interval --payment-frequency --currency-id --transaction-type --requester-ip …]` |
| `subscriptions_terminate` | W | `subscriptions terminate <id> --yes` |

### tokens — `flute tokens …`

| Tool | R/W | Wraps |
|---|---|---|
| `tokens_list` | R | `tokens list [--merchant-id]` |
| `tokens_create` | W | `tokens create --merchant-id --name` — **one-shot `clientSecret`** in the response; description warns to store it |
| `tokens_revoke` | W | `tokens revoke --client-id --merchant-id --yes` (404 idempotent) |

### utility — `flute …`

| Tool | R/W | Wraps |
|---|---|---|
| `ping` | R | `ping` |
| `version` | R | `version` |
| `auth_status` | R | probes credential presence via `flute auth status` (`--output json`); returns `{ authenticated, profile }`, never the JWT |

### Excluded by design

- `auth login` / `logout` / `switch` — interactive or mutate local credential state
- `update` — operator-only self-updater
- `completion` — shell-only output
- `pos create --wait` — long-running long-poll (agent polls `pos_get` instead)

## Data flow (single tool call)

```
agent → MCP rmcp server
  receives tools/call { name: "transactions_list", arguments: { limit: 25 } }
    │
    ▼
tool handler:
  - if WRITE and guarded production → return guard error (no spawn)
  - else build argv:
      ["--profile","sandbox","--output","json","transactions","list","--limit","25"]
    │
    ▼
CliRunner::run(argv)
  - tokio::process::Command::new(config.binary)
      .args(argv).env_remove("RUST_LOG")
      .stdin(null).stdout(piped).stderr(piped).kill_on_drop(true)
  - tokio::time::timeout(config.timeout, child.wait_with_output())
  - exit 0 → parse stdout as Value (empty stdout → Value::Null) → Ok(value)
    exit ≠ 0 → parse envelope → Err(FluteError::from_envelope_stdout(…))
    parse failure → Err(BadOutput { exit_code, stdout, stderr })
    │
    ▼
tool handler → CallToolResult { content: [Json(value)] }   (or isError on FluteError)
```

## Error handling

`FluteError` → MCP mapping (identical to the reference; `flute` wording):

| Variant | Source | MCP surfacing |
|---|---|---|
| `Api { status, message, correlation_id }` | `kind:"api"` envelope | `isError`, content `{kind,status,message,correlation_id}` |
| `Transport { message }` | `kind:"transport"` | `isError`; retry-safe |
| `Auth { message }` | `kind:"auth"` | `isError`; message appends "run `flute auth login`" |
| `Decode { message }` | `kind:"decode"` | `isError` |
| `Client { message }` | `kind:"client"` | `isError` (bad argv, or the prod-write guard) |
| `Spawn(msg)` | OS-level spawn failure | `isError`; "set FLUTE_BIN or install the CLI" |
| `Timeout { secs }` | tokio timeout fires | `isError`; child killed via `kill_on_drop` |
| `BadOutput { exit_code, stdout, stderr }` | nonzero exit, unparseable stdout | `isError`; stderr truncated to ~4 KB on a char boundary |

All variants pass structured fields through so an agent branches on
`kind`/`status` per `agents.md` — never flattened to a string. stderr is
captured but never returned on success; `FLUTE_MCP_DEBUG` routes it to
tracing.

## Concurrency

Each MCP tool call is independent. The only shared state is `Arc<Config>`
(frozen at startup) and `Arc<dyn CliRunner>` (`Send + Sync`). Multiple calls
in flight spawn multiple `flute` processes; each does its own OAuth
handshake internally. No locks on the hot path.

## Crate choices

Mirrors the reference. `rmcp` 1.7 (`server, transport-io, macros, schemars`),
`tokio` (`macros, rt-multi-thread, process, time, signal, io-util`),
`serde`/`serde_json`, `schemars` 1, `thiserror` 2, `tracing` +
`tracing-subscriber` (env-filter), `which` 8, `clap` 4 (`derive, env`),
`async-trait`. Dev: `tokio` (`test-util`), `pretty_assertions`, `tempfile`,
`assert_cmd`, `serde_json`. Deliberately **not** pulled in: `reqwest`,
`keyring`, `rust_decimal`, `anyhow` — the CLI owns HTTP, credentials,
decimals, and we keep typed errors.

## Project layout

```
flute-cli-mcp/
├── Cargo.toml                  package meta, [[bin]] flute-cli-mcp, [lib] flute_cli_mcp, [profile.dist]
├── dist-workspace.toml         cargo-dist config (mirrors reference)
├── readme.md                   install, run, env vars, Claude Desktop snippet, tool inventory, errors
├── .github/workflows/release.yml   generated by cargo-dist
├── src/
│   ├── main.rs                 clap Args, tracing, Config, serve over stdio
│   ├── lib.rs                  pub mod config, error, runner, server, tools
│   ├── config.rs               Config + Profile + ConfigError + env loading
│   ├── error.rs                FluteError + envelope deserializer
│   ├── runner.rs               CliRunner + ProcessRunner + MockRunner
│   ├── server.rs               FluteServer, base_args, run_cli, write guard, ServerHandler, router assembly
│   └── tools/
│       ├── mod.rs              module index + shared helpers (value_to_result, flute_err_to_result, guard)
│       ├── transactions.rs
│       ├── ach.rs
│       ├── customers.rs
│       ├── terminals.rs
│       ├── devices.rs
│       ├── pos.rs
│       ├── settlements.rs
│       ├── subscriptions.rs
│       ├── tokens.rs
│       └── util.rs             ping, version, auth_status
└── tests/
    ├── runner_unit.rs          ProcessRunner against fake binaries
    ├── tools_with_mock.rs      every tool: argv shape, error pass-through, prod-write guard
    └── e2e_stdio.rs            spawn binary + fake flute, real MCP frames
```

## Testing strategy

Three layers, mirroring the reference:

1. **Unit — `ProcessRunner` against a fake binary** (`runner_unit.rs`).
   A POSIX shell script in a `TempDir` echoes a canned payload and exits
   0/1. Cases: success body, `kind:"api"` envelope with `status` +
   `correlation_id`, malformed stdout (→ `BadOutput`), hung child (→
   `Timeout` with a short test timeout), missing binary (→ `Spawn`), empty
   stdout on success (→ `Value::Null`). Plus `config.rs` and `error.rs`
   `#[cfg(test)]` units (profile/timeout/binary/guard parsing; envelope
   parsing).

2. **Tool — every tool against `MockRunner`** (`tools_with_mock.rs`).
   For each tool: (a) happy path — assert the recorded argv matches
   `agents.md` verbatim, (b) error pass-through — an `Api` envelope surfaces
   intact with `isError`, (c) for representative writes — on a `production`
   guarded server the tool returns the guard error **and the runner is never
   called** (assert `mock.calls()` is empty), while a sandbox server runs
   normally and `FLUTE_MCP_ALLOW_PROD_WRITES` lifts the guard on production.

3. **End-to-end — real stdio MCP frames** (`e2e_stdio.rs`).
   `assert_cmd` launches the compiled binary with `FLUTE_BIN` pointing at a
   fake `flute` script. Hand-written JSON-RPC handshake (`initialize`,
   `tools/list`, `tools/call`) on stdin; parse responses on stdout. Golden
   tests: `tools/list` returns all 47 tools; a read tool returns JSON; an
   auth-error case surfaces `kind:"auth"`; a production instance rejects a
   write with the guard error.

**Out of scope for tests:** network mocks (the CLI's job); live-Flute
integration (real creds, flaky in CI).

## Distribution

Mirrors `flute-webhooks-mcp` exactly:

- **`dist-workspace.toml`** — cargo-dist `0.31.0`, `ci = "github"`,
  `installers = ["shell","powershell","homebrew"]`, `targets =
  ["aarch64-apple-darwin","x86_64-unknown-linux-gnu","x86_64-pc-windows-msvc"]`,
  `install-path = "CARGO_HOME"`, `hosting = "github"`,
  `install-updater = false`, `pr-run-mode = "skip"`.
- **`.github/workflows/release.yml`** — generated by `dist generate`;
  triggers on version tags.
- **`Cargo.toml`** — `repository`/`homepage` = `getflute/flute-cli-mcp`,
  MIT, `[profile.dist] inherits = "release", lto = "thin"`.
- Install paths mirror the reference's readme (curl/sh, Homebrew tap on the
  same repo, PowerShell irm). Operators update by reinstalling.

## Success criteria

1. `cargo build --release` produces a single `flute-cli-mcp` binary.
2. `cargo test` green across all three layers; `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
3. With `flute` installed and creds configured for `sandbox`, an MCP client
   (e.g. Claude Desktop) configured with `{ command: "flute-cli-mcp" }`
   exposes all 47 tools and `transactions_list` returns real JSON.
4. With **no** credentials, a read tool returns `isError: true` whose
   content includes `kind:"auth"` and a message to run `flute auth login`.
   The server never panics.
5. A `production` instance (no override) rejects every write tool with the
   guard error and never spawns the CLI for those calls; reads still work.
   Setting `FLUTE_MCP_ALLOW_PROD_WRITES=1` lifts the guard.
6. `readme.md` documents the env vars, all 47 tool names, the production
   write guard, and sandbox + production Claude Desktop config snippets.
