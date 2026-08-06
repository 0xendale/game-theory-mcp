//! Drives the built binary over real pipes for the resource and prompt
//! surface.
//!
//! The unit tests call `resources::read` and `prompts::get` directly, so they
//! cannot catch a handler method that was written but never reached -- an
//! unadvertised capability, a method the SDK routes elsewhere, or a result
//! shape the framing mangles. This test speaks raw JSON-RPC and depends on
//! nothing but the wire format.

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const PROTOCOL_VERSION: &str = "2025-06-18";

async fn send(stdin: &mut tokio::process::ChildStdin, v: serde_json::Value) {
    let line = format!("{v}\n");
    stdin.write_all(line.as_bytes()).await.unwrap();
    stdin.flush().await.unwrap();
}

fn initialize() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "gt-mcp-test", "version": "0.0.0"}
        }
    })
}

#[tokio::test]
async fn a_real_session_lists_and_reads_concepts_and_prompts() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gt-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("binary should start");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();

    send(&mut stdin, initialize()).await;
    let line = stdout.next_line().await.unwrap().expect("initialize reply");
    let reply: serde_json::Value = serde_json::from_str(&line)
        .unwrap_or_else(|e| panic!("stdout must carry only JSON-RPC, got {line:?}: {e}"));
    let caps = &reply["result"]["capabilities"];
    // All three surfaces, or a host will never ask for two of them.
    assert!(caps["tools"].is_object(), "{caps}");
    assert!(caps["resources"].is_object(), "{caps}");
    assert!(caps["prompts"].is_object(), "{caps}");

    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )
    .await;

    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "resources/list"}),
    )
    .await;
    let line = stdout.next_line().await.unwrap().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    let resources = reply["result"]["resources"].as_array().unwrap();
    let mut uris: Vec<&str> = resources
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    uris.sort_unstable();
    assert_eq!(
        uris,
        vec![
            "gt://concepts/archetypes",
            "gt://concepts/dominance",
            "gt://concepts/mixed-strategies",
            "gt://concepts/nash",
            "gt://concepts/repeated-games",
            "gt://concepts/subgame-perfect",
        ]
    );

    send(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "resources/read",
            "params": {"uri": "gt://concepts/nash"}
        }),
    )
    .await;
    let line = stdout.next_line().await.unwrap().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    let contents = reply["result"]["contents"].as_array().unwrap();
    assert_eq!(contents.len(), 1);
    assert_eq!(contents[0]["uri"], "gt://concepts/nash");
    let text = contents[0]["text"].as_str().unwrap();
    assert!(text.contains("Nash equilibrium"), "{text}");
    // The whole point of these resources: the anchor is inspectable.
    assert!(text.contains("Bonanno"), "{text}");
    assert!(text.contains("p. 32"), "{text}");
    // And the concept routes the host to a call it can actually make.
    assert!(text.contains("solve_pure_nash"), "{text}");

    // The repeated-games anchor is a different book, on purpose.
    send(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 4, "method": "resources/read",
            "params": {"uri": "gt://concepts/repeated-games"}
        }),
    )
    .await;
    let line = stdout.next_line().await.unwrap().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    let text = reply["result"]["contents"][0]["text"].as_str().unwrap();
    assert!(text.contains("Osborne"), "{text}");
    assert!(text.contains("Rubinstein"), "{text}");

    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 5, "method": "prompts/list"}),
    )
    .await;
    let line = stdout.next_line().await.unwrap().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    let prompts = reply["result"]["prompts"].as_array().unwrap();
    let mut names: Vec<&str> = prompts
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "analyze_competitive_dynamic",
            "design_incentive_scheme",
            "formalize_scenario",
        ]
    );
    let formalize = prompts
        .iter()
        .find(|p| p["name"] == "formalize_scenario")
        .unwrap();
    let declared = formalize["arguments"].as_array().unwrap();
    assert!(
        declared
            .iter()
            .any(|a| a["name"] == "scenario" && a["required"] == serde_json::Value::Bool(true)),
        "{formalize}"
    );

    send(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 6, "method": "prompts/get",
            "params": {
                "name": "formalize_scenario",
                "arguments": {"scenario": "Two ferry operators set schedules on one crossing."}
            }
        }),
    )
    .await;
    let line = stdout.next_line().await.unwrap().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    let messages = reply["result"]["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["role"], "user");
    let text = messages[0]["content"]["text"].as_str().unwrap();
    assert!(
        text.contains("Two ferry operators set schedules on one crossing."),
        "the scenario argument was not interpolated: {text}"
    );
    assert!(text.contains("payoff_kind"), "{text}");
    assert!(text.contains("validate_game"), "{text}");

    drop(stdin);
    let _ = child.wait_with_output().await;
}

/// A typo must not look like a concept with nothing to say.
#[tokio::test]
async fn an_unknown_uri_or_prompt_name_is_a_protocol_error() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gt-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();

    send(&mut stdin, initialize()).await;
    let _ = stdout.next_line().await.unwrap();
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )
    .await;

    send(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "resources/read",
            "params": {"uri": "gt://concepts/nash-equilibrium"}
        }),
    )
    .await;
    let line = stdout.next_line().await.unwrap().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert!(
        reply["result"].is_null(),
        "an unknown URI must not return a body: {reply}"
    );
    assert!(!reply["error"].is_null(), "{reply}");
    // The error names what does exist, so the typo is self-correcting.
    let available = reply["error"]["data"]["available"].as_array().unwrap();
    assert_eq!(available.len(), 6);

    send(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "prompts/get",
            "params": {"name": "formalise_scenario", "arguments": {"scenario": "x"}}
        }),
    )
    .await;
    let line = stdout.next_line().await.unwrap().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert!(reply["result"].is_null(), "{reply}");
    assert!(!reply["error"].is_null(), "{reply}");
    assert_eq!(
        reply["error"]["data"]["available"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    drop(stdin);
    let _ = child.wait_with_output().await;
}
