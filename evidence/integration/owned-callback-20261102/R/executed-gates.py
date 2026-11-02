import subprocess,json,os,sys,re,time,signal,hashlib
from pathlib import Path
repo=Path('/workspace/zil-core')
out=Path('/workspace/.zil-env/owned-callback/acceptance-R')
source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()
assert source=='288e29628389d83a335783d5488be39f22b2008b',source
assert not subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True).strip()
(out/'source-commit.txt').write_text(source+'\n')
report={'source_commit':source,'scope':'formal common-candidate gates','checks':[]}
stages=[('fmt',['cargo','fmt','--all','--','--check'],120,'target-core'),('workspace',['cargo','test','--offline','--locked','--workspace','--no-fail-fast'],1800,'target-core'),('clippy',['cargo','clippy','--offline','--locked','--workspace','--all-targets','--','-D','warnings'],600,'target-core'),('reference',['cargo','test','--offline','--locked','--workspace','--no-default-features','--no-fail-fast'],1800,'target-core-reference'),('text-reference',['cargo','test','--offline','--locked','-p','zil-llvm-api','--features','text-reference'],300,'target-core'),('migration',['python3','tools/llvm/test_validate_migration.py'],300,'target-core'),('transport',['python3','tools/ci/test_evidence_transport.py'],300,'target-core')]
for name,cmd,timeout,target in stages:
 env=dict(os.environ,CARGO_TARGET_DIR='/workspace/.zil-env/'+target,ZIL_TEST_REPORT_DIR=str(out/'native'/name))
 log=out/(name+'.log');start=time.monotonic();expired=False
 with log.open('w') as f:
  proc=subprocess.Popen(cmd,cwd=repo,env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
  try: code=proc.wait(timeout=timeout)
  except subprocess.TimeoutExpired:
   expired=True;os.killpg(proc.pid,signal.SIGTERM)
   try: code=proc.wait(timeout=10)
   except subprocess.TimeoutExpired: os.killpg(proc.pid,signal.SIGKILL);code=proc.wait()
 body=log.read_text(errors='replace')
 rows=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;',body)
 totals=dict(zip(['passed','failed','ignored','measured','filtered'],[sum(int(row[i]) for row in rows) for i in range(5)]))
 item={'name':name,'command':cmd,'exit_code':code,'timeout':expired,'elapsed_seconds':round(time.monotonic()-start,3),'log_sha256':hashlib.sha256(log.read_bytes()).hexdigest(),'test_summaries':len(rows),'totals':totals}
 if name in ['workspace','reference','text-reference'] and (not rows or totals['failed'] or totals['ignored'] or totals['filtered']): item['status']='Fail'
 else: item['status']='Pass' if code==0 and not expired else 'Fail'
 report['checks'].append(item);report['status']='Running' if item['status']=='Pass' else 'Fail';(out/'RESULT.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(item),flush=True)
 if item['status']!='Pass': sys.exit(1)
report['status']='Pass';(out/'RESULT.json').write_text(json.dumps(report,indent=2)+'\n')
