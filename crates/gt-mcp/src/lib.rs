//! MCP adapter for `gt-core`. Tool registration, wire types, and error
//! mapping only -- every number in a response comes from `gt-core`.
//!
//! Split into a library plus a thin binary so integration tests can drive the
//! adapter directly. The binary is `src/main.rs`; it does nothing but install
//! logging and serve this crate's server over stdio.

pub mod server;
pub mod tools;
pub mod wire;
