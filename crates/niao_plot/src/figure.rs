//! Figure and Axes model with subplot grid.

use crate::axis::{autoscale, merge_limits, nice_ticks, Limits, Scale, Transform};
use crate::charts::{
    draw_area, draw_bar, draw_box, draw_confusion_matrix, draw_errorbar, draw_heatmap, draw_hist,
    draw_hbar, draw_line, draw_pie, draw_roc, draw_scatter, draw_step, BarMode, BoxStats, Series,
};
use crate::color::{categorical, Rgba};
use crate::error::{PlotError, PlotResult};
use crate::scene::{Element, Scene, TextAnchor};
use std::fs;
use std::path::Path;

const MARGIN_OUTER: f64 = 40.0;
const MARGIN_INNER: f64 = 48.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum XScale {
    Linear,
    Log,
}

#[derive(Clone, Debug)]
pub struct Axes {
    pub title: Option<String>,
    pub xlabel: Option<String>,
    pub ylabel: Option<String>,
    pub xlim: Option<Limits>,
    pub ylim: Option<Limits>,
    pub xscale: XScale,
    pub yscale: XScale,
    pub grid: bool,
    pub legend: Vec<String>,
    series: Vec<Series>,
}

impl Axes {
    pub fn new() -> Self {
        Self {
            title: None,
            xlabel: None,
            ylabel: None,
            xlim: None,
            ylim: None,
            xscale: XScale::Linear,
            yscale: XScale::Linear,
            grid: true,
            legend: Vec::new(),
            series: Vec::new(),
        }
    }

    pub fn set_title(&mut self, t: impl Into<String>) -> &mut Self {
        self.title = Some(t.into());
        self
    }

    pub fn set_xlabel(&mut self, t: impl Into<String>) -> &mut Self {
        self.xlabel = Some(t.into());
        self
    }

    pub fn set_ylabel(&mut self, t: impl Into<String>) -> &mut Self {
        self.ylabel = Some(t.into());
        self
    }

    pub fn set_xlim(&mut self, min: f64, max: f64) -> &mut Self {
        self.xlim = Some(Limits::new(min, max));
        self
    }

    pub fn set_ylim(&mut self, min: f64, max: f64) -> &mut Self {
        self.ylim = Some(Limits::new(min, max));
        self
    }

    pub fn set_xscale(&mut self, log: bool) -> &mut Self {
        self.xscale = if log { XScale::Log } else { XScale::Linear };
        self
    }

    pub fn set_yscale(&mut self, log: bool) -> &mut Self {
        self.yscale = if log { XScale::Log } else { XScale::Linear };
        self
    }

    pub fn set_grid(&mut self, on: bool) -> &mut Self {
        self.grid = on;
        self
    }

    pub fn line(&mut self, x: &[f64], y: &[f64], label: Option<&str>) -> PlotResult<&mut Self> {
        draw_line(self, x, y, label)?;
        Ok(self)
    }

    pub fn scatter(&mut self, x: &[f64], y: &[f64], label: Option<&str>) -> PlotResult<&mut Self> {
        draw_scatter(self, x, y, label)?;
        Ok(self)
    }

    pub fn bar(
        &mut self,
        cats: &[String],
        vals: &[f64],
        label: Option<&str>,
        mode: BarMode,
    ) -> PlotResult<&mut Self> {
        draw_bar(self, cats, vals, label, mode)?;
        Ok(self)
    }

    pub fn hbar(&mut self, cats: &[String], vals: &[f64], label: Option<&str>) -> PlotResult<&mut Self> {
        draw_hbar(self, cats, vals, label)?;
        Ok(self)
    }

    pub fn hist(&mut self, data: &[f64], bins: usize, label: Option<&str>) -> PlotResult<&mut Self> {
        draw_hist(self, data, bins, label)?;
        Ok(self)
    }

    pub fn box_plot(&mut self, groups: &[(&str, &[f64])], label: Option<&str>) -> PlotResult<&mut Self> {
        draw_box(self, groups, label)?;
        Ok(self)
    }

    pub fn heatmap(&mut self, data: &[f64], rows: usize, cols: usize, label: Option<&str>) -> PlotResult<&mut Self> {
        draw_heatmap(self, data, rows, cols, label)?;
        Ok(self)
    }

