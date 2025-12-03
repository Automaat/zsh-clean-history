# Contributing

Contributions welcome! Follow these guidelines to get started.

## Prerequisites

- Python 3.11+
- [mise](https://mise.jdx.dev/) for dev environment
- zsh for plugin testing

## Setup

```bash
# Install mise if needed
curl https://mise.run | sh

# Install tools and dependencies
mise install
mise run install
```

## Development commands

```bash
# Run tests
mise run test

# Run linters
mise run check

# Manual commands
python -m pytest -v test_clean_history.py   # Run tests
ruff check --fix *.py                       # Lint Python files
ruff format *.py                            # Format Python files
shellcheck *.zsh                            # Lint shell scripts
```

## Testing plugin locally

```bash
# Source plugin in current shell
source zsh-clean-history.plugin.zsh

# Test commands
clean-history-stats                   # Dry run
clean-history                         # Run cleanup
clean-history-info                    # Show config
```

## Code quality

- Pass all Ruff linting checks (800+ rules enabled)
- Add tests for new functionality
- Follow existing code style (auto-formatted with Ruff)
- Keep functions small and focused (max complexity: 10)

## Pull requests

1. Fork and create feature branch
2. Make changes with tests
3. Run `mise run check` and `mise run test`
4. Commit with descriptive message
5. Open PR with:
   - Motivation for change
   - Implementation details
   - Supporting docs/issues

## Commit messages

Follow existing style:
- Imperative mood ("Add feature" not "Added feature")
- Concise subject line
- Include context in body if needed

## Project structure

```
.
├── clean_history.py              # Core cleanup logic
├── test_clean_history.py         # Unit tests
├── zsh-clean-history.plugin.zsh  # Plugin integration
├── .mise.toml                    # Dev environment config
└── .github/workflows/            # CI configuration
```

## Questions?

Open an issue for discussion before major changes.
