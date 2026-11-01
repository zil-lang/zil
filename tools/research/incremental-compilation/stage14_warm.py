#!/usr/bin/env python3
"""Repeated warm CLI requests after a validated edited cache history.

No new cold/O2 Rust compilation. An optional separate Rust seed reuses an earlier
validated edited history while collecting entirely new warm observations. Reuses the current edited sources/caches in the
specified seed window, rotates arm order, verifies current ZIL Verify/Analyze
reuse, and runs the existing native oracle after each request. The five pairs
share one seeded history per arm and are not independent cold histories.
Original edited ELF/LLVM artifacts are restored after sampling, so prior quality
inputs remain exact. Each newly emitted binary hash is recorded before restore.
"""
import argparse, hashlib, json, math, os, signal, statistics, subprocess, time, shutil
from pathlib import Path
def measured(cmd, run, label, timeout, env):
    def expired(_signum, _frame):raise TimeoutError('compiler timeout')
    prior_handler=signal.getsignal(signal.SIGALRM)
    start=time.perf_counter_ns();p=None;reaped=False;timed_out=False;status=None
    try:
        with (run/f'{label}.stdout').open('wb') as stdout,(run/f'{label}.stderr').open('wb') as stderr:
            signal.signal(signal.SIGALRM,expired)
            start=time.perf_counter_ns()
            try:p=subprocess.Popen(cmd,stdout=stdout,stderr=stderr,env=env,start_new_session=True,stdin=subprocess.DEVNULL)
            except OSError as error:
                return {'elapsed_ns':time.perf_counter_ns()-start,'exit':None,'timeout':False,'launch_error':str(error),'cpu_seconds':None,'max_rss_kib':None}
            signal.alarm(timeout)
            try:_,status,usage=os.wait4(p.pid,0)
            except TimeoutError:
                timed_out=True
                try:os.killpg(p.pid,signal.SIGKILL)
                except ProcessLookupError:pass
                _,status,usage=os.wait4(p.pid,0)
            reaped=True;p.returncode=os.waitstatus_to_exitcode(status)
            wall=time.perf_counter_ns()-start
    except BaseException as error:
        signal.alarm(0)
        cleanup_attempted=False
        if p is not None and not reaped:
            cleanup_attempted=True
            try:os.killpg(p.pid,signal.SIGKILL)
            except ProcessLookupError:pass
            try:
                _,status,usage=os.wait4(p.pid,0)
            except ChildProcessError:
                # A signal can arrive after wait4 reaped the child but before
                # the Python assignment records its status. Preserve the error.
                pass
            reaped=True
            if status is not None:p.returncode=os.waitstatus_to_exitcode(status)
        interrupted={'status':'Interrupted','command':cmd,'pid':p.pid if p is not None else None,
                     'exit':p.returncode if p is not None else None,'error':repr(error),
                     'elapsed_ns':time.perf_counter_ns()-start,
                     'process_group_cleanup_attempted':cleanup_attempted,'child_reaped':reaped}
        try:(run/f'{label}.interrupted.json').write_text(json.dumps(interrupted,indent=2)+'\n')
        except OSError as receipt_error:error.add_note('Could not write interruption receipt: '+str(receipt_error))
        raise
    finally:
        signal.alarm(0)
        signal.signal(signal.SIGALRM,prior_handler)
    return {'elapsed_ns':wall,'exit':p.returncode,'timeout':timed_out,'cpu_seconds':usage.ru_utime+usage.ru_stime,'max_rss_kib':usage.ru_maxrss}

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('seed',type=Path)
p.add_argument('--out',type=Path,required=True)
p.add_argument('--rust-seed',type=Path,help='existing validated Rust edited history; timings are never reused')
p.add_argument('--reps',type=int,default=5)
p.add_argument('--case',default='opaque-2048-branch')
a=p.parse_args();assert a.reps>0
a.seed=a.seed.resolve();a.out=a.out.resolve();a.out.mkdir(parents=True,exist_ok=False)
manifest=json.loads((a.seed/'manifest.json').read_text())
records=[json.loads(line) for line in (a.seed/'rows.jsonl').read_text().splitlines()]
seed={r['arm']:r for r in records if r['case']==a.case and r['contract']=='default' and r['request']=='edit' and r['rep']==0}
rust_seed=None
if a.rust_seed:
 a.rust_seed=a.rust_seed.resolve()
 rust_seed=json.loads((a.rust_seed/'manifest.json').read_text())
 rust_rows=[json.loads(line) for line in (a.rust_seed/'rows.jsonl').read_text().splitlines()]
 selected=[r for r in rust_rows if r['case']==a.case and r['contract']=='default' and r['request']=='edit' and r['rep']==0 and r['arm']=='rust']
 assert len(selected)==1 and 'rust' not in seed
 assert rust_seed['cpus']==manifest['cpus'] and rust_seed['library']==manifest['library']
 seed['rust']=selected[0]
 requested_rust=Path(shutil.which('rustc'))
 assert hashlib.sha256(requested_rust.read_bytes()).hexdigest()==rust_seed['arms']['rust']['sha256']
 assert subprocess.check_output([str(requested_rust),'-vV'],text=True)==rust_seed['rustc_verbose']
 if manifest.get('rustc_verbose') is not None:assert manifest['rustc_verbose']==rust_seed['rustc_verbose']
 manifest['arms']['rust']=rust_seed['arms']['rust']
 manifest['rustc_verbose']=rust_seed['rustc_verbose']
