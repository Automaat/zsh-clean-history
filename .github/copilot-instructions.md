# Code Review Instructions

## Project Context

**Stack:**
- Language: Rust (edition 2024, MSRV 1.85)
- Shell: zsh
- Build/Dev: mise, Cargo
- Testing: cargo test, proptest (property-based), criterion (benchmarks), assert_cmd (CLI integration)
- Linting: cargo clippy (`-D warnings`), cargo fmt

**Purpose:**
Smart zsh history cleanup plugin — removes typos and failed commands via similarity analysis

**Core Modules:**
- `src/similarity.rs`: Damerau-Levenshtein distance, BK-tree indexing, token normalization
- `src/cleaner.rs`: Removal strategies (dedup, failed-similar-to-successful, cross-base typos, rare variants)
- `src/clean.rs`: Cleanup orchestration — file locking, backup, atomic write, exit code compaction
- `src/history.rs`: zsh history format parser (multi-line, timestamps, exit codes)
- `src/exits.rs`: Exit code file parsing and compaction
- `src/secrets.rs`: Secret pattern detection and redaction
- `src/log.rs`: JSONL logging with rotation and redaction
- `src/allowlist.rs`: User-whitelist regex filtering
- `src/paths.rs`: File path construction
- `src/settings.rs`: Configuration struct
- `src/cli_definition.rs`: Shared CLI definition — `Cli` struct and `Cmd` enum included by both `src/main.rs` and `build.rs` via `include!()`; defines all flags (`--similarity`, `--rare-threshold`, `--dry-run`, `--verbose`, `--quiet`, `--remove-rare`, `--no-log`, `--log-max-bytes`) and subcommands (`Undo`, `RecordExit`, `Explain`)
- `src/main.rs`: CLI entry point — includes `cli_definition.rs`, wires up `clap`, dispatches subcommands
- `tests/cli.rs`: End-to-end CLI tests
- `benches/failed_similar_lookup.rs`: Criterion benchmarks

**Key Dependencies:**
- Error handling: `anyhow` (contextual propagation with `.with_context()`)
- Similarity: `strsim` (Damerau-Levenshtein), `bk-tree` (metric space indexing)
- CLI: `clap` (derive), `clap_complete`, `clap_mangen`
- File ops: `tempfile` (atomic writes), `fs2` (file locking)
- Testing: `proptest`, `assert_cmd`, `predicates`, `criterion`

**Conventions:**
- Error handling: `anyhow::Result<T>` with `.with_context(|| "...")`; no bare `.unwrap()` in library code
- Memory safety: no `unsafe` without a doc comment explaining why it is sound
- Testing: unit tests inline (`#[cfg(test)]`), property tests in `props` submodule, CLI tests in `tests/`
- Formatting: `cargo fmt` (default config, no `rustfmt.toml`)

**Critical Areas (Extra Scrutiny):**
- History file manipulation (data loss risk)
- Backup creation/restoration
- Exit code tracking (must be accurate)
- Command parsing (handles special chars, multiline, quoted args)
- Secret detection/redaction (log output must never leak secrets)

---

## Review Before CI Completes

You review PRs immediately, before CI finishes. Do NOT flag issues that CI will catch.

**CI Already Checks:**
- Formatting (`cargo fmt --check`)
- Linting (`cargo clippy --all-targets -- -D warnings`)
- Tests (`cargo test --all-targets`)
- Benchmark compile (`cargo bench --no-run`)
- Shell syntax (`zsh -n` on `.zsh` files, Linux only)
- Workflow linting (`actionlint`)

---

## Review Priority Levels

### 🔴 CRITICAL (Must Block PR)

**Data Integrity** (95%+ confidence)
- [ ] History file backup before modification
- [ ] Atomic file operations (`tempfile::NamedTempFile` → `persist`/`rename`, never direct write)
- [ ] `sync_all()` called before rename
- [ ] File lock acquired before any history read/write
- [ ] No data loss on error or SIGKILL
- [ ] Exit code file corruption prevention

**Correctness Issues** (90%+ confidence)
- [ ] Command parsing handles edge cases (multiline escapes, quoted args, empty lines)
- [ ] Similarity algorithm logic errors (edit distance, BK-tree radius)
- [ ] Off-by-one errors in line removal or index slicing
- [ ] Timestamp matching between history and exit codes
- [ ] BK-tree distance metric must be a proper metric (symmetry, triangle inequality)

**Memory Safety** (90%+ confidence)
- [ ] No `unsafe` block without soundness justification in doc comment
- [ ] No integer overflow in index arithmetic (use checked ops or assert bounds)

### 🟡 HIGH (Request Changes)

