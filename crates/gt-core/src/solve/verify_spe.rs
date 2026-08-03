//! Subgame-perfect equilibrium verification via the one-shot-deviation
//! principle: on a finite perfect-information tree, a profile is subgame
//! perfect iff no player can gain by changing the action at a single one of
//! their own decision nodes, holding the rest of the profile fixed.
//!
//! The check runs at *every* decision node, not only those on the equilibrium
//! path. That is the whole difference from Nash: a non-credible threat sits at
//! an unreached node, costs nothing under Nash, and is caught here.

use crate::error::GtError;
use crate::game::{Node, NodeId, PlayerId, Rational, StrategyId, ValidExtensiveGame};

/// A single profitable one-shot deviation — why a profile is not subgame
/// perfect, in enough detail to act on.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeDeviation {
    pub node: NodeId,
    pub player: PlayerId,
    pub from_action: StrategyId,
    pub to_action: StrategyId,
    pub gain: Rational,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpeVerifyResult {
    pub holds: bool,
    pub deviation: Option<SpeDeviation>,
}

/// Check whether `profile` is a subgame-perfect equilibrium of `game`.
///
/// `profile[p]` is player `p`'s complete plan — one `(node, action)` per
/// decision node of `p`, the shape [`crate::solve::SpeSolution::profile`]
/// produces.
///
/// Returns [`GtError::ImperfectInformationUnsupported`] on any non-singleton
/// information set.
///
/// A malformed profile — wrong arity, an unknown node, a terminal node, an
/// out-of-range action, or a decision node left unset — returns
/// [`GtError::InvalidPlanProfile`] naming the player and the problem. Callers
/// include protocol adapters relaying profiles they did not construct, so this
/// is bad input rather than a contract violation.
pub fn verify_spe(
    game: &ValidExtensiveGame,
    profile: &[Vec<(NodeId, StrategyId)>],
) -> Result<SpeVerifyResult, GtError> {
    game.require_perfect_information()?;

    if profile.len() != game.n_players() {
        return Err(GtError::InvalidPlanProfile {
            player: 0,
            reason: format!(
                "a profile must give one plan per player: got {} plan(s) for {} player(s)",
                profile.len(),
                game.n_players()
            ),
        });
    }

    // Flatten to a per-node action map for O(1) lookup during the fold.
    let mut action_at: Vec<Option<StrategyId>> = vec![None; game.nodes().len()];
    for (player, plan) in profile.iter().enumerate() {
        for &(node, action) in plan {
            if node >= game.nodes().len() {
                return Err(GtError::InvalidPlanProfile {
                    player,
                    reason: format!(
                        "node {node} does not exist; this tree has {} node(s)",
                        game.nodes().len()
                    ),
                });
            }
            let Node::Decision { actions, .. } = &game.nodes()[node] else {
                return Err(GtError::InvalidPlanProfile {
                    player,
                    reason: format!("node {node} is terminal, so no action can be fixed at it"),
                });
            };
            if action >= actions.len() {
                return Err(GtError::InvalidPlanProfile {
                    player,
                    reason: format!(
                        "node {node} has {} action(s), so action index {action} is out of range",
                        actions.len()
                    ),
                });
            }
            action_at[node] = Some(action);
        }
    }

    for (id, node) in game.nodes().iter().enumerate() {
        if let Node::Decision { player, .. } = node {
            if action_at[id].is_none() {
                return Err(GtError::InvalidPlanProfile {
                    player: *player,
                    reason: format!(
                        "a plan must fix an action at every decision node, and node {id} is unset"
                    ),
                });
            }
        }
    }

    // Continuation value at every node under the profile. Computed for all
    // nodes, not just those reachable from the root: the deviation check reads
    // values off the equilibrium path.
    let mut value: Vec<Option<Vec<Rational>>> = vec![None; game.nodes().len()];
    for id in 0..game.nodes().len() {
        continuation(game, id, &action_at, &mut value);
    }
    let value_at = |id: NodeId| -> &[Rational] {
        value[id].as_deref().expect("computed for every node above")
    };

    for (id, node) in game.nodes().iter().enumerate() {
        if let Node::Decision { player, actions } = node {
            let current = action_at[id].expect("checked above");
            let current_value = value_at(actions[current].1)[*player].clone();
            for (a, (_, child)) in actions.iter().enumerate() {
                if a == current {
                    continue;
                }
                let alternative = value_at(*child)[*player].clone();
                if alternative > current_value {
                    return Ok(SpeVerifyResult {
                        holds: false,
                        deviation: Some(SpeDeviation {
                            node: id,
                            player: *player,
                            from_action: current,
                            to_action: a,
                            gain: alternative - current_value,
                        }),
                    });
                }
            }
        }
    }

    Ok(SpeVerifyResult {
        holds: true,
        deviation: None,
    })
}

