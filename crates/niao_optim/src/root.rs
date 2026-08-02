//! Nonlinear system root finding.

use crate::result::{OptimizeResult, RootMethod};
use crate::utils::{approx_jacobian, copy_from, dot, mat_vec, norm2, norm_inf};

pub fn root<F>(mut f: F, x0: &[f64], method: RootMethod, max_iter: usize) -> OptimizeResult
where
    F: FnMut(&[f64], &mut [f64]),
{
    match method {
        RootMethod::Hybr => newton_system(&mut f, x0, max_iter),
        RootMethod::Broyden => broyden(&mut f, x0, max_iter),
    }
}

fn newton_system<F>(f: &mut F, x0: &[f64], max_iter: usize) -> OptimizeResult
where
    F: FnMut(&[f64], &mut [f64]),
{
    let n = x0.len();
    let mut x = x0.to_vec();
    let mut fx = vec![0.0; n];
    let mut jac = vec![0.0; n * n];
    let mut nfev = 0usize;
    for nit in 0..max_iter {
        f(&x, &mut fx);
        nfev += 1;
        if norm_inf(&fx) < 1e-10 {
            return OptimizeResult::ok(x, norm2(&fx), None, nit, nfev, "converged");
        }
        nfev += approx_jacobian(&mut *f, &x, n, &mut jac);
        let delta = solve_linear(n, &jac, &fx);
        for i in 0..n {
            x[i] -= delta[i];
        }
    }
    f(&x, &mut fx);
    OptimizeResult::fail(
        x,
        norm2(&fx),
        None,
        max_iter,
        nfev + 1,
        "maximum iterations exceeded",
    )
}

fn broyden<F>(f: &mut F, x0: &[f64], max_iter: usize) -> OptimizeResult
where
    F: FnMut(&[f64], &mut [f64]),
{
    let n = x0.len();
    let mut x = x0.to_vec();
    let mut fx = vec![0.0; n];
    let mut fx_new = vec![0.0; n];
    f(&x, &mut fx);
    let mut nfev = 1usize;
    let mut b = vec![0.0; n * n];
    for i in 0..n {
        b[i * n + i] = 1.0;
    }

    for nit in 0..max_iter {
        if norm_inf(&fx) < 1e-8 {
            return OptimizeResult::ok(x, norm2(&fx), None, nit, nfev, "converged");
        }
        let delta = solve_linear(n, &b, &fx);
        let mut x_new = x.clone();
        for i in 0..n {
            x_new[i] -= delta[i];
        }
        f(&x_new, &mut fx_new);
        nfev += 1;
        let mut s = vec![0.0; n];
        let mut y = vec![0.0; n];
        for i in 0..n {
            s[i] = x_new[i] - x[i];
            y[i] = fx_new[i] - fx[i];
        }
        let ss = dot(&s, &s);
        if ss > 1e-14 {
            let mut bs = vec![0.0; n];
            mat_vec(n, n, &b, &s, &mut bs);
            for i in 0..n {
                for j in 0..n {
                    b[i * n + j] += (y[i] - bs[i]) * s[j] / ss;
                }
            }
        }
        x = x_new;
        copy_from(&fx_new, &mut fx);
    }
    OptimizeResult::fail(
        x,
        norm2(&fx),
        None,
        max_iter,
        nfev,
        "maximum iterations exceeded",
    )
}

fn solve_linear(n: usize, a: &[f64], b: &[f64]) -> Vec<f64> {
    use niao_num::{from_slice, solve};
    let mut a_reg = a.to_vec();
    for i in 0..n {
        a_reg[i * n + i] += 1e-10;
    }
    let a_arr = from_slice(&[n, n], &a_reg).expect("matrix");
    let b_arr = from_slice(&[n, 1], b).expect("rhs");
    solve(&a_arr, &b_arr).expect("solve").to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system2(x: &[f64], out: &mut [f64]) {
        out[0] = x[0] * x[0] + x[1] * x[1] - 1.0;
        out[1] = x[0] - x[1];
    }

    #[test]
    fn broyden_2x2() {
        let res = root(system2, &[0.5, 0.5], RootMethod::Broyden, 100);
        assert!(res.success, "{}", res.message);
        assert!((res.x[0] - 2.0_f64.sqrt() / 2.0).abs() < 1e-5);
        assert!((res.x[1] - 2.0_f64.sqrt() / 2.0).abs() < 1e-5);
    }

    #[test]
    fn newton_2x2() {
        let res = root(system2, &[0.5, 0.5], RootMethod::Hybr, 50);
        assert!(res.success);
        assert!((res.x[0] - 2.0_f64.sqrt() / 2.0).abs() < 1e-6);
    }
}
