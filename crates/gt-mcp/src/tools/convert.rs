//! `convert_form` -- turn a perfect-information game tree into its strategic
//! (normal) form.
//!
//! The converted matrix alone is unreadable: a strategy in an extensive game is
//! a *complete contingent plan*, fixing an action at every decision node of
//! that player -- including nodes their own earlier choices make unreachable
//! (Bonanno §2.3). So the strategy indices in the returned game mean nothing
//! without the plan they stand for, and this tool returns both. The plan
//! mapping comes from `gt_core::plan_to_strategy_index`, the same enumeration
//! the conversion used, so the two can never drift apart.

use crate::server::GtServer;
use crate::wire::game::{GameJson, WireOutcome, WirePayoffKind, WirePlayer};
use crate::wire::outcome::{output_schema_of, RequestError, ToolOutput};
use gt_core::game::{
    to_strategic, Node, NodeId, PayoffKind, StrategyId, ValidExtensiveGame, ValidStrategicGame,
};
use rmcp::handler::server::router::tool::{SyncTool, ToolBase};
use rmcp::model::{CallToolResult, JsonObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct ConvertFormInput {
    /// The game to convert, in extensive form. Every information set must be a
    /// singleton -- v1.0 converts perfect-information trees only.
    pub game: GameJson,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ConvertFormOutput {
    /// The converted game, in the same shape the `strategic` variant of a game
    /// input takes. Send it straight back to any strategic-form tool.
    pub game: GameJson,
    /// What each generated strategy index actually means, one entry per player.
    /// Without this the converted matrix cannot be interpreted.
    pub plans: Vec<WirePlayerPlans>,
}

/// One player's strategy set, spelled out as plans.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WirePlayerPlans {
    pub player: usize,
    pub player_name: String,
    /// The player's decision nodes, ascending. Every plan fixes an action at
    /// each of them. Empty for a player who never moves.
    pub decision_nodes: Vec<NodeId>,
    /// One entry per generated strategy, in index order.
    pub strategies: Vec<WirePlan>,
}

/// A single strategy of the converted game and the plan it stands for.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WirePlan {
    /// Index of this strategy in the converted game's strategy list.
    pub strategy: usize,
    /// The converted game's label for this strategy.
    pub strategy_name: String,
    /// The action this plan fixes at every one of the player's decision nodes,
    /// including nodes unreachable given the player's own earlier choices.
    /// Empty for a player who never moves.
    pub plan: Vec<WirePlanStep>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WirePlanStep {
    pub node: NodeId,
    /// The action's index at that node, as used by `verify_equilibrium`'s
    /// `spe` profiles.
    pub action: StrategyId,
    pub action_label: String,
}

pub struct ConvertForm;

impl ToolBase for ConvertForm {
    type Parameter = ConvertFormInput;
    // CallToolResult rather than the payload, so the tool controls `isError`;
    // output_schema below republishes the payload's schema.
    type Output = CallToolResult;
    type Error = RequestError;

    fn output_schema() -> Option<Arc<JsonObject>> {
        output_schema_of::<ToolOutput<ConvertFormOutput>>()
    }

    fn name() -> Cow<'static, str> {
        "convert_form".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "Convert a perfect-information extensive-form game (a tree) to its \
             strategic form. Returns the converted game, ready to send to any \
             strategic-form tool, plus the plan each generated strategy index \
             stands for: the action fixed at every one of that player's \
             decision nodes, including nodes that player's own earlier choices \
             make unreachable. A player with k decision nodes therefore has one \
             strategy per combination of actions across all k, not one per path \
             through the tree. Needs an extensive-form game; anything else is \
             refused."
                .into(),
        )
    }
}

impl SyncTool<GtServer> for ConvertForm {
    fn invoke(_service: &GtServer, param: ConvertFormInput) -> Result<Self::Output, Self::Error> {
        let outcome = param
            .game
            .into_extensive()
            .and_then(|tree| to_strategic(&tree).map(|strategic| (tree, strategic)))
            .map(|(tree, strategic)| ConvertFormOutput {
                game: to_game_json(&strategic),
                plans: plans_of(&tree, &strategic),
            });

        let envelope: ToolOutput<ConvertFormOutput> = match outcome {
            Ok(v) => ToolOutput::ok(v),
            Err(e) => e.into(),
        };
        Ok(envelope.into_call_tool_result())
    }
}

/// Re-encode the converted game in the wire's `strategic` shape. Payoffs are
/// taken from the game as `gt-core` built it, so nothing is recomputed here.
fn to_game_json(g: &ValidStrategicGame) -> GameJson {
    let inner = g.game();
    GameJson::Strategic {
        players: inner
            .players
            .iter()
            .map(|p| WirePlayer {
                id: p.id,
                name: p.name.clone(),
            })
            .collect(),
        strategies: inner.strategies.clone(),
        outcomes: inner
            .outcomes
            .iter()
            .map(|o| WireOutcome {
                profile: o.profile.clone(),
                payoffs: o.payoffs.clone(),
            })
            .collect(),
        payoff_kind: match inner.payoff_kind {
            PayoffKind::Ordinal => WirePayoffKind::Ordinal,
            PayoffKind::Cardinal => WirePayoffKind::Cardinal,
        },
    }
}

