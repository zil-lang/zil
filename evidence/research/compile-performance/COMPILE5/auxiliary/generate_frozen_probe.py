#!/usr/bin/env python3
"""Generate only the three predeclared compile5 source/oracle fixtures.

No compiler, subprocess, import of benchmark runners, or performance execution.
The source design digest, type dimensions, call counts and oracles are fixed.
An existing output directory is never reused or replaced.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


DESIGN_SHA256 = "59bc3c8185f930a43b146503ea23b7309a5fa7e27e5d960e71a505941e5f27bb"
FROZEN_UTC = "2026-10-30T16:09:13Z"
BASE = "a715f828261ff4b6ef9c8125623064360745aae4"
CANDIDATE = "187eae4f6bb085f0d0d42765f047b7f60add6bed"
CASE_IDS = ("unchanged-wide", "multiple-rewrites", "repeated-hints")
FUNCTIONS, CALLS = 64, 128
REWRITES = {(1, 3, 1): "CellA", (3, 3, 1): "CellB"}
MAIN = '''import work
extern "C" fn putchar(x:i32)->i32;
fn main()->i32 {
    let result=work.run();
    unsafe { putchar(i32(result%251)); }
    return 0;
}
'''


def sha(data):
    return hashlib.sha256(data).hexdigest()


def shape(path=(), rewrite=False):
    if len(path) == 3:
        return REWRITES.get(path, "i64") if rewrite else "i64"
    width = 2 if len(path) == 2 else 4
    return tuple(shape(path + (i,), rewrite) for i in range(width))


def type_text(node):
    return "(" + ",".join(map(type_text, node)) + ")" if isinstance(node, tuple) else node


def value_text(node):
    if isinstance(node, tuple):
        return "(" + ",".join(map(value_text, node)) + ")"
    return "i64(1)" if node == "i64" else node + " { value:i64(1) }"


def node_count(node, resolved=False):
    if isinstance(node, tuple):
        return 1 + sum(node_count(child, resolved) for child in node)
    return 2 if resolved and node in REWRITES.values() else 1


def selected(value, rewrite):
    suffix = ".value" if rewrite else ""
    return f"{value}.0.0.0+{value}.1.3.1{suffix}+{value}.3.3.1{suffix}"


def work_source(case):
    rewrite = case == "multiple-rewrites"
    node = shape(rewrite=rewrite)
    ty, value = type_text(node), value_text(node)
    if case == "repeated-hints":
        lines = [f"fn consume(p:{ty})->i64 {{ return {selected('p', False)}; }}",
                 "pub fn run()->i64 {", f"    let p:{ty}={value};", "    var total:i64=0;"]
        lines += ["    total += consume(p);"] * CALLS
    else:
        lines = (["struct CellA { value:i64; }", "struct CellB { value:i64; }"] if rewrite else [])
        lines += [f"fn f{i}()->{ty} {{ return {value}; }}" for i in range(FUNCTIONS)]
        lines += ["pub fn run()->i64 {", "    var total:i64=0;"]
        for i in range(FUNCTIONS):
            lines += [f"    let v{i}=f{i}();", f"    total += {selected('v' + str(i), rewrite)};"]
    lines += ["    return total;", "}"]
    return "\n".join(lines) + "\n"


def write_file(root, relative, content):
    dest = root / relative
    dest.parent.mkdir(parents=True, exist_ok=True)
    with dest.open("xb") as stream:
        stream.write(content)
    return {"bytes": len(content), "sha256": sha(content)}


def generate(out, design):
    document = design.read_bytes()
    if sha(document) != DESIGN_SHA256:
        raise ValueError("Frozen design digest differs; do not silently amend the plan")
    if out.exists():
        raise ValueError("Output already exists; use a new directory")
    if node_count(shape()) != 53 or node_count(shape(rewrite=True), True) != 55:
        raise ValueError("Type-node count differs from frozen design")
    out.mkdir(parents=True)
    files, cases = {}, []
    for case in CASE_IDS:
        work = work_source(case)
        expected_value = ((CALLS if case == "repeated-hints" else FUNCTIONS) * 3) % 251
        expected = {"unchanged-wide": 192, "multiple-rewrites": 192, "repeated-hints": 133}[case]
        if expected_value != expected:
            raise ValueError("Logical count disagrees with frozen independent oracle")
        functions = (MAIN + work).count("fn ")
        if functions != (4 if case == "repeated-hints" else 67):
            raise ValueError("Function count differs from frozen design")
        if case == "repeated-hints" and work.count("total += consume(p);") != CALLS:
            raise ValueError("Repeated hint call count differs")
        for name, content in (("main.zil", MAIN.encode()), ("work.zil", work.encode()),
                              ("stdin.bin", b""), ("stdout.bin", bytes([expected])), ("stderr.bin", b"")):
            relative = case + "/" + name
            files[relative] = write_file(out, relative, content)
        cases.append({"id": case, "modules": 2, "function_declarations_including_extern": functions,
                      "source": case + "/main.zil", "stdin": case + "/stdin.bin",
                      "expected_stdout": case + "/stdout.bin", "expected_stderr": case + "/stderr.bin",
                      "expected_exit": 0, "raw_type_nodes": 53, "resolved_type_nodes": 55 if case == "multiple-rewrites" else 53,
                      "scalar_leaf_count_before_rewrites": 32, "max_tuple_arity": 4,
                      "generated_return_functions": 0 if case == "repeated-hints" else FUNCTIONS,
                      "same_depth_consume_calls": CALLS if case == "repeated-hints" else 0,
                      "nominal_leaf_rewrites_per_function": 2 if case == "multiple-rewrites" else 0,
                      "ct_vm_queries": 0, "preflight": "NotRun", "performance": "NotRun"})
    case_orders = [CASE_IDS, (CASE_IDS[1], CASE_IDS[2], CASE_IDS[0]), (CASE_IDS[2], CASE_IDS[0], CASE_IDS[1])]
    order = [{"block": block, "case": case, "position": position, "arm": arm}
             for block, names in enumerate(case_orders) for case in names
             for position, arm in enumerate("ABBA" if block % 2 == 0 else "BAAB")]
    meta = {"schema": "zil-compile5-fixed-auxiliary-probe/1", "frozen_at_utc": FROZEN_UTC,
            "design_sha256": DESIGN_SHA256, "generator_sha256": sha(Path(__file__).read_bytes()),
            "baseline_commit": BASE, "candidate_local_freeze_commit": CANDIDATE,
            "cases": cases, "files": files, "blocks": 3, "order": order, "formal_compile_rows": 36,
            "common_config": {"phase": "build", "profile": "O0", "units": 8, "jobs": 4,
                              "backend": "llvm-api", "request": "cold_process", "fresh_source_cache": True,
                              "fresh_object_cache": True, "query_cache": False, "timings": False,
                              "limits": "unchanged defaults", "host_affinity": "same frozen host and CPU set as main O0 bridge"},
            "preflight_compilations": 6, "preflight": "NotRun", "performance": "NotRun",
            "oracle": "Three selected values are each exactly one; sum 64*3 or 128*3, emit one byte modulo 251.",
            "scope": "Auxiliary mechanism cases only; no weighted combined score, no ZIL/Rust score, no change to the main matrix. Generation does not establish source admission or performance."}
    write_file(out, "meta.json", (json.dumps(meta, indent=2, ensure_ascii=False) + "\n").encode())
    return {"out": str(out), "cases": list(CASE_IDS), "source_and_oracle_files": len(files),
            "bytes": sum(item["bytes"] for item in files.values()), "meta_sha256": sha((out / "meta.json").read_bytes()),
            "preflight": "NotRun", "performance": "NotRun"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--design", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(generate(args.out, args.design), indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