    pub fn confusion_matrix(
        &mut self,
        cm: &[f64],
        n: usize,
        labels: &[String],
        title: Option<&str>,
    ) -> PlotResult<&mut Self> {
        draw_confusion_matrix(self, cm, n, labels, title)?;
        Ok(self)
    }

    pub fn roc(&mut self, fpr: &[f64], tpr: &[f64], label: Option<&str>) -> PlotResult<&mut Self> {
        draw_roc(self, fpr, tpr, label)?;
        Ok(self)
    }

    pub fn errorbar(
        &mut self,
        x: &[f64],
        y: &[f64],
        yerr: &[f64],
        label: Option<&str>,
    ) -> PlotResult<&mut Self> {
        draw_errorbar(self, x, y, yerr, label)?;
        Ok(self)
    }

    pub fn step(&mut self, x: &[f64], y: &[f64], label: Option<&str>) -> PlotResult<&mut Self> {
        draw_step(self, x, y, label)?;
        Ok(self)
    }

    pub fn area(&mut self, x: &[f64], y: &[f64], label: Option<&str>) -> PlotResult<&mut Self> {
        draw_area(self, x, y, label)?;
        Ok(self)
    }

    pub fn pie(&mut self, labels: &[String], vals: &[f64], title: Option<&str>) -> PlotResult<&mut Self> {
        draw_pie(self, labels, vals, title)?;
        Ok(self)
    }

    pub(crate) fn push_series(&mut self, s: Series) {
        if let Some(l) = &s.label {
            self.legend.push(l.clone());
        }
        self.series.push(s);
    }

    pub(crate) fn render_into(&self, scene: &mut Scene, left: f64, top: f64, w: f64, h: f64) {
        let plot_left = left + MARGIN_INNER;
        let plot_top = top + MARGIN_INNER;
        let plot_w = w - 2.0 * MARGIN_INNER;
        let plot_h = h - 2.0 * MARGIN_INNER;

        scene.push(Element::Rect {
            x: left,
            y: top,
            w,
            h,
            fill: Some(Rgba::new(250, 250, 252, 1.0)),
            stroke: Some(Rgba::new(200, 200, 210, 1.0)),
            stroke_width: 1.0,
        });

        if let Some(t) = &self.title {
            scene.push(Element::Text {
                x: left + w / 2.0,
                y: top + 20.0,
                content: t.clone(),
                fill: Rgba::new(30, 30, 40, 1.0),
                anchor: TextAnchor::Middle,
                size: 14.0,
            });
        }

        let (xlim, ylim) = self.compute_limits();
        let xtr = Transform::new(
            xlim,
            match self.xscale {
                XScale::Linear => Scale::Linear,
                XScale::Log => Scale::Log,
            },
            plot_left,
            plot_top,
            plot_w,
            plot_h,
        );
        let ytr = Transform::new(
            ylim,
            match self.yscale {
                XScale::Linear => Scale::Linear,
                XScale::Log => Scale::Log,
            },
            plot_left,
            plot_top,
            plot_w,
            plot_h,
        );

        if self.grid {
            draw_grid(scene, &xtr, &ytr);
        }

        draw_frame(scene, plot_left, plot_top, plot_w, plot_h);

        for (i, s) in self.series.iter().enumerate() {
            s.render(scene, &xtr, &ytr, categorical(i));
        }

        draw_ticks(scene, &xtr, &ytr, true, false);
        draw_ticks(scene, &xtr, &ytr, false, true);

        if let Some(xl) = &self.xlabel {
            scene.push(Element::Text {
                x: plot_left + plot_w / 2.0,
                y: top + h - 8.0,
                content: xl.clone(),
                fill: Rgba::new(60, 60, 70, 1.0),
                anchor: TextAnchor::Middle,
                size: 11.0,
            });
        }
        if let Some(yl) = &self.ylabel {
            scene.push(Element::Text {
                x: left + 14.0,
                y: plot_top + plot_h / 2.0,
                content: yl.clone(),
                fill: Rgba::new(60, 60, 70, 1.0),
                anchor: TextAnchor::Middle,
                size: 11.0,
            });
        }

        if !self.legend.is_empty() {
            draw_legend(scene, left + w - 110.0, top + 30.0, &self.legend);
        }
    }

