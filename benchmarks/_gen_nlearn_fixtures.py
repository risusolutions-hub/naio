"""Generate sklearn reference fixtures for niao_learn tests."""
import json
from pathlib import Path

import numpy as np
from sklearn.cluster import KMeans
from sklearn.datasets import load_iris, make_regression
from sklearn.decomposition import PCA
from sklearn.ensemble import RandomForestClassifier
from sklearn.linear_model import Lasso, LinearRegression, LogisticRegression, Ridge
from sklearn.naive_bayes import GaussianNB
from sklearn.neighbors import KNeighborsClassifier
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import MinMaxScaler, OneHotEncoder, StandardScaler
from sklearn.tree import DecisionTreeClassifier

X, y = make_regression(
    n_samples=40, n_features=3, n_informative=3, noise=0.0, random_state=42
)
lr = LinearRegression(fit_intercept=True).fit(X, y)
ridge = Ridge(alpha=1.0, fit_intercept=True).fit(X, y)
lasso = Lasso(alpha=0.1, fit_intercept=True, max_iter=10000).fit(X, y)

iris = load_iris()
Xi, yi = iris.data, iris.target
yb = (yi == 0).astype(float)
log = LogisticRegression(C=1e12, solver="lbfgs", max_iter=1000).fit(Xi, yb)
logm = LogisticRegression(C=1e12, solver="lbfgs", max_iter=1000).fit(Xi, yi)

ss = StandardScaler().fit(Xi)
mm = MinMaxScaler().fit(Xi)
pca = PCA(n_components=2, svd_solver="full").fit(Xi)
km = KMeans(n_clusters=3, init="k-means++", n_init=1, random_state=42, max_iter=300).fit(
    Xi
)
dt = DecisionTreeClassifier(max_depth=3, random_state=42).fit(Xi, yi)
rf = RandomForestClassifier(n_estimators=10, max_depth=3, random_state=42).fit(Xi, yi)
knn = KNeighborsClassifier(n_neighbors=5).fit(Xi, yi)
gnb = GaussianNB().fit(Xi, yi)
cats = np.array([[0], [1], [2], [0], [1]])
ohe = OneHotEncoder(sparse_output=False).fit(cats)
pipe = Pipeline(
    [
        ("sc", StandardScaler()),
        ("lr", LogisticRegression(C=1e12, solver="lbfgs", max_iter=1000)),
    ]
)
pipe.fit(Xi, yb)

fixtures = {
    "linreg_X": X.tolist(),
    "linreg_y": y.tolist(),
    "linreg_coef": lr.coef_.tolist(),
    "linreg_intercept": float(lr.intercept_),
    "ridge_coef": ridge.coef_.tolist(),
    "ridge_intercept": float(ridge.intercept_),
    "lasso_coef": lasso.coef_.tolist(),
    "lasso_intercept": float(lasso.intercept_),
    "iris_X": Xi.tolist(),
    "iris_y": yi.astype(float).tolist(),
    "log_binary_y": yb.tolist(),
    "log_binary_coef": log.coef_.ravel().tolist(),
    "log_binary_intercept": float(log.intercept_[0]),
    "log_binary_acc": float(log.score(Xi, yb)),
    "log_multi_acc": float(logm.score(Xi, yi)),
    "ss_mean": ss.mean_.tolist(),
    "ss_scale": ss.scale_.tolist(),
    "ss_transform_row0": ss.transform(Xi[:1]).ravel().tolist(),
    "mm_data_min": mm.data_min_.tolist(),
    "mm_data_max": mm.data_max_.tolist(),
    "mm_transform_row0": mm.transform(Xi[:1]).ravel().tolist(),
    "pca_components": pca.components_.tolist(),
    "pca_explained": pca.explained_variance_.tolist(),
    "pca_mean": pca.mean_.tolist(),
    "kmeans_centers": km.cluster_centers_.tolist(),
    "kmeans_labels": km.labels_.astype(int).tolist(),
    "dt_acc": float(dt.score(Xi, yi)),
    "rf_acc": float(rf.score(Xi, yi)),
    "knn_acc": float(knn.score(Xi, yi)),
    "knn_pred_first10": knn.predict(Xi[:10]).astype(int).tolist(),
    "gnb_acc": float(gnb.score(Xi, yi)),
    "gnb_theta": gnb.theta_.tolist(),
    "gnb_var": gnb.var_.tolist(),
    "ohe_cats": cats.ravel().astype(float).tolist(),
    "ohe_out": ohe.transform(cats).tolist(),
    "pipe_acc": float(pipe.score(Xi, yb)),
}

out = Path(__file__).resolve().parents[1] / "crates" / "niao_learn" / "tests" / "sklearn_fixtures.json"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(fixtures))
print("wrote", out)
for k in ("dt_acc", "rf_acc", "knn_acc", "gnb_acc", "log_binary_acc", "pipe_acc"):
    print(k, fixtures[k])
print("linreg_coef", fixtures["linreg_coef"])
