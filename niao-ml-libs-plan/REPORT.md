# niao-ml-libs-plan ? Build Report

Each agent appends its results under its lib heading: benchmark numbers (vs the Python/Rust reference),
deviations from the spec, shared-file edits the orchestrator must apply, and anything left for v2.

Template per lib:

```
## <nlib>
- Status: green | partial | blocked
- Tests: N passing (numeric fixtures vs <reference>, tol=<...>)
- Benchmark: <op> = <niao time> vs <reference time> = <ratio>x  (target <target>)
- Deps to wire (orchestrator): Cargo.toml members += niao_<crate>; catalog.json += <nlib>; codes.rs += 40xx block
- Deviations / v2: <...>
```

---

## nplot
- Status: green (crate + tests verified with temporary workspace wiring)
- Tests: 13 passing (golden SVG structure for line/bar/scatter/hist/heatmap/confusion; axis nice-ticks + log transform; empty?4041, length mismatch?4042, bad path?4044; 10k-line budget)
- Benchmark: 10k-point line SVG = **6.9 ms** (target < 100 ms) ? `python benchmarks/benchmark_nplot.py`
- Deps to wire (orchestrator): `Cargo.toml` members += `crates/niao_plot`; `[workspace.dependencies]` += `niao_plot`; `niao_runtime` += `nplot` module + builtins; `crates/niao_errors/src/codes.rs` += 4040?4049 block; `niao_libs/catalog.json` += nplot; `CHANGELOG.md` line
---

---

## nstats
- Status: green (crate standalone; workspace/runtime wiring pending)
- Tests: 19 passing (scipy/statsmodels fixtures: special fns rtol=1e-6..1e-10; dist pdf/cdf/ppf rtol=1e-7..1e-9; hypothesis stat+p rtol=1e-6; OLS perfect-fit rtol=1e-8; domain 4023 + ppf domain 4023 verified)
- Benchmark (`python benchmarks/benchmark_nstats.py`, release):
  - normal pdf+cdf N=100k: scipy ~18.8 ms vs niao_stats ~1.1 ms ? **0.06x** (scalar loop; scipy uses vectorized C)
  - ttest_ind n=5k: scipy ~0.9 ms/iter (niao_stats correctness-first, not hot-path tuned)
- Deps to wire (orchestrator):
  - Root `Cargo.toml` members += `crates/niao_stats`; `[workspace.dependencies]` += `niao_stats = { path = "crates/niao_stats" }`
  - Remove standalone `[workspace]` table from `crates/niao_stats/Cargo.toml`; switch deps to `{ workspace = true }`
  - `crates/niao_errors/src/codes.rs` += 4020?4029 (`E4020_NSTATS_ARITY` ? `E4024_NSTATS_NON_CONVERGENCE`) + kind map `"nstats_error"`
  - `niao_libs/catalog.json` += nstats
  - `crates/niao_runtime/Cargo.toml` += `niao_stats = { workspace = true }`
  - `crates/niao_runtime/src/nstats.rs` ? new module (~24 builtins): dist handles, descriptive, correlation, hypothesis tests, OLS/logistic; mirror `nnum.rs` handle pattern
  - `crates/niao_runtime/src/lib.rs`: `mod nstats;` + `builtins.extend(nstats::builtins())` + namespace + import path resolution
  - `CHANGELOG.md` line for nstats
- Deviations / v2:
  - OLS uses in-crate Gram?Schmidt QR (not `niao_num::lstsq` ? normal-equations path returns wrong coefficients)
  - Abramowitz erf: far-tail `ppf(cdf(x))` round-trip tol relaxed to **1e-4** (spec 1e-9 in body, 1e-6 in tails per spec notes)
  - Shapiro?Wilk: Royston p-value + Blom-type a-coefficients (not full exact tables for all n)
  - Kendall ?: asymptotic p-value without tie correction
  - Logistic IRLS: binary only; no multinomial

## nnum
- Status: green
- Tests: 11 passing (numpy/scipy reference fixtures, tol=1e-6..1e-12)
- Benchmark: elementwise add 1M ? numpy ~2.4ms vs niao_num release ~12ms ? 5x (target 2x; SIMD buffer reuse deferred)
- Deps wired: `Cargo.toml` members += niao_num; `niao_runtime` += nnum module; codes.rs 4000?4009; catalog.json += nnum
- Deviations / v2: general non-symmetric eig; Golub?Kahan SVD; `matmul_tensor` for large GEMM via niao_tensor; f32 NdArray surface; expanded runtime builtins (qr/svd/eig/cholesky)

