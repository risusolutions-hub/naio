//! Unconstrained minimization drivers.

use crate::linesearch::{armijo_backtracking, strong_wolfe};
use crate::result::{MinimizeMethod, MinimizeOptions, OptimizeResult};
use crate::utils::{
    approx_fprime, axpy, copy_from, dot, mat_vec, norm2, norm_inf, project_bounds, scale,
    validate_bounds,
};

pub fn minimize<F, G>(
    mut f: F,
    x0: &[f64],
    method: MinimizeMethod,
    jac: Option<G>,
    options: MinimizeOptions,
) -> OptimizeResult
where
    F: FnMut(&[f64], &mut [f64]) -> f64,
    G: FnMut(&[f64], &mut [f64]) -> (),
{
    if let (Some(lo), Some(hi)) = (&options.bounds_lo, &options.bounds_hi) {
        if validate_bounds(lo, hi).is_err() {
            return OptimizeResult::fail(
                x0.to_vec(),
                f(x0, &mut vec![0.0; x0.len()]),
                None,
                0,
                1,
                "bad bounds",
            );
        }
    }

    match method {
        MinimizeMethod::NelderMead => nelder_mead(&mut f, x0, &options),
        MinimizeMethod::Powell => powell(&mut f, x0, &options),
        MinimizeMethod::SteepestDescent => {
            gradient_method(&mut f, jac, x0, &options, MethodKind::SteepestDescent)
        }
        MinimizeMethod::CG => gradient_method(&mut f, jac, x0, &options, MethodKind::CG),
        MinimizeMethod::Bfgs => gradient_method(&mut f, jac, x0, &options, MethodKind::Bfgs),
        MinimizeMethod::LBfgs => gradient_method(&mut f, jac, x0, &options, MethodKind::LBfgs),
        MinimizeMethod::NewtonCG => {
            gradient_method(&mut f, jac, x0, &options, MethodKind::NewtonCG)
        }
    }
}

enum MethodKind {
    SteepestDescent,
    CG,
    Bfgs,
    LBfgs,
    NewtonCG,
}

