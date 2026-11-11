#!/usr/bin/env python3
import argparse,functools,hashlib,json,os,random,statistics,subprocess,sys,time
from pathlib import Path
sys.path.insert(0,str(Path(__file__).parent))
import corpus

def main():
 p=argparse.ArgumentParser();p.add_argument('--base',required=True);p.add_argument('--candidate',required=True);p.add_argument('--out',required=True);a=p.parse_args();root=Path(a.out).resolve();root.mkdir(parents=True,exist_ok=False);logs=root/'logs';logs.mkdir();os.sched_setaffinity(0,sorted(os.sched_getaffinity(0))[:2]);records=[];builds=[];cells=[]
 def run(cmd,label,data=None):
  t=time.perf_counter();r=subprocess.run(list(map(str,cmd)),input=data,capture_output=True,timeout=180);row={'label':label,'argv':list(map(str,cmd)),'ms':(time.perf_counter()-t)*1000,'exit':r.returncode,'input':None if data is None else data.hex(),'stdout':r.stdout.hex(),'stderr':r.stderr.decode(errors='replace')};records.append(row);(logs/f'{len(records)}.json').write_text(json.dumps(row,indent=2));return r,row
 @functools.lru_cache(None)
 def model(n,x,d):return corpus.run_model('owned'+str(n),n,x,d,False)
 def expected(n,variant,data):
  x,depth,hi,lo=data;count=hi*256+lo;d=depth%12
  for i in range(count):x=(model(n if variant=='unentered' else 1,((x+i)%65521)%251,d)+i)%65521
  return bytes((x%256,x//256))
 def source(n,lang,variant):
  full=corpus.resources(n,lang,False)
  if variant=='unused_definitions':
   one=corpus.resources(1,lang,False);marker='fn run(x:i64,d:i32)->i64';full=full.split(marker)[0]+marker+one.split(marker)[1]
  elif variant=='one_hot':
   for i in range(1,n):
    arg=f'u8((y+{i})%251)' if lang=='zil' else f'((y+{i})%251) as u8';text=f'y=c{i}({arg}); if y<0 {{return 65520;}} ';assert full.count(text)==1;full=full.replace(text,'')
  return full+corpus.io_source(lang).replace('depth%12+1','depth%12')
 try:
  for variant,sizes,depths in [('unentered',(8,16,32,64),(0,)),('one_hot',(16,32,64),(1,6)),('unused_definitions',(16,32,64),(1,6))]:
   for n in sizes:
    programs={}
    for arm,compiler in [('base',a.base),('cand',a.candidate),('rust','rustc')]:
     d=root/variant/str(n)/arm;d.mkdir(parents=True);lang='rust' if arm=='rust' else 'zil';src=d/('main.rs' if lang=='rust' else 'main.zil');src.write_text(source(n,lang,variant));exe=d/'app'
     cmd=([compiler,src,'--edition=2024','-C','opt-level=2','-C','overflow-checks=yes','-C','debuginfo=0','-C','incremental='+str(d/'incremental'),'-o',exe] if arm=='rust' else [compiler,'build',src,'-o',exe,'--json','--opt','O2','--codegen-units','8','--codegen-jobs','2','--incremental-o2','--function-merging','before-and-after' if arm=='cand' else 'after'])
     r,row=run(cmd,f'{variant}/{n}/{arm}/build');rep=None if arm=='rust' else json.loads(r.stdout);ok=r.returncode==0 and (arm=='rust' or rep['status']=='Success' and rep['verification_runs']==1);builds.append({'variant':variant,'n':n,'arm':arm,'valid':ok,'source_sha256':hashlib.sha256(src.read_bytes()).hexdigest(),'report':rep});assert ok,builds[-1];programs[arm]=exe
    for depth in depths:
     data=bytes((29,depth,255,255));want=expected(n,variant,data);vals={k:[] for k in programs}
     for repeat in range(-1,9):
      arms=list(programs);random.Random(77*n+depth+repeat).shuffle(arms)
      for arm in arms:
       r,row=run([programs[arm]],f'{variant}/{n}/{depth}/{repeat}/{arm}',data);assert r.returncode==0 and r.stdout==want,(row,want.hex())
       if repeat>=0:vals[arm].append(row['ms'])
     med={k:statistics.median(v) for k,v in vals.items()};ratio=med['cand']/med['base'];cells.append({'variant':variant,'n':n,'depth':depth,'median_ms':med,'values_ms':vals,'latency_ratio':ratio,'rust_throughput_ratio':med['rust']/med['cand'],'runtime_gate_pass':ratio<=1.10 and med['rust']/med['cand']>=.9});print(json.dumps(cells[-1]),flush=True)
  result={'status':'Complete finite neighbor coverage','runtime_gate_pass':all(c['runtime_gate_pass'] for c in cells),'compile_requests':len(builds),'runtime_requests':len(records)-len(builds),'cells':cells,'builds':builds,'scope':'additional common empty/mostly-inactive neighbors; separate from unchanged original weighted protocol'}
  (root/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
 except Exception as e:
  (root/'failure.json').write_text(json.dumps({'error':repr(e),'builds':builds,'cells':cells},indent=2));raise
 finally:(root/'rows.json').write_text(json.dumps(records,indent=2)+'\n')
if __name__=='__main__':main()
