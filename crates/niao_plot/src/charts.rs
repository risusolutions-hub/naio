//! Chart types and series rendering.

use crate::axis::Transform;
use crate::axis::{autoscale, Limits};
use crate::color::{categorical, sequential, Rgba};
use crate::error::{require_non_empty, require_same_len, PlotError, PlotResult};
use crate::figure::Axes;
use crate::scene::{Element, TextAnchor};

pub const MAX_PLOT_POINTS: usize = 50_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BarMode {
    Grouped,
    Stacked,
}

#[derive(Clone, Debug)]
pub struct BoxStats {
    pub min: f64,
    pub q1: f64,
    pub med: f64,
    pub q3: f64,
    pub max: f64,
}

#[derive(Clone, Debug)]
pub enum SeriesKind {
    Line {
        x: Vec<f64>,
        y: Vec<f64>,
    },
    Scatter {
        x: Vec<f64>,
        y: Vec<f64>,
    },
    Bar {
        cats: Vec<String>,
        vals: Vec<f64>,
        mode: BarMode,
    },
    HBar {
        cats: Vec<String>,
        vals: Vec<f64>,
    },
    Hist {
        counts: Vec<f64>,
        edges: Vec<f64>,
    },
    Box {
        groups: Vec<(String, BoxStats)>,
    },
    Heatmap {
        data: Vec<f64>,
        rows: usize,
        cols: usize,
    },
    ConfusionMatrix {
        cm: Vec<f64>,
        n: usize,
        labels: Vec<String>,
    },
    Roc {
        fpr: Vec<f64>,
        tpr: Vec<f64>,
    },
    ErrorBar {
        x: Vec<f64>,
        y: Vec<f64>,
        yerr: Vec<f64>,
    },
    Step {
        x: Vec<f64>,
        y: Vec<f64>,
    },
    Area {
        x: Vec<f64>,
        y: Vec<f64>,
    },
    Pie {
        labels: Vec<String>,
        vals: Vec<f64>,
    },
}

#[derive(Clone, Debug)]
pub struct Series {
    pub label: Option<String>,
    pub kind: SeriesKind,
}

impl Series {
    pub fn data_bounds(&self) -> (Limits, Limits) {
        match &self.kind {
            SeriesKind::Line { x, y } | SeriesKind::Scatter { x, y } => bounds_xy(x, y),
            SeriesKind::Bar {
                cats: _,
                vals,
                mode: _,
            } => {
                let ymax = vals.iter().cloned().fold(0.0f64, f64::max);
                (
                    Limits::new(0.0, vals.len() as f64),
                    Limits::new(0.0, ymax * 1.1),
                )
            }
            SeriesKind::HBar { cats, vals } => {
                let xmax = vals.iter().cloned().fold(0.0f64, f64::max);
                (
                    Limits::new(0.0, xmax * 1.1),
                    Limits::new(0.0, cats.len() as f64),
                )
            }
            SeriesKind::Hist { counts, edges } => {
                let ymax = counts.iter().cloned().fold(0.0f64, f64::max);
                let xmin = edges.first().copied().unwrap_or(0.0);
                let xmax = edges.last().copied().unwrap_or(1.0);
                (Limits::new(xmin, xmax), Limits::new(0.0, ymax * 1.1))
            }
            SeriesKind::Box { groups } => {
                let n = groups.len() as f64;
                let mut ymin = f64::INFINITY;
                let mut ymax = f64::NEG_INFINITY;
                for (_, s) in groups {
                    ymin = ymin.min(s.min);
                    ymax = ymax.max(s.max);
                }
                (Limits::new(0.0, n), Limits::new(ymin, ymax))
            }
            SeriesKind::Heatmap {
                data: _,
                rows,
                cols,
            } => (
                Limits::new(0.0, *cols as f64),
                Limits::new(0.0, *rows as f64),
            ),
            SeriesKind::ConfusionMatrix { n, .. } => {
                (Limits::new(0.0, *n as f64), Limits::new(0.0, *n as f64))
            }
            SeriesKind::Roc { fpr: _, tpr: _ } => (Limits::new(0.0, 1.0), Limits::new(0.0, 1.0)),
            SeriesKind::ErrorBar { x, y, yerr } => {
                let mut yl = bounds_xy(x, y).1;
                for (yi, e) in y.iter().zip(yerr.iter()) {
                    yl.min = yl.min.min(yi - e);
                    yl.max = yl.max.max(yi + e);
                }
                (bounds_xy(x, y).0, yl)
            }
            SeriesKind::Step { x, y } | SeriesKind::Area { x, y } => bounds_xy(x, y),
            SeriesKind::Pie { .. } => (Limits::new(-1.0, 1.0), Limits::new(-1.0, 1.0)),
        }
    }

