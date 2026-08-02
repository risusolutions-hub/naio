mod fixtures;

use clap::Parser;
use fixtures::{niao_command, nm_command, snapshot_matches, Snapshot};
use std::path::PathBuf;

const VERSION: &str = "0.2.3";

fn assert_parity_niao(argv: &[&str], clap_snap: Snapshot) {
    let cmd = niao_command(VERSION);
    let owned: Vec<String> = std::iter::once("niao".to_string())
        .chain(argv.iter().map(|s| s.to_string()))
        .collect();
    let matches = cmd.try_get_matches_from(owned).expect("niao_args parse");
    let ours = snapshot_matches(&matches);
    assert_eq!(ours, clap_snap, "argv: niao {}", argv.join(" "));
}

fn assert_parity_nm(argv: &[&str], clap_snap: Snapshot) {
    let cmd = nm_command(VERSION);
    let owned: Vec<String> = std::iter::once("nm".to_string())
        .chain(argv.iter().map(|s| s.to_string()))
        .collect();
    let matches = cmd.try_get_matches_from(owned).expect("niao_args parse");
    let ours = snapshot_matches(&matches);
    assert_eq!(ours, clap_snap, "argv: nm {}", argv.join(" "));
}

#[derive(Parser)]
#[command(name = "niao", arg_required_else_help = true)]
struct NiaoCli {
    #[command(subcommand)]
    command: NiaoCmd,
}

#[derive(clap::Subcommand)]
enum NiaoCmd {
    Run {
        file: PathBuf,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
        #[arg(long, default_value = "vm")]
        mode: String,
        #[arg(long, short = 't')]
        time: bool,
    },
    Version,
    New {
        name: String,
    },
    Test {
        #[arg(default_value = "tests")]
        dir: PathBuf,
    },
    Format {
        file: PathBuf,
        #[arg(long)]
        write: bool,
    },
    Lint {
        file: PathBuf,
    },
    Docs {
        file: PathBuf,
        #[arg(short, long, default_value = "docs-output")]
        output: PathBuf,
    },
    Build {
        file: PathBuf,
        #[arg(short, long, default_value = ".niao-build")]
        output: PathBuf,
    },
    Serve {
        file: PathBuf,
        #[arg(short, long, default_value_t = 3000)]
        port: u16,
    },
    Bench {
        file: PathBuf,
        #[arg(short, long, default_value_t = 5)]
        runs: u32,
    },
    Clean {
        #[arg(short, long, default_value = ".niao-build")]
        cache_dir: PathBuf,
        #[arg(long, default_value_t = 16)]
        keep: usize,
        #[arg(long)]
        all: bool,
    },
    Uninstall,
    Update {
        version: Option<String>,
        #[arg(long)]
        force: bool,
    },
    Ahiru {
        #[command(subcommand)]
        command: AhiruCmd,
    },
}

