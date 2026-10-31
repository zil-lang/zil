import json, hashlib, subprocess
from pathlib import Path
from fractions import Fraction
from collections import defaultdict

root=Path('/workspace/.zil-env/research/slice1-screen')
manifest=json.loads((root/'manifest.json').read_text())
summary=json.loads((root/'summary.json').read_text())
rows=[json.loads(s) for s in (root/'rows.jsonl').read_text().splitlines()]
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert len(rows)==200==summary['builds']==summary['native_checks']
assert manifest['candidate_source']=='8fb69d61b41a241806e7021de93a27ce7e213f7b'
assert manifest['baseline_source']=='21058d0e331de764e3a1b9123c3e1edf5e55a1aa'
assert manifest['modules']==[4,32] and manifest['blocks']==5
assert manifest['requests']==['cold','unchanged','unused','private','restore']
for arm,entry in manifest['compilers'].items():
 assert sha(Path(entry['path']))==entry['sha256'], arm
seen=set(); data=defaultdict(list); cache_cells=defaultdict(set); elfs=defaultdict(set); inventory={}
final_elf_checks=0; snapshot_hashes={}
for modules in [4,32]:
 leaves=modules-1
 names=[f'm{i:03}' for i in range(leaves)]
 contents={}
 for i,name in enumerate(names):
  body=f'fn h0()->i32{{return {i+1}}}\n'+''.join(f'fn h{j}()->i32{{return h{j-1}()}}\n' for j in range(1,7))+'pub fn value()->i32{return h6()}\n'
  contents[name+'.zil']=body
 base=contents[names[-1]+'.zil']; unused=base+'extern "C" fn unused_foreign(x:i32)->i32;\n'
 private=unused.replace(f'return {leaves}',f'return {leaves+1}',1)
 expected_snapshots={'cold':base,'unchanged':base,'unused':unused,'private':private,'restore':base}
 for request,text in expected_snapshots.items():snapshot_hashes[modules,request]=hashlib.sha256(text.encode()).hexdigest()
 contents['main.zil']=''.join(f'import {name}\n' for name in names)+'fn value()->i32{return '+'+'.join(f'{name}.value()' for name in names)+'}\n'
 for name,text in contents.items():assert (root/f'source-{modules}'/name).read_text()==text
for r in rows:
 m,b,pos,arm,req=[r[k] for k in ['modules','block','position','arm','request']]
 assert m in [4,32] and b in range(5) and pos in range(4) and req in manifest['requests']
 order=['baseline','candidate','candidate','baseline'] if b%2==0 else ['candidate','baseline','baseline','candidate']
 assert arm==order[pos]
 key=(m,b,pos,arm,req);assert key not in seen;seen.add(key)
 d=root/f'm{m}-b{b}-p{pos}-{arm}'
 report=json.loads((d/f'{req}.json').read_text()); native=(d/f'{req}.native').read_bytes()
 assert report['status']=='Success' and report['phase']=='link' and report['diagnostics']==[]
 assert report['verification_runs']==1==r['verification_runs']
 assert sum(e['phase']=='verify' and e['disposition']=='executed' for e in report['query_events'])==1
 assert len(report['commands'])==1 and report['commands'][0][0]=='/workspace/.zil-env/llvm-23.1.4/bin/clang'
 n=report['native_codegen'];c=n['object_cache'];units=4 if m==4 else 8
 assert n['backend']=='llvm-api' and n['actual_units']==r['units']==units
 assert c['enabled'] and c['hits']==r['hits'] and c['misses']==r['misses']
 assert c['hits']+c['misses']==units and c['corrupt']==0
 assert len(n['object_reports'])==units
 assert sum(o['cache']['state']=='hit' for o in n['object_reports'])==c['hits']
 assert sum(o['cache']['state']=='miss' for o in n['object_reports'])==c['misses']
 if req=='cold':assert c['hits']==0
 if req in ['unchanged','restore']:assert c['hits']==units
 if req=='unused':assert c['hits']==(units if arm=='candidate' else 0)
 if req=='private':assert c['misses']==1
 assert native==f'{m*(m-1)//2+int(req=="private")}\n'.encode()
 assert r['source_sha256']==snapshot_hashes[m,req]
 expected_cmd=[manifest['compilers'][arm]['path'],'build',str(root/f'source-{m}'/'main.zil'),*manifest['flags'],'--object-cache',str(d/'cache'),'-o',str(d/'program'),'--json']
 assert r['command']==expected_cmd
 assert isinstance(r['wall_ns'],int) and r['wall_ns']>0
 if req=='restore': assert sha(d/'program')==r['elf_sha256'];final_elf_checks+=1
 data[m,req,arm,b].append(r['wall_ns']);cache_cells[m,req,arm].add((c['hits'],c['misses']))
 elfs[m,req].add(r['elf_sha256'])
 for suffix in ['json','stderr','native']:
  p=d/f'{req}.{suffix}';inventory[str(p.relative_to(root))]=sha(p)