**Maintainability** (80%+ confidence)
- [ ] New features have unit tests
- [ ] Property-based tests for parsing/algorithmic code (use `proptest`)
- [ ] Complex logic has inline comments explaining the invariant, not the mechanics
- [ ] Edge cases tested (empty history, missing exit file, corrupt data, 100k+ lines)
- [ ] Error messages actionable for users

**Testing** (85%+ confidence)
- [ ] Test coverage for new code paths
- [ ] Unit tests use `tempfile::tempdir()` for isolation; no writes to `$HOME`
- [ ] Tests are independent (no shared mutable state)
- [ ] Both success and failure paths covered
- [ ] CLI tests use `assert_cmd::Command` against the compiled binary

**Error Handling** (85%+ confidence)
- [ ] No `.unwrap()` or `.expect()` in library code paths that run on user data
- [ ] `anyhow` context added at call sites: `.with_context(|| format!("reading {}", path.display()))`
- [ ] Errors propagated with `?`; no silent discard
- [ ] File I/O errors include the offending path in the message

**CLI/UX** (75%+ confidence)
- [ ] Help text clear and accurate
- [ ] Dry-run mode correct
- [ ] Stats output helpful
- [ ] Quiet mode suppresses appropriate output
- [ ] `--verbose` / `--quiet` flags behave consistently across all subcommands
- [ ] `--no-log` disables JSONL log writes; `--log-max-bytes` rotation threshold respected
- [ ] `Explain` subcommand output is human-readable and covers all removal reasons
- [ ] Shell completions and man page stay in sync with `src/cli_definition.rs` (regenerated via `build.rs`)

### 🟢 MEDIUM (Suggest/Comment)

**Performance** (70%+ confidence)
- [ ] Large history handling (10k+ lines); BK-tree used for similarity lookups (not O(n²) scan)
- [ ] Unnecessary `clone()` on large strings avoided (prefer `Arc<str>` or borrowing)
- [ ] `OnceLock`/`LazyLock` for regex patterns compiled once per process
- [ ] Allocations in hot loops minimized

**Code Quality** (65%+ confidence)
- [ ] Types encode invariants where possible (newtype wrappers, enums over stringly-typed flags)
- [ ] Iterator chains preferred over manual index loops
- [ ] `match` exhaustive; no wildcard `_` arms that silently ignore variants
- [ ] Functions focused: parsing separated from logic, I/O from computation

### ⚪ LOW (Optional/Skip)

Don't comment on:
- Formatting (`cargo fmt` handles)
- Import order (`rustfmt` handles)
- Clippy warnings (CI blocks on `-D warnings`)
- Naming style (`rustfmt`/clippy enforces snake_case, CamelCase, SCREAMING_SNAKE_CASE)

---

## Security Deep Dive

### File Operations
- [ ] Backup created BEFORE modification
- [ ] Atomic write: `NamedTempFile` in same directory as target (same filesystem → rename is atomic)
- [ ] File permissions preserved (history: 0o600, log: 0o600)
- [ ] Symlinks handled safely or rejected
- [ ] No TOCTOU: acquire lock, then stat; don't stat-then-open
- [ ] Path traversal prevented (use `Path`/`PathBuf`, not string concatenation)

### Input Validation
- [ ] CLI args validated via `clap` type annotations and validators
- [ ] Similarity threshold in valid range (0.0–1.0)
- [ ] Rare threshold positive integer
- [ ] File paths validated (exist, readable, writable) before locking

### Data Protection
- [ ] Secrets never written to log (redaction applied before serialisation)
- [ ] Exit code file not world-readable (0o600)
- [ ] Backup files same permissions as original
- [ ] Log rotation replaces old file atomically

---

## Code Quality Standards

### Naming
- Functions/variables/modules: `snake_case`
- Types/traits: `PascalCase`
- Constants/statics: `SCREAMING_SNAKE_CASE`
- Names express intent; avoid `tmp`, `data`, `info` as sole identifiers

### Error Handling
- Library code: `anyhow::Result<T>`, propagate with `?`
- Add context at I/O boundaries: `.with_context(|| ...)`
- No `.unwrap()` on user-supplied data; `.expect("invariant: ...")` only for logic-proven-safe paths
- No `panic!` in library code; return `Err` instead
- Match on specific error kinds when recovery differs (e.g., `io::ErrorKind::NotFound` vs. `PermissionDenied`)

### Testing Requirements
- **Coverage:** All new public functions tested
- **Required tests:**
  - [ ] New features have unit tests
  - [ ] Bug fixes include regression test
  - [ ] Edge cases: empty input, corrupt data, Unicode, 100k+ entries
  - [ ] Error conditions: missing file, permission denied, corrupt exit-code file
