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
            "clientInfo": {"name": "game-theory-mcp-test", "version": "0.0.0"}
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

/// A live server, already through the handshake.
///
/// The tests above drive the pipes by hand, which is what makes them readable
/// as a transcript of a whole session. The per-tool tests below repeat only
/// the handshake and one call each, so they share it from here rather than
/// nine copies of the same twenty lines. The plumbing is identical: same
/// binary, same real pipes, same raw JSON-RPC.
struct Session {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    next_id: i64,
}

impl Session {
    async fn start() -> Session {
        let mut child = Command::new(env!("CARGO_BIN_EXE_game-theory-mcp"))
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
        assert_eq!(reply["id"], 1);

        send(
            &mut stdin,
            serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        )
        .await;

        Session {
            child,
            stdin,
            stdout,
            next_id: 2,
        }
    }

    /// Send one request and return the whole reply, so a test can assert on
    /// `error` as readily as on `result`.
    async fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let id = self.next_id;
        self.next_id += 1;
        send(
            &mut self.stdin,
            serde_json::json!({
                "jsonrpc": "2.0", "id": id, "method": method, "params": params
            }),
        )
        .await;
        let line = self
            .stdout
            .next_line()
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("no reply to {method}"));
        let reply: serde_json::Value = serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("stdout must carry only JSON-RPC, got {line:?}: {e}"));
        assert_eq!(reply["id"], id, "replies must be matched to their request");
        reply
    }

    async fn call(&mut self, name: &str, arguments: serde_json::Value) -> serde_json::Value {
        self.request(
            "tools/call",
            serde_json::json!({"name": name, "arguments": arguments}),
        )
        .await
    }

    /// Call a tool that is expected to succeed and hand back its payload.
    ///
    /// Every assertion a caller would otherwise have to remember lives here:
    /// no protocol error, `isError` false, `ok` true, and the payload is the
    /// payload rather than a second result nested inside itself.
    async fn payload(&mut self, name: &str, arguments: serde_json::Value) -> serde_json::Value {
        let reply = self.call(name, arguments).await;
        assert!(
            reply["error"].is_null(),
            "{name} must not fail at the protocol level: {reply}"
        );
        let result = &reply["result"];
        assert_eq!(
            result["isError"],
            serde_json::Value::Bool(false),
            "{name} reported a failure: {result}"
        );
        let structured = result["structuredContent"].clone();
        assert!(
            structured["structuredContent"].is_null() && structured["isError"].is_null(),
            "structuredContent must be the payload, not a nested result: {structured}"
        );
        assert_eq!(
            structured["ok"],
            serde_json::Value::Bool(true),
            "{name} payload is not ok: {structured}"
        );
        structured
    }

    async fn finish(self) {
        let Session { child, stdin, .. } = self;
        drop(stdin);
        let _ = child.wait_with_output().await;
    }
}

/// Entry deterrence with two scales of entry: the Incumbent has two decision
/// nodes, hence four complete contingent plans (Bonanno §2.3, p. 73). Same
/// fixture as `convert_form`'s unit tests.
fn two_node_entry() -> serde_json::Value {
    serde_json::json!({
        "form": "extensive",
        "players": [{"id": 0, "name": "Entrant"}, {"id": 1, "name": "Incumbent"}],
        "root": 0,
        "nodes": [
            {"kind": "decision", "player": 0,
             "actions": [["EnterSmall", 1], ["EnterBig", 2], ["Out", 3]]},
            {"kind": "decision", "player": 1, "actions": [["Fight", 4], ["Accommodate", 5]]},
            {"kind": "decision", "player": 1, "actions": [["Fight", 6], ["Accommodate", 7]]},
            {"kind": "terminal", "payoffs": [0.0, 4.0]},
            {"kind": "terminal", "payoffs": [-1.0, 1.0]},
            {"kind": "terminal", "payoffs": [1.0, 2.0]},
            {"kind": "terminal", "payoffs": [-3.0, -1.0]},
            {"kind": "terminal", "payoffs": [3.0, 0.0]}
        ],
        "information_sets": [[0], [1], [2]],
        "payoff_kind": "cardinal"
    })
}

