#!/usr/bin/env python3
"""Exploratory sampling only: not an acceptance performance series."""
import argparse, hashlib, importlib.util, json, os, pathlib, shutil, subprocess, time
P = pathlib.Path

def sha(p):
    with P(p).open('rb') as f: return hashlib.file_digest(f, 'sha256').hexdigest()

def main():
    a=argparse.ArgumentParser(); a.add_argument('--repo',type=P,required=True); a.add_argument('--cli',type=P,required=True); a.add_argument('--out',type=P,required=True); a.add_argument('--perf',required=True); args=a.parse_args()
    repo=args.repo.resolve(); cli=args.cli.resolve(); out=args.out.resolve(); out.mkdir(parents=True,exist_ok=False)
    cpus=sorted(os.sched_getaffinity(0))[:2]; os.sched_setaffinity(0,cpus)
    spec=importlib.util.spec_from_file_location('corpus',repo/'tools/research/compiler-differential/corpus.py'); corpus=importlib.util.module_from_spec(spec); spec.loader.exec_module(corpus)
    rows=[]
    def run(name,argv,strict=True):
        t=time.perf_counter(); p=subprocess.run(list(map(str,argv)),capture_output=True,timeout=240); ms=(time.perf_counter()-t)*1000
        (out/(name+'.stdout')).write_bytes(p.stdout); (out/(name+'.stderr')).write_bytes(p.stderr)
        row={'name':name,'argv':list(map(str,argv)),'exit':p.returncode,'wall_ms':ms}; rows.append(row)
        print(json.dumps(row),flush=True)
        if strict and p.returncode: raise RuntimeError(name+' failed; original output retained')
        return p
    try:
        for modules in (256,1024):
            src=out/'corpus'/str(modules)/'zil'; corpus.tree_sources(src,'zil',modules,8)
            for stage in ('core','types','analyze','verify'):
                run(f'tree{modules}-{stage}',[cli,'check',src/'main.zil','--json','--stop-after',stage])
            work=out/'work'/str(modules); work.mkdir(parents=True)
            build=[cli,'build',src/'main.zil','--json','--opt','O0','--backend','llvm-api','--codegen-units','8','--codegen-jobs','2','--semantic-cache',work/'semantic','--object-cache',work/'objects','--clang',os.environ['ZIL_CLANG'],'-o',work/'app']
            run(f'tree{modules}-seed',build)
            for size in (0,7,256):
                data=bytes((i*73+19)%256 for i in range(size)); p=subprocess.run([str(work/'app')],input=data,capture_output=True,timeout=20)
                assert p.returncode==0 and p.stdout==corpus.tree_expected(modules,8,'base',data) and not p.stderr
            for mode,cmd in (('check',[cli,'check',src/'main.zil','--json']),('warm',build)):
                name=f'tree{modules}-{mode}-perf'; data=out/(name+'.data')
                result=run(name,[args.perf,'record','-o',data,'-e','cpu-clock:u','-F','997','--call-graph','fp','--',*cmd],False)
                if result.returncode==0:
                    for label,flags in (('self',['--no-children']),('children',['--children'])):
                        run(name+'-'+label,[args.perf,'report','-i',data,'--stdio','--percent-limit','0.3',*flags],False)
                data.unlink(missing_ok=True)
    finally:
        summary={'status':'Exploratory only','baseline_commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=repo,text=True).strip(),'cli_sha256':sha(cli),'cpus':cpus,'rows':rows,'source_sha256':{str(p.relative_to(out)):sha(p) for p in (out/'corpus').rglob('*.zil')},'not_claimed':['candidate improvement','stable timings','bisection attribution']}
        (out/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
if __name__=='__main__': main()
