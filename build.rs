use std::{env, fs, io};

use clap::{CommandFactory, Parser, Subcommand};

include!("src/cli_definition.rs");

fn main() -> io::Result<()> {
    let manifest = env::var_os("CARGO_MANIFEST_DIR").unwrap();
    let comp_dir = std::path::Path::new(&manifest).join("completions");
    let man_dir = std::path::Path::new(&manifest).join("man");
    fs::create_dir_all(&comp_dir)?;
    fs::create_dir_all(&man_dir)?;

    let shells = [
        clap_complete::Shell::Bash,
        clap_complete::Shell::Zsh,
        clap_complete::Shell::Fish,
        clap_complete::Shell::Elvish,
        clap_complete::Shell::PowerShell,
    ];
    let mut cmd = Cli::command();
    for shell in shells {
        clap_complete::generate_to(shell, &mut cmd, "zsh-clean-history", &comp_dir)?;
    }

    let man = clap_mangen::Man::new(Cli::command());
    let mut buf = Vec::<u8>::new();
    man.render(&mut buf)?;
    fs::write(man_dir.join("zsh-clean-history.1"), buf)?;

    Ok(())
}
