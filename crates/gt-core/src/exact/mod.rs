//! Exact arithmetic helpers. Nothing here approximates.

pub mod linear;
pub mod simplex;

pub use linear::{solve_linear_system, LinearSolution};
pub use simplex::{solve_lp, LpProblem, LpSolution};
