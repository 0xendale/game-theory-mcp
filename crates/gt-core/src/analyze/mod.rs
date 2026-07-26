//! Structural properties of a game — what shape it has, independent of any
//! one solution concept.

pub mod structure;

pub use structure::{
    analyze_structure, pareto_dominates, DominatedEquilibrium, SecurityLevel, StructureReport,
};
