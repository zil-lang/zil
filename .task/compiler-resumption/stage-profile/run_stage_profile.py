#!/usr/bin/env python3
"""One fixed diagnostic cold/warm/private history through the real CLI.
Not an acceptance benchmark; creates a new output root and never edits corpus.
"""
import argparse, hashlib, json, os, pathlib, shutil, subprocess, time
p=argparse.ArgumentParser();p.add_argument('--cli',required=True,type=pathlib.Path);p.add_argument('--base-source',required=True,type=pathlib.Path);p.add_argument('--private-source',required=True,type=pathlib.Path);p.add_argument('--clang',required=True,type=pathlib.Path);p.add_argument('--out',required=True,type=pathlib.Path);p.add_argument('--cpu',default='0,1');p.add_argument('--merging',choices=['off','after','before-and-after'],default='after')
a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False);work=out/'work';work.mkdir();cpus={int(s) for s in a.cpu.split(',')};assert cpus<=os.sched_getaffinity(0);os.sched_setaffinity(0,cpus)
cli=a.cli.resolve();clang=a.clang.resolve();sources={'base':a.base_source.resolve(),'private':a.private_source.resolve()}
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
manifest={'purpose':'fixed three-request diagnostic history, not performance acceptance','cli':str(cli),'cli_sha256':sha(cli),'clang':str(clang),'clang_sha256':sha(clang),'source_hashes':{k:sha(v) for k,v in sources.items()},'cpus':sorted(cpus),'history':['cold/base','warm/base','private/private'],'source_paths':{k:str(v) for k,v in sources.items()},'object_phase_times':'individual inclusive wall spans; do not sum and subtract from request wall'}
(out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n');env=os.environ.copy();env['ZIL_PROFILE_STAGE_TIMING']='1'
for request,snapshot in [('cold','base'),('warm','base'),('private','private')]:
    source_bytes=sources[snapshot].read_bytes()
    if not (work/'main.zil').exists() or (work/'main.zil').read_bytes()!=source_bytes:(work/'main.zil').write_bytes(source_bytes)
    (work/'app').unlink(missing_ok=True);pathlib.Path(str(work/'app')+'.ll').unlink(missing_ok=True)
    argv=[str(cli),'build',str(work/'main.zil'),'--json','--opt','O2','--backend','llvm-api','--clang',str(clang),'--codegen-units','8','--codegen-jobs','2','--semantic-cache',str(work/'semantic'),'--object-cache',str(work/'objects'),'-o',str(work/'app'),'--incremental-o2','--function-merging',a.merging]
    log=out/request;log.mkdir();start=time.perf_counter_ns()
    with (log/'stdout').open('wb') as stdout,(log/'stderr').open('wb') as stderr:
        r=subprocess.run(argv,stdout=stdout,stderr=stderr,env=env,timeout=300)
    elapsed=time.perf_counter_ns()-start;report=json.loads((log/'stdout').read_text())
    row={'request':request,'argv':argv,'wall_ns':elapsed,'exit':r.returncode,'source_sha256':sha(work/'main.zil'),'status':report.get('status'),'verification_runs':report.get('verification_runs'),'native_codegen':report.get('native_codegen'),'disk_cache':report.get('disk_cache')}
    assert r.returncode==0 and report['status']=='Success' and report['verification_runs']==1,row
    row['elf_sha256']=sha(work/'app');row['sidecar_sha256']=sha(pathlib.Path(str(work/'app')+'.ll'));shutil.copy2(work/'app',log/'app');shutil.copy2(pathlib.Path(str(work/'app')+'.ll'),log/'app.ll')
    (log/'record.json').write_text(json.dumps(row,indent=2)+'\n');print(json.dumps({'request':request,'wall_ms':elapsed/1e6,'elf_sha256':row['elf_sha256'],'units':report['native_codegen']['actual_units'],'cache':report['native_codegen']['object_cache']}),flush=True)
