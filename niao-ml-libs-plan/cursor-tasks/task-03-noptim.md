# Task 03 — noptim: scipy.optimize (crate `niao_optim`)
Wave 1 (needs nnum). Read `../MASTER_PLAN.md` + `../specs/niao_optim__noptim.md`. Error block **4030–4039**.
Depends on: `nnum`.

## Build (`crates/niao_optim`, zero new deps)
- Shared iteration driver (init→step→terminate on |grad|/Δf/Δx/max-iter) → result {x, fun, jac, nit, nfev, success, message}.
- minimize(f, x0, method, jac?): Nelder–Mead, Powell, steepest descent, CG(Polak–Ribière), BFGS, L-BFGS(two-loop),
  Newton-CG. Line searches: backtracking(Armijo), strong-Wolfe.
- root_scalar/root: bisection, Brent, secant, Newton; Broyden/Newton for systems.
- least_squares: Gauss–Newton + Levenberg–Marquardt (solve via nnum). minimize_scalar: golden/Brent/bounded.
- Box bounds via projected gradient (full L-BFGS-B = v2). linprog: revised simplex (c, A_ub, b_ub, A_eq, b_eq).
- approx_fprime: central finite differences (reuse buffers; no analytic derivative required).

## Wire up
- `niao_libs/noptim/` wrapper + builtins; `docs/NOPTIM.md`; `examples/noptim_demo.niao` (minimize Rosenbrock from a Niao closure).

## Acceptance
- Rosenbrock/Beale/Himmelblau: BFGS/L-BFGS/CG/Nelder–Mead reach known min within 1e-5; least_squares curve-fit vs scipy 1e-6;
  root finders on known roots; linprog vs known optima (+ infeasible/unbounded → typed errors); FD grad vs analytic 1e-5.
- max-iter→4033, bad bounds→4034, infeasible LP→4035. Unit-test strong-Wolfe + L-BFGS direction vs dense BFGS directly.
- `benchmarks/benchmark_noptim.py` vs scipy.optimize. `cargo test -p niao_optim` green.

See `../cursor-rules.md`.