/// Entry game, Bonanno (2015) Figure 2.9, p. 77. Published backward-induction
/// solution: (in, accommodate), payoffs (2, 2).
fn bonanno_entry_game() -> serde_json::Value {
    serde_json::json!({
        "form": "extensive",
        "players": [
            {"id": 0, "name": "Potential entrant"},
            {"id": 1, "name": "Incumbent"}
        ],
        "root": 0,
        "nodes": [
            {"kind": "decision", "player": 0, "actions": [["in", 1], ["out", 2]]},
            {"kind": "decision", "player": 1, "actions": [["fight", 3], ["accommodate", 4]]},
            {"kind": "terminal", "payoffs": [1.0, 5.0]},
            {"kind": "terminal", "payoffs": [0.0, 0.0]},
            {"kind": "terminal", "payoffs": [2.0, 2.0]}
        ],
        "information_sets": [[0], [1]],
        "payoff_kind": "cardinal"
    })
}

/// Battle of the Sexes: two pure equilibria, the two coordinated profiles.
fn battle_of_the_sexes() -> serde_json::Value {
    serde_json::json!({
        "form": "matrix",
        "players": ["Alice", "Bob"],
        "row_strategies": ["Opera", "Football"],
        "col_strategies": ["Opera", "Football"],
        "payoff_matrix": [[[2.0, 1.0], [0.0, 0.0]], [[0.0, 0.0], [1.0, 2.0]]],
        "payoff_kind": "cardinal"
    })
}

/// Bonanno, Table 5.5 (pp. 196-197). Unique equilibrium: Player 1 on
/// (1/5, 4/5) and Player 2 on (2/3, 1/3), paying 10/3 and 12/5.
fn bonanno_table_5_5() -> serde_json::Value {
    serde_json::json!({
        "form": "matrix",
        "players": ["Player 1", "Player 2"],
        "row_strategies": ["B", "C"],
        "col_strategies": ["E", "F"],
        "payoff_matrix": [[[4.0, 0.0], [2.0, 4.0]], [[3.0, 3.0], [4.0, 2.0]]],
        "payoff_kind": "cardinal"
    })
}

#[tokio::test]
async fn a_real_session_initializes_lists_tools_and_verifies_an_equilibrium() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-theory-mcp"))
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
    assert_eq!(reply["result"]["serverInfo"]["name"], "game-theory-mcp");

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
        stderr.contains("game-theory-mcp"),
        "expected a startup log on stderr, got {stderr:?}"
    );
}

#[tokio::test]
async fn a_malformed_game_comes_back_as_a_tool_result_not_a_protocol_error() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-theory-mcp"))
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-theory-mcp"))
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_game-theory-mcp"))
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

/// A tool registered but not reachable over the wire is invisible to hosts,
/// and the in-process router test cannot tell the difference.
#[tokio::test]
async fn tools_list_advertises_the_whole_v1_surface_over_the_wire() {
    let mut s = Session::start().await;
    let reply = s.request("tools/list", serde_json::json!({})).await;
    let tools = reply["result"]["tools"].as_array().unwrap();

    // Sorted, because the router returns tools by name rather than in
    // registration order.
    let mut names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "analyze_payoff_structure",
            "analyze_repeated_game",
            "convert_form",
            "solve_backward_induction",
            "solve_dominance",
            "solve_mixed_nash",
            "solve_pure_nash",
            "validate_game",
            "verify_equilibrium",
        ]
    );

    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert!(
            tool["description"].as_str().is_some_and(|d| !d.is_empty()),
            "{name} reaches the host with no description"
        );
        let schema = &tool["outputSchema"];
        assert!(
            schema.is_object(),
            "{name} reaches the host with no output schema"
        );
        // Every tool declares Output = CallToolResult so it can set isError,
        // then republishes its payload's schema. Without that override the
        // host is handed CallToolResult's shape, which says nothing.
        let text = serde_json::to_string(schema).unwrap();
        assert!(
            text.contains("\"ok\""),
            "{name} publishes CallToolResult's schema, not its payload's"
        );
    }

    s.finish().await;
}

#[tokio::test]
async fn convert_form_returns_a_strategic_game_and_the_plans_behind_it() {
    let mut s = Session::start().await;
    let v = s
        .payload(
            "convert_form",
            serde_json::json!({"game": two_node_entry()}),
        )
        .await;

    assert_eq!(v["game"]["form"], "strategic");
    // 3 Entrant strategies x 4 Incumbent strategies.
    assert_eq!(v["game"]["strategies"][1].as_array().unwrap().len(), 4);
    assert_eq!(v["game"]["outcomes"].as_array().unwrap().len(), 12);

    let incumbent = &v["plans"][1];
    assert_eq!(incumbent["player_name"], "Incumbent");
    assert_eq!(incumbent["decision_nodes"], serde_json::json!([1, 2]));
    // Each plan fixes an action at BOTH nodes, though at most one is reached.
    assert_eq!(
        incumbent["strategies"][1]["plan"],
        serde_json::json!([
            {"node": 1, "action": 0, "action_label": "Fight"},
            {"node": 2, "action": 1, "action_label": "Accommodate"}
        ])
    );

    // The converted game is not just readable, it is sendable: it solves.
    let solved = s
        .payload(
            "solve_pure_nash",
            serde_json::json!({"game": v["game"].clone()}),
        )
        .await;
    assert!(!solved["equilibria"].as_array().unwrap().is_empty());

    s.finish().await;
}

