//! `analyze_repeated_game` -- how patient must players be for a target profile
//! to survive infinite repetition?
//!
//! Source of record: Martin J. Osborne and Ariel Rubinstein, *A Course in Game
//! Theory*, MIT Press, 1994, ch. 8. This is the one tool in the server with no
//! Bonanno chapter behind it, and its description says so rather than claiming
//! an anchor it does not have.
//!
//! The critical discount factor is a *fraction*. Rounding it to a decimal
//! would put the boundary case -- a caller's δ sitting exactly on the
//! threshold -- at the mercy of floating point, so δ crosses the wire as an
//! exact fraction string in both directions.

use crate::server::GtServer;
use crate::wire::game::GameJson;
use crate::wire::number::{parse_rational, Exact};
use crate::wire::outcome::{output_schema_of, RequestError, ToolOutput};
use game_theory_core::analyze::{analyze_repeated_game, Punishment};
use game_theory_core::game::{StrategyId, ValidStrategicGame};
use rmcp::handler::server::router::tool::{SyncTool, ToolBase};
use rmcp::model::{CallToolResult, JsonObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;

/// How deviation is punished. Only what `game-theory-core` implements is offered:
/// grim trigger with reversion to a pure-strategy stage Nash equilibrium,
/// which is what makes the threat credible.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum WirePunishment {
    /// After any deviation, both players revert forever to a pure Nash
    /// equilibrium of the stage game. When the stage game has several, the one
    /// minimizing the sum of payoffs is used -- the harshest credible threat.
    #[default]
    GrimTrigger,
}

