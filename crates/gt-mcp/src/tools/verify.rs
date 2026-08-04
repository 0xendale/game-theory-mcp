//! `verify_equilibrium` -- check a claimed equilibrium and, if it fails, name
//! the deviation that refutes it.
//!
//! One tool, four concepts, three `gt-core` functions. `gt-core` deliberately
//! keeps `verify_equilibrium` to pure profiles: a mixed profile is a list of
//! distributions and an SPE profile is a set of tree plans, so widening that
//! signature would force every pure-Nash caller to build degenerate mixtures.
//! Routing is this layer's job.

use crate::server::GtServer;
use crate::wire::game::GameJson;
use crate::wire::number::{parse_rational, Exact};
use crate::wire::outcome::{output_schema_of, RequestError, ToolOutput};
use gt_core::error::GtError;
use gt_core::game::{NodeId, StrategyId};
use gt_core::solve::{verify_equilibrium, verify_mixed_nash, verify_spe, Concept, MixedStrategy};
use rmcp::handler::server::router::tool::{SyncTool, ToolBase};
use rmcp::model::{CallToolResult, JsonObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;

/// The concept, and the profile shape it implies. Adjacently tagged so an
/// illegal pairing is not representable.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "concept", content = "profile", rename_all = "snake_case")]
pub enum ClaimedProfile {
    /// `profile[p]` is player p's chosen strategy index.
    PureNash(Vec<StrategyId>),
    /// `profile[p]` is player p's chosen strategy index.
    DominantStrategy(Vec<StrategyId>),
    /// `profile[p]` is player p's distribution over their own strategies, as
    /// exact fractions like `"1/3"`. Each must sum to exactly 1.
    MixedNash(Vec<Vec<String>>),
    /// `profile[p]` is player p's plan: `[node_id, action_index]` at every
    /// decision node of p, including nodes unreachable given p's own choices.
    Spe(Vec<Vec<(NodeId, StrategyId)>>),
}

impl Default for ClaimedProfile {
    fn default() -> Self {
        ClaimedProfile::PureNash(Vec::new())
    }
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct VerifyInput {
    /// The game. Strategic or matrix form for pure_nash, dominant_strategy and
    /// mixed_nash; extensive form for spe.
    pub game: GameJson,
    #[serde(flatten)]
    pub claim: ClaimedProfile,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct VerifyOutput {
    /// Whether the claimed profile is an equilibrium under `concept`.
    pub holds: bool,
    pub concept: &'static str,
    pub evidence: Evidence,
    /// Set when the concept fails for a reason other than a deviation.
    pub note: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Evidence {
    Pure {
        deviations: Vec<WireDeviation>,
    },
    MixedNash {
        expected_payoffs: Vec<Exact>,
        support_violations: Vec<WireSupportViolation>,
        deviations: Vec<WireMixedDeviation>,
    },
    Spe {
        deviation: Option<WireSpeDeviation>,
    },
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WireDeviation {
    pub player: usize,
    pub from: usize,
    pub to: usize,
    pub gain: Exact,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WireMixedDeviation {
    pub player: usize,
    pub to: usize,
    pub gain: Exact,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WireSupportViolation {
    pub player: usize,
    pub strategy: usize,
    pub shortfall: Exact,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WireSpeDeviation {
    pub node: usize,
    pub player: usize,
    pub from_action: usize,
    pub to_action: usize,
    pub gain: Exact,
}

pub struct VerifyEquilibrium;

impl ToolBase for VerifyEquilibrium {
    type Parameter = VerifyInput;
    // CallToolResult rather than the payload, so the tool controls `isError`;
    // output_schema below republishes the payload's schema.
    type Output = CallToolResult;
    type Error = RequestError;

    fn output_schema() -> Option<Arc<JsonObject>> {
        output_schema_of::<ToolOutput<VerifyOutput>>()
    }

    fn name() -> Cow<'static, str> {
        "verify_equilibrium".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "Check a claimed equilibrium and, if it does not hold, name the \
             profitable deviation that refutes it. `concept` fixes the shape of \
             `profile`: pure_nash and dominant_strategy take one strategy index \
             per player; mixed_nash takes one probability distribution per \
             player as exact fractions such as \"1/2\"; spe takes one plan per \
             player, each an array of [node_id, action_index] pairs covering \
             every decision node of that player -- including nodes unreachable \
             given that player's own earlier choices. pure_nash, \
             dominant_strategy and mixed_nash need a strategic or matrix game; \
             spe needs an extensive one."
                .into(),
        )
    }
}

impl SyncTool<GtServer> for VerifyEquilibrium {
    fn invoke(_service: &GtServer, param: VerifyInput) -> Result<Self::Output, Self::Error> {
        let VerifyInput { game, claim } = param;

        let outcome = match claim {
            ClaimedProfile::PureNash(profile) => {
                run_pure(game, &profile, Concept::PureNash, "pure_nash")
            }
            ClaimedProfile::DominantStrategy(profile) => run_pure(
                game,
                &profile,
                Concept::DominantStrategy,
                "dominant_strategy",
            ),
            ClaimedProfile::MixedNash(rows) => {
                // A probability that is not an exact fraction is a malformed
                // request, not a game the host can fix -- so it is the one
                // case here that becomes a protocol error.
                let mut mixtures = Vec::with_capacity(rows.len());
                for row in &rows {
                    let mut probs = Vec::with_capacity(row.len());
                    for s in row {
                        probs.push(parse_rational(s).map_err(RequestError::InvalidParams)?);
                    }
                    mixtures.push(MixedStrategy { probs });
                }
                game.into_strategic()
                    .and_then(|g| verify_mixed_nash(&g, &mixtures))
                    .map(|r| VerifyOutput {
                        holds: r.holds,
                        concept: "mixed_nash",
                        evidence: Evidence::MixedNash {
                            expected_payoffs: r.expected_payoffs.iter().map(Exact::from).collect(),
                            support_violations: r
                                .support_violations
                                .iter()
                                .map(|v| WireSupportViolation {
                                    player: v.player,
                                    strategy: v.strategy,
                                    shortfall: Exact::from(&v.shortfall),
                                })
                                .collect(),
                            deviations: r
                                .deviations
                                .iter()
                                .map(|d| WireMixedDeviation {
                                    player: d.player,
                                    to: d.to,
                                    gain: Exact::from(&d.gain),
                                })
                                .collect(),
                        },
                        note: r.note,
                    })
            }
            ClaimedProfile::Spe(plans) => game
                .into_extensive()
                .and_then(|g| verify_spe(&g, &plans))
                .map(|r| VerifyOutput {
                    holds: r.holds,
                    concept: "spe",
                    evidence: Evidence::Spe {
                        deviation: r.deviation.as_ref().map(|d| WireSpeDeviation {
                            node: d.node,
                            player: d.player,
                            from_action: d.from_action,
                            to_action: d.to_action,
                            gain: Exact::from(&d.gain),
                        }),
                    },
                    note: None,
                }),
        };

        let envelope: ToolOutput<VerifyOutput> = match outcome {
            Ok(v) => ToolOutput::ok(v),
            Err(e) => e.into(),
        };
        Ok(envelope.into_call_tool_result())
    }
}

fn run_pure(
    game: GameJson,
    profile: &[StrategyId],
    concept: Concept,
    label: &'static str,
) -> Result<VerifyOutput, GtError> {
    let g = game.into_strategic()?;
    let r = verify_equilibrium(&g, profile, concept)?;
    Ok(VerifyOutput {
        holds: r.holds,
        concept: label,
        evidence: Evidence::Pure {
            deviations: r
                .deviations
                .iter()
                .map(|d| WireDeviation {
                    player: d.player,
                    from: d.from,
                    to: d.to,
                    gain: Exact::from(&d.gain),
                })
                .collect(),
        },
        note: r.note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(game: serde_json::Value, concept: &str, profile: serde_json::Value) -> VerifyInput {
        serde_json::from_value(serde_json::json!({
            "game": game, "concept": concept, "profile": profile
        }))
        .unwrap()
    }

    /// Prisoner's Dilemma. (Defect, Defect) is the unique equilibrium.
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

    /// Entry game, Bonanno p. 77. SPE: Entrant enters, Incumbent accommodates.
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

    /// Tools return `CallToolResult`, so assertions read `structuredContent`.
    fn value(out: &CallToolResult) -> serde_json::Value {
        out.structured_content
            .clone()
            .expect("tools return structured content")
    }

    #[test]
    fn the_tool_is_named_and_described() {
        assert_eq!(VerifyEquilibrium::name(), "verify_equilibrium");
        assert!(VerifyEquilibrium::description().is_some());
    }

    #[test]
    fn pure_nash_holds_at_mutual_defection() {
        let out = VerifyEquilibrium::invoke(
            &GtServer::new(),
            input(pd(), "pure_nash", serde_json::json!([1, 1])),
        )
        .unwrap();
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        assert_eq!(v["holds"], serde_json::Value::Bool(true));
        assert_eq!(v["concept"], "pure_nash");
    }

    #[test]
    fn pure_nash_fails_at_mutual_cooperation_and_names_the_deviation() {
        let out = VerifyEquilibrium::invoke(
            &GtServer::new(),
            input(pd(), "pure_nash", serde_json::json!([0, 0])),
        )
        .unwrap();
        let v = value(&out);
        assert_eq!(v["holds"], serde_json::Value::Bool(false));
        let devs = v["evidence"]["deviations"].as_array().unwrap();
        assert_eq!(devs.len(), 2, "both players can deviate profitably");
        assert_eq!(devs[0]["to"], 1);
        assert_eq!(devs[0]["gain"]["exact"], "1");
    }

    #[test]
    fn dominant_strategy_holds_at_mutual_defection() {
        let out = VerifyEquilibrium::invoke(
            &GtServer::new(),
            input(pd(), "dominant_strategy", serde_json::json!([1, 1])),
        )
        .unwrap();
        assert_eq!(value(&out)["holds"], serde_json::Value::Bool(true));
    }

    #[test]
    fn mixed_nash_holds_at_the_half_half_mixture() {
        let out = VerifyEquilibrium::invoke(
            &GtServer::new(),
            input(
                matching_pennies(),
                "mixed_nash",
                serde_json::json!([["1/2", "1/2"], ["1/2", "1/2"]]),
            ),
        )
        .unwrap();
        let v = value(&out);
        assert_eq!(v["holds"], serde_json::Value::Bool(true));
        assert_eq!(v["evidence"]["expected_payoffs"][0]["exact"], "0");
    }

    #[test]
    fn mixed_nash_fails_at_a_lopsided_mixture() {
        let out = VerifyEquilibrium::invoke(
            &GtServer::new(),
            input(
                matching_pennies(),
                "mixed_nash",
                serde_json::json!([["1/3", "2/3"], ["1/2", "1/2"]]),
            ),
        )
        .unwrap();
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        assert_eq!(v["holds"], serde_json::Value::Bool(false));
    }

    #[test]
    fn a_mixture_that_does_not_sum_to_one_is_a_domain_error() {
        let out = VerifyEquilibrium::invoke(
            &GtServer::new(),
            input(
                matching_pennies(),
                "mixed_nash",
                serde_json::json!([["1/3", "1/3"], ["1/2", "1/2"]]),
            ),
        )
        .unwrap();
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "invalid_mixed_strategy");
    }

    #[test]
    fn a_decimal_probability_is_a_protocol_error() {
        let err = VerifyEquilibrium::invoke(
            &GtServer::new(),
            input(
                matching_pennies(),
                "mixed_nash",
                serde_json::json!([["0.5", "0.5"], ["1/2", "1/2"]]),
            ),
        )
        .unwrap_err();
        let RequestError::InvalidParams(m) = err;
        assert!(m.contains("0.5"), "got {m}");
    }

    #[test]
    fn spe_holds_at_the_backward_induction_solution() {
        let out = VerifyEquilibrium::invoke(
            &GtServer::new(),
            input(entry(), "spe", serde_json::json!([[[0, 0]], [[1, 1]]])),
        )
        .unwrap();
        let v = value(&out);
        assert_eq!(v["holds"], serde_json::Value::Bool(true));
        assert_eq!(v["concept"], "spe");
    }

    #[test]
    fn spe_catches_the_non_credible_threat() {
        // Incumbent threatens to Fight; Entrant stays Out. Not subgame perfect.
        let out = VerifyEquilibrium::invoke(
            &GtServer::new(),
            input(entry(), "spe", serde_json::json!([[[0, 1]], [[1, 0]]])),
        )
        .unwrap();
        let v = value(&out);
        assert_eq!(v["holds"], serde_json::Value::Bool(false));
        assert_eq!(v["evidence"]["deviation"]["node"], 1);
    }

    #[test]
    fn a_malformed_plan_is_a_domain_error_not_a_panic() {
        let out = VerifyEquilibrium::invoke(
            &GtServer::new(),
            input(entry(), "spe", serde_json::json!([[[0, 0]], []])),
        )
        .unwrap();
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "invalid_plan_profile");
    }

    #[test]
    fn a_tree_sent_to_pure_nash_reports_wrong_game_form() {
        let out = VerifyEquilibrium::invoke(
            &GtServer::new(),
            input(entry(), "pure_nash", serde_json::json!([0, 0])),
        )
        .unwrap();
        let v = value(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "wrong_game_form");
        assert!(v["suggestion"].as_str().unwrap().contains("convert_form"));
    }

    #[test]
    fn a_matrix_sent_to_spe_reports_wrong_game_form() {
        let out = VerifyEquilibrium::invoke(
            &GtServer::new(),
            input(pd(), "spe", serde_json::json!([[[0, 0]], [[1, 1]]])),
        )
        .unwrap();
        assert_eq!(value(&out)["code"], "wrong_game_form");
    }

    #[test]
    fn an_ordinal_game_is_rejected_for_mixed_nash() {
        let mut g = matching_pennies();
        g["payoff_kind"] = serde_json::json!("ordinal");
        let out = VerifyEquilibrium::invoke(
            &GtServer::new(),
            input(
                g,
                "mixed_nash",
                serde_json::json!([["1/2", "1/2"], ["1/2", "1/2"]]),
            ),
        )
        .unwrap();
        assert_eq!(value(&out)["code"], "ordinal_payoffs_rejected");
    }

    #[test]
    fn the_router_registers_both_tools() {
        let names: Vec<String> = GtServer::tool_router()
            .list_all()
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            names.contains(&"validate_game".to_string()),
            "got {names:?}"
        );
        assert!(
            names.contains(&"verify_equilibrium".to_string()),
            "got {names:?}"
        );
    }
}
