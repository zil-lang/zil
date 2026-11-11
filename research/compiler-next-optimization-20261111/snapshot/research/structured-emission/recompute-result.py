#!/usr/bin/env python3
"""Read existing evidence only; never launch a compiler, program or sampler."""
from pathlib import Path
import collections,hashlib,json,statistics
ROOT=Path('/workspace/scratch/abf690ce880c')
R=ROOT/'out/next-optimization/structured-screen-ee0'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
tracked=[p for p in R.rglob('*') if p.is_file() and ('logs' in p.parts or p.parent==R)]
before={str(p):sha(p) for p in tracked}
plan=json.loads((R/'plan.json').read_text());rows=[json.loads(x) for x in (R/'rows.jsonl').read_text().splitlines()];pairs=[json.loads(x) for x in (R/'pairs.jsonl').read_text().splitlines()]
assert len(rows)==84 and len(pairs)==len(plan['pairs'])==42
for x in [plan['script'],plan['model'],*plan['tools'].values(),*(v for a in plan['sources'].values() for v in a.values())]:assert sha(x['path'])==x['sha256']
assert json.loads((R/'completion.json').read_text())['plan_sha256']==sha(R/'plan.json')
# Separately derived specializations of these shared-leaf and owning-closure corpora.
def oracle(case,snapshot,data):
 n={'shared4096':4096,'shared1024':1024,'owned8':8}[case];edit=snapshot=='private';m=65521
 x,depth,hi,lo=data;d=depth%12+1
 for outer in range(hi*256+lo):
  y=(x+outer)%m
  if case=='owned8':
   y%=251;sums=[0]*n;lens=[0]*n
   for _ in range(d):
    for i in range(n):
     byte=(y+i)%251
     if byte==0:sums[i]=lens[i]=0;y=0
     else:
      sums[i]+=byte;lens[i]+=1;y=(sums[i]+lens[i]+i+1+(4 if edit and i==0 else 0))%251
  else:
   half=n//2
   for i in range(half):
    bias=1+i%97+(4 if edit and i==0 else 0)
    a=(y*y+bias)%m;ai=half+i;ab=1+ai%97
    a=(a+ab)%m if d==1 else (a*a+ab)%m
    b=(a+y+3)%m;bi=half+(17*i+1)%half;bb=1+bi%97
    b=(b+bb)%m if d==1 else (b*b+bb)%m
    y=(b*31+y+bias)%m
   for i in range(half,n):y=(y*y+1+i%97)%m
  x=(y+outer)%m
 return bytes([x%256,x//256])
oracles={(c,s,h):oracle(c,s,bytes.fromhex(h)).hex() for c in plan['sources'] for s in ['base','private'] for h in plan['inputs_hex']}
for (c,s,h),v in oracles.items():assert plan['oracles'][c][s][h]==v
kinds=collections.Counter();verify=0;native=0;by_pair=collections.defaultdict(dict)
for ordinal,row in enumerate(rows):
 log=Path(row['log']);record=json.loads((log/'record.json').read_text());assert row==record
 report=json.loads((log/'stdout').read_text());assert (log/'stderr').read_bytes()==b''
 assert row['valid'] and not row['errors'] and row['exit']==0 and not row['timeout']
 for k in ['status','verification_runs','disk_cache','native_codegen','query_events']:assert row[k]==report[k]
 assert report['status']=='Success' and report['verification_runs']==1
 assert sum(x=={'phase':'verify','disposition':'executed'} for x in report['query_events'])==1;verify+=1
 assert row['source_sha256']==plan['sources'][row['case']][row['snapshot']]['sha256']
 nc=report['native_codegen'];assert nc['backend']=='llvm-api' and nc['requested_units']==8 and nc['jobs']==row['jobs'] and len(nc['object_reports'])==nc['actual_units']
 for unit in nc['object_reports']:
  assert unit['llvm_version']=='23.1.4' and unit['pipeline']=='default<O2>|merge-functions=after|aliases=false' and unit['preflight_ns']>0
  kind=unit['input_kind'];kinds[kind]+=1
  if kind=='builder':assert unit['verify_before_after'] is True
  else:assert kind=='object-cache' and unit['llvm_verification_this_request'] is False and unit['producer_verified_before_after'] is True
 for field in ['wall_ms','cpu_ms','rss_kib']:assert isinstance(row[field],(int,float)) and row[field]>0
 assert sha(log/'app')==row['elf_sha256'] and sha(log/'app.ll')==row['sidecar_sha256']
 assert len(row['native_checks'])==3
 for check in row['native_checks']:
  h=check['input_hex'];nlog=log/('native-'+h);expected=oracles[row['case'],row['snapshot'],h]
  assert (nlog/'stdin').read_bytes().hex()==h and (nlog/'stdout').read_bytes().hex()==check['actual_hex']==check['expected_hex']==expected
  assert (nlog/'stderr').read_bytes()==b'' and check['exit']==0 and check['pass'];native+=1
 by_pair[row['pair']][row['arm']]=row

def material(p):
 lines=p.read_bytes().splitlines(keepends=True);assert [i for i,x in enumerate(lines) if x.startswith(b'; verified-fingerprint: ')]==[1]
 return b''.join(lines[:1]+lines[2:])
def topology(row):
 nc=row['native_codegen'];cache=nc['object_cache']
 return [nc['actual_units'],nc['jobs'],nc['fallback']]+[cache[k] for k in ['hits','misses','corrupt','stored','write_failures']]
rawsame=0
for spec,pair in zip(plan['pairs'],pairs):
 assert pair['pair']==spec['pair'];actual=[r['arm'] for r in rows if r['pair']==spec['pair']];assert actual==spec['arms']
 a,b=(by_pair[spec['pair']][k] for k in ['base','cand']);la,lb=(Path(r['log']) for r in [a,b])
 assert (la/'app').read_bytes()==(lb/'app').read_bytes() and material(la/'app.ll')==material(lb/'app.ll') and topology(a)==topology(b)
 assert [x['input_kind'] for x in a['native_codegen']['object_reports']]==[x['input_kind'] for x in b['native_codegen']['object_reports']]
 assert [x['object_bytes'] for x in a['native_codegen']['object_reports']]==[x['object_bytes'] for x in b['native_codegen']['object_reports']]
 rawsame+=((la/'app.ll').read_bytes()==(lb/'app.ll').read_bytes())
 for key in ['elf_equal','sidecar_material_equal','cache_topology_equal','valid']:assert pair[key]
 assert pair['base_ms']==a['wall_ms'] and pair['cand_ms']==b['wall_ms'] and abs(pair['saved_ms']-(a['wall_ms']-b['wall_ms']))<1e-7
cells=[]
for case,jobs in [('shared4096',2),('shared1024',2),('owned8',2),('shared4096',1)]:
 for request in ['cold','warm','private','edit-warm']:
  arms={}
  for arm in ['base','cand']:
   rr=[r for r in rows if (r['case'],r['jobs'],r['request'],r['arm'])==(case,jobs,request,arm)]
   expected=(7 if jobs==2 else 3) if request=='warm' else (3 if jobs==2 else 1) if request=='edit-warm' else 1
   assert len(rr)==expected
   arms[arm]={k:statistics.mean([r[k] for r in rr]) for k in ['wall_ms','cpu_ms','rss_kib']};arms[arm]['n']=len(rr)
  cells.append((case,jobs,request,arms,100*(arms['cand']['wall_ms']/arms['base']['wall_ms']-1)))
assert before=={str(p):sha(p) for p in tracked},'evidence mutated during audit'
print('COUNTS',json.dumps(dict(verify=verify,preflight=sum(kinds.values()),kinds=dict(kinds),native=native,pairs=42,raw_sidecar_equal=rawsame,tracked_unchanged=len(tracked))))
for arm in ['base','cand']:
 rr=[r for r in rows if r['arm']==arm];print('TOTAL',arm,*[sum(r[k] for r in rr) for k in ['wall_ms','cpu_ms']],'rss range',min(r['rss_kib'] for r in rr),max(r['rss_kib'] for r in rr))
print('TABLE')
for c,j,q,arms,pct in cells:
 a,b=(arms[k] for k in ['base','cand']);print(f'| {c} | {j} | {q} | {a["n"]} | {a["wall_ms"]:.3f} | {b["wall_ms"]:.3f} | {pct:+.3f}% | {a["cpu_ms"]:.3f} | {b["cpu_ms"]:.3f} | {a["rss_kib"]:.1f} | {b["rss_kib"]:.1f} |')
for n in ['plan.json','rows.jsonl','pairs.jsonl','failures.json','completion.json']:print('SHA',n,sha(R/n))
print('FAILURES_FILE',json.loads((R/'failures.json').read_text()))
