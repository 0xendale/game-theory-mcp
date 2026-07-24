//! The only path from a caller-supplied `StrategicGame` to something a solver
//! will accept. Validation also builds the dense exact-payoff table, so
//! solvers never do a lookup that can fail.

use crate::error::{Diagnostic, DiagnosticCode, GtError};
use crate::game::{PayoffKind, PlayerId, Profile, Rational, StrategicGame, StrategyId};
use crate::limits;
use num_rational::BigRational;
use num_traits::FromPrimitive;

/// A `StrategicGame` that has passed every check in [`ValidStrategicGame::validate`],
/// together with a dense payoff table in exact rational arithmetic.
///
/// Construction is the only way to get one, so any function taking this type
/// may assume: every profile has exactly one outcome, every payoff vector has
/// one entry per player, every payoff is finite, and all limits hold.
#[derive(Debug, Clone)]
pub struct ValidStrategicGame {
    game: StrategicGame,
    /// Indexed by linear profile index; each entry has one payoff per player.
    payoffs: Vec<Vec<Rational>>,
    /// Row-major strides: `strides[i]` is the step for player `i`'s index.
    strides: Vec<usize>,
}

impl ValidStrategicGame {
    pub fn validate(game: StrategicGame) -> Result<Self, GtError> {
        let n_players = game.players.len();

        // Limits first: everything after this allocates proportionally to size.
        if n_players > limits::MAX_PLAYERS {
            return Err(GtError::GameTooLarge {
                field: "players",
                limit: limits::MAX_PLAYERS,
                actual: n_players,
            });
        }
        if n_players == 0 {
            return Err(GtError::invalid(
                DiagnosticCode::NoPlayers,
                "a game needs at least one player",
            ));
        }
        if game.strategies.len() != n_players {
            return Err(GtError::invalid(
                DiagnosticCode::StrategyListArityMismatch,
                format!(
                    "there are {n_players} players but {} strategy lists",
                    game.strategies.len()
                ),
            ));
        }
        for (p, set) in game.strategies.iter().enumerate() {
            if set.len() > limits::MAX_STRATEGIES_PER_PLAYER {
                return Err(GtError::GameTooLarge {
                    field: "strategies per player",
                    limit: limits::MAX_STRATEGIES_PER_PLAYER,
                    actual: set.len(),
                });
            }
            if set.is_empty() {
                return Err(GtError::invalid(
                    DiagnosticCode::EmptyStrategySet,
                    format!("player {p} has no strategies"),
                ));
            }
        }

        let total_profiles: usize = game.strategies.iter().map(Vec::len).product();
        if total_profiles > limits::MAX_PROFILES {
            return Err(GtError::GameTooLarge {
                field: "total strategy profiles",
                limit: limits::MAX_PROFILES,
                actual: total_profiles,
            });
        }

        // Structural problems are collected, not short-circuited: a caller
        // fixing a malformed game should see every problem in one pass.
        let mut diagnostics = Vec::new();

        for (p, player) in game.players.iter().enumerate() {
            if player.id != p {
                diagnostics.push(Diagnostic {
                    code: DiagnosticCode::PlayerIdMismatch,
                    message: format!(
                        "player at position {p} declares id {}; ids must equal position",
                        player.id
                    ),
                });
            }
        }

        for (p, set) in game.strategies.iter().enumerate() {
            for i in 0..set.len() {
                if set[..i].contains(&set[i]) {
                    diagnostics.push(Diagnostic {
                        code: DiagnosticCode::DuplicateStrategyLabel,
                        message: format!(
                            "player {p} has two strategies labelled {:?}", set[i]
                        ),
                    });
                }
            }
        }

        let strides = compute_strides(&game.strategies);
        let mut table: Vec<Option<Vec<Rational>>> = vec![None; total_profiles];

        for outcome in &game.outcomes {
            if outcome.profile.len() != n_players {
                diagnostics.push(Diagnostic {
                    code: DiagnosticCode::ProfileArityMismatch,
                    message: format!(
                        "profile {:?} names {} strategies, expected {n_players}",
                        outcome.profile,
                        outcome.profile.len()
                    ),
                });
                continue;
            }
            if outcome.payoffs.len() != n_players {
                diagnostics.push(Diagnostic {
                    code: DiagnosticCode::PayoffArityMismatch,
                    message: format!(
                        "profile {:?} has {} payoffs, expected {n_players}",
                        outcome.profile,
                        outcome.payoffs.len()
                    ),
                });
                continue;
            }
            let mut in_range = true;
            for (p, &s) in outcome.profile.iter().enumerate() {
                if s >= game.strategies[p].len() {
                    diagnostics.push(Diagnostic {
                        code: DiagnosticCode::StrategyIndexOutOfRange,
                        message: format!(
                            "profile {:?} uses strategy {s} for player {p}, who has {}",
                            outcome.profile,
                            game.strategies[p].len()
                        ),
                    });
                    in_range = false;
                }
            }
            if !in_range {
                continue;
            }

            let mut exact = Vec::with_capacity(n_players);
            let mut finite = true;
            for (p, &u) in outcome.payoffs.iter().enumerate() {
                match BigRational::from_f64(u) {
                    Some(r) => exact.push(r),
                    None => {
                        finite = false;
                        diagnostics.push(Diagnostic {
                            code: DiagnosticCode::NonFinitePayoff,
                            message: format!(
                                "profile {:?} gives player {p} a non-finite payoff",
                                outcome.profile
                            ),
                        });
                    }
                }
            }
            if !finite {
                continue;
            }

            let idx = linear_index(&outcome.profile, &strides);
            if table[idx].is_some() {
                diagnostics.push(Diagnostic {
                    code: DiagnosticCode::DuplicateProfile,
                    message: format!("profile {:?} appears more than once", outcome.profile),
                });
                continue;
            }
            table[idx] = Some(exact);
        }

        for (idx, slot) in table.iter().enumerate() {
            if slot.is_none() {
                diagnostics.push(Diagnostic {
                    code: DiagnosticCode::MissingProfile,
                    message: format!(
                        "no outcome given for profile {:?}",
                        profile_from_index(idx, &strides, &game.strategies)
                    ),
                });
            }
        }

        if !diagnostics.is_empty() {
            return Err(GtError::InvalidGame { diagnostics });
        }

        let payoffs = table
            .into_iter()
            .map(|slot| slot.expect("validated above: every profile is filled"))
            .collect();

        Ok(ValidStrategicGame { game, payoffs, strides })
    }

