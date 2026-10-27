#!/usr/bin/env python3
"""Replay the unchanged 159 gap corpus plus real 164 library consumers.

Every subprocess writes an incremental receipt. No historical run is reused,
and an incomplete run never becomes Pass merely because a directory exists.
"""
import argparse
import ast
import hashlib
import json
import pathlib
import subprocess
import time


def digest(path):
    with open(path, "rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def write(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    temporary.replace(path)


def command(argv, directory, timeout=90):
    directory.mkdir(parents=True, exist_ok=False)
    receipt = {"command": [str(x) for x in argv], "start": time.time(), "status": "Running"}
    write(directory / "command.json", receipt)
    try:
        result = subprocess.run(receipt["command"], capture_output=True, timeout=timeout)
        (directory / "stdout").write_bytes(result.stdout)
        (directory / "stderr").write_bytes(result.stderr)
        receipt.update(exit_code=result.returncode, status="Completed")
    except subprocess.TimeoutExpired as error:
        (directory / "stdout").write_bytes(error.stdout or b"")
        (directory / "stderr").write_bytes(error.stderr or b"")
        receipt.update(status="Timeout", exit_code=None)
        raise
    finally:
        receipt["end"] = time.time()
        receipt["seconds"] = receipt["end"] - receipt["start"]
        write(directory / "command.json", receipt)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=pathlib.Path, required=True)
    parser.add_argument("--cli", type=pathlib.Path, required=True)
    parser.add_argument("--out", type=pathlib.Path, required=True)
    parser.add_argument("--baseline", action="store_true")
    args = parser.parse_args()
    repo, cli, out = args.repo.resolve(), args.cli.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    original = repo / "tools/research/159/readiness.py"
    module = ast.parse(original.read_text())
    gaps = next(ast.literal_eval(node.value) for node in module.body
                if isinstance(node, ast.Assign)
                and any(isinstance(target, ast.Name) and target.id == "gaps" for target in node.targets))
    summary = {"status": "Running", "cli_sha256": digest(cli), "original_probe_sha256": digest(original),
               "source_commit": subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip(),
               "crates_tree": subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD:crates"], text=True).strip(),
               "consumer_sources": {str(path.relative_to(repo)): digest(path)
                                    for path in sorted((repo / "examples/mainline/zui_language164").glob("*.zil"))},
               "baseline": args.baseline, "gaps": [], "native": []}
    write(out / "summary.json", summary)
    required = set() if args.baseline else {"nested-owning-callback", "generic-trait-bound"}
    for name, source in gaps.items():
        path = out / (name + ".zil")
        path.write_text(source)
        result = command([cli, "check", path, "--json"], out / name)
        report = json.loads(result.stdout)
        row = {"capability": name, "source_sha256": digest(path), "exit_code": result.returncode,
               "status": report["status"], "phase": report["phase"], "diagnostics": report["diagnostics"],
               "verification_runs": report["verification_runs"]}
        summary["gaps"].append(row)
        write(out / "summary.json", summary)
        if report["status"] not in {"Success", "SafeReject", "Unsupported"}:
            raise RuntimeError(f"{name}: compiler/tool failure: {report}")
        if report["phase"] == "parse" or (result.returncode == 0) != (report["status"] == "Success"):
            raise RuntimeError(f"{name}: invalid probe or inconsistent compiler process result: {report}")
        if name in required and (report["status"] != "Success" or report["verification_runs"] != 1):
            raise RuntimeError(f"{name}: required source capability did not pass: {report}")
    if not args.baseline:
        source = repo / "examples/mainline/zui_language164/main.zil"
        for optimization in ("O0", "O2"):
            destination = out / optimization
            executable = out / (optimization + "-program")
            phases = out / (optimization + "-phases")
            build = command([cli, "build", source, "--opt", optimization, "-o", executable,
                             "--emit-dir", phases, "--json"], destination / "build")
            report = json.loads(build.stdout)
            if build.returncode or report["status"] != "Success" or report["verification_runs"] != 1:
                raise RuntimeError(f"{optimization}: consumer build failed: {report}")
            run = command([executable], destination / "run", timeout=30)
            if run.returncode or run.stdout != b"." * 1024:
                raise RuntimeError(f"{optimization}: value or unique-drop behavior differs")
            probe = out / (optimization + "-probe")
            linked = command(["clang", "-" + optimization, phases / "llvm.ll",
                              repo / "tools/research/142/allocator_probe.c", "-Wl,--wrap=realloc",
                              "-Wl,--wrap=free", "-o", probe], destination / "probe-link")
            if linked.returncode:
                raise RuntimeError(f"{optimization}: allocator probe link failed")
            observed = command([probe], destination / "probe-run", timeout=30)
            allocation = json.loads(observed.stderr)
            if observed.returncode or observed.stdout != b"." * 1024 or allocation["live"] != 0:
                raise RuntimeError(f"{optimization}: allocator or cleanup probe failed")
            if allocation["allocated"] != allocation["frees"]:
                raise RuntimeError(f"{optimization}: allocation imbalance")
            summary["native"].append({"optimization": optimization, "executions": 2,
                                      "events_per_execution": 1024, "drop_observations_per_execution": 1024,
                                      "llvm_sha256": digest(phases / "llvm.ll"), "allocation": allocation})
            write(out / "summary.json", summary)
    summary["status"] = "Completed" if args.baseline else "StagePass_FullZuiNotReady"
    summary["remaining_original_probes"] = [r["capability"] for r in summary["gaps"] if r["status"] != "Success"]
    write(out / "summary.json", summary)
    print(json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
