#!/usr/bin/env python3
"""Replay the user's unmodified lab in a private copy, then provide ZIL cases."""
import argparse, hashlib, json, shutil, subprocess, sys
from pathlib import Path
from run import invoke, inventory, write

EXPECTED = "c22948a96d9f9de4a615dcf3314f393a44a3066b2701b6a50fa5b3e15dc6572a"

def sources(out):
    source = out / "production-inputs"; source.mkdir()
    cases = []
    def add(name, text, relationship, expected_scope):
        path = source / (name + ".zil"); path.write_text(text)
        cases.append(dict(case=name, source=str(path), sha256=hashlib.sha256(text.encode()).hexdigest(),
                          relationship=relationship, expected_scope=expected_scope))
    for n in [0,4,8,12,16,20,32,64]:
        text = "resource Cell { n:i32 }\nfn f0(r:Cell) { r.n += 1 }\n"
        for i in range(1,n+1): text += f"fn f{i}(r:Cell) {{ f{i-1}(r); f{i-1}(r) }}\n"
        text += f"fn main()->i32 {{ let r=Cell{{n:0}}; f{n}(r); return 0 }}\n"
        add(f"diamond{n:02}",text,"Lab diamondSource topology; integer runtime/overflow is not executed",
            "Observe actual Types/Analyze/Verify body visits; not lab call replay")
    prefix = "resource Cell { n:i32 }\n"
    add("same-origin-write",prefix+"pub fn update(a:Cell,b:Cell)->i32{a.n+=1;b.n+=2;return a.n} fn main()->i32{let r=Cell{n:0};return update(r,r)-3}",
        "Lab actual alias requirement translated to legal sequential resource writes", "Ordinary Desc writes do not consume responsibility")
    add("consume-before-read",prefix+"fn consume(give r:Cell){} fn main()->i32{let r=Cell{n:1};let a=r;consume(give r);return a.n}",
        "Nearest existing production responsibility-consumption hazard", "Give/Drop, not arbitrary Desc destroy")
    add("read-before-consume",prefix+"fn consume(give r:Cell){} fn main()->i32{let r=Cell{n:1};let a=r;let n=a.n;consume(give r);return n-1}",
        "Order control for consume-before-read", "Finite initialized resource")
    add("recursive-write",prefix+"pub fn run(r:Cell){r.n+=1;run(r)} fn main()->i32{return 0}",
        "Lab recursive stable write; no returning native execution requested", "Observe current ordinary function summary representation")
    add("recursive-before-write",prefix+"pub fn run(r:Cell){run(r);r.n+=1} fn main()->i32{return 0}",
        "Lab unreachable suffix control", "Observe whether return reachability is explicit")
    add("recursive-live-branch",prefix+"pub fn run(r:Cell,b:bool){if b{return}r.n+=1;run(r,b)} fn main()->i32{return 0}",
        "Lab return path exists; production boolean is correlated, not independent nondeterminism", "Different input semantics recorded")
    add("early-return",prefix+"pub fn run(r:Cell,b:bool)->i32{if b{return r.n}r.n+=1;return r.n} fn main()->i32{let r=Cell{n:0};return run(r,true)}",
        "Source early-return CFG and exported contract", "Not destructive return-tag contract")
    add("desc-destroy",prefix+"pub fn run(r:Cell){r.destroy();run(r)} fn main()->i32{return 0}",
        "Lab destroy has no assumed production spelling", "Record actual unsupported/unknown-method result")
    add("desc-drop-entry",prefix+"impl Drop for Cell {fn drop(self:Self){}} pub fn run(r:Cell){r.drop();run(r)} fn main()->i32{return 0}",
        "Attempting to call Drop through Desc must not be mislabeled managed destroy", "Record actual cleanup-entry boundary")
    add("selected-node-field","resource Node { next:Ref<Node>, value:i32 } fn main()->i32{return 0}",
        "Lab selected-place linked-node requirement", "Record recursive resource-field support boundary")
    write(out/"production-cases.json",cases)

def main(args):
    out=args.out.resolve();out.mkdir(parents=True,exist_ok=False)
    assert hashlib.sha256(args.archive.read_bytes()).hexdigest()==EXPECTED
    original=args.lab.resolve(); original_inventory=inventory(original)
    write(out/"original-files.json",original_inventory)
    target=out/"source"; shutil.copytree(original,target)
    assert inventory(target)==original_inventory
    write(out/"identity.json",dict(archive=str(args.archive.resolve()),sha256=EXPECTED,files=len(original_inventory),
        original=str(original),source_copy=str(target),node=subprocess.check_output(["node","--version"],text=True),
        statement="Unmodified supplied 0.6.0 implementations; original archive and original tree are read-only"))
    invoke(["node","--test"],target,out/"logs/tests",timeout=180)
    invoke(["node","scripts/research-v06.mjs"],target,out/"logs/research",timeout=180)
    invoke(["node","cli.mjs","interface-v06","cases/v06/nonreturn-danger.zcase","[0]"],target,out/"logs/danger-interface",timeout=60)
    assert inventory(original)==original_inventory
    changed=[p for p,h in inventory(target).items() if original_inventory.get(p)!=h]
    assert all(p.startswith("evidence/v06/") for p in changed),changed
    data=json.loads((target/"evidence/v06/recursive-analysis.json").read_text())
    hybrid=json.loads((target/"evidence/v06/hybrid-analysis.json").read_text())
    write(out/"result.json",dict(status="Pass",counts=data["counts"],hybrid_queries=hybrid["hybridQueries"],
        ablations=data["ablations"],changed_replay_evidence=changed,
        provenance="Re-execution of prior design lab, not newly invented ZIL algorithms or production results"))
    sources(out)

if __name__=="__main__":
    p=argparse.ArgumentParser();p.add_argument("--lab",type=Path,required=True);p.add_argument("--archive",type=Path,required=True);p.add_argument("--out",type=Path,required=True)
    main(p.parse_args())
