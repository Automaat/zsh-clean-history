# Code Review Instructions

## Project Context

**Stack:**
- Language: Python 3.11+
- Shell: zsh
- Build/Dev: mise
- Testing: pytest
- Linting: ruff (800+ rules), shellcheck

**Purpose:**
Smart zsh history cleanup plugin - removes typos and failed commands via similarity analysis

**Core Modules:**
- `clean_history.py`: Core cleanup logic (similarity detection, history parsing, exit code tracking)
- `test_clean_history.py`: Comprehensive unit tests
- `zsh-clean-history.plugin.zsh`: Plugin integration (exit tracking, zsh functions)

**Conventions:**
- Error Handling: Context managers for file ops, explicit error suppression with `contextlib.suppress` only for expected errors
- Type Hints: All public functions must have type hints (Python 3.11+ syntax with `|` for unions)
- Testing: pytest, all new features require tests
- Formatting: Ruff auto-format (double quotes, 100 char line length)

**Critical Areas (Extra Scrutiny):**
- History file manipulation (data loss risk)
- Backup creation/restoration
- Exit code tracking (must be accurate)
- Command parsing (handles special chars, multiline)

---

## Review Before CI Completes

You review PRs immediately, before CI finishes. Do NOT flag issues that CI will catch.

**CI Already Checks:**
- Code formatting (ruff format)
- Linting (800+ ruff rules - see pyproject.toml)
- Type checking (no separate type checker, ruff handles)
- Shell linting (shellcheck for .zsh files)
- Test execution (pytest)

---

## Review Priority Levels

### 🔴 CRITICAL (Must Block PR)

**Data Integrity** (95%+ confidence)
- [ ] History file backup before modification
- [ ] Atomic file operations (write to temp, move)
- [ ] No data loss on error/crash
- [ ] Exit code file corruption prevention
- [ ] Proper file encoding (utf-8 with errors='ignore')

**Correctness Issues** (90%+ confidence)
- [ ] Command parsing handles edge cases (multiline, special chars, empty lines)
- [ ] Similarity algorithm logic errors
- [ ] Off-by-one errors in line removal
- [ ] Dict/set operations preserve correct indices
- [ ] Timestamp matching between history and exit codes

### 🟡 HIGH (Request Changes)

**Maintainability** (80%+ confidence)
- [ ] New features have unit tests
- [ ] Functions have type hints
- [ ] Complex logic has inline comments
- [ ] Edge cases tested (empty history, missing exit file, corrupt data)
- [ ] Error messages helpful for users

**Testing** (85%+ confidence)
- [ ] Test coverage for new code paths
- [ ] Tests use proper fixtures (tmp_path, monkeypatch)
- [ ] Tests independent (no shared state)
- [ ] Tests check both success and failure cases

**CLI/UX** (75%+ confidence)
- [ ] Help text clear and accurate
- [ ] Dry-run mode works correctly
- [ ] Stats output helpful
- [ ] Quiet mode suppresses appropriate output

### 🟢 MEDIUM (Suggest/Comment)

**Performance** (70%+ confidence)
- [ ] Large history file handling (10k+ lines)
- [ ] Unnecessary list copies avoided
- [ ] Dict lookups preferred over linear scans
- [ ] File reading efficient (not reading twice)

**Code Quality** (65%+ confidence)
- [ ] Functions under 50 lines (ruff max-statements)
- [ ] Cyclomatic complexity ≤10 (ruff mccabe)
- [ ] Function args ≤5 (ruff max-args)
- [ ] Dataclasses for structured data

### ⚪ LOW (Optional/Skip)

Don't comment on:
- Formatting (ruff format handles)
- Import order (ruff isort handles)
- Docstring style (Google convention enforced)
- Line length (100 chars enforced)
- Quote style (double quotes enforced)

---

## Security Deep Dive

### File Operations
- [ ] Backup created BEFORE modification
- [ ] Temp files in secure location
- [ ] File permissions preserved
- [ ] Symlinks handled safely (or rejected)
- [ ] Path traversal prevented (use Path, no string concat)

### Input Validation
- [ ] Command-line args validated (argparse handles types)
- [ ] Similarity threshold in valid range (0.0-1.0)
- [ ] Rare threshold positive integer
- [ ] File paths validated (exist, readable, writable)

