#!/usr/bin/env python3
"""
Smart history cleaner that removes likely typos based on:
1. Commands that failed and have similar successful variants
2. Commands that appear very rarely compared to similar common commands
"""

import argparse
import re
import shutil
from pathlib import Path
from collections import Counter, defaultdict
from difflib import SequenceMatcher

HISTORY_FILE = Path.home() / ".zsh_history"
EXIT_FILE = Path.home() / ".zsh_history_exits"
BACKUP_SUFFIX = ".backup"


def load_exit_codes():
    """Load exit codes from separate file."""
    exit_codes = {}
    if not EXIT_FILE.exists():
        return exit_codes

    with open(EXIT_FILE, 'r', encoding='utf-8', errors='ignore') as f:
        for line in f:
            line = line.strip()
            if ':' in line:
                timestamp, exit_code = line.split(':', 1)
                try:
                    exit_codes[timestamp] = int(exit_code)
                except ValueError:
                    pass
    return exit_codes


def parse_history_line(line):
    """Parse a zsh history line."""
    # Format: : timestamp:duration;command
    match = re.match(r': (\d+):\d+;(.+)', line)
    if not match:
        return None, None

    timestamp, command = match.groups()
    return timestamp, command.strip()


def similarity(a, b):
    """Calculate similarity ratio between two strings."""
    return SequenceMatcher(None, a, b).ratio()


def get_base_command(cmd):
    """Extract base command (first word) for grouping."""
    return cmd.split()[0] if cmd.split() else cmd


def find_duplicate_indices(cmd_to_lines, seen_commands):
    """Find indices of duplicate commands, keeping only first occurrence.

    Args:
        cmd_to_lines: Dict mapping commands to list of line indices
        seen_commands: Dict mapping commands to their first occurrence index

    Returns:
        Set of line indices to remove
    """
    lines_to_remove = set()
    for command, indices in cmd_to_lines.items():
        if len(indices) > 1:
            first_idx = seen_commands[command]
            for idx in indices:
                if idx != first_idx:
                    lines_to_remove.add(idx)
    return lines_to_remove


def main():
    parser = argparse.ArgumentParser(
        description="Smart zsh history cleaner - removes typos and failed commands"
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

    similarity_threshold = args.similarity
    rare_threshold = args.rare_threshold

    # Backup history (unless dry run)
    if not args.dry_run:
        backup_path = Path(str(HISTORY_FILE) + BACKUP_SUFFIX)
        shutil.copy2(HISTORY_FILE, backup_path)
        if not args.quiet:
            print(f"Created backup: {backup_path}")

    # Load exit codes
    exit_codes = load_exit_codes()

    # Parse history
    successful_cmds = Counter()
    failed_cmds = Counter()
    all_lines = []
    cmd_to_lines = defaultdict(list)
    seen_commands = {}  # Track first occurrence of each command

    with open(HISTORY_FILE, 'r', encoding='utf-8', errors='ignore') as f:
        for idx, line in enumerate(f):
            line = line.rstrip('\n')
            all_lines.append(line)

            timestamp, command = parse_history_line(line)
            if command:
                cmd_to_lines[command].append(idx)

                # Track first occurrence for deduplication
                if command not in seen_commands:
                    seen_commands[command] = idx

                # Look up exit code by timestamp
                exit_code = exit_codes.get(timestamp)
                if exit_code == 0:
                    successful_cmds[command] += 1
                elif exit_code is not None and exit_code != 0:
                    failed_cmds[command] += 1

    # Find commands to remove
    lines_to_remove = set()
    removal_reasons = {}

    # Strategy 0: Remove duplicate commands (keep only first occurrence)
    duplicate_indices = find_duplicate_indices(cmd_to_lines, seen_commands)
    for idx in duplicate_indices:
        lines_to_remove.add(idx)
        removal_reasons[idx] = "Duplicate"

    # Strategy 1: Remove failed commands similar to successful ones
    for failed_cmd, fail_count in failed_cmds.items():
        base_cmd = get_base_command(failed_cmd)

        for success_cmd, success_count in successful_cmds.items():
            if get_base_command(success_cmd) == base_cmd:
                # Check if failed command is a prefix of successful one
                if success_cmd.startswith(failed_cmd) and success_cmd != failed_cmd:
                    # Ensure it's a word boundary (space or end of string after prefix)
                    if len(success_cmd) > len(failed_cmd) and success_count > fail_count:
                        for idx in cmd_to_lines[failed_cmd]:
                            lines_to_remove.add(idx)
                            removal_reasons[idx] = f"Failed prefix of '{success_cmd}'"
                        break

                # Check similarity
                sim = similarity(failed_cmd, success_cmd)
                if similarity_threshold <= sim < 1.0 and success_count > fail_count:
                    for idx in cmd_to_lines[failed_cmd]:
                        lines_to_remove.add(idx)
                        removal_reasons[idx] = f"Failed similar to '{success_cmd}'"
                    break

    # Strategy 2: Remove rare commands similar to common ones (only if --remove-rare)
    if args.remove_rare:
        all_cmds = Counter()
        all_cmds.update(successful_cmds)
        all_cmds.update(failed_cmds)

        for rare_cmd, rare_count in all_cmds.items():
            if rare_count <= rare_threshold:
                base_cmd = get_base_command(rare_cmd)

                for common_cmd, common_count in all_cmds.items():
                    if common_count > rare_count * 3 and get_base_command(common_cmd) == base_cmd:
                        sim = similarity(rare_cmd, common_cmd)

                        if sim >= similarity_threshold:
                            for idx in cmd_to_lines[rare_cmd]:
                                if idx not in lines_to_remove:
                                    lines_to_remove.add(idx)
                                    removal_reasons[idx] = f"Rare variant of '{common_cmd}'"
                            break

    # Show stats
    removed_count = len(lines_to_remove)
    duplicates_count = sum(1 for r in removal_reasons.values() if r == "Duplicate")

    if args.dry_run or not args.quiet:
        print("\nStats:")
        print(f"  Total commands: {len(all_lines)}")
        print(f"  Unique commands: {len(seen_commands)}")
        print(f"  Successful: {sum(successful_cmds.values())}")
        print(f"  Failed: {sum(failed_cmds.values())}")
        print(f"  Duplicates: {duplicates_count}")
        print(f"  Would remove: {removed_count}" if args.dry_run else f"  Removed: {removed_count}")

    # Write cleaned history (unless dry run)
    if not args.dry_run and removed_count > 0:
        with open(HISTORY_FILE, 'w', encoding='utf-8') as f:
            for idx, line in enumerate(all_lines):
                if idx not in lines_to_remove:
                    f.write(line + '\n')

    if removed_count > 0 and not args.quiet:
        # Count removals by reason
        reason_counts = Counter(removal_reasons.values())
        action = "Would remove" if args.dry_run else "Removed"
        print(f"\n{action} {removed_count} lines:")
        for reason, count in sorted(reason_counts.items()):
            print(f"  {reason}: {count}")
    elif removed_count == 0 and not args.quiet:
        print("No commands to remove")


if __name__ == "__main__":
    main()
