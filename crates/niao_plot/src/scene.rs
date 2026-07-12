//! In-memory SVG scene graph.

use crate::color::Rgba;
use crate::fmt::write_f64;

#[derive(Clone, Debug)]
pub enum Element {
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        fill: Option<Rgba>,
        stroke: Option<Rgba>,
        stroke_width: f64,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stroke: Rgba,
        stroke_width: f64,
        dash: Option<String>,
    },
    Polyline {
        points: Vec<(f64, f64)>,
        stroke: Rgba,
        stroke_width: f64,
        fill: Option<Rgba>,
    },
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        fill: Rgba,
        stroke: Option<Rgba>,
    },
    Text {
        x: f64,
        y: f64,
        content: String,
        fill: Rgba,
        anchor: TextAnchor,
        size: f64,
    },
    Path {
        d: String,
        fill: Option<Rgba>,
        stroke: Option<Rgba>,
        stroke_width: f64,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum TextAnchor {
    Start,
    Middle,
    End,
}

pub struct Scene {
    pub width: f64,
    pub height: f64,
    pub bg: Rgba,
    elements: Vec<Element>,
}

impl Scene {
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            bg: Rgba::new(255, 255, 255, 1.0),
            elements: Vec::new(),
        }
    }

    pub fn with_capacity(width: f64, height: f64, cap: usize) -> Self {
        Self {
            width,
            height,
            bg: Rgba::new(255, 255, 255, 1.0),
            elements: Vec::with_capacity(cap),
        }
    }

    pub fn push(&mut self, el: Element) {
        self.elements.push(el);
    }

    pub fn to_svg_string(&self) -> String {
        let est = 256 + self.elements.len() * 80;
        let mut out = String::with_capacity(est);
        out.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"");
        write_f64(&mut out, self.width);
        out.push_str("\" height=\"");
        write_f64(&mut out, self.height);
        out.push_str("\" viewBox=\"0 0 ");
        write_f64(&mut out, self.width);
        out.push(' ');
        write_f64(&mut out, self.height);
        out.push_str("\">");
        out.push_str("<rect width=\"100%\" height=\"100%\" fill=\"");
        out.push_str(&self.bg.to_hex());
        out.push_str("\"/>");
        for el in &self.elements {
            render_element(&mut out, el);
        }
        out.push_str("</svg>");
        out
    }
}

fn render_element(buf: &mut String, el: &Element) {
    match el {
        Element::Rect {
            x,
            y,
            w,
            h,
            fill,
            stroke,
            stroke_width,
        } => {
            buf.push_str("<rect x=\"");
            write_f64(buf, *x);
            buf.push_str("\" y=\"");
            write_f64(buf, *y);
            buf.push_str("\" width=\"");
            write_f64(buf, *w);
            buf.push_str("\" height=\"");
            write_f64(buf, *h);
            buf.push('"');
            if let Some(f) = fill {
                buf.push_str(" fill=\"");
                buf.push_str(&f.to_hex());
                buf.push('"');
            } else {
                buf.push_str(" fill=\"none\"");
            }
            if let Some(s) = stroke {
                buf.push_str(" stroke=\"");
                buf.push_str(&s.to_hex());
                buf.push_str("\" stroke-width=\"");
                write_f64(buf, *stroke_width);
                buf.push('"');
            }
            buf.push_str("/>");
        }
        Element::Line {
            x1,
            y1,
            x2,
            y2,
            stroke,
            stroke_width,
            dash,
        } => {
            buf.push_str("<line x1=\"");
            write_f64(buf, *x1);
            buf.push_str("\" y1=\"");
            write_f64(buf, *y1);
            buf.push_str("\" x2=\"");
            write_f64(buf, *x2);
            buf.push_str("\" y2=\"");
            write_f64(buf, *y2);
            buf.push_str("\" stroke=\"");
            buf.push_str(&stroke.to_hex());
            buf.push_str("\" stroke-width=\"");
            write_f64(buf, *stroke_width);
            buf.push('"');
            if let Some(d) = dash {
                buf.push_str(" stroke-dasharray=\"");
                buf.push_str(d);
                buf.push('"');
            }
            buf.push_str("/>");
        }
        Element::Polyline {
            points,
            stroke,
            stroke_width,
            fill,
        } => {
            buf.push_str("<polyline points=\"");
            for (i, (x, y)) in points.iter().enumerate() {
                if i > 0 {
                    buf.push(' ');
                }
                write_f64(buf, *x);
                buf.push(',');
                write_f64(buf, *y);
            }
            buf.push_str("\" stroke=\"");
            buf.push_str(&stroke.to_hex());
            buf.push_str("\" stroke-width=\"");
            write_f64(buf, *stroke_width);
            buf.push('"');
            if let Some(f) = fill {
                buf.push_str(" fill=\"");
                buf.push_str(&f.to_hex());
                buf.push('"');
            } else {
                buf.push_str(" fill=\"none\"");
            }
            buf.push_str("/>");
        }
        Element::Circle {
            cx,
            cy,
            r,
            fill,
            stroke,
        } => {
            buf.push_str("<circle cx=\"");
            write_f64(buf, *cx);
            buf.push_str("\" cy=\"");
            write_f64(buf, *cy);
            buf.push_str("\" r=\"");
            write_f64(buf, *r);
            buf.push_str("\" fill=\"");
            buf.push_str(&fill.to_hex());
            buf.push('"');
            if let Some(s) = stroke {
                buf.push_str(" stroke=\"");
                buf.push_str(&s.to_hex());
                buf.push('"');
            }
            buf.push_str("/>");
        }
        Element::Text {
            x,
            y,
            content,
            fill,
            anchor,
            size,
        } => {
            buf.push_str("<text x=\"");
            write_f64(buf, *x);
            buf.push_str("\" y=\"");
            write_f64(buf, *y);
            buf.push_str("\" fill=\"");
            buf.push_str(&fill.to_hex());
            buf.push_str("\" font-size=\"");
            write_f64(buf, *size);
            buf.push_str("\" text-anchor=\"");
            match anchor {
                TextAnchor::Start => buf.push_str("start"),
                TextAnchor::Middle => buf.push_str("middle"),
                TextAnchor::End => buf.push_str("end"),
            }
            buf.push_str("\" font-family=\"sans-serif\">");
            escape_text(buf, content);
            buf.push_str("</text>");
        }
        Element::Path {
            d,
            fill,
            stroke,
            stroke_width,
        } => {
            buf.push_str("<path d=\"");
            buf.push_str(d);
            buf.push('"');
            if let Some(f) = fill {
                buf.push_str(" fill=\"");
                buf.push_str(&f.to_hex());
                buf.push('"');
            } else {
                buf.push_str(" fill=\"none\"");
            }
            if let Some(s) = stroke {
                buf.push_str(" stroke=\"");
                buf.push_str(&s.to_hex());
                buf.push_str("\" stroke-width=\"");
                write_f64(buf, *stroke_width);
                buf.push('"');
            }
            buf.push_str("/>");
        }
    }
}

fn escape_text(buf: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => buf.push_str("&amp;"),
            '<' => buf.push_str("&lt;"),
            '>' => buf.push_str("&gt;"),
            '"' => buf.push_str("&quot;"),
            c => buf.push(c),
        }
    }
}

/// Normalize SVG for golden tests: collapse whitespace, fix float formatting.
pub fn normalize_svg(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
