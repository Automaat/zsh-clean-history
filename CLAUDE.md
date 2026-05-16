# zsh-clean-history

Smart zsh history cleaner. Removes typos, failed commands, and duplicates from
`~/.zsh_history` via similarity analysis. Rust binary + a zsh plugin wrapper.

## Architecture

Data flow: `zsh-clean-history.plugin.zsh` records exit codes → `clean.rs` orchestrates
a run → `history.rs`/`exits.rs` parse → `cleaner.rs` decides removals → `clean.rs`
writes atomically and logs via `log.rs`.

- **I/O edge** — `clean.rs` owns locking, backup, atomic write, exit-code compaction;
  it is the only module that touches the filesystem for a cleaning run.
- **Pure core** — `cleaner.rs` (removal strategies via `identify_removals`),
  `similarity.rs` (Damerau-Levenshtein, BK-tree), `history.rs`/`exits.rs` (parsers).
  No I/O — keep it that way so they stay property-testable.
- **CLI** — `main.rs` dispatches subcommands; `src/cli_definition.rs` holds the
  `Cli`/`Cmd` types and is `include!()`'d by *both* `main.rs` and `build.rs`
  (`build.rs` generates `completions/*` and `man/zsh-clean-history.1`).
- **Cross-cutting** — `secrets.rs` redacts before `log.rs` serializes;
  `allowlist.rs` filters user-whitelisted commands; `paths.rs` builds HOME-based
  paths; `settings.rs` holds `CleaningSettings`; `lib.rs` re-exports the public surface.

## Tech Stack

Rust edition 2024, MSRV 1.85 (`.mise.toml` pins toolchain 1.95.0). Key deps with
non-obvious roles: `strsim`/`bk-tree` (typo detection), `tempfile`/`fs2` (atomic
writes + locking), `anyhow` (errors), `proptest` (property tests). CI runs
`check.yml` (fmt, clippy, test, `zsh -n`, actionlint) and `audit.yml` (RustSec).

## Common Commands

```bash
mise install          # install pinned toolchain (rust, shellcheck, actionlint)
mise run build        # cargo build --release
mise run test         # cargo test --all-targets
mise run check        # cargo fmt --check + clippy -D warnings + zsh -n + actionlint
mise run fmt          # cargo fmt --all
mise run install      # cargo install --path . --locked
mise run test-cov     # coverage via cargo-llvm-cov
mise run audit        # cargo audit --deny warnings
```

Run a single test: `cargo test --all-targets <name>`.

## Quality Gates

Before committing, all must pass:

- [ ] `mise run check` — fmt, clippy (`-D warnings`), `zsh -n` on the plugin, actionlint
- [ ] `mise run test` — `cargo test --all-targets`
- [ ] New behavior has tests (unit inline, property tests, or `tests/cli.rs`)
- [ ] Bug fixes include a regression test
- [ ] README/CONTRIBUTING updated if observable behavior or dev workflow changed

Never bypass a failing gate: fix the root cause. Do not add clippy-allow attributes,
`--no-verify`, or lint-skip directives.

## Development Workflows

Step-by-step procedures live in
[.claude/rules/development-workflows.md](.claude/rules/development-workflows.md):
add a CLI flag/subcommand, add a removal strategy, change history/exit-code parsing.

## Critical Invariants

History-file manipulation can lose user data. These are non-negotiable:

- **Atomic writes only.** Write via `tempfile::NamedTempFile::new_in(parent)`, call
  `sync_all()`, then `persist()`/rename. The temp file must be in the *same directory*
  as the target so the rename is atomic on one filesystem.
- **Backup before modify.** Create a timestamped backup (`Paths::backup_for`) before
  any write to `~/.zsh_history`.
- **Lock before touch.** Acquire the file lock (`Paths::lock_file`, `fs2`) before any
  history read/write — concurrent shells run this tool.
- **Permissions 0o600.** History, exits, log, and backup files must not be
  world-readable.
- **Redact before logging.** Run `secrets` redaction on command text *before* it is
  serialized into the JSONL log. The log must never leak a secret.
- **BK-tree metric.** The distance function must be a true metric (symmetry, triangle
  inequality) or `bk-tree` lookups silently miss matches.

## Error Handling

- Library code returns `anyhow::Result<T>`; propagate with `?`. No `panic!`.
- Add context at every I/O boundary, including the path:
  `.with_context(|| format!("reading {}", path.display()))`.
- No `.unwrap()` / `.expect()` on user-supplied data (history/exit/log files).
  `.expect("invariant: ...")` is acceptable only on logic-proven-safe paths.
- Match specific `io::ErrorKind`s when recovery differs (`NotFound` vs `PermissionDenied`).

## Anti-Patterns

**AVOID:**

- ❌ Direct `File::create` on the history file — non-atomic, no backup; a crash
  mid-write truncates the user's history. Always temp-file + `sync_all` + `persist`.
- ❌ Importing `zsh_clean_history` items into `src/cli_definition.rs`, or editing
  the `Cli`/`Cmd` definition without rebuilding — `completions/*` and the man page
  drift out of sync because both `main.rs` and `build.rs` `include!()` that file.
- ❌ Writing command text to the JSONL log without running secret redaction first —
  leaks credentials into a persisted file.
- ❌ `.unwrap()` / `.expect()` on parsed file contents — corrupt input becomes a panic
  instead of an actionable error.
- ❌ `static mut` / global mutable state — use `OnceLock`/`LazyLock` for compiled
  regexes, pass other state explicitly.
- ❌ Monolithic functions or match arms 3+ deep — keep functions ~50 lines, flatten
  with `let-else` and helpers.

## Conventions

- Naming: `snake_case` fns/vars, `PascalCase` types, `SCREAMING_SNAKE_CASE` consts.
- Tests: unit tests inline in `#[cfg(test)]`; property tests in a `mod props` block
  using `proptest! {}`; CLI tests in `tests/cli.rs`. Tests use `tempfile::tempdir()`
  for isolation — never write to the real `$HOME`. No `sleep`, no network.
- Pure functions for parsing and algorithms; I/O confined to the edges (`clean.rs`
  orchestrates, `cleaner.rs` is pure).
- `OnceLock` for regexes compiled once per process.
- `unsafe` requires a `// SAFETY:` doc comment justifying soundness.
- New dependency needs justification and must not raise MSRV (1.85) without an
  explicit decision; dev-only deps go in `[dev-dependencies]`.
- Doc comments (`///`) on public functions when the signature alone is unclear;
  comment the *invariant*, not the mechanics.

## Commit & PR

- Conventional commits: `type(scope): description`, scope required, title ≤ 50 chars.
  Types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert.
  Infra scopes: `ci(actions):`, `test(test):`, `docs(docs):`, `build(build):`.
- No PR references or AI attribution in commit messages.
- PR body: `## Motivation`, `## Implementation information`, optional
  `## Supporting documentation`.

## Extensibility

Add sections as the project grows — keep entries concrete (real commands, real file
paths, real function names) and actionable (step-by-step, not descriptive). When a
recurring task is performed 3+ times, capture it as a workflow above.
