#!/usr/bin/env python3
"""Build and execute the same actual commit in two isolated Git worktrees."""
import concurrent.futures,hashlib,json,os,pathlib,subprocess,tempfile,sys
ROOT=pathlib.Path(__file__).resolve().parents[2]
(ROOT/'.work').mkdir(exist_ok=True)
base=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
worktrees=[]
def run(wt,n):
    target=wt/'target';cache=wt/'cache';output=wt/'out';output.mkdir()
    env=dict(os.environ);env['CARGO_TARGET_DIR']=str(target)
    p=subprocess.run(['cargo','build','--offline','--locked','-p','zil-cli'],cwd=wt,env=env,text=True,capture_output=True,timeout=180)
    assert p.returncode==0,p.stderr
    source=wt/'probe.zil';source.write_text(f'fn main() -> i32 {{ return {n} }}\n')
    exe=output/'probe';binary=target/'debug/zil'
    p=subprocess.run([str(binary),'build',str(source),'--output',str(exe),'--emit-dir',str(output/'stages'),'--cache-dir',str(cache),'--json'],cwd=wt,env=env,text=True,capture_output=True,timeout=30)
    assert p.returncode==0,(p.stdout,p.stderr)
    report=json.loads(p.stdout);assert report['status']=='Success',report
    p=subprocess.run([str(exe)],timeout=10);assert p.returncode==n,(n,p.returncode)
    return {'target':str(target),'cache':str(cache),'output':str(output),'return_value':p.returncode,'executable_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'assertions':4}
# The system temporary filesystem hosts only this fixture's owned worktrees.
# Avoid a second implicit rmtree hiding the first Git cleanup failure.
temp=pathlib.Path(tempfile.mkdtemp(prefix='zil-isolation-'))
results=[];errors=[];cleanup=[];assertions=0
def failure(error):
    return {'type':type(error).__name__,'message':str(error),
            'stdout':getattr(error,'stdout',None),'stderr':getattr(error,'stderr',None)}
try:
    for i in range(2):
        wt=temp/f'worktree-{i}'
        subprocess.run(['git','worktree','add','--detach',str(wt),base],cwd=ROOT,check=True,capture_output=True,text=True,timeout=30);worktrees.append(wt)
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        futures=[pool.submit(run,wt,42+i) for i,wt in enumerate(worktrees)]
        for future in futures:
            result=future.result();results.append(result);assertions+=result['assertions']
    for key in ['target','cache','output','executable_sha256']:
        assert len({r[key] for r in results})==2,key
        assertions+=1
except Exception as error:
    errors.append({'phase':'execution',**failure(error)})
finally:
    for wt in worktrees:
        command=['git','worktree','remove','--force',str(wt)]
        try:
            p=subprocess.run(command,cwd=ROOT,capture_output=True,text=True,timeout=30)
            item={'command':command,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr,'directory_remaining':wt.exists()}
            if p.returncode!=0 or wt.exists():errors.append({'phase':'worktree-cleanup',**item})
        except Exception as error:
            item={'command':command,**failure(error)};errors.append({'phase':'worktree-cleanup',**item})
        cleanup.append(item)
    if not any(item['phase']=='worktree-cleanup' for item in errors):
        try:temp.rmdir()
        except OSError as error:errors.append({'phase':'temporary-directory-cleanup',**failure(error)})
print(json.dumps({'status':'Fail' if errors else 'Pass','executed':len(results),'assertions':assertions,'candidate_commit':base,'evidence_level':'NativeSourceTwoWorktrees','results':results,'cleanup':cleanup,'errors':errors,'retained_directory':str(temp) if temp.exists() else None},default=str))
sys.exit(bool(errors))
