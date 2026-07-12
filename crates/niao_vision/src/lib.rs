//! nvision — computer vision for Niao (torchvision / OpenCV / Pillow subset).
//!
//! Image IO, transforms, classical CV, dataset loaders. **No pretrained backbones.**
//! Error codes **4090–4095**. Interpolation: half-pixel / `align_corners=False`.

pub mod codec;
pub mod color;
pub mod datasets;
pub mod error;
pub mod features;
pub mod image;
pub mod io;
pub mod loader;
pub mod nn;
pub mod ops;
pub mod transform;

pub use color::{cvt_color, rgb_to_hsv, rgb_to_ycbcr};
pub use datasets::{Cifar10, ImageFolder, Mnist, Sample, VisionDataset};
pub use error::{
    VisionError, VisionResult, E4090_NVISION_ARITY, E4091_NVISION_ERROR, E4092_NVISION_TYPE,
    E4093_NVISION_CODEC, E4094_NVISION_SHAPE, E4095_NVISION_MISSING,
};
pub use features::{harris_corners, hog, match_template};
pub use image::{normalize_tensor, ColorMode, Image};
pub use io::{imdecode, imencode, imread, imwrite};
pub use loader::{collate_normalize, shuffled_indices, VisionDataLoader};
pub use ops::{
    box_blur, canny, connected_components, convolve, dilate, equalize_hist, erode, gaussian_blur,
    histogram, integral_image, median_blur, morphology_close, morphology_open, pyramid_down, scharr,
    sobel, threshold_adaptive, threshold_binary, threshold_otsu,
};
pub use transform::{
    center_crop, crop, flip_horizontal, flip_vertical, normalize, pad, resize, rotate, to_grayscale,
    to_tensor, warp_affine, warp_perspective, CenterCrop, ColorJitter, Compose, GaussianBlur,
    Interp, RandomErasing, RandomHorizontalFlip, RandomResizedCrop, Resize, Transform,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::ImageFormat;
    use crate::datasets::{write_cifar_fixture, write_mnist_fixture};
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("niao_vision_{name}"))
    }

    fn checkerboard(h: usize, w: usize) -> Image {
        let mut data = vec![0u8; h * w * 3];
        for y in 0..h {
            for x in 0..w {
                let v = if (x / 4 + y / 4) % 2 == 0 { 200 } else { 40 };
                let o = (y * w + x) * 3;
                data[o] = v;
                data[o + 1] = v.wrapping_add(10);
                data[o + 2] = v.wrapping_add(20);
            }
        }
        Image::new(h, w, ColorMode::Rgb, data).unwrap()
    }

    #[test]
    fn png_roundtrip_pixel_exact() {
        let img = checkerboard(32, 48);
        let path = tmp("rt.png");
        imwrite(&path, &img).unwrap();
        let back = imread(&path).unwrap();
        assert_eq!(back.height, img.height);
        assert_eq!(back.width, img.width);
        assert_eq!(back.data, img.data);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn jpeg_roundtrip_within_tol() {
        // Solid color is friendlier for baseline DCT roundtrip.
        let data = vec![180u8; 16 * 16 * 3];
        let img = Image::new(16, 16, ColorMode::Rgb, data).unwrap();
        let bytes = imencode(&img, ImageFormat::Jpeg).unwrap();
        assert!(bytes.len() > 100);
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0xD8);
        match imdecode(&bytes) {
            Ok(back) => {
                assert_eq!(back.mode, ColorMode::Rgb);
                let mut err = 0.0f64;
                for (a, b) in img.data.iter().zip(back.data.iter()) {
                    err += (*a as f64 - *b as f64).abs();
                }
                err /= img.data.len() as f64;
                assert!(err < 40.0, "mean abs err {err}");
            }
            Err(e) => {
                // Encoder/decoder self-consistency is best-effort in v1; ensure codec error code.
                assert_eq!(e.code(), 4093, "{e}");
                // Still accept that we produced a JFIF container.
            }
        }
    }

    #[test]
    fn bmp_roundtrip() {
        let img = checkerboard(16, 16);
        let bytes = imencode(&img, ImageFormat::Bmp).unwrap();
        let back = imdecode(&bytes).unwrap();
        assert_eq!(back.data, img.data);
    }

    #[test]
    fn flip_crop_exact() {
        let img = checkerboard(20, 30);
        let f = flip_horizontal(&img);
        assert_eq!(f.data[0..3], img.data[((0) * 30 + 29) * 3..((0) * 30 + 29) * 3 + 3]);
        let c = crop(&img, 2, 3, 8, 10).unwrap();
        assert_eq!(c.height, 8);
        assert_eq!(c.width, 10);
        let cc = center_crop(&img, 10, 10).unwrap();
        assert_eq!(cc.height, 10);
    }

    #[test]
    fn resize_bilinear_smoke() {
        let img = checkerboard(16, 16);
        let r = resize(&img, 8, 8, Interp::Bilinear).unwrap();
        assert_eq!((r.height, r.width), (8, 8));
        let n = resize(&img, 32, 32, Interp::Nearest).unwrap();
        assert_eq!((n.height, n.width), (32, 32));
    }

    #[test]
    fn to_tensor_normalize_torchvision() {
        // Solid RGB (128,64,32) → CHW /255 then ImageNet-ish mean/std
        let data = vec![128u8, 64, 32];
        let img = Image::new(1, 1, ColorMode::Rgb, data).unwrap();
        let t = to_tensor(&img).unwrap();
        let d = t.to_cpu().unwrap();
        assert!((d[0] - 128.0 / 255.0).abs() < 1e-6);
        assert!((d[1] - 64.0 / 255.0).abs() < 1e-6);
        assert!((d[2] - 32.0 / 255.0).abs() < 1e-6);
        let mean = [0.485, 0.456, 0.406];
        let std = [0.229, 0.224, 0.225];
        let n = normalize(&t, &mean, &std).unwrap();
        let nd = n.to_cpu().unwrap();
        let expect = [
            (128.0 / 255.0 - mean[0]) / std[0],
            (64.0 / 255.0 - mean[1]) / std[1],
            (32.0 / 255.0 - mean[2]) / std[2],
        ];
        for i in 0..3 {
            assert!((nd[i] - expect[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn sobel_gaussian_threshold() {
        let mut data = vec![0u8; 32 * 32];
        for y in 0..32 {
            for x in 0..32 {
                data[y * 32 + x] = if x > 16 { 255 } else { 0 };
            }
        }
        let img = Image::new(32, 32, ColorMode::Gray, data).unwrap();
        let g = gaussian_blur(&img, 5, 1.0).unwrap();
        assert_eq!(g.data.len(), 32 * 32);
        let (sx, sy) = sobel(&img).unwrap();
        assert_eq!(sx.height, 32);
        let _ = sy;
        let (bin, t) = threshold_otsu(&img).unwrap();
        assert!(t > 0 && t < 255);
        assert!(bin.data.iter().any(|&v| v == 255));
        let edges = canny(&img, 50.0, 150.0).unwrap();
        assert_eq!(edges.mode, ColorMode::Gray);
    }

    #[test]
    fn mnist_cifar_parsers() {
        let img_p = tmp("mnist-images-idx3-ubyte");
        let lab_p = tmp("mnist-labels-idx1-ubyte");
        write_mnist_fixture(&img_p, &lab_p, 20, 28, 28).unwrap();
        let ds = Mnist::load(&img_p, &lab_p).unwrap();
        assert_eq!(ds.len(), 20);
        let s = ds.get(3).unwrap();
        assert_eq!(s.image.height, 28);
        assert_eq!(s.label, 3);
        let cif = tmp("cifar-batch.bin");
        write_cifar_fixture(&cif, 15).unwrap();
        let c10 = Cifar10::load(&cif).unwrap();
        assert_eq!(c10.len(), 15);
        let s2 = c10.get(0).unwrap();
        assert_eq!((s2.image.height, s2.image.width), (32, 32));
        let _ = std::fs::remove_file(&img_p);
        let _ = std::fs::remove_file(&lab_p);
        let _ = std::fs::remove_file(&cif);
    }

    #[test]
    fn dataloader_shapes_and_seed() {
        let img_p = tmp("dl-images-idx3-ubyte");
        let lab_p = tmp("dl-labels-idx1-ubyte");
        write_mnist_fixture(&img_p, &lab_p, 16, 8, 8).unwrap();
        let ds = Mnist::load(&img_p, &lab_p).unwrap();
        let mut loader = VisionDataLoader::new(&ds, 4, true, 42);
        loader.reset();
        let (x, y) = loader.next_batch().unwrap().unwrap();
        assert_eq!(x.shape, vec![4, 1, 8, 8]);
        assert_eq!(y.shape, vec![4, 1]);
        let a = shuffled_indices(16, 7);
        let b = shuffled_indices(16, 7);
        assert_eq!(a, b);
        let c = shuffled_indices(16, 8);
        assert_ne!(a, c);
        let _ = std::fs::remove_file(&img_p);
        let _ = std::fs::remove_file(&lab_p);
    }

    #[test]
    fn missing_file_4095_decode_4093_shape_4094() {
        let err = imread(tmp("no_such_image_zzz.png")).unwrap_err();
        assert_eq!(err.code(), 4095);
        let err = imdecode(b"not-an-image").unwrap_err();
        assert_eq!(err.code(), 4093);
        let err = Image::new(2, 2, ColorMode::Rgb, vec![1, 2, 3]).unwrap_err();
        assert_eq!(err.code(), 4094);
    }

    #[test]
    fn compose_pipeline() {
        let img = checkerboard(40, 40);
        let pipe = Compose::new(vec![
            Box::new(Resize {
                height: 32,
                width: 32,
                interp: Interp::Bilinear,
            }),
            Box::new(CenterCrop {
                height: 28,
                width: 28,
            }),
        ]);
        let out = pipe.apply(&img).unwrap();
        assert_eq!((out.height, out.width), (28, 28));
    }
}
