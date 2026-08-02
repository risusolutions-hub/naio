//! Scalar minimization and root finding.

use crate::result::{OptimizeResult, RootScalarMethod, ScalarMethod};
use crate::utils::fd_step;

pub fn minimize_scalar<F>(mut f: F, bracket: (f64, f64), method: ScalarMethod) -> OptimizeResult
where
    F: FnMut(f64) -> f64,
{
    match method {
        ScalarMethod::Golden => golden_section(&mut f, bracket.0, bracket.1),
        ScalarMethod::Brent => brent_minimize(&mut f, bracket.0, bracket.1),
        ScalarMethod::Bounded => {
            let (a, b) = bracket;
            if a > b {
                return OptimizeResult::fail(vec![a], f(a), None, 0, 1, "bad bounds");
            }
            golden_section(&mut f, a, b)
        }
    }
}

pub fn root_scalar<F, G>(
    mut f: F,
    bracket: (f64, f64),
    method: RootScalarMethod,
    fprime: Option<G>,
    x0: Option<f64>,
) -> OptimizeResult
where
    F: FnMut(f64) -> f64,
    G: FnMut(f64) -> f64,
{
    match method {
        RootScalarMethod::Bisection => bisection(&mut f, bracket.0, bracket.1),
        RootScalarMethod::Brent => brent_root(&mut f, bracket.0, bracket.1),
        RootScalarMethod::Secant => {
            let x0 = x0.unwrap_or(bracket.0);
            let x1 = bracket.1;
            secant(&mut f, x0, x1)
        }
        RootScalarMethod::Newton => {
            let x0 = x0.unwrap_or(0.5 * (bracket.0 + bracket.1));
            if let Some(mut fp) = fprime {
                newton_scalar(&mut f, &mut fp, x0)
            } else {
                newton_scalar_fd(&mut f, x0)
            }
        }
    }
}

fn golden_section<F>(f: &mut F, mut a: f64, mut b: f64) -> OptimizeResult
where
    F: FnMut(f64) -> f64,
{
    let phi = (5.0_f64.sqrt() - 1.0) / 2.0;
    let mut nfev = 0usize;
    let mut c = b - phi * (b - a);
    let mut d = a + phi * (b - a);
    let mut fc = f(c);
    nfev += 1;
    let mut fd = f(d);
    nfev += 1;
    for nit in 0..200 {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - phi * (b - a);
            fc = f(c);
            nfev += 1;
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + phi * (b - a);
            fd = f(d);
            nfev += 1;
        }
        if (b - a).abs() < 1e-10 {
            let x = 0.5 * (a + b);
            return OptimizeResult::ok(vec![x], f(x), None, nit + 1, nfev + 1, "converged");
        }
    }
    let x = 0.5 * (a + b);
    OptimizeResult::fail(
        vec![x],
        f(x),
        None,
        200,
        nfev + 1,
        "maximum iterations exceeded",
    )
}

fn brent_minimize<F>(f: &mut F, mut a: f64, mut b: f64) -> OptimizeResult
where
    F: FnMut(f64) -> f64,
{
    let tol = 1e-10;
    let mut nfev = 0usize;
    let (mut x, mut w, mut v) = (a, a, a);
    let (mut fx, mut fw, mut fv) = (f(a), f(a), f(a));
    nfev += 3;
    let mut d: f64 = 0.0;
    let mut e: f64 = 0.0;
    for nit in 0..200 {
        let xm = 0.5 * (a + b);
        let tol1 = tol * x.abs() + 1e-11;
        if (x - xm).abs() <= 2.0 * tol1 {
            return OptimizeResult::ok(vec![x], fx, None, nit, nfev, "converged");
        }
        if e.abs() > tol1 {
            let mut r = (x - w) * (fx - fv);
            let mut q = (x - v) * (fx - fw);
            let mut p = (x - v) * q - (x - w) * r;
            q = 2.0 * (q - r);
            if q > 0.0 {
                p = -p;
            } else {
                q = -q;
            }
            r = e;
            e = d;
            if p.abs() >= q.abs() * xm.abs() || p <= q * (a - x) || p >= q * (b - x) {
                e = if x >= xm { a - x } else { b - x };
                d = 0.381966 * e;
            } else {
                d = p / q;
                let u = x + d;
                if u - a < 2.0 * tol1 || b - u < 2.0 * tol1 {
                    d = tol1.copysign(xm - x);
                }
            }
        } else {
            e = if x >= xm { a - x } else { b - x };
            d = 0.381966 * e;
        }
        let u = if d.abs() >= tol1 {
            x + d
        } else {
            x + tol1.copysign(d)
        };
        let fu = f(u);
        nfev += 1;
        if fu <= fx {
            if u >= x {
                a = x;
            } else {
                b = x;
            }
            v = w;
            w = x;
            x = u;
            fv = fw;
            fw = fx;
            fx = fu;
        } else {
            if u < x {
                a = u;
            } else {
                b = u;
            }
            if fu <= fw || w == x {
                v = w;
                w = u;
                fv = fw;
                fw = fu;
            } else if fu <= fv || v == x || v == w {
                v = u;
                fv = fu;
            }
        }
    }
    OptimizeResult::fail(vec![x], fx, None, 200, nfev, "maximum iterations exceeded")
}