    pub fn render(
        &self,
        scene: &mut crate::scene::Scene,
        xtr: &Transform,
        ytr: &Transform,
        color: Rgba,
    ) {
        match &self.kind {
            SeriesKind::Line { x, y } => render_line(scene, x, y, xtr, ytr, color, false),
            SeriesKind::Scatter { x, y } => render_scatter(scene, x, y, xtr, ytr, color),
            SeriesKind::Bar {
                cats: _,
                vals,
                mode,
            } => render_bar(scene, vals, xtr, ytr, color, *mode),
            SeriesKind::HBar { cats: _, vals } => render_hbar(scene, vals, xtr, ytr, color),
            SeriesKind::Hist { counts, edges } => {
                render_hist(scene, counts, edges, xtr, ytr, color)
            }
            SeriesKind::Box { groups } => render_box(scene, groups, xtr, ytr, color),
            SeriesKind::Heatmap { data, rows, cols } => {
                render_heatmap(scene, data, *rows, *cols, xtr, ytr)
            }
            SeriesKind::ConfusionMatrix { cm, n, labels } => {
                render_confusion(scene, cm, *n, labels, xtr, ytr)
            }
            SeriesKind::Roc { fpr, tpr } => render_line(scene, fpr, tpr, xtr, ytr, color, false),
            SeriesKind::ErrorBar { x, y, yerr } => {
                render_errorbar(scene, x, y, yerr, xtr, ytr, color)
            }
            SeriesKind::Step { x, y } => render_line(scene, x, y, xtr, ytr, color, true),
            SeriesKind::Area { x, y } => render_area(scene, x, y, xtr, ytr, color),
            SeriesKind::Pie { labels, vals } => render_pie(scene, labels, vals, xtr, ytr),
        }
    }
}

fn bounds_xy(x: &[f64], y: &[f64]) -> (Limits, Limits) {
    let n = x.len().min(y.len());
    let mut xmin = f64::INFINITY;
    let mut xmax = f64::NEG_INFINITY;
    let mut ymin = f64::INFINITY;
    let mut ymax = f64::NEG_INFINITY;
    for i in 0..n {
        if x[i].is_finite() && y[i].is_finite() {
            xmin = xmin.min(x[i]);
            xmax = xmax.max(x[i]);
            ymin = ymin.min(y[i]);
            ymax = ymax.max(y[i]);
        }
    }
    if !xmin.is_finite() {
        return (Limits::new(0.0, 1.0), Limits::new(0.0, 1.0));
    }
    (Limits::new(xmin, xmax), Limits::new(ymin, ymax))
}

fn downsample_indices(n: usize) -> Vec<usize> {
    if n <= MAX_PLOT_POINTS {
        return (0..n).collect();
    }
    let step = (n as f64 / MAX_PLOT_POINTS as f64).ceil() as usize;
    (0..n).step_by(step.max(1)).collect()
}

pub fn draw_line(ax: &mut Axes, x: &[f64], y: &[f64], label: Option<&str>) -> PlotResult<()> {
    require_non_empty(x, "line")?;
    require_same_len(x, y, "line")?;
    ax.push_series(Series {
        label: label.map(str::to_string),
        kind: SeriesKind::Line {
            x: x.to_vec(),
            y: y.to_vec(),
        },
    });
    Ok(())
}

