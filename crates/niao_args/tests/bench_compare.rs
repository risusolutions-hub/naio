//! Parse throughput report (niao_args vs clap). Run with:
//! `cargo test -p niao_args --release bench_parse_throughput -- --nocapture`

use clap::Parser;
use niao_args::{Arg, Command, NumArgs};
use std::path::PathBuf;
use std::time::Instant;

const ITERS: u32 = 100_000;

#[derive(Parser)]
#[command(name = "niao")]
struct ClapCli {
    #[command(subcommand)]
    command: ClapCmd,
}

#[derive(clap::Subcommand)]
enum ClapCmd {
    Run {
        file: PathBuf,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
        #[arg(long, default_value = "vm")]
        mode: String,
        #[arg(long, short = 't')]
        time: bool,
    },
}

fn niao_cmd() -> Command {
    Command::new("niao").subcommand(
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
    )
}

#[test]
fn bench_parse_throughput() {
    let argv = [
        "niao",
        "run",
        "bench.niao",
        "--mode",
        "vm",
        "-t",
        "--",
        "x",
        "-y",
    ];
    let cmd = niao_cmd();

    let start = Instant::now();
    for _ in 0..ITERS {
        let _ = cmd.clone().try_get_matches_from(argv).unwrap();
    }
    let niao_secs = start.elapsed().as_secs_f64();

    let start = Instant::now();
    for _ in 0..ITERS {
        let _ = ClapCli::parse_from(argv);
    }
    let clap_secs = start.elapsed().as_secs_f64();

    let niao_rate = ITERS as f64 / niao_secs;
    let clap_rate = ITERS as f64 / clap_secs;
    let ratio = niao_rate / clap_rate;

    println!(
        "args_parse_{ITERS}: niao_args {niao_rate:.0} parses/s, clap {clap_rate:.0} parses/s (ratio {ratio:.2}x)"
    );
    assert!(
        ratio > 0.25,
        "niao_args should stay within 4x of clap on parse-once workload"
    );
}
