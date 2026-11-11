#!/usr/bin/env python3
"""Independently read/recompute existing ThinLTO probe; no subprocesses or imports of runner."""
from pathlib import Path
import collections,hashlib,json,statistics
ROOT=Path('/workspace/scratch/abf690ce880c');R=ROOT/'out/next-optimization/thin-complete-runtime-v1';P=ROOT/'out/next-optimization/thin-complete-protocol-v1';S=ROOT/'research/thinlto-complete-visibility'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
read=lambda p:json.loads(Path(p).read_text())
files=[v for v in R.rglob('*') if v.is_file()];before={str(v):sha(v) for v in files}
p=read(P/'protocol.json');assert sha(P/'protocol.json')=='5b0ff104485ba18c34db170716ead50cc438bfc3b8c8ebc1c1d1266876946728' and (P/'protocol.json').read_bytes()==(R/'protocol.json').read_bytes()
assert p['source_claims']=={'A':'99035931d358add0eaed6d4535035c38413701dd','B':'2549cbec8e1f3096d4c8c8a960459c83095ad13d'}
assert p['counts']=={'cases':2,'clean_compilations':6,'native_formal':36,'native_warmups':6,'native_functional':12,'native_total':54}
assert p['cpus']==[0,1] and p['jobs']==2 and p['iterations']==100000
assert p['orders']==['ABR','BRA','RAB','ARB','RBA','BAR'] and p['limits']=={'B_over_A_max':1.1,'R_over_B_min':.9}
for d in p['tools'].values():assert sha(d['path'])==d['sha256']
for n,h in p['scripts'].items():assert sha(S/n)==h,n
for arm,d in p['build_receipts'].items():
 assert sha(d['path'])==d['sha256'];v=read(d['path']);t=p['tools']['baseline' if arm=='A' else 'exporter'];assert v['source_commit']==p['source_claims'][arm] and v['source_tree']==('1c16744d838064cb1c687ba56ef3a0d17ed3130c' if arm=='A' else 'ebb993128355fed8228627c6d7b37d5cb0a90d45') and v['binary_sha256']==t['sha256'] and str(Path(v['binary']).resolve())==t['path'] and v['release_build_status']=='Pass' and sha(v['build_log'])==v['build_log_sha256']
for tool in ['clang','lld','opt','bcanalyzer','dis','rustc']:
 log=R/('version-'+tool);a=read(log/'process-attempt.json');assert a==read(log/'raw-metrics.json');assert a['exit']==0 and a['reaped'] and not a['residual_descendants'] and a['exception'] is None
 assert ('release: 1.99.1' if tool=='rustc' else '23.1.4') in (log/'stdout').read_text()
