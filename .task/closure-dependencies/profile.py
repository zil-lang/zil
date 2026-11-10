#!/usr/bin/env python3
"""Diagnostic only: preserve current generated IR and real phase reports."""
import argparse, importlib.util, json, os, pathlib, shutil, subprocess, time
P=pathlib.Path
p=argparse.ArgumentParser();p.add_argument('--cli',required=True);p.add_argument('--out',type=P,required=True);a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True)
os.sched_setaffinity(0,sorted(os.sched_getaffinity(0))[:2])
spec=importlib.util.spec_from_file_location('corpus',P(__file__).parent/'corpus.py');corpus=importlib.util.module_from_spec(spec);spec.loader.exec_module(corpus)
cases=corpus.generate(out/'corpus');rows=[]
def run(argv, stem, env=None):
    t=time.perf_counter_ns();r=subprocess.run(list(map(str,argv)),capture_output=True,env=env,timeout=180);wall=(time.perf_counter_ns()-t)/1e6
    P(str(stem)+'.stdout').write_bytes(r.stdout);P(str(stem)+'.stderr').write_bytes(r.stderr)
    row={'argv':list(map(str,argv)),'exit':r.returncode,'wall_ms':wall,'stem':str(stem)};rows.append(row)
    (out/'requests.json').write_text(json.dumps(rows,indent=2)+'\n')
    if r.returncode:raise RuntimeError((row,r.stderr[-8000:],r.stdout[-8000:]))
    return r
clang=os.environ['ZIL_CLANG'];rust=subprocess.check_output(['rustup','which','rustc'],text=True).strip()
for case in cases:
    if case['id'] not in ('owned8','owned32'):continue
    root=out/case['id'];root.mkdir()
    zil=root/'main.zil';exe=root/'app'
    for request,snapshot in [('cold','base'),('warm','base'),('body','private')]:
        zil.write_bytes((out/'corpus'/case['id']/snapshot/'zil/main.zil').read_bytes())
        r=run([a.cli,'build',zil,'--json','--opt','O2','--backend','llvm-api','--clang',clang,'--codegen-units','8','--codegen-jobs','2','--incremental-o2','--function-merging','after','--semantic-cache',root/'semantic','--object-cache',root/'objects','-o',exe],root/('zil-'+request))
        report=json.loads(r.stdout);assert report['status']=='Success' and report['verification_runs']==1
        shutil.copy2(P(str(exe)+'.ll'),root/('zil-'+request+'.ll'))
        P(root/('zil-'+request+'.json')).write_text(json.dumps(report,indent=2)+'\n')
    # Standalone LLVM runs are diagnostic, not substituted for CLI benchmark.
    raw=root/'zil-body.ll'
    run([clang,'-O2','-c','-ftime-trace','-ftime-trace-granularity=0',raw,'-o',root/'zil-profile.o'],root/'clang-profile')
    run([clang,'-O2','-S','-emit-llvm',raw,'-o',root/'zil-optimized.ll'],root/'clang-opt-ir')
    rs=root/'main.rs';rs.write_bytes((out/'corpus'/case['id']/'private/rust/main.rs').read_bytes())
    base=[rust,rs,'--edition=2024','--crate-name','closure_cost','-A','warnings','-C','opt-level=2','-C','debuginfo=0','-C','overflow-checks=yes','-C','panic=abort','-C','embed-bitcode=no']
    run(base+['--emit=llvm-ir','-C','no-prepopulate-passes','-o',root/'rust-input.ll'],root/'rust-input')
    run(base+['--emit=llvm-ir','-o',root/'rust-optimized.ll'],root/'rust-optimized')
    diag=os.environ.copy();diag['RUSTC_BOOTSTRAP']='1'
    run(base+['-Ztime-passes','-C','incremental='+str(root/'rust-incremental'),'-o',root/'rust-app'],root/'rust-time-passes',env=diag)
print(json.dumps({'complete':True,'cases':['owned8','owned32'],'requests':len(rows),'classification':'diagnostic IR/phase windows; not a paired optimization gain'},indent=2))
