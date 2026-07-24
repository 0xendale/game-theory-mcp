//! Solution concepts. Every solver takes a `ValidStrategicGame` and returns a
//! report carrying both the answer and the derivation that produced it.

pub mod dominance;

pub use dominance::{solve_dominance, DominanceMode, DominanceResult, EliminationStep};
