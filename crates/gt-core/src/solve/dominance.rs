//! Iterated deletion of dominated strategies (Bonanno §1.2, §1.5, §5.4).
//!
//! Strict deletion is order-independent — any elimination order reaches the
//! same reduced game. Weak deletion is not, so its result is reported as one
//! valid reduction rather than the reduction.
//!
//! On cardinal games, strict deletion also tests dominance by *mixed*
//! strategies (§5.4): a pure strategy can be strictly dominated by a mixture
//! while no single pure strategy dominates it. That test is an LP feasibility
//! check per candidate, so pure dominance is tried first and the LP only runs
//! when it finds nothing.

use crate::exact::{solve_lp, LpProblem, LpSolution};
use crate::game::{PayoffKind, PlayerId, Profile, Rational, StrategyId, ValidStrategicGame};
use crate::limits;
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DominanceMode {
    Strict,
    Weak,
}

/// What beat the eliminated strategy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dominator {
    Pure(StrategyId),
    /// Probabilities over the player's full strategy list, zero off the
    /// support, summing to one.
    Mixed(Vec<Rational>),
}

/// One deletion, in the round it happened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EliminationStep {
    pub round: usize,
    pub player: PlayerId,
    pub eliminated: StrategyId,
    pub dominated_by: Dominator,
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
    /// True when dominance by mixed strategies was tested (Bonanno §5.4).
    /// False for ordinal payoffs, for weak mode, and when the reduced game
    /// exceeded `limits::MAX_MIXED_DOMINANCE_PROFILES` — the result is then
    /// pure dominance only, and says so rather than implying completeness.
    pub mixed_dominance_checked: bool,
}

