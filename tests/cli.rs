use std::fs;

use assert_cmd::Command;
use tempfile::tempdir;

fn run(home: &std::path::Path, args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("zsh-clean-history")
        .unwrap()
        .env("HOME", home)
        .args(args)
        .assert()
}

#[test]
fn dry_run_prints_summary_and_writes_log() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    fs::write(
        home.join(".zsh_history"),
        ": 1:0;git status\n: 2:0;git status\n: 3:0;pwd\n",
    )
    .unwrap();
    fs::write(home.join(".zsh_history_exits"), "1:0\n2:0\n3:0\n").unwrap();

    run(home, &["--dry-run"])
        .success()
        .stdout(predicates::str::contains("Would remove"));

    let history_after = fs::read_to_string(home.join(".zsh_history")).unwrap();
    assert!(history_after.contains("git status"));
    assert!(history_after.contains("pwd"));

    let log = fs::read_to_string(home.join(".zsh_history_cleanup.log")).unwrap();
    assert!(log.contains("\"dry_run\":true"));
}

#[test]
fn applies_dedup_keeping_newest() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    fs::write(home.join(".zsh_history"), ": 1:0;ls\n: 2:0;pwd\n: 3:0;ls\n").unwrap();
    fs::write(home.join(".zsh_history_exits"), "1:0\n2:0\n3:0\n").unwrap();

    run(home, &["--quiet"]).success();

    let after = fs::read_to_string(home.join(".zsh_history")).unwrap();
    let lines: Vec<&str> = after.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], ": 2:0;pwd");
    assert_eq!(lines[1], ": 3:0;ls");
}

#[test]
fn record_exit_appends_to_exits_file() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    run(home, &["record-exit", "1700000000", "0"]).success();
    run(home, &["record-exit", "1700000001", "127"]).success();
    let body = fs::read_to_string(home.join(".zsh_history_exits")).unwrap();
    assert!(body.contains("1700000000:0"));
    assert!(body.contains("1700000001:127"));
}

#[test]
fn undo_restores_latest_backup() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    fs::write(home.join(".zsh_history"), ": 1:0;ls\n: 2:0;ls\n: 3:0;pwd\n").unwrap();
    fs::write(home.join(".zsh_history_exits"), "1:0\n2:0\n3:0\n").unwrap();
    let pre = fs::read_to_string(home.join(".zsh_history")).unwrap();

    run(home, &["--quiet"]).success();
    let post = fs::read_to_string(home.join(".zsh_history")).unwrap();
    assert_ne!(pre, post);

    run(home, &["undo"]).success();
    let restored = fs::read_to_string(home.join(".zsh_history")).unwrap();
    assert_eq!(restored, pre);
}

#[test]
fn multiline_entry_round_trips_unchanged() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let original = ": 1:0;echo foo \\
  bar
: 2:0;ls
: 3:0;ls
";
    fs::write(home.join(".zsh_history"), original).unwrap();
    fs::write(home.join(".zsh_history_exits"), "1:0\n2:0\n3:0\n").unwrap();

    run(home, &["--quiet"]).success();

    let after = fs::read_to_string(home.join(".zsh_history")).unwrap();
    assert!(
        after.contains(": 1:0;echo foo \\\n  bar"),
        "multi-line entry corrupted: {after:?}"
    );
}

#[test]
fn compaction_runs_even_when_no_removals() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    fs::write(home.join(".zsh_history"), ": 1:0;ls\n: 2:0;pwd\n").unwrap();
    fs::write(
        home.join(".zsh_history_exits"),
        "1:0\n2:0\n9999:0\n8888:1\n",
    )
    .unwrap();

    run(home, &["--quiet"]).success();

    let exits_after = fs::read_to_string(home.join(".zsh_history_exits")).unwrap();
    assert!(exits_after.contains("1:0"));
    assert!(exits_after.contains("2:0"));
    assert!(!exits_after.contains("9999"));
    assert!(!exits_after.contains("8888"));
}
