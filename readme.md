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

Prereq: install `flute` **v1.1.0 or newer** (see [getflute/flute-cli](https://github.com/getflute/flute-cli)) and configure credentials (`flute auth login`, or env vars). The `keys_*` tools invoke `flute keys …`, which does not exist before v1.1.0. Note where both binaries land — you need their absolute paths to configure a client (see [Binary paths](#binary-paths)).

## Run

```bash
flute-cli-mcp        # talks JSON-RPC over stdio
```

Two flags mirror the env vars, for a client that can set arguments more easily than an environment: `--binary <path>` (same as `FLUTE_BIN`) and `--profile <sandbox|production>` (same as `FLUTE_PROFILE`). The flag wins over the env var.

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `FLUTE_PROFILE` | `sandbox` | `sandbox` or `production` (alias `prod`). Pinned at startup. |
| `FLUTE_BIN` | *(unset — falls back to a `PATH` lookup that usually fails under an MCP client; set it)* | Absolute path to the `flute` binary. See [Binary paths](#binary-paths). |
| `FLUTE_MERCHANT_ID` | unset | Pinned ISV merchant id (`keys_*` tools; per-call `merchant_id` overrides). |
| `FLUTE_MCP_TIMEOUT_SECS` | `30` | Per-call timeout for the child process. |
| `FLUTE_MCP_DEBUG` | off | Set to `1`/`true`/`yes`/`on` to route `flute` stderr into this server's tracing. |
| `FLUTE_MCP_ALLOW_PROD_WRITES` | off | Set to `1`/`true`/`yes`/`on` to lift the production write guard. Any other value (including `false`/`0`/empty) keeps it on. |
| `RUST_LOG` | `info` | tracing filter. Logs go to *stderr* only. |

## Binary paths

**Assume neither binary is on the client's `PATH`, and configure both by absolute path.**

Your MCP client spawns `flute-cli-mcp` directly as a child process — it does not run your login shell first. A client started from the macOS Dock, Windows Explorer, or an IDE inherits a minimal `PATH` (often just `/usr/bin:/bin:/usr/sbin:/sbin`) containing none of the directories your `.zshrc`/`.bashrc` or the installers add: `/usr/local/bin`, `/opt/homebrew/bin`, `~/.local/bin`. That `flute-cli-mcp` runs fine when *you* type it in a terminal proves nothing here — that `PATH` is your shell's, not the client's.

Two separate lookups depend on this, and each fails differently:

- **`command`** — the path the client uses to launch this server. If it can't be resolved, the server never starts and the client reports it as failed or disconnected, with nothing in this server's logs (there are none yet).
- **`FLUTE_BIN`** — where this server finds the `flute` CLI. Left unset, it falls back to a `PATH` lookup for `flute`; when that misses, the server prints ``could not find `flute` on PATH`` to stderr and exits 2 **before serving a single request**, so this too surfaces as a dead server rather than a tool error.

Get the real paths from your own shell:

```bash
# macOS / Linux
command -v flute
command -v flute-cli-mcp
```

```powershell
# Windows (PowerShell)
(Get-Command flute).Source
(Get-Command flute-cli-mcp).Source
```

If those come up empty, the binary isn't installed for this user — install it first (above) rather than guessing a path. For reference, the installers put `flute-cli-mcp` in:

| Installed via | Location |
|---|---|
| shell / PowerShell installer, `cargo install` | `~/.cargo/bin` (Windows: `%USERPROFILE%\.cargo\bin`) |
| Homebrew, Apple Silicon | `/opt/homebrew/bin` |
| Homebrew, Intel macOS / Linuxbrew | `/usr/local/bin`, `/home/linuxbrew/.linuxbrew/bin` |

`FLUTE_BIN` must name the executable itself, not the directory holding it, and the file must be executable. Otherwise the server exits 2 at startup with ``configuration error: FLUTE_BIN=`…` does not exist or is not executable`` — checked at launch on purpose, so a bad path shows up immediately instead of on the first tool call. (A binary that disappears *after* startup fails per-call with `kind:"spawn"` instead.)

Neither config format expands `~` or `$HOME` — write the path out in full. On Windows, escape the backslashes in JSON (`"C:\\Program Files\\flute\\flute.exe"`) or use a TOML literal string (`'C:\Program Files\flute\flute.exe'`), and include the `.exe`.

## Claude Desktop config

Replace both `/path/to/…` placeholders with the absolute paths you found above.

```jsonc
{
  "mcpServers": {
    "flute-sandbox": {
      "command": "/path/to/mcp/flute-cli-mcp",
      "env": {
        "FLUTE_PROFILE": "sandbox",
        "FLUTE_BIN": "/path/to/cli/flute"
      }
    },
    "flute-prod-readonly": {
      "command": "/path/to/mcp/flute-cli-mcp",
      "env": {
        "FLUTE_PROFILE": "production",
        "FLUTE_BIN": "/path/to/cli/flute"
      }
    }
  }
}
```

The `flute-prod-readonly` instance serves reads; production writes are refused unless you add `"FLUTE_MCP_ALLOW_PROD_WRITES": "1"`.

If a server shows as failed, check the client's MCP logs for this server's stderr — on macOS, `~/Library/Logs/Claude/mcp-server-flute-sandbox.log`. A `could not find flute on PATH` line there means `FLUTE_BIN` is unset or wrong; no log file at all usually means `command` itself didn't resolve.

## Codex app config

Codex stores MCP servers in `~/.codex/config.toml`. The Codex app, CLI, and IDE extension share this configuration — so even if you only ever launch `codex` from a shell that has both binaries on `PATH`, set the absolute paths anyway or the same config breaks under the app and the extension.

```toml
[mcp_servers.flute-sandbox]
command = "/path/to/mcp/flute-cli-mcp"
env = { FLUTE_PROFILE = "sandbox", FLUTE_BIN = "/path/to/cli/flute" }

[mcp_servers.flute-prod-readonly]
command = "/path/to/mcp/flute-cli-mcp"
env = { FLUTE_PROFILE = "production", FLUTE_BIN = "/path/to/cli/flute" }
```

The `flute-prod-readonly` instance serves reads; production writes are refused unless you add `FLUTE_MCP_ALLOW_PROD_WRITES = "1"` to its `env` table.

## Tools

47 tools across: `transactions` (get/list/inspect/sale/auth/capture/void/refund/settle/tip_adjust), `ach` (debit/credit/void/refund), `customers` (get/list/methods/create/update/delete/add_card/add_ach/remove_method), `terminals` (list/status), `devices` (list/get/ttp_jwt/register/ttp_activate), `pos` (get/list/create/cancel), `settlements` (list/get), `subscriptions` (get/list/payments/create/terminate), `keys` (list/create/revoke), plus `ping`, `version`, `auth_status`.

Reads are always allowed. On a guarded production instance, every write (anything that creates/moves money or mutates a resource) returns a `kind:"client"` error without spawning the CLI.

**Transaction reads and writes return different `data` shapes.** The MCP forwards the CLI's success envelope unchanged. `transactions_get` and `transactions_inspect` return the receipt-shaped response: amount is `data.amount.totalAmount`, with `data.authCode` and `data.responseDescription` at the top level. The POST tools return `data.status`, `data.details` and `data.transactionReceipt`; read their response text from `data.transactionReceipt.responseDescription` (falling back to `data.details.hostResponseMessage` or `data.details.message`) and their auth code from `data.transactionReceipt.authCode` or `data.details.authCode`. `transactions_sale` and `transactions_auth` additionally provide `data.processedAmount`; capture, void, refund and tip-adjust instead expose the amount only at `data.transactionReceipt.amount.totalAmount`. Only `data.transactionId` and `data.status` are common top-level fields across both shapes.

**The AVS result is advisory — do not branch on it as if it were the outcome.** `transactions_sale`, `transactions_auth`, `transactions_get` and `transactions_inspect` return the address check in `data.avsResponse` (`{responseCode, action, group, result, codeDescription, …}`), or `null` when AVS is off for the merchant/processor. An **Approved** transaction can still carry `result: "Failed"` when the merchant's AVS `action` is `"Allow"` — so a failed AVS result is not a failed charge, and retrying on it double-charges. Use `data.status` for the transaction outcome. Note also that `data.transactionReceipt.avsResponse` may be `null` even when `data.avsResponse` is populated; read the latter.

**Card AVS uses street + ZIP, not city + country.** `transactions_sale`, `transactions_auth`, `customers_create` and `customers_update` accept `billing_line1`, `billing_line2`, `billing_city`, `billing_state`, `billing_state_id`, `billing_postal_code` and `billing_country_id`. For AVS coverage, supply `billing_line1` + `billing_postal_code`; the gateway's pre-sale check falls back to `billing_line2` when line 1 is absent, although processor authorization carries line 1 only. City, state and country are stored but not AVS-matched. Billing fields are optional, but omitting the address can cause an AVS-sensitive processor to **decline** a card transaction. Under the default card data source 1 (Internet), a supplied ZIP must be at least 5 characters and state/country IDs are checked only when both are present. For card-present sources, a supplied ZIP must be at least 2 characters; source 7 (Manual) additionally requires ZIP when AVS is enabled. The address is sent only when at least one field is set. On `customers_update`, supplying any one of them replaces the stored address wholesale, so send the whole address rather than the single field you mean to change.

**Do not retry void, cancel or terminate blindly.** `transactions_void`, `ach_void`, `pos_cancel` and `subscriptions_terminate` are not idempotent: the CLI surfaces the server's repeat error rather than mapping it to success. Reconcile with the corresponding get/list tool before retrying. Customer delete/remove-method and key revoke are different—the CLI maps a repeat 404 to a successful no-op.

**Pagination is zero-based.** On the list tools (`transactions_list`, `customers_list`, `settlements_list`, `subscriptions_list`, `pos_list`), `page: 0` — or omitting `page` — returns the first page, `page: 1` the second, and so on; `limit` maps to the API page size. The MCP forwards `page` to the CLI/API verbatim (no offset). A nonzero `total` with an empty results list usually means the requested `page` is past the last page.

Excluded by design: `auth login/logout/switch` (interactive/local-state), `auth token` (**prints the raw bearer token** — exposing it as a tool would hand the credential to every connected agent; see Security), `update` (operator-only), `completion` (shell-only), `pos create --wait` (poll `pos_get` instead).

These exclusions are deliberate, not gaps. Most of them also emit no JSON envelope on success (they print plain text, a shell script, or a bare token), so wrapping them would return unparseable output as well.

## Errors

**Success and error payloads have different shapes by design — discriminate on the MCP `isError` flag, not on the body.**

- **Success** (`isError: false`): the CLI's envelope, `{ "object": …, "data": …, "meta": { "environment": … } }`.
- **Error** (`isError: true`): a flat `{ "kind": …, "message": …, "status"?: …, "correlation_id"?: … }`. This intentionally matches the CLI's documented error contract so you can branch on `kind`/`status` — it is **not** wrapped in `object`/`data`/`meta`.

So a deleted-then-fetched customer returns `isError: true` with `{kind:"api", status:404, …}` — that is the expected 404 shape, not a missing envelope. `kind` is one of `api`, `transport`, `auth`, `decode`, `client`, `spawn`, `timeout`, `bad_output`. Branch on `kind` first; `transport` and `api` with status ∈ {500,502,503,504} are safe to retry with backoff; `auth` means configure credentials on the operator's machine.

Startup failures never reach this layer: a bad or missing `flute` path makes the process exit 2 with a plain stderr line, so the client sees a server that won't start. See [Binary paths](#binary-paths).

## Security

This server is a thin wrapper around the `flute` CLI, which accepts sensitive values — card number, CVV, bank routing and account numbers — as **command-line arguments**. While a tool call runs, those arguments are visible to other processes on the same host via process inspection (`ps`, `/proc/<pid>/cmdline`). Run this server only on a trusted host, under a dedicated user, and avoid passing real card/bank data on shared or multi-tenant machines. This exposure is inherent to the CLI's interface; eliminating it requires an upstream `flute` change to accept secrets via stdin or environment rather than flags.

Other handling:

- Credentials (`FLUTE_CLIENT_ID`/`FLUTE_CLIENT_SECRET`) are never read or logged by this server — they are simply inherited by the spawned `flute` process.
- The CLI's `auth token` subcommand prints the current bearer token to stdout. It is **deliberately not exposed as a tool**: any agent that could call it would obtain a credential good for every other API call, bypassing this server's production write guard entirely. Do not add it.
- `auth_status` returns only `{authenticated, profile}` plus `api_base_url`/`client_id`/`merchant_id` when the CLI reports them, never the secret. It is a **live** check — the CLI pings the API, so `authenticated` is true only when the stored credentials actually round-trip, and the call costs a network round-trip.
- `keys_create` surfaces a one-shot `clientSecret` from the API response; capture and store it securely (the API never returns it again).
- On a non-JSON CLI failure, the raw output embedded in a `bad_output` error is truncated to 4 KiB so a large or sensitive body can't be echoed back wholesale.

## License

MIT.
