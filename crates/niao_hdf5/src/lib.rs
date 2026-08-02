//! HDF5 scientific dataset read/write for Niao (~h5py subset).

mod attrs;
mod data;
mod dataset;
mod dtype;
mod error;
mod file_ops;
mod location;
mod parallel;

pub use attrs::{attr_names, del_attr, get_attr, read_attrs, set_attr};
pub use data::{flatten_data, nest_data, DynData, SliceSpec};
pub use dataset::{
    create_dataset, dataset, dataset_dtype, dataset_shape, read_dataset, resize_dataset,
    write_dataset, CreateOpts,
};
pub use dtype::DType;
pub use error::{Hdf5Error, Hdf5Result};
pub use file_ops::{
    close_file, copy_file, create_file, flush_file, is_hdf5, library_version, open_file, Mode,
};
pub use location::{
    copy_object, create_group, link_exists, member_names, object_kind, resolve_group, tree,
    TreeNode,
};
pub use parallel::parallel_read;

pub use hdf5_metno::{Dataset, File};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_h5(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("niao_hdf5_tests");
        let _ = fs::create_dir_all(&dir);
        dir.join(name)
    }

    #[test]
    fn roundtrip_numeric() {
        let path = temp_h5("roundtrip.h5");
        let _ = fs::remove_file(&path);
        let f = create_file(path.to_str().unwrap(), Mode::Write).unwrap();
        let opts = CreateOpts::default();
        let ds = create_dataset(&f, "data", &[4, 3], &opts).unwrap();
        let values: Vec<f64> = (0..12).map(|x| x as f64).collect();
        write_dataset(&ds, &DynData::F64(values.clone()), None).unwrap();
        let out = read_dataset(&ds, None).unwrap();
        match out {
            DynData::F64(v) => assert_eq!(v, values),
            _ => panic!("expected f64"),
        }
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn groups_and_attrs() {
        let path = temp_h5("grp.h5");
        let _ = fs::remove_file(&path);
        let f = create_file(path.to_str().unwrap(), Mode::Write).unwrap();
        create_group(&f, "run/exp").unwrap();
        set_attr(&f, "run/exp", "version", &DynData::I64(vec![2])).unwrap();
        let v = get_attr(&f, "run/exp", "version").unwrap();
        match v {
            DynData::I64(n) => assert_eq!(n, vec![2]),
            _ => panic!("attr type"),
        }
        let names = member_names(&f, "run").unwrap();
        assert!(names.contains(&"exp".to_string()));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn slice_read() {
        let path = temp_h5("slice.h5");
        let _ = fs::remove_file(&path);
        let f = create_file(path.to_str().unwrap(), Mode::Write).unwrap();
        let ds = create_dataset(&f, "m", &[10, 10], &CreateOpts::default()).unwrap();
        let data: Vec<f64> = (0..100).map(|x| x as f64).collect();
        write_dataset(&ds, &DynData::F64(data), None).unwrap();
        let sl = SliceSpec::from_parts(vec![0, 0], vec![2, 3], None).unwrap();
        let part = read_dataset(&ds, Some(&sl)).unwrap();
        match part {
            DynData::F64(v) => assert_eq!(v.len(), 6),
            _ => panic!("slice"),
        }
        let _ = fs::remove_file(&path);
    }
}