fn gradient_method<F, G>(
    f: &mut F,
    mut jac: Option<G>,
    x0: &[f64],
    options: &MinimizeOptions,
    kind: MethodKind,
) -> OptimizeResult
where
    F: FnMut(&[f64], &mut [f64]) -> f64,
    G: FnMut(&[f64], &mut [f64]),
{
    let n = x0.len();
    let mut x = x0.to_vec();
    let mut grad = vec![0.0; n];
    let mut nfev = 0usize;
    let mut ngev = 0usize;

    let mut fval = f(&x, &mut grad);
    nfev += 1;
    if let Some(ref mut j) = jac {
        j(&x, &mut grad);
        ngev += 1;
    } else {
        nfev += approx_fprime(&mut *f, &x, &mut grad);
    }

    if let (Some(lo), Some(hi)) = (&options.bounds_lo, &options.bounds_hi) {
        project_bounds(&mut x, lo, hi);
        fval = f(&x, &mut grad);
        nfev += 1;
        if let Some(ref mut j) = jac {
            j(&x, &mut grad);
            ngev += 1;
        } else {
            nfev += approx_fprime(&mut *f, &x, &mut grad);
        }
    }

    let mut direction = vec![0.0; n];
    let mut x_new = vec![0.0; n];
    let mut grad_new = vec![0.0; n];
    let mut grad_old = vec![0.0; n];
    let mut b_mat = vec![0.0; n * n];
    for i in 0..n {
        b_mat[i * n + i] = 1.0;
    }
    let mut h = b_mat.clone();

    let m = options.lbfgs_m;
    let mut s_hist: Vec<Vec<f64>> = Vec::with_capacity(m);
    let mut y_hist: Vec<Vec<f64>> = Vec::with_capacity(m);
    let mut rho_hist: Vec<f64> = Vec::with_capacity(m);

    let mut nit = 0usize;
    let mut first_cg = true;

    for _iter in 0..options.max_iter {
        nit += 1;
        if norm_inf(&grad) < options.gtol {
            return OptimizeResult::ok(
                x,
                fval,
                Some(grad),
                nit,
                nfev + ngev,
                "gradient tolerance reached",
            );
        }

        match kind {
            MethodKind::SteepestDescent => {
                for i in 0..n {
                    direction[i] = -grad[i];
                }
            }
            MethodKind::CG => {
                if first_cg {
                    for i in 0..n {
                        direction[i] = -grad[i];
                    }
                    first_cg = false;
                } else {
                    let num = dot(&grad, &grad);
                    let den = dot(&grad_old, &grad_old);
                    let beta = if den > 0.0 {
                        ((num - dot(&grad, &grad_old)) / den).max(0.0)
                    } else {
                        0.0
                    };
                    for i in 0..n {
                        direction[i] = -grad[i] + beta * direction[i];
                    }
                }
            }
            MethodKind::Bfgs => {
                if let Some(dir) = bfgs_direction(&b_mat, n, &grad) {
                    copy_from(&dir, &mut direction);
                } else {
                    for i in 0..n {
                        direction[i] = -grad[i];
                    }
                }
            }
            MethodKind::LBfgs => {
                lbfgs_direction(&s_hist, &y_hist, &rho_hist, &grad, &mut direction);
            }
            MethodKind::NewtonCG => {
                newton_cg_direction(f, &x, &grad, &mut direction, &mut nfev);
            }
        }

        copy_from(&grad, &mut grad_old);
        let use_wolfe = jac.is_some() && matches!(kind, MethodKind::Bfgs | MethodKind::CG);
        let ls = if use_wolfe {
            let mut g_fn = |xp: &[f64], g: &mut [f64]| {
                if let Some(ref mut j) = jac {
                    j(xp, g);
                    ngev += 1;
                }
            };
            strong_wolfe(
                f,
                &mut g_fn,
                &x,
                &grad,
                &direction,
                fval,
                1e-4,
                0.9,
                10.0,
                &mut x_new,
                &mut grad_new,
            )
        } else {
            let ls = armijo_backtracking(f, &x, &grad, &direction, fval, 1e-4, 1.0, &mut x_new);
            if let Some(ref mut j) = jac {
                j(&x_new, &mut grad_new);
                ngev += 1;
            } else {
                nfev += approx_fprime(&mut *f, &x_new, &mut grad_new);
            }
            ls
        };
        nfev += ls.nfev;
        ngev += ls.ngev;

        if let (Some(lo), Some(hi)) = (&options.bounds_lo, &options.bounds_hi) {
            project_bounds(&mut x_new, lo, hi);
        }

        let dx_norm = norm2(&{
            let mut d = vec![0.0; n];
            for i in 0..n {
                d[i] = x_new[i] - x[i];
            }
            d
        });
        if dx_norm < options.xtol && norm_inf(&grad_new) < options.gtol * 10.0 {
            return OptimizeResult::ok(
                x_new,
                ls.f,
                Some(grad_new),
                nit,
                nfev + ngev,
                "step tolerance reached",
            );
        }
        if (fval - ls.f).abs() < options.ftol * (1.0 + fval.abs()) {
            return OptimizeResult::ok(
                x_new,
                ls.f,
                Some(grad_new),
                nit,
                nfev + ngev,
                "function tolerance reached",
            );
        }

        if matches!(kind, MethodKind::Bfgs) {
            let mut s = vec![0.0; n];
            let mut y = vec![0.0; n];
            for i in 0..n {
                s[i] = x_new[i] - x[i];
                y[i] = grad_new[i] - grad[i];
            }
            bfgs_forward_update(&mut b_mat, n, &s, &y);
        } else if matches!(kind, MethodKind::LBfgs) {
            let mut s = vec![0.0; n];
            let mut y = vec![0.0; n];
            for i in 0..n {
                s[i] = x_new[i] - x[i];
                y[i] = grad_new[i] - grad[i];
            }
            let sy = dot(&s, &y);
            if sy > 1e-10 {
                if s_hist.len() >= m {
                    s_hist.remove(0);
                    y_hist.remove(0);
                    rho_hist.remove(0);
                }
                s_hist.push(s);
                y_hist.push(y);
                rho_hist.push(1.0 / sy);
            }
        }

        copy_from(&grad_new, &mut grad);
        copy_from(&x_new, &mut x);
        fval = ls.f;
    }

    OptimizeResult::fail(
        x,
        fval,
        Some(grad),
        nit,
        nfev + ngev,
        "maximum iterations exceeded",
    )
}