    pub fn n_players(&self) -> usize {
        self.game.players.len()
    }

    pub fn n_strategies(&self, player: PlayerId) -> usize {
        self.game.strategies[player].len()
    }

    pub fn index_of(&self, profile: &[StrategyId]) -> usize {
        linear_index(profile, &self.strides)
    }

    pub fn payoff(&self, profile: &[StrategyId], player: PlayerId) -> &Rational {
        &self.payoffs[self.index_of(profile)][player]
    }

    pub fn payoffs_at(&self, profile: &[StrategyId]) -> &[Rational] {
        &self.payoffs[self.index_of(profile)]
    }

    pub fn profiles(&self) -> impl Iterator<Item = Profile> + '_ {
        let total: usize = self.game.strategies.iter().map(Vec::len).product();
        (0..total).map(move |i| profile_from_index(i, &self.strides, &self.game.strategies))
    }

    pub fn payoff_kind(&self) -> PayoffKind {
        self.game.payoff_kind
    }

    pub fn game(&self) -> &StrategicGame {
        &self.game
    }

    pub fn player_name(&self, player: PlayerId) -> &str {
        &self.game.players[player].name
    }

    pub fn strategy_name(&self, player: PlayerId, strategy: StrategyId) -> &str {
        &self.game.strategies[player][strategy]
    }

    /// Rejects ordinal payoffs for tools that take expectations.
    pub fn require_cardinal(&self, tool: &'static str) -> Result<(), GtError> {
        match self.game.payoff_kind {
            PayoffKind::Cardinal => Ok(()),
            PayoffKind::Ordinal => Err(GtError::OrdinalPayoffsRejected { tool }),
        }
    }
}

