use std::collections::HashMap;

use criterion::{Criterion, criterion_group, criterion_main};
use zsh_clean_history::{CleaningSettings, identify_removals, parse_history_text};

fn gen_corpus(
    success_cmds: &[(&str, usize)],
    failed_cmds: &[(&str, usize)],
) -> (String, HashMap<String, i32>) {
    let mut text = String::new();
    let mut exits: HashMap<String, i32> = HashMap::new();
    let mut ts = 1_000_000_000u64;

    for (cmd, count) in success_cmds {
        for _ in 0..*count {
            text.push_str(&format!(": {ts}:0;{cmd}\n"));
            exits.insert(ts.to_string(), 0);
            ts += 1;
        }
    }
    for (cmd, count) in failed_cmds {
        for _ in 0..*count {
            text.push_str(&format!(": {ts}:0;{cmd}\n"));
            exits.insert(ts.to_string(), 1);
            ts += 1;
        }
    }
    (text, exits)
}

/// 500 distinct git subcommands × 3 successful each = 1 500 entries.
/// 5 typo variants — low near-match density.
fn bench_large_bucket_few_matches(c: &mut Criterion) {
    let success_cmds: Vec<(String, usize)> = (0..500)
        .map(|i| (format!("git subcommand-variant-{i:04}"), 3usize))
        .collect();
    let success_refs: Vec<(&str, usize)> =
        success_cmds.iter().map(|(s, n)| (s.as_str(), *n)).collect();

    let failed: Vec<(&str, usize)> = vec![
        ("git subcommand-variatn-0001", 1),
        ("git subcommand-varaint-0050", 1),
        ("git subcommand-vairant-0100", 1),
        ("git subcommand-variantt-0200", 1),
        ("git subcommand-varinat-0300", 1),
    ];

    let (text, exits) = gen_corpus(&success_refs, &failed);
    let settings = CleaningSettings::default();

    c.bench_function("bench_large_bucket_few_matches", |b| {
        b.iter(|| {
            let parsed = parse_history_text(&text, &exits);
            identify_removals(&parsed, &settings)
        });
    });
}

/// 200 distinct git subcommands × 5 successful each = 1 000 entries.
/// 100 typo variants — high near-match density.
fn bench_large_bucket_many_matches(c: &mut Criterion) {
    let success_cmds: Vec<(String, usize)> = (0..200)
        .map(|i| (format!("git command-{i:03}"), 5usize))
        .collect();
    let success_refs: Vec<(&str, usize)> =
        success_cmds.iter().map(|(s, n)| (s.as_str(), *n)).collect();

    let failed_cmds: Vec<(String, usize)> = (0..100)
        .map(|i| (format!("git command-{i:03x}"), 1usize))
        .collect();
    let failed_refs: Vec<(&str, usize)> =
        failed_cmds.iter().map(|(s, n)| (s.as_str(), *n)).collect();

    let (text, exits) = gen_corpus(&success_refs, &failed_refs);
    let settings = CleaningSettings::default();

    c.bench_function("bench_large_bucket_many_matches", |b| {
        b.iter(|| {
            let parsed = parse_history_text(&text, &exits);
            identify_removals(&parsed, &settings)
        });
    });
}

/// Full 100k-line stress test.
///
/// 50 base commands × 40 subcommand variants × 50 successful each = 100 000 success lines.
/// 200 failed typos spread across bases.
fn bench_100k(c: &mut Criterion) {
    let bases = [
        "git",
        "cargo",
        "docker",
        "kubectl",
        "npm",
        "yarn",
        "pip",
        "go",
        "make",
        "cmake",
        "mvn",
        "gradle",
        "terraform",
        "ansible",
        "helm",
        "aws",
        "gcloud",
        "az",
        "systemctl",
        "journalctl",
        "apt",
        "yum",
        "dnf",
        "brew",
        "port",
        "nix",
        "guix",
        "pacman",
        "zypper",
        "flatpak",
        "snap",
        "conda",
        "mamba",
        "poetry",
        "uv",
        "rustup",
        "rbenv",
        "pyenv",
        "nvm",
        "asdf",
        "mise",
        "direnv",
        "tmux",
        "screen",
        "ssh",
        "scp",
        "rsync",
        "curl",
        "wget",
        "jq",
    ];

    let mut success_cmds: Vec<(String, usize)> = Vec::new();
    for base in &bases {
        for variant in 0..40usize {
            let cmd = format!("{base} action-{variant:02} --flag");
            success_cmds.push((cmd, 50));
        }
    }
    let success_refs: Vec<(&str, usize)> =
        success_cmds.iter().map(|(s, n)| (s.as_str(), *n)).collect();

    let failed_cmds: Vec<(String, usize)> = (0..200)
        .map(|i| {
            let base = bases[i % bases.len()];
            let variant = i % 40;
            let cmd = format!("{base} acniot-{variant:02} --flag");
            (cmd, 1usize)
        })
        .collect();
    let failed_refs: Vec<(&str, usize)> =
        failed_cmds.iter().map(|(s, n)| (s.as_str(), *n)).collect();

    let (text, exits) = gen_corpus(&success_refs, &failed_refs);
    let settings = CleaningSettings::default();

    c.bench_function("bench_100k", |b| {
        b.iter(|| {
            let parsed = parse_history_text(&text, &exits);
            identify_removals(&parsed, &settings)
        });
    });
}

criterion_group!(
    benches,
    bench_large_bucket_few_matches,
    bench_large_bucket_many_matches,
    bench_100k,
);
criterion_main!(benches);