fn bfgs_forward_update(b: &mut [f64], n: usize, s: &[f64], y: &[f64]) {
    let sy = dot(s, y);
    if sy <= 1e-16 {
        return;
    }
    let mut bs = vec![0.0; n];
    mat_vec(n, n, b, s, &mut bs);
    let stbs = dot(s, &bs);
    if stbs <= 1e-16 {
        return;
    }
    for i in 0..n {
        for j in 0..n {
            b[i * n + j] -= bs[i] * bs[j] / stbs;
            b[i * n + j] += y[i] * y[j] / sy;
        }
    }
}

fn bfgs_direction(b: &[f64], n: usize, grad: &[f64]) -> Option<Vec<f64>> {
    use niao_num::{from_slice, solve};
    let neg_g: Vec<f64> = grad.iter().map(|&g| -g).collect();
    let b_arr = from_slice(&[n, n], b).ok()?;
    let rhs = from_slice(&[n, 1], &neg_g).ok()?;
    solve(&b_arr, &rhs).ok().map(|x| x.to_vec())
}

fn bfgs_update(h: &mut [f64], n: usize, s: &[f64], y: &[f64]) {
    let sy = dot(s, y);
    if sy <= 1e-16 {
        return;
    }
    let rho = 1.0 / sy;
    let mut hy = vec![0.0; n];
    mat_vec(n, n, h, y, &mut hy);
    let yhy = dot(y, &hy);
    for i in 0..n {
        for j in 0..n {
            h[i * n + j] += rho * s[i] * s[j] - rho * (s[i] * hy[j] + hy[i] * s[j])
                + rho * (1.0 + rho * yhy) * hy[i] * hy[j];
        }
    }
}

fn lbfgs_direction(
    s_hist: &[Vec<f64>],
    y_hist: &[Vec<f64>],
    rho_hist: &[f64],
    grad: &[f64],
    direction: &mut [f64],
) {
    let n = grad.len();
    let k = s_hist.len();
    copy_from(grad, direction);
    let mut alpha = vec![0.0; k];
    for i in (0..k).rev() {
        alpha[i] = rho_hist[i] * dot(&s_hist[i], direction);
        axpy(-alpha[i], &y_hist[i], direction);
    }
    if k > 0 {
        let sy = dot(&s_hist[k - 1], &y_hist[k - 1]);
        let yy = dot(&y_hist[k - 1], &y_hist[k - 1]);
        let gamma = if yy > 0.0 { sy / yy } else { 1.0 };
        scale(gamma, direction);
    }
    for i in 0..k {
        let beta = rho_hist[i] * dot(&y_hist[i], direction);
        axpy(alpha[i] - beta, &s_hist[i], direction);
    }
    scale(-1.0, direction);
}

fn newton_cg_direction<F>(
    f: &mut F,
    x: &[f64],
    grad: &[f64],
    direction: &mut [f64],
    nfev: &mut usize,
) where
    F: FnMut(&[f64], &mut [f64]) -> f64,
{
    let n = x.len();
    let mut hvp = vec![0.0; n];
    hessian_vector_product(f, x, grad, &mut hvp, nfev);
    // Solve H p = -g via CG
    direction.fill(0.0);
    let mut r = vec![0.0; n];
    for i in 0..n {
        r[i] = -grad[i] - hvp[i];
    }
    copy_from(&r, direction);
    let mut p = direction.to_vec();
    let mut rsold = dot(&r, &r);
    for _ in 0..n.min(20) {
        hessian_vector_product(f, x, &p, &mut hvp, nfev);
        let alpha = rsold / dot(&p, &hvp).max(1e-16);
        axpy(alpha, &p, direction);
        axpy(-alpha, &hvp, &mut r);
        let rsnew = dot(&r, &r);
        if rsnew.sqrt() < 1e-8 {
            break;
        }
        scale(rsnew / rsold, &mut p);
        axpy(1.0, &r, &mut p);
        rsold = rsnew;
    }
    scale(-1.0, direction);
}

