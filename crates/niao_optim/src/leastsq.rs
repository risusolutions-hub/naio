//! Nonlinear least squares: Gauss-Newton and Levenberg-Marquardt.

use crate::result::{LeastSquaresMethod, LeastSquaresOptions, OptimizeResult};
use crate::utils::{approx_jacobian, dot, mat_t_vec, norm2, norm_inf, outer};

pub fn least_squares<F, J>(
    mut resid: F,
    x0: &[f64],
    method: LeastSquaresMethod,
    options: LeastSquaresOptions,
    jacobian: Option<J>,
) -> OptimizeResult
where
    F: FnMut(&[f64], &mut [f64]),
    J: FnMut(&[f64], &mut [f64]),
{
    match method {
        LeastSquaresMethod::GaussNewton => gauss_newton(&mut resid, x0, &options, jacobian),
        LeastSquaresMethod::LevenbergMarquardt => {
            levenberg_marquardt(&mut resid, x0, &options, jacobian)
        }
    }
}

fn eval_jacobian<F, J>(
    resid: &mut F,
    jac_fn: &mut Option<J>,
    x: &[f64],
    m: usize,
    n: usize,
    jac: &mut [f64],
    nfev: &mut usize,
) where
    F: FnMut(&[f64], &mut [f64]),
    J: FnMut(&[f64], &mut [f64]),
{
    if let Some(j) = jac_fn {
        j(x, jac);
    } else {
        *nfev += approx_jacobian(&mut *resid, x, m, jac);
    }
}

fn gauss_newton<F, J>(
    resid: &mut F,
    x0: &[f64],
    options: &LeastSquaresOptions,
    mut jacobian: Option<J>,
) -> OptimizeResult
where
    F: FnMut(&[f64], &mut [f64]),
    J: FnMut(&[f64], &mut [f64]),
{
    let n = x0.len();
    let mut x = x0.to_vec();
    let m = if options.n_resid > 0 {
        options.n_resid
    } else {
        1
    };
    let mut r = vec![0.0; m];
    let mut jac = vec![0.0; m * n];
    resid(&x, &mut r);
    let mut nfev = 1usize;

    for nit in 0..options.max_iter {
        if norm2(&r) < options.ftol {
            return OptimizeResult::ok(
                x,
                0.5 * dot(&r, &r),
                None,
                nit,
                nfev,
                "residual tolerance reached",
            );
        }
        eval_jacobian(resid, &mut jacobian, &x, m, n, &mut jac, &mut nfev);
        let mut jtr = vec![0.0; n];
        mat_t_vec(m, n, &jac, &r, &mut jtr);
        for i in 0..n {
            jtr[i] = -jtr[i];
        }
        let mut jtj = vec![0.0; n * n];
        for i in 0..m {
            let row = &jac[i * n..(i + 1) * n];
            outer(n, row, row, &mut jtj);
        }
        let delta = solve_spd(n, &jtj, &jtr);
        let mut alpha = 1.0;
        let mut x_new = x.clone();
        let mut r_new = r.clone();
        while alpha > 1e-8 {
            for i in 0..n {
                x_new[i] = x[i] + alpha * delta[i];
            }
            r_new.fill(0.0);
            resid(&x_new, &mut r_new);
            nfev += 1;
            if norm2(&r_new) < norm2(&r) {
                break;
            }
            alpha *= 0.5;
        }
        if norm2(&{
            let mut d = vec![0.0; n];
            for i in 0..n {
                d[i] = x_new[i] - x[i];
            }
            d
        }) < options.xtol
        {
            return OptimizeResult::ok(
                x_new,
                0.5 * dot(&r_new, &r_new),
                None,
                nit + 1,
                nfev,
                "step tolerance reached",
            );
        }
        x = x_new;
        r = r_new;
    }
    OptimizeResult::fail(
        x,
        0.5 * dot(&r, &r),
        None,
        options.max_iter,
        nfev,
        "maximum iterations exceeded",
    )
}

