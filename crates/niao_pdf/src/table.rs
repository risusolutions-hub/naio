//! Table layout and rendering for PDF builders.

use crate::build::{current_layer, ensure_font, pt_to_mm, rgb, BuilderState, BuiltinFontChoice};
use crate::error::{PdfError, PdfResult};
use printpdf::{Line, Point};

/// Table rendering options.
#[derive(Debug, Clone)]
pub struct TableOpts {
    pub x: f32,
    pub y: f32,
    pub col_widths: Option<Vec<f32>>,
    pub row_height: f32,
    pub font_size: f32,
    pub header: bool,
    pub border: bool,
    pub border_width: f32,
    pub padding: f32,
    pub header_fill: (f32, f32, f32),
    pub header_font: BuiltinFontChoice,
    pub body_font: BuiltinFontChoice,
}

impl Default for TableOpts {
    fn default() -> Self {
        Self {
            x: 72.0,
            y: 600.0,
            col_widths: None,
            row_height: 20.0,
            font_size: 10.0,
            header: true,
            border: true,
            border_width: 0.5,
            padding: 4.0,
            header_fill: (0.9, 0.9, 0.9),
            header_font: BuiltinFontChoice::HelveticaBold,
            body_font: BuiltinFontChoice::Helvetica,
        }
    }
}

pub(crate) fn draw_table(
    state: &mut BuilderState,
    rows: &[Vec<String>],
    opts: &TableOpts,
) -> PdfResult<()> {
    if rows.is_empty() {
        return Err(PdfError::InvalidInput(
            "table requires at least one row".into(),
        ));
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if cols == 0 {
        return Err(PdfError::InvalidInput(
            "table rows must have at least one column".into(),
        ));
    }
    let col_widths = match &opts.col_widths {
        Some(w) if w.len() == cols => w.clone(),
        Some(w) => {
            return Err(PdfError::InvalidInput(format!(
                "col_widths length {} does not match column count {cols}",
                w.len()
            )));
        }
        None => {
            let total = state.page_width - opts.x * 2.0;
            vec![total / cols as f32; cols]
        }
    };
    let table_width: f32 = col_widths.iter().sum();
    let table_height = opts.row_height * rows.len() as f32;
    let top_y = opts.y;
    let left_x = opts.x;

    if opts.border {
        let layer = current_layer(state)?;
        layer.set_outline_color(rgb((0.0, 0.0, 0.0)));
        layer.set_outline_thickness(opts.border_width);
        let outer = vec![
            (
                Point::new(pt_to_mm(left_x), pt_to_mm(top_y - table_height)),
                false,
            ),
            (
                Point::new(
                    pt_to_mm(left_x + table_width),
                    pt_to_mm(top_y - table_height),
                ),
                false,
            ),
            (
                Point::new(pt_to_mm(left_x + table_width), pt_to_mm(top_y)),
                false,
            ),
            (Point::new(pt_to_mm(left_x), pt_to_mm(top_y)), false),
        ];
        layer.add_line(Line {
            points: outer,
            is_closed: true,
        });
        let mut x = left_x;
        for w in &col_widths[..col_widths.len().saturating_sub(1)] {
            x += *w;
            let seg = vec![
                (
                    Point::new(pt_to_mm(x), pt_to_mm(top_y - table_height)),
                    false,
                ),
                (Point::new(pt_to_mm(x), pt_to_mm(top_y)), false),
            ];
            layer.add_line(Line {
                points: seg,
                is_closed: false,
            });
        }
        for row in 1..rows.len() {
            let y = top_y - opts.row_height * row as f32;
            let seg = vec![
                (Point::new(pt_to_mm(left_x), pt_to_mm(y)), false),
                (
                    Point::new(pt_to_mm(left_x + table_width), pt_to_mm(y)),
                    false,
                ),
            ];
            layer.add_line(Line {
                points: seg,
                is_closed: false,
            });
        }
    }

    for (row_idx, row) in rows.iter().enumerate() {
        let is_header = opts.header && row_idx == 0;
        let row_top = top_y - opts.row_height * row_idx as f32;
        let row_bottom = row_top - opts.row_height;
        if is_header {
            let layer = current_layer(state)?;
            layer.set_fill_color(rgb(opts.header_fill));
            let fill = vec![
                (Point::new(pt_to_mm(left_x), pt_to_mm(row_bottom)), false),
                (
                    Point::new(pt_to_mm(left_x + table_width), pt_to_mm(row_bottom)),
                    false,
                ),
                (
                    Point::new(pt_to_mm(left_x + table_width), pt_to_mm(row_top)),
                    false,
                ),
                (Point::new(pt_to_mm(left_x), pt_to_mm(row_top)), false),
            ];
            layer.add_line(Line {
                points: fill,
                is_closed: true,
            });
        }
        let mut cell_x = left_x;
        for (col_idx, cell) in row.iter().enumerate().take(cols) {
            let w = col_widths[col_idx];
            let font = if is_header {
                opts.header_font
            } else {
                opts.body_font
            };
            let font_ref = ensure_font(state, font)?;
            let layer = current_layer(state)?;
            layer.set_fill_color(rgb((0.0, 0.0, 0.0)));
            let text_x = cell_x + opts.padding;
            let text_y = row_bottom + opts.padding;
            layer.use_text(
                cell,
                opts.font_size,
                pt_to_mm(text_x),
                pt_to_mm(text_y),
                &font_ref,
            );
            cell_x += w;
        }
    }
    Ok(())
}
