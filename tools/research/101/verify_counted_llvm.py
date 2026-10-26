#!/usr/bin/env python3
"""Recompute the LLVM intersection omitted by the broad no-emit/emit trim."""
import argparse, gzip, hashlib, json, tarfile
from pathlib import Path


def verify(out):
    identity=json.loads((out/"identity.json").read_text());records=[]
    for case in identity["cases"]:
        if "build" not in case["modes"]:continue
        for variant in ["BASE","R"]:
            stem=f"{case['name']}-build-emit-{variant}"
            with tarfile.open(out/"preflight"/f"{stem}.tar.gz","r:gz") as tar:
                report=json.loads(gzip.decompress(tar.extractfile(f"{stem}/report.json.gz").read()))
                counted=json.loads(gzip.decompress((out/"counts"/f"{case['name']}-build-{variant}"/"report.json.gz").read_bytes()))
                for row in report["records"]:
                    member=Path(row["report"]["artifacts"]["llvm"]).relative_to(out/"preflight")
                    plain=tar.extractfile(str(member)).read()
                    observed=next(r for r in counted["records"] if r["scenario"]==row["scenario"])
                    assert plain==Path(observed["report"]["artifacts"]["llvm"]).read_bytes(),(case["name"],variant,row["scenario"])
                    records.append(dict(case=case["name"],variant=variant,scenario=row["scenario"],llvm_sha256=hashlib.sha256(plain).hexdigest()))
    assert len(records)==48
    return dict(status="Pass",scope="all shared LLVM artifacts equal between counted and unprobed full Build controls; no extra compiler/native execution",
                records=records,identity_sha256=hashlib.sha256((out/"identity.json").read_bytes()).hexdigest())


if __name__=="__main__":
    p=argparse.ArgumentParser();p.add_argument("out",type=Path);args=p.parse_args()
    print(json.dumps(verify(args.out.resolve()),indent=2))
