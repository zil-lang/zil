#!/usr/bin/env python3
"""Original upstream libraries; NEVER compare their times to reduced ZIL mechanisms."""
import argparse,hashlib,json,os,shutil,subprocess,time
from pathlib import Path
PIN='c0f257113ccc6e6e0742b318006eef2787c3d4c5'
CRATES=('bitmaps-3.2.1','syn-2.0.101','regex-automata-0.4.8')
def main():
    ap=argparse.ArgumentParser();ap.add_argument('--out',type=Path,required=True);a=ap.parse_args();out=a.out.resolve();out.mkdir(parents=True)
    repo=out/'upstream';repo.mkdir();log=out/'setup.log'
    def setup(argv):
        with log.open('ab') as f:subprocess.run(list(map(str,argv)),stdout=f,stderr=subprocess.STDOUT,check=True,timeout=300)
    setup(['git','init',repo]);setup(['git','-C',repo,'remote','add','origin','https://github.com/rust-lang/rustc-perf.git'])
    setup(['git','-C',repo,'config','core.sparseCheckout','true'])
    (repo/'.git/info/sparse-checkout').write_text('/LICENSE*\n'+''.join('/collector/compile-benchmarks/'+n+'/\n' for n in CRATES))
    setup(['git','-C',repo,'fetch','--depth=1','--filter=blob:none','origin',PIN]);setup(['git','-C',repo,'checkout','--detach',PIN])
    results=[]
    for name in CRATES:
        root=repo/'collector/compile-benchmarks'/name;record={'crate':name,'source_commit':PIN,'kind':'unmodified original Rust library, unpaired','zil_equivalent':'NotImplemented','timings':[]}
        try:
            source=[p for p in root.rglob('*.rs') if 'target' not in p.parts]
            record['rust_source_files']=len(source);record['rust_source_bytes']=sum(p.stat().st_size for p in source)
            record['source_hashes']={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in source}
            setup(['cargo','fetch','--locked','--manifest-path',root/'Cargo.toml'])
            env=dict(os.environ,CARGO_TARGET_DIR=str(out/'targets'/name),CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='1')
            for mode in ('check','build-release'):
                for scenario in ('project-cold','unchanged-cargo'):
                    if scenario=='project-cold':shutil.rmtree(out/'targets'/name,ignore_errors=True)
                    argv=['cargo','check' if mode=='check' else 'build','--locked','--offline','--manifest-path',str(root/'Cargo.toml')]
                    if mode=='build-release':argv+=['--release']
                    target=out/(name+'-'+mode+'-'+scenario+'.log')
                    with target.open('wb') as f:
                        t=time.perf_counter();pr=subprocess.run(argv,stdout=f,stderr=subprocess.STDOUT,env=env,timeout=300);elapsed=(time.perf_counter()-t)*1000
                    record['timings'].append({'mode':mode,'scenario':scenario,'wall_ms':elapsed,'exit':pr.returncode,'argv':argv,'log':target.name,'count':1,'purpose':'coverage/profiling only, no statistical speedup claim'})
                    if pr.returncode:raise RuntimeError('original upstream compilation failed')
            record['status']='Pass'
        except Exception as e:record['status']='Fail';record['error']=repr(e)
        results.append(record)
        (out/'summary.json').write_text(json.dumps({'scope':'original Rust-only references; NOT included or renormalized into paired ZIL weights','source':PIN,'crates':results},indent=2)+'\n')
    print(json.dumps({'original_crates':len(results),'passed':sum(r['status']=='Pass' for r in results),'paired_with_zil':False}),flush=True)
    # Source provenance is preserved by the immutable pin plus per-file digests.
    # Do not ship upstream copies or build artifacts without their license context.
    shutil.rmtree(repo,ignore_errors=True);shutil.rmtree(out/'targets',ignore_errors=True)
if __name__=='__main__':main()