pub fn draw_scatter(ax: &mut Axes, x: &[f64], y: &[f64], label: Option<&str>) -> PlotResult<()> {
    require_non_empty(x, "scatter")?;
    require_same_len(x, y, "scatter")?;
    ax.push_series(Series {
        label: label.map(str::to_string),
        kind: SeriesKind::Scatter {
            x: x.to_vec(),
            y: y.to_vec(),
        },
    });
    Ok(())
}

pub fn draw_bar(
    ax: &mut Axes,
    cats: &[String],
    vals: &[f64],
    label: Option<&str>,
    mode: BarMode,
) -> PlotResult<()> {
    require_non_empty(vals, "bar")?;
    if cats.len() != vals.len() {
        return Err(PlotError::LengthMismatch(format!(
            "bar: category count {} != value count {}",
            cats.len(),
            vals.len()
        )));
    }
    ax.push_series(Series {
        label: label.map(str::to_string),
        kind: SeriesKind::Bar {
            cats: cats.to_vec(),
            vals: vals.to_vec(),
            mode,
        },
    });
    Ok(())
}

pub fn draw_hbar(
    ax: &mut Axes,
    cats: &[String],
    vals: &[f64],
    label: Option<&str>,
) -> PlotResult<()> {
    require_non_empty(vals, "hbar")?;
    if cats.len() != vals.len() {
        return Err(PlotError::LengthMismatch(format!(
            "hbar: category count {} != value count {}",
            cats.len(),
            vals.len()
        )));
    }
    ax.push_series(Series {
        label: label.map(str::to_string),
        kind: SeriesKind::HBar {
            cats: cats.to_vec(),
            vals: vals.to_vec(),
        },
    });
    Ok(())
}

pub fn draw_hist(ax: &mut Axes, data: &[f64], bins: usize, label: Option<&str>) -> PlotResult<()> {
    require_non_empty(data, "hist")?;
    let bins = bins.max(1);
    let lim = autoscale(data)?;
    let step = lim.span() / bins as f64;
    let mut edges = vec![lim.min; bins + 1];
    for i in 1..=bins {
        edges[i] = lim.min + step * i as f64;
    }
    let mut counts = vec![0.0; bins];
    for &v in data {
        if !v.is_finite() {
            continue;
        }
        let mut b = ((v - lim.min) / step) as usize;
        if b >= bins {
            b = bins - 1;
        }
        counts[b] += 1.0;
    }
    ax.push_series(Series {
        label: label.map(str::to_string),
        kind: SeriesKind::Hist { counts, edges },
    });
    Ok(())
}

pub fn draw_box(ax: &mut Axes, groups: &[(&str, &[f64])], label: Option<&str>) -> PlotResult<()> {
    if groups.is_empty() {
        return Err(PlotError::Empty("box: no groups".into()));
    }
    let mut out = Vec::new();
    for (name, data) in groups {
        out.push((name.to_string(), crate::figure::box_stats(data)?));
    }
    ax.push_series(Series {
        label: label.map(str::to_string),
        kind: SeriesKind::Box { groups: out },
    });
    Ok(())
}

pub fn draw_heatmap(
    ax: &mut Axes,
    data: &[f64],
    rows: usize,
    cols: usize,
    label: Option<&str>,
) -> PlotResult<()> {
    if data.is_empty() || rows == 0 || cols == 0 || data.len() != rows * cols {
        return Err(PlotError::LengthMismatch(format!(
            "heatmap: data len {} != rows*cols {rows}*{cols}",
            data.len()
        )));
    }
    ax.push_series(Series {
        label: label.map(str::to_string),
        kind: SeriesKind::Heatmap {
            data: data.to_vec(),
            rows,
            cols,
        },
    });
    Ok(())
}

