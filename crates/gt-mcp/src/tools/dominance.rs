//! `solve_dominance` -- iterated deletion of dominated strategies.
//!
//! The arithmetic is entirely `gt_core::solve::dominance`. This layer maps the
//! wire mode onto `DominanceMode`, attaches names to every index the solver
//! reports, and renders the mixed dominator's probabilities as exact fractions.
//!
//! `mode: both` runs the two relations as two separate analyses rather than
//! merging them: iterated strict and iterated weak deletion are different
//! procedures with different guarantees, and a single merged surviving set
//! would hide which one produced which deletion.

use crate::server::GtServer;
use crate::wire::game::GameJson;
use crate::wire::number::Exact;
use crate::wire::outcome::{output_schema_of, RequestError, ToolOutput};
use gt_core::game::ValidStrategicGame;
use gt_core::solve::{solve_dominance, DominanceMode, DominanceResult, Dominator};
use num_traits::Zero;
use rmcp::handler::server::router::tool::{SyncTool, ToolBase};
use rmcp::model::{CallToolResult, JsonObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;

/// Which dominance relation to iterate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum WireDominanceMode {
    /// Delete only strictly dominated strategies. Order-independent, so the
    /// surviving set is *the* reduction. On cardinal games this also tests
    /// dominance by mixed strategies.
    #[default]
    Strict,
    /// Delete weakly dominated strategies. Order-dependent: the answer is one
    /// valid reduction, not the reduction.
    Weak,
    /// Run both procedures and return one analysis each, strict first.
    Both,
}

impl WireDominanceMode {
    fn label(self) -> &'static str {
        match self {
            WireDominanceMode::Strict => "strict",
            WireDominanceMode::Weak => "weak",
            WireDominanceMode::Both => "both",
        }
    }

    /// The core modes this request runs, in order.
    fn core_modes(self) -> Vec<DominanceMode> {
        match self {
            WireDominanceMode::Strict => vec![DominanceMode::Strict],
            WireDominanceMode::Weak => vec![DominanceMode::Weak],
            WireDominanceMode::Both => vec![DominanceMode::Strict, DominanceMode::Weak],
        }
    }
}

