//! `solve_pure_nash` -- every pure-strategy Nash equilibrium, plus the
//! profile-by-profile check the answer was read off.
//!
//! All arithmetic is `game_theory_core::solve::pure_nash`. This layer attaches names to
//! the indices and renders each deviation's gain as an exact rational.
//!
//! A game with no pure equilibrium is a *result*: `equilibria` comes back empty
//! with `ok: true`. Matching Pennies has no pure equilibrium, and saying so is
//! the correct answer -- there is deliberately no "no equilibrium exists"
//! error variant.

use crate::server::GtServer;
use crate::wire::game::GameJson;
use crate::wire::number::Exact;
use crate::wire::outcome::{output_schema_of, RequestError, ToolOutput};
use game_theory_core::game::ValidStrategicGame;
use game_theory_core::solve::{solve_pure_nash, Deviation};
use rmcp::handler::server::router::tool::{SyncTool, ToolBase};
use rmcp::model::{CallToolResult, JsonObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct PureNashInput {
    /// The game, in matrix or strategic form, with any number of players. An
    /// extensive-form game is rejected as `wrong_game_form`; convert it first.
    pub game: GameJson,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PureNashOutput {
    /// Player names in player order, so every index below can be read.
    pub players: Vec<String>,
    /// Every pure-strategy Nash equilibrium. Empty when the game has none --
    /// that is an answer, not a failure.
    pub equilibria: Vec<WireProfile>,
    /// Every strategy profile with the verdict on it and, when it is not an
    /// equilibrium, the deviation that rules it out. This is the derivation:
    /// the equilibria above are exactly the profiles here with
    /// `is_equilibrium: true`.
    pub checks: Vec<WireProfileCheck>,
}

/// A strategy profile, by index and by name.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireProfile {
    /// `strategies[p]` is player p's strategy index.
    pub strategies: Vec<usize>,
    /// The same profile by name, index-aligned with `strategies`.
    pub strategy_names: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WireProfileCheck {
    pub profile: WireProfile,
    /// True when no player can gain by changing only their own strategy.
    pub is_equilibrium: bool,
    /// The most profitable unilateral deviation available at this profile.
    /// `null` exactly when `is_equilibrium` is true.
    pub blocking_deviation: Option<WireDeviation>,
}

/// A unilateral change that makes one player strictly better off.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireDeviation {
    pub player: usize,
    pub player_name: String,
    /// The strategy that player is playing in this profile.
    pub from: usize,
    pub from_name: String,
    /// The strategy they would switch to.
    pub to: usize,
    pub to_name: String,
    /// How much that switch gains them, exactly.
    pub gain: Exact,
}

pub struct SolvePureNash;

impl ToolBase for SolvePureNash {
    type Parameter = PureNashInput;
    // CallToolResult rather than the payload, so the tool controls `isError`;
    // output_schema below republishes the payload's schema.
    type Output = CallToolResult;
    type Error = RequestError;

    fn output_schema() -> Option<Arc<JsonObject>> {
        output_schema_of::<ToolOutput<PureNashOutput>>()
    }

    fn name() -> Cow<'static, str> {
        "solve_pure_nash".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "Find every pure-strategy Nash equilibrium of a matrix or strategic \
             game, for any number of players. Returns the equilibria by index \
             and by name, together with the profile-by-profile check they were \
             read off: each profile, whether it is an equilibrium, and the \
             profitable unilateral deviation that rules it out when it is not. \
             A game with no pure equilibrium returns an empty list, not an \
             error -- Matching Pennies is the standard example. Works on \
             ordinal as well as cardinal payoffs, since the definition compares \
             payoffs rather than averaging them."
                .into(),
        )
    }
}

impl SyncTool<GtServer> for SolvePureNash {
    fn invoke(_service: &GtServer, param: PureNashInput) -> Result<Self::Output, Self::Error> {
        // A tree has no strategic-form profiles to check; `into_strategic`
        // reports that as WrongGameForm rather than as a validation problem.
        let outcome = param.game.into_strategic().map(|g| {
            let result = solve_pure_nash(&g);
            PureNashOutput {
                players: (0..g.n_players())
                    .map(|p| g.player_name(p).to_string())
                    .collect(),
                equilibria: result.equilibria.iter().map(|p| profile(&g, p)).collect(),
                checks: result
                    .checks
                    .iter()
                    .map(|c| WireProfileCheck {
                        profile: profile(&g, &c.profile),
                        is_equilibrium: c.is_equilibrium,
                        blocking_deviation: c.blocking_deviation.as_ref().map(|d| deviation(&g, d)),
                    })
                    .collect(),
            }
        });

        let envelope: ToolOutput<PureNashOutput> = match outcome {
            Ok(v) => ToolOutput::ok(v),
            Err(e) => e.into(),
        };
        Ok(envelope.into_call_tool_result())
    }
}

