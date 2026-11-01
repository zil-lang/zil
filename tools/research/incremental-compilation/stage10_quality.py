#!/usr/bin/env python3
"""Compare existing edited native artifacts without building or timing programs.

Usage: stage10_quality.py STAGE10_ROOT. GNU nm sizes exclude inter-function
alignment padding. Objdump comparisons preserve instructions, immediates and
symbolic branch destinations, normalizing only relocated absolute addresses
and annotated RIP-relative displacements. This is a bounded static comparison,
not a general runtime or function-address equality proof.
"""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

root = Path(sys.argv[1])
out = root / "quality"
out.mkdir(exist_ok=False)


def inspect(path, label):
    symbols = subprocess.check_output(["nm", "-S", "--defined-only", str(path)], text=True)
    assembly = subprocess.check_output(["objdump", "-d", "--no-show-raw-insn", str(path)], text=True)
    (out / f"{label}.nm").write_text(symbols)
    (out / f"{label}.asm").write_text(assembly)
    sizes = {}
    for line in symbols.splitlines():
        fields = line.split()
        if len(fields) == 4 and fields[2] in ("t", "T") and (fields[3].startswith("zil.") or fields[3] == "main"):
            sizes[fields[3]] = (int(fields[0], 16), int(fields[1], 16))
    functions = {name: [] for name in sizes}
    current = None
    for line in assembly.splitlines():
        header = re.fullmatch(r"[0-9a-f]+ <(.+)>:", line)
        if header:
            current = header[1] if header[1] in sizes else None
            continue
        instruction = re.match(r"\s*([0-9a-f]+):\s+(.*)", line)
        if current is None or instruction is None:
            continue
        address, text = int(instruction[1], 16), instruction[2]
        start, size = sizes[current]
        if not start <= address < start + size:
            continue
        if re.search(r"#\s+[0-9a-f]+ <[^>]+>", text):
            text = re.sub(r"-?0x[0-9a-f]+\(%rip\)", "RELOCATION(%rip)", text)
        text = re.sub(r"\b[0-9a-f]+ (?=<)", "", text)
        functions[current].append(text)
    return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "bytes": path.stat().st_size}, sizes, functions


pairs = []
for window in ["final-target", "final-guards", "final-closed-guard"]:
    for candidate in sorted((root / window).glob("*/*-candidate/program")):
        baseline = candidate.parent.with_name(candidate.parent.name.removesuffix("candidate") + "baseline") / "program"
        label = f"{window}-{candidate.parent.parent.name}-{candidate.parent.name.removesuffix('-candidate')}"
        old, old_sizes, old_functions = inspect(baseline, label + "-baseline")
        new, new_sizes, new_functions = inspect(candidate, label + "-candidate")
        common = old_functions.keys() & new_functions.keys()
        different = sorted(name for name in common if old_functions[name] != new_functions[name])
        pairs.append({"label": label, "baseline": old, "candidate": new,
                      "byte_identical": old["sha256"] == new["sha256"],
                      "common_functions": len(common),
                      "missing_functions": sorted(old_functions.keys() - new_functions.keys()),
                      "added_functions": sorted(new_functions.keys() - old_functions.keys()),
                      "different_instruction_functions": different,
                      "different_size_functions": sorted(name for name in common if old_sizes[name][1] != new_sizes[name][1])})
result = {"scope": __doc__, "pairs": pairs}
(out / "results.json").write_text(json.dumps(result, indent=2) + "\n")
for pair in pairs:
    print(pair["label"], pair["common_functions"], "instruction differences", len(pair["different_instruction_functions"]), "size differences", len(pair["different_size_functions"]), "byte identical", pair["byte_identical"])
