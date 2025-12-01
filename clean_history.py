#!/usr/bin/env python3
"""Smart history cleaner that removes likely typos.

Based on:
1. Commands that failed and have similar successful variants
2. Commands that appear very rarely compared to similar common commands.
"""

from __future__ import annotations

import argparse
import contextlib
import re
import shutil
from collections import Counter, defaultdict
from dataclasses import dataclass
from difflib import SequenceMatcher
from pathlib import Path

HISTORY_FILE = Path.home() / ".zsh_history"
EXIT_FILE = Path.home() / ".zsh_history_exits"
BACKUP_SUFFIX = ".backup"
SAMPLE_SIZE = 20  # Number of sample removals to show


@dataclass
class CleaningSettings:
    """Settings for history cleaning."""

    similarity_threshold: float
    rare_threshold: int
    remove_rare: bool


@dataclass
class CommandData:
    """Command statistics and line mappings."""

    failed_cmds: Counter[str]
    successful_cmds: Counter[str]
    cmd_to_lines: dict[str, list[int]]


def load_exit_codes() -> dict[str, int]:
    """Load exit codes from separate file."""
    exit_codes: dict[str, int] = {}
    if not EXIT_FILE.exists():
        return exit_codes

    with EXIT_FILE.open(encoding="utf-8", errors="ignore") as f:
        for raw_line in f:
            line = raw_line.strip()
            if ":" in line:
                timestamp, exit_code = line.split(":", 1)
                with contextlib.suppress(ValueError):
                    exit_codes[timestamp] = int(exit_code)
    return exit_codes


def parse_history_line(line: str) -> tuple[str | None, str | None]:
    """Parse a zsh history line."""
    # Format: : timestamp:duration;command
    match = re.match(r": (\d+):\d+;(.+)", line)
    if not match:
        return None, None

    timestamp, command = match.groups()
    return timestamp, command.strip()


def similarity(a: str, b: str) -> float:
    """Calculate similarity ratio between two strings."""
    return SequenceMatcher(None, a, b).ratio()


def get_base_command(cmd: str) -> str:
    """Extract base command (first word) for grouping."""
    return cmd.split()[0] if cmd.split() else cmd


def find_duplicate_indices(
    cmd_to_lines: dict[str, list[int]],
    seen_commands: dict[str, int],
) -> set[int]:
    """Find indices of duplicate commands, keeping only first occurrence.

    Args:
        cmd_to_lines: Dict mapping commands to list of line indices
        seen_commands: Dict mapping commands to their first occurrence index

    Returns:
        Set of line indices to remove
    """
    lines_to_remove: set[int] = set()
    for command, indices in cmd_to_lines.items():
        if len(indices) > 1:
            first_idx = seen_commands[command]
            for idx in indices:
                if idx != first_idx:
                    lines_to_remove.add(idx)
    return lines_to_remove


def _parse_history_file(
    exit_codes: dict[str, int],
) -> tuple[list[str], dict[str, list[int]], dict[str, int], Counter[str], Counter[str]]:
    """Parse history file and collect command statistics.

    Args:
        exit_codes: Dict mapping timestamps to exit codes

    Returns:
        Tuple of (all_lines, cmd_to_lines, seen_commands, successful_cmds, failed_cmds)
    """
    successful_cmds: Counter[str] = Counter()
    failed_cmds: Counter[str] = Counter()
    all_lines: list[str] = []
    cmd_to_lines: dict[str, list[int]] = defaultdict(list)
    seen_commands: dict[str, int] = {}

    with HISTORY_FILE.open(encoding="utf-8", errors="ignore") as f:
        for idx, raw_line in enumerate(f):
            line = raw_line.rstrip("\n")
            all_lines.append(line)

            timestamp, command = parse_history_line(line)
            if command and timestamp:
                cmd_to_lines[command].append(idx)

                if command not in seen_commands:
                    seen_commands[command] = idx

                exit_code = exit_codes.get(timestamp)
                if exit_code == 0:
                    successful_cmds[command] += 1
                elif exit_code is not None and exit_code != 0:
                    failed_cmds[command] += 1

    return all_lines, cmd_to_lines, seen_commands, successful_cmds, failed_cmds


def _remove_failed_similar_commands(
    cmd_data: CommandData,
    similarity_threshold: float,
) -> tuple[set[int], dict[int, str]]:
    """Remove failed commands similar to successful ones."""
    lines_to_remove: set[int] = set()
    removal_reasons: dict[int, str] = {}

    for failed_cmd, fail_count in cmd_data.failed_cmds.items():
        base_cmd = get_base_command(failed_cmd)

        for success_cmd, success_count in cmd_data.successful_cmds.items():
            if get_base_command(success_cmd) != base_cmd:
                continue

            # Check if failed command is a prefix of successful one
            if (success_cmd.startswith(failed_cmd) and success_cmd != failed_cmd
                    and len(success_cmd) > len(failed_cmd) and success_count > fail_count):
                for idx in cmd_data.cmd_to_lines[failed_cmd]:
                    lines_to_remove.add(idx)
                    removal_reasons[idx] = f"Failed prefix of '{success_cmd}'"
                break

            # Check similarity
            sim = similarity(failed_cmd, success_cmd)
            if similarity_threshold <= sim < 1.0 and success_count > fail_count:
                for idx in cmd_data.cmd_to_lines[failed_cmd]:
                    lines_to_remove.add(idx)
                    removal_reasons[idx] = f"Failed similar to '{success_cmd}'"
                break

    return lines_to_remove, removal_reasons


