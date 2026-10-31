#!/usr/bin/env python3
"""MergeFunctions O2 runtime quality: six live opaque-input families, complete native results."""
import argparse, hashlib, json, os, shutil, subprocess, time
from pathlib import Path
from stage6_bench import measured

ARM_OPTIONS = {
    'baseline': [], 'baseline_closed': ['--closed-executable'],
    'off': ['--function-merging', 'off'],
    'off_closed': ['--function-merging', 'off', '--closed-executable'],
    'after': ['--function-merging', 'after'],
    'after_closed': ['--function-merging', 'after', '--closed-executable'],
    'before_and_after': ['--function-merging', 'before-and-after'],
    'before_and_after_closed': ['--function-merging', 'before-and-after', '--closed-executable'],
    'rust': [],
}
DEFAULT_ARMS = ['off', 'off_closed', 'after', 'after_closed', 'before_and_after', 'before_and_after_closed']
CASES = [(512,'arithmetic'),(512,'branch'),(2048,'arithmetic'),(2048,'branch'),(512,'body'),(8192,'arithmetic')]
SEEDS = [0, 1, 7, 42, 255]

def exported_symbols(exe, root, arm):
    tool = shutil.which('nm')
    command = [tool or 'nm', '--extern-only', '--defined-only', '--format=posix', str(exe)]
    record = {'command': command, 'scope': 'ELF globally defined symbol table; not a claim about dynamic-export visibility'}
    try:
        result = subprocess.run(command, capture_output=True, timeout=30)
        (root / f'{arm}.nm.stdout').write_bytes(result.stdout)
        (root / f'{arm}.nm.stderr').write_bytes(result.stderr)
        names = [line.split()[0] for line in result.stdout.decode(errors='replace').splitlines() if line.split()]
        record.update(exit=result.returncode, stdout_sha256=hashlib.sha256(result.stdout).hexdigest(),
            stderr_hex=result.stderr.hex(), symbols=names, zil_function_count=sum(n.startswith('zil.fn.') for n in names))
        record['valid'] = result.returncode == 0 and not result.stderr
    except (OSError, subprocess.TimeoutExpired) as error:
        record.update(valid=False, error=str(error), timeout=isinstance(error, subprocess.TimeoutExpired))
    return record

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
    p.add_argument('--zil',type=Path,required=True);p.add_argument('--baseline',type=Path)
    p.add_argument('--rustc',type=Path);p.add_argument('--out',type=Path,required=True)
    p.add_argument('--blocks',type=int,default=6);p.add_argument('--rounds',type=int,default=20000)
    p.add_argument('--arm', action='append', choices=list(ARM_OPTIONS));p.add_argument('--case', action='append', choices=[f'{n}-{flavor}' for n,flavor in CASES])
    a=p.parse_args();arms=a.arm or DEFAULT_ARMS
    if len(set(arms)) != len(arms):p.error('duplicate arm')
    if a.blocks < 1 or a.rounds < 1:p.error('blocks and rounds must be positive')
    if any(x.startswith('baseline') for x in arms) and a.baseline is None:p.error('--baseline is required for a baseline arm')
    if 'rust' in arms and a.rustc is None:p.error('--rustc is required for rust')
    a.out.mkdir(parents=True,exist_ok=False)
    cpus=sorted(os.sched_getaffinity(0))[:2];os.sched_setaffinity(0,cpus)
    rows=[]; builds=[]
    identity=lambda p:dict(path=str(p),sha256=hashlib.sha256(p.read_bytes()).hexdigest())
    manifest=dict(cpus=cpus,blocks=a.blocks,rounds=a.rounds,binaries={k:identity(v) for k,v in [('zil',a.zil),('baseline',a.baseline),('rustc',a.rustc)] if v is not None},arms=arms,arm_options={k:ARM_OPTIONS[k] for k in arms},cases=[f'{n}-{f}' for n,f in CASES if not a.case or f'{n}-{f}' in a.case],seeds=SEEDS,order='forward/reverse arm order in alternating blocks; six blocks give three first/last positions per arm pair',compile_timing_boundary='Each artifact built once; build timings are diagnostic, not a six-block compilation benchmark',nm_tool=identity(Path(shutil.which('nm'))) if shutil.which('nm') else None,scope=__doc__,tool=identity(Path(__file__)),timing_tool=identity(Path(__file__).with_name('stage6_bench.py')),source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),sources={})
    def save_manifest(): (a.out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    save_manifest()
    shutil.copy2(Path(__file__),a.out/'runner.py')
    shutil.copy2(Path(__file__).with_name('stage6_bench.py'),a.out/'stage6_bench.py')
    for n,flavor in CASES:
        if a.case and f'{n}-{flavor}' not in a.case:continue
        branch=flavor=='branch'
        case=f'{n}-'+flavor;root=a.out/case;root.mkdir()
        generate(root,n,flavor,a.rounds)
        manifest['sources'][case]={p.name:identity(p) for p in [root/'main.zil',root/'main.rs']}
        save_manifest()
        valid_builds={}
        for arm in arms:
            exe=root/arm
            exe.unlink(missing_ok=True)
            if arm=='rust':
                cmd=[str(a.rustc),str(root/'main.rs'),'--edition=2024','-A','warnings','-C','opt-level=2','-C','overflow-checks=yes','-C','panic=abort','-C','codegen-units=8','-o',str(exe)]
            else:
                cmd=[str(a.baseline if arm.startswith('baseline') else a.zil),'build',str(root/'main.zil'),'--opt','O2','--codegen-units','8','--codegen-jobs','2','--json','-o',str(exe)]
                cmd += ARM_OPTIONS[arm]
            result=measured(cmd,root,arm,240)
            build=dict(case=case,arm=arm,command=cmd,**result)
            build['valid']=result['exit']==0 and not result['timeout'] and exe.is_file()
            if arm!='rust':
                try:
                    report=json.loads((root/f'{arm}.stdout').read_text())
                    build['verification_runs']=report['verification_runs'];build['status']=report['status'];build['native_codegen']=report['native_codegen']
                    build['valid'] &= report['status']=='Success' and report['verification_runs']==1
                except (ValueError,KeyError,TypeError) as error:
                    build['valid']=False;build['report_error']=str(error)
            if build['valid']:
                build['exe_bytes']=exe.stat().st_size
                build['exe_sha256']=hashlib.sha256(exe.read_bytes()).hexdigest()
                build['exports']=exported_symbols(exe,root,arm)
                build['valid'] &= build['exports']['valid']
            valid_builds[arm]=build['valid']
            builds.append(build)
            (a.out/'builds.json').write_text(json.dumps(builds,indent=2)+'\n')
        values=[sum(((x+64+i%97)%1009 if flavor=='body' else (x*3+i%97)%1009 if not branch or x%3==0 else (x+i%31)%1009) for i in range(n)) for x in range(1009)]
        expected_by_seed={}
        for seed in SEEDS:
            expected=0;x=seed
            for _ in range(a.rounds):
                expected=(expected+values[x])%1000003;x=(x+17)%1009
            expected_by_seed[seed]=expected
        for block in range(a.blocks):
            order=arms if block%2==0 else list(reversed(arms))
            for seed in SEEDS:
                expected=expected_by_seed[seed]
                for arm in order:
                    row=dict(case=case,block=block,seed=seed,arm=arm,expected=expected,valid=False)
                    if valid_builds[arm]:
                        start=time.perf_counter_ns()
                        try:
                            result=subprocess.run([str(root/arm)],input=bytes([seed]),capture_output=True,timeout=60)
                            row.update(elapsed_ns=time.perf_counter_ns()-start,exit=result.returncode,stdout_hex=result.stdout.hex(),stderr_hex=result.stderr.hex())
                            row['valid']=result.returncode==0 and result.stdout==expected.to_bytes(4,'little') and not result.stderr
                        except (subprocess.TimeoutExpired,OSError) as error:
                            row.update(elapsed_ns=time.perf_counter_ns()-start,error=str(error),timeout=isinstance(error,subprocess.TimeoutExpired),stdout_hex=(getattr(error,'stdout',None) or b'').hex(),stderr_hex=(getattr(error,'stderr',None) or b'').hex())
                    else:row['skipped']='invalid build'
                    rows.append(row)
                    (a.out/'rows.json').write_text(json.dumps(rows,indent=2)+'\n')
    (a.out/'builds.json').write_text(json.dumps(builds,indent=2)+'\n')
    print(json.dumps(dict(builds=len(builds),observations=len(rows),valid=all(r['valid'] for r in rows),failed_builds=sum(not b['valid'] for b in builds),failed_observations=sum(not r['valid'] for r in rows))))
if __name__=='__main__':main()
