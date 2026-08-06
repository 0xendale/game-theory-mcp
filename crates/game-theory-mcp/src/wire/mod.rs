//! Wire types. `game-theory-core`'s types do not cross the protocol boundary directly:
//! `Rational` serializes as `BigInt` internals, and `game-theory-core` derives no
//! `JsonSchema`. Round-trip tests keep these in step with their counterparts.

pub mod game;
pub mod number;
pub mod outcome;
