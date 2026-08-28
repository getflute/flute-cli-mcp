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
    format!(
        "{}\n",
        serde_json::to_string(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .unwrap()
    )
}

fn jsonrpc_notify(method: &str, params: Value) -> String {
    format!(
        "{}\n",
        serde_json::to_string(&json!({"jsonrpc":"2.0","method":method,"params":params})).unwrap()
    )
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
    stdin
        .write_all(jsonrpc_notify("notifications/initialized", json!({})).as_bytes())
        .unwrap();
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
    stdin
        .write_all(jsonrpc(2, "tools/list", json!({})).as_bytes())
        .unwrap();
    let listed = read_one_frame(&mut reader);
    let tools = listed["result"]["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 47, "expected 47 tools, got {}", tools.len());

    // Keep both parts of the card-AVS guidance in the exposed tool metadata:
    // recommend the fields AVS actually matches, and warn that `avsResponse`
    // is advisory so an agent does not retry an Approved charge and double-charge.
    for name in ["transactions_sale", "transactions_auth"] {
        let tool = tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing from tools/list"));
        let desc = tool["description"]
            .as_str()
            .unwrap_or_else(|| panic!("{name} description missing"));
        assert!(
            desc.contains("avsResponse") && desc.contains("never retry"),
            "{name} description must warn that avsResponse is advisory and must not be retried on"
        );
        assert!(
            desc.contains("billing_line1 + billing_postal_code")
                && !desc.contains("billing_city + billing_country_id"),
            "{name} description must recommend AVS-matched street + ZIP, not city + country"
        );

        let properties = &tool["inputSchema"]["properties"];
        let city_desc = properties["billing_city"]["description"]
            .as_str()
            .unwrap_or_else(|| panic!("{name} billing_city description missing"));
        let postal_desc = properties["billing_postal_code"]["description"]
            .as_str()
            .unwrap_or_else(|| panic!("{name} billing_postal_code description missing"));
        assert!(
            city_desc.contains("not matched by AVS"),
            "{name} billing_city must be documented as not AVS-matched"
        );
        assert!(
            postal_desc.contains("Manual") && postal_desc.contains("requires ZIP"),
            "{name} billing_postal_code must document the Manual+AVS requirement"
        );
        assert!(
            desc.contains("data.processedAmount")
                && desc.contains("data.transactionReceipt.responseDescription")
                && desc.contains("data.status"),
            "{name} description must document the POST transaction response shape"
        );
    }

    for name in ["transactions_get", "transactions_inspect"] {
        let desc = tools
            .iter()
            .find(|t| t["name"] == name)
            .and_then(|t| t["description"].as_str())
            .unwrap_or_else(|| panic!("{name} description missing"));
        assert!(
            desc.contains("data.amount.totalAmount")
                && desc.contains("data.authCode")
                && desc.contains("data.responseDescription"),
            "{name} description must document the GET/receipt response shape"
        );
    }

    // Mutation responses use the nested POST shape rather than the GET/receipt
    // shape, so agents must not look for amount or response text at data.*.
    for name in [
        "transactions_capture",
        "transactions_void",
        "transactions_refund",
        "transactions_tip_adjust",
    ] {
        let desc = tools
            .iter()
            .find(|t| t["name"] == name)
            .and_then(|t| t["description"].as_str())
            .unwrap_or_else(|| panic!("{name} description missing"));
        assert!(
            desc.contains("data.transactionReceipt.amount.totalAmount")
                && desc.contains("data.transactionReceipt.responseDescription")
                && desc.contains("data.status"),
            "{name} description must document the POST transaction response shape"
        );
    }

    // These operations surface a repeat error; describing them as idempotent
    // can make an agent retry instead of reconciling current state first.
    for name in [
        "transactions_void",
        "ach_void",
        "pos_cancel",
        "subscriptions_terminate",
    ] {
        let desc = tools
            .iter()
            .find(|t| t["name"] == name)
            .and_then(|t| t["description"].as_str())
            .unwrap_or_else(|| panic!("{name} description missing"));
        assert!(
            desc.contains("NOT idempotent") && !desc.contains("404 on repeat"),
            "{name} must not tell agents that a repeat is idempotent"
        );
    }

    // tools/call ping — fake returns success JSON.
    stdin
        .write_all(jsonrpc(3, "tools/call", json!({"name":"ping","arguments":{}})).as_bytes())
        .unwrap();
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

    stdin
        .write_all(
            jsonrpc(
                2,
                "tools/call",
                json!({"name":"transactions_list","arguments":{}}),
            )
            .as_bytes(),
        )
        .unwrap();
    let called = read_one_frame(&mut reader);
    assert_eq!(called["result"]["isError"], json!(true));
    let text = called["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
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

    stdin
        .write_all(
            jsonrpc(
                2,
                "tools/call",
                json!({"name":"transactions_sale","arguments":{"amount":"1.00"}}),
            )
            .as_bytes(),
        )
        .unwrap();
    let called = read_one_frame(&mut reader);
    assert_eq!(called["result"]["isError"], json!(true));
    let text = called["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    let payload: Value = serde_json::from_str(text).expect("parse error payload");
    assert_eq!(payload["kind"], "client");
    assert!(
        payload["message"]
            .as_str()
            .unwrap()
            .contains("FLUTE_MCP_ALLOW_PROD_WRITES")
    );

    drop(stdin);
    let _ = child.wait();
}