#[tokio::test]
async fn solve_dominance_reduces_the_prisoners_dilemma() {
    let mut s = Session::start().await;
    let v = s
        .payload(
            "solve_dominance",
            serde_json::json!({"game": prisoners_dilemma(), "mode": "strict"}),
        )
        .await;

    assert_eq!(v["mode"], "strict");
    let a = &v["analyses"][0];
    assert_eq!(a["unique_profile"]["strategies"], serde_json::json!([1, 1]));
    assert_eq!(
        a["unique_profile"]["strategy_names"],
        serde_json::json!(["Defect", "Defect"])
    );
    assert_eq!(a["order_dependent"], serde_json::Value::Bool(false));

    s.finish().await;
}

#[tokio::test]
async fn solve_pure_nash_finds_both_coordination_equilibria() {
    let mut s = Session::start().await;
    let v = s
        .payload(
            "solve_pure_nash",
            serde_json::json!({"game": battle_of_the_sexes()}),
        )
        .await;

    let eq = v["equilibria"].as_array().unwrap();
    assert_eq!(eq.len(), 2, "got {eq:?}");
    assert_eq!(
        eq[0]["strategy_names"],
        serde_json::json!(["Opera", "Opera"])
    );
    assert_eq!(
        eq[1]["strategy_names"],
        serde_json::json!(["Football", "Football"])
    );
    assert_eq!(v["checks"].as_array().unwrap().len(), 4);

    s.finish().await;
}

/// The published mixture is asymmetric, so a solver that lost exactness --
/// or a serializer that rendered fractions as floats -- shows up here.
#[tokio::test]
async fn solve_mixed_nash_returns_the_published_mixture_as_exact_fractions() {
    let mut s = Session::start().await;
    let v = s
        .payload(
            "solve_mixed_nash",
            serde_json::json!({"game": bonanno_table_5_5()}),
        )
        .await;

    let eqs = v["equilibria"].as_array().unwrap();
    assert_eq!(eqs.len(), 1, "the published answer is unique: {eqs:#?}");
    let players = &eqs[0]["players"];

    assert_eq!(players[0]["probabilities"][0]["exact"], "1/5");
    assert_eq!(players[0]["probabilities"][1]["exact"], "4/5");
    assert_eq!(players[0]["expected_payoff"]["exact"], "10/3");
    assert_eq!(players[1]["probabilities"][0]["exact"], "2/3");
    assert_eq!(players[1]["probabilities"][1]["exact"], "1/3");
    assert_eq!(players[1]["expected_payoff"]["exact"], "12/5");
    assert_eq!(v["degenerate"], serde_json::Value::Bool(false));

    s.finish().await;
}

#[tokio::test]
async fn solve_backward_induction_folds_the_entry_game_to_its_published_solution() {
    let mut s = Session::start().await;
    let v = s
        .payload(
            "solve_backward_induction",
            serde_json::json!({"game": bonanno_entry_game()}),
        )
        .await;

    let solutions = v["solutions"].as_array().unwrap();
    assert_eq!(solutions.len(), 1, "the published solution is unique");
    assert_eq!(v["multiple_solutions"], serde_json::Value::Bool(false));

    let spe = &solutions[0];
    assert_eq!(spe["path"][0]["action_label"], "in");
    assert_eq!(spe["path"][1]["action_label"], "accommodate");
    assert_eq!(spe["terminal_payoffs"][0]["exact"], "2");
    assert_eq!(spe["terminal_payoffs"][1]["exact"], "2");
    assert_eq!(spe["profile"][1]["moves"][0]["action_label"], "accommodate");

    s.finish().await;
}

