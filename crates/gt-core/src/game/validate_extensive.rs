//! The only path from a caller-supplied `ExtensiveGame` to something a tree
//! solver will accept. Validation also builds the exact terminal-payoff table,
//! so a solver never does a payoff lookup that can fail.

use crate::error::{Diagnostic, DiagnosticCode, GtError};
use crate::game::{ExtensiveGame, Node, NodeId, PayoffKind, Player, PlayerId, Rational};
use crate::limits;
use num_rational::BigRational;
use num_traits::FromPrimitive;

/// An `ExtensiveGame` that has passed every check in
/// [`ValidExtensiveGame::validate`], together with an exact terminal-payoff
/// table.
///
/// Construction is the only way to get one, so any function taking this type
/// may assume: the root is in range, every child index is in range, every node
/// is reachable from the root exactly once (it is a tree), every terminal
/// carries one finite payoff per player, every decision node has at least one
/// action with unique labels, the information sets partition the decision
/// nodes, and all limits hold.
#[derive(Debug, Clone)]
pub struct ValidExtensiveGame {
    game: ExtensiveGame,
    /// `terminals[id]` is `Some(payoffs)` iff node `id` is a terminal.
    terminals: Vec<Option<Vec<Rational>>>,
}

impl ValidExtensiveGame {
    pub fn validate(game: ExtensiveGame) -> Result<Self, GtError> {
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
        if game.nodes.len() > limits::MAX_TREE_NODES {
            return Err(GtError::GameTooLarge {
                field: "tree nodes",
                limit: limits::MAX_TREE_NODES,
                actual: game.nodes.len(),
            });
        }
        if game.nodes.is_empty() {
            return Err(GtError::invalid(
                DiagnosticCode::EmptyTree,
                "a game tree needs at least one node",
            ));
        }
        let n_nodes = game.nodes.len();

        for (p, player) in game.players.iter().enumerate() {
            if player.id != p {
                return Err(GtError::invalid(
                    DiagnosticCode::PlayerIdMismatch,
                    format!(
                        "player at position {p} declares id {}; ids must equal position",
                        player.id
                    ),
                ));
            }
        }

        // Structural problems are collected, not short-circuited: a caller
        // fixing a malformed game should see every problem in one pass.
        let mut diagnostics = Vec::new();

        if game.root >= n_nodes {
            diagnostics.push(Diagnostic {
                code: DiagnosticCode::RootOutOfRange,
                message: format!("root is node {}, but there are {n_nodes} nodes", game.root),
            });
        }

        for (id, node) in game.nodes.iter().enumerate() {
            match node {
                Node::Decision { player, actions } => {
                    if *player >= n_players {
                        diagnostics.push(Diagnostic {
                            code: DiagnosticCode::PlayerIdMismatch,
                            message: format!(
                                "node {id} is played by player {player}, who does not exist"
                            ),
                        });
                    }
                    if actions.is_empty() {
                        diagnostics.push(Diagnostic {
                            code: DiagnosticCode::EmptyActionSet,
                            message: format!("decision node {id} has no actions"),
                        });
                    }
                    for i in 0..actions.len() {
                        if actions[..i].iter().any(|(l, _)| l == &actions[i].0) {
                            diagnostics.push(Diagnostic {
                                code: DiagnosticCode::DuplicateActionLabel,
                                message: format!(
                                    "node {id} has two actions labelled {:?}",
                                    actions[i].0
                                ),
                            });
                        }
                        if actions[i].1 >= n_nodes {
                            diagnostics.push(Diagnostic {
                                code: DiagnosticCode::ChildOutOfRange,
                                message: format!(
                                    "node {id} action {:?} leads to node {}, out of range",
                                    actions[i].0, actions[i].1
                                ),
                            });
                        }
                    }
                }
                Node::Terminal { payoffs } => {
                    if payoffs.len() != n_players {
                        diagnostics.push(Diagnostic {
                            code: DiagnosticCode::TerminalPayoffArityMismatch,
                            message: format!(
                                "terminal node {id} has {} payoffs, expected {n_players}",
                                payoffs.len()
                            ),
                        });
                    }
                    for (p, &u) in payoffs.iter().enumerate() {
                        if BigRational::from_f64(u).is_none() {
                            diagnostics.push(Diagnostic {
                                code: DiagnosticCode::NonFinitePayoff,
                                message: format!(
                                    "terminal node {id} gives player {p} a non-finite payoff"
                                ),
                            });
                        }
                    }
                }
            }
        }

        // Tree check: reach every node from the root exactly once. A node seen
        // twice means a shared child or a cycle — not a tree. Only run when the
        // indices above were in range, so traversal cannot panic.
        let indices_sound = diagnostics.iter().all(|d| {
            !matches!(
                d.code,
                DiagnosticCode::RootOutOfRange | DiagnosticCode::ChildOutOfRange
            )
        });
        if indices_sound {
            let mut seen = vec![0u32; n_nodes];
            let mut stack = vec![game.root];
            seen[game.root] = 1;
            while let Some(id) = stack.pop() {
                if let Node::Decision { actions, .. } = &game.nodes[id] {
                    for (_, child) in actions {
                        seen[*child] += 1;
                        if seen[*child] == 1 {
                            stack.push(*child);
                        }
                    }
                }
            }
            for (id, &count) in seen.iter().enumerate() {
                if count == 0 {
                    diagnostics.push(Diagnostic {
                        code: DiagnosticCode::UnreachableNode,
                        message: format!("node {id} is not reachable from the root"),
                    });
                } else if count > 1 {
                    diagnostics.push(Diagnostic {
                        code: DiagnosticCode::NotATree,
                        message: format!(
                            "node {id} is reached {count} times; a game tree has one path to \
                             every node"
                        ),
                    });
                }
            }
        }

        validate_information_sets(&game, &mut diagnostics);

        if !diagnostics.is_empty() {
            return Err(GtError::InvalidGame { diagnostics });
        }

        // Build the exact terminal table now that every payoff is known finite.
        let terminals = game
            .nodes
            .iter()
            .map(|node| match node {
                Node::Terminal { payoffs } => Some(
                    payoffs
                        .iter()
                        .map(|&u| BigRational::from_f64(u).expect("validated finite above"))
                        .collect(),
                ),
                Node::Decision { .. } => None,
            })
            .collect();

        Ok(ValidExtensiveGame { game, terminals })
    }