fn levenberg_marquardt<F, J>(
    resid: &mut F,
    x0: &[f64],
    options: &LeastSquaresOptions,
    mut jacobian: Option<J>,
) -> OptimizeResult
where
    F: FnMut(&[f64], &mut [f64]),
    J: FnMut(&[f64], &mut [f64]),
{
    let n = x0.len();
    let mut x = x0.to_vec();
    let m = if options.n_resid > 0 {
        options.n_resid
    } else {
        1
    };
    let mut r = vec![0.0; m];
    let mut jac = vec![0.0; m * n];
    resid(&x, &mut r);
    let mut nfev = 1usize;
    let mut lambda = options.lambda_init;
    let mut cost = 0.5 * dot(&r, &r);

    for nit in 0..options.max_iter {
        if norm_inf(&r) < options.gtol {
            return OptimizeResult::ok(x, cost, None, nit, nfev, "residual tolerance reached");
        }
        eval_jacobian(resid, &mut jacobian, &x, m, n, &mut jac, &mut nfev);
        let mut jtr = vec![0.0; n];
        mat_t_vec(m, n, &jac, &r, &mut jtr);
        for i in 0..n {
            jtr[i] = -jtr[i];
        }
        let mut jtj = vec![0.0; n * n];
        for i in 0..m {
            let row = &jac[i * n..(i + 1) * n];
            outer(n, row, row, &mut jtj);
        }
        for i in 0..n {
            jtj[i * n + i] += lambda;
        }
        let delta = solve_spd(n, &jtj, &jtr);
        let mut alpha = 1.0;
        let mut accepted = false;
        let mut x_new = x.clone();
        let mut r_new = r.clone();
        let mut cost_new = cost;
        while alpha > 1e-8 {
            for i in 0..n {
                x_new[i] = x[i] + alpha * delta[i];
            }
            r_new.fill(0.0);
            resid(&x_new, &mut r_new);
            nfev += 1;
            cost_new = 0.5 * dot(&r_new, &r_new);
            if cost_new < cost {
                accepted = true;
                break;
            }
            alpha *= 0.5;
        }
        if accepted {
            let old_cost = cost;
            x = x_new;
            r = r_new;
            cost = cost_new;
            lambda = (lambda * 0.3).max(1e-12);
            if (old_cost - cost_new).abs() < options.ftol * (1.0 + old_cost)
                || norm_inf(&r) < options.gtol
            {
                return OptimizeResult::ok(x, cost_new, None, nit + 1, nfev, "converged");
            }
        } else {
            lambda *= 10.0;
        }
    }
    OptimizeResult::fail(
        x,
        0.5 * dot(&r, &r),
        None,
        options.max_iter,
        nfev,
        "maximum iterations exceeded",
    )
}

fn solve_spd(n: usize, a: &[f64], b: &[f64]) -> Vec<f64> {
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
    use crate::test_problems::{exp_fit_fixture, exp_jacobian, exp_residuals};

    #[test]
    fn exp_curve_fit_lm() {
        let (xs, ys, x0) = exp_fit_fixture();
        let xs_c = xs.clone();
        let resid_fn = |params: &[f64], out: &mut [f64]| {
            exp_residuals(params, &xs_c, &ys, out);
        };
        let xs_j = xs.clone();
        let jac_fn = |params: &[f64], jac: &mut [f64]| {
            exp_jacobian(params, &xs_j, jac);
        };
        let res = least_squares(
            resid_fn,
            &x0,
            LeastSquaresMethod::LevenbergMarquardt,
            LeastSquaresOptions {
                max_iter: 1000,
                n_resid: 10,
                lambda_init: 0.01,
                ..Default::default()
            },
            Some(jac_fn),
        );
        assert!(res.success, "{}", res.message);
        assert!((res.x[0] - 2.5).abs() < 0.4, "a={}", res.x[0]);
        assert!((res.x[1] - 1.0).abs() < 0.2, "b={}", res.x[1]);
    }

    #[test]
    fn linear_curve_fit_gn() {
        let xs: Vec<f64> = (0..8).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|&x| 2.0 * x + 1.0).collect();
        let xs_c = xs.clone();
        let ys_c = ys.clone();
        let resid_fn = move |params: &[f64], out: &mut [f64]| {
            for (i, &x) in xs_c.iter().enumerate() {
                out[i] = params[0] * x + params[1] - ys_c[i];
            }
        };
        let jac_fn = move |params: &[f64], jac: &mut [f64]| {
            let _ = params;
            for (i, &x) in xs.iter().enumerate() {
                jac[i * 2 + 0] = x;
                jac[i * 2 + 1] = 1.0;
            }
        };
        let res = least_squares(
            resid_fn,
            &[0.0, 0.0],
            LeastSquaresMethod::GaussNewton,
            LeastSquaresOptions {
                max_iter: 50,
                n_resid: 8,
                ..Default::default()
            },
            Some(jac_fn),
        );
        assert!(res.success, "{}", res.message);
        assert!((res.x[0] - 2.0).abs() < 1e-6);
        assert!((res.x[1] - 1.0).abs() < 1e-6);
    }
}
