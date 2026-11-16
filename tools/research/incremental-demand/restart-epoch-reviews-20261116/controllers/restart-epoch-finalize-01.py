from pathlib import Path
import json,statistics
root=Path('/workspace/zil-push-main');w=root/'out/runs/restart-epoch-full-window-01'
s=json.loads((w/'summary.json').read_text());rows=[json.loads(l)for l in(w/'rows.jsonl').read_text().splitlines()];rows=[r for r in rows if not r['sustained']]
assert s['execution_complete'] and s['candidate_rust_accepted'] and len(rows)==324
result={'compiler_source':'bcf409b37c5e5733c57376c0b6123b76321574a9','tree':'34df7372f666f32bcca59c0b78f185a68ca75ca0','binary_sha256':'57de85daefac2ec5f562d15f01bd4f54aafeb4f53838da975671cc49dd809bf2','tool_source':'bcf409b37c5e5733c57376c0b6123b76321574a9','actual_main':'9170bb4210554392e36eb7528596612596ebef80','window':'out/runs/restart-epoch-full-window-01','quality':{k:s[k]for k in('execution_complete','accepted','candidate_rust_accepted','main_regression_accepted','rows','quality_cells')},'runtime_cases':len(s['runtime_profile']['cases']),'runtime_R_over_B_GM':s['runtime_profile']['rust_efficiency_geomean'],'frequency_policy':'Each request exactly once in predeclared 18-request workflow including eight private edits; two balanced blocks, median complete total. No invented industry frequency. Compile quality scoped to separately named finite runtime protocol.','disk_scope':'Explicit user caches/artifacts incl diagnostic IR, global inode dedup/lstat st_blocks at request boundaries, not transient peaks; SDK/host Cargo/evidence excluded. Restore hardlink topology is not a new disk measurement.','sizes':{}}
def total(snapshot,arm):return sum(v['physical_bytes']for k,v in snapshot['categories'].items()if k.startswith(arm+'.'))
for size in('1000','2500','5105'):
 d={};r=[r for r in rows if r['preset']==size];arms=('candidate','rust')if size=='5105'else('baseline','candidate','rust')
 d['main_role']='ResourceLimit; no performance ratio'if size=='5105'else'actual main9170'
 d['total_wait_median_ms']={a:statistics.median(sum(x['measurement']['wall_ms']for x in r if x['arm']==a and x['block']==b)for b in(0,1))for a in arms}
 d['candidate_wait_reduction_vs_rust']=1-d['total_wait_median_ms']['candidate']/d['total_wait_median_ms']['rust']
 d['compile_median_ms']={req:{a:statistics.median(x['measurement']['wall_ms']for x in r if x['arm']==a and x['request']==req)for a in arms}for req in('cold','broad','interface','warm-restart','restore','unchanged')}
 d['compile_median_ms']['eight_private']={a:statistics.median(sum(x['measurement']['wall_ms']for x in r if x['arm']==a and x['block']==b and x['request'].startswith('private-'))for b in(0,1))for a in arms}
 d['disk']={}
 for a in arms:
  peaks=[];growth=[]
  for b in(0,1):
   rs=[x for x in r if x['arm']==a and x['block']==b];by={x['request']:x for x in rs};peaks.append(max(total(x[k],a)for x in rs for k in('disk_before','disk_after')));growth.append(total(by['unchanged-after-edits']['disk_after'],a)-total(by['cold']['disk_after'],a))
  d['disk'][a]={'boundary_peak_physical_bytes_by_block':peaks,'median_boundary_peak_MiB':statistics.median(peaks)/2**20,'eight_private_growth_bytes_by_block':growth,'median_eight_private_growth_MiB':statistics.median(growth)/2**20}
 cases=[c for c in s['runtime_profile']['cases']if c['case'].startswith(size+'/')];d['runtime_R_over_B_range']=[min(c['rust_over_candidate']for c in cases),max(c['rust_over_candidate']for c in cases)];d['runtime_rust_gate_all_pass']=all(c['rust_gate_pass']for c in cases);d['runtime_main_gate_all_pass']=None if size=='5105'else all(c['main_gate_pass']for c in cases)
 result['sizes'][size]=d
out=root/'out/runs/restart-epoch-weighted-final-01.json';out.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'quality':result['quality'],'runtime_cases':result['runtime_cases'],'runtime_R_over_B_GM':result['runtime_R_over_B_GM'],'sizes':{k:{'wait_s':{a:round(v/1000,3)for a,v in d['total_wait_median_ms'].items()},'reduction':d['candidate_wait_reduction_vs_rust'],'disk_MiB':{a:z['median_boundary_peak_MiB']for a,z in d['disk'].items()}}for k,d in result['sizes'].items()}},indent=2))
