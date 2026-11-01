#!/usr/bin/env python3
"""Inspect existing Stage11 edited executables; never build or run a workload.

Usage: stage11_quality.py STAGE11_ROOT [--output DIRECTORY]

Compare every defined text symbol named main or beginning zil. Resolve only
single-instruction, direct-jmp thunks to sized function entry points, with cycle
and depth bounds. Compare the resulting instruction bodies after normalizing
relocated branch addresses, self-relative labels and annotated RIP displacements.
Keep immediates, registers, external symbol names and all other instructions.

Raw symbol/body/size differences remain in the report even when effective bodies
match. This static comparison does not prove general runtime, locality, function
address, cache, or ABI equivalence. Exit zero means the reports were written,
not that code quality passed a gate. Missing artifacts/tool failures exit two.
"""
import argparse
from bisect import bisect_left
from collections import defaultdict
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess


WINDOWS = ("final-target", "final-guards", "final-closed-guard")
PRIVATE_PREFIX = "__zil_patch_old_"
MAX_THUNK_DEPTH = 64
MAX_FUNCTION_BYTES = 1024 * 1024
MAX_FUNCTION_INSTRUCTIONS = 100_000
SYMBOL = re.compile(r"^\s*([0-9a-fA-F]+)\s+([0-9a-fA-F]+)\s+(\S)\s+(.+?)\s*$")
UNSIZED_SYMBOL = re.compile(r"^\s*([0-9a-fA-F]+)\s+(\S)\s+(.+?)\s*$")
INSTRUCTION = re.compile(r"^\s*([0-9a-fA-F]+):\s+(.+?)\s*$")
DIRECT = re.compile(r"^(?:0x)?([0-9a-fA-F]+)(?:\s+<([^<>]+)>)?$")
CONTROL = re.compile(r"^(?:j[a-z]+|callq?|loop[a-z]*)$")
SECTION = re.compile(
    r"^\s*\d+\s+(\S+)\s+([0-9a-fA-F]+)\s+([0-9a-fA-F]+)"
    r"\s+[0-9a-fA-F]+\s+[0-9a-fA-F]+\s+\S+\s*$"
)


def canonical(name):
    return name == "main" or name.startswith("zil.")


def digest(lines):
    return hashlib.sha256(("\n".join(lines) + "\n").encode()).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def union_size(ranges):
    end = None
    size = 0
    for start, stop in sorted(ranges):
        if stop <= start:
            continue
        size += max(0, stop - max(start, end if end is not None else start))
        end = max(stop, end if end is not None else stop)
    return size


def parse_symbols(text):
    result = []
    for line in text.splitlines():
        match = SYMBOL.fullmatch(line)
        if match:
            address, size, kind, name = match.groups()
            result.append({"name": name, "address": int(address, 16),
                           "size": int(size, 16), "kind": kind})
        else:
            match = UNSIZED_SYMBOL.fullmatch(line)
            if match:
                address, kind, name = match.groups()
                result.append({"name": name, "address": int(address, 16),
                               "size": None, "kind": kind})
    return result


def parse_sections(text):
    result = []
    for line in text.splitlines():
        match = SECTION.fullmatch(line)
        if match:
            name, size, address = match.groups()
            result.append({"name": name, "size": int(size, 16),
                           "address": int(address, 16), "flags": ""})
        elif result and line.strip() and not result[-1]["flags"]:
            result[-1]["flags"] = line.strip()
    return result