pub fn solve_dominance(game: &ValidStrategicGame, mode: DominanceMode) -> DominanceResult {
    let n = game.n_players();
    let mut surviving: Vec<Vec<StrategyId>> = (0..n)
        .map(|p| (0..game.n_strategies(p)).collect())
        .collect();
    let mut steps = Vec::new();
    let mut round = 0usize;

    // Expected utility over ordinal ranks is meaningless, and mixed *weak*
    // dominance is not defined in this version.
    let mut mixed_enabled =
        matches!(mode, DominanceMode::Strict) && game.payoff_kind() == PayoffKind::Cardinal;
    let mut mixed_ran = false;

    loop {
        round += 1;
        let mut eliminated_this_round = false;

        for p in 0..n {
            // Recomputed each pass: a deletion can create new dominance.
            let found = find_dominated(game, &surviving, p, mode)
                .map(|(victim, dominator)| (victim, Dominator::Pure(dominator)))
                .or_else(|| {
                    if !mixed_enabled {
                        return None;
                    }
                    if others_profile_count(&surviving, p) > limits::MAX_MIXED_DOMINANCE_PROFILES {
                        mixed_enabled = false;
                        return None;
                    }
                    mixed_ran = true;
                    surviving[p].clone().into_iter().find_map(|candidate| {
                        strictly_dominated_by_mixture(game, &surviving, p, candidate)
                            .map(|mix| (candidate, Dominator::Mixed(mix)))
                    })
                });

            if let Some((victim, dominated_by)) = found {
                surviving[p].retain(|&s| s != victim);
                steps.push(EliminationStep {
                    round,
                    player: p,
                    eliminated: victim,
                    dominated_by,
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
        mixed_dominance_checked: mixed_ran,
    }
}

/// How many joint profiles the other players still have.
fn others_profile_count(surviving: &[Vec<StrategyId>], player: PlayerId) -> usize {
    surviving
        .iter()
        .enumerate()
        .filter(|(p, _)| *p != player)
        .map(|(_, s)| s.len())
        .product()
}

/// Is `candidate` strictly dominated by some mixture over `player`'s other
/// surviving strategies, against every surviving opponent profile?
///
/// Returns the dominating mixture as a full-length distribution over
/// `player`'s strategies (zero off the support), or `None` if no mixture
/// dominates. Bonanno §5.4 (p. 201): this is strictly stronger than pure
/// dominance.
///
/// The feasibility question — is there a mixture beating `candidate` against
/// every opponent profile? — is turned into an optimization by maximizing the
/// margin `eps`, so a strictly positive optimum is exactly strict dominance.
pub fn strictly_dominated_by_mixture(
    game: &ValidStrategicGame,
    surviving: &[Vec<StrategyId>],
    player: PlayerId,
    candidate: StrategyId,
) -> Option<Vec<Rational>> {
    let mixers: Vec<StrategyId> = surviving[player]
        .iter()
        .copied()
        .filter(|&s| s != candidate)
        .collect();
    if mixers.is_empty() {
        return None;
    }

    let opponents = others_profiles(surviving, player);
    let m = mixers.len();
    let j = opponents.len();
    // Columns: y_0..y_{m-1}, eps, slack_0..slack_{j-1}.
    let cols = m + 1 + j;
    let eps_col = m;

    let mut constraints: Vec<Vec<Rational>> = Vec::with_capacity(j + 1);
    let mut rhs: Vec<Rational> = Vec::with_capacity(j + 1);

    for (index, others) in opponents.iter().enumerate() {
        let mut row = vec![Rational::zero(); cols];
        for (k, &mixer) in mixers.iter().enumerate() {
            let mut profile = others.clone();
            profile[player] = mixer;
            row[k] = game.payoff(&profile, player).clone();
        }
        row[eps_col] = -Rational::one();
        row[m + 1 + index] = -Rational::one();
        constraints.push(row);

        let mut profile = others.clone();
        profile[player] = candidate;
        rhs.push(game.payoff(&profile, player).clone());
    }

    // The mixture is a probability distribution.
    let mut sum_row = vec![Rational::zero(); cols];
    for entry in sum_row.iter_mut().take(m) {
        *entry = Rational::one();
    }
    constraints.push(sum_row);
    rhs.push(Rational::one());

    let mut objective = vec![Rational::zero(); cols];
    objective[eps_col] = Rational::one();

    match solve_lp(LpProblem {
        objective,
        constraints,
        rhs,
    }) {
        LpSolution::Optimal { x, value } if value > Rational::zero() => {
            let mut probs = vec![Rational::zero(); game.n_strategies(player)];
            for (k, &mixer) in mixers.iter().enumerate() {
                probs[mixer] = x[k].clone();
            }
            Some(probs)
        }
        // Zero margin means no *strict* domination; infeasible and unbounded
        // cannot occur for this program (the mixture lies in a simplex and the
        // payoffs are finite), and are treated as "not dominated" rather than
        // silently retried.
        _ => None,
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
    use crate::game::{MatrixForm, PayoffKind, Rational, StrategicGame, ValidStrategicGame};
    use num_traits::{One, Zero};

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
            vec![vec![[3.0, 3.0], [0.0, 4.0]], vec![[4.0, 0.0], [1.0, 1.0]]],
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
        assert_eq!(first.dominated_by, Dominator::Pure(1), "by Defect");
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
            vec![vec![[2.0, 3.0], [4.0, 2.0]], vec![[3.0, 3.0], [1.0, 1.0]]],
        );
        let result = solve_dominance(&g, DominanceMode::Strict);
        // Col: L gives 3,3 vs R giving 2,1 — L strictly dominates R.
        assert!(result
            .steps
            .iter()
            .any(|s| s.player == 1 && s.eliminated == 1));
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
            vec![vec![[1.0, 1.0], [1.0, 0.0]], vec![[0.0, 1.0], [1.0, 1.0]]],
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
            vec![vec![[1.0, 0.0], [2.0, 0.0]], vec![[1.0, 0.0], [0.0, 0.0]]],
        );
        let strict = solve_dominance(&g, DominanceMode::Strict);
        assert!(strict.steps.is_empty(), "no strict dominance here");

        let weak = solve_dominance(&g, DominanceMode::Weak);
        assert!(weak
            .steps
            .iter()
            .any(|s| s.player == 0 && s.eliminated == 1));
    }

    /// Row's C is beaten by no pure strategy — A wins in the left column, B in
    /// the right — but the even mixture of A and B gives 3/2 in both columns,
    /// against C's 1. This is the §5.4 structure (concept at p. 201); the
    /// payoff numbers here were constructed for this crate.
    fn mixed_dominance_game() -> ValidStrategicGame {
        game(
            vec!["A", "B", "C"],
            vec!["L", "R"],
            vec![
                vec![[3.0, 0.0], [0.0, 0.0]],
                vec![[0.0, 0.0], [3.0, 0.0]],
                vec![[1.0, 0.0], [1.0, 0.0]],
            ],
        )
    }

    #[test]
    fn no_pure_strategy_dominates_the_candidate() {
        let g = mixed_dominance_game();
        let surviving = vec![vec![0, 1, 2], vec![0, 1]];
        assert!(
            !dominates(&g, &surviving, 0, 0, 2, DominanceMode::Strict),
            "A does not dominate C: A gets 0 against R, C gets 1"
        );
        assert!(
            !dominates(&g, &surviving, 0, 1, 2, DominanceMode::Strict),
            "B does not dominate C: B gets 0 against L, C gets 1"
        );
    }

    #[test]
    fn a_mixture_of_a_and_b_strictly_dominates_c() {
        let g = mixed_dominance_game();
        let surviving = vec![vec![0, 1, 2], vec![0, 1]];
        let mix = strictly_dominated_by_mixture(&g, &surviving, 0, 2).expect("C is dominated");
        assert_eq!(
            mix.len(),
            3,
            "full-length distribution over Row's strategies"
        );
        assert!(mix[2].is_zero(), "the candidate carries no weight");
        let total: Rational = mix.iter().sum();
        assert_eq!(total, Rational::one(), "probabilities sum to one");
        // Independently re-check the dominance the LP claims to have found.
        for col in 0..2 {
            let mixed_payoff: Rational =
                (0..3).map(|row| &mix[row] * g.payoff(&[row, col], 0)).sum();
            assert!(
                mixed_payoff > *g.payoff(&[2, col], 0),
                "mixture must beat C in column {col}"
            );
        }
    }

    #[test]
    fn a_strategy_that_is_not_dominated_by_any_mixture_is_reported_as_such() {
        // Matching Pennies: nothing is dominated, purely or mixed.
        let mp = game(
            vec!["Heads", "Tails"],
            vec!["Heads", "Tails"],
            vec![
                vec![[1.0, -1.0], [-1.0, 1.0]],
                vec![[-1.0, 1.0], [1.0, -1.0]],
            ],
        );
        let surviving = vec![vec![0, 1], vec![0, 1]];
        assert!(strictly_dominated_by_mixture(&mp, &surviving, 0, 0).is_none());
        assert!(strictly_dominated_by_mixture(&mp, &surviving, 0, 1).is_none());
    }

    #[test]
    fn iterated_deletion_uses_mixed_dominance_and_records_the_mixture() {
        let result = solve_dominance(&mixed_dominance_game(), DominanceMode::Strict);
        assert!(result.mixed_dominance_checked);
        let step = result
            .steps
            .iter()
            .find(|s| s.player == 0 && s.eliminated == 2)
            .expect("C is eliminated");
        match &step.dominated_by {
            Dominator::Mixed(probs) => {
                assert_eq!(probs.len(), 3);
                assert!(probs[2].is_zero());
            }
            Dominator::Pure(s) => panic!("expected a mixture, got pure strategy {s}"),
        }
    }

    #[test]
    fn pure_dominance_is_still_reported_as_pure() {
        let result = solve_dominance(&prisoners_dilemma(), DominanceMode::Strict);
        assert_eq!(result.steps[0].dominated_by, Dominator::Pure(1));
    }

    #[test]
    fn ordinal_games_get_pure_dominance_only() {
        // Expected utility over ranks is meaningless, so the mixed check is
        // not run and the result says so.
        let form = MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: vec!["A".into(), "B".into(), "C".into()],
            col_strategies: vec!["L".into(), "R".into()],
            payoff_matrix: vec![
                vec![[3.0, 0.0], [0.0, 0.0]],
                vec![[0.0, 0.0], [3.0, 0.0]],
                vec![[1.0, 0.0], [1.0, 0.0]],
            ],
            payoff_kind: PayoffKind::Ordinal,
        };
        let g = ValidStrategicGame::validate(StrategicGame::try_from(form).unwrap()).unwrap();
        let result = solve_dominance(&g, DominanceMode::Strict);
        assert!(!result.mixed_dominance_checked);
        assert!(
            result.steps.is_empty(),
            "C survives: no pure strategy dominates it"
        );
    }

    #[test]
    fn weak_mode_does_not_run_the_mixed_check() {
        // Weak dominance by mixtures is not defined in this version; the flag
        // must not claim a check that did not happen.
        let result = solve_dominance(&mixed_dominance_game(), DominanceMode::Weak);
        assert!(!result.mixed_dominance_checked);
    }

    #[test]
    fn dominance_works_on_ordinal_payoffs() {
        // Dominance is an ordinal notion — no expectation is taken.
        let form = MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: vec!["C".into(), "D".into()],
            col_strategies: vec!["C".into(), "D".into()],
            payoff_matrix: vec![vec![[3.0, 3.0], [0.0, 4.0]], vec![[4.0, 0.0], [1.0, 1.0]]],
            payoff_kind: PayoffKind::Ordinal,
        };
        let g = ValidStrategicGame::validate(StrategicGame::try_from(form).unwrap()).unwrap();
        let result = solve_dominance(&g, DominanceMode::Strict);
        assert_eq!(result.unique_profile, Some(vec![1, 1]));
    }
}