- **Test quality:**
  - Isolated: use `tempfile::tempdir()`, never touch real `$HOME`
  - Fast: no `sleep`, no network
  - Descriptive: `test_parse_history_line_with_multiline_command`
  - Property tests in `mod props` using `proptest! {}` macro

### Documentation
- [ ] Public functions have a doc comment (`///`) when the signature alone doesn't convey intent
- [ ] Complex algorithms explained (similarity scoring, BK-tree radius selection, time-decay weighting)
- [ ] Non-obvious decisions have `// SAFETY:` or `// INVARIANT:` comments
- [ ] README updated if observable behavior changes
- [ ] CONTRIBUTING.md updated if dev workflow changes

---

## Rust-Specific Guidelines

### Ownership & Borrowing
- Prefer borrowing (`&str`, `&[T]`) over cloning in function signatures
- Use `Arc<str>` when cheap cloning across threads is needed (removal reasons)
- Avoid `Rc` in code that may become multi-threaded

### Error Handling Patterns
```rust
// ✅ Context at I/O boundary
fn load_exits(path: &Path) -> anyhow::Result<ExitMap> {
    let data = fs::read_to_string(path)
        .with_context(|| format!("reading exit codes from {}", path.display()))?;
    parse_exits(&data).with_context(|| "parsing exit codes")
}

// ❌ Silent discard
fn load_exits(path: &Path) -> Option<ExitMap> {
    fs::read_to_string(path).ok().and_then(|d| parse_exits(&d).ok())
}
```

### Unsafe
- Every `unsafe` block needs a `// SAFETY:` comment explaining why the invariants hold
- Prefer safe abstractions (`slice::get`, checked arithmetic) over `unsafe` for performance
- Flag any new `unsafe` without a safety comment as CRITICAL

### Modern Rust (Edition 2024)
- Use `impl Trait` in return position for simple cases
- `let-else` for early-exit pattern matching
- `?` in `main` (already returns `anyhow::Result`)
- `OnceLock`/`LazyLock` from `std::sync` for lazy statics; no external `lazy_static`

