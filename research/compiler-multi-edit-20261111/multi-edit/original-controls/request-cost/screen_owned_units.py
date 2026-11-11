#!/usr/bin/env python3
"""Fixed 84-request screen of compiler-owned machine-unit lifetimes.

Only Python stdlib. `prepare` reads and freezes the prior primary corpus/model;
`run` executes that one plan once. This is not the full weighted protocol.
"""
from __future__ import annotations
import argparse, collections, datetime, hashlib, importlib.util, json, os
from pathlib import Path
import random, shutil, signal, statistics, subprocess, sys, time

CASES = [('shared4096', 4096), ('shared1024', 1024), ('owned8', 8)]
INPUTS = ['01000001', '1d020002', 'fb050003']
LANES = [('shared4096', 2, 7, 3), ('shared1024', 2, 7, 3),
         ('owned8', 2, 7, 3), ('shared4096', 1, 3, 1)]
SCHEMA = 'owned-machine-unit-lifetime-screen-v1'
DEFAULT_ROOT = Path('/workspace/scratch/beabdcf5ebf4')

def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def save(path, value): Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2)+'\n')
def utc(): return datetime.datetime.now(datetime.timezone.utc).isoformat()
def load_model(path):
    spec = importlib.util.spec_from_file_location('frozen_screen_corpus', path)
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    return module

def prepare(a):
    plan_path = a.plan.resolve()
    if plan_path.exists(): raise SystemExit('Refuse to overwrite a frozen plan')
    out = a.out.resolve()
    if out.exists(): raise SystemExit('Output already exists')
    cpus = [int(v) for v in a.cpu.split(',')]
    if len(cpus) != 2 or len(set(cpus)) != 2 or not set(cpus) <= os.sched_getaffinity(0):
        raise SystemExit('Exactly two available fixed CPUs are required')
    tools = {name: {'path':str(path.resolve()), 'sha256':sha(path)}
             for name,path in [('base',a.base),('cand',a.candidate),('clang',a.clang)]}
    model_path = a.model.resolve(); model = load_model(model_path)
    sources = {}; oracles = {}
    for case,n in CASES:
        sources[case] = {}; oracles[case] = {}
        for snapshot in ['base','private']:
            path = (a.corpus/case/snapshot/'zil/main.zil').resolve()
            sources[case][snapshot] = {'path':str(path),'sha256':sha(path),'bytes':path.stat().st_size}
            oracles[case][snapshot] = {value:model.expected(case,n,snapshot,bytes.fromhex(value)).hex() for value in INPUTS}
    rng = random.Random(a.seed); first_flip = rng.randrange(2); position = 0; pairs = []
    for case,jobs,warm,edit_warm in LANES:
        requests = [('cold','base',0)] + [('warm','base',i) for i in range(warm)]
        requests += [('private','private',0)] + [('edit-warm','private',i) for i in range(edit_warm)]
        for request,snapshot,rep in requests:
            arms = ['base','cand'] if (position+first_flip)%2 == 0 else ['cand','base']
            pairs.append({'pair':position,'case':case,'jobs':jobs,'request':request,
                          'snapshot':snapshot,'repetition':rep,'arms':arms})
            position += 1
    assert len(pairs) == 42 and sum(len(p['arms']) for p in pairs) == 84
    plan = {'schema':SCHEMA,'created_utc':utc(),'script':{'path':str(Path(__file__).resolve()),'sha256':sha(__file__)},
            'candidate_source':a.candidate_source,'baseline_source':a.baseline_source,
            'tools':tools,'model':{'path':str(model_path),'sha256':sha(model_path)},
            'sources':sources,'oracles':oracles,'inputs_hex':INPUTS,'cpus':cpus,
            'seed':a.seed,'out':str(out),'pairs':pairs,'compile_requests':84,
            'configuration':{'backend':'llvm-api','opt':'O2','codegen_units':8,
                             'incremental_o2':True,'function_merging':'after',
                             'semantic_cache':True,'object_cache':True,
                             'boundary':'fresh CLI process through complete output/publication; OS caches retained'},
            'screen_decision':{'all_84_requests_required':True,'current_verification_runs':1,
                'complete_elf_equality_required':True,'sidecar':'preserve raw; require exact equality except the single verified-fingerprint comment when raw differs',
                'three_short_output_oracles_required':True,'cache_topology_parity_required':True,
                'compile_regression_warning_ratio':1.10,
                'interpretation':'fixed screen only; retain all values and any regression; no repeated runtime timing, Rust arm, weighted aggregate, or automatic adoption'},
            'timeout_seconds':300,'prior_failures':'All prior protocol results remain unchanged; a new screen cannot overwrite their decisions',
            'attempt_policy':'Exactly one run. Interruptions consume the attempt. Never overwrite raw, alter this plan, or add repetitions after observing results.'}
    plan_path.parent.mkdir(parents=True,exist_ok=True);save(plan_path,plan)
    print(json.dumps({'plan':str(plan_path),'sha256':sha(plan_path),'requests':84,'pairs':42,'executed':0},indent=2))

