//! Exact game-theoretic computation.
//!
//! No I/O, no async, no protocol handling. A caller-supplied [`StrategicGame`]
//! passes through [`ValidStrategicGame::validate`] once; every solver takes
//! that type, so an unchecked game cannot reach a solver.
//!
//! Payoffs cross the API as `f64` but are stored and computed as exact
//! rationals, so there is no tolerance anywhere and answers match published
//! textbook solutions exactly.
//!
//! ```
//! use gt_core::game::{MatrixForm, PayoffKind, StrategicGame, ValidStrategicGame};
//! use gt_core::solve::pure_nash::solve_pure_nash;
//!
//! let matrix = MatrixForm {
//!     players: ["Row".into(), "Col".into()],
//!     row_strategies: vec!["Cooperate".into(), "Defect".into()],
//!     col_strategies: vec!["Cooperate".into(), "Defect".into()],
//!     payoff_matrix: vec![
//!         vec![[3.0, 3.0], [0.0, 4.0]],
//!         vec![[4.0, 0.0], [1.0, 1.0]],
//!     ],
//!     payoff_kind: PayoffKind::Cardinal,
//! };
//!
//! let game = ValidStrategicGame::validate(StrategicGame::try_from(matrix)?)?;
//! let result = solve_pure_nash(&game);
//!
//! // Both players defect: the unique equilibrium, and worse for both than
//! // mutual cooperation.
//! assert_eq!(result.equilibria, vec![vec![1, 1]]);
//! # Ok::<(), gt_core::GtError>(())
//! ```

pub mod analyze;
pub mod error;
pub mod exact;
pub mod game;
pub mod limits;
pub mod solve;

pub use analyze::{
    analyze_structure, classify, pareto_dominates, Archetype, ArchetypeReport,
    DominatedEquilibrium, SecurityLevel, StructureReport,
};
pub use error::{Diagnostic, DiagnosticCode, GtError};
pub use exact::{solve_linear_system, LinearSolution};
pub use game::{
    plan_to_strategy_index, to_strategic, ExtensiveGame, MatrixForm, Node, NodeId, Outcome,
    PayoffKind, Player, PlayerId, Profile, Rational, StrategicGame, StrategyId, ValidExtensiveGame,
    ValidStrategicGame,
};
pub use solve::{
    expected_payoff_per_strategy, profitable_deviation, solve_backward_induction, solve_dominance,
    solve_mixed_nash, solve_pure_nash, verify_equilibrium, BackwardInductionResult, Concept,
    Deviation, DominanceMode, DominanceResult, EliminationStep, MixedEquilibrium, MixedNashResult,
    MixedStrategy, NodeDecision, ProfileCheck, PureNashResult, SpeSolution, VerifyResult,
};
