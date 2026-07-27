//! Game representation. Plain data — validation lives in `validate.rs`.

use serde::{Deserialize, Serialize};

pub mod extensive;
pub mod matrix_form;
pub mod validate;
pub use extensive::{ExtensiveGame, Node, NodeId};
pub use matrix_form::MatrixForm;
pub use validate::ValidStrategicGame;

/// Index into `StrategicGame::players`.
pub type PlayerId = usize;

/// Index into a player's own strategy list.
pub type StrategyId = usize;

/// One strategy per player, in player order.
pub type Profile = Vec<StrategyId>;

/// Exact payoff arithmetic. Every comparison in this crate uses this type;
/// `f64` exists only at the serde boundary.
pub type Rational = num_rational::BigRational;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
}

/// Whether payoffs are ranks or utilities.
///
/// Ordinal payoffs are ordinal only: an expectation over them is meaningless,
/// so tools that take expectations reject them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PayoffKind {
    Ordinal,
    Cardinal,
}

/// One strategy profile and the payoff each player receives at it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub profile: Profile,
    pub payoffs: Vec<f64>,
}

/// A game in strategic (normal) form. Unvalidated — see `ValidStrategicGame`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrategicGame {
    pub players: Vec<Player>,
    pub strategies: Vec<Vec<String>>,
    pub outcomes: Vec<Outcome>,
    pub payoff_kind: PayoffKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prisoner's Dilemma. Row/Col each choose Cooperate or Defect.
    fn prisoners_dilemma() -> StrategicGame {
        StrategicGame {
            players: vec![
                Player {
                    id: 0,
                    name: "Row".into(),
                },
                Player {
                    id: 1,
                    name: "Col".into(),
                },
            ],
            strategies: vec![
                vec!["Cooperate".into(), "Defect".into()],
                vec!["Cooperate".into(), "Defect".into()],
            ],
            outcomes: vec![
                Outcome {
                    profile: vec![0, 0],
                    payoffs: vec![3.0, 3.0],
                },
                Outcome {
                    profile: vec![0, 1],
                    payoffs: vec![0.0, 4.0],
                },
                Outcome {
                    profile: vec![1, 0],
                    payoffs: vec![4.0, 0.0],
                },
                Outcome {
                    profile: vec![1, 1],
                    payoffs: vec![1.0, 1.0],
                },
            ],
            payoff_kind: PayoffKind::Cardinal,
        }
    }

    #[test]
    fn game_survives_a_json_round_trip() {
        let game = prisoners_dilemma();
        let json = serde_json::to_string(&game).expect("serializes");
        let back: StrategicGame = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(game, back);
    }

    #[test]
    fn payoff_kind_serializes_as_a_lowercase_string() {
        let json = serde_json::to_string(&PayoffKind::Cardinal).expect("serializes");
        assert_eq!(json, "\"cardinal\"");
    }

    #[test]
    fn a_game_missing_required_fields_fails_to_deserialize() {
        let json = r#"{"players":[],"strategies":[],"outcomes":[]}"#;
        let parsed: Result<StrategicGame, _> = serde_json::from_str(json);
        assert!(parsed.is_err(), "payoff_kind is required");
    }
}