/// Fill `value[id]` and everything below it, following the profile's actions.
fn continuation(
    game: &ValidExtensiveGame,
    id: NodeId,
    action_at: &[Option<StrategyId>],
    value: &mut Vec<Option<Vec<Rational>>>,
) {
    if value[id].is_some() {
        return;
    }
    let v = match game.node(id) {
        Node::Terminal { .. } => game.terminal_payoffs(id).to_vec(),
        Node::Decision { actions, .. } => {
            let action = action_at[id].expect("validated: every decision node has an action");
            let child = actions[action].1;
            continuation(game, child, action_at, value);
            value[child].as_ref().expect("just computed").clone()
        }
    };
    value[id] = Some(v);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{ExtensiveGame, Node, PayoffKind, Player, ValidExtensiveGame};
    use crate::solve::backward_induction::solve_backward_induction;

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
    fn the_backward_induction_solution_verifies() {
        let g = entry_deterrence();
        let spe = &solve_backward_induction(&g).expect("solved").solutions[0];
        let result = verify_spe(&g, &spe.profile).expect("checked");
        assert!(result.holds);
        assert!(result.deviation.is_none());
    }

    #[test]
    fn a_non_credible_threat_is_caught() {
        // (Out, Fight) is a Nash equilibrium of the converted strategic form but
        // NOT subgame perfect: at node 1 the Incumbent would rather Accommodate
        // (1) than Fight (-1). The node is off the path, which is exactly why
        // Nash misses it and subgame perfection does not.
        let g = entry_deterrence();
        let profile = vec![vec![(0, 1)], vec![(1, 0)]];
        let result = verify_spe(&g, &profile).expect("checked");
        assert!(!result.holds);
        let dev = result.deviation.expect("a deviation exists");
        assert_eq!(dev.node, 1);
        assert_eq!(dev.player, 1);
        assert_eq!(dev.from_action, 0); // Fight
        assert_eq!(dev.to_action, 1); // Accommodate
        assert_eq!(dev.gain, r(2)); // 1 - (-1)
    }

    #[test]
    fn an_on_path_deviation_is_caught() {
        // (Out, Accommodate): the Incumbent plays its best action, but the
        // Entrant should enter — In yields 1, Out yields 0.
        let g = entry_deterrence();
        let profile = vec![vec![(0, 1)], vec![(1, 1)]];
        let result = verify_spe(&g, &profile).expect("checked");
        assert!(!result.holds);
        let dev = result.deviation.expect("a deviation exists");
        assert_eq!(dev.node, 0);
        assert_eq!(dev.player, 0);
        assert_eq!(dev.from_action, 1); // Out
        assert_eq!(dev.to_action, 0); // In
        assert_eq!(dev.gain, r(1)); // 1 - 0
    }

    #[test]
    fn every_backward_induction_solution_verifies_on_a_deeper_tree() {
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
        assert!(!result.solutions.is_empty());
        for spe in &result.solutions {
            let checked = verify_spe(&g, &spe.profile).expect("checked");
            assert!(checked.holds, "rejected an SPE: {:?}", spe.profile);
        }
    }

    #[test]
    fn an_off_path_suboptimal_action_is_still_caught() {
        // P0 strictly prefers Out. P1's node is unreached, but a profile that
        // has P1 playing its worse action there is not subgame perfect.
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
                    payoffs: vec![9.0, 0.0],
                }, // Out — best for P0 either way
                Node::Terminal {
                    payoffs: vec![0.0, 1.0],
                }, // In, L
                Node::Terminal {
                    payoffs: vec![0.0, 5.0],
                }, // In, R — better for P1
            ],
            information_sets: vec![vec![0], vec![1]],
            payoff_kind: PayoffKind::Cardinal,
        })
        .expect("valid");
        // P0: Out. P1: L at the unreached node — worse than R (1 < 5).
        let profile = vec![vec![(0, 1)], vec![(1, 0)]];
        let result = verify_spe(&g, &profile).expect("checked");
        assert!(
            !result.holds,
            "off-path suboptimality breaks subgame perfection"
        );
        let dev = result.deviation.expect("a deviation exists");
        assert_eq!(dev.node, 1);
        assert_eq!(dev.gain, r(4)); // 5 - 1
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
        let profile = vec![vec![(0, 0)], vec![(1, 0), (2, 0)]];
        assert!(matches!(
            verify_spe(&g, &profile),
            Err(GtError::ImperfectInformationUnsupported { .. })
        ));
    }

    #[test]
    fn a_profile_of_the_wrong_arity_is_rejected() {
        let g = entry_deterrence();
        let err = verify_spe(&g, &[vec![(0, 0)]]).unwrap_err();
        assert!(
            matches!(err, GtError::InvalidPlanProfile { player, ref reason }
                if player == 0 && reason.contains("one plan per player")),
            "got {err:?}"
        );
    }

    #[test]
    fn a_profile_missing_a_node_is_rejected() {
        let g = entry_deterrence();
        // Player 1's plan omits node 1.
        let err = verify_spe(&g, &[vec![(0, 0)], vec![]]).unwrap_err();
        assert!(
            matches!(err, GtError::InvalidPlanProfile { player, ref reason }
                if player == 1 && reason.contains("node 1")),
            "got {err:?}"
        );
    }

    #[test]
    fn a_node_id_out_of_range_is_rejected() {
        let g = entry_deterrence();
        let err = verify_spe(&g, &[vec![(99, 0)], vec![(1, 0)]]).unwrap_err();
        assert!(
            matches!(err, GtError::InvalidPlanProfile { player, ref reason }
                if player == 0 && reason.contains("99")),
            "got {err:?}"
        );
    }

    #[test]
    fn an_action_index_out_of_range_is_rejected() {
        let g = entry_deterrence();
        let err = verify_spe(&g, &[vec![(0, 0)], vec![(1, 99)]]).unwrap_err();
        assert!(
            matches!(err, GtError::InvalidPlanProfile { player, ref reason }
                if player == 1 && reason.contains("99")),
            "got {err:?}"
        );
    }
}
