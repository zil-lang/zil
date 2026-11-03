#!/usr/bin/env python3
"""Check the frozen 52 inputs with a new compiler; preserve the old receipt."""
import argparse
import collections
import hashlib
import json
from pathlib import Path
import re
import subprocess


ROOT = Path(__file__).resolve().parents[3]
HISTORY = ROOT / "docs/validation/language-gaps-334.json"
HISTORY_SHA256 = "ce3aadadfba28d9d5299a698e36664b7b4316a38d0ebd4a4e8bf39871ea125b0"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--zil", required=True, type=Path)
    parser.add_argument("--compiler-source", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    old_bytes = HISTORY.read_bytes()
    if sha(old_bytes) != HISTORY_SHA256:
        parser.error("the frozen historical receipt changed")
    old = json.loads(old_bytes)
    binary = args.zil.resolve(strict=True)
    binary_hash = sha(binary.read_bytes())
    if binary_hash == old["compiler"]["sha256"]:
        parser.error("the historical compiler cannot establish new candidate behavior")
    if args.output.resolve() == HISTORY.resolve():
        parser.error("the historical receipt is immutable; choose a separate output")
    if not re.fullmatch(r"[0-9a-f]{40}", args.compiler_source):
        parser.error("compiler-source must be the full frozen commit")
    subprocess.run(["git", "cat-file", "-e", args.compiler_source + "^{commit}"],
                   cwd=ROOT, check=True, capture_output=True)
    assert len(old["observations"]) == 52
    observations, changes, regressions, infrastructure = [], [], [], []
    for prior in old["observations"]:
        source = prior["source"]
        path = ROOT / source
        assert ROOT in path.resolve().parents
        assert sha(path.read_bytes()) == prior["sha256"], source
        command = [str(binary), "check", source, "--json"]
        try:
            process = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=60)
        except subprocess.TimeoutExpired:
            infrastructure.append({"source": source, "reason": "CLI timeout", "seconds": 60})
            continue
        try:
            report = json.loads(process.stdout)
        except ValueError:
            infrastructure.append({"source": source, "exit_code": process.returncode,
                                   "stdout_sha256": sha(process.stdout),
                                   "stderr": process.stderr.decode(errors="replace")[:2000]})
            continue
        row = {"source": source, "sha256": prior["sha256"],
               "exit_code": process.returncode, "status": report["status"],
               "phase": report["phase"], "verification_runs": report["verification_runs"],
               "diagnostics": [{k: d[k] for k in ["phase", "category", "code", "message"]}
                               for d in report["diagnostics"]],
               "stdout_sha256": sha(process.stdout), "stderr": process.stderr.decode()}
        before = {k: prior[k] for k in ["status", "phase", "exit_code", "verification_runs"]}
        after = {k: row[k] for k in before}
        if before != after or [d["code"] for d in prior["diagnostics"]] != [d["code"] for d in row["diagnostics"]]:
            changes.append({"source": source, "before": before, "after": after,
                            "before_codes": [d["code"] for d in prior["diagnostics"]],
                            "after_codes": [d["code"] for d in row["diagnostics"]]})
        if prior["status"] == "Success" and row["status"] != "Success":
            regressions.append({"source": source, "reason": "prior positive no longer succeeds"})
        if "/recovery/rejects/" in source or "/recovery/probes/" in source:
            if row["status"] != "SafeReject":
                regressions.append({"source": source, "reason": "protected rejection changed"})
        observations.append(row)
    result = {
        "kind": "Current compiler projection of the immutable334 source set; no native/full-language acceptance",
        "compiler": {"path": str(binary), "sha256": binary_hash,
                     "source_commit": args.compiler_source,
                     "source_identity_basis": "Caller supplies the frozen source revision; final review must also bind the observed build receipt. This collector does not authenticate compiler provenance."},
        "historical_receipt": "docs/validation/language-gaps-334.json",
        "historical_receipt_sha256": sha(old_bytes), "historical_receipt_modified": False,
        "selected": len(observations),
        "counts": dict(collections.Counter(r["status"] for r in observations)),
        "recovery25_counts": dict(collections.Counter(r["status"] for r in observations if "/recovery/" in r["source"])),
        "changes": changes, "regressions": regressions, "infrastructure": infrastructure,
        "observations": observations, "native_runs": 0,
    }
    assert HISTORY.read_bytes() == old_bytes
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({k: result[k] for k in ["selected", "counts", "recovery25_counts", "regressions", "infrastructure"]}))
    return int(bool(regressions or infrastructure))


if __name__ == "__main__":
    raise SystemExit(main())
