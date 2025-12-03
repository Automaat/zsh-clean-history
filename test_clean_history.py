#!/usr/bin/env python3
"""Tests for clean_history.py."""

from collections import Counter, defaultdict
from unittest.mock import mock_open, patch

import pytest

from clean_history import (
    CleaningSettings,
    CommandData,
    _identify_removals,
    _parse_arguments,
    _parse_history_file,
    _remove_failed_similar_commands,
    _remove_rare_variants,
    find_duplicate_indices,
    get_base_command,
    load_exit_codes,
    main,
    parse_history_line,
    similarity,
)


def test_valid_line() -> None:
    """Test parsing valid history line."""
    line = ": 1234567890:0;ls -la"
    timestamp, command = parse_history_line(line)
    assert timestamp == "1234567890"
    assert command == "ls -la"


def test_invalid_line() -> None:
    """Test parsing invalid line."""
    line = "invalid line"
    timestamp, command = parse_history_line(line)
    assert timestamp is None
    assert command is None


def test_command_with_spaces() -> None:
    """Test parsing command with extra spaces."""
    line = ": 1234567890:0;  git status  "
    timestamp, command = parse_history_line(line)
    assert timestamp == "1234567890"
    assert command == "git status"


def test_identical_strings() -> None:
    """Test identical strings return 1.0."""
    result = similarity("test", "test")
    assert result == 1.0


def test_completely_different() -> None:
    """Test completely different strings."""
    result = similarity("abc", "xyz")
    assert result < 0.5


def test_similar_strings() -> None:
    """Test similar strings."""
    result = similarity("git status", "git statsu")
    assert result > 0.8


def test_single_word() -> None:
    """Test single word command."""
    result = get_base_command("ls")
    assert result == "ls"


def test_command_with_args() -> None:
    """Test command with arguments."""
    result = get_base_command("git status -s")
    assert result == "git"


def test_empty_string() -> None:
    """Test empty string."""
    result = get_base_command("")
    assert result == ""


def test_no_duplicates() -> None:
    """Test when there are no duplicates."""
    cmd_to_lines = defaultdict(list)
    cmd_to_lines["ls -la"] = [0]
    cmd_to_lines["git status"] = [1]
    seen_commands = {"ls -la": 0, "git status": 1}

    result = find_duplicate_indices(cmd_to_lines, seen_commands)
    assert result == set()


def test_single_duplicate() -> None:
    """Test removing single duplicate command."""
    cmd_to_lines = defaultdict(list)
    cmd_to_lines["ls -la"] = [0, 5]
    seen_commands = {"ls -la": 0}

    result = find_duplicate_indices(cmd_to_lines, seen_commands)
    assert result == {5}


def test_multiple_duplicates() -> None:
    """Test removing multiple duplicate instances."""
    cmd_to_lines = defaultdict(list)
    cmd_to_lines["ls -la"] = [0, 5, 10, 15]
    seen_commands = {"ls -la": 0}

    result = find_duplicate_indices(cmd_to_lines, seen_commands)
    assert result == {5, 10, 15}


def test_mixed_commands() -> None:
    """Test with mix of duplicate and unique commands."""
    cmd_to_lines = defaultdict(list)
    cmd_to_lines["ls -la"] = [0, 5, 10]
    cmd_to_lines["git status"] = [1]
    cmd_to_lines["pwd"] = [2, 7]
    seen_commands = {"ls -la": 0, "git status": 1, "pwd": 2}

    result = find_duplicate_indices(cmd_to_lines, seen_commands)
    assert result == {5, 7, 10}


def test_load_valid_exit_codes() -> None:
    """Test loading valid exit codes."""
    exit_data = "1234567890:0\n1234567891:1\n1234567892:127\n"

    with patch("clean_history.EXIT_FILE") as mock_file:
        mock_file.exists.return_value = True
        mock_file.open = mock_open(read_data=exit_data)
        result = load_exit_codes()

    assert result["1234567890"] == 0
    assert result["1234567891"] == 1
    assert result["1234567892"] == 127


def test_load_nonexistent_file() -> None:
    """Test loading from nonexistent file."""
    with patch("clean_history.EXIT_FILE") as mock_file:
        mock_file.exists.return_value = False
        result = load_exit_codes()

    assert result == {}


def test_load_invalid_exit_codes() -> None:
    """Test loading invalid exit codes."""
    exit_data = "1234567890:0\ninvalid:line\n1234567891:abc\n"

    with patch("clean_history.EXIT_FILE") as mock_file:
        mock_file.exists.return_value = True
        mock_file.open = mock_open(read_data=exit_data)
        result = load_exit_codes()

    assert result["1234567890"] == 0
    assert "invalid" not in result
    assert "1234567891" not in result


