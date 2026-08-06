//! Backward induction over perfect-information trees (Bonanno §2.2).
//!
//! Ties are never broken silently: a player indifferent between actions yields
//! multiple subgame-perfect equilibria, and because a subgame-perfect profile
//! specifies an action at *every* node — including nodes the equilibrium path
//! never reaches — a tie anywhere in the tree multiplies the returned set.

use crate::error::GtError;
use crate::game::{Node, NodeId, PlayerId, Rational, StrategyId, ValidExtensiveGame};

/// One subgame-perfect equilibrium.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeSolution {
    /// `profile[p]` is player `p`'s complete plan: one `(node, action)` per
    /// decision node of `p`, ascending by node id.
    pub profile: Vec<Vec<(NodeId, StrategyId)>>,
    /// The induced path, root to terminal, as `(node, action)` steps.
    pub path: Vec<(NodeId, StrategyId)>,
    pub terminal_payoffs: Vec<Rational>,
}

/// What happened at one decision node, for the derivation trace.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeDecision {
    pub node: NodeId,
    pub player: PlayerId,
    pub chosen: Vec<StrategyId>,
    pub pruned: Vec<StrategyId>,
    /// `action_values[a]` is the continuation payoff vector under optimal play
    /// of action `a` (one representative optimal continuation on ties).
    pub action_values: Vec<Vec<Rational>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackwardInductionResult {
    pub solutions: Vec<SpeSolution>,
    pub node_log: Vec<NodeDecision>,
}

/// One optimal way to play the subtree rooted at a node.
#[derive(Clone)]
struct SubtreeOption {
    /// Payoff vector reached when this subtree is played optimally.
    value: Vec<Rational>,
    /// Chosen action at every decision node in this subtree.
    assign: Vec<(NodeId, StrategyId)>,
    /// Path from this node down to its terminal.
    path: Vec<(NodeId, StrategyId)>,
}

/// Solve a perfect-information extensive game by backward induction.
///
/// Returns every subgame-perfect equilibrium; a finite perfect-information tree
/// always has at least one. Returns
/// [`GtError::ImperfectInformationUnsupported`] on any non-singleton
/// information set.
pub fn solve_backward_induction(
    game: &ValidExtensiveGame,
) -> Result<BackwardInductionResult, GtError> {
    game.require_perfect_information()?;

    let mut node_log = Vec::new();
    let options = solve_node(game, game.root(), &mut node_log);
    // Ascending node order gives a stable, readable trace.
    node_log.sort_by_key(|d| d.node);

    let n_players = game.n_players();
    let solutions = options
        .into_iter()
        .map(|opt| SpeSolution {
            profile: regroup_by_player(game, &opt.assign, n_players),
            path: opt.path,
            terminal_payoffs: opt.value,
        })
        .collect();

    Ok(BackwardInductionResult {
        solutions,
        node_log,
    })
}

fn solve_node(
    game: &ValidExtensiveGame,
    id: NodeId,
    log: &mut Vec<NodeDecision>,
) -> Vec<SubtreeOption> {
    match game.node(id) {
        Node::Terminal { .. } => vec![SubtreeOption {
            value: game.terminal_payoffs(id).to_vec(),
            assign: vec![],
            path: vec![],
        }],
        Node::Decision { player, actions } => {
            let player = *player;
            let child_options: Vec<Vec<SubtreeOption>> = actions
                .iter()
                .map(|(_, child)| solve_node(game, *child, log))
                .collect();

            // Trace: value each action by one representative optimal
            // continuation, then record which actions survive.
            let action_values: Vec<Vec<Rational>> = child_options
                .iter()
                .map(|opts| opts[0].value.clone())
                .collect();
            let best = action_values
                .iter()
                .map(|v| v[player].clone())
                .max()
                .expect("a decision node has at least one action");
            let mut chosen = Vec::new();
            let mut pruned = Vec::new();
            for (a, v) in action_values.iter().enumerate() {
                if v[player] == best {
                    chosen.push(a);
                } else {
                    pruned.push(a);
                }
            }
            log.push(NodeDecision {
                node: id,
                player,
                chosen,
                pruned,
                action_values,
            });

            // A subgame-perfect profile fixes an action in every subtree, not
            // only the one played. So take the cartesian product of the
            // children's option lists — that is what makes an off-path tie
            // produce distinct equilibria — then let `player` best-respond
            // within each combination.
            let mut result = Vec::new();
            for combo in cartesian_options(&child_options) {
                let best_here = combo
                    .iter()
                    .map(|o| o.value[player].clone())
                    .max()
                    .expect("a decision node has at least one action");
                let mut merged_assign = Vec::new();
                for o in &combo {
                    merged_assign.extend(o.assign.iter().cloned());
                }
                for (a, o) in combo.iter().enumerate() {
                    if o.value[player] == best_here {
                        let mut assign = merged_assign.clone();
                        assign.push((id, a));
                        let mut path = vec![(id, a)];
                        path.extend(o.path.iter().cloned());
                        result.push(SubtreeOption {
                            value: o.value.clone(),
                            assign,
                            path,
                        });
                    }
                }
            }
            dedup_options(&mut result);
            result
        }
    }
}

