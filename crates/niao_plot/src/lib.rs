//! nplot — matplotlib/seaborn-style SVG plotting for Niao.

pub mod axis;
pub mod charts;
pub mod color;
pub mod convenience;
pub mod error;
pub mod figure;
pub mod fmt;
pub mod scene;

pub use axis::{autoscale, nice_ticks, Limits, Scale, Transform};
pub use charts::{BarMode, BoxStats, Series, SeriesKind, MAX_PLOT_POINTS};
pub use color::{categorical, sequential, Rgba, TAB10, VIRIDIS};
pub use convenience::{bar, confusion_matrix, heatmap, hist, line, line_fn, roc_curve, scatter};
pub use error::{
    PlotError, PlotResult, E4040_NPLOT_ARITY, E4041_NPLOT_EMPTY, E4042_NPLOT_LENGTH,
    E4043_NPLOT_HANDLE, E4044_NPLOT_RENDER,
};
pub use figure::{box_stats, Axes, Figure, XScale};
pub use scene::{normalize_svg, Element, Scene, TextAnchor};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::scene::normalize_svg;

    fn fixture_line() -> Figure {
        let x: Vec<f64> = (0..20).map(|i| i as f64).collect();
        let y: Vec<f64> = x.iter().map(|v| (v * 0.3).sin() * 5.0 + 5.0).collect();
        let mut fig = Figure::new(400.0, 300.0);
        fig.axes(0).unwrap().line(&x, &y, Some("sin")).unwrap();
        fig
    }

    #[test]
    fn golden_line_contains_polyline() {
        let svg = fixture_line().to_svg_string();
        let norm = normalize_svg(&svg);
        assert!(norm.contains("<polyline"));
        assert!(norm.contains("sin"));
        assert!(norm.contains("<svg"));
    }

    #[test]
    fn golden_bar_renders() {
        let cats = vec!["A".into(), "B".into(), "C".into()];
        let vals = vec![3.0, 7.0, 2.0];
        let fig = bar(&cats, &vals).unwrap();
        let norm = normalize_svg(&fig.to_svg_string());
        assert!(norm.contains("<rect"));
    }

    #[test]
    fn golden_scatter_renders() {
        let x = vec![1.0, 2.0, 3.0, 4.0];
        let y = vec![1.0, 4.0, 2.0, 3.0];
        let fig = scatter(&x, &y).unwrap();
        let norm = normalize_svg(&fig.to_svg_string());
        assert!(norm.contains("<circle"));
    }

    #[test]
    fn golden_hist_renders() {
        let data: Vec<f64> = (0..100).map(|i| (i as f64 * 0.1).sin() + 1.0).collect();
        let fig = hist(&data, 10).unwrap();
        let norm = normalize_svg(&fig.to_svg_string());
        assert!(norm.contains("<rect"));
    }

    #[test]
    fn golden_heatmap_renders() {
        let data: Vec<f64> = (0..16).map(|i| i as f64).collect();
        let fig = heatmap(&data, 4, 4).unwrap();
        let norm = normalize_svg(&fig.to_svg_string());
        assert!(norm.contains("<rect"));
    }

    #[test]
    fn golden_confusion_matrix_renders() {
        let cm = vec![5.0, 1.0, 0.0, 4.0];
        let labels = vec!["neg".into(), "pos".into()];
        let fig = confusion_matrix(&cm, 2, &labels).unwrap();
        let norm = normalize_svg(&fig.to_svg_string());
        assert!(norm.contains("Confusion Matrix"));
        assert!(norm.contains("<text"));
    }

    #[test]
    fn empty_data_error() {
        let err = line(&[], &[]).err().unwrap();
        assert_eq!(err.code(), E4041_NPLOT_EMPTY);
    }

    #[test]
    fn length_mismatch_error() {
        let err = scatter(&[1.0, 2.0], &[1.0]).err().unwrap();
        assert_eq!(err.code(), E4042_NPLOT_LENGTH);
    }

    #[test]
    fn bad_save_path_error() {
        let fig = fixture_line();
        let err = fig.save_svg("/nonexistent_dir_xyz/chart.svg").err().unwrap();
        assert_eq!(err.code(), E4044_NPLOT_RENDER);
    }

    #[test]
    fn render_10k_line_under_budget() {
        let n = 10_000usize;
        let x: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let y: Vec<f64> = x.iter().map(|v| v.sin()).collect();
        let mut fig = Figure::new(800.0, 600.0);
        fig.axes(0).unwrap().line(&x, &y, None).unwrap();
        let t0 = std::time::Instant::now();
        let _svg = fig.to_svg_string();
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        assert!(ms < 100.0, "10k line render took {ms:.2} ms (budget 100 ms)");
    }
}
