#!/usr/bin/env python3
"""Run only the 224 author slice and retain actual machine/raw/Wire evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[3]
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    raw = out / "raw"
    raw.mkdir()
    env = dict(os.environ, ZIL_TEST_REPORT_DIR=str(raw))
    command = ["cargo", "test", "--offline", "-p", "zil-driver", "--features", "llvm-api",
               "--test", "impl224_shared_product", "--", "--nocapture"]
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
    status = subprocess.check_output(["git", "status", "--porcelain"], cwd=repo, text=True)
    toolchains = {}
    for name, tool in [("rustc", "rustc"), ("cargo", "cargo"),
                       ("clang", env.get("ZIL_CLANG", "clang"))]:
        toolchains[name] = subprocess.check_output([tool, "--version"], text=True).strip()
    with (out / "cargo.log").open("w") as log:
        process = subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT)
    log = (out / "cargo.log").read_text()
    totals = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored", log)
    ok = process.returncode == 0 and totals == [("11", "0", "0")]
    raw_rows = []
    for path in sorted(raw.glob("*/input.wire")):
        row = {"case": path.parent.name, "wire_sha256": digest(path)}
        for route in ["raw", "wire"]:
            text = (path.parent / f"{route}.txt").read_text()
            row[route] = {"result": "Accept" if text.startswith("Ok(") else "Reject",
                          "codes": re.findall(r'code: "([^"]+)"', text),
                          "messages": re.findall(r'message: "([^"]+)"', text)}
        raw_rows.append(row)
    native_rows = []
    for path in sorted(raw.glob("*/*.json")):
        report = json.loads(path.read_text())
        if report.get("native_codegen"):
            native_rows.append({"case": path.parent.name,
                                "status": report["status"],
                                "native_codegen": report["native_codegen"],
                                "commands": report["commands"]})
    allocations = []
    for path in sorted(raw.glob("*/failure-*.stderr")):
        row = json.loads(path.with_suffix(".json").read_text())
        row.update(case=path.parent.name, allocator=json.loads(path.read_text()),
                   stdout=path.with_suffix(".stdout").read_text())
        allocations.append(row)
    boundaries = [json.loads(p.read_text()) for p in sorted(raw.glob("*/boundaries.json"))]
    manifest = [{"path": str(p.relative_to(raw)), "sha256": digest(p), "bytes": p.stat().st_size}
                for p in sorted(raw.rglob("*")) if p.is_file()]
    source_paths = subprocess.check_output(
        ["git", "ls-files", "crates", "examples/mainline/shared_product224", "tools/language/224"],
        cwd=repo, text=True).splitlines()
    sources = [{"path": p, "sha256": digest(repo / p)} for p in source_paths]
    (out / "artifact-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (out / "source-manifest.json").write_text(json.dumps(sources, indent=2) + "\n")
    # Full actual phase snapshots and object files remain independently
    # replayable; compression changes storage only, not their evidence level.
    archive = out / "raw.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        tar.add(raw, arcname="raw")
    result = {
        "task": "224", "evidence_role": "implementation_author_limited_tests",
        "implementation_author": "/root/scope_audit", "tested_revision": revision,
        "source_worktree_status": status, "result": "Pass" if ok else "Fail",
        "command": command, "return_code": process.returncode,
        "tests": {"passed": int(totals[0][0]) if totals else 0,
                  "failed": int(totals[0][1]) if totals else None,
                  "ignored": int(totals[0][2]) if totals else None},
        "toolchains": toolchains, "environment": {name: env.get(name) for name in
            ["CARGO_TARGET_DIR", "CARGO_BUILD_JOBS", "CARGO_PROFILE_TEST_DEBUG",
             "CARGO_PROFILE_DEV_DEBUG", "CARGO_INCREMENTAL", "ZIL_LLVM_LIB_DIR", "ZIL_CLANG"]},
        "raw_wire": raw_rows, "native_compiles": native_rows,
        "allocator_runs": allocations, "phase_boundaries": boundaries,
        "raw_archive": {"file": archive.name, "sha256": digest(archive),
                        "bytes": archive.stat().st_size},
        "limits": ["Author tests are not independent review or whole-workspace acceptance.",
                   "Host Linux only; Windows and asynchronous execution are not tested.",
                   "No comparative Rust performance or product-gate conclusion."],
    }
    (out / "RESULT.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"result": result["result"], "tested_revision": revision,
                      "tests": result["tests"], "raw_wire_pairs": len(raw_rows),
                      "native_compiles": len(native_rows), "allocator_runs": len(allocations),
                      "output": str(out)}))
    raise SystemExit(0 if ok else 1)


if __name__ == "__main__":
    main()
