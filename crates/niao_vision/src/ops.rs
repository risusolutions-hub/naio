//! Classical CV ops: filters, edges, threshold, morphology, histogram.

use crate::error::{VisionError, VisionResult};
use crate::image::{ColorMode, Image};
use crate::transform::to_grayscale;

pub fn convolve(img: &Image, kernel: &[f32], ksize: usize) -> VisionResult<Image> {
    if ksize % 2 == 0 || kernel.len() != ksize * ksize {
        return Err(VisionError::Shape("kernel must be odd square".into()));
    }
    let gray = to_grayscale(img)?;
    let r = (ksize / 2) as isize;
    let mut out = vec![0u8; gray.data.len()];
    for y in 0..gray.height as isize {
        for x in 0..gray.width as isize {
            let mut acc = 0.0f32;
            for ky in -r..=r {
                for kx in -r..=r {
                    let yy = (y + ky).clamp(0, gray.height as isize - 1) as usize;
                    let xx = (x + kx).clamp(0, gray.width as isize - 1) as usize;
                    let kv = kernel[((ky + r) as usize) * ksize + (kx + r) as usize];
                    acc += gray.data[yy * gray.width + xx] as f32 * kv;
                }
            }
            out[(y as usize) * gray.width + x as usize] = acc.round().clamp(0.0, 255.0) as u8;
        }
    }
    Image::new(gray.height, gray.width, ColorMode::Gray, out)
}

/// Separable 1-D convolution (horizontal then vertical).
pub fn convolve_separable(img: &Image, kernel_1d: &[f32]) -> VisionResult<Image> {
    let gray = to_grayscale(img)?;
    let k = kernel_1d.len();
    if k % 2 == 0 {
        return Err(VisionError::Shape("separable kernel must be odd".into()));
    }
    let r = (k / 2) as isize;
    let mut tmp = vec![0.0f32; gray.data.len()];
    let mut out = vec![0u8; gray.data.len()];
    for y in 0..gray.height {
        for x in 0..gray.width as isize {
            let mut acc = 0.0f32;
            for i in -r..=r {
                let xx = (x + i).clamp(0, gray.width as isize - 1) as usize;
                acc += gray.data[y * gray.width + xx] as f32 * kernel_1d[(i + r) as usize];
            }
            tmp[y * gray.width + x as usize] = acc;
        }
    }
    for y in 0..gray.height as isize {
        for x in 0..gray.width {
            let mut acc = 0.0f32;
            for i in -r..=r {
                let yy = (y + i).clamp(0, gray.height as isize - 1) as usize;
                acc += tmp[yy * gray.width + x] * kernel_1d[(i + r) as usize];
            }
            out[(y as usize) * gray.width + x] = acc.round().clamp(0.0, 255.0) as u8;
        }
    }
    Image::new(gray.height, gray.width, ColorMode::Gray, out)
}

pub fn gaussian_kernel_1d(ksize: usize, sigma: f32) -> VisionResult<Vec<f32>> {
    if ksize % 2 == 0 || ksize == 0 {
        return Err(VisionError::Shape("gaussian ksize must be odd".into()));
    }
    let r = (ksize / 2) as isize;
    let mut k = vec![0.0f32; ksize];
    let s2 = 2.0 * sigma * sigma;
    let mut sum = 0.0f32;
    for i in -r..=r {
        let v = (-((i * i) as f32) / s2).exp();
        k[(i + r) as usize] = v;
        sum += v;
    }
    for v in &mut k {
        *v /= sum;
    }
    Ok(k)
}

pub fn gaussian_blur(img: &Image, ksize: usize, sigma: f32) -> VisionResult<Image> {
    let k = gaussian_kernel_1d(ksize, sigma)?;
    // Apply per-channel for color
    if img.mode == ColorMode::Gray {
        return convolve_separable(img, &k);
    }
    let c = img.channels();
    let mut planes = vec![vec![0u8; img.height * img.width]; c.min(3)];
    for i in 0..img.height * img.width {
        for ch in 0..planes.len() {
            planes[ch][i] = img.data[i * c + ch];
        }
    }
    let mut out_planes = Vec::new();
    for p in &planes {
        let plane = Image::new(img.height, img.width, ColorMode::Gray, p.clone())?;
        out_planes.push(convolve_separable(&plane, &k)?);
    }
    let mut data = vec![0u8; img.data.len()];
    for i in 0..img.height * img.width {
        for ch in 0..out_planes.len() {
            data[i * c + ch] = out_planes[ch].data[i];
        }
        if c == 4 {
            data[i * c + 3] = img.data[i * c + 3];
        }
    }
    Image::new(img.height, img.width, img.mode, data)
}

