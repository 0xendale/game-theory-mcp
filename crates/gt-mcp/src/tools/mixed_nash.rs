//! `solve_mixed_nash` -- every mixed-strategy Nash equilibrium of a 2-player
//! cardinal game.
//!
//! `gt-core` enumerates equal-size support pairs and solves the indifference
//! conditions exactly, so pure equilibria come back too, as singleton-support
//! mixtures. When the game is degenerate that enumeration can miss equilibria
//! on unequal-size supports; `gt-core` says so, and this layer forwards its
//! warning verbatim rather than presenting a possibly-partial list as complete.

use crate::server::GtServer;
use crate::wire::game::GameJson;
use crate::wire::number::Exact;
use crate::wire::outcome::{output_schema_of, RequestError, ToolOutput};
use gt_core::game::ValidStrategicGame;
use gt_core::solve::{solve_mixed_nash, MixedEquilibrium};
use rmcp::handler::server::router::tool::{SyncTool, ToolBase};
use rmcp::model::{CallToolResult, JsonObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct SolveMixedNashInput {
    /// The game, in matrix or strategic form. Exactly two players, and
    /// `payoff_kind` must be "cardinal" -- a mixed equilibrium is defined by
    /// expected payoffs, which ranks cannot express.
    pub game: GameJson,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SolveMixedNashOutput {
    /// Every equilibrium found, pure ones included as singleton supports.
    pub equilibria: Vec<WireMixedEquilibrium>,
    /// True when this game is degenerate, so equilibria may exist on
    /// unequal-size supports that the enumeration does not cover. When true,
    /// `equilibria` is a lower bound, not the complete set.
    pub degenerate: bool,
    /// The solver's own warning, present exactly when `degenerate` is true.
    pub warning: Option<String>,
}

/// One equilibrium: one entry per player, in player order.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireMixedEquilibrium {
    pub players: Vec<WirePlayerMixture>,
}

/// One player's side of an equilibrium.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WirePlayerMixture {
    pub player: usize,
    pub player_name: String,
    /// This player's strategy names, in index order. `probabilities[i]` and
    /// `strategy_names[i]` describe the same strategy.
    pub strategy_names: Vec<String>,
    /// Exact probability on each strategy, summing to 1. Off-support entries
    /// are "0".
    pub probabilities: Vec<Exact>,
    /// Indices of the strategies played with positive probability.
    pub support: Vec<usize>,
    /// Names of the strategies in `support`, in the same order.
    pub support_names: Vec<String>,
    /// This player's expected payoff at the equilibrium.
    pub expected_payoff: Exact,
}

pub struct SolveMixedNash;

impl ToolBase for SolveMixedNash {
    type Parameter = SolveMixedNashInput;
    // CallToolResult rather than the payload, so the tool controls `isError`;
    // output_schema below republishes the payload's schema.
    type Output = CallToolResult;
    type Error = RequestError;

    fn output_schema() -> Option<Arc<JsonObject>> {
        output_schema_of::<ToolOutput<SolveMixedNashOutput>>()
    }

    fn name() -> Cow<'static, str> {
        "solve_mixed_nash".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "Find every mixed-strategy Nash equilibrium of a 2-player game with \
             cardinal payoffs. Each player's mixture comes back as exact \
             fractions aligned with that player's strategy names, together with \
             its support and the player's expected payoff. Pure equilibria are \
             included, as mixtures placing probability 1 on one strategy. If \
             `degenerate` is true the list may be incomplete and `warning` says \
             why. Three or more players, ordinal payoffs, and extensive-form \
             games are refused rather than approximated."
                .into(),
        )
    }
}

impl SyncTool<GtServer> for SolveMixedNash {
    fn invoke(
        _service: &GtServer,
        param: SolveMixedNashInput,
    ) -> Result<Self::Output, Self::Error> {
        let outcome = param.game.into_strategic().and_then(|game| {
            solve_mixed_nash(&game).map(|result| SolveMixedNashOutput {
                equilibria: result
                    .equilibria
                    .iter()
                    .map(|eq| describe(&game, eq))
                    .collect(),
                degenerate: result.degenerate,
                warning: result.warning,
            })
        });

        let envelope: ToolOutput<SolveMixedNashOutput> = match outcome {
            Ok(v) => ToolOutput::ok(v),
            Err(e) => e.into(),
        };
        Ok(envelope.into_call_tool_result())
    }
}

