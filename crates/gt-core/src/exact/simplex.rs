//! Two-phase primal simplex over exact rationals.
//!
//! Standard form: maximize `c · x` subject to `A x = b`, `x >= 0`. Callers
//! supply their own slack and surplus columns — keeping the solver to one
//! canonical form removes an entire class of transcription bug at the call
//! site, where the modelling actually happens.
//!
//! Pivot selection uses Bland's rule (lowest index, both entering and
//! leaving). Bland's rule is slower than Dantzig's on large programs but is
//! the only rule that provably cannot cycle, and cycling on a degenerate
//! vertex is the realistic failure mode here: dominance programs are full of
//! ties. The programs this crate builds are small (bounded by
//! `limits::MAX_MIXED_DOMINANCE_PROFILES`), so termination beats speed.

use crate::game::Rational;
use num_traits::{One, Zero};

#[derive(Debug, Clone, PartialEq)]
pub struct LpProblem {
    /// Maximized. Length must equal the column count of `constraints`.
    pub objective: Vec<Rational>,
    /// `A`, row-major. Every row must have the same length.
    pub constraints: Vec<Vec<Rational>>,
    /// `b`. Length must equal `constraints.len()`. May be negative — rows are
    /// normalized internally.
    pub rhs: Vec<Rational>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LpSolution {
    Optimal {
        x: Vec<Rational>,
        value: Rational,
    },
    /// No point satisfies `A x = b, x >= 0`.
    Infeasible,
    /// The objective grows without bound on the feasible region.
    Unbounded,
}

/// Solve `max c · x` subject to `A x = b`, `x >= 0`.
///
/// # Panics
///
/// Panics if `constraints` is ragged, if `rhs` does not match the row count,
/// or if `objective` does not match the column count. These checks run in
/// release builds too: a mis-shaped program has no meaningful optimum, and
/// this crate reports what went wrong rather than returning a plausible wrong
/// answer.
pub fn solve_lp(problem: LpProblem) -> LpSolution {
    let LpProblem {
        objective,
        mut constraints,
        mut rhs,
    } = problem;

    let rows = constraints.len();
    assert_eq!(
        rows,
        rhs.len(),
        "constraint rows and right-hand sides must match: {rows} rows, {} values",
        rhs.len()
    );
    if rows == 0 {
        // No constraints: x = 0 is feasible, and any positive objective
        // coefficient can be pushed to infinity.
        return if objective.iter().any(|c| *c > Rational::zero()) {
            LpSolution::Unbounded
        } else {
            LpSolution::Optimal {
                x: vec![Rational::zero(); objective.len()],
                value: Rational::zero(),
            }
        };
    }
    let cols = constraints[0].len();
    assert!(
        constraints.iter().all(|row| row.len() == cols),
        "constraint matrix must be rectangular: expected {cols} columns in every row"
    );
    assert_eq!(
        objective.len(),
        cols,
        "objective length must match the column count: {} vs {cols}",
        objective.len()
    );

    // Normalize so every right-hand side is non-negative; the phase-one basis
    // of artificial variables is only feasible when b >= 0.
    for r in 0..rows {
        if rhs[r] < Rational::zero() {
            for entry in constraints[r].iter_mut() {
                *entry = -&*entry;
            }
            rhs[r] = -&rhs[r];
        }
    }

    // Phase one: minimize the sum of artificials, i.e. maximize its negation.
    // Tableau columns are [original | artificial], artificials basic.
    let total = cols + rows;
    let mut tableau: Vec<Vec<Rational>> = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut row = Vec::with_capacity(total + 1);
        row.extend(constraints[r].iter().cloned());
        for a in 0..rows {
            row.push(if a == r {
                Rational::one()
            } else {
                Rational::zero()
            });
        }
        row.push(rhs[r].clone());
        tableau.push(row);
    }
    let mut basis: Vec<usize> = (cols..total).collect();

