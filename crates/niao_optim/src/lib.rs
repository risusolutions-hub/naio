//! noptim — scipy.optimize for Niao (function minimization, root finding, least squares, LP).

pub mod error;
pub mod leastsq;
pub mod linprog;
pub mod linesearch;
pub mod minimize;
pub mod result;
pub mod root;
pub mod scalar;
pub mod test_problems;
pub mod utils;

pub use error::{
    OptimError, OptimResult, E4030_NOPTIM_ARITY, E4031_NOPTIM_ERROR, E4032_NOPTIM_TYPE,
    E4033_NOPTIM_NON_CONVERGENCE, E4034_NOPTIM_BAD_BOUNDS, E4035_NOPTIM_INFEASIBLE,
    E4036_NOPTIM_UNBOUNDED,
};
pub use leastsq::least_squares;
pub use linprog::linprog;
pub use minimize::minimize;
pub use result::{
    LeastSquaresMethod, LeastSquaresOptions, LinprogProblem, LinprogResult, MinimizeMethod,
    MinimizeOptions, OptimizeResult, RootMethod, RootScalarMethod, ScalarMethod,
};
pub use root::root;
pub use scalar::{minimize_scalar, root_scalar};
pub use utils::approx_fprime;

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::test_problems::{beale, beale_grad, rosenbrock, rosenbrock_grad, sphere, sphere_grad};

    #[test]
    fn fd_grad_vs_analytic() {
        let x = vec![1.5, -0.5];
        let mut fd = vec![0.0; 2];
        approx_fprime(rosenbrock, &x, &mut fd);
        let mut ana = vec![0.0; 2];
        rosenbrock_grad(&x, &mut ana);
        for i in 0..2 {
            assert!(
                (fd[i] - ana[i]).abs() < 1e-5 * ana[i].abs().max(1.0),
                "i={i} fd={} ana={}",
                fd[i],
                ana[i]
            );
        }
    }

    #[test]
    fn fd_grad_sphere() {
        let x = vec![1.0, 2.0, 3.0];
        let mut fd = vec![0.0; 3];
        approx_fprime(sphere, &x, &mut fd);
        let mut ana = vec![0.0; 3];
        sphere_grad(&x, &mut ana);
        for i in 0..3 {
            assert!((fd[i] - ana[i]).abs() < 1e-5);
        }
    }

    #[test]
    fn beale_all_methods() {
        for method in [
            MinimizeMethod::Bfgs,
            MinimizeMethod::LBfgs,
            MinimizeMethod::CG,
            MinimizeMethod::NelderMead,
        ] {
            let res = minimize(
                beale,
                &[1.0, 1.0],
                method,
                Some(beale_grad),
                MinimizeOptions {
                    max_iter: if method == MinimizeMethod::NelderMead {
                        1000
                    } else {
                        5000
                    },
                    gtol: 1e-6,
                    ..Default::default()
                },
            );
            assert!((res.x[0] - 3.0).abs() < 1.2, "{method:?}: x={:?} fun={}", res.x, res.fun);
            assert!((res.x[1] - 0.5).abs() < 0.8, "{method:?}: x={:?} fun={}", res.x, res.fun);
        }
    }
}
