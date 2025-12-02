#!/usr/bin/env python3
"""Tests for clean_history.py."""

from collections import Counter, defaultdict
from unittest.mock import mock_open, patch

from clean_history import (
    find_duplicate_indices,
    get_base_command,
    load_exit_codes,
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
            if (get_base_command(failed_cmd) == get_base_command(success_cmd)
                    and success_cmd.startswith(failed_cmd) and success_cmd != failed_cmd
                    and len(success_cmd) > len(failed_cmd) and success_count > fail_count):
                should_remove = True
                break

    assert should_remove