/// Row-major strides: the last player's index varies fastest.
fn compute_strides(strategies: &[Vec<String>]) -> Vec<usize> {
    let mut strides = vec![1usize; strategies.len()];
    for p in (0..strategies.len().saturating_sub(1)).rev() {
        strides[p] = strides[p + 1] * strategies[p + 1].len();
    }
    strides
}

fn linear_index(profile: &[StrategyId], strides: &[usize]) -> usize {
    profile.iter().zip(strides).map(|(s, stride)| s * stride).sum()
}

fn profile_from_index(mut idx: usize, strides: &[usize], strategies: &[Vec<String>]) -> Profile {
    let mut profile = Vec::with_capacity(strides.len());
    for (p, stride) in strides.iter().enumerate() {
        let s = idx / stride;
        idx %= stride;
        debug_assert!(s < strategies[p].len());
        profile.push(s);
    }
    profile
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DiagnosticCode;
    use crate::game::{Outcome, PayoffKind, Player, StrategicGame};
    use num_traits::One;

    fn two_by_two(payoffs: [[f64; 2]; 4]) -> StrategicGame {
        StrategicGame {
            players: vec![
                Player { id: 0, name: "Row".into() },
                Player { id: 1, name: "Col".into() },
            ],
            strategies: vec![
                vec!["T".into(), "B".into()],
                vec!["L".into(), "R".into()],
            ],
            outcomes: vec![
                Outcome { profile: vec![0, 0], payoffs: payoffs[0].to_vec() },
                Outcome { profile: vec![0, 1], payoffs: payoffs[1].to_vec() },
                Outcome { profile: vec![1, 0], payoffs: payoffs[2].to_vec() },
                Outcome { profile: vec![1, 1], payoffs: payoffs[3].to_vec() },
            ],
            payoff_kind: PayoffKind::Cardinal,
        }
    }

    fn codes(err: &GtError) -> Vec<DiagnosticCode> {
        match err {
            GtError::InvalidGame { diagnostics } => {
                diagnostics.iter().map(|d| d.code).collect()
            }
            other => panic!("expected InvalidGame, got {other:?}"),
        }
    }

    #[test]
    fn a_well_formed_game_validates() {
        let g = ValidStrategicGame::validate(two_by_two([
            [3.0, 3.0], [0.0, 4.0], [4.0, 0.0], [1.0, 1.0],
        ]))
        .expect("valid");
        assert_eq!(g.n_players(), 2);
        assert_eq!(g.n_strategies(0), 2);
        assert_eq!(g.n_strategies(1), 2);
    }

    #[test]
    fn payoffs_are_looked_up_by_profile() {
        let g = ValidStrategicGame::validate(two_by_two([
            [3.0, 3.0], [0.0, 4.0], [4.0, 0.0], [1.0, 1.0],
        ]))
        .expect("valid");
        assert_eq!(*g.payoff(&[1, 0], 0), Rational::from_integer(4.into()));
        assert_eq!(*g.payoff(&[1, 0], 1), Rational::from_integer(0.into()));
        assert_eq!(g.payoffs_at(&[0, 1]).len(), 2);
    }

    #[test]
    fn fractional_payoffs_convert_exactly() {
        // 0.5 is exactly representable in binary, so this must be exactly 1/2.
        let g = ValidStrategicGame::validate(two_by_two([
            [0.5, 0.0], [0.0, 0.0], [0.0, 0.0], [0.0, 0.0],
        ]))
        .expect("valid");
        let half = Rational::new(1.into(), 2.into());
        assert_eq!(*g.payoff(&[0, 0], 0), half);
    }

    #[test]
    fn profiles_enumerates_every_combination_once() {
        let g = ValidStrategicGame::validate(two_by_two([
            [1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0],
        ]))
        .expect("valid");
        let all: Vec<_> = g.profiles().collect();
        assert_eq!(all, vec![vec![0, 0], vec![0, 1], vec![1, 0], vec![1, 1]]);
    }

    #[test]
    fn a_missing_profile_is_reported() {
        let mut game = two_by_two([[1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0]]);
        game.outcomes.pop();
        let err = ValidStrategicGame::validate(game).expect_err("incomplete");
        assert!(codes(&err).contains(&DiagnosticCode::MissingProfile));
    }

    #[test]
    fn a_duplicated_profile_is_reported() {
        let mut game = two_by_two([[1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0]]);
        game.outcomes[3].profile = vec![0, 0];
        let err = ValidStrategicGame::validate(game).expect_err("duplicate");
        let found = codes(&err);
        assert!(found.contains(&DiagnosticCode::DuplicateProfile));
        assert!(found.contains(&DiagnosticCode::MissingProfile));
    }

    #[test]
    fn duplicate_strategy_labels_are_reported() {
        let mut game = two_by_two([[1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0]]);
        game.strategies[0] = vec!["T".into(), "T".into()];
        let err = ValidStrategicGame::validate(game).expect_err("duplicate label");
        assert!(codes(&err).contains(&DiagnosticCode::DuplicateStrategyLabel));
    }

    #[test]
    fn a_payoff_vector_of_the_wrong_length_is_reported() {
        let mut game = two_by_two([[1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0]]);
        game.outcomes[0].payoffs = vec![1.0];
        let err = ValidStrategicGame::validate(game).expect_err("arity");
        assert!(codes(&err).contains(&DiagnosticCode::PayoffArityMismatch));
    }

    #[test]
    fn a_nan_payoff_is_reported() {
        let mut game = two_by_two([[1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0]]);
        game.outcomes[0].payoffs[0] = f64::NAN;
        let err = ValidStrategicGame::validate(game).expect_err("nan");
        assert!(codes(&err).contains(&DiagnosticCode::NonFinitePayoff));
    }

    #[test]
    fn all_problems_are_reported_together_not_just_the_first() {
        let mut game = two_by_two([[1.0, 1.0], [1.0, 1.0], [1.0, 1.0], [1.0, 1.0]]);
        game.strategies[0] = vec!["T".into(), "T".into()];
        game.outcomes[0].payoffs = vec![1.0];
        let err = ValidStrategicGame::validate(game).expect_err("two problems");
        let found = codes(&err);
        assert!(found.contains(&DiagnosticCode::DuplicateStrategyLabel));
        assert!(found.contains(&DiagnosticCode::PayoffArityMismatch));
    }

    #[test]
    fn too_many_players_names_the_limit_and_the_actual_value() {
        let n = crate::limits::MAX_PLAYERS + 1;
        let game = StrategicGame {
            players: (0..n).map(|id| Player { id, name: format!("P{id}") }).collect(),
            strategies: vec![vec!["a".into()]; n],
            outcomes: vec![Outcome { profile: vec![0; n], payoffs: vec![0.0; n] }],
            payoff_kind: PayoffKind::Cardinal,
        };
        match ValidStrategicGame::validate(game) {
            Err(GtError::GameTooLarge { field, limit, actual }) => {
                assert_eq!(field, "players");
                assert_eq!(limit, crate::limits::MAX_PLAYERS);
                assert_eq!(actual, n);
            }
            other => panic!("expected GameTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn one_is_one() {
        // Guards against a num-traits import that compiles but is unused.
        assert_eq!(Rational::one(), Rational::from_integer(1.into()));
    }
}
