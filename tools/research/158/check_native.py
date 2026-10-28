#!/usr/bin/env python3
"""Real source/LLVM artifacts and target verifier controls, not timing samples."""
import pathlib,json,subprocess,sys,hashlib
root=pathlib.Path(sys.argv[1]);base=pathlib.Path(sys.argv[2]);candidate=pathlib.Path(sys.argv[3]);out=pathlib.Path(sys.argv[4]);out.mkdir(parents=True,exist_ok=False);rows=[];bins={'A':base,'C':candidate}
spec=json.loads((root/'frozen158.json').read_text());by={c['name']:c for c in spec['cases']}
def invoke(binary,src,d,args):
 d.mkdir(parents=True,exist_ok=False);cmd=[str(binary),*args,str(src),'--json'];p=subprocess.run(cmd,capture_output=True,timeout=60);(d/'command.json').write_text(json.dumps(cmd));(d/'stdout').write_bytes(p.stdout);(d/'stderr').write_bytes(p.stderr)
 try:r=json.loads(p.stdout)
 except:r={'status':'InvalidReport'}
 return r
cases=['tree256x8','tree1024x8','tiny','reverse128','cycle1024x8','owned_component','integrated_zui','text_decode']
for name in cases:
 for v,b in bins.items():
  for opt in ['O0','O2']:
   d=out/f'{name}-{v}-{opt}';src=pathlib.Path(by[name]['root']);r=invoke(b,src,d,['build','--opt',opt,'-o',str(d/'program'),'--emit-dir',str(d/'phases')]+(['--entry','value'] if name in ['integrated_zui','text_decode']else []));ok=False;observed=None
   if r['status']=='Success':
    p=subprocess.run([str(d/'program')],input=b'',capture_output=True,timeout=10);(d/'native-stdout').write_bytes(p.stdout);(d/'native-stderr').write_bytes(p.stderr);observed={'exit':p.returncode,'stdout':p.stdout.decode(errors='replace'),'stderr':p.stderr.decode(errors='replace')};ok=p.returncode==0 and r['verification_runs']==1
    if name=='integrated_zui':ok &= p.stdout==b'RRT46\n'
    if name=='text_decode':ok &= p.stdout==b'0\n'
   expected_limit=False # compose-149 already includes the 146 capability improvement
   rows.append({'case':name,'variant':v,'opt':opt,'status':r['status'],'phase':r.get('phase'),'observed':observed,'expected_limit':expected_limit,'pass':(r['status']=='ResourceLimit')if expected_limit else ok});print(rows[-1],flush=True);(out/'native.json').write_text(json.dumps(rows,indent=2))
# Compare identical stages and generated LLVM where both variants succeed.
comparisons=[]
for name in cases:
 for opt in ['O0','O2']:
  a=out/f'{name}-A-{opt}'/'phases';c=out/f'{name}-C-{opt}'/'phases'
  files={str(x.relative_to(a))for x in a.rglob('*')if x.is_file()};other={str(x.relative_to(c))for x in c.rglob('*')if x.is_file()};assert files==other
  for f in sorted(files):comparisons.append({'case':name,'opt':opt,'file':f,'equal':(a/f).read_bytes()==(c/f).read_bytes()})
(out/'artifacts.json').write_text(json.dumps(comparisons,indent=2))
# A reachable 40-link chain exercises the inherited effect graph, ending
# in a real truncate. Invalid old window must reach Verify's borrow rule.
controls=[]
for n in [-1,0,1]:
 chain=''.join(f'fn step{i}(b:ByteBuffer){{'+(f'step{i+1}(b)'if i<39 else f'bytes_truncate(b,{n})')+'}\n'for i in range(40))
 for valid in [False,True]:
  body='let w=bytes_view(b);let before=w.len();step0(b);return i32(before)-1;'if valid else'let w=bytes_view(b);step0(b);return i32(w.len())-1;'
  src=out/f'control-{n}-{valid}.zil';src.write_text(chain+'fn main()->i32{let b=bytes_new();bytes_reserve(b,4);bytes_push(b,1u8);'+body+'}')
  for v,b in bins.items():
   r=invoke(b,src,out/f'control-{n}-{valid}-{v}',['check']);hit=any(x['code']=='E-BORROW-CONFLICT'for x in r.get('diagnostics',[]));ok=r['status']=='Success'if valid else r['status']=='SafeReject'and r['phase']=='verify'and hit
   controls.append({'length':n,'valid':valid,'variant':v,'status':r['status'],'phase':r.get('phase'),'target_rule_hit':hit,'pass':ok})
(out/'controls.json').write_text(json.dumps(controls,indent=2))
summary={'native_builds':len(rows),'actual_native_runs':sum(r['observed']is not None for r in rows),'known_baseline_resource_limits':sum(r['expected_limit']for r in rows),'artifact_comparisons':len(comparisons),'controls':len(controls),'pass':all(r['pass']for r in rows+controls)and all(r['equal']for r in comparisons),'note':'baseline ResourceLimit is capability evidence, never a successful speed sample'};(out/'summary.json').write_text(json.dumps(summary,indent=2));print(summary)
if not summary['pass']:sys.exit(1)
