# nonnx standard library

ONNX model loading and CPU inference for small models. Pure-Rust engine via [tract](https://github.com/sonos/tract) — a practical **onnxruntime** subset for Niao.

## Import

```niao
import "nonnx"
```

Paths `import "std/nonnx"` and `import "nonnx"` are equivalent.

## Quick start

```niao
import "nonnx"

let meta = nonnx.inspect("model.onnx")
print(meta.inputs[0].name, meta.inputs[0].shape)

let h = nonnx.load("model.onnx", {threads: 4})
let inp = nonnx.inputs(h)[0]
let out = nonnx.run_input(h, inp.name, nonnx.zeros(inp.shape))
let logits = nonnx.output_at(out, 0)
print(len(logits.data))
nonnx.close(h)
```

Load from bytes with `io_read_bytes`:

```niao
let bytes = io_read_bytes("model.onnx")
let h = nonnx.load_bytes(bytes)
nonnx.close(h)
```

Sessions are opaque integer handles (like `ngeo` entities). Always call `nonnx.close(handle)` when finished.

Tensor feeds use `{shape: [..], data: float_array}` objects — build them with `nonnx.tensor(shape, data)`, `nonnx.zeros(shape)`, or `nonnx.batch(rows)`.

**Note:** Dynamic object keys (`feed[name] = tensor`) are not supported in the current Niao parser. Use `nonnx.run_input(handle, name, tensor)` for single-input models, or `nonnx.run(handle, {data: tensor})` when the input name is a literal.

## Model I/O

| Method | Description |
|--------|-------------|
| `nonnx.version()` | Engine/library version string. |
| `nonnx.inspect(path)` | Read input/output metadata without compiling a runnable plan. Returns `{inputs, outputs}` arrays of `{name, shape, dtype}`. |
| `nonnx.inspect_bytes(bytes)` | Same as `inspect` from a `byte_array`. |
| `nonnx.load(path, opts?)` | Load + optimize + compile; returns session handle. Opts: `{threads: N}`. |
| `nonnx.load_bytes(bytes, opts?)` | Load from in-memory ONNX protobuf. |
| `nonnx.close(handle)` | Release session resources; returns `true` on success. |

Dynamic axes appear as `nil` in `shape` arrays. Only **float32** inference is supported in v0.1.

## Session methods

| Method | Description |
|--------|-------------|
| `nonnx.inputs(handle)` | Input descriptors for the loaded graph. |
| `nonnx.outputs(handle)` | Output descriptors. |
| `nonnx.run(handle, feed)` | Run inference. `feed` is an object mapping input **names** to tensor objects (literal keys only). Returns an object mapping output names to `{shape, data}` plus `_order`. |
| `nonnx.run_input(handle, name, tensor)` | Convenience for single-input models when the input name is dynamic. |
| `nonnx.output_at(result, index)` | Fetch output tensor by index from a `run`/`run_input` result (uses internal `_order`). |

## Tensor helpers

| Method | Description |
|--------|-------------|
| `nonnx.tensor(shape, data)` | Build a float32 feed tensor; validates element count. |
| `nonnx.zeros(shape)` | Zero-filled float32 tensor (efficient for large inputs). |
| `nonnx.batch(rows)` | Stack equal-length float rows into `[batch, features]`. |

## Errors

Soft errors return a Niao error object (`nonnx_error`) with codes **4610–4616**:

| Code | Meaning |
|------|---------|
| 4610 | Wrong argument count. |
| 4611 | Inference engine failure. |
| 4612 | Type mismatch. |
| 4613 | Invalid parameter or unknown tensor name. |
| 4614 | Invalid or closed session handle. |
| 4615 | I/O error (missing file, empty bytes). |
| 4616 | Shape/dtype/size mismatch. |

## Deferred (v0.1 non-goals)

- GPU / DirectML / CUDA execution providers
- INT8/FP16 quantized models
- Dynamic-shape re-inference without fixed dims
- ONNX Runtime C++ backend (`ort`) — tract is the v0.1 engine
- Full ONNX opset parity (large transformer graphs may fail to load)

## See also

- `nml` / `ntensor` — native training tensors
- `nvision` — image preprocessing
- `nrag` — embeddings (uses `ort` separately)
