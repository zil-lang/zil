#!/usr/bin/env python3
"""Keep construction IDs out of permanent paths, declarations, and delivery.

Use --full for the whole tree, or --base REF for an incremental development check.
"""

import argparse
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
GOAL_ID = re.compile(r"(?<![A-Za-z0-9])G[0-9]+(?:[._][0-9]+)*(?![0-9])", re.I)
IMPLEMENTATION_ID = re.compile(r"(?<![A-Za-z0-9])impl[0-9]+(?![0-9])", re.I)
CHECKPOINT_ID = re.compile(r"(?<![A-Za-z0-9])cp[0-9]+(?![0-9])", re.I)
NUMBERED_LABEL = re.compile(
    r"(?<![A-Za-z0-9])(?:language|batch|implementation|review|research|task)[-_]?[0-9]+(?![0-9])", re.I
)
NUMBERED_DIRECTORY = re.compile(r"[0-9]+")
NUMBERED_PREFIX = re.compile(r"[0-9]+[-_]")
ORDINAL_CASE = re.compile(r"[0-9]{1,2}[-_][A-Za-z][A-Za-z0-9]*(?:[-_][A-Za-z0-9]+)*")
NUMBERED_SUFFIX = re.compile(r"[-_][0-9]{3}$")
STABLE_CASE_ID = re.compile(r"(?:case[-_]?[0-9]+|PLAN-[A-Z][0-9]{1,2}-[0-9]{3})", re.I)
PERFORMANCE_SIZE = re.compile(
    r"(?:chain[-_](?:forward|reverse)[-_][0-9]+|islands[-_][0-9]+[-_]plus[-_][0-9]+)", re.I
)
RESERVED_STATE = {".task", "_zil_goal"}
RESERVED_FILES = {"_zil_goal.md", "goal.md"}
RESERVED_DATA = {"data/handoff.json", "data/construction-progress.json", "data/task-index.json"}
CODE_SUFFIXES = {".rs", ".zil", ".py", ".c", ".h", ".js", ".mjs", ".ts"}
DECLARATION = re.compile(
    r"^\s*(?:(?:pub(?:\([^)]*\))?|async|unsafe|extern|export|default)\s+)*"
    r"(?:fn|struct|enum|trait|mod|const|static|def|class|function)\s+"
    r"([A-Za-z_][A-Za-z0-9_]*)\b",
    re.I | re.M,
)
CONSTRUCTION_NAME = re.compile(
    r"(?:^|_)(?:g|impl|cp|review|batch|task|research)[0-9]+(?:_[0-9]+)*(?:_|$)"
    r"|^[a-z][a-z_]*[0-9]{3,}(?:_|$)"
    r"|_[0-9]{3,}(?:_|$)", re.I
)
SEMANTIC_NUMBER = re.compile(
    r"^(?:schema|wire|llvm|utf|sha|u|i|f|m|r|island)[0-9]+(?:_|$)"
    r"|^(?:schema|wire)[_-]v?[0-9]+(?:_|$)"
    r"|^(?:case|fixture)[_-]?[0-9]+_[a-z]", re.I
)
HASH_NUMBER = re.compile(r"(?:^|_)sha(?:256|512)(?:_|$)", re.I)
MEASURED_QUANTITY = re.compile(r"_[0-9]{3,}_(?:level|levels|pairs|rectangles)(?=_|$)", re.I)


def git(root, *args):
    return subprocess.run(
        ["git", *args], cwd=root, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, check=True,
    ).stdout


def paths_from_nul(data):
    return {item.decode("utf-8", "surrogateescape") for item in data.split(b"\0") if item}