fn bisection<F>(f: &mut F, mut a: f64, mut b: f64) -> OptimizeResult
where
    F: FnMut(f64) -> f64,
{
    let mut fa = f(a);
    let mut fb = f(b);
    let mut nfev = 2usize;
    if fa * fb > 0.0 {
        return OptimizeResult::fail(vec![a], fa, None, 0, nfev, "bracket must contain root");
    }
    for nit in 0..200 {
        let c = 0.5 * (a + b);
        let fc = f(c);
        nfev += 1;
        if fc.abs() < 1e-12 || (b - a).abs() < 1e-12 {
            return OptimizeResult::ok(vec![c], fc, None, nit + 1, nfev, "converged");
        }
        if fa * fc < 0.0 {
            b = c;
            fb = fc;
        } else {
            a = c;
            fa = fc;
        }
    }
    let c = 0.5 * (a + b);
    OptimizeResult::fail(
        vec![c],
        f(c),
        None,
        200,
        nfev + 1,
        "maximum iterations exceeded",
    )
}

fn brent_root<F>(f: &mut F, mut a: f64, mut b: f64) -> OptimizeResult
where
    F: FnMut(f64) -> f64,
{
    let mut fa = f(a);
    let mut fb = f(b);
    let mut nfev = 2usize;
    if fa * fb > 0.0 {
        return OptimizeResult::fail(vec![a], fa, None, 0, nfev, "bracket must contain root");
    }
    if fa.abs() < fb.abs() {
        std::mem::swap(&mut a, &mut b);
        std::mem::swap(&mut fa, &mut fb);
    }
    let mut c = a;
    let mut fc = fa;
    let mut d = b - a;
    let mut e = d;
    for nit in 0..200 {
        if fb.abs() < 1e-12 || (b - a).abs() < 1e-12 {
            return OptimizeResult::ok(vec![b], fb, None, nit, nfev, "converged");
        }
        if fa.abs() > fc.abs() {
            a = b;
            b = c;
            c = a;
            fa = fb;
            fb = fc;
            fc = fa;
        }
        let tol = 2.0 * f64::EPSILON * b.abs() + 0.5 * 1e-12;
        let m = 0.5 * (c - b);
        if m.abs() <= tol {
            return OptimizeResult::ok(vec![b], fb, None, nit + 1, nfev, "converged");
        }
        if e.abs() >= tol && fa.abs() > fb.abs() {
            let s = fb / fa;
            let (p, q) = if a == c {
                (2.0 * m * s, 1.0 - s)
            } else {
                let q_ = fa / fc;
                let r = fb / fc;
                (
                    s * (2.0 * m * q_ * (q_ - r) - (b - a) * (r - 1.0)),
                    (q_ - 1.0) * (r - 1.0) * (s - 1.0),
                )
            };
            if p > 0.0 {
                // q unchanged
            } else {
                // use bisection
            }
            if 2.0 * p < 3.0 * m * q - (tol * q).abs() && p < (e * q).abs() / 2.0 {
                e = d;
                d = p / q;
            } else {
                d = m;
                e = m;
            }
        } else {
            d = m;
            e = m;
        }
        a = b;
        fa = fb;
        if d.abs() > tol {
            b += d;
        } else {
            b += tol.copysign(m);
        }
        fb = f(b);
        nfev += 1;
        if fb * fc > 0.0 {
            c = a;
            fc = fa;
            d = b - a;
            e = d;
        }
    }
    OptimizeResult::fail(vec![b], fb, None, 200, nfev, "maximum iterations exceeded")
}

