#!/usr/bin/env python3
"""Additional nearest-boundary runtime check; never replaces the frozen matrix."""
from pathlib import Path
import argparse,json,os,random,statistics,subprocess,sys
HERE=Path(__file__).resolve().parent
sys.path.insert(0,str(HERE))
import multimodule as w
p=argparse.ArgumentParser();p.add_argument('--base',required=True);p.add_argument('--candidate',required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True)
os.sched_setaffinity(0,sorted(os.sched_getaffinity(0))[:2]);rust=subprocess.check_output(['rustup','which','rustc'],text=True).strip()
rows=[];builds=[];errors=[];quality=[]
try:
 for case,n in w.SPECS:
  for arm in ['base','cand','rust']:
   root=out/case/arm;root.mkdir(parents=True)
   for name,text in w.files(case,n,'rust' if arm=='rust' else 'zil','public').items():(root/name).write_text(text)
   exe=root/'app'
   if arm=='rust':cmd=[rust,root/'main.rs','--edition=2024','--crate-name','signature_boundary','-A','warnings','-C','opt-level=2','-C','debuginfo=0','-C','overflow-checks=yes','-C','panic=abort','-C','embed-bitcode=no','-C','incremental='+str(root/'incremental'),'-o',exe]
   else:cmd=[a.base if arm=='base' else a.candidate,'build',root/'main.zil','--json','--opt','O2','--backend','llvm-api','--clang',os.environ['ZIL_CLANG'],'--codegen-units','8','--codegen-jobs','2','--incremental-o2','--function-merging','after','-o',exe]
   r,stdout,stderr=w.m.timed(cmd,out/'build-logs'/str(len(builds)),timeout=180)
   assert r['exit']==0 and not r['timeout'],(r,stdout,stderr)
   if arm!='rust':
    report=json.loads(stdout);assert report['status']=='Success' and report['verification_runs']==1;r['report']=report
   r.update(case=case,arm=arm);builds.append(r)
   for data in [bytes([0,0,0,2]),bytes([15,11,0,3])]:
    check=subprocess.run([str(exe)],input=data,capture_output=True,timeout=30)
    assert check.returncode==0 and check.stdout==w.expected(case,n,'public',data) and not check.stderr
  for depth in [0,11]:
   data=bytes([73,depth,255,255]);expected=w.expected(case,n,'public',data)
   for rep in range(-1,9):
    arms=['base','cand','rust'];random.Random(f'{case}/{depth}/{rep}').shuffle(arms)
    for arm in arms:
     r,stdout,stderr=w.m.timed([out/case/arm/'app'],out/'runtime-logs'/str(len(rows)),data,60)
     r.update(case=case,arm=arm,depth=depth,iteration_count=65535,repetition=rep,valid=r['exit']==0 and stdout==expected and not stderr)
     rows.append(r);(out/'rows.jsonl').open('a').write(json.dumps(r)+'\n');assert r['valid'],r
except Exception as error:
 errors.append(repr(error));raise
finally:
 for case,n in w.SPECS:
  for depth in [0,11]:
   vals={arm:[r['wall_ms'] for r in rows if r['case']==case and r['depth']==depth and r['arm']==arm and r['repetition']>=0] for arm in ['base','cand','rust']}
   if all(len(v)==9 for v in vals.values()):
    med={arm:statistics.median(v) for arm,v in vals.items()};quality.append({'case':case,'depth':depth,'median_ms':med,'values_ms':vals,'relative_pass':med['cand']<=1.1*med['base'],'baseline_rust90':med['rust']/med['base']>=.9,'candidate_rust90':med['rust']/med['cand']>=.9})
 passed=not errors and len(quality)==8 and all(q['relative_pass'] and (not q['baseline_rust90'] or q['candidate_rust90']) for q in quality)
 result={'complete':not errors and len(builds)==12 and len(rows)==240,'passed':passed,'builds':builds,'runtime_requests':len(rows),'quality':quality,'errors':errors,'scope':'additional public-signature boundary; 65535 iterations; original workloads and weights unchanged; no data substituted'}
 (out/'summary.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'complete':result['complete'],'passed':passed,'runtime_requests':len(rows),'quality':quality},indent=2))
 if not passed:raise SystemExit('public signature runtime boundary rejected')
