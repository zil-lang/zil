#!/usr/bin/env python3
"""Compare the recovered copy editor and new in-place editor on the same native workload.
Allocation counts, not a wall-clock or Rust performance claim. No third-party dependencies.
"""
import argparse, hashlib, json, os, pathlib, shutil, subprocess


def source(in_place):
    call = '''let result=edit.replace_in_place(text,position,position+1,bytes_view(insertion),64)
        if result.code!=0 {return 11}''' if in_place else '''let result=edit.replace(bytes_view(text),position,position+1,bytes_view(insertion),64)
        if result.code!=0 {return 11}
        text=edit.take(result)'''
    text = '''import edit
package Storage {manages value:ByteBuffer}
fn main()->i32 {
    var storage=Storage{value:bytes_new()};let insertion=bytes_new()
    if !bytes_reserve(text,64) || !bytes_reserve(insertion,1) {return 1}
    for i in 0..64 {bytes_push(text,97u8)}
    bytes_push(insertion,88u8)
    for i in 0..1000 {
        let position:i64=if i%3==0 {0} else {if i%3==1 {31} else {63}}
        '''+call+'''
    }
    let bytes=bytes_view(text)
    if bytes.len()!=64 {return 2}
    for i in 0..64 {
        let expected=if i==0 || i==31 || i==63 {88u8} else {97u8}
        if bytes[i]!=expected {return 3}
    }
    return 0
}
'''
    return text.replace('text','storage.value')


def main():
    p=argparse.ArgumentParser();p.add_argument('--driver',required=True);p.add_argument('--clang',default='clang');p.add_argument('--out',required=True);a=p.parse_args()
    repo=pathlib.Path(__file__).resolve().parents[3];out=pathlib.Path(a.out).resolve();out.mkdir(parents=True,exist_ok=False)
    env={**os.environ,'ZIL_CLANG':a.clang};records=[];commands=[]
    def run(cmd,**kw):
        r=subprocess.run(list(map(str,cmd)),capture_output=True,timeout=60,**kw)
        commands.append({'command':list(map(str,cmd)),'exit_code':r.returncode,'stdout':r.stdout.decode(errors='replace'),'stderr':r.stderr.decode(errors='replace')})
        (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n');return r
    for variant in ['copy','in-place']:
        src=out/variant;shutil.copytree(repo/'examples/mainline/owned_text',src);(src/'main.zil').write_text(source(variant=='in-place'))
        for opt in ['O0','O2']:
            exe=out/(variant+'-'+opt)
            r=run([a.driver,'build',src/'main.zil','--opt',opt,'--clang',a.clang,'--emit-dir',out/(variant+'-'+opt+'-phases'),'-o',exe,'--json'],env=env)
            report=json.loads(r.stdout);(out/(variant+'-'+opt+'-compile.json')).write_text(json.dumps(report,indent=2)+'\n')
            assert r.returncode==0 and report['status']=='Success' and report['verification_runs']==1,report
            assert run([exe]).returncode==0
            probe=out/(variant+'-'+opt+'-probe')
            assert run([a.clang,'-Wno-override-module','-'+opt,report['artifacts']['llvm'],repo/'tools/research/139/allocator_probe.c','-Wl,--wrap=realloc','-Wl,--wrap=free','-o',probe]).returncode==0
            for fail_at in [0,3]:
                execution=run([probe],env={**env,'ZIL_TEST_FAIL_AT':str(fail_at)});m=json.loads(execution.stderr)
                expected_calls=2 if variant=='in-place' else (3 if fail_at else 1002)
                assert m['allocator_calls']==expected_calls and m['live']==0 and m['allocated']==m['released'],m
                assert execution.returncode==(11 if fail_at and variant=='copy' else 0),(variant,opt,fail_at,execution)
                records.append({'variant':variant,'opt':opt,'edits_requested':1000,'edits_completed':0 if variant=='copy' and fail_at else 1000,'fail_at':fail_at,'exit_code':execution.returncode,'allocator':m})
    receipt={'format':'zil-139-repeated-edits/1','status':'Pass','records':records,'scope':'Same compiler and source library; 1000 equal-size edits at beginning/middle/end. Counts include two initial allocations. O0/O2 both measured. No wall-clock comparison.','driver_sha256':hashlib.sha256(pathlib.Path(a.driver).read_bytes()).hexdigest()}
    (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt,indent=2))

if __name__=='__main__':main()