pub fn box_blur(img: &Image, ksize: usize) -> VisionResult<Image> {
    let k = vec![1.0 / ksize as f32; ksize];
    convolve_separable(img, &k)
}

pub fn median_blur(img: &Image, ksize: usize) -> VisionResult<Image> {
    if ksize % 2 == 0 {
        return Err(VisionError::Shape("median ksize must be odd".into()));
    }
    let gray = to_grayscale(img)?;
    let r = (ksize / 2) as isize;
    let mut out = vec![0u8; gray.data.len()];
    let mut window = vec![0u8; ksize * ksize];
    for y in 0..gray.height as isize {
        for x in 0..gray.width as isize {
            let mut n = 0;
            for ky in -r..=r {
                for kx in -r..=r {
                    let yy = (y + ky).clamp(0, gray.height as isize - 1) as usize;
                    let xx = (x + kx).clamp(0, gray.width as isize - 1) as usize;
                    window[n] = gray.data[yy * gray.width + xx];
                    n += 1;
                }
            }
            window[..n].sort_unstable();
            out[(y as usize) * gray.width + x as usize] = window[n / 2];
        }
    }
    Image::new(gray.height, gray.width, ColorMode::Gray, out)
}

pub fn sobel(img: &Image) -> VisionResult<(Image, Image)> {
    let kx = [-1.0f32, 0.0, 1.0, -2.0, 0.0, 2.0, -1.0, 0.0, 1.0];
    let ky = [-1.0f32, -2.0, -1.0, 0.0, 0.0, 0.0, 1.0, 2.0, 1.0];
    Ok((convolve(img, &kx, 3)?, convolve(img, &ky, 3)?))
}

pub fn scharr(img: &Image) -> VisionResult<(Image, Image)> {
    let kx = [-3.0f32, 0.0, 3.0, -10.0, 0.0, 10.0, -3.0, 0.0, 3.0];
    let ky = [-3.0f32, -10.0, -3.0, 0.0, 0.0, 0.0, 3.0, 10.0, 3.0];
    Ok((convolve(img, &kx, 3)?, convolve(img, &ky, 3)?))
}

/// Canny edge detector (simplified: Gaussian → Sobel → NMS → double threshold + weak hysteresis).
pub fn canny(img: &Image, low: f32, high: f32) -> VisionResult<Image> {
    let blur = gaussian_blur(img, 5, 1.4)?;
    let gray = to_grayscale(&blur)?;
    let (gx_img, gy_img) = sobel(&gray)?;
    let n = gray.data.len();
    let mut mag = vec![0.0f32; n];
    let mut ang = vec![0.0f32; n];
    for i in 0..n {
        let gx = gx_img.data[i] as f32 - 128.0; // convolve clamps — use raw-ish
        let gy = gy_img.data[i] as f32 - 128.0;
        // recompute properly from gray
        let _ = (gx, gy);
    }
    // Proper sobel on float
    let w = gray.width;
    let h = gray.height;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let mut sx = 0.0f32;
            let mut sy = 0.0f32;
            for ky in -1..=1 {
                for kx in -1..=1 {
                    let v = gray.data[((y as isize + ky) as usize) * w + (x as isize + kx) as usize]
                        as f32;
                    let ix = (ky + 1) * 3 + (kx + 1);
                    const KX: [f32; 9] = [-1.0, 0.0, 1.0, -2.0, 0.0, 2.0, -1.0, 0.0, 1.0];
                    const KY: [f32; 9] = [-1.0, -2.0, -1.0, 0.0, 0.0, 0.0, 1.0, 2.0, 1.0];
                    sx += v * KX[ix as usize];
                    sy += v * KY[ix as usize];
                }
            }
            let i = y * w + x;
            mag[i] = (sx * sx + sy * sy).sqrt();
            ang[i] = sy.atan2(sx);
        }
    }
    // NMS
    let mut nms = vec![0.0f32; n];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let a = ang[i].to_degrees().rem_euclid(180.0);
            let (d1, d2) = if (0.0..22.5).contains(&a) || (157.5..180.0).contains(&a) {
                (i - 1, i + 1)
            } else if (22.5..67.5).contains(&a) {
                (i - w + 1, i + w - 1)
            } else if (67.5..112.5).contains(&a) {
                (i - w, i + w)
            } else {
                (i - w - 1, i + w + 1)
            };
            if mag[i] >= mag[d1] && mag[i] >= mag[d2] {
                nms[i] = mag[i];
            }
        }
    }
    let mut out = vec![0u8; n];
    for i in 0..n {
        if nms[i] >= high {
            out[i] = 255;
        } else if nms[i] >= low {
            out[i] = 128; // weak
        }
    }
    // hysteresis
    let mut changed = true;
    while changed {
        changed = false;
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let i = y * w + x;
                if out[i] != 128 {
                    continue;
                }
                let mut strong = false;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if out[((y as isize + dy) as usize) * w + (x as isize + dx) as usize] == 255
                        {
                            strong = true;
                        }
                    }
                }
                if strong {
                    out[i] = 255;
                    changed = true;
                }
            }
        }
    }
    for v in &mut out {
        if *v == 128 {
            *v = 0;
        }
    }
    Image::new(h, w, ColorMode::Gray, out)
}