    let mut phase_one_cost = vec![Rational::zero(); total];
    for c in phase_one_cost.iter_mut().skip(cols) {
        *c = -Rational::one();
    }

    // Phase one is never unbounded (its objective is bounded above by zero),
    // so an Unbounded verdict here would be a bug in the pivot loop.
    match simplex(&mut tableau, &mut basis, &phase_one_cost, total) {
        SimplexOutcome::Optimal => {}
        SimplexOutcome::Unbounded => unreachable!("phase one is bounded above by zero"),
    }

    let phase_one_value = objective_value(&tableau, &basis, &phase_one_cost);
    if phase_one_value < Rational::zero() {
        return LpSolution::Infeasible;
    }

    // Drive any artificial still in the basis out of it. If its row has no
    // non-zero original column, the row is redundant and is dropped.
    let mut r = 0usize;
    while r < tableau.len() {
        if basis[r] < cols {
            r += 1;
            continue;
        }
        match (0..cols).find(|&c| !tableau[r][c].is_zero()) {
            Some(c) => {
                pivot(&mut tableau, &mut basis, r, c);
                r += 1;
            }
            None => {
                tableau.remove(r);
                basis.remove(r);
            }
        }
    }

    // Phase two: original objective, artificial columns priced at zero and
    // held out of the basis by construction.
    let mut phase_two_cost = objective.clone();
    phase_two_cost.resize(total, Rational::zero());

    match simplex(&mut tableau, &mut basis, &phase_two_cost, cols) {
        SimplexOutcome::Unbounded => LpSolution::Unbounded,
        SimplexOutcome::Optimal => {
            let mut x = vec![Rational::zero(); cols];
            for (r, &b) in basis.iter().enumerate() {
                if b < cols {
                    x[b] = tableau[r][total].clone();
                }
            }
            let value = objective
                .iter()
                .zip(&x)
                .fold(Rational::zero(), |acc, (c, v)| acc + c * v);
            LpSolution::Optimal { x, value }
        }
    }
}

enum SimplexOutcome {
    Optimal,
    Unbounded,
}

/// Pivot until no reduced cost is positive. `cost` is indexed by column.
///
/// Only columns below `enterable` may enter the basis. Phase two passes the
/// original column count here: letting an artificial column back into the
/// basis would abandon the equation it was standing in for, and the reported
/// point would not satisfy `A x = b`.
fn simplex(
    tableau: &mut [Vec<Rational>],
    basis: &mut [usize],
    cost: &[Rational],
    enterable: usize,
) -> SimplexOutcome {
    let total = cost.len();
    loop {
        // Reduced costs are recomputed from scratch each iteration. That is
        // O(rows * cols) per pivot rather than O(cols), but it keeps a single
        // source of truth for the tableau and cannot drift.
        let entering = (0..enterable).find(|&c| {
            let dual: Rational = basis
                .iter()
                .enumerate()
                .fold(Rational::zero(), |acc, (r, &b)| {
                    acc + &cost[b] * &tableau[r][c]
                });
            &cost[c] - dual > Rational::zero()
        });
        let Some(entering) = entering else {
            return SimplexOutcome::Optimal;
        };

        // Ratio test, ties broken by lowest basis index (Bland).
        let mut leaving: Option<usize> = None;
        let mut best_ratio: Option<Rational> = None;
        for r in 0..tableau.len() {
            if tableau[r][entering] <= Rational::zero() {
                continue;
            }
            let ratio = &tableau[r][total] / &tableau[r][entering];
            let take = match &best_ratio {
                None => true,
                Some(best) => {
                    ratio < *best || (ratio == *best && basis[r] < basis[leaving.unwrap()])
                }
            };
            if take {
                best_ratio = Some(ratio);
                leaving = Some(r);
            }
        }

        let Some(leaving) = leaving else {
            return SimplexOutcome::Unbounded;
        };
        pivot(tableau, basis, leaving, entering);
    }
}

