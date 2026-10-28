#!/usr/bin/env python3
"""208 actual API objects: current language177 consumer and allocation failures.
The independent business oracle is the existing 177 consumer's scenarios().
Probe executables are linked from API objects, never recompiled from .ll by Clang.
"""
import argparse, hashlib, importlib.util, json, os, subprocess, sys, time
from pathlib import Path

def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def write(p,j): p.write_text(json.dumps(j,ensure_ascii=False,indent=2)+'\n')
def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--repo',type=Path,required=True);ap.add_argument('--cli',type=Path,required=True)
    ap.add_argument('--clang',type=Path,required=True);ap.add_argument('--out',type=Path,required=True)
    ap.add_argument('--source-commit',help='explicit delivery identity when the source ZIP has no Git database')
    a=ap.parse_args(); repo=a.repo.resolve();cli=a.cli.resolve();clang=a.clang.resolve();out=a.out.resolve()
    out.mkdir(parents=True,exist_ok=False)
    spec=importlib.util.spec_from_file_location('consumer177',repo/'tools/language/177/consumer.py')
    oracle=importlib.util.module_from_spec(spec);spec.loader.exec_module(oracle)
    sources=repo/'examples/mainline/zui_language177'
    identity=a.source_commit or subprocess.check_output(['git','-C',str(repo),'rev-parse','HEAD'],text=True).strip()
    if len(identity)!=40 or any(c not in '0123456789abcdef' for c in identity):
        raise ValueError('source identity must be a 40-character Git commit hash')
    summary={'status':'Running','source_commit':identity,'source_identity_kind':'supplied-delivery' if a.source_commit else 'observed-git',
             'cli_sha256':sha(cli),'clang_sha256':sha(clang),'source_hashes':{p.name:sha(p) for p in sources.glob('*.zil')},
             'oracle':'tools/language/177/consumer.py scenarios (preexisting business oracle)',
             'oracle_sha256':sha(repo/'tools/language/177/consumer.py'), 'builds':[], 'observations':[]}
    records=[]
    def command(argv,name,input=b'',env=None):
        start=time.time();p=out/name;p.mkdir(parents=True,exist_ok=False)
        record={'argv':list(map(str,argv)),'input_hex':input.hex(),'environment':env or {}}
        try:
            result=subprocess.run(record['argv'],input=input,capture_output=True,timeout=120,env=dict(os.environ,**(env or {})))
            record.update(returncode=result.returncode,seconds=time.time()-start)
            (p/'stdout').write_bytes(result.stdout);(p/'stderr').write_bytes(result.stderr)
            return result
        except subprocess.TimeoutExpired:
            record['timeout']=True;raise
        finally: write(p/'record.json',record);records.append(record)
    try:
        probe=out/'allocator.o'
        c=command([clang,'-O0','-c',repo/'tools/language/173/allocator_probe.c','-o',probe],'prepare-probe')
        assert c.returncode==0,c.stderr
        wrapper=out/'link-probe.py';linklog=out/'probe-link-argv.jsonl'
        wrapper.write_text('#!'+sys.executable+'\nimport os,sys,json\n'+
            f'cmd={[str(clang)]!r}+sys.argv[1:]+{[str(probe),"-Wl,--wrap=realloc","-Wl,--wrap=free"]!r}\n'+
            'assert "ir" not in sys.argv and "-c" not in sys.argv, "probe must only link actual API objects"\n'+
            f'with open({str(linklog)!r},"a") as f:f.write(json.dumps(cmd)+"\\n")\n'+
            'os.execv(cmd[0],cmd)\n')
        wrapper.chmod(0o755)
        summary['probe_sha256']=sha(probe);summary['link_wrapper_sha256']=sha(wrapper)
        for units in (1,8):
            for entry in ('main','field_exit'):
                for opt in ('O0','O2'):
                    base=f'u{units}/{entry}/{opt}'; expected_ir=None
                    for mode in ('ordinary','probe'):
                        name=f'{base}/{mode}'; exe=out/(name+'-program')
                        argv=[cli,'build',sources/(entry+'.zil'),'-o',exe,'--backend','llvm-api','--opt',opt,
                              '--codegen-units',str(units),'--codegen-jobs','4','--clang',wrapper if mode=='probe' else clang,'--json']
                        result=command(argv,name+'/build');report=json.loads(result.stdout)
                        assert result.returncode==0 and report['status']=='Success',report
                        assert report['verification_runs']==1,report
                        cg=report['native_codegen'];assert cg['backend']=='llvm-api' and len(cg['object_reports'])==cg['actual_units']
                        assert len(report['commands'])==1 and 'ir' not in report['commands'][0]
                        irsha=sha(Path(str(exe)+'.ll'))
                        if mode=='ordinary':expected_ir=irsha
                        else:assert irsha==expected_ir,'link probe must not change input IR'
                        summary['builds'].append({'name':name,'llvm_sha256':irsha,'executable_sha256':sha(exe),'codegen':cg,'verification_runs':1})
                        for row in oracle.scenarios(entry):
                            cases=[(0,row['stdout'],row['exit'],row['allocated'])]
                            if mode=='probe':
                                if row.get('ordinary_only'):continue
                                cases += [(at,text,exit,allocated) for at,text,allocated,exit in row['failures']]
                            for at,text,exit,allocated in cases:
                                run=command([exe],name+'/'+row['name']+f'-fail{at}',row['input'],{'ZIL_TEST_FAIL_AT':str(at)} if mode=='probe' else None)
                                assert run.returncode==exit and run.stdout==text,(name,row,at,run.returncode,run.stdout,run.stderr)
                                allocation=None
                                if mode=='probe':
                                    allocation=json.loads(run.stderr)
                                    assert allocation['live']==0 and allocation['allocated']==allocated and allocation['frees']==allocated and allocation['failures']==int(at>0),allocation
                                else:assert not run.stderr,run.stderr
                                summary['observations'].append({'case':name+'/'+row['name'],'fail_at':at,'exit':run.returncode,'stdout':run.stdout.decode(),'allocation':allocation})
                        write(out/'summary.json',summary)
        summary['status']='Pass'
    except Exception as error:
        summary['status']='Fail';summary['error']=repr(error);raise
    finally:
        summary['commands']=len(records);write(out/'summary.json',summary)
    print(json.dumps({'status':summary['status'],'builds':len(summary['builds']),'executions':len(summary['observations']),
                      'injected_failures':sum(x['fail_at']>0 for x in summary['observations'])}))
if __name__=='__main__':main()