rows=[json.loads(v) for v in (R/'native-rows.jsonl').read_text().splitlines()];assert len(rows)==54
counts=collections.Counter();compiles=[];results=[];all_imports=[]
for case,n in [('n64/base',64),('n256/base',256)]:
 fixture=p['cases'][case];assert fixture['selected_indices']==[] and fixture['total_files_per_language']==n
 for name,h in fixture['source_hashes'].items():assert sha(P/case/name)==h
 # Directly derived from the four source recurrence steps, independent of all generator functions.
 values=[]
 for seed in range(251):
  total=0
  for i in range(1,n):
   for j in range(8):
    y=seed
    for k in range(4):y=((2+(3*i+j+2*k)%11)*y+1+(7*i+3*j+k)%37)%251
    total+=y
  values.append(total)
 q,rem=divmod(100000,251);vectors={}
 for b in [0,65,255]:vectors[bytes([b]).hex()]=((q*sum(values)+sum(values[(b+i)%251] for i in range(rem)))%1000003).to_bytes(4,'little').hex()
 assert values==fixture['runtime_oracle']['value_by_seed'] and sum(values)==fixture['runtime_oracle']['cycle_sum']
 assert vectors=={x['stdin_hex']:x['stdout_hex'] for x in fixture['native_vectors']}
 artifacts=read(R/case/'artifacts.json')['arms']
 for arm in 'ABR':
  log=R/case/('compile-'+arm);c=read(log/'validated.json');m=read(log/'process-attempt.json');assert m==read(log/'raw-metrics.json')==c['metrics'];assert m['exit']==0 and m['exception'] is None and not m['timeout'] and m['reaped'] and not m['residual_descendants']
  assert c['program']['path']==str(log/'program') and sha(log/'program')==c['program']['sha256'];assert not c['compile_performance_claim'];compiles.append(c)
  if arm=='R':continue
  d=read(log/'stdout');assert d['status']=='Success' and d['verification_runs']==1 and len(d['source_files'])==n and sum(x=={'phase':'verify','disposition':'executed'} for x in d['query_events'])==1;counts['Verify']+=1
  assert sha(d['artifacts']['executable'])==artifacts[arm]['elf_sha256']
  raw=Path(d['artifacts']['llvm']).read_bytes();assert hashlib.sha256(raw).hexdigest()==artifacts[arm]['sidecar_raw_sha256'];lines=raw.splitlines(keepends=True);assert [i for i,v in enumerate(lines) if v.startswith(b'; verified-fingerprint: ')]==[1];assert hashlib.sha256(b''.join(lines[:1]+lines[2:])).hexdigest()==artifacts[arm]['sidecar_without_verified_fingerprint_sha256']
  nc=d['native_codegen'];assert nc['actual_units']==8 and nc['jobs']==2 and nc['fallback'] is None
  if arm=='A':
   assert nc['backend']=='llvm-api' and len(nc['object_reports'])==8
   for u in nc['object_reports']:
    assert u['input_kind']=='builder' and u['preflight_ns']>0 and u['verify_before_after'] and u['llvm_version']=='23.1.4' and u['pipeline']=='default<O2>|merge-functions=after|aliases=false' and u['cpu']=='x86-64' and u['features']=='' and u['relocation']=='PIC' and u['code_model']=='default';counts['A_unit_preflight_and_LLVM_before_after']+=1
  else:
   b=read(log/'preflight.json');assert nc['object_reports']==[b] and nc['backend']=='clang' and b['original_preflight_ns']>0 and b['partition_input_preflight_ns']>0 and [u['unit'] for u in b['units']]==list(range(8)) and all(u['preflight_ns']>0 for u in b['units']);counts['B_original_model_preflight']+=1;counts['B_partition_input_preflight']+=1;counts['B_unit_preflight']+=8
   receipts=list((log/'wrapper').glob('*/receipt.json'));units=[];links=[]
   for path in receipts:
    v=read(path);directory=path.parent
    for item in ['input','bitcode']:
     if item in v:assert sha(v[item]['path'])==v[item]['sha256']
    if v['kind']=='unit-prelink':
     units.append(v);assert all(v[k] for k in ['summary_block','module_hash_record','verify_before','verify_prelink_after']);dump=(directory/'summary-dump/stdout').read_text();assert 'GLOBALVAL_SUMMARY_BLOCK' in dump and '<HASH ' in dump;assert 'target triple = "x86_64-unknown-linux-gnu"' in (directory/'prelink.ll').read_text();assert '-flto=thin' in v['actual_command'] and '-O2' in v['actual_command'] and '-march=x86-64' in v['actual_command'] and '-fPIC' in v['actual_command'];counts['actual_summary_and_module_hash']+=1
     for step in ['verify-before','thin-prelink','verify-prelink','summary-dump','disassemble-prelink']:
      x=read(directory/step/'process-attempt.json');assert x==read(directory/step/'raw-metrics.json') and x['exit']==0 and not x['timeout'] and x['exception'] is None and x['reaped'] and not x['residual_descendants'];assert x['scope']=='inherited-owned-ZIL-group';assert x['owned_group']==m['owned_group']
      if step in ['verify-before','verify-prelink']:assert '-passes=verify' in x['command'];counts['external_verify_before_or_prelink']+=1
    elif v['kind']=='thin-link':links.append(v)
    else:raise AssertionError(v['kind'])
   assert len(units)==8 and len(links)==1
   v=links[0];link_path=next(q.parent for q in receipts if read(q)['kind']=='thin-link');assert v['cache_was_initially_empty'] and v['cache_before']==[] and v['postlink_optimized_modules_verified']==8
   assert '--ld-path='+p['tools']['lld']['path'] in v['actual_command'] and '-pie' in v['actual_command'] and '-flto=thin' in v['actual_command'];assert any('--thinlto-jobs=2' in a and '--threads=2' in a and '--icf=none' in a for a in v['actual_command']);assert any('enable-merge-functions=true' in a and 'mergefunc-use-aliases=false' in a for a in v['actual_command'])
   for item in v['saved']+v['cache_after']:assert sha(item['path'])==item['sha256']
   x=read(link_path/'thin-link/process-attempt.json');assert x==read(link_path/'thin-link/raw-metrics.json') and x['command']==v['actual_command'] and x['exit']==0 and x['exception'] is None and x['reaped'] and x['owned_group']==m['owned_group']
   saved=[Path(x['path']) for x in v['saved']];optimized=[x for x in saved if x.name.endswith('.4.opt.bc')];imports=[x for x in saved if x.suffix=='.imports'];assert len(optimized)==len(imports)==8 and len(v['actual_imports'])==8
   for i,bc in enumerate(sorted(optimized)):
    x=read(link_path/f'verify-postlink-{i}/process-attempt.json');assert x==read(link_path/f'verify-postlink-{i}/raw-metrics.json') and x['exit']==0 and x['exception'] is None and x['reaped'] and x['owned_group']==m['owned_group'] and '-passes=verify' in x['command'] and str(bc) in x['command'];counts['external_postlink_optimized_verify']+=1
   for imp in imports:
    names=imp.read_text().splitlines();assert names==v['actual_imports'][imp.name];all_imports.append((case,imp.name,names))
   assert v['output_sha256_before_ZIL_publication']==c['program']['sha256']
   for u in units:
    copy=next(x for x in v['saved'] if x['original']==u['original_output']);assert copy['sha256']==u['bitcode']['sha256']
 schedule=[('functional',i,a,h) for h in ['00','ff'] for i,a in enumerate('ABR')]+[('warmup',0,a,'41') for a in 'ABR']+[( 'formal',i,a,'41') for i,order in enumerate(p['orders']) for a in order]
 selected=[v for v in rows if v['case']==case];assert len(selected)==27
 for position,(v,(kind,round_index,arm,h)) in enumerate(zip(selected,schedule)):
  assert (v['kind'],v['round'],v['arm'],v['stdin_hex'],v['position'])==(kind,round_index,arm,h,position)
  log=R/case/'native'/f'{position:03d}-{kind}-{arm}';assert read(log/'record.json')==v;attempt=read(log/'process-attempt.json');assert attempt==read(log/'raw-metrics.json')
  for key,value in attempt.items():assert v[key]==value
  assert v['valid'] and v['exit']==0 and v['exception'] is None and v['reaped'] and not v['timeout'] and not v['residual_descendants']
  assert (log/'stdin').read_bytes().hex()==h and (log/'stdout').read_bytes().hex()==v['stdout_hex']==v['expected_stdout_hex']==vectors[h] and not (log/'stderr').read_bytes();assert sha(v['artifact']['path'])==v['artifact']['sha256'];counts[kind]+=1
 stats={}
 for arm in 'ABR':
  formal=[v for v in selected if v['arm']==arm and v['kind']=='formal'];assert len(formal)==6
  stats[arm]={k:dict(values=[v[k] for v in formal],mean=statistics.mean(v[k] for v in formal),median=statistics.median(v[k] for v in formal),p95=max(v[k] for v in formal),max=max(v[k] for v in formal)) for k in ['wall_ms','cpu_ms','rss_kib']}
 a,b,c=[stats[k]['wall_ms']['mean'] for k in 'ABR'];results.append(dict(case=case,arms=stats,ratios={'B_over_A':b/a,'R_over_B':c/b},pass_gate=b/a<=1.1 and c/b>=.9))
