//! Standard optimization test problems (scipy.optimize fixtures).

pub fn rosenbrock(x: &[f64], _buf: &mut [f64]) -> f64 {
    let a = 1.0 - x[0];
    let b = x[1] - x[0] * x[0];
    a * a + 100.0 * b * b
}

pub fn rosenbrock_grad(x: &[f64], g: &mut [f64]) {
    g[0] = -2.0 * (1.0 - x[0]) - 400.0 * x[0] * (x[1] - x[0] * x[0]);
    g[1] = 200.0 * (x[1] - x[0] * x[0]);
}

pub fn beale(x: &[f64], _buf: &mut [f64]) -> f64 {
    let t1 = 1.5 - x[0] + x[0] * x[1];
    let t2 = 2.25 - x[0] + x[0] * x[1] * x[1];
    let t3 = 2.625 - x[0] + x[0] * x[1] * x[1] * x[1];
    t1 * t1 + t2 * t2 + t3 * t3
}

pub fn beale_grad(x: &[f64], g: &mut [f64]) {
    let t1 = 1.5 - x[0] + x[0] * x[1];
    let t2 = 2.25 - x[0] + x[0] * x[1] * x[1];
    let t3 = 2.625 - x[0] + x[0] * x[1] * x[1] * x[1];
    g[0] = 2.0 * t1 * (-1.0 + x[1])
        + 2.0 * t2 * (-1.0 + x[1] * x[1])
        + 2.0 * t3 * (-1.0 + x[1] * x[1] * x[1]);
    g[1] = 2.0 * t1 * x[0] + 2.0 * t2 * (2.0 * x[0] * x[1]) + 2.0 * t3 * (3.0 * x[0] * x[1] * x[1]);
}

pub fn himmelblau(x: &[f64], _buf: &mut [f64]) -> f64 {
    let a = x[0] * x[0] + x[1] - 11.0;
    let b = x[0] + x[1] * x[1] - 7.0;
    a * a + b * b
}

pub fn sphere(x: &[f64], _buf: &mut [f64]) -> f64 {
    x.iter().map(|v| v * v).sum()
}

pub fn sphere_grad(x: &[f64], g: &mut [f64]) {
    for i in 0..x.len() {
        g[i] = 2.0 * x[i];
    }
}

/// Nonlinear curve-fit: y = a * exp(b * x), scipy curve_fit fixture params.
pub fn exp_model(params: &[f64], x: f64) -> f64 {
    params[0] * (params[1] * x).exp()
}

pub fn exp_residuals(params: &[f64], xs: &[f64], ys: &[f64], out: &mut [f64]) {
    for (i, &xi) in xs.iter().enumerate() {
        out[i] = exp_model(params, xi) - ys[i];
    }
}

/// scipy reference for exp fit: a≈2.5, b≈1.0 on synthetic data.
pub fn exp_fit_fixture() -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let xs: Vec<f64> = (0..10).map(|i| i as f64 * 0.5).collect();
    let ys: Vec<f64> = xs.iter().map(|&x| 2.5 * (1.0 * x).exp()).collect();
    (xs, ys, vec![2.0, 0.9])
}

pub fn exp_jacobian(params: &[f64], xs: &[f64], jac: &mut [f64]) {
    let a = params[0];
    let b = params[1];
    for (i, &x) in xs.iter().enumerate() {
        let e = (b * x).exp();
        jac[i * 2 + 0] = e;
        jac[i * 2 + 1] = a * x * e;
    }
}

pub fn lp_feasible() -> (Vec<f64>, Vec<Vec<f64>>, Vec<f64>, Vec<f64>) {
    // min -x1 - x2 s.t. x1 + x2 <= 1, x1,x2 >= 0  => x*=(1,0), f=-1
    let c = vec![-1.0, -1.0];
    let a_ub = vec![vec![1.0, 1.0]];
    let b_ub = vec![1.0];
    let bounds_lo = vec![0.0, 0.0];
    (c, a_ub, b_ub, bounds_lo)
}

pub fn lp_infeasible() -> (Vec<f64>, Vec<Vec<f64>>, Vec<f64>) {
    let c = vec![1.0, 1.0];
    let a_ub = vec![vec![1.0, 1.0], vec![-1.0, -1.0]];
    let b_ub = vec![1.0, -2.0];
    (c, a_ub, b_ub)
}
