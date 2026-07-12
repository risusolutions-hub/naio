//! Download and install Niao toolchain binaries (`niao` + `nm`) from the release registry.

use crate::error::{PkgError, PkgResult};
use crate::paths::{global_install_state_path, resolve_niao_home};
use crate::registry::{fetch_bytes, registry_url};
use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;
use tar::Archive;

#[derive(Debug, Deserialize)]
struct ReleasesIndex {
  latest: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseDetail {
  version: String,
  #[serde(default)]
  variants: Vec<ReleaseVariant>,
}

#[derive(Debug, Deserialize)]
struct ReleaseVariant {
  id: String,
  #[serde(default)]
  label: String,
  url: String,
  #[serde(default)]
  shasum: String,
  #[serde(default)]
  ext: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolchainUpdateOptions {
  pub force: bool,
}

pub fn update_toolchain(
  target_version: Option<&str>,
  opts: &ToolchainUpdateOptions,
) -> PkgResult<()> {
  let registry = registry_url();
  let version = match target_version {
    Some(v) => v.trim().to_string(),
    None => {
      let url = format!("{registry}/v1/releases/niao");
      let detail: ReleasesIndex = fetch_json(&url)?;
      detail.latest
    }
  };

  if version.is_empty() {
    return Err(PkgError::Message("release version is empty".into()));
  }

  let current = installed_toolchain_version();
  if !opts.force {
    if let Some(cur) = &current {
      if cur == &version {
        println!("niao {version} and nm {version} are already installed.");
        return Ok(());
      }
    }
  }

  let url = format!("{registry}/v1/releases/niao/{version}");
  let detail: ReleaseDetail = fetch_json(&url)?;
  let variant_id = detect_platform_variant();
  let variant = detail
    .variants
    .iter()
    .find(|v| v.id == variant_id)
    .ok_or_else(|| {
      PkgError::Message(format!(
        "no release build for this platform ({variant_id}) in version {version}"
      ))
    })?;

  if variant.url.is_empty() {
    return Err(PkgError::Message(format!(
      "download URL missing for {variant_id} in version {version}"
    )));
  }

  println!("Downloading niao {version} ({})…", variant.label_or_id());
  let bytes = fetch_bytes(&variant.url)?;
  if !variant.shasum.is_empty() {
    verify_sha256(&bytes, &variant.shasum)?;
  }

  let home = resolve_niao_home();
  let bin_dir = home.join("bin");
  fs::create_dir_all(&bin_dir)?;

  let ext_owned = extension_from_url(&variant.url);
  let ext = if !variant.ext.is_empty() {
    variant.ext.as_str()
  } else {
    ext_owned.as_deref().unwrap_or("zip")
  };

  let (niao_bytes, nm_bytes) = extract_binaries(&bytes, ext)?;
  update_install_json_version(&home, &version)?;
  replace_binary(&bin_dir.join(binary_name("niao")), &niao_bytes)?;
  replace_binary(&bin_dir.join(binary_name("nm")), &nm_bytes)?;
  #[cfg(windows)]
  update_windows_uninstall_version(&home, &version)?;

  println!("Updated niao and nm to {version}.");
  if current.is_some() {
    println!("Open a new terminal window to use the updated binaries.");
  }
  Ok(())
}

impl ReleaseVariant {
  fn label_or_id(&self) -> &str {
    if self.label.is_empty() {
      self.id.as_str()
    } else {
      self.label.as_str()
    }
  }
}

fn fetch_json<T: for<'de> Deserialize<'de>>(url: &str) -> PkgResult<T> {
  let response = ureq::get(url)
    .call()
    .map_err(|e| PkgError::Message(format!("release request failed: {e}")))?;
  if !(200..300).contains(&response.status()) {
    return Err(PkgError::Message(format!(
      "release HTTP {} for {}",
      response.status(),
      url
    )));
  }
  let text = response
    .into_string()
    .map_err(|e| PkgError::Message(format!("release read failed: {e}")))?;
  serde_json::from_str(&text)
    .map_err(|e| PkgError::Message(format!("parse release JSON: {e}")))
}

fn installed_toolchain_version() -> Option<String> {
  let path = global_install_state_path();
  let text = fs::read_to_string(path).ok()?;
  let value: serde_json::Value = serde_json::from_str(strip_utf8_bom(&text)).ok()?;
  value
    .get("niao_version")
    .and_then(|v| v.as_str())
    .map(|s| s.to_string())
}

fn strip_utf8_bom(text: &str) -> &str {
  text.strip_prefix('\u{feff}').unwrap_or(text)
}

fn verify_sha256(data: &[u8], expected: &str) -> PkgResult<()> {
  let digest = Sha256::digest(data);
  let got = hex::encode(digest);
  if got.eq_ignore_ascii_case(expected.trim()) {
    Ok(())
  } else {
    Err(PkgError::Message(format!(
      "checksum mismatch (expected {expected}, got {got})"
    )))
  }
}

fn extension_from_url(url: &str) -> Option<String> {
  let path = url.split('?').next()?;
  if path.ends_with(".tar.gz") {
    return Some("tar.gz".into());
  }
  path.rsplit('.').next().map(|s| s.to_string())
}

fn detect_platform_variant() -> &'static str {
  #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
  {
    return "windows-x64";
  }
  #[cfg(all(target_os = "windows", target_arch = "x86"))]
  {
    return "windows-x86";
  }
  #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
  {
    return "windows-arm64";
  }
  #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
  {
    return "linux-x64";
  }
  #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
  {
    return "linux-arm64";
  }
  #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
  {
    return "macos-x64";
  }
  #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
  {
    return "macos-arm64";
  }
  #[allow(unreachable_code)]
  "windows-x64"
}

fn binary_name(base: &str) -> String {
  #[cfg(windows)]
  {
    return format!("{base}.exe");
  }
  #[allow(unreachable_code)]
  base.to_string()
}

fn extract_binaries(data: &[u8], ext: &str) -> PkgResult<(Vec<u8>, Vec<u8>)> {
  match ext {
    "zip" => {
      if data.len() < 4 || &data[..2] != b"PK" {
        let preview = String::from_utf8_lossy(&data[..data.len().min(160)]);
        return Err(PkgError::Message(format!(
          "download is not a valid zip archive (server returned an error page?). Preview: {}",
          preview.split_whitespace().take(12).collect::<Vec<_>>().join(" ")
        )));
      }
      extract_from_zip(data)
    }
    "tar.gz" | "tgz" => extract_from_tar_gz(data),
    _ => Err(PkgError::Message(format!(
      "unsupported release archive type: {ext}"
    ))),
  }
}

fn extract_from_zip(data: &[u8]) -> PkgResult<(Vec<u8>, Vec<u8>)> {
  let cursor = std::io::Cursor::new(data);
  let mut archive =
    zip::ZipArchive::new(cursor).map_err(|e| PkgError::Message(format!("open zip: {e}")))?;
  let mut niao = None;
  let mut nm = None;
  for i in 0..archive.len() {
    let mut file = archive
      .by_index(i)
      .map_err(|e| PkgError::Message(format!("zip entry: {e}")))?;
    let name = file.name().replace('\\', "/");
    let base = name.rsplit('/').next().unwrap_or(&name);
    if base == "niao.exe" || base == "niao" {
      let mut buf = Vec::new();
      file.read_to_end(&mut buf)
        .map_err(|e| PkgError::Message(format!("read niao from zip: {e}")))?;
      niao = Some(buf);
    } else if base == "nm.exe" || base == "nm" {
      let mut buf = Vec::new();
      file.read_to_end(&mut buf)
        .map_err(|e| PkgError::Message(format!("read nm from zip: {e}")))?;
      nm = Some(buf);
    }
  }
  match (niao, nm) {
    (Some(niao), Some(nm)) => Ok((niao, nm)),
    _ => Err(PkgError::Message(
      "release zip did not contain bin/niao and bin/nm".into(),
    )),
  }
}

fn extract_from_tar_gz(data: &[u8]) -> PkgResult<(Vec<u8>, Vec<u8>)> {
  let decoder = GzDecoder::new(data);
  let mut archive = Archive::new(decoder);
  let mut niao = None;
  let mut nm = None;
  for entry in archive
    .entries()
    .map_err(|e| PkgError::Message(format!("read tar.gz: {e}")))?
  {
    let mut entry = entry.map_err(|e| PkgError::Message(format!("tar entry: {e}")))?;
    let path = entry
      .path()
      .map_err(|e| PkgError::Message(format!("tar path: {e}")))?
      .to_string_lossy()
      .replace('\\', "/");
    let base = path.rsplit('/').next().unwrap_or(&path);
    if base == "niao" {
      let mut buf = Vec::new();
      entry
        .read_to_end(&mut buf)
        .map_err(|e| PkgError::Message(format!("read niao from tar: {e}")))?;
      niao = Some(buf);
    } else if base == "nm" {
      let mut buf = Vec::new();
      entry
        .read_to_end(&mut buf)
        .map_err(|e| PkgError::Message(format!("read nm from tar: {e}")))?;
      nm = Some(buf);
    }
  }
  match (niao, nm) {
    (Some(niao), Some(nm)) => Ok((niao, nm)),
    _ => Err(PkgError::Message(
      "release archive did not contain bin/niao and bin/nm".into(),
    )),
  }
}

fn replace_binary(path: &Path, data: &[u8]) -> PkgResult<()> {
  let staging = path.with_extension("new");
  fs::write(&staging, data).map_err(PkgError::from)?;
  let backup = path.with_extension("old");
  fs::remove_file(&backup).ok();
  if path.exists() {
    fs::rename(path, &backup).map_err(PkgError::from)?;
  }
  fs::rename(&staging, path).map_err(PkgError::from)?;
  Ok(())
}

fn update_install_json_version(home: &Path, version: &str) -> PkgResult<()> {
  let path = home.join("install.json");
  let mut value = if path.is_file() {
    let text = fs::read_to_string(&path).map_err(PkgError::from)?;
    match serde_json::from_str::<serde_json::Value>(strip_utf8_bom(&text)) {
      Ok(v) => v,
      Err(e) => {
        eprintln!("warning: repairing invalid install.json ({e})");
        serde_json::json!({})
      }
    }
  } else {
    serde_json::json!({})
  };

  if let Some(obj) = value.as_object_mut() {
    obj.insert(
      "niao_version".to_string(),
      serde_json::Value::String(version.to_string()),
    );
    obj.insert(
      "updated_at".to_string(),
      serde_json::Value::String(crate::state::chrono_now_public()),
    );
    obj.entry("mode".to_string())
      .or_insert(serde_json::Value::String("global".to_string()));
    obj.entry("root".to_string())
      .or_insert(serde_json::Value::String(home.to_string_lossy().to_string()));
    obj.entry("source_root".to_string())
      .or_insert(serde_json::Value::String(String::new()));
    obj.entry("libs".to_string())
      .or_insert(serde_json::json!({}));
  }

  fs::write(
    &path,
    serde_json::to_string_pretty(&value).map_err(|e| PkgError::Message(e.to_string()))?,
  )
  .map_err(PkgError::from)?;
  Ok(())
}

#[cfg(windows)]
fn update_windows_uninstall_version(home: &Path, version: &str) -> PkgResult<()> {
  use winreg::enums::*;
  use winreg::RegKey;
  let hkcu = RegKey::predef(HKEY_CURRENT_USER);
  if let Ok(key) = hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Niao") {
    let _ = key.set_value("DisplayVersion", &version.to_string());
    let _ = key.set_value("InstallLocation", &home.to_string_lossy().to_string());
  }
  Ok(())
}

#[cfg(not(windows))]
fn update_windows_uninstall_version(_home: &Path, _version: &str) -> PkgResult<()> {
  Ok(())
}

// hex encoding without adding a crate dependency beyond sha2's optional hex feature
mod hex {
  pub fn encode(bytes: impl AsRef<[u8]>) -> String {
    bytes
      .as_ref()
      .iter()
      .map(|b| format!("{b:02x}"))
      .collect()
  }
}
