import pathlib,subprocess,json,hashlib,sys
base=pathlib.Path('/workspace/scratch/39be3faa4442/run-logs/review164/tool-controls-initial');base.mkdir()
repo=base/'tiny';repo.mkdir();(repo/'crates').mkdir();(repo/'crates/source.rs').write_text('initial\n')
def git(*args):
 r=subprocess.run(['git',*args],cwd=repo,capture_output=True,text=True,check=True);return r.stdout.strip()
git('init','-b','main');git('add','.');git('-c','user.name=Review','-c','user.email=review@example.invalid','commit','-m','control fixture')
commit=git('rev-parse','HEAD');git('branch','retained');bundle=base/'fixture.bundle';git('bundle','create',str(bundle),'--all')
sha=lambda p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
tools=pathlib.Path('/workspace/scratch/39be3faa4442/zil-restored/compose159/tools/language/164')
# Freeze the reviewed tool version so later integrator edits cannot alter this control.
for name in ('record.py','restore.py','readiness.py'):(base/name).write_bytes((tools/name).read_bytes())
rows=[]
zero=subprocess.run([sys.executable,str(base/'record.py'),'--out',str(base/'zero'), '--',sys.executable,'-c',"print('test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 100 filtered out; finished in 0s')"],cwd=repo,capture_output=True,text=True)
rows.append({'control':'synthetic_zero_test_exit0','exit_code':zero.returncode,'receipt':json.loads((base/'zero/command.json').read_text())})
(repo/'crates/source.rs').write_text('already dirty\n')
dirty=subprocess.run([sys.executable,str(base/'record.py'),'--out',str(base/'dirty'), '--',sys.executable,'-c',"from pathlib import Path;Path('crates/source.rs').write_text('changed during run\\n')"],cwd=repo,capture_output=True,text=True)
rows.append({'control':'preexisting_dirty_file_changes_content','exit_code':dirty.returncode,'receipt':json.loads((base/'dirty/command.json').read_text())})
refs=dict(line.split(' ',1)for line in git('for-each-ref','--format=%(refname) %(objectname)').splitlines())
for name,trees,listed in [('wrong-source',{'main':commit},refs),('empty-worktrees',{}, {})]:
 manifest={'source_commit':'0'*40,'bundle':{'file':bundle.name,'sha256':sha(bundle)},'worktrees':trees,'refs':listed}
 path=base/(name+'.json');path.write_text(json.dumps(manifest))
 r=subprocess.run([sys.executable,str(base/'restore.py'),'--manifest',str(path),'--destination',str(base/(name+'-restored'))],capture_output=True,text=True)
 (base/(name+'.stdout')).write_text(r.stdout);(base/(name+'.stderr')).write_text(r.stderr)
 rows.append({'control':name,'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr})
(base/'results.json').write_text(json.dumps({'scope':'Synthetic runner/restore controls, not language test evidence','tool_sha256':{n:sha(base/n)for n in ('record.py','restore.py','readiness.py')},'controls':rows},indent=2)+'\n')
print(json.dumps([{'control':r['control'],'exit_code':r['exit_code'],'status':r.get('receipt',{}).get('status')}for r in rows]))