/// Attach names to one equilibrium. Purely presentational -- every number here
/// is copied from what `gt-core` computed.
fn describe(game: &ValidStrategicGame, eq: &MixedEquilibrium) -> WireMixedEquilibrium {
    WireMixedEquilibrium {
        players: (0..game.n_players())
            .map(|p| WirePlayerMixture {
                player: p,
                player_name: game.player_name(p).to_string(),
                strategy_names: (0..game.n_strategies(p))
                    .map(|s| game.strategy_name(p, s).to_string())
                    .collect(),
                probabilities: eq.strategies[p].probs.iter().map(Exact::from).collect(),
                support: eq.supports[p].clone(),
                support_names: eq.supports[p]
                    .iter()
                    .map(|&s| game.strategy_name(p, s).to_string())
                    .collect(),
                expected_payoff: Exact::from(&eq.expected_payoffs[p]),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(game: serde_json::Value) -> SolveMixedNashInput {
        serde_json::from_value(serde_json::json!({ "game": game })).unwrap()
    }

    /// Matching Pennies. No pure equilibrium; the mixed one is (1/2, 1/2).
    fn matching_pennies() -> serde_json::Value {
        serde_json::json!({
            "form": "matrix",
            "players": ["Row", "Col"],
            "row_strategies": ["Heads", "Tails"],
            "col_strategies": ["Heads", "Tails"],
            "payoff_matrix": [[[1.0, -1.0], [-1.0, 1.0]], [[-1.0, 1.0], [1.0, -1.0]]],
            "payoff_kind": "cardinal"
        })
    }

    /// Bonanno, Table 5.5 (pp. 196-197): the game left by iterated deletion of
    /// strictly dominated strategies in §5.3. Its unique equilibrium is
    /// Player 1 on (1/5, 4/5) and Player 2 on (2/3, 1/3), with expected
    /// payoffs 10/3 and 12/5.
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

    /// Entry game, Bonanno p. 77.
    fn entry() -> serde_json::Value {
        serde_json::json!({
            "form": "extensive",
            "players": [{"id": 0, "name": "Entrant"}, {"id": 1, "name": "Incumbent"}],
            "root": 0,
            "nodes": [
                {"kind": "decision", "player": 0, "actions": [["In", 1], ["Out", 2]]},
                {"kind": "decision", "player": 1, "actions": [["Fight", 3], ["Accommodate", 4]]},
                {"kind": "terminal", "payoffs": [0.0, 2.0]},
                {"kind": "terminal", "payoffs": [-1.0, -1.0]},
                {"kind": "terminal", "payoffs": [1.0, 1.0]}
            ],
            "information_sets": [[0], [1]],
            "payoff_kind": "cardinal"
        })
    }

    /// A 3-player game, every payoff zero. Refused on the player count alone.
    fn three_player() -> serde_json::Value {
        let mut outcomes = Vec::new();
        for a in 0..2 {
            for b in 0..2 {
                for c in 0..2 {
                    outcomes.push(serde_json::json!({
                        "profile": [a, b, c], "payoffs": [0.0, 0.0, 0.0]
                    }));
                }
            }
        }
        serde_json::json!({
            "form": "strategic",
            "players": [
                {"id": 0, "name": "P0"}, {"id": 1, "name": "P1"}, {"id": 2, "name": "P2"}
            ],
            "strategies": [["A", "B"], ["A", "B"], ["A", "B"]],
            "outcomes": outcomes,
            "payoff_kind": "cardinal"
        })
    }

    /// Tools return `CallToolResult`, so assertions read `structuredContent`.
    fn value(out: &CallToolResult) -> serde_json::Value {
        out.structured_content
            .clone()
            .expect("tools return structured content")
    }

    fn solve(game: serde_json::Value) -> serde_json::Value {
        value(&SolveMixedNash::invoke(&GtServer::new(), input(game)).unwrap())
    }

    #[test]
    fn the_tool_is_named_and_described() {
        assert_eq!(SolveMixedNash::name(), "solve_mixed_nash");
        assert!(SolveMixedNash::description().is_some());
        assert!(SolveMixedNash::input_schema().is_some());
        // The published output schema must describe the payload, not
        // CallToolResult's own shape.
        let schema = SolveMixedNash::output_schema().unwrap();
        let text = serde_json::to_string(&*schema).unwrap();
        assert!(text.contains("degenerate"), "got {text}");
        assert!(text.contains("support_names"), "got {text}");
    }

    #[test]
    fn matching_pennies_has_one_equilibrium_at_the_half_half_mixture() {
        let v = solve(matching_pennies());
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        assert_eq!(v["degenerate"], serde_json::Value::Bool(false));
        assert_eq!(v["warning"], serde_json::Value::Null);

        let eqs = v["equilibria"].as_array().unwrap();
        assert_eq!(eqs.len(), 1, "got {eqs:#?}");
        let players = eqs[0]["players"].as_array().unwrap();
        assert_eq!(players.len(), 2);
        for (p, side) in players.iter().enumerate() {
            // The exact fractions are authoritative; the approx floats are
            // display only, so nothing here asserts on them.
            assert_eq!(side["player"], p);
            assert_eq!(side["probabilities"][0]["exact"], "1/2");
            assert_eq!(side["probabilities"][1]["exact"], "1/2");
            assert_eq!(side["expected_payoff"]["exact"], "0");
            assert_eq!(side["support"], serde_json::json!([0, 1]));
            assert_eq!(side["support_names"], serde_json::json!(["Heads", "Tails"]));
            assert_eq!(
                side["strategy_names"],
                serde_json::json!(["Heads", "Tails"])
            );
        }
        assert_eq!(players[0]["player_name"], "Row");
        assert_eq!(players[1]["player_name"], "Col");
    }

    #[test]
    fn bonanno_table_5_5_has_its_published_asymmetric_mixture() {
        // Bonanno §5.3, Table 5.5, pp. 196-197: p = 1/5 on B, q = 2/3 on E.
        let v = solve(bonanno_table_5_5());
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
    }

    #[test]
    fn a_pure_equilibrium_comes_back_as_a_singleton_support_mixture() {
        // Prisoner's Dilemma: (Defect, Defect) and nothing else.
        let v = solve(serde_json::json!({
            "form": "matrix",
            "players": ["Row", "Col"],
            "row_strategies": ["Cooperate", "Defect"],
            "col_strategies": ["Cooperate", "Defect"],
            "payoff_matrix": [[[3.0, 3.0], [0.0, 4.0]], [[4.0, 0.0], [1.0, 1.0]]],
            "payoff_kind": "cardinal"
        }));
        let eqs = v["equilibria"].as_array().unwrap();
        assert_eq!(eqs.len(), 1);
        let row = &eqs[0]["players"][0];
        assert_eq!(row["support"], serde_json::json!([1]));
        assert_eq!(row["support_names"], serde_json::json!(["Defect"]));
        assert_eq!(row["probabilities"][0]["exact"], "0");
        assert_eq!(row["probabilities"][1]["exact"], "1");
        assert_eq!(row["expected_payoff"]["exact"], "1");
        assert_eq!(v["degenerate"], serde_json::Value::Bool(false));
    }

    #[test]
    fn a_degenerate_game_carries_the_solvers_warning_verbatim() {
        // Every payoff equal: the equal-support enumeration cannot be complete.
        let v = solve(serde_json::json!({
            "form": "matrix",
            "players": ["Row", "Col"],
            "row_strategies": ["A", "B"],
            "col_strategies": ["A", "B"],
            "payoff_matrix": [[[1.0, 1.0], [1.0, 1.0]], [[1.0, 1.0], [1.0, 1.0]]],
            "payoff_kind": "cardinal"
        }));
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        assert_eq!(v["degenerate"], serde_json::Value::Bool(true));
        let warning = v["warning"].as_str().expect("a warning accompanies it");
        assert!(warning.contains("unequal-size supports"), "got {warning}");
    }

    #[test]
    fn three_players_are_refused_rather_than_approximated() {
        let v = solve(three_player());
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "n_player_mixed_unsupported");
        assert_eq!(v["players"], 3);
        assert!(v["suggestion"]
            .as_str()
            .unwrap()
            .contains("solve_pure_nash"));
    }

    #[test]
    fn ordinal_payoffs_are_refused() {
        let mut g = matching_pennies();
        g["payoff_kind"] = serde_json::json!("ordinal");
        let v = solve(g);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "ordinal_payoffs_rejected");
        assert_eq!(v["tool"], "solve_mixed_nash");
    }

    #[test]
    fn a_tree_reports_wrong_game_form() {
        let v = solve(entry());
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "wrong_game_form");
        assert_eq!(v["expected"], "strategic");
        assert_eq!(v["actual"], "extensive");
        assert!(v["suggestion"].as_str().unwrap().contains("convert_form"));
    }
}