fn hessian_vector_product<F>(f: &mut F, x: &[f64], v: &[f64], out: &mut [f64], nfev: &mut usize)
where
    F: FnMut(&[f64], &mut [f64]) -> f64,
{
    let n = x.len();
    let h = crate::utils::fd_step(1.0);
    let mut x_plus = x.to_vec();
    let mut x_minus = x.to_vec();
    let mut grad_plus = vec![0.0; n];
    let mut grad_minus = vec![0.0; n];
    for i in 0..n {
        x_plus[i] += h * v[i];
        x_minus[i] -= h * v[i];
    }
    f(&x_plus, &mut grad_plus);
    *nfev += 1;
    f(&x_minus, &mut grad_minus);
    *nfev += 1;
    for i in 0..n {
        out[i] = (grad_plus[i] - grad_minus[i]) / (2.0 * h);
    }
}

fn nelder_mead<F>(f: &mut F, x0: &[f64], options: &MinimizeOptions) -> OptimizeResult
where
    F: FnMut(&[f64], &mut [f64]) -> f64,
{
    let n = x0.len();
    let mut simplex: Vec<Vec<f64>> = Vec::with_capacity(n + 1);
    simplex.push(x0.to_vec());
    for i in 0..n {
        let mut v = x0.to_vec();
        v[i] += if x0[i].abs() > 1e-6 {
            0.05 * x0[i]
        } else {
            0.00025
        };
        simplex.push(v);
    }
    let mut fvals: Vec<f64> = simplex.iter().map(|s| f(s, &mut vec![0.0; n])).collect();
    let mut nfev = simplex.len();
    let alpha = 1.0;
    let gamma = 2.0;
    let rho = 0.5;
    let sigma = 0.5;
    let mut nit = 0usize;
    let mut buf = vec![0.0; n];

    for _ in 0..options.max_iter {
        nit += 1;
        let mut order: Vec<usize> = (0..=n).collect();
        order.sort_by(|&a, &b| fvals[a].partial_cmp(&fvals[b]).unwrap());
        let best = order[0];
        let worst = order[n];
        let second_worst = order[n - 1];

        if norm_inf(&{
            let mut d = vec![0.0; n];
            for i in 0..n {
                d[i] = (simplex[best][i] - simplex[worst][i]).abs();
            }
            d
        }) < options.xtol
        {
            return OptimizeResult::ok(
                simplex[best].clone(),
                fvals[best],
                None,
                nit,
                nfev,
                "simplex size tolerance reached",
            );
        }

        let mut centroid = vec![0.0; n];
        for &idx in &order[..n] {
            for i in 0..n {
                centroid[i] += simplex[idx][i];
            }
        }
        scale(1.0 / n as f64, &mut centroid);

        let mut xr = centroid.clone();
        for i in 0..n {
            xr[i] += alpha * (centroid[i] - simplex[worst][i]);
        }
        let fr = f(&xr, &mut buf);
        nfev += 1;

        if fr < fvals[best] {
            let mut xe = centroid.clone();
            for i in 0..n {
                xe[i] += gamma * (xr[i] - centroid[i]);
            }
            let fe = f(&xe, &mut buf);
            nfev += 1;
            if fe < fr {
                simplex[worst] = xe;
                fvals[worst] = fe;
            } else {
                simplex[worst] = xr;
                fvals[worst] = fr;
            }
        } else if fr < fvals[second_worst] {
            simplex[worst] = xr;
            fvals[worst] = fr;
        } else {
            let mut xc = centroid.clone();
            for i in 0..n {
                xc[i] += rho * (simplex[worst][i] - centroid[i]);
            }
            let fc = f(&xc, &mut buf);
            nfev += 1;
            if fc < fvals[worst] {
                simplex[worst] = xc;
                fvals[worst] = fc;
            } else {
                for j in 0..=n {
                    if j == best {
                        continue;
                    }
                    for i in 0..n {
                        simplex[j][i] =
                            simplex[best][i] + sigma * (simplex[j][i] - simplex[best][i]);
                    }
                    fvals[j] = f(&simplex[j], &mut buf);
                    nfev += 1;
                }
            }
        }
    }

    let best = (0..=n)
        .min_by(|&a, &b| fvals[a].partial_cmp(&fvals[b]).unwrap())
        .unwrap();
    OptimizeResult::fail(
        simplex[best].clone(),
        fvals[best],
        None,
        nit,
        nfev,
        "maximum iterations exceeded",
    )
}

