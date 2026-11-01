#!/usr/bin/env python3
"""Independent read-only reconstruction of scoped performance evidence; no benchmark execution."""
from pathlib import Path
from collections import Counter,defaultdict
from fractions import Fraction
import hashlib,json,math,re,subprocess
R=Path('/workspace/.zil-env/research'); REPO=Path('/workspace/zil')
INVENTORY={}; HASH_CACHE={}; numeric_checks=0; max_error=0.0

def sha(p):
 p=Path(p)
 if p not in HASH_CACHE: HASH_CACHE[p]=hashlib.sha256(p.read_bytes()).hexdigest()
 return HASH_CACHE[p]
def track(p): INVENTORY[str(p)]=sha(p)
def read(p):track(p);return json.loads(Path(p).read_text())
def rows(p):track(p);return [json.loads(s) for s in Path(p).read_text().splitlines()]
def median(v):
 v=sorted(v);n=len(v);return (v[(n-1)//2]+v[n//2])/2

def number(actual,expected):
 global numeric_checks,max_error
 expected=float(expected);error=abs(actual-expected);numeric_checks+=1;max_error=max(error,max_error)
 assert math.isclose(actual,expected,rel_tol=1e-12,abs_tol=1e-12),(actual,expected)
def leaf(i,edit):
 suffix=f'+{edit}' if i==1 and edit else ''
 return f'fn h0()->i32{{return {i}{suffix};}}\n'+''.join(f'fn h{j}()->i32{{return h{j-1}();}}\n' for j in range(1,7))+'pub fn value()->i32{return h6();}\n'
def main_source(n,rust=False):
 if rust:return ''.join(f'mod m{i};\n' for i in range(1,n))+'fn main(){let mut total:i32=0;\n'+''.join(f'total=total+m{i}::value();\n' for i in range(1,n))+'println!("{}",total);}\n'
 return ''.join(f'import m{i}\n' for i in range(1,n))+'fn value()->i32{var total:i32=0;\n'+''.join(f'total=total+m{i}.value();\n' for i in range(1,n))+'return total;}\n'
def bytes_sha(s):return hashlib.sha256(s.encode()).hexdigest()
def arm_order(block,rust=False):
 a=['baseline','candidate','candidate','baseline'] if block%2==0 else ['candidate','baseline','baseline','candidate']
 return (['rust']+a+['rust'] if block%2==0 else a[:2]+['rust','rust']+a[2:]) if rust else a

def identities(m):
 out=[]
 for kind in ['compilers','probes']:
  for arm,identity in m.get(kind,{}).items():
   p=Path(identity['path']);current=sha(p) if p.exists() else None
   if p.exists():track(p)
   out.append({'kind':kind,'arm':arm,'recorded_sha256':identity['sha256'],'current_sha256':current,'matches_current_file':current==identity['sha256'],'path':str(p)})
 return out

def window(name):
 d=R/name;m=read(d/'manifest.json');rs=rows(d/'rows.jsonl');summary=read(d/'summary.json')
 blocks=m['blocks'];assert isinstance(blocks,int) and blocks>0
 assert len(rs)==summary['rows']==312*blocks
 assert blocks==(10 if name.startswith('stage4-') else 5)
 assert m['cli_modules']==[32,128] and m['functions_per_leaf']==8
 assert m['session_cases']==[[32,32,'check'],[128,32,'check'],[512,32,'check'],[128,8192,'check'],[32,32,'build'],[128,32,'build']]
 labels={'cold':0,'unchanged':0,'private1':1,'private2':2,'restore':0}
 assert m['requests']==list(labels)
 expected=set()
 for n in [32,128]:
  for mode in ['check','build']:
   for block in range(blocks):
    for pos,arm in enumerate(arm_order(block,True)):
     for label,edit in labels.items():expected.add(('cli',n,mode,0,block,pos,arm,label,edit))
 for n,cap,mode in m['session_cases']:
  for block in range(blocks):
   for pos,arm in enumerate(arm_order(block)):
    for label,edit in [('cold',0),('unchanged',0),*[("private",i) for i in range(1,6)],('restore',0)]:expected.add(('session',n,mode,cap,block,pos,arm,label,edit))
 observed=set();groups=defaultdict(list);session_transcripts={};cache_patterns=Counter();counts=Counter();retained_elves=0
 for row in rs:
  interface,n,mode,block,pos,arm,label=[row[k] for k in ['interface','modules','mode','block','position','arm','request']]
  cap=row.get('capacity',0);edit=labels[label] if interface=='cli' else row['edit'];key=(interface,n,mode,cap,block,pos,arm,label,edit)
  assert key not in observed;observed.add(key);assert row['elapsed_ns']>0
  group=(interface,n,mode,cap,label);groups[group].append(row);counts[interface]+=1;counts['zil']+=arm!='rust'
  if interface=='cli':
   run=d/f'cli-{n}-{mode}-{block}-{pos}-{arm}';out=run/f'{label}.stdout';err=run/f'{label}.stderr';track(out);track(err)
   assert row['source_sha256']==bytes_sha(main_source(n,arm=='rust'))
   assert row['edited_sha256']==bytes_sha(leaf(1,edit))
   source=d/f'source-{n}'
   if arm=='rust':
    cmd=[m['compilers'][arm]['path'],str(source/'main.rs'),'--edition=2024','-C','opt-level=0','-C','overflow-checks=yes','-C','codegen-units=8','-C',f'incremental={run/"incremental"}','-o',str(run/'program')]
    if mode=='check':cmd+=['--emit=metadata']
   else:
    cmd=[m['compilers'][arm]['path'],mode,str(source/'main.zil'),'--json']
    if mode=='build':cmd+=['--entry','value','--opt','O0','--codegen-units','8','--codegen-jobs','2','--object-cache',str(run/'objects'),'-o',str(run/'program')]
    report=read(out);assert report['status']=='Success' and report['verification_runs']==row['verification_runs']==1 and report['diagnostics']==[]
    assert report['phase']==('link' if mode=='build' else 'verify');assert len(report['source_files'])==n;assert row['native_codegen']==report['native_codegen'];counts['cli_verify']+=1
    if mode=='build':
     cg=report['native_codegen'];oc=cg['object_cache'];assert cg['backend']=='llvm-api' and cg['requested_units']==cg['actual_units']==8 and cg['jobs']==2
     assert oc['enabled'] and oc['hits']+oc['misses']==8 and oc['corrupt']==oc['write_failures']==0
     assert len(cg['object_reports'])==8
     assert all((x['producer_verified_before_after'] and x['preflight_ns']>0 and x['llvm_verification_this_request'] is False) if x['input_kind']=='object-cache' else (x['verify_before_after'] and x['pipeline']=='default<O0>') for x in cg['object_reports'])
     cache_patterns[(arm,label,oc['hits'],oc['misses'])]+=1
   assert cmd==row['command'];assert row['resources']['max_rss_kib']>0 and row['resources']['user_seconds']>=0 and row['resources']['system_seconds']>=0
   if mode=='build':
    native=run/f'{label}.native';track(native);assert native.read_bytes()==f'{sum(range(1,n))+edit}\n'.encode();counts['cli_native_stdout']+=1
    if label=='restore':
     assert sha(run/'program')==row['elf_sha256'];track(run/'program');retained_elves+=1
     if arm!='rust':track(run/'program.ll')
  else:
   run=d/f'session-{n}-{cap}-{mode}-{block}-{pos}-{arm}'
   if run not in session_transcripts:
    parsed=rows(run/'stdout.jsonl');track(run/'stderr');assert (run/'stderr').read_bytes()==b'';assert len(parsed)==8
    session_transcripts[run]={(x['request'],x['edit']):x for x in parsed}
    if mode=='build':track(run/'program');track(run/'program.ll')
   raw=session_transcripts[run][(label,edit)];assert row=={'interface':'session','mode':mode,'block':block,'position':pos,'arm':arm,**raw}
   assert row['functions_per_leaf']==8 and row['rss_kib']>0
   if name.startswith('stage4-'):assert isinstance(row['process_cpu_ns'],int) and 0<row['process_cpu_ns']<=len(m['cpus'])*row['elapsed_ns']+1000000
   else:assert 'process_cpu_ns' not in row
   if arm=='candidate' and label=='unchanged':assert row['phases']['parse']['executed']==0
   phases=row['phases'];assert phases['verify']['executed']==1 and phases['verify']['reused']==0;counts['session_verify_phase']+=1
   assert phases['parse']['executed']+phases['parse']['reused']==n
   wanted={'parse','expand','ct','types','analyze','verify'}|({'lower','link'} if mode=='build' else set());assert set(phases)==wanted
   assert sum(p['interval_ns'] for p in phases.values())<=row['elapsed_ns']
   assert all(p['interval_ns']>=0 for p in phases.values())
   for phase in wanted-{'parse'}:assert phases[phase]['executed']+phases[phase]['reused']==1
   if arm=='candidate':assert row['retained_raw']<=cap
   if mode=='build':counts['session_native_assertions']+=1
 assert observed==expected,(len(observed),len(expected),expected-observed)
 assert dict(counts)=={k:v*blocks for k,v in {'cli':120,'zil':272,'cli_verify':80,'cli_native_stdout':60,'session':192,'session_verify_phase':192,'session_native_assertions':64}.items()},counts
 indexed={(x['interface'],x['modules'],x['mode'],x['capacity'],x['request']):x for x in summary['cells']};assert len(indexed)==len(summary['cells'])==len(groups)==44 and set(indexed)==set(groups)
 unfavorable=[];memory_unfavorable=[];recomputed=[]
 for key,group in sorted(groups.items()):
  record=indexed[key];armstats={}
  for arm in sorted({x['arm'] for x in group}):
   relevant=[x for x in group if x['arm']==arm];means=[]
   for block in range(blocks):
    chunk=[x['elapsed_ns'] for x in relevant if x['block']==block];assert len(chunk)==(10 if key[0]=='session' and key[4]=='private' else 2)
    means.append(Fraction(sum(chunk),len(chunk)))
   mid=median(means);walls=sorted(x['elapsed_ns'] for x in relevant);p95=walls[min(len(walls)-1,int(.95*len(walls)))];memory=[x.get('rss_kib',x.get('resources',{}).get('max_rss_kib')) for x in relevant]
   actual=record['arms'][arm];number(actual['median_block_mean_ms'],mid/1000000);number(actual['observed_p95_ms'],Fraction(p95,1000000));number(actual['median_rss_kib'],median([Fraction(x) for x in memory]));armstats[arm]={'median_ns':mid,'median_rss_kib':median([Fraction(x) for x in memory])}
  ratio=armstats['candidate']['median_ns']/armstats['baseline']['median_ns'];number(record['candidate_over_baseline'],ratio)
  if 'rust' in armstats:number(record['candidate_over_rust'],armstats['candidate']['median_ns']/armstats['rust']['median_ns'])
  tag=dict(zip(['interface','modules','mode','capacity','request'],key));detail={**tag,'candidate_over_baseline':float(ratio),'baseline_ms':float(armstats['baseline']['median_ns']/1000000),'candidate_ms':float(armstats['candidate']['median_ns']/1000000),'rss_ratio':float(armstats['candidate']['median_rss_kib']/armstats['baseline']['median_rss_kib']),'arms':record['arms']};recomputed.append(detail)
  if key[0]=='session' and name.startswith('stage4-'):
   cpu={}
   for arm in ['baseline','candidate']:
    means=[]
    for block in range(blocks):
     samples=[x['process_cpu_ns'] for x in group if x['arm']==arm and x['block']==block];means.append(Fraction(sum(samples),len(samples)))
    cpu[arm]=median(means)
   detail['process_cpu_ms']={a:float(v/1000000) for a,v in cpu.items()};detail['candidate_over_baseline_process_cpu']=float(cpu['candidate']/cpu['baseline'])
  if ratio>1:unfavorable.append(detail)
  if detail['rss_ratio']>1:memory_unfavorable.append(detail)
 expected_patterns=Counter({(arm,label,*({'cold':(0,8),'unchanged':(8,0),'private1':(7,1),'private2':(7,1),'restore':(8,0)}[label])):4*blocks for arm in ['baseline','candidate'] for label in labels})
 assert cache_patterns==expected_patterns
 # Current source tree contains the final restore state for every generated module.
 for n in [32,128]:
  source=d/f'source-{n}'
  for ext in ['zil','rs']:
   track(source/f'main.{ext}');assert (source/f'main.{ext}').read_text()==main_source(n,ext=='rs')
   for i in range(1,n):
    track(source/f'm{i}.{ext}');assert (source/f'm{i}.{ext}').read_text()==leaf(i,0)
 return {'window':name,'baseline_source':m['baseline_source'],'candidate_source':m['candidate_source'],'counts':dict(counts),'blocks':blocks,'rows':len(rs),'native_controls':124*blocks,'cells':44,'summary_values_recomputed':388,'retained_cli_restore_elf_hashes_checked':retained_elves,'identities':identities(m),'cache_hit_miss_patterns':[{'arm':a,'request':q,'hits':h,'misses':x,'count':c} for (a,q,h,x),c in sorted(cache_patterns.items())],'time_unfavorable_cells':unfavorable,'rss_unfavorable_cells':memory_unfavorable,'all_cells':recomputed,'cpu_rows':sum('process_cpu_ns' in r for r in rs),'cpu_over_wall_min_max':([min(r['process_cpu_ns']/r['elapsed_ns'] for r in rs if 'process_cpu_ns' in r),max(r['process_cpu_ns']/r['elapsed_ns'] for r in rs if 'process_cpu_ns' in r)] if name.startswith('stage4-') else None)}

def mixed():
 d=R/'stage4-mixed';m=read(d/'manifest.json');blocks=m['blocks'];assert blocks==5;rs=rows(d/'rows.jsonl');summary=read(d/'summary.json');assert len(rs)==summary['requests']==320 and summary['native_checks']==336 and summary['oracle_builds']==16
 cases={x['path']:(i,x) for i,x in enumerate(m['cases'])};expected={};bad=[];seen=set()
 for case,(idx,case_manifest) in cases.items():
  for path,digest in case_manifest['files'].items():
   track(REPO/'examples/mainline'/path);assert sha(REPO/'examples/mainline'/path)==digest
  oracle=[]
  for opt in ['O0','O2']:
   run=d/f'oracle-{idx}-{opt}';report=read(run/'report.json');assert report['status']=='Success' and report['verification_runs']==1;track(run/'stderr');oracle.append(read(run/'native.json'));track(run/'program');track(run/'program.ll');assert report['phase']=='link' and report['native_codegen']['backend']=='clang' and not report['native_codegen']['object_cache']['requested']
  assert oracle[0]==oracle[1];expected[case]=oracle[0]
 for row in rs:
  case,opt,block,pos,arm=[row[k] for k in ['case','opt','block','position','arm']];assert arm_order(block)[pos]==arm
  key=(case,opt,block,pos,arm);assert key not in seen;seen.add(key)
  idx=cases[case][0];run=d/f'{idx}-{opt}-{block}-{pos}-{arm}';report=read(run/'report.json');track(run/'stderr');assert report['status']=='Success' and report['verification_runs']==1
  assert row['elapsed_ns']>0 and row['command']==[m['compilers'][arm]['path'],'build',str(REPO/'examples/mainline'/case),'--opt',opt,'--json','-o',str(run/'program')]
  assert report['phase']=='link' and report['diagnostics']==[]
  cg=report['native_codegen'];assert cg['backend']=='llvm-api' and cg['requested_units']==cg['actual_units']==cg['jobs']==1 and not cg['object_cache']['requested'];assert len(cg['object_reports'])==1 and cg['object_reports'][0]['pipeline']==f'default<{opt}>' and cg['object_reports'][0]['verify_before_after']
  assert report['artifacts']['executable']==str(run/'program') and report['artifacts']['llvm']==str(run/'program.ll');track(run/'program');track(run/'program.ll')
  outcome=read(run/'native.json');assert outcome==row['native'];assert row['matches_oracle']==(outcome==expected[case]);assert arm=='baseline' or row['matches_oracle']
  if not row['matches_oracle']:bad.append(row)
 assert seen=={(case,opt,block,pos,arm) for case in cases for opt in ['O0','O2'] for block in range(blocks) for pos,arm in enumerate(arm_order(block))};assert len(seen)==320
 assert len(bad)==10 and all(x['case']=='byte_io/main.zil' and x['opt']=='O2' and x['arm']=='baseline' and x['native']['exit']==2 for x in bad)
 assert all(x['native']['stdout_hex']==expected[x['case']]['stdout_hex'] and x['native']['stderr_hex']==expected[x['case']]['stderr_hex'] for x in bad)
 idx={(x['case'],x['opt']):x for x in summary['cells']};assert len(idx)==len(summary['cells'])==16
 unfavorable=[]
 for (case,opt),cell in idx.items():
  selected=[x for x in rs if x['case']==case and x['opt']==opt];times={}
  for arm in ['baseline','candidate']:
   means=[]
   for block in range(blocks):
    samples=[x['elapsed_ns'] for x in selected if x['arm']==arm and x['block']==block];assert len(samples)==2;means.append(Fraction(sum(samples),2))
   times[arm]=median(means);number(cell['median_block_mean_ms'][arm],times[arm]/1000000)
  valid=all(x['matches_oracle'] for x in selected);assert cell['semantically_equivalent']==valid
  assert cell['baseline_wrong_results']==sum(not x['matches_oracle'] for x in selected if x['arm']=='baseline')
  if valid:
   ratio=times['candidate']/times['baseline'];number(cell['candidate_over_baseline'],ratio)
   if ratio>1:unfavorable.append({'case':case,'opt':opt,'ratio':float(ratio)})
  else:assert cell['candidate_over_baseline'] is None
 return {'window':d.name,'identities':identities(m),'all_cells':summary['cells'],'requests':320,'oracle_builds':16,'native_controls':336,'zil_verify1_reports_checked':336,'wrong_baseline_results':10,'excluded_cell':['byte_io/main.zil','O2'],'excluded_ratio':None,'all_other_cells_equivalent':True,'time_unfavorable_cells':unfavorable}

def main():
 windows=[window(n) for n in ['stage3-main-final','stage3-isolation','stage4-main-final','stage4-isolation']];mix=mixed()
 assert all(i['matches_current_file'] for w in [*windows,mix] for i in w['identities'])
 for p in ['stage4-path-tests.log','stage4-final-clippy.log','stage4-final-fmt.log','stage4-review-artifacts.log','stage4-review-frontend.log','stage4-review-source-protection.log']:track(R/p)
 for p in ['combined_bench.py','mixed_bench.py','session_probe.rs']:track(REPO/'tools/research/incremental-compilation'/p)
 log=R/'stage4-scoped.log';track(log);text=log.read_text();totals=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out',text);assert sum(int(t[0]) for t in totals)==495 and all(int(t[1])==0 for t in totals);assert not re.search(r'test result: FAILED|^error:',text,re.M)
 out={'reviewer':'/root/review_incremental_slice1','verdict':'Pass','scope':'Scoped data integrity/statistical reconstruction; not product, Rust parity or formal full-suite acceptance.','windows':windows,'mixed':mix,'scoped_test_log':{'passed':495,'failed':0,'result_groups':len(totals),'zero_test_groups':sum(int(t[0])==0 for t in totals),'note':'Zero-test targets contribute zero, not a separate pass.'},'numeric_fields_recomputed':numeric_checks,'max_absolute_float_difference':max_error,'limitations':['Session per-request Verify=1 and native success are asserted by the hash-bound probe before emitting each row; full CompileReport and native stdout are not separately persisted for those requests. Raw phase records independently show one Verify execution.','CLI build records retain all native stdout and per-request ELF hashes; each run overwrites the executable, so only the final restore ELF per run remains for present-day hash verification.','All four windows have separate candidate CLI files preserved for current hash checks. Source/binary build provenance is separate from this statistical reconstruction.', 'Stage-4 process_cpu_ns includes compiler threads and excludes child linkers/native controls; it is diagnostic and not whole-build-tree CPU. Stage-3 has no such field.','CLI ru_maxrss is child high-water RSS; Session VmRSS is a post-request sample. They are different metrics and cannot be combined into a peak-memory claim.','Phase fields are time between observer events, including intervening driver work; they are not exclusive phase CPU cost.','Session private pools five consecutive edits per arm/run. Each block/arm mean has ten private samples versus two for other request labels.','Rust reference is direct rustc O0/metadata with its stated incremental/CGU flags, not Cargo or full language/product parity.','No benchmark was rerun; all unfavorable cells are retained in the review.']}
 (R/'stage4-evidence-recomputed.json').write_text(json.dumps(out,indent=2)+'\n');(R/'stage4-evidence-review-inventory.json').write_text(json.dumps(INVENTORY,indent=2)+'\n')
 print(json.dumps({'windows':[{'name':x['window'],'rows':x['rows'],'blocks':x['blocks'],'cells':44,'time_unfavorable':len(x['time_unfavorable_cells']),'rss_unfavorable':len(x['rss_unfavorable_cells'])} for x in windows],'mixed':mix,'scoped_tests':out['scoped_test_log'],'numeric_fields':numeric_checks,'max_float_difference':max_error,'inventory_files':len(INVENTORY)},indent=2))
if __name__=='__main__':main()
