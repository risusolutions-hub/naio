"""Emit Rust fixture module from sklearn JSON."""
import json
from pathlib import Path

d = json.loads(
    Path(r"c:\Risu\Neko\crates\niao_learn\tests\sklearn_fixtures.json").read_text()
)
out = Path(r"c:\Risu\Neko\crates\niao_learn\src\fixtures.rs")
lines = [
    "//! Sklearn reference fixtures (generated).",
    "#![allow(dead_code)]",
    "",
]


def fmt_f(x: float) -> str:
    s = f"{float(x):.17g}"
    if "." not in s and "e" not in s and "E" not in s:
        s += ".0"
    return s


def arr(name: str, v) -> None:
    if v and isinstance(v[0], list):
        flat = []
        rows = len(v)
        cols = len(v[0])
        for row in v:
            flat.extend(row)
        lines.append(f"pub const {name}_ROWS: usize = {rows};")
        lines.append(f"pub const {name}_COLS: usize = {cols};")
        body = ", ".join(fmt_f(x) for x in flat)
        lines.append(f"pub const {name}: &[f64] = &[{body}];")
    else:
        body = ", ".join(fmt_f(x) for x in v)
        lines.append(f"pub const {name}: &[f64] = &[{body}];")


def iarr(name: str, v) -> None:
    body = ", ".join(str(int(x)) for x in v)
    lines.append(f"pub const {name}: &[i32] = &[{body}];")


def scalar(name: str, v: float) -> None:
    lines.append(f"pub const {name}: f64 = {fmt_f(v)};")


arr("LINREG_X", d["linreg_X"])
arr("LINREG_Y", d["linreg_y"])
arr("LINREG_COEF", d["linreg_coef"])
scalar("LINREG_INTERCEPT", d["linreg_intercept"])
arr("RIDGE_COEF", d["ridge_coef"])
scalar("RIDGE_INTERCEPT", d["ridge_intercept"])
arr("LASSO_COEF", d["lasso_coef"])
scalar("LASSO_INTERCEPT", d["lasso_intercept"])
arr("IRIS_X", d["iris_X"])
arr("IRIS_Y", d["iris_y"])
arr("LOG_BINARY_Y", d["log_binary_y"])
arr("LOG_BINARY_COEF", d["log_binary_coef"])
scalar("LOG_BINARY_INTERCEPT", d["log_binary_intercept"])
scalar("LOG_BINARY_ACC", d["log_binary_acc"])
scalar("LOG_MULTI_ACC", d["log_multi_acc"])
arr("SS_MEAN", d["ss_mean"])
arr("SS_SCALE", d["ss_scale"])
arr("SS_TRANSFORM_ROW0", d["ss_transform_row0"])
arr("MM_DATA_MIN", d["mm_data_min"])
arr("MM_DATA_MAX", d["mm_data_max"])
arr("MM_TRANSFORM_ROW0", d["mm_transform_row0"])
arr("PCA_COMPONENTS", d["pca_components"])
arr("PCA_EXPLAINED", d["pca_explained"])
arr("PCA_MEAN", d["pca_mean"])
arr("KMEANS_CENTERS", d["kmeans_centers"])
iarr("KMEANS_LABELS", d["kmeans_labels"])
scalar("DT_ACC", d["dt_acc"])
scalar("RF_ACC", d["rf_acc"])
scalar("KNN_ACC", d["knn_acc"])
iarr("KNN_PRED_FIRST10", d["knn_pred_first10"])
scalar("GNB_ACC", d["gnb_acc"])
arr("GNB_THETA", d["gnb_theta"])
arr("GNB_VAR", d["gnb_var"])
arr("OHE_CATS", d["ohe_cats"])
arr("OHE_OUT", d["ohe_out"])
scalar("PIPE_ACC", d["pipe_acc"])

out.write_text("\n".join(lines) + "\n", encoding="utf-8")
print("wrote", out, "bytes", out.stat().st_size)
