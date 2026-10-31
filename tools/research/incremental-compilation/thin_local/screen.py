#!/usr/bin/env python3
"""Paired Clang/Thin full-service screen; default and closed are separate runs."""
import argparse,hashlib,json,os,shutil,statistics,subprocess,sys
from pathlib import Path

here=Path(__file__).resolve().parent;sys.path.insert(0,str(here.parent))
from stage6_bench import generate,shapes,measured,REQUESTS
from run_request import identity

def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
 p=argparse.ArgumentParser(description=__doc__)
 for name in ['zil','clang','lld','snapshot-receipt','out']:p.add_argument('--'+name,type=Path,required=True)
 p.add_argument('--blocks',type=int,default=2);p.add_argument('--case',action='append');p.add_argument('--closed',action='store_true');p.add_argument('--dso-local',action='store_true');p.add_argument('--imports',choices=['off','on'],default='off');p.add_argument('--timeout',type=int,default=240)
 a=p.parse_args();a.out=a.out.resolve();a.out.mkdir(parents=True,exist_ok=False)
 cpus=sorted(os.sched_getaffinity(0))[:2];os.sched_setaffinity(0,cpus)
 names=a.case or ['large-128','large-512','large-2048','large-4096','large-8192','large-even-4x2048','large-skew-8','body-64x16','branch-64x16']
 cases=[c for c in shapes() if c['name'] in names];assert set(names)=={c['name'] for c in cases}
 for path in [Path(__file__),here/'run_request.py',here/'clang_wrapper.py',here.parent/'stage6_bench.py']:shutil.copy2(path,a.out/path.name)
 manifest={'scope':__doc__,'source_cases':cases,'blocks':a.blocks,'cpus':cpus,'requests':REQUESTS,'closed':a.closed,'dso_local':a.dso_local,'imports':a.imports,'tool_identities':{k:identity(getattr(a,k)) for k in ['zil','clang','lld']},'snapshot_sha256':sha(a.snapshot_receipt),'scripts':{name:sha(a.out/name) for name in ['screen.py','run_request.py','clang_wrapper.py','stage6_bench.py']},'primary_time':'compile_service_elapsed_ns includes per-request tool hashing, source driver, current Verify/preflight, serialization, compile, Thin index/cache/link, audit and transactional publication; excludes native. Outer elapsed includes native and Python process overhead. Both recorded. No API-vs-Clang gain attribution.'}
 (a.out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');rows=[]
 for case in cases:
  source=a.out/('source-'+case['name'])
  for block in range(a.blocks):
   for position,mode in enumerate(['baseline','thin'] if block%2==0 else ['thin','baseline']):
    sequence=a.out/f'{case["name"]}-{block}-{position}-{mode}';sequence.mkdir();cache=sequence/'cache'
    for label,edit in REQUESTS:
     expected=generate(case,source,edit).to_bytes(4,'little');dest=sequence/label
     source_identity={f.name:sha(f) for f in sorted(source.glob('*.zil'))}
     (sequence/(label+'.source.json')).write_text(json.dumps(source_identity,indent=2)+'\n')
     cmd=[sys.executable,str(here/'run_request.py'),'--zil',str(a.zil.resolve()),'--clang',str(a.clang.absolute()),'--lld',str(a.lld.absolute()),'--snapshot-receipt',str(a.snapshot_receipt.resolve()),'--source',str(source/'main.zil'),'--out',str(dest),'--cache',str(cache),'--mode',mode,'--imports',a.imports]
     if a.closed:cmd+=['--closed']
     if a.dso_local:cmd+=['--dso-local']
     outer=measured(cmd,sequence,label,a.timeout)
     row={'case':case['name'],'block':block,'position':position,'mode':mode,'request':label,'expected_hex':expected.hex(),'source_identity_sha256':hashlib.sha256(json.dumps(source_identity,sort_keys=True).encode()).hexdigest(),'command':cmd,'outer':outer,'valid':False}
     try:
      result=json.loads((dest/'result.json').read_text());report=json.loads((dest/'driver.stdout').read_text());native=result.get('native',{})
      row.update(compile_service_elapsed_ns=result['compile_service_elapsed_ns'],driver_elapsed_ns=result['elapsed_ns'],driver_status=report.get('status'),verification_runs=report.get('verification_runs'),native=native,pre_cache_committed=result['pre_cache_committed'],post_cache_committed=result['post_cache_committed'],post_corrupt_skipped=result['post_corrupt_skipped'])
      row['valid']=outer['exit']==0 and not outer['timeout'] and result['driver_success'] and report.get('verification_runs')==1 and native.get('exit')==0 and native.get('stdout_hex')==expected.hex() and native.get('stderr_hex')==''
      units=[json.loads(f.read_text()) for f in sorted(dest.glob('unit-*.input.json'))];row.update(actual_units=len(units),pre_hits=sum(u['pre_cache_hit'] for u in units),actual_pre_keys=[u['action_key'] for u in units],actual_post_keys_before=sorted(k for k in result['post_before'] if k.startswith('llvmcache-')),actual_post_keys_after=sorted(k for k in result['post_after'] if k.startswith('llvmcache-')))
      row['post_new_keys']=sorted(set(row['actual_post_keys_after'])-set(row['actual_post_keys_before']))
      if row['valid']:
       exe=dest/'program';row['executable_bytes']=exe.stat().st_size
       size=subprocess.check_output(['size','--format=berkeley',str(exe)],text=True);(dest/'size.txt').write_text(size)
       values=size.splitlines()[-1].split();row['text_bytes']=int(values[0]);row['data_bytes']=int(values[1]);row['bss_bytes']=int(values[2])
       symbols=subprocess.check_output(['nm','-g','--defined-only',str(exe)],text=True);dynamic=subprocess.check_output(['nm','-D','--defined-only',str(exe)],text=True);(dest/'symbols.txt').write_text(symbols);(dest/'dynamic-symbols.txt').write_text(dynamic)
       row['global_zil_function_defs']=sum(' T zil.fn.' in line for line in symbols.splitlines());exports=set((dest/'units/exports.txt').read_text().splitlines());dynamic_names={line.split()[-1] for line in dynamic.splitlines() if line.split()}
       row['export_roots_preserved']=exports<=dynamic_names;row['valid'] &= row['export_roots_preserved']
     except (OSError,ValueError,KeyError,TypeError,subprocess.SubprocessError) as error:row['validation_error']=str(error)
     rows.append(row)
     with (a.out/'rows.jsonl').open('a') as f:f.write(json.dumps(row)+'\n')
     if not row['valid']:print('FAIL',case['name'],block,mode,label,flush=True)
    print(case['name'],block+1,'/',a.blocks,mode,flush=True)
 summary=[]
 for case in cases:
  for label,_ in REQUESTS:
   selected=[r for r in rows if r['case']==case['name'] and r['request']==label];arms={}
   for mode in ['baseline','thin']:
    rs=[r for r in selected if r['mode']==mode];valid=[r for r in rs if r['valid']]
    arms[mode]={'failures':len(rs)-len(valid),'full_service_ms':[r['compile_service_elapsed_ns']/1e6 for r in valid],'outer_wall_ms':[r['outer']['elapsed_ns']/1e6 for r in valid],'driver_ms':[r['driver_elapsed_ns']/1e6 for r in valid],'text_bytes':[r['text_bytes'] for r in valid],'executable_bytes':[r['executable_bytes'] for r in valid],'actual_units':[r['actual_units'] for r in valid],'pre_hits':[r['pre_hits'] for r in valid],'new_post_keys':[len(r['post_new_keys']) for r in valid]}
   ratio=statistics.median(arms['thin']['full_service_ms'])/statistics.median(arms['baseline']['full_service_ms']) if all(not arm['failures'] and arm['full_service_ms'] for arm in arms.values()) else None
   summary.append({'case':case['name'],'request':label,'arms':arms,'thin_over_baseline_full_service':ratio})
 (a.out/'summary.json').write_text(json.dumps({'rows':len(rows),'failures':sum(not r['valid'] for r in rows),'cells':summary},indent=2)+'\n');print('complete',len(rows),flush=True)
if __name__=='__main__':main()
