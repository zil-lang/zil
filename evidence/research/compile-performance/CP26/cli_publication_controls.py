#!/usr/bin/env python3
"""Correctness-only fresh-process controls for CP26 publication ownership.

The collision technique follows the independent CP22 counterexample: exec the
compiler without changing PID, and select its next private temp path as -o.
No timings from these controls are used as performance evidence.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--clang", type=Path, required=True)
    parser.add_argument("--case", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    case = args.case.resolve()
    case.mkdir(parents=True, exist_ok=False)
    evidence = args.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=False)
    original_binary_hash = sha(binary)
    original_binary_stat = binary.stat()
    source = case / "main.zil"
    source.write_text("fn main()->i32=>0\n")
    cache = case / "cache"
    rows = []

    def request(label, command, native=False):
        result = subprocess.run(command, capture_output=True)
        (evidence / f"{label}.stdout").write_bytes(result.stdout)
        (evidence / f"{label}.stderr").write_bytes(result.stderr)
        assert result.returncode == 0, (label, result.returncode, result.stderr)
        report = json.loads(result.stdout)
        assert report["status"] == "Success", report
        assert report["verification_runs"] == 1, report
        row = {"label": label, "command": command, "returncode": result.returncode,
               "status": report["status"], "verification_runs": report["verification_runs"],
               "parse_cache": report["parse_cache"], "query_cache": report["query_cache"]}
        if native:
            output = Path(report["artifacts"]["executable"])
            assert output.is_file(), (label, "successful build lost its published executable", report)
            process = subprocess.run([str(output)], capture_output=True)
            (evidence / f"{label}.native.stdout").write_bytes(process.stdout)
            (evidence / f"{label}.native.stderr").write_bytes(process.stderr)
            assert (process.returncode, process.stdout, process.stderr) == (0, b"", b""), process
            row.update({"executable": str(output), "native_exit": process.returncode,
                        "elf_sha256": sha(output), "llvm_sha256": sha(Path(str(output) + ".ll"))})
        rows.append(row)
        return report

    seed = request("seed", [str(binary), "check", str(source), "--query-cache", str(cache), "--json"])
    assert (seed["parse_cache"]["parsed"], seed["parse_cache"]["stored"]) == (1, 1), seed
    entries = list((cache / "parse-cp22-v1").glob("*.psyn"))
    assert len(entries) == 1, entries
    entry = entries[0]
    complete_entry = entry.read_bytes()
    entry.write_bytes(b"force current cold recovery")
    damaged_hash = sha(entry)
    helper = case / "exec_same_pid.py"
    helper.write_text(
        "import os, sys\n"
        "from pathlib import Path\n"
        "binary, source, cache, key, clang, enabled = sys.argv[1:]\n"
        "output = Path(cache) / 'parse-cp22-v1' / f'.{key}.{os.getpid()}.0.tmp'\n"
        "argv = [binary, 'build', source, '--json', '--clang', clang, '-o', str(output)]\n"
        "if enabled == 'yes': argv += ['--query-cache', cache]\n"
        "os.execv(binary, argv)\n"
    )
    common = [sys.executable, str(helper), str(binary), str(source), str(cache), entry.stem, str(args.clang.resolve())]
    uncached = request("uncached_collision_path", common + ["no"], native=True)
    assert uncached["parse_cache"]["requested"] is False
    collision = request("cached_collision_path", common + ["yes"], native=True)
    assert (collision["parse_cache"]["corrupt"], collision["parse_cache"]["parsed"],
            collision["parse_cache"]["stored"]) == (1, 1, 0), collision
    assert "File exists" in collision["parse_cache"]["write_reason"], collision
    assert sha(entry) == damaged_hash, "failed publication replaced the existing cache entry"

    program = case / "program"
    normal = [str(binary), "build", str(source), "--json", "--clang", str(args.clang.resolve()),
              "-o", str(program), "--query-cache", str(cache)]
    repaired = request("repaired", normal, native=True)
    assert (repaired["parse_cache"]["corrupt"], repaired["parse_cache"]["parsed"],
            repaired["parse_cache"]["stored"]) == (1, 1, 1), repaired
    assert entry.read_bytes() == complete_entry
    warm = request("warm", normal, native=True)
    assert (warm["parse_cache"]["hits"], warm["parse_cache"]["parsed"], warm["parse_cache"]["stored"]) == (1, 0, 0), warm
    assert warm["query_cache"]["state"] == "hit", warm

    # The collision outputs intentionally end in .tmp and are valid artifacts;
    # only names not reported as those outputs would be orphaned pending files.
    owned_outputs = {Path(row["executable"]) for row in rows if "executable" in row}
    pending = set((cache / "parse-cp22-v1").glob("*.tmp"))
    assert pending == {path for path in owned_outputs if path.suffix == ".tmp"}, pending
    after = binary.stat()
    assert sha(binary) == original_binary_hash
    assert (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns) == (
        original_binary_stat.st_dev, original_binary_stat.st_ino, original_binary_stat.st_size,
        original_binary_stat.st_mtime_ns, original_binary_stat.st_ctime_ns)
    manifest = {"source_commit": args.source_commit, "binary": str(binary),
                "binary_sha256": original_binary_hash, "scope": "correctness_only_no_performance_claim",
                "source_sha256": sha(source), "requests": len(rows), "native_runs": 4,
                "rows": rows, "native_elf_equal": len({row["elf_sha256"] for row in rows if "elf_sha256" in row}) == 1,
                "native_llvm_equal": len({row["llvm_sha256"] for row in rows if "llvm_sha256" in row}) == 1,
                "files": {path.name: sha(path) for path in sorted(evidence.iterdir()) if path.is_file()}}
    (evidence / "MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({"requests": len(rows), "native_runs": 4, "collision_preserved_executable": True,
                      "native_elf_equal": manifest["native_elf_equal"], "native_llvm_equal": manifest["native_llvm_equal"]}))


if __name__ == "__main__":
    main()
