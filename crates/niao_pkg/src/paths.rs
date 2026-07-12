use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMode {
    Global,
    Venv,
}

/// Resolve the active Niao home directory (app install, legacy, or env override).
pub fn resolve_niao_home() -> PathBuf {
    if let Ok(dir) = env::var("NIAO_HOME") {
        return PathBuf::from(dir);
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(root) = install_root_from_exe(&exe) {
            return root;
        }
    }

    #[cfg(windows)]
    {
        if let Ok(local) = env::var("LOCALAPPDATA") {
            let app = PathBuf::from(local).join("Programs").join("Niao");
            if app.join("install.json").is_file() {
                return app;
            }
        }
    }

  if let Ok(home) = env::var("HOME") {
        let legacy = PathBuf::from(home).join(".niao");
        if legacy.join("install.json").is_file() {
            return legacy;
        }
    }

    #[cfg(windows)]
    {
        if let Ok(profile) = env::var("USERPROFILE") {
            let legacy = PathBuf::from(profile).join(".niao");
            if legacy.join("install.json").is_file() {
                return legacy;
            }
        }
    }

    niao_home_default()
}

/// User-wide Niao home default: `%USERPROFILE%/.niao` on Windows, `~/.niao` elsewhere.
pub fn niao_home() -> PathBuf {
    resolve_niao_home()
}

fn niao_home_default() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(profile) = env::var("USERPROFILE") {
            return PathBuf::from(profile).join(".niao");
        }
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".niao");
    }
    PathBuf::from(".niao")
}

fn install_root_from_exe(exe: &Path) -> Option<PathBuf> {
    let bin = exe.parent()?;
    if bin.file_name()? != "bin" {
        return None;
    }
    let root = bin.parent()?;
    if root.join("install.json").is_file() {
        Some(root.to_path_buf())
    } else {
        None
    }
}

pub fn niao_bin_dir() -> PathBuf {
    niao_home().join("bin")
}

pub fn niao_libs_dir() -> PathBuf {
    niao_home().join("niao_libs")
}

pub fn global_install_state_path() -> PathBuf {
    niao_home().join("install.json")
}

pub fn global_catalog_path() -> PathBuf {
    niao_libs_dir().join("catalog.json")
}

/// Project-local venv root: `<project>/.niao`
pub fn project_venv_dir(project: &Path) -> PathBuf {
    project.join(".niao")
}

pub fn venv_libs_dir(project: &Path) -> PathBuf {
    project_venv_dir(project).join("niao_libs")
}

pub fn venv_install_state_path(project: &Path) -> PathBuf {
    project_venv_dir(project).join("install.json")
}

pub fn venv_catalog_path(project: &Path) -> PathBuf {
    venv_libs_dir(project).join("catalog.json")
}

pub fn lib_manifest_dir(base: &Path, name: &str, version: &str) -> PathBuf {
    base.join(name).join(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_root_from_bin_layout() {
        let root = PathBuf::from(r"C:\Users\me\AppData\Local\Programs\Niao");
        let exe = root.join("bin").join("niao.exe");
        assert_eq!(install_root_from_exe(&exe), Some(root));
    }
}
