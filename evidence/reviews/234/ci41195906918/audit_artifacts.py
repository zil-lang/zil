#!/usr/bin/env python3
"""Read this CI run's archived outputs; never invoke a compiler or executable.

This audit records a stage result. FINAL additionally requires the other stage
and the integration owner's complete green gate receipts. Counters are derived
from the selected current-run files, not copied from an earlier run.
"""
from pathlib import Path
import argparse
import hashlib
import io
import json
import re
import subprocess
import sys
import tarfile

RUN = 41195906918
CANDIDATE = "c0418f532e750e7df2b00d9b7d76da82baa5086c"
CHECKOUT = "db07200ba221bdb67f3f1b7bef7f8e9851592a1a"
IMPLEMENTATION = "32f218d419cc162f8f51e5a24a5e2fb24d50e3b0"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read_json(path):
    return json.loads(path.read_text())


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--stage", choices=["workspace", "reference"], required=True)
    args = parser.parse_args()
    repo, artifacts, stage = args.repo.resolve(), args.artifacts.resolve(), args.stage
    phase, native = artifacts / stage, artifacts / f"{stage}-native"
    output = repo / f"evidence/reviews/234/ci{RUN}/{stage}"
    joint_output = repo / f"evidence/reviews/construction11/ci{RUN}/{stage}"
    output.mkdir(parents=True, exist_ok=True)
    joint_output.mkdir(parents=True, exist_ok=True)

    def git_bytes(commit, path):
        return subprocess.check_output(["git", "show", f"{commit}:{path}"], cwd=repo)

    tree = subprocess.check_output(["git", "ls-tree", "-r", CANDIDATE], cwd=repo)
    assert tree == (phase / "source-tree.txt").read_bytes()
    assert (phase / "commit.txt").read_text().strip() == CHECKOUT
    toolchain = read_json(phase / "llvm-api-toolchain.json")
    assert toolchain["status"] == "Pass"
    assert all(toolchain["checks"][name]["version"] == "23.1.4" for name in ["llvm", "clang", "lld_link"])
    log = (phase / f"{stage}.log").read_text()
    rows = re.findall(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;", log)
    totals = dict(zip(["passed", "failed", "ignored", "measured", "filtered_out"], map(sum, zip(*(map(int, r) for r in rows)))))
    assert totals["passed"] > 0 and all(totals[k] == 0 for k in ["failed", "ignored", "filtered_out"])
    manifest_path = phase / f"{stage}-native-manifest.json"
    manifest = read_json(manifest_path)
    entries = {row["path"]: row for row in manifest["files"]}
    assert len(entries) == len(manifest["files"])
    download_records = []
    for suffix in ["", "-native"]:
        receipt_path = artifacts / f"{stage}{suffix}-download-receipt.json"
        if not receipt_path.exists():
            continue
        receipt = read_json(receipt_path)
        archive_path = artifacts / receipt["file"]
        if archive_path.exists():
            with archive_path.open("rb") as file:
                digest = hashlib.file_digest(file, "sha256").hexdigest()
            assert archive_path.stat().st_size == receipt["bytes"] and digest == receipt["sha256"]
        download_records.append({"receipt": receipt, "local_zip_rehashed": archive_path.exists()})
        (output / receipt_path.name).write_bytes(receipt_path.read_bytes())
    for name in ["commit.txt", "toolchain.txt", "llvm-api-toolchain.json"]:
        (output / name).write_bytes((phase / name).read_bytes())

    source_paths = [
        "crates/zil-llvm-api/src/native/builder.rs",
        "crates/zil-llvm-api/tests/builder210.rs",
        "crates/zil-llvm-api/tests/support210/mod.rs",
        "crates/zil-driver/tests/impl234_narrow_constants.rs",
        "crates/zil-driver/tests/review234_full_width.rs",
        "crates/zil-driver/tests/construction11_integration.rs",
        "examples/mainline/byte_io/main.zil",
        "examples/mainline/byte_io/io.zil",
    ]
    sources = {path: git_bytes(CANDIDATE, path) for path in source_paths}
    for name in source_paths[:2] + [source_paths[3]]:
        assert sources[name] == git_bytes(IMPLEMENTATION, name)
    source_records = [{"path": p, "bytes": len(b), "sha256": sha(b)} for p, b in sources.items()]

    def group(name):
        start = log.index(f"     Running tests/{name}.rs")
        end = log.index("\n", log.index("test result:", start))
        text = log[start:end + 1]
        match = re.search(r"test result: ok\. (\d+) passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;", text)
        assert match, name
        target = joint_output if name == "construction11_integration" else output
        (target / f"{name}.log").write_text(text)
        return int(match[1])

    test_counts = {name: group(name) for name in ["builder210", "impl234_narrow_constants", "review234_full_width", "construction11_integration"]}
    assert test_counts["impl234_narrow_constants"] == 2
    assert test_counts["review234_full_width"] == 1
    assert test_counts["construction11_integration"] == 2
    if stage == "workspace":
        assert test_counts["builder210"] == 9
        assert "test signed_and_unsigned_spellings_of_integer_bits_are_identical_at_all_widths ... ok" in log

    selected = {"234": [], "joint": []}
    kept = {"234": [], "joint": []}
    archive_members = {"234": [], "joint": []}

    def retain(directory, category):
        for path in sorted(directory.rglob("*")):
            if not path.is_file():
                continue
            assert not path.is_symlink() and path.resolve().is_relative_to(native)
            relative = path.relative_to(native).as_posix()
            row = entries[relative]
            data = path.read_bytes()
            assert len(data) == row["bytes"] and sha(data) == row["sha256"], relative
            selected[category].append(row)
            inside = path.relative_to(directory)
            include = len(inside.parts) == 1 and path.name != "program"
            if category == "joint" and ("active-loan-reject" in directory.name or "LlvmApi-O2" in directory.name):
                include |= inside.as_posix() in {"phases/core.txt", "phases/typed.txt", "phases/candidate.txt", "phases/verified.txt"}
            if include:
                kept[category].append(row)
                archive_members[category].append((path, f"cases/{relative}"))

    def compile_report(path, warm, cached):
        report = read_json(path)
        assert report["status"] == "Success" and report["phase"] == "link"
        assert report["verification_runs"] == 1
        cg = report["native_codegen"]
        assert cg["backend"] in ["clang", "llvm-api"] and cg["fallback"] is None
        commands = report["commands"]
        assert commands and all(c[0].endswith("/llvm-23.1.4/bin/clang") for c in commands)
        opts = {arg[1:] for c in commands for arg in c if arg in ["-O0", "-O2"]}
        assert len(opts) == 1
        opt = opts.pop()
        if cg["backend"] == "clang":
            assert not cg["object_reports"] and all("-x" in c and "ir" in c for c in commands)
        else:
            objects = cg["object_reports"]
            assert objects and all(o["llvm_version"] == "23.1.4" and o["parse_ns"] == 0 for o in objects)
            assert cg["object_cache"]["corrupt"] == 0
            if warm and cached:
                assert cg["object_cache"]["enabled"] and cg["object_cache"]["hits"] > 0
                assert all(o["input_kind"] == "object-cache" and o["producer_verified_before_after"] is True and o["llvm_verification_this_request"] is False for o in objects)
            else:
                assert all(o["input_kind"] == "builder" and o["verify_before_after"] is True and o["pipeline"] == f"default<{opt}>" for o in objects)
                if cached:
                    cache = cg["object_cache"]
                    assert cache["requested"] and cache["enabled"] and cache["hits"] == 0 and cache["misses"] > 0 and cache["stored"] > 0
        return {"report": path.relative_to(native).as_posix(), "input_fingerprint": report["input_fingerprint"], "warm": warm, "optimization": opt, "verification_runs": report["verification_runs"], **cg}

    pairs = []

    def compile_pair(directory, cold_name, warm_name):
        cold = compile_report(directory / cold_name, False, True)
        warm = compile_report(directory / warm_name, True, True)
        assert (cold["backend"], cold["optimization"], cold["input_fingerprint"]) == (warm["backend"], warm["optimization"], warm["input_fingerprint"])
        if cold["backend"] == "llvm-api":
            keys = [o["cache"]["action_key"] for o in cold["object_reports"]]
            assert keys == [o["cache"]["action_key"] for o in warm["object_reports"]]
            pairs.append({"case": directory.name, "action_keys": keys, "cold_builder": True, "warm_object_cache": True, "source_verify_runs_each": 1})
        return [cold, warm]

    def saved_exit(path):
        match = re.fullmatch(r"Some\((-?\d+)\)\s*", path.read_text())
        assert match, path
        return int(match[1])

    full_source = re.search(r'const SOURCE: &str = r#"(.*?)"#;', sources["crates/zil-driver/tests/review234_full_width.rs"].decode(), re.S).group(1).encode()
    inputs = [b"", bytes([0]), bytes([1]), bytes([2]), bytes([3]), bytes([4]), bytes([255])]

    def mathematical_returns(input_bytes):
        x = input_bytes[0] if input_bytes else -1
        if x < 0:
            return -(2 ** 31), -(2 ** 63)
        if x == 0:
            return -257, -4294967297
        if x == 1:
            return -256, -4294967296
        if x == 2:
            return -1, -1
        if x == 3:
            return 2 ** 31 - 1, 2 ** 63 - 1
        if x == 4:
            return -(2 ** 31) + 256, -(2 ** 63) + 256
        return -2, -2

    full_rows = []
    for directory in sorted(native.glob("review234-*")):
        assert (directory / "source.zil").read_bytes() == full_source
        compiles = compile_pair(directory, "compile-false.json", "compile-true.json")
        observations = []
        for temperature in ["false", "true"]:
            for index, input_bytes in enumerate(inputs):
                prefix = directory / f"{temperature}-{index}"
                signed32, signed64 = mathematical_returns(input_bytes)
                expected = signed32.to_bytes(4, "little", signed=True) + signed64.to_bytes(8, "little", signed=True)
                actual = prefix.with_suffix(".stdout").read_bytes()
                assert prefix.with_suffix(".input").read_bytes() == input_bytes
                assert prefix.with_suffix(".expected").read_bytes() == expected
                assert actual == expected and len(actual) == 12
                assert prefix.with_suffix(".stderr").read_bytes() == b"" and saved_exit(prefix.with_suffix(".exit")) == 0
                observations.append({"warm": temperature == "true", "index": index, "input_hex": input_bytes.hex(), "i32": signed32, "i64": signed64, "expected_hex": expected.hex(), "actual_hex": actual.hex(), "exit": 0})
        retain(directory, "234")
        full_rows.append({"case": directory.name, "compiles": compiles, "observations": observations})

    backends = ["clang", "llvm-api"] if stage == "workspace" else ["clang"]
    expected_modes = {(backend, opt) for backend in backends for opt in ["O0", "O2"]}
    assert len(full_rows) == len(expected_modes)
    assert {(r["compiles"][0]["backend"], r["compiles"][0]["optimization"]) for r in full_rows} == expected_modes

    impl_rows = []
    for directory in sorted(native.glob("zil-impl234-narrow-*")):
        compiles = compile_pair(directory, "compile-0.json", "compile-1.json")
        report = read_json(directory / "compile-0.json")
        stream = any(p.endswith("examples/mainline/byte_io/main.zil") for p in report["source_files"].values())
        if stream:
            fixture, cases = "byte_stream", [b"", b"hello, Zil!\n\0\x80", b"Abc\0\xff\n" * 100]
        else:
            llvm = (directory / "program.ll").read_text()
            assert "declare i32 @getchar()" in llvm
            fixture = "negative_loop" if "@llvm.ssub.with.overflow.i32" in llvm else "negative_branch"
            cases = [b"", b"\0", b"1", b"Z"]
        observations = []
        assert len(list(directory.glob("run-*.exit"))) == 2 * len(cases)
        for index in range(2 * len(cases)):
            input_bytes = cases[index % len(cases)]
            prefix = directory / f"run-{index}"
            if stream:
                expected_output, code = input_bytes.upper(), 0
            else:
                x = input_bytes[0] if input_bytes else -1
                code = 254 if x < 0 or x > 64 else 0 if fixture == "negative_loop" or x == 0 else 255
                expected_output = b""
            assert prefix.with_suffix(".input").read_bytes() == input_bytes
            assert prefix.with_suffix(".stdout").read_bytes() == expected_output
            assert prefix.with_suffix(".stderr").read_bytes() == b"" and saved_exit(prefix.with_suffix(".exit")) == code
            observations.append({"warm": index >= len(cases), "index": index, "input_bytes": len(input_bytes), "input_sha256": sha(input_bytes), "stdout_sha256": sha(expected_output), "exit": code})
        retain(directory, "234")
        impl_rows.append({"case": directory.name, "fixture": fixture, "identification": "byte-stream report source path, otherwise emitted subtraction-loop CFG; never selected from runtime output", "compiles": compiles, "observations": observations})
    expected_fixtures = {(f, b, o) for f in ["byte_stream", "negative_loop", "negative_branch"] for b, o in expected_modes}
    assert len(impl_rows) == len(expected_fixtures)
    assert {(r["fixture"], r["compiles"][0]["backend"], r["compiles"][0]["optimization"]) for r in impl_rows} == expected_fixtures

    joint_sources = {name: git_bytes(CANDIDATE, f"examples/mainline/construction11/{name}.zil") for name in ["main", "rules", "data"]}
    assert b"if value != 14 { return 1 }" in joint_sources["main"]
    positive, negative = [], []
    for directory in sorted(native.glob("construction11-*")):
        bad = "active-loan-reject" in directory.name
        for name, text in joint_sources.items():
            expected = text
            if bad and name == "main":
                expected = text.replace(b"let value = callback() + callback()\n    let moved = item", b"let moved = item\n    let value = callback() + callback()")
                assert expected != text
            assert (directory / f"{name}.zil").read_bytes() == expected
        if bad:
            report = read_json(directory / "report.json")
            assert report["status"] == "SafeReject" and report["phase"] == "verify" and report["verification_runs"] == 1
            assert any(d["code"] == "E-BORROW-CONFLICT" for d in report["diagnostics"])
            assert report["native_codegen"] is None and report["commands"] == []
            assert "executable" not in report["artifacts"] and not (directory / "program").exists()
            negative.append({"case": directory.name, "report": report, "native_execution": "NotRun"})
        else:
            compile_record = compile_report(directory / "report.json", False, False)
            assert saved_exit(directory / "exit.txt") == 0
            assert (directory / "stdout.bin").read_bytes() == b"A" and (directory / "stderr.bin").read_bytes() == b""
            positive.append({"case": directory.name, "compile": compile_record, "full_i32_result_guard": 14, "exit": 0, "stdout_hex": "41", "stderr_hex": ""})
        retain(directory, "joint")
    assert len(negative) == 1 and len(positive) == len(expected_modes)
    assert {(r["compile"]["backend"], r["compile"]["optimization"]) for r in positive} == expected_modes

    archive_records = {}
    for category, destination in [("234", output), ("joint", joint_output)]:
        archive_path = destination / "raw-artifacts.tar.gz"
        with tarfile.open(archive_path, "w:gz") as archive:
            for path, name in archive_members[category]:
                archive.add(path, arcname=name, recursive=False)
            for path, content in sources.items():
                if (category == "joint") != (path.endswith("construction11_integration.rs")):
                    continue
                info = tarfile.TarInfo(f"source/{path}")
                info.size = len(content)
                archive.addfile(info, io.BytesIO(content))
        archive_records[category] = {"path": archive_path.name, "bytes": archive_path.stat().st_size, "sha256": sha(archive_path.read_bytes())}
        write_json(destination / "SELECTED-NATIVE-MANIFEST.json", {"original_manifest_sha256": sha(manifest_path.read_bytes()), "verified_files": selected[category], "retained_files": kept[category], "omissions": "Executables and nested object-cache payloads are hash-checked against the uploaded manifest but not copied."})

    identity = {
        "run_id": RUN, "stage": stage, "reviewer": "/root/next_language_slices",
        "audit_command": ["python3", *sys.argv], "audit_workdir": str(repo),
        "implementation_commit": IMPLEMENTATION, "candidate_commit": CANDIDATE, "actual_checkout": CHECKOUT,
        "source_tree_sha256": sha(tree), "source_tree_byte_equal": True,
        "source_snapshots": source_records, "log_sha256": sha((phase / f"{stage}.log").read_bytes()),
        "native_manifest_sha256": sha(manifest_path.read_bytes()), "uploaded_manifest_files": len(entries),
        "stage_test_totals": totals, "stage_test_groups": len(rows), "toolchain": toolchain,
        "download_receipts": download_records, "new_test_or_native_execution_during_audit": False,
    }
    write_json(output / "OBSERVATIONS.json", {"review234": full_rows, "impl234": impl_rows, "api_cold_warm_pairs": pairs})
    result = {
        **identity, "verdict": "StageArtifactAuditPass_FinalPending", "test_groups": test_counts,
        "review234": {"compilations": sum(len(r["compiles"]) for r in full_rows), "native_observations": sum(len(r["observations"]) for r in full_rows), "returned_bytes_per_observation": 12, "oracle": "Independently recomputed little-endian signed i32 plus i64; process exit only reports success."},
        "impl234": {"compilations": sum(len(r["compiles"]) for r in impl_rows), "native_observations": sum(len(r["observations"]) for r in impl_rows), "negative_exit_observations": sum(len(r["observations"]) for r in impl_rows if r["fixture"] != "byte_stream"), "byte_stream_observations": sum(len(r["observations"]) for r in impl_rows if r["fixture"] == "byte_stream"), "source_boundary": "Inline negative SourceUnit bytes were not emitted by the original test; exact pinned Rust source and emitted CFG retained. Negative exit observations are not a full-width integer oracle."},
        "builder210": {"passed_rust_tests": test_counts["builder210"], "widths": [1, 8, 16, 32, 64], "generated_model_executions_from_frozen_loops": 5 * 2 * 2 * 2 if stage == "workspace" else 0, "oracle": "Actual full-width Compare eq then boolean exit; per-case temporary objects deleted by original test. Exact source and current CI success log retained."},
        "api_cold_warm_pairs": len(pairs), "actual_modes": sorted(expected_modes),
        "verified_native_files": len(selected["234"]), "retained_native_files": len(kept["234"]),
        "raw_archive": archive_records["234"], "final_acceptance": "Pending both stage audits and complete green integration-gate receipts; this stage file alone is not FINAL.",
    }
    joint_result = {**identity, "verdict": "StageArtifactAuditPass_FinalPending", "rust_tests": test_counts["construction11_integration"], "positive_native_observations": len(positive), "negative_source_rejections": len(negative), "actual_modes": sorted(expected_modes), "full_i32_comparison": 14, "cleanup_stdout_hex": "41", "negative": "Verify/E-BORROW-CONFLICT, verification_runs=1, no executable or native commands", "verified_native_files": len(selected["joint"]), "retained_native_files": len(kept["joint"]), "raw_archive": archive_records["joint"]}
    write_json(output / "RESULT.json", result)
    write_json(joint_output / "OBSERVATIONS.json", {"positive": positive, "negative": negative})
    write_json(joint_output / "RESULT.json", joint_result)
    print(json.dumps({"run_id": RUN, "stage": stage, "actual_checkout": CHECKOUT, "verdict": result["verdict"], "totals": totals, "review234": result["review234"], "impl234": result["impl234"], "api_pairs": len(pairs), "joint_positives": len(positive), "joint_negatives": len(negative), "verified_files": {k: len(v) for k, v in selected.items()}}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
