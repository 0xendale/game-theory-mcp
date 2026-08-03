//! Drives the built binary over real pipes.
//!
//! The unit tests call tools directly and never touch `main.rs`, so they
//! cannot catch a stray write to stdout -- which is the classic way a stdio
//! MCP server breaks. Nor can they catch a result that is well formed in
//! isolation but mangled by the router on its way out. This test speaks raw
//! JSON-RPC so it depends on nothing but the framing.

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

fn prisoners_dilemma() -> serde_json::Value {
    serde_json::json!({
        "form": "matrix",
        "players": ["Row", "Col"],
        "row_strategies": ["Cooperate", "Defect"],
        "col_strategies": ["Cooperate", "Defect"],
        "payoff_matrix": [[[3.0, 3.0], [0.0, 4.0]], [[4.0, 0.0], [1.0, 1.0]]],
        "payoff_kind": "cardinal"
    })
}

#[tokio::test]
async fn a_real_session_initializes_lists_tools_and_verifies_an_equilibrium() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gt-mcp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary should start");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();

    send(&mut stdin, initialize()).await;

    let line = stdout.next_line().await.unwrap().expect("initialize reply");
    let reply: serde_json::Value = serde_json::from_str(&line)
        .unwrap_or_else(|e| panic!("stdout must carry only JSON-RPC, got {line:?}: {e}"));
    assert_eq!(reply["id"], 1);
    assert!(reply["result"]["capabilities"]["tools"].is_object());
    // The handshake must name this server, not the SDK it is built on.
    assert_eq!(reply["result"]["serverInfo"]["name"], "gt-mcp");

    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    )
    .await;

    send(
        &mut stdin,
        serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    let line = stdout.next_line().await.unwrap().expect("tools/list reply");
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    let tools = reply["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"validate_game"), "got {names:?}");
    assert!(names.contains(&"verify_equilibrium"), "got {names:?}");

    // The advertised output schema must describe the payload. If a tool's
    // Output type leaked through instead, this would describe CallToolResult.
    let verify = tools
        .iter()
        .find(|t| t["name"] == "verify_equilibrium")
        .unwrap();
    let schema = serde_json::to_string(&verify["outputSchema"]).unwrap();
    assert!(
        schema.contains("holds"),
        "output schema looks wrong: {schema}"
    );
    assert!(
        !schema.contains("structuredContent"),
        "output schema describes CallToolResult rather than the payload: {schema}"
    );

    send(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {
                "name": "verify_equilibrium",
                "arguments": {
                    "game": prisoners_dilemma(),
                    "concept": "pure_nash",
                    "profile": [1, 1]
                }
            }
        }),
    )
    .await;
    let line = stdout.next_line().await.unwrap().expect("tools/call reply");
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    let result = &reply["result"];
    assert_eq!(result["isError"], serde_json::Value::Bool(false));

    let structured = &result["structuredContent"];
    // Guard against double wrapping: routing a CallToolResult through rmcp's
    // Json wrapper would nest a whole result inside structuredContent.
    assert!(
        structured["structuredContent"].is_null() && structured["isError"].is_null(),
        "structuredContent must be the payload, not a nested result: {structured}"
    );
    assert_eq!(structured["ok"], serde_json::Value::Bool(true));
    assert_eq!(structured["holds"], serde_json::Value::Bool(true));
    assert_eq!(structured["concept"], "pure_nash");

    drop(stdin);
    let out = child.wait_with_output().await.unwrap();

    // The startup log must have gone to stderr, not into the protocol stream.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("gt-mcp"),
        "expected a startup log on stderr, got {stderr:?}"
    );
}

#[tokio::test]
async fn a_malformed_game_comes_back_as_a_tool_result_not_a_protocol_error() {
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
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {
                "name": "validate_game",
                "arguments": {
                    "game": {
                        "form": "strategic",
                        "players": [{"id": 0, "name": "Row"}, {"id": 1, "name": "Col"}],
                        "strategies": [["C"], ["C"]],
                        "outcomes": [{"profile": [0, 0], "payoffs": [1.0]}],
                        "payoff_kind": "cardinal"
                    }
                }
            }
        }),
    )
    .await;
    let line = stdout.next_line().await.unwrap().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();

    assert!(
        reply["error"].is_null(),
        "must not be a JSON-RPC error: {reply}"
    );
    assert_eq!(reply["result"]["isError"], serde_json::Value::Bool(true));
    let structured = &reply["result"]["structuredContent"];
    assert_eq!(structured["ok"], serde_json::Value::Bool(false));
    assert_eq!(structured["code"], "invalid_game");
    assert!(!structured["suggestion"].as_str().unwrap().is_empty());
    // Validation reports every problem, not just the first.
    assert!(structured["diagnostics"].as_array().unwrap().len() >= 2);

    drop(stdin);
    let _ = child.wait_with_output().await;
}

/// The mixed-strategy path is where exactness is easiest to lose: a solver
/// that returned 0.5 instead of 1/2 would still look plausible.
#[tokio::test]
async fn a_mixed_equilibrium_comes_back_as_exact_fractions() {
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

    // Matching Pennies: the unique equilibrium mixes 1/2 on each side.
    send(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {
                "name": "verify_equilibrium",
                "arguments": {
                    "game": {
                        "form": "matrix",
                        "players": ["Row", "Col"],
                        "row_strategies": ["Heads", "Tails"],
                        "col_strategies": ["Heads", "Tails"],
                        "payoff_matrix": [
                            [[1.0, -1.0], [-1.0, 1.0]],
                            [[-1.0, 1.0], [1.0, -1.0]]
                        ],
                        "payoff_kind": "cardinal"
                    },
                    "concept": "mixed_nash",
                    "profile": [["1/2", "1/2"], ["1/2", "1/2"]]
                }
            }
        }),
    )
    .await;
    let line = stdout.next_line().await.unwrap().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    let structured = &reply["result"]["structuredContent"];
    assert_eq!(structured["holds"], serde_json::Value::Bool(true));

    let payoffs = structured["evidence"]["expected_payoffs"]
        .as_array()
        .unwrap();
    assert_eq!(payoffs.len(), 2);
    for p in payoffs {
        // Zero-sum game, so both expect exactly nothing.
        assert_eq!(p["exact"], "0");
        assert_eq!(p["approx"], 0.0);
    }

    drop(stdin);
    let _ = child.wait_with_output().await;
}

/// A decimal probability is a malformed request rather than a bad game, so it
/// is the one input here that becomes a JSON-RPC error.
#[tokio::test]
async fn a_decimal_probability_is_a_protocol_error() {
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
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {
                "name": "verify_equilibrium",
                "arguments": {
                    "game": prisoners_dilemma(),
                    "concept": "mixed_nash",
                    "profile": [["0.5", "0.5"], ["0.5", "0.5"]]
                }
            }
        }),
    )
    .await;
    let line = stdout.next_line().await.unwrap().unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert!(
        !reply["error"].is_null(),
        "a decimal probability should be a protocol error: {reply}"
    );
    assert_eq!(reply["error"]["code"], -32602);

    drop(stdin);
    let _ = child.wait_with_output().await;
}
