//! Niao Windows uninstaller — used from Settings → Apps and `niao uninstall`.

mod common;

use common::{
    default_install_dir, pause, remove_from_user_path, remove_terminal_shortcut,
    schedule_delete_install_dir, unregister_uninstall, APP_NAME,
};
use std::env;
use std::io::{self, Write};
use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<String> = env::args().collect();
    let quiet = args.iter().any(|a| a == "/quiet" || a == "/S" || a == "--quiet");
    let purge_flag = args.iter().any(|a| a == "--purge");

    if let Err(e) = run(quiet, purge_flag, &args) {
        if !quiet {
            eprintln!("\nUninstall failed: {e}");
            pause();
        }
        std::process::exit(1);
    }
}

fn run(quiet: bool, purge_flag: bool, args: &[String]) -> io::Result<()> {
    let install_root = resolve_install_root(purge_flag, args)?;

    if !quiet && !purge_flag {
        println!("{APP_NAME} {VERSION} Uninstaller\n");
        print!("Remove {APP_NAME} from this computer? [y/N]: ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        if !matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let bin_dir = install_root.join("bin");
    remove_terminal_shortcut()?;
    unregister_uninstall()?;
    remove_from_user_path(&bin_dir)?;
    schedule_delete_install_dir(&install_root)?;
    if !quiet {
        println!("\n{APP_NAME} has been removed.");
        pause();
    }
    Ok(())
}

fn resolve_install_root(purge_flag: bool, args: &[String]) -> io::Result<PathBuf> {
    if purge_flag {
        if let Some(path) = args.iter().position(|a| a == "--purge").and_then(|i| args.get(i + 1)) {
            return Ok(PathBuf::from(path));
        }
    }

    if let Ok(dir) = env::var("NIAO_INSTALL_DIR") {
        return Ok(PathBuf::from(dir));
    }

    let exe = env::current_exe()?;
    if let Some(bin) = exe.parent() {
        if bin.file_name().is_some_and(|n| n == "bin") {
            if let Some(root) = bin.parent() {
                if root.join("install.json").is_file() || root.join("bin").join("uninstall.exe").is_file()
                {
                    return Ok(root.to_path_buf());
                }
            }
        }
    }

    let root = default_install_dir();
    if root.join("install.json").is_file() || root.join("bin").join("uninstall.exe").is_file() {
        return Ok(root);
    }

    // Legacy install location.
    if let Ok(profile) = env::var("USERPROFILE") {
        let legacy = PathBuf::from(profile).join(".niao");
        if legacy.join("install.json").is_file() {
            return Ok(legacy);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Niao installation not found",
    ))
}