impl From<WirePunishment> for Punishment {
    fn from(p: WirePunishment) -> Self {
        match p {
            WirePunishment::GrimTrigger => Punishment::GrimTrigger,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct RepeatedGameInput {
    /// The stage game: exactly 2 players, cardinal payoffs, in matrix or
    /// strategic form. Ordinal payoffs are rejected -- discounted sums over
    /// ranks are meaningless.
    pub game: GameJson,
    /// The profile to sustain, as one strategy index per player. Typically
    /// mutual cooperation.
    pub target: Vec<StrategyId>,
    /// The punishment used to deter deviation. Defaults to grim trigger.
    #[serde(default)]
    pub punishment: WirePunishment,
    /// Optional actual discount factor, as an exact fraction string such as
    /// `"1/2"` or `"9/10"`. Must lie in [0, 1). Decimals are rejected: they
    /// are not exact, and this threshold is a boundary a decimal can land on
    /// the wrong side of. Supply it to be told whether the target holds at it.
    #[serde(default)]
    pub discount_factor: Option<String>,
}

/// One player's sustainability arithmetic.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WirePlayerThreshold {
    pub player: usize,
    pub player_name: String,
    /// Stage payoff under the target profile.
    pub target_payoff: Exact,
    /// The most profitable one-shot deviation, if any pays. Absent when the
    /// target is already this player's best response.
    pub best_deviation: Option<usize>,
    pub best_deviation_label: Option<String>,
    /// Stage payoff from that deviation; equals `target_payoff` when none pays.
    pub deviation_payoff: Exact,
    /// Stage payoff under the punishment profile.
    pub punishment_payoff: Exact,
    /// Pure minmax value, reported for context. The punishment actually used
    /// is Nash reversion, which is credible where minmax generally is not.
    pub minmax: Exact,
    /// Smallest δ sustaining the target for this player. Absent when no δ
    /// below 1 does -- i.e. the punishment pays at least as much as the target.
    pub critical_discount_factor: Option<Exact>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RepeatedGameOutput {
    /// The target profile, as supplied.
    pub target: Vec<usize>,
    /// The target profile's strategy labels, in player order.
    pub target_labels: Vec<String>,
    pub punishment: &'static str,
    /// The stage Nash equilibrium reverted to after a deviation.
    pub punishment_profile: Vec<usize>,
    pub punishment_profile_labels: Vec<String>,
    /// The binding threshold δ*: the maximum over players, as an exact
    /// fraction. Absent when some player's target is unsustainable at any
    /// δ < 1; `note` then says why.
    pub critical_discount_factor: Option<Exact>,
    pub per_player: Vec<WirePlayerThreshold>,
    /// Echo of the caller's δ, when one was supplied.
    pub discount_factor: Option<Exact>,
    /// Whether the target is sustainable at the caller's δ. Absent when no δ
    /// was supplied. The comparison is δ >= δ*, so the threshold itself holds.
    pub sustainable_at: Option<bool>,
    /// Set when there is something to explain that the numbers alone do not.
    pub note: Option<String>,
}

pub struct AnalyzeRepeatedGame;

impl ToolBase for AnalyzeRepeatedGame {
    type Parameter = RepeatedGameInput;
    // CallToolResult rather than the payload, so the tool controls `isError`;
    // output_schema below republishes the payload's schema.
    type Output = CallToolResult;
    type Error = RequestError;

    fn output_schema() -> Option<Arc<JsonObject>> {
        output_schema_of::<ToolOutput<RepeatedGameOutput>>()
    }

    fn name() -> Cow<'static, str> {
        "analyze_repeated_game".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "Given a 2-player cardinal stage game and a target profile to \
             sustain, compute the critical discount factor delta* above which \
             that profile survives infinite repetition as a subgame-perfect \
             equilibrium. Payoffs are discounted sums; the punishment is grim \
             trigger with reversion to a pure-strategy stage Nash equilibrium. \
             Returns delta* as an exact fraction, plus each player's target \
             payoff, best one-shot deviation and what it pays, punishment \
             payoff, minmax value and own threshold. Supply `discount_factor` \
             as a fraction string to be told whether the target holds at it. \
             Unlike the other tools here, this one is NOT anchored in \
             Bonanno's textbook, which does not cover repeated games; its \
             definitions and conventions come from Osborne & Rubinstein, \
             A Course in Game Theory (MIT Press, 1994), ch. 8."
                .into(),
        )
    }
}

impl SyncTool<GtServer> for AnalyzeRepeatedGame {
    fn invoke(_service: &GtServer, param: RepeatedGameInput) -> Result<Self::Output, Self::Error> {
        let RepeatedGameInput {
            game,
            target,
            punishment,
            discount_factor,
        } = param;

        // A δ that is not an exact fraction is a malformed request, not a
        // game the host can fix -- so it is the one case here that becomes a
        // protocol error. A δ that parses but lies outside [0, 1) is a domain
        // error, and game-theory-core raises it.
        let delta = discount_factor
            .as_deref()
            .map(parse_rational)
            .transpose()
            .map_err(RequestError::InvalidParams)?;

        let outcome = game.into_strategic().and_then(|g| {
            analyze_repeated_game(&g, &target, punishment.into(), delta.clone())
                .map(|report| wire_report(&g, report, delta.as_ref()))
        });

        let envelope: ToolOutput<RepeatedGameOutput> = match outcome {
            Ok(v) => ToolOutput::ok(v),
            Err(e) => e.into(),
        };
        Ok(envelope.into_call_tool_result())
    }
}

/// Label a profile's strategies, in player order.
fn labels(game: &ValidStrategicGame, profile: &[StrategyId]) -> Vec<String> {
    profile
        .iter()
        .enumerate()
        .map(|(p, &s)| game.strategy_name(p, s).to_string())
        .collect()
}

fn wire_report(
    game: &ValidStrategicGame,
    report: game_theory_core::analyze::RepeatedGameReport,
    delta: Option<&game_theory_core::Rational>,
) -> RepeatedGameOutput {
    RepeatedGameOutput {
        target_labels: labels(game, &report.target),
        target: report.target.clone(),
        punishment: match report.punishment {
            Punishment::GrimTrigger => "grim_trigger",
        },
        punishment_profile_labels: labels(game, &report.punishment_profile),
        punishment_profile: report.punishment_profile.clone(),
        critical_discount_factor: report.critical_discount_factor.as_ref().map(Exact::from),
        per_player: report
            .per_player
            .iter()
            .map(|t| WirePlayerThreshold {
                player: t.player,
                player_name: game.player_name(t.player).to_string(),
                target_payoff: Exact::from(&t.target_payoff),
                best_deviation: t.best_deviation,
                best_deviation_label: t
                    .best_deviation
                    .map(|s| game.strategy_name(t.player, s).to_string()),
                deviation_payoff: Exact::from(&t.deviation_payoff),
                punishment_payoff: Exact::from(&t.punishment_payoff),
                minmax: Exact::from(&t.minmax),
                critical_discount_factor: t.critical_discount_factor.as_ref().map(Exact::from),
            })
            .collect(),
        discount_factor: delta.map(Exact::from),
        sustainable_at: report.sustainable_at,
        note: report.note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(game: serde_json::Value, target: serde_json::Value) -> RepeatedGameInput {
        serde_json::from_value(serde_json::json!({
            "game": game, "target": target, "punishment": "grim_trigger"
        }))
        .unwrap()
    }

    fn input_with_delta(
        game: serde_json::Value,
        target: serde_json::Value,
        delta: &str,
    ) -> RepeatedGameInput {
        serde_json::from_value(serde_json::json!({
            "game": game,
            "target": target,
            "punishment": "grim_trigger",
            "discount_factor": delta
        }))
        .unwrap()
    }

    /// Prisoner's Dilemma: index 0 = Cooperate, 1 = Defect. c = 3, d = 4,
    /// p = 1, so grim trigger sustains cooperation exactly when
    /// δ >= (4 - 3) / (4 - 1) = 1/3.
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

    /// Matching Pennies. No pure Nash equilibrium, so grim trigger has
    /// nothing credible to revert to.
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

    /// Tools return `CallToolResult`, so assertions read `structuredContent`.
    fn value(out: &CallToolResult) -> serde_json::Value {
        out.structured_content
            .clone()
            .expect("tools return structured content")
    }

    #[test]
    fn the_tool_is_named_and_described() {
        assert_eq!(AnalyzeRepeatedGame::name(), "analyze_repeated_game");
        let description = AnalyzeRepeatedGame::description().expect("described");
        // The one tool with no Bonanno anchor must name its actual source.
        assert!(
            description.contains("Osborne & Rubinstein"),
            "{description}"
        );
        assert!(description.contains("NOT anchored"), "{description}");
        assert!(AnalyzeRepeatedGame::input_schema().is_some());
        // The published output schema must describe the payload, not
        // CallToolResult's own shape.
        let schema = AnalyzeRepeatedGame::output_schema().unwrap();
        let text = serde_json::to_string(&*schema).unwrap();
        assert!(text.contains("critical_discount_factor"), "got {text}");
    }

    #[test]
    fn cooperation_in_the_prisoners_dilemma_needs_delta_at_least_one_third() {
        let out =
            AnalyzeRepeatedGame::invoke(&GtServer::new(), input(pd(), serde_json::json!([0, 0])))
                .unwrap();
        assert_eq!(out.is_error, Some(false));
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        // The exact fraction is authoritative; the decimal is display only.
        assert_eq!(v["critical_discount_factor"]["exact"], "1/3");
        assert_eq!(v["punishment"], "grim_trigger");
        assert_eq!(v["punishment_profile"], serde_json::json!([1, 1]));
        assert_eq!(
            v["punishment_profile_labels"],
            serde_json::json!(["Defect", "Defect"])
        );
        assert_eq!(
            v["target_labels"],
            serde_json::json!(["Cooperate", "Cooperate"])
        );
        assert!(v["sustainable_at"].is_null(), "no δ was supplied");
        assert!(v["discount_factor"].is_null());
        assert!(v["note"].is_null());
    }

    #[test]
    fn the_per_player_breakdown_names_the_deviation_and_its_payoffs() {
        let out =
            AnalyzeRepeatedGame::invoke(&GtServer::new(), input(pd(), serde_json::json!([0, 0])))
                .unwrap();
        let v = value(&out);
        let row = &v["per_player"][0];
        assert_eq!(row["player_name"], "Row");
        assert_eq!(row["target_payoff"]["exact"], "3");
        assert_eq!(row["best_deviation"], 1);
        assert_eq!(row["best_deviation_label"], "Defect");
        assert_eq!(row["deviation_payoff"]["exact"], "4");
        assert_eq!(row["punishment_payoff"]["exact"], "1");
        assert_eq!(row["minmax"]["exact"], "1");
        assert_eq!(row["critical_discount_factor"]["exact"], "1/3");
    }

    #[test]
    fn sustainability_flips_around_the_threshold() {
        let above = AnalyzeRepeatedGame::invoke(
            &GtServer::new(),
            input_with_delta(pd(), serde_json::json!([0, 0]), "1/2"),
        )
        .unwrap();
        let v = value(&above);
        assert_eq!(v["sustainable_at"], serde_json::Value::Bool(true));
        assert_eq!(v["discount_factor"]["exact"], "1/2");

        let below = AnalyzeRepeatedGame::invoke(
            &GtServer::new(),
            input_with_delta(pd(), serde_json::json!([0, 0]), "1/4"),
        )
        .unwrap();
        assert_eq!(
            value(&below)["sustainable_at"],
            serde_json::Value::Bool(false)
        );

        // The threshold itself holds -- this is why δ must be exact.
        let exactly = AnalyzeRepeatedGame::invoke(
            &GtServer::new(),
            input_with_delta(pd(), serde_json::json!([0, 0]), "1/3"),
        )
        .unwrap();
        assert_eq!(
            value(&exactly)["sustainable_at"],
            serde_json::Value::Bool(true)
        );
    }

    #[test]
    fn a_target_that_is_already_a_stage_equilibrium_needs_no_patience() {
        let out =
            AnalyzeRepeatedGame::invoke(&GtServer::new(), input(pd(), serde_json::json!([1, 1])))
                .unwrap();
        let v = value(&out);
        assert_eq!(v["critical_discount_factor"]["exact"], "0");
        assert!(v["per_player"][0]["best_deviation"].is_null());
        assert!(v["per_player"][0]["best_deviation_label"].is_null());
    }

    #[test]
    fn an_unsustainable_target_reports_no_threshold_and_says_why() {
        // B strictly dominates A for Row, so reversion to (B, R) pays Row more
        // than the target does. No δ below 1 deters anything.
        let g = serde_json::json!({
            "form": "matrix",
            "players": ["Row", "Col"],
            "row_strategies": ["A", "B"],
            "col_strategies": ["L", "R"],
            "payoff_matrix": [[[1.0, 1.0], [0.0, 0.0]], [[5.0, 0.0], [5.0, 2.0]]],
            "payoff_kind": "cardinal"
        });
        let out =
            AnalyzeRepeatedGame::invoke(&GtServer::new(), input(g, serde_json::json!([0, 0])))
                .unwrap();
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        assert!(v["critical_discount_factor"].is_null());
        // game-theory-core's explanation must reach the caller, not be swallowed.
        assert!(v["note"].as_str().unwrap().contains("punishment"), "{v}");
    }

    #[test]
    fn a_three_player_game_is_refused() {
        let g = serde_json::json!({
            "form": "strategic",
            "players": [
                {"id": 0, "name": "P0"}, {"id": 1, "name": "P1"}, {"id": 2, "name": "P2"}
            ],
            "strategies": [["C", "D"], ["C", "D"], ["C", "D"]],
            "outcomes": [
                {"profile": [0, 0, 0], "payoffs": [3.0, 3.0, 3.0]},
                {"profile": [0, 0, 1], "payoffs": [0.0, 0.0, 4.0]},
                {"profile": [0, 1, 0], "payoffs": [0.0, 4.0, 0.0]},
                {"profile": [0, 1, 1], "payoffs": [0.0, 1.0, 1.0]},
                {"profile": [1, 0, 0], "payoffs": [4.0, 0.0, 0.0]},
                {"profile": [1, 0, 1], "payoffs": [1.0, 0.0, 1.0]},
                {"profile": [1, 1, 0], "payoffs": [1.0, 1.0, 0.0]},
                {"profile": [1, 1, 1], "payoffs": [1.0, 1.0, 1.0]}
            ],
            "payoff_kind": "cardinal"
        });
        let out =
            AnalyzeRepeatedGame::invoke(&GtServer::new(), input(g, serde_json::json!([0, 0, 0])))
                .unwrap();
        assert_eq!(out.is_error, Some(true));
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["players"], 3);
    }

    #[test]
    fn ordinal_payoffs_are_rejected() {
        let mut g = pd();
        g["payoff_kind"] = serde_json::json!("ordinal");
        let out =
            AnalyzeRepeatedGame::invoke(&GtServer::new(), input(g, serde_json::json!([0, 0])))
                .unwrap();
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "ordinal_payoffs_rejected");
        assert_eq!(v["tool"], "analyze_repeated_game");
    }

