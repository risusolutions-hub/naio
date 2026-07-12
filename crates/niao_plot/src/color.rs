//! Color palettes: tab10-like categorical + viridis-like sequential.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f64,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: f64) -> Self {
        Self { r, g, b, a }
    }

    pub fn to_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn with_alpha(&self, a: f64) -> Self {
        Self { a, ..*self }
    }
}

/// Matplotlib tab10-like categorical palette.
pub const TAB10: [Rgba; 10] = [
    Rgba::new(31, 119, 180, 1.0),
    Rgba::new(255, 127, 14, 1.0),
    Rgba::new(44, 160, 44, 1.0),
    Rgba::new(214, 39, 40, 1.0),
    Rgba::new(148, 103, 189, 1.0),
    Rgba::new(140, 86, 75, 1.0),
    Rgba::new(227, 119, 194, 1.0),
    Rgba::new(127, 127, 127, 1.0),
    Rgba::new(188, 189, 34, 1.0),
    Rgba::new(23, 190, 207, 1.0),
];

/// Simplified viridis ramp (11 steps).
pub const VIRIDIS: [Rgba; 11] = [
    Rgba::new(68, 1, 84, 1.0),
    Rgba::new(72, 40, 120, 1.0),
    Rgba::new(62, 74, 137, 1.0),
    Rgba::new(49, 104, 142, 1.0),
    Rgba::new(38, 130, 142, 1.0),
    Rgba::new(31, 158, 137, 1.0),
    Rgba::new(53, 183, 121, 1.0),
    Rgba::new(109, 205, 89, 1.0),
    Rgba::new(180, 222, 44, 1.0),
    Rgba::new(253, 231, 37, 1.0),
    Rgba::new(255, 255, 255, 1.0),
];

pub fn categorical(i: usize) -> Rgba {
    TAB10[i % TAB10.len()]
}

pub fn sequential(t: f64) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let idx = (t * (VIRIDIS.len() - 1) as f64).round() as usize;
    VIRIDIS[idx.min(VIRIDIS.len() - 1)]
}
