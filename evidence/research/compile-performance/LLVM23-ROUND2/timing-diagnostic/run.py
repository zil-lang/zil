import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

out=Path('/tmp/zil-round2-diagnostic')
frozen=Path('/tmp/zil-llvm23-round2/rust-o0-incremental-query-plan')
plan=json.loads((frozen/'plan.json').read_text())
case=next(c for c in plan['cases'] if c['id']=='tree1024x8')
binary=Path('/workspace/scratch/ac5966c4c370/binaries/combined-6e8e63a5-timings')
work=out/'work';source=work/'src';source.mkdir(parents=True,exist_ok=False)
rows=[]
for i,(request,snapshot) in enumerate([('cold_process','base'),('unchanged','base'),('private_edit','private'),('public_edit','public')]):
    origin=frozen/case['root']/snapshot/'zil'
    files={p.relative_to(origin):p for p in origin.rglob('*') if p.is_file()}
    for p in source.rglob('*'):
        if p.is_file() and p.relative_to(source) not in files:p.unlink()
    for rel,p in files.items():
        dest=source/rel;dest.parent.mkdir(parents=True,exist_ok=True)
        if not dest.exists() or dest.read_bytes()!=p.read_bytes():shutil.copy2(p,dest)
    executable=work/'program'
    executable.unlink(missing_ok=True);executable.with_suffix('.ll').unlink(missing_ok=True)
    argv=[str(binary),'build',str(source/'main.zil'),'-o',str(executable),'--json','--timings','--backend','llvm-api','--clang',plan['binaries']['clang']['path'],'--opt','O0','--codegen-units','8','--codegen-jobs','4','--cache-dir',str(work/'source-candidates'),'--object-cache',str(work/'objects'),'--query-cache',str(work/'queries')]
    with (out/f'{i}-{request}.json').open('xb') as stdout,(out/f'{i}-{request}.stderr').open('xb') as stderr:
        p=subprocess.run(argv,stdout=stdout,stderr=stderr,preexec_fn=lambda:os.sched_setaffinity(0,[0,1,2,3]))
    report=json.loads((out/f'{i}-{request}.json').read_text())
    assert p.returncode==0 and report['status']=='Success' and report['verification_runs']==1
    assert report['query_cache']['state']==('hit' if request=='unchanged' else 'miss')
    checks=[]
    for n,sample in enumerate(case['inputs']):
        data=(frozen/sample['path']).read_bytes();expected=(frozen/sample['expected'][snapshot]).read_bytes()
        result=subprocess.run([str(executable)],input=data,capture_output=True)
        (out/f'{i}-oracle{n}.stdout').write_bytes(result.stdout);(out/f'{i}-oracle{n}.stderr').write_bytes(result.stderr)
        ok=result.returncode==0 and result.stdout==expected and result.stderr==b''
        assert ok
        checks.append({'input':sample['path'],'expected':sample['expected'][snapshot],'bytes':len(data),'pass':ok,'output_sha256':hashlib.sha256(result.stdout).hexdigest()})
    spans=report['timings']['spans']
    top=[s for s in spans if s['parent']==0]
    backend=[s for s in spans if s['name'] in ['machine_lower','llvm_text_render','cgu_partition','llvm_cache_prepare','object_emit','optimize','preflight','builder','lir_binding']]
    rows.append({'request':request,'snapshot':snapshot,'argv':argv,'status':report['status'],'verification_runs':report['verification_runs'],'query_cache':report['query_cache'],'object_cache':report['native_codegen']['object_cache'],'checks':checks,'compiler_request_seconds':spans[0]['wall_ns']/1e9,'top_level_spans':top,'selected_backend_spans':backend,'raw_report':f'{i}-{request}.json','raw_report_sha256':hashlib.sha256((out/f'{i}-{request}.json').read_bytes()).hexdigest()})
    print(request,rows[-1]['compiler_request_seconds'],[(s['name'],round(s['wall_ns']/1e6,2)) for s in top],flush=True)
receipt={'schema':'zil.compile-performance.diagnostic.v1','purpose':'Non-scoring stage attribution after all formal windows finished; profiling enabled, shared host and other audit work active. Not a speed benchmark.','source':'6e8e63a5ceae861447d0447c325bed78b59c3f42','binary':{'path':str(binary),'sha256':hashlib.sha256(binary.read_bytes()).hexdigest()},'features':['llvm-api','timings'],'case':case['id'],'frozen_plan_sha256':hashlib.sha256((frozen/'plan.json').read_bytes()).hexdigest(),'completed_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'rows':rows,'accounting':'Parent spans include children and parallel unit spans overlap. Sum of key_ns or per-unit work is not critical-path wall time.'}
(out/'RESULT.json').write_text(json.dumps(receipt,indent=2)+'\n')
