#!/usr/bin/env python3
"""107 frozen source/plain/count preparation. Formal timing lives in confirm.py."""
from __future__ import annotations
import argparse, hashlib, json, os, platform, shutil, subprocess, sys
from pathlib import Path
from inputs import inputs, ORDINARY, USED_BODY
from probe import NAMES, instrument, replace
from support import BASE, SOURCE_BASE, ENV, sha, write, inventory, invoke, extract, normalize, compact_report, archive_directory

VARIANTS = ["BASE", "OFF", "ON"]
SCRIPT_NAMES = ["run.py", "support.py", "inputs.py", "probe.py", "main.rs", "guards.rs", "guards_extra.rs", "Cargo.toml", "Cargo.lock"]
HISTORICAL_INPUTS = "907b2c3cd03f1ad3c2738d8fe945b4ae7bf284f3"
CONSTANT_PATH = "crates/zil-driver/src/session/construction.rs"
CONSTANT_BEFORE = "pub(super) const ENABLED: bool = true;"
CONSTANT_AFTER = "pub(super) const ENABLED: bool = false;"


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def exact_commit(repo, commit):
    actual = subprocess.check_output(["git", "rev-parse", commit + "^{commit}"], cwd=repo, text=True).strip()
    assert actual == commit and len(commit) == 40
    return commit


def tools_identity():
    names = {name: str(Path(shutil.which(name)).resolve()) for name in ["cargo", "rustc"]}
    names.update(clang=ENV["ZIL_CLANG"], python=str(Path(sys.executable).resolve()))
    return {name: dict(path=path, sha256=sha(path)) for name, path in names.items()}


def request(out, variant, case, mode, tag, counted=False, native=False):
    identity = json.loads((out / "identity.json").read_text())
    folder = out / tag / f"{case['name']}-{mode}-{variant}"
    key = f"{variant}-{'counted' if counted else 'timed'}"
    source = out / "inputs" / case["edit"]
    before = sha(source)
    command = invoke([out/"bin"/key, folder, mode, out/"inputs"/case["root"],
                      out/"edit-manifests"/case["name"], case["capacity"]],
                     out, folder.parent/(folder.name+"-runner"), identity["execution_env"], timeout=180)
    data = json.loads((folder / "report.json").read_text())
    assert data["production_identity"] == identity["variants"][variant]["identity"] and data["schema"] == 17
    assert data["counting"] is counted and data["diagnostic"] is False and data["complete"] is True
    assert data["mode"] == mode and data["capacity"] == case["capacity"] and sha(source) == before
    assert [r["scenario"] for r in data["records"]] == case["scenarios"]
    controls = []
    for row in data["records"]:
        assert row["report"]["status"] == "Success" and row["report"]["verification_runs"] == 1 and row["active_queries"] == 0
        assert len(row["counts"]) == len(NAMES) and all(type(n) is int and n >= 0 for n in row["counts"])
        if not counted:
            assert row["counts"] == [0] * len(NAMES) and row["raw_snapshots"] == []
        control = normalize(row)
        if native and "executable" in row["report"]["artifacts"]:
            prefix = folder / (row["scenario"] + "-native")
            execution = invoke([row["report"]["artifacts"]["executable"]], out, prefix,
                               identity["execution_env"], stdin=case["stdin"].encode(), timeout=20)
            expected = case.get("stdout_by_scenario", {}).get(row["scenario"], case["stdout"])
            assert execution["exit"] == 0 and prefix.with_suffix(".stdout").read_bytes() == expected.encode(), (case["name"],row["scenario"],"native output")
            assert prefix.with_suffix(".stderr").read_bytes() == b""
            control["native"] = dict(exit=0, stdout_sha256=sha(prefix.with_suffix(".stdout")))
        controls.append(control)
    fingerprints = [r["report"]["input_fingerprint"] for r in data["records"]]
    signatures = [[i["signature_fingerprint"] for i in r["report"]["scalar_interfaces"]] for r in data["records"]]
    if case["scenarios"] == USED_BODY:
        assert fingerprints[0] == fingerprints[1] == fingerprints[3] and fingerprints[2] != fingerprints[0]
        assert all(sig == signatures[0] for sig in signatures)
    else:
        assert case["scenarios"] == ORDINARY
        assert fingerprints[0] == fingerprints[1] and len(set(fingerprints)) == 3
        assert signatures[0] == signatures[2] and signatures[2] != signatures[3]
    assert any(e["disposition"] == "reused" for e in data["records"][1]["report"]["query_events"])
    write(folder/"control-hash.json", dict(sha256=hashlib.sha256(canonical(controls)).hexdigest(),
        normalization="support.normalize removes time/probes; preserves all public fields and full stage bytes"))
    compact_report(folder/"report.json")
    return data, command, controls


