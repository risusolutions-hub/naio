//! Line search: Armijo backtracking and strong Wolfe.

use crate::utils::{axpy, copy_from, dot};

#[derive(Debug, Clone, Copy)]
pub struct LineSearchResult {
    pub alpha: f64,
    pub f: f64,
    pub nfev: usize,
    pub ngev: usize,
}

/// Backtracking Armijo; gradient is not evaluated inside the search loop.
pub fn armijo_backtracking<F>(
    f: &mut F,
    x: &[f64],
    grad: &[f64],
    direction: &[f64],
    f0: f64,
    c1: f64,
    alpha_init: f64,
    x_new: &mut [f64],
) -> LineSearchResult
where
    F: FnMut(&[f64], &mut [f64]) -> f64,
{
    let n = x.len();
    let mut alpha = alpha_init;
    let slope = c1 * dot(grad, direction);
    let mut nfev = 0usize;
    let mut buf = vec![0.0; n];
    copy_from(x, x_new);
    axpy(alpha, direction, x_new);
    let mut f_new = f(x_new, &mut buf);
    nfev += 1;
    while f_new > f0 + alpha * slope && alpha > 1e-20 {
        alpha *= 0.5;
        copy_from(x, x_new);
        axpy(alpha, direction, x_new);
        f_new = f(x_new, &mut buf);
        nfev += 1;
    }
    LineSearchResult {
        alpha,
        f: f_new,
        nfev,
        ngev: 0,
    }
}

pub fn strong_wolfe<F, G>(
    f: &mut F,
    g: &mut G,
    x: &[f64],
    grad: &[f64],
    direction: &[f64],
    f0: f64,
    c1: f64,
    c2: f64,
    alpha_max: f64,
    x_new: &mut [f64],
    grad_new: &mut [f64],
) -> LineSearchResult
where
    F: FnMut(&[f64], &mut [f64]) -> f64,
    G: FnMut(&[f64], &mut [f64]),
{
    let n = x.len();
    let mut alpha0 = 0.0f64;
    let mut alpha1 = alpha_max.min(1.0);
    let phi0 = f0;
    let dphi0 = dot(grad, direction);
    let mut nfev = 0usize;
    let mut ngev = 0usize;
    let mut buf = vec![0.0; n];

    copy_from(x, x_new);
    axpy(alpha1, direction, x_new);
    let mut phi1 = f(x_new, &mut buf);
    nfev += 1;

    let mut alpha = alpha1;
    let mut phi = phi1;

    for _ in 0..20 {
        if phi > phi0 + c1 * alpha * dphi0 || (alpha1 > alpha0 && phi >= phi1) {
            alpha = zoom(
                f,
                g,
                x,
                direction,
                phi0,
                dphi0,
                alpha0,
                alpha1,
                c1,
                c2,
                x_new,
                grad_new,
                &mut buf,
                &mut nfev,
                &mut ngev,
            );
            break;
        }
        g(x_new, grad_new);
        ngev += 1;
        let dphi = dot(grad_new, direction);
        if dphi.abs() <= -c2 * dphi0 {
            return LineSearchResult {
                alpha,
                f: phi,
                nfev,
                ngev,
            };
        }
        if dphi >= 0.0 {
            alpha = zoom(
                f,
                g,
                x,
                direction,
                phi0,
                dphi0,
                alpha,
                alpha0,
                c1,
                c2,
                x_new,
                grad_new,
                &mut buf,
                &mut nfev,
                &mut ngev,
            );
            break;
        }
        alpha0 = alpha1;
        phi1 = phi;
        alpha1 = (2.0 * alpha1).min(alpha_max);
        copy_from(x, x_new);
        axpy(alpha1, direction, x_new);
        phi = f(x_new, &mut buf);
        nfev += 1;
        alpha = alpha1;
    }
    g(x_new, grad_new);
    ngev += 1;
    LineSearchResult {
        alpha,
        f: phi,
        nfev,
        ngev,
    }
}

fn zoom<F, G>(
    f: &mut F,
    g: &mut G,
    x: &[f64],
    direction: &[f64],
    phi0: f64,
    dphi0: f64,
    alpha_lo: f64,
    alpha_hi: f64,
    c1: f64,
    c2: f64,
    x_new: &mut [f64],
    grad_new: &mut [f64],
    buf: &mut [f64],
    nfev: &mut usize,
    ngev: &mut usize,
) -> f64
where
    F: FnMut(&[f64], &mut [f64]) -> f64,
    G: FnMut(&[f64], &mut [f64]),
{
    let mut alo = alpha_lo;
    let mut ahi = alpha_hi;
    let mut phi_lo = {
        copy_from(x, x_new);
        axpy(alo, direction, x_new);
        f(x_new, buf)
    };
    *nfev += 1;

    for _ in 0..20 {
        let alpha = 0.5 * (alo + ahi);
        copy_from(x, x_new);
        axpy(alpha, direction, x_new);
        let phi = f(x_new, buf);
        *nfev += 1;
        if phi > phi0 + c1 * alpha * dphi0 || phi >= phi_lo {
            ahi = alpha;
        } else {
            g(x_new, grad_new);
            *ngev += 1;
            let dphi = dot(grad_new, direction);
            if dphi.abs() <= -c2 * dphi0 {
                return alpha;
            }
            if (dphi * (ahi - alo)) >= 0.0 {
                ahi = alo;
            }
            alo = alpha;
            phi_lo = phi;
        }
    }
    0.5 * (alo + ahi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::approx_fprime;

    #[test]
    fn strong_wolfe_quadratic() {
        let x = vec![3.0, 4.0];
        let f = |x: &[f64], _buf: &mut [f64]| x[0] * x[0] + x[1] * x[1];
        let mut grad = vec![0.0; 2];
        approx_fprime(f, &x, &mut grad);
        let direction = vec![-grad[0], -grad[1]];
        let f0 = f(&x, &mut [0.0]);
        let mut x_new = vec![0.0; 2];
        let mut grad_new = vec![0.0; 2];
        let mut f_mut = f;
        let mut g = |x: &[f64], g: &mut [f64]| {
            approx_fprime(f, x, g);
        };
        let ls = strong_wolfe(
            &mut f_mut,
            &mut g,
            &x,
            &grad,
            &direction,
            f0,
            1e-4,
            0.9,
            10.0,
            &mut x_new,
            &mut grad_new,
        );
        assert!(ls.alpha > 0.0);
        assert!(ls.f <= f0);
    }
}
