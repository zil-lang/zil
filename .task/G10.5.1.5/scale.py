import argparse,importlib.util,json,os,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--base',required=True);p.add_argument('--candidate',required=True);p.add_argument('--out',type=Path,required=True);a=p.parse_args();a.out.mkdir(parents=True)
s=importlib.util.spec_from_file_location('corpus',Path(__file__).parent/'bench/corpus.py');c=importlib.util.module_from_spec(s);s.loader.exec_module(c)
rows=[]
for n in (32,64,96,128):
 source=a.out/f'owned{n}.zil';source.write_text(c.resources(n,'zil',False)+c.io_source('zil'))
 for arm,tool in [('base',a.base),('cand',a.candidate)]:
  for mode in ('check','O2'):
   exe=a.out/f'{arm}-{n}';cmd=[tool,'check' if mode=='check' else 'build',str(source),'--json']
   if mode!='check':cmd+=['--opt','O2','--backend','llvm-api','--clang',os.environ['ZIL_CLANG'],'--codegen-units','8','--codegen-jobs','2','--incremental-o2','--function-merging','after','-o',str(exe)]
   t=time.perf_counter();r=subprocess.run(cmd,capture_output=True,timeout=240);elapsed=time.perf_counter()-t
   (a.out/f'{arm}-{n}-{mode}.stdout').write_bytes(r.stdout);(a.out/f'{arm}-{n}-{mode}.stderr').write_bytes(r.stderr)
   report=json.loads(r.stdout);row={'n':n,'arm':arm,'mode':mode,'wall_ms':elapsed*1000,'exit':r.returncode,'report':report,'observations':[]}
   assert report['status'] in ('Success','ResourceLimit'),report
   if report['status']=='Success':
    assert r.returncode==0 and report['verification_runs']==1
    if mode!='check':
     for data in (bytes([1,0,0,1]),bytes([29,2,0,2]),bytes([251,5,0,3])):
      run=subprocess.run([str(exe)],input=data,capture_output=True,timeout=30);want=c.expected(f'owned{n}',n,'base',data)
      row['observations'].append({'input':data.hex(),'actual':run.stdout.hex(),'expected':want.hex(),'exit':run.returncode,'pass':run.returncode==0 and run.stdout==want and not run.stderr})
      assert row['observations'][-1]['pass'],row
   rows.append(row);(a.out/'summary.json').write_text(json.dumps(rows,indent=2));print(json.dumps({'n':n,'arm':arm,'mode':mode,'status':report['status'],'phase':report['phase'],'wall_ms':elapsed*1000}),flush=True)
