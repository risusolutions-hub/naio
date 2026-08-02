//! `niao_nblob` — unified object-store VFS: local dir, memory, S3, Azure Blob,
//! GCS behind one open/read/write/list API (~fsspec / smart_open).

pub mod azure;
pub mod error;
pub mod gcs;
pub mod local;
pub mod memory;
pub mod s3;
pub mod sigv4;
pub mod store;
pub mod uri;
pub mod vfs;

pub use error::{BlobError, BlobResult};
pub use store::{AzureOpts, BackendKind, Entry, GcsOpts, ObjectStore, OpenMode, S3Opts, StoreArc};
pub use uri::{join, parse, scheme_of, BlobUri};
pub use vfs::{
    fs_azure, fs_from_uri, fs_gcs, fs_local, fs_memory, fs_s3, global_vfs, FsHandle, OpenFile, Vfs,
};

#[cfg(test)]
mod integration {
    use super::*;
    use crate::memory::MemoryStore;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn memory_uri_roundtrip() {
        let vfs = Vfs::default();
        let fs = fs_memory(Some("integ"));
        fs.store.write("x", b"abc", None).unwrap();
        assert_eq!(vfs.read_uri("memory://integ/x").unwrap(), b"abc");
        assert_eq!(vfs.write_uri("memory://integ/y", b"zzz", None).unwrap(), 3);
        assert!(vfs.exists_uri("memory://integ/y").unwrap());
        vfs.remove_uri("memory://integ/y").unwrap();
        assert!(!vfs.exists_uri("memory://integ/y").unwrap());
        let _ = fs;
    }

    #[test]
    fn open_seek_flush() {
        let vfs = Vfs::default();
        vfs.write_uri("memory://op/f.txt", b"hello", None).unwrap();
        let mut f = vfs.open_uri("memory://op/f.txt", OpenMode::Read).unwrap();
        assert_eq!(f.read(Some(2)).unwrap(), b"he");
        f.seek(0, 0).unwrap();
        assert_eq!(f.read(None).unwrap(), b"hello");
        let mut w = vfs.open_uri("memory://op/w.txt", OpenMode::Write).unwrap();
        w.write(b"xyz").unwrap();
        w.flush().unwrap();
        assert_eq!(vfs.read_uri("memory://op/w.txt").unwrap(), b"xyz");
    }

    #[test]
    fn cp_mv_cross_memory() {
        let vfs = Vfs::default();
        vfs.write_uri("memory://cp/a", b"payload", None).unwrap();
        vfs.copy_uri("memory://cp/a", "memory://cp/b").unwrap();
        assert_eq!(vfs.read_uri("memory://cp/b").unwrap(), b"payload");
        vfs.move_uri("memory://cp/b", "memory://cp/c").unwrap();
        assert!(!vfs.exists_uri("memory://cp/b").unwrap());
        assert_eq!(vfs.read_uri("memory://cp/c").unwrap(), b"payload");
    }

    #[test]
    fn local_roundtrip() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("nblob_integ_{stamp}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let vfs = Vfs::default();
        let path = root.join("note.txt");
        let uri = path.to_string_lossy();
        vfs.write_uri(&uri, b"local-hi", None).unwrap();
        assert_eq!(vfs.read_uri(&uri).unwrap(), b"local-hi");
        let fs = fs_local(Some(root.to_str().unwrap()));
        fs.store.write("rel.txt", b"rel", None).unwrap();
        assert_eq!(fs.store.read("rel.txt").unwrap(), b"rel");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unicode_keys() {
        let s = MemoryStore::named("uni");
        s.write("café/文件.txt", "日本語".as_bytes(), None).unwrap();
        assert_eq!(s.read("café/文件.txt").unwrap(), "日本語".as_bytes());
    }

    #[test]
    fn sigv4_known_vector() {
        use crate::sigv4::{secs_to_amz, sign, SignInput};
        let (dt, d) = secs_to_amz(1_440_938_160);
        assert_eq!(dt, "20150830T123600Z");
        let inp = SignInput {
            method: "GET",
            host: "example.amazonaws.com",
            path: "/",
            query: "",
            region: "us-east-1",
            service: "s3",
            access_key: "AKIDEXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            session_token: None,
            body: b"",
            amz_datetime: &dt,
            amz_date: &d,
            extra_headers: &[],
        };
        let signed = sign(&inp);
        assert!(signed.headers.iter().any(|(k, _)| k == "authorization"));
    }
}