    fn compute_limits(&self) -> (Limits, Limits) {
        let mut xmin = f64::INFINITY;
        let mut xmax = f64::NEG_INFINITY;
        let mut ymin = f64::INFINITY;
        let mut ymax = f64::NEG_INFINITY;
        for s in &self.series {
            let (xl, yl) = s.data_bounds();
            xmin = xmin.min(xl.min);
            xmax = xmax.max(xl.max);
            ymin = ymin.min(yl.min);
            ymax = ymax.max(yl.max);
        }
        let mut xlim = if xmin.is_finite() {
            Limits::new(xmin, xmax)
        } else {
            Limits::new(0.0, 1.0)
        };
        let mut ylim = if ymin.is_finite() {
            Limits::new(ymin, ymax)
        } else {
            Limits::new(0.0, 1.0)
        };
        if let Some(x) = self.xlim {
            xlim = x;
        } else if let Ok(a) = autoscale(&[xlim.min, xlim.max]) {
            xlim = a;
        }
        if let Some(y) = self.ylim {
            ylim = y;
        } else if let Ok(a) = autoscale(&[ylim.min, ylim.max]) {
            ylim = a;
        }
        (xlim, ylim)
    }
}

impl Default for Axes {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Figure {
    pub width: f64,
    pub height: f64,
    pub rows: usize,
    pub cols: usize,
    pub axes: Vec<Axes>,
    pub suptitle: Option<String>,
}

impl Figure {
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            rows: 1,
            cols: 1,
            axes: vec![Axes::new()],
            suptitle: None,
        }
    }

    pub fn subplots(&mut self, rows: usize, cols: usize) -> PlotResult<&mut Self> {
        if rows == 0 || cols == 0 {
            return Err(PlotError::Render("subplots: rows and cols must be > 0".into()));
        }
        self.rows = rows;
        self.cols = cols;
        self.axes = (0..rows * cols).map(|_| Axes::new()).collect();
        Ok(self)
    }

    pub fn axes(&mut self, idx: usize) -> PlotResult<&mut Axes> {
        self.axes
            .get_mut(idx)
            .ok_or_else(|| PlotError::InvalidHandle(format!("axes index {idx} out of range")))
    }

    pub fn set_suptitle(&mut self, t: impl Into<String>) -> &mut Self {
        self.suptitle = Some(t.into());
        self
    }

    pub fn to_svg_string(&self) -> String {
        let n = self.axes.len().max(1);
        let mut scene = Scene::with_capacity(self.width, self.height, n * 200);
        if let Some(t) = &self.suptitle {
            scene.push(Element::Text {
                x: self.width / 2.0,
                y: 22.0,
                content: t.clone(),
                fill: Rgba::new(20, 20, 30, 1.0),
                anchor: TextAnchor::Middle,
                size: 16.0,
            });
        }
        let cell_w = (self.width - 2.0 * MARGIN_OUTER) / self.cols as f64;
        let cell_h = (self.height - 2.0 * MARGIN_OUTER) / self.rows as f64;
        let offset_y = if self.suptitle.is_some() { 24.0 } else { 0.0 };
        for (i, ax) in self.axes.iter().enumerate() {
            let r = i / self.cols;
            let c = i % self.cols;
            let left = MARGIN_OUTER + c as f64 * cell_w;
            let top = MARGIN_OUTER + offset_y + r as f64 * cell_h;
            ax.render_into(&mut scene, left, top, cell_w, cell_h);
        }
        scene.to_svg_string()
    }

    pub fn save_svg(&self, path: impl AsRef<Path>) -> PlotResult<()> {
        let p = path.as_ref();
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                return Err(PlotError::Render(format!(
                    "save_svg: parent directory does not exist: {}",
                    parent.display()
                )));
            }
        }
        let svg = self.to_svg_string();
        fs::write(p, svg).map_err(|e| PlotError::Render(format!("save_svg: {e}")))
    }
}

fn draw_frame(scene: &mut Scene, x: f64, y: f64, w: f64, h: f64) {
    scene.push(Element::Rect {
        x,
        y,
        w,
        h,
        fill: None,
        stroke: Some(Rgba::new(80, 80, 90, 1.0)),
        stroke_width: 1.0,
    });
}

