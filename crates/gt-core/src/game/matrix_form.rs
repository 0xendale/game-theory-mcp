//! The 2-player payoff-matrix input shape, and its conversion to the canonical
//! outcome-list form. This is a convenience for callers, not a second
//! representation — everything downstream sees a `StrategicGame`.

use crate::error::{DiagnosticCode, GtError};
use crate::game::{Outcome, PayoffKind, Player, StrategicGame};
use serde::{Deserialize, Serialize};

/// A 2-player game written as a matrix. `payoff_matrix[row][col] == [u_row, u_col]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatrixForm {
    pub players: [String; 2],
    pub row_strategies: Vec<String>,
    pub col_strategies: Vec<String>,
    pub payoff_matrix: Vec<Vec<[f64; 2]>>,
    pub payoff_kind: PayoffKind,
}

impl TryFrom<MatrixForm> for StrategicGame {
    type Error = GtError;

    fn try_from(m: MatrixForm) -> Result<Self, Self::Error> {
        let rows = m.row_strategies.len();
        let cols = m.col_strategies.len();

        if m.payoff_matrix.len() != rows {
            return Err(GtError::invalid(
                DiagnosticCode::ProfileArityMismatch,
                format!(
                    "payoff_matrix has {} rows but {rows} row strategies are named",
                    m.payoff_matrix.len()
                ),
            ));
        }
        for (r, row) in m.payoff_matrix.iter().enumerate() {
            if row.len() != cols {
                return Err(GtError::invalid(
                    DiagnosticCode::ProfileArityMismatch,
                    format!(
                        "payoff_matrix row {r} has {} cells but {cols} column strategies \
                         are named",
                        row.len()
                    ),
                ));
            }
        }

        let mut outcomes = Vec::with_capacity(rows * cols);
        for (r, row) in m.payoff_matrix.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                outcomes.push(Outcome {
                    profile: vec![r, c],
                    payoffs: cell.to_vec(),
                });
            }
        }

        let [row_name, col_name] = m.players;
        Ok(StrategicGame {
            players: vec![
                Player {
                    id: 0,
                    name: row_name,
                },
                Player {
                    id: 1,
                    name: col_name,
                },
            ],
            strategies: vec![m.row_strategies, m.col_strategies],
            outcomes,
            payoff_kind: m.payoff_kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DiagnosticCode;
    use crate::game::Rational;
    use crate::game::{PayoffKind, ValidStrategicGame};

    fn pd_matrix() -> MatrixForm {
        MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: vec!["Cooperate".into(), "Defect".into()],
            col_strategies: vec!["Cooperate".into(), "Defect".into()],
            payoff_matrix: vec![vec![[3.0, 3.0], [0.0, 4.0]], vec![[4.0, 0.0], [1.0, 1.0]]],
            payoff_kind: PayoffKind::Cardinal,
        }
    }

    #[test]
    fn a_matrix_becomes_a_game_with_one_outcome_per_cell() {
        let game = StrategicGame::try_from(pd_matrix()).expect("converts");
        assert_eq!(game.outcomes.len(), 4);
        assert_eq!(game.players.len(), 2);
        assert_eq!(game.strategies[0], vec!["Cooperate", "Defect"]);
    }

    #[test]
    fn row_index_is_the_first_players_strategy() {
        let game = StrategicGame::try_from(pd_matrix()).expect("converts");
        let valid = ValidStrategicGame::validate(game).expect("valid");
        // Row defects, Col cooperates: row gets 4, col gets 0.
        assert_eq!(*valid.payoff(&[1, 0], 0), Rational::from_integer(4.into()));
        assert_eq!(*valid.payoff(&[1, 0], 1), Rational::from_integer(0.into()));
    }

    #[test]
    fn the_converted_game_always_validates() {
        let game = StrategicGame::try_from(pd_matrix()).expect("converts");
        ValidStrategicGame::validate(game).expect("a converted matrix is complete");
    }

    #[test]
    fn a_ragged_matrix_is_rejected() {
        let mut m = pd_matrix();
        m.payoff_matrix[1] = vec![[4.0, 0.0]];
        let err = StrategicGame::try_from(m).expect_err("ragged");
        match err {
            GtError::InvalidGame { diagnostics } => {
                assert_eq!(diagnostics[0].code, DiagnosticCode::ProfileArityMismatch);
            }
            other => panic!("expected InvalidGame, got {other:?}"),
        }
    }

    #[test]
    fn a_matrix_whose_shape_disagrees_with_its_labels_is_rejected() {
        let mut m = pd_matrix();
        m.col_strategies.push("Third".into());
        let err = StrategicGame::try_from(m).expect_err("shape mismatch");
        match err {
            GtError::InvalidGame { diagnostics } => {
                assert_eq!(diagnostics[0].code, DiagnosticCode::ProfileArityMismatch);
            }
            other => panic!("expected InvalidGame, got {other:?}"),
        }
    }

    #[test]
    fn matrix_form_survives_a_json_round_trip() {
        let json = serde_json::to_string(&pd_matrix()).expect("serializes");
        let back: MatrixForm = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back.payoff_matrix, pd_matrix().payoff_matrix);
    }
}
