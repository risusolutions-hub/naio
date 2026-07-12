//! Niao Windows installer — extracts payload, registers app, Start Menu, uninstall entry.

mod common;

use common::{
    add_to_user_path, create_terminal_shortcut, default_install_dir, patch_install_json,
    pause, register_uninstall, APP_NAME, UNINSTALL_EXE,
};
use rust_embed::RustEmbed;
use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(RustEmbed)]
#[folder = "../payload/"]
struct Payload;

fn main() {
    if let Err(e) = run() {
        eprintln!("\nInstall failed: {e}");
        pause();
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    println!("{APP_NAME} {VERSION} Setup");
    println!("====================\n");

    let install_root = default_install_dir();
    let bin_dir = install_root.join("bin");
    fs::create_dir_all(&bin_dir)?;

    let file_count = extract_all(&install_root)?;
    patch_install_json(&install_root)?;
    add_to_user_path(&bin_dir)?;
    register_uninstall(&install_root, VERSION)?;
    create_terminal_shortcut(&install_root)?;

    println!("\nInstalled to: {}", install_root.display());
    println!("  Files:      {file_count}");
    println!("  niao.exe:   {}", bin_dir.join("niao.exe").display());
    println!("  nm.exe:     {}", bin_dir.join("nm.exe").display());
    println!("  uninstall:  {}", bin_dir.join(UNINSTALL_EXE).display());
    println!("  Libraries:  15 standard libs (pre-installed)");

    if let Ok(out) = Command::new(bin_dir.join("niao.exe"))
        .arg("version")
        .output()
    {
        if out.status.success() {
            print!("  Version:    ");
            io::Write::write_all(&mut io::stdout(), &out.stdout)?;
        }
    }

    println!("\nOpen Niao from the Start Menu, or open a NEW terminal and run:");
    println!("  niao version");
    println!("  niao run examples\\hello.niao");
    println!("\nTo uninstall later: niao uninstall  (or Windows Settings → Apps)");
    println!("\nDone.");
    pause();
    Ok(())
}

fn extract_all(root: &Path) -> io::Result<usize> {
    let mut count = 0usize;
    for file in Payload::iter() {
        let rel = file.as_ref();
        if rel.is_empty() {
            continue;
        }
        let dest = root.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = Payload::get(rel).expect("embedded file");
        fs::write(&dest, data.data.as_ref())?;
        count += 1;
        if count % 10 == 0 || rel.ends_with(".exe") || rel.ends_with(".cmd") {
            println!("  {}", rel.replace('/', "\\"));
        }
    }
    Ok(count)
}
