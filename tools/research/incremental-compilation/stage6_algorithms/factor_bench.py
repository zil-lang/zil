#!/usr/bin/env python3
"""Record every planned shadow-query request, including partial or failed runs.

The 120 successful measurements frozen at 7963d69ff used the earlier runner.
This revision improves future failure evidence; it does not relabel those runs.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import sys
import time


SMALL_CASES = [f"bool-{n}" for n in [1, 2, 4, 8]] + [
    f"sequence-{n}" for n in [1, 4, 8, 16]
]
LARGE_CASES = [f"bool-{n}" for n in [16, 32, 64, 128]]
ACTIVE_RUN = {}


def utc_now():
    return datetime.now(timezone.utc).isoformat()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def captured_bytes(value):
    if value is None:
        return b""
    return value if isinstance(value, bytes) else value.encode()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def function_map(functions, id_key):
    mapped = {function[id_key]: function for function in functions}
    require(len(mapped) == len(functions), "duplicate function IDs")
    require(bool(mapped), "empty function inventory")
    return mapped


def control_inventory(expected, compressed_only):
    functions = function_map(expected["functions"], "function")
    for function in functions.values():
        require(function["outcome"] == "CompleteFactored", "incomplete control function")
        require(function["check"] in ["ExactOracleRows", "ExpansionDeferred"],
                "unknown control check")
        if function["check"] == "ExpansionDeferred":
            require(compressed_only and function["name"] == "work" and
                    function["canonical_fingerprint"] is None,
                    "unexpected deferred control function")
        else:
            require(isinstance(function["canonical_fingerprint"], str),
                    "exact control lacks fingerprint")
    if compressed_only:
        require(sum(function["check"] == "ExpansionDeferred"
                    for function in functions.values()) == 1,
                "compressed control must contain exactly one deferred work function")
    return functions


def validate(data, arm, expected):
    actual_functions = function_map(data["functions"], "id" if arm == "baseline" else "function")
    require(actual_functions.keys() == expected.keys(), "function inventory differs")
    for function_id, function in actual_functions.items():
        require(function["name"] == expected[function_id]["name"], "function name differs")
    fingerprints = {function_id: function["canonical_fingerprint"]
                    for function_id, function in expected.items()
                    if function["check"] == "ExactOracleRows"}
    if arm == "baseline":
        require(all(f["outcome"] == "Complete" for f in data["functions"]),
                "baseline has an incomplete function")
        actual = {f["id"]: f["canonical_fingerprint"] for f in data["functions"]}
    else:
        require(all(f["outcome"] == "CompleteFactored" for f in data["functions"]),
                "factor has an incomplete function")
        for function_id, function in actual_functions.items():
            require(function["check"] == expected[function_id]["check"],
                    "function check differs")
            if function["check"] == "ExpansionDeferred":
                require(function["canonical_fingerprint"] is None,
                        "deferred function claims a complete fingerprint")
        actual = {f["function"]: f["canonical_fingerprint"]
                  for f in data["functions"] if f["check"] == "ExactOracleRows"}
    require(actual == fingerprints, "complete canonical fingerprints differ")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--factor", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--controls", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--window", required=True)
    parser.add_argument("--timeout-seconds", type=float, default=60)
    parser.add_argument("--blocks", type=int, default=6)
    parser.add_argument("--cases", nargs="+", choices=SMALL_CASES + LARGE_CASES)
    args = parser.parse_args()
    require(bool(args.window), "window identity must not be empty")
    require(args.blocks > 0 and args.timeout_seconds > 0,
            "blocks and timeout must be positive")
    require(not args.out.exists(), "output directory already exists")
    cases = args.cases if args.cases is not None else SMALL_CASES + LARGE_CASES
    require(len(cases) == len(set(cases)), "duplicate cases are not allowed")
    args.out.mkdir(parents=True)

    identity_errors = []

    def identity_sha(path):
        try:
            return sha(path)
        except OSError as error:
            identity_errors.append({"path": str(path), "error": repr(error)})
            return None

    identity = {
        "runner_revision": "recorded-requests-v2",
        "prior_measured_runner_commit": "7963d69ff",
        "window": args.window,
        "factor_sha256": identity_sha(args.factor),
        "baseline_sha256": identity_sha(args.baseline),
        "runner_sha256": identity_sha(Path(__file__)),
        "scope": "Actual frontend + shadow relation query process cost, excluding Analyze/Verify and CLI Lower/LLVM/link. Small comparisons reconstruct identical complete canonical rows; factor emits additional compressed metadata. ExpansionDeferred functions do not run the full oracle. Large cases report compressed-query costs only, with no ratio against failed enumeration.",
        "cases": [case for case in cases if case in SMALL_CASES],
        "compressed_only": [case for case in cases if case in LARGE_CASES],
        "blocks": args.blocks,
        "timeout_seconds": args.timeout_seconds,
        "identity_errors": identity_errors,
    }
    write_json(args.out / "identity.json", identity)
    requests = []
    for case in cases:
        for block in range(args.blocks):
            arms = ["baseline", "factor"] if block % 2 == 0 else ["factor", "baseline"]
            if case in LARGE_CASES:
                arms = ["factor"]
            for arm in arms:
                requests.append({
                    "request_id": f"{case}-{block}-{arm}",
                    "case": case, "block": block, "arm": arm,
                    "status": "NotStarted", "command": None,
                    "planned_binary": str(args.factor if arm == "factor" else args.baseline),
                    "planned_method": None if arm == "factor" else "local:direct:owner:gc",
                    "control_receipt": str(args.controls / f"{case}.factor.command.json"),
                    "expected_output": str(args.controls / f"{case}.factor.json"),
                })
    manifest = {
        "schema": 1, "identity": identity, "created_utc": utc_now(),
        "status": "Running", "planned_request_count": len(requests),
        "sources": {}, "requests": requests,
    }
    ACTIVE_RUN.update({"out": args.out, "manifest": manifest, "request": None})

    def checkpoint():
        manifest["updated_utc"] = utc_now()
        manifest["counts"] = {
            state: sum(request["status"] == state for request in requests)
            for state in ["NotStarted", "Running", "Completed", "Failed"]
        }
        write_json(args.out / "partial-manifest.json", manifest)

    checkpoint()
    prepared = {}
    rows = []
    with (args.out / "rows.jsonl").open("w") as log:
        for request in requests:
            ACTIVE_RUN["request"] = request
            case, block, arm = request["case"], request["block"], request["arm"]
            request["status"] = "Running"
            request["started_utc"] = utc_now()
            checkpoint()
            stdout, stderr, returncode, wall_ns = b"", b"", None, None
            failure = None
            phase = "PreparationError"
            start = None
            try:
                phase = "IdentityError"
                require(identity["runner_sha256"] is not None and
                        identity[f"{arm}_sha256"] is not None,
                        "required executable or runner identity could not be hashed")
                phase = "PreparationError"
                if case not in prepared:
                    receipt = json.loads(Path(request["control_receipt"]).read_text())
                    source = Path(receipt["command"][1])
                    source_sha = sha(source)
                    require(source_sha == receipt["source_sha256"], "source SHA changed")
                    expected_path = Path(request["expected_output"])
                    expected = json.loads(expected_path.read_text())
                    inventory = control_inventory(expected, case in LARGE_CASES)
                    prepared[case] = (source, inventory)
                    manifest["sources"][case] = {
                        "path": str(source), "sha256": source_sha,
                        "control_receipt_sha256": sha(Path(request["control_receipt"])),
                        "expected_output_sha256": sha(expected_path),
                        "function_inventory": [
                            {key: function[key] for key in ["function", "name", "check", "canonical_fingerprint"]}
                            for function in inventory.values()
                        ],
                    }
                source, inventory = prepared[case]
                command = [str(args.factor if arm == "factor" else args.baseline), str(source)]
                if arm == "baseline":
                    command.append("local:direct:owner:gc")
                request["command"] = command
                checkpoint()
                phase = "OSError"
                start = time.perf_counter_ns()
                result = subprocess.run(command, capture_output=True, timeout=args.timeout_seconds)
                wall_ns = time.perf_counter_ns() - start
                stdout, stderr, returncode = result.stdout, result.stderr, result.returncode
                if returncode != 0:
                    failure = {"kind": "NonzeroExit", "returncode": returncode}
            except subprocess.TimeoutExpired as error:
                wall_ns = time.perf_counter_ns() - start
                stdout, stderr = captured_bytes(error.stdout), captured_bytes(error.stderr)
                failure = {"kind": "Timeout", "timeout_seconds": args.timeout_seconds,
                           "error": str(error)}
            except Exception as error:
                if start is not None:
                    wall_ns = time.perf_counter_ns() - start
                failure = {"kind": phase, "error": repr(error)}

            # Persist raw evidence and an execution receipt before interpreting stdout.
            stem = request["request_id"]
            stdout_path = args.out / f"{stem}.stdout"
            stderr_path = args.out / f"{stem}.stderr"
            stdout_path.write_bytes(stdout)
            stderr_path.write_bytes(stderr)
            receipt = {
                "request_id": stem, "case": case, "block": block, "arm": arm,
                "command": request["command"], "wall_ns": wall_ns,
                "started_utc": request["started_utc"], "finished_utc": utc_now(),
                "returncode": returncode, "stdout_path": stdout_path.name,
                "stderr_path": stderr_path.name, "stdout_bytes": len(stdout),
                "stdout_sha256": sha(stdout_path), "stderr_sha256": sha(stderr_path),
                "status": "ExecutionFailed" if failure else "AwaitingValidation",
                "failure": failure,
            }
            receipt_path = args.out / f"{stem}.receipt.json"
            write_json(receipt_path, receipt)
            if failure is None:
                try:
                    data = json.loads(stdout)
                    validate(data, arm, inventory)
                except Exception as error:
                    failure = {"kind": "ProtocolError", "error": repr(error)}
            receipt["status"] = "Failed" if failure else "Completed"
            receipt["failure"] = failure
            write_json(receipt_path, receipt)
            request.update({"status": receipt["status"], "receipt": receipt_path.name,
                            "finished_utc": receipt["finished_utc"], "failure": failure})
            row = {"case": case, "block": block, "arm": arm,
                   "command": request["command"], "wall_ns": wall_ns,
                   "stdout_bytes": len(stdout), "stderr": stderr.decode(errors="replace"),
                   "output_sha256": receipt["stdout_sha256"],
                   "status": receipt["status"], "failure": failure,
                   "receipt": receipt_path.name}
            log.write(json.dumps(row) + "\n")
            log.flush()
            if failure:
                manifest["status"] = "Failed"
                manifest["failure_request"] = stem
                checkpoint()
                print(json.dumps({"failed_request": stem, "failure": failure,
                                  "manifest": str(args.out / "partial-manifest.json")}))
                return 1
            (args.out / f"{stem}.json").write_bytes(stdout)
            rows.append(row)
            checkpoint()
            ACTIVE_RUN["request"] = None

    summary = []
    for case in cases:
        group = [row for row in rows if row["case"] == case]
        fixed = {row["block"]: row for row in group if row["arm"] == "factor"}
        old = {row["block"]: row for row in group if row["arm"] == "baseline"}
        summary.append({
            "case": case,
            "factor_wall_ms": statistics.median(row["wall_ns"] for row in fixed.values()) / 1e6,
            "factor_stdout_bytes": statistics.median(row["stdout_bytes"] for row in fixed.values()),
            "baseline_wall_ms": statistics.median(row["wall_ns"] for row in old.values()) / 1e6 if old else None,
            "paired_wall_ratio": statistics.median(fixed[b]["wall_ns"] / old[b]["wall_ns"] for b in old) if old else None,
        })
    write_json(args.out / "summary.json", summary)
    manifest["status"] = "Completed"
    manifest["finished_utc"] = utc_now()
    checkpoint()
    print(json.dumps(summary, indent=2))
    return 0


def guarded_main():
    try:
        return main()
    except (OSError, KeyboardInterrupt) as error:
        failure = {"kind": "Interrupted" if isinstance(error, KeyboardInterrupt)
                   else "PersistenceError", "error": repr(error)}
        manifest = ACTIVE_RUN.get("manifest")
        recorded = False
        if manifest is not None:
            manifest["status"] = "Failed"
            manifest["runner_failure"] = failure
            manifest["updated_utc"] = utc_now()
            request = ACTIVE_RUN.get("request")
            if request is not None:
                request.update({"status": "Failed", "failure": failure})
                manifest["failure_request"] = request["request_id"]
            manifest["counts"] = {
                state: sum(request["status"] == state for request in manifest["requests"])
                for state in ["NotStarted", "Running", "Completed", "Failed"]
            }
            try:
                write_json(ACTIVE_RUN["out"] / "runner-failure.json", failure)
                write_json(ACTIVE_RUN["out"] / "partial-manifest.json", manifest)
                recorded = True
            except OSError:
                pass
        print(json.dumps({"runner_failure": failure, "failure_manifest_saved": recorded,
                          "boundary": "Persistence failure or interruption may leave a prior Running checkpoint and incomplete request artifacts; no success result is claimed."}),
              file=sys.stderr)
        return 130 if isinstance(error, KeyboardInterrupt) else 1


if __name__ == "__main__":
    raise SystemExit(guarded_main())