pub fn draw_confusion_matrix(
    ax: &mut Axes,
    cm: &[f64],
    n: usize,
    labels: &[String],
    title: Option<&str>,
) -> PlotResult<()> {
    if cm.len() != n * n {
        return Err(PlotError::LengthMismatch(format!(
            "confusion_matrix: cm len {} != n*n ({n}*{n})",
            cm.len()
        )));
    }
    if let Some(t) = title {
        ax.set_title(t);
    }
    ax.push_series(Series {
        label: None,
        kind: SeriesKind::ConfusionMatrix {
            cm: cm.to_vec(),
            n,
            labels: labels.to_vec(),
        },
    });
    Ok(())
}

pub fn draw_roc(ax: &mut Axes, fpr: &[f64], tpr: &[f64], label: Option<&str>) -> PlotResult<()> {
    require_non_empty(fpr, "roc")?;
    require_same_len(fpr, tpr, "roc")?;
    ax.push_series(Series {
        label: label.map(str::to_string),
        kind: SeriesKind::Roc {
            fpr: fpr.to_vec(),
            tpr: tpr.to_vec(),
        },
    });
    Ok(())
}

pub fn draw_errorbar(
    ax: &mut Axes,
    x: &[f64],
    y: &[f64],
    yerr: &[f64],
    label: Option<&str>,
) -> PlotResult<()> {
    require_non_empty(x, "errorbar")?;
    require_same_len(x, y, "errorbar")?;
    require_same_len(y, yerr, "errorbar")?;
    ax.push_series(Series {
        label: label.map(str::to_string),
        kind: SeriesKind::ErrorBar {
            x: x.to_vec(),
            y: y.to_vec(),
            yerr: yerr.to_vec(),
        },
    });
    Ok(())
}

pub fn draw_step(ax: &mut Axes, x: &[f64], y: &[f64], label: Option<&str>) -> PlotResult<()> {
    require_non_empty(x, "step")?;
    require_same_len(x, y, "step")?;
    ax.push_series(Series {
        label: label.map(str::to_string),
        kind: SeriesKind::Step {
            x: x.to_vec(),
            y: y.to_vec(),
        },
    });
    Ok(())
}

pub fn draw_area(ax: &mut Axes, x: &[f64], y: &[f64], label: Option<&str>) -> PlotResult<()> {
    require_non_empty(x, "area")?;
    require_same_len(x, y, "area")?;
    ax.push_series(Series {
        label: label.map(str::to_string),
        kind: SeriesKind::Area {
            x: x.to_vec(),
            y: y.to_vec(),
        },
    });
    Ok(())
}

pub fn draw_pie(
    ax: &mut Axes,
    labels: &[String],
    vals: &[f64],
    title: Option<&str>,
) -> PlotResult<()> {
    require_non_empty(vals, "pie")?;
    if labels.len() != vals.len() {
        return Err(PlotError::LengthMismatch(format!(
            "pie: labels {} != vals {}",
            labels.len(),
            vals.len()
        )));
    }
    if let Some(t) = title {
        ax.set_title(t);
    }
    ax.push_series(Series {
        label: None,
        kind: SeriesKind::Pie {
            labels: labels.to_vec(),
            vals: vals.to_vec(),
        },
    });
    Ok(())
}

fn render_line(
    scene: &mut crate::scene::Scene,
    x: &[f64],
    y: &[f64],
    xtr: &Transform,
    ytr: &Transform,
    color: Rgba,
    step: bool,
) {
    let n = x.len().min(y.len());
    if n == 0 {
        return;
    }
    let idx = downsample_indices(n);
    let mut pts: Vec<(f64, f64)> = Vec::with_capacity(idx.len() * 2);
    for &i in &idx {
        let px = xtr.data_to_px_x(x[i]);
        let py = ytr.data_to_px_y(y[i]);
        if step && !pts.is_empty() {
            let last_y = pts.last().unwrap().1;
            pts.push((px, last_y));
        }
        pts.push((px, py));
    }
    scene.push(Element::Polyline {
        points: pts,
        stroke: color,
        stroke_width: 1.5,
        fill: None,
    });
}

