#!/usr/bin/env python3
"""Extract, compile, verify, and execute every runnable ```zil block in Markdown guides."""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DOCS = [ROOT / "docs" / "guide" / "language-tour.md"]


@dataclass
class ExampleBlock:
    doc_path: Path
    index: int
    line: int
    heading: str
    code: str


def extract_zil_blocks(doc_path: Path) -> list[ExampleBlock]:
    blocks: list[ExampleBlock] = []
    heading = "(top)"
    in_block = False
    block_lang = ""
    block_start = 0
    buf: list[str] = []

    for lineno, line in enumerate(doc_path.read_text(encoding="utf-8").splitlines(), 1):
        if not in_block:
            m_head = re.match(r"^(#{1,6})\s+(.*)$", line)
            if m_head:
                heading = m_head.group(2).strip()
            m_fence = re.match(r"^```([A-Za-z0-9_-]*)\s*$", line.strip())
            if m_fence:
                in_block = True
                block_lang = m_fence.group(1)
                block_start = lineno + 1
                buf = []
        else:
            if line.strip() == "```":
                in_block = False
                if block_lang == "zil":
                    blocks.append(
                        ExampleBlock(
                            doc_path=doc_path,
                            index=len(blocks) + 1,
                            line=block_start,
                            heading=heading,
                            code="\n".join(buf) + "\n",
                        )
                    )
                buf = []
            else:
                buf.append(line)
    return blocks


def resolve_zil_bin(explicit: str | None) -> Path:
    candidates: list[Path] = []
    if explicit:
        candidates.append(Path(explicit))
    env_bin = os.environ.get("ZIL_BIN")
    if env_bin:
        candidates.append(Path(env_bin))
    candidates.extend(
        [
            ROOT / "target" / "release" / "zil",
            ROOT / "target" / "debug" / "zil",
        ]
    )
    for c in candidates:
        if c.is_file() and os.access(c, os.X_OK):
            return c.resolve()
    raise FileNotFoundError(
        "Could not locate zil binary; pass --zil-bin or set ZIL_BIN (or build with cargo build -p zil-cli)."
    )


def run_examples(zil_bin: Path, doc_paths: list[Path], opt: str) -> int:
    all_blocks: list[ExampleBlock] = []
    for p in doc_paths:
        all_blocks.extend(extract_zil_blocks(p))

    if not all_blocks:
        print("ERROR: no ```zil blocks found", file=sys.stderr)
        return 1

    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="zil-doc-examples-") as tmp:
        tmp_path = Path(tmp)
        for blk in all_blocks:
            rel_doc = blk.doc_path.relative_to(ROOT).as_posix()
            src_path = tmp_path / f"ex_{blk.index:02d}.zil"
            out_bin = tmp_path / f"ex_{blk.index:02d}"
            src_path.write_text(blk.code, encoding="utf-8")

            build_cmd = [str(zil_bin), "build", str(src_path), "-o", str(out_bin), "--opt", opt]
            build_proc = subprocess.run(
                build_cmd,
                cwd=ROOT,
                capture_output=True,
                text=True,
                timeout=60,
            )
            if build_proc.returncode != 0:
                msg = (
                    f"FAIL [build] #{blk.index} {rel_doc}:L{blk.line} ({blk.heading})\n"
                    f"  stdout: {build_proc.stdout.strip()}\n"
                    f"  stderr: {build_proc.stderr.strip()}"
                )
                print(msg, file=sys.stderr)
                failures.append(msg)
                continue

            run_proc = subprocess.run(
                [str(out_bin)],
                cwd=ROOT,
                capture_output=True,
                text=True,
                timeout=30,
            )
            if run_proc.returncode != 0:
                msg = (
                    f"FAIL [run exit={run_proc.returncode}] #{blk.index} {rel_doc}:L{blk.line} ({blk.heading})\n"
                    f"  stdout: {run_proc.stdout.strip()}\n"
                    f"  stderr: {run_proc.stderr.strip()}"
                )
                print(msg, file=sys.stderr)
                failures.append(msg)
                continue

            print(f"PASS #{blk.index:02d} {rel_doc}:L{blk.line} — {blk.heading}")

    print(f"\nSummary: {len(all_blocks) - len(failures)}/{len(all_blocks)} runnable ZIL doc examples passed.")
    return 0 if not failures else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--zil-bin", help="path to compiled zil binary")
    parser.add_argument("--opt", default="O0", choices=["O0", "O2"], help="optimization level (default: O0)")
    parser.add_argument("docs", nargs="*", help="markdown files to check (default: docs/guide/language-tour.md)")
    args = parser.parse_args()

    zil_bin = resolve_zil_bin(args.zil_bin)
    doc_paths = [Path(d).resolve() for d in args.docs] if args.docs else DEFAULT_DOCS
    return run_examples(zil_bin, doc_paths, args.opt)


if __name__ == "__main__":
    sys.exit(main())
