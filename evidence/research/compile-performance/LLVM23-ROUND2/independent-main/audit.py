#!/usr/bin/env python3
"""Independent Decimal audit of a completed frozen main-to-candidate window.

Does not import a benchmark runner or aggregate helper, compile code, or execute
benchmark programs. The sole compiler invocation is clang -###, which prints the
default linker command without executing it. Every numeric aggregate below is
implemented here from raw records with Decimal inputs.
"""
from collections import Counter, defaultdict
from datetime import datetime, timezone
from decimal import Decimal, getcontext
from functools import lru_cache
import hashlib
import json
import os
from pathlib import Path
import random
import re
import subprocess
import traceback

getcontext().prec = 50
HERE = Path(__file__).resolve().parent
REPO = Path('/workspace/scratch/ac5966c4c370/zil-research')
ROUND = Path('/tmp/zil-llvm23-round2')
PLAN_DIR = ROUND / 'main-o0-plan'
PRE = ROUND / 'main-o0-preflight'
WINDOW = ROUND / 'measurements-confirmation'
RUN = WINDOW / 'main-o0-build'
REQUESTS = ('cold_process', 'unchanged', 'private_edit', 'public_edit')
SNAPSHOTS = dict(zip(REQUESTS, ('base', 'base', 'private', 'public')))
KEYS = ('block', 'phase', 'case', 'position', 'arm', 'request')
FILES = {}
CHECKS = 0

def check(condition, message):
    global CHECKS
    if not condition:
        raise AssertionError(message)
    CHECKS += 1

def file_hash(path):
    path = Path(path)
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for data in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(data)
    value = digest.hexdigest()
    FILES[str(path)] = {'sha256': value, 'bytes': path.stat().st_size}
    return value

def read_json(path):
    path = Path(path)
    file_hash(path)
    return json.loads(path.read_text(), parse_float=Decimal)

def read_rows(path):
    path = Path(path)
    file_hash(path)
    with path.open() as stream:
        return [json.loads(line, parse_float=Decimal) for line in stream]

def serializable(value):
    if isinstance(value, Decimal):
        return str(value)
    if isinstance(value, dict):
        return {str(k): serializable(v) for k,v in value.items()}
    if isinstance(value, (list, tuple)):
        return [serializable(v) for v in value]
    if isinstance(value, Path):
        return str(value)
    return value

def save(path, value):
    Path(path).write_text(json.dumps(serializable(value), ensure_ascii=False, indent=2) + '\n')

def git(*args):
    return subprocess.check_output(['git', *args], cwd=REPO, text=True).strip()

def verify_inventory(spec):
    path = Path(spec['path'])
    check(file_hash(path) == spec['sha256'], f'file content identity: {path}')
    st = path.stat()
    check(st.st_size == spec['size'], f'file size identity: {path}')
    for key, actual in [('mtime_ns', st.st_mtime_ns), ('ctime_ns', st.st_ctime_ns)]:
        if key in spec:
            check(actual == spec[key], f'file installation identity: {path} {key}')

def mean(values):
    return sum(values, Decimal(0)) / Decimal(len(values))

