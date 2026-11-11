#!/usr/bin/env python3
"""Separate mechanism lane; never silently add these cases to old weights."""
from pathlib import Path
import argparse,importlib.util,json,os,random,shutil,statistics,subprocess,time,sys
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('prior_matrix',HERE/'bench/measure.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
c=m.corpus
SPECS=[('dispatch8',8),('pipeline8',8),('dispatch16',16),('pipeline16',16)]
SNAPS=[('cold','base'),('warm','base'),('private','private'),('edit-warm','private'),('public','public'),('restore','base')]

def files(kind,n,lang,snapshot):
    result={};isrust=lang=='rust';ext='rs' if isrust else 'zil'
    for i in range(n):
        body=c.resources(4,lang,snapshot!='base' and i==0).replace('fn run(x:i64,d:i32)->i64','pub fn handle(x:i64,d:i32)->i64')
        if isrust:
            body=body.replace('data.push(byte);','''if data.len()==data.capacity(){
                let wanted=if data.capacity()<8{8}else{match data.capacity().checked_mul(2){Some(v) if v<=isize::MAX as usize=>v,_=>return -1}};
                if data.try_reserve_exact(wanted-data.len()).is_err(){return -1;}
            } data.push(byte);''')
        if snapshot=='public' and i==0:
            body=body.replace('pub fn handle(x:i64,d:i32)->i64','pub fn handle(x:i64,d:i32,extra:i64)->i64')
            assert body.count('return y; }')==1;body=body.replace('return y; }','return (y+extra)%65521; }')
        result[f'worker{i}.{ext}']=body
    rows=[f'mod worker{i};\n' if isrust else f'import worker{i} as w{i}\n' for i in range(n)]
    rows.append('fn run(x:i64,d:i32)->i64 { ')
    def call(i,arg):return (f'worker{i}::handle' if isrust else f'w{i}.handle')+'('+arg+',d'+(',3' if snapshot=='public' and i==0 else '')+')'
    if kind.startswith('dispatch'):
        for i in range(n):rows.append(f'if x%{n}=={i} {{return {call(i,"x")};}} ')
        rows.append('return 0; }\n')
    else:
        rows.append(('let mut' if isrust else 'var')+' y=x; ')
        for i in range(n):rows.append('y='+call(i,f'(y+{17*i})%65521')+f'; y=(y+{i})%65521; ')
        rows.append('return y; }\n')
    result['main.'+ext]=''.join(rows)+c.io_source(lang)
    return result

def expected(kind,n,snapshot,data):
    x,depth,hi,lo=data;d=depth%12+1
    def worker(i,x):
        y=c.run_model('owned4',4,x,d,snapshot!='base' and i==0)
        return (y+3)%65521 if snapshot=='public' and i==0 else y
    for k in range(hi*256+lo):
        y=(x+k)%65521
        if kind.startswith('dispatch'):y=worker(y%n,y)
        else:
            for i in range(n):y=(worker(i,(y+17*i)%65521)+i)%65521
        x=(y+k)%65521
    return bytes([x%256,x//256])

def main():
    p=argparse.ArgumentParser();p.add_argument('--base',type=Path,required=True);p.add_argument('--candidate',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--rounds',type=int,default=5);a=p.parse_args()
    out=a.out.resolve();out.mkdir(parents=True);os.sched_setaffinity(0,sorted(os.sched_getaffinity(0))[:2])
    tools={'base':a.base.resolve(),'cand':a.candidate.resolve(),'rust':Path(subprocess.check_output(['rustup','which','rustc'],text=True).strip())}
    rows=[];runs=[];failures=[]
    inputs=[bytes([i,3,0,1]) for i in range(16)]+[bytes([73,11,0,3]),bytes([0,0,0,2])]
    manifest={'specs':SPECS,'rounds':a.rounds,'modes':['check','O0','O2'],'history':SNAPS,'inputs':[x.hex() for x in inputs],'tools':{k:m.sha(v) for k,v in tools.items()},'rust':'Vec with explicit fallible reserve and checked doubling; no identical allocator claim','scope':'multi-handler mechanisms, not a complete upstream app port or part of old weighted aggregate','baseline':'099d0af969a51260355437603b77d3f9030d83bc'}
    for kind,n in SPECS:
        for snapshot in ['base','private','public']:
         for lang in ['zil','rust']:
          folder=out/'corpus'/kind/snapshot/lang;folder.mkdir(parents=True)
          for name,text in files(kind,n,lang,snapshot).items():(folder/name).write_text(text)
    manifest['source_hashes']={str(p.relative_to(out)):m.sha(p) for p in (out/'corpus').rglob('*') if p.is_file()}
    m.dump(out/'manifest.json',manifest)
    try:
      for kind,n in SPECS:
       for mode in ['check','O0','O2']:
        arms=['base','cand','rust'] if mode=='O2' else ['base','cand']
        for block in range(-1,a.rounds):
         for request,snapshot in SNAPS:
          order=arms[:];random.Random(f'{kind}/{mode}/{block}/{request}').shuffle(order)
          for arm in order:
           work=out/'work'/kind/mode/str(block)/arm;work.mkdir(parents=True,exist_ok=True);lang='rust' if arm=='rust' else 'zil'
           for name,text in files(kind,n,lang,snapshot).items():
            path=work/name
            if not path.exists() or path.read_text()!=text:path.write_text(text)
           exe=work/'app';exe.unlink(missing_ok=True)
           if arm=='rust':argv=[tools[arm],work/'main.rs','--edition=2024','--crate-name','multimodule_handlers','-A','warnings','-C','opt-level=2','-C','debuginfo=0','-C','overflow-checks=yes','-C','panic=abort','-C','embed-bitcode=no','-C','incremental='+str(work/'incremental'),'-o',exe]
           else:
            argv=[tools[arm],'build',work/'main.zil','--json','--opt',mode,'--backend','llvm-api','--clang',os.environ['ZIL_CLANG'],'--codegen-units','8','--codegen-jobs','2','--semantic-cache',work/'semantic','--object-cache',work/'objects','-o',exe]
            if mode=='O2':argv+=['--incremental-o2','--function-merging','before-and-after' if arm=='cand' else 'after']
           if mode=='check':argv=[tools[arm],'check',work/'main.zil','--json','--semantic-cache',work/'semantic']
           row,stdout,stderr=m.timed(argv,out/'logs'/str(len(rows)),timeout=180)
           row.update(case=kind,n=n,mode=mode,block=block,request=request,snapshot=snapshot,arm=arm,warmup=block<0,valid=False,native_checks=[])
           if row['exit']!=0 or row['timeout']:
            failures.append({'row':row,'stdout':stdout.decode(errors='replace'),'stderr':stderr.decode(errors='replace')});rows.append(row);raise RuntimeError(f'{kind}/{mode}/{request}/{arm} compilation failed')
           if arm!='rust':
            report=json.loads(stdout);assert report['status']=='Success' and report['verification_runs']==1,report
            row['verification_runs']=1;row['native_codegen']=report['native_codegen']
           for data in (inputs if mode!='check' else []):
            r=subprocess.run([str(exe)],input=data,capture_output=True,timeout=30);want=expected(kind,n,snapshot,data)
            assert r.returncode==0 and r.stdout==want and not r.stderr,(kind,snapshot,arm,data.hex(),r.stdout.hex(),want.hex(),r.stderr)
            row['native_checks'].append({'input':data.hex(),'expected':want.hex(),'actual':r.stdout.hex()})
           row['text_sha256']=m.text_hash(exe) if mode!='check' else None;row['valid']=True;rows.append(row)
           (out/'rows.jsonl').open('a').write(json.dumps(row)+'\n')
           if block==0 and mode=='O2' and request in ('cold','private'):
            target=out/'quality'/kind/snapshot/arm;target.mkdir(parents=True);shutil.copy2(exe,target/'app')
          print(json.dumps({'case':kind,'mode':mode,'block':block,'request':request,'requests':len(rows)}),flush=True)
         shutil.rmtree(out/'work'/kind/mode/str(block))
      for kind,n in SPECS:
       for snapshot in ['base','private']:
        root=out/'quality'/kind/snapshot;iterations=256
        while True:
            data=bytes([73,3,iterations//256,iterations%256]);t=time.perf_counter();r=subprocess.run([str(root/'base/app')],input=data,capture_output=True,timeout=30);elapsed=time.perf_counter()-t;assert r.returncode==0
            if elapsed>.15 or iterations>=16384:break
            iterations*=2
        for depth in [0,11]:
         data=bytes([73,depth,iterations//256,iterations%256]);want=expected(kind,n,snapshot,data)
         for rep in range(-1,7):
          arms=['base','cand','rust'];random.Random(f'{kind}/{snapshot}/{depth}/{rep}').shuffle(arms)
          for arm in arms:
           row,stdout,stderr=m.timed([root/arm/'app'],out/'runtime-logs'/str(len(runs)),data,60)
           row.update(case=kind,snapshot=snapshot,depth=depth,repetition=rep,arm=arm,iterations=iterations,valid=row['exit']==0 and stdout==want and not stderr)
           runs.append(row);(out/'runtime.jsonl').open('a').write(json.dumps(row)+'\n');assert row['valid'],row
    except Exception as error:
      failures.append({'error':repr(error)});raise
    finally:
      cells=[]
      for kind,n in SPECS:
       for mode in ['check','O0','O2']:
        for request,_ in SNAPS:
         cell={'case':kind,'mode':mode,'request':request}
         for arm in ['base','cand','rust']:
          values=[r['wall_ms'] for r in rows if r['valid'] and (r['case'],r['mode'],r['request'],r['arm'])==(kind,mode,request,arm) and not r['warmup']]
          if values:cell[arm]={'n':len(values),'mean_ms':statistics.mean(values),'median_ms':statistics.median(values),'values_ms':values}
         cells.append(cell)
      expected_requests=len(SPECS)*(a.rounds+1)*len(SNAPS)*7
      complete=not failures and len(rows)==expected_requests
      quality=[]
      for kind,n in SPECS:
       for snapshot in ['base','private']:
        for depth in [0,11]:
         values={arm:[r['wall_ms'] for r in runs if (r['case'],r['snapshot'],r['depth'],r['arm'])==(kind,snapshot,depth,arm) and r['repetition']>=0] for arm in ['base','cand','rust']}
         if all(len(v)==7 for v in values.values()):
          med={a:statistics.median(v) for a,v in values.items()};quality.append({'case':kind,'snapshot':snapshot,'depth':depth,'median_ms':med,'values_ms':values,'delta_pass':med['cand']<=1.1*med['base'],'rust90_pass':med['rust']/med['cand']>=.9})
      result={'complete':complete,'compile_requests':len(rows),'formal_requests':sum(not r['warmup'] for r in rows),'native_output_checks':sum(len(r['native_checks']) for r in rows),'runtime_requests':len(runs),'cells':cells,'quality':quality,'quality_complete':len(quality)==16,'no_new_runtime_regression':len(quality)==16 and all(q['delta_pass'] for q in quality),'rust90_all':len(quality)==16 and all(q['rust90_pass'] for q in quality),'failures':failures,'weighted_in_original_protocol':False}
      m.dump(out/'summary.json',result);print(json.dumps({k:v for k,v in result.items() if k not in ('cells','quality')},indent=2))
if __name__=='__main__':main()
