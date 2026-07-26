//! Gaussian elimination over exact rationals.
//!
//! No pivoting strategy is needed for numerical stability — the arithmetic is
//! exact — so pivot selection only has to avoid a zero pivot.

use crate::game::Rational;
use num_traits::Zero;

#[derive(Debug, Clone, PartialEq)]
pub enum LinearSolution {
    Unique(Vec<Rational>),
    /// Inconsistent: no assignment satisfies every equation.
    None,
    /// Consistent but underdetermined.
    Infinite,
}

/// Solve `a x = b`. `a` is row-major.
///
/// Precondition (caller must uphold): `a` is rectangular and `a.len() == b.len()`.
/// This is checked with `debug_assert!` in debug builds only; release builds do
/// not validate shape and will misbehave if the caller violates it.
pub fn solve_linear_system(mut a: Vec<Vec<Rational>>, mut b: Vec<Rational>) -> LinearSolution {
    let rows = a.len();
    debug_assert_eq!(rows, b.len(), "coefficient rows and constants must match");
    if rows == 0 {
        return LinearSolution::Infinite;
    }
    let cols = a[0].len();
    debug_assert!(
        a.iter().all(|row| row.len() == cols),
        "coefficient matrix must be rectangular"
    );

    let mut pivot_row = 0usize;
    let mut pivot_col_of_row: Vec<usize> = Vec::new();

    for col in 0..cols {
        // Find a row at or below pivot_row with a non-zero entry in this column.
        let Some(swap) = (pivot_row..rows).find(|&r| !a[r][col].is_zero()) else {
            continue;
        };
        a.swap(pivot_row, swap);
        b.swap(pivot_row, swap);

        // Normalize the pivot row so the pivot is 1.
        let pivot = a[pivot_row][col].clone();
        for entry in a[pivot_row].iter_mut() {
            *entry /= &pivot;
        }
        b[pivot_row] /= &pivot;

        // Clear this column everywhere else.
        for r in 0..rows {
            if r == pivot_row || a[r][col].is_zero() {
                continue;
            }
            let factor = a[r][col].clone();
            for c in 0..cols {
                let subtract = &factor * &a[pivot_row][c];
                a[r][c] -= subtract;
            }
            let subtract = &factor * &b[pivot_row];
            b[r] -= subtract;
        }

        pivot_col_of_row.push(col);
        pivot_row += 1;
        if pivot_row == rows {
            break;
        }
    }

    // Any all-zero row with a non-zero constant is 0 = c, which is unsatisfiable.
    for r in 0..rows {
        if a[r].iter().all(Zero::is_zero) && !b[r].is_zero() {
            return LinearSolution::None;
        }
    }

    if pivot_col_of_row.len() < cols {
        return LinearSolution::Infinite;
    }

    let mut solution = vec![Rational::zero(); cols];
    for (r, &col) in pivot_col_of_row.iter().enumerate() {
        solution[col] = b[r].clone();
    }
    LinearSolution::Unique(solution)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Rational;

    fn r(n: i64, d: i64) -> Rational {
        Rational::new(n.into(), d.into())
    }

    fn int(n: i64) -> Rational {
        Rational::from_integer(n.into())
    }

    #[test]
    fn solves_a_two_by_two_system() {
        // 2x + y = 5, x - y = 1  =>  x = 2, y = 1
        let a = vec![vec![int(2), int(1)], vec![int(1), int(-1)]];
        let b = vec![int(5), int(1)];
        match solve_linear_system(a, b) {
            LinearSolution::Unique(x) => assert_eq!(x, vec![int(2), int(1)]),
            other => panic!("expected a unique solution, got {other:?}"),
        }
    }

    #[test]
    fn produces_exact_fractions_not_approximations() {
        // 3x = 1  =>  x = 1/3 exactly
        let a = vec![vec![int(3)]];
        let b = vec![int(1)];
        match solve_linear_system(a, b) {
            LinearSolution::Unique(x) => assert_eq!(x, vec![r(1, 3)]),
            other => panic!("expected a unique solution, got {other:?}"),
        }
    }

    #[test]
    fn reports_an_inconsistent_system_as_none() {
        // x + y = 1, x + y = 2
        let a = vec![vec![int(1), int(1)], vec![int(1), int(1)]];
        let b = vec![int(1), int(2)];
        assert!(matches!(solve_linear_system(a, b), LinearSolution::None));
    }

    #[test]
    fn reports_an_underdetermined_system_as_infinite() {
        // x + y = 1, 2x + 2y = 2
        let a = vec![vec![int(1), int(1)], vec![int(2), int(2)]];
        let b = vec![int(1), int(2)];
        assert!(matches!(solve_linear_system(a, b), LinearSolution::Infinite));
    }

    #[test]
    fn handles_a_zero_pivot_by_swapping_rows() {
        // 0x + 1y = 2, 1x + 0y = 3  =>  x = 3, y = 2
        let a = vec![vec![int(0), int(1)], vec![int(1), int(0)]];
        let b = vec![int(2), int(3)];
        match solve_linear_system(a, b) {
            LinearSolution::Unique(x) => assert_eq!(x, vec![int(3), int(2)]),
            other => panic!("expected a unique solution, got {other:?}"),
        }
    }

    #[test]
    fn a_non_square_system_is_rejected_rather_than_guessed() {
        let a = vec![vec![int(1), int(1)]];
        let b = vec![int(1)];
        // One equation, two unknowns.
        assert!(matches!(solve_linear_system(a, b), LinearSolution::Infinite));
    }
}