fn render_scatter(
    scene: &mut crate::scene::Scene,
    x: &[f64],
    y: &[f64],
    xtr: &Transform,
    ytr: &Transform,
    color: Rgba,
) {
    let n = x.len().min(y.len());
    let idx = downsample_indices(n);
    for &i in &idx {
        scene.push(Element::Circle {
            cx: xtr.data_to_px_x(x[i]),
            cy: ytr.data_to_px_y(y[i]),
            r: 2.5,
            fill: color.with_alpha(0.8),
            stroke: None,
        });
    }
}

fn render_bar(
    scene: &mut crate::scene::Scene,
    vals: &[f64],
    xtr: &Transform,
    ytr: &Transform,
    color: Rgba,
    mode: BarMode,
) {
    let n = vals.len();
    if n == 0 {
        return;
    }
    let bar_w = xtr.plot_width / n as f64 * 0.7;
    let gap = xtr.plot_width / n as f64 * 0.3;
    let mut stack = 0.0;
    for (i, &v) in vals.iter().enumerate() {
        let x0 = xtr.plot_left + i as f64 * (bar_w + gap) + gap / 2.0;
        let y_base = match mode {
            BarMode::Grouped => ytr.data_to_px_y(0.0),
            BarMode::Stacked => ytr.data_to_px_y(stack),
        };
        let y_top = match mode {
            BarMode::Grouped => ytr.data_to_px_y(v),
            BarMode::Stacked => ytr.data_to_px_y(stack + v),
        };
        if mode == BarMode::Stacked {
            stack += v;
        }
        let h = (y_base - y_top).abs();
        let y = y_top.min(y_base);
        scene.push(Element::Rect {
            x: x0,
            y,
            w: bar_w,
            h,
            fill: Some(color),
            stroke: None,
            stroke_width: 0.0,
        });
    }
}

fn render_hbar(
    scene: &mut crate::scene::Scene,
    vals: &[f64],
    xtr: &Transform,
    ytr: &Transform,
    color: Rgba,
) {
    let n = vals.len();
    let bar_h = ytr.plot_height / n as f64 * 0.7;
    let gap = ytr.plot_height / n as f64 * 0.3;
    for (i, &v) in vals.iter().enumerate() {
        let y0 = ytr.plot_top + i as f64 * (bar_h + gap) + gap / 2.0;
        let x0 = xtr.data_to_px_x(0.0);
        let x1 = xtr.data_to_px_x(v);
        scene.push(Element::Rect {
            x: x0.min(x1),
            y: y0,
            w: (x1 - x0).abs(),
            h: bar_h,
            fill: Some(color),
            stroke: None,
            stroke_width: 0.0,
        });
    }
}

fn render_hist(
    scene: &mut crate::scene::Scene,
    counts: &[f64],
    edges: &[f64],
    xtr: &Transform,
    ytr: &Transform,
    color: Rgba,
) {
    for (i, &cnt) in counts.iter().enumerate() {
        if i + 1 >= edges.len() {
            break;
        }
        let x0 = xtr.data_to_px_x(edges[i]);
        let x1 = xtr.data_to_px_x(edges[i + 1]);
        let y0 = ytr.data_to_px_y(0.0);
        let y1 = ytr.data_to_px_y(cnt);
        scene.push(Element::Rect {
            x: x0.min(x1),
            y: y1.min(y0),
            w: (x1 - x0).abs(),
            h: (y1 - y0).abs(),
            fill: Some(color.with_alpha(0.85)),
            stroke: Some(Rgba::new(255, 255, 255, 1.0)),
            stroke_width: 0.5,
        });
    }
}

