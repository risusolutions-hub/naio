//! Fast float formatting for SVG output (fixed precision, no per-point format!).

#[inline]
pub fn write_f64(buf: &mut String, v: f64) {
    if v.is_nan() {
        buf.push_str("NaN");
        return;
    }
    if v.is_infinite() {
        if v.is_sign_positive() {
            buf.push_str("Inf");
        } else {
            buf.push_str("-Inf");
        }
        return;
    }
    // Fixed 4 decimal places — stable across platforms for golden tests.
    let rounded = (v * 10_000.0).round() / 10_000.0;
    let mut s = format!("{rounded:.4}");
    while s.contains('.') && (s.ends_with('0') || s.ends_with('.')) {
        s.pop();
    }
    buf.push_str(&s);
}
