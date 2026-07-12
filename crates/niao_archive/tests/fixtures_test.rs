//! Integration tests against stdlib-generated fixtures.

use niao_archive::{gzip_decode, tar::Archive, zip::ZipArchive};

#[test]
fn fixture_gzip_hello() {
    let data = include_bytes!("fixtures/hello.txt.gz");
    let out = gzip_decode(data).unwrap();
    assert_eq!(&out, b"hello archive\n");
}

#[test]
fn fixture_tar_gz_package() {
    let data = include_bytes!("fixtures/package.tar.gz");
    let arc = Archive::open_gz(data).unwrap();
    let paths: Vec<_> = arc.entries().iter().map(|e| e.path.as_str()).collect();
    assert!(paths.iter().any(|p| p.contains("package.json")));
}

#[test]
fn fixture_zip_release() {
    let data = include_bytes!("fixtures/release.zip");
    let arc = ZipArchive::open(data).unwrap();
    assert!(arc.len() >= 2);
    let names: Vec<_> = arc.entries().iter().map(|e| e.name.as_str()).collect();
    assert!(names.iter().any(|n| n.ends_with("niao")));
    assert!(names.iter().any(|n| n.ends_with("nm")));
}

#[test]
fn fixture_zip_deflated() {
    let data = include_bytes!("fixtures/deflated.zip");
    let arc = ZipArchive::open(data).unwrap();
    assert_eq!(arc.by_index(0).unwrap().data, b"deflated zip fixture");
}

#[test]
fn gzip_roundtrip_own() {
    let raw = b"roundtrip payload 12345";
    let gz = niao_archive::gzip_encode(raw).unwrap();
    assert_eq!(gzip_decode(&gz).unwrap(), raw);
}
