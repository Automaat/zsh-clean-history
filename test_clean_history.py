#!/usr/bin/env python3
"""Tests for clean_history.py"""

import unittest
from collections import defaultdict, Counter
from unittest.mock import patch, mock_open
from clean_history import (
    parse_history_line,
    similarity,
    get_base_command,
    load_exit_codes,
    find_duplicate_indices,
)


class TestParseHistoryLine(unittest.TestCase):
    """Test parse_history_line function"""

    def test_valid_line(self):
        """Test parsing valid history line"""
        line = ": 1234567890:0;ls -la"
        timestamp, command = parse_history_line(line)
        self.assertEqual(timestamp, "1234567890")
        self.assertEqual(command, "ls -la")

    def test_invalid_line(self):
        """Test parsing invalid line"""
        line = "invalid line"
        timestamp, command = parse_history_line(line)
        self.assertIsNone(timestamp)
        self.assertIsNone(command)

    def test_command_with_spaces(self):
        """Test parsing command with extra spaces"""
        line = ": 1234567890:0;  git status  "
        timestamp, command = parse_history_line(line)
        self.assertEqual(timestamp, "1234567890")
        self.assertEqual(command, "git status")


class TestSimilarity(unittest.TestCase):
    """Test similarity function"""

    def test_identical_strings(self):
        """Test identical strings return 1.0"""
        result = similarity("test", "test")
        self.assertEqual(result, 1.0)

    def test_completely_different(self):
        """Test completely different strings"""
        result = similarity("abc", "xyz")
        self.assertLess(result, 0.5)

    def test_similar_strings(self):
        """Test similar strings"""
        result = similarity("git status", "git statsu")
        self.assertGreater(result, 0.8)


class TestGetBaseCommand(unittest.TestCase):
    """Test get_base_command function"""

    def test_single_word(self):
        """Test single word command"""
        result = get_base_command("ls")
        self.assertEqual(result, "ls")

    def test_command_with_args(self):
        """Test command with arguments"""
        result = get_base_command("git status -s")
        self.assertEqual(result, "git")

    def test_empty_string(self):
        """Test empty string"""
        result = get_base_command("")
        self.assertEqual(result, "")


class TestFindDuplicateIndices(unittest.TestCase):
    """Test find_duplicate_indices function"""

    def test_no_duplicates(self):
        """Test when there are no duplicates"""
        cmd_to_lines = defaultdict(list)
        cmd_to_lines["ls -la"] = [0]
        cmd_to_lines["git status"] = [1]
        seen_commands = {"ls -la": 0, "git status": 1}

        result = find_duplicate_indices(cmd_to_lines, seen_commands)
        self.assertEqual(result, set())

    def test_single_duplicate(self):
        """Test removing single duplicate command"""
        cmd_to_lines = defaultdict(list)
        cmd_to_lines["ls -la"] = [0, 5]
        seen_commands = {"ls -la": 0}

        result = find_duplicate_indices(cmd_to_lines, seen_commands)
        self.assertEqual(result, {5})

    def test_multiple_duplicates(self):
        """Test removing multiple duplicate instances"""
        cmd_to_lines = defaultdict(list)
        cmd_to_lines["ls -la"] = [0, 5, 10, 15]
        seen_commands = {"ls -la": 0}

        result = find_duplicate_indices(cmd_to_lines, seen_commands)
        self.assertEqual(result, {5, 10, 15})

    def test_mixed_commands(self):
        """Test with mix of duplicate and unique commands"""
        cmd_to_lines = defaultdict(list)
        cmd_to_lines["ls -la"] = [0, 5, 10]
        cmd_to_lines["git status"] = [1]
        cmd_to_lines["pwd"] = [2, 7]
        seen_commands = {"ls -la": 0, "git status": 1, "pwd": 2}

        result = find_duplicate_indices(cmd_to_lines, seen_commands)
        self.assertEqual(result, {5, 7, 10})


class TestLoadExitCodes(unittest.TestCase):
    """Test load_exit_codes function"""

    def test_load_valid_exit_codes(self):
        """Test loading valid exit codes"""
        exit_data = "1234567890:0\n1234567891:1\n1234567892:127\n"

        with patch('clean_history.EXIT_FILE') as mock_file:
            mock_file.exists.return_value = True
            with patch('builtins.open', mock_open(read_data=exit_data)):
                result = load_exit_codes()

        self.assertEqual(result["1234567890"], 0)
        self.assertEqual(result["1234567891"], 1)
        self.assertEqual(result["1234567892"], 127)

    def test_load_nonexistent_file(self):
        """Test loading from nonexistent file"""
        with patch('clean_history.EXIT_FILE') as mock_file:
            mock_file.exists.return_value = False
            result = load_exit_codes()

        self.assertEqual(result, {})

    def test_load_invalid_exit_codes(self):
        """Test loading invalid exit codes"""
        exit_data = "1234567890:0\ninvalid:line\n1234567891:abc\n"

        with patch('clean_history.EXIT_FILE') as mock_file:
            mock_file.exists.return_value = True
            with patch('builtins.open', mock_open(read_data=exit_data)):
                result = load_exit_codes()

        self.assertEqual(result["1234567890"], 0)
        self.assertNotIn("invalid", result)
        self.assertNotIn("1234567891", result)


class TestIntegration(unittest.TestCase):
    """Integration tests"""

    def test_duplicate_removal(self):
        """Test that duplicates are identified correctly"""
        cmd_to_lines = defaultdict(list)
        cmd_to_lines["ls -la"] = [0, 5, 10]
        cmd_to_lines["git status"] = [1]

        seen_commands = {"ls -la": 0, "git status": 1}

        lines_to_remove = find_duplicate_indices(cmd_to_lines, seen_commands)

        self.assertEqual(lines_to_remove, {5, 10})

    def test_failed_command_similarity(self):
        """Test failed command similarity detection"""
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

        self.assertTrue(should_remove)

    def test_failed_command_exact_match_not_removed(self):
        """Test that failed commands matching successful ones exactly are not removed"""
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

        self.assertFalse(should_remove)


if __name__ == '__main__':
    unittest.main()
