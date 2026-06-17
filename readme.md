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
