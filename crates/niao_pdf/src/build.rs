//! PDF creation: pages, text, images, shapes, tables.

use crate::error::{PdfError, PdfResult};
use crate::table::{draw_table, TableOpts};
use image::io::Reader as ImageReader;
use printpdf::{
    BuiltinFont, Color, Image, ImageTransform, Line, Mm, PdfDocument, PdfDocumentReference,
    PdfLayerReference, PdfPageIndex, Point, Rgb,
};
use std::collections::HashMap;
use std::io::{BufWriter, Cursor};

/// Default US Letter size in points.
pub const DEFAULT_PAGE_WIDTH: f32 = 612.0;
pub const DEFAULT_PAGE_HEIGHT: f32 = 792.0;

/// Creation options for a new PDF builder.
#[derive(Debug, Clone)]
pub struct CreateOpts {
    pub page_width: f32,
    pub page_height: f32,
    pub margin: f32,
    pub title: String,
}

impl Default for CreateOpts {
    fn default() -> Self {
        Self {
            page_width: DEFAULT_PAGE_WIDTH,
            page_height: DEFAULT_PAGE_HEIGHT,
            margin: 72.0,
            title: "Niao PDF".into(),
        }
    }
}

/// Text draw options (coordinates in points from bottom-left).
#[derive(Debug, Clone)]
pub struct TextOpts {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub font: BuiltinFontChoice,
    pub color: (f32, f32, f32),
}

