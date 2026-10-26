#!/usr/bin/env python3
"""Source-level mechanism probe, not a candidate admission/timing campaign."""
import argparse
import json
from pathlib import Path
import run


def source(n):
    body = "s=e;"
    for i in reversed(range(n)):
        body = f"if e<0{{return 0;}}else{{let p{i}=e;" + body + f"s+=p{i};" + "}"
    return "fn event(e:i32)->i32{var s=0;" + body + "return s;}\nfn main()->i32=>event(1)-" + str(n + 1) + "\n"


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--prepared", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    out, prepared = args.out.resolve(), args.prepared.resolve()
    out.mkdir(parents=True, exist_ok=False)
    meta = json.loads((prepared / "preregistered-inputs.json").read_text())
    cases = []
    for n in [4, 16, 32, 64]:
        name = f"early-return{n}"
        folder = out / "inputs" / name
        folder.mkdir(parents=True)
        text = source(n)
        (folder / "main.zil").write_text(text)
        case = dict(name=name, root=f"{name}/main.zil", edit=f"{name}/main.zil", modes=["verify"],
                    family="early-return", n=n, capacity=1024, stdin="", stdout="",
                    scenarios=["cold", "unchanged", "private_edit", "public_edit"])
        edits = out / "edit-manifests" / name
        edits.mkdir(parents=True)
        (edits / "scenarios.txt").write_text("\n".join(case["scenarios"]) + "\n")
        for scenario, replacement in [("private_edit", text + "\nfn probe_private()->i32=>17\n"),
                                       ("public_edit", text + "\nfn probe_private()->i32=>17\npub fn probe_public()->i32=>29\n")]:
            (edits / (scenario + ".zil")).write_text(replacement)
            (edits / (scenario + ".tsv")).write_text(str(folder / "main.zil") + "\t" + str(edits / (scenario + ".zil")) + "\n")
        cases.append(case)
    (out / "bin").mkdir()
    (out / "bin/BASE-diag").symlink_to(prepared / "bin/BASE-diag")
    run.write(out / "protocol.json", dict(scope="baseline source mechanism probe; no comparative speed/admission claim",
        baseline=meta["variants"]["BASE"], binary_sha256=run.sha(prepared / "bin/BASE-diag"),
        script_sha256=run.sha(__file__), cases=cases, source_sha256=run.inventory(out / "inputs"),
        edit_sha256=run.inventory(out / "edit-manifests"), stop="fixed four sizes once, retain rejected points"))
    identity = dict(variants=meta["variants"], execution_env=run.ENV)
    rows = []
    for case in cases:
        row, _ = run.request(out, identity, "BASE", case, "verify-emit", "probe", kind="diag")
        rows.append(row)
        run.write(out / "results.json", rows)
        cold = row["records"][0]
        print(case["name"], row["complete"], cold["status"], "transfers", cold["counts"][15], flush=True)


if __name__ == "__main__":
    main()