def timed(argv, log, timeout):
    log.mkdir(parents=True,exist_ok=False)
    previous = signal.getsignal(signal.SIGALRM)
    def alarm(*_): raise TimeoutError('deadline')
    row = {'argv':list(map(str,argv)),'exit':None,'timeout':False,'log':str(log)}
    with (log/'stdout').open('wb') as out,(log/'stderr').open('wb') as err:
        start = time.perf_counter_ns()
        try:
            proc = subprocess.Popen(row['argv'],stdout=out,stderr=err,start_new_session=True)
        except OSError as error:
            row.update(wall_ms=(time.perf_counter_ns()-start)/1e6,cpu_ms=None,rss_kib=None,spawn_error=repr(error))
            return row
        signal.signal(signal.SIGALRM,alarm);signal.setitimer(signal.ITIMER_REAL,timeout)
        try:
            _,status,use = os.wait4(proc.pid,0)
        except TimeoutError:
            row['timeout']=True
            os.killpg(proc.pid,signal.SIGKILL);_,status,use=os.wait4(proc.pid,0)
        except BaseException:
            os.killpg(proc.pid,signal.SIGKILL);os.wait4(proc.pid,0)
            raise
        finally:
            end=time.perf_counter_ns();signal.setitimer(signal.ITIMER_REAL,0);signal.signal(signal.SIGALRM,previous)
        proc.returncode=os.waitstatus_to_exitcode(status)
        row.update(exit=proc.returncode,wall_ms=(end-start)/1e6,
                   cpu_ms=1000*(use.ru_utime+use.ru_stime),rss_kib=use.ru_maxrss)
    return row

def sidecar_material(data):
    lines=data.splitlines(keepends=True)
    indexes=[i for i,line in enumerate(lines) if line.startswith(b'; verified-fingerprint: ')]
    if indexes != [1]: raise ValueError('Expected exactly one verified fingerprint on second line')
    return b''.join(lines[:1]+lines[2:])

def summarize(plan, rows, pairs, failures, complete, interrupted=None):
    cells=[]
    for case,jobs,_,_ in LANES:
        for request in ['cold','warm','private','edit-warm']:
            cell={'case':case,'jobs':jobs,'request':request,'arms':{}}
            for arm in ['base','cand']:
                selected=[r for r in rows if (r['case'],r['jobs'],r['request'],r['arm'])==(case,jobs,request,arm)]
                good=[r for r in selected if r.get('valid')]
                if good:
                    values=[r['wall_ms'] for r in good]
                    cell['arms'][arm]={'n':len(good),'expected_n':(7 if jobs==2 else 3) if request=='warm' else (3 if jobs==2 else 1) if request=='edit-warm' else 1,
                        'mean_ms':statistics.mean(values),'median_ms':statistics.median(values),
                        'values_ms':values,'cpu_values_ms':[r['cpu_ms'] for r in good],
                        'rss_values_kib':[r['rss_kib'] for r in good]}
            if len(cell['arms'])==2:
                base=cell['arms']['base']['mean_ms'];cand=cell['arms']['cand']['mean_ms']
                cell.update(saved_ms=base-cand,candidate_ratio=cand/base,
                            over_10pct=cand>1.10*base)
            cells.append(cell)
    return {'schema':SCHEMA,'complete':complete,'compile_requests':len(rows),'expected_compile_requests':84,
            'valid_requests':sum(r.get('valid',False) for r in rows),'pairs':len(pairs),'expected_pairs':42,
            'all_elf_equal':len(pairs)==42 and all(r.get('elf_equal',False) for r in pairs),
            'all_sidecar_material_equal':len(pairs)==42 and all(r.get('sidecar_material_equal',False) for r in pairs),
            'all_cache_topology_equal':len(pairs)==42 and all(r.get('cache_topology_equal',False) for r in pairs),
            'native_output_checks':sum(len(r.get('native_checks',[])) for r in rows),
            'expected_native_output_checks':252,'failures':failures,'cells':cells,'interrupted':interrupted,
            'classification':'Fixed diagnostic screen; not the original weighted matrix and not an adoption decision',
            'rerun_allowed':False,'prior_failures_unchanged':True}

