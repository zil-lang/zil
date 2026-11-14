#!/usr/bin/env python3
"""Run separately identified foundation positives and original-obligation negatives.

This is source/check evidence only. Native and C consumers remain required.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    repo = args.repo.resolve()
    cli = args.cli.resolve()
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=True)
    profile_path = repo / "validation/language-foundation-profile.json"
    profile = json.loads(profile_path.read_text())
    rows = []
    for group in ("positive_cases", "negative_cases"):
        for case in profile[group]:
            directory = output / group / case["family"]
            directory.mkdir(parents=True, exist_ok=True)
            source = repo / case["source"]
            row = dict(case, group=group, source_sha256=digest(source), **{"pass": False})
            mismatches = [name for name, expected in case.get("project_sources", {}).items()
                          if digest(repo / name) != expected]
            if row["source_sha256"] != case["source_sha256"] or mismatches:
                row["project_manifest_mismatches"] = mismatches
                row.update(status="SourceManifestMismatch", verification_runs=0)
                rows.append(row)
                print(json.dumps(row, ensure_ascii=False), flush=True)
                continue
            command = [str(cli), "check", str(source), "--emit-dir", str(directory / "phases"), "--json"]
            row["command"] = command
            try:
                run = subprocess.run(command, cwd=repo, capture_output=True, text=True, timeout=180)
                (directory / "compiler.stdout").write_text(run.stdout)
                (directory / "compiler.stderr").write_text(run.stderr)
                try:
                    report = json.loads(run.stdout)
                except json.JSONDecodeError:
                    report = {"status": "InvalidCompilerReport"}
                row.update(exit=run.returncode, status=report.get("status"),
                           verification_runs=report.get("verification_runs", 0),
                           diagnostics=report.get("diagnostics", []))
                for phase in ("core", "typed"):
                    path = directory / "phases" / (phase + ".txt")
                    dump = path.read_text() if path.exists() else ""
                    row[phase + "_original_complete"] = (
                        bool(re.search(r"semantic:\s*Some\(", dump))
                        and "FunctionWithBody {" in dump
                        and not re.search(r"body:\s*Function\s*\{", dump))
                if group == "positive_cases":
                    row["pass"] = (run.returncode == 0 and row["status"] == "Success"
                                   and type(row["verification_runs"]) is int
                                   and row["verification_runs"] > 0
                                   and row["core_original_complete"]
                                   and row["typed_original_complete"])
                else:
                    # Require the missing generic obligation itself. An
                    # unrelated reject or an unsupported phase is no success.
                    intended = any(
                        diagnostic.get("phase") in ("types", "analyze", "verify")
                        and "syncdrop" in json.dumps(diagnostic, ensure_ascii=False).casefold()
                        for diagnostic in row["diagnostics"])
                    row["pass"] = (run.returncode == 1 and row["status"] == "SafeReject"
                                   and intended and row["core_original_complete"])
            except subprocess.TimeoutExpired as error:
                (directory / "compiler.stdout").write_bytes(error.stdout or b"")
                (directory / "compiler.stderr").write_bytes(error.stderr or b"")
                row.update(status="TimedOut", verification_runs=0, diagnostics=[])
            rows.append(row)
            print(json.dumps(row, ensure_ascii=False), flush=True)
    totals = {}
    for group in ("positive_cases", "negative_cases"):
        selected = [row for row in rows if row["group"] == group]
        totals[group] = {"passed": sum(row["pass"] for row in selected), "total": len(selected)}
    receipt = {
        "candidate": subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip(),
        "cli_sha256": digest(cli), "profile_sha256": digest(profile_path),
        "scope": "Foundation source/check evidence; not native, C-provider or Alpha acceptance",
        "rows": rows, **totals,
    }
    (output / "receipt.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")
    return 0 if all(row["pass"] for row in rows) else 1


if __name__ == "__main__":
    raise SystemExit(main())
