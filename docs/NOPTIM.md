# NOPTIM — scipy.optimize for Niao

`noptim` replaces a subset of **scipy.optimize** with a std-only native library
(`crates/niao_optim`). Depends on `nnum` for linear solves.

Import:

```niao
import "noptim"
```

## minimize

Unconstrained (or box-bounded) minimization of a scalar objective.

| Function | Description |
|----------|-------------|
| `noptim.minimize(f, x0, method)` | Minimize `f(x)` from initial guess `x0` |
| `noptim.minimize(f, x0, method, jac)` | Same with analytic gradient callback |

Methods: `"bfgs"`, `"l-bfgs"`, `"cg"`, `"nelder-mead"`, `"powell"`, `"sd"`, `"newton-cg"`.

Returns `{x, fun, nit, nfev, success, message}`.

## Scalar & root finding

| Function | Description |
|----------|-------------|
| `noptim.minimize_scalar(f, a, b, method)` | 1-D minimum on `[a,b]` (`"brent"`, `"golden"`) |
| `noptim.root_scalar(f, a, b, method)` | Scalar root on bracket (`"brent"`, `"bisection"`, `"secant"`, `"newton"`) |
| `noptim.root(f, x0, method)` | Nonlinear system (`"hybr"`, `"broyden"`) |

## least_squares

Nonlinear least squares with Gauss–Newton or Levenberg–Marquardt.

```niao
let res = noptim.least_squares(resid, x0, "lm")
```

`resid(params, out)` writes residual vector into `out`.

## linprog

Small dense linear programs via revised simplex:

```niao
let res = noptim.linprog(c, a_ub, b_ub)
```

## approx_fprime

Central-difference gradient when no analytic `jac` is supplied.

## Error codes (4030–4039)

| Code | Meaning |
|------|---------|
| 4030 | arity |
| 4031 | general error |
| 4032 | type mismatch |
| 4033 | non-convergence (max iterations) |
| 4034 | bad bounds (`lo > hi`) |
| 4035 | infeasible LP |
| 4036 | unbounded LP |

## v1 limitations

- Box bounds: projected gradient after line search (full L-BFGS-B deferred to v2)
- BFGS uses forward Hessian + `nnum` solve (stable; not identical to scipy's L-BFGS-B path)
- Interior-point LP and trust-region reflective least squares deferred to v2