fn pivot(tableau: &mut [Vec<Rational>], basis: &mut [usize], row: usize, col: usize) {
    let p = tableau[row][col].clone();
    for entry in tableau[row].iter_mut() {
        *entry /= &p;
    }
    let pivot_row = tableau[row].clone();
    for (r, current) in tableau.iter_mut().enumerate() {
        if r == row || current[col].is_zero() {
            continue;
        }
        let factor = current[col].clone();
        for (entry, pivot_entry) in current.iter_mut().zip(&pivot_row) {
            *entry -= &factor * pivot_entry;
        }
    }
    basis[row] = col;
}

fn objective_value(tableau: &[Vec<Rational>], basis: &[usize], cost: &[Rational]) -> Rational {
    let total = cost.len();
    basis
        .iter()
        .enumerate()
        .fold(Rational::zero(), |acc, (r, &b)| {
            acc + &cost[b] * &tableau[r][total]
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Rational;

    fn int(n: i64) -> Rational {
        Rational::from_integer(n.into())
    }

    fn r(n: i64, d: i64) -> Rational {
        Rational::new(n.into(), d.into())
    }

    /// max x + y  s.t.  x + y + s = 4,  x + 3y + t = 6,  all >= 0
    /// Optimum 4 (the whole face x + y = 4 is optimal; any vertex on it is fine).
    #[test]
    fn maximizes_a_bounded_two_variable_program() {
        let problem = LpProblem {
            objective: vec![int(1), int(1), int(0), int(0)],
            constraints: vec![
                vec![int(1), int(1), int(1), int(0)],
                vec![int(1), int(3), int(0), int(1)],
            ],
            rhs: vec![int(4), int(6)],
        };
        match solve_lp(problem) {
            LpSolution::Optimal { value, x } => {
                assert_eq!(value, int(4));
                assert_eq!(x.len(), 4);
                // Feasibility of the reported point, checked independently.
                assert_eq!(&x[0] + &x[1] + &x[2], int(4));
                assert_eq!(&x[0] + int(3) * &x[1] + &x[3], int(6));
                assert!(x.iter().all(|v| *v >= int(0)));
            }
            other => panic!("expected an optimum, got {other:?}"),
        }
    }

    /// The optimum is a fraction, and it must come back exact.
    /// max x  s.t.  3x + s = 1  =>  x = 1/3.
    #[test]
    fn reports_a_fractional_optimum_exactly() {
        let problem = LpProblem {
            objective: vec![int(1), int(0)],
            constraints: vec![vec![int(3), int(1)]],
            rhs: vec![int(1)],
        };
        match solve_lp(problem) {
            LpSolution::Optimal { value, .. } => assert_eq!(value, r(1, 3)),
            other => panic!("expected an optimum, got {other:?}"),
        }
    }

    /// x + y = 1 and x + y = 2 cannot both hold.
    #[test]
    fn reports_an_infeasible_program() {
        let problem = LpProblem {
            objective: vec![int(1), int(1)],
            constraints: vec![vec![int(1), int(1)], vec![int(1), int(1)]],
            rhs: vec![int(1), int(2)],
        };
        assert!(matches!(solve_lp(problem), LpSolution::Infeasible));
    }

    /// max x  s.t.  x - y = 1, x, y >= 0. x can grow without bound.
    #[test]
    fn reports_an_unbounded_program() {
        let problem = LpProblem {
            objective: vec![int(1), int(0)],
            constraints: vec![vec![int(1), int(-1)]],
            rhs: vec![int(1)],
        };
        assert!(matches!(solve_lp(problem), LpSolution::Unbounded));
    }

    /// A negative right-hand side is normalized, not rejected.
    /// max x  s.t.  -x - s = -2  =>  x <= 2, optimum 2.
    #[test]
    fn normalizes_a_negative_right_hand_side() {
        let problem = LpProblem {
            objective: vec![int(1), int(0)],
            constraints: vec![vec![int(-1), int(-1)]],
            rhs: vec![int(-2)],
        };
        match solve_lp(problem) {
            LpSolution::Optimal { value, .. } => assert_eq!(value, int(2)),
            other => panic!("expected an optimum, got {other:?}"),
        }
    }

    /// A redundant equation is dropped in phase one rather than reported as
    /// infeasible: row 2 is row 1 doubled.
    #[test]
    fn a_redundant_constraint_does_not_make_the_program_infeasible() {
        let problem = LpProblem {
            objective: vec![int(1), int(0)],
            constraints: vec![vec![int(1), int(1)], vec![int(2), int(2)]],
            rhs: vec![int(1), int(2)],
        };
        match solve_lp(problem) {
            LpSolution::Optimal { value, .. } => assert_eq!(value, int(1)),
            other => panic!("expected an optimum, got {other:?}"),
        }
    }

    /// A degenerate vertex — the first row forces `x = 0` with its slack also
    /// at zero — must not cycle. Bland's rule is what guarantees termination.
    /// max x + y  s.t.  x + s = 0,  y + t = 1  =>  optimum 1 at x = 0, y = 1.
    #[test]
    fn a_degenerate_program_terminates() {
        let problem = LpProblem {
            objective: vec![int(1), int(1), int(0), int(0)],
            constraints: vec![
                vec![int(1), int(0), int(1), int(0)],
                vec![int(0), int(1), int(0), int(1)],
            ],
            rhs: vec![int(0), int(1)],
        };
        match solve_lp(problem) {
            LpSolution::Optimal { value, x } => {
                assert_eq!(value, int(1));
                assert_eq!(x[0], int(0), "the degenerate variable stays at zero");
                assert_eq!(x[1], int(1));
            }
            other => panic!("expected an optimum, got {other:?}"),
        }
    }

    /// The shape `solve_dominance` builds: mixture weights, a margin, and one
    /// surplus column per opponent profile. Regression guard — an earlier
    /// version let phase two pull an artificial column back into the basis and
    /// called this program unbounded, when its optimum is 1/2.
    #[test]
    fn solves_a_dominance_shaped_program() {
        // Columns: y_A, y_B, eps, s_L, s_R.
        let problem = LpProblem {
            objective: vec![int(0), int(0), int(1), int(0), int(0)],
            constraints: vec![
                vec![int(3), int(0), int(-1), int(-1), int(0)],
                vec![int(0), int(3), int(-1), int(0), int(-1)],
                vec![int(1), int(1), int(0), int(0), int(0)],
            ],
            rhs: vec![int(1), int(1), int(1)],
        };
        match solve_lp(problem) {
            LpSolution::Optimal { value, x } => {
                assert_eq!(
                    value,
                    r(1, 2),
                    "the even mixture beats the candidate by 1/2"
                );
                assert_eq!(x[0], r(1, 2));
                assert_eq!(x[1], r(1, 2));
            }
            other => panic!("expected an optimum, got {other:?}"),
        }
    }

    #[test]
    #[should_panic(expected = "constraint rows and right-hand sides must match")]
    fn a_mismatched_right_hand_side_panics_naming_the_mismatch() {
        let problem = LpProblem {
            objective: vec![int(1)],
            constraints: vec![vec![int(1)], vec![int(1)]],
            rhs: vec![int(1)],
        };
        let _ = solve_lp(problem);
    }

    #[test]
    #[should_panic(expected = "constraint matrix must be rectangular")]
    fn a_ragged_constraint_matrix_panics() {
        let problem = LpProblem {
            objective: vec![int(1), int(1)],
            constraints: vec![vec![int(1), int(1)], vec![int(1)]],
            rhs: vec![int(1), int(1)],
        };
        let _ = solve_lp(problem);
    }

    #[test]
    #[should_panic(expected = "objective length must match the column count")]
    fn an_objective_of_the_wrong_length_panics() {
        let problem = LpProblem {
            objective: vec![int(1)],
            constraints: vec![vec![int(1), int(1)]],
            rhs: vec![int(1)],
        };
        let _ = solve_lp(problem);
    }
}
