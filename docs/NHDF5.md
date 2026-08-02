# nhdf5 — HDF5 scientific I/O

HDF5 file open/create, groups, dataset read/write, attributes, tree introspection, and parallel multi-file reads. ~h5py subset.

Backed by **hdf5-metno** (libhdf5) with static linking and gzip chunk filters.

## Import

```niao
import "nhdf5"
```

Paths `import "std/nhdf5"` and `import "nhdf5"` are equivalent.

## Quick start

```niao
import "nhdf5"

// Create file and write a 2D float dataset
let f = nhdf5.create("experiment.h5")
nhdf5.create_group(f, "run/exp1")
let ds = nhdf5.create_dataset(f, "run/exp1/matrix", [100, 50], {
    dtype: "f64",
    chunk: [25, 25],
    deflate: 4,
    shuffle: true,
})
let data = [0.0, 1.0, 2.0, 3.0]   // flat row-major payload
nhdf5.write(ds, data)

// Read back (nested arrays by default)
let f2 = nhdf5.open("experiment.h5")
let ds2 = nhdf5.dataset(f2, "run/exp1/matrix")
let grid = nhdf5.read(ds2)
print(nhdf5.shape(ds2))   // [100, 50]

// Attributes
nhdf5.set_attr(f2, "run/exp1", "version", 2)
print(nhdf5.get_attr(f2, "run/exp1", "version"))

nhdf5.close(f2)
```

## Handles

| Handle | Created by | Use with |
|--------|------------|----------|
| File | `open`, `create` | `keys`, `create_group`, `create_dataset`, `dataset`, attrs, `tree`, `close` |
| Dataset | `create_dataset`, `dataset` | `read`, `write`, `shape`, `dtype`, `resize` |

Handles are opaque positive integers (same pattern as `nsorted`, `nframe`).

## Functions

| Method | Description |
|--------|-------------|
| `nhdf5.is_hdf5(path)` | `true` when `path` has HDF5 magic bytes. |
| `nhdf5.version()` | Linked libhdf5 version string. |
| `nhdf5.open(path, opts?)` | Open existing file. `opts.mode`: `r` (default), `r+`, `w`, `w-`, `a`. Returns file handle. |
| `nhdf5.create(path, opts?)` | Create/truncate file (`mode` default `w`). |
| `nhdf5.close(handle)` | Close file handle and release resources. |
| `nhdf5.flush(handle)` | Flush file metadata and data. |
| `nhdf5.keys(file, path?)` | Member names in group (`path` default root). |
| `nhdf5.exists(file, name, base?)` | Whether link exists (`base` optional group path). |
| `nhdf5.kind(file, path)` | `"group"`, `"dataset"`, `"datatype"`, or `"unknown"`. |
| `nhdf5.create_group(file, path)` | Create group (intermediate groups created automatically). |
| `nhdf5.create_dataset(file, path, shape, opts?)` | Create dataset; returns dataset handle. |
| `nhdf5.dataset(file, path)` | Open existing dataset handle. |
| `nhdf5.read(ds, opts?)` | Read dataset to Niao arrays (`IntArray`/`FloatArray` or nested). |
| `nhdf5.write(ds, data, opts?)` | Write flat or nested numeric data. |
| `nhdf5.shape(ds)` | `IntArray` shape. |
| `nhdf5.dtype(ds)` | Dtype name (`i8`…`u64`, `f32`, `f64`, `bool`, `string`). |
| `nhdf5.resize(ds, shape)` | Resize extensible dataset. |
| `nhdf5.attrs(file, path?)` | Object map of all attributes on location. |
| `nhdf5.get_attr(file, path, name)` | Read one attribute. |
| `nhdf5.set_attr(file, path, name, value)` | Create/overwrite attribute. |
| `nhdf5.del_attr(file, path, name)` | Delete attribute. |
| `nhdf5.tree(file, path?, opts?)` | Nested tree of groups/datasets (`opts.depth`, default 8). |
| `nhdf5.copy(src_file, src_path, dst_file, dst_path)` | Copy dataset between locations. |
| `nhdf5.copy_file(src, dst)` | Shallow file copy (groups + datasets). |
| `nhdf5.parallel_read(paths, dataset_path, opts?)` | Read same dataset from many files in parallel (`opts.threads`). |

### Dataset create options

| Key | Default | Description |
|-----|---------|-------------|
| `dtype` | `f64` | `i8`…`u64`, `f32`, `f64`, `bool`, `string` |
| `chunk` | none | Chunk shape `IntArray` for chunked storage |
| `deflate` | none | Gzip level 0–9 (requires chunking) |
| `shuffle` | `false` | Byte-shuffle filter before compression |
| `fill_value` | none | Dataset fill value hint |

### Read/write slice options

| Key | Description |
|-----|-------------|
| `start` | `IntArray` hyperslab start per dimension |
| `count` | `IntArray` element count per dimension |
| `stride` | Optional `IntArray` (only `1` supported today) |
| `nested` | Read option: reshape flat data to nested arrays (default `true`) |

## Dtypes

| Name | HDF5 |
|------|------|
| `i8`…`i64` | signed integers |
| `u8`…`u64` | unsigned integers |
| `f32`, `f64` | floats |
| `bool` | boolean |
| `string` | variable-length UTF-8 strings |

## Errors

Operations return `nhdf5_error` values (import `core` error helpers or `ntest.assert_error`) for I/O failures, missing objects, dtype/shape mismatch, and invalid handles.

## Deferred / limitations

- No HDF5 compound types, enums, references, or opaque dtypes
- No SWMR, MPI-parallel HDF5, or virtual datasets
- Hyperslab `stride` ≠ 1 and rank > 2 slice I/O: use full read/write
- Recursive group copy (copy whole subtrees) — copy datasets individually
- `blosc`/`lzf`/`szip` filters not exposed (gzip/deflate/shuffle only)