def test_duplicate_removal() -> None:
    """Test that duplicates are identified correctly."""
    cmd_to_lines = defaultdict(list)
    cmd_to_lines["ls -la"] = [0, 5, 10]
    cmd_to_lines["git status"] = [1]

    seen_commands = {"ls -la": 0, "git status": 1}

    lines_to_remove = find_duplicate_indices(cmd_to_lines, seen_commands)

    assert lines_to_remove == {5, 10}


def test_failed_command_similarity() -> None:
    """Test failed command similarity detection."""
    failed_cmds = Counter({"git statsu": 1})
    successful_cmds = Counter({"git status": 10})

    threshold = 0.8
    should_remove = False

    for failed_cmd in failed_cmds:
        for success_cmd in successful_cmds:
            if get_base_command(failed_cmd) == get_base_command(success_cmd):
                sim = similarity(failed_cmd, success_cmd)
                if threshold <= sim < 1.0:
                    should_remove = True
                    break

    assert should_remove


def test_failed_command_exact_match_not_removed() -> None:
    """Test that failed commands matching successful ones exactly are not removed."""
    failed_cmds = Counter({"git status": 2})
    successful_cmds = Counter({"git status": 10})

    threshold = 0.8
    should_remove = False

    for failed_cmd in failed_cmds:
        for success_cmd in successful_cmds:
            if get_base_command(failed_cmd) == get_base_command(success_cmd):
                sim = similarity(failed_cmd, success_cmd)
                if threshold <= sim < 1.0:
                    should_remove = True
                    break

    assert not should_remove


def test_failed_prefix_removed() -> None:
    """Test that failed commands that are prefixes of successful ones are removed."""
    failed_cmds = Counter({"mise ins": 2})
    successful_cmds = Counter({"mise install": 10})

    should_remove = False
    fail_count = failed_cmds["mise ins"]
    success_count = successful_cmds["mise install"]

    for failed_cmd in failed_cmds:
        for success_cmd in successful_cmds:
            if (
                get_base_command(failed_cmd) == get_base_command(success_cmd)
                and success_cmd.startswith(failed_cmd)
                and success_cmd != failed_cmd
                and len(success_cmd) > len(failed_cmd)
                and success_count > fail_count
            ):
                should_remove = True
                break

    assert should_remove


def test_parse_history_file_success_and_fail() -> None:
    """Test parsing history file with successful and failed commands."""
    history_data = ": 1234567890:0;ls -la\n: 1234567891:0;git status\n: 1234567892:0;pwd\n"
    exit_codes = {"1234567890": 0, "1234567891": 1, "1234567892": 0}

    with patch("clean_history.HISTORY_FILE") as mock_file:
        mock_file.open = mock_open(read_data=history_data)
        all_lines, cmd_to_lines, _seen_commands, successful_cmds, failed_cmds = _parse_history_file(
            exit_codes,
        )

    assert len(all_lines) == 3
    assert "ls -la" in cmd_to_lines
    assert "git status" in cmd_to_lines
    assert successful_cmds["ls -la"] == 1
    assert successful_cmds["pwd"] == 1
    assert failed_cmds["git status"] == 1


def test_parse_history_file_duplicates() -> None:
    """Test parsing history file with duplicate commands."""
    history_data = ": 1234567890:0;ls -la\n: 1234567891:0;ls -la\n: 1234567892:0;ls -la\n"
    exit_codes = {"1234567890": 0, "1234567891": 0, "1234567892": 0}

    with patch("clean_history.HISTORY_FILE") as mock_file:
        mock_file.open = mock_open(read_data=history_data)
        all_lines, cmd_to_lines, seen_commands, successful_cmds, _failed_cmds = _parse_history_file(
            exit_codes,
        )

    assert len(all_lines) == 3
    assert len(cmd_to_lines["ls -la"]) == 3
    assert seen_commands["ls -la"] == 0
    assert successful_cmds["ls -la"] == 3


def test_parse_history_file_invalid_lines() -> None:
    """Test parsing history file with invalid lines."""
    history_data = ": 1234567890:0;ls -la\ninvalid line\n: 1234567891:0;pwd\n"
    exit_codes = {"1234567890": 0, "1234567891": 0}

    with patch("clean_history.HISTORY_FILE") as mock_file:
        mock_file.open = mock_open(read_data=history_data)
        result = _parse_history_file(exit_codes)
        all_lines, cmd_to_lines = result[0], result[1]

    assert len(all_lines) == 3
    assert len(cmd_to_lines) == 2