    pub fn n_players(&self) -> usize {
        self.game.players.len()
    }

    pub fn players(&self) -> &[Player] {
        &self.game.players
    }

    pub fn root(&self) -> NodeId {
        self.game.root
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.game.nodes[id]
    }

    pub fn nodes(&self) -> &[Node] {
        &self.game.nodes
    }

    pub fn payoff_kind(&self) -> PayoffKind {
        self.game.payoff_kind
    }

    pub fn game(&self) -> &ExtensiveGame {
        &self.game
    }

    pub fn terminal_payoffs(&self, id: NodeId) -> &[Rational] {
        self.terminals[id]
            .as_deref()
            .expect("validated: caller holds a terminal node id")
    }

    /// This player's decision nodes, ascending by id. The order fixes the
    /// strategy enumeration used by `to_strategic`.
    pub fn decision_nodes_of(&self, player: PlayerId) -> Vec<NodeId> {
        self.game
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(id, node)| match node {
                Node::Decision { player: p, .. } if *p == player => Some(id),
                _ => None,
            })
            .collect()
    }

    pub fn is_perfect_information(&self) -> bool {
        self.game.information_sets.iter().all(|s| s.len() == 1)
    }

    /// Rejects imperfect information for the v1.0 tree solvers, naming the
    /// first non-singleton information set.
    pub fn require_perfect_information(&self) -> Result<(), GtError> {
        for (i, set) in self.game.information_sets.iter().enumerate() {
            if set.len() != 1 {
                return Err(GtError::ImperfectInformationUnsupported { information_set: i });
            }
        }
        Ok(())
    }

    pub fn player_name(&self, player: PlayerId) -> &str {
        &self.game.players[player].name
    }
}

