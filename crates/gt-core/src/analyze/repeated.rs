//! Infinitely repeated 2-player games: when is a target profile sustainable?
//!
//! Source of record: Martin J. Osborne and Ariel Rubinstein, *A Course in Game
//! Theory*, MIT Press, 1994, ch. 8. Repeated games are the one tool in this
//! crate with no Bonanno chapter behind them, so nothing here cites Bonanno.
//!
//! Convention: discounted-sum payoffs, δ in [0, 1), grim trigger with Nash
//! reversion — after any deviation, both players play a pure-strategy Nash
//! equilibrium of the stage game forever. Nash reversion is what makes the
//! threat credible, so a stage game with no pure Nash equilibrium is refused
//! rather than punished with a non-credible minmax threat.

use crate::error::GtError;
use crate::game::{PlayerId, Profile, Rational, StrategyId, ValidStrategicGame};
use crate::solve::pure_nash::solve_pure_nash;
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Punishment {
    GrimTrigger,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerThreshold {
    pub player: PlayerId,
    /// Stage payoff under the target profile.
    pub target_payoff: Rational,
    /// The most profitable one-shot deviation, if any pays.
    pub best_deviation: Option<StrategyId>,
    /// Stage payoff from that deviation; equals `target_payoff` when none pays.
    pub deviation_payoff: Rational,
    /// Stage payoff under the punishment profile.
    pub punishment_payoff: Rational,
    /// Pure minmax value, reported for context.
    pub minmax: Rational,
    /// Smallest δ sustaining the target for this player; `None` when no δ < 1
    /// does.
    pub critical_discount_factor: Option<Rational>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepeatedGameReport {
    pub target: Profile,
    pub punishment: Punishment,
    /// The stage Nash equilibrium reverted to after a deviation.
    pub punishment_profile: Profile,
    pub per_player: Vec<PlayerThreshold>,
    /// The binding threshold: the maximum over players, or `None` when some
    /// player's target is not sustainable at any δ < 1.
    pub critical_discount_factor: Option<Rational>,
    /// Set only when the caller supplied a discount factor.
    pub sustainable_at: Option<bool>,
    pub note: Option<String>,
}

/// Analyse an infinitely repeated stage game under grim trigger.
///
/// For player *i*, with `c` the target stage payoff, `d` the best one-shot
/// deviation payoff, and `p` the punishment payoff, cooperation survives when
/// `(1 - δ) d + δ p <= c`. Solving for δ gives the threshold below.
pub fn analyze_repeated_game(
    game: &ValidStrategicGame,
    target: &[StrategyId],
    punishment: Punishment,
    discount_factor: Option<Rational>,
) -> Result<RepeatedGameReport, GtError> {
    game.require_cardinal("analyze_repeated_game")?;

    if game.n_players() != 2 {
        return Err(GtError::NPlayerMixedUnsupported {
            players: game.n_players(),
        });
    }
    if target.len() != game.n_players()
        || target
            .iter()
            .enumerate()
            .any(|(p, &s)| s >= game.n_strategies(p))
    {
        return Err(GtError::UnknownProfile {
            player: 0,
            profile: target.to_vec(),
        });
    }
    if let Some(delta) = &discount_factor {
        if *delta < Rational::zero() || *delta >= Rational::one() {
            return Err(GtError::InvalidDiscountFactor {
                value: delta.to_string(),
            });
        }
    }

    // Nash reversion needs a pure stage equilibrium. When several exist, the
    // harshest available punishment is the one that sustains the most, so the
    // equilibrium minimizing the sum of payoffs is chosen — deterministically,
    // by taking the lexicographically first among the minimizers.
    let pure = solve_pure_nash(game);
    let punishment_profile = pure
        .equilibria
        .iter()
        .min_by(|a, b| {
            let sum = |profile: &Profile| -> Rational {
                (0..2).fold(Rational::zero(), |acc, p| acc + game.payoff(profile, p))
            };
            sum(a).cmp(&sum(b)).then_with(|| a.cmp(b))
        })
        .cloned()
        .ok_or(GtError::NoPureNashForPunishment)?;

    let mut per_player = Vec::with_capacity(2);
    for player in 0..2 {
        let target_payoff = game.payoff(target, player).clone();
        let punishment_payoff = game.payoff(&punishment_profile, player).clone();

        let mut best_deviation = None;
        let mut deviation_payoff = target_payoff.clone();
        for s in 0..game.n_strategies(player) {
            if s == target[player] {
                continue;
            }
            let mut candidate = target.to_vec();
            candidate[player] = s;
            let payoff = game.payoff(&candidate, player);
            if *payoff > deviation_payoff {
                deviation_payoff = payoff.clone();
                best_deviation = Some(s);
            }
        }

        // (1 - δ) d + δ p <= c, solved for δ.
        let critical_discount_factor = if deviation_payoff <= target_payoff {
            Some(Rational::zero())
        } else if deviation_payoff > punishment_payoff {
            Some((&deviation_payoff - &target_payoff) / (&deviation_payoff - &punishment_payoff))
        } else {
            None
        };

        per_player.push(PlayerThreshold {
            player,
            target_payoff,
            best_deviation,
            deviation_payoff,
            punishment_payoff,
            minmax: minmax_value(game, player),
            critical_discount_factor,
        });
    }

    let critical_discount_factor = if per_player
        .iter()
        .any(|p| p.critical_discount_factor.is_none())
    {
        None
    } else {
        per_player
            .iter()
            .filter_map(|p| p.critical_discount_factor.clone())
            .max()
    };

    let sustainable_at = discount_factor.as_ref().map(|delta| {
        critical_discount_factor
            .as_ref()
            .is_some_and(|threshold| delta >= threshold)
    });

    let note = match &critical_discount_factor {
        Some(_) => None,
        None => Some(
            "no discount factor below 1 sustains this profile: some player's best \
             one-shot deviation pays at least as much as the punishment does"
                .to_string(),
        ),
    };

    Ok(RepeatedGameReport {
        target: target.to_vec(),
        punishment,
        punishment_profile,
        per_player,
        critical_discount_factor,
        sustainable_at,
        note,
    })
}

/// Pure minmax: the lowest payoff the opponents can hold `player` to, given
/// that `player` best-responds. Reported for context — the punishment actually
/// used is Nash reversion, which is credible where minmax generally is not.
fn minmax_value(game: &ValidStrategicGame, player: PlayerId) -> Rational {
    let opponent = 1 - player;
    (0..game.n_strategies(opponent))
        .map(|opp| {
            (0..game.n_strategies(player))
                .map(|own| {
                    let profile = if player == 0 {
                        vec![own, opp]
                    } else {
                        vec![opp, own]
                    };
                    game.payoff(&profile, player).clone()
                })
                .max()
                .expect("every player has at least one strategy")
        })
        .min()
        .expect("every player has at least one strategy")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{MatrixForm, PayoffKind, StrategicGame, ValidStrategicGame};

    fn r(n: i64, d: i64) -> Rational {
        Rational::new(n.into(), d.into())
    }

    fn int(n: i64) -> Rational {
        Rational::from_integer(n.into())
    }

    /// Prisoner's Dilemma, index 0 = Cooperate, 1 = Defect.
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
    fn cooperation_in_the_prisoners_dilemma_needs_delta_at_least_one_third() {
        // c = 3, d = 4, p = 1  =>  δ* = (4 - 3) / (4 - 1) = 1/3, exactly.
        let report =
            analyze_repeated_game(&pd(), &[0, 0], Punishment::GrimTrigger, None).expect("analyzed");
        assert_eq!(report.critical_discount_factor, Some(r(1, 3)));
        assert_eq!(report.punishment_profile, vec![1, 1], "Nash reversion");
        assert_eq!(report.sustainable_at, None, "no δ was supplied");
    }

    #[test]
    fn the_per_player_breakdown_names_the_deviation_and_its_payoffs() {
        let report =
            analyze_repeated_game(&pd(), &[0, 0], Punishment::GrimTrigger, None).expect("analyzed");
        let row = &report.per_player[0];
        assert_eq!(row.target_payoff, int(3));
        assert_eq!(row.best_deviation, Some(1), "Defect");
        assert_eq!(row.deviation_payoff, int(4));
        assert_eq!(row.punishment_payoff, int(1));
        assert_eq!(row.minmax, int(1));
        assert_eq!(row.critical_discount_factor, Some(r(1, 3)));
    }

    #[test]
    fn a_supplied_discount_factor_is_answered_yes_or_no() {
        let above = analyze_repeated_game(&pd(), &[0, 0], Punishment::GrimTrigger, Some(r(1, 2)))
            .expect("analyzed");
        assert_eq!(above.sustainable_at, Some(true));

        let below = analyze_repeated_game(&pd(), &[0, 0], Punishment::GrimTrigger, Some(r(1, 4)))
            .expect("analyzed");
        assert_eq!(below.sustainable_at, Some(false));

        let exactly = analyze_repeated_game(&pd(), &[0, 0], Punishment::GrimTrigger, Some(r(1, 3)))
            .expect("analyzed");
        assert_eq!(exactly.sustainable_at, Some(true), "the threshold is weak");
    }

    #[test]
    fn a_target_that_is_already_a_stage_nash_equilibrium_needs_no_patience() {
        let report =
            analyze_repeated_game(&pd(), &[1, 1], Punishment::GrimTrigger, None).expect("analyzed");
        assert_eq!(report.critical_discount_factor, Some(int(0)));
        assert!(report.per_player.iter().all(|p| p.best_deviation.is_none()));
    }

    #[test]
    fn a_discount_factor_outside_the_unit_interval_is_rejected() {
        let err = analyze_repeated_game(&pd(), &[0, 0], Punishment::GrimTrigger, Some(int(1)))
            .expect_err("δ = 1 is not allowed");
        assert!(matches!(err, GtError::InvalidDiscountFactor { .. }));

        let err = analyze_repeated_game(&pd(), &[0, 0], Punishment::GrimTrigger, Some(r(-1, 2)))
            .expect_err("negative δ");
        assert!(matches!(err, GtError::InvalidDiscountFactor { .. }));
    }

    #[test]
    fn a_stage_game_without_a_pure_nash_equilibrium_is_refused() {
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
        let mp = ValidStrategicGame::validate(StrategicGame::try_from(form).unwrap()).unwrap();
        let err = analyze_repeated_game(&mp, &[0, 0], Punishment::GrimTrigger, None)
            .expect_err("no pure Nash to revert to");
        assert!(matches!(err, GtError::NoPureNashForPunishment));
    }

    #[test]
    fn ordinal_payoffs_are_rejected() {
        let form = MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: vec!["C".into(), "D".into()],
            col_strategies: vec!["C".into(), "D".into()],
            payoff_matrix: vec![vec![[3.0, 3.0], [0.0, 4.0]], vec![[4.0, 0.0], [1.0, 1.0]]],
            payoff_kind: PayoffKind::Ordinal,
        };
        let g = ValidStrategicGame::validate(StrategicGame::try_from(form).unwrap()).unwrap();
        let err = analyze_repeated_game(&g, &[0, 0], Punishment::GrimTrigger, None)
            .expect_err("expectation over ranks");
        assert!(matches!(err, GtError::OrdinalPayoffsRejected { .. }));
    }

    #[test]
    fn a_target_naming_a_nonexistent_strategy_is_rejected() {
        let err = analyze_repeated_game(&pd(), &[9, 0], Punishment::GrimTrigger, None)
            .expect_err("out of range");
        assert!(matches!(err, GtError::UnknownProfile { .. }));
    }

    #[test]
    fn a_target_no_patience_can_sustain_is_reported_as_unsustainable() {
        // B strictly dominates A for Row, so (B, R) is the pure Nash used for
        // reversion, giving Row 5 — more than the target (A, L)'s 1, and no
        // less than Row's deviation payoff. No δ < 1 works.
        let form = MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: vec!["A".into(), "B".into()],
            col_strategies: vec!["L".into(), "R".into()],
            payoff_matrix: vec![vec![[1.0, 1.0], [0.0, 0.0]], vec![[5.0, 0.0], [5.0, 2.0]]],
            payoff_kind: PayoffKind::Cardinal,
        };
        let g = ValidStrategicGame::validate(StrategicGame::try_from(form).unwrap()).unwrap();
        let report =
            analyze_repeated_game(&g, &[0, 0], Punishment::GrimTrigger, None).expect("analyzed");
        assert_eq!(report.per_player[0].critical_discount_factor, None);
        assert_eq!(report.critical_discount_factor, None);
        assert!(report.note.is_some(), "must explain why there is no δ*");
    }
}