fn powell<F>(f: &mut F, x0: &[f64], options: &MinimizeOptions) -> OptimizeResult
where
    F: FnMut(&[f64], &mut [f64]) -> f64,
{
    let n = x0.len();
    let mut x = x0.to_vec();
    let mut dirs: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            let mut d = vec![0.0; n];
            d[i] = 1.0;
            d
        })
        .collect();
    let mut fval = f(&x, &mut vec![0.0; n]);
    let mut nfev = 1usize;
    let mut nit = 0usize;
    let mut buf = vec![0.0; n];

    for _ in 0..options.max_iter {
        nit += 1;
        let x_start = x.clone();
        let f_start = fval;
        for d in &dirs {
            let (xn, fn_) = line_minimize_1d(f, &x, d, fval, &mut nfev, &mut buf);
            x = xn;
            fval = fn_;
        }
        let mut d_last = vec![0.0; n];
        for i in 0..n {
            d_last[i] = x[i] - x_start[i];
        }
        if norm2(&d_last) > options.xtol {
            let (xn, fn_) = line_minimize_1d(f, &x, &d_last, fval, &mut nfev, &mut buf);
            x = xn;
            fval = fn_;
        }
        if norm2(&d_last) > 1e-12 {
            dirs.remove(0);
            dirs.push(d_last);
        }
        if (f_start - fval).abs() < options.ftol * (1.0 + f_start.abs()) {
            return OptimizeResult::ok(x, fval, None, nit, nfev, "function tolerance reached");
        }
    }
    OptimizeResult::fail(x, fval, None, nit, nfev, "maximum iterations exceeded")
}

fn line_minimize_1d<F>(
    f: &mut F,
    x: &[f64],
    direction: &[f64],
    f0: f64,
    nfev: &mut usize,
    buf: &mut [f64],
) -> (Vec<f64>, f64)
where
    F: FnMut(&[f64], &mut [f64]) -> f64,
{
    let n = x.len();
    let mut alpha = 0.0;
    let mut best_x = x.to_vec();
    let mut best_f = f0;
    let mut step = 1.0;
    let mut x_try = vec![0.0; n];
    for _ in 0..20 {
        copy_from(x, &mut x_try);
        axpy(step, direction, &mut x_try);
        let fv = f(&x_try, buf);
        *nfev += 1;
        if fv < best_f {
            best_f = fv;
            best_x = x_try.clone();
            alpha = step;
        } else {
            break;
        }
        step *= 2.0;
    }
    // refine with golden section around [0, alpha*2]
    let hi = alpha * 2.0;
    let (a, _, f_mid) = golden_section_bracket(f, x, direction, 0.0, hi.max(1e-6), buf, nfev);
    copy_from(x, &mut best_x);
    axpy(a, direction, &mut best_x);
    (best_x, f_mid)
}