pub fn threshold_binary(img: &Image, thresh: u8, maxval: u8) -> VisionResult<Image> {
    let gray = to_grayscale(img)?;
    let data: Vec<u8> = gray
        .data
        .iter()
        .map(|&v| if v >= thresh { maxval } else { 0 })
        .collect();
    Image::new(gray.height, gray.width, ColorMode::Gray, data)
}

pub fn threshold_otsu(img: &Image) -> VisionResult<(Image, u8)> {
    let gray = to_grayscale(img)?;
    let mut hist = [0u32; 256];
    for &v in &gray.data {
        hist[v as usize] += 1;
    }
    let total = gray.data.len() as f64;
    let mut sum_all = 0.0f64;
    for i in 0..256 {
        sum_all += i as f64 * hist[i] as f64;
    }
    let mut sum_b = 0.0f64;
    let mut w_b = 0.0f64;
    let mut max_var = -1.0f64;
    let mut best = 0u8;
    for t in 0..256 {
        w_b += hist[t] as f64;
        if w_b == 0.0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f == 0.0 {
            break;
        }
        sum_b += t as f64 * hist[t] as f64;
        let m_b = sum_b / w_b;
        let m_f = (sum_all - sum_b) / w_f;
        let var = w_b * w_f * (m_b - m_f) * (m_b - m_f);
        if var >= max_var {
            max_var = var;
            best = t as u8;
        }
    }
    Ok((threshold_binary(&gray, best, 255)?, best))
}

pub fn threshold_adaptive(img: &Image, block: usize, c: i16) -> VisionResult<Image> {
    if block % 2 == 0 {
        return Err(VisionError::Shape("adaptive block must be odd".into()));
    }
    let gray = to_grayscale(img)?;
    let integral = integral_image(&gray)?;
    let r = (block / 2) as isize;
    let mut out = vec![0u8; gray.data.len()];
    let w = gray.width as isize;
    let h = gray.height as isize;
    for y in 0..h {
        for x in 0..w {
            let y0 = (y - r).max(0);
            let x0 = (x - r).max(0);
            let y1 = (y + r).min(h - 1);
            let x1 = (x + r).min(w - 1);
            let area = ((y1 - y0 + 1) * (x1 - x0 + 1)) as i32;
            let sum = rect_sum(&integral, gray.width, x0 as usize, y0 as usize, x1 as usize, y1 as usize);
            let mean = sum / area;
            let v = gray.data[(y as usize) * gray.width + x as usize] as i32;
            out[(y as usize) * gray.width + x as usize] =
                if v >= mean - c as i32 { 255 } else { 0 };
        }
    }
    Image::new(gray.height, gray.width, ColorMode::Gray, out)
}

pub fn integral_image(img: &Image) -> VisionResult<Vec<i32>> {
    let gray = to_grayscale(img)?;
    let w = gray.width;
    let h = gray.height;
    let mut integ = vec![0i32; (h + 1) * (w + 1)];
    for y in 0..h {
        let mut row = 0i32;
        for x in 0..w {
            row += gray.data[y * w + x] as i32;
            integ[(y + 1) * (w + 1) + (x + 1)] = integ[y * (w + 1) + (x + 1)] + row;
        }
    }
    Ok(integ)
}

fn rect_sum(integ: &[i32], w: usize, x0: usize, y0: usize, x1: usize, y1: usize) -> i32 {
    let stride = w + 1;
    integ[(y1 + 1) * stride + (x1 + 1)]
        - integ[y0 * stride + (x1 + 1)]
        - integ[(y1 + 1) * stride + x0]
        + integ[y0 * stride + x0]
}

pub fn erode(img: &Image, ksize: usize) -> VisionResult<Image> {
    morph(img, ksize, true)
}
pub fn dilate(img: &Image, ksize: usize) -> VisionResult<Image> {
    morph(img, ksize, false)
}
pub fn morphology_open(img: &Image, ksize: usize) -> VisionResult<Image> {
    dilate(&erode(img, ksize)?, ksize)
}
pub fn morphology_close(img: &Image, ksize: usize) -> VisionResult<Image> {
    erode(&dilate(img, ksize)?, ksize)
}

