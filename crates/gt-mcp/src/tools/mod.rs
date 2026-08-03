//! One tool per file. Each implements `ToolBase` for its name, description,
//! and schemas, and `SyncTool` for its body -- the solvers are pure and
//! synchronous, so there is nothing to await.

pub mod validate;
