//! Extensive -> strategic conversion. A strategy is a *complete contingent
//! plan*: one action at every decision node of the player, including nodes
//! their own earlier choices make unreachable (Bonanno §2.3). This is the most
//! common modelling error in dynamic games, and the strategic form built here
//! feeds the backward-induction/Nash cross-check that fails loudly if the
//! enumeration is wrong.

use crate::error::GtError;
use crate::game::{
    Node, NodeId, Outcome, PlayerId, StrategicGame, StrategyId, ValidExtensiveGame,
    ValidStrategicGame,
};
use num_traits::ToPrimitive;

/// Convert a perfect-information extensive game to its strategic (normal) form.
///
/// Each player's strategy set is the cartesian product of the action sets at
/// that player's decision nodes, taken in ascending node order with the **last
/// node varying fastest**. A player with no decision nodes gets one strategy,
/// the empty plan.
///
/// Returns [`GtError::ImperfectInformationUnsupported`] on any non-singleton
/// information set, and `GameTooLarge` (via [`ValidStrategicGame::validate`])
/// if the enumerated strategy or profile counts exceed the strategic-form
/// limits.
pub fn to_strategic(game: &ValidExtensiveGame) -> Result<ValidStrategicGame, GtError> {
    game.require_perfect_information()?;
    let n_players = game.n_players();

    let decision_nodes: Vec<Vec<NodeId>> =
        (0..n_players).map(|p| game.decision_nodes_of(p)).collect();
    let action_counts: Vec<Vec<usize>> = decision_nodes
        .iter()
        .map(|nodes| nodes.iter().map(|&id| n_actions(game, id)).collect())
        .collect();

    let mut strategies: Vec<Vec<String>> = Vec::with_capacity(n_players);
    for p in 0..n_players {
        let labels: Vec<String> = cartesian_indices(&action_counts[p])
            .iter()
            .map(|choice| plan_label(game, &decision_nodes[p], choice))
            .collect();
        // A player with no decision nodes has exactly one strategy: the empty
        // plan, which `cartesian_indices` yields as a single empty tuple.
        strategies.push(if labels.len() == 1 && labels[0].is_empty() {
            vec!["(no move)".into()]
        } else {
            labels
        });
    }

    // For every profile (one strategy per player), play the tree to a terminal.
    let strategy_counts: Vec<usize> = strategies.iter().map(Vec::len).collect();
    let mut outcomes = Vec::new();
    for profile in cartesian_indices(&strategy_counts) {
        let choice_per_player: Vec<Vec<usize>> = (0..n_players)
            .map(|p| unrank(profile[p], &action_counts[p]))
            .collect();
        let terminal = play(game, &decision_nodes, &choice_per_player);
        let payoffs = game
            .terminal_payoffs(terminal)
            .iter()
            .map(|r| {
                r.to_f64()
                    .expect("terminal payoff came from an f64, so it round-trips")
            })
            .collect();
        outcomes.push(Outcome { profile, payoffs });
    }

    ValidStrategicGame::validate(StrategicGame {
        players: game.players().to_vec(),
        strategies,
        outcomes,
        payoff_kind: game.payoff_kind(),
    })
}

/// The strategic-form strategy index for a per-player plan — a `(node, action)`
/// list covering every decision node of `player`. Inverts the enumeration
/// [`to_strategic`] uses, so a tree solution can be located in the converted
/// game.
///
/// Panics if `plan` omits one of the player's decision nodes.
pub fn plan_to_strategy_index(
    game: &ValidExtensiveGame,
    player: PlayerId,
    plan: &[(NodeId, StrategyId)],
) -> usize {
    let nodes = game.decision_nodes_of(player);
    if nodes.is_empty() {
        return 0; // the single empty plan
    }
    let counts: Vec<usize> = nodes.iter().map(|&id| n_actions(game, id)).collect();
    let choice: Vec<usize> = nodes
        .iter()
        .map(|&id| {
            plan.iter()
                .find(|(n, _)| *n == id)
                .map(|(_, a)| *a)
                .expect("a plan must fix an action at every decision node of the player")
        })
        .collect();
    rank(&choice, &counts)
}