class Image:
    def __init__(self, symbols, assembly, sections):
        self.symbols = symbols
        self.sections = sections
        self.by_name = defaultdict(list)
        self.by_address = defaultdict(list)
        self.instructions = {}
        self.errors = []
        for symbol in symbols:
            if symbol["kind"] in ("t", "T", "w", "W"):
                self.by_name[symbol["name"]].append(symbol)
                if symbol["size"]:
                    self.by_address[symbol["address"]].append(symbol)
        for line in assembly.splitlines():
            match = INSTRUCTION.fullmatch(line)
            if match:
                address = int(match[1], 16)
                text = re.sub(r"\s+", " ", match[2]).strip()
                if address in self.instructions:
                    self.errors.append(f"duplicate disassembly address {address:x}")
                self.instructions[address] = text
        self.addresses = sorted(self.instructions)

    def body(self, symbol):
        start, size = symbol["address"], symbol["size"]
        if size is None or size == 0:
            return None, "missing_or_zero_symbol_size"
        if size > MAX_FUNCTION_BYTES:
            return None, "function_byte_bound"
        if not any("CODE" in s["flags"] and s["address"] <= start
                   and start + size <= s["address"] + s["size"] for s in self.sections):
            return None, "symbol_outside_one_executable_section"
        begin = bisect_left(self.addresses, start)
        end = bisect_left(self.addresses, start + size)
        if end - begin > MAX_FUNCTION_INSTRUCTIONS:
            return None, "function_instruction_bound"
        addresses = self.addresses[begin:end]
        if not addresses or addresses[0] != start:
            return None, "function_entry_not_disassembled"
        return [(address, self.instructions[address]) for address in addresses], None

    def target(self, address, annotation):
        candidates = self.by_address.get(address, [])
        if not candidates:
            return None, "single_jump_target_has_no_sized_function"
        named = [s for s in candidates if s["name"] == annotation]
        if len(named) == 1:
            return named[0], None
        if len({s["size"] for s in candidates}) != 1:
            return None, "ambiguous_single_jump_target_size"
        return min(candidates, key=lambda s: (not canonical(s["name"]), s["name"])), None

    def normalize(self, symbol, body):
        start, end = symbol["address"], symbol["address"] + symbol["size"]
        instruction_addresses = {address for address, _ in body}
        normalized, issues = [], []
        for address, text in body:
            parts = text.split(" ", 1)
            mnemonic, operands = parts[0], parts[1] if len(parts) == 2 else ""
            if CONTROL.fullmatch(mnemonic):
                direct = DIRECT.fullmatch(operands)
                if direct:
                    destination = int(direct[1], 16)
                    if start <= destination < end:
                        if destination not in instruction_addresses:
                            issues.append(f"branch_target_not_instruction_at_{address - start:x}")
                        operands = f"<SELF+0x{destination - start:x}>"
                    elif direct[2]:
                        operands = f"<{direct[2]}>"
                    else:
                        issues.append(f"unannotated_external_control_at_{address - start:x}")
                elif operands.startswith("*"):
                    issues.append(f"indirect_control_at_{address - start:x}")
                else:
                    issues.append(f"unrecognized_control_at_{address - start:x}")
            # Annotated RIP references retain the destination's symbol/offset.
            # Never erase a numeric immediate merely because it is hexadecimal.
            if "(%rip)" in operands:
                annotation = re.search(r"#\s*([0-9a-fA-F]+)\s+<([^<>]+)>", operands)
                if annotation:
                    operands = re.sub(r"-?0x[0-9a-fA-F]+\(%rip\)", "RELOCATION(%rip)", operands)
                    operands = re.sub(r"#\s*[0-9a-fA-F]+\s+<([^<>]+)>", r"# <\1>", operands)
                else:
                    issues.append(f"unannotated_rip_reference_at_{address - start:x}")
            normalized.append(mnemonic + (" " + operands if operands else ""))
        return normalized, issues

    def resolve(self, symbol):
        path, seen = [], set()
        current = symbol
        for _ in range(MAX_THUNK_DEPTH + 1):
            address = current["address"]
            if address in seen:
                return {"status": "uncompared", "reason": "thunk_cycle", "chain": path}
            seen.add(address)
            path.append(current["name"])
            body, error = self.body(current)
            if error:
                return {"status": "uncompared", "reason": error, "chain": path}
            if len(body) == 1:
                parts = body[0][1].split(" ", 1)
                if parts[0] in ("jmp", "jmpq"):
                    direct = DIRECT.fullmatch(parts[1] if len(parts) == 2 else "")
                    if not direct:
                        return {"status": "uncompared", "reason": "non_direct_single_jump", "chain": path}
                    current, error = self.target(int(direct[1], 16), direct[2])
                    if error:
                        return {"status": "uncompared", "reason": error, "chain": path}
                    continue
            normalized, issues = self.normalize(current, body)
            return {"status": "uncompared" if issues else "body",
                    "reason": "normalization_incomplete" if issues else None,
                    "issues": issues, "chain": path, "terminal": current["name"],
                    "terminal_address": current["address"], "effective_size": current["size"],
                    "instructions": normalized, "instruction_sha256": digest(normalized)}
        return {"status": "uncompared", "reason": "thunk_depth_bound", "chain": path}


