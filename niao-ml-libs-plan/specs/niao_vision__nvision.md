# Library spec: `nvision`  →  crate `niao_vision`

| | |
|---|---|
| Category | Computer vision |
| Replaces (Python) | `torchvision` + `OpenCV` (core) + `Pillow` |
| Rust reference | `image`, `imageproc` |
| Target Niao crate | `crates/niao_vision` |
| Niao import name | `nvision` |
| Difficulty | 4/5 — Very Hard |
| Wave | 2 (needs nnum, niao_tensor, niao_ml) |
| Depends on Niao libs | `nnum`, `niao_tensor`, `niao_ml`, `ncodec` (image decode/encode) |
| Error block | 4090–4099 |

## Goal
Image IO, transforms, augmentation, classical CV ops, and dataset loaders — the torchvision layer that feeds
`niao_ml`. **Zero external deps.** Image decode/encode via `ncodec`; tensors via `niao_tensor`; conv/pooling
building blocks via `niao_ml` (do not re-implement autograd conv).

## Scope (v1)
- **Image type + IO:** `Image` (H×W×C, u8 or f32; gray/RGB/RGBA) backed by an `nnum`/`niao_tensor` buffer;
  `imread`/`imwrite` for PNG/JPEG/BMP via `ncodec`; `to_tensor`/`from_tensor` (CHW, normalized), `to_frame` (pixels).
- **Geometric transforms:** resize (nearest/bilinear/bicubic), crop / center_crop / random_crop, pad,
  flip (h/v), rotate, affine/warp, perspective. torchvision-compatible `Compose([...])` pipeline.
- **Photometric / tensor transforms:** `ToTensor`, `Normalize(mean, std)`, `ColorJitter` (brightness/contrast/
  saturation/hue), grayscale, gaussian blur, `RandomResizedCrop`, `RandomHorizontalFlip`, `RandomErasing`, cutout.
- **Classical CV ops:** convolution/correlation, Sobel/Scharr gradients, Gaussian/box/median filters,
  Canny edges, threshold (binary/Otsu/adaptive), morphology (erode/dilate/open/close), histogram + equalization,
  color conversions (RGB↔gray↔HSV↔YCbCr), integral image, connected components, resize pyramids.
- **Feature-ish (lightweight):** Harris corners, HOG descriptor, template matching (v1 modest; SIFT/ORB = v2).
- **Datasets / loaders:** `MNIST`, `FashionMNIST`, `CIFAR-10` (parse the standard binary formats),
  `ImageFolder` (class-per-directory), a `DataLoader` (batch/shuffle/collate → `niao_tensor`, delegate batching to
  `niao_ml`'s dataloader where possible). No network downloads — load from a local path.
- **Conv building blocks:** thin wrappers exposing `niao_ml` conv2d/pool/batchnorm so vision models compose in Niao.
  **Do not** ship pretrained ResNet/ViT weights in v1 (that's `niao_ml_models`).

## Implementation blueprint (make it FAST + LIGHT)
- Images are contiguous `niao_tensor`/`nnum` buffers (HWC for IO, CHW for model input) — no `Vec<Vec<..>>`.
- Resize/warp: precompute source coordinates + interpolation weights once per output row; separable kernels for
  Gaussian/box (two 1-D passes, not one 2-D). Convolution reuses `niao_tensor` where it maps to GEMM/im2col.
- Augmentations are lazy transform objects composed in a pipeline; RNG from `nrand` with a seed for reproducible aug.
- Otsu/adaptive threshold and histogram equalization from the image histogram (single pass).
- Dataset parsers read the canonical MNIST/CIFAR byte layouts directly; ImageFolder walks dirs and lazy-decodes.

### Performance rules
- No per-pixel allocation; operate on rows/planes. Separable filters, precomputed resize weights, reused buffers.
- `#[inline]` the inner filter/interpolation kernels; SIMD the per-pixel math with scalar fallback. Batch decode+
  transform in the loader across threads (bounded pool).

## Public API surface
`Image`, `imread/imwrite`, transforms (`resize/crop/flip/rotate/normalize/color_jitter/...`) + `Compose`,
classical ops (`convolve/sobel/canny/threshold/morphology/hist_eq/cvt_color`), `datasets::{MNIST,CIFAR10,ImageFolder}`,
`DataLoader`, `to_tensor/from_tensor`. Expose to Niao via `niao_libs/nvision/` + builtins.

## Performance target
- Transform correctness vs torchvision/PIL fixtures (resize/normalize/flip) within `rtol=1e-4` (interpolation
  differences allowed, documented).
- A 1000-image load→resize→normalize pass within **2×** of torchvision wall-clock.

## Tests required
- `imread`→`imwrite`→`imread` round-trip (lossless PNG) pixel-exact; JPEG within a tolerance.
- Resize (bilinear/bicubic) vs PIL/torchvision fixtures on a known image, `rtol=1e-4`; flip/rotate/crop exact.
- `Normalize`/`ToTensor` output matches torchvision (CHW, scaled, mean/std) `rtol=1e-6`.
- Sobel/Gaussian/Canny/threshold vs reference fixtures on a known image (edge maps compared with a tolerance).
- MNIST/CIFAR parser loads the expected number of samples with correct labels + shapes from a local fixture subset.
- `DataLoader` yields correctly-shaped, correctly-batched tensors; seeded shuffle is reproducible.
- Degenerate: missing file → 4095; decode failure → 4093; shape/channel mismatch → 4094.
- Plus: in-crate unit tests, `examples/nvision_demo.niao`, `benchmarks/benchmark_nvision.py` vs torchvision.

## Risk / notes
- **Reuse, don't rebuild:** codecs (`ncodec`), conv/pool/autograd (`niao_ml`), tensors (`niao_tensor`).
- Interpolation will never be bit-identical to PIL/OpenCV — pick one convention (align_corners documented), test
  with tolerance, and state it.
- SIFT/ORB, optical flow, video, and pretrained backbones are explicit v2.
- Datasets load from local paths only — no downloader in v1 (avoids network + licensing in tests).

## Done criteria
- `cargo check --workspace` and `cargo test -p niao_vision` green; torchvision/PIL fixtures pass in tolerance.
- `niao_libs/nvision/` wrapper + `examples/nvision_demo.niao` loads an image, transforms it, makes a tensor.
- Benchmark + notes in `REPORT.md`; `CHANGELOG.md` updated; shared-file edits reported, not applied.
