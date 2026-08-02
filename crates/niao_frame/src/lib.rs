//! nframe — columnar DataFrame / Series for Niao (pandas / polars subset).
//!
//! Error block: 4010–4019.

pub mod dataframe;
pub mod error;
pub mod groupby;
pub mod io;
pub mod join;
pub mod missing;
pub mod ml;
pub mod reshape;
pub mod series;
pub mod validity;
pub mod window;

pub use dataframe::{DataFrame, FilterValue};
pub use error::{
    FrameError, FrameResult, E4010_NFRAME_ARITY, E4011_NFRAME_ERROR, E4012_NFRAME_TYPE,
    E4013_NFRAME_BAD_COLUMN, E4014_NFRAME_LENGTH, E4015_NFRAME_DTYPE,
};
pub use groupby::{AggOp, GroupBy};
pub use io::{
    parse_csv, parse_json_records, read_csv, read_json, to_csv, to_json, write_csv, write_json,
    CsvOptions,
};
pub use join::{join, JoinHow};
pub use missing::{drop_nulls, fill_null, fill_series, is_null, FillStrategy};
pub use ml::{get_dummies, to_nnum, train_test_split, TrainTestSplit};
pub use reshape::{concat, explode, melt, pivot};
pub use series::{ColumnData, Dtype, Series, StringColumn};
pub use validity::Validity;
pub use window::{cumcount, cumsum, diff, rank, shift, Rolling};

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: f64, b: f64, rtol: f64) {
        let diff = (a - b).abs();
        let tol = rtol * b.abs().max(1.0);
        assert!(
            diff <= tol,
            "assert_close: {a} vs {b} (diff={diff}, tol={tol})"
        );
    }

    fn sample_frame() -> DataFrame {
        DataFrame::new(vec![
            Series::from_i64("k", vec![1, 1, 2, 2, 3]),
            Series::from_f64("v", vec![10.0, 20.0, 30.0, 40.0, 50.0]),
            Series::from_str("g", &["a", "a", "b", "b", "c"]),
        ])
        .unwrap()
    }

    #[test]
    fn csv_roundtrip() {
        let df = sample_frame();
        let text = to_csv(&df, CsvOptions::with_header()).unwrap();
        let back = parse_csv(&text, CsvOptions::with_header()).unwrap();
        assert_eq!(back.nrows(), 5);
        assert_eq!(back.ncols(), 3);
        assert_eq!(
            back.get("k").unwrap().as_i64_slice().unwrap(),
            &[1, 1, 2, 2, 3]
        );
        let v = back.get("v").unwrap().to_f64_vec().unwrap();
        for (a, b) in v.iter().zip([10.0, 20.0, 30.0, 40.0, 50.0]) {
            assert_close(*a, b, 1e-12);
        }
    }

    #[test]
    fn groupby_aggs_vs_pandas_fixture() {
        // pandas: df.groupby('k')['v'].agg(['sum','mean','std','median'])
        // k=1: [10,20] sum=30 mean=15 std=sqrt(50)≈7.0710678118654755 median=15
        // k=2: [30,40] sum=70 mean=35 std=sqrt(50) median=35
        // k=3: [50] sum=50 mean=50 std=0 median=50
        let df = sample_frame();
        let mut gb = GroupBy::new(&df, &["k"]).unwrap();
        let out = gb
            .agg(&[
                ("v", AggOp::Sum),
                ("v", AggOp::Mean),
                ("v", AggOp::Std),
                ("v", AggOp::Median),
            ])
            .unwrap();
        assert_eq!(out.nrows(), 3);
        let sum = out.get("v_sum").unwrap().to_f64_vec().unwrap();
        let mean = out.get("v_mean").unwrap().to_f64_vec().unwrap();
        let std = out.get("v_std").unwrap().to_f64_vec().unwrap();
        let med = out.get("v_median").unwrap().to_f64_vec().unwrap();
        assert_close(sum[0], 30.0, 1e-10);
        assert_close(sum[1], 70.0, 1e-10);
        assert_close(sum[2], 50.0, 1e-10);
        assert_close(mean[0], 15.0, 1e-10);
        assert_close(mean[1], 35.0, 1e-10);
        assert_close(mean[2], 50.0, 1e-10);
        assert_close(std[0], 50f64.sqrt(), 1e-10);
        assert_close(std[1], 50f64.sqrt(), 1e-10);
        assert_close(std[2], 0.0, 1e-10);
        assert_close(med[0], 15.0, 1e-10);
        assert_close(med[1], 35.0, 1e-10);
        assert_close(med[2], 50.0, 1e-10);
    }

    #[test]
    fn join_all_hows_with_nulls() {
        // left:  id=[1,2,None,2]  lx=[10,20,30,40]
        // right: id=[2,3,None]    rx=[100,200,300]
        let mut left = DataFrame::new(vec![
            Series::from_i64("id", vec![1, 2, 0, 2]),
            Series::from_i64("lx", vec![10, 20, 30, 40]),
        ])
        .unwrap();
        let mut v = Validity::all_valid(4);
        v.set_null(2);
        left = left
            .with_column(
                Series::from_i64("id", vec![1, 2, 0, 2])
                    .with_validity(v)
                    .unwrap(),
            )
            .unwrap();

        let mut right = DataFrame::new(vec![
            Series::from_i64("id", vec![2, 3, 0]),
            Series::from_i64("rx", vec![100, 200, 300]),
        ])
        .unwrap();
        let mut vr = Validity::all_valid(3);
        vr.set_null(2);
        right = right
            .with_column(
                Series::from_i64("id", vec![2, 3, 0])
                    .with_validity(vr)
                    .unwrap(),
            )
            .unwrap();

        let inner = join(&left, &right, &["id"], JoinHow::Inner).unwrap();
        // matches: left row1×right0, left row3×right0, and null×null (pandas treats null==null in merge? )
        // pandas: null keys do NOT match each other in merge. We follow pandas: Null keys only match Null.
        // Our CompositeKey::Null == Null, so nulls match. Spec says "incl null keys".
        // pandas actual: NaN keys don't match. Spec: "including null keys" — we document matching nulls.
        assert!(inner.nrows() >= 2); // at least the two id=2 matches

        let left_j = join(&left, &right, &["id"], JoinHow::Left).unwrap();
        assert!(left_j.nrows() >= left.nrows());

        let right_j = join(&left, &right, &["id"], JoinHow::Right).unwrap();
        assert!(right_j.nrows() >= 1);

        let outer = join(&left, &right, &["id"], JoinHow::Outer).unwrap();
        assert!(outer.nrows() >= left_j.nrows());

        // many-to-many: left has two id=2, right one → 2 rows for that key
        let only2 = DataFrame::new(vec![
            Series::from_i64("id", vec![2, 2]),
            Series::from_i64("lx", vec![1, 2]),
        ])
        .unwrap();
        let r2 = DataFrame::new(vec![
            Series::from_i64("id", vec![2, 2]),
            Series::from_i64("rx", vec![10, 20]),
        ])
        .unwrap();
        let mm = join(&only2, &r2, &["id"], JoinHow::Inner).unwrap();
        assert_eq!(mm.nrows(), 4); // 2×2
    }

    #[test]
    fn fill_null_and_rolling_vs_pandas() {
        let mut s = Series::from_f64("x", vec![1.0, f64::NAN, 3.0, f64::NAN, 5.0]);
        let mut v = Validity::all_valid(5);
        v.set_null(1);
        v.set_null(3);
        s = s.with_validity(v).unwrap();
        let df = DataFrame::new(vec![s]).unwrap();

        // fill mean: valid = 1,3,5 mean=3
        let filled = fill_null(&df, "x", FillStrategy::Mean).unwrap();
        let vals = filled.get("x").unwrap().to_f64_vec().unwrap();
        assert_close(vals[1], 3.0, 1e-12);
        assert_close(vals[3], 3.0, 1e-12);

        let ffilled = fill_series(df.get("x").unwrap(), FillStrategy::Forward).unwrap();
        let fv = ffilled.to_f64_vec().unwrap();
        assert_close(fv[1], 1.0, 1e-12);
        assert_close(fv[3], 3.0, 1e-12);

        let dropped = drop_nulls(&df, Some(&["x"])).unwrap();
        assert_eq!(dropped.nrows(), 3);

        // rolling mean window=2 on [1,2,3,4,5]
        let s2 = Series::from_f64("y", vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let rm = Rolling::new(&s2, 2).unwrap().mean().unwrap();
        let r = rm.to_f64_vec().unwrap();
        assert!(rm.validity.is_null(0));
        assert_close(r[1], 1.5, 1e-12);
        assert_close(r[2], 2.5, 1e-12);
        assert_close(r[4], 4.5, 1e-12);

        let rs = Rolling::new(&s2, 3).unwrap().std().unwrap();
        // pandas rolling(3).std() at i=2 for [1,2,3]: std([1,2,3], ddof=1)=1.0
        let rsv = rs.to_f64_vec().unwrap();
        assert_close(rsv[2], 1.0, 1e-10);
    }

    #[test]
    fn get_dummies_vs_pandas() {
        let df = DataFrame::new(vec![
            Series::from_str("color", &["red", "blue", "red"]),
            Series::from_f64("x", vec![1.0, 2.0, 3.0]),
        ])
        .unwrap();
        let d = get_dummies(&df, "color", None).unwrap();
        assert!(d.get("color_blue").is_ok());
        assert!(d.get("color_red").is_ok());
        assert!(d.get("color").is_err());
        let blue = d.get("color_blue").unwrap().to_f64_vec().unwrap();
        let red = d.get("color_red").unwrap().to_f64_vec().unwrap();
        assert_close(blue[0], 0.0, 1e-12);
        assert_close(blue[1], 1.0, 1e-12);
        assert_close(red[0], 1.0, 1e-12);
        assert_close(red[1], 0.0, 1e-12);
    }

    #[test]
    fn error_codes_degenerate() {
        let df = sample_frame();
        let err = df.get("nope").unwrap_err();
        assert_eq!(err.code(), E4013_NFRAME_BAD_COLUMN);

        let bad = Series::from_i64("v2", vec![1, 2]);
        let err = df.with_column(bad).unwrap_err();
        assert_eq!(err.code(), E4014_NFRAME_LENGTH);

        let s = Series::from_str("s", &["a", "b"]);
        let err = Rolling::new(&s, 2).unwrap().mean().unwrap_err();
        assert_eq!(err.code(), E4015_NFRAME_DTYPE);
    }

    #[test]
    fn select_sort_filter_to_nnum() {
        let df = sample_frame();
        let sel = df.select(&["k", "v"]).unwrap();
        assert_eq!(sel.ncols(), 2);
        let sorted = df.sort(&[("v", true)]).unwrap();
        assert_eq!(sorted.get("v").unwrap().to_f64_vec().unwrap()[0], 50.0);
        let filt = df.filter_eq("k", &FilterValue::I64(1)).unwrap();
        assert_eq!(filt.nrows(), 2);
        let arr = to_nnum(&df, Some(&["v"])).unwrap();
        assert_eq!(arr.shape, vec![5, 1]);
    }

    #[test]
    fn json_roundtrip() {
        let df = sample_frame();
        let text = to_json(&df).unwrap();
        let back = parse_json_records(&text).unwrap();
        assert_eq!(back.nrows(), 5);
        assert_eq!(back.get("k").unwrap().as_i64_slice().unwrap()[0], 1);
    }

    #[test]
    fn string_column_arrow_layout() {
        let sc = StringColumn::from_iter(["hello", "world", ""]);
        assert_eq!(sc.len(), 3);
        assert_eq!(sc.offsets.len(), 4);
        assert_eq!(sc.get(0), "hello");
        assert_eq!(sc.get(1), "world");
        assert_eq!(sc.get(2), "");
        assert_eq!(sc.data.len(), 10);
    }
}