fn n_actions(game: &ValidExtensiveGame, id: NodeId) -> usize {
    match game.node(id) {
        Node::Decision { actions, .. } => actions.len(),
        Node::Terminal { .. } => {
            unreachable!("only decision nodes carry action counts")
        }
    }
}

/// Play the tree from the root, following each player's chosen action at every
/// node they own, until a terminal is reached. Validation guarantees a finite
/// tree, so this terminates.
fn play(
    game: &ValidExtensiveGame,
    decision_nodes: &[Vec<NodeId>],
    choice_per_player: &[Vec<usize>],
) -> NodeId {
    let mut id = game.root();
    loop {
        match game.node(id) {
            Node::Terminal { .. } => return id,
            Node::Decision { player, actions } => {
                let slot = decision_nodes[*player]
                    .iter()
                    .position(|&n| n == id)
                    .expect("a decision node is listed among its own player's nodes");
                id = actions[choice_per_player[*player][slot]].1;
            }
        }
    }
}

/// Mixed-radix enumeration: every index tuple for the given per-position
/// counts, last position varying fastest. An empty `counts` yields one empty
/// tuple.
fn cartesian_indices(counts: &[usize]) -> Vec<Vec<usize>> {
    let mut out = vec![vec![]];
    for &c in counts {
        let mut next = Vec::with_capacity(out.len() * c);
        for prefix in &out {
            for a in 0..c {
                let mut row = prefix.clone();
                row.push(a);
                next.push(row);
            }
        }
        out = next;
    }
    out
}

/// Mixed-radix rank/unrank, last position varying fastest — the inverse pair
/// that moves between an action-per-node tuple and a flat strategy index.
fn rank(choice: &[usize], counts: &[usize]) -> usize {
    let mut idx = 0;
    for (c, r) in choice.iter().zip(counts) {
        idx = idx * r + c;
    }
    idx
}

fn unrank(mut idx: usize, counts: &[usize]) -> Vec<usize> {
    let mut out = vec![0usize; counts.len()];
    for i in (0..counts.len()).rev() {
        out[i] = idx % counts[i];
        idx /= counts[i];
    }
    out
}

