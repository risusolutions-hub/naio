//! One-call convenience functions (seaborn-ish).

use crate::charts::BarMode;
use crate::error::PlotResult;
use crate::figure::Figure;
use niao_num::linspace;

pub fn line(x: &[f64], y: &[f64]) -> PlotResult<Figure> {
    let mut fig = Figure::new(640.0, 480.0);
    fig.axes(0)?.line(x, y, None)?;
    Ok(fig)
}

/// Line chart with x = linspace(start, stop, n) and y = f(x).
pub fn line_fn(start: f64, stop: f64, n: usize, f: fn(f64) -> f64) -> PlotResult<Figure> {
    let x_arr = linspace(start, stop, n)?;
    let x: Vec<f64> = x_arr.to_vec();
    let y: Vec<f64> = x.iter().map(|&v| f(v)).collect();
    line(&x, &y)
}

pub fn scatter(x: &[f64], y: &[f64]) -> PlotResult<Figure> {
    let mut fig = Figure::new(640.0, 480.0);
    fig.axes(0)?.scatter(x, y, None)?;
    Ok(fig)
}

pub fn bar(cats: &[String], vals: &[f64]) -> PlotResult<Figure> {
    let mut fig = Figure::new(640.0, 480.0);
    fig.axes(0)?.bar(cats, vals, None, BarMode::Grouped)?;
    Ok(fig)
}

pub fn hist(data: &[f64], bins: usize) -> PlotResult<Figure> {
    let mut fig = Figure::new(640.0, 480.0);
    fig.axes(0)?.hist(data, bins, None)?;
    Ok(fig)
}

pub fn heatmap(matrix: &[f64], rows: usize, cols: usize) -> PlotResult<Figure> {
    let mut fig = Figure::new(512.0, 512.0);
    fig.axes(0)?.heatmap(matrix, rows, cols, None)?;
    Ok(fig)
}

pub fn confusion_matrix(cm: &[f64], n: usize, labels: &[String]) -> PlotResult<Figure> {
    let mut fig = Figure::new(480.0, 480.0);
    fig.axes(0)?
        .confusion_matrix(cm, n, labels, Some("Confusion Matrix"))?;
    Ok(fig)
}

pub fn roc_curve(fpr: &[f64], tpr: &[f64]) -> PlotResult<Figure> {
    let mut fig = Figure::new(480.0, 480.0);
    let ax = fig.axes(0)?;
    ax.set_xlabel("FPR");
    ax.set_ylabel("TPR");
    ax.roc(fpr, tpr, Some("ROC"))?;
    Ok(fig)
}
