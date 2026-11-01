#!/usr/bin/env python3
"""Functional fault injection for the revised runner; never performance evidence."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def require(condition, message):
    if not condition:
        raise ValueError(message)


MOCK = '''#!/usr/bin/env python3
import json
from pathlib import Path
import sys
import time
root = Path(__file__).parent
mode = (root / "mode").read_text()
counter = root / "counter"
count = int(counter.read_text()) + 1 if counter.exists() else 1
counter.write_text(str(count))
if mode == "nonzero" and count == 3:
    print("partial output before exit", flush=True)
    print("intentional exit seven", file=sys.stderr, flush=True)
    raise SystemExit(7)
if mode == "timeout":
    print("partial stdout before timeout", flush=True)
    print("partial stderr before timeout", file=sys.stderr, flush=True)
    time.sleep(5)
if mode == "malformed":
    print("this is not JSON")
    raise SystemExit(0)
if mode == "persistence":
    (root / "run" / "bool-1-0-baseline.stdout").mkdir()
arm = "baseline" if len(sys.argv) == 3 else "factor"
compressed = Path(sys.argv[1]).stem == "bool-16"
fingerprint = "wrong" if mode == "mismatch" else "same"
if arm == "baseline":
    functions = [{"id": 1, "name": "work", "outcome": "Complete", "canonical_fingerprint": fingerprint}]
else:
    functions = [{"function": 1, "name": "work", "outcome": "CompleteFactored",
                  "check": "ExactOracleRows", "canonical_fingerprint": fingerprint}]
    if compressed:
        functions[0]["check"] = "ExpansionDeferred"
        functions[0]["canonical_fingerprint"] = None
if mode == "duplicate":
    functions.append(dict(functions[0]))
if mode == "extra_deferred":
    functions.append({"function": 9, "name": "extra", "outcome": "CompleteFactored",
                      "check": "ExpansionDeferred", "canonical_fingerprint": None})
print(json.dumps({"functions": functions}))
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    require(not args.out.exists(), "output already exists")
    args.out.mkdir(parents=True)
    runner = Path(__file__).with_name("factor_bench.py")
    summary = []
    scenarios = [
        ("success", None, 6, 0),
        ("nonzero", "NonzeroExit", 2, 1),
        ("timeout", "Timeout", 0, 1),
        ("oserror", "OSError", 0, 1),
        ("missing", "IdentityError", 0, 1),
        ("malformed", "ProtocolError", 0, 1),
        ("mismatch", "ProtocolError", 0, 1),
        ("duplicate", "ProtocolError", 0, 1),
        ("extra_deferred", "ProtocolError", 0, 1),
        ("source_mismatch", "PreparationError", 0, 1),
        ("persistence", "PersistenceError", 0, 1),
        ("compressed", None, 3, 0),
    ]
    for name, expected_failure, completed, failed in scenarios:
        root = args.out / name
        root.mkdir()
        controls = root / "controls"
        controls.mkdir()
        binary = root / "mock_compiler.py"
        binary.write_text(MOCK)
        binary.chmod(0o644 if name == "oserror" else 0o755)
        (root / "mode").write_text(name)
        case = "bool-16" if name in ["compressed", "extra_deferred"] else "bool-1"
        source = root / f"{case}.zil"
        source.write_text("functional fixture; not a source-language benchmark\n")
        write_json(controls / f"{case}.factor.command.json", {
            "command": [str(binary), str(source)],
            "source_sha256": "wrong" if name == "source_mismatch" else sha(source),
        })
        write_json(controls / f"{case}.factor.json", {"functions": [{
            "function": 1,
            "name": "work", "outcome": "CompleteFactored",
            "check": "ExpansionDeferred" if case == "bool-16" else "ExactOracleRows",
            "canonical_fingerprint": None if case == "bool-16" else "same",
        }]})
        launched_binary = root / "absent_binary" if name == "missing" else binary
        out = root / "run"
        command = [sys.executable, str(runner), "--factor", str(launched_binary),
                   "--baseline", str(launched_binary), "--controls", str(controls),
                   "--out", str(out), "--window", "FUNCTIONAL-FAULT-CONTROL-NOT-TIMING",
                   "--cases", case, "--blocks", "3", "--timeout-seconds", "0.2"]
        result = subprocess.run(command, capture_output=True, timeout=20)
        (root / "runner.stdout").write_bytes(result.stdout)
        (root / "runner.stderr").write_bytes(result.stderr)
        write_json(root / "command.json", {
            "command": command, "returncode": result.returncode,
            "runner_sha256": sha(runner), "mock_sha256": sha(binary),
            "scope": "Functional fault injection only; elapsed values are not benchmark evidence.",
        })
        require(result.returncode == (1 if failed else 0), f"{name}: unexpected return code")
        manifest = json.loads((out / "partial-manifest.json").read_text())
        planned = 3 if case == "bool-16" else 6
        require(manifest["planned_request_count"] == planned, f"{name}: lost plan")
        require(len(manifest["requests"]) == planned, f"{name}: missing request states")
        require(manifest["counts"] == {"NotStarted": planned - completed - failed,
                                      "Running": 0, "Completed": completed, "Failed": failed},
                f"{name}: incorrect partial status counts")
        rows = [json.loads(line) for line in (out / "rows.jsonl").read_text().splitlines()]
        require(len(rows) == (0 if name == "persistence" else completed + failed),
                f"{name}: missing execution rows")
        for request in manifest["requests"]:
            if request["status"] == "NotStarted":
                require("receipt" not in request, f"{name}: unstarted request has receipt")
                continue
            if name == "persistence":
                require("receipt" not in request, "persistence: fabricated request receipt")
                continue
            receipt = json.loads((out / request["receipt"]).read_text())
            require(receipt["status"] == request["status"], f"{name}: mismatched receipt status")
            require(sha(out / receipt["stdout_path"]) == receipt["stdout_sha256"],
                    f"{name}: missing or modified raw stdout")
            require(sha(out / receipt["stderr_path"]) == receipt["stderr_sha256"],
                    f"{name}: missing or modified raw stderr")
        if failed:
            require(manifest["status"] == "Failed", f"{name}: incomplete manifest failure")
            failed_request = next(r for r in manifest["requests"] if r["status"] == "Failed")
            if name == "persistence":
                require(manifest["runner_failure"]["kind"] == expected_failure,
                        "persistence: missing runner failure")
                require((out / "runner-failure.json").exists(),
                        "persistence: missing best-effort failure artifact")
            else:
                receipt = json.loads((out / failed_request["receipt"]).read_text())
                require(receipt["failure"]["kind"] == expected_failure, f"{name}: wrong failure kind")
                require(rows[-1]["failure"]["kind"] == expected_failure, f"{name}: lost failure row")
            require(not (out / "summary.json").exists(), f"{name}: misleading success summary")
            if name in ["timeout", "nonzero"]:
                require((out / receipt["stdout_path"]).read_bytes().startswith(b"partial"),
                        f"{name}: partial stdout lost")
                require(bool((out / receipt["stderr_path"]).read_bytes()),
                        f"{name}: partial stderr lost")
            if name == "nonzero":
                require(receipt["returncode"] == 7, "nonzero: lost child exit code")
            if name == "missing":
                require(bool(manifest["identity"]["identity_errors"]),
                        "missing: binary hash failure not recorded")
        else:
            require(manifest["status"] == "Completed", f"{name}: missing completion")
            measures = json.loads((out / "summary.json").read_text())
            require(len(measures) == 1, f"{name}: incorrect summary")
            if name == "compressed":
                require(measures[0]["baseline_wall_ms"] is None and
                        measures[0]["paired_wall_ratio"] is None,
                        "compressed: invented enumeration denominator")
        summary.append({"scenario": name, "status": "Passed",
                        "expected_failure": expected_failure,
                        "planned_requests": planned, "completed": completed, "failed": failed})
    write_json(args.out / "summary.json", {
        "scope": "Functional mock fault controls only, not performance or compiler correctness evidence.",
        "runner_sha256": sha(runner), "probe_sha256": sha(Path(__file__)),
        "scenarios": summary,
    })
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
