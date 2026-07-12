//! `niao uninstall` — remove the Windows app install (double confirmation).

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const UNINSTALL_REG_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Niao";
const START_MENU_FOLDER: &str = "Niao";
const TERMINAL_SHORTCUT: &str = "Niao Terminal.lnk";

pub fn run_uninstall() -> Result<(), Box<dyn std::error::Error>> {
    let install_root = find_install_root()?;

    println!("This will remove Niao and all installed libraries from your computer.");
    print!("Are you sure you want to uninstall Niao? [y/N]: ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    if !matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        println!("Cancelled.");
        return Ok(());
    }

    print!("Type 'uninstall' to confirm: ");
    io::stdout().flush()?;
    line.clear();
    io::stdin().read_line(&mut line)?;
    if line.trim() != "uninstall" {
        println!("Cancelled.");
        return Ok(());
    }

    println!("\nRemoving Niao...");
    perform_uninstall(&install_root)?;
    println!("Niao has been removed. You can close this window.");
    Ok(())
}

pub fn perform_uninstall(install_root: &Path) -> io::Result<()> {
    let bin_dir = install_root.join("bin");
    remove_terminal_shortcut()?;
    unregister_uninstall()?;
    remove_from_user_path(&bin_dir)?;
    schedule_delete_install_dir(install_root)?;
    Ok(())
}

fn start_menu_dir() -> io::Result<PathBuf> {
    let appdata = env::var("APPDATA").map_err(io::Error::other)?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join(START_MENU_FOLDER))
}

fn remove_terminal_shortcut() -> io::Result<()> {
    let shortcut = start_menu_dir()?.join(TERMINAL_SHORTCUT);
    fs::remove_file(shortcut).ok();
    if let Ok(dir) = start_menu_dir() {
        let _ = fs::remove_dir(dir);
    }
    Ok(())
}

fn unregister_uninstall() -> io::Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.delete_subkey_all(UNINSTALL_REG_KEY).ok();
    Ok(())
}

fn remove_from_user_path(bin_dir: &Path) -> io::Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (env_key, _) = hkcu.create_subkey("Environment")?;
    let path: String = env_key.get_value("Path").unwrap_or_default();

    let parts: Vec<_> = env::split_paths(&path)
        .filter(|p| p.as_path() != bin_dir)
        .collect();
    if parts.len() == env::split_paths(&path).count() {
        return Ok(());
    }

    let new_path = env::join_paths(parts)
        .map_err(io::Error::other)?
        .to_string_lossy()
        .to_string();
    env_key.set_value("Path", &new_path)?;
    broadcast_env_change();
    Ok(())
}

fn schedule_delete_install_dir(install_root: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const DETACHED_PROCESS: u32 = 0x00000008;

    let install_str = install_root.to_string_lossy().replace('"', "\"\"");
    let script = format!(
        "@echo off\r\n\
         timeout /t 2 /nobreak >nul\r\n\
         rd /s /q \"{install_str}\"\r\n\
         del \"%~f0\"\r\n"
    );
    let script_path = env::temp_dir().join(format!("niao_cleanup_{}.cmd", std::process::id()));
    fs::write(&script_path, script)?;
    Command::new("cmd.exe")
        .arg("/C")
        .arg(&script_path)
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()?;
    Ok(())
}

fn broadcast_env_change() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    extern "system" {
        fn SendMessageTimeoutW(
            hwnd: isize,
            msg: u32,
            wparam: usize,
            lparam: *const u16,
            flags: u32,
            timeout: u32,
            pdw_result: *mut usize,
        ) -> isize;
    }
    let wide: Vec<u16> = OsStr::new("Environment")
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        SendMessageTimeoutW(
            0xffff,
            0x001A,
            0,
            wide.as_ptr(),
            0x0002,
            5000,
            std::ptr::null_mut(),
        );
    }
}

pub fn find_install_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(dir) = env::var("NIAO_INSTALL_DIR") {
        let root = PathBuf::from(dir);
        if root.join("install.json").is_file() {
            return Ok(root);
        }
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(root) = install_root_from_exe(&exe) {
            return Ok(root);
        }
    }

    if let Ok(local) = env::var("LOCALAPPDATA") {
        let root = PathBuf::from(local).join("Programs").join("Niao");
        if root.join("install.json").is_file() {
            return Ok(root);
        }
    }

    if let Ok(profile) = env::var("USERPROFILE") {
        let legacy = PathBuf::from(profile).join(".niao");
        if legacy.join("install.json").is_file() {
            return Ok(legacy);
        }
    }

    Err("Niao is not installed (install.json not found).".into())
}

fn install_root_from_exe(exe: &Path) -> Option<PathBuf> {
    let bin = exe.parent()?;
    if bin.file_name()? != "bin" {
        return None;
    }
    let root = bin.parent()?;
    if root.join("install.json").is_file() || root.join("bin").join("uninstall.exe").is_file() {
        Some(root.to_path_buf())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_root_from_bin_layout() {
        let root = std::env::temp_dir().join(format!("niao_cli_uninstall_{}", std::process::id()));
        let _ = std::fs::create_dir_all(root.join("bin"));
        std::fs::write(root.join("install.json"), b"{}").expect("install.json");
        let exe = root.join("bin").join("niao.exe");
        assert_eq!(install_root_from_exe(&exe), Some(root.clone()));
        let _ = std::fs::remove_dir_all(root);
    }
}
