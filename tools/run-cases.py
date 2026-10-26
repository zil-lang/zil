#!/usr/bin/env python3
"""Execute registered tests and record actual results. Missing/empty/unsupported never pass."""
from __future__ import annotations
import argparse,datetime,hashlib,json,os,pathlib,re,subprocess,sys,tempfile,time,uuid
ROOT=pathlib.Path(__file__).resolve().parents[1]
class CaseFailure(Exception): pass

def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def artifact_records(paths):
    records={}
    for key,name in paths.items():
        p=pathlib.Path(name).resolve()
        if not p.is_file():raise CaseFailure('declared artifact missing: '+str(p))
        records[key]={'path':str(p.relative_to(ROOT)) if p.is_relative_to(ROOT) else str(p),'sha256':digest(p),'bytes':p.stat().st_size}
    return records
def write(path,data):
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n')
def inside(name):
    p=(ROOT/name).resolve()
    if not p.is_relative_to(ROOT):raise CaseFailure('path escapes project')
    return p

def execute(cmd,folder,timeout=120,cwd=None,env=None):
    folder.mkdir(parents=True,exist_ok=True)
    start=time.monotonic()
    try:
        p=subprocess.run(cmd,cwd=cwd or ROOT,text=True,capture_output=True,timeout=timeout,env=env)
        item={'command':[str(x) for x in cmd],'cwd':str(cwd or ROOT),'exit_code':p.returncode,'seconds':time.monotonic()-start,'timeout':False}
        (folder/'stdout.txt').write_text(p.stdout);(folder/'stderr.txt').write_text(p.stderr)
    except subprocess.TimeoutExpired as ex:
        item={'command':[str(x) for x in cmd],'cwd':str(cwd or ROOT),'exit_code':None,'seconds':time.monotonic()-start,'timeout':True}
        (folder/'stdout.txt').write_text(ex.stdout.decode(errors='replace') if isinstance(ex.stdout,bytes) else ex.stdout or '')
        (folder/'stderr.txt').write_text(ex.stderr.decode(errors='replace') if isinstance(ex.stderr,bytes) else ex.stderr or '')
        write(folder/'command.json',item);raise CaseFailure('timeout')
    except OSError as ex:
        raise CaseFailure('infrastructure error: '+str(ex)) from ex
    write(folder/'command.json',item)
    return p

def zil_path():
    explicit=os.environ.get('ZIL_BIN')
    p=pathlib.Path(explicit) if explicit else ROOT/'target/debug'/('zil.exe' if os.name=='nt' else 'zil')
    if not p.is_file():raise CaseFailure('compiler binary missing: '+str(p))
    return str(p.resolve())
def result_json(proc):
    try:return json.loads(proc.stdout)
    except ValueError as ex:raise CaseFailure('compiler did not return JSON: '+proc.stdout[:400]+' '+proc.stderr[:400]) from ex

