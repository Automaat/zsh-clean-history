use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::process::Command as ProcessCommand;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use tempfile::tempdir;
use zsh_clean_history::clean::LockedHistory;

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
fn detected_secret_stays_out_of_output_and_log() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let secret = "token=abcdefgh";
    fs::write(
        home.join(".zsh_history"),
        format!(": 1:0;curl {secret}\n: 2:0;curl {secret}\n: 3:0;curl token=\n"),
    )
    .unwrap();
    fs::write(home.join(".zsh_history_exits"), "1:0\n2:0\n3:1\n").unwrap();

    run(home, &["--dry-run", "--verbose"])
        .success()
        .stdout(predicates::str::contains(secret).not())
        .stdout(predicates::str::contains("<redacted>"));
    let log = fs::read_to_string(home.join(".zsh_history_cleanup.log")).unwrap();
    assert!(!log.contains(secret));
    assert!(log.contains("<redacted>"));
}

#[test]
fn invalid_utf8_does_not_rewrite_history() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let original = b": 1:0;echo \xff\n: 2:0;ls\n: 3:0;ls\n";
    fs::write(home.join(".zsh_history"), original).unwrap();
    fs::write(home.join(".zsh_history_exits"), "1:0\n2:0\n3:0\n").unwrap();

    run(home, &[])
        .failure()
        .stderr(predicates::str::contains("invalid UTF-8"));
    assert_eq!(fs::read(home.join(".zsh_history")).unwrap(), original);
    assert_eq!(
        fs::read_dir(home)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains(".backup-"))
            .count(),
        0
    );
}

#[cfg(unix)]
#[test]
fn existing_log_permissions_are_secured() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    fs::write(home.join(".zsh_history"), ": 1:0;pwd\n").unwrap();
    let log = home.join(".zsh_history_cleanup.log");
    fs::write(&log, "").unwrap();
    fs::set_permissions(&log, fs::Permissions::from_mode(0o644)).unwrap();

    run(home, &["--dry-run"]).success();
    assert_eq!(
        fs::metadata(log).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[cfg(unix)]
#[test]
fn zsh_and_cleaner_use_the_same_lock() {
    let dir = tempdir().unwrap();
    let lock_path = dir.path().join(".zsh_history.cleaner.lock");
    let guard = LockedHistory::acquire(&lock_path).unwrap();
    let attempt = || {
        ProcessCommand::new("zsh")
            .args([
                "-fc",
                "zmodload zsh/system; zsystem flock -t 0 -f fd \"$LOCK_PATH\"",
            ])
            .env("LOCK_PATH", &lock_path)
            .output()
            .unwrap()
            .status
    };
    assert!(!attempt().success());
    drop(guard);
    assert!(attempt().success());
}

#[test]
fn plugin_keeps_share_history_without_incremental_append() {
    let dir = tempdir().unwrap();
    let script = format!(
        "setopt SHARE_HISTORY; source '{}'; [[ -o SHARE_HISTORY && ! -o INC_APPEND_HISTORY ]]",
        concat!(env!("CARGO_MANIFEST_DIR"), "/zsh-clean-history.plugin.zsh")
    );
    let status = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .env("HOME", dir.path())
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn lock_failure_blocks_cleanup_without_changing_history() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let original = b": 1:0;ls\n: 2:0;ls\n";
    fs::write(home.join(".zsh_history"), original).unwrap();
    fs::write(home.join(".zsh_history.cleaner.lock-failed"), "").unwrap();

    run(home, &["--quiet"])
        .failure()
        .stderr(predicates::str::contains("history lock failed"));
    assert_eq!(fs::read(home.join(".zsh_history")).unwrap(), original);
}

#[test]
fn plugin_marks_lock_failure_without_dropping_history() {
    let dir = tempdir().unwrap();
    let script = format!(
        "source '{}'; _zsh_clean_history_lock_supported=false; _zsh_clean_history_before_history 'echo saved\\n'; [[ $? == 2 ]]",
        concat!(env!("CARGO_MANIFEST_DIR"), "/zsh-clean-history.plugin.zsh")
    );
    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .env("HOME", dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(dir.path().join(".zsh_history.cleaner.lock-failed").exists());
    let pending = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains(".cleaner.pending.")
        })
        .unwrap();
    assert_eq!(fs::read_to_string(pending.path()).unwrap(), "echo saved\\n");
}