fn mode_label(m: DominanceMode) -> &'static str {
    match m {
        DominanceMode::Strict => "strict",
        DominanceMode::Weak => "weak",
    }
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct DominanceInput {
    /// The game, in matrix or strategic form. An extensive-form game is
    /// rejected as `wrong_game_form`; convert it first.
    pub game: GameJson,
    /// Which dominance relation to iterate. Defaults to `strict`.
    #[serde(default)]
    pub mode: WireDominanceMode,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DominanceOutput {
    /// The mode requested: `strict`, `weak`, or `both`.
    pub mode: &'static str,
    /// One analysis per relation run: a single entry for `strict` or `weak`,
    /// two for `both` (strict first, then weak).
    pub analyses: Vec<DominanceAnalysis>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DominanceAnalysis {
    /// The relation this analysis iterated: `strict` or `weak`.
    pub mode: &'static str,
    /// Surviving strategies, one entry per player, in player order.
    pub surviving: Vec<SurvivingSet>,
    /// Every deletion, in the round it happened.
    pub eliminations: Vec<WireElimination>,
    /// Set when exactly one strategy survives for every player.
    pub unique_profile: Option<WireProfile>,
    /// True for weak deletion: a different elimination order may leave a
    /// different set of strategies standing.
    pub order_dependent: bool,
    /// Present exactly when `order_dependent` is true.
    pub warning: Option<String>,
    /// True when dominance by mixed strategies was tested. False for ordinal
    /// payoffs, for weak mode, and when the reduced game was too large -- the
    /// result is then pure dominance only.
    pub mixed_dominance_checked: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SurvivingSet {
    pub player: usize,
    pub player_name: String,
    /// Surviving strategy indices, ascending.
    pub strategies: Vec<usize>,
    /// The same strategies by name, index-aligned with `strategies`.
    pub strategy_names: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WireElimination {
    /// 1-based pass over the players in which this deletion happened.
    pub round: usize,
    pub player: usize,
    pub player_name: String,
    /// Index of the deleted strategy.
    pub eliminated: usize,
    pub eliminated_name: String,
    /// What beat it.
    pub dominated_by: WireDominator,
    /// The relation applied: `strict` or `weak`.
    pub mode: &'static str,
}

/// The dominating strategy, pure or mixed.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireDominator {
    Pure {
        strategy: usize,
        strategy_name: String,
    },
    /// A mixture over the player's own strategies. Only the strategies
    /// carrying positive probability are listed; they sum to exactly 1.
    Mixed {
        probabilities: Vec<MixtureComponent>,
    },
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct MixtureComponent {
    pub strategy: usize,
    pub strategy_name: String,
    pub probability: Exact,
}

/// A strategy profile, by index and by name.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireProfile {
    /// `strategies[p]` is player p's strategy index.
    pub strategies: Vec<usize>,
    /// The same profile by name, index-aligned with `strategies`.
    pub strategy_names: Vec<String>,
}

const ORDER_DEPENDENCE_WARNING: &str =
    "iterated deletion of weakly dominated strategies is order-dependent: a \
     different elimination order can leave a different set of strategies \
     standing. This is one valid reduction, not the reduction.";

pub struct SolveDominance;

impl ToolBase for SolveDominance {
    type Parameter = DominanceInput;
    // CallToolResult rather than the payload, so the tool controls `isError`;
    // output_schema below republishes the payload's schema.
    type Output = CallToolResult;
    type Error = RequestError;

    fn output_schema() -> Option<Arc<JsonObject>> {
        output_schema_of::<ToolOutput<DominanceOutput>>()
    }

    fn name() -> Cow<'static, str> {
        "solve_dominance".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "Iteratively delete dominated strategies from a matrix or strategic \
             game. Returns the surviving strategies per player, the per-round \
             elimination log naming what dominated what, and the unique \
             surviving profile when there is one. `mode` is strict (default), \
             weak, or both. Strict deletion is order-independent; weak deletion \
             is not, and its result carries an explicit order-dependence \
             warning. On cardinal games strict deletion also tests dominance by \
             mixed strategies, reporting the dominating mixture as exact \
             probabilities; on ordinal games only pure dominance is defined."
                .into(),
        )
    }
}

impl SyncTool<GtServer> for SolveDominance {
    fn invoke(_service: &GtServer, param: DominanceInput) -> Result<Self::Output, Self::Error> {
        let DominanceInput { game, mode } = param;

        // A tree has no strategic-form strategies to delete; `into_strategic`
        // reports that as WrongGameForm rather than as a validation problem.
        let outcome = game.into_strategic().map(|g| DominanceOutput {
            mode: mode.label(),
            analyses: mode
                .core_modes()
                .into_iter()
                .map(|m| analysis(&g, m, solve_dominance(&g, m)))
                .collect(),
        });

        let envelope: ToolOutput<DominanceOutput> = match outcome {
            Ok(v) => ToolOutput::ok(v),
            Err(e) => e.into(),
        };
        Ok(envelope.into_call_tool_result())
    }
}

fn analysis(
    game: &ValidStrategicGame,
    mode: DominanceMode,
    result: DominanceResult,
) -> DominanceAnalysis {
    DominanceAnalysis {
        mode: mode_label(mode),
        surviving: result
            .surviving
            .iter()
            .enumerate()
            .map(|(p, strategies)| SurvivingSet {
                player: p,
                player_name: game.player_name(p).to_string(),
                strategies: strategies.clone(),
                strategy_names: strategies
                    .iter()
                    .map(|&s| game.strategy_name(p, s).to_string())
                    .collect(),
            })
            .collect(),
        eliminations: result
            .steps
            .iter()
            .map(|step| WireElimination {
                round: step.round,
                player: step.player,
                player_name: game.player_name(step.player).to_string(),
                eliminated: step.eliminated,
                eliminated_name: game.strategy_name(step.player, step.eliminated).to_string(),
                dominated_by: dominator(game, step.player, &step.dominated_by),
                mode: mode_label(step.mode),
            })
            .collect(),
        unique_profile: result.unique_profile.as_ref().map(|p| profile(game, p)),
        order_dependent: result.order_dependent,
        warning: result
            .order_dependent
            .then(|| ORDER_DEPENDENCE_WARNING.to_string()),
        mixed_dominance_checked: result.mixed_dominance_checked,
    }
}

fn dominator(game: &ValidStrategicGame, player: usize, d: &Dominator) -> WireDominator {
    match d {
        Dominator::Pure(s) => WireDominator::Pure {
            strategy: *s,
            strategy_name: game.strategy_name(player, *s).to_string(),
        },
        Dominator::Mixed(probs) => WireDominator::Mixed {
            probabilities: probs
                .iter()
                .enumerate()
                // Zero-weight strategies are not part of the mixture; listing
                // them would bury the support in noise.
                .filter(|(_, p)| !p.is_zero())
                .map(|(s, p)| MixtureComponent {
                    strategy: s,
                    strategy_name: game.strategy_name(player, s).to_string(),
                    probability: Exact::from(p),
                })
                .collect(),
        },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn input(game: serde_json::Value, mode: &str) -> DominanceInput {
        serde_json::from_value(serde_json::json!({"game": game, "mode": mode})).unwrap()
    }

    /// Prisoner's Dilemma. Defect strictly dominates Cooperate for both
    /// players, so iterated strict deletion leaves (Defect, Defect) alone --
    /// Bonanno §1.2 (p. 14) and §1.5 (p. 28), the strict-dominance and
    /// iterated-deletion sections. The payoff numbers encode the standard
    /// ranking of the four outcomes.
    fn pd(kind: &str) -> serde_json::Value {
        serde_json::json!({
            "form": "matrix",
            "players": ["Row", "Col"],
            "row_strategies": ["Cooperate", "Defect"],
            "col_strategies": ["Cooperate", "Defect"],
            "payoff_matrix": [[[3.0, 3.0], [0.0, 4.0]], [[4.0, 0.0], [1.0, 1.0]]],
            "payoff_kind": kind
        })
    }

    /// Row's C is beaten by no pure strategy -- A wins against L, B against R
    /// -- but the even mixture of A and B gives 3/2 in both columns against
    /// C's 1. Bonanno §5.4 (p. 201) is the concept; these payoff numbers were
    /// constructed for this repository.
    fn mixed_dominance_game() -> serde_json::Value {
        serde_json::json!({
            "form": "matrix",
            "players": ["Row", "Col"],
            "row_strategies": ["A", "B", "C"],
            "col_strategies": ["L", "R"],
            "payoff_matrix": [
                [[3.0, 0.0], [0.0, 0.0]],
                [[0.0, 0.0], [3.0, 0.0]],
                [[1.0, 0.0], [1.0, 0.0]]
            ],
            "payoff_kind": "cardinal"
        })
    }

    /// T weakly dominates B: equal against L, strictly better against R.
    fn weakly_dominated_game() -> serde_json::Value {
        serde_json::json!({
            "form": "matrix",
            "players": ["Row", "Col"],
            "row_strategies": ["T", "B"],
            "col_strategies": ["L", "R"],
            "payoff_matrix": [[[1.0, 0.0], [2.0, 0.0]], [[1.0, 0.0], [0.0, 0.0]]],
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

    fn run(game: serde_json::Value, mode: &str) -> serde_json::Value {
        let out = SolveDominance::invoke(&GtServer::new(), input(game, mode)).unwrap();
        assert_eq!(out.is_error, Some(false));
        value(&out)
    }

    #[test]
    fn the_tool_is_named_and_described() {
        assert_eq!(SolveDominance::name(), "solve_dominance");
        assert!(SolveDominance::description().is_some());
        assert!(SolveDominance::input_schema().is_some());
        // The published output schema must describe the payload, not
        // CallToolResult's own shape.
        let schema = SolveDominance::output_schema().unwrap();
        let text = serde_json::to_string(&*schema).unwrap();
        assert!(text.contains("mixed_dominance_checked"), "got {text}");
        assert!(text.contains("unique_profile"), "got {text}");
    }

    #[test]
    fn the_mode_defaults_to_strict() {
        let param: DominanceInput =
            serde_json::from_value(serde_json::json!({"game": pd("cardinal")})).unwrap();
        assert_eq!(param.mode, WireDominanceMode::Strict);
    }

    #[test]
    fn strict_deletion_solves_the_prisoners_dilemma() {
        let v = run(pd("cardinal"), "strict");
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        assert_eq!(v["mode"], "strict");
        let a = &v["analyses"][0];
        assert_eq!(a["mode"], "strict");
        assert_eq!(a["unique_profile"]["strategies"], serde_json::json!([1, 1]));
        assert_eq!(
            a["unique_profile"]["strategy_names"],
            serde_json::json!(["Defect", "Defect"])
        );
        assert_eq!(a["order_dependent"], serde_json::Value::Bool(false));
        assert_eq!(a["warning"], serde_json::Value::Null);
    }

    #[test]
    fn the_elimination_log_names_players_and_strategies() {
        let v = run(pd("cardinal"), "strict");
        let steps = v["analyses"][0]["eliminations"].as_array().unwrap();
        assert_eq!(steps.len(), 2, "one deletion per player, got {steps:?}");
        let first = &steps[0];
        assert_eq!(first["round"], 1);
        assert_eq!(first["player_name"], "Row");
        assert_eq!(first["eliminated_name"], "Cooperate");
        assert_eq!(first["mode"], "strict");
        assert_eq!(first["dominated_by"]["kind"], "pure");
        assert_eq!(first["dominated_by"]["strategy_name"], "Defect");
    }

    #[test]
    fn surviving_sets_carry_both_indices_and_names() {
        let v = run(pd("cardinal"), "strict");
        let surviving = &v["analyses"][0]["surviving"];
        assert_eq!(surviving[0]["player_name"], "Row");
        assert_eq!(surviving[0]["strategies"], serde_json::json!([1]));
        assert_eq!(
            surviving[0]["strategy_names"],
            serde_json::json!(["Defect"])
        );
        assert_eq!(surviving[1]["player_name"], "Col");
    }

    #[test]
    fn a_dominating_mixture_is_reported_with_exact_probabilities() {
        let v = run(mixed_dominance_game(), "strict");
        let a = &v["analyses"][0];
        assert_eq!(a["mixed_dominance_checked"], serde_json::Value::Bool(true));
        let step = a["eliminations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["player"] == 0 && s["eliminated_name"] == "C")
            .expect("C is eliminated by a mixture")
            .clone();
        assert_eq!(step["dominated_by"]["kind"], "mixed");
        let parts = step["dominated_by"]["probabilities"].as_array().unwrap();
        assert_eq!(parts.len(), 2, "the support is A and B: {parts:?}");
        for part in parts {
            assert_eq!(part["probability"]["exact"], "1/2");
            assert!(part["probability"]["approx"].is_number());
            assert_ne!(
                part["strategy_name"], "C",
                "the candidate carries no weight"
            );
        }
    }

    #[test]
    fn weak_deletion_warns_that_the_reduction_is_order_dependent() {
        let v = run(weakly_dominated_game(), "weak");
        let a = &v["analyses"][0];
        assert_eq!(a["mode"], "weak");
        assert_eq!(a["order_dependent"], serde_json::Value::Bool(true));
        let warning = a["warning"].as_str().expect("a warning is attached");
        assert!(warning.contains("order-dependent"), "got {warning}");
        assert!(warning.contains("not the reduction"), "got {warning}");
        // Weak dominance by mixtures is not defined here, so the flag must not
        // claim a check that did not happen.
        assert_eq!(a["mixed_dominance_checked"], serde_json::Value::Bool(false));
        assert!(a["eliminations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["player"] == 0 && s["eliminated_name"] == "B"));
    }

    #[test]
    fn both_returns_one_analysis_per_relation_strict_first() {
        let v = run(weakly_dominated_game(), "both");
        assert_eq!(v["mode"], "both");
        let analyses = v["analyses"].as_array().unwrap();
        assert_eq!(analyses.len(), 2);
        assert_eq!(analyses[0]["mode"], "strict");
        assert_eq!(analyses[1]["mode"], "weak");
        assert!(analyses[0]["eliminations"].as_array().unwrap().is_empty());
        assert!(!analyses[1]["eliminations"].as_array().unwrap().is_empty());
        assert_eq!(
            analyses[0]["order_dependent"],
            serde_json::Value::Bool(false)
        );
        assert_eq!(
            analyses[1]["order_dependent"],
            serde_json::Value::Bool(true)
        );
    }

    #[test]
    fn an_ordinal_game_gets_pure_dominance_only_and_still_solves() {
        // Dominance is an ordinal notion -- no expectation is taken -- so the
        // Prisoner's Dilemma reduces the same way on ranks.
        let v = run(pd("ordinal"), "strict");
        let a = &v["analyses"][0];
        assert_eq!(a["unique_profile"]["strategies"], serde_json::json!([1, 1]));
        assert_eq!(
            a["mixed_dominance_checked"],
            serde_json::Value::Bool(false),
            "expected utility over ranks is meaningless"
        );
    }

    #[test]
    fn a_tree_reports_wrong_game_form_rather_than_a_diagnostic() {
        let out = SolveDominance::invoke(&GtServer::new(), input(entry_tree(), "strict")).unwrap();
        assert_eq!(out.is_error, Some(true));
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "wrong_game_form");
        assert_eq!(v["expected"], "strategic");
        assert_eq!(v["actual"], "extensive");
    }

    #[test]
    fn a_malformed_game_returns_diagnostics_not_a_protocol_error() {
        let param: DominanceInput = serde_json::from_value(serde_json::json!({
            "game": {
                "form": "strategic",
                "players": [{"id": 0, "name": "Row"}, {"id": 1, "name": "Col"}],
                "strategies": [["C"], ["C"]],
                "outcomes": [{"profile": [0, 0], "payoffs": [1.0]}],
                "payoff_kind": "cardinal"
            }
        }))
        .unwrap();
        let out = SolveDominance::invoke(&GtServer::new(), param).unwrap();
        assert_eq!(out.is_error, Some(true));
        assert_eq!(value(&out)["code"], "invalid_game");
    }
}