### Data Protection
- [ ] No secrets in history file logged/printed
- [ ] Exit code file not world-readable
- [ ] Backup files same permissions as original

---

## Code Quality Standards

### Naming
- Functions/Variables: `snake_case`
- Classes: `PascalCase`
- Constants: `UPPER_SNAKE_CASE`
- Meaningful names (intent clear without comments)

### Type Hints (Python 3.11+)
- All public functions: full type hints
- Use `|` for unions: `str | None` not `Optional[str]`
- Use `list[str]` not `List[str]` (no typing imports needed)
- Collections from stdlib: `dict`, `set`, `list`, `tuple`

### Error Handling
- Context managers for files: `with open(...) as f:`
- Explicit error suppression: `with contextlib.suppress(ValueError):`
- Never bare `except:`
- Specific exceptions: `FileNotFoundError`, `ValueError`, etc.
- User-facing errors: clear messages, suggest fixes

### Testing Requirements
- **Coverage:** All new functions tested
- **Required tests:**
  - [ ] New features have unit tests
  - [ ] Bug fixes include regression test
  - [ ] Edge cases covered (empty, corrupt, large files)
  - [ ] Error conditions tested
- **Test quality:**
  - Use fixtures: `tmp_path`, `monkeypatch`
  - Isolated (no shared state)
  - Fast (no sleep, mock file I/O if needed)
  - Descriptive names: `test_parse_history_line_with_multiline_command`

### Documentation
- [ ] Public functions have docstrings (Google style)
- [ ] Complex algorithms explained (similarity logic, duplicate removal)
- [ ] Non-obvious decisions have comments
- [ ] README updated if behavior changes
- [ ] CONTRIBUTING.md updated if dev workflow changes

---

## Python-Specific Guidelines

### Modern Python (3.11+)
- Use `|` for unions: `str | None`
- Use `from __future__ import annotations` for forward refs
- Dataclasses for structured data
- Context managers for resources
- Use `Path` not string paths

### Standard Library Preferred
- `difflib.SequenceMatcher` for similarity (already used)
- `collections.Counter` for counting
- `contextlib.suppress` for expected errors
- `pathlib.Path` for file operations
- `re` for parsing (already used)

