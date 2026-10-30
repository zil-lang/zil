"""Independent schema/2 controls; every compiler/native call is synthetic.

This tests acceptance and refusal of records. It is not performance evidence.
Run with the exact reviewed runner's repository as argv[1] and output as argv[2].
"""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import sys

repo, output = Path(sys.argv[1]), Path(sys.argv[2])
spec = importlib.util.spec_from_file_location(
    "review_runner_controls", repo / "tools/research/compile-performance/test_rust_bench.py"
)
controls = importlib.util.module_from_spec(spec)
spec.loader.exec_module(controls)
bench = controls.bench
case = controls.RustBenchmarkControls()
case.setUp()
results = []
try:
    case.plan.update(
        cache_mode="incremental", query_cache=True, closed_executable=True,
        request_weights=copy.deepcopy(bench.common.REQUESTS),
    )
    case.plan_path.write_text(json.dumps(case.plan))
    original_compiler = controls.RustBenchmarkControls.compiler

    def compiler(argv, stdout, stderr, cpus, timeout):
        result = original_compiler(argv, stdout, stderr, cpus, timeout)
        if "--json" not in argv:
            return result
        source = Path(argv[2]).read_bytes()
        key = hashlib.sha256(source).hexdigest()
        directory = Path(argv[argv.index("--query-cache") + 1])
        directory.mkdir(parents=True, exist_ok=True)
        marker = directory / key
        hit = marker.exists()
        marker.write_bytes(b"synthetic checkpoint")
        report = {
            "status": "Success", "verification_runs": 1, "timings": None,
            "native_codegen": {
                "backend": "llvm-api", "actual_units": 1,
                "closed_executable": "--closed-executable" in argv,
                "internalized_functions": 1,
                "object_cache": {"requested": True, "enabled": True, "hits": int(hit)},
            },
            "query_cache": {
                "requested": True, "state": "hit" if hit else "miss",
                "stored": not hit, "key": key, "reason": None, "write_reason": None,
                "ct_steps": 3, "ct_nodes": 17, "ct_functions": 2,
            },
        }
        stdout.write_text(json.dumps(report))
        return result

    case.compiler = compiler
    preflight, measured = case.measure()
    positive = case.summarize(preflight, measured)
    assert positive["verdict"] == "ExploratoryComplete_NotAProductGate"
    assert positive["rows"] == 48 and positive["query_cache"] and positive["closed_executable"]
    results.append({"control": "valid synthetic combined-mode full sequence", "result": "Pass", "rows": 48})

    def mutation(name, request, update):
        destination = case.root / ("mutated-" + name)
        shutil.copytree(measured, destination)
        (destination / "summary.json").unlink()
        records = bench.rows(destination / "records.jsonl")
        row = next(r for r in records if r["arm"] == "B" and r["request"] == request)
        log = destination / "logs" / row["stdout"]
        report = json.loads(log.read_text())
        update(report)
        log.write_text(json.dumps(report))
        # Keep outer hashes consistent so refusal must reach report validation,
        # rather than relying only on a trivial changed-log/receipt mismatch.
        row["stdout_sha256"] = bench.sha(log)
        row["reported"] = {key: report.get(key) for key in row["reported"]}
        records_path = destination / "records.jsonl"
        records_path.write_text("".join(json.dumps(r) + "\n" for r in records))
        receipt = json.loads((destination / "complete.json").read_text())
        receipt["records_sha256"] = bench.sha(records_path)
        (destination / "complete.json").write_text(json.dumps(receipt))
        try:
            case.summarize(preflight, destination)
        except SystemExit:
            summary = json.loads((destination / "summary.json").read_text())
            assert summary["verdict"] == "IncompleteOrFailed_NoScore"
            assert row["index"] in summary["failures"]
            assert all(key not in summary for key in ("wall", "cpu", "exploratory_paired_block_bootstrap95"))
            results.append({"control": name, "request": request, "result": "Pass: invalid report receives no score"})
        else:
            raise AssertionError("invalid report was scored: " + name)

    for request in bench.common.REQUESTS:
        mutation("wrong-query-state-" + request, request,
                 lambda r, request=request: r["query_cache"].update(state="miss" if request == "unchanged" else "hit"))
    for name, field, value in [
        ("missing-store", "stored", False),
        ("query-not-requested", "requested", False),
        ("query-bypass", "state", "bypass"),
        ("query-write-failure", "write_reason", "write denied"),
        ("query-identity-failure", "reason", "identity mismatch"),
        ("invalid-key", "key", "q" * 64),
        ("negative-usage", "ct_steps", -1),
        ("boolean-usage", "ct_nodes", True),
        ("float-usage", "ct_functions", 2.0),
    ]:
        mutation(name, "cold_process", lambda r, field=field, value=value: r["query_cache"].update({field: value}))
    mutation("missing-query-report", "cold_process", lambda r: r.pop("query_cache"))
    mutation("missing-query-usage", "cold_process", lambda r: r["query_cache"].pop("ct_nodes"))
    mutation("unchanged-unexpected-store", "unchanged", lambda r: r["query_cache"].update(stored=True))
    mutation("closed-mode-ignored", "cold_process", lambda r: r["native_codegen"].update(closed_executable=False))
    mutation("closed-mode-absent", "cold_process", lambda r: r["native_codegen"].pop("closed_executable"))
    mutation("closed-count-absent", "cold_process", lambda r: r["native_codegen"].pop("internalized_functions"))
    mutation("closed-count-boolean", "cold_process", lambda r: r["native_codegen"].update(internalized_functions=True))
    mutation("closed-count-negative", "cold_process", lambda r: r["native_codegen"].update(internalized_functions=-1))
    mutation("no-current-verify", "unchanged", lambda r: r.update(verification_runs=0))
    mutation("double-current-verify", "unchanged", lambda r: r.update(verification_runs=2))
    mutation("string-current-verify", "unchanged", lambda r: r.update(verification_runs="1"))
    mutation("timings-enabled", "cold_process", lambda r: r.update(timings={}))

    for query, closed in [(1, False), (False, 1), ("true", False), (False, "false"), (None, False)]:
        try:
            bench.validate_modes("incremental", query, closed)
        except SystemExit:
            results.append({"control": "malformed-plan-mode", "query_cache": query, "closed_executable": closed, "result": "Pass: rejected"})
        else:
            raise AssertionError("nonboolean frozen mode accepted")
    try:
        bench.validate_modes("cold", True, False)
    except SystemExit:
        results.append({"control": "query-cache-in-cold-mode", "result": "Pass: rejected"})
    else:
        raise AssertionError("query cache accepted in compiler-cache-cold profile")

    output.write_text(json.dumps({
        "scope": "Synthetic validation controls only; no real compiler or native program run and no performance result",
        "implementation_commit": "8bb908a8c7187d0206d75fb44a6d4aaed2abdd32",
        "results": results,
    }, indent=2) + "\n")
    print(len(results), "independent controls Pass; invalid reports never scored")
finally:
    case.tearDown()