fn plan_label(game: &ValidExtensiveGame, nodes: &[NodeId], choice: &[usize]) -> String {
    nodes
        .iter()
        .zip(choice)
        .map(|(&id, &a)| match game.node(id) {
            Node::Decision { actions, .. } => format!("n{id}={}", actions[a].0),
            Node::Terminal { .. } => unreachable!("only decision nodes are enumerated"),
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{ExtensiveGame, Node, PayoffKind, Player, Rational, ValidExtensiveGame};
    use crate::solve::pure_nash::solve_pure_nash;

    fn r(n: i64) -> Rational {
        Rational::from_integer(n.into())
    }

    fn entry_deterrence() -> ValidExtensiveGame {
        ValidExtensiveGame::validate(ExtensiveGame {
            players: vec![
                Player {
                    id: 0,
                    name: "Entrant".into(),
                },
                Player {
                    id: 1,
                    name: "Incumbent".into(),
                },
            ],
            root: 0,
            nodes: vec![
                Node::Decision {
                    player: 0,
                    actions: vec![("In".into(), 1), ("Out".into(), 2)],
                },
                Node::Decision {
                    player: 1,
                    actions: vec![("Fight".into(), 3), ("Accommodate".into(), 4)],
                },
                Node::Terminal {
                    payoffs: vec![0.0, 2.0],
                }, // Out
                Node::Terminal {
                    payoffs: vec![-1.0, -1.0],
                }, // In, Fight
                Node::Terminal {
                    payoffs: vec![1.0, 1.0],
                }, // In, Accommodate
            ],
            information_sets: vec![vec![0], vec![1]],
            payoff_kind: PayoffKind::Cardinal,
        })
        .expect("valid")
    }

    #[test]
    fn each_player_has_the_expected_strategy_count() {
        let strategic = to_strategic(&entry_deterrence()).expect("converts");
        // Entrant has one decision node (2 actions); Incumbent one (2 actions).
        assert_eq!(strategic.n_strategies(0), 2);
        assert_eq!(strategic.n_strategies(1), 2);
    }

    #[test]
    fn incumbent_has_a_strategy_even_when_unreached() {
        // When Entrant plays Out (strategy 1), the Incumbent's node is unreached,
        // but each Incumbent strategy still fixes an action there, so the payoff
        // is the same across Incumbent strategies: (0, 2).
        let strategic = to_strategic(&entry_deterrence()).expect("converts");
        let out_fight = strategic.payoffs_at(&[1, 0]).to_vec();
        let out_accom = strategic.payoffs_at(&[1, 1]).to_vec();
        assert_eq!(
            out_fight, out_accom,
            "Out ends the game regardless of the Incumbent's plan"
        );
        assert_eq!(out_fight, vec![r(0), r(2)]);
    }

    #[test]
    fn on_path_payoffs_match_the_tree() {
        let strategic = to_strategic(&entry_deterrence()).expect("converts");
        // In (0), Fight (0) -> terminal 3 -> (-1,-1).
        assert_eq!(strategic.payoffs_at(&[0, 0]), &[r(-1), r(-1)]);
        // In (0), Accommodate (1) -> terminal 4 -> (1,1).
        assert_eq!(strategic.payoffs_at(&[0, 1]), &[r(1), r(1)]);
    }

    #[test]
    fn converted_game_has_the_expected_pure_nash_equilibria() {
        // (In, Accommodate) and (Out, Fight) are both pure Nash of the
        // strategic form; only the first is subgame perfect. (Out, Fight)
        // survives only because the Incumbent's node is never reached, so the
        // non-credible threat to Fight costs nothing — that is the whole point
        // of subgame perfection, and Task 5 checks it.
        let strategic = to_strategic(&entry_deterrence()).expect("converts");
        let nash = solve_pure_nash(&strategic);
        assert!(nash.equilibria.contains(&vec![0, 1]));
        assert!(nash.equilibria.contains(&vec![1, 0]));
    }

    #[test]
    fn strategy_labels_name_the_node_and_action() {
        let strategic = to_strategic(&entry_deterrence()).expect("converts");
        assert_eq!(strategic.strategy_name(1, 0), "n1=Fight");
        assert_eq!(strategic.strategy_name(1, 1), "n1=Accommodate");
    }

    #[test]
    fn a_player_with_two_decision_nodes_gets_the_product_of_action_counts() {
        // Player 0 moves at the root and again after action A; player 1 never moves.
        let g = ValidExtensiveGame::validate(ExtensiveGame {
            players: vec![
                Player {
                    id: 0,
                    name: "Solo".into(),
                },
                Player {
                    id: 1,
                    name: "Idle".into(),
                },
            ],
            root: 0,
            nodes: vec![
                Node::Decision {
                    player: 0,
                    actions: vec![("A".into(), 1), ("B".into(), 2)],
                },
                Node::Decision {
                    player: 0,
                    actions: vec![("L".into(), 3), ("R".into(), 4)],
                },
                Node::Terminal {
                    payoffs: vec![9.0, 0.0],
                },
                Node::Terminal {
                    payoffs: vec![1.0, 0.0],
                },
                Node::Terminal {
                    payoffs: vec![2.0, 0.0],
                },
            ],
            information_sets: vec![vec![0], vec![1]],
            payoff_kind: PayoffKind::Cardinal,
        })
        .expect("valid");
        let strategic = to_strategic(&g).expect("converts");
        assert_eq!(strategic.n_strategies(0), 4); // {A,B} x {L,R}
        assert_eq!(strategic.n_strategies(1), 1); // idle player: one empty plan

        // Strategy order is mixed-radix with the LAST node varying fastest:
        // 0 = (A,L), 1 = (A,R), 2 = (B,L), 3 = (B,R).
        // (A,L) -> node 1 -> L -> terminal 3 -> payoff 1.
        assert_eq!(strategic.payoff(&[0, 0], 0), &r(1));
        // (A,R) -> node 1 -> R -> terminal 4 -> payoff 2.
        assert_eq!(strategic.payoff(&[1, 0], 0), &r(2));
        // (B,*) -> terminal 2 -> payoff 9, regardless of the unreached node 1.
        assert_eq!(strategic.payoff(&[2, 0], 0), &r(9));
        assert_eq!(strategic.payoff(&[3, 0], 0), &r(9));
    }

    #[test]
    fn plan_index_round_trips_with_enumeration_order() {
        let g = entry_deterrence();
        // Incumbent (player 1) node 1, action 1 (Accommodate) -> strategy index 1.
        assert_eq!(plan_to_strategy_index(&g, 1, &[(1, 1)]), 1);
        assert_eq!(plan_to_strategy_index(&g, 1, &[(1, 0)]), 0);
    }

    #[test]
    fn plan_index_matches_the_generated_strategy_labels() {
        // Exhaustive check that `plan_to_strategy_index` inverts the same
        // enumeration `to_strategic` used, for a player with two nodes.
        let g = ValidExtensiveGame::validate(ExtensiveGame {
            players: vec![
                Player {
                    id: 0,
                    name: "Solo".into(),
                },
                Player {
                    id: 1,
                    name: "Idle".into(),
                },
            ],
            root: 0,
            nodes: vec![
                Node::Decision {
                    player: 0,
                    actions: vec![("A".into(), 1), ("B".into(), 2)],
                },
                Node::Decision {
                    player: 0,
                    actions: vec![("L".into(), 3), ("R".into(), 4)],
                },
                Node::Terminal {
                    payoffs: vec![9.0, 0.0],
                },
                Node::Terminal {
                    payoffs: vec![1.0, 0.0],
                },
                Node::Terminal {
                    payoffs: vec![2.0, 0.0],
                },
            ],
            information_sets: vec![vec![0], vec![1]],
            payoff_kind: PayoffKind::Cardinal,
        })
        .expect("valid");
        let strategic = to_strategic(&g).expect("converts");
        for a0 in 0..2 {
            for a1 in 0..2 {
                let idx = plan_to_strategy_index(&g, 0, &[(0, a0), (1, a1)]);
                let label = strategic.strategy_name(0, idx);
                let expected = format!("n0={},n1={}", ["A", "B"][a0], ["L", "R"][a1]);
                assert_eq!(label, expected, "index {idx} mislabelled");
            }
        }
    }

    #[test]
    fn an_idle_players_plan_is_index_zero() {
        let g = ValidExtensiveGame::validate(ExtensiveGame {
            players: vec![
                Player {
                    id: 0,
                    name: "Solo".into(),
                },
                Player {
                    id: 1,
                    name: "Idle".into(),
                },
            ],
            root: 0,
            nodes: vec![
                Node::Decision {
                    player: 0,
                    actions: vec![("A".into(), 1)],
                },
                Node::Terminal {
                    payoffs: vec![1.0, 2.0],
                },
            ],
            information_sets: vec![vec![0]],
            payoff_kind: PayoffKind::Cardinal,
        })
        .expect("valid");
        assert_eq!(plan_to_strategy_index(&g, 1, &[]), 0);
    }

    #[test]
    fn imperfect_information_is_rejected() {
        let g = ValidExtensiveGame::validate(ExtensiveGame {
            players: vec![
                Player {
                    id: 0,
                    name: "A".into(),
                },
                Player {
                    id: 1,
                    name: "B".into(),
                },
            ],
            root: 0,
            nodes: vec![
                Node::Decision {
                    player: 0,
                    actions: vec![("A".into(), 1), ("B".into(), 2)],
                },
                Node::Decision {
                    player: 1,
                    actions: vec![("L".into(), 3), ("R".into(), 4)],
                },
                Node::Decision {
                    player: 1,
                    actions: vec![("L".into(), 5), ("R".into(), 6)],
                },
                Node::Terminal {
                    payoffs: vec![0.0, 0.0],
                },
                Node::Terminal {
                    payoffs: vec![0.0, 0.0],
                },
                Node::Terminal {
                    payoffs: vec![0.0, 0.0],
                },
                Node::Terminal {
                    payoffs: vec![0.0, 0.0],
                },
            ],
            information_sets: vec![vec![0], vec![1, 2]],
            payoff_kind: PayoffKind::Cardinal,
        })
        .expect("structurally valid");
        assert!(matches!(
            to_strategic(&g),
            Err(GtError::ImperfectInformationUnsupported { .. })
        ));
    }
}
