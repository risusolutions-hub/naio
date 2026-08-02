//! Cell and range styling.

use rust_xlsxwriter::{Color, Format, FormatAlign, FormatBorder};

#[derive(Debug, Clone, Default)]
pub struct CellStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub font_size: Option<f64>,
    pub font_color: Option<String>,
    pub bg_color: Option<String>,
    pub number_format: Option<String>,
    pub align: Option<String>,
    pub valign: Option<String>,
    pub wrap: bool,
    pub border: Option<String>,
}

impl CellStyle {
    pub fn to_format(&self) -> Result<Format, String> {
        let mut fmt = Format::new();
        if self.bold {
            fmt = fmt.set_bold();
        }
        if self.italic {
            fmt = fmt.set_italic();
        }
        if self.underline {
            fmt = fmt.set_underline(rust_xlsxwriter::FormatUnderline::Single);
        }
        if let Some(sz) = self.font_size {
            fmt = fmt.set_font_size(sz);
        }
        if let Some(ref c) = self.font_color {
            fmt = fmt.set_font_color(parse_color(c)?);
        }
        if let Some(ref c) = self.bg_color {
            fmt = fmt.set_background_color(parse_color(c)?);
        }
        if let Some(ref nf) = self.number_format {
            fmt = fmt.set_num_format(nf);
        }
        if let Some(ref a) = self.align {
            fmt = fmt.set_align(parse_halign(a)?);
        }
        if let Some(ref v) = self.valign {
            fmt = fmt.set_align(parse_valign(v)?);
        }
        if self.wrap {
            fmt = fmt.set_text_wrap();
        }
        if let Some(ref b) = self.border {
            let border = parse_border(b)?;
            fmt = fmt.set_border(border).set_border_color(Color::Black);
        }
        Ok(fmt)
    }
}

fn parse_color(s: &str) -> Result<Color, String> {
    let s = s.trim();
    if s.starts_with('#') && s.len() == 7 {
        let hex = u32::from_str_radix(&s[1..], 16).map_err(|e| e.to_string())?;
        return Ok(Color::RGB(hex));
    }
    match s.to_ascii_lowercase().as_str() {
        "black" => Ok(Color::Black),
        "white" => Ok(Color::White),
        "red" => Ok(Color::Red),
        "green" => Ok(Color::Green),
        "blue" => Ok(Color::Blue),
        "yellow" => Ok(Color::Yellow),
        "gray" | "grey" => Ok(Color::Gray),
        other => Err(format!("unknown color: {other}")),
    }
}

fn parse_halign(s: &str) -> Result<FormatAlign, String> {
    match s.to_ascii_lowercase().as_str() {
        "left" => Ok(FormatAlign::Left),
        "center" | "centre" => Ok(FormatAlign::Center),
        "right" => Ok(FormatAlign::Right),
        "justify" => Ok(FormatAlign::Justify),
        other => Err(format!("unknown horizontal align: {other}")),
    }
}

fn parse_valign(s: &str) -> Result<FormatAlign, String> {
    match s.to_ascii_lowercase().as_str() {
        "top" => Ok(FormatAlign::Top),
        "center" | "centre" | "vcenter" => Ok(FormatAlign::VerticalCenter),
        "bottom" => Ok(FormatAlign::Bottom),
        other => Err(format!("unknown vertical align: {other}")),
    }
}

fn parse_border(s: &str) -> Result<FormatBorder, String> {
    match s.to_ascii_lowercase().as_str() {
        "thin" => Ok(FormatBorder::Thin),
        "medium" => Ok(FormatBorder::Medium),
        "thick" => Ok(FormatBorder::Thick),
        "dashed" => Ok(FormatBorder::Dashed),
        "dotted" => Ok(FormatBorder::Dotted),
        "double" => Ok(FormatBorder::Double),
        "none" => Ok(FormatBorder::None),
        other => Err(format!("unknown border: {other}")),
    }
}
