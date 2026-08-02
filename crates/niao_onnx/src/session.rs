use crate::error::{OnnxError, OnnxResult};
use crate::io_desc::{dtype_name, shape_from_concrete, shape_from_fact, IoDesc};
use std::collections::HashMap;
use std::path::Path;
use tract_onnx::prelude::*;

/// Options applied when compiling an ONNX graph for CPU inference.
#[derive(Debug, Clone, Default)]
pub struct SessionOptions {
    pub num_threads: Option<usize>,
}

/// Compiled ONNX inference session.
pub struct OnnxSession {
    plan: SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>,
    inputs: Vec<IoDesc>,
    outputs: Vec<IoDesc>,
}

impl OnnxSession {
    pub fn inputs(&self) -> &[IoDesc] {
        &self.inputs
    }

    pub fn outputs(&self) -> &[IoDesc] {
        &self.outputs
    }

    /// Run inference with named float32 tensors.
    ///
    /// Each value is `(shape, data)` where `data.len()` must equal the product of `shape`.
    pub fn run_f32(
        &self,
        feed: &HashMap<String, (Vec<usize>, Vec<f32>)>,
    ) -> OnnxResult<HashMap<String, (Vec<usize>, Vec<f32>)>> {
        if feed.is_empty() {
            return Err(OnnxError::Empty);
        }

        let mut input_tensors = tvec![];
        for desc in &self.inputs {
            let (shape, data) = feed
                .get(&desc.name)
                .ok_or_else(|| OnnxError::MissingInput(desc.name.clone()))?;
            validate_f32_feed(desc, shape, data)?;
            let tract_shape: TVec<usize> = shape.iter().copied().collect();
            let tensor = Tensor::from_shape(&tract_shape, data).map_err(map_engine)?;
            input_tensors.push(tensor.into());
        }

        let outputs = self.plan.run(input_tensors).map_err(map_engine)?;
        let mut out = HashMap::new();
        for (idx, desc) in self.outputs.iter().enumerate() {
            let tv = outputs
                .get(idx)
                .ok_or_else(|| OnnxError::Engine("missing output tensor".into()))?;
            let view = tv.to_array_view::<f32>().map_err(map_engine)?;
            let shape: Vec<usize> = view.shape().iter().copied().collect();
            let data: Vec<f32> = view.iter().copied().collect();
            out.insert(desc.name.clone(), (shape, data));
        }
        Ok(out)
    }
}

fn validate_f32_feed(desc: &IoDesc, shape: &[usize], data: &[f32]) -> OnnxResult<()> {
    if desc.dtype != "float32" {
        return Err(OnnxError::DtypeMismatch {
            name: desc.name.clone(),
            expected: desc.dtype.clone(),
            got: "float32".into(),
        });
    }
    let expected_elems: usize = shape.iter().product();
    if data.len() != expected_elems {
        return Err(OnnxError::SizeMismatch {
            name: desc.name.clone(),
            expected: expected_elems,
            got: data.len(),
        });
    }
    if desc.shape.iter().any(|d| d.is_some()) {
        for (i, (&got, exp)) in shape.iter().zip(desc.shape.iter()).enumerate() {
            if let Some(want) = exp {
                if got != *want {
                    return Err(OnnxError::ShapeMismatch {
                        name: desc.name.clone(),
                        expected: desc.shape_display(),
                        got: format!("axis {i}: {got}"),
                    });
                }
            }
        }
    }
    Ok(())
}

fn map_engine(e: TractError) -> OnnxError {
    OnnxError::Engine(e.to_string())
}

fn compile_model(model: InferenceModel, opts: &SessionOptions) -> OnnxResult<OnnxSession> {
    let typed = model.into_typed().map_err(map_engine)?;
    if let Some(n) = opts.num_threads {
        if n == 0 {
            return Err(OnnxError::Param("num_threads must be >= 1".into()));
        }
    }
    let inputs = io_descs(&typed, true)?;
    let outputs = io_descs(&typed, false)?;
    let plan = typed
        .into_optimized()
        .map_err(map_engine)?
        .into_runnable()
        .map_err(map_engine)?;
    Ok(OnnxSession {
        plan,
        inputs,
        outputs,
    })
}

