//! Linear programming via revised simplex.

use crate::error::{OptimError, OptimResult};
use crate::result::{LinprogProblem, LinprogResult};
use crate::utils::dot;

pub fn linprog(problem: &LinprogProblem) -> OptimResult<LinprogResult> {
    let n = problem.n_vars;
    let n_ub = problem.n_ub;
    let n_eq = problem.n_eq;

    let n_slack = n_ub;
    let total_vars = n + n_slack;

    if problem.c.len() != n {
        return Err(OptimError::Error("c length mismatch".into()));
    }

    let mut c = vec![0.0; total_vars];
    c[..n].copy_from_slice(&problem.c);

    let mut a: Vec<Vec<f64>> = vec![vec![0.0; total_vars]; n_ub + n_eq];
    let mut b = vec![0.0; n_ub + n_eq];

    if let (Some(a_ub), Some(b_ub)) = (&problem.a_ub, &problem.b_ub) {
        if a_ub.len() != n_ub * n || b_ub.len() != n_ub {
            return Err(OptimError::Error("A_ub/b_ub shape mismatch".into()));
        }
        for i in 0..n_ub {
            for j in 0..n {
                a[i][j] = a_ub[i * n + j];
            }
            a[i][n + i] = 1.0;
            b[i] = b_ub[i];
        }
    }

    if let (Some(a_eq), Some(b_eq)) = (&problem.a_eq, &problem.b_eq) {
        if a_eq.len() != n_eq * n || b_eq.len() != n_eq {
            return Err(OptimError::Error("A_eq/b_eq shape mismatch".into()));
        }
        for i in 0..n_eq {
            for j in 0..n {
                a[n_ub + i][j] = a_eq[i * n + j];
            }
            b[n_ub + i] = b_eq[i];
        }
    }

    let m = a.len();
    if m == 0 {
        let x = vec![0.0; n];
        if problem.c.iter().all(|&ci| ci >= 0.0) {
            return Ok(LinprogResult {
                x,
                fun: 0.0,
                success: true,
                message: "optimal at origin".into(),
                nit: 0,
            });
        }
        return Err(OptimError::Unbounded("problem is unbounded".into()));
    }

    revised_simplex(&c, &a, &b, n, total_vars)
}

