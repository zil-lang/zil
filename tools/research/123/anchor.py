#!/usr/bin/env python3
"""Fresh complete public/native controls in the isolated execution location."""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path
import run
from confirm import sources_match


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--controller", required=True)
    args = p.parse_args()
    out, repo = args.out.resolve(), Path(__file__).resolve().parents[3]
    run.exact(repo, args.controller)
    assert subprocess.check_output(["git", "show", args.controller + ":tools/research/123/anchor.py"], cwd=repo) == Path(__file__).read_bytes()
    identity = run.verify(out)
    assert not (out / "functional-anchor-start.json").exists()
    run.write(out / "functional-anchor-start.json", dict(controller=args.controller, controller_sha256=run.sha(__file__),
        identity_sha256=run.sha(out / "identity.json"), old_full_stage_functional_sha256=run.sha(out / "functional.json")))
    refs, rows = {}, []
    for case in identity["cases"]:
        for mode in case["modes"]:
            controls = {}
            for arm in ["BASE", "ON"]:
                edit = out / "inputs" / case["edit"]
                before = run.sha(edit)
                assert before == identity["input_sha256"][case["edit"]]
                row, control = run.request(out, identity, arm, case, mode, "functional-anchor", native=mode == "build")
                row["input_guard"] = dict(before=before, after=run.sha(edit), expected=identity["input_sha256"][case["edit"]])
                assert row["complete"] and row["input_guard"]["after"] == before
                controls[arm] = [{k:v for k,v in r.items() if k != "native"} for r in control]
                sources_match(controls[arm], case, identity, out)
                rows.append(row)
                run.write(out / "functional-anchor-rows.json", rows)
            assert controls["BASE"] == controls["ON"], (case["name"], mode, "public/native control mismatch")
            key = case["name"] + ":" + mode
            refs[key] = dict(controls=controls["BASE"], sha256=hashlib.sha256(run.canonical(controls["BASE"])).hexdigest())
            print("anchor", key, "Pass", flush=True)
    assert len(rows) == 56 and len(refs) == 28
    run.verify(out)
    run.write(out / "functional-anchor.json", dict(status="Pass", scope="fresh no-dump full public/LLVM/native controls for relocated paths",
        references=refs, processes=56, requests=224, native_executions=48,
        controller=args.controller, controller_sha256=run.sha(__file__), identity_sha256=run.sha(out / "identity.json"),
        old_full_stage_functional_sha256=run.sha(out / "functional.json"), rows_sha256=run.sha(out / "functional-anchor-rows.json")))


if __name__ == "__main__":
    if not __debug__:
        raise SystemExit("assertions require ordinary Python")
    main()
