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
| `FLUTE_MCP_DEBUG` | off | Set to `1`/`true`/`yes`/`on` to route `flute` stderr into this server's tracing. |
| `FLUTE_MCP_ALLOW_PROD_WRITES` | off | Set to `1`/`true`/`yes`/`on` to lift the production write guard. Any other value (including `false`/`0`/empty) keeps it on. |
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

**Pagination is zero-based.** On the list tools (`transactions_list`, `customers_list`, `settlements_list`, `subscriptions_list`, `pos_list`), `page: 0` — or omitting `page` — returns the first page, `page: 1` the second, and so on; `limit` maps to the API page size. The MCP forwards `page` to the CLI/API verbatim (no offset). A nonzero `total` with an empty results list usually means the requested `page` is past the last page.

Excluded by design: `auth login/logout/switch` (interactive/local-state), `update` (operator-only), `completion` (shell-only), `pos create --wait` (poll `pos_get` instead).

## Errors

**Success and error payloads have different shapes by design — discriminate on the MCP `isError` flag, not on the body.**

- **Success** (`isError: false`): the CLI's envelope, `{ "object": …, "data": …, "meta": { "environment": … } }`.
- **Error** (`isError: true`): a flat `{ "kind": …, "message": …, "status"?: …, "correlation_id"?: … }`. This intentionally matches the CLI's documented error contract so you can branch on `kind`/`status` — it is **not** wrapped in `object`/`data`/`meta`.

So a deleted-then-fetched customer returns `isError: true` with `{kind:"api", status:404, …}` — that is the expected 404 shape, not a missing envelope. `kind` is one of `api`, `transport`, `auth`, `decode`, `client`, `spawn`, `timeout`, `bad_output`. Branch on `kind` first; `transport` and `api` with status ∈ {500,502,503,504} are safe to retry with backoff; `auth` means configure credentials on the operator's machine.

## Security

This server is a thin wrapper around the `flute` CLI, which accepts sensitive values — card number, CVV, bank routing and account numbers — as **command-line arguments**. While a tool call runs, those arguments are visible to other processes on the same host via process inspection (`ps`, `/proc/<pid>/cmdline`). Run this server only on a trusted host, under a dedicated user, and avoid passing real card/bank data on shared or multi-tenant machines. This exposure is inherent to the CLI's interface; eliminating it requires an upstream `flute` change to accept secrets via stdin or environment rather than flags.

Other handling:

- Credentials (`FLUTE_CLIENT_ID`/`FLUTE_CLIENT_SECRET`) are never read or logged by this server — they are simply inherited by the spawned `flute` process.
- `auth_status` returns only `{authenticated, profile}`, never the token.
- `tokens_create` surfaces a one-shot `clientSecret` from the API response; capture and store it securely (the API never returns it again).
- On a non-JSON CLI failure, the raw output embedded in a `bad_output` error is truncated to 4 KiB so a large or sensitive body can't be echoed back wholesale.

## License

MIT.
