#!/usr/bin/env python3
"""Run the original Task helper native matrix per Source, without first-assert masking."""
from pathlib import Path
import argparse,concurrent.futures,fcntl,hashlib,json,os,shlex,subprocess
from lossless_phase_logs import compress_phases
p=argparse.ArgumentParser();p.add_argument('--repo',type=Path,required=True);p.add_argument('--run',required=True);p.add_argument('--jobs',type=int,default=2);p.add_argument('--case-prefix');p.add_argument('--exact-simple-fixtures',action='store_true');a=p.parse_args()
b=a.repo/'out/runs'/a.run
with open('/workspace/.zil-env/language-build.lock','a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX)
 identity=json.loads((b/'compiler-build-receipt.json').read_text());census=json.loads((b/('original-task-positive-fixtures/receipt.json' if a.exact_simple_fixtures else 'all-task-helper-sources/receipt.json')).read_text());candidate=subprocess.check_output(['git','rev-parse','HEAD'],cwd=a.repo,text=True).strip();assert candidate==identity['source_commit']==census['candidate']
 cli=b/'all-task-helper-sources/zil';assert hashlib.sha256(cli.read_bytes()).hexdigest()==identity['cli_sha256']==census['cli_sha256']
 out=b/('original-simple-task-native' if a.exact_simple_fixtures else 'all-task-helper-native');out.mkdir(exist_ok=False);cc=os.environ.get('ZIL_CLANG','clang');observer=a.repo/'tests/language-readiness/task-aggregate-results/frame-observer.c';observer_hash=hashlib.sha256(observer.read_bytes()).hexdigest()
 required={'returned-task-result-fnonce','static-callable-input'} if a.exact_simple_fixtures else None
 selected=[r for r in census['rows'] if r['status']=='Success' and r['verification_runs']==1 and (required is None or r['case'] in required) and (a.case_prefix is None or r['case'].startswith(a.case_prefix))]
 def case(row):
  name=row['case'];d=out/name;d.mkdir();source=b/('original-task-positive-fixtures' if a.exact_simple_fixtures else 'all-task-helper-sources')/name/'source.zil';assert hashlib.sha256(source.read_bytes()).hexdigest()==row['source_sha256']
  # Exact original retained_task_fnonce_keeps_original_pc_inputs_and_one_cleanup_native oracle.
  expected=b'' if a.exact_simple_fixtures or name in ('basic','closed-owner') else b'A';quiet=int(name.startswith('static-frame-owning-'));calls=2 if name=='static-frame-owning-fnmut' else 1 if name in ('static-frame-owning-fnmut-cancel-after-second-pending','static-frame-owning-fnonce') else 0
  obj=d/'observer.o';cmd=[cc,'-std=c11','-Wall','-Wextra','-Werror','-DZIL_TASK_EXPECTED_FRAMES=1',f'-DZIL_TASK_EXPECTED_QUIET={quiet}',f'-DZIL_TASK_EXPECTED_QUIET_CALLS={calls}','-c',str(observer),'-o',str(obj)];r=subprocess.run(cmd,capture_output=True);(d/'observer.stderr').write_bytes(r.stderr);assert r.returncode==0,r.stderr.decode()
  wrapper=d/'clang-observer.sh';wrapper.write_text('#!/bin/sh\nset -eu\nlink=1\ninput=0\nfor arg in "$@"; do\ncase "$arg" in -c|-S|-E|--version) link=0 ;; *.o|*.obj|*.ll) input=1 ;; esac\ndone\nif [ "$link" = 1 ] && [ "$input" = 1 ]; then\nexec '+shlex.quote(cc)+' "$@" -x none '+shlex.quote(str(obj))+' -Wl,--wrap=calloc -Wl,--wrap=free\nfi\nexec '+shlex.quote(cc)+' "$@"\n');wrapper.chmod(0o755);results=[]
  for backend in ['clang','llvm-api']:
   for opt in ['O0','O2']:
    cell=d/f'{backend}-{opt}';cell.mkdir();exe=cell/'program';argv=[str(cli),'build',str(source),'--work-profile','standard','--backend',backend,'--opt',opt,'--clang',str(wrapper),'--emit-dir',str(cell/'phases'),'-o',str(exe),'--json'];r=subprocess.run(argv,cwd=a.repo,capture_output=True,timeout=240);(cell/'build.json').write_bytes(r.stdout);(cell/'build.stderr').write_bytes(r.stderr);v=json.loads(r.stdout)
    item={'case':name,'backend':backend,'opt':opt,'source_sha256':row['source_sha256'],'command':argv,'build_exit':r.returncode,'status':v.get('status'),'diagnostics':v.get('diagnostics'),'observer_command':cmd,'executions':[],'passed':False}
    if r.returncode==0 and exe.exists():
     cg=v.get('native_codegen') or {};assert v.get('verification_runs')==1 and cg.get('backend')==backend and cg.get('fallback') is None
     for label,env,code in [('normal',{},0),('allocation-failure',{'ZIL_TASK_AGGREGATE_FAIL':'1'},1)]:
      x=subprocess.run([str(exe)],capture_output=True,env=dict(os.environ,**env),timeout=20);(cell/f'{label}.stdout.bin').write_bytes(x.stdout);(cell/f'{label}.stderr.bin').write_bytes(x.stderr);item['executions'].append({'label':label,'environment':env,'expected_exit':code,'actual_exit':x.returncode,'expected_stdout_hex':expected.hex(),'actual_stdout_hex':x.stdout.hex(),'stderr_hex':x.stderr.hex(),'passed':x.returncode==code and x.stdout==expected and not x.stderr})
     item['executable_sha256']=hashlib.sha256(exe.read_bytes()).hexdigest();item['passed']=all(x['passed'] for x in item['executions'])
    compress_phases(cell);(cell/'receipt.json').write_text(json.dumps(item,indent=2)+'\n');results.append(item);print(name,backend,opt,item['status'],item['passed'],flush=True)
  return results
 with concurrent.futures.ThreadPoolExecutor(max_workers=a.jobs) as pool:rows=[x for group in pool.map(case,selected) for x in group]
 receipt={'candidate':candidate,'cli_sha256':identity['cli_sha256'],'work_profile':'standard','observer_source_sha256':observer_hash,'original_driver_sha256':census.get('original_helper_file_sha256'),'contract':'Exact original existing helper Sources and SDK observer/output/Quiet/fail1 oracles; per-case execution supplements and does not replace original SDK assertions or final Alpha native plan','selected_source_cases':len(selected),'case_prefix':a.case_prefix,'exact_original_simple_fixtures':a.exact_simple_fixtures,'required_source_cases':sorted(required) if required is not None else None,'builds_selected':len(rows),'builds_passed':sum(r['build_exit']==0 for r in rows),'rows':rows};(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print('NATIVE_CENSUS_COMPLETE',receipt['selected_source_cases'],sum(r['passed'] for r in rows),'/',len(rows),flush=True)
 raise SystemExit(0 if rows and (required is None or {r['case'] for r in rows}==required) and all(r['passed'] for r in rows) else 1)
