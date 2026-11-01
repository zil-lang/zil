#!/usr/bin/env python3
"""Small four-case screen. Independent caches, current Verify, native oracle, raw logs."""
import argparse, hashlib, json, os, signal, shutil, subprocess, sys, time
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT/'tools/research/incremental-compilation'))
from stage6_bench import shapes, generate
from stage6_opaque_bench import generate_variant, expected_values
p=argparse.ArgumentParser()
p.add_argument('--zil',action='append',default=[],help='label=absolute executable')
p.add_argument('--out',type=Path,required=True)
p.add_argument('--reps',type=int,default=1)
p.add_argument('--rust',action='store_true')
p.add_argument('--rustc',default=shutil.which('rustc'))
p.add_argument('--llvm-library',type=Path,required=True)
p.add_argument('--clang',type=Path,required=True)
p.add_argument('--case',action='append')
p.add_argument('--contract',action='append',choices=['default','closed'])
a=p.parse_args();a.out=a.out.resolve();a.out.mkdir(parents=True,exist_ok=False)
library=a.llvm_library.resolve();lib=library.parent
env=dict(os.environ);env['LD_LIBRARY_PATH']=str(lib);env['ZIL_CLANG']=str(a.clang.resolve())
cases=[c for c in shapes() if c['name'] in ['large-even-8x1024','large-skew-8','large-8192']]
cases.append({'name':'opaque-2048-branch','opaque':True})
if a.case:cases=[c for c in cases if c['name'] in a.case]
arms=dict(v.split('=',1) for v in a.zil)
if a.rust:arms['rust']=a.rustc
manifest={'source':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'arms':{},'library':{'path':str(library),'sha256':hashlib.sha256(library.read_bytes()).hexdigest()},'reps':a.reps,'contracts':a.contract or ['default','closed'],'cpus':sorted(os.sched_getaffinity(0)),'cases':[c['name'] for c in cases],'runner_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
manifest['working_diff_sha256']=hashlib.sha256(subprocess.check_output(['git','diff','HEAD','--','crates'],cwd=ROOT)).hexdigest()
manifest['rustc_verbose']=subprocess.check_output([a.rustc,'-vV'],text=True) if a.rust else None
for k,v in arms.items():manifest['arms'][k]={'path':v,'sha256':hashlib.sha256(Path(v).read_bytes()).hexdigest()}
(a.out/'manifest.json').write_text(json.dumps(manifest,indent=2))
rows=[]
def run(cmd,folder,label,runenv,input=None):
 start=time.monotonic_ns();proc=subprocess.Popen(cmd,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=runenv,start_new_session=True,stdin=subprocess.PIPE if input is not None else subprocess.DEVNULL)
 timeout=False
 try:stdout,stderr=proc.communicate(input=input,timeout=240)
 except subprocess.TimeoutExpired:
  timeout=True;os.killpg(proc.pid,signal.SIGKILL);stdout,stderr=proc.communicate()
 elapsed=(time.monotonic_ns()-start)/1e9
 (folder/(label+'.stdout')).write_bytes(stdout);(folder/(label+'.stderr')).write_bytes(stderr)
 return {'command':cmd,'seconds':elapsed,'exit':proc.returncode,'timeout':timeout},stdout,stderr
for case in cases:
 name=case['name'];variants={};expected={}
 for edit in [0,1]:
  variant=a.out/name/f'source-{edit}'
  if case.get('opaque'):
   generate_variant(variant,2048,'branch',edit);expected[edit]=expected_values(2048,'branch',edit)[7]
  else:expected[edit]=generate(case,variant,edit)
  variants[edit]=variant
 for rep in range(a.reps):
  for contract in a.contract or ['default','closed']:
   # Rotate order to avoid systematically favoring the same compiler.
   order=list(arms);shift=rep%len(order);order=order[shift:]+order[:shift]
   for arm in order:
    if arm=='rust' and contract=='closed' and 'default' in manifest['contracts']:continue
    folder=a.out/name/f'{rep}-{contract}-{arm}';folder.mkdir()
    src=folder/'src';src.mkdir()
    for request,edit in [('cold',0),('warm',0),('edit',1)]:
     for file in variants[edit].iterdir():shutil.copyfile(file,src/file.name)
     exe=folder/'program'
     if arm=='rust':
      cmd=[arms[arm],str(src/'main.rs'),'--edition=2024','-A','dead_code','-C','overflow-checks=yes','-C','panic=abort','-C','opt-level=2','-C','codegen-units=8','-C','incremental='+str(folder/'incremental'),'-o',str(exe)];runenv=os.environ.copy()
     else:
      cmd=[arms[arm],'build',str(src/'main.zil'),'--json','--opt','O2','--codegen-units','8','--codegen-jobs','2','--object-cache',str(folder/'objects'),'--semantic-cache',str(folder/'semantic'),'--function-merging','after','--backend','llvm-api','--incremental-o2','-o',str(exe)]
      if contract=='closed':cmd+=['--closed-executable']
      runenv=env
     result,stdout,stderr=run(cmd,folder,request,runenv)
     row=dict(case=name,rep=rep,contract=contract,arm=arm,request=request,**result)
     row['valid']=result['exit']==0 and not result['timeout']
     row['phases_ms']={}
     for line in stderr.decode(errors='replace').splitlines():
      if line.startswith('RESEARCH_TIME '):
       _,phase,ns=line.split();row['phases_ms'][phase]=row['phases_ms'].get(phase,0)+int(ns)/1e6
     if arm!='rust' and row['valid']:
      try:
       report=json.loads(stdout);row['report_seconds']=report['seconds'];row['verification_runs']=report['verification_runs'];row['native_codegen']=report['native_codegen'];row['disk_cache']=report['disk_cache'];row['valid'] &= report['status']=='Success' and report['verification_runs']==1
      except Exception as e:row.update(valid=False,error=repr(e))
     if row['valid']:
      native,nout,nerr=run([str(exe)],folder,request+'.native',os.environ.copy(),bytes([7]) if case.get('opaque') else None)
      row['native_valid']=native['exit']==0 and not nerr and nout==expected[edit].to_bytes(4,'little');row['valid'] &= row['native_valid']
     rows.append(row)
     with (a.out/'rows.jsonl').open('a') as f:f.write(json.dumps(row)+'\n')
     print(name,rep,contract,arm,request,round(row['seconds']*1000,2),row['valid'],json.dumps(row['phases_ms']),flush=True)
(a.out/'summary.json').write_text(json.dumps({'requests':len(rows),'valid':sum(r['valid'] for r in rows)},indent=2))