def test_remove_failed_similar_commands_prefix() -> None:
    """Test removing failed commands that are prefixes."""
    cmd_data = CommandData(
        failed_cmds=Counter({"mise ins": 2}),
        successful_cmds=Counter({"mise install": 10}),
        cmd_to_lines={"mise ins": [0, 1], "mise install": [2, 3]},
    )

    lines_to_remove, removal_reasons = _remove_failed_similar_commands(cmd_data, 0.8)

    assert 0 in lines_to_remove
    assert 1 in lines_to_remove
    assert "Failed prefix" in removal_reasons[0]


def test_remove_failed_similar_commands_similarity() -> None:
    """Test removing failed commands based on similarity."""
    cmd_data = CommandData(
        failed_cmds=Counter({"git statsu": 1}),
        successful_cmds=Counter({"git status": 10}),
        cmd_to_lines={"git statsu": [0], "git status": [1, 2]},
    )

    lines_to_remove, removal_reasons = _remove_failed_similar_commands(cmd_data, 0.8)

    assert 0 in lines_to_remove
    assert "Failed similar" in removal_reasons[0]


def test_remove_failed_similar_commands_different_base() -> None:
    """Test that commands with different base commands are not removed."""
    cmd_data = CommandData(
        failed_cmds=Counter({"ls -la": 1}),
        successful_cmds=Counter({"git status": 10}),
        cmd_to_lines={"ls -la": [0], "git status": [1]},
    )

    lines_to_remove, _removal_reasons = _remove_failed_similar_commands(cmd_data, 0.8)

    assert len(lines_to_remove) == 0


def test_remove_rare_variants_basic() -> None:
    """Test removing rare command variants."""
    cmd_data = CommandData(
        failed_cmds=Counter(),
        successful_cmds=Counter({"git status": 20, "git statsu": 2}),
        cmd_to_lines={"git status": [0, 1, 2], "git statsu": [3, 4]},
    )
    settings = CleaningSettings(similarity_threshold=0.8, rare_threshold=3, remove_rare=True)

    lines_to_remove, removal_reasons = _remove_rare_variants(cmd_data, settings, set())

    assert 3 in lines_to_remove
    assert 4 in lines_to_remove
    assert "Rare variant" in removal_reasons[3]


def test_remove_rare_variants_skip_existing() -> None:
    """Test that rare variants skip already removed lines."""
    cmd_data = CommandData(
        failed_cmds=Counter(),
        successful_cmds=Counter({"git status": 20, "git statsu": 2}),
        cmd_to_lines={"git status": [0, 1, 2], "git statsu": [3, 4]},
    )
    settings = CleaningSettings(similarity_threshold=0.8, rare_threshold=3, remove_rare=True)
    existing_removals = {3}

    lines_to_remove, _removal_reasons = _remove_rare_variants(cmd_data, settings, existing_removals)

    assert 3 not in lines_to_remove
    assert 4 in lines_to_remove


def test_remove_rare_variants_threshold() -> None:
    """Test that commands above rare threshold are not removed."""
    cmd_data = CommandData(
        failed_cmds=Counter(),
        successful_cmds=Counter({"git status": 20, "git statsu": 5}),
        cmd_to_lines={"git status": [0, 1, 2], "git statsu": [3, 4, 5, 6, 7]},
    )
    settings = CleaningSettings(similarity_threshold=0.8, rare_threshold=3, remove_rare=True)

    lines_to_remove, _removal_reasons = _remove_rare_variants(cmd_data, settings, set())

    assert len(lines_to_remove) == 0


def test_identify_removals_all_strategies() -> None:
    """Test removal identification with all strategies enabled."""
    cmd_data = CommandData(
        failed_cmds=Counter({"git statsu": 1}),
        successful_cmds=Counter({"git status": 10, "ls -la": 3}),
        cmd_to_lines={
            "git statsu": [0],
            "git status": [1, 2, 5],
            "ls -la": [3, 4],
        },
    )
    seen_commands = {"git statsu": 0, "git status": 1, "ls -la": 3}
    settings = CleaningSettings(similarity_threshold=0.8, rare_threshold=3, remove_rare=True)

    lines_to_remove, _removal_reasons = _identify_removals(cmd_data, seen_commands, settings)

    assert 0 in lines_to_remove
    assert 2 in lines_to_remove
    assert 4 in lines_to_remove
    assert 5 in lines_to_remove


def test_identify_removals_no_rare() -> None:
    """Test removal identification with rare removal disabled."""
    cmd_data = CommandData(
        failed_cmds=Counter({"git statsu": 1}),
        successful_cmds=Counter({"git status": 10}),
        cmd_to_lines={"git statsu": [0], "git status": [1, 2]},
    )
    seen_commands = {"git statsu": 0, "git status": 1}
    settings = CleaningSettings(similarity_threshold=0.8, rare_threshold=3, remove_rare=False)

    lines_to_remove, _removal_reasons = _identify_removals(cmd_data, seen_commands, settings)

    assert 0 in lines_to_remove
    assert 2 in lines_to_remove


