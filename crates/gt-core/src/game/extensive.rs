//! Extensive-form (game-tree) representation. Plain data — validation lives in
//! `validate_extensive.rs`. Nodes are stored in a flat `Vec` and referenced by
//! index (`NodeId`); an edge is an action label paired with the child index.

use crate::game::{PayoffKind, Player, PlayerId};
use serde::{Deserialize, Serialize};

/// Index into `ExtensiveGame::nodes`.
pub type NodeId = usize;

/// A single tree node. A decision node hands the move to one player; a terminal
/// node ends play with a payoff per player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Node {
    /// `actions[i]` is the `i`-th action's label and the child it leads to.
    /// The action index `i` is the action's `StrategyId` at this node.
    Decision {
        player: PlayerId,
        actions: Vec<(String, NodeId)>,
    },
    /// One payoff per player, in player order.
    Terminal { payoffs: Vec<f64> },
}

/// A game in extensive form. Unvalidated — see `ValidExtensiveGame`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtensiveGame {
    pub players: Vec<Player>,
    pub root: NodeId,
    pub nodes: Vec<Node>,
    /// A partition of the decision nodes. Every v1.0 tree solver requires all
    /// sets to be singletons (perfect information); the field exists so the
    /// schema does not break when imperfect-information solving lands.
    pub information_sets: Vec<Vec<NodeId>>,
    pub payoff_kind: PayoffKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{PayoffKind, Player};

    /// Entry deterrence. Entrant chooses In/Out (node 0). If In, Incumbent
    /// chooses Fight/Accommodate (node 1). Out ends at node 2.
    fn entry_deterrence() -> ExtensiveGame {
        ExtensiveGame {
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

    #[test]
    fn game_survives_a_json_round_trip() {
        let game = entry_deterrence();
        let json = serde_json::to_string(&game).expect("serializes");
        let back: ExtensiveGame = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(game, back);
    }

    #[test]
    fn a_terminal_node_tags_its_kind() {
        let json =
            serde_json::to_string(&Node::Terminal { payoffs: vec![1.0] }).expect("serializes");
        assert!(json.contains("\"kind\":\"terminal\""), "got {json}");
    }

    #[test]
    fn a_node_missing_its_kind_tag_fails_to_deserialize() {
        let json = r#"{"player":0,"actions":[["L",1]]}"#;
        let parsed: Result<Node, _> = serde_json::from_str(json);
        assert!(parsed.is_err(), "kind tag is required");
    }
}
