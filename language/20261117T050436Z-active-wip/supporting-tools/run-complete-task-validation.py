from pathlib import Path
import argparse,fcntl,json,subprocess,hashlib,shutil,time,sys
sys.path.insert(0,'/workspace/.zil-env')
from lossless_phase_logs import compress_phases
p=argparse.ArgumentParser();p.add_argument('--candidate',required=True);p.add_argument('--run',required=True);a=p.parse_args();repo=Path('/workspace/zil-alpha-candidate');run=repo/'out/runs'/a.run
rows=[]
with open('/workspace/.zil-env/language-build.lock','a') as lock:
 fcntl.flock(lock,fcntl.LOCK_EX);identity=json.loads((run/'compiler-build-receipt.json').read_text());assert identity['source_commit']==a.candidate and identity['status']=='Pass';assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()==a.candidate
 cli=run/'frozen-cli/zil';cli.parent.mkdir();shutil.copy2('/workspace/.zil-env/target-language-alpha/debug/zil',cli);assert hashlib.sha256(cli.read_bytes()).hexdigest()==identity['cli_sha256']
 steps=[]
 for profile in ('standard','alpha'):
  steps.append(('original-16-'+profile,['python3','/workspace/.zil-env/check-task-positive-fixtures.py','--repo',str(repo),'--run',a.run,'--cli',str(cli),'--work-profile',profile,'--jobs','2']))
 steps.append(('all-helper-sources',['python3','/workspace/.zil-env/check-all-task-helper-sources.py','--repo',str(repo),'--cli',str(cli),'--build-receipt',str(run/'compiler-build-receipt.json'),'--out',str(run/'all-task-helper-sources'),'--work-profile','standard','--jobs','2']))
 steps.append(('sdk-derived-sources',['python3','/workspace/.zil-env/check-sdk-derived-task-sources.py','--repo',str(repo),'--run',a.run]))
 steps.append(('task-owner-complete',['cargo','test','--locked','-p','zil-driver','--features','llvm-api','--test','task_owner_transport','--','--nocapture','--test-threads=1']))
 steps.append(('direct-native',['python3','/workspace/.zil-env/check-task-direct-existing-native.py','--repo',str(repo),'--run',a.run]))
 steps.append(('source-integrations',['cargo','test','--locked','--no-fail-fast','-p','zil-expand','--test','option_borrowed_input','--test','task_callable_source','--','--nocapture']))
 steps.append(('source-static-controls',['cargo','test','--locked','-p','zil-expand','--lib','static_result','--','--nocapture']))
 steps.append(('address-product-controls',['cargo','test','--locked','-p','zil-types','--lib','address_product_tests','--','--nocapture']))
 steps.append(('frame-consumer-controls',['cargo','test','--locked','-p','zil-analyze','--lib','frame_consumer_tests','--','--nocapture']))
 # The true aggregate Static probe must run before large SDK linking.
 d=run/'real-nested-static-probe';d.mkdir();source=d/'source.zil';shutil.copy2('/tmp/zil-nested-static-capture-probe/source.zil',source);cmd=[str(cli),'check',str(source),'--work-profile','standard','--emit-dir',str(d/'phases'),'--json'];r=subprocess.run(cmd,cwd=repo,capture_output=True);(d/'stdout.json').write_bytes(r.stdout);(d/'stderr.log').write_bytes(r.stderr);v=json.loads(r.stdout);(d/'receipt.json').write_text(json.dumps({'candidate':a.candidate,'cli_sha256':identity['cli_sha256'],'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'exit_code':r.returncode,'report':v},indent=2)+'\n');compress_phases(d);print('NESTED_PROBE',v.get('status'),v.get('phase'),flush=True)
 for label,cmd in steps:
  folder=run/label;folder.mkdir();start=time.time();print('START',label,flush=True)
  if label in ('direct-native','sdk-derived-sources'):fcntl.flock(lock,fcntl.LOCK_UN)
  with (folder/'test.log').open('w') as log:r=subprocess.run(cmd,cwd=repo,stdout=log,stderr=subprocess.STDOUT)
  if label in ('direct-native','sdk-derived-sources'):
   fcntl.flock(lock,fcntl.LOCK_EX);assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()==a.candidate;assert hashlib.sha256(cli.read_bytes()).hexdigest()==identity['cli_sha256']
  row={'candidate':a.candidate,'cli_sha256':identity['cli_sha256'],'command':cmd,'exit_code':r.returncode,'seconds':time.time()-start};(folder/'test-receipt.json').write_text(json.dumps(row,indent=2)+'\n');rows.append(dict(step=label,**row));(run/'complete-validation-execution.json').write_text(json.dumps({'rows':rows},indent=2)+'\n');print('END',label,r.returncode,flush=True)
  if label=='task-owner-complete':subprocess.run(['python3','/tmp/alpha-task-failure-inventory.py',str(folder),a.candidate],check=True)
