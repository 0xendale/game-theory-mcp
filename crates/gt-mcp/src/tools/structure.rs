//! `analyze_payoff_structure` -- what shape a game has, independent of any one
//! solution concept, plus its match against the named archetypes.
//!
//! Both halves of `gt-core`'s analysis come back from one call: a host asking
//! "is this a prisoner's dilemma?" also wants the Pareto frontier and the
//! efficiency gap that make the answer mean something, and splitting them
//! would be two round trips over one traversal of the same game.
//!
//! Ordinal payoffs are rejected outright. The structure report adds payoffs
//! across players (welfare, constant-sum detection) and the archetype
//! classifier reads `is_zero_sum` back out of it, so no half of this tool
//! survives ranks -- a sum of ranks is not a quantity. Answering with the
//! ordinal-safe fields only would mean silently dropping most of the payload.

use crate::server::GtServer;
use crate::wire::game::GameJson;
use crate::wire::number::Exact;
use crate::wire::outcome::{output_schema_of, RequestError, ToolOutput};
use gt_core::analyze::{analyze_structure, classify, Archetype, StructureReport};
use gt_core::error::GtError;
use gt_core::game::{Profile, ValidStrategicGame};
use rmcp::handler::server::router::tool::{SyncTool, ToolBase};
use rmcp::model::{CallToolResult, JsonObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;

/// The tool's name, also what it reports when it refuses ordinal payoffs.
const TOOL: &str = "analyze_payoff_structure";

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct StructureInput {
    /// The game, in matrix or strategic form, with cardinal payoffs. A tree is
    /// rejected with `wrong_game_form`: this analysis is defined over strategy
    /// profiles, so convert the tree with `convert_form` first.
    pub game: GameJson,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StructureOutput {
    /// Shape of the payoffs: efficiency, conflict, and what each player can
    /// guarantee themselves.
    pub structure: WireStructure,
    /// Which named pattern the game matches, and the criteria behind it.
    pub archetype: WireArchetype,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WireStructure {
    /// Profiles that no other profile Pareto-dominates.
    pub pareto_frontier: Vec<WireProfile>,
    /// Payoffs sum to zero at every profile.
    pub is_zero_sum: bool,
    /// Payoffs sum to the same value at every profile. Zero-sum implies this.
    pub is_constant_sum: bool,
    /// That common sum, present exactly when `is_constant_sum`.
    pub sum_constant: Option<Exact>,
    /// Per player, the best payoff they can guarantee themselves.
    pub security_levels: Vec<WireSecurityLevel>,
    /// Profiles maximizing the sum of payoffs.
    pub welfare_optima: Vec<WireProfile>,
    /// That maximum sum.
    pub welfare_value: Exact,
    /// Pure equilibria that some other profile leaves nobody worse off at and
    /// somebody strictly better off at. This is the prisoner's-dilemma
    /// signature.
    pub dominated_equilibria: Vec<WireDominatedEquilibrium>,
    /// Welfare at the optimum minus welfare at the best pure equilibrium.
    /// Absent when the game has no pure equilibrium.
    pub efficiency_gap: Option<Exact>,
}

/// A strategy profile with the names and payoffs it stands for, so the caller
/// need not index back into the game to read a result.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireProfile {
    /// `profile[p]` is player p's strategy index.
    pub profile: Vec<usize>,
    /// `strategy_names[p]` is the name of `profile[p]`.
    pub strategy_names: Vec<String>,
    /// `payoffs[p]` is player p's payoff at this profile.
    pub payoffs: Vec<Exact>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WireSecurityLevel {
    pub player: usize,
    pub player_name: String,
    /// The maxmin value: what this player gets whatever the others do.
    pub value: Exact,
    /// The strategy index that guarantees it.
    pub maxmin_strategy: usize,
    pub maxmin_strategy_name: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WireDominatedEquilibrium {
    /// A pure Nash equilibrium.
    pub equilibrium: WireProfile,
    /// A profile that Pareto-dominates it.
    pub dominated_by: WireProfile,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WireArchetype {
    /// One of `prisoners_dilemma`, `stag_hunt`, `chicken`,
    /// `battle_of_the_sexes`, `matching_pennies`, or `none_matched`. The
    /// classifier never picks the nearest label: a game that fails a criterion
    /// is `none_matched`, and `criteria_failed` says which criterion.
    pub archetype: &'static str,
    /// Criteria the game satisfies, in the order they were checked.
    pub criteria_met: Vec<String>,
    /// Criteria the game fails, each naming the archetype it rules out.
    pub criteria_failed: Vec<String>,
}

fn archetype_name(a: Archetype) -> &'static str {
    match a {
        Archetype::PrisonersDilemma => "prisoners_dilemma",
        Archetype::StagHunt => "stag_hunt",
        Archetype::Chicken => "chicken",
        Archetype::BattleOfTheSexes => "battle_of_the_sexes",
        Archetype::MatchingPennies => "matching_pennies",
        Archetype::Unclassified => "none_matched",
    }
}

fn wire_profile(game: &ValidStrategicGame, profile: &Profile) -> WireProfile {
    WireProfile {
        profile: profile.clone(),
        strategy_names: profile
            .iter()
            .enumerate()
            .map(|(p, &s)| game.strategy_name(p, s).to_string())
            .collect(),
        payoffs: game.payoffs_at(profile).iter().map(Exact::from).collect(),
    }
}

fn wire_structure(game: &ValidStrategicGame, report: StructureReport) -> WireStructure {
    WireStructure {
        pareto_frontier: report
            .pareto_frontier
            .iter()
            .map(|p| wire_profile(game, p))
            .collect(),
        is_zero_sum: report.is_zero_sum,
        is_constant_sum: report.is_constant_sum,
        sum_constant: report.sum_constant.as_ref().map(Exact::from),
        security_levels: report
            .security_levels
            .iter()
            .map(|s| WireSecurityLevel {
                player: s.player,
                player_name: game.player_name(s.player).to_string(),
                value: Exact::from(&s.value),
                maxmin_strategy: s.maxmin_strategy,
                maxmin_strategy_name: game.strategy_name(s.player, s.maxmin_strategy).to_string(),
            })
            .collect(),
        welfare_optima: report
            .welfare_optima
            .iter()
            .map(|p| wire_profile(game, p))
            .collect(),
        welfare_value: Exact::from(&report.welfare_value),
        dominated_equilibria: report
            .dominated_equilibria
            .iter()
            .map(|d| WireDominatedEquilibrium {
                equilibrium: wire_profile(game, &d.equilibrium),
                dominated_by: wire_profile(game, &d.dominated_by),
            })
            .collect(),
        efficiency_gap: report.efficiency_gap.as_ref().map(Exact::from),
    }
}

pub struct AnalyzePayoffStructure;

impl ToolBase for AnalyzePayoffStructure {
    type Parameter = StructureInput;
    // CallToolResult rather than the payload, so the tool controls `isError`;
    // output_schema below republishes the payload's schema.
    type Output = CallToolResult;
    type Error = RequestError;

    fn output_schema() -> Option<Arc<JsonObject>> {
        output_schema_of::<ToolOutput<StructureOutput>>()
    }

    fn name() -> Cow<'static, str> {
        TOOL.into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "Describe the shape of a strategic game's payoffs and classify it \
             against the named archetypes, in one call. Returns the Pareto \
             frontier, zero-sum and constant-sum detection, each player's \
             maxmin security level and the strategy that guarantees it, the \
             welfare-maximizing profiles, any pure equilibrium that another \
             profile Pareto-dominates, and the efficiency gap between the best \
             equilibrium and the welfare optimum. The archetype half reports \
             prisoners_dilemma, stag_hunt, chicken, battle_of_the_sexes, \
             matching_pennies, or none_matched -- always with the criteria met \
             and the criteria failed, so the label can be checked rather than \
             trusted, and never a nearest guess. The prisoner's-dilemma \
             criterion applies at any number of players; the other four are 2x2 \
             patterns. Needs cardinal payoffs: welfare and constant-sum \
             detection add payoffs across players, which is meaningless over \
             ranks."
                .into(),
        )
    }
}

impl SyncTool<GtServer> for AnalyzePayoffStructure {
    fn invoke(_service: &GtServer, param: StructureInput) -> Result<Self::Output, Self::Error> {
        let envelope: ToolOutput<StructureOutput> = match analyze(param.game) {
            Ok(v) => ToolOutput::ok(v),
            Err(e) => e.into(),
        };
        Ok(envelope.into_call_tool_result())
    }
}

fn analyze(game: GameJson) -> Result<StructureOutput, GtError> {
    let game = game.into_strategic()?;
    game.require_cardinal(TOOL)?;
    let archetype = classify(&game);
    Ok(StructureOutput {
        structure: wire_structure(&game, analyze_structure(&game)),
        archetype: WireArchetype {
            archetype: archetype_name(archetype.archetype),
            criteria_met: archetype.criteria_met,
            criteria_failed: archetype.criteria_failed,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x2 game in matrix form. `m[row][col] == [u_row, u_col]`.
    fn matrix(rows: [&str; 2], cols: [&str; 2], m: [[[f64; 2]; 2]; 2]) -> serde_json::Value {
        serde_json::json!({
            "form": "matrix",
            "players": ["Row", "Col"],
            "row_strategies": rows,
            "col_strategies": cols,
            "payoff_matrix": m,
            "payoff_kind": "cardinal"
        })
    }

    fn run(game: serde_json::Value) -> serde_json::Value {
        let param: StructureInput = serde_json::from_value(serde_json::json!({ "game": game }))
            .expect("the fixture must match the input schema");
        AnalyzePayoffStructure::invoke(&GtServer::new(), param)
            .unwrap()
            .structured_content
            .expect("tools return structured content")
    }

    /// Prisoner's Dilemma. Bonanno, *Game Theory* (2015), §1.6 "Nash
    /// equilibrium", p. 32 -- a dominant-strategy profile that another profile
    /// Pareto-dominates.
    fn prisoners_dilemma() -> serde_json::Value {
        matrix(
            ["Cooperate", "Defect"],
            ["Cooperate", "Defect"],
            [[[3.0, 3.0], [0.0, 4.0]], [[4.0, 0.0], [1.0, 1.0]]],
        )
    }

    /// Matching Pennies. Bonanno, *Game Theory* (2015), §5.3 "Computing the
    /// mixed-strategy Nash equilibria", p. 195 -- zero-sum, no pure
    /// equilibrium.
    fn matching_pennies() -> serde_json::Value {
        matrix(
            ["Heads", "Tails"],
            ["Heads", "Tails"],
            [[[1.0, -1.0], [-1.0, 1.0]], [[-1.0, 1.0], [1.0, -1.0]]],
        )
    }

    /// Stag Hunt. Not a named example in Bonanno; this is the standard
    /// coordination matrix satisfying the design's criterion -- two pure
    /// equilibria, one Pareto-dominating the other, no dominant strategy.
    fn stag_hunt() -> serde_json::Value {
        matrix(
            ["Stag", "Hare"],
            ["Stag", "Hare"],
            [[[4.0, 4.0], [0.0, 3.0]], [[3.0, 0.0], [3.0, 3.0]]],
        )
    }

    /// Chicken / Hawk-Dove. Not a named example in Bonanno; standard matrix
    /// meeting the design's criterion -- two off-diagonal equilibria, with
    /// mutual aggression worst for both.
    fn chicken() -> serde_json::Value {
        matrix(
            ["Swerve", "Straight"],
            ["Swerve", "Straight"],
            [[[3.0, 3.0], [2.0, 4.0]], [[4.0, 2.0], [1.0, 1.0]]],
        )
    }

    /// Battle of the Sexes. Not a named example in Bonanno; standard matrix
    /// meeting the design's criterion -- two coordination equilibria that the
    /// players rank oppositely.
    fn battle_of_the_sexes() -> serde_json::Value {
        matrix(
            ["Opera", "Football"],
            ["Opera", "Football"],
            [[[2.0, 1.0], [0.0, 0.0]], [[0.0, 0.0], [1.0, 2.0]]],
        )
    }

    /// Three players, two strategies each, with `payoffs` computed per profile.
    fn three_player(payoffs: impl Fn(usize, usize, usize) -> [f64; 3]) -> serde_json::Value {
        let mut outcomes = Vec::new();
        for a in 0..2usize {
            for b in 0..2usize {
                for c in 0..2usize {
                    outcomes.push(serde_json::json!({
                        "profile": [a, b, c], "payoffs": payoffs(a, b, c)
                    }));
                }
            }
        }
        serde_json::json!({
            "form": "strategic",
            "players": [
                {"id": 0, "name": "P0"}, {"id": 1, "name": "P1"}, {"id": 2, "name": "P2"}
            ],
            "strategies": [["L", "R"], ["L", "R"], ["L", "R"]],
            "outcomes": outcomes,
            "payoff_kind": "cardinal"
        })
    }

    /// Three-player public-goods game: contributing (L) costs 2 and gives every
    /// player 1. Defecting is dominant; universal contribution is better for
    /// all. A prisoner's dilemma with no 2x2 shape.
    fn three_player_public_goods() -> serde_json::Value {
        three_player(|a, b, c| {
            let contributors = [a, b, c].iter().filter(|&&x| x == 0).count() as f64;
            let payoff = |own: usize| contributors - if own == 0 { 2.0 } else { 0.0 };
            [payoff(a), payoff(b), payoff(c)]
        })
    }

    /// Entry game, Bonanno, *Game Theory* (2015), Figure 2.9, p. 77.
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

    fn criteria_met(v: &serde_json::Value) -> &Vec<serde_json::Value> {
        v["archetype"]["criteria_met"].as_array().unwrap()
    }

    #[test]
    fn the_tool_is_named_and_described() {
        assert_eq!(AnalyzePayoffStructure::name(), "analyze_payoff_structure");
        assert!(AnalyzePayoffStructure::description().is_some());
        assert!(AnalyzePayoffStructure::input_schema().is_some());
        // The published output schema must describe the payload, not
        // CallToolResult's own shape.
        let schema = AnalyzePayoffStructure::output_schema().unwrap();
        let text = serde_json::to_string(&*schema).unwrap();
        assert!(text.contains("pareto_frontier"), "got {text}");
        assert!(text.contains("criteria_failed"), "got {text}");
    }

    #[test]
    fn the_prisoners_dilemma_structure_is_reported_in_full() {
        let v = run(prisoners_dilemma());
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        let s = &v["structure"];

        // Mutual defection is the one profile off the frontier.
        let frontier = s["pareto_frontier"].as_array().unwrap();
        assert_eq!(frontier.len(), 3);
        assert!(!frontier
            .iter()
            .any(|p| p["profile"] == serde_json::json!([1, 1])));

        assert_eq!(s["is_zero_sum"], serde_json::Value::Bool(false));
        assert_eq!(s["is_constant_sum"], serde_json::Value::Bool(false));
        assert!(s["sum_constant"].is_null());

        assert_eq!(s["welfare_value"]["exact"], "6");
        assert_eq!(s["welfare_optima"][0]["profile"], serde_json::json!([0, 0]));
        assert_eq!(
            s["welfare_optima"][0]["strategy_names"],
            serde_json::json!(["Cooperate", "Cooperate"])
        );
        assert_eq!(s["efficiency_gap"]["exact"], "4");

        // The dilemma itself: the equilibrium, and the profile that beats it.
        let d = &s["dominated_equilibria"][0];
        assert_eq!(
            d["equilibrium"]["strategy_names"],
            serde_json::json!(["Defect", "Defect"])
        );
        assert_eq!(
            d["dominated_by"]["strategy_names"],
            serde_json::json!(["Cooperate", "Cooperate"])
        );
        assert_eq!(d["equilibrium"]["payoffs"][0]["exact"], "1");

        // Defecting guarantees 1; cooperating guarantees 0.
        let row = &s["security_levels"][0];
        assert_eq!(row["player_name"], "Row");
        assert_eq!(row["value"]["exact"], "1");
        assert_eq!(row["maxmin_strategy_name"], "Defect");
    }

    #[test]
    fn the_prisoners_dilemma_is_classified_with_its_criteria() {
        let v = run(prisoners_dilemma());
        assert_eq!(v["archetype"]["archetype"], "prisoners_dilemma");
        assert!(!criteria_met(&v).is_empty());
    }

    #[test]
    fn the_stag_hunt_is_classified_with_its_criteria() {
        let v = run(stag_hunt());
        assert_eq!(v["archetype"]["archetype"], "stag_hunt");
        assert!(!criteria_met(&v).is_empty());
    }

    #[test]
    fn chicken_is_classified_with_its_criteria() {
        let v = run(chicken());
        assert_eq!(v["archetype"]["archetype"], "chicken");
        assert!(!criteria_met(&v).is_empty());
    }

    #[test]
    fn battle_of_the_sexes_is_classified_with_its_criteria() {
        let v = run(battle_of_the_sexes());
        assert_eq!(v["archetype"]["archetype"], "battle_of_the_sexes");
        assert!(!criteria_met(&v).is_empty());
    }

    #[test]
    fn matching_pennies_is_classified_with_its_criteria() {
        let v = run(matching_pennies());
        assert_eq!(v["archetype"]["archetype"], "matching_pennies");
        assert!(!criteria_met(&v).is_empty());
    }

    #[test]
    fn a_zero_sum_game_is_detected_with_exact_security_levels() {
        let v = run(matching_pennies());
        let s = &v["structure"];
        assert_eq!(s["is_zero_sum"], serde_json::Value::Bool(true));
        assert_eq!(s["is_constant_sum"], serde_json::Value::Bool(true));
        assert_eq!(s["sum_constant"]["exact"], "0");
        // Either coin guarantees -1, so that is what each player can secure.
        assert_eq!(s["security_levels"][0]["value"]["exact"], "-1");
        assert_eq!(s["security_levels"][1]["value"]["exact"], "-1");
        assert_eq!(s["welfare_value"]["exact"], "0");
        // No pure equilibrium, so no gap to measure.
        assert!(s["efficiency_gap"].is_null());
        assert!(s["dominated_equilibria"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_constant_sum_game_that_is_not_zero_sum_reports_the_constant() {
        let v = run(matrix(
            ["r0", "r1"],
            ["c0", "c1"],
            [[[3.0, 7.0], [6.0, 4.0]], [[8.0, 2.0], [1.0, 9.0]]],
        ));
        let s = &v["structure"];
        assert_eq!(s["is_constant_sum"], serde_json::Value::Bool(true));
        assert_eq!(s["is_zero_sum"], serde_json::Value::Bool(false));
        assert_eq!(s["sum_constant"]["exact"], "10");
    }

    #[test]
    fn a_game_matching_nothing_says_so_rather_than_guessing() {
        // Every payoff identical: no pattern applies, and that is a result,
        // not a failure.
        let v = run(matrix(
            ["r0", "r1"],
            ["c0", "c1"],
            [[[1.0, 1.0], [1.0, 1.0]], [[1.0, 1.0], [1.0, 1.0]]],
        ));
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        assert_eq!(v["archetype"]["archetype"], "none_matched");
        assert!(
            !v["archetype"]["criteria_failed"]
                .as_array()
                .unwrap()
                .is_empty(),
            "must say why nothing matched: {v}"
        );
    }

    #[test]
    fn the_prisoners_dilemma_criterion_generalizes_past_two_by_two() {
        let v = run(three_player_public_goods());
        assert_eq!(v["archetype"]["archetype"], "prisoners_dilemma");
        assert!(!criteria_met(&v).is_empty());
        // Structure is reported at three players too.
        assert_eq!(
            v["structure"]["security_levels"].as_array().unwrap().len(),
            3
        );
        // Universal contribution pays each player 1.
        assert_eq!(v["structure"]["welfare_value"]["exact"], "3");
    }

    #[test]
    fn the_other_four_archetypes_are_two_by_two_only() {
        // Three players, every payoff equal: nothing to match, and the four
        // 2x2 patterns are not even reachable.
        let v = run(three_player(|_, _, _| [0.0, 0.0, 0.0]));
        assert_eq!(v["archetype"]["archetype"], "none_matched");
        let failed = v["archetype"]["criteria_failed"].as_array().unwrap();
        assert!(
            failed.iter().any(|c| c.as_str().unwrap().contains("2x2")),
            "the non-2x2 shape must be the stated reason: {failed:?}"
        );
    }

    #[test]
    fn a_tree_reports_wrong_game_form() {
        let v = run(entry_tree());
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "wrong_game_form");
        assert_eq!(v["expected"], "strategic");
        assert_eq!(v["actual"], "extensive");
    }

    #[test]
    fn an_ordinal_game_is_rejected_rather_than_partly_answered() {
        let mut g = prisoners_dilemma();
        g["payoff_kind"] = serde_json::json!("ordinal");
        let v = run(g);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "ordinal_payoffs_rejected");
        assert_eq!(v["tool"], "analyze_payoff_structure");
    }

    #[test]
    fn a_malformed_game_returns_diagnostics_not_a_protocol_error() {
        let v = run(serde_json::json!({
            "form": "strategic",
            "players": [{"id": 0, "name": "Row"}, {"id": 1, "name": "Col"}],
            "strategies": [["C"], ["C"]],
            "outcomes": [{"profile": [0, 0], "payoffs": [1.0]}],
            "payoff_kind": "cardinal"
        }));
        assert_eq!(v["code"], "invalid_game");
        assert!(!v["diagnostics"].as_array().unwrap().is_empty());
    }
}