def _remove_rare_variants(
    cmd_data: CommandData,
    settings: CleaningSettings,
    existing_removals: set[int],
) -> tuple[set[int], dict[int, str]]:
    """Remove rare command variants similar to common ones."""
    lines_to_remove: set[int] = set()
    removal_reasons: dict[int, str] = {}

    all_cmds: Counter[str] = Counter()
    all_cmds.update(cmd_data.successful_cmds)
    all_cmds.update(cmd_data.failed_cmds)

    for rare_cmd, rare_count in all_cmds.items():
        if rare_count <= settings.rare_threshold:
            base_cmd = get_base_command(rare_cmd)

            for common_cmd, common_count in all_cmds.items():
                if common_count > rare_count * 3 and get_base_command(common_cmd) == base_cmd:
                    sim = similarity(rare_cmd, common_cmd)

                    if sim >= settings.similarity_threshold:
                        for idx in cmd_data.cmd_to_lines[rare_cmd]:
                            if idx not in existing_removals:
                                lines_to_remove.add(idx)
                                removal_reasons[idx] = f"Rare variant of '{common_cmd}'"
                        break

    return lines_to_remove, removal_reasons


def _identify_removals(
    cmd_data: CommandData,
    seen_commands: dict[str, int],
    settings: CleaningSettings,
) -> tuple[set[int], dict[int, str]]:
    """Identify which lines to remove and why."""
    lines_to_remove: set[int] = set()
    removal_reasons: dict[int, str] = {}

    # Strategy 0: Remove duplicate commands
    duplicate_indices = find_duplicate_indices(cmd_data.cmd_to_lines, seen_commands)
    for idx in duplicate_indices:
        lines_to_remove.add(idx)
        removal_reasons[idx] = "Duplicate"

    # Strategy 1: Remove failed commands similar to successful ones
    failed_removals, failed_reasons = _remove_failed_similar_commands(
        cmd_data,
        settings.similarity_threshold,
    )
    lines_to_remove.update(failed_removals)
    removal_reasons.update(failed_reasons)

    # Strategy 2: Remove rare commands similar to common ones
    if settings.remove_rare:
        rare_removals, rare_reasons = _remove_rare_variants(
            cmd_data,
            settings,
            lines_to_remove,
        )
        lines_to_remove.update(rare_removals)
        removal_reasons.update(rare_reasons)

    return lines_to_remove, removal_reasons


def _parse_arguments() -> tuple[CleaningSettings, argparse.Namespace]:
    """Parse command-line arguments and create settings."""
    parser = argparse.ArgumentParser(
        description="Smart zsh history cleaner - removes typos and failed commands",
    )
    parser.add_argument(
        "--similarity",
        type=float,
        default=0.8,
        help="Similarity threshold (0-1, default: 0.8)",
    )
    parser.add_argument(
        "--rare-threshold",
        type=int,
        default=3,
        help="Max occurrences to consider rare (default: 3)",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show stats only, don't modify history",
    )
    parser.add_argument(
        "--quiet",
        "-q",
        action="store_true",
        help="Minimal output",
    )
    parser.add_argument(
        "--remove-rare",
        action="store_true",
        help="Remove rare command variants (default: keep them)",
    )

    args = parser.parse_args()

    settings = CleaningSettings(
        similarity_threshold=args.similarity,
        rare_threshold=args.rare_threshold,
        remove_rare=args.remove_rare,
    )

    return settings, args


def main() -> None:
    """Main entry point for the history cleaner."""
    settings, args = _parse_arguments()

    # Backup history (unless dry run)
    if not args.dry_run:
        backup_path = Path(str(HISTORY_FILE) + BACKUP_SUFFIX)
        shutil.copy2(HISTORY_FILE, backup_path)

    # Load exit codes and parse history
    exit_codes = load_exit_codes()
    all_lines, cmd_to_lines, seen_commands, successful_cmds, failed_cmds = _parse_history_file(
        exit_codes,
    )

    cmd_data = CommandData(
        failed_cmds=failed_cmds,
        successful_cmds=successful_cmds,
        cmd_to_lines=cmd_to_lines,
    )

    # Find commands to remove
    lines_to_remove, _removal_reasons = _identify_removals(
        cmd_data,
        seen_commands,
        settings,
    )

    # Write cleaned history (unless dry run)
    removed_count = len(lines_to_remove)
    if not args.dry_run and removed_count > 0:
        with HISTORY_FILE.open("w", encoding="utf-8") as f:
            for idx, line in enumerate(all_lines):
                if idx not in lines_to_remove:
                    f.write(line + "\n")

    if removed_count > 0 and not args.quiet:
        # Count removals by reason
        reason_counts = Counter(_removal_reasons.values())
        action = "Would remove" if args.dry_run else "Removed"
        print(f"\n{action} {removed_count} lines:")
        for reason, count in sorted(reason_counts.items(), key=lambda x: (-x[1], x[0])):
            print(f"  {reason}: {count}")
    elif removed_count == 0 and not args.quiet:
        print("No commands to remove")


if __name__ == "__main__":
    main()