def run_tool(command, destination, commands):
    result = subprocess.run(command, text=True, capture_output=True,
                            env={**os.environ, "LC_ALL": "C"}, check=False)
    destination.write_text(result.stdout)
    destination.with_suffix(destination.suffix + ".stderr").write_text(result.stderr)
    commands.append({"argv": command, "exit": result.returncode,
                     "stdout": str(destination), "stderr": str(destination) + ".stderr"})
    if result.returncode:
        raise RuntimeError(f"tool exited {result.returncode}: {command[0]}")
    return result.stdout


def inspect(path, label, out, commands):
    symbols = parse_symbols(run_tool(["nm", "-S", "--defined-only", str(path)], out / f"{label}.nm", commands))
    assembly = run_tool(["objdump", "-d", "--wide", "--no-show-raw-insn", str(path)], out / f"{label}.asm", commands)
    sections = parse_sections(run_tool(["objdump", "-h", str(path)], out / f"{label}.sections", commands))
    image = Image(symbols, assembly, sections)
    if not any(canonical(name) for name in image.by_name):
        raise ValueError("no canonical text symbols found; stripped/unsupported artifact")
    functions = {}
    for name in sorted(name for name in image.by_name if canonical(name)):
        choices = image.by_name[name]
        if len(choices) != 1:
            functions[name] = {"status": "uncompared", "reason": "duplicate_canonical_symbol", "symbols": choices}
            continue
        symbol = choices[0]
        resolved = image.resolve(symbol)
        if image.errors:
            resolved = {**resolved, "status": "uncompared", "reason": "ambiguous_disassembly"}
        body, error = image.body(symbol)
        raw, raw_issues = image.normalize(symbol, body) if body is not None else (None, [error])
        functions[name] = {**resolved, "symbol": symbol, "raw_instructions": raw,
                           "raw_issues": raw_issues, "raw_instruction_sha256": digest(raw) if raw is not None else None}
    private = [s for s in symbols if s["name"].startswith(PRIVATE_PREFIX)]
    global_names = sorted(s["name"] for s in symbols if canonical(s["name"]) and s["kind"] in ("T", "W"))
    effective_ranges = {(f["terminal_address"], f["terminal_address"] + f["effective_size"])
                        for f in functions.values() if "terminal_address" in f}
    summary = {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
               "elf_bytes": path.stat().st_size, "canonical_functions": len(functions),
               "global_canonical_symbols": global_names,
               "canonical_symbol_bytes": sum(s["size"] or 0 for s in symbols if canonical(s["name"]) and s["kind"] in ("t", "T", "w", "W")),
               "distinct_effective_body_bytes": union_size(effective_ranges),
               "text_bytes": sum(s["size"] for s in sections if s["name"] == ".text" or s["name"].startswith(".text.")),
               "text_sections": [s for s in sections if s["name"] == ".text" or s["name"].startswith(".text.")],
               "private_old_count": len(private),
               "private_old_symbol_bytes": sum(s["size"] or 0 for s in private),
               "private_old_distinct_bytes": union_size((s["address"], s["address"] + s["size"]) for s in private if s["size"]),
               "private_old_nonlocal_symbols": [s["name"] for s in private if s["kind"] != "t"],
               "private_old_symbols": private, "parse_errors": image.errors,
               "symbols_file": str(out / f"{label}.symbols.json"),
               "functions_file": str(out / f"{label}.functions.json")}
    write_json(out / f"{label}.symbols.json", symbols)
    write_json(out / f"{label}.functions.json", functions)
    return summary, functions


