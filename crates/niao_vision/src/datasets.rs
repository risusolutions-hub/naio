//! Dataset loaders (local paths only — no network downloads).

use crate::error::{VisionError, VisionResult};
use crate::image::{ColorMode, Image};
use crate::io::imread;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Sample {
    pub image: Image,
    pub label: i64,
}

pub trait VisionDataset {
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> VisionResult<Sample>;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// MNIST / Fashion-MNIST IDX format (local files).
pub struct Mnist {
    images: Vec<u8>,
    labels: Vec<u8>,
    n: usize,
    rows: usize,
    cols: usize,
}

impl Mnist {
    pub fn load(images_path: impl AsRef<Path>, labels_path: impl AsRef<Path>) -> VisionResult<Self> {
        let images = read_file(images_path.as_ref())?;
        let labels = read_file(labels_path.as_ref())?;
        if images.len() < 16 || labels.len() < 8 {
            return Err(VisionError::Codec("MNIST files too short".into()));
        }
        let magic_i = u32::from_be_bytes(images[0..4].try_into().unwrap());
        let magic_l = u32::from_be_bytes(labels[0..4].try_into().unwrap());
        if magic_i != 2051 || magic_l != 2049 {
            return Err(VisionError::Codec(format!(
                "bad MNIST magic {magic_i}/{magic_l}"
            )));
        }
        let n = u32::from_be_bytes(images[4..8].try_into().unwrap()) as usize;
        let rows = u32::from_be_bytes(images[8..12].try_into().unwrap()) as usize;
        let cols = u32::from_be_bytes(images[12..16].try_into().unwrap()) as usize;
        let n_l = u32::from_be_bytes(labels[4..8].try_into().unwrap()) as usize;
        if n != n_l {
            return Err(VisionError::Shape("MNIST image/label count mismatch".into()));
        }
        let expect = 16 + n * rows * cols;
        if images.len() < expect || labels.len() < 8 + n {
            return Err(VisionError::Codec("MNIST truncated".into()));
        }
        Ok(Self {
            images: images[16..expect].to_vec(),
            labels: labels[8..8 + n].to_vec(),
            n,
            rows,
            cols,
        })
    }

    pub fn fashion(images_path: impl AsRef<Path>, labels_path: impl AsRef<Path>) -> VisionResult<Self> {
        Self::load(images_path, labels_path)
    }
}

impl VisionDataset for Mnist {
    fn len(&self) -> usize {
        self.n
    }
    fn get(&self, index: usize) -> VisionResult<Sample> {
        if index >= self.n {
            return Err(VisionError::Shape("MNIST index OOB".into()));
        }
        let start = index * self.rows * self.cols;
        let data = self.images[start..start + self.rows * self.cols].to_vec();
        Ok(Sample {
            image: Image::new(self.rows, self.cols, ColorMode::Gray, data)?,
            label: self.labels[index] as i64,
        })
    }
}

/// CIFAR-10 binary batch (10_000 × 3073 records).
pub struct Cifar10 {
    data: Vec<u8>,
    n: usize,
}

impl Cifar10 {
    pub fn load(batch_path: impl AsRef<Path>) -> VisionResult<Self> {
        let data = read_file(batch_path.as_ref())?;
        if data.len() % 3073 != 0 {
            return Err(VisionError::Codec(format!(
                "CIFAR batch size {} not multiple of 3073",
                data.len()
            )));
        }
        let n = data.len() / 3073;
        Ok(Self { data, n })
    }
}

impl VisionDataset for Cifar10 {
    fn len(&self) -> usize {
        self.n
    }
    fn get(&self, index: usize) -> VisionResult<Sample> {
        if index >= self.n {
            return Err(VisionError::Shape("CIFAR index OOB".into()));
        }
        let base = index * 3073;
        let label = self.data[base] as i64;
        let mut rgb = vec![0u8; 32 * 32 * 3];
        // CIFAR layout: R plane, G plane, B plane (32×32 each)
        for y in 0..32 {
            for x in 0..32 {
                let p = y * 32 + x;
                let d = (y * 32 + x) * 3;
                rgb[d] = self.data[base + 1 + p];
                rgb[d + 1] = self.data[base + 1 + 1024 + p];
                rgb[d + 2] = self.data[base + 1 + 2048 + p];
            }
        }
        Ok(Sample {
            image: Image::new(32, 32, ColorMode::Rgb, rgb)?,
            label,
        })
    }
}

/// ImageFolder: class-per-subdirectory.
pub struct ImageFolder {
    samples: Vec<(PathBuf, i64)>,
    pub classes: Vec<String>,
}

impl ImageFolder {
    pub fn new(root: impl AsRef<Path>) -> VisionResult<Self> {
        let root = root.as_ref();
        if !root.is_dir() {
            return Err(VisionError::MissingFile(root.display().to_string()));
        }
        let mut classes: Vec<String> = fs::read_dir(root)
            .map_err(|e| VisionError::Codec(e.to_string()))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        classes.sort();
        let mut samples = Vec::new();
        for (label, class) in classes.iter().enumerate() {
            let dir = root.join(class);
            for entry in fs::read_dir(&dir).map_err(|e| VisionError::Codec(e.to_string()))? {
                let entry = entry.map_err(|e| VisionError::Codec(e.to_string()))?;
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        let e = ext.to_ascii_lowercase();
                        if matches!(e.as_str(), "png" | "jpg" | "jpeg" | "bmp") {
                            samples.push((path, label as i64));
                        }
                    }
                }
            }
        }
        Ok(Self { samples, classes })
    }
}

