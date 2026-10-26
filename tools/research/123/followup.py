#!/usr/bin/env python3
"""Freeze a new candidate while reusing byte-identical frozen baseline builds."""
import argparse
import importlib.util
import json
import shutil
import subprocess
from pathlib import Path
import run


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--prepared", type=Path, required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--controller", required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    old, out, repo = args.prepared.resolve(), args.out.resolve(), args.repo.resolve()
    original = run.verify(old)
    for commit in [args.candidate, args.controller]:
        run.exact(repo, commit)
    assert subprocess.check_output(["git", "show", args.controller + ":tools/research/123/followup.py"], cwd=repo) == Path(__file__).read_bytes()
    out.mkdir(parents=True, exist_ok=False)
    cases = run.inputs(repo, out)
    variants = {"BASE": run.BASE, "ON": args.candidate}
    harness = original["harness"]
    toolset = original["tools"]
    run.write(out / "preregistered-inputs.json", dict(variants=variants, cases=cases, harness=harness,
        input_sha256=run.inventory(out / "inputs"), edits_sha256=run.inventory(out / "edit-manifests"),
        historical_cells=19, historical_endpoints=95, growth_sizes=[16, 32, 64],
        baseline_reuse=dict(source=str(old), identity_sha256=run.sha(old / "identity.json")),
        followup_controller=args.controller, followup_sha256=run.sha(__file__),
        scope="new fixed candidate; original inputs/margin/stopping unchanged; original campaign preserved"))
    for kind in ["plain", "diag"]:
        key = "BASE-" + kind
        shutil.copytree(old / ("source-" + key), out / ("source-" + key))
        shutil.copy2(old / ("source-" + key + "-sha256.json"), out / ("source-" + key + "-sha256.json"))
        (out / "bin").mkdir(exist_ok=True)
        shutil.copy2(old / "bin" / key, out / "bin" / key)
        assert run.sha(out / "bin" / key) == original["binaries"][key]
        assert run.inventory(out / ("source-" + key)) == json.loads((out / ("source-" + key + "-sha256.json")).read_text())
        run.write(out / ("logs/reuse-" + key + ".json"), dict(original_identity_sha256=run.sha(old / "identity.json"),
            binary_sha256=run.sha(out / "bin" / key), source_manifest_sha256=run.sha(out / ("source-" + key + "-sha256.json")),
            original_build_command=json.loads((old / ("logs/build-" + key + ".command.json")).read_text()),
            original_build_stdout_sha256=run.sha(old / ("logs/build-" + key + ".stdout")),
            original_build_stderr_sha256=run.sha(old / ("logs/build-" + key + ".stderr"))))
        print("reused frozen", key, flush=True)
    for kind in ["plain", "diag"]:
        key = "ON-" + kind
        tree = out / ("source-" + key)
        tree.mkdir()
        run.extract(repo, args.candidate, ["Cargo.toml", "Cargo.lock", "crates"], tree)
        run.extract(repo, harness, ["tools/research/123", "tools/research/101/probe.py", "tools/research/089"], tree)
        if kind == "diag":
            spec = importlib.util.spec_from_file_location("probe123", tree / "tools/research/101/probe.py")
            probe = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(probe)
            probe.instrument(tree, diagnostic=True)
            path = tree / "crates/zil-analyze/src/lib.rs"
            path.write_text(probe.at_entry(path.read_text(), "fn transfer(", probe.add(15)))
        run.write(out / ("source-" + key + "-sha256.json"), run.inventory(tree))
        target = out / ("target-" + key)
        flags = "--check-cfg=cfg(zil101_count) --check-cfg=cfg(zil101_diag)" + (" --cfg zil101_diag" if kind == "diag" else "")
        run.invoke([toolset["cargo"]["path"], "build", "--offline", "--locked", "--release", "--manifest-path",
            tree / "tools/research/123/Cargo.toml", "--target-dir", target], tree, out / ("logs/build-" + key),
            {"RUSTFLAGS": flags, "ZIL123_PRODUCTION_IDENTITY": args.candidate, "RUSTC": toolset["rustc"]["path"]})
        shutil.copy2(target / "release/pipeline123", out / "bin" / key)
        run.write(out / ("logs/prune-" + key + ".json"), dict(path=str(target), retained_binary=str(out / "bin" / key), sha256=run.sha(out / "bin" / key)))
        shutil.rmtree(target)
        print("built", key, flush=True)
    identity = original | dict(variants=variants, cases=cases,
        binaries={f"{a}-{k}": run.sha(out / "bin" / f"{a}-{k}") for a in variants for k in ["plain", "diag"]},
        source_manifests={f"{a}-{k}": run.sha(out / f"source-{a}-{k}-sha256.json") for a in variants for k in ["plain", "diag"]},
        input_sha256=run.inventory(out / "inputs"), edits_sha256=run.inventory(out / "edit-manifests"),
        reuse_baseline_identity_sha256=run.sha(old / "identity.json"), followup_controller=args.controller,
        followup_sha256=run.sha(__file__))
    run.write(out / "identity.json", identity)
    run.verify(out)


if __name__ == "__main__":
    if not __debug__:
        raise SystemExit("assertions require ordinary Python")
    main()