### Cargo & Dependencies
- New dependency needs justification (functionality + why existing deps don't cover it)
- Prefer `std` over micro-crates for trivial utilities
- Pin MSRV in `Cargo.toml`; new deps must not raise MSRV without explicit decision
- Dev-only deps in `[dev-dependencies]`

### Clippy Compliance
- CI runs `cargo clippy --all-targets -- -D warnings`; all new code must be lint-clean
- Do not `#[allow(clippy::...)]` without a comment explaining why the lint is wrong here
- Common lint families to watch: `clippy::pedantic` patterns (even if not enforced), `clippy::correctness` (always blocked)

---

## Shell Script Guidelines (zsh)

### zsh-clean-history.plugin.zsh
- [ ] `zsh -n` syntax-clean
- [ ] Functions use `local` for variables
- [ ] Exit code tracking accurate (`$?` captured immediately after command)
- [ ] Plugin load/unload safe (no side effects if loaded twice)
- [ ] Environment vars documented

### Common Issues
- **Exit code:** Must capture `$?` immediately after command; any intervening command overwrites it
- **Quoting:** Variables in double quotes: `"$variable"`
- **Arrays:** Use `()` for array literals
- **Conditionals:** `[[ ]]` not `[ ]`

---

## Architecture Patterns

**Follow these patterns:**
- Structs for structured data (`Settings`, `HistoryEntry`, `ExitMap`)
- Pure functions for parsing and algorithms (no I/O side effects)
- Separate parsing from logic (`parse_history_line`, `load_exit_codes`)
- I/O at the edges: `clean.rs` orchestrates; `cleaner.rs` is pure logic
- `OnceLock` for compiled regexes — compile once, reuse across calls

**Avoid these anti-patterns:**
- Global mutable state (`static mut`); use `OnceLock` or pass state explicitly
- Monolithic functions; keep under ~50 lines
- Nested match arms 3+ deep; flatten with `let-else` or helper functions
- String parsing with manual splits when a regex or proper parser is cleaner
- `.clone()` on large `Vec`/`String` inside hot loops

---

## Review Examples

### ✅ Good: Atomic File Operation
```rust
// backup first
let backup = history_path.with_extension("bak");
fs::copy(&history_path, &backup)
    .with_context(|| format!("creating backup at {}", backup.display()))?;

// write to temp in same directory, then rename (atomic on same fs)
let mut tmp = NamedTempFile::new_in(history_path.parent().unwrap())?;
for line in &lines_to_keep {
    writeln!(tmp, "{}", line)?;
}
tmp.as_file().sync_all()?;
tmp.persist(&history_path)
    .with_context(|| "persisting history file")?;
```

### ❌ Bad: Direct Write (No Backup, Not Atomic)
```rust
// data loss if process crashes mid-write
let mut f = File::create(&history_path)?;
for line in &lines_to_keep {
    writeln!(f, "{}", line)?;
}
```

---

### ✅ Good: Error With Context
```rust
let content = fs::read_to_string(&exits_path)
    .with_context(|| format!("reading {}", exits_path.display()))?;
```

### ❌ Bad: Unwrap on User Data
```rust
let content = fs::read_to_string(&exits_path).unwrap();
```

---

### ✅ Good: Unsafe With Safety Justification
```rust
// SAFETY: index is bounds-checked by the caller; len > 0 asserted above
let first = unsafe { slice.get_unchecked(0) };
```

### ❌ Bad: Unsafe Without Justification
```rust
let first = unsafe { slice.get_unchecked(0) };
```

---

### ✅ Good: Property-Based Test
```rust
proptest! {
    #[test]
    fn parse_never_panics(s in ".*") {
        let _ = parse_history_line(&s);
    }
}
```

### ❌ Bad: Only Happy-Path Test
```rust
#[test]
fn test_parse_history_line() {
    assert_eq!(parse_history_line(": 1234:0;echo hi"), Some(("1234", "echo hi")));
}
```

---

### ✅ Good: CLI Integration Test
```rust
#[test]
fn cleanup_creates_backup() {
    let dir = tempdir().unwrap();
    let history = dir.path().join(".zsh_history");
    fs::write(&history, ": 1234:0;echo test\n").unwrap();

    Command::cargo_bin("zsh-clean-history").unwrap()
        .args(["cleanup", "--history", history.to_str().unwrap()])
        .assert()
        .success();

    assert!(dir.path().join(".zsh_history.bak").exists());
}
```

---

## Maintainer Priorities

**What matters most:**
1. **Data integrity:** Cannot lose user's history under any circumstance
2. **Correctness:** Similarity algorithm must work as documented
3. **Testing:** All edge cases covered (corrupt files, empty history, large input)
4. **UX:** Clear output, helpful errors, safe defaults

**Trade-offs we accept:**
- Verbose error handling for safety (explicit context chains)
- Some allocation for correctness (re-read file if lock was lost)
- Conservative defaults (0.8 similarity, 3 rare threshold)
- BK-tree memory overhead for O(log n) similarity search

---

## Confidence Threshold

Only flag issues you're **80% or more confident** about.

If uncertain:
- Phrase as question: "Could this lose data if the process is killed mid-write?"
- Suggest investigation: "Consider testing with a 100k-line history file"
- Don't block PR on speculation

---

## Review Tone

- **Constructive:** Explain WHY, not just WHAT
- **Specific:** Point to exact `file:line`
- **Actionable:** Suggest fix or alternative
- **Educational:** This is OSS; a learning opportunity

**Example:**
❌ "This is unsafe"
✅ "In `clean.rs:142`, writing directly to the history file without a temp file. If the process is killed mid-write the file will be truncated and the user loses history. Use `NamedTempFile::new_in(parent)` + `persist()` instead, and call `sync_all()` before `persist()`."

---

## Out of Scope

Do NOT review:
- [ ] Formatting (`cargo fmt` handles)
- [ ] Import order (`rustfmt` handles)
- [ ] Clippy lints (`cargo clippy -D warnings` blocks CI)
- [ ] Naming style (enforced by compiler + clippy)
- [ ] Personal style preferences

---

## Special Cases

**When PR is:**
- **Hotfix:** Focus only on data integrity + correctness
- **Refactor:** Ensure tests still pass, behavior unchanged, no new `.unwrap()` introduced
- **New feature:** Require tests, update README
- **Bug fix:** Require regression test
- **Shell script change:** Verify exit code tracking still works
- **New dependency:** Require justification; check MSRV impact

---

## Checklist Summary

Before approving PR, verify:
- [ ] No data loss risk (backup before modify, atomic write, `sync_all` before rename)
- [ ] Tests exist and cover new code (unit + edge cases)
- [ ] No `.unwrap()` on user-supplied data
- [ ] No `unsafe` without `// SAFETY:` comment
- [ ] New deps justified and MSRV-compatible
- [ ] Error messages include path/context for actionable debugging
- [ ] README updated if behavior changes
- [ ] Shell changes don't break exit code tracking

---

## Additional Context

**See also:**
- [CONTRIBUTING.md](../CONTRIBUTING.md) — Dev setup, testing, PR guidelines
- [README.md](../README.md) — Usage, configuration, how it works
- [Cargo.toml](../Cargo.toml) — Dependencies and build profiles

**Reference style:** [dtolnay/case-studies](https://github.com/dtolnay/case-studies/) — idiomatic Rust patterns, API design, error handling

**For questions:** Open issue before major architectural changes
