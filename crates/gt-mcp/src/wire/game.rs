//! The game input union.
//!
//! Three forms, matching what `gt-core` already accepts: `matrix` is the
//! ergonomic 2-player path, `strategic` the n-player canonical form, and
//! `extensive` the tree. A form that cannot serve the requested concept comes
//! back as `WrongGameForm` rather than being converted -- `to_strategic` exists,
//! but converting would silently redefine what the caller's profile indices mean.

use gt_core::error::GtError;
use gt_core::game::{
    ExtensiveGame, MatrixForm, Node, NodeId, Outcome, PayoffKind, Player, StrategicGame,
    ValidExtensiveGame, ValidStrategicGame,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "form", rename_all = "lowercase")]
pub enum GameJson {
    /// A 2-player game as a matrix. `payoff_matrix[row][col] == [u_row, u_col]`.
    Matrix {
        players: [String; 2],
        row_strategies: Vec<String>,
        col_strategies: Vec<String>,
        payoff_matrix: Vec<Vec<[f64; 2]>>,
        payoff_kind: WirePayoffKind,
    },
    /// Any number of players, one outcome per strategy profile.
    Strategic {
        players: Vec<WirePlayer>,
        strategies: Vec<Vec<String>>,
        outcomes: Vec<WireOutcome>,
        payoff_kind: WirePayoffKind,
    },
    /// A game tree.
    Extensive {
        players: Vec<WirePlayer>,
        root: NodeId,
        nodes: Vec<WireNode>,
        information_sets: Vec<Vec<NodeId>>,
        payoff_kind: WirePayoffKind,
    },
}