def source_step(s,folder):
    src=inside(s['source'])
    if not src.is_file():raise CaseFailure('missing source '+str(src))
    artifacts=folder/'artifacts';kind=s['kind'];assertions=0
    native=kind=='native'
    output=folder/('program.exe' if os.name=='nt' else 'program')
    cmd=[zil_path(),'build' if native else 'check',str(src),'--json','--emit-dir',str(artifacts)]
    if native:
        cmd+=['--output',str(output)]
        if 'entry' in s:cmd+=['--entry',s['entry']]
    else:cmd+=['--stop-after',s.get('phase','verify')]
    p=execute(cmd,folder/'compile',s.get('timeout',120));d=result_json(p)
    if kind=='reject':
        if p.returncode==0 or d.get('status')!='SafeReject':raise CaseFailure('expected SafeReject, got '+str(d.get('status')))
        matches=[x for x in d.get('diagnostics',[]) if x.get('code')==s['diagnostic'] and x.get('phase')==s['phase']]
        if not matches:raise CaseFailure('wrong rejection stage/reason: '+str(d.get('diagnostics')))
        if output.exists():raise CaseFailure('rejected source produced executable')
        return {'executed':1,'assertions':3,'observed':d}
    if p.returncode or d.get('status')!='Success':raise CaseFailure('compiler did not succeed: '+str(d))
    assertions+=1
    for key in s.get('required_artifacts',[]):
        paths=d.get('artifacts',{})
        if key not in paths or not pathlib.Path(paths[key]).is_file():raise CaseFailure('missing stage artifact '+key)
        assertions+=1
    if native:
        if not output.is_file():raise CaseFailure('successful compile produced no executable')
        run=execute([str(output)],folder/'native',s.get('run_timeout',15))
        if 'entry' in s:
            try:actual=int(run.stdout.strip())
            except ValueError as ex:raise CaseFailure('native harness returned no complete integer result') from ex
            if run.returncode!=0 or actual!=s['return_value']:raise CaseFailure(f'native return mismatch: {actual}, exit {run.returncode}')
        else:
            if run.returncode!=s['exit_code']:raise CaseFailure(f'native exit {run.returncode}, expected {s["exit_code"]}')
        assertions+=2
    return {'executed':1,'assertions':assertions,'observed':d,'source_sha256':digest(src),'compiler_sha256':digest(pathlib.Path(zil_path())),'artifacts':artifact_records(d.get('artifacts',{})),'evidence_level':'NativeSource' if native else 'SourcePhase-'+s.get('phase','verify')}

def cargo_step(s,folder):
    cmd=['cargo','test','--offline','--locked','-p',s['package']]
    if s.get('filter'):cmd.append(s['filter'])
    if s.get('doc'):cmd.append('--doc')
    p=execute(cmd,folder,s.get('timeout',180))
    counts=[int(n) for n in re.findall(r'test result: ok\. (\d+) passed',p.stdout)]
    total=sum(counts)
    if p.returncode or total<s.get('min_passed',1):raise CaseFailure(f'cargo tests did not execute required count: {total}, exit {p.returncode}')
    return {'executed':total,'assertions':2,'observed_tests':total}

def command_step(s,folder):
    cmd=[str(x).replace('{python}',sys.executable).replace('{root}',str(ROOT)) for x in s['command']]
    p=execute(cmd,folder,s.get('timeout',120))
    if p.returncode!=s.get('exit_code',0):raise CaseFailure('command exit mismatch')
    checks=1
    for text in s.get('stdout_contains',[]):
        if text not in p.stdout:raise CaseFailure('missing stdout assertion '+text)
        checks+=1
    if s.get('result_json'):
        obj=json.loads(p.stdout)
        if obj.get('executed',0)<1 or obj.get('assertions',0)<1:raise CaseFailure('zero execution/assertions')
        if obj.get('status','Pass') not in ('Pass','Success') or obj.get('ok',True) is not True:raise CaseFailure('command reported a nonpassing result')
        checks+=2
    return {'executed':1,'assertions':checks}

def run_step(s,folder):
    kind=s.get('kind')
    if kind in ('native','accept','reject'):return source_step(s,folder)
    if kind=='cargo-test':return cargo_step(s,folder)
    if kind=='command':return command_step(s,folder)
    raise CaseFailure('unknown fixture kind '+str(kind))

def conformance_fixture(c):
    e=c['expected']
    if e.get('fixture'):
        fixture=ROOT/'tests/fixtures/closure-runtime.ready.json'
        if not fixture.is_file():raise CaseFailure('required runtime fixture is not implemented')
    s={'source':c['path'],'phase':c['phase']}
    if c['kind']=='reject':s.update(kind='reject',diagnostic=c['diagnostic'])
    elif c['phase']=='run':
        s['kind']='native'
        if 'entry' in e:s.update(entry=e['entry'],return_value=e['return_value'])
        else:s['exit_code']=e['exit_code']
    else:s['kind']='accept'
    return {'steps':[s]}

