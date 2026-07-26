//! Hard size limits. Exceeding one is an error naming the limit and the
//! actual value — the crate never truncates a game and answers anyway.

pub const MAX_PLAYERS: usize = 8;
pub const MAX_STRATEGIES_PER_PLAYER: usize = 20;
pub const MAX_PROFILES: usize = 100_000;
pub const MAX_MIXED_STRATEGIES_PER_PLAYER: usize = 12;
pub const MAX_TREE_NODES: usize = 10_000;
