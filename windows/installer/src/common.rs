//! Shared install / uninstall helpers for the Windows Niao app.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use winreg::enums::*;
use winreg::RegKey;

pub const APP_NAME: &str = "Niao";
pub const APP_PUBLISHER: &str = "Niao";
pub const UNINSTALL_REG_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Niao";
pub const START_MENU_FOLDER: &str = "Niao";
pub const TERMINAL_SHORTCUT: &str = "Niao Terminal.lnk";
pub const TERMINAL_CMD: &str = "NiaoTerminal.cmd";
pub const UNINSTALL_EXE: &str = "uninstall.exe";

pub fn default_install_dir() -> PathBuf {
    if let Ok(dir) = env::var("NIAO_INSTALL_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(local) = env::var("LOCALAPPDATA") {
        return PathBuf::from(local).join("Programs").join("Niao");
    }
    if let Ok(profile) = env::var("USERPROFILE") {
        return PathBuf::from(profile).join(".niao");
    }
    PathBuf::from(".niao")
}

pub fn start_menu_dir() -> io::Result<PathBuf> {
    let appdata = env::var("APPDATA").map_err(io::Error::other)?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join(START_MENU_FOLDER))
}

pub fn shortcut_path() -> io::Result<PathBuf> {
    Ok(start_menu_dir()?.join(TERMINAL_SHORTCUT))
}

pub fn patch_install_json(root: &Path) -> io::Result<()> {
    let path = root.join("install.json");
    if !path.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(&path)?;
    let root_str = root.to_string_lossy().replace('\\', "\\\\");
    let patched = text
        .replace("%LOCALAPPDATA%\\\\Programs\\\\Niao", &root_str)
        .replace("%LOCALAPPDATA%\\Programs\\Niao", &root.display().to_string())
        .replace("%USERPROFILE%\\\\.niao", &root_str)
        .replace("%USERPROFILE%\\.niao", &root.display().to_string());
    fs::write(path, patched)
}

pub fn add_to_user_path(bin_dir: &Path) -> io::Result<()> {
    let bin = bin_dir.to_string_lossy().to_string();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (env, _) = hkcu.create_subkey("Environment")?;
    let path: String = env.get_value("Path").unwrap_or_default();

    let already = env::split_paths(&path).any(|p| p == *bin_dir);
    if already {
        println!("\nPATH already contains {bin}");
        return Ok(());
    }

    let new_path = if path.is_empty() {
        bin.clone()
    } else {
        format!("{path};{bin}")
    };
    env.set_value("Path", &new_path)?;
    broadcast_env_change();
    println!("\nAdded to user PATH: {bin}");
    Ok(())
}

pub fn remove_from_user_path(bin_dir: &Path) -> io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (env, _) = hkcu.create_subkey("Environment")?;
    let path: String = env.get_value("Path").unwrap_or_default();
    let bin = bin_dir.to_string_lossy().to_string();

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
    env.set_value("Path", &new_path)?;
    broadcast_env_change();
    println!("Removed from user PATH: {bin}");
    Ok(())
}

pub fn register_uninstall(install_root: &Path, version: &str) -> io::Result<()> {
    let uninstall_exe = install_root.join("bin").join(UNINSTALL_EXE);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(UNINSTALL_REG_KEY)?;

    let uninstall = format!("\"{}\"", uninstall_exe.display());
    let quiet = format!("\"{}\" /quiet", uninstall_exe.display());
    let icon = install_root.join("bin").join("niao.exe");

    key.set_value("DisplayName", &APP_NAME)?;
    key.set_value("DisplayVersion", &version)?;
    key.set_value("Publisher", &APP_PUBLISHER)?;
    key.set_value("InstallLocation", &install_root.to_string_lossy().to_string())?;
    key.set_value("UninstallString", &uninstall)?;
    key.set_value("QuietUninstallString", &quiet)?;
    key.set_value(
        "DisplayIcon",
        &icon.to_string_lossy().to_string(),
    )?;
    key.set_value("NoModify", &1u32)?;
    key.set_value("NoRepair", &1u32)?;
    Ok(())
}

pub fn unregister_uninstall() -> io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.delete_subkey_all(UNINSTALL_REG_KEY).ok();
    Ok(())
}

pub fn create_terminal_shortcut(install_root: &Path) -> io::Result<()> {
    let menu_dir = start_menu_dir()?;
    fs::create_dir_all(&menu_dir)?;

    let terminal_cmd = install_root.join("bin").join(TERMINAL_CMD);
    let icon = install_root.join("bin").join("niao.exe");
    let shortcut = shortcut_path()?;

    let ps = format!(
        r#"
$shell = New-Object -ComObject WScript.Shell
$lnk = $shell.CreateShortcut('{shortcut}')
$lnk.TargetPath = '{target}'
$lnk.WorkingDirectory = '{work}'
$lnk.IconLocation = '{icon},0'
$lnk.Description = 'Niao programming language terminal'
$lnk.Save()
"#,
        shortcut = ps_escape(&shortcut),
        target = ps_escape(&terminal_cmd),
        work = ps_escape(Path::new(
            &env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into()),
        )),
        icon = ps_escape(&icon),
    );

    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &ps,
        ])
        .status()?;
    if !status.success() {
        return Err(io::Error::other("failed to create Start Menu shortcut"));
    }
    println!(
        "Start Menu: {}\\{}",
        START_MENU_FOLDER, TERMINAL_SHORTCUT
    );
    Ok(())
}

pub fn remove_terminal_shortcut() -> io::Result<()> {
    if let Ok(path) = shortcut_path() {
        fs::remove_file(path).ok();
    }
    if let Ok(dir) = start_menu_dir() {
        let _ = fs::remove_dir(dir);
    }
    Ok(())
}

pub fn schedule_delete_install_dir(install_root: &Path) -> io::Result<()> {
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

pub fn broadcast_env_change() {
    #[cfg(windows)]
    {
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
}

pub fn pause() {
    use std::io::Write;
    print!("\nPress Enter to close...");
    let _ = io::stdout().flush();
    let mut line = String::new();
    let _ = io::stdin().read_line(&mut line);
}

fn ps_escape(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}
