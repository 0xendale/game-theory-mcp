//! Mixed-strategy Nash equilibrium for 2-player games (Bonanno §5.3).
//!
//! Support enumeration over equal-size support pairs, solved exactly. This
//! finds every equilibrium of a non-degenerate game. Degenerate games can also
//! have equilibria on unequal-size supports; those need a full LP, so
//! degeneracy is detected and reported instead of being silently incomplete.
//! Degeneracy is flagged by two independent signals, combined with OR: a
//! structural heuristic (`is_degenerate`) that looks for a wholly-duplicated
//! row or column, and an algebraic one that fires whenever some enumerated
//! support's indifference system comes back consistent-but-underdetermined
//! (`LinearSolution::Infinite`) — a degeneracy the structural check can miss.

use crate::error::GtError;
use crate::exact::{solve_linear_system, LinearSolution};
use crate::game::{Rational, StrategyId, ValidStrategicGame};
use crate::limits;
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};

/// A probability distribution over one player's strategies. Full length, with
/// zeros off the support.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixedStrategy {
    pub probs: Vec<Rational>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixedEquilibrium {
    pub strategies: Vec<MixedStrategy>,
    pub supports: Vec<Vec<StrategyId>>,
    pub expected_payoffs: Vec<Rational>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixedNashResult {
    pub equilibria: Vec<MixedEquilibrium>,
    /// True when the enumeration may be incomplete for this game.
    pub degenerate: bool,
    pub warning: Option<String>,
}

pub fn solve_mixed_nash(game: &ValidStrategicGame) -> Result<MixedNashResult, GtError> {
    if game.n_players() != 2 {
        return Err(GtError::NPlayerMixedUnsupported { players: game.n_players() });
    }
    game.require_cardinal("solve_mixed_nash")?;

    for p in 0..2 {
        if game.n_strategies(p) > limits::MAX_MIXED_STRATEGIES_PER_PLAYER {
            return Err(GtError::GameTooLarge {
                field: "strategies per player (mixed Nash)",
                limit: limits::MAX_MIXED_STRATEGIES_PER_PLAYER,
                actual: game.n_strategies(p),
            });
        }
    }

    let m = game.n_strategies(0);
    let n = game.n_strategies(1);
    let mut equilibria: Vec<MixedEquilibrium> = Vec::new();
    let mut saw_underdetermined = false;

    for size in 1..=m.min(n) {
        for s0 in subsets_of_size(m, size) {
            for s1 in subsets_of_size(n, size) {
                let (eq, underdetermined) = try_support_pair(game, &s0, &s1);
                saw_underdetermined |= underdetermined;
                if let Some(eq) = eq {
                    if !equilibria.contains(&eq) {
                        equilibria.push(eq);
                    }
                }
            }
        }
    }

    let degenerate = is_degenerate(game) || saw_underdetermined;
    let warning = degenerate.then(|| {
        "this game is degenerate: some payoffs tie, so equilibria may exist on \
         unequal-size supports that this enumeration does not cover"
            .to_string()
    });

    Ok(MixedNashResult { equilibria, degenerate, warning })
}

/// Outcome of solving one player's indifference system for a candidate
/// support pair.
enum MixOutcome {
    /// A unique, non-negative probability mixture.
    Mix(Vec<Rational>),
    /// The system was consistent but underdetermined (infinitely many
    /// solutions) — a degeneracy signal, not just "no equilibrium here".
    Underdetermined,
    /// The system was inconsistent, or the unique solution had a negative
    /// probability: no equilibrium on this support.
    NoSolution,
}

/// Solve the indifference conditions for one support pair, then check the
/// result really is an equilibrium. Returns the equilibrium (if any) and
/// whether an underdetermined (degenerate) indifference system was
/// encountered along the way.
fn try_support_pair(
    game: &ValidStrategicGame,
    s0: &[StrategyId],
    s1: &[StrategyId],
) -> (Option<MixedEquilibrium>, bool) {
    let mut saw_underdetermined = false;

    let y = match solve_opponent_mix(game, 0, s0, s1, game.n_strategies(1)) {
        MixOutcome::Mix(v) => v,
        MixOutcome::Underdetermined => return (None, true),
        MixOutcome::NoSolution => return (None, saw_underdetermined),
    };
    let x = match solve_opponent_mix(game, 1, s1, s0, game.n_strategies(0)) {
        MixOutcome::Mix(v) => v,
        MixOutcome::Underdetermined => {
            saw_underdetermined = true;
            return (None, saw_underdetermined);
        }
        MixOutcome::NoSolution => return (None, saw_underdetermined),
    };

    let eq = MixedEquilibrium {
        strategies: vec![MixedStrategy { probs: x }, MixedStrategy { probs: y }],
        supports: vec![s0.to_vec(), s1.to_vec()],
        expected_payoffs: Vec::new(),
    };

    // Verify independently of how it was constructed.
    for player in 0..2 {
        let per_strategy = expected_payoff_per_strategy(game, &eq, player);
        let support = &eq.supports[player];
        let target = per_strategy[support[0]].clone();
        if support.iter().any(|&s| per_strategy[s] != target) {
            return (None, saw_underdetermined);
        }
        if (0..per_strategy.len()).any(|s| !support.contains(&s) && per_strategy[s] > target) {
            return (None, saw_underdetermined);
        }
    }

    let expected_payoffs = (0..2)
        .map(|player| {
            let per_strategy = expected_payoff_per_strategy(game, &eq, player);
            per_strategy[eq.supports[player][0]].clone()
        })
        .collect();

    (Some(MixedEquilibrium { expected_payoffs, ..eq }), saw_underdetermined)
}

/// Find the mixture the *opponent* must play to leave `player` indifferent
/// across `player_support`. Returns a full-length probability vector, or
/// an outcome flagging why no such mixture was found.
fn solve_opponent_mix(
    game: &ValidStrategicGame,
    player: usize,
    player_support: &[StrategyId],
    opponent_support: &[StrategyId],
    opponent_strategies: usize,
) -> MixOutcome {
    let k = opponent_support.len();
    // Unknowns: one probability per opponent-support strategy, plus the
    // equilibrium payoff u. Equations: indifference across the player's
    // support, plus normalization.
    let unknowns = k + 1;
    let mut a: Vec<Vec<Rational>> = Vec::with_capacity(player_support.len() + 1);
    let mut b: Vec<Rational> = Vec::with_capacity(player_support.len() + 1);

    for &own in player_support {
        let mut row = Vec::with_capacity(unknowns);
        for &opp in opponent_support {
            let profile = if player == 0 { vec![own, opp] } else { vec![opp, own] };
            row.push(game.payoff(&profile, player).clone());
        }
        row.push(-Rational::one()); // coefficient on u
        a.push(row);
        b.push(Rational::zero());
    }

    let mut normalization = vec![Rational::one(); k];
    normalization.push(Rational::zero());
    a.push(normalization);
    b.push(Rational::one());

    // The system is square only when the supports are equal-sized, which the
    // caller guarantees.
    let solution = match solve_linear_system(a, b) {
        LinearSolution::Unique(solution) => solution,
        LinearSolution::Infinite => return MixOutcome::Underdetermined,
        LinearSolution::None => return MixOutcome::NoSolution,
    };

    let mut probs = vec![Rational::zero(); opponent_strategies];
    for (i, &opp) in opponent_support.iter().enumerate() {
        if solution[i] < Rational::zero() {
            return MixOutcome::NoSolution; // not a probability distribution
        }
        probs[opp] = solution[i].clone();
    }
    MixOutcome::Mix(probs)
}

/// Expected payoff to `player` from each of their own pure strategies, given
/// the opponent's mixture in `eq`.
pub fn expected_payoff_per_strategy(
    game: &ValidStrategicGame,
    eq: &MixedEquilibrium,
    player: usize,
) -> Vec<Rational> {
    let opponent = 1 - player;
    let opponent_mix = &eq.strategies[opponent].probs;

    (0..game.n_strategies(player))
        .map(|own| {
            let mut total = Rational::zero();
            for (opp, weight) in opponent_mix.iter().enumerate() {
                if weight.is_zero() {
                    continue;
                }
                let profile = if player == 0 { vec![own, opp] } else { vec![opp, own] };
                total += weight * game.payoff(&profile, player);
            }
            total
        })
        .collect()
}

/// A 2-player game is degenerate when some pure strategy has the same payoff
/// against two different opponent strategies in a way that ties the
/// indifference system. The cheap sufficient check: any two profiles giving a
/// player identical payoffs across a whole row or column.
fn is_degenerate(game: &ValidStrategicGame) -> bool {
    for player in 0..2 {
        let own_count = game.n_strategies(player);
        let opp_count = game.n_strategies(1 - player);
        for a in 0..own_count {
            for b in (a + 1)..own_count {
                let identical = (0..opp_count).all(|opp| {
                    let pa = if player == 0 { vec![a, opp] } else { vec![opp, a] };
                    let pb = if player == 0 { vec![b, opp] } else { vec![opp, b] };
                    game.payoff(&pa, player) == game.payoff(&pb, player)
                });
                if identical {
                    return true;
                }
            }
        }
    }
    false
}

/// All subsets of `{0..n}` with exactly `size` elements, ascending.
fn subsets_of_size(n: usize, size: usize) -> Vec<Vec<StrategyId>> {
    let mut out = Vec::new();
    let mut current = Vec::with_capacity(size);
    fn recurse(
        start: usize,
        n: usize,
        size: usize,
        current: &mut Vec<StrategyId>,
        out: &mut Vec<Vec<StrategyId>>,
    ) {
        if current.len() == size {
            out.push(current.clone());
            return;
        }
        for s in start..n {
            current.push(s);
            recurse(s + 1, n, size, current, out);
            current.pop();
        }
    }
    recurse(0, n, size, &mut current, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{MatrixForm, PayoffKind, StrategicGame, ValidStrategicGame};

    fn game(m: Vec<Vec<[f64; 2]>>, kind: PayoffKind) -> ValidStrategicGame {
        let rows = m.len();
        let cols = m[0].len();
        let form = MatrixForm {
            players: ["Row".into(), "Col".into()],
            row_strategies: (0..rows).map(|i| format!("r{i}")).collect(),
            col_strategies: (0..cols).map(|j| format!("c{j}")).collect(),
            payoff_matrix: m,
            payoff_kind: kind,
        };
        ValidStrategicGame::validate(StrategicGame::try_from(form).expect("converts"))
            .expect("valid")
    }

    fn r(n: i64, d: i64) -> Rational {
        Rational::new(n.into(), d.into())
    }

    fn int(n: i64) -> Rational {
        Rational::from_integer(n.into())
    }

    #[test]
    fn matching_pennies_has_the_half_half_equilibrium_exactly() {
        let mp = game(
            vec![
                vec![[1.0, -1.0], [-1.0, 1.0]],
                vec![[-1.0, 1.0], [1.0, -1.0]],
            ],
            PayoffKind::Cardinal,
        );
        let result = solve_mixed_nash(&mp).expect("2-player cardinal");
        let mixed: Vec<_> = result
            .equilibria
            .iter()
            .filter(|e| e.supports.iter().all(|s| s.len() == 2))
            .collect();
        assert_eq!(mixed.len(), 1);
        assert_eq!(mixed[0].strategies[0].probs, vec![r(1, 2), r(1, 2)]);
        assert_eq!(mixed[0].strategies[1].probs, vec![r(1, 2), r(1, 2)]);
        assert_eq!(mixed[0].expected_payoffs, vec![int(0), int(0)]);
    }

    #[test]
    fn battle_of_the_sexes_has_three_equilibria_two_pure_and_one_mixed() {
        // Row prefers (Opera, Opera) = (2,1); Col prefers (Football, Football) = (1,2).
        let bos = game(
            vec![
                vec![[2.0, 1.0], [0.0, 0.0]],
                vec![[0.0, 0.0], [1.0, 2.0]],
            ],
            PayoffKind::Cardinal,
        );
        let result = solve_mixed_nash(&bos).expect("2-player cardinal");
        assert_eq!(result.equilibria.len(), 3);

        let mixed = result
            .equilibria
            .iter()
            .find(|e| e.supports.iter().all(|s| s.len() == 2))
            .expect("a fully mixed equilibrium exists");
        // Row plays Opera with probability 2/3; Col plays Opera with probability 1/3.
        assert_eq!(mixed.strategies[0].probs, vec![r(2, 3), r(1, 3)]);
        assert_eq!(mixed.strategies[1].probs, vec![r(1, 3), r(2, 3)]);
    }

    #[test]
    fn pure_equilibria_appear_as_singleton_support_equilibria() {
        let pd = game(
            vec![
                vec![[3.0, 3.0], [0.0, 4.0]],
                vec![[4.0, 0.0], [1.0, 1.0]],
            ],
            PayoffKind::Cardinal,
        );
        let result = solve_mixed_nash(&pd).expect("2-player cardinal");
        assert_eq!(result.equilibria.len(), 1);
        assert_eq!(result.equilibria[0].supports, vec![vec![1], vec![1]]);
        assert_eq!(result.equilibria[0].strategies[0].probs, vec![int(0), int(1)]);
    }

    #[test]
    fn every_reported_equilibrium_satisfies_the_indifference_condition() {
        let bos = game(
            vec![
                vec![[2.0, 1.0], [0.0, 0.0]],
                vec![[0.0, 0.0], [1.0, 2.0]],
            ],
            PayoffKind::Cardinal,
        );
        let result = solve_mixed_nash(&bos).expect("2-player cardinal");
        for eq in &result.equilibria {
            for player in 0..2 {
                let payoffs = expected_payoff_per_strategy(&bos, eq, player);
                let on_support: Vec<_> = eq.supports[player]
                    .iter()
                    .map(|&s| payoffs[s].clone())
                    .collect();
                assert!(
                    on_support.windows(2).all(|w| w[0] == w[1]),
                    "support strategies must be payoff-equivalent"
                );
                let best = on_support[0].clone();
                for (s, u) in payoffs.iter().enumerate() {
                    if !eq.supports[player].contains(&s) {
                        assert!(*u <= best, "an off-support strategy earns more");
                    }
                }
            }
        }
    }

    #[test]
    fn ordinal_payoffs_are_rejected() {
        let g = game(
            vec![
                vec![[1.0, -1.0], [-1.0, 1.0]],
                vec![[-1.0, 1.0], [1.0, -1.0]],
            ],
            PayoffKind::Ordinal,
        );
        match solve_mixed_nash(&g) {
            Err(GtError::OrdinalPayoffsRejected { tool }) => {
                assert_eq!(tool, "solve_mixed_nash");
            }
            other => panic!("expected OrdinalPayoffsRejected, got {other:?}"),
        }
    }

    #[test]
    fn three_player_games_are_rejected_with_a_pointer_to_pure_nash() {
        use crate::game::{Outcome, Player};
        let mut outcomes = Vec::new();
        for a in 0..2 {
            for b in 0..2 {
                for c in 0..2 {
                    outcomes.push(Outcome {
                        profile: vec![a, b, c],
                        payoffs: vec![0.0, 0.0, 0.0],
                    });
                }
            }
        }
        let g = ValidStrategicGame::validate(StrategicGame {
            players: (0..3).map(|id| Player { id, name: format!("P{id}") }).collect(),
            strategies: vec![vec!["A".into(), "B".into()]; 3],
            outcomes,
            payoff_kind: PayoffKind::Cardinal,
        })
        .expect("valid");

        match solve_mixed_nash(&g) {
            Err(GtError::NPlayerMixedUnsupported { players }) => assert_eq!(players, 3),
            other => panic!("expected NPlayerMixedUnsupported, got {other:?}"),
        }
    }

    #[test]
    fn a_degenerate_game_is_flagged_rather_than_reported_as_exhaustive() {
        // All payoffs equal: every mixture is an equilibrium, so the
        // equal-support enumeration cannot be complete.
        let flat = game(
            vec![
                vec![[1.0, 1.0], [1.0, 1.0]],
                vec![[1.0, 1.0], [1.0, 1.0]],
            ],
            PayoffKind::Cardinal,
        );
        let result = solve_mixed_nash(&flat).expect("2-player cardinal");
        assert!(result.degenerate);
        assert!(result.warning.is_some());
    }

    #[test]
    fn an_underdetermined_support_is_flagged_even_without_a_duplicated_row_or_column() {
        // Row has 3 strategies, Col has 2. Row's own payoffs are all distinct
        // pairs, so `is_degenerate` never fires for player 0. Col's two full
        // columns, read down all three rows, are (5,3,1) vs (5,3,9) — also
        // distinct, so `is_degenerate` never fires for player 1 either.
        //
        // But restricted to the support pair (rows {0,1}, cols {0,1}) — the
        // only size-2 support Col has — Col's indifference system between
        // col0 and col1 sees identical payoffs at both rows in that support
        // (row0: 5 vs 5, row1: 3 vs 3), so the two indifference equations
        // collapse into one: a consistent but underdetermined
        // (`LinearSolution::Infinite`) system that the structural heuristic,
        // which only ever looks at *whole* rows/columns, cannot see (row2's
        // 1 vs 9 breaks the whole-column comparison).
        let g = game(
            vec![
                vec![[2.0, 5.0], [0.0, 5.0]],
                vec![[0.0, 3.0], [1.0, 3.0]],
                vec![[1.0, 1.0], [1.0, 9.0]],
            ],
            PayoffKind::Cardinal,
        );
        let result = solve_mixed_nash(&g).expect("2-player cardinal");
        assert!(result.degenerate, "underdetermined support must set degenerate = true");
        assert!(result.warning.is_some(), "degenerate result must carry a warning");
    }
}