/// An information set must group decision nodes of one player with equal action
/// counts, and every decision node must belong to exactly one set.
fn validate_information_sets(game: &ExtensiveGame, diagnostics: &mut Vec<Diagnostic>) {
    let n_nodes = game.nodes.len();
    let mut membership = vec![0u32; n_nodes];

    for set in &game.information_sets {
        let mut first: Option<(PlayerId, usize)> = None;
        for &id in set {
            if id >= n_nodes {
                // Range problems are reported by the caller's own checks.
                continue;
            }
            membership[id] += 1;
            match &game.nodes[id] {
                Node::Terminal { .. } => diagnostics.push(Diagnostic {
                    code: DiagnosticCode::InformationSetOnTerminal,
                    message: format!("terminal node {id} appears in an information set"),
                }),
                Node::Decision { player, actions } => match first {
                    None => first = Some((*player, actions.len())),
                    Some((p0, a0)) => {
                        if *player != p0 {
                            diagnostics.push(Diagnostic {
                                code: DiagnosticCode::InformationSetMixedPlayers,
                                message: format!(
                                    "an information set groups node {id} (player {player}) with \
                                     nodes of player {p0}"
                                ),
                            });
                        }
                        if actions.len() != a0 {
                            diagnostics.push(Diagnostic {
                                code: DiagnosticCode::InformationSetActionCountMismatch,
                                message: format!(
                                    "node {id} has {} actions but is grouped with nodes having {a0}",
                                    actions.len()
                                ),
                            });
                        }
                    }
                },
            }
        }
    }

    for (id, node) in game.nodes.iter().enumerate() {
        if matches!(node, Node::Decision { .. }) && membership[id] != 1 {
            diagnostics.push(Diagnostic {
                code: DiagnosticCode::InformationSetNotPartition,
                message: format!(
                    "decision node {id} appears in {} information sets, expected exactly 1",
                    membership[id]
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DiagnosticCode;
    use crate::game::{ExtensiveGame, Node, PayoffKind, Player};

    fn players() -> Vec<Player> {
        vec![
            Player {
                id: 0,
                name: "Entrant".into(),
            },
            Player {
                id: 1,
                name: "Incumbent".into(),
            },
        ]
    }

    /// The entry-deterrence tree — valid, perfect information.
    fn entry_deterrence() -> ExtensiveGame {
        ExtensiveGame {
            players: players(),
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
                    payoffs: vec![2.0, 0.0],
                },
                Node::Terminal {
                    payoffs: vec![0.0, 0.0],
                },
                Node::Terminal {
                    payoffs: vec![1.0, 1.0],
                },
            ],
            information_sets: vec![vec![0], vec![1]],
            payoff_kind: PayoffKind::Cardinal,
        }
    }

    fn codes(err: &GtError) -> Vec<DiagnosticCode> {
        match err {
            GtError::InvalidGame { diagnostics } => diagnostics.iter().map(|d| d.code).collect(),
            other => panic!("expected InvalidGame, got {other:?}"),
        }
    }

    #[test]
    fn a_well_formed_tree_validates() {
        let g = ValidExtensiveGame::validate(entry_deterrence()).expect("valid");
        assert_eq!(g.n_players(), 2);
        assert_eq!(g.decision_nodes_of(0), vec![0]);
        assert_eq!(g.decision_nodes_of(1), vec![1]);
        assert!(g.is_perfect_information());
    }

    #[test]
    fn terminal_payoffs_are_stored_exactly() {
        let g = ValidExtensiveGame::validate(entry_deterrence()).expect("valid");
        assert_eq!(
            g.terminal_payoffs(4),
            &[
                Rational::from_integer(1.into()),
                Rational::from_integer(1.into()),
            ]
        );
    }

    #[test]
    fn a_root_out_of_range_is_reported() {
        let mut g = entry_deterrence();
        g.root = 99;
        let err = ValidExtensiveGame::validate(g).expect_err("bad root");
        assert!(codes(&err).contains(&DiagnosticCode::RootOutOfRange));
    }

    #[test]
    fn a_child_out_of_range_is_reported() {
        let mut g = entry_deterrence();
        g.nodes[0] = Node::Decision {
            player: 0,
            actions: vec![("In".into(), 1), ("Out".into(), 99)],
        };
        let err = ValidExtensiveGame::validate(g).expect_err("bad child");
        assert!(codes(&err).contains(&DiagnosticCode::ChildOutOfRange));
    }

    #[test]
    fn an_unreachable_node_is_reported() {
        let mut g = entry_deterrence();
        // Node 2 (Out terminal) is no longer pointed at by anything.
        g.nodes[0] = Node::Decision {
            player: 0,
            actions: vec![("In".into(), 1)],
        };
        let err = ValidExtensiveGame::validate(g).expect_err("unreachable");
        assert!(codes(&err).contains(&DiagnosticCode::UnreachableNode));
    }

    #[test]
    fn a_shared_child_is_not_a_tree() {
        let mut g = entry_deterrence();
        // Both of node 1's actions lead to terminal 3 — node 3 has two parents.
        g.nodes[1] = Node::Decision {
            player: 1,
            actions: vec![("Fight".into(), 3), ("Accommodate".into(), 3)],
        };
        let err = ValidExtensiveGame::validate(g).expect_err("shared child");
        let found = codes(&err);
        assert!(found.contains(&DiagnosticCode::NotATree));
        // Node 4 is now unreachable too.
        assert!(found.contains(&DiagnosticCode::UnreachableNode));
    }

    #[test]
    fn a_cycle_is_reported() {
        // A child pointing back at an ancestor is not a tree.
        let g = ExtensiveGame {
            players: players(),
            root: 0,
            nodes: vec![
                Node::Decision {
                    player: 0,
                    actions: vec![("A".into(), 1)],
                },
                Node::Decision {
                    player: 1,
                    actions: vec![("B".into(), 0)],
                },
            ],
            information_sets: vec![vec![0], vec![1]],
            payoff_kind: PayoffKind::Cardinal,
        };
        let err = ValidExtensiveGame::validate(g).expect_err("cycle");
        assert!(codes(&err).contains(&DiagnosticCode::NotATree));
    }

    #[test]
    fn a_terminal_with_wrong_payoff_arity_is_reported() {
        let mut g = entry_deterrence();
        g.nodes[2] = Node::Terminal { payoffs: vec![2.0] };
        let err = ValidExtensiveGame::validate(g).expect_err("arity");
        assert!(codes(&err).contains(&DiagnosticCode::TerminalPayoffArityMismatch));
    }

    #[test]
    fn a_non_finite_payoff_is_reported() {
        let mut g = entry_deterrence();
        g.nodes[2] = Node::Terminal {
            payoffs: vec![f64::INFINITY, 0.0],
        };
        let err = ValidExtensiveGame::validate(g).expect_err("infinite");
        assert!(codes(&err).contains(&DiagnosticCode::NonFinitePayoff));
    }

    #[test]
    fn a_decision_node_with_no_actions_is_reported() {
        let mut g = entry_deterrence();
        g.nodes[1] = Node::Decision {
            player: 1,
            actions: vec![],
        };
        let err = ValidExtensiveGame::validate(g).expect_err("no actions");
        assert!(codes(&err).contains(&DiagnosticCode::EmptyActionSet));
    }

    #[test]
    fn duplicate_action_labels_at_one_node_are_reported() {
        let mut g = entry_deterrence();
        g.nodes[1] = Node::Decision {
            player: 1,
            actions: vec![("X".into(), 3), ("X".into(), 4)],
        };
        let err = ValidExtensiveGame::validate(g).expect_err("dupe label");
        assert!(codes(&err).contains(&DiagnosticCode::DuplicateActionLabel));
    }

    #[test]
    fn information_sets_must_partition_the_decision_nodes() {
        let mut g = entry_deterrence();
        g.information_sets = vec![vec![0]]; // node 1 is in no set
        let err = ValidExtensiveGame::validate(g).expect_err("not a partition");
        assert!(codes(&err).contains(&DiagnosticCode::InformationSetNotPartition));
    }

    #[test]
    fn an_information_set_over_a_terminal_is_reported() {
        let mut g = entry_deterrence();
        g.information_sets = vec![vec![0], vec![1], vec![2]]; // 2 is terminal
        let err = ValidExtensiveGame::validate(g).expect_err("terminal in set");
        assert!(codes(&err).contains(&DiagnosticCode::InformationSetOnTerminal));
    }

    #[test]
    fn a_non_singleton_set_makes_the_game_imperfect_information() {
        // Two decision nodes for the same player, grouped, same action count.
        let g = ExtensiveGame {
            players: players(),
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
        };
        let g = ValidExtensiveGame::validate(g).expect("structurally valid");
        assert!(!g.is_perfect_information());
        match g.require_perfect_information() {
            Err(GtError::ImperfectInformationUnsupported { information_set }) => {
                assert_eq!(information_set, 1);
            }
            other => panic!("expected ImperfectInformationUnsupported, got {other:?}"),
        }
    }

    #[test]
    fn an_information_set_mixing_players_is_reported() {
        let g = ExtensiveGame {
            players: players(),
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
                    player: 0,
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
            information_sets: vec![vec![0], vec![1, 2]], // nodes 1,2 are different players
            payoff_kind: PayoffKind::Cardinal,
        };
        let err = ValidExtensiveGame::validate(g).expect_err("mixed players");
        assert!(codes(&err).contains(&DiagnosticCode::InformationSetMixedPlayers));
    }

    #[test]
    fn an_information_set_with_mismatched_action_counts_is_reported() {
        let g = ExtensiveGame {
            players: players(),
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
                    actions: vec![("L".into(), 5)],
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
        };
        let err = ValidExtensiveGame::validate(g).expect_err("action count");
        assert!(codes(&err).contains(&DiagnosticCode::InformationSetActionCountMismatch));
    }

    #[test]
    fn all_problems_are_reported_together_not_just_the_first() {
        let mut g = entry_deterrence();
        g.nodes[1] = Node::Decision {
            player: 1,
            actions: vec![("X".into(), 3), ("X".into(), 4)],
        };
        g.nodes[2] = Node::Terminal { payoffs: vec![2.0] };
        let err = ValidExtensiveGame::validate(g).expect_err("two problems");
        let found = codes(&err);
        assert!(found.contains(&DiagnosticCode::DuplicateActionLabel));
        assert!(found.contains(&DiagnosticCode::TerminalPayoffArityMismatch));
    }

    #[test]
    fn too_many_nodes_names_the_limit() {
        let n = crate::limits::MAX_TREE_NODES + 1;
        let mut nodes = Vec::with_capacity(n);
        // A degenerate chain long enough to exceed the node limit.
        for i in 0..n {
            let next = i + 1;
            if next < n {
                nodes.push(Node::Decision {
                    player: 0,
                    actions: vec![("a".into(), next)],
                });
            } else {
                nodes.push(Node::Terminal {
                    payoffs: vec![0.0, 0.0],
                });
            }
        }
        let g = ExtensiveGame {
            players: players(),
            root: 0,
            nodes,
            information_sets: vec![],
            payoff_kind: PayoffKind::Cardinal,
        };
        match ValidExtensiveGame::validate(g) {
            Err(GtError::GameTooLarge {
                field,
                limit,
                actual,
            }) => {
                assert_eq!(field, "tree nodes");
                assert_eq!(limit, crate::limits::MAX_TREE_NODES);
                assert_eq!(actual, n);
            }
            other => panic!("expected GameTooLarge, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_tree_is_reported() {
        let g = ExtensiveGame {
            players: players(),
            root: 0,
            nodes: vec![],
            information_sets: vec![],
            payoff_kind: PayoffKind::Cardinal,
        };
        let err = ValidExtensiveGame::validate(g).expect_err("empty");
        assert!(codes(&err).contains(&DiagnosticCode::EmptyTree));
    }
}