fn profile(game: &ValidStrategicGame, strategies: &[usize]) -> WireProfile {
    WireProfile {
        strategies: strategies.to_vec(),
        strategy_names: strategies
            .iter()
            .enumerate()
            .map(|(p, &s)| game.strategy_name(p, s).to_string())
            .collect(),
    }
}

fn deviation(game: &ValidStrategicGame, d: &Deviation) -> WireDeviation {
    WireDeviation {
        player: d.player,
        player_name: game.player_name(d.player).to_string(),
        from: d.from,
        from_name: game.strategy_name(d.player, d.from).to_string(),
        to: d.to,
        to_name: game.strategy_name(d.player, d.to).to_string(),
        gain: Exact::from(&d.gain),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(game: serde_json::Value) -> PureNashInput {
        serde_json::from_value(serde_json::json!({"game": game})).unwrap()
    }

    /// Battle of the Sexes: two pure equilibria, the two coordinated profiles.
    /// Nash equilibrium is Bonanno §1.6 (p. 32); this is the standard
    /// opposed-preferences coordination matrix, whose two pure equilibria are
    /// the published answer for that structure.
    fn battle_of_the_sexes(kind: &str) -> serde_json::Value {
        serde_json::json!({
            "form": "matrix",
            "players": ["Alice", "Bob"],
            "row_strategies": ["Opera", "Football"],
            "col_strategies": ["Opera", "Football"],
            "payoff_matrix": [[[2.0, 1.0], [0.0, 0.0]], [[0.0, 0.0], [1.0, 2.0]]],
            "payoff_kind": kind
        })
    }

    /// Prisoner's Dilemma. (Defect, Defect) is the only pure equilibrium --
    /// Bonanno §1.5 (p. 28), §1.6 (p. 32).
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

    /// Matching Pennies: no pure equilibrium at all. Bonanno §1.6 (p. 32).
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

    /// Three players each pick A or B; everyone is paid 1 iff all three agree.
    fn three_player_coordination() -> serde_json::Value {
        let mut outcomes = Vec::new();
        for a in 0..2 {
            for b in 0..2 {
                for c in 0..2 {
                    let u = if a == b && b == c { 1.0 } else { 0.0 };
                    outcomes.push(serde_json::json!({
                        "profile": [a, b, c], "payoffs": [u, u, u]
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

    fn entry_tree() -> serde_json::Value {
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

    /// Tools return `CallToolResult`, so assertions read `structuredContent`.
    fn value(out: &CallToolResult) -> serde_json::Value {
        out.structured_content
            .clone()
            .expect("tools return structured content")
    }

    fn run(game: serde_json::Value) -> serde_json::Value {
        let out = SolvePureNash::invoke(&GtServer::new(), input(game)).unwrap();
        assert_eq!(out.is_error, Some(false));
        value(&out)
    }

    #[test]
    fn the_tool_is_named_and_described() {
        assert_eq!(SolvePureNash::name(), "solve_pure_nash");
        assert!(SolvePureNash::description().is_some());
        assert!(SolvePureNash::input_schema().is_some());
        // The published output schema must describe the payload, not
        // CallToolResult's own shape.
        let schema = SolvePureNash::output_schema().unwrap();
        let text = serde_json::to_string(&*schema).unwrap();
        assert!(text.contains("equilibria"), "got {text}");
        assert!(text.contains("blocking_deviation"), "got {text}");
    }

    #[test]
    fn a_coordination_game_has_two_pure_equilibria_named() {
        let v = run(battle_of_the_sexes("cardinal"));
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        assert_eq!(v["players"], serde_json::json!(["Alice", "Bob"]));
        let eq = v["equilibria"].as_array().unwrap();
        assert_eq!(eq.len(), 2, "got {eq:?}");
        assert_eq!(eq[0]["strategies"], serde_json::json!([0, 0]));
        assert_eq!(
            eq[0]["strategy_names"],
            serde_json::json!(["Opera", "Opera"])
        );
        assert_eq!(eq[1]["strategies"], serde_json::json!([1, 1]));
        assert_eq!(
            eq[1]["strategy_names"],
            serde_json::json!(["Football", "Football"])
        );
    }

    #[test]
    fn the_check_table_covers_every_profile_and_agrees_with_the_answer() {
        let v = run(battle_of_the_sexes("cardinal"));
        let checks = v["checks"].as_array().unwrap();
        assert_eq!(checks.len(), 4, "2x2 game has four profiles");
        let stable: Vec<&serde_json::Value> = checks
            .iter()
            .filter(|c| c["is_equilibrium"] == serde_json::Value::Bool(true))
            .collect();
        assert_eq!(stable.len(), v["equilibria"].as_array().unwrap().len());
        for c in checks {
            let equilibrium = c["is_equilibrium"] == serde_json::Value::Bool(true);
            assert_eq!(
                c["blocking_deviation"].is_null(),
                equilibrium,
                "a blocking deviation must exist exactly when the profile fails: {c}"
            );
        }
    }

    #[test]
    fn a_blocked_profile_names_the_deviation_with_an_exact_gain() {
        let v = run(pd());
        let checks = v["checks"].as_array().unwrap();
        let cc = checks
            .iter()
            .find(|c| c["profile"]["strategies"] == serde_json::json!([0, 0]))
            .expect("mutual cooperation is checked");
        assert_eq!(cc["is_equilibrium"], serde_json::Value::Bool(false));
        let dev = &cc["blocking_deviation"];
        assert_eq!(dev["from_name"], "Cooperate");
        assert_eq!(dev["to_name"], "Defect");
        assert_eq!(dev["gain"]["exact"], "1", "4 - 3");
        assert_eq!(dev["gain"]["approx"], 1.0);
        assert!(dev["player_name"].is_string());
    }

    #[test]
    fn a_game_with_no_pure_equilibrium_returns_an_empty_list_not_an_error() {
        let out = SolvePureNash::invoke(&GtServer::new(), input(matching_pennies())).unwrap();
        // No pure equilibrium is an answer, so `ok` stays true.
        assert_eq!(out.is_error, Some(false));
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        assert!(v["equilibria"].as_array().unwrap().is_empty());
        let checks = v["checks"].as_array().unwrap();
        assert_eq!(checks.len(), 4);
        assert!(checks.iter().all(|c| c["blocking_deviation"].is_object()
            && c["is_equilibrium"] == serde_json::Value::Bool(false)));
    }

    #[test]
    fn three_player_games_are_supported() {
        let v = run(three_player_coordination());
        let eq = v["equilibria"].as_array().unwrap();
        assert_eq!(eq.len(), 2, "everyone A or everyone B: {eq:?}");
        assert_eq!(eq[0]["strategies"], serde_json::json!([0, 0, 0]));
        assert_eq!(eq[1]["strategies"], serde_json::json!([1, 1, 1]));
        assert_eq!(eq[0]["strategy_names"], serde_json::json!(["A", "A", "A"]));
        assert_eq!(v["checks"].as_array().unwrap().len(), 8);
    }

    #[test]
    fn an_ordinal_game_is_solved_too() {
        // Pure Nash compares payoffs rather than averaging them, so it is
        // defined on ranks.
        let v = run(battle_of_the_sexes("ordinal"));
        let eq = v["equilibria"].as_array().unwrap();
        assert_eq!(eq.len(), 2);
        assert_eq!(eq[0]["strategies"], serde_json::json!([0, 0]));
        assert_eq!(eq[1]["strategies"], serde_json::json!([1, 1]));
    }

    #[test]
    fn a_tree_reports_wrong_game_form_rather_than_a_diagnostic() {
        let out = SolvePureNash::invoke(&GtServer::new(), input(entry_tree())).unwrap();
        assert_eq!(out.is_error, Some(true));
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "wrong_game_form");
        assert_eq!(v["expected"], "strategic");
        assert_eq!(v["actual"], "extensive");
    }

    #[test]
    fn a_malformed_game_returns_diagnostics_not_a_protocol_error() {
        let param: PureNashInput = serde_json::from_value(serde_json::json!({
            "game": {
                "form": "strategic",
                "players": [{"id": 0, "name": "Row"}, {"id": 1, "name": "Col"}],
                "strategies": [["C"], ["C"]],
                "outcomes": [{"profile": [0, 0], "payoffs": [1.0]}],
                "payoff_kind": "cardinal"
            }
        }))
        .unwrap();
        let out = SolvePureNash::invoke(&GtServer::new(), param).unwrap();
        assert_eq!(out.is_error, Some(true));
        assert_eq!(value(&out)["code"], "invalid_game");
    }
}
