//! `solve_backward_induction` -- fold a perfect-information tree and return
//! every subgame-perfect equilibrium.
//!
//! `game-theory-core` already refuses to break ties, so this layer's only job is
//! translation: node and action *indices* are what the solver speaks, but the
//! host LLM reads names. Every index therefore travels with its label, and
//! every payoff as an exact fraction.

use crate::server::GtServer;
use crate::wire::game::GameJson;
use crate::wire::number::Exact;
use crate::wire::outcome::{output_schema_of, RequestError, ToolOutput};
use game_theory_core::game::{Node, NodeId, StrategyId, ValidExtensiveGame};
use game_theory_core::solve::solve_backward_induction;
use rmcp::handler::server::router::tool::{SyncTool, ToolBase};
use rmcp::model::{CallToolResult, JsonObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct BackwardInductionInput {
    /// The game, in extensive (tree) form with singleton information sets.
    /// Strategic and matrix games are rejected: backward induction needs the
    /// order of moves, which those forms do not record.
    pub game: GameJson,
}

/// One move: which node, which action index at that node, and its label.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireMove {
    pub node: NodeId,
    /// Index of the action within that node's `actions` list.
    pub action: StrategyId,
    /// The action's label, as supplied in the game.
    pub action_label: String,
}

/// One player's complete plan: an action at every decision node they own,
/// including nodes their own earlier choices make unreachable.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WirePlan {
    pub player: usize,
    pub player_name: String,
    pub moves: Vec<WireMove>,
}

/// One subgame-perfect equilibrium.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireSolution {
    /// One plan per player, in player order.
    pub profile: Vec<WirePlan>,
    /// The path this profile induces, root to terminal.
    pub path: Vec<WireMove>,
    /// Payoff at the terminal that path reaches, one per player.
    pub terminal_payoffs: Vec<Exact>,
}

/// What one action was worth at a decision node.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireActionValue {
    pub action: StrategyId,
    pub action_label: String,
    /// Continuation payoff vector under optimal play of this action, one entry
    /// per player. On a tie below, one representative optimal continuation.
    pub payoffs: Vec<Exact>,
}

/// An action index with its label.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireActionRef {
    pub action: StrategyId,
    pub action_label: String,
}

/// What happened at one decision node, for the derivation trace.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireNodeDecision {
    pub node: NodeId,
    pub player: usize,
    pub player_name: String,
    /// Actions attaining the player's best continuation value here. More than
    /// one means the player is indifferent, which multiplies `solutions`.
    pub chosen: Vec<WireActionRef>,
    /// Actions the fold discarded here.
    pub pruned: Vec<WireActionRef>,
    /// Every action's value, in action order.
    pub action_values: Vec<WireActionValue>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BackwardInductionOutput {
    /// Every subgame-perfect equilibrium. A finite perfect-information tree
    /// always has at least one; ties produce several and all are returned.
    pub solutions: Vec<WireSolution>,
    /// Whether more than one equilibrium was found -- i.e. some player is
    /// indifferent somewhere in the tree.
    pub multiple_solutions: bool,
    /// The per-node fold, ascending by node id.
    pub node_log: Vec<WireNodeDecision>,
}

pub struct SolveBackwardInduction;

impl ToolBase for SolveBackwardInduction {
    type Parameter = BackwardInductionInput;
    // CallToolResult rather than the payload, so the tool controls `isError`;
    // output_schema below republishes the payload's schema.
    type Output = CallToolResult;
    type Error = RequestError;

    fn output_schema() -> Option<Arc<JsonObject>> {
        output_schema_of::<ToolOutput<BackwardInductionOutput>>()
    }

    fn name() -> Cow<'static, str> {
        "solve_backward_induction".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "Solve a perfect-information game tree by backward induction. \
             Returns EVERY subgame-perfect equilibrium -- a player indifferent \
             between actions yields several, and all are returned rather than \
             one being picked silently. Each solution carries a complete plan \
             per player as (node, action) pairs covering every decision node \
             that player owns, the induced path from root to terminal, and the \
             terminal payoffs as exact fractions. The node log shows the fold \
             itself: at each decision node, who moved, which actions were \
             chosen, which were pruned, and what every action was worth. Node \
             and action indices always travel with their labels. Needs an \
             extensive-form game; every information set must be a singleton."
                .into(),
        )
    }
}

