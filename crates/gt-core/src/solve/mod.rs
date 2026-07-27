//! Solution concepts. Every solver takes a validated game — `ValidStrategicGame`
//! for the strategic-form solvers, `ValidExtensiveGame` for the tree solvers —
//! and returns a report carrying both the answer and the derivation that
//! produced it.

pub mod backward_induction;
pub mod dominance;
pub mod mixed_nash;
pub mod pure_nash;
pub mod verify;
pub mod verify_spe;

pub use backward_induction::{
    solve_backward_induction, BackwardInductionResult, NodeDecision, SpeSolution,
};
pub use dominance::{
    solve_dominance, strictly_dominated_by_mixture, DominanceMode, DominanceResult, Dominator,
    EliminationStep,
};
pub use mixed_nash::{
    expected_payoff_per_strategy, solve_mixed_nash, MixedEquilibrium, MixedNashResult,
    MixedStrategy,
};
pub use pure_nash::{
    profitable_deviation, solve_pure_nash, Deviation, ProfileCheck, PureNashResult,
};
pub use verify::{verify_equilibrium, Concept, VerifyResult};
pub use verify_spe::{verify_spe, SpeDeviation, SpeVerifyResult};