def key_bytes(key):
    assert set(key) == {"module", "phase", "input", "rules", "target"}
    phase = key["phase"].encode()
    return (key["module"].to_bytes(4, "little") + len(phase).to_bytes(8, "little") + phase
            + b"".join(bytes.fromhex(key[name]) for name in ["input", "rules", "target"]))


def encoded(snapshot):
    data = b"ZIL-DRIVER-RAW-CONSTRUCTION-1\0" + bytes([snapshot["kind"]]) + key_bytes(snapshot["key"])
    data += len(snapshot["dependencies"]).to_bytes(8, "little")
    for dep in snapshot["dependencies"]:
        assert set(dep) == {"key", "kind", "observed"}
        data += key_bytes(dep["key"]) + bytes([dep["kind"]]) + bytes.fromhex(dep["observed"])
    return data


def check_counts(records):
    assert records["BASE"] == records["OFF"], "constant-off differs from original actual work/raw identities"
    histories = {v: {} for v in VARIANTS}
    for index in range(4):
        arms = {v: records[v][index] for v in VARIANTS}
        counts = {v: a["counts"] for v, a in arms.items()}
        snapshots = {v: a["raw_snapshots"] for v, a in arms.items()}
        assert all(counts[v][5:] == counts["BASE"][5:] for v in VARIANTS), "DB/Types/Analyze/Verify/publication work changed"
        assert all(len(snapshots[v]) == counts[v][13] for v in VARIANTS)
        for v in ["BASE", "OFF"]:
            assert counts[v][0] == counts[v][13] and counts[v][2:5] == [0, 0, 0]
        assert counts["ON"][0:2] == [0, 0] and counts["ON"][2] == counts["ON"][13]
        assert counts["ON"][3] == sum(len(encoded(s)) for s in snapshots["ON"])
        assert counts["ON"][4] == sum(len(s["dependencies"]) for s in snapshots["ON"])
        trim = lambda s: {**{k: value for k, value in s.items() if k not in ["semantic", "private_implementation", "bytes", "dependencies"]},
                          "dependencies": [{k:value for k,value in d.items() if k != "observed"} for d in s["dependencies"]]}
        assert [trim(s) for s in snapshots["BASE"]] == [trim(s) for s in snapshots["ON"]]
        for v in VARIANTS:
            for snapshot in snapshots[v]:
                assert snapshot["semantic"] == snapshot["private_implementation"] == snapshot["bytes"]
                assert snapshot["abi"] == snapshot["key"]["target"] and snapshot["source_positions"] == snapshot["key"]["input"]
                if v == "ON":
                    assert hashlib.sha256(encoded(snapshot)).hexdigest() == snapshot["bytes"]
                for dep in snapshot["dependencies"]:
                    assert dep["kind"] == 3 and histories[v][canonical(dep["key"])] == dep["observed"]
                histories[v][canonical(snapshot["key"])] = snapshot["private_implementation"]