def compare(label, old, old_functions, new, new_functions):
    common = sorted(old_functions.keys() & new_functions.keys())
    different, uncompared, size_differences = [], [], []
    for name in common:
        a, b = old_functions[name], new_functions[name]
        if a["status"] != "body" or b["status"] != "body":
            uncompared.append({"name": name, "baseline": a, "candidate": b})
        elif a["instructions"] != b["instructions"]:
            different.append({"name": name, "baseline": a, "candidate": b})
        if a.get("effective_size") != b.get("effective_size"):
            size_differences.append({"name": name, "baseline": a.get("effective_size"), "candidate": b.get("effective_size")})
    old_globals, new_globals = set(old["global_canonical_symbols"]), set(new["global_canonical_symbols"])
    return {"label": label, "baseline": old, "candidate": new,
            "byte_identical": old["sha256"] == new["sha256"], "common_functions": len(common),
            "missing_functions": sorted(old_functions.keys() - new_functions.keys()),
            "added_functions": sorted(new_functions.keys() - old_functions.keys()),
            "missing_global_canonical_symbols": sorted(old_globals - new_globals),
            "added_global_canonical_symbols": sorted(new_globals - old_globals),
            "raw_instruction_differences": [n for n in common if old_functions[n].get("raw_instructions") != new_functions[n].get("raw_instructions")],
            "raw_size_differences": [n for n in common if old_functions[n].get("symbol", {}).get("size") != new_functions[n].get("symbol", {}).get("size")],
            "canonical_kind_differences": [n for n in common if old_functions[n].get("symbol", {}).get("kind") != new_functions[n].get("symbol", {}).get("kind")],
            "effective_instruction_equal_count": len(common) - len(different) - len(uncompared),
            "effective_instruction_differences": different, "effective_uncompared": uncompared,
            "effective_size_differences": size_differences,
            "size_deltas": {key: new[key] - old[key] for key in (
                "elf_bytes", "text_bytes", "canonical_symbol_bytes", "distinct_effective_body_bytes",
                "private_old_count", "private_old_symbol_bytes", "private_old_distinct_bytes")}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--expected-pairs", type=int, default=6)
    arguments = parser.parse_args()
    root = arguments.root.resolve()
    out = arguments.output.resolve() if arguments.output else root / "quality"
    out.mkdir(parents=True, exist_ok=False)
    pairs, errors, commands = [], [], []
    found = []
    for window in WINDOWS:
        directory = root / window
        if not directory.is_dir():
            errors.append(f"missing window: {directory}")
        for candidate in sorted(directory.glob("*/*-candidate/program")):
            baseline = candidate.parent.with_name(candidate.parent.name.removesuffix("candidate") + "baseline") / "program"
            label = f"{window}-{candidate.parent.parent.name}-{candidate.parent.name.removesuffix('-candidate')}"
            found.append(label)
            try:
                old, old_functions = inspect(baseline, label + "-baseline", out, commands)
                new, new_functions = inspect(candidate, label + "-candidate", out, commands)
                pair = compare(label, old, old_functions, new, new_functions)
                pairs.append(pair)
                print(label, "effective differences", len(pair["effective_instruction_differences"]),
                      "uncompared", len(pair["effective_uncompared"]),
                      "raw differences", len(pair["raw_instruction_differences"]),
                      "size deltas", json.dumps(pair["size_deltas"], sort_keys=True), flush=True)
            except (OSError, RuntimeError, ValueError) as error:
                errors.append(f"{label}: {error}")
    if len(found) != arguments.expected_pairs:
        errors.append(f"expected {arguments.expected_pairs} edited pairs; found {len(found)}")
    result = {"scope": __doc__, "root": str(root), "expected_pairs": arguments.expected_pairs,
              "found_pairs": found, "completed_pairs": len(pairs), "operational_errors": errors,
              "bounds": {"thunk_depth": MAX_THUNK_DEPTH, "function_bytes": MAX_FUNCTION_BYTES,
                         "function_instructions": MAX_FUNCTION_INSTRUCTIONS},
              "commands": commands, "pairs": pairs}
    write_json(out / "results.json", result)
    for error in errors:
        print("ERROR", error, flush=True)
    print("Report:", out / "results.json", "(static observations, not a runtime-quality verdict)", flush=True)
    return 2 if errors else 0


if __name__ == "__main__":
    raise SystemExit(main())
