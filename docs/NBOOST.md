# NBOOST — Histogram GBDT for Niao

`nboost` replaces core **XGBoost / LightGBM** tabular boosting with a std-only
native library (`crates/niao_boost`). Depends on `nnum` and `nframe`.

Import:

```niao
import "nboost"
```

## Estimators

| Class | Task |
|-------|------|
| `nboost.gb_regressor(params?)` | Regression (squared error) |
| `nboost.gb_classifier(params?, num_class?)` | Binary / multiclass (logistic / softmax) |

Same contract as `nlearn`: `fit(x, y)`, `predict(x)`, `predict_proba(x)` (classifiers),
`score(x, y)`, `feature_importance(kind)`.

## Key parameters

| Param | Default | Description |
|-------|---------|-------------|
| `learning_rate` | 0.1 | Shrinkage (eta) |
| `n_estimators` | 100 | Boosting rounds |
| `max_depth` | 6 | Max tree depth |
| `max_leaves` | 31 | Leaf-wise cap |
| `max_bins` | 256 | Histogram bins per feature |
| `lambda_l2` | 1.0 | L2 on leaf weights |
| `gamma` | 0.0 | Min split gain |
| `min_data_in_leaf` | 20 | Min rows per leaf |
| `subsample` | 1.0 | Row subsample |
| `colsample` | 1.0 | Feature subsample |
| `early_stopping_rounds` | — | Stop when val metric stalls |

## Objectives

- Regression: squared error (L2)
- Binary: logistic loss
- Multiclass: softmax (one tree per class per round)

## Missing values

NaN features learn a default left/right direction per split (XGBoost-style sparsity).

## Model I/O

```niao
nboost.save_model(model, "model.json")
let loaded = nboost.load_model("model.json")
```

JSON format (stdlib serializer; runtime may wrap with `njson` when wired).

## Errors (4060–4069)

| Code | Meaning |
|------|---------|
| 4063 | Not fitted |
| 4064 | Bad parameter |
| 4065 | Shape mismatch |

## Performance

Histogram binning (u8 codes, column-major) is the default training path.
Target: train within 3–5× LightGBM on 100k×50 / 100 rounds (see `benchmarks/benchmark_nboost.py`).
