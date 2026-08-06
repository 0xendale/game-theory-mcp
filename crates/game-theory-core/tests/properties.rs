//! Definitional properties, checked on randomly generated games.
//!
//! Each test states a fact that follows from the definition of a solution
//! concept, independently of how this crate computes it.

use game_theory_core::analyze::repeated::{analyze_repeated_game, Punishment};
use game_theory_core::analyze::structure::analyze_structure;
use game_theory_core::game::{
    Outcome, PayoffKind, Player, Rational, StrategicGame, ValidStrategicGame,
};
use game_theory_core::solve::dominance::{
    solve_dominance, strictly_dominated_by_mixture, DominanceMode,
};
use game_theory_core::solve::mixed_nash::{expected_payoff_per_strategy, solve_mixed_nash};
use game_theory_core::solve::pure_nash::solve_pure_nash;
use game_theory_core::solve::verify::{verify_equilibrium, Concept};
use game_theory_core::solve::verify_mixed::verify_mixed_nash;
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
        use game_theory_core::analyze::structure::pareto_dominates;
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

/// Cross-checks tying the extensive form to the strategic form.
///
/// These catch what the fixtures cannot: a `to_strategic` that enumerates only
/// on-path actions passes every hand-written example whose tree happens to be
/// fully reached, and fails here.
mod extensive {
    use game_theory_core::game::{ExtensiveGame, Node, PayoffKind, Player, ValidExtensiveGame};
    use game_theory_core::solve::pure_nash::solve_pure_nash;
    use game_theory_core::{
        plan_to_strategy_index, solve_backward_induction, to_strategic, verify_spe,
    };
    use proptest::prelude::*;

    /// Depth of node `i` in a full binary tree with the root at index 0 and the
    /// children of `i` at `2i+1`, `2i+2`.
    fn depth_of(i: usize) -> u32 {
        (usize::BITS - (i + 1).leading_zeros()) - 1
    }

    /// A full binary tree of the given depth. The player acting at depth `d` is
    /// `d % n_players`, which keeps any one player's strategy count — the
    /// product of their nodes' action counts — inside the strategic-form limit.
    fn build_full_binary_tree(depth: u32, n_players: usize, leaves: &[Vec<i64>]) -> ExtensiveGame {
        let n_internal = (1usize << depth) - 1;
        let mut nodes = Vec::with_capacity(n_internal + leaves.len());
        let mut information_sets = Vec::with_capacity(n_internal);
        for i in 0..n_internal {
            nodes.push(Node::Decision {
                player: (depth_of(i) as usize) % n_players,
                actions: vec![("0".into(), 2 * i + 1), ("1".into(), 2 * i + 2)],
            });
            information_sets.push(vec![i]);
        }
        for payoffs in leaves {
            nodes.push(Node::Terminal {
                payoffs: payoffs.iter().map(|&u| u as f64).collect(),
            });
        }
        ExtensiveGame {
            players: (0..n_players)
                .map(|id| Player {
                    id,
                    name: format!("P{id}"),
                })
                .collect(),
            root: 0,
            nodes,
            information_sets,
            payoff_kind: PayoffKind::Cardinal,
        }
    }

    fn arb_tree(depth: u32, n_players: usize) -> impl Strategy<Value = ExtensiveGame> {
        let n_leaves = 1usize << depth;
        prop::collection::vec(prop::collection::vec(-3i64..=3, n_players), n_leaves)
            .prop_map(move |leaves| build_full_binary_tree(depth, n_players, &leaves))
    }

    /// Bonanno §2.4: every backward-induction solution, read as a profile of
    /// complete contingent plans, is a Nash equilibrium of the converted
    /// strategic form.
    fn assert_bi_outcome_is_nash(game: ExtensiveGame) -> Result<(), TestCaseError> {
        let g = ValidExtensiveGame::validate(game).expect("the generator builds valid trees");
        let bi = solve_backward_induction(&g).expect("perfect information");
        let strategic = to_strategic(&g).expect("converts");
        let nash = solve_pure_nash(&strategic);

        for spe in &bi.solutions {
            let profile: Vec<usize> = (0..g.n_players())
                .map(|p| plan_to_strategy_index(&g, p, &spe.profile[p]))
                .collect();
            prop_assert!(
                nash.equilibria.contains(&profile),
                "SPE {:?} maps to strategic profile {:?}, which is not among the pure Nash \
                 equilibria {:?}",
                spe.profile,
                profile,
                nash.equilibria
            );
        }
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        #[test]
        fn bi_outcome_is_nash_of_converted_form_two_players(game in arb_tree(2, 2)) {
            assert_bi_outcome_is_nash(game)?;
        }

        #[test]
        fn bi_outcome_is_nash_of_converted_form_three_players(game in arb_tree(3, 3)) {
            assert_bi_outcome_is_nash(game)?;
        }

        /// verify_spe accepts every solution the solver produces — the two are
        /// written independently, so agreement is evidence about both.
        #[test]
        fn verify_accepts_every_bi_solution(game in arb_tree(3, 3)) {
            let g = ValidExtensiveGame::validate(game).expect("valid");
            let bi = solve_backward_induction(&g).expect("perfect information");
            prop_assert!(!bi.solutions.is_empty(), "a finite tree always has an SPE");
            for spe in &bi.solutions {
                let checked = verify_spe(&g, &spe.profile).expect("checked");
                prop_assert!(
                    checked.holds,
                    "verify_spe rejected an SPE: {:?}",
                    checked.deviation
                );
            }
        }

        /// Every backward-induction solution assigns an action at every decision
        /// node — the complete-contingent-plan requirement itself.
        #[test]
        fn every_solution_is_a_complete_contingent_plan(game in arb_tree(3, 3)) {
            let g = ValidExtensiveGame::validate(game).expect("valid");
            let bi = solve_backward_induction(&g).expect("perfect information");
            for spe in &bi.solutions {
                for p in 0..g.n_players() {
                    let owned = g.decision_nodes_of(p);
                    let planned: Vec<_> = spe.profile[p].iter().map(|(n, _)| *n).collect();
                    prop_assert_eq!(planned, owned);
                }
            }
        }
    }
}