def test_parse_arguments_defaults() -> None:
    """Test argument parsing with defaults."""
    with patch("sys.argv", ["clean_history.py"]):
        settings, args = _parse_arguments()

    assert settings.similarity_threshold == 0.8
    assert settings.rare_threshold == 3
    assert settings.remove_rare is False
    assert args.dry_run is False
    assert args.quiet is False


def test_parse_arguments_custom() -> None:
    """Test argument parsing with custom values."""
    with patch(
        "sys.argv",
        [
            "clean_history.py",
            "--similarity",
            "0.9",
            "--rare-threshold",
            "5",
            "--remove-rare",
            "--dry-run",
            "--quiet",
        ],
    ):
        settings, args = _parse_arguments()

    assert settings.similarity_threshold == 0.9
    assert settings.rare_threshold == 5
    assert settings.remove_rare is True
    assert args.dry_run is True
    assert args.quiet is True


def test_main_dry_run(capsys: pytest.CaptureFixture[str]) -> None:
    """Test main function in dry-run mode."""
    history_data = ": 1234567890:0;git status\n: 1234567891:0;git status\n"
    exit_data = "1234567890:0\n1234567891:0\n"

    with patch("sys.argv", ["clean_history.py", "--dry-run"]), patch(
        "clean_history.HISTORY_FILE"
    ) as mock_history, patch("clean_history.EXIT_FILE") as mock_exit:
        mock_history.open = mock_open(read_data=history_data)
        mock_exit.exists.return_value = True
        mock_exit.open = mock_open(read_data=exit_data)

        main()

    captured = capsys.readouterr()
    assert "Would remove" in captured.out
    assert "Duplicate" in captured.out


def test_main_no_removals(capsys: pytest.CaptureFixture[str]) -> None:
    """Test main function with no commands to remove."""
    history_data = ": 1234567890:0;git status\n"
    exit_data = "1234567890:0\n"

    with patch("sys.argv", ["clean_history.py"]), patch(
        "clean_history.HISTORY_FILE"
    ) as mock_history, patch("clean_history.EXIT_FILE") as mock_exit, patch("shutil.copy2"):
        mock_history.open = mock_open(read_data=history_data)
        mock_exit.exists.return_value = True
        mock_exit.open = mock_open(read_data=exit_data)

        main()

    captured = capsys.readouterr()
    assert "No commands to remove" in captured.out


def test_main_quiet_mode(capsys: pytest.CaptureFixture[str]) -> None:
    """Test main function in quiet mode."""
    history_data = ": 1234567890:0;git status\n: 1234567891:0;git status\n"
    exit_data = "1234567890:0\n1234567891:0\n"

    with patch("sys.argv", ["clean_history.py", "--quiet", "--dry-run"]), patch(
        "clean_history.HISTORY_FILE"
    ) as mock_history, patch("clean_history.EXIT_FILE") as mock_exit:
        mock_history.open = mock_open(read_data=history_data)
        mock_exit.exists.return_value = True
        mock_exit.open = mock_open(read_data=exit_data)

        main()

    captured = capsys.readouterr()
    assert captured.out == ""


def test_main_creates_backup() -> None:
    """Test that main creates a backup file."""
    history_data = ": 1234567890:0;git status\n"
    exit_data = "1234567890:0\n"

    with patch("sys.argv", ["clean_history.py"]), patch(
        "clean_history.HISTORY_FILE"
    ) as mock_history, patch("clean_history.EXIT_FILE") as mock_exit, patch(
        "shutil.copy2"
    ) as mock_copy:
        mock_history.open = mock_open(read_data=history_data)
        mock_exit.exists.return_value = True
        mock_exit.open = mock_open(read_data=exit_data)

        main()

    mock_copy.assert_called_once()


def test_main_writes_cleaned_history() -> None:
    """Test that main writes cleaned history."""
    history_data = ": 1234567890:0;git status\n: 1234567891:0;git status\n: 1234567892:0;pwd\n"
    exit_data = "1234567890:0\n1234567891:0\n1234567892:0\n"

    mock_file_handle = mock_open(read_data=history_data)

    with patch("sys.argv", ["clean_history.py"]), patch(
        "clean_history.HISTORY_FILE"
    ) as mock_history, patch("clean_history.EXIT_FILE") as mock_exit, patch("shutil.copy2"):
        mock_history.open = mock_file_handle
        mock_exit.exists.return_value = True
        mock_exit.open = mock_open(read_data=exit_data)

        main()

    handle = mock_file_handle()
    written_lines = [call.args[0] for call in handle.write.call_args_list]
    assert len(written_lines) == 2
