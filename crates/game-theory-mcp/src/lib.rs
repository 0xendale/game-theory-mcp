//! MCP adapter for `game-theory-core`. Tool registration, wire types, and error
//! mapping only -- every number in a response comes from `game-theory-core`.
//!
//! Split into a library plus a thin binary so integration tests can drive the
//! adapter directly. The binary is `src/main.rs`; it does nothing but install
//! logging and serve this crate's server over stdio.

pub mod prompts;
pub mod resources;
pub mod server;
pub mod tools;
pub mod wire;
