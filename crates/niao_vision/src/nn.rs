//! Thin wrappers around `niao_ml` conv / batch-norm building blocks.
//! No pretrained backbones (ResNet/ViT weights → `niao_ml_models` / v2).

use crate::error::VisionResult;
use niao_ml::Layer;
use niao_tensor::{Device, Tensor};

pub fn conv2d(
    in_channels: usize,
    out_channels: usize,
    kernel_size: usize,
    device: Device,
) -> VisionResult<Layer> {
    Ok(Layer::conv2d(
        in_channels,
        out_channels,
        (kernel_size, kernel_size),
        device,
    )?)
}

pub fn batch_norm2d(channels: usize, device: Device) -> VisionResult<Layer> {
    Ok(Layer::batch_norm2d(channels, device)?)
}

pub fn relu() -> Layer {
    Layer::relu()
}

pub fn forward(layer: &mut Layer, input: &Tensor) -> VisionResult<Tensor> {
    Ok(layer.forward(input)?)
}
