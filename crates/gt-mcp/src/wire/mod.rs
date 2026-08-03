//! Wire types. `gt-core`'s types do not cross the protocol boundary directly:
//! `Rational` serializes as `BigInt` internals, and `gt-core` derives no
//! `JsonSchema`. Round-trip tests keep these in step with their counterparts.

pub mod number;
pub mod outcome;