def run(a):
    plan_path=a.plan.resolve();plan=json.loads(plan_path.read_text());assert plan['schema']==SCHEMA
    if sha(__file__)!=plan['script']['sha256']:raise SystemExit('Script changed since plan freeze')
    for item in list(plan['tools'].values())+[plan['model']]+[s for snapshots in plan['sources'].values() for s in snapshots.values()]:
        if sha(item['path'])!=item['sha256']:raise SystemExit('Frozen input/tool hash mismatch: '+item['path'])
    cpus=set(plan['cpus'])
    if not cpus <= os.sched_getaffinity(0):raise SystemExit('Frozen CPUs unavailable')
    os.sched_setaffinity(0,cpus)
    out=Path(plan['out']);attempt=plan_path.with_name(plan_path.name+'.attempt.json')
    with attempt.open('x') as f:json.dump({'started_utc':utc(),'plan_sha256':sha(plan_path),'out':str(out),'pid':os.getpid()},f,indent=2)
    out.mkdir(parents=True,exist_ok=False);shutil.copy2(plan_path,out/'plan.json')
    save(out/'host.json',{'platform':os.uname()._asdict() if hasattr(os.uname(),'_asdict') else list(os.uname()),
        'cpus':sorted(os.sched_getaffinity(0)),
        'cpuinfo':Path('/proc/cpuinfo').read_text(),'loadavg_start':Path('/proc/loadavg').read_text(),
        'cgroup_cpu_stat':Path('/sys/fs/cgroup/cpu.stat').read_text() if Path('/sys/fs/cgroup/cpu.stat').exists() else None})
    rows=[];pairs=[];failures=[];interrupted=None
    try:
        for pair in plan['pairs']:
            current={}
            for arm in pair['arms']:
                case=pair['case'];jobs=pair['jobs'];request=pair['request'];snapshot=pair['snapshot'];number=len(rows)
                work=out/'work'/case/f'jobs{jobs}'/arm;work.mkdir(parents=True,exist_ok=True)
                source=Path(plan['sources'][case][snapshot]['path']).read_bytes();path=work/'main.zil'
                if not path.exists() or path.read_bytes()!=source:path.write_bytes(source)
                exe=work/'app';sidecar=work/'app.ll';exe.unlink(missing_ok=True);sidecar.unlink(missing_ok=True)
                argv=[plan['tools'][arm]['path'],'build',str(path),'--json','--opt','O2','--backend','llvm-api','--clang',plan['tools']['clang']['path'],'--codegen-units','8','--codegen-jobs',str(jobs),'--semantic-cache',str(work/'semantic'),'--object-cache',str(work/'objects'),'-o',str(exe),'--incremental-o2','--function-merging','after']
                log=out/'logs'/f'{number:03d}';row=timed(argv,log,plan['timeout_seconds'])
                row.update(pair=pair['pair'],case=case,jobs=jobs,request=request,snapshot=snapshot,
                           repetition=pair['repetition'],arm=arm,source_sha256=sha(path),valid=False,native_checks=[])
                errors=[]
                try:
                    if row['exit']!=0 or row['timeout']:raise ValueError('Compiler failed or timed out')
                    report=json.loads((log/'stdout').read_text())
                    row.update(status=report.get('status'),verification_runs=report.get('verification_runs'),
                               disk_cache=report.get('disk_cache'),native_codegen=report.get('native_codegen'),query_events=report.get('query_events'))
                    if report['status']!='Success' or report['verification_runs']!=1:raise ValueError('Missing current successful Verify=1')
                    if sha(path)!=plan['sources'][case][snapshot]['sha256']:raise ValueError('Source changed')
                    nc=report['native_codegen'];cache=nc['object_cache']
                    if not cache['enabled'] or cache['corrupt'] or cache['write_failures']:raise ValueError('Unexpected cache state')
                    if request in ('warm','edit-warm') and (cache['misses']!=0 or cache['hits']!=nc['actual_units']):raise ValueError('Warm request did not hit all current objects')
                    row['elf_sha256']=sha(exe);row['sidecar_sha256']=sha(sidecar)
                    shutil.copy2(exe,log/'app');shutil.copy2(sidecar,log/'app.ll')
                    for data in plan['inputs_hex']:
                        native_log=log/('native-'+data);native_log.mkdir();(native_log/'stdin').write_bytes(bytes.fromhex(data))
                        try:
                            native=subprocess.run([str(exe)],input=bytes.fromhex(data),capture_output=True,timeout=30)
                            (native_log/'stdout').write_bytes(native.stdout);(native_log/'stderr').write_bytes(native.stderr)
                            expected=plan['oracles'][case][snapshot][data]
                            ok=native.returncode==0 and not native.stderr and native.stdout.hex()==expected
                            check={'input_hex':data,'expected_hex':expected,'actual_hex':native.stdout.hex(),'exit':native.returncode,'pass':ok}
                        except Exception as error:check={'input_hex':data,'pass':False,'error':repr(error)}
                        row['native_checks'].append(check)
                        if not check['pass']:errors.append('Native output oracle failed: '+data)
                except Exception as error:errors.append(repr(error))
                row['valid']=not errors;row['errors']=errors
                if errors:failures.append({'kind':'request','row':number,'pair':pair['pair'],'arm':arm,'errors':errors})
                rows.append(row);current[arm]=row;save(log/'record.json',row)
                with (out/'rows.jsonl').open('a') as f:f.write(json.dumps(row)+'\n')
            paired={'pair':pair['pair'],'case':case,'jobs':jobs,'request':request,'repetition':pair['repetition'],'valid':False}
            try:
                base=current['base'];cand=current['cand']
                if not base['valid'] or not cand['valid']:raise ValueError('Invalid request in pair')
                base_log=Path(base['log']);cand_log=Path(cand['log'])
                paired['elf_equal']=(base_log/'app').read_bytes()==(cand_log/'app').read_bytes()
                bs=(base_log/'app.ll').read_bytes();cs=(cand_log/'app.ll').read_bytes()
                paired['sidecar_raw_equal']=bs==cs;paired['sidecar_material_equal']=sidecar_material(bs)==sidecar_material(cs)
                def topology(row):
                    nc=row['native_codegen'];c=nc['object_cache']
                    return [nc['actual_units'],nc['jobs'],nc['fallback']]+[c[k] for k in ['hits','misses','corrupt','stored','write_failures']]
                paired['cache_topology_equal']=topology(base)==topology(cand)
                paired['valid']=paired['elf_equal'] and paired['sidecar_material_equal'] and paired['cache_topology_equal']
                paired.update(saved_ms=base['wall_ms']-cand['wall_ms'],base_ms=base['wall_ms'],cand_ms=cand['wall_ms'])
                if not paired['valid']:raise ValueError('ELF/sidecar/cache topology parity failed')
            except Exception as error:
                paired['error']=repr(error);failures.append({'kind':'pair','pair':pair['pair'],'error':repr(error)})
            pairs.append(paired)
            with (out/'pairs.jsonl').open('a') as f:f.write(json.dumps(paired)+'\n')
            print(json.dumps({'pair':pair['pair'],'case':case,'jobs':jobs,'request':request,'repetition':pair['repetition'],'valid':paired['valid'],'saved_ms':paired.get('saved_ms')}),flush=True)
    except BaseException as error:
        interrupted=repr(error);raise
    finally:
        complete=len(rows)==84 and len(pairs)==42 and not failures and interrupted is None
        result=summarize(plan,rows,pairs,failures,complete,interrupted)
        save(out/'summary.json',result);save(out/'failures.json',failures)
        save(out/'completion.json',{'ended_utc':utc(),'complete':complete,'plan_sha256':sha(plan_path),'loadavg_end':Path('/proc/loadavg').read_text()})
    if not complete:raise SystemExit(2)