recomputed=[]; comparisons=0;max_error=0.0
for m in [4,32]:
 for req in manifest['requests']:
  values={};means_out={}
  for arm in ['baseline','candidate']:
   means=[]
   for b in range(5):
    samples=data[m,req,arm,b];assert len(samples)==2
    means.append(Fraction(sum(samples),2))
   values[arm]=sorted(means)[2]/1_000_000
   means_out[arm]=[float(x/1_000_000) for x in means]
  ratio=values['candidate']/values['baseline']
  published=[x for x in summary['cells'] if (x['modules'],x['request'])==(m,req)];assert len(published)==1
  cell=published[0]
  for arm in ['baseline','candidate']:
   error=abs(float(values[arm])-cell['median_block_mean_ms'][arm]);assert error<1e-12;max_error=max(max_error,error);comparisons+=1
  error=abs(float(ratio)-cell['candidate_over_baseline']);assert error<1e-12;max_error=max(max_error,error);comparisons+=1
  recomputed.append({'modules':m,'request':req,'block_means_ms':means_out,'medians_ms':{k:float(v) for k,v in values.items()},'ratio':float(ratio),'elf_receipt_unique_hashes_across_arms':len(elfs[m,req])})
assert len(seen)==200 and final_elf_checks==40 and comparisons==30
inventory.update({n:sha(root/n) for n in ['manifest.json','summary.json','rows.jsonl']})
(root.parent/'slice1-screen-review-inventory.json').write_text(json.dumps(inventory,indent=2)+'\n')
result={'reviewer':'/root/review_incremental_slice1','implementation_commit':manifest['candidate_source'],'verdict':'Pass','scope':'Independent recomputation and retained-evidence audit of scoped O0 wall-time screen; not Rust comparison or product acceptance','build_records_checked':200,'native_stdout_checked':200,'current_verify_checked':200,'complete_unique_requests':200,'numeric_summary_fields_checked':30,'max_numeric_difference':max_error,'raw_log_files_hashed':600,'retained_final_elf_sha256_checked':40,'intermediate_elf_receipts_only':160,'compiler_sha256_checked':manifest['compilers'],'recomputed_cells':recomputed,'limitations':['No timing rerun; source identity of binaries also relies on author build receipts','Only 40 final restore ELF files remain; 160 intermediate ELF hashes are contemporaneous receipts, not fresh byte verification','Native stdout is retained for all 200 requests; native exit status was asserted by runner but not stored as a separate field','Two synthetic module-tree sizes, O0 only, wall-only measurement, OS cache uncontrolled, five paired blocks; no CPU/RSS or cross-host confidence claim','4-module private-edit ratio 1.0112265607698805 is unfavorable and retained; these data do not establish a general private-edit speedup','No Rust comparison, full-language acceptance, main integration or product breakthrough claimed'],'evidence_manifest_sha256':inventory['manifest.json'],'rows_sha256':inventory['rows.jsonl'],'summary_sha256':inventory['summary.json']}
(root.parent/'slice1-screen-review.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({k:result[k] for k in ['verdict','build_records_checked','numeric_summary_fields_checked','max_numeric_difference','retained_final_elf_sha256_checked','intermediate_elf_receipts_only']},indent=2))
