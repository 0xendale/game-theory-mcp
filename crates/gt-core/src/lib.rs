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
//!
//! Extensive-form (game-tree) games take the same route — validate once, then
//! solve. Backward induction returns every subgame-perfect equilibrium:
//!
//! ```
//! use gt_core::game::Player;
//! use gt_core::{solve_backward_induction, ExtensiveGame, Node, PayoffKind, ValidExtensiveGame};
//!
//! // The Entrant moves first; if it enters, the Incumbent chooses whether to
//! // fight. Fighting hurts the Incumbent too, so the threat is not credible.
//! let game = ExtensiveGame {
//!     players: vec![
//!         Player { id: 0, name: "Entrant".into() },
//!         Player { id: 1, name: "Incumbent".into() },
//!     ],
//!     root: 0,
//!     nodes: vec![
//!         Node::Decision { player: 0, actions: vec![("In".into(), 1), ("Out".into(), 2)] },
//!         Node::Decision { player: 1, actions: vec![("Fight".into(), 3), ("Accommodate".into(), 4)] },
//!         Node::Terminal { payoffs: vec![0.0, 2.0] },
//!         Node::Terminal { payoffs: vec![-1.0, -1.0] },
//!         Node::Terminal { payoffs: vec![1.0, 1.0] },
//!     ],
//!     information_sets: vec![vec![0], vec![1]],
//!     payoff_kind: PayoffKind::Cardinal,
//! };
//!
//! let game = ValidExtensiveGame::validate(game)?;
//! let result = solve_backward_induction(&game)?;
//!
//! // Unique equilibrium: the Entrant enters and the Incumbent accommodates.
//! assert_eq!(result.solutions.len(), 1);
//! assert_eq!(result.solutions[0].path, vec![(0, 0), (1, 1)]);
//! # Ok::<(), gt_core::GtError>(())
//! ```
//!
//! Repeating a stage game changes what is sustainable. Grim trigger with Nash
//! reversion gives the exact patience cooperation requires:
//!
//! ```
//! use gt_core::{
//!     analyze_repeated_game, MatrixForm, PayoffKind, Punishment, Rational,
//!     StrategicGame, ValidStrategicGame,
//! };
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
//! let game = ValidStrategicGame::validate(StrategicGame::try_from(matrix)?)?;
//!
//! let report = analyze_repeated_game(&game, &[0, 0], Punishment::GrimTrigger, None)?;
//!
//! // Mutual cooperation is sustainable exactly when the discount factor
//! // reaches 1/3 — an exact fraction, not a rounded decimal.
//! assert_eq!(
//!     report.critical_discount_factor,
//!     Some(Rational::new(1.into(), 3.into()))
//! );
//! # Ok::<(), gt_core::GtError>(())
//! ```

pub mod analyze;
pub mod error;
pub mod exact;
pub mod game;
pub mod limits;
pub mod solve;

pub use analyze::{
    analyze_repeated_game, analyze_structure, classify, pareto_dominates, Archetype,
    ArchetypeReport, DominatedEquilibrium, PlayerThreshold, Punishment, RepeatedGameReport,
    SecurityLevel, StructureReport,
};
pub use error::{Diagnostic, DiagnosticCode, GtError};
pub use exact::{solve_linear_system, solve_lp, LinearSolution, LpProblem, LpSolution};
pub use game::{
    plan_to_strategy_index, to_strategic, ExtensiveGame, MatrixForm, Node, NodeId, Outcome,
    PayoffKind, Player, PlayerId, Profile, Rational, StrategicGame, StrategyId, ValidExtensiveGame,
    ValidStrategicGame,
};
pub use solve::{
    expected_payoff_against, expected_payoff_per_strategy, profitable_deviation,
    solve_backward_induction, solve_dominance, solve_mixed_nash, solve_pure_nash,
    strictly_dominated_by_mixture, verify_equilibrium, verify_mixed_nash, verify_spe,
    BackwardInductionResult, Concept, Deviation, DominanceMode, DominanceResult, Dominator,
    EliminationStep, MixedDeviation, MixedEquilibrium, MixedNashResult, MixedStrategy,
    MixedVerifyResult, NodeDecision, ProfileCheck, PureNashResult, SpeDeviation, SpeSolution,
    SpeVerifyResult, SupportViolation, VerifyResult,
};