#[tokio::test]
async fn analyze_payoff_structure_classifies_the_prisoners_dilemma() {
    let mut s = Session::start().await;
    let v = s
        .payload(
            "analyze_payoff_structure",
            serde_json::json!({"game": prisoners_dilemma()}),
        )
        .await;

    assert_eq!(v["archetype"]["archetype"], "prisoners_dilemma");
    let structure = &v["structure"];
    assert_eq!(structure["welfare_value"]["exact"], "6");
    assert_eq!(structure["efficiency_gap"]["exact"], "4");
    // Mutual defection is the one profile off the frontier.
    assert_eq!(structure["pareto_frontier"].as_array().unwrap().len(), 3);
    assert_eq!(
        structure["dominated_equilibria"][0]["equilibrium"]["strategy_names"],
        serde_json::json!(["Defect", "Defect"])
    );

    s.finish().await;
}

/// The threshold is a fraction, and the caller's δ can land exactly on it, so
/// both directions of the wire have to carry an exact fraction.
#[tokio::test]
async fn analyze_repeated_game_puts_the_threshold_at_exactly_one_third() {
    let mut s = Session::start().await;
    let v = s
        .payload(
            "analyze_repeated_game",
            serde_json::json!({"game": prisoners_dilemma(), "target": [0, 0]}),
        )
        .await;

    assert_eq!(v["critical_discount_factor"]["exact"], "1/3");
    assert_eq!(v["punishment"], "grim_trigger");
    assert_eq!(v["punishment_profile"], serde_json::json!([1, 1]));
    assert_eq!(v["per_player"][0]["best_deviation_label"], "Defect");
    assert!(v["sustainable_at"].is_null(), "no δ was supplied");

    // The threshold itself sustains the target: δ >= δ*, not δ > δ*.
    let at_threshold = s
        .payload(
            "analyze_repeated_game",
            serde_json::json!({
                "game": prisoners_dilemma(), "target": [0, 0], "discount_factor": "1/3"
            }),
        )
        .await;
    assert_eq!(at_threshold["discount_factor"]["exact"], "1/3");
    assert_eq!(
        at_threshold["sustainable_at"],
        serde_json::Value::Bool(true)
    );

    let below = s
        .payload(
            "analyze_repeated_game",
            serde_json::json!({
                "game": prisoners_dilemma(), "target": [0, 0], "discount_factor": "1/4"
            }),
        )
        .await;
    assert_eq!(below["sustainable_at"], serde_json::Value::Bool(false));

    s.finish().await;
}

/// §7.5c of the product spec: a domain refusal is an answer, not a transport
/// failure. It must reach the host as a successful JSON-RPC response whose
/// result carries `isError: true` and a payload with a stable `code`, so the
/// host LLM can read the reason and retry instead of seeing the call itself
/// break. Ordinal payoffs to a tool that sums across players is the cleanest
/// case: the game is well formed, the request is not answerable.
#[tokio::test]
async fn a_domain_refusal_arrives_as_a_tool_result_carrying_a_stable_code() {
    let mut s = Session::start().await;
    let mut ordinal = prisoners_dilemma();
    ordinal["payoff_kind"] = serde_json::json!("ordinal");

    let reply = s
        .call(
            "analyze_payoff_structure",
            serde_json::json!({"game": ordinal}),
        )
        .await;

    assert!(
        reply["error"].is_null(),
        "a domain refusal must not be a JSON-RPC error: {reply}"
    );
    let result = &reply["result"];
    assert_eq!(result["isError"], serde_json::Value::Bool(true));

    let structured = &result["structuredContent"];
    assert!(
        structured["structuredContent"].is_null(),
        "structuredContent must be the payload, not a nested result: {structured}"
    );
    assert_eq!(structured["ok"], serde_json::Value::Bool(false));
    assert_eq!(structured["code"], "ordinal_payoffs_rejected");
    assert_eq!(structured["tool"], "analyze_payoff_structure");
    assert!(!structured["suggestion"].as_str().unwrap().is_empty());

    s.finish().await;
}

/// The counterpart to the refusal above: a decimal where an exact fraction is
/// required cannot be acted on at all -- there is no game-theoretic answer to
/// return -- so it stays a JSON-RPC error rather than becoming a result.
#[tokio::test]
async fn a_decimal_discount_factor_is_a_protocol_error() {
    let mut s = Session::start().await;
    let reply = s
        .call(
            "analyze_repeated_game",
            serde_json::json!({
                "game": prisoners_dilemma(), "target": [0, 0], "discount_factor": "0.5"
            }),
        )
        .await;

    assert!(
        !reply["error"].is_null(),
        "a decimal discount factor should be a protocol error: {reply}"
    );
    assert_eq!(reply["error"]["code"], -32602);
    assert!(reply["result"].is_null());

    s.finish().await;
}