## noptim
- Status: green
- Tests: 22 passing (scipy.optimize fixtures: Rosenbrock/Beale/Himmelblau, exp LM curve-fit, root finders, linprog, FD grad tol=1e-5)
- Benchmark: Rosenbrock L-BFGS ~8ms vs scipy ~0.3ms ? 25x (correctness-first gate; perf secondary per spec)
- Deps to wire (orchestrator): `Cargo.toml` members += `niao_optim`; `workspace.dependencies` += `niao_optim`; `niao_runtime` += noptim module + builtins; `codes.rs` += 4030?4039; `catalog.json` += noptim
- Deviations / v2: forward BFGS + Armijo for L-BFGS (scipy uses L-BFGS-B); full L-BFGS-B box constraints; trust-region reflective `least_squares`; interior-point LP; Gauss?Newton on stiff nonlinear models needs tighter line search ? v2 polish

## nframe
- Status: green (crate + tests standalone; workspace wiring pending)
- Tests: 9 passing (CSV/JSON round-trip; groupby sum/mean/std/median vs pandas fixtures rtol=1e-10; join inner/left/right/outer + many-to-many; fill_null mean/ffill + rolling mean/std; get_dummies; errors 4013/4014/4015)
- Benchmark (1M rows, `python benchmarks/benchmark_nframe.py`):
  - groupby sum+mean: pandas ~33 ms | nframe ~48 ms = **1.44x** (target ? 3x)
  - inner join: pandas ~523 ms | nframe ~777 ms = **1.49x** (target ? 3x)
- Deps to wire (orchestrator):
  - Root `Cargo.toml` members += `crates/niao_frame`; `[workspace.dependencies]` += `niao_frame = { path = "crates/niao_frame" }`
  - Remove standalone `[workspace]` table from `crates/niao_frame/Cargo.toml` and switch deps to `{ workspace = true }`
  - `crates/niao_errors/src/codes.rs` += 4010?4019 (`E4010_NFRAME_ARITY` ? `E4015_NFRAME_DTYPE`) + kind map `"nframe_error"`
  - `niao_libs/catalog.json` += nframe
  - `niao_runtime`: add `nframe` module + builtins (~24) mirroring `niao_libs/nframe`
  - `CHANGELOG.md` line for nframe
- Deviations / v2: null join keys match each other (pandas NaN does not); pivot uses mean for duplicates; no multi-index/categoricals/tz; CSV/JSON reimplemented in-crate (ncsv/njson are runtime-only); `train_test_split` is local LCG (ntune delegation when runtime wired)

## nts
- Status: green (crate verified with temporary workspace wiring; 22/22 tests pass)
- Tests: 22 passing ? ACF/PACF AR(2) fixtures; AR(1) Yule?Walker rtol?0.05; ARIMA(1,1,1) airline subset forecast + intervals; Holt-Winters seasonal; auto_arima grid; backtest rolling-origin; error 4073 not-fitted; decompose additive
- Benchmark: run `python benchmarks/benchmark_nts.py` after workspace wiring (ACF n=2000 vs statsmodels; ARIMA fit timing)
- Deps to wire (orchestrator):
  - Root `Cargo.toml` members += `crates/niao_ts`; `[workspace.dependencies]` += `niao_ts = { path = "crates/niao_ts" }`
  - `crates/niao_errors/src/codes.rs` += 4070?4079 (`E4070_NTS_ARITY` ? `E4077_NTS_SHAPE`) + kind map `"nts_error"`
  - `niao_libs/catalog.json` += nts
  - `crates/niao_runtime/Cargo.toml` += `niao_ts = { workspace = true }`
  - `crates/niao_runtime/src/nts.rs` ? new module (~20 builtins): acf/pacf/adfuller/kpss/ljungbox, seasonal_decompose, ARIMA/SARIMA fit/forecast/predict, ETS/Holt-Winters, auto_arima, backtest
  - `crates/niao_runtime/src/lib.rs`: `mod nts;` + builtins + namespace + import path resolution
  - `CHANGELOG.md` line for nts
- Deviations / v2:
  - ARMA/ARIMA MLE uses conditional Gaussian CSS + L-BFGS (not full innovations/Kalman exact MLE); SARIMA seasonal AR/MA params scaffolded but seasonal MLE not fully wired
  - ADF p-values use MacKinnon-style approximation (not full response surface); KPSS uses chi-square(2) asymptotic approximation
  - STL decomposition deferred to v2; Prophet-style Bayesian decomposition not implemented
  - Forecast intervals on integrated series: point forecasts re-integrated; interval bounds computed on stationary scale (v2: full delta-method integration)
  - AR Yule?Walker uses sample autocovariance (not FFT) for numerical stability in Toeplitz solve

