//! Structural properties of a game — what shape it has, independent of any
//! one solution concept.

pub mod archetype;
pub mod repeated;
pub mod structure;

pub use archetype::{classify, Archetype, ArchetypeReport};
pub use repeated::{analyze_repeated_game, PlayerThreshold, Punishment, RepeatedGameReport};
pub use structure::{
    analyze_structure, pareto_dominates, DominatedEquilibrium, SecurityLevel, StructureReport,
};