impl SyncTool<GtServer> for SolveBackwardInduction {
    fn invoke(
        _service: &GtServer,
        param: BackwardInductionInput,
    ) -> Result<Self::Output, Self::Error> {
        let outcome = param
            .game
            .into_extensive()
            .and_then(|game| solve_backward_induction(&game).map(|r| (game, r)))
            .map(|(game, result)| {
                let solutions: Vec<WireSolution> = result
                    .solutions
                    .iter()
                    .map(|s| WireSolution {
                        profile: s
                            .profile
                            .iter()
                            .enumerate()
                            .map(|(player, plan)| WirePlan {
                                player,
                                player_name: game.player_name(player).to_string(),
                                moves: plan.iter().map(|&m| wire_move(&game, m)).collect(),
                            })
                            .collect(),
                        path: s.path.iter().map(|&m| wire_move(&game, m)).collect(),
                        terminal_payoffs: s.terminal_payoffs.iter().map(Exact::from).collect(),
                    })
                    .collect();

                let node_log = result
                    .node_log
                    .iter()
                    .map(|d| WireNodeDecision {
                        node: d.node,
                        player: d.player,
                        player_name: game.player_name(d.player).to_string(),
                        chosen: d
                            .chosen
                            .iter()
                            .map(|&a| action_ref(&game, d.node, a))
                            .collect(),
                        pruned: d
                            .pruned
                            .iter()
                            .map(|&a| action_ref(&game, d.node, a))
                            .collect(),
                        action_values: d
                            .action_values
                            .iter()
                            .enumerate()
                            .map(|(a, payoffs)| WireActionValue {
                                action: a,
                                action_label: action_label(&game, d.node, a),
                                payoffs: payoffs.iter().map(Exact::from).collect(),
                            })
                            .collect(),
                    })
                    .collect();

                BackwardInductionOutput {
                    multiple_solutions: solutions.len() > 1,
                    solutions,
                    node_log,
                }
            });

        let envelope: ToolOutput<BackwardInductionOutput> = match outcome {
            Ok(v) => ToolOutput::ok(v),
            Err(e) => e.into(),
        };
        Ok(envelope.into_call_tool_result())
    }
}

/// The label of action `action` at `node`. The solver only ever names actions
/// that exist, so the empty fallback is unreachable in practice.
fn action_label(game: &ValidExtensiveGame, node: NodeId, action: StrategyId) -> String {
    match game.node(node) {
        Node::Decision { actions, .. } => actions
            .get(action)
            .map(|(label, _)| label.clone())
            .unwrap_or_default(),
        Node::Terminal { .. } => String::new(),
    }
}

fn action_ref(game: &ValidExtensiveGame, node: NodeId, action: StrategyId) -> WireActionRef {
    WireActionRef {
        action,
        action_label: action_label(game, node, action),
    }
}