def median(values):
    values = sorted(values)
    n = len(values)
    return values[n//2] if n % 2 else (values[n//2-1] + values[n//2]) / Decimal(2)

def relation(a, b):
    return {'A_seconds': a, 'B_seconds': b, 'B_over_A': b/a,
            'time_reduction_percent': (1-b/a)*100, 'A_over_B_speedup': a/b}

def verify_rows(plan, rows, directory, expected, label, fixture_sources):
    check(len(rows) == len(expected), f'{label}: record count')
    observed = [tuple(row.get(key) for key in KEYS) for row in rows]
    check(observed == expected, f'{label}: exact ordered cell coverage')
    plan_sha = file_hash(PLAN_DIR/'plan.json')
    cases = {case['id']:case for case in plan['cases']}
    diagnostics = {'rows':len(rows), 'oracles':0, 'raw_reports_checked':0,
                   'linkers':Counter(), 'source_cache_by_request':defaultdict(Counter),
                   'actual_units':defaultdict(set), 'object_reports':0,
                   'ephemeral_executables_absent':0, 'ephemeral_sidecars_absent':0}
    for index,row in enumerate(rows):
        check(row['index'] == index, f'{label}/{index}: monotonic index')
        check(row['plan_sha256'] == plan_sha, f'{label}/{index}: plan binding')
        check(row['pass'] is True and row['exit'] == 0 and row['timeout'] is False,
              f'{label}/{index}: build success and no timeout')
        for field in ('wall_seconds', 'cpu_user', 'cpu_system'):
            value = Decimal(row[field])
            check(value.is_finite() and value >= 0, f'{label}/{index}: finite {field}')
        check(row['wall_seconds'] > 0, f'{label}/{index}: positive wall time')
        case = cases[row['case']]
        request = row['request']
        check(row['snapshot'] == SNAPSHOTS[request], f'{label}/{index}: snapshot selection')
        check(row['source_before'] == fixture_sources[(row['case'], row['snapshot'])],
              f'{label}/{index}: exact source bytes match frozen snapshot')
        check(row['source_after'] == row['source_before'], f'{label}/{index}: source unchanged during build')
        work = directory / f"s{index//4:04d}-{row['case']}-{row['phase']}-{row['arm']}"
        program = work/'program'
        argv = [plan['binaries'][row['arm']]['path'], 'build', str(work/'src/main.zil'), '--json',
                '--cache-dir', str(work/'source-candidates'), '--clang', plan['binaries']['clang']['path'],
                '--opt', 'O0', '--codegen-units', '8', '--codegen-jobs', '4', '-o', str(program)]
        check(row['argv'] == argv, f'{label}/{index}: exact argv/cache/path contract')
        check(row['stdout'] == f'{index:05d}.stdout.json' and row['stderr'] == f'{index:05d}.stderr.txt',
              f'{label}/{index}: compile log names')
        logdir = directory/'logs'
        report = read_json(logdir/row['stdout'])
        file_hash(logdir/row['stderr'])
        check(report['status'] == 'Success' and report['phase'] == 'link', f'{label}/{index}: raw CLI success')
        check(report['verification_runs'] == 1, f'{label}/{index}: Verify once')
        check(sum(e['phase'] == 'verify' for e in report['query_events']) == 1,
              f'{label}/{index}: one actual Verify event')
        check(report['diagnostics'] == [] and report.get('timings') is None, f'{label}/{index}: diagnostics/timings mode')
        for field in ('status','phase','verification_runs','native_codegen','commands','disk_cache','diagnostics'):
            check(row['reported_'+field] == report.get(field), f'{label}/{index}: raw JSON vs record {field}')
        check(report['artifacts']['executable'] == str(program), f'{label}/{index}: executable publication path')
        check(report['artifacts']['llvm'] == str(program.with_suffix('.ll')), f'{label}/{index}: LLVM publication path')
        check(report['disk_cache'] != 'disabled', f'{label}/{index}: raw source candidate cache is enabled')
        if request == 'cold_process':
            check(report['disk_cache'] == 'missing', f'{label}/{index}: fresh source cache')
        if request == 'unchanged':
            check(report['disk_cache'] == 'hit', f'{label}/{index}: unchanged source cache')
        diagnostics['source_cache_by_request'][(row['arm'],request)][report['disk_cache']] += 1
        native = report['native_codegen']
        check(native['backend'] == 'llvm-api' and native['requested_units'] == 8,
              f'{label}/{index}: actual backend/requested CGU')
        check(native['jobs'] == min(4,native['actual_units']), f'{label}/{index}: actual job bound')
        check(not native['object_cache']['requested'] and not native['object_cache']['enabled'],
              f'{label}/{index}: object cache disabled')
        check(all(native['object_cache'][key] == 0 for key in ('hits','misses','stored','corrupt','write_failures')),
              f'{label}/{index}: no object reuse')
        if row['arm'] == 'B':
            check(native['closed_executable'] is False and native['internalized_functions'] == 0,
                  f'{label}/{index}: closed linkage disabled')
            cache = report['query_cache']
            check(cache['requested'] is False and cache['state'] == 'disabled' and cache['key'] is None
                  and cache['stored'] is False, f'{label}/{index}: query cache disabled')
        else:
            check('closed_executable' not in native and 'query_cache' not in report,
                  f'{label}/{index}: exact legacy main report schema')
        check(len(native['object_reports']) == native['actual_units'], f'{label}/{index}: object report coverage')
        for obj in native['object_reports']:
            check(obj['llvm_version'] == '23.1.4' and obj['pipeline'] == 'default<O0>'
                  and obj['input_kind'] == 'builder' and obj['verify_before_after'] is True
                  and obj['triple'] == 'x86_64-unknown-linux-gnu', f'{label}/{index}: LLVM configuration')
        diagnostics['object_reports'] += len(native['object_reports'])
        diagnostics['actual_units'][(row['arm'],row['case'])].add(native['actual_units'])
        commands = report['commands']
        check(len(commands) == 1, f'{label}/{index}: one actual link command')
        command = commands[0]
        check(command[0] == plan['binaries']['clang']['path'] and '--target=x86_64-unknown-linux-gnu' in command
              and '-O0' in command, f'{label}/{index}: frozen clang link command')
        check(sum(arg.endswith('.o') for arg in command) == native['actual_units'], f'{label}/{index}: all objects linked')
        linker_flags = [arg for arg in command if arg.startswith(('--ld-path','-fuse-ld'))]
        if row['arm'] == 'B':
            wanted = '--ld-path=' + str(Path(plan['binaries']['clang']['path']).with_name('ld.lld'))
            check(linker_flags == [wanted], f'{label}/{index}: actual companion LLD argument')
            diagnostics['linkers']['B explicit companion ld.lld'] += 1
        else:
            check(linker_flags == [], f'{label}/{index}: main uses clang default linker selection')
            diagnostics['linkers']['A clang system default'] += 1
        check(bool(re.fullmatch('[0-9a-f]{64}',row['executable_sha256'])) and row['executable_bytes'] > 0
              and bool(re.fullmatch('[0-9a-f]{64}',row['llvm_sha256'])), f'{label}/{index}: recorded output identities')
        diagnostics['ephemeral_executables_absent'] += int(not program.exists())
        diagnostics['ephemeral_sidecars_absent'] += int(not program.with_suffix('.ll').exists())
        check(len(row['checks']) == len(case['inputs']), f'{label}/{index}: oracle count')
        for i,(oracle,sample) in enumerate(zip(row['checks'],case['inputs'])):
            check(oracle['matches'] is True and oracle['exit'] == 0 and not oracle.get('timeout',False),
                  f'{label}/{index}/{i}: oracle status')
            check(oracle['input'] == sample['path'] and oracle['expected'] == sample['expected'][row['snapshot']],
                  f'{label}/{index}/{i}: oracle binding')
            check(oracle['stdout'] == f'{index:05d}.run{i}.stdout.bin' and oracle['stderr'] == f'{index:05d}.run{i}.stderr.bin',
                  f'{label}/{index}/{i}: oracle log names')
            stdout,stderr = logdir/oracle['stdout'],logdir/oracle['stderr']
            check(file_hash(stdout) == oracle['stdout_sha256'], f'{label}/{index}/{i}: oracle stdout hash')
            check(file_hash(stderr) == oracle['stderr_sha256'], f'{label}/{index}/{i}: oracle stderr hash')
            check(stdout.read_bytes() == (PLAN_DIR/oracle['expected']).read_bytes() and stderr.read_bytes() == b'',
                  f'{label}/{index}/{i}: actual oracle bytes')
            diagnostics['oracles'] += 1
        diagnostics['raw_reports_checked'] += 1
    diagnostics['linkers'] = dict(diagnostics['linkers'])
    diagnostics['source_cache_by_request'] = {f'{arm}/{req}':dict(counts) for (arm,req),counts in diagnostics['source_cache_by_request'].items()}
    diagnostics['actual_units'] = {f'{arm}/{case}':sorted(values) for (arm,case),values in diagnostics['actual_units'].items()}
    return diagnostics

def calculations(plan, rows, author_summary):
    blocks = tuple(range(plan['blocks']))
    names = tuple(c['id'] for c in plan['cases'])
    weights = {c['id']:c['weight'] for c in plan['cases']}
    reqweights = plan['request_weights']
    check(sum(weights.values()) == 1 and sum(reqweights.values()) == 1, 'normalized fixed weights')
    cells = defaultdict(list)
    for row in rows:
        key = (row['block'],row['case'],row['request'],row['arm'])
        cells[key].append({'wall':row['wall_seconds'], 'cpu':row['cpu_user']+row['cpu_system']})
    check(all(len(values) == 2 for values in cells.values()), 'two arm observations per block/case/request')
    means = {key:{metric:mean([v[metric] for v in values]) for metric in ('wall','cpu')}
             for key,values in cells.items()}

    @lru_cache(maxsize=None)
    def aggregate(selected_blocks, selected_cases, metric):
        normalization = sum(weights[name] for name in selected_cases)
        total = {'A':Decimal(0),'B':Decimal(0)}
        by_case,by_request,by_cell = {},{},{}
        for req in REQUESTS:
            by_request[req] = {'A':Decimal(0),'B':Decimal(0)}
        for case in selected_cases:
            case_cost = {'A':Decimal(0),'B':Decimal(0)}
            for req in REQUESTS:
                cost = {arm:median([means[(block,case,req,arm)][metric] for block in selected_blocks]) for arm in 'AB'}
                by_cell[case+'/'+req] = relation(cost['A'],cost['B'])
                for arm in 'AB':
                    case_cost[arm] += reqweights[req]*cost[arm]
                    by_request[req][arm] += weights[case]/normalization*cost[arm]
                    total[arm] += weights[case]/normalization*reqweights[req]*cost[arm]
            by_case[case] = relation(case_cost['A'],case_cost['B'])
        return {**relation(total['A'],total['B']), 'by_case':by_case,
                'by_request':{r:relation(v['A'],v['B']) for r,v in by_request.items()}, 'by_case_request':by_cell}

    output = {metric:aggregate(blocks,names,metric) for metric in ('wall','cpu')}
    output['per_block'] = {str(b):{metric:aggregate((b,),names,metric) for metric in ('wall','cpu')} for b in blocks}
    reduced = tuple(n for n in names if n != 'tree1024x8')
    output['without_largest_tree'] = {'excluded':'tree1024x8', 'retained_original_weight':sum(weights[n] for n in reduced),
                                     'normalization':'remaining case weights divided by 0.70; request weights unchanged',
                                     **{metric:aggregate(blocks,reduced,metric) for metric in ('wall','cpu')}}
    for label,selected in [('all_cases',names),('without_largest_tree',reduced)]:
        rng = random.Random(plan['seed'])
        draws = [tuple(rng.choices(blocks,k=len(blocks))) for _ in range(2000)]
        values = sorted(aggregate(draw,selected,'wall')['B_over_A'] for draw in draws)
        interval = [values[49],values[1949]]
        output[label+'_descriptive_block_bootstrap95'] = interval
    differences = []
    def compare(a,b,path):
        delta = abs(Decimal(a)-Decimal(b))
        check(delta <= Decimal('0.000000000001'), 'independent Decimal vs supplied summary: '+path)
        differences.append({'field':path,'absolute_delta':delta})
    supplied = author_summary['by_phase']['build']
    for metric in ('wall','cpu'):
        ours = output[metric]
        for key,other in [('A_seconds','A'),('B_seconds','B'),('B_over_A','B_over_A')]:
            compare(ours[key],supplied[metric][other],metric+'/'+other)
        for group in ('by_case','by_request'):
            for name,values in ours[group].items():
                for key,other in [('A_seconds','A'),('B_seconds','B'),('B_over_A','B_over_A')]:
                    compare(values[key],supplied[metric][group][name][other],metric+'/'+group+'/'+name+'/'+other)
    for name,values in output['wall']['by_case_request'].items():
        for key,other in [('A_seconds','A'),('B_seconds','B'),('B_over_A','B_over_A')]:
            compare(values[key],supplied['by_case_request'][name][other],'wall/by_case_request/'+name+'/'+other)
    for i,value in enumerate(output['all_cases_descriptive_block_bootstrap95']):
        compare(value,supplied['exploratory_paired_block_bootstrap95'][i],f'bootstrap/{i}')
    output['summary_comparison'] = {'compared_values':len(differences),'tolerance_seconds_or_ratio':'1e-12',
                                    'maximum_absolute_delta':max(d['absolute_delta'] for d in differences)}
    return output

def compare_retained_o0():
    pairs = [
        (Path('/workspace/scratch/ac5966c4c370/runs/rust-o0-cold-e8-plan'),Path('/workspace/scratch/ac5966c4c370/runs/rust-o0-cold-e8-preflight')),
        (ROUND/'rust-o0-cold-plan',ROUND/'rust-o0-cold-preflight'),
    ]
    groups = []
    for plan_dir,pre_dir in pairs:
        plan = read_json(plan_dir/'plan.json')
        result = read_json(pre_dir/'result.json')
        check(result['pass'] is True and not result['failures'], f'{pre_dir}: preflight success')
        check(file_hash(plan_dir/'plan.json') == result['plan_sha256'], f'{pre_dir}: original plan binding')
        check(file_hash(pre_dir/'records.jsonl') == result['records_sha256'], f'{pre_dir}: original records binding')
        check(file_hash(pre_dir/'artifacts.json') == result['artifacts_sha256'], f'{pre_dir}: artifact manifest binding')
        check(plan['profile']=='O0' and plan['units']==8 and plan['jobs']==4 and plan['cache_mode']=='cold'
              and not plan.get('query_cache',False) and not plan.get('closed_executable',False), f'{pre_dir}: O0 configuration')
        verify_inventory(plan['binaries']['B'])
        artifacts = {row['case']:row for row in read_json(pre_dir/'artifacts.json') if row['arm']=='B'}
        records = {row['case']:row for row in read_rows(pre_dir/'records.jsonl') if row['arm']=='B'}
        check(set(artifacts)==set(records) and len(artifacts)==5, f'{pre_dir}: five ZIL artifacts')
        for case,artifact in artifacts.items():
            spec = artifact['executable']
            verify_inventory(spec)
            row = records[case]
            check(row['pass'] is True and row['plan_sha256']==result['plan_sha256'], f'{pre_dir}/{case}: success')
            check(row['executable']['sha256']==spec['sha256'] and row['executable']['size']==spec['size'], f'{pre_dir}/{case}: executable provenance')
            check(row['argv'][0]==plan['binaries']['B']['path'] and '--closed-executable' not in row['argv']
                  and '--query-cache' not in row['argv'] and '--object-cache' not in row['argv'], f'{pre_dir}/{case}: uncached/open mode')
            for flag,wanted in [('--opt','O0'),('--backend','llvm-api'),('--codegen-units','8'),('--codegen-jobs','4')]:
                check(row['argv'][row['argv'].index(flag)+1]==wanted, f'{pre_dir}/{case}: {flag}')
            stdout,stderr=pre_dir/'logs'/row['stdout'],pre_dir/'logs'/row['stderr']
            check(file_hash(stdout)==row['stdout_sha256'] and file_hash(stderr)==row['stderr_sha256'], f'{pre_dir}/{case}: compile logs')
            report=read_json(stdout)
            check(report['verification_runs']==1 and report['status']=='Success', f'{pre_dir}/{case}: actual Verify')
            check(report['native_codegen']['object_cache']['requested'] is False, f'{pre_dir}/{case}: no object cache')
        groups.append({'plan':plan,'artifacts':artifacts,'records':records,'preflight':str(pre_dir)})
    results=[]
    for case in groups[0]['artifacts']:
        old=groups[0]['artifacts'][case]['executable'];new=groups[1]['artifacts'][case]['executable']
        equal=Path(old['path']).read_bytes()==Path(new['path']).read_bytes()
        results.append({'case':case,'old':old,'new':new,'byte_equal':equal,
                        'sha256_equal':old['sha256']==new['sha256'],
                        'source_snapshot_hashes_equal':groups[0]['records'][case]['source_before']==groups[1]['records'][case]['source_before']})
    return {'old_source':groups[0]['plan']['source_identity']['B'], 'new_source':groups[1]['plan']['source_identity']['B'],
            'profile':'O0; llvm-api; 8 CGU / 4 jobs; no compilation caches; closed mode off',
            'cases':results, 'all_five_byte_equal':all(row['byte_equal'] for row in results),
            'claim_boundary':'These five retained ZIL executables are identical bytes. This does not prove all O0 programs unchanged or establish a cross-window runtime comparison.'}

def audit():
    plan=read_json(PLAN_DIR/'plan.json')
    summary=read_json(WINDOW/'main-o0-build-summary.json')
    complete=read_json(RUN/'complete.json')
    pre_result=read_json(PRE/'result.json')
    quiet=read_json(WINDOW/'quiet-window.json')
    quiet_end=read_json(WINDOW/'quiet-window-complete.json')
    commands=read_json(WINDOW/'commands.json')
    check(plan['profile']=='O0' and plan['units']==8 and plan['jobs']==4 and plan['object_cache'] is False
          and plan['blocks']==3 and plan['phases']==['build'] and plan['cpus']==[0,1,2,3], 'frozen bridge profile')
    for arm,source in plan['source_identity'].items():
        check(git('rev-parse',source['commit']+':crates')==source['crates_tree'], f'{arm}: committed source tree binding')
    check(file_hash(REPO/'tools/research/compile-performance/bench.py')==plan['runner_sha256'], 'frozen runner hash')
    for path,digest in plan['dependencies_sha256'].items():
        check(file_hash(REPO/path)==digest, 'frozen helper/oracle: '+path)
    for spec in [*plan['binaries'].values(),*plan['extra_tools']]:
        verify_inventory(spec)
    actual_fixtures={str(p.relative_to(PLAN_DIR/'corpus')):file_hash(p) for p in sorted((PLAN_DIR/'corpus').rglob('*')) if p.is_file()}
    check(actual_fixtures==plan['fixtures_sha256'], 'entire frozen fixture set and all content hashes')
    fixture_sources={}
    for case in plan['cases']:
        for variant in ('base','private','public'):
            directory=PLAN_DIR/case['root']/variant/'zil'
            fixture_sources[(case['id'],variant)]={str(p.relative_to(directory)):file_hash(p) for p in sorted(directory.rglob('*')) if p.is_file()}
    plan_sha=file_hash(PLAN_DIR/'plan.json')
    for result,directory,count in [(complete,RUN,240),(pre_result,PRE,40)]:
        check(result['pass'] is True and result['failures']==[] and result['rows']==count, f'{directory}: complete successful receipt')
        check(result['plan_sha256']==plan_sha and result['records_sha256']==file_hash(directory/'records.jsonl'), f'{directory}: record hash binding')
    check(complete['preflight_sha256']==file_hash(PRE/'result.json'), 'formal measurement binds passing preflight')
    check(summary['plan_sha256']==plan_sha and summary['records_sha256']==complete['records_sha256']
          and summary['rows']==240 and summary['native_checks']==768 and summary['coverage_complete'] is True
          and summary['failures']==[], 'summary coverage and identities')
    check(quiet_end['pass'] is True and quiet_end['commands']==len(commands)==22, 'quiet window completed')
    check(quiet['source']==plan['source_identity']['B']['commit'] and quiet['crates_tree']==plan['source_identity']['B']['crates_tree'], 'quiet window source')
    check(quiet['compiler_processes_observed_before_start']==[], 'coordinator recorded no pre-existing compiler processes')
    check(datetime.fromisoformat(quiet['started_utc']) < datetime.fromisoformat(quiet_end['completed_utc']), 'quiet chronological receipts')
    for path,digest in quiet['checks'].items():
        check(file_hash(Path(path))==digest, 'quiet prerequisite receipt: '+path)
    main_commands=[row for row in commands if row['label'] in ('main-o0-build','main-o0-build-summary')]
    check(len(main_commands)==2 and all(row['exit']==0 for row in main_commands), 'formal bridge command completion')
    measure=next(row for row in main_commands if row['label']=='main-o0-build')
    check('--quiet-window' in measure['argv'] and str(PLAN_DIR/'plan.json') in measure['argv']
          and str(PRE/'result.json') in measure['argv'] and str(RUN) in measure['argv'], 'selected confirmation command contract')
    for row in main_commands:
        file_hash(WINDOW/row['log'])
    expected=[tuple({**seq,'request':req}[k] for k in KEYS) for seq in plan['order'] for req in REQUESTS]
    expected_pre=[tuple({'block':0,'phase':phase,'case':case['id'],'position':pos,'arm':arm,'request':req}[k] for k in KEYS)
                  for phase in plan['phases'] for case in plan['cases'] for pos,arm in enumerate('AB') for req in REQUESTS]
    rows=read_rows(RUN/'records.jsonl');pre_rows=read_rows(PRE/'records.jsonl')
    observations=verify_rows(plan,rows,RUN,expected,'formal',fixture_sources)
    preflight=verify_rows(plan,pre_rows,PRE,expected_pre,'preflight',fixture_sources)
    check(observations['oracles']==768 and preflight['oracles']==128, 'actual oracle byte check counts')
    numeric=calculations(plan,rows,summary)
    dry_env=dict(os.environ)
    for key in list(dry_env):
        if key in {'PATH','LANG','TMPDIR','TMP','TEMP'} or key.startswith(('ZIL_','LLVM_','LD_','RUST','CARGO_','LC_')):
            dry_env.pop(key)
    dry_env.update(plan['environment'])
    dry_argv=[plan['binaries']['clang']['path'],'-###','--target=x86_64-unknown-linux-gnu','-O0','/dev/null','-o','/dev/null']
    dry=subprocess.run(dry_argv,capture_output=True,text=True,env=dry_env,check=False)
    check(dry.returncode==0 and '"/usr/bin/ld"' in dry.stderr, 'read-only clang default-linker resolution')
    system_linker=Path('/usr/bin/ld').resolve()
    companion=Path(plan['binaries']['clang']['path']).with_name('ld.lld')
    bound={Path(spec['path']).resolve():spec['sha256'] for spec in plan['extra_tools']}
    check(file_hash(system_linker)==bound[system_linker], 'system default is frozen ld.bfd content')
    check(file_hash(companion.resolve())==bound[companion.resolve()], 'explicit companion is frozen LLD content')
    retained=compare_retained_o0()
    same_sets=defaultdict(set)
    for row in rows:
        same_sets[(row['case'],row['snapshot'])].add(row['executable_sha256'])
    same_main_outputs=all(len(values)==1 for values in same_sets.values())
    check(same_main_outputs==summary['by_phase']['build']['same_binary_for_each_case_snapshot'], 'summary executable hash agreement')
    retained_sha={r['case']:r['new']['sha256'] for r in retained['cases']}
    base_matches=all(row['executable_sha256']==retained_sha[row['case']] for row in rows if row['arm']=='B' and row['snapshot']=='base')
    return {
        'schema':'zil.compile-performance.independent-measurement-review.v1',
        'reviewer':'/root/toolchain_setup','verdict':'Pass',
        'reviewed_utc':datetime.now(timezone.utc).isoformat(),
        'source_identity':plan['source_identity'],'plan_sha256':plan_sha,
        'formal_records_sha256':complete['records_sha256'],'summary_sha256':file_hash(WINDOW/'main-o0-build-summary.json'),
        'method':'Decimal precision 50 from JSON decimal spellings. Mean of each pair within a block, then median of block means per case/request/arm, then frozen case and request weights. CPU sums user+system before aggregation. No benchmark module imported.',
        'case_weights':{c['id']:c['weight'] for c in plan['cases']},'request_weights':plan['request_weights'],
        'numeric_results':numeric,'formal_observations':observations,'preflight_observations':preflight,
        'quiet_window':{'start':quiet,'complete':quiet_end,'main_commands':main_commands,
                        'limits':'Coordinator receipt and consistent frozen records; no physical-host exclusivity or independent continuous process trace claimed.'},
        'hash_checks':{'fixture_files':len(actual_fixtures),'all_frozen_helper_tool_and_binary_hashes_verified':True,
                       'raw_compile_reports':len(rows)+len(pre_rows),'actual_oracle_output_pairs':observations['oracles']+preflight['oracles'],
                       'input_file_manifest':'FILE-HASHES.json'},
        'linker_observations':{'A':'No linker selection argument in every actual main command. Frozen clang -### resolves its system default to /usr/bin/ld, whose real file is the bound ld.bfd.',
                               'B':'Every actual candidate command explicitly contains --ld-path=<frozen clang directory>/ld.lld; the resolved executable matches frozen LLD.',
                               'dry_run':{'argv':dry_argv,'exit':dry.returncode,'stdout':dry.stdout,'stderr':dry.stderr},
                               'system_linker_resolved':str(system_linker),'companion_linker_resolved':str(companion.resolve()),
                               'boundary':'-### is a read-only diagnostic, not a historical execve trace. This A/B window measures combined changes, including the default linker choice.'},
        'e8_to_candidate_o0_retained_executables':retained,
        'formal_B_base_hashes_match_retained_candidate_executables':base_matches,
        'formal_main_A_vs_B_same_binary_each_snapshot':same_main_outputs,
        'checks_passed':CHECKS,
        'limitations':[
            'This is main-to-candidate O0 8/4 with the raw source-candidate cache enabled. It is neither the Rust comparison nor a strict no-cache window.',
            'Query cache, machine-object cache and closed-executable mode are off. Each successful build still performs a new Verify and complete link.',
            'Only the completed confirmation window is scored. The aborted disk-full attempt is excluded, not pooled or silently substituted.',
            'Three paired blocks and fixed synthetic-heavy corpus weights support descriptive local conclusions. Bootstrap intervals are not a general workload guarantee.',
            'The full source snapshot and all actual oracle bytes are independently checked. Formal executable/sidecar files were deleted by the frozen runner after their generation-time hashes were recorded; those deleted files cannot be rehashed now.',
            'The separately retained five O0 ZIL executables are directly rehashed and compared byte for byte. That finite artifact equality is not a language-wide claim.',
            'Main A and candidate B select different linker strategies, so the measured net effect must not be attributed to one compiler optimization.',
            'All decimal values in this receipt are strings to retain exact decimal precision; native counters remain integers.',
        ],
    }

if __name__=='__main__':
    try:
        result=audit()
    except Exception as error:
        result={'verdict':'Fail','reviewer':'/root/toolchain_setup','checks_passed':CHECKS,
                'error':repr(error),'traceback':traceback.format_exc(),
                'reviewed_utc':datetime.now(timezone.utc).isoformat()}
    file_hash(Path(__file__))
    save(HERE/'FILE-HASHES.json',{'algorithm':'SHA-256','files':FILES})
    result['file_hash_manifest_sha256']=file_hash(HERE/'FILE-HASHES.json')
    save(HERE/'RESULT.json',result)
    if result['verdict']=='Pass':
        values=result['numeric_results']
        print(json.dumps(serializable({'verdict':'Pass','checks_passed':CHECKS,
              'wall':{k:v for k,v in values['wall'].items() if not k.startswith('by_')},
              'cpu':{k:v for k,v in values['cpu'].items() if not k.startswith('by_')},
              'without_largest_tree_wall':{k:v for k,v in values['without_largest_tree']['wall'].items() if not k.startswith('by_')},
              'all_five_o0_executables_byte_equal':result['e8_to_candidate_o0_retained_executables']['all_five_byte_equal'],
              'audited_files':len(FILES)}),ensure_ascii=False,indent=2))
    else:
        print(json.dumps(result,ensure_ascii=False,indent=2))
        raise SystemExit(1)
