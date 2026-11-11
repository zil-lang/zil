#!/usr/bin/env python3
"""Independent, read-only audit of an already completed fixed screen."""
from pathlib import Path
import collections, hashlib, json, math, statistics

ROOT=Path('/workspace/scratch/abf690ce880c/out/request-cost')
RUN=ROOT/'screen'
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def read(path):return json.loads(Path(path).read_text())
def jsonl(path):return [json.loads(line) for line in Path(path).read_text().splitlines()]
def dump(path,value):Path(path).write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
plan=read(RUN/'plan.json');rows=jsonl(RUN/'rows.jsonl');pairs=jsonl(RUN/'pairs.jsonl');summary=read(RUN/'summary.json')
errors=[];checks=collections.Counter();pinned=[]
def check(ok,message):
    checks['assertions']+=1
    if not ok:errors.append(message)
check((RUN/'plan.json').read_bytes()==(ROOT/'screen-plan.json').read_bytes(),'Copied plan bytes differ')
attempt=read(ROOT/'screen-plan.json.attempt.json');check(attempt['plan_sha256']==sha(RUN/'plan.json'),'Attempt plan hash mismatch')
check(read(RUN/'completion.json')['plan_sha256']==sha(RUN/'plan.json'),'Completion plan hash mismatch')
for item in list(plan['tools'].values())+[plan['script'],plan['model']]+[s for snapshots in plan['sources'].values() for s in snapshots.values()]:
    actual=sha(item['path']);check(actual==item['sha256'],'Pinned hash mismatch: '+item['path'])
    pinned.append({'path':item['path'],'sha256':actual})
schedule=[(pair,arm) for pair in plan['pairs'] for arm in pair['arms']]
check(len(rows)==len(schedule)==84,'Request count');check(len(pairs)==len(plan['pairs'])==42,'Pair count')
check(read(RUN/'failures.json')==[],'Producer recorded failures')
native_count=0;pipeline_count=collections.Counter();cache_count=collections.Counter()
for i,(row,(pair,arm)) in enumerate(zip(rows,schedule)):
    for key in ['pair','case','jobs','request','snapshot','repetition']:
        check(row[key]==pair[key],f'Row {i} schedule {key}')
    check(row['arm']==arm,f'Row {i} arm order')
    log=RUN/'logs'/f'{i:03d}';check(Path(row['log'])==log,f'Row {i} log path')
    check(read(log/'record.json')==row,f'Row {i} record differs from JSONL')
    check(row['source_sha256']==plan['sources'][row['case']][row['snapshot']]['sha256'],f'Row {i} source hash')
    check(row['exit']==0 and not row['timeout'] and row['valid'] and row['errors']==[],f'Row {i} validity')
    check(all(math.isfinite(row[k]) and row[k]>0 for k in ['wall_ms','cpu_ms','rss_kib']),f'Row {i} metrics')
    report=read(log/'stdout');check(report['status']=='Success' and report['verification_runs']==1,f'Row {i} current verification')
    for field in ['native_codegen','query_events','disk_cache']:
        check(report[field]==row[field],f'Row {i} report {field}')
    work=RUN/'work'/row['case']/f'jobs{row["jobs"]}'/arm
    argv=[plan['tools'][arm]['path'],'build',str(work/'main.zil'),'--json','--opt','O2','--backend','llvm-api','--clang',plan['tools']['clang']['path'],'--codegen-units','8','--codegen-jobs',str(row['jobs']),'--semantic-cache',str(work/'semantic'),'--object-cache',str(work/'objects'),'-o',str(work/'app'),'--incremental-o2','--function-merging','after']
    check(row['argv']==argv,f'Row {i} argv')
    check(sha(log/'app')==row['elf_sha256'] and sha(log/'app.ll')==row['sidecar_sha256'],f'Row {i} retained artifact hashes')
    check(bool((log/'app').stat().st_mode&0o111),f'Row {i} retained executable permission')
    nc=report['native_codegen'];cache=nc['object_cache'];cache_count[cache['hits'],cache['misses']]+=1
    check(cache['enabled'] and cache['corrupt']==0 and cache['write_failures']==0,f'Row {i} cache integrity')
    if row['request'] in ['warm','edit-warm']:check(cache['hits']==nc['actual_units'] and cache['misses']==0,f'Row {i} warm hits')
    for obj in nc['object_reports']:
        pipeline_count[obj['pipeline']]+=1
        check(obj['pipeline']=='default<O2>|merge-functions=after|aliases=false' and obj['llvm_version']=='23.1.4',f'Row {i} actual LLVM/pipeline')
    check(len(row['native_checks'])==3,f'Row {i} native check count')
    for data,checkrow in zip(plan['inputs_hex'],row['native_checks']):
        nlog=log/('native-'+data);want=bytes.fromhex(plan['oracles'][row['case']][row['snapshot']][data])
        check((nlog/'stdin').read_bytes()==bytes.fromhex(data),f'Row {i} native input')
        check((nlog/'stdout').read_bytes()==want and (nlog/'stderr').read_bytes()==b'',f'Row {i} raw native oracle')
        check(checkrow['input_hex']==data and checkrow['expected_hex']==want.hex() and checkrow['actual_hex']==want.hex() and checkrow['exit']==0 and checkrow['pass'],f'Row {i} native record')
        native_count+=1

