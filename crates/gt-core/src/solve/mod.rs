//! Solution concepts. Every solver takes a `ValidStrategicGame` and returns a
//! report carrying both the answer and the derivation that produced it.

pub mod dominance;
pub mod mixed_nash;
pub mod pure_nash;
pub mod verify;

pub use dominance::{solve_dominance, DominanceMode, DominanceResult, EliminationStep};
pub use mixed_nash::{
    expected_payoff_per_strategy, solve_mixed_nash, MixedEquilibrium, MixedNashResult,
    MixedStrategy,
};
pub use pure_nash::{profitable_deviation, solve_pure_nash, Deviation, ProfileCheck, PureNashResult};
pub use verify::{verify_equilibrium, Concept, VerifyResult};