fn revised_simplex(
    c: &[f64],
    a: &[Vec<f64>],
    b: &[f64],
    n_orig: usize,
    n_vars: usize,
) -> OptimResult<LinprogResult> {
    let m = a.len();
    let mut basis: Vec<usize> = (0..m).map(|i| n_orig + i).collect();
    let mut x = vec![0.0; n_vars];
    for i in 0..m {
        if b[i] >= 0.0 {
            x[basis[i]] = b[i];
        } else {
            return Err(OptimError::Infeasible("problem is infeasible".into()));
        }
    }

    let mut nit = 0usize;
    for _ in 0..1000 {
        nit += 1;
        let b_mat = extract_basis(a, &basis, m);
        let b_inv_b = solve_basis(&b_mat, b)?;
        for i in 0..m {
            x[basis[i]] = b_inv_b[i];
        }
        for j in 0..n_vars {
            if basis.contains(&j) {
                continue;
            }
            x[j] = 0.0;
        }

        let mut c_b = vec![0.0; m];
        for i in 0..m {
            c_b[i] = c[basis[i]];
        }
        let pi = solve_basis_transpose(&b_mat, &c_b)?;
        let mut reduced = vec![0.0; n_vars];
        for j in 0..n_vars {
            let mut col_j = vec![0.0; m];
            for i in 0..m {
                col_j[i] = a[i][j];
            }
            reduced[j] = c[j] - dot(&pi, &col_j);
        }

        if reduced.iter().all(|&r| r >= -1e-10) {
            let fun = dot(&c[..n_orig], &x[..n_orig]);
            return Ok(LinprogResult {
                x: x[..n_orig].to_vec(),
                fun,
                success: true,
                message: "optimal".into(),
                nit,
            });
        }

        let enter = (0..n_vars)
            .filter(|&j| !basis.contains(&j) && reduced[j] < -1e-10)
            .min_by(|&a, &b| {
                reduced[a]
                    .partial_cmp(&reduced[b])
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .ok_or_else(|| OptimError::Infeasible("no entering variable".into()))?;

        if reduced[enter] >= -1e-10 {
            let fun = dot(&c[..n_orig], &x[..n_orig]);
            return Ok(LinprogResult {
                x: x[..n_orig].to_vec(),
                fun,
                success: true,
                message: "optimal".into(),
                nit,
            });
        }

        let mut d = vec![0.0; m];
        for i in 0..m {
            d[i] = a[i][enter];
        }
        let d_solved = solve_basis(&b_mat, &d)?;
        let mut theta = f64::INFINITY;
        let mut leave_idx = 0usize;
        for i in 0..m {
            if d_solved[i] > 1e-12 {
                let t = x[basis[i]] / d_solved[i];
                if t < theta {
                    theta = t;
                    leave_idx = i;
                }
            }
        }
        if theta.is_infinite() {
            return Err(OptimError::Unbounded("problem is unbounded".into()));
        }
        basis[leave_idx] = enter;
    }
    Err(OptimError::NonConvergence("simplex iteration limit".into()))
}

fn extract_basis(a: &[Vec<f64>], basis: &[usize], m: usize) -> Vec<f64> {
    let mut b_mat = vec![0.0; m * m];
    for i in 0..m {
        for j in 0..m {
            b_mat[i * m + j] = a[i][basis[j]];
        }
    }
    b_mat
}

fn solve_basis(b: &[f64], rhs: &[f64]) -> OptimResult<Vec<f64>> {
    let m = rhs.len();
    use niao_num::{from_slice, solve};
    let b_arr = from_slice(&[m, m], b).map_err(|e| OptimError::Error(e.to_string()))?;
    let rhs_arr = from_slice(&[m, 1], rhs).map_err(|e| OptimError::Error(e.to_string()))?;
    solve(&b_arr, &rhs_arr)
        .map(|x| x.to_vec())
        .map_err(|e| OptimError::Error(e.to_string()))
}

fn solve_basis_transpose(b: &[f64], rhs: &[f64]) -> OptimResult<Vec<f64>> {
    let m = rhs.len();
    use niao_num::{from_slice, solve};
    let b_arr = from_slice(&[m, m], b).map_err(|e| OptimError::Error(e.to_string()))?;
    let bt = b_arr
        .transpose()
        .map_err(|e| OptimError::Error(e.to_string()))?;
    let rhs_arr = from_slice(&[m, 1], rhs).map_err(|e| OptimError::Error(e.to_string()))?;
    solve(&bt, &rhs_arr)
        .map(|x| x.to_vec())
        .map_err(|e| OptimError::Error(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_problems::{lp_feasible, lp_infeasible};

    #[test]
    fn textbook_lp() {
        let (c, a_ub, b_ub, _lo) = lp_feasible();
        let flat: Vec<f64> = a_ub[0].clone();
        let problem = LinprogProblem {
            c,
            a_ub: Some(flat),
            b_ub: Some(b_ub),
            a_eq: None,
            b_eq: None,
            n_vars: 2,
            n_ub: 1,
            n_eq: 0,
        };
        let res = linprog(&problem).unwrap();
        assert!(res.success);
        assert!((res.x[0] - 1.0).abs() < 1e-4 || (res.x[1] - 1.0).abs() < 1e-4);
        assert!(res.fun <= -0.99);
    }

    #[test]
    fn infeasible_lp() {
        let (c, a_ub, b_ub) = lp_infeasible();
        let flat: Vec<f64> = a_ub.concat();
        let problem = LinprogProblem {
            c,
            a_ub: Some(flat),
            b_ub: Some(b_ub),
            a_eq: None,
            b_eq: None,
            n_vars: 2,
            n_ub: 2,
            n_eq: 0,
        };
        let err = linprog(&problem).unwrap_err();
        assert!(matches!(err, OptimError::Infeasible(_)));
        assert_eq!(err.code(), 4035);
    }
}
