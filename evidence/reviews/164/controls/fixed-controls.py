import pathlib,subprocess,json,hashlib,sys,shutil
root=pathlib.Path('/workspace/scratch/39be3faa4442/run-logs/review164')
base=root/'tool-controls-fixed';base.mkdir()
old=root/'tool-controls-initial';repo=old/'tiny'
tools=pathlib.Path('/workspace/scratch/39be3faa4442/zil-restored/compose159/tools/language/164')
for name in ('record.py','restore.py','readiness.py'):(base/name).write_bytes((tools/name).read_bytes())
sha=lambda p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
def git(*args):return subprocess.check_output(['git',*args],cwd=repo,text=True).strip()
rows=[]
git('restore','crates/source.rs')
zero=subprocess.run([sys.executable,str(base/'record.py'),'--out',str(base/'zero'),'--require-tests','--',sys.executable,'-c',"print('test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 100 filtered out; finished in 0s')"],cwd=repo,capture_output=True,text=True)
r=json.loads((base/'zero/command.json').read_text());assert zero.returncode!=0 and r['status']=='Fail'
rows.append({'control':'synthetic_zero_tests_with_require_tests','exit_code':zero.returncode,'receipt':r})
(repo/'crates/source.rs').write_text('already dirty\n')
dirty=subprocess.run([sys.executable,str(base/'record.py'),'--out',str(base/'dirty'),'--',sys.executable,'-c',"from pathlib import Path;Path('crates/source.rs').write_text('changed during run\\n')"],cwd=repo,capture_output=True,text=True)
r=json.loads((base/'dirty/command.json').read_text());assert dirty.returncode==2 and r['status']=='DirtySources';assert (repo/'crates/source.rs').read_text()=='already dirty\n'
rows.append({'control':'dirty_source_rejects_before_execution','exit_code':dirty.returncode,'receipt':r})
git('restore','crates/source.rs')
build=subprocess.run([sys.executable,str(base/'record.py'),'--out',str(base/'ordinary'),'--',sys.executable,'-c','pass'],cwd=repo,capture_output=True,text=True)
r=json.loads((base/'ordinary/command.json').read_text());assert build.returncode==0 and r['status']=='Pass'
rows.append({'control':'ordinary_exit0_command_not_requiring_tests','exit_code':build.returncode,'receipt':r})
refs=dict(line.split(' ',1)for line in git('for-each-ref','--format=%(refname) %(objectname)').splitlines());commit=git('rev-parse','HEAD')
shutil.copyfile(old/'fixture.bundle',base/'fixture.bundle')
for name,source,trees,listed,expected in [('wrong-source','0'*40,{'main':commit},refs,False),('empty-worktrees',commit,{}, {},False),('missing-retained-ref',commit,{'main':commit},{'refs/heads/main':commit},False),('valid-complete',commit,{'main':commit},refs,True)]:
 manifest={'source_commit':source,'bundle':{'file':'fixture.bundle','sha256':sha(base/'fixture.bundle')},'worktrees':trees,'refs':listed}
 path=base/(name+'.json');path.write_text(json.dumps(manifest))
 r=subprocess.run([sys.executable,str(base/'restore.py'),'--manifest',str(path),'--destination',str(base/(name+'-restored'))],capture_output=True,text=True)
 (base/(name+'.stdout')).write_text(r.stdout);(base/(name+'.stderr')).write_text(r.stderr)
 assert (r.returncode==0)==expected,(name,r.stdout,r.stderr)
 rows.append({'control':name,'exit_code':r.returncode,'expected_accept':expected,'stdout':r.stdout,'stderr':r.stderr})
probe=repo/'tools/research/159';probe.mkdir(parents=True,exist_ok=True);(probe/'readiness.py').write_text('gaps={}\n')
fake=base/'fake-cli';fake.write_text('#!/usr/bin/env python3\nraise RuntimeError("should never run empty corpus")\n');fake.chmod(0o755)
r=subprocess.run([sys.executable,str(base/'readiness.py'),'--repo',str(repo),'--cli',str(fake),'--out',str(base/'empty-corpus'),'--baseline'],capture_output=True,text=True)
summary=json.loads((base/'empty-corpus/summary.json').read_text());assert r.returncode!=0 and summary['status']=='Fail' and 'all six' in summary['error']
rows.append({'control':'empty_corpus_rejects_and_records_fail','exit_code':r.returncode,'summary':summary})
report={'scope':'Synthetic metadata-tool controls; not language tests','reviewed_code_commit':'e08ee25ff7471640708acfc2ceef83d7ba484bae','tool_sha256':{n:sha(base/n)for n in ('record.py','restore.py','readiness.py')},'controls':rows,'status':'Pass'}
(base/'results.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'status':'Pass','controls':len(rows),'results':str(base/'results.json')}))
