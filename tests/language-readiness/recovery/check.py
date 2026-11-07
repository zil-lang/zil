#!/usr/bin/env python3
"""Collect real CLI observations; pending/probe outcomes are not safety passes."""

import argparse
import collections
import hashlib
import json
from pathlib import Path
import re
import subprocess


BASELINE = "7f039d16dd65c3608dc3a909bc9628f3db9a975a"
NORMATIVE = [
    "_zil_design.md", "spec/grammar.ebnf", "spec/attributes.json",
    "validation/product-gates.json", "policy/dependencies.json",
    "spec/interfaces.json", "spec/diagnostics.json",
]
INTENT = {
    "borrow-parameter-return": "Existing parameter-origin shared address return.",
    "borrow-selected-parameter": "Existing return summary must include both actual input roots.",
    "ordinary-mutex-poll-names": "Ordinary struct/function names confer no locking, frame or thread semantics.",
    "static-trait": "Existing nominal static trait, concrete Self impl and ordinary generic consumer.",
    "scoped-callback": "Existing same-thread Direct callback scope; unsafe external role promises are not executed.",
    "scalar-function-pointer": "Existing pure Copy function pointer; address contracts are a separate missing combination.",
    "borrow-local-return": "Local storage must not escape through an address result.",
    "borrow-overlapping-call": "Shared/exclusive windows to the same actual object conflict at the call.",
    "trait-missing-impl": "A matching method name or data shape cannot replace nominal impl evidence.",
    "trait-empty-transfer": "An empty user trait/impl cannot manufacture Transfer capability.",
    "returned-captured-callback": "c_callback cannot expose a returned closure environment without stable storage and close obligations.",
    "life-parameter-return": "Legal finite life binder and matching input/output relation still need implementation.",
    "borrow-function-pointer": "Indirect borrowed-address parameter contracts are absent even before adding life syntax.",
    "life-higher-rank": "Higher-ranked per-call life binding requires callable signature and source checking.",
    "trait-type-argument": "Concrete trait type argument and concrete Self implementation need full nominal instance identity.",
    "trait-associated-type": "Associated declaration/projection remains a required unimplemented trait profile.",
    "trait-default-method": "A default trait body is a required future ordinary trait combination.",
    "trait-generic-impl": "Generic impl must establish its premises and preserve actual Self instance identity.",
    "suspend-contract": "Normative suspend effect cannot be implemented by parsing a contract or naming a poll function.",
    "spawn-contract": "Normative spawn/resume bound needs actual thread/transfer semantics.",
    "transfer-bound": "Built-in structural Transfer qualification needs an implementation, not an empty nominal trait.",
    "share-bound": "Built-in structural Share qualification needs actual simultaneous-access and dependency rules.",
    "undeclared-life": "Observed undeclared life spelling is not a supported life binder or proof.",
    "mismatched-undeclared-life": "Observed ignored life names must not imply a declared result-source restriction.",
    "receiver-undeclared-life": "Observed Self receiver life spelling is not a checked life relation.",
}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git(repo, *arguments):
    return subprocess.check_output(["git", *arguments], cwd=repo, text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--zil", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[3]
    binary = args.zil.resolve(strict=True)
    inventory = repo / "crates/zil-ir/src/wire.rs"
    decoder = inventory.read_text()
    instruction_decoder = decoder.split("fn instruction(&mut self) -> WireResult<Instruction> {", 1)[1].split("fn locals(", 1)[0]
    tags = sorted({int(tag) for tag in re.findall(r"^            (\d+) =>", instruction_decoder, re.M)})
    observations = []
    regressions = []
    for source in sorted((repo / "tests/language-readiness/recovery").glob("*/*.zil")):
        relative = source.relative_to(repo).as_posix()
        command = [str(binary), "check", relative, "--json"]
        process = subprocess.run(command, cwd=repo, text=True, capture_output=True, timeout=30)
        actual = json.loads(process.stdout)
        role = source.parent.name
        observation = {
            "source": relative,
            "sha256": sha(source),
            "role": role,
            "intent": INTENT[source.stem],
            "command": command,
            "exit_code": process.returncode,
            "status": actual["status"],
            "phase": actual["phase"],
            "diagnostics": actual["diagnostics"],
            "verification_runs": actual["verification_runs"],
            "executed_phases": [event["phase"] for event in actual["query_events"] if event["disposition"] == "executed"],
            "result_origins": [entry for interface in (actual.get("scalar_interfaces") or []) for entry in (interface["result_origins"] or [])],
            "stdout_sha256": hashlib.sha256(process.stdout.encode()).hexdigest(),
            "stderr": process.stderr,
            "native_executed": False,
        }
        if role == "controls" and (actual["status"] != "Success" or actual["verification_runs"] != 1):
            regressions.append(relative)
        if role == "rejects" and actual["status"] != "SafeReject":
            regressions.append(relative)
        observations.append(observation)
    if not observations:
        raise SystemExit("No sources selected; no result is valid.")
    result = {
        "schema_version": 1,
        "kind": "source-to-Verify observations and source inventory; not native/concurrency/full-language acceptance",
        "source_baseline": BASELINE,
        "collector_checkout": git(repo, "rev-parse", "HEAD"),
        "baseline_crates_tree": git(repo, "rev-parse", BASELINE + ":crates"),
        "collector_crates_tree": git(repo, "rev-parse", "HEAD:crates"),
        "compiler": {"path": str(binary), "sha256": sha(binary), "provenance": "Integrator-built recovery baseline binary; runtime LD_LIBRARY_PATH supplied by /workspace/.zil-env/env.sh."},
        "normative_sha256": {path: sha(repo / path) for path in NORMATIVE},
        "replay": "source /workspace/.zil-env/env.sh && python3 tests/language-readiness/recovery/check.py --zil /workspace/.zil-env/target-recovery-root/debug/zil --output validation/results/language-readiness/recovery-capabilities.json",
        "inventory": {
            "runtime_instruction_decoder": "crates/zil-ir/src/wire.rs",
            "sha256": sha(inventory),
            "source_sha256": {path: sha(repo / path) for path in ["crates/zil-ir/src/lib.rs", "crates/zil-lower/src/lib.rs", "crates/zil-expand/src/lib.rs", "crates/zil-expand/src/generics.rs", "crates/zil-expand/src/static_traits.rs", "crates/zil-expand/src/generic_trait_bounds.rs", "crates/zil-types/src/functions.rs", "crates/zil-verify/src/borrows.rs"]},
            "decoded_instruction_tags": tags,
            "old_public_tags_121_122_123_present": {str(tag): tag in tags for tag in [121, 122, 123]},
            "sabi_mutex_thread_implementation": "Absent from current production Type/InstructionKind, decoder, lowering and target primitive inventory; ordinary identifiers are not implementation.",
            "source_anchors": [
                "crates/zil-expand/src/lib.rs:type_ref erases reference lifetime field",
                "crates/zil-expand/src/generics.rs:Engine::new rejects lifetime/const binders",
                "crates/zil-expand/src/static_traits.rs:collect_trait_declaration and collect_static_traits",
                "crates/zil-expand/src/generic_trait_bounds.rs:Index::declaration",
                "crates/zil-types/src/lending.rs:sources",
                "crates/zil-verify/src/borrows.rs:independent parameter-root reconstruction",
                "crates/zil-ir/src/lib.rs:Type and InstructionKind",
                "crates/zil-ir/src/wire.rs:instruction decoder",
            ],
        },
        "collection_status": "Complete",
        "selected": len(observations),
        "observed_counts": dict(collections.Counter(item["status"] for item in observations)),
        "control_or_safety_regressions": regressions,
        "observations": observations,
        "limits": [
            "pending cases remain legal targets and are not permanent reject tests",
            "probe Success does not validate the ignored lifetime annotations",
            "no native execution, weak-memory model, target synchronization, full-suite or product measurement",
            "source checks cannot prove unsafe external declaration promises",
            "old unrecovered 301-312 objects and their results are not inherited",
        ],
        "initial_infrastructure_attempt": {
            "command": "/workspace/.zil-env/target-recovery-root/debug/zil --help",
            "exit_code": 127,
            "stderr": "error while loading shared libraries: libLLVM.so.23.1: cannot open shared object file: No such file or directory",
            "resolution": "Source existing /workspace/.zil-env/env.sh; no toolchain or production modification.",
            "language_verdict": None,
        },
        "initial_collector_failure": {
            "command": "python3 tests/language-readiness/recovery/check.py --zil /workspace/.zil-env/target-recovery-root/debug/zil --output validation/results/language-readiness/recovery-capabilities.json",
            "exit_code": 1,
            "error": "TypeError: 'NoneType' object is not iterable at scalar_interfaces/result_origins extraction",
            "resolution": "Treat absent optional interface/origin lists as empty; no compiler or source change.",
            "language_verdict": None,
        },
    }
    output = args.output if args.output.is_absolute() else repo / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"selected": len(observations), "statuses": result["observed_counts"], "control_or_safety_regressions": regressions}))
    return bool(regressions)


if __name__ == "__main__":
    raise SystemExit(main())
