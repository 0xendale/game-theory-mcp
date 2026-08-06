//! Checking a claimed equilibrium.
//!
//! The point of this module is to be independent of the solvers: it re-derives
//! the answer from the definition rather than asking whether a solver produced
//! the profile. A caller who states an equilibrium gets it confirmed or gets
//! the exact deviation that refutes it.

use crate::error::GtError;
use crate::game::{StrategyId, ValidStrategicGame};
use crate::solve::dominance::{solve_dominance, DominanceMode};
use crate::solve::pure_nash::{profitable_deviation, Deviation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Concept {
    PureNash,
    DominantStrategy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifyResult {
    pub holds: bool,
    pub concept: Concept,
    /// Every player who can profitably deviate, with their best deviation.
    pub deviations: Vec<Deviation>,
    /// Set when the concept fails for a reason other than a deviation.
    pub note: Option<String>,
}

pub fn verify_equilibrium(
    game: &ValidStrategicGame,
    profile: &[StrategyId],
    concept: Concept,
) -> Result<VerifyResult, GtError> {
    check_profile(game, profile)?;

    let deviations: Vec<Deviation> = (0..game.n_players())
        .filter_map(|p| profitable_deviation(game, profile, p))
        .collect();

    match concept {
        Concept::PureNash => Ok(VerifyResult {
            holds: deviations.is_empty(),
            concept,
            deviations,
            note: None,
        }),
        Concept::DominantStrategy => {
            // Every player's component must strictly dominate all their other
            // strategies — a stronger requirement than being a best response
            // to this particular profile.
            let dominance = solve_dominance(game, DominanceMode::Strict);
            let holds = dominance.unique_profile.as_deref() == Some(profile);
            let note = if holds {
                None
            } else {
                Some(match &dominance.unique_profile {
                    Some(other) => {
                        format!("iterated strict dominance yields {other:?}, not {profile:?}")
                    }
                    None => "no player has a strictly dominant strategy in this game".to_string(),
                })
            };
            Ok(VerifyResult {
                holds,
                concept,
                deviations,
                note,
            })
        }
    }
}

fn check_profile(game: &ValidStrategicGame, profile: &[StrategyId]) -> Result<(), GtError> {
    if profile.len() != game.n_players() {
        return Err(GtError::UnknownProfile {
            player: 0,
            profile: profile.to_vec(),
        });
    }
    for (p, &s) in profile.iter().enumerate() {
        if s >= game.n_strategies(p) {
            return Err(GtError::UnknownProfile {
                player: p,
                profile: profile.to_vec(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{MatrixForm, PayoffKind, Rational, StrategicGame, ValidStrategicGame};

    fn pd() -> ValidStrategicGame {
        let form = MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: vec!["Cooperate".into(), "Defect".into()],
            col_strategies: vec!["Cooperate".into(), "Defect".into()],
            payoff_matrix: vec![vec![[3.0, 3.0], [0.0, 4.0]], vec![[4.0, 0.0], [1.0, 1.0]]],
            payoff_kind: PayoffKind::Cardinal,
        };
        ValidStrategicGame::validate(StrategicGame::try_from(form).unwrap()).unwrap()
    }

    #[test]
    fn a_real_equilibrium_is_confirmed() {
        let result = verify_equilibrium(&pd(), &[1, 1], Concept::PureNash).expect("checked");
        assert!(result.holds);
        assert!(result.deviations.is_empty());
    }

    #[test]
    fn a_false_claim_is_refused_with_the_specific_deviation() {
        let result = verify_equilibrium(&pd(), &[0, 0], Concept::PureNash).expect("checked");
        assert!(!result.holds);
        assert_eq!(result.deviations.len(), 2, "both players can profit");

        let row = result
            .deviations
            .iter()
            .find(|d| d.player == 0)
            .expect("row");
        assert_eq!(row.from, 0);
        assert_eq!(row.to, 1);
        assert_eq!(row.gain, Rational::from_integer(1.into()));
    }

    #[test]
    fn dominant_strategy_equilibrium_is_confirmed_for_the_prisoners_dilemma() {
        let result =
            verify_equilibrium(&pd(), &[1, 1], Concept::DominantStrategy).expect("checked");
        assert!(result.holds);
    }

    #[test]
    fn a_nash_equilibrium_that_is_not_in_dominant_strategies_is_distinguished() {
        // Battle of the Sexes: (Opera, Opera) is Nash but neither player has a
        // dominant strategy.
        let form = MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: vec!["Opera".into(), "Football".into()],
            col_strategies: vec!["Opera".into(), "Football".into()],
            payoff_matrix: vec![vec![[2.0, 1.0], [0.0, 0.0]], vec![[0.0, 0.0], [1.0, 2.0]]],
            payoff_kind: PayoffKind::Cardinal,
        };
        let bos = ValidStrategicGame::validate(StrategicGame::try_from(form).unwrap()).unwrap();

        assert!(
            verify_equilibrium(&bos, &[0, 0], Concept::PureNash)
                .unwrap()
                .holds
        );
        let dominant = verify_equilibrium(&bos, &[0, 0], Concept::DominantStrategy).unwrap();
        assert!(!dominant.holds);
        assert!(dominant.note.is_some(), "should explain what failed");
    }

    #[test]
    fn a_profile_of_the_wrong_length_is_rejected() {
        let err = verify_equilibrium(&pd(), &[1], Concept::PureNash).expect_err("arity");
        assert!(matches!(err, GtError::UnknownProfile { .. }));
    }

    #[test]
    fn a_profile_naming_a_nonexistent_strategy_is_rejected() {
        let err = verify_equilibrium(&pd(), &[9, 0], Concept::PureNash).expect_err("range");
        assert!(matches!(err, GtError::UnknownProfile { .. }));
    }
}
