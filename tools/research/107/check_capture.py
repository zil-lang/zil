#!/usr/bin/env python3
"""Bounded normal-mode capture check, separate from the formal cost series."""
import argparse, copy, json
from pathlib import Path
import confirm
import run as prepared
from support import sha, write


def run(args):
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=False)
    frozen=args.prepared.resolve();identity=prepared.verify(frozen)
    # The earlier freeze may be superseded only for stricter seal checks. Its
    # functional reference is retained unchanged and checked before refreezing.
    protocol=json.loads((frozen/"confirmation-protocol.json").read_text())
    assert protocol["build_identity_sha256"]==sha(frozen/"identity.json")
    assert protocol["functional_sha256"]==sha(frozen/"functional.json")
    assert protocol["functional_controls_sha256"]==sha(frozen/"functional-controls.json")
    selected=[("tiny","verify"),("owned-bytes-main-used","build")]
    checks=[];negatives=[]
    for name,mode in selected:
        case=next(c for c in identity["cases"] if c["name"]==name)
        controls={}
        for variant in ["BASE","ON"]:
            tag=str(out/"captures")
            data,command,control=prepared.request(frozen,variant,case,mode,tag)
            folder=out/"captures"/f"{name}-{mode}-{variant}"
            row=dict(case=name,mode=mode,block=-1,warmup=True,variant=variant,position=0,
                process_wall_ns=command["elapsed_ns"],vmhwm_kib=data["vmhwm_kib"],session_drop_ns=data["session_drop_ns"],
                report_path=str(folder/"report.json.gz"),records=[confirm.record_metadata(r) for r in data["records"]])
            confirm.validate_capture(frozen,row,identity,protocol)
            checks.append(dict(case=name,mode=mode,variant=variant,status="Pass",row=row))
            controls[variant]=control
            for kind in ["metadata-time", "semantic-reference"] + (["llvm-reference"] if mode=="build" else []):
                changed=copy.deepcopy(row);fake_protocol=copy.deepcopy(protocol)
                if kind=="metadata-time":changed["records"][0]["api_wall_ns"]+=1
                elif kind=="semantic-reference":fake_protocol["reference_observations"][f"{name}:{mode}"][0]["semantic_sha256"]="0"*64
                else:fake_protocol["reference_observations"][f"{name}:{mode}"][0]["llvm_sha256"]="0"*64
                fixture=out/"counterexamples"/f"{name}-{mode}-{variant}-{kind}.json"
                write(fixture,dict(row=changed,protocol=fake_protocol))
                try:confirm.validate_capture(frozen,changed,identity,fake_protocol)
                except (AssertionError,KeyError,TypeError,ValueError) as error:
                    negatives.append(dict(case=name,mode=mode,variant=variant,mutation=kind,status="Rejected",reason=str(error)))
                else:raise AssertionError("accepted changed capture: "+str(fixture))
            write(out/"progress.json",dict(checks=checks,negative_controls=negatives))
        assert controls["BASE"]==controls["ON"],(name,mode,"normal-mode public observations changed")
    assert len(checks)==4 and len(negatives)==10
    write(out/"receipt.json",dict(status="Pass",scope="normal-mode functional reference and raw capture checks only; no formal performance observations",
        implementation_commit=identity["variants"]["ON"]["commit"],build_identity_sha256=sha(frozen/"identity.json"),
        controller_sha256=sha(Path(confirm.__file__)),fixture_sha256=sha(__file__),reference_protocol_sha256=sha(frozen/"confirmation-protocol.json"),
        compiler_processes=4,full_compile_requests=16,new_native_executions=0,valid_captures=checks,negative_controls=negatives,
        semantics="All public fields and source observations match the preflight reference after only removing times/locations/probes; LLVM bytes match at Build; each Verify runs once"))
    print("capture check:4 complete processes/16 requests;10 changed metadata/reference originals rejected;no formal timing",flush=True)


if __name__=="__main__":
    if not __debug__:raise SystemExit("107 assertions require ordinary Python; -O is forbidden")
    p=argparse.ArgumentParser();p.add_argument("--prepared",type=Path,required=True);p.add_argument("--out",type=Path,required=True)
    run(p.parse_args())