fn draw_grid(scene: &mut Scene, xtr: &Transform, ytr: &Transform) {
    let grid_color = Rgba::new(220, 220, 230, 1.0);
    for &tx in &nice_ticks(xtr.limits.min, xtr.limits.max, 6) {
        let px = xtr.data_to_px_x(tx);
        scene.push(Element::Line {
            x1: px,
            y1: ytr.plot_top,
            x2: px,
            y2: ytr.plot_top + ytr.plot_height,
            stroke: grid_color,
            stroke_width: 0.5,
            dash: Some("2,2".into()),
        });
    }
    for &ty in &nice_ticks(ytr.limits.min, ytr.limits.max, 6) {
        let py = ytr.data_to_px_y(ty);
        scene.push(Element::Line {
            x1: xtr.plot_left,
            y1: py,
            x2: xtr.plot_left + xtr.plot_width,
            y2: py,
            stroke: grid_color,
            stroke_width: 0.5,
            dash: Some("2,2".into()),
        });
    }
}

fn draw_ticks(scene: &mut Scene, xtr: &Transform, ytr: &Transform, x_axis: bool, y_axis: bool) {
    let tick_color = Rgba::new(80, 80, 90, 1.0);
    if x_axis {
        for &tx in &nice_ticks(xtr.limits.min, xtr.limits.max, 5) {
            let px = xtr.data_to_px_x(tx);
            let py = ytr.plot_top + ytr.plot_height;
            scene.push(Element::Line {
                x1: px,
                y1: py,
                x2: px,
                y2: py + 4.0,
                stroke: tick_color,
                stroke_width: 1.0,
                dash: None,
            });
            scene.push(Element::Text {
                x: px,
                y: py + 16.0,
                content: format_tick(tx),
                fill: tick_color,
                anchor: TextAnchor::Middle,
                size: 9.0,
            });
        }
    }
    if y_axis {
        for &ty in &nice_ticks(ytr.limits.min, ytr.limits.max, 5) {
            let py = ytr.data_to_px_y(ty);
            let px = xtr.plot_left;
            scene.push(Element::Line {
                x1: px - 4.0,
                y1: py,
                x2: px,
                y2: py,
                stroke: tick_color,
                stroke_width: 1.0,
                dash: None,
            });
            scene.push(Element::Text {
                x: px - 8.0,
                y: py + 3.0,
                content: format_tick(ty),
                fill: tick_color,
                anchor: TextAnchor::End,
                size: 9.0,
            });
        }
    }
}

fn format_tick(v: f64) -> String {
    if v.abs() >= 1000.0 || (v.abs() < 0.01 && v != 0.0) {
        format!("{v:.2e}")
    } else {
        format!("{v:.2}")
    }
}

fn draw_legend(scene: &mut Scene, x: f64, y: f64, labels: &[String]) {
    scene.push(Element::Rect {
        x,
        y,
        w: 100.0,
        h: 14.0 * labels.len() as f64 + 8.0,
        fill: Some(Rgba::new(255, 255, 255, 0.9)),
        stroke: Some(Rgba::new(200, 200, 210, 1.0)),
        stroke_width: 0.5,
    });
    for (i, label) in labels.iter().enumerate() {
        let cy = y + 12.0 + i as f64 * 14.0;
        scene.push(Element::Line {
            x1: x + 8.0,
            y1: cy,
            x2: x + 24.0,
            y2: cy,
            stroke: categorical(i),
            stroke_width: 2.0,
            dash: None,
        });
        scene.push(Element::Text {
            x: x + 30.0,
            y: cy + 3.0,
            content: label.clone(),
            fill: Rgba::new(40, 40, 50, 1.0),
            anchor: TextAnchor::Start,
            size: 9.0,
        });
    }
}

pub fn box_stats(data: &[f64]) -> PlotResult<BoxStats> {
    if data.is_empty() {
        return Err(PlotError::Empty("box_stats: empty data".into()));
    }
    let mut v: Vec<f64> = data.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return Err(PlotError::Empty("box_stats: no finite values".into()));
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    let q1 = v[n / 4];
    let med = v[n / 2];
    let q3 = v[(3 * n) / 4];
    let min = v[0];
    let max = v[n - 1];
    Ok(BoxStats { min, q1, med, q3, max })
}

pub fn merge_series_limits(a: Limits, b: Limits) -> Limits {
    merge_limits(a, b)
}
