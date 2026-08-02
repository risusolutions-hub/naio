# ntune standard library

Hyperparameter search for Niao ML workflows: exhaustive **grid** search, **random** search, and **successive halving** over a resource budget (`nlearn` / `neval` steps). Includes **train/test split** and **k-fold** index helpers. Scoped port of Python **Optuna** / scikit-learn `GridSearchCV` patterns; pairs with `nlearn` and `neval`.

Objectives run synchronously on the calling thread. Pass the objective as a **top-level function name string** (e.g. `"loss"`) or as a direct function reference when the compiler provides one — Niao does not yet support first-class function values through variables (same model as `net` HTTP handlers).

## Import

```niao
import "ntune"
```

Paths `import "std/ntune"` and `import "ntune"` are equivalent. Flat builtins (`ntune_grid_search`, `ntune_halving`, …) are also available globally after import.

## Quick start

```niao
import "ntune"

// Quadratic loss: minimum at lr=0.1, depth=4
fn loss(params) {
    let lr = params.lr
    let depth = params.depth
    return (lr - 0.1) * (lr - 0.1) + (depth - 4) * (depth - 4)
}

let grid = ntune.grid_search("loss", {
    lr: [0.01, 0.1, 0.5],
    depth: [2, 4, 6]
}, {direction: "minimize"})

print(grid.best.params)   // {lr: 0.1, depth: 4}
print(grid.best.value)    // ~0.0

let random = ntune.random_search("loss", {
    lr: {type: "float", low: 0.001, high: 0.5, log: true},
    depth: {type: "int", low: 2, high: 8}
}, {n_trials: 40, seed: 42})

// Successive halving: objective receives (params, budget)
fn train_with_budget(params, budget) {
    return loss(params) + 1.0 / budget   // more budget -> lower penalty
}

let halved = ntune.halving("train_with_budget", {
    lr: {type: "float", low: 0.01, high: 0.3},
    depth: {type: "int", low: 2, high: 10}
}, {
    n_trials: 27,
    min_resource: 1,
    max_resource: 81,
    reduction_factor: 3
})

let split = ntune.train_test_split(1000, {test_size: 0.2, seed: 7})
let folds = ntune.kfold(100, {n_splits: 5, shuffle: true, seed: 1})
```

## Search space

| Form | Example | Used by |
|------|---------|---------|
| Grid list | `{lr: [0.01, 0.1]}` | `grid_search`, `grid_points`, `grid_size` |
| Float range | `{lr: {type: "float", low: 0.001, high: 0.1, log: true}}` | `random_search`, `sample`, `halving` |
| Int range | `{depth: {type: "int", low: 1, high: 10}}` | same |
| Categorical | `{opt: {type: "categorical", choices: ["a", "b"]}}` | same |

## Core API

| Method | Description |
|--------|-------------|
| `ntune.grid_search(fn, space, opts?)` | Evaluate every grid combination. Returns `{trials, best, n_trials, direction}`. |
| `ntune.random_search(fn, space, opts?)` | Sample `n_trials` configs (default 10). Same result shape. |
| `ntune.halving(fn, space, opts?)` | Successive halving; `fn(params, budget)` receives resource steps. |
| `ntune.grid_size(space)` | Cartesian product size (grid lists only). |
| `ntune.grid_points(space)` | Array of all grid param objects. |
| `ntune.sample(space, n, seed?)` | `n` random param objects. |
| `ntune.validate_space(space)` | Returns `true` or catchable `ntune_error`. |
| `ntune.best(trials, opts?)` | Best trial from an array of trial objects. |
| `ntune.is_better(a, b, opts?)` | Compare scores under `direction`. |

## Data splitting

| Method | Description |
|--------|-------------|
| `ntune.train_test_split(n, opts?)` | `{train: [indices], test: [indices]}`. Default `test_size: 0.2`. |
| `ntune.kfold(n, opts?)` | Array of `{train, test}` index lists. Default `n_splits: 5`. |

## Options

| Key | Default | Meaning |
|-----|---------|---------|
| `direction` | `"minimize"` | `"minimize"` or `"maximize"`. |
| `seed` | `0` | RNG seed for random search / halving / shuffled k-fold. |
| `n_trials` | `10` (random), `27` (halving) | Number of configurations. |
| `min_resource` | `1` | Initial budget per halving bracket. |
| `max_resource` | `81` | Final budget for survivors. |
| `reduction_factor` | `3` | Keep top `1/eta` each round. |

## Trial object

Each trial: `{trial, params, value, budget?, status}` where `status` is `"complete"` or `"pruned"`.

## Errors

| Code | Meaning |
|------|---------|
| 2750 | Wrong argument count. |
| 2751 | Semantic / validation failure (catchable `ntune_error`). |
| 2752 | Wrong argument type (hard error). |
| 2753 | Invalid search space (hard error). |

## See also

- `neval` — metrics and benchmark helpers after tuning.
- `nlearn` — estimators; CV helpers delegate to `ntune` when wired.
