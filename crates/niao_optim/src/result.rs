//! Optimization result and option types.

use crate::error::{OptimError, OptimResult};

#[derive(Debug, Clone)]
pub struct OptimizeResult {
    pub x: Vec<f64>,
    pub fun: f64,
    pub jac: Option<Vec<f64>>,
    pub nit: usize,
    pub nfev: usize,
    pub success: bool,
    pub message: String,
}

impl OptimizeResult {
    pub fn ok(
        x: Vec<f64>,
        fun: f64,
        jac: Option<Vec<f64>>,
        nit: usize,
        nfev: usize,
        message: impl Into<String>,
    ) -> Self {
        Self {
            x,
            fun,
            jac,
            nit,
            nfev,
            success: true,
            message: message.into(),
        }
    }

    pub fn fail(
        x: Vec<f64>,
        fun: f64,
        jac: Option<Vec<f64>>,
        nit: usize,
        nfev: usize,
        message: impl Into<String>,
    ) -> Self {
        Self {
            x,
            fun,
            jac,
            nit,
            nfev,
            success: false,
            message: message.into(),
        }
    }

    pub fn into_result(self) -> OptimResult<Self> {
        if self.success {
            Ok(self)
        } else {
            Err(OptimError::NonConvergence(self.message))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinimizeMethod {
    NelderMead,
    Powell,
    SteepestDescent,
    CG,
    Bfgs,
    LBfgs,
    NewtonCG,
}

impl MinimizeMethod {
    pub fn parse(s: &str) -> OptimResult<Self> {
        match s.to_ascii_lowercase().as_str() {
            "nelder-mead" | "nelder_mead" | "nm" => Ok(Self::NelderMead),
            "powell" => Ok(Self::Powell),
            "sd" | "steepest-descent" | "steepest_descent" => Ok(Self::SteepestDescent),
            "cg" | "conjugate-gradient" => Ok(Self::CG),
            "bfgs" => Ok(Self::Bfgs),
            "l-bfgs" | "lbfgs" | "l_bfgs" => Ok(Self::LBfgs),
            "newton-cg" | "newton_cg" => Ok(Self::NewtonCG),
            _ => Err(OptimError::Error(format!("unknown minimize method: {s}"))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MinimizeOptions {
    pub max_iter: usize,
    pub ftol: f64,
    pub gtol: f64,
    pub xtol: f64,
    pub lbfgs_m: usize,
    pub bounds_lo: Option<Vec<f64>>,
    pub bounds_hi: Option<Vec<f64>>,
}

impl Default for MinimizeOptions {
    fn default() -> Self {
        Self {
            max_iter: 200,
            ftol: 1e-8,
            gtol: 1e-5,
            xtol: 1e-8,
            lbfgs_m: 10,
            bounds_lo: None,
            bounds_hi: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarMethod {
    Golden,
    Brent,
    Bounded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootScalarMethod {
    Bisection,
    Brent,
    Secant,
    Newton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootMethod {
    Hybr,
    Broyden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeastSquaresMethod {
    GaussNewton,
    LevenbergMarquardt,
}

#[derive(Debug, Clone)]
pub struct LeastSquaresOptions {
    pub max_iter: usize,
    pub ftol: f64,
    pub xtol: f64,
    pub gtol: f64,
    pub lambda_init: f64,
    pub n_resid: usize,
}

impl Default for LeastSquaresOptions {
    fn default() -> Self {
        Self {
            max_iter: 100,
            ftol: 1e-8,
            xtol: 1e-8,
            gtol: 1e-8,
            lambda_init: 1e-2,
            n_resid: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LinprogProblem {
    pub c: Vec<f64>,
    pub a_ub: Option<Vec<f64>>,
    pub b_ub: Option<Vec<f64>>,
    pub a_eq: Option<Vec<f64>>,
    pub b_eq: Option<Vec<f64>>,
    pub n_vars: usize,
    pub n_ub: usize,
    pub n_eq: usize,
}

#[derive(Debug, Clone)]
pub struct LinprogResult {
    pub x: Vec<f64>,
    pub fun: f64,
    pub success: bool,
    pub message: String,
    pub nit: usize,
}