    #[test]
    fn a_stage_game_with_no_pure_nash_equilibrium_is_refused() {
        let out = AnalyzeRepeatedGame::invoke(
            &GtServer::new(),
            input(matching_pennies(), serde_json::json!([0, 0])),
        )
        .unwrap();
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "no_pure_nash_for_punishment");
    }

    #[test]
    fn a_discount_factor_outside_the_unit_interval_is_a_domain_error() {
        let out = AnalyzeRepeatedGame::invoke(
            &GtServer::new(),
            input_with_delta(pd(), serde_json::json!([0, 0]), "1"),
        )
        .unwrap();
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "invalid_discount_factor");
        assert_eq!(v["value"], "1");

        let negative = AnalyzeRepeatedGame::invoke(
            &GtServer::new(),
            input_with_delta(pd(), serde_json::json!([0, 0]), "-1/2"),
        )
        .unwrap();
        assert_eq!(value(&negative)["code"], "invalid_discount_factor");
    }

    #[test]
    fn a_decimal_discount_factor_is_a_protocol_error() {
        let err = AnalyzeRepeatedGame::invoke(
            &GtServer::new(),
            input_with_delta(pd(), serde_json::json!([0, 0]), "0.5"),
        )
        .unwrap_err();
        let RequestError::InvalidParams(m) = err;
        assert!(m.contains("0.5"), "got {m}");
        assert!(m.contains("fraction"), "got {m}");
    }

    #[test]
    fn a_target_naming_a_nonexistent_strategy_is_a_domain_error() {
        let out =
            AnalyzeRepeatedGame::invoke(&GtServer::new(), input(pd(), serde_json::json!([9, 0])))
                .unwrap();
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "unknown_profile");
    }

    #[test]
    fn an_extensive_game_reports_wrong_game_form() {
        let tree = serde_json::json!({
            "form": "extensive",
            "players": [{"id": 0, "name": "A"}, {"id": 1, "name": "B"}],
            "root": 0,
            "nodes": [
                {"kind": "decision", "player": 0, "actions": [["In", 1], ["Out", 2]]},
                {"kind": "decision", "player": 1, "actions": [["L", 3], ["R", 4]]},
                {"kind": "terminal", "payoffs": [0.0, 2.0]},
                {"kind": "terminal", "payoffs": [-1.0, -1.0]},
                {"kind": "terminal", "payoffs": [1.0, 1.0]}
            ],
            "information_sets": [[0], [1]],
            "payoff_kind": "cardinal"
        });
        let out =
            AnalyzeRepeatedGame::invoke(&GtServer::new(), input(tree, serde_json::json!([0, 0])))
                .unwrap();
        let v = value(&out);
        assert_eq!(v["code"], "wrong_game_form");
        assert_eq!(v["actual"], "extensive");
    }

    #[test]
    fn punishment_defaults_to_grim_trigger_when_omitted() {
        let param: RepeatedGameInput = serde_json::from_value(serde_json::json!({
            "game": pd(), "target": [0, 0]
        }))
        .unwrap();
        let v = value(&AnalyzeRepeatedGame::invoke(&GtServer::new(), param).unwrap());
        assert_eq!(v["punishment"], "grim_trigger");
        assert_eq!(v["critical_discount_factor"]["exact"], "1/3");
    }
}
