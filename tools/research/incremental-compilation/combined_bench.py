#!/usr/bin/env python3
"""Frozen scoped CLI + Session A/B screen, optionally direct rustc reference.

Not the formal product gate. Includes per-process startup and actual source reads
for CLI, current Verify for every successful ZIL request, link/publication for
build. Native semantic checks run outside timing. CPU affinity is fixed. The
/usr/bin/time wrapper is included in CLI wall time for all arms.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sources(root, modules, functions, edit):
    root.mkdir(exist_ok=True, parents=True)
    zil = ''.join(f'import m{i}\n' for i in range(1, modules))
    zil += 'fn value()->i32{var total:i32=0;\n'
    zil += ''.join(f'total=total+m{i}.value();\n' for i in range(1, modules))
    zil += 'return total;}\n'
    rust = ''.join(f'mod m{i};\n' for i in range(1, modules))
    rust += 'fn main(){let mut total:i32=0;\n'
    rust += ''.join(f'total=total+m{i}::value();\n' for i in range(1, modules))
    rust += 'println!("{}",total);}\n'
    (root/'main.zil').write_text(zil)
    (root/'main.rs').write_text(rust)
    for i in range(1, modules):
        suffix = f'+{edit}' if i == 1 and edit else ''
        body = f'fn h0()->i32{{return {i}{suffix};}}\n'
        body += ''.join(f'fn h{j}()->i32{{return h{j-1}();}}\n' for j in range(1, functions-1))
        body += f'pub fn value()->i32{{return h{functions-2}();}}\n'
        (root/f'm{i}.zil').write_text(body)
        (root/f'm{i}.rs').write_text(body)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for arg in ['baseline', 'candidate', 'baseline-probe', 'candidate-probe']:
        p.add_argument('--'+arg, required=True, type=Path)
    p.add_argument('--rustc', type=Path)
    p.add_argument('--out', required=True, type=Path)
    p.add_argument('--blocks', type=int, default=5)
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=False)
    cpus = sorted(os.sched_getaffinity(0))[:2]
    os.sched_setaffinity(0, cpus)
    compilers = {k: getattr(a, k).resolve() for k in ['baseline', 'candidate']}
    if a.rustc:
        compilers['rust'] = a.rustc.resolve()
    probes = {k: getattr(a, k+'_probe').resolve() for k in ['baseline', 'candidate']}
    manifest = {'scope': __doc__, 'baseline_source': '5d7c1eeb2b74052eb239e19529dc5a943c6622a3',
                'candidate_source': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
                'compilers': {k: {'path': str(v), 'sha256': sha(v)} for k, v in compilers.items()},
                'probes': {k: {'path': str(v), 'sha256': sha(v)} for k, v in probes.items()},
                'cpus': cpus, 'blocks': a.blocks, 'cli_modules': [32, 128], 'functions_per_leaf': 8,
                'session_cases': [[32,32,'check'], [128,32,'check'], [512,32,'check'],
                                  [128,8192,'check'], [32,32,'build'], [128,32,'build']],
                'requests': ['cold','unchanged','private1','private2','restore'],
                'rust_reference': 'direct rustc, O0, overflow checks, 8 CGU, persistent -C incremental per run; metadata check is not Cargo check',
                'measurement': 'median of within-block paired means; observed per-run RSS; no OS-cache flush or thermal reset'}
    (a.out/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    rows=[]
    def record(row):
        rows.append(row)
        with (a.out/'rows.jsonl').open('a') as f: f.write(json.dumps(row)+'\n')
    for modules in manifest['cli_modules']:
        source=a.out/f'source-{modules}'
        for mode in ['check','build']:
            for block in range(a.blocks):
                arms=['baseline','candidate','candidate','baseline'] if block%2==0 else ['candidate','baseline','baseline','candidate']
                if a.rustc:
                    arms = (['rust']+arms+['rust']) if block%2==0 else (arms[:2]+['rust','rust']+arms[2:])
                for position,arm in enumerate(arms):
                    run=a.out/f'cli-{modules}-{mode}-{block}-{position}-{arm}'
                    run.mkdir()
                    for label,edit in zip(manifest['requests'],[0,0,1,2,0]):
                        sources(source,modules,8,edit)
                        if arm=='rust':
                            cmd=[str(compilers[arm]),str(source/'main.rs'),'--edition=2024','-C','opt-level=0','-C','overflow-checks=yes','-C','codegen-units=8','-C',f'incremental={run/"incremental"}','-o',str(run/'program')]
                            if mode=='check': cmd+=['--emit=metadata']
                        else:
                            cmd=[str(compilers[arm]),mode,str(source/'main.zil'),'--json']
                            if mode=='build':
                                cmd+=['--entry','value','--opt','O0','--codegen-units','8','--codegen-jobs','2','--object-cache',str(run/'objects'),'-o',str(run/'program')]
                        resource=run/f'{label}.resources.json'
                        timed=['/usr/bin/time','-f','{"max_rss_kib":%M,"user_seconds":%U,"system_seconds":%S}','-o',str(resource),*cmd]
                        started=time.perf_counter_ns()
                        result=subprocess.run(timed,capture_output=True,timeout=180)
                        elapsed=time.perf_counter_ns()-started
                        (run/f'{label}.stdout').write_bytes(result.stdout)
                        (run/f'{label}.stderr').write_bytes(result.stderr)
                        assert result.returncode==0,(cmd,result.stderr.decode()[:3000])
                        row={'interface':'cli','modules':modules,'mode':mode,'block':block,'position':position,'arm':arm,'request':label,'elapsed_ns':elapsed,'resources':json.loads(resource.read_text()),'command':cmd,'source_sha256':sha(source/('main.rs' if arm=='rust' else 'main.zil')),'edited_sha256':sha(source/('m1.rs' if arm=='rust' else 'm1.zil'))}
                        if arm!='rust':
                            report=json.loads(result.stdout)
                            assert report['status']=='Success' and report['verification_runs']==1,report
                            row['verification_runs']=report['verification_runs']
                            row['native_codegen']=report['native_codegen']
                        if mode=='build':
                            native=subprocess.run([str(run/'program')],capture_output=True,timeout=10)
                            assert native.returncode==0 and native.stdout==f'{sum(range(1,modules))+edit}\n'.encode(),native
                            (run/f'{label}.native').write_bytes(native.stdout)
                            row['elf_sha256']=sha(run/'program')
                        record(row)
                print(f'CLI modules={modules} {mode} block={block+1}/{a.blocks}',flush=True)
    for modules,capacity,mode in manifest['session_cases']:
        for block in range(a.blocks):
            arms=['baseline','candidate','candidate','baseline'] if block%2==0 else ['candidate','baseline','baseline','candidate']
            for position,arm in enumerate(arms):
                run=a.out/f'session-{modules}-{capacity}-{mode}-{block}-{position}-{arm}'
                cmd=[str(probes[arm]),str(modules),'8',str(capacity),'5',str(run),mode]
                result=subprocess.run(cmd,capture_output=True,text=True,timeout=180)
                run.mkdir(exist_ok=True)
                (run/'stdout.jsonl').write_text(result.stdout)
                (run/'stderr').write_text(result.stderr)
                assert result.returncode==0,(cmd,result.stderr[:3000])
                for probe in map(json.loads,result.stdout.splitlines()):
                    record({'interface':'session','mode':mode,'block':block,'position':position,'arm':arm,**probe})
            print(f'Session modules={modules} capacity={capacity} {mode} block={block+1}/{a.blocks}',flush=True)
    summary=[]
    groups=sorted({(r['interface'],r['modules'],r['mode'],r.get('capacity',0),r['request']) for r in rows})
    for interface,modules,mode,capacity,request in groups:
        selected=[r for r in rows if (r['interface'],r['modules'],r['mode'],r.get('capacity',0),r['request'])==(interface,modules,mode,capacity,request)]
        cells={}
        for arm in sorted({r['arm'] for r in selected}):
            matches=[r for r in selected if r['arm']==arm]
            means=[statistics.mean(r['elapsed_ns'] for r in matches if r['block']==b) for b in range(a.blocks)]
            walls=sorted(r['elapsed_ns'] for r in matches)
            memory=[r.get('rss_kib',r.get('resources',{}).get('max_rss_kib')) for r in matches]
            cells[arm]={'median_block_mean_ms':statistics.median(means)/1e6,'observed_p95_ms':walls[min(len(walls)-1,int(.95*len(walls)))]/1e6,'median_rss_kib':statistics.median(memory)}
        summary.append({'interface':interface,'modules':modules,'mode':mode,'capacity':capacity,'request':request,'arms':cells,'candidate_over_baseline':cells['candidate']['median_block_mean_ms']/cells['baseline']['median_block_mean_ms'],**({'candidate_over_rust':cells['candidate']['median_block_mean_ms']/cells['rust']['median_block_mean_ms']} if 'rust' in cells else {})})
    (a.out/'summary.json').write_text(json.dumps({'rows':len(rows),'cells':summary},indent=2)+'\n')
    print(f'Complete: {len(rows)} measured requests',flush=True)

if __name__=='__main__': main()
