#!/usr/bin/env python3
"""Stage6 real ZIL/Rust pairs: full CLI cold/warm, optional trusted semantic cache and explicit ZIL flags."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
from distribution_bench import ROOT, sha
from stage6_bench import measured

CASES=['app','stream','slices','byte_io','byte_buffer','byte_buffer_owned','resources','resource_closures','error_cleanup','owned_component','package_ui','retained_ui','update_ui','helper_ui']
INPUTS=[b'',b'!',b'D',b'S',b'U',b'O',b'M',b'E',b'T',b'L',b'0',b'1',b'2',b'3',b'12as+!-q\0\x80']

def natives(path, args=()):
    outcomes=[]
    for data in INPUTS:
        try:
            p=subprocess.run([str(path),*args],input=data,capture_output=True,timeout=10)
            outcomes.append({'stdin_hex':data.hex(),'exit':p.returncode,'stdout_hex':p.stdout.hex(),'stderr_hex':p.stderr.hex()})
        except (OSError,subprocess.TimeoutExpired) as error:
            outcomes.append({'stdin_hex':data.hex(),'error':str(error)})
    return outcomes

def valid_outcomes(outcomes):
    return isinstance(outcomes,list) and len(outcomes)==len(INPUTS) and all(
        isinstance(x,dict) and isinstance(x.get('exit'),int) and x['exit']>=0
        and 'error' not in x and 'stdout_hex' in x and 'stderr_hex' in x for x in outcomes)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    for n in ['zil','rustc','out']:p.add_argument('--'+n,type=Path,required=True)
    p.add_argument('--blocks',type=int,default=6);p.add_argument('--case',action='append',choices=CASES);p.add_argument('--closed-executable',action='store_true');p.add_argument('--mode',action='append',choices=['check','O0','O2'])
    p.add_argument('--zil-arg',action='append',default=[]);p.add_argument('--semantic-cache',action='store_true');p.add_argument('--timeout',type=int,default=180)
    a=p.parse_args()
    if a.blocks < 1 or a.timeout < 1:p.error('blocks and timeout must be positive')
    a.out.mkdir(parents=True,exist_ok=False);cpus=sorted(os.sched_getaffinity(0))[:2];os.sched_setaffinity(0,cpus)
    shutil.copy2(Path(__file__),a.out/'runner.py')
    shutil.copy2(Path(__file__).with_name('distribution_bench.py'),a.out/'distribution_bench.py')
    shutil.copy2(Path(__file__).with_name('stage6_bench.py'),a.out/'stage6_bench.py')
    cases=[c for c in CASES if not a.case or c in a.case];assert cases;modes=a.mode or ['check','O0','O2']
    manifest={'source':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'tool_sha256':sha(Path(__file__)),'runner_sha256':sha(Path(__file__).with_name('distribution_bench.py')),'binaries':{n:{'path':str(v.resolve()),'sha256':sha(v)} for n,v in [('zil',a.zil),('rust',a.rustc)]},'cases':cases,'modes':modes,'blocks':a.blocks,'cpus':cpus,'closed_executable':a.closed_executable,'zil_args':a.zil_arg,'semantic_cache':a.semantic_cache,'order':'alternating AB/BA blocks, complete cold/unchanged sequence per arm','stdin_hex':[x.hex() for x in INPUTS],'native_arguments':{'app':{'rust':['--check'],'zil':[]}},'boundary':'Complete subprocess; native outside clock. 8CGU O0/O2 overflow checks panic abort. Both arms persistent object/incremental cache per sequence. Rust source is the checked-in independent native behavior pair, not equal internal compiler work.'}
    manifest['timing_runner_sha256']=sha(Path(__file__).with_name('stage6_bench.py'))
    (a.out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');rows=[]
    for case in cases:
        source=a.out/('source-'+case);shutil.copytree(ROOT/'examples/mainline'/case,source)
        source_files={str(p.relative_to(source)):sha(p) for p in sorted(source.rglob('*')) if p.is_file()}
        (a.out/f'{case}-sources.json').write_text(json.dumps(source_files,indent=2)+'\n')
        oracle=a.out/('oracle-'+case);oracle.mkdir();cmd=[str(a.rustc.resolve()),str(source/'main.rs'),'--edition=2024','-C','overflow-checks=yes','-C','panic=abort','-C','opt-level=0','-o',str(oracle/'program')]
        result=measured(cmd,oracle,'compile',a.timeout);expected=natives(oracle/'program',['--check'] if case=='app' else []) if result['exit']==0 and not result['timeout'] else None
        oracle_valid=valid_outcomes(expected)
        (oracle/'record.json').write_text(json.dumps({'command':cmd,**result,'native':expected,'oracle_valid':oracle_valid,'native_arguments':['--check'] if case=='app' else []},indent=2)+'\n')
        for mode in modes:
            for block in range(a.blocks):
                for position,arm in enumerate(['zil','rust'] if block%2==0 else ['rust','zil']):
                    run=a.out/f'{case}-{mode}-{block}-{position}-{arm}';run.mkdir()
                    history_valid=True
                    for request in ['cold','unchanged']:
                        (run/'program').unlink(missing_ok=True)
                        if arm=='zil':
                            cmd=[str(a.zil.resolve()),'check' if mode=='check' else 'build',str(source/'main.zil'),'--json']
                            if mode!='check':
                                cmd+=['--opt',mode,'--codegen-units','8','--codegen-jobs','2','--object-cache',str(run/'objects'),'-o',str(run/'program')]
                                if a.closed_executable:cmd+=['--closed-executable']
                            if a.semantic_cache:cmd+=['--semantic-cache',str(run/'semantics')]
                            cmd+=a.zil_arg
                        else:
                            cmd=[str(a.rustc.resolve()),str(source/'main.rs'),'--edition=2024','-A','warnings','-C','overflow-checks=yes','-C','panic=abort','-C','opt-level='+('2' if mode=='O2' else '0'),'-C','codegen-units=8','-C','incremental='+str(run/'incremental'),'-o',str(run/'program')]
                            if mode=='check':cmd+=['--emit=metadata']
                        result=measured(cmd,run,request,a.timeout);row={'case':case,'mode':mode,'request':request,'block':block,'position':position,'arm':arm,'command':cmd,**result};row['valid']=result['exit']==0 and not result['timeout'] and oracle_valid
                        if arm=='zil' and not result['timeout']:
                            try:
                                report=json.loads((run/f'{request}.stdout').read_text());row['verification_runs']=report['verification_runs'];row['native_codegen']=report['native_codegen'];row['status']=report['status'];row['disk_cache']=report['disk_cache'];row['queries']=report.get('query_events');row['valid'] &= report['status']=='Success' and report['verification_runs']==1
                            except (ValueError,KeyError,TypeError) as e:row['valid']=False;row['validation_error']=str(e)
                        if row['valid'] and mode!='check':
                            row['native_arguments']=['--check'] if case=='app' and arm=='rust' else []
                            row['native']=natives(run/'program',row['native_arguments']);row['valid'] &= valid_outcomes(row['native']) and row['native']==expected
                        row['source_identity_sha256']=hashlib.sha256(json.dumps(source_files,sort_keys=True).encode()).hexdigest()
                        if mode!='check' and (run/'program').is_file():
                            row['exe_bytes']=(run/'program').stat().st_size;row['exe_sha256']=sha(run/'program')
                        row['execution_valid']=bool(row['valid']);row['sequence_history_valid']=history_valid
                        row['valid']=row['execution_valid'] and history_valid
                        history_valid=bool(row['valid'])
                        rows.append(row)
                        with (a.out/'rows.jsonl').open('a') as f:f.write(json.dumps(row)+'\n')
                        if not row['valid']:print('FAIL',case,mode,arm,request,flush=True)
                print(case,mode,block+1,'/',a.blocks,flush=True)
    cells=[]
    for case in cases:
        for mode in modes:
            for request in ['cold','unchanged']:
                arms={}
                for arm in ['zil','rust']:
                    selected=[r for r in rows if (r['case'],r['mode'],r['request'],r['arm'])==(case,mode,request,arm)]
                    arms[arm]={'median_ms':statistics.median(r['elapsed_ns']/1e6 for r in selected),'failures':sum(not r['valid'] for r in selected),'values_ms':[r['elapsed_ns']/1e6 for r in selected]}
                cells.append({'case':case,'mode':mode,'request':request,'arms':arms,'zil_over_rust':arms['zil']['median_ms']/arms['rust']['median_ms'] if not any(v['failures'] for v in arms.values()) else None})
    (a.out/'summary.json').write_text(json.dumps({'rows':len(rows),'cells':cells},indent=2)+'\n');print('complete',len(rows),flush=True)
if __name__=='__main__':main()
