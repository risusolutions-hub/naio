# NVISION — Niao Computer Vision

Image IO, transforms, classical CV, and dataset loaders — a torchvision / OpenCV / Pillow subset for feeding `niao_ml`. **No pretrained backbones** (ResNet/ViT → v2 / `niao_ml_models`).

## Import

```niao
import "nvision"
```

Flat builtins (`nvision_imread`, etc.) are available after runtime wiring.

## Quick start

```niao
import "nvision"

fn main() {
    let img = nvision.imread("photo.png")
    let resized = nvision.resize(img, 224, 224)
    let t = nvision.to_tensor(resized)
    let n = nvision.normalize(t, [0.485, 0.456, 0.406], [0.229, 0.224, 0.225])
    print(n)
}
```

## Image IO

| API | Description |
|-----|-------------|
| `imread(path)` | Load PNG / JPEG / BMP |
| `imwrite(path, img)` | Save by extension |
| `to_tensor(img)` | HWC u8 → CHW f32 in `[0,1]` |
| `from_tensor(t)` | CHW f32 → HWC u8 |

Codecs use in-crate PNG/BMP/JPEG (zlib via `niao_archive`). Spec targets `ncodec` image APIs once available.

## Transforms

Interpolation uses **half-pixel / `align_corners=False`** (torchvision default). Not bit-identical to PIL — compare with `rtol≈1e-4`.

- Geometric: `resize` (nearest/bilinear/bicubic), `crop` / `center_crop`, `pad`, `flip_h` / `flip_v`, `rotate`, `warp_affine`, `warp_perspective`
- Photometric: `normalize`, `ColorJitter`, grayscale, `GaussianBlur`, `RandomResizedCrop`, `RandomHorizontalFlip`, `RandomErasing`
- Pipeline: `Compose([...])`

## Classical CV

`convolve`, `sobel` / `scharr`, `gaussian_blur` / `box_blur` / `median_blur`, `canny`, `threshold` (binary / Otsu / adaptive), morphology (erode/dilate/open/close), histogram + equalization, color convert (RGB↔gray↔HSV↔YCbCr), integral image, connected components, Harris / HOG / template match.

## Datasets

Local paths only (no downloads):

- `MNIST` / `FashionMNIST` (IDX)
- `CIFAR-10` (binary batch)
- `ImageFolder` (class-per-directory)
- `DataLoader` (batch / seeded shuffle → `niao_tensor`; can collate into `niao_ml::DataLoader`)

## Errors (4090–4095)

| Code | Meaning |
|------|---------|
| 4090 | Arity |
| 4091 | General |
| 4092 | Type |
| 4093 | Decode / encode |
| 4094 | Shape / channel mismatch |
| 4095 | Missing file |

## Conv building blocks

Thin wrappers over `niao_ml` `conv2d` / `batch_norm2d` / `relu` — compose models in Niao without shipping weights.