/// Spell out every player's strategy set as plans.
///
/// Each plan is placed at the index `gt_core::plan_to_strategy_index` assigns
/// it, so the mapping is the conversion's own enumeration rather than a guess
/// at it. A slot left unfilled would mean the two disagreed, which is a bug in
/// this layer, not something a caller can cause -- hence the assertion.
fn plans_of(tree: &ValidExtensiveGame, strategic: &ValidStrategicGame) -> Vec<WirePlayerPlans> {
    let mut out = Vec::with_capacity(tree.n_players());
    for player in 0..tree.n_players() {
        let nodes = tree.decision_nodes_of(player);
        let mut slots: Vec<Option<WirePlan>> =
            (0..strategic.n_strategies(player)).map(|_| None).collect();

        for plan in every_plan(tree, &nodes) {
            let raw: Vec<(NodeId, StrategyId)> = plan.iter().map(|s| (s.node, s.action)).collect();
            let strategy = gt_core::plan_to_strategy_index(tree, player, &raw);
            slots[strategy] = Some(WirePlan {
                strategy,
                strategy_name: strategic.strategy_name(player, strategy).to_string(),
                plan,
            });
        }

        out.push(WirePlayerPlans {
            player,
            player_name: tree.player_name(player).to_string(),
            decision_nodes: nodes,
            strategies: slots
                .into_iter()
                .map(|s| s.expect("every converted strategy is some plan"))
                .collect(),
        });
    }
    out
}