assert len(compiles)==6 and counts['formal']==36 and counts['warmup']==6 and counts['functional']==12 and len(all_imports)==16 and sum(bool(x[2]) for x in all_imports)==2

# Separately extract LLVM function bodies with whole-function regex, then use
# fixed-point reachability, not the producer's parser/DFS/receipt booleans.
import re
SYMBOL=r'[A-Za-z$._][A-Za-z0-9$._-]*'
def functions(path):
 text=Path(path).read_text();result={}
 for m in re.finditer(r'^define ([^\n]+) \{\n(.*?)^\}',text,re.M|re.S):
  header,body=m.groups();name=re.search('@('+SYMBOL+r')\(',header).group(1)
  assert name not in result
  lines=[v.split(';',1)[0].strip() for v in body.splitlines()];lines=[v for v in lines if v]
  labels=[v for v in lines if v.endswith(':')]
  inst=[v for v in lines if not v.endswith(':')]
  targets=set(re.findall(r'\bcall\b[^\n]*?@('+SYMBOL+r')\(',body))
  result[name]={'calls':targets,'cost':len(labels)+len(inst),'AE':'available_externally' in header.split(),'body':body}
 assert len(result)==len(re.findall(r'^define ',text,re.M))
 return result
for case,n in [('n64/base',64),('n256/base',256)]:
 case_root=R/case/'compile-B';pre=read(case_root/'preflight.json');proof=pre['visibility_proof']
 assert all(proof[k] for k in ['current_complete_planner','canonical_owners_equal','original_required_exactly_allowed'])
 assert proof['original_source_cost']==(35777 if n==64 else 144449) and proof['total_copy_cost']==(31360 if n==64 else 125440)
 proof_units=proof['units'];assert [x['unit'] for x in proof_units]==list(range(8))
 owners={};allow={};cost={};original={};prelink={};imported={};outdir=case_root/'wrapper'
 receipts=[(v,read(v)) for v in outdir.glob('*/receipt.json')];link_path,link=next((v.parent,d) for v,d in receipts if d['kind']=='thin-link')
 for u in proof_units:
  i=u['unit'];own=set(u['canonical']);assert len(own)==len(u['canonical']);assert not own & set(owners);owners.update({name:i for name in own})
  allow[i]={v['name'] for v in u['allowed']};assert len(allow[i])==len(u['allowed']) and set(u['required_original'])==allow[i]
  assert sum(v['cost'] for v in u['allowed'])==u['copy_cost']
  for v in u['allowed']:
   assert isinstance(v['cost'],int) and v['cost']>0 and (v['name'] not in cost or cost[v['name']]==v['cost']);cost[v['name']]=v['cost']
 assert sum(u['copy_cost'] for u in proof_units)==proof['total_copy_cost']<=proof['original_source_cost']
 for i,names in allow.items():assert all(k in owners and owners[k]!=i and k!='main' for k in names)
 for path,d in receipts:
  if d['kind']!='unit-prelink':continue
  i=d['proof_unit'];assert i not in original and d['current_machine_receipt_sha256']==sha(case_root/'preflight.json')
  assert sha(d['prelink_ir']['path'])==d['prelink_ir']['sha256']
  original[i]=functions(d['input']['path']);prelink[i]=functions(d['prelink_ir']['path']);imported[i]=functions(link_path/f'import-{i}.ll')
  assert set(original[i])==set(proof_units[i]['canonical']) and set(prelink[i])<=set(original[i])
 assert set(original)==set(prelink)==set(imported)==set(range(8))
 all_original={k:v for part in original.values() for k,v in part.items()};all_prelink={k:v for part in prelink.values() for k,v in part.items()}
 original_cost=sum(v['cost'] for k,v in all_original.items() if k!='main');assert original_cost==proof['original_source_cost']
 assert all(all_original[k]['cost']==v for k,v in cost.items())
 foreign=set(proof['foreign'])
 def reach(part,whole):
  result=set(part)
  while True:
   new={c for k in result for c in whole[k]['calls'] if c in owners};assert all(c in whole for c in new)
   if new<=result:return result-set(part)
   result|=new
 actual_details=[]
 for i in range(8):
  original_required=reach(original[i],all_original);assert original_required==allow[i]
  required=reach(prelink[i],all_prelink);assert required<=allow[i]
  assert len(required)==((56 if n==64 else 224) if i==0 else 0)
  actual=set(imported[i]);own=set(proof_units[i]['canonical']);ae={k for k,v in imported[i].items() if v['AE']};added=actual-own
  assert required<=actual and ae<=allow[i] and actual<=own|allow[i]
  assert added==ae==required
  recorded=link['visibility']['units'][i];assert set(recorded['required_prelink'])==required and set(recorded['actual_definitions'])==actual and set(recorded['actual_AE'])==ae and set(recorded['allowed_original'])==allow[i]
  for f in imported[i].values():assert all(c not in owners or owners[c]==i or c in allow[i] for c in f['calls'])
  for step in ['verify-import-'+str(i),'dis-import-'+str(i)]:
   x=read(link_path/step/'process-attempt.json');assert x==read(link_path/step/'raw-metrics.json') and x['exit']==0 and x['reaped'] and not x['timeout'] and x['exception'] is None and not x['residual_descendants']
  actual_details.append({'unit':i,'own':len(own),'Allowed':len(allow[i]),'allowed_cost':proof_units[i]['copy_cost'],'Required':len(required),'ActualAdded':len(added),'AE':len(ae),'original_name_cost':sum(cost[k] for k in added),'actual_IR_block_instruction_count':sum(imported[i][k]['cost'] for k in added)})
 assert link['current_machine_receipt_sha256']==sha(case_root/'preflight.json')
 assert sum(a.count('force-import-all=true') for a in link['actual_command'])==1
 assert len(link['known_no_body_diagnostics'])==1 and link['known_no_body_diagnostics'][0]['symbol']=='getchar' and 'getchar' in foreign and 'getchar' not in owners
 assert (link_path/'thin-link/stderr').read_text().strip()==link['known_no_body_diagnostics'][0]['raw']
 print('VISIBILITY',case,'source_cost',original_cost,'copy_cost',proof['total_copy_cost'],'units',actual_details)

