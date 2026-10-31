#!/usr/bin/env python3
"""Audit existing CI artifacts only. Never compile or execute a test/program.

The output is scoped evidence for a failed CI run, not final acceptance. Large
executables and object-cache files are hashed against the uploaded manifest but
are not copied. Builder210 destroys its temporary objects; its width control is
therefore represented by exact test source and the original successful CI log.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess

CANDIDATE = "77b30b4e13187ca9e5ddf8b830f11675fcaffa5c"
CHECKOUT = "89406b33b43fc17fe967b5bb9f629a16cb44b763"
RUN = 41194080948


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_json(path: Path):
    return json.loads(path.read_text())


def write_json(path: Path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def saved_exit(path: Path) -> int:
    match = re.fullmatch(r"Some\((-?\d+)\)\s*", path.read_text())
    assert match, path
    return int(match.group(1))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--repo", type=Path, required=True)
    args = parser.parse_args()
    repo = args.repo.resolve()
    artifacts = args.artifacts.resolve()
    workspace = artifacts / "workspace"
    native = artifacts / "workspace-native"
    out234 = repo / "evidence/reviews/234/ci41194080948"
    out_joint = repo / "evidence/reviews/construction11/ci41194080948"

    def git_bytes(path):
        return subprocess.check_output(["git", "show", f"{CANDIDATE}:{path}"], cwd=repo)

    candidate_tree = subprocess.check_output(["git", "ls-tree", "-r", CANDIDATE], cwd=repo)
    assert candidate_tree == (workspace / "source-tree.txt").read_bytes()
    assert (workspace / "commit.txt").read_text().strip() == CHECKOUT
    toolchain = read_json(workspace / "llvm-api-toolchain.json")
    assert toolchain["status"] == "Pass"
    assert toolchain["checks"]["llvm"]["version"] == "23.1.4"
    assert toolchain["checks"]["clang"]["version"] == "23.1.4"
    original_log = (workspace / "workspace.log").read_text()
    totals = [tuple(map(int, row)) for row in re.findall(
        r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;",
        original_log,
    )]
    workspace_counts = [sum(row[column] for row in totals) for column in range(5)]
    assert workspace_counts == [3020, 4, 0, 0, 0]

    manifest = read_json(workspace / "workspace-native-manifest.json")
    assert len(manifest["files"]) == 68300
    entries = {row["path"]: row for row in manifest["files"]}
    assert len(entries) == 68300
    checked_files = {"234": [], "joint": []}
    kept_files = {"234": [], "joint": []}

    for label, source in [
        ("commit.txt", workspace / "commit.txt"),
        ("toolchain.txt", workspace / "toolchain.txt"),
        ("llvm-api-toolchain.json", workspace / "llvm-api-toolchain.json"),
        ("workspace-download-receipt.json", artifacts / "workspace-download-receipt.json"),
        ("workspace-native-download-receipt.json", artifacts / "workspace-native-download-receipt.json"),
        ("WORKSPACE-FAIL.json", artifacts / "WORKSPACE-FAIL.json"),
    ]:
        target = out234 / "receipts" / label
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)

    source_files = [
        "crates/zil-llvm-api/src/native/builder.rs",
        "crates/zil-llvm-api/tests/builder210.rs",
        "crates/zil-llvm-api/tests/support210/mod.rs",
        "crates/zil-driver/tests/impl234_narrow_constants.rs",
        "crates/zil-driver/tests/review234_full_width.rs",
        "examples/mainline/byte_io/main.zil",
        "examples/mainline/byte_io/io.zil",
    ]
    source_records = []
    for name in source_files:
        data = git_bytes(name)
        target = out234 / "source" / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        source_records.append({"path": name, "bytes": len(data), "sha256": sha(data)})
    joint_test = git_bytes("crates/zil-driver/tests/construction11_integration.rs")
    (out_joint / "source").mkdir(parents=True, exist_ok=True)
    (out_joint / "source/construction11_integration.rs").write_bytes(joint_test)

    def log_group(name):
        start = original_log.index(f"     Running tests/{name}.rs")
        end = original_log.index("test result:", start)
        end = original_log.index("\n", end)
        return original_log[start:end + 1]

    expected_groups = {"builder210": 9, "impl234_narrow_constants": 2,
                       "review234_full_width": 1, "construction11_integration": 2}
    for name, count in expected_groups.items():
        group = log_group(name)
        assert f"test result: ok. {count} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;" in group
        target = out_joint if name == "construction11_integration" else out234
        (target / f"{name}.log").write_text(group)
    assert "test signed_and_unsigned_spellings_of_integer_bits_are_identical_at_all_widths ... ok" in log_group("builder210")

    def preserve_case(directory, category):
        destination = out234 if category == "234" else out_joint
        for path in sorted(directory.rglob("*")):
            if not path.is_file():
                continue
            assert not path.is_symlink()
            relative = path.relative_to(native).as_posix()
            original = entries[relative]
            data = path.read_bytes()
            assert original["bytes"] == len(data) and original["sha256"] == sha(data), relative
            checked_files[category].append(original)
            inside = path.relative_to(directory)
            keep = len(inside.parts) == 1 and path.name != "program"
            if category == "joint" and (
                "active-loan-reject" in directory.name or "LlvmApi-O2" in directory.name
            ) and inside.as_posix() in {
                "phases/core.txt", "phases/typed.txt", "phases/candidate.txt", "phases/verified.txt"
            }:
                keep = True
            if keep:
                target = destination / "cases" / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
                kept_files[category].append(original)

    def compile_record(path, warm, cached):
        report = read_json(path)
        assert report["status"] == "Success" and report["phase"] == "link"
        assert report["verification_runs"] == 1
        codegen = report["native_codegen"]
        backend = codegen["backend"]
        assert backend in ("clang", "llvm-api") and codegen["fallback"] is None
        commands = report["commands"]
        assert commands and all(command[0].endswith("/llvm-23.1.4/bin/clang") for command in commands)
        opts = {arg[1:] for command in commands for arg in command if arg in ("-O0", "-O2")}
        assert len(opts) == 1
        optimization = opts.pop()
        if backend == "clang":
            assert all("-x" in command and "ir" in command for command in commands)
            assert not codegen["object_reports"]
        else:
            objects = codegen["object_reports"]
            assert objects
            assert all(obj["llvm_version"] == "23.1.4" and obj["parse_ns"] == 0 for obj in objects)
            if warm and cached:
                assert codegen["object_cache"]["hits"] > 0
                assert all(obj["input_kind"] == "object-cache" for obj in objects)
                assert all(obj["producer_verified_before_after"] is True for obj in objects)
                assert all(obj["llvm_verification_this_request"] is False for obj in objects)
            else:
                assert all(obj["input_kind"] == "builder" for obj in objects)
                assert all(obj["verify_before_after"] is True for obj in objects)
                assert all(obj["pipeline"] == f"default<{optimization}>" for obj in objects)
            if cached:
                cache = codegen["object_cache"]
                assert cache["requested"] and cache["enabled"] and cache["corrupt"] == 0
                if not warm:
                    assert cache["hits"] == 0 and cache["misses"] > 0 and cache["stored"] > 0
        return {"report": path.relative_to(native).as_posix(), "backend": backend,
                "optimization": optimization, "warm": warm, "verification_runs": 1,
                "fallback": None, "object_reports": codegen["object_reports"],
                "object_cache": codegen["object_cache"]}

    def check_pair(cold, warm):
        assert cold["backend"] == warm["backend"] and cold["optimization"] == warm["optimization"]
        if cold["backend"] == "llvm-api":
            assert [x["cache"]["action_key"] for x in cold["object_reports"]] == [
                x["cache"]["action_key"] for x in warm["object_reports"]]

    review_source = git_bytes("crates/zil-driver/tests/review234_full_width.rs").decode()
    source_match = re.search(r'const SOURCE: &str = r#"(.*?)"#;', review_source, re.S)
    assert source_match
    exact_review_source = source_match.group(1).encode()
    oracle = [(b"", -(1 << 31), -(1 << 63)), (b"\0", -257, -4294967297),
              (b"\1", -256, -4294967296), (b"\2", -1, -1),
              (b"\3", (1 << 31) - 1, (1 << 63) - 1),
              (b"\4", -(1 << 31) + 256, -(1 << 63) + 256), (b"\xff", -2, -2)]
    review_rows = []
    for directory in sorted(native.glob("review234-*")):
        assert (directory / "source.zil").read_bytes() == exact_review_source
        cold = compile_record(directory / "compile-false.json", False, True)
        warm = compile_record(directory / "compile-true.json", True, True)
        check_pair(cold, warm)
        observations = []
        for temperature in ("false", "true"):
            for index, (input_bytes, i32, i64) in enumerate(oracle):
                stem = directory / f"{temperature}-{index}"
                expected = struct.pack("<iq", i32, i64)
                assert stem.with_suffix(".input").read_bytes() == input_bytes
                assert stem.with_suffix(".expected").read_bytes() == expected
                actual = stem.with_suffix(".stdout").read_bytes()
                assert actual == expected and len(actual) == 12
                assert stem.with_suffix(".stderr").read_bytes() == b""
                assert saved_exit(stem.with_suffix(".exit")) == 0
                observations.append({"warm": temperature == "true", "index": index,
                                     "input_hex": input_bytes.hex(), "i32": i32, "i64": i64,
                                     "expected_hex": expected.hex(), "actual_hex": actual.hex(), "exit": 0})
        preserve_case(directory, "234")
        review_rows.append({"case": directory.name, "compiles": [cold, warm], "observations": observations})
    assert {(r["compiles"][0]["backend"], r["compiles"][0]["optimization"]) for r in review_rows} == {
        (backend, opt) for backend in ("clang", "llvm-api") for opt in ("O0", "O2")}
    assert len(review_rows) == 4

    impl_rows = []
    for directory in sorted(native.glob("zil-impl234-narrow-*")):
        cold = compile_record(directory / "compile-0.json", False, True)
        warm = compile_record(directory / "compile-1.json", True, True)
        check_pair(cold, warm)
        sources = read_json(directory / "compile-0.json")["source_files"].values()
        stream = any(path.endswith("examples/mainline/byte_io/main.zil") for path in sources)
        if stream:
            label = "byte_stream"
            inputs = [b"", b"hello, Zil!\n\0\x80", b"Abc\0\xff\n" * 100]
        else:
            # Identify which exact frozen fixture was emitted from its CFG,
            # without consulting native output or choosing an expected exit.
            llvm = (directory / "program.ll").read_text()
            label = "negative_loop" if "@llvm.ssub.with.overflow.i32" in llvm else "negative_branch"
            assert "declare i32 @getchar()" in llvm
            inputs = [b"", b"\0", b"1", b"Z"]
        observations = []
        assert len(list(directory.glob("run-*.exit"))) == 2 * len(inputs)
        for index in range(2 * len(inputs)):
            input_bytes = inputs[index % len(inputs)]
            stem = directory / f"run-{index}"
            assert stem.with_suffix(".input").read_bytes() == input_bytes
            if stream:
                expected_output, expected_exit = input_bytes.upper(), 0
            else:
                value = input_bytes[0] if input_bytes else -1
                expected_output = b""
                expected_exit = 254 if value < 0 or value > 64 else (
                    0 if label == "negative_loop" or value == 0 else 255)
            assert stem.with_suffix(".stdout").read_bytes() == expected_output
            assert stem.with_suffix(".stderr").read_bytes() == b""
            assert saved_exit(stem.with_suffix(".exit")) == expected_exit
            observations.append({"index": index, "warm": index >= len(inputs),
                                 "input_bytes": len(input_bytes), "input_sha256": sha(input_bytes),
                                 "stdout_sha256": sha(expected_output), "exit": expected_exit})
        preserve_case(directory, "234")
        impl_rows.append({"case": directory.name, "fixture": label,
                          "fixture_identification": "report source path for byte stream; emitted subtraction-loop CFG for negative fixtures, not runtime results",
                          "compiles": [cold, warm], "observations": observations})
    assert len(impl_rows) == 12
    assert {(r["fixture"], r["compiles"][0]["backend"], r["compiles"][0]["optimization"]) for r in impl_rows} == {
        (label, backend, opt) for label in ("byte_stream", "negative_loop", "negative_branch")
        for backend in ("clang", "llvm-api") for opt in ("O0", "O2")}

    joint_sources = {name: git_bytes(f"examples/mainline/construction11/{name}.zil")
                     for name in ("main", "rules", "data")}
    assert b"if value != 14 { return 1 }" in joint_sources["main"]
    joint_rows = []
    negative_rows = []
    for directory in sorted(native.glob("construction11-*")):
        negative = "active-loan-reject" in directory.name
        for name, source in joint_sources.items():
            expected = source
            if negative and name == "main":
                expected = source.replace(
                    b"let value = callback() + callback()\n    let moved = item",
                    b"let moved = item\n    let value = callback() + callback()")
                assert expected != source
            assert (directory / f"{name}.zil").read_bytes() == expected
        if negative:
            report = read_json(directory / "report.json")
            assert report["status"] == "SafeReject" and report["phase"] == "verify"
            assert report["verification_runs"] == 1
            assert any(d["code"] == "E-BORROW-CONFLICT" for d in report["diagnostics"])
            assert report["native_codegen"] is None and report["commands"] == []
            assert "executable" not in report["artifacts"] and not (directory / "program").exists()
            negative_rows.append({"case": directory.name, "status": report["status"],
                                  "phase": report["phase"], "verification_runs": 1,
                                  "diagnostics": report["diagnostics"], "native_execution": "NotRun"})
        else:
            compile_result = compile_record(directory / "report.json", False, False)
            assert saved_exit(directory / "exit.txt") == 0
            assert (directory / "stdout.bin").read_bytes() == b"A"
            assert (directory / "stderr.bin").read_bytes() == b""
            joint_rows.append({"case": directory.name, "compile": compile_result,
                               "full_result_guard": "value != 14 returns 1 in exact source",
                               "exit": 0, "stdout_hex": "41", "stderr_hex": ""})
        preserve_case(directory, "joint")
    assert len(joint_rows) == 4 and len(negative_rows) == 1
    assert {(r["compile"]["backend"], r["compile"]["optimization"]) for r in joint_rows} == {
        (backend, opt) for backend in ("clang", "llvm-api") for opt in ("O0", "O2")}

    identity = {"run_id": RUN, "candidate": CANDIDATE, "actual_checkout": CHECKOUT,
                "source_tree_sha256": sha(candidate_tree), "exact_source_tree_match": True,
                "workspace_log_sha256": sha((workspace / "workspace.log").read_bytes()),
                "native_manifest_sha256": sha((workspace / "workspace-native-manifest.json").read_bytes()),
                "original_manifest_files": 68300, "llvm_version": "23.1.4",
                "whole_workspace": {"passed": 3020, "failed": 4, "ignored": 0, "filtered": 0},
                "whole_run_acceptance": "Fail; these selected stages do not override workspace failure",
                "new_test_or_binary_execution": False}
    for category, destination in (("234", out234), ("joint", out_joint)):
        write_json(destination / "SOURCE-IDENTITY.json", identity)
        write_json(destination / "SELECTED-NATIVE-MANIFEST.json", {
            "original_manifest_sha256": identity["native_manifest_sha256"],
            "verified_files": checked_files[category], "retained_files": kept_files[category],
            "excluded_binary_policy": "program and object-cache payloads verified by size/SHA256 but not copied"})
    write_json(out234 / "SOURCE-SNAPSHOTS.json", source_records)
    write_json(out234 / "OBSERVATIONS.json", {"review234": review_rows, "impl234": impl_rows})
    write_json(out234 / "RESULT.json", {
        **identity, "status": "ScopedArtifactAuditPass", "scope": "First failed CI run, workspace stage only",
        "reviewer": "/root/safety_review", "builder210": {
            "suite": "9 passed / 0 failed", "new_width_test": "Pass in original CI log",
            "widths": [1, 8, 16, 32, 64], "generated_model_executions_from_frozen_test_loops": 40,
            "oracle": "signed/unsigned equivalent bits versus one-bit-neighbor, actual exit 1/0",
            "evidence_boundary": "Exact Rust test source and successful CI log; temporary per-case objects/receipts were deleted by test Drop, not independently archived here"},
        "impl234": {"rust_tests": 2, "compilations": 24, "native_observations": 88,
                    "negative_exit_observations": 64, "byte_stream_observations": 24,
                    "source_boundary": "Exact pinned test/source files retained; inline negative SourceUnit bytes were not written by original test, do not claim captured .zil files"},
        "review234": {"rust_tests": 1, "compilations": 8, "native_observations": 56,
                      "output_bytes_per_case": 12, "independent_little_endian_oracle_recomputed": True},
        "backends": ["LLVM API 23.1.4", "Clang 23.1.4"], "optimizations": ["O0", "O2"],
        "cold_warm_api_pairs": 8, "cold_builder_parse_ns_zero": True,
        "warm_matching_object_cache_keys": True, "fallbacks": 0,
        "warm_boundary": "Source Verify runs once again; LLVM object report correctly says producer verified, LLVM verification not repeated on cache hit",
        "verified_native_files": len(checked_files["234"]), "retained_native_files": len(kept_files["234"]),
        "final_234_acceptance": "Pending final green CI review by /root/next_language_slices"})
    write_json(out_joint / "OBSERVATIONS.json", {"positive": joint_rows, "negative": negative_rows})
    write_json(out_joint / "RESULT.json", {
        **identity, "status": "ScopedArtifactAuditPass", "scope": "Joint consumer in first failed CI run only",
        "reviewer": "/root/safety_review", "rust_tests": 2, "positive_native_observations": 4,
        "full_result": "14 compared as full i32 before choosing process exit 0/1",
        "cleanup": "All four stdout files contain exactly byte 0x41 (A), empty stderr, exit 0",
        "backends": ["LLVM API 23.1.4", "Clang 23.1.4"], "optimizations": ["O0", "O2"],
        "api_input_kind": "builder", "fallbacks": 0,
        "negative": "Exact moved-before-final-use source, SafeReject at Verify, E-BORROW-CONFLICT, verification_runs=1, no executable/native commands",
        "verified_native_files": len(checked_files["joint"]), "retained_native_files": len(kept_files["joint"]),
        "final_construction11_acceptance": "Pending corrected candidate and complete final green CI"})
    print(json.dumps({"status": "ScopedArtifactAuditPass", "whole_run_acceptance": "Fail",
                      "impl234_native_observations": 88, "review234_native_observations": 56,
                      "joint_positive_native_observations": 4, "joint_negative": 1,
                      "native_files_verified": {k: len(v) for k, v in checked_files.items()},
                      "native_files_retained": {k: len(v) for k, v in kept_files.items()}}, indent=2))


if __name__ == "__main__":
    main()
