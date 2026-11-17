"""Execute the two existing direct_cleanup SDK cases independently of first assertions."""
from pathlib import Path
import argparse,fcntl,hashlib,json,os,shlex,subprocess
from lossless_phase_logs import compress_phases
p=argparse.ArgumentParser();p.add_argument('--repo',type=Path,required=True);p.add_argument('--run',required=True);a=p.parse_args();run=a.repo/'out/runs'/a.run
with open('/workspace/.zil-env/language-build.lock','a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX);identity=json.loads((run/'compiler-build-receipt.json').read_text());census=json.loads((run/'original-task-positive-fixtures/receipt.json').read_text());cli=run/'frozen-cli/zil';assert identity['source_commit']==census['candidate']==subprocess.check_output(['git','rev-parse','HEAD'],cwd=a.repo,text=True).strip();assert hashlib.sha256(cli.read_bytes()).hexdigest()==identity['cli_sha256']==census['cli_sha256']
 out=run/'existing-direct-cleanup-native';out.mkdir();observer=a.repo/'tests/language-readiness/task-owner-transport/direct-frame-observer.c';cc=os.environ.get('ZIL_CLANG','clang');rows=[];skipped=[]
 for name,frames in [('direct-default-local',1),('adapter-default-local',2)]:
  source_row=next(r for r in census['rows'] if r['case']==name)
  if not source_row['verified']:skipped.append({'case':name,'reason':'Original Source has not passed independent Verify','source_row':source_row});continue
  d=out/name;d.mkdir();source=run/'original-task-positive-fixtures'/name/'source.zil';assert hashlib.sha256(source.read_bytes()).hexdigest()==source_row['source_sha256'];obj=d/'observer.o';command=[cc,'-std=c11','-Wall','-Wextra','-Werror','-c','-DZIL_TASK_EXPECTED_QUIET=1','-DZIL_TASK_DEFAULT_LOCAL=1',f'-DZIL_TASK_EXPECTED_FRAMES={frames}',str(observer),'-o',str(obj)];r=subprocess.run(command,capture_output=True);(d/'observer.stderr').write_bytes(r.stderr);assert r.returncode==0,r.stderr
  wrapper=d/'clang-observer.sh';wrapper.write_text('#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in "$@"; do\ncase "$arg" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ "$link" = 1 ] && [ "$input" = 1 ]; then\nexec '+shlex.quote(cc)+' "$@" -x none '+shlex.quote(str(obj))+' -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec '+shlex.quote(cc)+' "$@"\n');wrapper.chmod(0o755)
  for backend in ('clang','llvm-api'):
   for opt in ('O0','O2'):
    cell=d/f'{backend}-{opt}';cell.mkdir();exe=cell/'program';cmd=[str(cli),'build',str(source),'--work-profile','standard','--backend',backend,'--opt',opt,'--clang',str(wrapper),'--emit-dir',str(cell/'phases'),'-o',str(exe),'--json'];r=subprocess.run(cmd,cwd=a.repo,capture_output=True,timeout=300);(cell/'stdout.json').write_bytes(r.stdout);(cell/'stderr.txt').write_bytes(r.stderr);v=json.loads(r.stdout);row={'case':name,'backend':backend,'opt':opt,'source_sha256':source_row['source_sha256'],'command':cmd,'build_exit':r.returncode,'report':v,'executions':[],'passed':False}
    if r.returncode==0:
     assert exe.exists();assert v.get('verification_runs')==1 and v['native_codegen']['backend']==backend and v['native_codegen'].get('fallback') is None
     for failure in range(frames+1):
      x=subprocess.run([str(exe)],env=dict(os.environ,ZIL_TASK_AGGREGATE_FAIL_AT=str(failure)),capture_output=True,timeout=20);(cell/f'failure-{failure}.stdout.bin').write_bytes(x.stdout);(cell/f'failure-{failure}.stderr.bin').write_bytes(x.stderr);row['executions'].append({'failure':failure,'actual_exit':x.returncode,'expected_exit':int(failure!=0),'stdout_hex':x.stdout.hex(),'stderr_hex':x.stderr.hex(),'passed':x.returncode==int(failure!=0) and not x.stdout and not x.stderr})
     row['executable_sha256']=hashlib.sha256(exe.read_bytes()).hexdigest();row['passed']=all(x['passed'] for x in row['executions'])
    compress_phases(cell);(cell/'receipt.json').write_text(json.dumps(row,indent=2)+'\n');rows.append(row);print(name,backend,opt,row['passed'],flush=True)
 receipt={'candidate':identity['source_commit'],'cli_sha256':identity['cli_sha256'],'observer_source_sha256':hashlib.sha256(observer.read_bytes()).hexdigest(),'original_test_sha256':hashlib.sha256((a.repo/'crates/zil-driver/tests/task_owner_transport/direct_cleanup.rs').read_bytes()).hexdigest(),'scope':'Exact existing Source, direct observer and original allocation failure/Quiet/output oracle; supplements original SDK assertions','rows':rows,'skipped':skipped};(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');raise SystemExit(0 if rows and not skipped and all(r['passed'] for r in rows) else 1)
