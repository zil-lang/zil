from pathlib import Path
import argparse,fcntl,json,subprocess,hashlib,sys
sys.path.insert(0,'/workspace/.zil-env')
from lossless_phase_logs import compress_phases
p=argparse.ArgumentParser();p.add_argument('--run',required=True);a=p.parse_args();repo=Path('/workspace/zil-alpha-candidate');run=repo/'out/runs'/a.run;out=run/'real-nested-static-native';out.mkdir()
with open('/workspace/.zil-env/language-build.lock','a')as lock:
 fcntl.flock(lock,fcntl.LOCK_EX);identity=json.loads((run/'compiler-build-receipt.json').read_text());probe=json.loads((run/'real-nested-static-probe/receipt.json').read_text());source=run/'real-nested-static-probe/source.zil';cli=run/'frozen-cli/zil'
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()==identity['source_commit']
 assert identity['status']=='Pass'and probe['candidate']==identity['source_commit']and probe['cli_sha256']==identity['cli_sha256'];assert hashlib.sha256(cli.read_bytes()).hexdigest()==identity['cli_sha256'];assert hashlib.sha256(source.read_bytes()).hexdigest()==probe['source_sha256']=='528d69dc979d081fe8ad35cc30d9b4fba34e522011af0fd4bf22f78244c83544'
 assert probe['exit_code']==0 and probe['report']['status']=='Success' and probe['report']['verification_runs']==1,'No verified original positive baseline: native cannot pass or be skipped as success'
 rows=[]
 for backend in ('clang','llvm-api'):
  for opt in ('O0','O2'):
   d=out/f'{backend}-{opt}';d.mkdir();exe=d/'program';cmd=[str(cli),'build',str(source),'--work-profile','standard','--backend',backend,'--opt',opt,'--clang','/workspace/.zil-env/llvm-23.1.4/bin/clang','--emit-dir',str(d/'phases'),'-o',str(exe),'--json'];r=subprocess.run(cmd,cwd=repo,capture_output=True,timeout=240);(d/'build.json').write_bytes(r.stdout);(d/'build.stderr').write_bytes(r.stderr);report=json.loads(r.stdout);cg=report.get('native_codegen')or{};build_ok=r.returncode==0 and report.get('status')=='Success'and report.get('verification_runs')==1 and cg.get('backend')==backend and cg.get('fallback')is None
   row={'backend':backend,'opt':opt,'source_sha256':probe['source_sha256'],'command':cmd,'build_exit':r.returncode,'build_passed':build_ok,'status':report.get('status'),'executions':[],'passed':False}
   if build_ok:
    execution=subprocess.run([str(exe)],capture_output=True,timeout=20);actual={'expected_exit':0,'actual_exit':execution.returncode,'expected_stdout_hex':'','actual_stdout_hex':execution.stdout.hex(),'stderr_hex':execution.stderr.hex(),'passed':execution.returncode==0 and execution.stdout==b''and execution.stderr==b''};(d/'execution.stdout').write_bytes(execution.stdout);(d/'execution.stderr').write_bytes(execution.stderr);row['executions'].append(actual);row['executable_sha256']=hashlib.sha256(exe.read_bytes()).hexdigest();row['passed']=actual['passed']
   compress_phases(d);(d/'receipt.json').write_text(json.dumps(row,indent=2)+'\n');rows.append(row);print(json.dumps({'backend':backend,'opt':opt,'build_passed':build_ok,'actual_executions':len(row['executions']),'passed':row['passed']}),flush=True)
 receipt={'candidate':identity['source_commit'],'cli_sha256':identity['cli_sha256'],'source_sha256':probe['source_sha256'],'work_profile':'standard','contract':'Exact original528d Source; real make/call/byte observer returning0 with empty output. No Source rewriting.','failure_injection':'Not applicable: exact original program contains no Task, heap owner or allocator failure branch; no synthetic failure execution claimed.','rows':rows,'builds_selected':4,'builds_passed':sum(x['build_passed']for x in rows),'executions_selected':sum(len(x['executions'])for x in rows),'executions_passed':sum(y['passed']for x in rows for y in x['executions']),'passed':len(rows)==4 and all(x['passed']for x in rows),'alpha_acceptance_claim':False};(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
raise SystemExit(0 if receipt['passed']else 1)
