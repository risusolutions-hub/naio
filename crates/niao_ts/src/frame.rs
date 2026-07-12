//! nframe integration: load f64 series from DataFrame columns.

use crate::error::{TsError, TsResult};
use niao_frame::Series;

/// Extract an f64 series from a nframe Series column.
pub fn series_to_vec(s: &Series) -> TsResult<Vec<f64>> {
    s.as_f64_slice()
        .map(|v| v.to_vec())
        .ok_or_else(|| TsError::Type("series_to_vec: expected f64 column".into()))
}

/// Wrap a fitted vector as an f64 Series (for nframe pipelines).
pub fn vec_to_series(name: &str, data: &[f64]) -> Series {
    Series::from_f64(name, data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_series() {
        let data = vec![1.0, 2.0, 3.0];
        let s = vec_to_series("y", &data);
        let back = series_to_vec(&s).unwrap();
        assert_eq!(back, data);
    }
}
