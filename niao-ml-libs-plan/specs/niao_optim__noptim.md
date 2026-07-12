# Library spec: `noptim`  →  crate `niao_optim`

| | |
|---|---|
| Category | Optimization / scientific |
| Replaces (Python) | `scipy.optimize` |
| Rust reference | `argmin` (numerical optimization in pure Rust) |
| Target Niao crate | `crates/niao_optim` |
| Niao import name | `noptim` |
| Difficulty | 3/5 — Hard |
| Wave | 1 (needs nnum) |
| Depends on Niao libs | `nnum` |
| Error block | 4030–4039 |

## Goal
Function minimization, root finding, least squares, and small LP/QP — the solver layer `nlearn` (logistic
regression, SVM), `nts` (MLE for ARIMA), and general users rely on. **Zero external deps**; vectors/matrices
via `nnum`. Mirror scipy.optimize's `minimize`/`root`/`least_squares` entry points.

## Scope (v1)
- **Unconstrained minimize** (`minimize(f, x0, method, jac?)`):
  - Derivative-free: **Nelder–Mead**, Powell.
  - Gradient-based: steepest descent, nonlinear **CG** (Polak–Ribière), **BFGS**, **L-BFGS** (limited memory).
  - Newton-CG (Hessian-vector via finite differences if no Hessian supplied).
- **Line searches:** backtracking (Armijo), strong-Wolfe; used by CG/BFGS/L-BFGS.
- **Root finding** (`root_scalar`, `root`): bisection, **Brent**, secant, Newton (scalar); Broyden / Newton for systems.
- **Least squares** (`least_squares(resid, x0)`): Gauss–Newton, **Levenberg–Marquardt** (trust-region damping).
- **Scalar minimization:** golden-section, Brent (`minimize_scalar`, `bounded`).
- **Bounds / simple constraints:** box bounds via projection / L-BFGS-B-style active set (v1: projected gradient
  is acceptable; document full L-BFGS-B as v2).
- **Linear programming** (`linprog`): revised simplex for small/medium dense problems (`c, A_ub, b_ub, A_eq, b_eq`).
- **Numerical gradients/Jacobians:** central finite differences when the user gives no analytic derivative.

## Implementation blueprint
- One iteration-driver pattern (init → step → check termination on `|grad|`, `Δf`, `Δx`, max-iter) shared by all
  gradient methods — mirrors argmin's solver/executor split. Return `{x, fun, nit, success, message}`.
- BFGS stores the inverse-Hessian approximation `H` (dense); L-BFGS stores the last `m` (s, y) pairs and does the
  two-loop recursion — **no `n×n` matrix**, so it scales to large `n`.
- LM: solve `(JᵀJ + λ diag) δ = −Jᵀr` via `nnum` (Cholesky/`solve`); adapt λ on accept/reject.
- Finite-difference gradient reuses one buffer; step `h = sqrt(eps)*max(1,|x|)`.
- Termination and failure are explicit: non-convergence → typed error 4033, never a silent bad answer.

### Performance rules
- No allocation inside the inner line-search loop; reuse gradient/step buffers.
- `#[inline]` dot/axpy helpers (or call `nnum`); avoid recomputing `f`/`grad` unnecessarily (cache last eval).

## Public API surface
`minimize(f, x0, method, options)`, `minimize_scalar`, `root_scalar`, `root`, `least_squares`, `linprog`,
`approx_fprime`. Result struct with `x, fun, jac, nit, nfev, success, message`. Expose to Niao via
`niao_libs/noptim/` + builtins; a Niao program passes a callback closure as the objective.

## Performance target
Correctness + convergence within tolerance is the gate. On standard test problems (Rosenbrock, Himmelblau,
Beale, sphere) each applicable method converges to the known optimum within `1e-6`. Perf secondary.

## Tests required
- Rosenbrock/Beale/Himmelblau: BFGS, L-BFGS, CG, Nelder–Mead all reach the known minimum (`x*`, `f*`) within `1e-5`.
- `least_squares` on a nonlinear curve-fit fixture → parameters vs scipy `least_squares`, `rtol=1e-6`.
- Root finding: Brent/bisection/Newton on functions with known roots; Broyden on a 2×2 nonlinear system.
- `linprog` on textbook LPs vs known optima (feasible + one infeasible + one unbounded → typed errors).
- Finite-difference gradient vs analytic gradient on smooth fixtures, `rtol=1e-5`.
- Degenerate: max-iter exceeded → 4033; bad bounds (lo>hi) → 4034; infeasible LP → 4035.
- Plus: in-crate unit tests, `examples/noptim_demo.niao`, `benchmarks/benchmark_noptim.py` vs scipy.optimize.

## Risk / notes
- Line-search correctness (strong-Wolfe) is what makes BFGS/CG reliable — test it directly, not just end-to-end.
- L-BFGS two-loop recursion is easy to get subtly wrong (sign/order) — unit-test the direction against dense BFGS.
- Full L-BFGS-B and interior-point LP are out of scope for v1; document as v2.

## Done criteria
- `cargo check --workspace` and `cargo test -p niao_optim` green; standard problems converge in tolerance.
- Non-convergence and infeasibility return typed errors (no panic, no fake optimum).
- `niao_libs/noptim/` wrapper + `examples/noptim_demo.niao` minimizes Rosenbrock from a Niao closure.
- Benchmark + notes in `REPORT.md`; `CHANGELOG.md` updated; shared-file edits reported, not applied.
