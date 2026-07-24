//! Exact game-theoretic computation.
//!
//! No I/O, no async, no protocol handling. Games are validated once via
//! [`game::ValidStrategicGame::validate`] and every solver takes that type,
//! so an unchecked game cannot reach a solver.

pub mod error;
pub mod game;
pub mod limits;

pub use error::{Diagnostic, DiagnosticCode, GtError};
pub use game::{
    MatrixForm, PayoffKind, Player, PlayerId, Outcome, Profile, Rational, StrategicGame,
    StrategyId, ValidStrategicGame,
};
