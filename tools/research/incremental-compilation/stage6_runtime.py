#!/usr/bin/env python3
"""O2 quality control: opaque runtime input and live, repeatedly called helpers."""
import argparse, hashlib, json, os, subprocess, time
from pathlib import Path

def generate(root, n, flavor, rounds):
    branch=flavor=="branch"
    for lang in ['zil','rust']:
        mut='var' if lang=='zil' else 'let mut'
        code=[]
        for i in range(n):
            expr=f'(x*3+{i%97})%1009'
            body=(f'if x%3==0 {{return {expr};}} else {{return (x+{i%31})%1009;}}' if branch else f'return {expr};')
            if flavor=='body':body=f'{mut} y:i32=x;'+''.join('y=y+1;' for _ in range(64))+f'return (y+{i%97})%1009;'
            code.append(f'fn h{i}(x:i32)->i32{{{body}}}')
        code.append(f'fn value(x:i32)->i32{{{mut} result:i32=0;')
        code.extend(f'result=result+h{i}(x);' for i in range(n))
        code.append('return result;}')
        if lang=='zil': code.append('extern "C" fn getchar()->i32;extern "C" fn putchar(x:i32)->i32;fn main()->i32{')
        else: code.append('unsafe extern "C"{fn getchar()->i32;fn putchar(x:i32)->i32;}fn main(){')
        code.append(f'{mut} seed:i32=0;unsafe{{seed=getchar();}}{mut} i:i32=0;{mut} result:i32=0;while i<{rounds}{{result=(result+value(seed))%1000003;seed=(seed+17)%1009;i=i+1;}}unsafe{{')
        code.extend(f'putchar((result>>{shift})&255);' for shift in [0,8,16,24])
        code.append('}return 0;}' if lang=='zil' else '}}')
        (root/('main.zil' if lang=='zil' else 'main.rs')).write_text('\n'.join(code))

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--zil',type=Path,required=True);p.add_argument('--baseline',type=Path,required=True)
    p.add_argument('--rustc',type=Path,required=True);p.add_argument('--out',type=Path,required=True)
    p.add_argument('--blocks',type=int,default=6);p.add_argument('--rounds',type=int,default=20000)
    a=p.parse_args();a.out.mkdir(parents=True,exist_ok=False)
    cpus=sorted(os.sched_getaffinity(0))[:2];os.sched_setaffinity(0,cpus)
    rows=[]; builds=[]
    arms=['baseline','incremental','closed_incremental','rust']
    for n,flavor in [(512,'arithmetic'),(512,'branch'),(2048,'arithmetic'),(2048,'branch'),(512,'body'),(8192,'arithmetic')]:
        branch=flavor=='branch'
        case=f'{n}-'+flavor;root=a.out/case;root.mkdir()
        generate(root,n,flavor,a.rounds)
        for arm in arms:
            exe=root/arm
            if arm=='rust':
                cmd=[str(a.rustc),str(root/'main.rs'),'--edition=2024','-A','warnings','-C','opt-level=2','-C','overflow-checks=yes','-C','panic=abort','-C','codegen-units=8','-o',str(exe)]
            else:
                cmd=[str(a.baseline if arm=='baseline' else a.zil),'build',str(root/'main.zil'),'--opt','O2','--codegen-units','8','--codegen-jobs','2','--json','-o',str(exe)]
                if arm!='baseline':cmd+=['--incremental-o2']
                if arm=='closed_incremental':cmd+=['--closed-executable']
            start=time.perf_counter_ns(); result=subprocess.run(cmd,capture_output=True,timeout=240)
            builds.append(dict(case=case,arm=arm,command=cmd,elapsed_ns=time.perf_counter_ns()-start,exit=result.returncode,stdout=result.stdout.decode(errors='replace'),stderr=result.stderr.decode(errors='replace')))
            (a.out/'builds.json').write_text(json.dumps(builds,indent=2)+'\n')
            assert result.returncode==0,builds[-1]
            builds[-1]['exe_bytes']=exe.stat().st_size
            builds[-1]['exe_sha256']=hashlib.sha256(exe.read_bytes()).hexdigest()
        values=[sum(((x+64+i%97)%1009 if flavor=='body' else (x*3+i%97)%1009 if not branch or x%3==0 else (x+i%31)%1009) for i in range(n)) for x in range(1009)]
        expected_by_seed={}
        for seed in [0,1,7,42,255]:
            expected=0;x=seed
            for _ in range(a.rounds):
                expected=(expected+values[x])%1000003;x=(x+17)%1009
            expected_by_seed[seed]=expected
        for block in range(a.blocks):
            order=arms if block%2==0 else list(reversed(arms))
            for seed in [0,1,7,42,255]:
                expected=expected_by_seed[seed]
                for arm in order:
                    start=time.perf_counter_ns();result=subprocess.run([str(root/arm)],input=bytes([seed]),capture_output=True,timeout=60)
                    row=dict(case=case,block=block,seed=seed,arm=arm,elapsed_ns=time.perf_counter_ns()-start,exit=result.returncode,stdout_hex=result.stdout.hex(),stderr_hex=result.stderr.hex(),expected=expected)
                    row['valid']=result.returncode==0 and result.stdout==expected.to_bytes(4,'little') and not result.stderr
                    rows.append(row)
                    (a.out/'rows.json').write_text(json.dumps(rows,indent=2)+'\n')
                    assert row['valid'],row
    (a.out/'builds.json').write_text(json.dumps(builds,indent=2)+'\n')
    identity=lambda p:dict(path=str(p),sha256=hashlib.sha256(p.read_bytes()).hexdigest())
    (a.out/'manifest.json').write_text(json.dumps(dict(cpus=cpus,blocks=a.blocks,rounds=a.rounds,binaries={k:identity(v) for k,v in [('zil',a.zil),('baseline',a.baseline),('rustc',a.rustc)]},scope=__doc__,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()),indent=2)+'\n')
    print(json.dumps(dict(builds=len(builds),observations=len(rows),valid=all(r['valid'] for r in rows))))
if __name__=='__main__':main()