summary=read(R/'summary.json');assert summary['status']=='NoGo' and not summary['gate_pass'] and summary['outliers_removed']==0
for actual,stored in zip(results,summary['cases']):
 assert actual['case']==stored['case'] and not actual['pass_gate'] and not stored['gate_pass']
 for arm in 'ABR':
  for key in ['wall_ms','cpu_ms','rss_kib']:assert actual['arms'][arm][key]==stored['arms'][arm][key]
 assert actual['ratios']==stored['ratios']
assert before=={str(v):sha(v) for v in files},'raw changed during audit'
print('COUNTS',dict(counts),'compiles',len(compiles),'nonempty_import_files',sum(bool(x[2]) for x in all_imports),'raw_unchanged',len(files),'attempts',len(list(R.rglob('process-attempt.json'))))
for v in results:
 print('CASE',v['case'],'ratios',v['ratios'])
 for arm in 'ABR':
  a=v['arms'][arm];print('RUNTIME',v['case'],arm,'wall',a['wall_ms'],'cpu',a['cpu_ms'],'rss',a['rss_kib'])
for v in compiles:print('COMPILE',v['case'],v['arm'],{k:v['metrics'][k] for k in ['wall_ms','cpu_ms','rss_kib']})
for f in ['protocol.json','summary.json','native-rows.jsonl','execution-manifest.json']:print('SHA',f,sha(R/f))
