# NLEARN — Classical ML for Niao

`nlearn` is a **scikit-learn** subset: a uniform estimator API over linear models,
neighbors, naïve Bayes, decision trees, random forests, KMeans, PCA, preprocessing,
and `Pipeline`. Implemented in `crates/niao_learn` (std + `niao_num` / `niao_optim` /
`niao_rand` / `niao_frame` / `niao_stats` only — zero new third-party crates).

Import (after runtime wiring):

```niao
import "nlearn"
```

## Estimator contract

Every model shares:

| Method | Role |
|--------|------|
| `fit(x, y?)` | Learn parameters |
| `predict(x)` | Labels / continuous targets |
| `predict_proba(x)` | Class probabilities (classifiers) |
| `transform(x)` | Feature maps (scalers, PCA, …) |
| `score(x, y)` | Accuracy (clf) or R² (reg) |

Errors: **4053** not fitted, **4054** shape mismatch, **4055** non-convergence.

## Models (v1)

| Family | Types |
|--------|--------|
| Linear | `LinearRegression`, `Ridge`, `Lasso`, `ElasticNet` |
| Logistic | `LogisticRegression` (binary + multinomial, L-BFGS) |
| Neighbors | `KNeighborsClassifier`, `KNeighborsRegressor` |
| Naïve Bayes | `GaussianNB`, `MultinomialNB`, `BernoulliNB` |
| Trees | `DecisionTreeClassifier`, `DecisionTreeRegressor` |
| Ensembles | `RandomForestClassifier`, `RandomForestRegressor` |
| Clustering | `KMeans` (k-means++) |
| Decomposition | `PCA` |
| Preprocessing | `StandardScaler`, `MinMaxScaler`, `RobustScaler`, `Normalizer`, `OneHotEncoder`, `OrdinalEncoder`, `LabelEncoder`, `PolynomialFeatures`, `SimpleImputer`, `Binarizer` |
| Compose | `Pipeline` |
| Selection | `train_test_split`, `KFold` |

## Example

```niao
import "nlearn"

fn main() {
    // Iris-like matrix: rows = samples, cols = features
    let x = /* n x 4 */
    let y = /* n labels */
    let mut pipe = nlearn.Pipeline([
        ("sc", nlearn.StandardScaler()),
        ("clf", nlearn.LogisticRegression()),
    ])
    pipe.fit(x, y)
    print("accuracy:", pipe.score(x, y))
    print("preds:", pipe.predict(x))
}
```

See `examples/nlearn_demo.niao` and `cargo test -p niao_learn`.

## v2 / deferred

SVM (SMO), GradientBoosting / AdaBoost / Bagging (see `nboost`), DBSCAN / GMM,
TruncatedSVD / KernelPCA, `ColumnTransformer` / `FeatureUnion`, `GridSearchCV`
(reuse `ntune`), metrics via `neval`.
