//! Iterated deletion of dominated strategies (Bonanno §1.2, §1.5).
//!
//! Strict deletion is order-independent — any elimination order reaches the
//! same reduced game. Weak deletion is not, so its result is reported as one
//! valid reduction rather than the reduction.

use crate::game::{PlayerId, Profile, StrategyId, ValidStrategicGame};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DominanceMode {
    Strict,
    Weak,
}

/// One deletion, in the round it happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EliminationStep {
    pub round: usize,
    pub player: PlayerId,
    pub eliminated: StrategyId,
    pub dominated_by: StrategyId,
    pub mode: DominanceMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DominanceResult {
    /// Surviving strategies per player, ascending.
    pub surviving: Vec<Vec<StrategyId>>,
    pub steps: Vec<EliminationStep>,
    /// `Some` when exactly one strategy survives for every player.
    pub unique_profile: Option<Profile>,
    /// True for weak deletion: a different elimination order may give a
    /// different surviving set.
    pub order_dependent: bool,
}

pub fn solve_dominance(game: &ValidStrategicGame, mode: DominanceMode) -> DominanceResult {
    let n = game.n_players();
    let mut surviving: Vec<Vec<StrategyId>> =
        (0..n).map(|p| (0..game.n_strategies(p)).collect()).collect();
    let mut steps = Vec::new();
    let mut round = 0usize;

    loop {
        round += 1;
        let mut eliminated_this_round = false;

        for p in 0..n {
            // Recomputed each pass: a deletion can create new dominance.
            if let Some((victim, dominator)) = find_dominated(game, &surviving, p, mode) {
                surviving[p].retain(|&s| s != victim);
                steps.push(EliminationStep {
                    round,
                    player: p,
                    eliminated: victim,
                    dominated_by: dominator,
                    mode,
                });
                eliminated_this_round = true;
            }
        }

        if !eliminated_this_round {
            break;
        }
    }

    let unique_profile = if surviving.iter().all(|s| s.len() == 1) {
        Some(surviving.iter().map(|s| s[0]).collect())
    } else {
        None
    };

    DominanceResult {
        surviving,
        steps,
        unique_profile,
        order_dependent: matches!(mode, DominanceMode::Weak),
    }
}

/// First `(dominated, dominator)` pair found for `player` in the reduced game.
fn find_dominated(
    game: &ValidStrategicGame,
    surviving: &[Vec<StrategyId>],
    player: PlayerId,
    mode: DominanceMode,
) -> Option<(StrategyId, StrategyId)> {
    let own = &surviving[player];
    for &candidate in own {
        for &alternative in own {
            if candidate == alternative {
                continue;
            }
            if dominates(game, surviving, player, alternative, candidate, mode) {
                return Some((candidate, alternative));
            }
        }
    }
    None
}

/// Does `better` dominate `worse` for `player`, over the surviving profiles
/// of the other players?
fn dominates(
    game: &ValidStrategicGame,
    surviving: &[Vec<StrategyId>],
    player: PlayerId,
    better: StrategyId,
    worse: StrategyId,
    mode: DominanceMode,
) -> bool {
    let mut strict_somewhere = false;

    for others in others_profiles(surviving, player) {
        let mut with_better = others.clone();
        let mut with_worse = others;
        with_better[player] = better;
        with_worse[player] = worse;

        let u_better = game.payoff(&with_better, player);
        let u_worse = game.payoff(&with_worse, player);

        match mode {
            DominanceMode::Strict => {
                if u_better <= u_worse {
                    return false;
                }
            }
            DominanceMode::Weak => {
                if u_better < u_worse {
                    return false;
                }
                if u_better > u_worse {
                    strict_somewhere = true;
                }
            }
        }
    }

    match mode {
        DominanceMode::Strict => true,
        // Weak dominance needs at least one strictly better case; without it
        // the two strategies are payoff-equivalent and neither dominates.
        DominanceMode::Weak => strict_somewhere,
    }
}

