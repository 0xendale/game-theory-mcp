//! Checking a claimed mixed-strategy equilibrium.
//!
//! Like `verify`, this module re-derives the answer from the definition rather
//! than asking a solver: every strategy in the support must earn the same
//! expected payoff, and no strategy outside the support may earn more
//! (Bonanno §5.3). A caller who states a mixed equilibrium gets it confirmed,
//! or gets the exact strategy and margin that refutes it.

use crate::error::GtError;
use crate::game::{PlayerId, Rational, StrategyId, ValidStrategicGame};
use crate::solve::mixed_nash::{expected_payoff_against, MixedStrategy};
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};

/// A pure strategy outside the support that earns strictly more than the
/// mixture does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixedDeviation {
    pub player: PlayerId,
    pub to: StrategyId,
    pub gain: Rational,
}

/// A pure strategy *inside* the support that earns strictly less than the
/// mixture does — a violation of the indifference condition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupportViolation {
    pub player: PlayerId,
    pub strategy: StrategyId,
    pub shortfall: Rational,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixedVerifyResult {
    pub holds: bool,
    /// Expected payoff to each player under the claimed profile.
    pub expected_payoffs: Vec<Rational>,
    pub support_violations: Vec<SupportViolation>,
    pub deviations: Vec<MixedDeviation>,
    pub note: Option<String>,
}

/// Verify a claimed mixed-strategy Nash equilibrium of a 2-player cardinal
/// game. Errors describe a malformed request; a well-formed request that is
/// not an equilibrium comes back as `holds: false` with the evidence.
pub fn verify_mixed_nash(
    game: &ValidStrategicGame,
    profile: &[MixedStrategy],
) -> Result<MixedVerifyResult, GtError> {
    game.require_cardinal("verify_mixed_nash")?;

    if profile.len() != game.n_players() || game.n_players() != 2 {
        return Err(GtError::InvalidMixedStrategy {
            player: 0,
            reason: format!(
                "expected one mixture for each of 2 players, got {} for a {}-player game",
                profile.len(),
                game.n_players()
            ),
        });
    }

    for (player, mixture) in profile.iter().enumerate() {
        check_distribution(game, player, mixture)?;
    }

    let mut expected_payoffs = Vec::with_capacity(2);
    let mut support_violations = Vec::new();
    let mut deviations = Vec::new();

    for player in 0..2 {
        let opponent = 1 - player;
        let per_strategy = expected_payoff_against(game, &profile[opponent].probs, player);

        // The mixture's own value is the probability-weighted average.
        let value = profile[player]
            .probs
            .iter()
            .zip(&per_strategy)
            .fold(Rational::zero(), |acc, (p, u)| acc + p * u);

        for (strategy, payoff) in per_strategy.iter().enumerate() {
            let in_support = !profile[player].probs[strategy].is_zero();
            if in_support && *payoff < value {
                support_violations.push(SupportViolation {
                    player,
                    strategy,
                    shortfall: &value - payoff,
                });
            }
            if *payoff > value {
                deviations.push(MixedDeviation {
                    player,
                    to: strategy,
                    gain: payoff - &value,
                });
            }
        }

        expected_payoffs.push(value);
    }

    let holds = support_violations.is_empty() && deviations.is_empty();
    let note = (!holds).then(|| {
        "a mixed equilibrium requires every strategy in the support to earn the \
         mixture's expected payoff and no strategy outside it to earn more"
            .to_string()
    });

    Ok(MixedVerifyResult {
        holds,
        expected_payoffs,
        support_violations,
        deviations,
        note,
    })
}

