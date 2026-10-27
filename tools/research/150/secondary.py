#!/usr/bin/env python3
"""Bound every process, preserve all failures, and separate coverage from latency.
Paired timing runs only after other current-session build/test jobs have finished.
"""
import argparse, pathlib, json, subprocess, hashlib, time, random, os
p=argparse.ArgumentParser();p.add_argument('--root',required=True);p.add_argument('--baseline',required=True);p.add_argument('--candidate',required=True);a=p.parse_args()
root=pathlib.Path(a.root); bins={'A':str(pathlib.Path(a.baseline).resolve()),'C':str(pathlib.Path(a.candidate).resolve())};spec=json.loads((root/'frozen150.json').read_text())
def passed(r):
 return r['returncode']==0 and r.get('complete') and len(r.get('requests',[]))==4 and all(x['status']=='Success' and x['verification_runs']==1 and x['active_queries']==0 for x in r['requests'])
def one(c,v,label,emit=False):
 originals={f:f.read_bytes() for f in pathlib.Path(c['root']).parent.rglob('*.zil')};dst=root/label;dst.parent.mkdir(parents=True,exist_ok=True)
 cmd=[bins[v],str(dst),'verify-emit' if emit else 'verify',c['root'],c['edits'],str(c['capacity'])];t=time.perf_counter_ns()
 try:
  r=subprocess.run(cmd,capture_output=True,timeout=40);rc=r.returncode;stdout=r.stdout;stderr=r.stderr;timeout=False
 except subprocess.TimeoutExpired as e:rc=None;stdout=e.stdout or b'';stderr=e.stderr or b'';timeout=True
 dt=time.perf_counter_ns()-t;restored=True
 for f,b in originals.items():
  if f.read_bytes()!=b:restored=False;f.write_bytes(b)
 dst.mkdir(exist_ok=True);(dst/'stdout').write_bytes(stdout);(dst/'stderr').write_bytes(stderr);(dst/'command.json').write_text(json.dumps(cmd))
 row={'case':c['name'],'variant':v,'category':c['category'],'weight':c['weight'],'process_ns':dt,'returncode':rc,'timeout':timeout,'source_was_restored':restored,'path':label}
 if (dst/'report.json').exists():
  d=json.loads((dst/'report.json').read_text());row.update(complete=d['complete'],vmhwm_kib=d['vmhwm_kib'],requests=[{'scenario':x['scenario'],'status':x['report']['status'],'phase':x['report']['phase'],'api_wall_ns':x['api_wall_ns'],'verification_runs':x['report']['verification_runs'],'active_queries':x['active_queries'],'fingerprint':x['report']['input_fingerprint']}for x in d['records']])
 row['pass']=passed(row) and restored;return row
# A separate, predeclared scaling follow-up. Does not change primary weights.
names=['reverse4096','cycle1024x8','tree256x8','tree1024x8']
cases=[c for name in names for c in spec['cases'] if c['name']==name]
assert len(cases)==4
cpu=min(os.sched_getaffinity(0));os.sched_setaffinity(0,{cpu})
plan={'schema':'zil150-secondary/1','selection':names,'blocks':4,'label':'secondary scaling diagnostic, selected before its own measurements, not original weighted primary','sizes':{c['name']:c['size'] for c in cases},'cpu_affinity':[cpu],'shared_host':True,'primary_frozen_sha256':hashlib.sha256((root/'frozen150.json').read_bytes()).hexdigest(),'binaries':{v:hashlib.sha256(pathlib.Path(b).read_bytes()).hexdigest()for v,b in bins.items()},'started':time.time()}
(root/'secondary-plan150.json').write_text(json.dumps(plan,indent=2));rows=[]
with(root/'secondary-timings150.jsonl').open('x')as f:
 for block in range(4):
  for ci,c in enumerate(cases if block%2==0 else cases[::-1]):
   for pos,v in enumerate(['A','C','C','A']if(block+ci)%2==0 else['C','A','A','C']):
    r=one(c,v,f'secondary/{block}-{c["name"]}-{pos}-{v}');r.update(block=block,position=pos);rows.append(r);f.write(json.dumps(r)+'\n');f.flush()
   print(block,c['name'],all(r['pass']for r in rows[-4:]),flush=True)
plan.update(finished=time.time(),processes=len(rows),requests=sum(len(r.get('requests',[]))for r in rows),failures=sum(not r['pass']for r in rows),all_inputs_restored=all(hashlib.sha256((root/f).read_bytes()).hexdigest()==h for f,h in spec['input_hashes'].items()))
(root/'secondary-receipt150.json').write_text(json.dumps(plan,indent=2))
if not all(r['pass']for r in rows):raise SystemExit(1)
summary={'scope':plan['label'],'processes':len(rows),'requests':plan['requests'],'cases':{}}
for c in cases:
 name=c['name'];pairs=[]
 for b in range(4):
  cells={v:[r for r in rows if(r['block'],r['case'],r['variant'])==(b,name,v)]for v in bins}
  scores={v:[sum(spec['scenario_weights'][x['scenario']]*x['api_wall_ns']for x in r['requests'])for r in rs]for v,rs in cells.items()}
  pairs.append([sum(scores[v])/len(scores[v])for v in ['A','C']])
 summary['cases'][name]={'size':c['size'],'request_ratio_C_over_A':sum(x[1]for x in pairs)/sum(x[0]for x in pairs),'block_ratios':[x[1]/x[0]for x in pairs]}
(root/'secondary-summary150.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))
