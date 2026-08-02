//! Attribute read/write on HDF5 locations.

use crate::data::{read_attr_values, write_attr_scalar, DynData};
use crate::error::{Hdf5Error, Hdf5Result};
use crate::location::resolve_group;
use hdf5_metno::file::File;
use hdf5_metno::Attribute;
use std::collections::BTreeMap;

/// List attribute names on a location.
pub fn attr_names(file: &File, path: &str) -> Hdf5Result<Vec<String>> {
    let loc = resolve_group(file, path)?;
    Ok(loc.attr_names()?)
}

/// Read all attributes into a map.
pub fn read_attrs(file: &File, path: &str) -> Hdf5Result<BTreeMap<String, DynData>> {
    let loc = resolve_group(file, path)?;
    let mut out = BTreeMap::new();
    for name in loc.attr_names()? {
        let attr = loc.attr(&name)?;
        out.insert(name, read_attr(&attr)?);
    }
    Ok(out)
}

/// Read a single attribute.
pub fn read_attr(attr: &Attribute) -> Hdf5Result<DynData> {
    read_attr_values(attr)
}

/// Read attribute by name.
pub fn get_attr(file: &File, path: &str, name: &str) -> Hdf5Result<DynData> {
    let loc = resolve_group(file, path)?;
    if !loc.attr_names()?.iter().any(|n| n == name) {
        return Err(Hdf5Error::NotFound(format!("attribute '{name}' not found")));
    }
    let attr = loc.attr(name)?;
    read_attr(&attr)
}

/// Set (create or overwrite) a scalar or 1d attribute.
pub fn set_attr(file: &File, path: &str, name: &str, value: &DynData) -> Hdf5Result<()> {
    if file.is_read_only() {
        return Err(Hdf5Error::ReadOnly("file opened read-only".into()));
    }
    let loc = resolve_group(file, path)?;
    if loc.attr_names()?.iter().any(|n| n == name) {
        loc.delete_attr(name)?;
    }
    write_attr_scalar(&loc, name, value)
}

/// Delete an attribute by name.
pub fn del_attr(file: &File, path: &str, name: &str) -> Hdf5Result<()> {
    if file.is_read_only() {
        return Err(Hdf5Error::ReadOnly("file opened read-only".into()));
    }
    let loc = resolve_group(file, path)?;
    if !loc.attr_names()?.iter().any(|n| n == name) {
        return Err(Hdf5Error::NotFound(format!("attribute '{name}' not found")));
    }
    loc.delete_attr(name)?;
    Ok(())
}
