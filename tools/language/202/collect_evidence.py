#!/usr/bin/env python3
"""Seal text-only 202 consumer evidence after both final platform runs pass."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import zipfile


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load(path):
    return json.loads(path.read_text())


def final_run(path, platform):
    report = load(path / "summary.json")
    observations = report.get("observations", [])
    planned = 2 * sum(1 + len(row["probes"]) for row in report["oracles"])
    if (report.get("status") != "Pass" or report.get("platform") != platform
            or not observations or report.get("executions") != len(observations)
            or len(observations) != planned
            or {build["optimization"] for build in report["builds"]} != {"O0", "O2"}
            or any(build["module_count"] != 5 or build["verification_runs"] != 1
                   for build in report["builds"])):
        raise ValueError(f"{path}: final {platform} consumer matrix is not complete")
    expected_executions = "native_executions" if platform == "linux" else "wine_executions"
    if report.get(expected_executions) != planned:
        raise ValueError(f"{path}: execution kind/count differs")
    for name, expected in report["sources"].items():
        if sha((path / "sources" / name).read_bytes()) != expected:
            raise ValueError(f"{path}: exact source snapshot differs: {name}")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--linux", type=Path, required=True)
    parser.add_argument("--wine", type=Path, required=True)
    parser.add_argument("--prior-run", action="append", type=Path, default=[])
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--max-bytes", type=int, default=50_000_000)
    args = parser.parse_args()
    # Do not create an archive, or count a planned execution, without both
    # actual successful final runs. Failed and pre-run evidence stays separate.
    linux, wine = final_run(args.linux, "linux"), final_run(args.wine, "win64")
    for key in ("source_commit", "compiler_source_commit", "cli_sha256", "sources",
                "design_sha256", "normative_inputs", "allocator_sources"):
        if linux[key] != wine[key]:
            raise ValueError(f"final platform identities differ: {key}")
    repo = args.repo.resolve()
    collector_commit = subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    if subprocess.check_output(["git", "-C", str(repo), "status", "--porcelain"]):
        raise ValueError("collector checkout must be clean")
    if args.out.exists() or args.out.with_suffix(".zip.partial").exists():
        raise ValueError("archive output already exists")
    payloads = {}
    excluded = []
    allowed = {".json", ".log", ".txt", ".ll", ".zil", ".py", ".c", ".md", ".patch"}
    directories = [("linux", args.linux), ("wine", args.wine)]
    directories += [(f"prior/{index}-{path.name}", path)
                    for index, path in enumerate(args.prior_run, 1)]
    for label, directory in directories:
        if not directory.is_dir():
            raise ValueError(f"evidence directory is missing: {directory}")
        for path in sorted(directory.rglob("*")):
            if not path.is_file():
                continue
            member = str(Path(label) / path.relative_to(directory))
            if path.suffix not in allowed:
                excluded.append(member)
                continue
            data = path.read_bytes()
            data.decode("utf-8")  # Executable/binary data cannot masquerade as text.
            payloads[member] = data
            if path.name == "command.json":
                receipt = json.loads(data)
                for name in ("stdout.log", "stderr.log"):
                    expected = receipt.get(name + "_sha256", receipt.get(name[:-4] + "_sha256"))
                    if expected and sha((path.parent / name).read_bytes()) != expected:
                        raise ValueError(f"command output differs: {path.parent / name}")
    # The compiler build receipt and its actual logs are part of its identity;
    # the frozen compiler executable is deliberately not part of the archive.
    build_receipt = Path(linux["compiler_build_receipt"])
    if sha(build_receipt.read_bytes()) != linux["compiler_build_receipt_sha256"]:
        raise ValueError("compiler build receipt changed")
    for name in ("command.json", "stdout.log", "stderr.log"):
        payloads["compiler-build/" + name] = (build_receipt.parent / name).read_bytes()
    for path in ("tools/language/202/consumer.py", "tools/language/173/allocator_probe.c",
                 "tools/language/181/heap_probe.c", "examples/mainline/store_template202/README.md"):
        payloads["run-source/" + path] = subprocess.check_output(
            ["git", "-C", str(repo), "show", linux["source_commit"] + ":" + path])
    payloads["collector/collect_evidence.py"] = Path(__file__).read_bytes()
    index = {"status": "Pass", "collector_source_commit": collector_commit,
             "run_source_commit": linux["source_commit"],
             "compiler_source_commit": linux["compiler_source_commit"],
             "cli_sha256": linux["cli_sha256"], "native_windows": "NotRun",
             "full_language": "NotSatisfied", "product": "NotRun",
             "execution_artifacts": False, "excluded_paths": excluded,
             "runs": {label: {key: report[key] for key in
                               ("executions", "native_executions", "wine_executions",
                                "ordinary_executions", "instrumented_executions",
                                "actual_allocation_failures")}
                      for label, report in (("linux", linux), ("wine", wine))}}
    payloads["INDEX.json"] = (json.dumps(index, indent=2) + "\n").encode()
    manifest = {name: {"bytes": len(data), "sha256": sha(data)}
                for name, data in sorted(payloads.items())}
    pending = args.out.with_suffix(".zip.partial")
    pending.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(pending, "x", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, data in sorted(payloads.items()):
            archive.writestr(name, data)
        archive.writestr("MANIFEST.json", json.dumps(manifest, indent=2) + "\n")
    with zipfile.ZipFile(pending) as archive:
        if archive.testzip() is not None:
            raise ValueError("ZIP CRC verification failed")
        for name, expected in manifest.items():
            data = archive.read(name)
            if len(data) != expected["bytes"] or sha(data) != expected["sha256"]:
                raise ValueError(f"archive payload differs: {name}")
    if pending.stat().st_size > args.max_bytes:
        raise ValueError("archive exceeds the configured byte cap; partial archive retained")
    # A no-overwrite final publication; input evidence is never removed.
    args.out.hardlink_to(pending)
    pending.unlink()
    print(json.dumps({"status": "Pass", "path": str(args.out.resolve()),
                      "bytes": args.out.stat().st_size,
                      "sha256": sha(args.out.read_bytes()), "payload_files": len(payloads),
                      "execution_artifacts": False}, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError) as error:
        raise SystemExit(json.dumps({"status": "NotSealed", "reason": str(error)}))
