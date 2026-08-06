//! One tool per file. Each implements `ToolBase` for its name, description,
//! and schemas, and `SyncTool` for its body -- the solvers are pure and
//! synchronous, so there is nothing to await.

pub mod backward_induction;
pub mod convert;
pub mod dominance;
pub mod mixed_nash;
pub mod pure_nash;
pub mod repeated;
pub mod structure;
pub mod validate;
pub mod verify;
