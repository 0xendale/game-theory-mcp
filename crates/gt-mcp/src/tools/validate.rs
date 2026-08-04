//! `validate_game` -- normalize and check a caller-supplied game.
//!
//! Every other tool runs this internally. Exposing it separately lets the host
//! LLM check its formalization before committing to an analysis.

use crate::server::GtServer;
use crate::wire::game::GameJson;
use crate::wire::outcome::{output_schema_of, RequestError, ToolOutput};
use rmcp::handler::server::router::tool::{SyncTool, ToolBase};
use rmcp::model::{CallToolResult, JsonObject};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct ValidateGameInput {
    /// The game to check, in matrix, strategic, or extensive form.
    pub game: GameJson,
}

/// A summary of a game that passed validation.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ValidateGameOutput {
    Strategic {
        form: &'static str,
        n_players: usize,
        player_names: Vec<String>,
        strategy_counts: Vec<usize>,
        payoff_kind: &'static str,
    },
    Extensive {
        form: &'static str,
        n_players: usize,
        player_names: Vec<String>,
        n_nodes: usize,
        perfect_information: bool,
        payoff_kind: &'static str,
    },
}

fn kind_name(k: gt_core::game::PayoffKind) -> &'static str {
    match k {
        gt_core::game::PayoffKind::Ordinal => "ordinal",
        gt_core::game::PayoffKind::Cardinal => "cardinal",
    }
}

pub struct ValidateGame;

impl ToolBase for ValidateGame {
    type Parameter = ValidateGameInput;
    // CallToolResult rather than the payload, so the tool controls `isError`;
    // output_schema below republishes the payload's schema.
    type Output = CallToolResult;
    type Error = RequestError;

    fn output_schema() -> Option<Arc<JsonObject>> {
        output_schema_of::<ToolOutput<ValidateGameOutput>>()
    }

    fn name() -> Cow<'static, str> {
        "validate_game".into()
    }

    fn description() -> Option<Cow<'static, str>> {
        Some(
            "Normalize and check a game before analysing it. Accepts matrix \
             (2-player), strategic (any number of players), or extensive (tree) \
             form. Returns a summary on success, or every validation problem \
             found -- not just the first."
                .into(),
        )
    }
}

impl SyncTool<GtServer> for ValidateGame {
    fn invoke(_service: &GtServer, param: ValidateGameInput) -> Result<Self::Output, Self::Error> {
        let form = param.game.form_name();
        // A tree validates as a tree; the other two forms validate as strategic.
        let out = if form == "extensive" {
            param
                .game
                .into_extensive()
                .map(|g| ValidateGameOutput::Extensive {
                    form: "extensive",
                    n_players: g.n_players(),
                    player_names: g.players().iter().map(|p| p.name.clone()).collect(),
                    n_nodes: g.nodes().len(),
                    perfect_information: g.is_perfect_information(),
                    payoff_kind: kind_name(g.payoff_kind()),
                })
        } else {
            param
                .game
                .into_strategic()
                .map(|g| ValidateGameOutput::Strategic {
                    form,
                    n_players: g.n_players(),
                    player_names: (0..g.n_players())
                        .map(|p| g.player_name(p).to_string())
                        .collect(),
                    strategy_counts: (0..g.n_players()).map(|p| g.n_strategies(p)).collect(),
                    payoff_kind: kind_name(g.payoff_kind()),
                })
        };

        let envelope: ToolOutput<ValidateGameOutput> = match out {
            Ok(summary) => ToolOutput::ok(summary),
            Err(e) => e.into(),
        };
        Ok(envelope.into_call_tool_result())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pd_input() -> ValidateGameInput {
        serde_json::from_value(serde_json::json!({
            "game": {
                "form": "matrix",
                "players": ["Row", "Col"],
                "row_strategies": ["Cooperate", "Defect"],
                "col_strategies": ["Cooperate", "Defect"],
                "payoff_matrix": [[[3.0, 3.0], [0.0, 4.0]], [[4.0, 0.0], [1.0, 1.0]]],
                "payoff_kind": "cardinal"
            }
        }))
        .unwrap()
    }

    /// Tools return `CallToolResult`, so assertions read `structuredContent`.
    fn payload(r: &CallToolResult) -> &serde_json::Value {
        r.structured_content
            .as_ref()
            .expect("tools return structured content")
    }

    #[test]
    fn the_tool_is_named_and_described() {
        assert_eq!(ValidateGame::name(), "validate_game");
        assert!(ValidateGame::description().is_some());
        assert!(ValidateGame::input_schema().is_some());
        // The published output schema must describe the payload, not
        // CallToolResult's own shape.
        let schema = ValidateGame::output_schema().unwrap();
        let text = serde_json::to_string(&*schema).unwrap();
        assert!(text.contains("n_players"), "got {text}");
    }

    #[test]
    fn a_well_formed_matrix_game_validates() {
        let out = ValidateGame::invoke(&GtServer::new(), pd_input()).unwrap();
        assert_eq!(out.is_error, Some(false));
        let v = payload(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        assert_eq!(v["form"], "matrix");
        assert_eq!(v["n_players"], 2);
        assert_eq!(v["strategy_counts"], serde_json::json!([2, 2]));
        assert_eq!(v["payoff_kind"], "cardinal");
    }

    #[test]
    fn a_well_formed_tree_validates() {
        let input: ValidateGameInput = serde_json::from_value(serde_json::json!({
            "game": {
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
            }
        }))
        .unwrap();
        let out = ValidateGame::invoke(&GtServer::new(), input).unwrap();
        let v = payload(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(true));
        assert_eq!(v["form"], "extensive");
        assert_eq!(v["n_nodes"], 5);
        assert_eq!(v["perfect_information"], serde_json::Value::Bool(true));
    }

    #[test]
    fn a_malformed_game_returns_diagnostics_not_a_protocol_error() {
        let input: ValidateGameInput = serde_json::from_value(serde_json::json!({
            "game": {
                "form": "strategic",
                "players": [{"id": 0, "name": "Row"}, {"id": 1, "name": "Col"}],
                "strategies": [["C"], ["C"]],
                "outcomes": [{"profile": [0, 0], "payoffs": [1.0]}],
                "payoff_kind": "cardinal"
            }
        }))
        .unwrap();
        // Ok at the protocol level; the failure is in the payload.
        let out = ValidateGame::invoke(&GtServer::new(), input).unwrap();
        assert_eq!(out.is_error, Some(true));
        let v = payload(&out);
        assert_eq!(v["ok"], serde_json::Value::Bool(false));
        assert_eq!(v["code"], "invalid_game");
        assert!(!v["diagnostics"].as_array().unwrap().is_empty());
    }

    #[test]
    fn the_router_registers_the_tool() {
        let router = GtServer::tool_router();
        let names: Vec<String> = router
            .list_all()
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        assert!(
            names.contains(&"validate_game".to_string()),
            "got {names:?}"
        );
    }
}