def run_case(c,out,conformance):
    folder=out/c['id'];folder.mkdir()
    result={'id':c['id'],'status':'Fail','attempted_steps':0,'executed':0,'assertions':0,'steps':[]}
    try:
        if conformance:f=conformance_fixture(c)
        else:
            fp=inside(c['required_fixture'])
            if not fp.is_file():raise CaseFailure('missing fixture '+c['required_fixture'])
            f=json.loads(fp.read_text())
        steps=f.get('steps',[])
        if not steps:raise CaseFailure('zero fixture steps')
        for i,s in enumerate(steps):
            result['attempted_steps']+=1
            r=run_step(s,folder/f'step-{i+1:02}')
            if r['executed']<1 or r['assertions']<1:raise CaseFailure('zero execution/assertions')
            result['steps'].append(r);result['executed']+=r['executed'];result['assertions']+=r['assertions']
        if result['executed']<c.get('minimum_executed',1) or result['assertions']<c.get('minimum_assertions',1):raise CaseFailure('registered minimum execution/assertion count not met')
        result['status']='Pass'
    except (CaseFailure,KeyError,ValueError,OSError) as ex:result['reason']=str(ex)
    write(folder/'result.json',result)
    return result

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--task',required=True);p.add_argument('--manifest',required=True);p.add_argument('--out-root',default='out/runs/cases')
    args=p.parse_args();manifest=inside(args.manifest)
    try: data=json.loads(manifest.read_text())
    except (OSError,ValueError) as ex:print(json.dumps({'status':'InfraError','reason':str(ex)}));return 2
    cases=[c for c in data.get('cases',[]) if c.get('owner')==args.task]
    if not cases:print(json.dumps({'status':'Fail','selected':0,'executed':0,'assertions':0,'reason':'zero selected cases'}));return 1
    stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'-'+uuid.uuid4().hex[:8]
    out=inside(args.out_root)/stamp;out.mkdir(parents=True,exist_ok=False)
    frozen=json.loads((ROOT/'data/normative-inputs.json').read_text())
    hashes={name:digest(inside(name)) for name in frozen['files']}
    normative=hashlib.sha256(''.join(name+'\0'+sha+'\n' for name,sha in sorted(hashes.items())).encode()).hexdigest()
    if normative!=frozen['normative_hash'] or hashes!=frozen['files']:
        print(json.dumps({'status':'Fail','reason':'normative inputs changed; explicit baseline refresh required'}));return 1
    taskpath=ROOT/'.task'/f'{args.task}.md'
    task=json.loads(re.search(r'```json\n(.*?)\n```',taskpath.read_text(),re.S).group(1)) if taskpath.is_file() else {}
    conformance='conformance' in args.manifest
    results=[run_case(c,out,conformance) for c in cases]
    try:commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    except (OSError,subprocess.CalledProcessError):commit=None
    report={'schema_version':1,'task':args.task,'task_revision':task.get('task_revision'),'base_revision':task.get('base_revision'),'candidate_commit':commit,'run_id':stamp,'normative_hash':normative,'manifest_sha256':digest(manifest),'selected':len(cases),'executed':sum(r['attempted_steps']>0 for r in results),'passed':sum(r['status']=='Pass' for r in results),'assertions':sum(r['assertions'] for r in results),'status':'Pass' if all(r['status']=='Pass' for r in results) else 'Fail','cases':results,'result_path':str((out/'result.json').relative_to(ROOT))}
    for key,name in [('toolchain_sha256','tools/toolchain-profile.json'),('profile_sha256','validation/product-gates.json')]:
        fp=ROOT/name;report[key]=digest(fp) if fp.is_file() else None
    write(out/'result.json',report)
    print(json.dumps({k:v for k,v in report.items() if k!='cases'},ensure_ascii=False,indent=2))
    return 0 if report['status']=='Pass' else 1
if __name__=='__main__':sys.exit(main())