/// Every surviving profile, with `player`'s own slot left at a placeholder for
/// the caller to overwrite.
fn others_profiles(surviving: &[Vec<StrategyId>], player: PlayerId) -> Vec<Profile> {
    let n = surviving.len();
    let mut out = vec![vec![0usize; n]];

    for p in 0..n {
        if p == player {
            continue;
        }
        let mut next = Vec::with_capacity(out.len() * surviving[p].len());
        for base in &out {
            for &s in &surviving[p] {
                let mut extended = base.clone();
                extended[p] = s;
                next.push(extended);
            }
        }
        out = next;
    }

    out
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

    /// Prisoner's Dilemma: Defect strictly dominates Cooperate for both.
    fn prisoners_dilemma() -> ValidStrategicGame {
        game(
            vec!["Cooperate", "Defect"],
            vec!["Cooperate", "Defect"],
            vec![
                vec![[3.0, 3.0], [0.0, 4.0]],
                vec![[4.0, 0.0], [1.0, 1.0]],
            ],
        )
    }

    #[test]
    fn strict_dominance_solves_the_prisoners_dilemma_to_a_single_profile() {
        let result = solve_dominance(&prisoners_dilemma(), DominanceMode::Strict);
        assert_eq!(result.surviving, vec![vec![1], vec![1]]);
        assert_eq!(result.unique_profile, Some(vec![1, 1]));
        assert_eq!(result.steps.len(), 2, "one elimination per player");
        assert!(!result.order_dependent);
    }

    #[test]
    fn the_trace_names_the_dominating_strategy() {
        let result = solve_dominance(&prisoners_dilemma(), DominanceMode::Strict);
        let first = &result.steps[0];
        assert_eq!(first.eliminated, 0, "Cooperate is eliminated");
        assert_eq!(first.dominated_by, 1, "by Defect");
        assert_eq!(first.round, 1);
    }

    #[test]
    fn a_game_with_no_dominated_strategies_eliminates_nothing() {
        // Matching Pennies: no dominance in either direction.
        let mp = game(
            vec!["Heads", "Tails"],
            vec!["Heads", "Tails"],
            vec![
                vec![[1.0, -1.0], [-1.0, 1.0]],
                vec![[-1.0, 1.0], [1.0, -1.0]],
            ],
        );
        let result = solve_dominance(&mp, DominanceMode::Strict);
        assert!(result.steps.is_empty());
        assert_eq!(result.surviving, vec![vec![0, 1], vec![0, 1]]);
        assert_eq!(result.unique_profile, None);
    }

    #[test]
    fn elimination_iterates_a_strategy_dominated_only_after_an_earlier_deletion() {
        // Row: T dominates nothing initially; after Col's R goes, T beats B.
        // Col: L strictly dominates R (3>2, 3>1). Then for Row against L: T=2 < B=3?
        // Construct so that round 2 has a Row elimination.
        let g = game(
            vec!["T", "B"],
            vec!["L", "R"],
            vec![
                vec![[2.0, 3.0], [4.0, 2.0]],
                vec![[3.0, 3.0], [1.0, 1.0]],
            ],
        );
        let result = solve_dominance(&g, DominanceMode::Strict);
        // Col: L gives 3,3 vs R giving 2,1 — L strictly dominates R.
        assert!(result.steps.iter().any(|s| s.player == 1 && s.eliminated == 1));
        // With R gone, Row compares T=2 against B=3 — B strictly dominates T.
        assert!(result
            .steps
            .iter()
            .any(|s| s.player == 0 && s.eliminated == 0 && s.round >= 2));
        assert_eq!(result.unique_profile, Some(vec![1, 0]));
    }

    #[test]
    fn weak_dominance_is_flagged_as_order_dependent() {
        let g = game(
            vec!["T", "B"],
            vec!["L", "R"],
            vec![
                vec![[1.0, 1.0], [1.0, 0.0]],
                vec![[0.0, 1.0], [1.0, 1.0]],
            ],
        );
        let result = solve_dominance(&g, DominanceMode::Weak);
        assert!(
            result.order_dependent,
            "iterated weak deletion can depend on elimination order"
        );
    }

    #[test]
    fn weak_dominance_eliminates_where_strict_dominance_does_not() {
        // T weakly dominates B: equal against L, strictly better against R.
        let g = game(
            vec!["T", "B"],
            vec!["L", "R"],
            vec![
                vec![[1.0, 0.0], [2.0, 0.0]],
                vec![[1.0, 0.0], [0.0, 0.0]],
            ],
        );
        let strict = solve_dominance(&g, DominanceMode::Strict);
        assert!(strict.steps.is_empty(), "no strict dominance here");

        let weak = solve_dominance(&g, DominanceMode::Weak);
        assert!(weak.steps.iter().any(|s| s.player == 0 && s.eliminated == 1));
    }

    #[test]
    fn dominance_works_on_ordinal_payoffs() {
        // Dominance is an ordinal notion — no expectation is taken.
        let form = MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: vec!["C".into(), "D".into()],
            col_strategies: vec!["C".into(), "D".into()],
            payoff_matrix: vec![
                vec![[3.0, 3.0], [0.0, 4.0]],
                vec![[4.0, 0.0], [1.0, 1.0]],
            ],
            payoff_kind: PayoffKind::Ordinal,
        };
        let g = ValidStrategicGame::validate(StrategicGame::try_from(form).unwrap()).unwrap();
        let result = solve_dominance(&g, DominanceMode::Strict);
        assert_eq!(result.unique_profile, Some(vec![1, 1]));
    }
}