fn check_distribution(
    game: &ValidStrategicGame,
    player: PlayerId,
    mixture: &MixedStrategy,
) -> Result<(), GtError> {
    let expected = game.n_strategies(player);
    if mixture.probs.len() != expected {
        return Err(GtError::InvalidMixedStrategy {
            player,
            reason: format!(
                "expected {expected} probabilities, got {}",
                mixture.probs.len()
            ),
        });
    }
    if let Some((index, value)) = mixture
        .probs
        .iter()
        .enumerate()
        .find(|(_, p)| **p < Rational::zero())
    {
        return Err(GtError::InvalidMixedStrategy {
            player,
            reason: format!("probability {value} on strategy {index} is negative"),
        });
    }
    let total: Rational = mixture.probs.iter().sum();
    if total != Rational::one() {
        return Err(GtError::InvalidMixedStrategy {
            player,
            reason: format!("probabilities sum to {total}, not 1"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{MatrixForm, PayoffKind, Rational, StrategicGame, ValidStrategicGame};

    fn r(n: i64, d: i64) -> Rational {
        Rational::new(n.into(), d.into())
    }

    fn int(n: i64) -> Rational {
        Rational::from_integer(n.into())
    }

    fn mix(probs: Vec<Rational>) -> MixedStrategy {
        MixedStrategy { probs }
    }

    fn matching_pennies() -> ValidStrategicGame {
        let form = MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: vec!["Heads".into(), "Tails".into()],
            col_strategies: vec!["Heads".into(), "Tails".into()],
            payoff_matrix: vec![
                vec![[1.0, -1.0], [-1.0, 1.0]],
                vec![[-1.0, 1.0], [1.0, -1.0]],
            ],
            payoff_kind: PayoffKind::Cardinal,
        };
        ValidStrategicGame::validate(StrategicGame::try_from(form).unwrap()).unwrap()
    }

    /// Battle of the Sexes: the mixed equilibrium is (2/3, 1/3) for Row and
    /// (1/3, 2/3) for Col, with expected payoff 2/3 to each.
    fn battle_of_the_sexes() -> ValidStrategicGame {
        let form = MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: vec!["Opera".into(), "Football".into()],
            col_strategies: vec!["Opera".into(), "Football".into()],
            payoff_matrix: vec![vec![[2.0, 1.0], [0.0, 0.0]], vec![[0.0, 0.0], [1.0, 2.0]]],
            payoff_kind: PayoffKind::Cardinal,
        };
        ValidStrategicGame::validate(StrategicGame::try_from(form).unwrap()).unwrap()
    }

    #[test]
    fn the_matching_pennies_equilibrium_is_confirmed() {
        let half = || mix(vec![r(1, 2), r(1, 2)]);
        let result = verify_mixed_nash(&matching_pennies(), &[half(), half()]).expect("checked");
        assert!(result.holds);
        assert!(result.support_violations.is_empty());
        assert!(result.deviations.is_empty());
        assert_eq!(result.expected_payoffs, vec![int(0), int(0)]);
    }

    #[test]
    fn the_battle_of_the_sexes_equilibrium_is_confirmed_with_exact_fractions() {
        let profile = [mix(vec![r(2, 3), r(1, 3)]), mix(vec![r(1, 3), r(2, 3)])];
        let result = verify_mixed_nash(&battle_of_the_sexes(), &profile).expect("checked");
        assert!(result.holds);
        assert_eq!(result.expected_payoffs, vec![r(2, 3), r(2, 3)]);
    }

    #[test]
    fn a_wrong_mixture_is_refused_with_the_profitable_deviation() {
        // Against Col playing (1/2, 1/2) in Battle of the Sexes, Row's Opera
        // yields 1 and Football yields 1/2, so Row is not indifferent and any
        // mixture putting weight on Football is not a best response.
        let profile = [mix(vec![r(1, 2), r(1, 2)]), mix(vec![r(1, 2), r(1, 2)])];
        let result = verify_mixed_nash(&battle_of_the_sexes(), &profile).expect("checked");
        assert!(!result.holds);
        let row = result
            .deviations
            .iter()
            .find(|d| d.player == 0)
            .expect("Row can deviate");
        assert_eq!(row.to, 0, "Opera");
        assert_eq!(row.gain, r(1, 4), "1 against an expected 3/4");
    }

    #[test]
    fn a_support_strategy_earning_less_is_named() {
        let profile = [mix(vec![r(1, 2), r(1, 2)]), mix(vec![r(1, 2), r(1, 2)])];
        let result = verify_mixed_nash(&battle_of_the_sexes(), &profile).expect("checked");
        let violation = result
            .support_violations
            .iter()
            .find(|v| v.player == 0 && v.strategy == 1)
            .expect("Football is in the support and earns less");
        assert_eq!(violation.shortfall, r(1, 4), "3/4 - 1/2");
    }

    #[test]
    fn a_pure_profile_expressed_as_degenerate_mixtures_is_accepted() {
        // (Opera, Opera) is a pure Nash equilibrium; as mixtures it must verify.
        let profile = [mix(vec![int(1), int(0)]), mix(vec![int(1), int(0)])];
        let result = verify_mixed_nash(&battle_of_the_sexes(), &profile).expect("checked");
        assert!(result.holds);
    }

    #[test]
    fn probabilities_that_do_not_sum_to_one_are_rejected() {
        let profile = [mix(vec![r(1, 2), r(1, 4)]), mix(vec![r(1, 2), r(1, 2)])];
        let err = verify_mixed_nash(&battle_of_the_sexes(), &profile).expect_err("bad mixture");
        assert!(matches!(
            err,
            GtError::InvalidMixedStrategy { player: 0, .. }
        ));
    }

    #[test]
    fn a_negative_probability_is_rejected() {
        let profile = [mix(vec![r(3, 2), r(-1, 2)]), mix(vec![r(1, 2), r(1, 2)])];
        let err = verify_mixed_nash(&battle_of_the_sexes(), &profile).expect_err("bad mixture");
        assert!(matches!(
            err,
            GtError::InvalidMixedStrategy { player: 0, .. }
        ));
    }

    #[test]
    fn a_mixture_of_the_wrong_length_is_rejected() {
        let profile = [mix(vec![int(1)]), mix(vec![r(1, 2), r(1, 2)])];
        let err = verify_mixed_nash(&battle_of_the_sexes(), &profile).expect_err("bad arity");
        assert!(matches!(
            err,
            GtError::InvalidMixedStrategy { player: 0, .. }
        ));
    }

    #[test]
    fn three_players_are_refused_rather_than_approximated() {
        // Reuse the two-player game but claim three mixtures.
        let profile = [
            mix(vec![r(1, 2), r(1, 2)]),
            mix(vec![r(1, 2), r(1, 2)]),
            mix(vec![r(1, 2), r(1, 2)]),
        ];
        let err = verify_mixed_nash(&battle_of_the_sexes(), &profile).expect_err("arity");
        assert!(matches!(err, GtError::InvalidMixedStrategy { .. }));
    }

    #[test]
    fn ordinal_payoffs_are_rejected() {
        let form = MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: vec!["A".into(), "B".into()],
            col_strategies: vec!["L".into(), "R".into()],
            payoff_matrix: vec![vec![[2.0, 1.0], [0.0, 0.0]], vec![[0.0, 0.0], [1.0, 2.0]]],
            payoff_kind: PayoffKind::Ordinal,
        };
        let g = ValidStrategicGame::validate(StrategicGame::try_from(form).unwrap()).unwrap();
        let profile = [mix(vec![r(1, 2), r(1, 2)]), mix(vec![r(1, 2), r(1, 2)])];
        let err = verify_mixed_nash(&g, &profile).expect_err("ordinal");
        assert!(matches!(err, GtError::OrdinalPayoffsRejected { .. }));
    }
}