fn secant<F>(f: &mut F, mut x0: f64, mut x1: f64) -> OptimizeResult
where
    F: FnMut(f64) -> f64,
{
    let mut f0 = f(x0);
    let mut f1 = f(x1);
    let mut nfev = 2usize;
    for nit in 0..100 {
        if f1.abs() < 1e-12 {
            return OptimizeResult::ok(vec![x1], f1, None, nit, nfev, "converged");
        }
        let dx = x1 - x0;
        if dx.abs() < 1e-12 {
            return OptimizeResult::ok(vec![x1], f1, None, nit, nfev, "converged");
        }
        let x2 = x1 - f1 * dx / (f1 - f0);
        x0 = x1;
        f0 = f1;
        x1 = x2;
        f1 = f(x1);
        nfev += 1;
    }
    OptimizeResult::fail(vec![x1], f1, None, 100, nfev, "maximum iterations exceeded")
}

fn newton_scalar<F, G>(f: &mut F, fp: &mut G, mut x: f64) -> OptimizeResult
where
    F: FnMut(f64) -> f64,
    G: FnMut(f64) -> f64,
{
    let mut nfev = 0usize;
    for nit in 0..100 {
        let fx = f(x);
        nfev += 1;
        if fx.abs() < 1e-12 {
            return OptimizeResult::ok(vec![x], fx, None, nit, nfev, "converged");
        }
        let dfx = fp(x);
        if dfx.abs() < 1e-15 {
            break;
        }
        x -= fx / dfx;
    }
    OptimizeResult::fail(
        vec![x],
        f(x),
        None,
        100,
        nfev + 1,
        "maximum iterations exceeded",
    )
}

fn newton_scalar_fd<F>(f: &mut F, mut x: f64) -> OptimizeResult
where
    F: FnMut(f64) -> f64,
{
    let mut nfev = 0usize;
    for nit in 0..100 {
        let fx = f(x);
        nfev += 1;
        if fx.abs() < 1e-12 {
            return OptimizeResult::ok(vec![x], fx, None, nit, nfev, "converged");
        }
        let h = fd_step(x);
        let dfx = (f(x + h) - f(x - h)) / (2.0 * h);
        nfev += 2;
        if dfx.abs() < 1e-15 {
            break;
        }
        x -= fx / dfx;
    }
    OptimizeResult::fail(
        vec![x],
        f(x),
        None,
        100,
        nfev + 1,
        "maximum iterations exceeded",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brent_root_cos() {
        // cos(x) - x = 0 at ~0.739085
        let f = |x: f64| x.cos() - x;
        let res = root_scalar(
            f,
            (0.0, 1.0),
            RootScalarMethod::Brent,
            None::<fn(f64) -> f64>,
            None,
        );
        assert!(res.success);
        assert!((res.x[0] - 0.7390851332151607).abs() < 1e-8);
    }

    #[test]
    fn bisection_linear() {
        let f = |x: f64| x - 2.0;
        let res = root_scalar(
            f,
            (0.0, 5.0),
            RootScalarMethod::Bisection,
            None::<fn(f64) -> f64>,
            None,
        );
        assert!(res.success);
        assert!((res.x[0] - 2.0).abs() < 1e-10);
    }

    #[test]
    fn minimize_scalar_brent() {
        let f = |x: f64| (x - 2.0).powi(2);
        let res = minimize_scalar(f, (-5.0, 5.0), ScalarMethod::Brent);
        assert!(res.success);
        assert!((res.x[0] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn bad_bounds_scalar() {
        let f = |x: f64| x;
        let res = minimize_scalar(f, (3.0, 1.0), ScalarMethod::Bounded);
        assert!(!res.success);
    }
}
