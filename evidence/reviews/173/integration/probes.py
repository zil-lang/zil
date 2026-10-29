#!/usr/bin/env python3
"""Non-author probes of 173's ordinary owned-error inference connection."""
from pathlib import Path
import argparse,hashlib,json,os,subprocess,time


def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def save(path,obj): path.write_text(json.dumps(obj,ensure_ascii=False,indent=2)+'\n')
def run(argv,path,env=None):
    path.mkdir(parents=True,exist_ok=False)
    receipt={'command':list(map(str,argv)),'controlled_environment':env or {},'start':time.time()}
    result=subprocess.run(receipt['command'],capture_output=True,timeout=120,env=dict(os.environ,**(env or {})))
    (path/'stdout.log').write_bytes(result.stdout);(path/'stderr.log').write_bytes(result.stderr)
    receipt.update(end=time.time(),exit_code=result.returncode,stdout_sha256=sha(path/'stdout.log'),stderr_sha256=sha(path/'stderr.log'))
    save(path/'command.json',receipt)
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli',type=Path,required=True)
    parser.add_argument('--repo',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    parser.add_argument('--commit',required=True)
    args=parser.parse_args();cli=args.cli.resolve();repo=args.repo.resolve();out=args.out.resolve()
    out.mkdir(parents=True,exist_ok=False)
    inputs=Path(__file__).resolve().parent/'probe-inputs'
    report={'reviewer':'/root/dynamic_store','implementation_author':'/root','implementation_commit':args.commit,
            'scope':'173 ordinary owned-error helper inference and closed adjacent capture/callable contracts; no approval of reviewer-authored 172',
            'cli_sha256':sha(cli),'input_files':{str(p.relative_to(inputs)):sha(p) for p in inputs.rglob('*.zil')},
            'native':[],'negative':[],'status':'Running'}
    save(out/'summary.json',report)
    for case,expected,normal_allocations in [('ordinary-abi',b'ABC',2),('both-owning-payloads',b'DEC',0)]:
        for opt in ['O0','O2']:
            target=out/case/opt;program=target/'program';phases=target/'phases'
            built=run([cli,'build',inputs/case/'main.zil','--opt',opt,'-o',program,'--emit-dir',phases,'--json'],target/'build')
            compiled=json.loads(built.stdout)
            assert built.returncode==0 and compiled['status']=='Success' and compiled['verification_runs']==1,compiled
            ordinary=run([program],target/'ordinary')
            assert ordinary.returncode==0 and ordinary.stdout==expected and ordinary.stderr==b'',(case,opt,ordinary)
            report['native'].append({'case':case,'opt':opt,'probe':False,'stdout':ordinary.stdout.decode(),'exit':ordinary.returncode,'llvm_sha256':sha(phases/'llvm.ll')})
            probe=target/'probe'
            linked=run(['clang','-'+opt,phases/'llvm.ll',repo/'tools/language/173/allocator_probe.c','-Wl,--wrap=realloc','-Wl,--wrap=free','-o',probe],target/'probe-link')
            assert linked.returncode==0,linked.stderr.decode()
            for fail_at in ([0,1,2] if normal_allocations else [0]):
                observed=run([probe],target/f'allocator-{fail_at}',{'ZIL_TEST_FAIL_AT':str(fail_at)})
                assert observed.returncode==0 and observed.stdout==expected,(case,opt,fail_at,observed)
                allocation=json.loads(observed.stderr)
                allocated=normal_allocations-int(fail_at!=0)
                assert allocation['alloc_calls']==normal_allocations and allocation['allocated']==allocated and allocation['frees']==allocated and allocation['live']==0 and allocation['failures']==int(fail_at!=0),allocation
                report['native'].append({'case':case,'opt':opt,'probe':True,'failure_index':fail_at,'stdout':observed.stdout.decode(),'exit':observed.returncode,'allocation':allocation})
            save(out/'summary.json',report)
    for source in sorted(inputs.glob('*/main.zil')):
        case=source.parent.name
        if case in ['ordinary-abi','both-owning-payloads']: continue
        target=out/'negative'/case
        built=run([cli,'build',source,'-o',target/'program','--emit-dir',target/'phases','--json'],target/'build')
        compiled=json.loads(built.stdout)
        row={'case':case,'status':compiled['status'],'phase':compiled['phase'],'diagnostics':compiled['diagnostics'],'verification_runs':compiled['verification_runs'],'exit_code':built.returncode,'native_commands':compiled['commands']}
        report['negative'].append(row)
        save(out/'summary.json',report)
        # This is only collection. The review records each decisive message;
        # an early syntax error is a broken probe, never a safety success.
        assert compiled['status'] in ['SafeReject','Unsupported'] and compiled['phase'] in ['types','analyze','verify','lower'] and built.returncode!=0 and not compiled['commands'],row
    report.update(status='ProbeObservationsComplete',native_executions=len(report['native']),negative_cases=len(report['negative']))
    save(out/'summary.json',report)
    print(json.dumps(report,ensure_ascii=False,indent=2))


if __name__=='__main__':main()
