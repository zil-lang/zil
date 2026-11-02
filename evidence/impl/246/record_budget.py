"""Run with `source /workspace/.zil-env/env.sh; python3 .../record_budget.py`.

Links only explicitly named, read-only rlibs. The baseline directory is the
root-owned health build; its source identity is confirmed in the dispatch.
"""
import hashlib
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[3]
OUT = pathlib.Path(__file__).resolve().parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


result = {
    "scope": "Standalone source-to-Expand/Types cost evidence; no native callback execution",
    "baseline_source_identity": "81b7ae0c9ce5e7245613ed536b4702c419fe99bd",
    "baseline_build": "Root-owned 0140/77b health build, Rust production source unchanged from 81b7ae0c; root confirmed read-only artifact provenance",
    "candidate_source_commit": subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
    ).strip(),
    "source": "evidence/impl/246/copy_enum_helper.zil",
    "source_sha256": sha(OUT / "copy_enum_helper.zil"),
    "probe": "evidence/impl/246/budget_probe.rs",
    "probe_sha256": sha(OUT / "budget_probe.rs"),
    "runs": [],
}
for label, target in [("baseline", "target"), ("candidate", "target-246")]:
    deps = pathlib.Path("/workspace/.zil-env") / target / "debug/deps"
    files = {
        "zil_base": "libzil_base-f8787e05901477bc.rlib",
        "zil_ir": "libzil_ir-fcef0ed96d262ed9.rlib",
        "zil_expand": "libzil_expand-99d8eb937b73e1b3.rlib",
        "zil_types": "libzil_types-571f611b402b80ed.rlib",
        "zil_syntax": "libzil_syntax-5e151b39b2246141.rlib",
    }
    libs = {
        name: {"path": str(deps / filename), "sha256": sha(deps / filename)}
        for name, filename in files.items()
    }
    binary = "/tmp/zil246-" + label + "-budget"
    command = [
        "/workspace/.zil-env/cargo/bin/rustc", "--edition=2024",
        "evidence/impl/246/budget_probe.rs", "-L", "dependency=" + str(deps),
    ]
    for name in ["zil_base", "zil_expand", "zil_types", "zil_syntax"]:
        command += ["--extern", name + "=" + libs[name]["path"]]
    command += ["-o", binary]
    compiled = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
    assert compiled.returncode == 0, compiled.stderr
    run = subprocess.run([binary], cwd=ROOT, text=True, capture_output=True)
    stdout = OUT / ("budget-" + label + ".stdout")
    stderr = OUT / ("budget-" + label + ".stderr")
    stdout.write_text(run.stdout)
    stderr.write_text(run.stderr)
    result["runs"].append({
        "label": label, "rlibs": libs, "compile_command": command,
        "compile_exit": compiled.returncode, "execute_command": [binary],
        "exit_code": run.returncode,
        "stdout": str(stdout.relative_to(ROOT)), "stderr": str(stderr.relative_to(ROOT)),
        "results": [json.loads(line) for line in run.stdout.splitlines()],
    })
    assert run.returncode == 0, run.stderr
(OUT / "budget-comparison.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps({r["label"]: r["results"] for r in result["runs"]}))