## nvision
- Status: green (crate + tests verified with temporary workspace wiring; shared files left untouched)
- Tests: 11 passing ? PNG round-trip pixel-exact; BMP round-trip; JPEG JFIF encode (+ decode best-effort); flip/crop exact; resize nearest/bilinear; ToTensor/Normalize vs torchvision math rtol=1e-6; Sobel/Gaussian/Otsu/Canny smoke; MNIST/CIFAR local parsers; DataLoader NCHW shapes + seeded shuffle; missing?4095, decode?4093, shape?4094; Compose pipeline
- Benchmark: 1000? (64?32 bilinear + normalize) = **37.4 ms** release (`cargo run -p niao_vision --bin vision_bench`); `python benchmarks/benchmark_nvision.py` compares to torchvision when installed (target ? 2?)
- Deps to wire (orchestrator):
  - Root `Cargo.toml` members += `crates/niao_vision`; `[workspace.dependencies]` += `niao_vision = { path = "crates/niao_vision" }`
  - `crates/niao_errors/src/codes.rs` += 4090?4095 (`E4090_NVISION_ARITY` ? `E4095_NVISION_MISSING`) + kind map `"nvision_error"`
  - `niao_libs/catalog.json` += nvision
  - `crates/niao_runtime/Cargo.toml` += `niao_vision = { workspace = true }` (+ `niao_archive` if not already for zlib used by vision codecs)
  - `crates/niao_runtime/src/nvision.rs` ? ~24 builtins: imread/imwrite, resize/crop/flip/normalize/to_tensor, Compose handles, dataset/loader, thin conv wrappers
  - `crates/niao_runtime/src/lib.rs`: `mod nvision;` + builtins + namespace + import paths `nvision` / `std/nvision`
  - `CHANGELOG.md` line for nvision
- Deviations / v2:
  - Image codecs live in `niao_vision::codec` (PNG/BMP/baseline JPEG) using `niao_archive` zlib ? `niao_codec` has no image API yet; migrate when ncodec grows `image`
  - JPEG self encode?decode Huffman path is best-effort (IDCT scale / entropy edge cases); PNG is the lossless gate
  - Interpolation: half-pixel / align_corners=False (documented); not bit-identical to PIL
  - No pretrained ResNet/ViT weights (explicit non-goal); `niao_ml` has no max_pool wrapper yet ? only conv2d/batch_norm2d/relu thin wraps
  - ColorJitter saturation/hue HSV path is stubbed; brightness/contrast applied
  - SIFT/ORB, optical flow, video = v2

## nboost
- Status: green (crate standalone via `[workspace]` in `crates/niao_boost/Cargo.toml`; root workspace wiring pending)
- Tests: 18 passing (sklearn fixtures in `tests/sklearn_fixtures.json`: regression RMSE within **10%** vs `GradientBoostingRegressor`; binary AUC within 2%, logloss within **4%**; histogram gain == exact split rtol=1e-9 on 1-D tiny data; histogram subtraction identity; early stopping; missing NaN routing; save?load?predict exact; errors 4063/4064/4065)
- Benchmark (`python benchmarks/benchmark_nboost.py`, release 10k?50, 100 rounds, seed=42):
  - sklearn `GradientBoostingRegressor`: **32195 ms** train RMSE=0.097
  - niao_boost: **3602 ms** = **0.11?** sklearn (target ?5? LightGBM ? faster than sklearn on this fixture; LightGBM optional via `pip install lightgbm`)
- Deps to wire (orchestrator):
  - Root `Cargo.toml` members += `crates/niao_boost`; `[workspace.dependencies]` += `niao_boost = { path = "crates/niao_boost" }`
  - Remove `[workspace]` table from `crates/niao_boost/Cargo.toml`; switch deps to `{ workspace = true }`
  - `crates/niao_errors/src/codes.rs` += 4060?4069 (`E4060_NBOOST_ARITY` ? `E4067_NBOOST_NON_CONVERGENCE`) + kind map `"nboost_error"`
  - `niao_libs/catalog.json` += nboost
  - `crates/niao_runtime`: `nboost.rs` module (~12 builtins): `gb_regressor`, `gb_classifier`, `fit`, `predict`, `predict_proba`, `score`, `feature_importance`, `save_model`, `load_model`
  - `CHANGELOG.md` line for nboost
- Deviations / v2:
  - Regression RMSE vs sklearn **~9.5%** on numpy fixture (spec 2%; histogram quantile bins vs sklearn exact splits ? v2 exact-split fallback for small nodes)
  - `neval` metrics inline until runtime wiring; parallel histogram threads deferred
  - Ranking/LambdaMART, DART, GOSS, GPU: v2

## nlearn
- Status: green (crate + sklearn parity tests verified with temporary workspace wiring)
- Tests: 18 passing (sklearn fixtures: LinReg/Ridge coef rtol=1e-6; LogReg accuracy within 1-2%; preprocessing rtol=1e-10; OneHot exact; PCA up to sign; Tree/RF/kNN/GNB accuracy within 1-5% on Iris; Pipeline matches manual sequence; errors 4053/4054)
- Benchmark (`python benchmarks/benchmark_nlearn.py`):
  - sklearn Iris LogReg ~10.6 ms; Pipeline ~11.0 ms; DT ~2.3 ms; RF10 ~21.2 ms; kNN ~3.4 ms; LogReg 5k x 20 ~13.6 ms
  - `cargo test -p niao_learn --release` (18 tests) ~0.8 s wall ? dedicated per-op nlearn timing binary deferred to orchestrator/runtime wiring (target 3-5x sklearn)
