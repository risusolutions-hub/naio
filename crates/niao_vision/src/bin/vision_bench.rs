//! Micro-benchmark: load→resize→normalize path (synthetic images).

use niao_vision::{normalize, resize, to_tensor, ColorMode, Image, Interp};
use std::time::Instant;

fn main() {
    let n = 1000usize;
    let mut images = Vec::with_capacity(n);
    for i in 0..n {
        let mut data = vec![0u8; 64 * 64 * 3];
        for (p, v) in data.iter_mut().enumerate() {
            *v = ((i * 13 + p) % 256) as u8;
        }
        images.push(Image::new(64, 64, ColorMode::Rgb, data).unwrap());
    }
    let mean = [0.485f32, 0.456, 0.406];
    let std = [0.229f32, 0.224, 0.225];

    let t0 = Instant::now();
    for img in &images {
        let r = resize(img, 32, 32, Interp::Bilinear).unwrap();
        let t = to_tensor(&r).unwrap();
        let _ = normalize(&t, &mean, &std).unwrap();
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("{ms:.4}");
}
