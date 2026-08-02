use crate::error::{ProtoError, ProtoResult};
use crate::schema::ProtoSchema;
use prost_reflect::DescriptorPool;
use prost_types::FileDescriptorSet;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Compile one or more `.proto` files from disk.
pub fn compile_files(
    files: &[impl AsRef<str>],
    include_paths: &[impl AsRef<str>],
) -> ProtoResult<ProtoSchema> {
    let paths: Vec<PathBuf> = files.iter().map(|f| PathBuf::from(f.as_ref())).collect();
    let includes: Vec<PathBuf> = include_paths
        .iter()
        .map(|p| PathBuf::from(p.as_ref()))
        .collect();
    let fds = protox::compile(&paths, &includes).map_err(|e| ProtoError::Compile(e.to_string()))?;
    pool_from_fds(fds)
}

/// Compile inline `.proto` source via a temporary file (virtual name for errors).
pub fn compile_source(
    filename: &str,
    source: &str,
    include_paths: &[&str],
) -> ProtoResult<ProtoSchema> {
    let mut includes: Vec<PathBuf> = include_paths.iter().map(PathBuf::from).collect();
    let tmp_dir = std::env::temp_dir().join(format!("nproto_{}", std::process::id()));
    fs::create_dir_all(&tmp_dir).map_err(|e| ProtoError::Io(e.to_string()))?;
    let file_path = tmp_dir.join(filename);
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| ProtoError::Io(e.to_string()))?;
    }
    let mut f = fs::File::create(&file_path).map_err(|e| ProtoError::Io(e.to_string()))?;
    f.write_all(source.as_bytes())
        .map_err(|e| ProtoError::Io(e.to_string()))?;
    includes.push(tmp_dir.clone());
    let include_refs: Vec<String> = includes
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let include_strs: Vec<&str> = include_refs.iter().map(|s| s.as_str()).collect();
    let path = file_path.to_string_lossy().into_owned();
    let result = compile_files(&[path.as_str()], &include_strs);
    let _ = fs::remove_dir_all(&tmp_dir);
    result
}

/// Load a binary `FileDescriptorSet` blob.
pub fn load_descriptor_set(bytes: &[u8]) -> ProtoResult<ProtoSchema> {
    use prost::Message;
    let fds = FileDescriptorSet::decode(bytes).map_err(|e| ProtoError::Parse(e.to_string()))?;
    pool_from_fds(fds)
}

fn pool_from_fds(fds: FileDescriptorSet) -> ProtoResult<ProtoSchema> {
    let pool = DescriptorPool::from_file_descriptor_set(fds.clone())
        .map_err(|e| ProtoError::Schema(e.to_string()))?;
    Ok(ProtoSchema { pool, fds })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERSON_PROTO: &str = r#"
syntax = "proto3";
package demo;

message Person {
  string name = 1;
  int32 age = 2;
  repeated string tags = 3;
}
"#;

    #[test]
    fn compile_inline() {
        let schema = compile_source("person.proto", PERSON_PROTO, &[]).unwrap();
        assert!(schema.message_descriptor("demo.Person").is_ok());
    }
}