/// Pure strict dominance, re-derived here so the property test does not lean
/// on the module it is testing.
fn strictly_dominates_pure(
    game: &ValidStrategicGame,
    surviving: &[Vec<usize>],
    player: usize,
    better: usize,
    worse: usize,
) -> bool {
    all_profiles(surviving, player).into_iter().all(|others| {
        let mut with_better = others.clone();
        let mut with_worse = others;
        with_better[player] = better;
        with_worse[player] = worse;
        game.payoff(&with_better, player) > game.payoff(&with_worse, player)
    })
}

/// Every joint profile of the players other than `player`; `player`'s own slot
/// is a placeholder for the caller to overwrite.
fn all_profiles(surviving: &[Vec<usize>], player: usize) -> Vec<Vec<usize>> {
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

/// The full strategy set of every player — the starting point of iterated
/// deletion, and the setting in which "dominated" is asked below.
fn full_survival(game: &ValidStrategicGame) -> Vec<Vec<usize>> {
    (0..game.n_players())
        .map(|p| (0..game.n_strategies(p)).collect())
        .collect()
}

proptest! {
    /// A strategy strictly dominated by a *pure* strategy is also dominated by
    /// a mixture — the degenerate mixture putting weight 1 on the dominator.
    /// The LP must never miss what the cheap check finds.
    #[test]
    fn mixed_dominance_finds_everything_pure_dominance_finds(game in arb_game(4)) {
        let surviving = full_survival(&game);

        for player in 0..game.n_players() {
            for candidate in 0..game.n_strategies(player) {
                let pure_dominated = (0..game.n_strategies(player)).any(|other| {
                    other != candidate
                        && strictly_dominates_pure(&game, &surviving, player, other, candidate)
                });
                if pure_dominated {
                    prop_assert!(
                        strictly_dominated_by_mixture(&game, &surviving, player, candidate)
                            .is_some(),
                        "pure dominance implies mixed dominance"
                    );
                }
            }
        }
    }

    /// Whatever mixture the LP returns really does dominate: re-checked against
    /// every opponent profile, independently of the solver.
    #[test]
    fn a_reported_dominating_mixture_really_dominates(game in arb_game(4)) {
        let surviving = full_survival(&game);
        let zero = Rational::from_integer(0.into());
        let one = Rational::from_integer(1.into());

        for player in 0..game.n_players() {
            for candidate in 0..game.n_strategies(player) {
                let Some(mix) =
                    strictly_dominated_by_mixture(&game, &surviving, player, candidate)
                else {
                    continue;
                };

                let total: Rational = mix.iter().sum();
                prop_assert_eq!(total, one.clone());
                prop_assert!(mix.iter().all(|p| *p >= zero));

                for others in all_profiles(&surviving, player) {
                    let mut own_profile = others.clone();
                    own_profile[player] = candidate;
                    let baseline = game.payoff(&own_profile, player).clone();

                    let mut mixed = zero.clone();
                    for (s, weight) in mix.iter().enumerate() {
                        let mut profile = others.clone();
                        profile[player] = s;
                        mixed += weight * game.payoff(&profile, player);
                    }
                    prop_assert!(mixed > baseline, "mixture must beat the candidate everywhere");
                }
            }
        }
    }

    /// Every equilibrium `solve_mixed_nash` reports must verify against the
    /// independent definition-checker.
    #[test]
    fn solved_mixed_equilibria_verify(game in arb_game(3)) {
        let solved = solve_mixed_nash(&game).expect("2-player cardinal game");
        for eq in &solved.equilibria {
            let result = verify_mixed_nash(&game, &eq.strategies)
                .expect("a solver-produced equilibrium is well formed");
            prop_assert!(
                result.holds,
                "solver produced a profile the verifier rejects: {:?}",
                result
            );
        }
    }

    /// The critical discount factor is a genuine threshold: at it, the target
    /// is sustainable; at half of it, it is not.
    #[test]
    fn the_critical_discount_factor_is_a_threshold(
        game in arb_game(3),
        row in 0usize..3,
        col in 0usize..3,
    ) {
        let target = [row % game.n_strategies(0), col % game.n_strategies(1)];
        let report =
            match analyze_repeated_game(&game, &target, Punishment::GrimTrigger, None) {
                Ok(report) => report,
                // Stage games with no pure Nash equilibrium have no grim
                // trigger to analyse; that case is covered by unit tests.
                Err(_) => return Ok(()),
            };
        let Some(threshold) = report.critical_discount_factor.clone() else {
            return Ok(());
        };

        let at = analyze_repeated_game(
            &game,
            &target,
            Punishment::GrimTrigger,
            Some(threshold.clone()),
        )
        .expect("the threshold is itself a valid discount factor")
        .sustainable_at;
        prop_assert_eq!(at, Some(true));

        if threshold > Rational::from_integer(0.into()) {
            let below = &threshold / Rational::from_integer(2.into());
            let result =
                analyze_repeated_game(&game, &target, Punishment::GrimTrigger, Some(below))
                    .expect("half of the threshold is a valid discount factor")
                    .sustainable_at;
            prop_assert_eq!(result, Some(false));
        }
    }
}