assert set(seed)=={'baseline','candidate','rust'} and all(r['valid'] and r['native_valid'] for r in seed.values())
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
snapshots={};programs={};expected={};linkers={}
for arm,r in seed.items():
 assert sha(r['command'][0])==manifest['arms'][arm]['sha256']
 cmd=r['command'];program=Path(cmd[cmd.index('-o')+1]);programs[arm]=program
 expected[arm]=(program.parent/'edit.native.stdout').read_bytes()
 paths=[program]
 if arm!='rust':
  report=json.loads((program.parent/'edit.stdout').read_text());paths.append(Path(report['artifacts']['llvm']))
  linkers[arm]=Path(report['commands'][0][0]).resolve()
 for path in paths:snapshots[path]=(path.read_bytes(),path.stat().st_mode)
assert len(set(expected.values()))==1
if rust_seed is not None:
 rust_source=Path(seed['rust']['command'][1])
 assert rust_source.read_bytes()==(programs['candidate'].parent/'src/main.rs').read_bytes(), 'Rust seed must use exactly the current generated source'
assert len(set(linkers.values()))==1
clang=next(iter(linkers.values()));assert clang.is_file()
cpus=sorted(os.sched_getaffinity(0));assert cpus==manifest['cpus']
source_paths={Path(value) for r in seed.values() for value in r['command'] if str(value).endswith(('.rs','.zil'))}
source_hashes={str(path):sha(path) for source in source_paths for path in source.parent.iterdir() if path.suffix in {'.rs','.zil'}}
assert sha(manifest['library']['path'])==manifest['library']['sha256']
metadata={'scope':__doc__,'cpus':cpus,'clang':{'path':str(clang),'sha256':sha(clang)},'seed':str(a.seed),'seed_manifest_sha256':sha(a.seed/'manifest.json'),'production_source':manifest['source'],'runner_sha256':sha(__file__),'case':a.case,'reps':a.reps,'arms':manifest['arms'],'source_hashes':source_hashes,'seed_artifacts':{str(path):hashlib.sha256(data).hexdigest() for path,(data,_) in snapshots.items()},'library':manifest['library'],'rustc_verbose':manifest['rustc_verbose']}
metadata['requested_rust_path']=str(requested_rust) if rust_seed else None
metadata['rust_seed']=str(a.rust_seed) if a.rust_seed else None
metadata['rust_seed_manifest_sha256']=sha(a.rust_seed/'manifest.json') if a.rust_seed else None
metadata['rust_seed_scope']='earlier edited cache history, same source/compiler/affinity, newly measured warm requests only' if rust_seed else 'same seed window'
(a.out/'manifest.json').write_text(json.dumps(metadata,indent=2)+'\n')
env=dict(os.environ,LD_LIBRARY_PATH=str(Path(manifest['library']['path']).parent),ZIL_CLANG=str(clang))
rows=[]
try:
 for rep in range(a.reps):
  arms=list(seed);shift=rep%len(arms);arms=arms[shift:]+arms[:shift]
  for arm in arms:
   r=seed[arm];label=f'{rep}-{arm}';runenv=os.environ.copy() if arm=='rust' else env
   row=measured(r['command'],a.out,label,240,runenv)
   row.update(rep=rep,arm=arm,case=a.case,request='warm-after-edit',command=r['command'])
   row['valid']=row['exit']==0 and not row['timeout'];row['seconds']=row.pop('elapsed_ns')/1e9
   try:
    if row['valid'] and arm!='rust':
     report=json.loads((a.out/(label+'.stdout')).read_text())
     row['verification_runs']=report['verification_runs'];row['query_events']=report['query_events'];row['object_cache']=report['native_codegen']['object_cache']
     row['valid'] &= report['status']=='Success' and row['verification_runs']==1 and any(e=={'phase':'analyze','disposition':'reused'} for e in report['query_events']) and row['object_cache']['enabled'] and row['object_cache']['hits']==report['native_codegen']['actual_units']==8 and row['object_cache']['misses']==0
    if row['valid']:
     native=subprocess.run([str(programs[arm])],input=bytes([7]),capture_output=True,timeout=10)
     (a.out/(label+'.native.stdout')).write_bytes(native.stdout);(a.out/(label+'.native.stderr')).write_bytes(native.stderr)
     row['native_valid']=native.returncode==0 and not native.stderr and native.stdout==expected[arm]
     row['program_sha256']=sha(programs[arm]);row['byte_equal_to_seed']=row['program_sha256']==metadata['seed_artifacts'][str(programs[arm])]
     row['valid'] &= row['native_valid']
   except Exception as error:
    row.update(valid=False,error=repr(error))
   rows.append(row)
   with (a.out/'rows.jsonl').open('a') as output:output.write(json.dumps(row)+'\n')
   print(rep,arm,round(row['seconds']*1000,3),row['valid'],flush=True)
   assert row['valid'], 'invalid request retained; no aggregate claim'
finally:
 for path,(data,mode) in snapshots.items():path.write_bytes(data);path.chmod(mode)
 restored={str(path):sha(path)==metadata['seed_artifacts'][str(path)] for path in snapshots}
 (a.out/'restored.json').write_text(json.dumps(restored,indent=2)+'\n')
assert all(restored.values()) and all(sha(path)==digest for path,digest in source_hashes.items())
lookup={(r['rep'],r['arm']):r for r in rows};assert len(lookup)==3*a.reps
summary={'requests':len(rows),'valid':sum(r['valid'] for r in rows),'repeated_pairs':a.reps,'cache_histories_per_arm':1,'medians_ms':{arm:statistics.median(r['seconds']*1000 for r in rows if r['arm']==arm) for arm in seed},'pairs':{other:[lookup[rep,'candidate']['seconds']/lookup[rep,other]['seconds'] for rep in range(a.reps)] for other in ['baseline','rust']}}
summary['geomean_ratios']={arm:math.exp(statistics.mean(map(math.log,ratios))) for arm,ratios in summary['pairs'].items()}
(a.out/'analysis.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps(summary),flush=True)