def prepare(args):
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    repo = args.repo.resolve()
    for commit in [BASE, SOURCE_BASE, args.candidate, args.harness]: exact_commit(repo, commit)
    assert subprocess.check_output(["git", "diff", BASE, SOURCE_BASE, "--", "Cargo.toml", "Cargo.lock", "crates"], cwd=repo) == b""
    for name in SCRIPT_NAMES:
        frozen = subprocess.check_output(["git", "show", args.harness+":tools/research/107/"+name], cwd=repo)
        assert frozen == Path(__file__).with_name(name).read_bytes(), (name, "controller differs from committed harness")
    variants = {"BASE": dict(commit=BASE, identity=BASE),
                "OFF": dict(commit=args.candidate, identity=args.candidate+";construction=false", constant=False),
                "ON": dict(commit=args.candidate, identity=args.candidate, constant=True)}
    cases = inputs(repo, out)
    tools = tools_identity()
    write(out/"preregistered-inputs.json", dict(variants=variants, cases=cases, harness=args.harness,
        historical_inputs_commit=HISTORICAL_INPUTS, input_sha256=inventory(out/"inputs"), edits_sha256=inventory(out/"edit-manifests"),
        formal_admission="NotRun", formal_arms=["BASE", "ON"], off_timing="NotRun; work-count/function ablation only"))
    for variant, spec in variants.items():
        for kind in ["timed", "counted"]:
            key = f"{variant}-{kind}"
            tree = out / ("source-"+key)
            tree.mkdir()
            extract(repo, spec["commit"], ["Cargo.toml", "Cargo.lock", "crates"], tree)
            if variant == "OFF":
                path = tree/CONSTANT_PATH
                path.write_text(replace(path.read_text(), CONSTANT_BEFORE, CONSTANT_AFTER))
            extract(repo, args.harness, ["tools/research/107"], tree)
            if kind == "counted": instrument(tree)
            write(out/("source-"+key+"-sha256.json"), inventory(tree))
            target = out/("target-"+key)
            flags = "--check-cfg=cfg(zil107_count)" + (" --cfg zil107_count" if kind == "counted" else "")
            env = {"ZIL107_PRODUCTION_IDENTITY": spec["identity"], "RUSTFLAGS": flags, "RUSTC": tools["rustc"]["path"]}
            invoke([tools["cargo"]["path"], "build", "--offline", "--locked", "--release", "--manifest-path", tree/"tools/research/107/Cargo.toml", "--target-dir", target],
                   tree, out/f"logs/build-{key}", env)
            dest = out/"bin"/key
            dest.parent.mkdir(exist_ok=True)
            shutil.copy2(target/"release/pipeline107", dest)
            assert sha(dest) == sha(target/"release/pipeline107")
            write(out/f"logs/prune-{key}.json", dict(path=str(target), retained_binary=str(dest), sha256=sha(dest), scope="reproducible Cargo target only"))
            shutil.rmtree(target)
            print("built", key, flush=True)
    identity = dict(domain="ZIL-107-SOURCE-PLAIN-COUNT-1", variants=variants, harness=args.harness, schema=17, cases=cases,
        scripts={name:sha(Path(__file__).with_name(name)) for name in SCRIPT_NAMES},
        binaries={f"{v}-{k}":sha(out/"bin"/f"{v}-{k}") for v in VARIANTS for k in ["timed", "counted"]},
        input_sha256=inventory(out/"inputs"), edits_sha256=inventory(out/"edit-manifests"), counters=NAMES,
        source_manifests={key:sha(out/("source-"+key+"-sha256.json")) for key in [f"{v}-{k}" for v in VARIANTS for k in ["timed", "counted"]]},
        tools=tools, execution_env=ENV, formal_admission="NotRun", off_timing="NotRun; counted times never supply admission",
        rustc=subprocess.check_output([tools["rustc"]["path"],"-Vv"],text=True),
        cargo=subprocess.check_output([tools["cargo"]["path"],"-V"],text=True),
        clang=subprocess.check_output([tools["clang"]["path"],"--version"],text=True),
        host=platform.platform(), profile="release codegen-units1 debug0 incremental0 jobs2")
    write(out/"identity.json", identity)
    functional(out)


