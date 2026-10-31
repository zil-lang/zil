#!/usr/bin/env python3
"""Extract exact old/new preflight plus common model; compare externally."""
from pathlib import Path
import hashlib, json, subprocess
ROOT=Path('/workspace/zil')
OUT=Path(__file__).resolve().parent/'review-fanin-oracle'
OUT.mkdir(exist_ok=True)
OLD=subprocess.check_output(['git','rev-parse','801933a9e'],cwd=ROOT,text=True).strip()
NEW=subprocess.check_output(['git','rev-parse','59d8753dc23f0ddfcddbaf5780e3057f3c60b9e9'],cwd=ROOT,text=True).strip()
def read(sha,path):return subprocess.check_output(['git','show',sha+':'+path],cwd=ROOT)
def h(b):return hashlib.sha256(b).hexdigest()
model=read(NEW,'crates/zil-llvm-api/src/model.rs')
assert model==read(OLD,'crates/zil-llvm-api/src/model.rs')
(OUT/'model.rs').write_bytes(model)
bridge='''
pub fn review_dom(s: &[Vec<usize>], p: &[Vec<usize>]) -> (Vec<bool>,Vec<usize>) { dominators(s,p) }
'''
oldbridge='''
pub fn review_walk(reachable: &[bool], idom: &[usize], a: usize, mut b: usize) -> bool {
    if !reachable[b] {return true;} if !reachable[a] {return false;}
    loop {if a==b {return true;} if b==0 {return false;} b=idom[b];}
}
'''
newbridge='''
pub fn review_spans(r: &[bool], d: &[usize]) -> Vec<(usize,usize)> {dominator_intervals(r,d)}
pub fn review_query(r: &[bool], d: &[usize], s: Option<&[(usize,usize)]>, a: usize,b: usize)->bool {dominates(r,d,s,a,b)}
'''
sources={}
for name,sha,tail in [('old',OLD,oldbridge),('new',NEW,newbridge)]:
    raw=read(sha,'crates/zil-llvm-api/src/preflight.rs')
    (OUT/(name+'-exact.rs')).write_bytes(raw)
    (OUT/(name+'.rs')).write_bytes(raw+(bridge+tail).encode())
    sources[name]={'commit':sha,'exact_sha256':h(raw),'wrapped_sha256':h((OUT/(name+'.rs')).read_bytes())}
source=Path(__file__).with_suffix('.rs').read_bytes()
(OUT/'main.rs').write_bytes(source)
compile_cmd=['rustc','--edition=2024','-O',str(OUT/'main.rs'),'-o',str(OUT/'probe')]
comp=subprocess.run(compile_cmd,capture_output=True)
(OUT/'compile.stdout').write_bytes(comp.stdout);(OUT/'compile.stderr').write_bytes(comp.stderr)
assert comp.returncode==0,comp.stderr.decode()
run=subprocess.run([str(OUT/'probe')],capture_output=True)
(OUT/'run.stdout').write_bytes(run.stdout);(OUT/'run.stderr').write_bytes(run.stderr)
assert run.returncode==0,run.stderr.decode()
result=json.loads(run.stdout)
result.update({'old_commit':OLD,'implementation_commit':NEW,'sources':sources,'model_sha256':h(model),'harness_sha256':h(source),'command':compile_cmd,'rustc':subprocess.check_output(['rustc','--version'],text=True).strip(),'scope':'Model/query differential only; old/new full success schedules or first-error text exact; no timing/product gate.'})
(OUT/'result.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result))