fn golden_section_bracket<F>(
    f: &mut F,
    x: &[f64],
    direction: &[f64],
    mut a: f64,
    mut b: f64,
    buf: &mut [f64],
    nfev: &mut usize,
) -> (f64, f64, f64)
where
    F: FnMut(&[f64], &mut [f64]) -> f64,
{
    let phi = (5.0_f64.sqrt() - 1.0) / 2.0;
    let n = x.len();
    let mut x_try = vec![0.0; n];
    let mut eval = |t: f64| {
        copy_from(x, &mut x_try);
        axpy(t, direction, &mut x_try);
        let v = f(&x_try, buf);
        *nfev += 1;
        v
    };
    let mut c = b - phi * (b - a);
    let mut d = a + phi * (b - a);
    let mut fc = eval(c);
    let mut fd = eval(d);
    for _ in 0..40 {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - phi * (b - a);
            fc = eval(c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + phi * (b - a);
            fd = eval(d);
        }
    }
    let mid = 0.5 * (a + b);
    (mid, b, eval(mid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_problems::*;

    fn check_result(res: &OptimizeResult, x_star: &[f64], tol: f64) {
        for (a, b) in res.x.iter().zip(x_star.iter()) {
            assert!(
                (a - b).abs() < tol,
                "x={:?} expected {:?} fun={} msg={}",
                res.x,
                x_star,
                res.fun,
                res.message
            );
        }
    }

    #[test]
    fn rosenbrock_bfgs() {
        let res = minimize(
            rosenbrock,
            &[-1.2, 1.0],
            MinimizeMethod::Bfgs,
            Some(rosenbrock_grad),
            MinimizeOptions {
                max_iter: 5000,
                gtol: 1e-6,
                ftol: 1e-10,
                ..Default::default()
            },
        );
        check_result(&res, &[1.0, 1.0], 1.5);
    }

    #[test]
    fn rosenbrock_lbfgs() {
        let res = minimize(
            rosenbrock,
            &[-1.2, 1.0],
            MinimizeMethod::LBfgs,
            Some(rosenbrock_grad),
            MinimizeOptions {
                max_iter: 5000,
                gtol: 1e-6,
                ftol: 1e-10,
                ..Default::default()
            },
        );
        check_result(&res, &[1.0, 1.0], 1.5);
    }

    #[test]
    fn rosenbrock_cg() {
        let res = minimize(
            rosenbrock,
            &[-1.2, 1.0],
            MinimizeMethod::CG,
            Some(rosenbrock_grad),
            MinimizeOptions {
                max_iter: 500,
                ..Default::default()
            },
        );
        check_result(&res, &[1.0, 1.0], 1.5);
    }

    #[test]
    fn rosenbrock_nelder_mead() {
        let res = minimize(
            rosenbrock,
            &[-1.2, 1.0],
            MinimizeMethod::NelderMead,
            None::<fn(&[f64], &mut [f64]) -> ()>,
            MinimizeOptions {
                max_iter: 1000,
                ..Default::default()
            },
        );
        check_result(&res, &[1.0, 1.0], 1.5);
    }

    #[test]
    fn beale_bfgs() {
        let res = minimize(
            beale,
            &[1.0, 1.0],
            MinimizeMethod::Bfgs,
            Some(beale_grad),
            MinimizeOptions {
                max_iter: 5000,
                gtol: 1e-6,
                ..Default::default()
            },
        );
        check_result(&res, &[3.0, 0.5], 1.2);
    }

    #[test]
    fn himmelblau_nelder_mead() {
        let res = minimize(
            himmelblau,
            &[0.0, 0.0],
            MinimizeMethod::NelderMead,
            None::<fn(&[f64], &mut [f64]) -> ()>,
            MinimizeOptions {
                max_iter: 1000,
                ..Default::default()
            },
        );
        assert!(res.success);
        assert!(res.fun < 1e-6);
    }

    #[test]
    fn lbfgs_direction_is_descent() {
        let x0 = vec![-1.0, 1.0];
        let mut g1 = vec![0.0; 2];
        rosenbrock_grad(&x0, &mut g1);
        let s = vec![0.1, -0.05];
        let mut y = vec![0.0; 2];
        let mut x1 = x0.clone();
        for i in 0..2 {
            x1[i] += s[i];
        }
        let mut g0 = vec![0.0; 2];
        rosenbrock_grad(&x0, &mut g0);
        rosenbrock_grad(&x1, &mut g1);
        for i in 0..2 {
            y[i] = g1[i] - g0[i];
        }
        let mut dir = vec![0.0; 2];
        lbfgs_direction(
            &[s.clone()],
            &[y.clone()],
            &[1.0 / dot(&s, &y)],
            &g1,
            &mut dir,
        );
        assert!(dot(&g1, &dir) < 0.0, "not descent: g·d={}", dot(&g1, &dir));
    }

    #[test]
    fn max_iter_returns_failure() {
        let res = minimize(
            rosenbrock,
            &[-1.2, 1.0],
            MinimizeMethod::Bfgs,
            Some(rosenbrock_grad),
            MinimizeOptions {
                max_iter: 1,
                ..Default::default()
            },
        );
        assert!(!res.success);
        assert_eq!(res.into_result().unwrap_err().code(), 4033);
    }
}
