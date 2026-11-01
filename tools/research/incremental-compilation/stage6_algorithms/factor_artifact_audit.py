#!/usr/bin/env python3
"""Revalidate preserved outputs without executing compilers or replacing measurements."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--research", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    require(not args.out.exists(), "output already exists")
    runner = Path(__file__).with_name("factor_bench.py")
    spec = importlib.util.spec_from_file_location("factor_bench", runner)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    benchmark = args.research / "stage6-factor-benchmark"
    controls = args.research / "stage6-factor-frozen"
    saved_rows = [json.loads(line) for line in (benchmark / "rows.jsonl").read_text().splitlines()]
    require(len(saved_rows) == 120, "formal factor window has unexpected request count")
    inventories = {}
    measurements = []
    for row in saved_rows:
        case, block, arm = row["case"], row["block"], row["arm"]
        if case not in inventories:
            expected = json.loads((controls / f"{case}.factor.json").read_text())
            inventories[case] = module.control_inventory(expected, case in module.LARGE_CASES)
        output = benchmark / f"{case}-{block}-{arm}.json"
        require(sha(output) == row["output_sha256"], f"measurement SHA mismatch: {output}")
        module.validate(json.loads(output.read_text()), arm, inventories[case])
        measurements.append({"path": str(output), "sha256": sha(output),
                             "status": "CompleteInventoryAndFingerprintMatch"})

    summary_path = args.research / "stage6-algorithms/factored-consumer-event-support/summary.json"
    consumer = json.loads(summary_path.read_text())
    supports = []
    for row in consumer["rows"]:
        support_path = Path(row["support"])
        support = json.loads(support_path.read_text())
        previous = Path(support["provenance"]["metadata"])
        frozen = controls / previous.name
        require(sha(previous) == support["provenance"]["metadata_sha256"],
                "intermediate support provenance SHA differs")
        old_data, new_data = [json.loads(path.read_text()) for path in [previous, frozen]]
        function_id = support["provenance"]["function"]
        old_function, new_function = [next(function for function in data["functions"]
                                          if function["function"] == function_id)
                                      for data in [old_data, new_data]]
        fields = ["name", "outcome", "check", "canonical_fingerprint", "component_fields",
                  "canonical_fields", "physical_fields", "location_codes"]
        for field in fields:
            require(old_function[field] == new_function[field],
                    f"support metadata differs: {previous.name}:{function_id}:{field}")
        source_receipts = [path.with_name(path.name.replace(".factor.json", ".factor.command.json"))
                           for path in [previous, frozen]]
        source_shas = []
        for path in source_receipts:
            receipt = json.loads(path.read_text())
            source = Path(receipt["command"][1])
            require(sha(source) == receipt["source_sha256"], "support source SHA differs")
            source_shas.append(receipt["source_sha256"])
        require(source_shas[0] == source_shas[1], "intermediate and frozen source bytes differ")
        supports.append({"support": str(support_path), "support_sha256": sha(support_path),
                         "original_metadata": str(previous), "original_metadata_sha256": sha(previous),
                         "frozen_metadata": str(frozen), "frozen_metadata_sha256": sha(frozen),
                         "function": function_id, "compared_fields": fields,
                         "source_sha256": source_shas[0], "status": "IdenticalSupportAndExactRows"})
    require(len(supports) == 10, "unexpected actual-support consumer function count")
    result = {
        "scope": "Post-measurement artifact validation only. No compiler processes or performance reruns. The original formal window retains its original runner/binary identities. Intermediate support provenance is retained and compared with final frozen metadata, not relabeled.",
        "audit_sha256": sha(Path(__file__)), "validation_runner_sha256": sha(runner),
        "original_measurement_identity_sha256": sha(benchmark / "identity.json"),
        "formal_requests_checked": len(measurements), "formal_requests": measurements,
        "support_functions_checked": len(supports), "support_functions": supports,
    }
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"formal_requests_checked": len(measurements),
                      "support_functions_checked": len(supports), "out": str(args.out)}))


if __name__ == "__main__":
    main()
