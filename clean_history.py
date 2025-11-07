#!/usr/bin/env python3
"""
Smart history cleaner that removes likely typos based on:
1. Commands that failed and have similar successful variants
2. Commands that appear very rarely compared to similar common commands
"""

import argparse
import re
import shutil
import sys
from pathlib import Path
from collections import Counter, defaultdict
from difflib import SequenceMatcher

HISTORY_FILE = Path.home() / ".zsh_history"
BACKUP_SUFFIX = ".backup"


def parse_history_line(line):
    """Parse a zsh history line with optional exit code."""
    # Format: : timestamp:duration;command###EXIT:code
    # Or old format: : timestamp:duration;command
    match = re.match(r': (\d+):\d+;(.+)', line)
    if not match:
        return None, None, None

    timestamp, rest = match.groups()

    # Check for exit code
    if "###EXIT:" in rest:
        command, exit_code = rest.rsplit("###EXIT:", 1)
        try:
            exit_code = int(exit_code.strip())
        except ValueError:
            exit_code = None
    else:
        command = rest
        exit_code = None

    return timestamp, command.strip(), exit_code


def similarity(a, b):
    """Calculate similarity ratio between two strings."""
    return SequenceMatcher(None, a, b).ratio()


def get_base_command(cmd):
    """Extract base command (first word) for grouping."""
    return cmd.split()[0] if cmd.split() else cmd


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

    args = parser.parse_args()

    similarity_threshold = args.similarity
    rare_threshold = args.rare_threshold

    # Backup history (unless dry run)
    if not args.dry_run:
        backup_path = Path(str(HISTORY_FILE) + BACKUP_SUFFIX)
        shutil.copy2(HISTORY_FILE, backup_path)
        if not args.quiet:
            print(f"Created backup: {backup_path}")

    # Parse history
    successful_cmds = Counter()
    failed_cmds = Counter()
    all_lines = []
    cmd_to_lines = defaultdict(list)

    with open(HISTORY_FILE, 'r', encoding='utf-8', errors='ignore') as f:
        for idx, line in enumerate(f):
            line = line.rstrip('\n')
            all_lines.append(line)

            timestamp, command, exit_code = parse_history_line(line)
            if command:
                cmd_to_lines[command].append(idx)

                if exit_code == 0:
                    successful_cmds[command] += 1
                elif exit_code is not None and exit_code != 0:
                    failed_cmds[command] += 1

    # Find commands to remove
    lines_to_remove = set()
    removal_reasons = {}

    # Strategy 1: Remove failed commands similar to successful ones
    for failed_cmd, fail_count in failed_cmds.items():
        base_cmd = get_base_command(failed_cmd)

        for success_cmd, success_count in successful_cmds.items():
            if get_base_command(success_cmd) == base_cmd:
                sim = similarity(failed_cmd, success_cmd)

                if sim >= similarity_threshold and success_count > fail_count:
                    for idx in cmd_to_lines[failed_cmd]:
                        lines_to_remove.add(idx)
                        removal_reasons[idx] = f"Failed similar to '{success_cmd}'"
                    break

    # Strategy 2: Remove rare commands similar to common ones
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

    if args.dry_run or not args.quiet:
        print(f"\nStats:")
        print(f"  Total commands: {len(all_lines)}")
        print(f"  Successful: {sum(successful_cmds.values())}")
        print(f"  Failed: {sum(failed_cmds.values())}")
        print(f"  Would remove: {removed_count}" if args.dry_run else f"  Removed: {removed_count}")

    # Write cleaned history (unless dry run)
    if not args.dry_run and removed_count > 0:
        with open(HISTORY_FILE, 'w', encoding='utf-8') as f:
            for idx, line in enumerate(all_lines):
                if idx not in lines_to_remove:
                    f.write(line + '\n')

        if not args.quiet:
            print(f"\nRemoved {removed_count} lines:")
            sample_indices = sorted(lines_to_remove)[:20]
            for idx in sample_indices:
                _, cmd, exit_code = parse_history_line(all_lines[idx])
                reason = removal_reasons.get(idx, "unknown")
                exit_str = f" (exit: {exit_code})" if exit_code is not None else ""
                print(f"  - {cmd}{exit_str} [{reason}]")

            if removed_count > 20:
                print(f"  ... and {removed_count - 20} more")
    elif args.dry_run and removed_count > 0 and not args.quiet:
        print(f"\nWould remove {removed_count} lines:")
        sample_indices = sorted(lines_to_remove)[:20]
        for idx in sample_indices:
            _, cmd, exit_code = parse_history_line(all_lines[idx])
            reason = removal_reasons.get(idx, "unknown")
            exit_str = f" (exit: {exit_code})" if exit_code is not None else ""
            print(f"  - {cmd}{exit_str} [{reason}]")

        if removed_count > 20:
            print(f"  ... and {removed_count - 20} more")
    elif removed_count == 0 and not args.quiet:
        print("No commands to remove")


if __name__ == "__main__":
    main()
