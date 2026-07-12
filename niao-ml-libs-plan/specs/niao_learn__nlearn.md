# Library spec: `nlearn`  →  crate `niao_learn`

| | |
|---|---|
| Category | Classical ML |
| Replaces (Python) | `scikit-learn` |
| Rust reference | `linfa`, `smartcore` (pure-Rust classical ML) |
| Target Niao crate | `crates/niao_learn` |
| Niao import name | `nlearn` |
| Difficulty | 5/5 — Extreme (breadth) |
| Wave | 2 (needs nnum, nframe, nstats) |
| Depends on Niao libs | `nnum`, `nframe`, `nstats`, `nrand`, `neval`, `noptim` |
| Error block | 4050–4059 |

## Goal
The scikit-learn of Niao: a uniform **estimator API** over classical supervised/unsupervised algorithms,
preprocessing, pipelines, and model selection. **Zero external deps** — linear algebra via `nnum`, optimizers
via `noptim`, metrics via `neval`, CV/splits via `ntune`, RNG via `nrand`. Breadth is the challenge; ship
families one at a time behind a stable trait.

## The estimator contract (build this FIRST)
Every model implements the same shape so Pipelines and model_selection compose:
```
trait Estimator { fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> Result<()>; }
trait Predictor { fn predict(&self, x: &NdArray) -> Result<NdArray>; }        // + predict_proba for classifiers
trait Transformer { fn transform(&self, x: &NdArray) -> Result<NdArray>; }    // + fit_transform
```
`score(x, y)` delegates to `neval` (accuracy / R²). "Not fitted" → error 4053; shape mismatch → 4054.

## Scope (v1)
- **Linear models:** LinearRegression (OLS via nnum lstsq), Ridge, Lasso (coordinate descent), ElasticNet,
  LogisticRegression (binary + multinomial, via noptim L-BFGS / IRLS), SGDClassifier/Regressor.
- **Neighbors:** KNeighborsClassifier/Regressor (brute-force + a KD-tree for low dim).
- **Naive Bayes:** GaussianNB, MultinomialNB, BernoulliNB.
- **SVM:** SVC / SVR via SMO (linear + RBF/poly kernels); LinearSVC (hinge loss via noptim).
- **Trees:** DecisionTreeClassifier/Regressor (CART, Gini/entropy/MSE, max_depth/min_samples).
- **Ensembles:** RandomForest{Classifier,Regressor} (bagging + feature subsampling), ExtraTrees,
  GradientBoosting (basic — heavy boosting lives in `nboost`), Bagging, AdaBoost.
- **Clustering:** KMeans (k-means++ init), MiniBatchKMeans, DBSCAN, AgglomerativeClustering, GaussianMixture (EM).
- **Decomposition / manifold:** PCA (via nnum SVD), TruncatedSVD, KernelPCA, optional t-SNE (v2).
- **Preprocessing:** StandardScaler, MinMaxScaler, RobustScaler, Normalizer, OneHotEncoder, OrdinalEncoder,
  LabelEncoder, PolynomialFeatures, SimpleImputer, Binarizer.
- **Pipeline / compose:** `Pipeline([(name, step)...])`, `ColumnTransformer`, `FeatureUnion`.
- **Model selection:** `train_test_split` (→ ntune), `KFold`/`StratifiedKFold`, `cross_val_score`,
  `GridSearchCV`, `RandomizedSearchCV` (→ reuse ntune search where possible).
- **Metrics:** delegate to `neval` (accuracy, precision, recall, F1, confusion, ROC-AUC, MSE, MAE, R²).

## Implementation blueprint (make it FAST + LIGHT)
- **One trait, many models** — no `dyn` in inner loops; dispatch once. Estimators own compact fitted state.
- Trees: CART with sorted-feature split search; histogram binning optional (nboost owns the fast path). Reuse a
  single index/threshold scan; pre-sort features once. Random forest parallelizes trees (std threads, bounded pool).
- Linear/logistic: standardize → solve. Ridge closed-form via nnum; Lasso/ElasticNet coordinate descent with
  warm starts; Logistic via noptim L-BFGS with analytic gradient.
- KMeans: k-means++ seeding, Lloyd iterations, `nnum` for distances (batched), empty-cluster reseed.
- PCA: center → nnum SVD → components/explained_variance; sign convention fixed for deterministic tests.
- SVM SMO: working-set of 2, cached kernel rows, shrinking optional (v2).

### Performance rules
- No per-sample allocation; operate on `nnum` matrices in batches. Pre-sort tree features once, reuse buffers.
- `#[inline]` distance/kernel kernels; SIMD where nnum exposes it; parallelize forests/CV across threads.

## Public API surface
The estimator families above, each with `fit/predict[/predict_proba]/transform/score`, plus `Pipeline`,
`ColumnTransformer`, `KFold`, `cross_val_score`, `GridSearchCV`. Expose to Niao via `niao_libs/nlearn/` + builtins;
Niao surface mirrors sklearn: `m = nlearn.LogisticRegression(); m.fit(x, y); m.predict(xt)`.

## Performance target
- Predictions match scikit-learn within tolerance on fixtures (see below).
- Wall-clock within **3–5×** of scikit-learn on the benchmark datasets (Iris, digits, a 100k×20 synthetic set).

## Tests required
- **Prediction parity** vs sklearn fixtures (seeded): LinearRegression/Ridge coefficients `rtol=1e-6`; LogisticReg
  accuracy within 1% and coefficients close; KMeans labels match up to permutation given the same seed+init;
  PCA components match up to sign; DecisionTree/RandomForest accuracy within 1–2% on Iris/digits.
- Preprocessing transforms exact vs sklearn (StandardScaler/MinMax/OneHot) `rtol=1e-10`.
- Pipeline: `fit_transform` then estimator reproduces the manual sequence.
- `cross_val_score` returns the expected fold scores on a seeded split.
- Degenerate: predict before fit → 4053; X/y row mismatch → 4054; non-convergence → 4055.
- Plus: in-crate unit tests, `examples/nlearn_demo.niao` (train/predict/score on Iris), `benchmarks/benchmark_nlearn.py` vs sklearn.

## Risk / notes
- **Scope discipline is the whole risk.** Ship in this order and gate the rest: (1) estimator trait + preprocessing
  + LinearRegression/LogisticRegression/KMeans/PCA/DecisionTree, (2) RandomForest/kNN/NB, (3) SVM/ensembles,
  (4) Pipeline+GridSearch. Each layer must be green before the next.
- Don't duplicate `neval`/`ntune` — call them for metrics and CV.
- t-SNE, kernel approximations, calibration, and multi-output are explicit v2.
- Determinism: fix seeds and sign conventions so parity tests aren't flaky.

## Done criteria
- `cargo check --workspace` and `cargo test -p niao_learn` green; sklearn parity fixtures pass in tolerance.
- The estimator trait is stable and shared; Pipeline + one GridSearchCV run end-to-end.
- `niao_libs/nlearn/` wrapper + `examples/nlearn_demo.niao` trains and scores a model.
- Benchmark + notes in `REPORT.md`; `CHANGELOG.md` updated; shared-file edits reported, not applied.
