//! Structural properties of a game — what shape it has, independent of any
//! one solution concept.

pub mod archetype;
pub mod structure;

pub use archetype::{classify, Archetype, ArchetypeReport};
pub use structure::{
    analyze_structure, pareto_dominates, DominatedEquilibrium, SecurityLevel, StructureReport,
};