### Type Safety
- Type hints on all public functions
- Avoid `Any` (project doesn't use it)
- Use specific collection types: `dict[str, int]` not `dict`

### Ruff Compliance
- Project uses 800+ ruff rules (see pyproject.toml)
- Some disabled for good reason:
  - `T201`: Allow print (CLI tool)
  - `D100-D107`: Docstrings on public funcs only
  - `ANN401`: No `Any` allowed
- Test files have relaxed rules (S101 allow assert, no docstrings)

---

## Shell Script Guidelines (zsh)

### zsh-clean-history.plugin.zsh
- [ ] Shellcheck clean
- [ ] Functions use `local` for variables
- [ ] Exit code tracking accurate (`$?` captured immediately)
- [ ] Plugin load/unload safe (no side effects if loaded twice)
- [ ] Environment vars documented

### Common Issues
- **Exit code:** Must capture `$?` IMMEDIATELY after command
- **Quoting:** Variables in double quotes: `"$variable"`
- **Arrays:** Use `()` for array literals
- **Conditionals:** `[[ ]]` not `[ ]`

---

## Architecture Patterns

**Follow these patterns:**
- Dataclasses for structured data (`CleaningSettings`, `CommandData`)
- Pure functions where possible (no side effects)
- Separate parsing from logic (parse_history_line, load_exit_codes)
- CLI parsing in main, logic in functions

**Avoid these anti-patterns:**
- Global state (use function params)
- Monolithic functions (keep under 50 lines)
- Nested conditionals (early returns preferred)
- String parsing with splits (use regex for complex formats)

---

## Review Examples

### ✅ Good: Safe File Operation
```python
# Backup before modification
backup_path = history_file.with_suffix(BACKUP_SUFFIX)
shutil.copy2(history_file, backup_path)

# Write to temp, then atomic move
tmp_path = history_file.with_suffix('.tmp')
with tmp_path.open('w', encoding='utf-8') as f:
    f.writelines(lines_to_keep)
tmp_path.replace(history_file)
```

### ❌ Bad: No Backup
```python
# Direct write - data loss risk if crash
with history_file.open('w', encoding='utf-8') as f:
    f.writelines(lines_to_keep)
```

---

### ✅ Good: Type Hints (Python 3.11+)
```python
def parse_history_line(line: str) -> tuple[str | None, str | None]:
    """Parse a zsh history line."""
    match = re.match(r": (\d+):\d+;(.+)", line)
    if not match:
        return None, None
    return match.groups()
```

### ❌ Bad: No Type Hints
```python
def parse_history_line(line):
    match = re.match(r": (\d+):\d+;(.+)", line)
    if not match:
        return None, None
    return match.groups()
```

---

### ✅ Good: Explicit Error Suppression
```python
with contextlib.suppress(ValueError):
    exit_codes[timestamp] = int(exit_code)
```

### ❌ Bad: Bare Except
```python
try:
    exit_codes[timestamp] = int(exit_code)
except:  # Too broad, hides bugs
    pass
```

---

### ✅ Good: Test with Fixtures
```python
def test_clean_history_creates_backup(tmp_path: Path) -> None:
    history = tmp_path / ".zsh_history"
    history.write_text(": 1234:0;echo test\n")

    clean_history(history, similarity=0.8)

    assert (tmp_path / ".zsh_history.backup").exists()
```

### ❌ Bad: No Isolation
```python
def test_clean_history_creates_backup():
    # Uses real home directory - dangerous!
    clean_history(Path.home() / ".zsh_history", similarity=0.8)
    assert Path.home() / ".zsh_history.backup".exists()
```

---

## Maintainer Priorities

**What matters most:**
1. **Data integrity:** Cannot lose user's history under any circumstance
2. **Correctness:** Similarity algorithm must work as documented
3. **Testing:** All edge cases covered (corrupt files, empty history, etc.)
4. **UX:** Clear output, helpful errors, safe defaults

**Trade-offs we accept:**
- Code verbosity for safety (explicit backups, error handling)
- Some performance for correctness (re-read file if needed)
- Conservative defaults (0.8 similarity, 3 rare threshold)

---

## Confidence Threshold

Only flag issues you're **80% or more confident** about.

If uncertain:
- Phrase as question: "Could this lose data if process crashes?"
- Suggest investigation: "Consider testing with 100k line history"
- Don't block PR on speculation

---

## Review Tone

- **Constructive:** Explain WHY, not just WHAT
- **Specific:** Point to exact file:line
- **Actionable:** Suggest fix or alternative
- **Educational:** This is OSS learning opportunity

**Example:**
❌ "This is unsafe"
✅ "In clean_history.py:142, writing directly to history file without backup. If process crashes mid-write, user loses history. Create backup first with `shutil.copy2(history, backup_path)` before opening for write."

---

## Out of Scope

Do NOT review:
- [ ] Code formatting (ruff format handles)
- [ ] Import ordering (ruff isort handles)
- [ ] Lint warnings (ruff check handles with 800+ rules)
- [ ] Docstring format (Google convention enforced)
- [ ] Personal style preferences
- [ ] Shell formatting (shellcheck handles)

---

## Special Cases

**When PR is:**
- **Hotfix:** Focus only on data integrity + correctness
- **Refactor:** Ensure tests still pass, behavior unchanged
- **New feature:** Require tests, update README
- **Bug fix:** Require regression test
- **Shell script change:** Verify exit code tracking still works

---

## Checklist Summary

Before approving PR, verify:
- [ ] No data loss risk (backup before modify)
- [ ] Tests exist and cover new code
- [ ] Type hints on public functions
- [ ] Edge cases handled (empty, corrupt, large files)
- [ ] Error messages helpful
- [ ] README updated if behavior changes
- [ ] Shell changes don't break exit code tracking
- [ ] No bare except or broad error suppression

---

## Additional Context

**See also:**
- [CONTRIBUTING.md](../CONTRIBUTING.md) - Dev setup, testing, PR guidelines
- [README.md](../README.md) - Usage, configuration, how it works
- [pyproject.toml](../pyproject.toml) - Ruff configuration (800+ rules)

**For questions:** Open issue before major architectural changes
