# Task 05 — nlearn: scikit-learn (crate `niao_learn`)
Wave 2 (needs nnum, nframe, nstats). Read `../MASTER_PLAN.md` + `../specs/niao_learn__nlearn.md`. Error block **4050–4059**.
Depends on: `nnum`, `nframe`, `nstats`, `nrand`, `neval`, `noptim`. **Breadth = the risk — ship in layers.**

## Build (`crates/niao_learn`, zero new deps) — build the estimator trait FIRST
- Traits: `Estimator::fit(x,y)`, `Predictor::predict(+predict_proba)`, `Transformer::transform(+fit_transform)`;
  `score`→neval. Not-fitted→4053, shape mismatch→4054, non-convergence→4055.
- **Layer 1:** preprocessing (StandardScaler/MinMax/Robust/Normalizer/OneHot/Ordinal/Label/Polynomial/SimpleImputer/Binarizer),
  LinearRegression(nnum lstsq)/Ridge/Lasso(coord descent)/ElasticNet, LogisticRegression(noptim L-BFGS/IRLS),
  KMeans(k-means++), PCA(nnum SVD), DecisionTree(CART Gini/entropy/MSE).
- **Layer 2:** kNN(brute + KD-tree), GaussianNB/MultinomialNB/BernoulliNB, RandomForest/ExtraTrees(bagging+feature subsample, parallel trees).
- **Layer 3:** SVM(SMO, linear+RBF+poly)/SVR/LinearSVC, GradientBoosting(basic — heavy path is nboost), AdaBoost/Bagging,
  DBSCAN/Agglomerative/GaussianMixture(EM), TruncatedSVD/KernelPCA.
- **Layer 4:** Pipeline/ColumnTransformer/FeatureUnion, KFold/StratifiedKFold, cross_val_score, GridSearchCV/RandomizedSearchCV
  (reuse ntune where possible). Metrics delegate to neval.

## Wire up
- `niao_libs/nlearn/` wrapper + builtins; `docs/NLEARN.md`; `examples/nlearn_demo.niao` (train/predict/score on Iris).

## Acceptance
- Prediction parity vs sklearn fixtures (seeded): LinReg/Ridge coeffs 1e-6; LogReg accuracy within 1%; KMeans labels
  match up to permutation; PCA up to sign; Tree/RF accuracy within 1–2% on Iris/digits. Preprocessing exact 1e-10.
- Pipeline reproduces manual sequence; cross_val_score expected folds. Each layer green before the next.
- `benchmarks/benchmark_nlearn.py` vs sklearn; within 3–5x. `cargo test -p niao_learn` green.

See `../cursor-rules.md`.