impl Default for TextOpts {
    fn default() -> Self {
        Self {
            x: 72.0,
            y: 720.0,
            size: 12.0,
            font: BuiltinFontChoice::Helvetica,
            color: (0.0, 0.0, 0.0),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum BuiltinFontChoice {
    #[default]
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    HelveticaBoldOblique,
    Times,
    TimesBold,
    TimesItalic,
    TimesBoldItalic,
    Courier,
    CourierBold,
    CourierOblique,
    CourierBoldOblique,
}

impl BuiltinFontChoice {
    fn to_builtin(self) -> BuiltinFont {
        match self {
            Self::Helvetica => BuiltinFont::Helvetica,
            Self::HelveticaBold => BuiltinFont::HelveticaBold,
            Self::HelveticaOblique => BuiltinFont::HelveticaOblique,
            Self::HelveticaBoldOblique => BuiltinFont::HelveticaBoldOblique,
            Self::Times => BuiltinFont::TimesRoman,
            Self::TimesBold => BuiltinFont::TimesBold,
            Self::TimesItalic => BuiltinFont::TimesItalic,
            Self::TimesBoldItalic => BuiltinFont::TimesBoldItalic,
            Self::Courier => BuiltinFont::Courier,
            Self::CourierBold => BuiltinFont::CourierBold,
            Self::CourierOblique => BuiltinFont::CourierOblique,
            Self::CourierBoldOblique => BuiltinFont::CourierBoldOblique,
        }
    }
}

/// Image placement options.
#[derive(Debug, Clone)]
pub struct ImageOpts {
    pub x: f32,
    pub y: f32,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub scale: f32,
}

impl Default for ImageOpts {
    fn default() -> Self {
        Self {
            x: 72.0,
            y: 400.0,
            width: None,
            height: None,
            scale: 1.0,
        }
    }
}

/// Line draw options.
#[derive(Debug, Clone)]
pub struct LineOpts {
    pub width: f32,
    pub color: (f32, f32, f32),
}

impl Default for LineOpts {
    fn default() -> Self {
        Self {
            width: 1.0,
            color: (0.0, 0.0, 0.0),
        }
    }
}

/// Rectangle draw options.
#[derive(Debug, Clone)]
pub struct RectOpts {
    pub fill: Option<(f32, f32, f32)>,
    pub stroke: Option<(f32, f32, f32)>,
    pub stroke_width: f32,
}

impl Default for RectOpts {
    fn default() -> Self {
        Self {
            fill: None,
            stroke: Some((0.0, 0.0, 0.0)),
            stroke_width: 1.0,
        }
    }
}

pub(crate) struct BuilderState {
    pub page_width: f32,
    doc: PdfDocumentReference,
    page: Option<(PdfPageIndex, printpdf::PdfLayerIndex)>,
    page_height: f32,
    margin: f32,
    fonts: HashMap<BuiltinFontChoice, printpdf::IndirectFontRef>,
}

/// Builder store for in-progress PDF documents.
pub struct BuilderStore {
    next_id: i64,
    builders: HashMap<i64, BuilderState>,
}

impl BuilderStore {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            builders: HashMap::new(),
        }
    }

    fn get_mut(&mut self, id: i64) -> PdfResult<&mut BuilderState> {
        self.builders.get_mut(&id).ok_or(PdfError::InvalidHandle)
    }

    pub fn remove(&mut self, id: i64) -> bool {
        self.builders.remove(&id).is_some()
    }
}

impl Default for BuilderStore {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn pt_to_mm(v: f32) -> Mm {
    Mm(v * 25.4 / 72.0)
}

pub(crate) fn rgb(c: (f32, f32, f32)) -> Color {
    Color::Rgb(Rgb::new(c.0, c.1, c.2, None))
}

pub(crate) fn current_layer(state: &BuilderState) -> PdfResult<PdfLayerReference> {
    let (page_idx, layer_idx) = state
        .page
        .ok_or_else(|| PdfError::Build("no active page — call add_page first".into()))?;
    Ok(state.doc.get_page(page_idx).get_layer(layer_idx))
}

pub(crate) fn ensure_font(
    state: &mut BuilderState,
    choice: BuiltinFontChoice,
) -> PdfResult<printpdf::IndirectFontRef> {
    if let Some(font) = state.fonts.get(&choice) {
        return Ok(font.clone());
    }
    let font = state
        .doc
        .add_builtin_font(choice.to_builtin())
        .map_err(|e| PdfError::Build(e.to_string()))?;
    state.fonts.insert(choice, font.clone());
    Ok(font)
}

/// Create a new PDF builder handle.
pub fn create_builder(store: &mut BuilderStore, opts: &CreateOpts) -> PdfResult<i64> {
    if opts.page_width <= 0.0 || opts.page_height <= 0.0 {
        return Err(PdfError::InvalidInput(
            "page dimensions must be positive".into(),
        ));
    }
    let (doc, page_idx, layer_idx) = PdfDocument::new(
        opts.title.clone(),
        pt_to_mm(opts.page_width),
        pt_to_mm(opts.page_height),
        "Layer 1",
    );
    let id = store.next_id;
    store.next_id += 1;
    store.builders.insert(
        id,
        BuilderState {
            doc,
            page: Some((page_idx, layer_idx)),
            page_width: opts.page_width,
            page_height: opts.page_height,
            margin: opts.margin,
            fonts: HashMap::new(),
        },
    );
    Ok(id)
}

/// Release a builder without producing output.
pub fn close_builder(store: &mut BuilderStore, id: i64) -> PdfResult<()> {
    if store.remove(id) {
        Ok(())
    } else {
        Err(PdfError::InvalidHandle)
    }
}

/// Append a page.
pub fn add_page(store: &mut BuilderStore, id: i64, _opts: Option<CreateOpts>) -> PdfResult<()> {
    let state = store.get_mut(id)?;
    let page_num = if state.page.is_some() { 2 } else { 1 };
    let (page_idx, layer_idx) = state.doc.add_page(
        pt_to_mm(state.page_width),
        pt_to_mm(state.page_height),
        format!("Page {page_num}"),
    );
    state.page = Some((page_idx, layer_idx));
    Ok(())
}

/// Draw text on the active page.
pub fn text(store: &mut BuilderStore, id: i64, content: &str, opts: &TextOpts) -> PdfResult<()> {
    let font_choice = opts.font;
    let state = store.get_mut(id)?;
    let font = ensure_font(state, font_choice)?;
    let layer = current_layer(state)?;
    layer.set_fill_color(rgb(opts.color));
    layer.use_text(
        content,
        opts.size,
        pt_to_mm(opts.x),
        pt_to_mm(opts.y),
        &font,
    );
    Ok(())
}

/// Draw an image from encoded bytes (PNG/JPEG/GIF/WebP).
pub fn image(store: &mut BuilderStore, id: i64, data: &[u8], opts: &ImageOpts) -> PdfResult<()> {
    let reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| PdfError::Build(e.to_string()))?;
    let dyn_img = reader
        .decode()
        .map_err(|e| PdfError::Build(e.to_string()))?;
    let (iw, ih) = (dyn_img.width() as f32, dyn_img.height() as f32);
    let target_w = opts.width.unwrap_or(iw * opts.scale);
    let target_h = opts.height.unwrap_or(ih * opts.scale);
    let state = store.get_mut(id)?;
    let layer = current_layer(state)?;
    let image = Image::from_dynamic_image(&dyn_img);
    image.add_to_layer(
        layer,
        ImageTransform {
            translate_x: Some(pt_to_mm(opts.x)),
            translate_y: Some(pt_to_mm(opts.y)),
            rotate: None,
            scale_x: Some(target_w / iw),
            scale_y: Some(target_h / ih),
            dpi: Some(300.0),
        },
    );
    Ok(())
}

/// Draw a line segment.
pub fn line(
    store: &mut BuilderStore,
    id: i64,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    opts: &LineOpts,
) -> PdfResult<()> {
    let state = store.get_mut(id)?;
    let layer = current_layer(state)?;
    layer.set_outline_color(rgb(opts.color));
    layer.set_outline_thickness(opts.width);
    let points = vec![
        (Point::new(pt_to_mm(x1), pt_to_mm(y1)), false),
        (Point::new(pt_to_mm(x2), pt_to_mm(y2)), false),
    ];
    layer.add_line(Line {
        points,
        is_closed: false,
    });
    Ok(())
}

/// Draw a rectangle with optional fill and stroke.
pub fn rect(
    store: &mut BuilderStore,
    id: i64,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    opts: &RectOpts,
) -> PdfResult<()> {
    let state = store.get_mut(id)?;
    let layer = current_layer(state)?;
    if let Some(fill) = opts.fill {
        layer.set_fill_color(rgb(fill));
    }
    if let Some(stroke) = opts.stroke {
        layer.set_outline_color(rgb(stroke));
        layer.set_outline_thickness(opts.stroke_width);
    }
    let points = vec![
        (Point::new(pt_to_mm(x), pt_to_mm(y)), false),
        (Point::new(pt_to_mm(x + w), pt_to_mm(y)), false),
        (Point::new(pt_to_mm(x + w), pt_to_mm(y + h)), false),
        (Point::new(pt_to_mm(x), pt_to_mm(y + h)), false),
    ];
    layer.add_line(Line {
        points,
        is_closed: true,
    });
    Ok(())
}

/// Draw a table from rows of cell strings.
pub fn table(
    store: &mut BuilderStore,
    id: i64,
    rows: &[Vec<String>],
    opts: &TableOpts,
) -> PdfResult<()> {
    let state = store.get_mut(id)?;
    draw_table(state, rows, opts)
}

/// Finish builder and return PDF bytes.
pub fn finish_builder(store: &mut BuilderStore, id: i64) -> PdfResult<Vec<u8>> {
    let state = store.builders.remove(&id).ok_or(PdfError::InvalidHandle)?;
    let mut buf = BufWriter::new(Vec::new());
    state
        .doc
        .save(&mut buf)
        .map_err(|e| PdfError::Build(e.to_string()))?;
    Ok(buf
        .into_inner()
        .map_err(|e| PdfError::Build(e.to_string()))?)
}

/// Write builder output directly to a path.
pub fn write_builder(store: &mut BuilderStore, id: i64, path: &std::path::Path) -> PdfResult<()> {
    let bytes = finish_builder(store, id)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::{is_valid, open_bytes, page_count, DocumentStore};

    #[test]
    fn create_minimal_pdf() {
        let mut builders = BuilderStore::new();
        let b = create_builder(&mut builders, &CreateOpts::default()).unwrap();
        text(&mut builders, b, "Hello", &TextOpts::default()).unwrap();
        let bytes = finish_builder(&mut builders, b).unwrap();
        assert!(is_valid(&bytes));
        let mut store = DocumentStore::new();
        let id = open_bytes(&mut store, &bytes).unwrap();
        assert_eq!(page_count(&store, id).unwrap(), 1);
    }
}