fn render_box(
    scene: &mut crate::scene::Scene,
    groups: &[(String, BoxStats)],
    xtr: &Transform,
    ytr: &Transform,
    color: Rgba,
) {
    let n = groups.len();
    let w = xtr.plot_width / n.max(1) as f64 * 0.4;
    for (i, (_, s)) in groups.iter().enumerate() {
        let cx = xtr.data_to_px_x(i as f64 + 0.5);
        let y_q1 = ytr.data_to_px_y(s.q1);
        let y_q3 = ytr.data_to_px_y(s.q3);
        let y_med = ytr.data_to_px_y(s.med);
        scene.push(Element::Rect {
            x: cx - w / 2.0,
            y: y_q3.min(y_q1),
            w,
            h: (y_q1 - y_q3).abs(),
            fill: Some(color.with_alpha(0.3)),
            stroke: Some(color),
            stroke_width: 1.0,
        });
        scene.push(Element::Line {
            x1: cx - w / 2.0,
            y1: y_med,
            x2: cx + w / 2.0,
            y2: y_med,
            stroke: color,
            stroke_width: 1.5,
            dash: None,
        });
        for &(yv, dash) in &[(s.min, true), (s.max, true)] {
            let py = ytr.data_to_px_y(yv);
            scene.push(Element::Line {
                x1: cx,
                y1: py,
                x2: cx,
                y2: if dash { y_q1 } else { y_q3 },
                stroke: color,
                stroke_width: 1.0,
                dash: None,
            });
        }
    }
}

fn render_heatmap(
    scene: &mut crate::scene::Scene,
    data: &[f64],
    rows: usize,
    cols: usize,
    xtr: &Transform,
    ytr: &Transform,
) {
    let max = data.iter().cloned().fold(0.0f64, f64::max).max(1e-12);
    let cw = xtr.plot_width / cols as f64;
    let ch = ytr.plot_height / rows as f64;
    for r in 0..rows {
        for c in 0..cols {
            let v = data[r * cols + c];
            let t = v / max;
            let col = sequential(t);
            scene.push(Element::Rect {
                x: xtr.plot_left + c as f64 * cw,
                y: ytr.plot_top + r as f64 * ch,
                w: cw,
                h: ch,
                fill: Some(col),
                stroke: None,
                stroke_width: 0.0,
            });
        }
    }
}

fn render_confusion(
    scene: &mut crate::scene::Scene,
    cm: &[f64],
    n: usize,
    labels: &[String],
    xtr: &Transform,
    ytr: &Transform,
) {
    let max = cm.iter().cloned().fold(0.0f64, f64::max).max(1.0);
    let cw = xtr.plot_width / n as f64;
    let ch = ytr.plot_height / n as f64;
    for r in 0..n {
        for c in 0..n {
            let v = cm[r * n + c];
            let t = v / max;
            let col = sequential(t);
            let x = xtr.plot_left + c as f64 * cw;
            let y = ytr.plot_top + r as f64 * ch;
            scene.push(Element::Rect {
                x,
                y,
                w: cw,
                h: ch,
                fill: Some(col),
                stroke: Some(Rgba::new(255, 255, 255, 1.0)),
                stroke_width: 0.5,
            });
            scene.push(Element::Text {
                x: x + cw / 2.0,
                y: y + ch / 2.0 + 3.0,
                content: format!("{v:.0}"),
                fill: Rgba::new(20, 20, 30, 1.0),
                anchor: TextAnchor::Middle,
                size: 9.0,
            });
        }
    }
    for (i, lbl) in labels.iter().enumerate().take(n) {
        let px = xtr.plot_left + (i as f64 + 0.5) * cw;
        scene.push(Element::Text {
            x: px,
            y: ytr.plot_top + ytr.plot_height + 14.0,
            content: lbl.clone(),
            fill: Rgba::new(50, 50, 60, 1.0),
            anchor: TextAnchor::Middle,
            size: 8.0,
        });
    }
}

