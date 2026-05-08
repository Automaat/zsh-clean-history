use std::{env, fs, io};

fn main() -> io::Result<()> {
    let out = env::var_os("OUT_DIR").unwrap();
    let comp_dir = std::path::Path::new(&out).join("completions");
    let man_dir = std::path::Path::new(&out).join("man");
    fs::create_dir_all(&comp_dir)?;
    fs::create_dir_all(&man_dir)?;

    let shells = [
        clap_complete::Shell::Bash,
        clap_complete::Shell::Zsh,
        clap_complete::Shell::Fish,
        clap_complete::Shell::Elvish,
        clap_complete::Shell::PowerShell,
    ];
    let mut cmd = cli();
    for shell in shells {
        clap_complete::generate_to(shell, &mut cmd, "zsh-clean-history", &comp_dir)?;
    }

    let man = clap_mangen::Man::new(cli());
    let mut buf = Vec::<u8>::new();
    man.render(&mut buf)?;
    fs::write(man_dir.join("zsh-clean-history.1"), buf)?;

    Ok(())
}

fn cli() -> clap::Command {
    clap::Command::new("zsh-clean-history")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Smart zsh history cleaner - removes typos and failed commands")
        .arg(
            clap::Arg::new("similarity")
                .long("similarity")
                .default_value("0.8")
                .value_parser(clap::value_parser!(f64))
                .help("Similarity threshold (0.0–1.0) for deduplication"),
        )
        .arg(
            clap::Arg::new("rare-threshold")
                .long("rare-threshold")
                .default_value("3")
                .value_parser(clap::value_parser!(usize))
                .help("Commands run fewer than this many times are considered rare"),
        )
        .arg(
            clap::Arg::new("dry-run")
                .long("dry-run")
                .action(clap::ArgAction::SetTrue)
                .help("Print what would be removed without writing changes"),
        )
        .arg(
            clap::Arg::new("quiet")
                .long("quiet")
                .short('q')
                .action(clap::ArgAction::SetTrue)
                .help("Suppress output"),
        )
        .arg(
            clap::Arg::new("remove-rare")
                .long("remove-rare")
                .action(clap::ArgAction::SetTrue)
                .help("Remove commands used fewer than --rare-threshold times"),
        )
        .arg(
            clap::Arg::new("no-log")
                .long("no-log")
                .action(clap::ArgAction::SetTrue)
                .help("Disable cleanup run logging"),
        )
        .arg(
            clap::Arg::new("log-max-bytes")
                .long("log-max-bytes")
                .default_value("1048576")
                .value_parser(clap::value_parser!(u64))
                .help("Max log file size in bytes before rotation (default: 1 MiB)"),
        )
        .subcommand(clap::Command::new("undo").about("Restore history from the latest backup"))
        .subcommand(
            clap::Command::new("record-exit")
                .about("Record exit code of a command (called by the zsh plugin)")
                .arg(clap::Arg::new("timestamp").required(true))
                .arg(
                    clap::Arg::new("exit-code")
                        .required(true)
                        .value_parser(clap::value_parser!(i32)),
                ),
        )
        .subcommand(
            clap::Command::new("explain")
                .about("Explain why a command would be removed")
                .arg(clap::Arg::new("command").required(true)),
        )
}
