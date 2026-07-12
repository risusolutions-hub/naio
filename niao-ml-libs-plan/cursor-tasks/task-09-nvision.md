# Task 09 — nvision: torchvision / OpenCV / Pillow (crate `niao_vision`)
Wave 2 (needs nnum, niao_tensor, niao_ml). Read `../MASTER_PLAN.md` + `../specs/niao_vision__nvision.md`. Error block **4090–4099**.
Depends on: `nnum`, `niao_tensor`, `niao_ml`, `ncodec`. **Reuse: codecs=ncodec, conv/pool/autograd=niao_ml, tensors=niao_tensor.**

## Build (`crates/niao_vision`, zero new deps)
- `Image` (H×W×C u8/f32, gray/RGB/RGBA) over an nnum/niao_tensor buffer (contiguous, no Vec<Vec>). imread/imwrite PNG/JPEG/BMP
  via ncodec; to_tensor/from_tensor (CHW normalized); to_frame.
- Geometric: resize(nearest/bilinear/bicubic — precompute weights per row), crop/center_crop/random_crop, pad, flip(h/v),
  rotate, affine/warp, perspective. torchvision-style Compose([...]).
- Photometric/tensor: ToTensor, Normalize(mean,std), ColorJitter, grayscale, gaussian blur, RandomResizedCrop,
  RandomHorizontalFlip, RandomErasing/cutout (RNG via nrand, seeded/reproducible).
- Classical CV: convolve/correlate (separable kernels; im2col→niao_tensor GEMM where it maps), Sobel/Scharr, Gaussian/box/median,
  Canny, threshold(binary/Otsu/adaptive), morphology(erode/dilate/open/close), histogram+equalization, color cvt (RGB/gray/HSV/YCbCr),
  integral image, connected components. Lightweight features: Harris, HOG, template match (SIFT/ORB=v2).
- Datasets: MNIST/FashionMNIST/CIFAR-10 (parse canonical binary layouts), ImageFolder; DataLoader (batch/shuffle/collate→tensor,
  delegate batching to niao_ml dataloader). Local paths only, no downloader (v1). Conv building blocks = thin niao_ml wrappers.
- No per-pixel alloc (operate on rows/planes); SIMD per-pixel math with fallback; batch decode+transform across threads.

## Wire up
- `niao_libs/nvision/` wrapper + builtins; `docs/NVISION.md`; `examples/nvision_demo.niao` (load→transform→tensor).

## Acceptance
- imread→imwrite→imread lossless-PNG pixel-exact (JPEG within tol); resize/flip/rotate/crop vs PIL/torchvision fixtures rtol 1e-4;
  Normalize/ToTensor vs torchvision 1e-6; Sobel/Gaussian/Canny/threshold vs reference; MNIST/CIFAR parser correct count/labels/shape;
  DataLoader shapes+seeded shuffle reproducible.
- missing file→4095, decode fail→4093, shape/channel mismatch→4094. Document interpolation convention (won't be bit-identical to PIL).
- `benchmarks/benchmark_nvision.py` vs torchvision; 1000-image load→resize→normalize within 2x. `cargo test -p niao_vision` green.

See `../cursor-rules.md`.