def side_material(data):
    lines=data.splitlines(keepends=True);check([i for i,l in enumerate(lines) if l.startswith(b'; verified-fingerprint: ')]==[1],'Unexpected sidecar comment location')
    return b''.join(lines[:1]+lines[2:])
elf_equal=0;side_raw_equal=0;side_material_equal=0;cache_equal=0
for position,(pair,declared) in enumerate(zip(plan['pairs'],pairs)):
    rr=[r for r in rows if r['pair']==pair['pair']];check(len(rr)==2,f'Pair {position} count')
    by={r['arm']:r for r in rr};b=by['base'];c=by['cand'];bl=Path(b['log']);cl=Path(c['log'])
    ee=(bl/'app').read_bytes()==(cl/'app').read_bytes();bs=(bl/'app.ll').read_bytes();cs=(cl/'app.ll').read_bytes();se=side_material(bs)==side_material(cs)
    def topology(r):
        nc=r['native_codegen'];oc=nc['object_cache'];return [nc['actual_units'],nc['jobs'],nc['fallback']]+[oc[k] for k in ['hits','misses','corrupt','stored','write_failures']]
    ce=topology(b)==topology(c)
    check(ee and se and ce,f'Pair {position} artifact/cache equality');check(declared['valid'] and declared['elf_equal']==ee and declared['sidecar_raw_equal']==(bs==cs) and declared['sidecar_material_equal']==se and declared['cache_topology_equal']==ce,f'Pair {position} producer summary')
    check(abs(declared['saved_ms']-(b['wall_ms']-c['wall_ms']))<1e-9,f'Pair {position} wall delta')
    elf_equal+=ee;side_raw_equal+=bs==cs;side_material_equal+=se;cache_equal+=ce

