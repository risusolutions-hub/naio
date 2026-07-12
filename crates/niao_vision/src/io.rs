//! Image IO via in-crate codecs (ncodec contract; see codec/mod.rs).

use crate::codec::{self, ImageFormat};
use crate::error::{VisionError, VisionResult};
use crate::image::Image;
use std::fs;
use std::path::Path;

pub fn imread(path: impl AsRef<Path>) -> VisionResult<Image> {
    let path = path.as_ref();
    let bytes = fs::read(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            VisionError::MissingFile(path.display().to_string())
        } else {
            VisionError::Codec(format!("read {}: {e}", path.display()))
        }
    })?;
    codec::decode(&bytes)
}

pub fn imwrite(path: impl AsRef<Path>, img: &Image) -> VisionResult<()> {
    let path = path.as_ref();
    let format = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => ImageFormat::Png,
        Some("jpg") | Some("jpeg") => ImageFormat::Jpeg,
        Some("bmp") => ImageFormat::Bmp,
        _ => ImageFormat::Png,
    };
    let bytes = codec::encode(img, format)?;
    fs::write(path, bytes).map_err(|e| VisionError::Codec(format!("write {}: {e}", path.display())))
}

pub fn imdecode(bytes: &[u8]) -> VisionResult<Image> {
    codec::decode(bytes)
}

pub fn imencode(img: &Image, format: ImageFormat) -> VisionResult<Vec<u8>> {
    codec::encode(img, format)
}