#[derive(clap::Subcommand)]
enum AhiruCmd {
    Create {
        name: String,
        #[arg(long)]
        yes: bool,
    },
    Serve {
        #[arg(long, default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, default_value = "vm")]
        mode: String,
        #[arg(long)]
        dev: bool,
        #[arg(long)]
        net: bool,
        #[arg(short, long)]
        port: Option<u16>,
    },
    Bench {
        #[arg(long, value_delimiter = ',', default_value = "health")]
        routes: Vec<String>,
        #[arg(long, default_value_t = 32)]
        concurrency: usize,
        #[arg(long, default_value_t = 5000)]
        iterations: usize,
    },
    Migrate {
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    Routes {
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    Db {
        #[command(subcommand)]
        command: AhiruDbCmd,
    },
    Doctor {
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    Add {
        feature: String,
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    Generate {
        #[command(subcommand)]
        command: AhiruGenCmd,
    },
    Console {
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    Openapi {
        #[arg(long, default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        serve: bool,
    },
    Test {
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    Worker {
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
}

#[derive(clap::Subcommand)]
enum AhiruDbCmd {
    Migrate {
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    Status {
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    Seed {
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    Rollback {
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    Reset {
        #[arg(long, default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        force: bool,
    },
}

#[derive(clap::Subcommand)]
enum AhiruGenCmd {
    Resource {
        name: String,
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
}

fn clap_niao(argv: &[&str]) -> Snapshot {
    let owned: Vec<String> = std::iter::once("niao".to_string())
        .chain(argv.iter().map(|s| s.to_string()))
        .collect();
    niao_to_snapshot(NiaoCli::parse_from(owned).command)
}

fn niao_to_snapshot(cmd: NiaoCmd) -> Snapshot {
    let mut snap = Snapshot::default();
    match cmd {
        NiaoCmd::Run {
            file,
            args,
            mode,
            time,
        } => {
            snap.subcommands.push("run".into());
            snap.values
                .insert("file".into(), vec![file.display().to_string()]);
            if !args.is_empty() {
                snap.values.insert("args".into(), args);
            }
            snap.values.insert("mode".into(), vec![mode]);
            if time {
                snap.flags.push("time".into());
            }
        }
        NiaoCmd::Version => snap.subcommands.push("version".into()),
        NiaoCmd::New { name } => {
            snap.subcommands.push("new".into());
            snap.values.insert("name".into(), vec![name]);
        }
        NiaoCmd::Test { dir } => {
            snap.subcommands.push("test".into());
            snap.values
                .insert("dir".into(), vec![dir.display().to_string()]);
        }
        NiaoCmd::Format { file, write } => {
            snap.subcommands.push("format".into());
            snap.values
                .insert("file".into(), vec![file.display().to_string()]);
            if write {
                snap.flags.push("write".into());
            }
        }
        NiaoCmd::Lint { file } => {
            snap.subcommands.push("lint".into());
            snap.values
                .insert("file".into(), vec![file.display().to_string()]);
        }
        NiaoCmd::Docs { file, output } => {
            snap.subcommands.push("docs".into());
            snap.values
                .insert("file".into(), vec![file.display().to_string()]);
            snap.values
                .insert("output".into(), vec![output.display().to_string()]);
        }
        NiaoCmd::Build { file, output } => {
            snap.subcommands.push("build".into());
            snap.values
                .insert("file".into(), vec![file.display().to_string()]);
            snap.values
                .insert("output".into(), vec![output.display().to_string()]);
        }
        NiaoCmd::Serve { file, port } => {
            snap.subcommands.push("serve".into());
            snap.values
                .insert("file".into(), vec![file.display().to_string()]);
            snap.values.insert("port".into(), vec![port.to_string()]);
        }
        NiaoCmd::Bench { file, runs } => {
            snap.subcommands.push("bench".into());
            snap.values
                .insert("file".into(), vec![file.display().to_string()]);
            snap.values.insert("runs".into(), vec![runs.to_string()]);
        }
        NiaoCmd::Clean {
            cache_dir,
            keep,
            all,
        } => {
            snap.subcommands.push("clean".into());
            snap.values
                .insert("cache_dir".into(), vec![cache_dir.display().to_string()]);
            snap.values.insert("keep".into(), vec![keep.to_string()]);
            if all {
                snap.flags.push("all".into());
            }
        }
        NiaoCmd::Uninstall => snap.subcommands.push("uninstall".into()),
        NiaoCmd::Update { version, force } => {
            snap.subcommands.push("update".into());
            if let Some(v) = version {
                snap.values.insert("version".into(), vec![v]);
            }
            if force {
                snap.flags.push("force".into());
            }
        }
        NiaoCmd::Ahiru { command } => {
            snap.subcommands.push("ahiru".into());
            snap.merge(ahiru_to_snapshot(command));
        }
    }
    snap
}

fn ahiru_to_snapshot(cmd: AhiruCmd) -> Snapshot {
    let mut snap = Snapshot::default();
    match cmd {
        AhiruCmd::Create { name, yes } => {
            snap.subcommands.push("create".into());
            snap.values.insert("name".into(), vec![name]);
            if yes {
                snap.flags.push("yes".into());
            }
        }
        AhiruCmd::Serve {
            project,
            file,
            mode,
            dev,
            net,
            port,
        } => {
            snap.subcommands.push("serve".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
            if let Some(f) = file {
                snap.values
                    .insert("file".into(), vec![f.display().to_string()]);
            }
            snap.values.insert("mode".into(), vec![mode]);
            if dev {
                snap.flags.push("dev".into());
            }
            if net {
                snap.flags.push("net".into());
            }
            if let Some(p) = port {
                snap.values.insert("port".into(), vec![p.to_string()]);
            }
        }
        AhiruCmd::Bench {
            routes,
            concurrency,
            iterations,
        } => {
            snap.subcommands.push("bench".into());
            snap.values.insert("routes".into(), routes);
            snap.values
                .insert("concurrency".into(), vec![concurrency.to_string()]);
            snap.values
                .insert("iterations".into(), vec![iterations.to_string()]);
        }
        AhiruCmd::Migrate { project } => {
            snap.subcommands.push("migrate".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        AhiruCmd::Routes { project } => {
            snap.subcommands.push("routes".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        AhiruCmd::Db { command } => {
            snap.subcommands.push("db".into());
            snap.merge(db_to_snapshot(command));
        }
        AhiruCmd::Doctor { project } => {
            snap.subcommands.push("doctor".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        AhiruCmd::Add { feature, project } => {
            snap.subcommands.push("add".into());
            snap.values.insert("feature".into(), vec![feature]);
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        AhiruCmd::Generate { command } => {
            snap.subcommands.push("generate".into());
            snap.merge(gen_to_snapshot(command));
        }
        AhiruCmd::Console { project } => {
            snap.subcommands.push("console".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        AhiruCmd::Openapi { project, serve } => {
            snap.subcommands.push("openapi".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
            if serve {
                snap.flags.push("serve".into());
            }
        }
        AhiruCmd::Test { project } => {
            snap.subcommands.push("test".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        AhiruCmd::Worker { project } => {
            snap.subcommands.push("worker".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
    }
    snap
}

fn db_to_snapshot(cmd: AhiruDbCmd) -> Snapshot {
    let mut snap = Snapshot::default();
    match cmd {
        AhiruDbCmd::Migrate { project } => {
            snap.subcommands.push("migrate".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        AhiruDbCmd::Status { project } => {
            snap.subcommands.push("status".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        AhiruDbCmd::Seed { project } => {
            snap.subcommands.push("seed".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        AhiruDbCmd::Rollback { project } => {
            snap.subcommands.push("rollback".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        AhiruDbCmd::Reset { project, force } => {
            snap.subcommands.push("reset".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
            if force {
                snap.flags.push("force".into());
            }
        }
    }
    snap
}

fn gen_to_snapshot(cmd: AhiruGenCmd) -> Snapshot {
    let mut snap = Snapshot::default();
    match cmd {
        AhiruGenCmd::Resource { name, project } => {
            snap.subcommands.push("resource".into());
            snap.values.insert("name".into(), vec![name]);
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
    }
    snap
}

#[derive(Parser)]
#[command(name = "nm", arg_required_else_help = true)]
struct NmCli {
    #[command(subcommand)]
    command: NmCmd,
}

#[derive(clap::Subcommand)]
enum NmCmd {
    #[command(visible_alias = "i", visible_alias = "add")]
    Install {
        #[arg(value_name = "LIB")]
        libs: Vec<String>,
        #[arg(long)]
        global: bool,
        #[arg(long)]
        venv: bool,
        #[arg(long, default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        source: Option<PathBuf>,
        #[arg(long)]
        niao_bin: Option<PathBuf>,
        #[arg(long)]
        nm_bin: Option<PathBuf>,
        #[arg(long)]
        registry: Option<String>,
    },
    #[command(visible_alias = "rm", visible_alias = "remove", visible_alias = "un")]
    Uninstall {
        #[arg(value_name = "LIB", required = true)]
        libs: Vec<String>,
        #[arg(long)]
        venv: bool,
        #[arg(long, default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        force: bool,
    },
    #[command(visible_alias = "ls")]
    List {
        #[arg(long)]
        installed: bool,
        #[arg(long)]
        available: bool,
        #[arg(long)]
        venv: bool,
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    #[command(visible_alias = "find")]
    Search {
        query: Option<String>,
        #[arg(long)]
        venv: bool,
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    #[command(visible_alias = "up")]
    Update {
        #[arg(value_name = "LIB")]
        libs: Vec<String>,
        #[arg(long)]
        toolchain: bool,
        #[arg(long, value_name = "VERSION")]
        toolchain_version: Option<String>,
        #[arg(long)]
        venv: bool,
        #[arg(long, default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        source: Option<PathBuf>,
        #[arg(long)]
        registry: Option<String>,
    },
    Version {
        #[arg(long)]
        venv: bool,
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    Info {
        name: String,
        #[arg(long)]
        venv: bool,
        #[arg(long, default_value = ".")]
        project: PathBuf,
    },
    Venv {
        #[arg(long, default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        force: bool,
    },
    Home,
    Source,
}

fn clap_nm(argv: &[&str]) -> Snapshot {
    let owned: Vec<String> = std::iter::once("nm".to_string())
        .chain(argv.iter().map(|s| s.to_string()))
        .collect();
    nm_to_snapshot(NmCli::parse_from(owned).command)
}

fn nm_to_snapshot(cmd: NmCmd) -> Snapshot {
    let mut snap = Snapshot::default();
    match cmd {
        NmCmd::Install {
            libs,
            global,
            venv,
            project,
            force,
            source,
            niao_bin,
            nm_bin,
            registry,
        } => {
            snap.subcommands.push("install".into());
            if !libs.is_empty() {
                snap.values.insert("libs".into(), libs);
            }
            if global {
                snap.flags.push("global".into());
            }
            if venv {
                snap.flags.push("venv".into());
            }
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
            if force {
                snap.flags.push("force".into());
            }
            if let Some(s) = source {
                snap.values
                    .insert("source".into(), vec![s.display().to_string()]);
            }
            if let Some(s) = niao_bin {
                snap.values
                    .insert("niao_bin".into(), vec![s.display().to_string()]);
            }
            if let Some(s) = nm_bin {
                snap.values
                    .insert("nm_bin".into(), vec![s.display().to_string()]);
            }
            if let Some(s) = registry {
                snap.values.insert("registry".into(), vec![s]);
            }
        }
        NmCmd::Uninstall {
            libs,
            venv,
            project,
            force,
        } => {
            snap.subcommands.push("uninstall".into());
            snap.values.insert("libs".into(), libs);
            if venv {
                snap.flags.push("venv".into());
            }
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
            if force {
                snap.flags.push("force".into());
            }
        }
        NmCmd::List {
            installed,
            available,
            venv,
            project,
        } => {
            snap.subcommands.push("list".into());
            if installed {
                snap.flags.push("installed".into());
            }
            if available {
                snap.flags.push("available".into());
            }
            if venv {
                snap.flags.push("venv".into());
            }
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        NmCmd::Search {
            query,
            venv,
            project,
        } => {
            snap.subcommands.push("search".into());
            if let Some(q) = query {
                snap.values.insert("query".into(), vec![q]);
            }
            if venv {
                snap.flags.push("venv".into());
            }
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        NmCmd::Update {
            libs,
            toolchain,
            toolchain_version,
            venv,
            project,
            force,
            source,
            registry,
        } => {
            snap.subcommands.push("update".into());
            if !libs.is_empty() {
                snap.values.insert("libs".into(), libs);
            }
            if toolchain {
                snap.flags.push("toolchain".into());
            }
            if let Some(v) = toolchain_version {
                snap.values.insert("toolchain_version".into(), vec![v]);
            }
            if venv {
                snap.flags.push("venv".into());
            }
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
            if force {
                snap.flags.push("force".into());
            }
            if let Some(s) = source {
                snap.values
                    .insert("source".into(), vec![s.display().to_string()]);
            }
            if let Some(s) = registry {
                snap.values.insert("registry".into(), vec![s]);
            }
        }
        NmCmd::Version { venv, project } => {
            snap.subcommands.push("version".into());
            if venv {
                snap.flags.push("venv".into());
            }
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        NmCmd::Info {
            name,
            venv,
            project,
        } => {
            snap.subcommands.push("info".into());
            snap.values.insert("name".into(), vec![name]);
            if venv {
                snap.flags.push("venv".into());
            }
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
        }
        NmCmd::Venv { project, force } => {
            snap.subcommands.push("venv".into());
            snap.values
                .insert("project".into(), vec![project.display().to_string()]);
            if force {
                snap.flags.push("force".into());
            }
        }
        NmCmd::Home => snap.subcommands.push("home".into()),
        NmCmd::Source => snap.subcommands.push("source".into()),
    }
    snap
}

#[test]
fn niao_run_basic() {
    let argv = &["run", "hello.niao"];
    assert_parity_niao(argv, clap_niao(argv));
}

#[test]
fn niao_run_with_script_args_and_time() {
    let argv = &["run", "app.niao", "--", "foo", "-bar"];
    assert_parity_niao(argv, clap_niao(argv));
}

#[test]
fn niao_run_time_short() {
    let argv = &["run", "app.niao", "-t"];
    assert_parity_niao(argv, clap_niao(argv));
}

#[test]
fn niao_serve_port() {
    let argv = &["serve", "web.niao", "-p", "8080"];
    assert_parity_niao(argv, clap_niao(argv));
}

#[test]
fn niao_ahiru_nested_db_migrate() {
    let argv = &["ahiru", "db", "migrate", "--project", "myapp"];
    assert_parity_niao(argv, clap_niao(argv));
}

#[test]
fn niao_ahiru_bench_routes() {
    let argv = &["ahiru", "bench", "--routes", "a,b,c"];
    assert_parity_niao(argv, clap_niao(argv));
}

#[test]
fn niao_clean_defaults() {
    let argv = &["clean"];
    assert_parity_niao(argv, clap_niao(argv));
}

#[test]
fn nm_install_alias() {
    let argv = &["i", "json", "io", "--global"];
    assert_parity_nm(argv, clap_nm(argv));
}

#[test]
fn nm_uninstall_required_libs() {
    let argv = &["rm", "json"];
    assert_parity_nm(argv, clap_nm(argv));
}

#[test]
fn nm_search_find_alias() {
    let argv = &["find", "http", "--venv"];
    assert_parity_nm(argv, clap_nm(argv));
}

#[test]
fn nm_update_toolchain() {
    let argv = &["up", "--toolchain", "--toolchain-version", "0.2.3"];
    assert_parity_nm(argv, clap_nm(argv));
}

#[test]
fn nm_info_name() {
    let argv = &["info", "json"];
    assert_parity_nm(argv, clap_nm(argv));
}

#[test]
fn help_on_empty_niao() {
    let cmd = niao_command(VERSION);
    let err = cmd.try_get_matches_from(["niao"]).unwrap_err();
    assert_eq!(err.kind, niao_args::ErrorKind::DisplayHelp);
}

#[test]
fn version_flag_niao() {
    let cmd = niao_command(VERSION);
    let err = cmd.try_get_matches_from(["niao", "--version"]).unwrap_err();
    assert_eq!(err.kind, niao_args::ErrorKind::DisplayVersion);
    assert_eq!(err.message, VERSION);
}