#[test]
fn plugin_keeps_session_history_when_pending_write_fails() {
    let dir = tempdir().unwrap();
    let script = format!(
        "source '{}'; _zsh_clean_history_lock_supported=false; _zsh_clean_history_pending_file=$HOME; _zsh_clean_history_before_history 'echo saved\\n'; [[ $? == 2 ]]",
        concat!(env!("CARGO_MANIFEST_DIR"), "/zsh-clean-history.plugin.zsh")
    );
    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .env("HOME", dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("kept in session history"));
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

#[cfg(unix)]
#[test]
fn backup_is_private_even_when_history_was_readable() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    let history = home.join(".zsh_history");
    fs::write(&history, ": 1:0;ls\n: 2:0;ls\n").unwrap();
    fs::set_permissions(&history, fs::Permissions::from_mode(0o644)).unwrap();

    run(home, &["--quiet"]).success();
    let backup = fs::read_dir(home)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .find(|entry| entry.file_name().to_string_lossy().contains(".backup-"))
        .unwrap();
    assert_eq!(
        backup.metadata().unwrap().permissions().mode() & 0o777,
        0o600
    );
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
    let backup_contents: Vec<_> = fs::read_dir(home)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains(".backup-"))
        .map(|entry| fs::read_to_string(entry.path()).unwrap())
        .collect();
    assert!(backup_contents.contains(&post));
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
fn retained_entry_without_final_newline_is_unchanged() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    fs::write(home.join(".zsh_history"), b": 1:0;ls\n: 2:0;ls\n: 3:0;pwd").unwrap();

    run(home, &["--quiet"]).success();
    assert_eq!(
        fs::read(home.join(".zsh_history")).unwrap(),
        b": 2:0;ls\n: 3:0;pwd"
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

#[test]
fn dry_run_verbose_shows_sample_removals() {
    let dir = tempdir().unwrap();
    let home = dir.path();
    fs::write(
        home.join(".zsh_history"),
        ": 1:0;git statsu\n: 2:0;git status\n: 3:0;git status\n",
    )
    .unwrap();
    fs::write(home.join(".zsh_history_exits"), "1:1\n2:0\n3:0\n").unwrap();

    run(home, &["--dry-run", "--verbose"])
        .success()
        .stdout(predicates::str::contains("Failed similar to 'git status'"))
        .stdout(predicates::str::contains("git statsu"));
}

#[test]
fn cross_base_typo_removed_in_cleanup() {
    let dir = tempdir().unwrap();
    let home = dir.path();

    let mut history = String::new();
    let mut exits = String::new();
    for i in 1..=20usize {
        history.push_str(&format!(": {i}:0;git status\n"));
        exits.push_str(&format!("{i}:0\n"));
    }
    history.push_str(": 21:0;gti status\n");
    exits.push_str("21:1\n");

    fs::write(home.join(".zsh_history"), &history).unwrap();
    fs::write(home.join(".zsh_history_exits"), &exits).unwrap();

    run(home, &["--dry-run", "--verbose"])
        .success()
        .stdout(predicates::str::contains("gti status"))
        .stdout(predicates::str::contains("Cross-base typo of 'git'"));
}

#[test]
fn cross_base_typo_explain() {
    let dir = tempdir().unwrap();
    let home = dir.path();

    let mut history = String::new();
    let mut exits = String::new();
    for i in 1..=20usize {
        history.push_str(&format!(": {i}:0;git status\n"));
        exits.push_str(&format!("{i}:0\n"));
    }
    history.push_str(": 21:0;gti status\n");
    exits.push_str("21:1\n");

    fs::write(home.join(".zsh_history"), &history).unwrap();
    fs::write(home.join(".zsh_history_exits"), &exits).unwrap();

    run(home, &["explain", "gti status"])
        .success()
        .stdout(predicates::str::contains("Cross-base typo of 'git'"));
}