/// Cartesian product over the children's option lists: one option per child.
fn cartesian_options(children: &[Vec<SubtreeOption>]) -> Vec<Vec<SubtreeOption>> {
    let mut out: Vec<Vec<SubtreeOption>> = vec![vec![]];
    for opts in children {
        let mut next = Vec::with_capacity(out.len() * opts.len());
        for prefix in &out {
            for o in opts {
                let mut row = prefix.clone();
                row.push(o.clone());
                next.push(row);
            }
        }
        out = next;
    }
    out
}

/// Two options describing the same equilibrium (same assignment, path and
/// value) are one equilibrium. The cartesian product above can generate a
/// duplicate when different combinations agree on everything that matters.
fn dedup_options(opts: &mut Vec<SubtreeOption>) {
    type Key = (
        Vec<Rational>,
        Vec<(NodeId, StrategyId)>,
        Vec<(NodeId, StrategyId)>,
    );
    let mut seen: Vec<Key> = Vec::new();
    opts.retain(|o| {
        let mut assign = o.assign.clone();
        assign.sort();
        let key = (o.value.clone(), assign, o.path.clone());
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });
}

fn regroup_by_player(
    game: &ValidExtensiveGame,
    assign: &[(NodeId, StrategyId)],
    n_players: usize,
) -> Vec<Vec<(NodeId, StrategyId)>> {
    let mut profile: Vec<Vec<(NodeId, StrategyId)>> = vec![Vec::new(); n_players];
    for &(node, action) in assign {
        if let Node::Decision { player, .. } = game.node(node) {
            profile[*player].push((node, action));
        }
    }
    for plan in &mut profile {
        plan.sort_by_key(|(n, _)| *n);
    }
    profile
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{ExtensiveGame, Node, PayoffKind, Player, ValidExtensiveGame};

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
    fn the_unique_spe_is_enter_and_accommodate() {
        // Incumbent accommodates (1 > -1). Anticipating that, the Entrant enters
        // (1 > 0). The threat to Fight is not credible.
        let result = solve_backward_induction(&entry_deterrence()).expect("solved");
        assert_eq!(result.solutions.len(), 1);
        let spe = &result.solutions[0];
        assert_eq!(spe.profile[0], vec![(0, 0)]); // Entrant: In at node 0
        assert_eq!(spe.profile[1], vec![(1, 1)]); // Incumbent: Accommodate at node 1
        assert_eq!(spe.path, vec![(0, 0), (1, 1)]);
        assert_eq!(spe.terminal_payoffs, vec![r(1), r(1)]);
    }

    #[test]
    fn the_node_log_records_pruned_branches() {
        let result = solve_backward_induction(&entry_deterrence()).expect("solved");
        let incumbent = result
            .node_log
            .iter()
            .find(|d| d.node == 1)
            .expect("node 1 logged");
        assert_eq!(incumbent.player, 1);
        assert_eq!(incumbent.chosen, vec![1]); // Accommodate
        assert_eq!(incumbent.pruned, vec![0]); // Fight
        assert_eq!(incumbent.action_values[0], vec![r(-1), r(-1)]);
        assert_eq!(incumbent.action_values[1], vec![r(1), r(1)]);
    }

    #[test]
    fn the_node_log_is_ordered_by_node_id() {
        let result = solve_backward_induction(&entry_deterrence()).expect("solved");
        let ids: Vec<_> = result.node_log.iter().map(|d| d.node).collect();
        assert_eq!(ids, vec![0, 1]);
    }

    #[test]
    fn a_tie_yields_multiple_spe() {
        // Player 1 is indifferent (both actions give player 1 payoff 0), so both
        // are subgame perfect and both must be returned.
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
                    actions: vec![("Go".into(), 1)],
                },
                Node::Decision {
                    player: 1,
                    actions: vec![("L".into(), 2), ("R".into(), 3)],
                },
                Node::Terminal {
                    payoffs: vec![1.0, 0.0],
                },
                Node::Terminal {
                    payoffs: vec![5.0, 0.0],
                },
            ],
            information_sets: vec![vec![0], vec![1]],
            payoff_kind: PayoffKind::Cardinal,
        })
        .expect("valid");
        let result = solve_backward_induction(&g).expect("solved");
        assert_eq!(result.solutions.len(), 2, "player 1 indifferent -> two SPE");
        let payoffs: Vec<_> = result
            .solutions
            .iter()
            .map(|s| s.terminal_payoffs.clone())
            .collect();
        assert!(payoffs.contains(&vec![r(1), r(0)]));
        assert!(payoffs.contains(&vec![r(5), r(0)]));
    }

    #[test]
    fn an_off_path_tie_still_multiplies_the_solution_set() {
        // Player 0 strictly prefers Out (3 > 1). Player 1's node is never
        // reached on the equilibrium path, but a subgame-perfect profile must
        // still specify an action there, and player 1 is indifferent — so there
        // are two SPE that share one path.
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
                    actions: vec![("In".into(), 1), ("Out".into(), 2)],
                },
                Node::Decision {
                    player: 1,
                    actions: vec![("L".into(), 3), ("R".into(), 4)],
                },
                Node::Terminal {
                    payoffs: vec![3.0, 0.0],
                }, // Out
                Node::Terminal {
                    payoffs: vec![1.0, 7.0],
                }, // In, L
                Node::Terminal {
                    payoffs: vec![1.0, 7.0],
                }, // In, R  (player 1 indifferent)
            ],
            information_sets: vec![vec![0], vec![1]],
            payoff_kind: PayoffKind::Cardinal,
        })
        .expect("valid");
        let result = solve_backward_induction(&g).expect("solved");
        assert_eq!(result.solutions.len(), 2);
        for spe in &result.solutions {
            assert_eq!(spe.path, vec![(0, 1)], "every SPE takes Out");
            assert_eq!(spe.terminal_payoffs, vec![r(3), r(0)]);
            // Each still fixes an action at the unreached node 1.
            assert_eq!(spe.profile[1].len(), 1);
            assert_eq!(spe.profile[1][0].0, 1);
        }
        let off_path: Vec<_> = result.solutions.iter().map(|s| s.profile[1][0].1).collect();
        assert!(off_path.contains(&0) && off_path.contains(&1));
    }

    #[test]
    fn every_solution_fixes_an_action_at_every_decision_node() {
        let g = entry_deterrence();
        let result = solve_backward_induction(&g).expect("solved");
        for spe in &result.solutions {
            for p in 0..g.n_players() {
                let owned = g.decision_nodes_of(p);
                let planned: Vec<_> = spe.profile[p].iter().map(|(n, _)| *n).collect();
                assert_eq!(planned, owned, "player {p} plan must cover all own nodes");
            }
        }
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
        match solve_backward_induction(&g) {
            Err(GtError::ImperfectInformationUnsupported { information_set }) => {
                assert_eq!(information_set, 1);
            }
            other => panic!("expected ImperfectInformationUnsupported, got {other:?}"),
        }
    }

    #[test]
    fn a_root_terminal_has_one_trivial_solution() {
        let g = ValidExtensiveGame::validate(ExtensiveGame {
            players: vec![Player {
                id: 0,
                name: "A".into(),
            }],
            root: 0,
            nodes: vec![Node::Terminal { payoffs: vec![7.0] }],
            information_sets: vec![],
            payoff_kind: PayoffKind::Cardinal,
        })
        .expect("valid");
        let result = solve_backward_induction(&g).expect("solved");
        assert_eq!(result.solutions.len(), 1);
        assert_eq!(result.solutions[0].terminal_payoffs, vec![r(7)]);
        assert!(result.solutions[0].path.is_empty());
        assert!(result.node_log.is_empty());
    }

    #[test]
    fn a_three_level_tree_folds_correctly() {
        // Player 0 at root; player 1 at both second-level nodes; terminals below.
        //         0 (P0)
        //      A /     \ B
        //       1(P1)   2(P1)
        //      /  \     /  \
        //     3    4   5    6
        //  (2,1)(0,3)(4,0)(1,2)
        // P1 at node 1: L->1, R->3 => R (payoff 0 for P0).
        // P1 at node 2: L->0, R->2 => R (payoff 1 for P0).
        // P0: A -> 0, B -> 1 => B. SPE payoff (1,2), path [(0,1),(2,1)].
        let g = ValidExtensiveGame::validate(ExtensiveGame {
            players: vec![
                Player {
                    id: 0,
                    name: "P0".into(),
                },
                Player {
                    id: 1,
                    name: "P1".into(),
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
                    payoffs: vec![2.0, 1.0],
                },
                Node::Terminal {
                    payoffs: vec![0.0, 3.0],
                },
                Node::Terminal {
                    payoffs: vec![4.0, 0.0],
                },
                Node::Terminal {
                    payoffs: vec![1.0, 2.0],
                },
            ],
            information_sets: vec![vec![0], vec![1], vec![2]],
            payoff_kind: PayoffKind::Cardinal,
        })
        .expect("valid");
        let result = solve_backward_induction(&g).expect("solved");
        assert_eq!(result.solutions.len(), 1);
        let spe = &result.solutions[0];
        assert_eq!(spe.path, vec![(0, 1), (2, 1)]);
        assert_eq!(spe.terminal_payoffs, vec![r(1), r(2)]);
        // P1's plan covers both of its nodes, R at each.
        assert_eq!(spe.profile[1], vec![(1, 1), (2, 1)]);
    }
}