fn wire_move(game: &ValidExtensiveGame, (node, action): (NodeId, StrategyId)) -> WireMove {
    WireMove {
        node,
        action,
        action_label: action_label(game, node, action),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(game: serde_json::Value) -> BackwardInductionInput {
        serde_json::from_value(serde_json::json!({ "game": game })).unwrap()
    }

    /// Entry game, Bonanno (2015) Figure 2.9, p. 77. Published
    /// backward-induction solution: (in, accommodate), payoffs (2, 2).
    /// Mirrors `crates/game-theory-core/tests/fixtures/extensive/bonanno_fig_2_9_entry_game.json`.
    fn entry_game() -> serde_json::Value {
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

    /// Player 0 strictly prefers Out, but player 1 is indifferent at a node
    /// the equilibrium path never reaches -- so there are two subgame-perfect
    /// equilibria sharing one path.
    fn off_path_tie() -> serde_json::Value {
        serde_json::json!({
            "form": "extensive",
            "players": [{"id": 0, "name": "A"}, {"id": 1, "name": "B"}],
            "root": 0,
            "nodes": [
                {"kind": "decision", "player": 0, "actions": [["In", 1], ["Out", 2]]},
                {"kind": "decision", "player": 1, "actions": [["L", 3], ["R", 4]]},
                {"kind": "terminal", "payoffs": [3.0, 0.0]},
                {"kind": "terminal", "payoffs": [1.0, 7.0]},
                {"kind": "terminal", "payoffs": [1.0, 7.0]}
            ],
            "information_sets": [[0], [1]],
            "payoff_kind": "cardinal"
        })
    }

    /// Tools return `CallToolResult`, so assertions read `structuredContent`.
    fn value(out: &CallToolResult) -> serde_json::Value {
        out.structured_content
            .clone()
            .expect("tools return structured content")
    }

    #[test]
    fn the_tool_is_named_and_described() {
        assert_eq!(SolveBackwardInduction::name(), "solve_backward_induction");
        assert!(SolveBackwardInduction::description().is_some());
        assert!(SolveBackwardInduction::input_schema().is_some());
        // The published output schema must describe the payload, not
        // CallToolResult's own shape.
        let schema = SolveBackwardInduction::output_schema().unwrap();
        let text = serde_json::to_string(&*schema).unwrap();
        assert!(text.contains("node_log"), "got {text}");
        assert!(text.contains("terminal_payoffs"), "got {text}");
    }

    #[test]
    fn the_entry_game_folds_to_the_published_solution() {
        let out = SolveBackwardInduction::invoke(&GtServer::new(), input(entry_game())).unwrap();
        assert_eq!(out.is_error, Some(false));
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(true));

        let solutions = v["solutions"].as_array().unwrap();
        assert_eq!(solutions.len(), 1, "the published solution is unique");
        assert_eq!(v["multiple_solutions"], serde_json::Value::Bool(false));

        let spe = &solutions[0];
        assert_eq!(spe["path"][0]["action_label"], "in");
        assert_eq!(spe["path"][1]["action_label"], "accommodate");
        assert_eq!(spe["terminal_payoffs"][0]["exact"], "2");
        assert_eq!(spe["terminal_payoffs"][1]["exact"], "2");

        // Plans are per player, labelled, and cover the player's own nodes.
        assert_eq!(spe["profile"][0]["player_name"], "Potential entrant");
        assert_eq!(spe["profile"][0]["moves"][0]["node"], 0);
        assert_eq!(spe["profile"][0]["moves"][0]["action_label"], "in");
        assert_eq!(spe["profile"][1]["player_name"], "Incumbent");
        assert_eq!(spe["profile"][1]["moves"][0]["action_label"], "accommodate");
    }

    #[test]
    fn the_node_log_names_the_pruned_branch_and_its_value() {
        let out = SolveBackwardInduction::invoke(&GtServer::new(), input(entry_game())).unwrap();
        let v = value(&out);
        let log = v["node_log"].as_array().unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0]["node"], 0, "the log is ascending by node id");

        let incumbent = &log[1];
        assert_eq!(incumbent["node"], 1);
        assert_eq!(incumbent["player_name"], "Incumbent");
        assert_eq!(incumbent["chosen"][0]["action_label"], "accommodate");
        assert_eq!(incumbent["pruned"][0]["action_label"], "fight");
        // Fighting is worth 0 to the Incumbent; accommodating is worth 2.
        assert_eq!(incumbent["action_values"][0]["action_label"], "fight");
        assert_eq!(incumbent["action_values"][0]["payoffs"][1]["exact"], "0");
        assert_eq!(incumbent["action_values"][1]["payoffs"][1]["exact"], "2");
    }

    #[test]
    fn a_tie_returns_more_than_one_solution() {
        let out = SolveBackwardInduction::invoke(&GtServer::new(), input(off_path_tie())).unwrap();
        let v = value(&out);
        let solutions = v["solutions"].as_array().unwrap();
        assert_eq!(solutions.len(), 2, "player B is indifferent -> two SPE");
        assert_eq!(v["multiple_solutions"], serde_json::Value::Bool(true));

        // Both share the same path and payoffs; they differ off-path.
        for spe in solutions {
            assert_eq!(spe["path"].as_array().unwrap().len(), 1);
            assert_eq!(spe["path"][0]["action_label"], "Out");
            assert_eq!(spe["terminal_payoffs"][0]["exact"], "3");
        }
        let off_path: Vec<&str> = solutions
            .iter()
            .map(|s| {
                s["profile"][1]["moves"][0]["action_label"]
                    .as_str()
                    .unwrap()
            })
            .collect();
        assert!(off_path.contains(&"L"), "got {off_path:?}");
        assert!(off_path.contains(&"R"), "got {off_path:?}");

        // The indifference is visible in the trace, not only in the count.
        let log = v["node_log"].as_array().unwrap();
        let node_one = log.iter().find(|d| d["node"] == 1).unwrap();
        assert_eq!(node_one["chosen"].as_array().unwrap().len(), 2);
        assert!(node_one["pruned"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_strategic_game_reports_wrong_game_form() {
        let pd = serde_json::json!({
            "form": "matrix",
            "players": ["Row", "Col"],
            "row_strategies": ["Cooperate", "Defect"],
            "col_strategies": ["Cooperate", "Defect"],
            "payoff_matrix": [[[3.0, 3.0], [0.0, 4.0]], [[4.0, 0.0], [1.0, 1.0]]],
            "payoff_kind": "cardinal"
        });
        let out = SolveBackwardInduction::invoke(&GtServer::new(), input(pd)).unwrap();
        assert_eq!(out.is_error, Some(true));
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "wrong_game_form");
        assert_eq!(v["expected"], "extensive");
        assert_eq!(v["actual"], "matrix");
    }

    #[test]
    fn a_non_singleton_information_set_is_refused() {
        // Player 1 cannot tell node 1 from node 2, so the tree cannot be
        // folded node by node.
        let g = serde_json::json!({
            "form": "extensive",
            "players": [{"id": 0, "name": "A"}, {"id": 1, "name": "B"}],
            "root": 0,
            "nodes": [
                {"kind": "decision", "player": 0, "actions": [["A", 1], ["B", 2]]},
                {"kind": "decision", "player": 1, "actions": [["L", 3], ["R", 4]]},
                {"kind": "decision", "player": 1, "actions": [["L", 5], ["R", 6]]},
                {"kind": "terminal", "payoffs": [0.0, 0.0]},
                {"kind": "terminal", "payoffs": [0.0, 0.0]},
                {"kind": "terminal", "payoffs": [0.0, 0.0]},
                {"kind": "terminal", "payoffs": [0.0, 0.0]}
            ],
            "information_sets": [[0], [1, 2]],
            "payoff_kind": "cardinal"
        });
        let out = SolveBackwardInduction::invoke(&GtServer::new(), input(g)).unwrap();
        assert_eq!(out.is_error, Some(true));
        let v = value(&out);
        assert_eq!(v["code"], "imperfect_information_unsupported");
        assert_eq!(v["information_set"], 1);
    }

    #[test]
    fn a_malformed_tree_reports_diagnostics_rather_than_solving() {
        // Node 1 is unreachable from the root.
        let g = serde_json::json!({
            "form": "extensive",
            "players": [{"id": 0, "name": "A"}],
            "root": 0,
            "nodes": [
                {"kind": "terminal", "payoffs": [1.0]},
                {"kind": "terminal", "payoffs": [2.0]}
            ],
            "information_sets": [],
            "payoff_kind": "cardinal"
        });
        let out = SolveBackwardInduction::invoke(&GtServer::new(), input(g)).unwrap();
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "invalid_game");
        assert!(!v["diagnostics"].as_array().unwrap().is_empty());
    }
}