group=collections.defaultdict(list)
for r in rows:group[r['case'],r['jobs'],r['request'],r['arm']].append(r)
cells=[]
def metrics(values):return {'n':len(values),'mean':statistics.mean(values),'median':statistics.median(values),'min':min(values),'max':max(values),'sample_sd':statistics.stdev(values) if len(values)>1 else None,'values':values}
for case,jobs in [('shared4096',2),('shared1024',2),('owned8',2),('shared4096',1)]:
    for req in ['cold','warm','private','edit-warm']:
        cell={'case':case,'jobs':jobs,'request':req,'arms':{}}
        old=next(x for x in summary['cells'] if (x['case'],x['jobs'],x['request'])==(case,jobs,req))
        for arm in ['base','cand']:
            rr=group[case,jobs,req,arm];stats={key:metrics([r[field] for r in rr]) for key,field in [('wall_ms','wall_ms'),('cpu_ms','cpu_ms'),('rss_kib','rss_kib')]}
            cell['arms'][arm]=stats;check(stats['wall_ms']['n']==old['arms'][arm]['expected_n'],f'{case}/{jobs}/{req}/{arm} samples')
            check(abs(stats['wall_ms']['mean']-old['arms'][arm]['mean_ms'])<1e-9,f'{case}/{jobs}/{req}/{arm} mean')
        for key in ['wall_ms','cpu_ms','rss_kib']:
            b=cell['arms']['base'][key]['mean'];c=cell['arms']['cand'][key]['mean'];cell[key+'_candidate_change_pct']=100*(c/b-1)
        cells.append(cell)

# Hash all retained per-request files, including artifacts, native I/O and reports.
# Save only a canonical inventory digest here to keep the audit compact.
inventory=[]
for path in sorted((RUN/'logs').rglob('*')):
    if path.is_file():inventory.append([str(path.relative_to(RUN)),path.stat().st_size,sha(path)])
inventory_bytes=json.dumps(inventory,separators=(',',':'),ensure_ascii=False).encode()
audit={'decision':'Stop; candidate not adopted; no expansion or repeat sampling','candidate_source':plan['candidate_source'],'baseline_source':plan['baseline_source'],
    'counts':{'requests':len(rows),'pairs':len(pairs),'current_verify_one':sum(r['verification_runs']==1 for r in rows),'native_outputs':native_count,'elf_equal_pairs':elf_equal,'sidecar_raw_equal_pairs':side_raw_equal,'sidecar_material_equal_pairs':side_material_equal,'cache_equal_pairs':cache_equal,'producer_failures':len(summary['failures']),'audit_errors':len(errors),'audit_assertions':checks['assertions']},
    'hash_checks':{'plan_sha256':sha(RUN/'plan.json'),'attempt_sha256':sha(ROOT/'screen-plan.json.attempt.json'),'rows_sha256':sha(RUN/'rows.jsonl'),'pairs_sha256':sha(RUN/'pairs.jsonl'),'summary_sha256':sha(RUN/'summary.json'),'retained_files_count':len(inventory),'retained_files_bytes':sum(x[1] for x in inventory),'retained_files_inventory_sha256':hashlib.sha256(inventory_bytes).hexdigest(),'pinned_inputs':pinned},
    'pipeline_counts':dict(pipeline_count),'cells':cells,'errors':errors,
    'rss_observation':{'mode_kib':collections.Counter(r['rss_kib'] for r in rows).most_common(1)[0][0],'mode_count':collections.Counter(r['rss_kib'] for r in rows).most_common(1)[0][1],'interpretation':'wait4 ru_maxrss observation only; repeated identical values do not establish exact live memory or a reproducible memory win'},
    'limits':['All means retain every preregistered observation. Medians are descriptive, not replacement acceptance statistics.','Cold/private each have one observation per arm. No claim of stable causal regression or stable speedup.','Wall tails coincide with elevated CPU values in both arms; no attribution to pure waiting, allocator contention, scheduler, frequency or any unmeasured cause.','This screen omits original six-request/all-mode/eight-case coverage and has no weighted net-performance claim.','Original gates, prior failures and this failed-benefit screen remain separate; no new test or native performance sample was executed for this audit.','Original working source paths were overwritten by later snapshots; historical source bytes are supported by pinned corpus+recorded per-row hashes+frozen script, not asserted to be independently retained in every log.']}
dump(ROOT/'performance-audit.json',audit)
print(json.dumps({'counts':audit['counts'],'rss_observation':audit['rss_observation'],'errors':errors,'audit':str(ROOT/'performance-audit.json')},ensure_ascii=False,indent=2))
if errors:raise SystemExit(1)