fn io_descs(model: &TypedModel, inputs: bool) -> OnnxResult<Vec<IoDesc>> {
    let outlets = if inputs {
        model.input_outlets().map_err(map_engine)?
    } else {
        model.output_outlets().map_err(map_engine)?
    };
    let mut out = Vec::with_capacity(outlets.len());
    for (ix, outlet) in outlets.iter().enumerate() {
        let fact = if inputs {
            model.input_fact(ix).map_err(map_engine)?
        } else {
            model.output_fact(ix).map_err(map_engine)?
        };
        let node = model.node(outlet.node);
        let name = if node.name.is_empty() {
            format!("{}_{ix}", if inputs { "input" } else { "output" })
        } else {
            node.name.clone()
        };
        let shape = if let Some(concrete) = fact.shape.as_concrete() {
            shape_from_concrete(concrete)
        } else {
            shape_from_fact(fact.shape.as_ref())
        };
        out.push(IoDesc {
            name,
            shape,
            dtype: dtype_name(fact.datum_type),
        });
    }
    Ok(out)
}

/// Load and compile an ONNX model from a filesystem path.
pub fn load_path(path: impl AsRef<Path>, opts: &SessionOptions) -> OnnxResult<OnnxSession> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(OnnxError::Path(format!(
            "file not found: {}",
            path.display()
        )));
    }
    let model = tract_onnx::onnx()
        .model_for_path(path)
        .map_err(map_engine)?;
    compile_model(model, opts)
}

/// Load and compile an ONNX model from raw bytes.
pub fn load_bytes(bytes: &[u8], opts: &SessionOptions) -> OnnxResult<OnnxSession> {
    if bytes.is_empty() {
        return Err(OnnxError::Empty);
    }
    let model = tract_onnx::onnx()
        .model_for_read(&mut std::io::Cursor::new(bytes))
        .map_err(map_engine)?;
    compile_model(model, opts)
}

/// Inspect ONNX metadata without compiling the full execution plan.
pub fn inspect_path(path: impl AsRef<Path>) -> OnnxResult<(Vec<IoDesc>, Vec<IoDesc>)> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(OnnxError::Path(format!(
            "file not found: {}",
            path.display()
        )));
    }
    let model = tract_onnx::onnx()
        .model_for_path(path)
        .map_err(map_engine)?;
    inspect_model(model)
}

/// Inspect ONNX metadata from bytes.
pub fn inspect_bytes(bytes: &[u8]) -> OnnxResult<(Vec<IoDesc>, Vec<IoDesc>)> {
    if bytes.is_empty() {
        return Err(OnnxError::Empty);
    }
    let model = tract_onnx::onnx()
        .model_for_read(&mut std::io::Cursor::new(bytes))
        .map_err(map_engine)?;
    inspect_model(model)
}

fn inspect_model(model: InferenceModel) -> OnnxResult<(Vec<IoDesc>, Vec<IoDesc>)> {
    let typed = model.into_typed().map_err(map_engine)?;
    let inputs = io_descs(&typed, true)?;
    let outputs = io_descs(&typed, false)?;
    Ok((inputs, outputs))
}

/// tract / ONNX stack version string exposed to Niao callers.
pub fn engine_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn inspect_mobilenet() {
        let path = fixture("mobilenetv2-7.onnx");
        if !path.exists() {
            return;
        }
        let (inputs, outputs) = inspect_path(&path).unwrap();
        assert!(!inputs.is_empty());
        assert!(!outputs.is_empty());
        assert_eq!(inputs[0].dtype, "float32");
    }

    #[test]
    fn load_and_run_mobilenet_smoke() {
        let path = fixture("mobilenetv2-7.onnx");
        if !path.exists() {
            return;
        }
        let session = load_path(&path, &SessionOptions::default()).unwrap();
        let input = session.inputs()[0].clone();
        let shape = input
            .shape
            .iter()
            .map(|d| d.unwrap_or(1))
            .collect::<Vec<_>>();
        let n: usize = shape.iter().product();
        let data = vec![0.0f32; n];
        let mut feed = HashMap::new();
        feed.insert(input.name.clone(), (shape, data));
        let out = session.run_f32(&feed).unwrap();
        assert!(!out.is_empty());
    }
}
