//! Definitional properties, checked on randomly generated games.
//!
//! Each test states a fact that follows from the definition of a solution
//! concept, independently of how this crate computes it.

use gt_core::analyze::structure::analyze_structure;
use gt_core::game::{Outcome, PayoffKind, Player, Rational, StrategicGame, ValidStrategicGame};
use gt_core::solve::dominance::{solve_dominance, DominanceMode};
use gt_core::solve::mixed_nash::{expected_payoff_per_strategy, solve_mixed_nash};
use gt_core::solve::pure_nash::solve_pure_nash;
use gt_core::solve::verify::{verify_equilibrium, Concept};
use proptest::prelude::*;

/// Small integer payoffs keep the games interesting without making the
/// rational arithmetic slow.
fn arb_game(max_strategies: usize) -> impl Strategy<Value = ValidStrategicGame> {
    (2..=max_strategies, 2..=max_strategies).prop_flat_map(|(m, n)| {
        prop::collection::vec(-5i64..=5, m * n * 2).prop_map(move |flat| {
            let mut outcomes = Vec::with_capacity(m * n);
            for r in 0..m {
                for c in 0..n {
                    let base = (r * n + c) * 2;
                    outcomes.push(Outcome {
                        profile: vec![r, c],
                        payoffs: vec![flat[base] as f64, flat[base + 1] as f64],
                    });
                }
            }
            ValidStrategicGame::validate(StrategicGame {
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
                    (0..m).map(|i| format!("r{i}")).collect(),
                    (0..n).map(|j| format!("c{j}")).collect(),
                ],
                outcomes,
                payoff_kind: PayoffKind::Cardinal,
            })
            .expect("generated games are well formed by construction")
        })
    })
}

proptest! {
    /// Every equilibrium a solver reports must survive independent verification.
    #[test]
    fn pure_equilibria_verify(game in arb_game(4)) {
        for eq in solve_pure_nash(&game).equilibria {
            let result = verify_equilibrium(&game, &eq, Concept::PureNash)
                .expect("solver output is a valid profile");
            prop_assert!(result.holds, "solver produced a non-equilibrium: {eq:?}");
        }
    }

    /// A strictly dominated strategy is a best response to nothing, so it
    /// cannot appear in any pure equilibrium.
    #[test]
    fn strictly_dominated_strategies_are_in_no_equilibrium(game in arb_game(4)) {
        let dominance = solve_dominance(&game, DominanceMode::Strict);
        let equilibria = solve_pure_nash(&game).equilibria;

        for step in &dominance.steps {
            for eq in &equilibria {
                prop_assert_ne!(
                    eq[step.player], step.eliminated,
                    "strategy {} of player {} was strictly dominated yet appears in {:?}",
                    step.eliminated, step.player, eq
                );
            }
        }
    }

    /// Iterated deletion of *strictly* dominated strategies is order
    /// independent: the surviving set does not depend on elimination order.
    /// Running it twice must give the identical answer.
    #[test]
    fn strict_deletion_is_deterministic(game in arb_game(4)) {
        let first = solve_dominance(&game, DominanceMode::Strict);
        let second = solve_dominance(&game, DominanceMode::Strict);
        prop_assert_eq!(first.surviving, second.surviving);
    }

    /// Every strategy in the support of a mixed equilibrium earns the same
    /// expected payoff, and nothing outside the support earns more. This is
    /// the definition, checked without reference to how it was computed.
    #[test]
    fn mixed_equilibria_satisfy_indifference(game in arb_game(3)) {
        let result = solve_mixed_nash(&game).expect("2-player cardinal game");
        for eq in &result.equilibria {
            for player in 0..2 {
                let payoffs = expected_payoff_per_strategy(&game, eq, player);
                let support = &eq.supports[player];
                let target = payoffs[support[0]].clone();

                for &s in support {
                    prop_assert_eq!(
                        &payoffs[s], &target,
                        "support strategies must be payoff-equivalent"
                    );
                }
                for (s, u) in payoffs.iter().enumerate() {
                    if !support.contains(&s) {
                        prop_assert!(
                            *u <= target,
                            "off-support strategy {s} earns more than the support"
                        );
                    }
                }
            }
        }
    }

    /// Mixed-strategy probabilities must be a distribution: non-negative and
    /// summing to exactly one.
    #[test]
    fn mixed_strategies_are_probability_distributions(game in arb_game(3)) {
        let result = solve_mixed_nash(&game).expect("2-player cardinal game");
        for eq in &result.equilibria {
            for mix in &eq.strategies {
                let total: Rational = mix.probs.iter().sum();
                prop_assert_eq!(total, Rational::from_integer(1.into()));
                prop_assert!(mix.probs.iter().all(|p| *p >= Rational::from_integer(0.into())));
            }
        }
    }

    /// Every pure equilibrium appears in the mixed-equilibrium list as a
    /// singleton-support equilibrium. A pure equilibrium is a mixed one.
    #[test]
    fn pure_equilibria_appear_among_the_mixed_ones(game in arb_game(3)) {
        let pure = solve_pure_nash(&game).equilibria;
        let mixed = solve_mixed_nash(&game).expect("2-player cardinal game");

        for eq in &pure {
            let found = mixed.equilibria.iter().any(|m| {
                m.supports[0] == vec![eq[0]] && m.supports[1] == vec![eq[1]]
            });
            prop_assert!(found, "pure equilibrium {eq:?} missing from the mixed list");
        }
    }

    /// A profile on the Pareto frontier is dominated by nothing, by definition.
    #[test]
    fn the_pareto_frontier_is_undominated(game in arb_game(4)) {
        use gt_core::analyze::structure::pareto_dominates;
        let report = analyze_structure(&game);
        let all: Vec<_> = game.profiles().collect();

        for on_frontier in &report.pareto_frontier {
            for other in &all {
                prop_assert!(
                    !pareto_dominates(&game, other, on_frontier),
                    "{other:?} dominates supposedly-efficient {on_frontier:?}"
                );
            }
        }
    }

    /// A player can always guarantee themselves their security level, so no
    /// equilibrium can pay them less than it.
    #[test]
    fn equilibrium_payoffs_are_at_least_the_security_level(game in arb_game(4)) {
        let report = analyze_structure(&game);
        for eq in solve_pure_nash(&game).equilibria {
            for level in &report.security_levels {
                prop_assert!(
                    *game.payoff(&eq, level.player) >= level.value,
                    "equilibrium {eq:?} pays player {} less than their maxmin value",
                    level.player
                );
            }
        }
    }
}