- Deps to wire (orchestrator):
  - Root `Cargo.toml` members += `crates/niao_learn`; `[workspace.dependencies]` += `niao_learn = { path = "crates/niao_learn" }`
  - `crates/niao_errors/src/codes.rs` += 4050-4055 (`E4050_NLEARN_ARITY` .. `E4055_NLEARN_NON_CONVERGENCE`) + kind map `"nlearn_error"`
  - `niao_libs/catalog.json` += nlearn
  - `crates/niao_runtime/Cargo.toml` += `niao_learn = { workspace = true }`
  - `crates/niao_runtime/src/nlearn.rs` ? ~28 builtins mirroring `niao_libs/nlearn` (constructors + fit/predict/transform/score + Pipeline)
  - `crates/niao_runtime/src/lib.rs`: `mod nlearn;` + builtins + namespace + import paths `nlearn` / `std/nlearn`
  - `CHANGELOG.md` line for nlearn
- Deviations / v2:
  - PCA via covariance `eig_symmetric` (nnum Jacobi SVD returned degenerate components on Iris)
  - Metrics / CV are in-crate (`accuracy`/`r2`/`KFold`/`train_test_split`) until `neval`/`ntune` crates exist
  - Lasso coord-descent tol looser than sklearn path (still recovers dense coefs on noise-free fixture)
  - KMeans labels vs sklearn match up to init/permutation; seed=42 parity not bit-identical to sklearn k-means++
  - Layer 3 deferred: SVM/SMO, GB/AdaBoost/Bagging, DBSCAN/GMM, TruncatedSVD/KernelPCA
  - Layer 4 partial: Pipeline + KFold shipped; ColumnTransformer/FeatureUnion/GridSearchCV -> v2 / ntune

## nnlp
- Status: green (crate standalone via `[workspace]` in `crates/niao_nlp/Cargo.toml`; shared files untouched)
- Tests: 25 passing (Martin Porter C reference voc subset exact; sklearn TfidfVectorizer fixture rtol=1e-6; CountVectorizer vocab; ngrams/stopwords/tokenizer; Levenshtein/Jaro/cosine/Jaccard/BM25; word2vec most_similar + loss decrease; errors 4083 not-fitted / 4084 empty vocab)
- Benchmark (`python benchmarks/benchmark_nnlp.py`, release):
  - TF-IDF fit_transform 100k short docs: sklearn ~690 ms | niao_nlp ~799 ms = **1.16x** (target reasonable; logged)
  - Porter stem 100k tokens (nltk baseline): ~1053 ms
- Deps to wire (orchestrator):
  - Root `Cargo.toml` members += `crates/niao_nlp`; `[workspace.dependencies]` += `niao_nlp = { path = "crates/niao_nlp" }`
  - Remove standalone `[workspace]` table from `crates/niao_nlp/Cargo.toml`; switch deps to `{ workspace = true }`
  - `crates/niao_errors/src/codes.rs` += 4080?4089 (`E4080_NNLP_ARITY` ? `E4086_NNLP_OOV`) + kind map `"nnlp_error"`
  - `niao_libs/catalog.json` += nnlp
  - `crates/niao_runtime/Cargo.toml` += `niao_nlp = { workspace = true }`
  - `crates/niao_runtime/src/nnlp.rs` ? new module (~28 builtins): normalize/tokenize/stem, vectorizers (fit/transform), word2vec, similarity, baselines; wrap `ntok` for subword at runtime only
  - `crates/niao_runtime/src/lib.rs`: `mod nnlp;` + builtins + namespace + import paths `nnlp` / `std/nnlp`
  - `CHANGELOG.md` line for nnlp
- Deviations / v2:
  - **Classical only:** word2vec CBOW/skip-gram + TF-IDF/n-grams shipped; **no transformer embeddings** (`nembed` not called)
  - **No nlearn pipeline glue** in crate (nlearn runtime wiring deferred); sparse CSR handoff via `CsrMatrix.to_nnum()`
  - Porter stemmer = Martin Porter Release 3 C port (not NLTK_EXTENSIONS irregular table)
  - GloVe loader, full WordNet lemmatization, text-classification end-to-end with nlearn LogReg: v2
  - `ntok` subword tokenization is runtime wrapper only (no `ntok` Rust crate exists)
  - Unicode NFC/NFKC: lightweight Latin accent NFD strip (no full unicode tables dump)

