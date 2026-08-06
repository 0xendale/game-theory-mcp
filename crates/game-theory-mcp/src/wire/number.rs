//! Exact rationals on the wire.
//!
//! `game-theory-core` computes in `BigRational`, whose serde form exposes `BigInt`
//! internals. Output carries the exact fraction plus a labelled decimal;
//! the exact value is authoritative and the decimal exists only so the host
//! does not compute it itself and get it wrong.
//!
//! Input is asymmetric on purpose: a bare fraction string, never a decimal.
//! Accepting `0.3333333333333333` for `1/3` would reintroduce at the boundary
//! the tolerance this project does not have.

use game_theory_core::Rational;
use num_traits::ToPrimitive;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// An exact value and its decimal rendering. `exact` is authoritative.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Exact {
    /// Exact value as a fraction: `"1/3"`, `"-7/4"`, or a bare integer `"2"`.
    pub exact: String,
    /// Decimal rendering of `exact`, for display only.
    pub approx: f64,
}

impl From<&Rational> for Exact {
    fn from(r: &Rational) -> Self {
        Exact {
            // BigRational's Display prints a bare numerator when the
            // denominator is one, so integers come out as "2", not "2/1".
            exact: r.to_string(),
            approx: r.to_f64().unwrap_or(f64::NAN),
        }
    }
}

/// Parse an exact rational from `"n/d"` or `"n"`. Decimals are rejected.
pub fn parse_rational(s: &str) -> Result<Rational, String> {
    Rational::from_str(s).map_err(|_| {
        format!(
            "{s:?} is not an exact value; write a fraction like \"1/3\", \
             a bare integer like \"2\", or a signed fraction like \"-7/4\" \
             (decimals are rejected because they are not exact)"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_rational::BigRational;
    use std::str::FromStr;

    #[test]
    fn a_fraction_renders_exactly_and_approximately() {
        let third = BigRational::from_str("1/3").unwrap();
        let e = Exact::from(&third);
        assert_eq!(e.exact, "1/3");
        assert!((e.approx - 0.333_333_333_333_333_3).abs() < 1e-15);
    }

    #[test]
    fn an_integer_renders_without_a_denominator() {
        let two = BigRational::from_str("2").unwrap();
        assert_eq!(Exact::from(&two).exact, "2");
    }

    #[test]
    fn a_negative_fraction_keeps_its_sign() {
        let v = BigRational::from_str("-7/4").unwrap();
        let e = Exact::from(&v);
        assert_eq!(e.exact, "-7/4");
        assert_eq!(e.approx, -1.75);
    }

    #[test]
    fn parsing_accepts_fractions_and_bare_integers() {
        assert_eq!(
            parse_rational("1/3").unwrap(),
            BigRational::from_str("1/3").unwrap()
        );
        assert_eq!(
            parse_rational("2").unwrap(),
            BigRational::from_str("2").unwrap()
        );
        assert_eq!(
            parse_rational("-7/4").unwrap(),
            BigRational::from_str("-7/4").unwrap()
        );
    }

    #[test]
    fn parsing_normalizes() {
        assert_eq!(
            parse_rational("2/4").unwrap(),
            BigRational::from_str("1/2").unwrap()
        );
    }

    #[test]
    fn parsing_rejects_a_decimal() {
        let err = parse_rational("0.5").unwrap_err();
        assert!(err.contains("0.5"), "got {err}");
        assert!(err.contains("fraction"), "got {err}");
    }

    #[test]
    fn parsing_rejects_a_zero_denominator() {
        assert!(parse_rational("1/0").is_err());
    }

    #[test]
    fn parsing_rejects_empty_and_garbage() {
        assert!(parse_rational("").is_err());
        assert!(parse_rational("half").is_err());
    }

    #[test]
    fn exact_serializes_with_both_fields() {
        let e = Exact::from(&BigRational::from_str("1/3").unwrap());
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["exact"], "1/3");
        assert!(v["approx"].is_number());
    }
}
