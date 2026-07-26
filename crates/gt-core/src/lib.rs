//! Exact game-theoretic computation.
//!
//! No I/O, no async, no protocol handling. Games are validated once via
//! [`game::ValidStrategicGame::validate`] and every solver takes that type,
//! so an unchecked game cannot reach a solver.

pub mod error;
pub mod exact;
pub mod game;
pub mod limits;
pub mod solve;

pub use error::{Diagnostic, DiagnosticCode, GtError};
pub use exact::{solve_linear_system, LinearSolution};
pub use game::{
    MatrixForm, PayoffKind, Player, PlayerId, Outcome, Profile, Rational, StrategicGame,
    StrategyId, ValidStrategicGame,
};
pub use solve::{
    profitable_deviation, solve_dominance, solve_pure_nash, Deviation, DominanceMode,
    DominanceResult, EliminationStep, ProfileCheck, PureNashResult,
};
