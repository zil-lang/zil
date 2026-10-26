#!/usr/bin/env python3
"""Create fresh isolated inputs from frozen bytes; never repair drifted evidence."""
import argparse
import gzip
import hashlib
import json
import shutil
import subprocess
import tarfile
from pathlib import Path
import run


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--repo", type=Path, required=True)
    p.add_argument("--prepared", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--controller", required=True)
    args = p.parse_args()
    old, out, repo = args.prepared.resolve(), args.out.resolve(), args.repo.resolve()
    run.exact(repo, args.controller)
    assert subprocess.check_output(["git", "show", args.controller + ":tools/research/123/relocate.py"], cwd=repo) == Path(__file__).read_bytes()
    identity = json.loads((old / "identity.json").read_text())
    assert run.tools_identity() == identity["tools"] and run.ENV == identity["execution_env"]
    for name, digest in identity["scripts"].items():
        assert run.sha(Path(__file__).with_name(name)) == digest
    assert run.inventory(old / "edit-manifests") == identity["edits_sha256"]
    # Deliberately do not require the drifted input tree to match and do not
    # overwrite it. Only independently sealed compiler sources/binaries move.
    out.mkdir(parents=True, exist_ok=False)
    for key, digest in identity["binaries"].items():
        assert run.sha(old / "bin" / key) == digest
        manifest = "source-" + key + "-sha256.json"
        assert run.sha(old / manifest) == identity["source_manifests"][key]
        assert run.inventory(old / ("source-" + key)) == json.loads((old / manifest).read_text())
        shutil.copytree(old / ("source-" + key), out / ("source-" + key))
        shutil.copy2(old / manifest, out / manifest)
        (out / "bin").mkdir(exist_ok=True)
        shutil.copy2(old / "bin" / key, out / "bin" / key)
    cases = run.inputs(repo, out)
    assert cases == identity["cases"]
    assert run.inventory(out / "inputs") == identity["input_sha256"]
    # Re-check every historical functional request against frozen base/edit
    # source hashes, not against the subsequently drifted working input file.
    functional = json.loads((old / "functional.json").read_text())
    assert len(functional["cells"]) == 28 and all(c["status"] == "Pass" and c["complete"] is True for c in functional["cells"])
    cases_by_name = {c["name"]: c for c in cases}
    checked = []
    requests = observations = 0
    for row in functional["rows"]:
        case = cases_by_name[row["case"]]
        name = row["case"] + "-" + row["mode"] + "-" + row["arm"] + "-" + row["kind"]
        archive = old / "functional" / (name + ".tar.gz")
        archive_meta = json.loads((old / "functional" / (name + ".archive.json")).read_text())
        assert run.sha(archive) == archive_meta["sha256"]
        with tarfile.open(archive, "r:gz") as tar:
            packed = tar.extractfile(name + "/report.json.gz").read()
        assert hashlib.sha256(packed).hexdigest() == archive_meta["files"]["report.json.gz"][1]
        data = json.loads(gzip.decompress(packed))
        assert data["production_identity"] == identity["variants"][row["arm"]] and data["complete"] is True
        assert [r["scenario"] for r in data["records"]] == case["scenarios"]
        for record in data["records"]:
            assert record["report"]["status"] == "Success" and record["active_queries"] == 0 and record["report"]["verification_runs"] == 1
            expected = dict(identity["input_sha256"])
            scenario = record["scenario"]
            if scenario.endswith("_edit") or scenario == "body_restore":
                expected[case["edit"]] = identity["edits_sha256"][case["name"] + "/" + scenario + ".zil"]
            assert record["sources"]
            for source in record["sources"]:
                relative = str(Path(source["path"]).relative_to(old / "inputs"))
                assert source["sha256"] == expected[relative], (name, scenario, relative)
                observations += 1
            requests += 1
        checked.append(dict(archive=str(archive), sha256=archive_meta["sha256"], report_sha256=hashlib.sha256(packed).hexdigest()))
    assert requests == 448 and len(checked) == 112
    shutil.copy2(old / "functional.json", out / "functional.json")
    run.write(out / "functional-reuse.json", dict(scope="same sealed compiler and source bytes; artifact/source paths retained at original evidence location",
        functional_origin=str(old / "functional.json"), functional_sha256=run.sha(old / "functional.json"),
        requests_rechecked=requests, source_observations_rechecked=observations, archives=checked,
        source_bytes_match=identity["input_sha256"], controller=args.controller, controller_sha256=run.sha(__file__)))
    relocated = identity | dict(edits_sha256=run.inventory(out / "edit-manifests"),
        execution_relocation=dict(origin=str(old), original_identity_sha256=run.sha(old / "identity.json"),
            controller=args.controller, controller_sha256=run.sha(__file__), reason="input drift detected before any exploration process; original left untouched"))
    run.write(out / "identity.json", relocated)
    run.write(out / "preregistered-inputs.json", dict(variants=relocated["variants"], cases=cases,
        input_sha256=relocated["input_sha256"], edits_sha256=relocated["edits_sha256"],
        unchanged_harness=identity["harness"], relocation=relocated["execution_relocation"]))
    run.verify(out)
    print("isolated inputs verified; reused 448 source-checked functional requests", flush=True)


if __name__ == "__main__":
    if not __debug__:
        raise SystemExit("assertions require ordinary Python")
    main()
