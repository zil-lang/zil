#!/usr/bin/env python3
"""Adversarial synthetic records only; this never launches a compiler."""
import argparse, copy, gzip, hashlib, json
from pathlib import Path
import confirm
from support import sha, write


def run(args):
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=False)
    prepared=args.prepared.resolve();identity=json.loads((prepared/"identity.json").read_text())
    schedule=json.loads((prepared/"confirmation-schedule.json").read_text())
    cases={c["name"]:c for c in identity["cases"]}
    rows=[]
    for cell in schedule:
        for position,variant in enumerate(cell["order"]):
            records=[dict(scenario=s,api_wall_ns=100+i,counts=[0]*16,active_queries=0,cached_queries=5,
                          observation_ns=10,status="Success",phase="verify" if cell["mode"]=="verify" else "link",verification_runs=1)
                     for i,s in enumerate(cases[cell["case"]]["scenarios"])]
            rows.append({k:cell[k] for k in ["case","mode","block","warmup"]}|dict(variant=variant,position=position,
                process_wall_ns=1000,vmhwm_kib=1,session_drop_ns=10,
                report_path=str(confirm.report_folder(out,cell,position,variant)/"report.json.gz"),records=records))
    confirm.validate_observations(rows,schedule,identity["cases"],out)
    assert not confirm.equal_json([dict(api_wall_ns=100)],[dict(api_wall_ns=100.0)])
    assert not confirm.equal_json([dict(api_wall_ns=1)],[dict(api_wall_ns=True)])
    (out/"synthetic-valid.json.gz").write_bytes(gzip.compress(json.dumps(rows,sort_keys=True).encode(),mtime=0))
    outcomes=[]
    mutations=[
        ("duplicate-r-position1-as-position2",lambda r:r.__setitem__(2,copy.deepcopy(r[1]))),
        ("missing-process",lambda r:r.pop()),
        ("reordered-processes",lambda r:r.__setitem__(slice(0,2),[r[1],r[0]])),
        ("changed-case",lambda r:r[0].__setitem__("case","unregistered")),
        ("changed-mode",lambda r:r[0].__setitem__("mode","parse")),
        ("changed-block",lambda r:r[0].__setitem__("block",1)),
        ("warm-flag-as-integer",lambda r:r[0].__setitem__("warmup",1)),
        ("duplicate-scenario",lambda r:r[0]["records"].__setitem__(1,copy.deepcopy(r[0]["records"][0]))),
        ("missing-scenario",lambda r:r[0]["records"].pop()),
        ("reordered-scenarios",lambda r:r[0]["records"].reverse()),
        ("zero-api",lambda r:r[0]["records"][0].__setitem__("api_wall_ns",0)),
        ("float-api",lambda r:r[0]["records"][0].__setitem__("api_wall_ns",100.0)),
        ("bool-wall",lambda r:r[0].__setitem__("process_wall_ns",True)),
        ("negative-wall",lambda r:r[0].__setitem__("process_wall_ns",-1)),
        ("wall-less-than-contained-api",lambda r:r[0].__setitem__("process_wall_ns",10)),
        ("failed-request",lambda r:r[0]["records"][0].__setitem__("status","ResourceLimit")),
        ("verify-not-run",lambda r:r[0]["records"][0].__setitem__("verification_runs",0)),
        ("live-query",lambda r:r[0]["records"][0].__setitem__("active_queries",1)),
        ("wrong-phase",lambda r:r[0]["records"][0].__setitem__("phase","parse")),
        ("instrumented-timing",lambda r:r[0]["records"][0]["counts"].__setitem__(0,1)),
        ("wrong-report-path",lambda r:r[0].__setitem__("report_path",r[1]["report_path"])),
    ]
    for name,mutate in mutations:
        changed=copy.deepcopy(rows);mutate(changed)
        (out/f"{name}.json.gz").write_bytes(gzip.compress(json.dumps(changed,sort_keys=True).encode(),mtime=0))
        try:confirm.validate_observations(changed,schedule,identity["cases"],out)
        except (AssertionError,KeyError,TypeError,ValueError) as error:
            outcomes.append(dict(case=name,status="Rejected",reason=str(error)))
        else:raise AssertionError(f"accepted corrupt fixture: {name}")
    # Seal controls retain a separately named copy of every changed original.
    proto=dict(scope="synthetic seal fixture; no production or timing observations")
    sample_bytes=json.dumps(rows,sort_keys=True).encode()
    journal_bytes=("\n".join(json.dumps(r,sort_keys=True) for r in rows)+"\n").encode()
    seal_names=["valid-seal","sample-sha","journal-sha","protocol-sha","window-id","failed-finish","wrong-count","journal-float-with-resealed-sha","float-affinity"]
    for name in seal_names:
        folder=out/name;folder.mkdir();write(folder/"confirmation-protocol.json",proto)
        (folder/"confirmation-samples.json").write_bytes(sample_bytes)
        (folder/"confirmation-samples.jsonl").write_bytes(journal_bytes)
        start=dict(protocol_sha256=sha(folder/"confirmation-protocol.json"),window="synthetic",affinity=[2])
        finish=dict(status="Pass",protocol_sha256=start["protocol_sha256"],window="synthetic",processes=4940,
                    requests=19760,warmup_processes=76,retained_processes=4864,
                    samples_sha256=sha(folder/"confirmation-samples.json"),journal_sha256=sha(folder/"confirmation-samples.jsonl"))
        if name=="sample-sha":finish["samples_sha256"]="0"*64
        elif name=="journal-sha":finish["journal_sha256"]="0"*64
        elif name=="protocol-sha":finish["protocol_sha256"]="0"*64
        elif name=="window-id":finish["window"]="different"
        elif name=="failed-finish":finish["status"]="Failed"
        elif name=="wrong-count":finish["processes"]-=1
        elif name=="journal-float-with-resealed-sha":
            changed=copy.deepcopy(rows);changed[0]["process_wall_ns"]=1000.0
            (folder/"confirmation-samples.jsonl").write_text("\n".join(json.dumps(r,sort_keys=True) for r in changed)+"\n")
            finish["journal_sha256"]=sha(folder/"confirmation-samples.jsonl")
        elif name=="float-affinity":start["affinity"]=[2.0]
        write(folder/"confirmation-start.json",start);write(folder/"confirmation-finish.json",finish)
        try:got=confirm.load_sealed_rows(folder)
        except (AssertionError,KeyError,TypeError,ValueError) as error:
            assert name!="valid-seal";outcomes.append(dict(case=name,status="Rejected",reason=str(error)))
        else:
            assert name=="valid-seal" and got==rows
            outcomes.append(dict(case=name,status="AcceptedSyntheticControl"))
    write(out/"receipt.json",dict(status="Pass",scope="synthetic input integrity controls only; not compiler correctness or performance",
        controller_sha256=sha(Path(confirm.__file__)),fixture_script_sha256=sha(__file__),prepared_identity_sha256=sha(prepared/"identity.json"),
        valid_schedule_rows=4940,compiler_processes=0,native_executions=0,outcomes=outcomes))
    print("synthetic controller integrity:",len(mutations)+len(seal_names)-1,"corrupt originals rejected;4940-rowvalid control and seal accepted",flush=True)


if __name__=="__main__":
    p=argparse.ArgumentParser();p.add_argument("--prepared",type=Path,required=True);p.add_argument("--out",type=Path,required=True)
    run(p.parse_args())
