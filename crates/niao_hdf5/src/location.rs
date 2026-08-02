//! Group / location navigation and introspection.

use crate::error::{Hdf5Error, Hdf5Result};
use hdf5_metno::file::File;
use hdf5_metno::Group;
use hdf5_metno::{Dataset, LocationType};
use std::collections::BTreeMap;

/// Resolve a location handle (root file or nested group path).
pub fn resolve_group(file: &File, path: &str) -> Hdf5Result<Group> {
    if path.is_empty() || path == "/" {
        return file.as_group().map_err(Hdf5Error::from);
    }
    Ok(file.group(path)?)
}

/// List immediate member names in a group.
pub fn member_names(file: &File, path: &str) -> Hdf5Result<Vec<String>> {
    let g = resolve_group(file, path)?;
    Ok(g.member_names()?)
}

/// Check whether a link exists at `name` relative to `base`.
pub fn link_exists(file: &File, base: &str, name: &str) -> Hdf5Result<bool> {
    let g = resolve_group(file, base)?;
    Ok(g.link_exists(name))
}

/// Return object kind: `group`, `dataset`, `datatype`, or `unknown`.
pub fn object_kind(file: &File, path: &str) -> Hdf5Result<String> {
    let g = resolve_group(file, parent_path(path))?;
    let name = leaf_name(path);
    let ty = g.loc_type_by_name(name)?;
    Ok(match ty {
        LocationType::Group => "group".into(),
        LocationType::Dataset => "dataset".into(),
        LocationType::NamedDatatype => "datatype".into(),
        _ => "unknown".into(),
    })
}

pub fn create_group(file: &File, path: &str) -> Hdf5Result<()> {
    if path.is_empty() {
        return Err(Hdf5Error::Io("group path cannot be empty".into()));
    }
    file.create_group_builder()
        .create_intermediate_group(true)
        .create(path)?;
    Ok(())
}

pub fn open_dataset(file: &File, path: &str) -> Hdf5Result<Dataset> {
    let g = resolve_group(file, parent_path(path))?;
    let name = leaf_name(path);
    Ok(g.dataset(name)?)
}

fn parent_path(path: &str) -> &str {
    path.rfind('/').map(|i| &path[..i]).unwrap_or("")
}

fn leaf_name(path: &str) -> &str {
    path.rfind('/').map(|i| &path[i + 1..]).unwrap_or(path)
}

/// Build a nested tree of groups and datasets (one level of children per node).
pub fn tree(file: &File, path: &str, depth: usize) -> Hdf5Result<BTreeMap<String, TreeNode>> {
    if depth == 0 {
        return Ok(BTreeMap::new());
    }
    let g = resolve_group(file, path)?;
    let mut out = BTreeMap::new();
    for name in g.member_names()? {
        let child_path = if path.is_empty() {
            name.clone()
        } else {
            format!("{path}/{name}")
        };
        let ty = g.loc_type_by_name(&name)?;
        let node = match ty {
            LocationType::Group => {
                let children = tree(file, &child_path, depth.saturating_sub(1))?;
                TreeNode::Group { children }
            }
            LocationType::Dataset => {
                let ds = g.dataset(&name)?;
                TreeNode::Dataset {
                    shape: ds.shape().into_iter().map(|d| d as i64).collect(),
                    dtype: crate::dtype::dtype_name(&ds.dtype()?)?,
                }
            }
            _ => TreeNode::Other,
        };
        out.insert(name, node);
    }
    Ok(out)
}

#[derive(Debug, Clone)]
pub enum TreeNode {
    Group {
        children: BTreeMap<String, TreeNode>,
    },
    Dataset {
        shape: Vec<i64>,
        dtype: String,
    },
    Other,
}

/// Copy dataset from src path to dst path (same or different file).
pub fn copy_object(src: &File, src_path: &str, dst: &File, dst_path: &str) -> Hdf5Result<()> {
    let parent = parent_path(src_path);
    let name = leaf_name(src_path);
    let g = resolve_group(src, parent)?;
    let ty = g.loc_type_by_name(name)?;
    match ty {
        LocationType::Dataset => {
            let ds = g.dataset(name)?;
            let dst_parent = parent_path(dst_path);
            let dst_name = leaf_name(dst_path);
            let dg = resolve_group(dst, dst_parent)?;
            ds.copy_to(&dg, dst_name)?;
        }
        LocationType::Group => {
            return Err(Hdf5Error::H5(
                "recursive group copy not supported; copy datasets individually".into(),
            ));
        }
        _ => {
            return Err(Hdf5Error::NotFound(format!(
                "no copyable object at '{src_path}'"
            )));
        }
    }
    Ok(())
}
