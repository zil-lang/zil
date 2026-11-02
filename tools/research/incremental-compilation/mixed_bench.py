#!/usr/bin/env python3
"""Scoped existing-source O0/O2 CLI A/B check; no product-gate claim."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import time

CASES = [
    ('byte_io/main.zil', b'hello, Zil!\n\0\x80'),
    ('resource_closures/main.zil', b'D'),
    ('generic_store178/main.zil', b'2'),
    ('mixed_capture182/main.zil', b'2'),
    ('ct_selection219/main.zil', b'2'),
    ('ct_types228/arrays_defaults.zil', b''),
    ('where_copy229/main.zil', b''),
    ('shared_product224/main.zil', b''),
]
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['baseline','candidate','out']: p.add_argument('--'+name,type=Path,required=True)
    a=p.parse_args();a.out.mkdir(parents=True,exist_ok=False)
    compilers={name:getattr(a,name).resolve() for name in ['baseline','candidate']}
    cpus=sorted(os.sched_getaffinity(0))[:2];os.sched_setaffinity(0,cpus)
    sources=Path('examples/mainline').resolve()
    manifest={'scope':__doc__,'blocks':5,'cpus':cpus,'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
              'compilers':{name:{'path':str(path),'sha256':sha(path)} for name,path in compilers.items()},
              'cases':[{'path':name,'stdin_hex':data.hex(),'files':{str(path.relative_to(sources)):sha(path) for path in sorted((sources/name).parent.glob('*.zil'))}} for name,data in CASES],
              'flags':['build','--opt','O0/O2','--json'],'timing':'full subprocess wall; native executions outside timer; no object cache; median paired block means'}
    (a.out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    rows=[];expected={}
    # Use both optimization levels of the independently constructed text path
    # as the native oracle. A known-wrong old API result is retained, but never
    # included in a semantically equivalent performance ratio.
    for index,(source,data) in enumerate(CASES):
        for opt in ['O0','O2']:
            run=a.out/f'oracle-{index}-{opt}';run.mkdir()
            cmd=[str(compilers['candidate']),'build',str(sources/source),'--backend','clang','--opt',opt,'--json','-o',str(run/'program')]
            result=subprocess.run(cmd,capture_output=True,timeout=180)
            (run/'report.json').write_bytes(result.stdout);(run/'stderr').write_bytes(result.stderr)
            assert result.returncode==0,(cmd,result.stderr)
            report=json.loads(result.stdout);assert report['status']=='Success' and report['verification_runs']==1
            native=subprocess.run([str(run/'program')],input=data,capture_output=True,cwd=run,timeout=10)
            outcome={'exit':native.returncode,'stdout_hex':native.stdout.hex(),'stderr_hex':native.stderr.hex()}
            if source not in expected:expected[source]=outcome
            assert outcome==expected[source],(source,opt,outcome,expected[source])
            (run/'native.json').write_text(json.dumps(outcome)+'\n')
    for index,(source,data) in enumerate(CASES):
        for opt in ['O0','O2']:
            for block in range(5):
                for pos,arm in enumerate(['baseline','candidate','candidate','baseline'] if block%2==0 else ['candidate','baseline','baseline','candidate']):
                    run=a.out/f'{index}-{opt}-{block}-{pos}-{arm}';run.mkdir()
                    cmd=[str(compilers[arm]),'build',str(sources/source),'--opt',opt,'--json','-o',str(run/'program')]
                    start=time.perf_counter_ns();result=subprocess.run(cmd,capture_output=True,timeout=180);elapsed=time.perf_counter_ns()-start
                    (run/'report.json').write_bytes(result.stdout);(run/'stderr').write_bytes(result.stderr)
                    assert result.returncode==0,(cmd,result.stderr)
                    report=json.loads(result.stdout);assert report['status']=='Success' and report['verification_runs']==1
                    native=subprocess.run([str(run/'program')],input=data,capture_output=True,cwd=run,timeout=10)
                    outcome={'exit':native.returncode,'stdout_hex':native.stdout.hex(),'stderr_hex':native.stderr.hex()}
                    correct=outcome==expected[source]
                    if arm=='candidate':assert correct,(source,arm,opt,outcome,expected[source])
                    (run/'native.json').write_text(json.dumps(outcome)+'\n')
                    row={'case':source,'opt':opt,'block':block,'position':pos,'arm':arm,'elapsed_ns':elapsed,'command':cmd,'native':outcome,'matches_oracle':correct}
                    rows.append(row)
                    with (a.out/'rows.jsonl').open('a') as f:f.write(json.dumps(row)+'\n')
            print(source,opt,'complete',flush=True)
    cells=[]
    for case,_ in CASES:
        for opt in ['O0','O2']:
            arms={arm:statistics.median(statistics.mean(r['elapsed_ns'] for r in rows if r['case']==case and r['opt']==opt and r['arm']==arm and r['block']==b) for b in range(5))/1e6 for arm in compilers}
            valid=all(r['matches_oracle'] for r in rows if r['case']==case and r['opt']==opt)
            cells.append({'case':case,'opt':opt,'median_block_mean_ms':arms,'candidate_over_baseline':arms['candidate']/arms['baseline'] if valid else None,'semantically_equivalent':valid,'baseline_wrong_results':sum(not r['matches_oracle'] for r in rows if r['case']==case and r['opt']==opt and r['arm']=='baseline')})
    (a.out/'summary.json').write_text(json.dumps({'requests':len(rows),'native_checks':len(rows)+2*len(CASES),'oracle_builds':2*len(CASES),'cells':cells},indent=2)+'\n')
    print(f'Complete: {len(rows)} builds and exact native controls',flush=True)
if __name__=='__main__':main()
