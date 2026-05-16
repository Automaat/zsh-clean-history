# Development Workflows

Step-by-step procedures for recurring changes. Referenced from `CLAUDE.md`.

## Add a CLI flag or subcommand

1. Edit `src/cli_definition.rs` — add the `#[arg(...)]` field to `Cli` or a variant to `Cmd`.
   Keep this file free of any `zsh_clean_history` crate imports: `build.rs` `include!()`s
   it and a crate dependency would create a circular build.
2. Wire it in `src/main.rs` — `match cli.cmd` arm for a subcommand, or thread the flag
   through `settings_from_cli` / the relevant function.
3. Rebuild — `cargo build` runs `build.rs`, which regenerates `completions/*` and
   `man/zsh-clean-history.1`. Commit the regenerated files alongside the code change.
4. If the flag has shell-facing behavior, expose it in `zsh-clean-history.plugin.zsh`
   and document it in README (`## CLI flags` / `## Configuration`).
5. Add a `tests/cli.rs` test exercising the flag against the compiled binary.

## Add a removal strategy

1. Add a private `fn` in `src/cleaner.rs` following `failed_similar_to_successful` /
   `cross_base_typos` — take `&ParsedHistory` + `&mut HashMap<usize, Arc<str>>`,
   stay pure (no I/O).
2. Call it from `identify_removals`. Respect existing entries: do not overwrite a
   removal another strategy already recorded (see `cross_base_does_not_override_existing`).
3. The removal reason is an `Arc<str>` — keep it short and human-readable; it surfaces
   in the JSONL log and the `Explain` subcommand.
4. Add unit tests in the `#[cfg(test)]` module and a `proptest!` in `mod props` for
   any new parsing/scoring logic. Cover false-positive boundaries (short commands,
   threshold edges) as the existing `cross_base_*` tests do.
5. If user-gated, add a `CleaningSettings` field + a `--flag` (see workflow above).

## Change history or exit-code parsing

1. Edit `src/history.rs` (`parse_history_*`) or `src/exits.rs`. Keep parsers pure
   functions separate from I/O.
2. Handle every edge case: empty lines, multi-line commands with `\` escapes,
   quoted args, missing/zero timestamps, corrupt exit-code lines.
3. Add a `proptest!` asserting the parser never panics on arbitrary input
   (`s in ".*"`), plus targeted unit tests for each format variant.
4. Verify timestamp matching between history entries and exit codes still holds —
   exit-code attribution depends on it.
5. Touching the on-disk format means existing user files must still parse; add a
   regression test with a sample of the old format.