impl Default for GameJson {
    fn default() -> Self {
        // rmcp's ToolBase::Parameter requires Default. This value is never
        // used: rmcp only falls back to it for tools with no input schema,
        // and every tool here has one.
        GameJson::Matrix {
            players: ["Row".into(), "Col".into()],
            row_strategies: Vec::new(),
            col_strategies: Vec::new(),
            payoff_matrix: Vec::new(),
            payoff_kind: WirePayoffKind::Cardinal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum WirePayoffKind {
    /// Ranks. Expectation-taking tools reject these.
    Ordinal,
    /// Utilities. Required by any tool that takes an expectation.
    #[default]
    Cardinal,
}

impl From<WirePayoffKind> for PayoffKind {
    fn from(k: WirePayoffKind) -> Self {
        match k {
            WirePayoffKind::Ordinal => PayoffKind::Ordinal,
            WirePayoffKind::Cardinal => PayoffKind::Cardinal,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WirePlayer {
    pub id: usize,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WireOutcome {
    pub profile: Vec<usize>,
    pub payoffs: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum WireNode {
    /// `actions[i]` is the i-th action's label and the child it leads to.
    /// The index `i` is that action's id at this node.
    Decision {
        player: usize,
        actions: Vec<(String, NodeId)>,
    },
    /// One payoff per player, in player order.
    Terminal { payoffs: Vec<f64> },
}

impl GameJson {
    /// The form tag, for `WrongGameForm` diagnostics.
    pub fn form_name(&self) -> &'static str {
        match self {
            GameJson::Matrix { .. } => "matrix",
            GameJson::Strategic { .. } => "strategic",
            GameJson::Extensive { .. } => "extensive",
        }
    }

    /// Validate as a strategic-form game.
    pub fn into_strategic(self) -> Result<ValidStrategicGame, GtError> {
        let raw = match self {
            GameJson::Matrix {
                players,
                row_strategies,
                col_strategies,
                payoff_matrix,
                payoff_kind,
            } => StrategicGame::try_from(MatrixForm {
                players,
                row_strategies,
                col_strategies,
                payoff_matrix,
                payoff_kind: payoff_kind.into(),
            })?,
            GameJson::Strategic {
                players,
                strategies,
                outcomes,
                payoff_kind,
            } => StrategicGame {
                players: players
                    .into_iter()
                    .map(|p| Player {
                        id: p.id,
                        name: p.name,
                    })
                    .collect(),
                strategies,
                outcomes: outcomes
                    .into_iter()
                    .map(|o| Outcome {
                        profile: o.profile,
                        payoffs: o.payoffs,
                    })
                    .collect(),
                payoff_kind: payoff_kind.into(),
            },
            other => {
                return Err(GtError::WrongGameForm {
                    expected: "strategic",
                    actual: other.form_name(),
                })
            }
        };
        ValidStrategicGame::validate(raw)
    }

    /// Validate as an extensive-form game.
    pub fn into_extensive(self) -> Result<ValidExtensiveGame, GtError> {
        let GameJson::Extensive {
            players,
            root,
            nodes,
            information_sets,
            payoff_kind,
        } = self
        else {
            let actual = self.form_name();
            return Err(GtError::WrongGameForm {
                expected: "extensive",
                actual,
            });
        };
        ValidExtensiveGame::validate(ExtensiveGame {
            players: players
                .into_iter()
                .map(|p| Player {
                    id: p.id,
                    name: p.name,
                })
                .collect(),
            root,
            nodes: nodes
                .into_iter()
                .map(|n| match n {
                    WireNode::Decision { player, actions } => Node::Decision { player, actions },
                    WireNode::Terminal { payoffs } => Node::Terminal { payoffs },
                })
                .collect(),
            information_sets,
            payoff_kind: payoff_kind.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prisoner's Dilemma in matrix form.
    fn pd() -> GameJson {
        serde_json::from_value(serde_json::json!({
            "form": "matrix",
            "players": ["Row", "Col"],
            "row_strategies": ["Cooperate", "Defect"],
            "col_strategies": ["Cooperate", "Defect"],
            "payoff_matrix": [[[3.0, 3.0], [0.0, 4.0]], [[4.0, 0.0], [1.0, 1.0]]],
            "payoff_kind": "cardinal"
        }))
        .unwrap()
    }

    /// Entry game, Bonanno p. 77: Entrant moves, then Incumbent.
    fn entry() -> GameJson {
        serde_json::from_value(serde_json::json!({
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
        }))
        .unwrap()
    }

    #[test]
    fn a_matrix_game_becomes_a_validated_strategic_game() {
        let g = pd().into_strategic().unwrap();
        assert_eq!(g.n_players(), 2);
        assert_eq!(g.n_strategies(0), 2);
        assert_eq!(g.strategy_name(0, 1), "Defect");
    }

    #[test]
    fn a_strategic_game_becomes_a_validated_strategic_game() {
        let g: GameJson = serde_json::from_value(serde_json::json!({
            "form": "strategic",
            "players": [{"id": 0, "name": "Row"}, {"id": 1, "name": "Col"}],
            "strategies": [["C", "D"], ["C", "D"]],
            "outcomes": [
                {"profile": [0, 0], "payoffs": [3.0, 3.0]},
                {"profile": [0, 1], "payoffs": [0.0, 4.0]},
                {"profile": [1, 0], "payoffs": [4.0, 0.0]},
                {"profile": [1, 1], "payoffs": [1.0, 1.0]}
            ],
            "payoff_kind": "cardinal"
        }))
        .unwrap();
        assert_eq!(g.into_strategic().unwrap().n_players(), 2);
    }

    #[test]
    fn an_extensive_game_becomes_a_validated_tree() {
        let g = entry().into_extensive().unwrap();
        assert_eq!(g.n_players(), 2);
        assert_eq!(g.nodes().len(), 5);
    }

    #[test]
    fn a_tree_sent_to_the_strategic_path_reports_wrong_game_form() {
        let err = entry().into_strategic().unwrap_err();
        assert!(
            matches!(
                err,
                GtError::WrongGameForm {
                    expected: "strategic",
                    actual: "extensive"
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn a_matrix_sent_to_the_tree_path_reports_wrong_game_form() {
        let err = pd().into_extensive().unwrap_err();
        assert!(
            matches!(
                err,
                GtError::WrongGameForm {
                    expected: "extensive",
                    actual: "matrix"
                }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn form_name_matches_the_tag() {
        assert_eq!(pd().form_name(), "matrix");
        assert_eq!(entry().form_name(), "extensive");
    }

    #[test]
    fn an_invalid_game_reports_diagnostics_rather_than_panicking() {
        // Payoff vector arity does not match the player count.
        let g: GameJson = serde_json::from_value(serde_json::json!({
            "form": "strategic",
            "players": [{"id": 0, "name": "Row"}, {"id": 1, "name": "Col"}],
            "strategies": [["C"], ["C"]],
            "outcomes": [{"profile": [0, 0], "payoffs": [1.0]}],
            "payoff_kind": "cardinal"
        }))
        .unwrap();
        assert!(matches!(
            g.into_strategic(),
            Err(GtError::InvalidGame { .. })
        ));
    }

    #[test]
    fn game_json_has_a_default_for_the_tool_parameter_bound() {
        // rmcp's ToolBase::Parameter requires Default.
        let _ = GameJson::default();
    }
}
