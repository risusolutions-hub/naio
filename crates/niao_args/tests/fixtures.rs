//! Command trees mirroring `niao_cli` and `niao_nm` clap definitions.

use niao_args::{Arg, Command, NumArgs};

pub fn niao_command(version: &str) -> Command {
    let ahiru_db = Command::new("db").about("Database operations").subcommands(vec![
        Command::new("migrate").arg(project_arg()),
        Command::new("status").arg(project_arg()),
        Command::new("seed").arg(project_arg()),
        Command::new("rollback").arg(project_arg()),
        Command::new("reset")
            .arg(project_arg())
            .arg(Arg::long_flag("force", "force")),
    ]);

    let ahiru_gen = Command::new("generate").subcommand(
        Command::new("resource")
            .arg(Arg::positional("name"))
            .arg(project_arg()),
    );

    let ahiru = Command::new("ahiru")
        .about("ahiru-server backend framework")
        .subcommands(vec![
            Command::new("create")
                .arg(Arg::positional("name"))
                .arg(Arg::long_flag("yes", "yes")),
            Command::new("serve")
                .arg(project_arg())
                .arg(Arg::new("file").long("file"))
                .arg(Arg::new("mode").long("mode").default_value("vm"))
                .arg(Arg::long_flag("dev", "dev"))
                .arg(Arg::long_flag("net", "net"))
                .arg(Arg::new("port").long("port").short('p').num_args(NumArgs::ZeroOrOne)),
            Command::new("bench")
                .arg(
                    Arg::new("routes")
                        .long("routes")
                        .value_delimiter(',')
                        .default_value("health"),
                )
                .arg(Arg::new("concurrency").long("concurrency").default_value("32"))
                .arg(Arg::new("iterations").long("iterations").default_value("5000")),
            Command::new("migrate").arg(project_arg()),
            Command::new("routes").arg(project_arg()),
            ahiru_db,
            Command::new("doctor").arg(project_arg()),
            Command::new("add")
                .arg(Arg::positional("feature"))
                .arg(project_arg()),
            ahiru_gen,
            Command::new("console").arg(project_arg()),
            Command::new("openapi")
                .arg(project_arg())
                .arg(Arg::long_flag("serve", "serve")),
            Command::new("test").arg(project_arg()),
            Command::new("worker").arg(project_arg()),
        ]);

    Command::new("niao")
        .version(version)
        .about("Niao programming language CLI")
        .arg_required_else_help(true)
        .subcommands(vec![
            Command::new("run")
                .arg(Arg::positional("file"))
                .arg(
                    Arg::new("args")
                        .long("")
                        .trailing_var_arg(true)
                        .allow_hyphen_values(true)
                        .num_args(NumArgs::ZeroOrMore),
                )
                .arg(Arg::new("mode").long("mode").default_value("vm"))
                .arg(Arg::long_flag("time", "time").short('t')),
            Command::new("version"),
            Command::new("new").arg(Arg::positional("name")),
            Command::new("test").arg(Arg::positional("dir").default_value("tests")),
            Command::new("format")
                .arg(Arg::positional("file"))
                .arg(Arg::long_flag("write", "write")),
            Command::new("lint").arg(Arg::positional("file")),
            Command::new("docs")
                .arg(Arg::positional("file"))
                .arg(Arg::new("output").long("output").short('o').default_value("docs-output")),
            Command::new("build")
                .arg(Arg::positional("file"))
                .arg(Arg::new("output").long("output").short('o').default_value(".niao-build")),
            Command::new("serve")
                .arg(Arg::positional("file"))
                .arg(Arg::new("port").long("port").short('p').default_value("3000")),
            Command::new("bench")
                .arg(Arg::positional("file"))
                .arg(Arg::new("runs").long("runs").short('r').default_value("5")),
            Command::new("clean")
                .arg(Arg::new("cache_dir").long("cache-dir").short('c').default_value(".niao-build"))
                .arg(Arg::new("keep").long("keep").default_value("16"))
                .arg(Arg::long_flag("all", "all")),
            Command::new("uninstall"),
            Command::new("update")
                .arg(Arg::positional("version").num_args(NumArgs::ZeroOrOne))
                .arg(Arg::long_flag("force", "force")),
            ahiru,
        ])
}

