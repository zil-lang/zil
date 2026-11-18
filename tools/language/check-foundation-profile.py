#!/usr/bin/env python3
"""Run separately identified foundation positives and original-obligation negatives.

This is source/check evidence only. Native and C consumers remain required.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
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
    parser.add_argument("--build-receipt", type=Path,
                        help="Compiler build receipt binding a source commit to the tested CLI SHA-256")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--jobs", type=int, choices=range(1, 5), default=4,
                        help="Independent compiler processes; 1..4 (default: 4)")
    parser.add_argument("--work-profile", choices=("standard", "alpha"), default="standard")
    args = parser.parse_args()
    repo = args.repo.resolve()
    cli = args.cli.resolve()
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=True)
    profile_path = repo / "validation/language-foundation-profile.json"
    fixture_revision = subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
    cli_hash = digest(cli)
    candidate = None
    policy_path = repo / "validation/language-alpha-resource-profile.json"
    profile_bytes = profile_path.read_bytes()
    policy_bytes = policy_path.read_bytes()
    if args.build_receipt:
        build = json.loads(args.build_receipt.read_bytes())
        if build.get("exit_code") != 0 or build.get("cli_sha256") != cli_hash:
            parser.error("the build receipt must identify a successful build of the tested CLI")
        candidate = build["source_commit"]
        if not re.fullmatch(r"[0-9a-f]{40}", candidate):
            parser.error("the build receipt must specify a full source commit")
        for path, content in ((profile_path, profile_bytes), (policy_path, policy_bytes)):
            committed = subprocess.check_output(
                ["git", "-C", str(repo), "show", f"{candidate}:{path.relative_to(repo)}"])
            if committed != content:
                parser.error(f"{path.name} differs from the compiler candidate")
    profile = json.loads(profile_bytes)
    expected = json.loads(policy_bytes)["profiles"][args.work_profile]
    def run_case(selected):
        group, case = selected
        directory = output / group / case["family"]
        directory.mkdir(parents=True, exist_ok=True)
        source = repo / case["source"]
        row = dict(case, group=group, source_sha256=digest(source), **{"pass": False})
        mismatches = [name for name, expected in case.get("project_sources", {}).items()
                      if digest(repo / name) != expected]
        if row["source_sha256"] != case["source_sha256"] or mismatches:
            row["project_manifest_mismatches"] = mismatches
            row.update(status="SourceManifestMismatch", verification_runs=0)
            return row
        command = [str(cli), "check", str(source), "--work-profile", args.work_profile,
                   "--emit-dir", str(directory / "phases"), "--json"]
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
            row.update(work_profile=report.get("work_profile"),
                       work_profile_revision=report.get("work_profile_revision"),
                       effective_limits=report.get("effective_limits"))
            row["work_policy_matches"] = (
                row["work_profile"] == args.work_profile
                and row["work_profile_revision"] == expected["revision"]
                and row["effective_limits"] == expected["effective_limits"])
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
                               and row["typed_original_complete"] and row["work_policy_matches"])
            else:
                # Require the missing generic obligation itself. An
                # unrelated reject or an unsupported phase is no success.
                intended = any(
                    diagnostic.get("phase") in ("types", "analyze", "verify")
                    and "syncdrop" in json.dumps(diagnostic, ensure_ascii=False).casefold()
                    for diagnostic in row["diagnostics"])
                row["pass"] = (run.returncode == 1 and row["status"] == "SafeReject"
                               and intended and row["core_original_complete"] and row["work_policy_matches"])
        except subprocess.TimeoutExpired as error:
            (directory / "compiler.stdout").write_bytes(error.stdout or b"")
            (directory / "compiler.stderr").write_bytes(error.stderr or b"")
            row.update(status="TimedOut", verification_runs=0, diagnostics=[])
        return row

    selected = [(group, case) for group in ("positive_cases", "negative_cases")
                for case in profile[group]]
    rows = []
    # Each request owns its artifacts and compiler process. Keep receipt order
    # deterministic while independent positive/negative programs run together.
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        for row in pool.map(run_case, selected):
            rows.append(row)
            print(json.dumps(row, ensure_ascii=False), flush=True)
    totals = {}
    for group in ("positive_cases", "negative_cases"):
        selected = [row for row in rows if row["group"] == group]
        totals[group] = {"passed": sum(row["pass"] for row in selected), "total": len(selected)}
    if digest(cli) != cli_hash:
        parser.error("the CLI changed during validation")
    receipt = {
        "candidate": candidate, "fixture_checkout": fixture_revision,
        "cli_sha256": cli_hash, "profile_sha256": hashlib.sha256(profile_bytes).hexdigest(),
        "work_profile": args.work_profile,
        "scope": "Foundation source/check evidence; not native, C-provider or Alpha acceptance",
        "rows": rows, **totals,
    }
    (output / "receipt.json").write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + "\n")
    return 0 if all(row["pass"] for row in rows) else 1


if __name__ == "__main__":
    raise SystemExit(main())