def functional(out):
    identity = json.loads((out/"identity.json").read_text())
    assert not (out/"functional-start.json").exists(), "retain existing attempt; choose a new output"
    write(out/"functional-start.json", dict(identity_sha256=sha(out/"identity.json")))
    guards, cells, count_rows, all_controls = {}, [], [], []
    for variant in VARIANTS:
        folder = out/f"guards-{variant}"
        invoke([out/"bin"/f"{variant}-timed", "guards", folder, out/"guard-common"], out, out/f"logs/guards-{variant}", identity["execution_env"])
        data = json.loads((folder/"report.json").read_text())
        assert data["production_commit"] == identity["variants"][variant]["identity"] and data["schema"] == 17 and data["status"] == "Pass"
        guards[variant] = dict(analysis_n=data["analysis_n"], ct_n=data["ct_n"], requests=data["requests"], records=[normalize(r) for r in data["records"]])
        write(out/"guard-controls.json", guards)
    assert guards["BASE"] == guards["OFF"] == guards["ON"], "guard behavior/budget changed"
    native_executions = 0
    for case in identity["cases"]:
        for mode in case["modes"]:
            controls, records = {}, {}
            for variant in VARIANTS:
                _, _, controls[variant] = request(out,variant,case,mode+"-emit","preflight",native=True)
                data, _, counted = request(out,variant,case,mode+"-emit","counts",counted=True)
                trim = lambda rows: [{k:v for k,v in row.items() if k != "native"} for row in rows]
                assert trim(controls[variant]) == counted, (case["name"],mode,variant,"probe changes public/stage bytes")
                records[variant] = [{k:r[k] for k in ["scenario", "counts", "raw_snapshots"]} for r in data["records"]]
                native_executions += 4 * (mode == "build")
            assert controls["BASE"] == controls["OFF"] == controls["ON"], (case["name"],mode,"semantic/stage/native mismatch")
            check_counts(records)
            all_controls.append(dict(case=case["name"],mode=mode,controls=controls))
            count_rows.append(dict(case=case["name"],mode=mode,records=records))
            cells.append(dict(case=case["name"],mode=mode,status="Pass"))
            write(out/"counts.json",dict(names=NAMES,cells=count_rows))
            write(out/"functional-controls.json",all_controls)
            for variant in VARIANTS:
                for tag in ["preflight", "counts"]:
                    archive_directory(out/tag/f"{case['name']}-{mode}-emit-{variant}")
            write(out/"functional.json",dict(status="Pass" if len(cells)==19 else "Running",scope="all full public outputs, actual private raw ledger and work-count ablation",
                cells=cells, requests=len(cells)*24, native_executions=native_executions,
                guard_requests={v:g["requests"] for v,g in guards.items()},
                budget_boundaries={v:dict(analysis_n=g["analysis_n"],ct_n=g["ct_n"]) for v,g in guards.items()},
                controls_sha256=sha(out/"functional-controls.json"), counts_sha256=sha(out/"counts.json"),
                guard_controls_sha256=sha(out/"guard-controls.json"), formal_admission="NotRun", off_timing="NotRun"))
            print("functional",case["name"],mode,"Pass",flush=True)


def verify(out):
    identity = json.loads((out/"identity.json").read_text())
    assert identity["domain"] == "ZIL-107-SOURCE-PLAIN-COUNT-1" and tools_identity() == identity["tools"] and ENV == identity["execution_env"]
    for name, digest in identity["scripts"].items(): assert sha(Path(__file__).with_name(name)) == digest, name
    assert set(identity["binaries"]) == {f"{v}-{k}" for v in VARIANTS for k in ["timed", "counted"]}
    for key, digest in identity["binaries"].items():
        assert sha(out/"bin"/key) == digest
        manifest = out/("source-"+key+"-sha256.json")
        assert sha(manifest) == identity["source_manifests"][key]
        assert inventory(out/("source-"+key)) == json.loads(manifest.read_text())
    assert inventory(out/"inputs") == identity["input_sha256"] and inventory(out/"edit-manifests") == identity["edits_sha256"]
    functional = json.loads((out/"functional.json").read_text())
    expected = [dict(case=c["name"],mode=m,status="Pass") for c in identity["cases"] for m in c["modes"]]
    assert functional["status"] == "Pass" and functional["cells"] == expected and len(expected) == 19
    assert functional["requests"] == 456 and functional["native_executions"] == 72
    for filename, field in [("counts.json", "counts_sha256"), ("functional-controls.json", "controls_sha256"), ("guard-controls.json", "guard_controls_sha256")]:
        assert sha(out/filename) == functional[field]
    return identity


if __name__ == "__main__":
    if not __debug__: raise SystemExit("107 assertions require ordinary Python; -O is forbidden")
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="operation",required=True)
    q = sub.add_parser("prepare")
    q.add_argument("--repo",type=Path,required=True);q.add_argument("--candidate",required=True);q.add_argument("--harness",required=True);q.add_argument("--out",type=Path,required=True)
    for name in ["functional", "verify"]:
        q = sub.add_parser(name);q.add_argument("--out",type=Path,required=True)
    args = parser.parse_args()
    if args.operation == "prepare": prepare(args)
    else: globals()[args.operation](args.out.resolve())
