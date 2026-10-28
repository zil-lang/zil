#!/usr/bin/env python3
"""Bound every process, preserve all failures, and separate coverage from latency.
Paired timing runs only after other current-session build/test jobs have finished.
"""
import argparse, pathlib, json, subprocess, hashlib, time, random, os
p=argparse.ArgumentParser();p.add_argument('--root',required=True);p.add_argument('--baseline',required=True);p.add_argument('--candidate',required=True);p.add_argument('--mode',choices=['preflight','measure'],required=True);a=p.parse_args()
root=pathlib.Path(a.root); bins={'A':str(pathlib.Path(a.baseline).resolve()),'C':str(pathlib.Path(a.candidate).resolve())};spec=json.loads((root/'frozen158.json').read_text())
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
if a.mode=='preflight':
 rows=[]
 for c in spec['cases']:
  for v in bins:
   r=one(c,v,'preflight/'+c['name']+'-'+v);rows.append(r);(root/'preflight158.json').write_text(json.dumps(rows,indent=2));print(c['name'],v,r['pass'],[(x['scenario'],x['status'])for x in r.get('requests',[])],flush=True)
else:
 pre=json.loads((root/'preflight158.json').read_text());by={(r['case'],r['variant']):r for r in pre};cases=[c for c in spec['cases'] if c['weight']>0 and all(passed(by[c['name'],v])for v in bins)]
 cpu=min(os.sched_getaffinity(0));os.sched_setaffinity(0,{cpu})
 plan={'schema':'zil158-paired/1','spec_sha256':hashlib.sha256((root/'frozen158.json').read_bytes()).hexdigest(),'cases':[c['name']for c in cases],'coverage':sum(c['weight']for c in cases),'excluded':[c['name']for c in spec['cases']if c['weight']>0 and c not in cases],'blocks':spec['blocks'],'cpu_affinity':[cpu],'independent_machine_exclusivity':False,'source_identity':'bound by frozen input and binary manifests, no inherited old measurements','started':time.time()};(root/'measurement-plan158.json').write_text(json.dumps(plan,indent=2))
 rows=[]
 with (root/'timings158.jsonl').open('x') as f:
  for block in range(spec['blocks']):
   order=cases.copy();random.Random(15800+block).shuffle(order)
   for ci,c in enumerate(order):
    for pos,v in enumerate(['A','C','C','A']if(block+ci)%2==0 else ['C','A','A','C']):
     r=one(c,v,f'paired/{block}-{c["name"]}-{pos}-{v}');r.update(block=block,position=pos);rows.append(r);f.write(json.dumps(r)+'\n');f.flush()
    print('block',block,c['name'],all(r['pass']for r in rows[-4:]),flush=True)
 plan.update(finished=time.time(),processes=len(rows),requests=sum(len(r.get('requests',[]))for r in rows),failures=sum(not r['pass']for r in rows),all_inputs_restored=all(hashlib.sha256((root/f).read_bytes()).hexdigest()==h for f,h in spec['input_hashes'].items()));(root/'measurement-receipt158.json').write_text(json.dumps(plan,indent=2))