def main():
    parser=argparse.ArgumentParser(description=__doc__);sp=parser.add_subparsers(dest='command',required=True)
    p=sp.add_parser('prepare');p.add_argument('--base',type=Path,required=True);p.add_argument('--candidate',type=Path,required=True);p.add_argument('--clang',type=Path,required=True)
    p.add_argument('--corpus',type=Path,default=DEFAULT_ROOT/'evidence/resource-acceptance/primary/corpus')
    p.add_argument('--model',type=Path,default=DEFAULT_ROOT/'audit-acceptance/helper-current/bench/corpus.py')
    p.add_argument('--plan',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--cpu',default='0,1');p.add_argument('--seed',type=int,default=20261111)
    p.add_argument('--baseline-source',default='main99035931/tree1c16744d838064cb1c687ba56ef3a0d17ed3130c; local d0b2eeeb466f8e7d5d4edca5c59265727da78254')
    p.add_argument('--candidate-source',default='9e15ab226b5c0e95f18b1e1a7d1d2517dc646850/treeba2a08e59c67339b1420e47480d2055ef81d1021')
    p=sp.add_parser('run');p.add_argument('--plan',type=Path,required=True)
    a=parser.parse_args();prepare(a) if a.command=='prepare' else run(a)
if __name__=='__main__':main()