fn morph(img: &Image, ksize: usize, erode: bool) -> VisionResult<Image> {
    let gray = to_grayscale(img)?;
    let r = (ksize / 2) as isize;
    let mut out = vec![0u8; gray.data.len()];
    for y in 0..gray.height as isize {
        for x in 0..gray.width as isize {
            let mut best = if erode { 255u8 } else { 0u8 };
            for ky in -r..=r {
                for kx in -r..=r {
                    let yy = (y + ky).clamp(0, gray.height as isize - 1) as usize;
                    let xx = (x + kx).clamp(0, gray.width as isize - 1) as usize;
                    let v = gray.data[yy * gray.width + xx];
                    if erode {
                        best = best.min(v);
                    } else {
                        best = best.max(v);
                    }
                }
            }
            out[(y as usize) * gray.width + x as usize] = best;
        }
    }
    Image::new(gray.height, gray.width, ColorMode::Gray, out)
}

pub fn histogram(img: &Image) -> VisionResult<[u32; 256]> {
    let gray = to_grayscale(img)?;
    let mut h = [0u32; 256];
    for &v in &gray.data {
        h[v as usize] += 1;
    }
    Ok(h)
}

pub fn equalize_hist(img: &Image) -> VisionResult<Image> {
    let gray = to_grayscale(img)?;
    let hist = histogram(&gray)?;
    let total = gray.data.len() as f64;
    let mut cdf = [0u32; 256];
    cdf[0] = hist[0];
    for i in 1..256 {
        cdf[i] = cdf[i - 1] + hist[i];
    }
    let cdf_min = cdf.iter().copied().find(|&v| v > 0).unwrap_or(0);
    let mut lut = [0u8; 256];
    for i in 0..256 {
        if cdf[i] == 0 {
            lut[i] = 0;
        } else {
            lut[i] = (((cdf[i] - cdf_min) as f64 / (total - cdf_min as f64)) * 255.0).round() as u8;
        }
    }
    let data: Vec<u8> = gray.data.iter().map(|&v| lut[v as usize]).collect();
    Image::new(gray.height, gray.width, ColorMode::Gray, data)
}

/// Connected components (4-connectivity). Returns label image + count.
pub fn connected_components(img: &Image) -> VisionResult<(Image, u32)> {
    let bin = to_grayscale(img)?;
    let w = bin.width;
    let h = bin.height;
    let mut labels = vec![0u32; w * h];
    let mut parent: Vec<u32> = vec![0];
    let mut next_label = 1u32;

    fn find(parent: &mut [u32], mut x: u32) -> u32 {
        while parent[x as usize] != x {
            parent[x as usize] = parent[parent[x as usize] as usize];
            x = parent[x as usize];
        }
        x
    }
    fn union(parent: &mut [u32], a: u32, b: u32) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[rb as usize] = ra;
        }
    }

    for y in 0..h {
        for x in 0..w {
            if bin.data[y * w + x] < 128 {
                continue;
            }
            let mut neighbors = Vec::new();
            if x > 0 && labels[y * w + x - 1] > 0 {
                neighbors.push(labels[y * w + x - 1]);
            }
            if y > 0 && labels[(y - 1) * w + x] > 0 {
                neighbors.push(labels[(y - 1) * w + x]);
            }
            if neighbors.is_empty() {
                parent.push(next_label);
                labels[y * w + x] = next_label;
                next_label += 1;
            } else {
                let m = *neighbors.iter().min().unwrap();
                labels[y * w + x] = m;
                for &n in &neighbors {
                    union(&mut parent, m, n);
                }
            }
        }
    }
    let mut remap = vec![0u32; parent.len()];
    let mut count = 0u32;
    for i in 1..parent.len() {
        let r = find(&mut parent, i as u32);
        if remap[r as usize] == 0 {
            count += 1;
            remap[r as usize] = count;
        }
    }
    let mut out = vec![0u8; w * h];
    for i in 0..w * h {
        if labels[i] > 0 {
            let r = find(&mut parent, labels[i]);
            out[i] = (remap[r as usize] % 255) as u8;
            if out[i] == 0 {
                out[i] = 1;
            }
        }
    }
    Ok((Image::new(h, w, ColorMode::Gray, out)?, count))
}

pub fn pyramid_down(img: &Image) -> VisionResult<Image> {
    let blur = gaussian_blur(img, 5, 1.0)?;
    crate::transform::resize(
        &blur,
        (img.height / 2).max(1),
        (img.width / 2).max(1),
        crate::transform::Interp::Nearest,
    )
}
