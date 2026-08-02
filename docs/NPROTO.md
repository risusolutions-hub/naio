# nproto — Protocol Buffers codec + codegen

Dynamic protobuf compile, encode/decode, canonical JSON mapping, wire introspection, and Niao source codegen. Native Rust implementation backed by `prost` + `prost-reflect` (~Python `protobuf` subset).

## Import

```niao
import "nproto"
```

Paths `import "std/nproto"` and `import "nproto"` are equivalent. Flat builtins (`nproto_compile`, `nproto_encode`, …) are also available globally after import.

## Quick start

```niao
import "nproto"

let src = 'syntax = "proto3";\npackage demo;\nmessage Person {\n  string name = 1;\n  int32 age = 2;\n  repeated string tags = 3;\n}\n'

let schema = nproto.compile(src)
let msg = nproto.new_message(schema, "demo.Person", {
    name: "Ada",
    age: 36,
    tags: ["engineer", "niao"]
})

let bytes = nproto.encode(msg)
let decoded = nproto.decode(schema, "demo.Person", bytes)
print(nproto.get(decoded, "name"))          // Ada

let json = nproto.to_json(msg)
print(nproto.from_json(schema, "demo.Person", json))

nproto.close_message(msg)
nproto.close_message(decoded)
nproto.close_schema(schema)
```

## Schema handles

Compile `.proto` source or load a binary `FileDescriptorSet`. Schema handles are opaque ints; call `close_schema` when done.

| Method | Description |
|--------|-------------|
| `nproto.compile(source, opts?)` | Compile inline `.proto` text. Options: `{filename, include_paths}`. |
| `nproto.compile_file(path, opts?)` | Compile a `.proto` file from disk. |
| `nproto.compile_files(files, opts?)` | Compile multiple files; returns schema handle. |
| `nproto.load_descriptor_set(bytes)` | Load compiled descriptors from bytes. |
| `nproto.save_descriptor_set(schema)` | Export `FileDescriptorSet` bytes. |
| `nproto.message_names(schema)` | Array of fully-qualified message names. |
| `nproto.enum_names(schema)` | Array of fully-qualified enum names. |
| `nproto.describe(schema, message)` | `{name, full_name, fields, oneofs}` metadata object. |
| `nproto.codegen(schema, opts?)` | Generate Niao helper source. Options: `{module_name, include_helpers}`. |
| `nproto.close_schema(schema)` | Free schema handle. |

## Message handles

Create, mutate, serialize, and inspect dynamic messages. Handles are opaque ints; call `close_message` when done.

| Method | Description |
|--------|-------------|
| `nproto.new_message(schema, type, fields?)` | Empty or pre-filled message. |
| `nproto.decode(schema, type, bytes)` | Parse wire bytes into a message handle. |
| `nproto.from_json(schema, type, json_text)` | Parse canonical protobuf JSON. |
| `nproto.get(msg, field)` | Read field as a Niao value. |
| `nproto.set(msg, field, value)` | Write field; returns the same handle. |
| `nproto.has(msg, field)` | `true` when the field is set. |
| `nproto.fields(msg)` | Object of all set fields. |
| `nproto.encode(msg)` | Serialize to `byte_array`. |
| `nproto.merge(dst, src)` | Merge `src` into `dst` (same type). |
| `nproto.clone(msg)` | Deep copy handle. |
| `nproto.clear(msg)` | Clear all fields. |
| `nproto.to_json(msg, pretty?)` | Canonical JSON string (`pretty` default `false`). |
| `nproto.type_name(msg)` | Fully-qualified protobuf message name. |
| `nproto.close_message(msg)` | Free message handle. |

Supported field value types: `nil`, bool, int, float, string, `byte_array`, arrays (repeated), objects (maps and nested messages).

## Wire utilities

Low-level helpers for debugging and custom codecs (no schema required).

| Method | Description |
|--------|-------------|
| `nproto.decode_raw(bytes)` | Array of `{field_number, wire_type, wire_name, value}`. |
| `nproto.encode_tag(field_num, wire_type)` | Encode a field tag varint. |
| `nproto.encode_varint(n)` | Unsigned varint bytes. |
| `nproto.decode_varint(bytes, offset?)` | `{value, offset}`. |
| `nproto.valid_descriptor_set(bytes)` | `true` when bytes decode as `FileDescriptorSet`. |

Wire types: `0` varint, `1` fixed64, `2` length-delimited, `5` fixed32.

## Codegen

`nproto.codegen(schema, {include_helpers: true})` emits commented Niao helpers:

- `new_MessageName(schema, fields)` wrappers
- `decode_MessageName(schema, bytes)` wrappers
- `FIELD_MESSAGENAME_FIELDNAME` constants

Generated files import `nproto` and are meant as a starting point for typed call sites.

## Errors

| Code | Meaning |
|------|---------|
| 4340 | Wrong argument count. |
| 4341 | General protobuf error (catchable `nproto_error`). |
| 4342 | Type mismatch (hard error). |
| 4343 | `.proto` compile / descriptor parse failure. |
| 4344 | Invalid or closed schema/message handle. |

## Performance notes

- Schema compilation is cached in handles — compile once, encode/decode many times.
- Hot encode/decode uses `prost-reflect` dynamic messages (allocation-aware, no Python/runtime shell-out).
- For maximum throughput on a fixed schema, use `codegen` helpers and keep message handles alive across requests.

## Deferred / limitations

- **gRPC client/server stubs** — service metadata is listed via descriptors, but RPC runtime is not generated.
- **Proto2 extensions** — not supported in v0.1.0.
- **Text format** — wire + JSON only; protobuf text format is deferred.
- **Descriptor pools across processes** — handles are in-process only.
