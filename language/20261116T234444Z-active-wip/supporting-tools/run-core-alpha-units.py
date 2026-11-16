import argparse,pathlib,subprocess,json,concurrent.futures,hashlib,time,fcntl,re
p=argparse.ArgumentParser();p.add_argument('--candidate',required=True);p.add_argument('--out',type=pathlib.Path,required=True);a=p.parse_args();repo=pathlib.Path('/workspace/zil-alpha-candidate');out=a.out;out.mkdir(parents=True,exist_ok=False)
with open('/workspace/.zil-env/language-build.lock','a')as lock:
 fcntl.flock(lock,fcntl.LOCK_EX)
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip()==a.candidate
 assert not subprocess.check_output(['git','status','--porcelain'],cwd=repo,text=True).strip()
 argv=['cargo','test','--locked','--no-run','--lib','--message-format=json']
 for name in ['zil-ir','zil-expand','zil-types','zil-analyze','zil-verify']:argv+=['-p',name]
 with (out/'compile.stdout').open('w')as stdout,(out/'compile.stderr').open('w')as stderr:r=subprocess.run(argv,cwd=repo,stdout=stdout,stderr=stderr)
 (out/'compile-receipt.json').write_text(json.dumps({'command':argv,'source_commit':a.candidate,'exit':r.returncode}))
 if r.returncode:print('UNIT_COMPILE_FAIL',r.returncode);raise SystemExit(r.returncode)
 exes={}
 for line in (out/'compile.stdout').read_text().splitlines():
  try:v=json.loads(line)
  except ValueError:continue
  if v.get('reason')=='compiler-artifact' and v.get('executable') and v.get('profile',{}).get('test') and 'lib'in v['target']['kind']:exes[v['target']['name']]=v['executable']
 assert len(exes)==5,exes
 def run(item):
  name,exe=item;d=out/name;d.mkdir();cmd=[exe,'--test-threads=2','--show-output'];start=time.time()
  with (d/'stdout.log').open('w')as stdout,(d/'stderr.log').open('w')as stderr:r=subprocess.run(cmd,cwd=repo,stdout=stdout,stderr=stderr,timeout=900)
  text=(d/'stdout.log').read_text();m=re.search(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',text)
  row={'library':name,'exit':r.returncode,'command':cmd,'executable_sha256':hashlib.sha256(pathlib.Path(exe).read_bytes()).hexdigest(),'seconds':time.time()-start,'passed':int(m[1])if m else None,'failed':int(m[2])if m else None,'ignored':int(m[3])if m else None}
  (d/'receipt.json').write_text(json.dumps(row));print(json.dumps(row),flush=True);return row
 with concurrent.futures.ThreadPoolExecutor(max_workers=2)as pool:rows=list(pool.map(run,exes.items()))
 (out/'receipt.json').write_text(json.dumps({'candidate':a.candidate,'rows':rows},indent=2))
raise SystemExit(0 if len(rows) == 5 and all(row['exit'] == 0 and isinstance(row['passed'], int) and row['passed'] > 0 and row['failed'] == 0 for row in rows) else 1)
