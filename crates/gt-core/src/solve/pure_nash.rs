//! Pure-strategy Nash equilibrium (Bonanno §1.6).
//!
//! A profile is an equilibrium when no player can gain by changing only their
//! own strategy. Checking that directly is O(profiles x players x strategies),
//! which is fine inside the size limits and keeps the derivation honest: every
//! rejected profile carries the deviation that rejected it.

use crate::game::{PlayerId, Profile, Rational, StrategyId, ValidStrategicGame};
use serde::{Deserialize, Serialize};

/// A unilateral change that makes one player strictly better off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deviation {
    pub player: PlayerId,
    pub from: StrategyId,
    pub to: StrategyId,
    pub gain: Rational,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileCheck {
    pub profile: Profile,
    pub is_equilibrium: bool,
    /// The deviation that disqualified this profile, if any. `None` exactly
    /// when `is_equilibrium` is true.
    pub blocking_deviation: Option<Deviation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PureNashResult {
    pub equilibria: Vec<Profile>,
    pub checks: Vec<ProfileCheck>,
}

pub fn solve_pure_nash(game: &ValidStrategicGame) -> PureNashResult {
    let mut equilibria = Vec::new();
    let mut checks = Vec::new();

    for profile in game.profiles() {
        let blocking = (0..game.n_players())
            .filter_map(|p| profitable_deviation(game, &profile, p))
            .max_by(|a, b| a.gain.cmp(&b.gain));

        let is_equilibrium = blocking.is_none();
        if is_equilibrium {
            equilibria.push(profile.clone());
        }
        checks.push(ProfileCheck {
            profile,
            is_equilibrium,
            blocking_deviation: blocking,
        });
    }

    PureNashResult { equilibria, checks }
}

/// The most profitable unilateral deviation available to `player` at `profile`,
/// or `None` when they are already playing a best response.
pub fn profitable_deviation(
    game: &ValidStrategicGame,
    profile: &[StrategyId],
    player: PlayerId,
) -> Option<Deviation> {
    let current = profile[player];
    let u_current = game.payoff(profile, player).clone();
    let mut best: Option<Deviation> = None;

    for alternative in 0..game.n_strategies(player) {
        if alternative == current {
            continue;
        }
        let mut deviated = profile.to_vec();
        deviated[player] = alternative;
        let u_alternative = game.payoff(&deviated, player);

        if *u_alternative > u_current {
            let gain = u_alternative - &u_current;
            let better = match &best {
                Some(existing) => gain > existing.gain,
                None => true,
            };
            if better {
                best = Some(Deviation {
                    player,
                    from: current,
                    to: alternative,
                    gain,
                });
            }
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{MatrixForm, PayoffKind, StrategicGame, ValidStrategicGame};

    fn game(rows: Vec<&str>, cols: Vec<&str>, m: Vec<Vec<[f64; 2]>>) -> ValidStrategicGame {
        let form = MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: rows.into_iter().map(String::from).collect(),
            col_strategies: cols.into_iter().map(String::from).collect(),
            payoff_matrix: m,
            payoff_kind: PayoffKind::Cardinal,
        };
        ValidStrategicGame::validate(StrategicGame::try_from(form).expect("converts"))
            .expect("valid")
    }

    #[test]
    fn the_prisoners_dilemma_has_exactly_one_pure_equilibrium() {
        let pd = game(
            vec!["Cooperate", "Defect"],
            vec!["Cooperate", "Defect"],
            vec![vec![[3.0, 3.0], [0.0, 4.0]], vec![[4.0, 0.0], [1.0, 1.0]]],
        );
        let result = solve_pure_nash(&pd);
        assert_eq!(result.equilibria, vec![vec![1, 1]]);
    }

    #[test]
    fn battle_of_the_sexes_has_two_pure_equilibria() {
        let bos = game(
            vec!["Opera", "Football"],
            vec!["Opera", "Football"],
            vec![vec![[2.0, 1.0], [0.0, 0.0]], vec![[0.0, 0.0], [1.0, 2.0]]],
        );
        let result = solve_pure_nash(&bos);
        assert_eq!(result.equilibria, vec![vec![0, 0], vec![1, 1]]);
    }

    #[test]
    fn matching_pennies_has_no_pure_equilibrium() {
        let mp = game(
            vec!["Heads", "Tails"],
            vec!["Heads", "Tails"],
            vec![
                vec![[1.0, -1.0], [-1.0, 1.0]],
                vec![[-1.0, 1.0], [1.0, -1.0]],
            ],
        );
        let result = solve_pure_nash(&mp);
        assert!(
            result.equilibria.is_empty(),
            "no pure equilibrium is a result, not an error"
        );
        assert!(result.checks.iter().all(|c| !c.is_equilibrium));
        assert!(result.checks.iter().all(|c| c.blocking_deviation.is_some()));
    }

    #[test]
    fn a_non_equilibrium_reports_the_profitable_deviation() {
        let pd = game(
            vec!["Cooperate", "Defect"],
            vec!["Cooperate", "Defect"],
            vec![vec![[3.0, 3.0], [0.0, 4.0]], vec![[4.0, 0.0], [1.0, 1.0]]],
        );
        let result = solve_pure_nash(&pd);
        let cc = result
            .checks
            .iter()
            .find(|c| c.profile == vec![0, 0])
            .expect("profile checked");
        let dev = cc.blocking_deviation.as_ref().expect("CC is not stable");
        assert_eq!(dev.from, 0);
        assert_eq!(dev.to, 1);
        assert_eq!(dev.gain, Rational::from_integer(1.into()), "4 - 3");
    }

    #[test]
    fn three_player_games_are_supported() {
        // Each of three players picks 0 or 1; everyone is paid 1 iff all agree.
        let mut outcomes = Vec::new();
        for a in 0..2 {
            for b in 0..2 {
                for c in 0..2 {
                    let agree = a == b && b == c;
                    let u = if agree { 1.0 } else { 0.0 };
                    outcomes.push(crate::game::Outcome {
                        profile: vec![a, b, c],
                        payoffs: vec![u, u, u],
                    });
                }
            }
        }
        let g = ValidStrategicGame::validate(StrategicGame {
            players: (0..3)
                .map(|id| crate::game::Player {
                    id,
                    name: format!("P{id}"),
                })
                .collect(),
            strategies: vec![vec!["A".into(), "B".into()]; 3],
            outcomes,
            payoff_kind: PayoffKind::Cardinal,
        })
        .expect("valid");

        let result = solve_pure_nash(&g);
        assert_eq!(result.equilibria, vec![vec![0, 0, 0], vec![1, 1, 1]]);
    }

    #[test]
    fn profitable_deviation_returns_the_largest_gain() {
        let g = game(
            vec!["T", "B"],
            vec!["L", "R"],
            vec![vec![[0.0, 0.0], [0.0, 0.0]], vec![[5.0, 0.0], [0.0, 0.0]]],
        );
        let dev = profitable_deviation(&g, &[0, 0], 0).expect("Row can improve");
        assert_eq!(dev.to, 1);
        assert_eq!(dev.gain, Rational::from_integer(5.into()));
    }
}