def path_problems(path):
    """Check only path components; semantic version and case IDs remain valid."""
    parts = path.split("/")
    if parts[0] == ".task":
        return []  # Process contracts live only on task branches; delivery rejects all of .task.
    problems = []
    if any(GOAL_ID.search(part) for part in parts):
        problems.append("goal ID")
    if any(IMPLEMENTATION_ID.search(part) for part in parts):
        problems.append("implementation ID")
    if any(CHECKPOINT_ID.search(part) for part in parts):
        problems.append("checkpoint ID")
    if any(NUMBERED_LABEL.search(part) for part in parts):
        problems.append("numbered construction label")
    if any(NUMBERED_DIRECTORY.fullmatch(part) for part in parts[:-1]):
        problems.append("number-only directory")
    leaf = parts[-1]
    stem = Path(leaf).stem
    if stem.isdecimal() or (NUMBERED_PREFIX.match(stem) and not ORDINAL_CASE.fullmatch(stem)):
        problems.append("numbered filename")
    elif NUMBERED_SUFFIX.search(stem) and not (STABLE_CASE_ID.fullmatch(stem)
                                               or PERFORMANCE_SIZE.fullmatch(stem)):
        problems.append("numbered filename")
    return problems


def delivery_problems(path):
    parts = path.split("/")
    problems = []
    if any(part in RESERVED_STATE for part in parts[:-1]) or parts[-1] in RESERVED_STATE:
        problems.append("task-state directory")
    if path in RESERVED_FILES:
        problems.append("duplicate root task status")
    if path in RESERVED_DATA:
        problems.append("duplicate task-status data")
    return problems


def construction_declarations(text):
    names = set()
    for name in DECLARATION.findall(text):
        if not CONSTRUCTION_NAME.search(name) or SEMANTIC_NUMBER.match(name):
            continue
        semantic_numbers = MEASURED_QUANTITY.sub("_quantity", HASH_NUMBER.sub("hash", name))
        if not CONSTRUCTION_NAME.search(semantic_numbers):
            continue
        names.add(name)
    return names


def check(base=None, delivery=False, root=ROOT, full=False):
    if full == (base is not None):
        raise ValueError("select exactly one of full or base")
    baseline = set() if full else paths_from_nul(
        git(root, "ls-tree", "-r", "--name-only", "-z", base))
    listed = paths_from_nul(git(root, "ls-files", "--cached", "--others", "--exclude-standard", "-z"))
    current = {path for path in listed if (root / path).exists() or (root / path).is_symlink()}
    new = sorted(current - baseline)
    violations = [(path, issue) for path in new for issue in path_problems(path)]
    legacy = [] if full else [(path, issue) for path in sorted(current & baseline)
                              for issue in path_problems(path)]
    changed = current if full else paths_from_nul(
        git(root, "diff", "--name-only", "-z", base, "--")) | set(new)
    for path in sorted(changed & current):
        if Path(path).suffix not in CODE_SUFFIXES:
            continue
        source = (root / path).read_text(encoding="utf-8", errors="replace")
        old = ""
        if not full and path in baseline:
            old = git(root, "show", f"{base}:{path}").decode("utf-8", "replace")
        for name in sorted(construction_declarations(source) - construction_declarations(old)):
            violations.append((path, f"construction ID declaration {name}"))
    if delivery:
        violations.extend((path, issue) for path in sorted(current)
                          for issue in delivery_problems(path))
    return new, violations, legacy


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    selection = parser.add_mutually_exclusive_group(required=True)
    selection.add_argument("--base", help="baseline Git commit/ref for incremental checks")
    selection.add_argument("--full", action="store_true", help="check all tracked and untracked files")
    parser.add_argument("--delivery", action="store_true",
                        help="also reject task state and duplicate root status across all files")
    parser.add_argument("--audit-all", action="store_true",
                        help="print every legacy path (default prints counts and samples)")
    args = parser.parse_args(argv)
    try:
        new, violations, legacy = check(args.base, args.delivery, full=args.full)
    except subprocess.CalledProcessError as exc:
        sys.stderr.write(exc.stderr.decode("utf-8", "replace"))
        return 2
    for path, issue in violations:
        print(f"FAIL {issue}: {path}")
    counts = Counter(issue for _, issue in legacy)
    if args.full:
        print(f"Checked {len(new)} paths and declarations across the full tree.")
    else:
        print(f"Checked {len(new)} new paths against {args.base}; legacy numbered paths: "
              + ", ".join(f"{kind}={count}" for kind, count in sorted(counts.items())))
    printed = Counter()
    for path, issue in legacy:
        printed[issue] += 1
        if not args.audit_all and printed[issue] > 3:
            continue
        print(f"LEGACY {issue}: {path}")
    if not args.audit_all and legacy:
        print("Use --audit-all for the complete legacy path inventory.")
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())