/// Every combination of one action per decision node. A player with no
/// decision nodes gets exactly one plan, the empty one.
fn every_plan(tree: &ValidExtensiveGame, nodes: &[NodeId]) -> Vec<Vec<WirePlanStep>> {
    let mut plans = vec![Vec::new()];
    for &node in nodes {
        let Node::Decision { actions, .. } = tree.node(node) else {
            unreachable!("decision_nodes_of returns decision nodes only");
        };
        let mut next = Vec::with_capacity(plans.len() * actions.len());
        for prefix in &plans {
            for (action, (label, _child)) in actions.iter().enumerate() {
                let mut extended = prefix.clone();
                extended.push(WirePlanStep {
                    node,
                    action,
                    action_label: label.clone(),
                });
                next.push(extended);
            }
        }
        plans = next;
    }
    plans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(game: serde_json::Value) -> ConvertFormInput {
        serde_json::from_value(serde_json::json!({ "game": game })).unwrap()
    }

    /// Entry deterrence with two scales of entry, so the Incumbent has two
    /// decision nodes and therefore four plans -- one per combination of
    /// Fight/Accommodate across both, even though at most one node is ever
    /// reached (Bonanno §2.3, p. 73).
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

    /// Prisoner's Dilemma, already in matrix form.
    fn pd() -> serde_json::Value {
        serde_json::json!({
            "form": "matrix",
            "players": ["Row", "Col"],
            "row_strategies": ["Cooperate", "Defect"],
            "col_strategies": ["Cooperate", "Defect"],
            "payoff_matrix": [[[3.0, 3.0], [0.0, 4.0]], [[4.0, 0.0], [1.0, 1.0]]],
            "payoff_kind": "cardinal"
        })
    }

    /// Tools return `CallToolResult`, so assertions read `structuredContent`.
    fn value(out: &CallToolResult) -> serde_json::Value {
        out.structured_content
            .clone()
            .expect("tools return structured content")
    }

    fn convert(game: serde_json::Value) -> serde_json::Value {
        value(&ConvertForm::invoke(&GtServer::new(), input(game)).unwrap())
    }

    #[test]
    fn the_tool_is_named_and_described() {
        assert_eq!(ConvertForm::name(), "convert_form");
        assert!(ConvertForm::description().is_some());
        assert!(ConvertForm::input_schema().is_some());
        // The published output schema must describe the payload, not
        // CallToolResult's own shape.
        let schema = ConvertForm::output_schema().unwrap();
        let text = serde_json::to_string(&*schema).unwrap();
        assert!(text.contains("action_label"), "got {text}");
        assert!(text.contains("decision_nodes"), "got {text}");
    }

    #[test]
    fn the_converted_game_is_a_strategic_game_the_caller_can_send_back() {
        let v = convert(two_node_entry());
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        let game = &v["game"];
        assert_eq!(game["form"], "strategic");
        assert_eq!(game["payoff_kind"], "cardinal");
        assert_eq!(
            game["players"],
            serde_json::json!([
                {"id": 0, "name": "Entrant"}, {"id": 1, "name": "Incumbent"}
            ])
        );
        // 3 Entrant strategies x 4 Incumbent strategies.
        assert_eq!(game["strategies"][0].as_array().unwrap().len(), 3);
        assert_eq!(game["strategies"][1].as_array().unwrap().len(), 4);
        assert_eq!(game["outcomes"].as_array().unwrap().len(), 12);

        // It round-trips: the emitted shape parses as a game input again.
        let back: GameJson = serde_json::from_value(game.clone()).unwrap();
        assert_eq!(back.form_name(), "strategic");
        assert_eq!(back.into_strategic().unwrap().n_strategies(1), 4);
    }

    #[test]
    fn the_second_movers_four_strategies_are_spelled_out_as_plans() {
        let v = convert(two_node_entry());
        let incumbent = &v["plans"][1];
        assert_eq!(incumbent["player"], 1);
        assert_eq!(incumbent["player_name"], "Incumbent");
        assert_eq!(incumbent["decision_nodes"], serde_json::json!([1, 2]));

        // Mixed-radix with the LAST node varying fastest, matching gt-core's
        // enumeration. Each plan fixes an action at BOTH nodes, though the
        // Entrant's single move reaches at most one of them.
        let expected = [
            (0, "Fight", "Fight"),
            (1, "Fight", "Accommodate"),
            (2, "Accommodate", "Fight"),
            (3, "Accommodate", "Accommodate"),
        ];
        let strategies = incumbent["strategies"].as_array().unwrap();
        assert_eq!(strategies.len(), 4);
        for (idx, at_node_1, at_node_2) in expected {
            let s = &strategies[idx];
            assert_eq!(s["strategy"], idx);
            assert_eq!(
                s["plan"],
                serde_json::json!([
                    {"node": 1, "action": if at_node_1 == "Fight" {0} else {1},
                     "action_label": at_node_1},
                    {"node": 2, "action": if at_node_2 == "Fight" {0} else {1},
                     "action_label": at_node_2}
                ]),
                "strategy {idx} maps to the wrong plan"
            );
        }
    }

    #[test]
    fn each_plan_carries_the_converted_games_own_label_for_its_index() {
        let v = convert(two_node_entry());
        let strategies = v["plans"][1]["strategies"].as_array().unwrap();
        for s in strategies {
            let idx = s["strategy"].as_u64().unwrap() as usize;
            assert_eq!(
                s["strategy_name"], v["game"]["strategies"][1][idx],
                "plan {idx} is labelled differently from the converted game"
            );
        }
    }

    #[test]
    fn the_first_mover_gets_one_plan_per_action_at_its_single_node() {
        let v = convert(two_node_entry());
        let entrant = &v["plans"][0];
        assert_eq!(entrant["decision_nodes"], serde_json::json!([0]));
        let strategies = entrant["strategies"].as_array().unwrap();
        assert_eq!(strategies.len(), 3);
        assert_eq!(
            strategies[2]["plan"],
            serde_json::json!([{"node": 0, "action": 2, "action_label": "Out"}])
        );
    }

    #[test]
    fn a_player_who_never_moves_gets_a_single_empty_plan() {
        let v = convert(serde_json::json!({
            "form": "extensive",
            "players": [{"id": 0, "name": "Solo"}, {"id": 1, "name": "Idle"}],
            "root": 0,
            "nodes": [
                {"kind": "decision", "player": 0, "actions": [["A", 1], ["B", 2]]},
                {"kind": "terminal", "payoffs": [1.0, 2.0]},
                {"kind": "terminal", "payoffs": [0.0, 3.0]}
            ],
            "information_sets": [[0]],
            "payoff_kind": "cardinal"
        }));
        let idle = &v["plans"][1];
        assert_eq!(idle["decision_nodes"], serde_json::json!([]));
        let strategies = idle["strategies"].as_array().unwrap();
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0]["plan"], serde_json::json!([]));
        assert_eq!(strategies[0]["strategy_name"], "(no move)");
    }

    #[test]
    fn a_strategic_game_is_refused_rather_than_passed_through() {
        let v = convert(pd());
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "wrong_game_form");
        assert_eq!(v["expected"], "extensive");
        assert_eq!(v["actual"], "matrix");
    }

    #[test]
    fn a_non_singleton_information_set_is_refused() {
        let v = convert(serde_json::json!({
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
        }));
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "imperfect_information_unsupported");
    }

    #[test]
    fn an_invalid_tree_reports_diagnostics_not_a_protocol_error() {
        let out = ConvertForm::invoke(
            &GtServer::new(),
            input(serde_json::json!({
                "form": "extensive",
                "players": [{"id": 0, "name": "A"}],
                "root": 0,
                "nodes": [{"kind": "decision", "player": 0, "actions": [["A", 9]]}],
                "information_sets": [[0]],
                "payoff_kind": "cardinal"
            })),
        )
        .unwrap();
        // Ok at the protocol level; the failure is in the payload.
        assert_eq!(out.is_error, Some(true));
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "invalid_game");
        assert!(!v["diagnostics"].as_array().unwrap().is_empty());
    }
}