fn render_errorbar(
    scene: &mut crate::scene::Scene,
    x: &[f64],
    y: &[f64],
    yerr: &[f64],
    xtr: &Transform,
    ytr: &Transform,
    color: Rgba,
) {
    let n = x.len().min(y.len()).min(yerr.len());
    for i in 0..n {
        let px = xtr.data_to_px_x(x[i]);
        let py = ytr.data_to_px_y(y[i]);
        let py_lo = ytr.data_to_px_y(y[i] - yerr[i]);
        let py_hi = ytr.data_to_px_y(y[i] + yerr[i]);
        scene.push(Element::Line {
            x1: px,
            y1: py_lo,
            x2: px,
            y2: py_hi,
            stroke: color,
            stroke_width: 1.0,
            dash: None,
        });
        scene.push(Element::Line {
            x1: px - 3.0,
            y1: py_lo,
            x2: px + 3.0,
            y2: py_lo,
            stroke: color,
            stroke_width: 1.0,
            dash: None,
        });
        scene.push(Element::Line {
            x1: px - 3.0,
            y1: py_hi,
            x2: px + 3.0,
            y2: py_hi,
            stroke: color,
            stroke_width: 1.0,
            dash: None,
        });
        scene.push(Element::Circle {
            cx: px,
            cy: py,
            r: 3.0,
            fill: color,
            stroke: None,
        });
    }
}

fn render_area(
    scene: &mut crate::scene::Scene,
    x: &[f64],
    y: &[f64],
    xtr: &Transform,
    ytr: &Transform,
    color: Rgba,
) {
    let n = x.len().min(y.len());
    if n == 0 {
        return;
    }
    let y0 = ytr.data_to_px_y(0.0);
    let mut d = String::with_capacity(n * 24);
    d.push_str(&format!("M {} {}", xtr.data_to_px_x(x[0]), y0));
    for i in 0..n {
        d.push_str(&format!(
            " L {} {}",
            xtr.data_to_px_x(x[i]),
            ytr.data_to_px_y(y[i])
        ));
    }
    d.push_str(&format!(" L {} {} Z", xtr.data_to_px_x(x[n - 1]), y0));
    scene.push(Element::Path {
        d,
        fill: Some(color.with_alpha(0.35)),
        stroke: Some(color),
        stroke_width: 1.0,
    });
}

fn render_pie(
    scene: &mut crate::scene::Scene,
    labels: &[String],
    vals: &[f64],
    xtr: &Transform,
    ytr: &Transform,
) {
    let cx = xtr.plot_left + xtr.plot_width / 2.0;
    let cy = ytr.plot_top + ytr.plot_height / 2.0;
    let r = xtr.plot_width.min(ytr.plot_height) * 0.35;
    let sum: f64 = vals.iter().sum();
    if sum <= 0.0 {
        return;
    }
    let mut angle = -std::f64::consts::FRAC_PI_2;
    for (i, &v) in vals.iter().enumerate() {
        let sweep = v / sum * std::f64::consts::TAU;
        let x1 = cx + r * angle.cos();
        let y1 = cy + r * angle.sin();
        angle += sweep;
        let x2 = cx + r * angle.cos();
        let y2 = cy + r * angle.sin();
        let large = if sweep > std::f64::consts::PI { 1 } else { 0 };
        let d = format!("M {cx} {cy} L {x1:.4} {y1:.4} A {r} {r} 0 {large} 1 {x2:.4} {y2:.4} Z");
        scene.push(Element::Path {
            d,
            fill: Some(categorical(i)),
            stroke: Some(Rgba::new(255, 255, 255, 1.0)),
            stroke_width: 1.0,
        });
        if i < labels.len() {
            let mid = angle - sweep / 2.0;
            scene.push(Element::Text {
                x: cx + r * 0.6 * mid.cos(),
                y: cy + r * 0.6 * mid.sin(),
                content: labels[i].clone(),
                fill: Rgba::new(30, 30, 40, 1.0),
                anchor: TextAnchor::Middle,
                size: 8.0,
            });
        }
    }
}
