#!/usr/bin/env python3
"""Post-confirmation no-dump diagnosis; probe times never supply admission."""
import argparse
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
    assert subprocess.check_output(["git", "show", args.controller + ":tools/research/123/diagnose.py"], cwd=repo) == Path(__file__).read_bytes()
    identity = run.verify(out)
    refs = json.loads((out / "functional-anchor.json").read_text())["references"]
    assert not (out / "post-diagnostic-protocol.json").exists()
    run.write(out / "post-diagnostic-protocol.json", dict(controller=args.controller, controller_sha256=run.sha(__file__),
        identity_sha256=run.sha(out / "identity.json"), scope="each predeclared Verify cell once in each diag arm, after formal finish",
        source="existing frozen diag binaries; no new production or input candidate", times="instrumented shared environment; not admission",
        stop="one fixed complete pass, any mismatch/failure stops and retains prefix"))
    rows = []
    for case in identity["cases"]:
        assert "verify" in case["modes"]
        for arm in ["BASE", "ON"]:
            before = run.sha(out / "inputs" / case["edit"])
            assert before == identity["input_sha256"][case["edit"]]
            row, control = run.request(out, identity, arm, case, "verify", "post-diagnostic", kind="diag")
            rows.append(row)
            run.write(out / "post-diagnostic.json", dict(scope="counts and instrumented phase attribution only", rows=rows))
            assert row["complete"] and row["controls_sha256"] == refs[case["name"] + ":verify"]["sha256"]
            sources_match(control, case, identity, out)
            assert run.sha(out / "inputs" / case["edit"]) == before
        print("diagnostic", case["name"], "Pass", flush=True)
    assert len(rows) == 44
    run.verify(out)


if __name__ == "__main__":
    if not __debug__:
        raise SystemExit("assertions require ordinary Python")
    main()