pub fn nm_command(version: &str) -> Command {
    Command::new("nm")
        .version(version)
        .about("Niao package manager — install, uninstall, and manage standard libraries")
        .arg_required_else_help(true)
        .subcommands(vec![
            Command::new("install")
                .aliases(["i", "add"])
                .arg(
                    Arg::new("libs")
                        .long("")
                        .value_name("LIB")
                        .num_args(NumArgs::ZeroOrMore),
                )
                .arg(Arg::long_flag("global", "global"))
                .arg(Arg::long_flag("venv", "venv"))
                .arg(project_arg())
                .arg(Arg::long_flag("force", "force"))
                .arg(Arg::new("source").long("source"))
                .arg(Arg::new("niao_bin").long("niao-bin"))
                .arg(Arg::new("nm_bin").long("nm-bin"))
                .arg(Arg::new("registry").long("registry")),
            Command::new("uninstall")
                .aliases(["rm", "remove", "un"])
                .arg(
                    Arg::new("libs")
                        .long("")
                        .value_name("LIB")
                        .required(true)
                        .num_args(NumArgs::OneOrMore),
                )
                .arg(Arg::long_flag("venv", "venv"))
                .arg(project_arg())
                .arg(Arg::long_flag("force", "force")),
            Command::new("list")
                .aliases(["ls"])
                .arg(Arg::long_flag("installed", "installed"))
                .arg(Arg::long_flag("available", "available"))
                .arg(Arg::long_flag("venv", "venv"))
                .arg(project_arg()),
            Command::new("search")
                .aliases(["find"])
                .arg(Arg::positional("query").num_args(NumArgs::ZeroOrOne))
                .arg(Arg::long_flag("venv", "venv"))
                .arg(project_arg()),
            Command::new("update")
                .aliases(["up"])
                .arg(
                    Arg::new("libs")
                        .long("")
                        .value_name("LIB")
                        .num_args(NumArgs::ZeroOrMore),
                )
                .arg(Arg::long_flag("toolchain", "toolchain"))
                .arg(Arg::new("toolchain_version").long("toolchain-version").value_name("VERSION"))
                .arg(Arg::long_flag("venv", "venv"))
                .arg(project_arg())
                .arg(Arg::long_flag("force", "force"))
                .arg(Arg::new("source").long("source"))
                .arg(Arg::new("registry").long("registry")),
            Command::new("version")
                .arg(Arg::long_flag("venv", "venv"))
                .arg(project_arg()),
            Command::new("info")
                .arg(Arg::positional("name"))
                .arg(Arg::long_flag("venv", "venv"))
                .arg(project_arg()),
            Command::new("venv")
                .arg(project_arg())
                .arg(Arg::long_flag("force", "force")),
            Command::new("home"),
            Command::new("source"),
        ])
}

fn project_arg() -> Arg {
    Arg::new("project").long("project").default_value(".")
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub subcommands: Vec<String>,
    pub values: std::collections::BTreeMap<String, Vec<String>>,
    pub flags: Vec<String>,
}

impl Snapshot {
    pub fn merge(&mut self, other: Snapshot) {
        self.subcommands.extend(other.subcommands);
        for (k, v) in other.values {
            self.values.entry(k).or_default().extend(v);
        }
        self.flags.extend(other.flags);
        self.flags.sort();
        self.flags.dedup();
    }
}

pub fn snapshot_matches(matches: &niao_args::ArgMatches) -> Snapshot {
    let mut snap = Snapshot::default();
    walk_matches(matches, &mut snap);
    snap.flags.sort();
    snap.flags.dedup();
    snap
}

fn walk_matches(matches: &niao_args::ArgMatches, snap: &mut Snapshot) {
    if let Some((sub, m)) = matches.subcommand() {
        snap.subcommands.push(sub.to_string());
        walk_matches(m, snap);
        return;
    }

    for id in [
        "time", "write", "all", "force", "dev", "net", "yes", "serve", "global", "venv",
        "installed", "available", "toolchain",
    ] {
        if matches.get_flag(id) {
            snap.flags.push(id.to_string());
        }
    }

    for id in [
        "file", "name", "dir", "output", "mode", "port", "runs", "cache_dir", "keep", "version",
        "project", "feature", "source", "registry", "niao_bin", "nm_bin", "routes", "concurrency",
        "iterations", "toolchain_version", "query", "libs", "args",
    ] {
        if let Some(vals) = matches.get_many::<String>(id) {
            let collected: Vec<String> = vals.collect();
            if !collected.is_empty() {
                snap.values.insert(id.to_string(), collected);
            }
        } else if let Some(v) = matches.get_one::<String>(id) {
            snap.values.insert(id.to_string(), vec![v]);
        }
    }
}
