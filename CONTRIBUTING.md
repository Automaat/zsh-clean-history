# Contributing

## Prerequisites

- Rust 1.85+
- [mise](https://mise.jdx.dev/) for dev environment
- zsh for plugin testing

## Setup

```bash
mise install
cargo build
```

## Development

```bash
mise run test       # cargo test --all-targets
mise run check      # fmt-check + clippy + zsh -n + actionlint
mise run fmt        # cargo fmt --all
mise run install    # install binary to ~/.cargo/bin
```

## Testing the plugin locally

```bash
cargo build --release
source zsh-clean-history.plugin.zsh
clean-history-stats
clean-history-info
```

## Code quality

- `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings` must pass
- Add tests for new behavior in either the relevant `src/<module>.rs` test module or `tests/cli.rs`
- Keep modules focused

## Pull requests

1. Fork and create a feature branch
2. Make changes with tests
3. Run `mise run check` and `mise run test`
4. Open a PR with:
   - Motivation
   - Implementation details
   - Supporting docs/issues

## Releases

1. Run the Release workflow from `main`
2. `prepare` computes the version and refuses existing tags
3. Build jobs bump the version locally and upload archives/checksums
4. `release` verifies every artifact before pushing the version commit/tag
5. `release` creates the GitHub release from verified artifacts

Failed builds leave `main` and tags unchanged.

## Project structure

```
.
├── Cargo.toml
├── src/
│   ├── main.rs            # CLI entry point
│   ├── lib.rs             # Public surface
│   ├── cleaner.rs         # Removal strategies
│   ├── history.rs         # ~/.zsh_history parser
│   ├── exits.rs           # ~/.zsh_history_exits I/O
│   ├── log.rs             # JSONL run log
│   ├── paths.rs           # XDG/HOME path resolution
│   ├── settings.rs        # CleaningSettings
│   └── similarity.rs      # Damerau-Levenshtein ratio
├── tests/cli.rs           # End-to-end CLI tests
├── zsh-clean-history.plugin.zsh
└── .github/workflows/check.yml
```