impl VisionDataset for ImageFolder {
    fn len(&self) -> usize {
        self.samples.len()
    }
    fn get(&self, index: usize) -> VisionResult<Sample> {
        let (path, label) = self
            .samples
            .get(index)
            .ok_or_else(|| VisionError::Shape("ImageFolder index OOB".into()))?;
        Ok(Sample {
            image: imread(path)?,
            label: *label,
        })
    }
}

fn read_file(path: &Path) -> VisionResult<Vec<u8>> {
    fs::read(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            VisionError::MissingFile(path.display().to_string())
        } else {
            VisionError::Codec(format!("read {}: {e}", path.display()))
        }
    })
}

/// Write a tiny MNIST-like fixture for tests.
pub fn write_mnist_fixture(
    images_path: &Path,
    labels_path: &Path,
    n: usize,
    rows: usize,
    cols: usize,
) -> VisionResult<()> {
    let mut images = Vec::with_capacity(16 + n * rows * cols);
    images.extend_from_slice(&2051u32.to_be_bytes());
    images.extend_from_slice(&(n as u32).to_be_bytes());
    images.extend_from_slice(&(rows as u32).to_be_bytes());
    images.extend_from_slice(&(cols as u32).to_be_bytes());
    let mut labels = Vec::with_capacity(8 + n);
    labels.extend_from_slice(&2049u32.to_be_bytes());
    labels.extend_from_slice(&(n as u32).to_be_bytes());
    for i in 0..n {
        labels.push((i % 10) as u8);
        for p in 0..rows * cols {
            images.push(((i * 17 + p) % 256) as u8);
        }
    }
    fs::write(images_path, images).map_err(|e| VisionError::Codec(e.to_string()))?;
    fs::write(labels_path, labels).map_err(|e| VisionError::Codec(e.to_string()))?;
    Ok(())
}

pub fn write_cifar_fixture(path: &Path, n: usize) -> VisionResult<()> {
    let mut data = Vec::with_capacity(n * 3073);
    for i in 0..n {
        data.push((i % 10) as u8);
        for p in 0..3072 {
            data.push(((i * 3 + p) % 256) as u8);
        }
    }
    fs::write(path, data).map_err(|e| VisionError::Codec(e.to_string()))
}
