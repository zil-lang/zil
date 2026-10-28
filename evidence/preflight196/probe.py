#!/usr/bin/env python3
"""Twelve source-only probes on the immutable, previously built 193 compiler."""
import hashlib
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parent
CLI = Path('/workspace/scratch/39be3faa4442/targets178/author193/debug/zil')
SOURCE = 'f594c57665aaf34d0c1d54df11d5047d4d042f0d'
PRODUCTION = '03c20e5b05c8066ed703d6d00e3968e9ac134585'
EXPECTED_SHA = '3830f924b7fa1223aeec88ecfd406f0a3012095ebd5be42a10aae0a7357ae33e'
SHARED = 'fn make(n:i32)->impl Fn()->i32{return move()=>n}'
MUTABLE = 'fn make(n:i32)->impl FnMut()->i32{var value=n;return move()=>{value+=1;return value}}'
PAIR = 'fn pair<F:Fn()->i32>(left:&F,right:&F)->i32{return left()+right()}'
MUT_PAIR = 'fn pair<F:FnMut()->i32>(left:&mut F,right:&mut F)->i32{return left()+right()+left()}'
ONCE_PAIR = 'fn pair<F:FnOnce()->i32>(left:F,right:F)->i32{return left()+right()}'

CASES = [
 ('00_one_role_control', SHARED + 'fn twice<F:Fn()->i32>(f:&F)->i32{return f()+f()}fn once<F:FnOnce()->i32>(f:F)->i32{return f()}fn main()->i32{let a=make(7);let b=make(4);return twice(&a)+once(b)-18}', 'Success', 'Existing one-role Fn and FnOnce adaptation control'),
 ('01_same_factory_two_instances_direct', MUTABLE + 'fn main()->i32{var left=make(1);var right=make(10);return left()+right()-13}', 'Success', 'Distinct runtime environments of one factory concrete type'),
 ('02_same_f_shared_distinct', SHARED + PAIR + 'fn main()->i32{let left=make(1);let right=make(2);return pair(&left,&right)-3}', 'Unsupported', 'Proposed positive: same F type, two distinct actual environments'),
 ('03_same_f_shared_same_actual', SHARED + PAIR + 'fn main()->i32{let f=make(4);return pair(&f,&f)-8}', 'Unsupported', 'Proposed positive: two shared roles may name one actual environment'),
 ('04_same_f_mut_distinct', MUTABLE + MUT_PAIR + 'fn main()->i32{var left=make(10);var right=make(20);return pair(&mut left,&mut right)-44}', 'Unsupported', 'Proposed positive: independent exclusive roles preserve their instances'),
 ('05_same_f_mut_same_actual', MUTABLE + MUT_PAIR + 'fn main()->i32{var f=make(10);return pair(&mut f,&mut f)}', 'Unsupported', 'Proposed rejection: simultaneous exclusive actuals alias; loan checker not reached today'),
 ('06_f_g_composition', 'fn compose<F:Fn(i32)->i32,G:Fn(i32)->i32>(map:&F,finish:&G,x:i32)->i32{return finish(map(x))}fn main()->i32{let map=move(v:i32)=>v+1;let finish=move(v:i32)=>v*2;return compose(&map,&finish,20)-42}', 'Unsupported', 'Proposed positive: independent generic F/G roles and ordered effects'),
 ('07_f_g_mixed_modes', 'fn step<F:FnMut(i32)->i32,G:Fn(i32)->i32>(update:&mut F,read:&G,x:i32)->i32{let next=update(x);return read(next)}fn main()->i32{var total=0;var update=move(v:i32)=>{total+=v;return total};let read=move(v:i32)=>v*2;return step(&mut update,&read,21)-42}', 'Unsupported', 'Proposed positive: one exclusive environment and one shared environment'),
 ('08_same_f_once_distinct', SHARED + ONCE_PAIR + 'fn main()->i32{let left=make(1);let right=make(2);return pair(left,right)-3}', 'Unsupported', 'Proposed positive: each actual environment is consumed once'),
 ('09_same_f_once_same_actual', SHARED + ONCE_PAIR + 'fn main()->i32{let f=make(1);return pair(f,f)}', 'Unsupported', 'Proposed rejection: duplicate owned actual does not gain a second move'),
 ('10_nested_role_permutation', SHARED + 'fn leaf<F:Fn()->i32>(a:&F,b:&F)->i32{return a()*10+b()}fn outer<F:Fn()->i32>(a:&F,b:&F)->i32{return leaf(a,b)+leaf(b,a)}fn main()->i32{let a=make(1);let b=make(2);return outer(&a,&b)-33}', 'Unsupported', 'Proposed positive: two static child call sites require different role substitutions'),
 ('11_second_role_desc_alias', 'resource R{n:i32}fn pair<F:Fn()->i32,G:Fn()->i32>(first:&F,second:&G)->i32{return first()+second()}fn main()->i32{let origin=R{n:2};let edit=origin;let held=&origin;let view=&held.n;let first=move()=>1;let second=()=>{edit.n+=1;return edit.n};let answer=pair(&first,&second);return answer+*view}', 'Unsupported', 'Proposed rejection: writes of role two overlap a live external shared view; loans not reached today'),
]

assert len(CASES) == 12
binary_sha = hashlib.sha256(CLI.read_bytes()).hexdigest()
assert binary_sha == EXPECTED_SHA
results = []
for name, source, expected_status, meaning in CASES:
    dest = ROOT / 'cases' / name
    dest.mkdir(parents=True, exist_ok=True)
    path = dest / 'source.zil'
    path.write_text(source + '\n')
    command = [str(CLI), 'check', str(path), '--stop-after', 'verify', '--emit-dir', str(dest / 'phases'), '--json']
    started = time.time()
    process = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=20)
    (dest / 'stdout.json').write_bytes(process.stdout)
    (dest / 'stderr.log').write_bytes(process.stderr)
    report = json.loads(process.stdout)
    row = {
        'case': name, 'purpose': meaning, 'source_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
        'command': command, 'compiler_source': SOURCE, 'compiler_production': PRODUCTION,
        'cli_sha256': binary_sha, 'exit_code': process.returncode, 'start': started, 'end': time.time(),
        'status': report['status'], 'phase': report['phase'], 'expected_current_status': expected_status,
        'verification_runs': report['verification_runs'], 'diagnostics': report['diagnostics'],
        'stdout_sha256': hashlib.sha256(process.stdout).hexdigest(),
        'stderr_sha256': hashlib.sha256(process.stderr).hexdigest(),
    }
    (dest / 'command.json').write_text(json.dumps(row, indent=2) + '\n')
    results.append(row)

summary = {
    'kind': 'ReadOnlySourcePreflightNotImplementation', 'compiler_source': SOURCE,
    'compiler_production': PRODUCTION, 'cli': str(CLI), 'cli_sha256': binary_sha,
    'input_count': len(results), 'expected_current_outcomes_match': all(r['status'] == r['expected_current_status'] for r in results),
    'results': results,
}
(ROOT / 'RESULTS.json').write_text(json.dumps(summary, indent=2) + '\n')
for row in results:
    print(json.dumps({k: row[k] for k in ('case', 'status', 'phase', 'verification_runs', 'diagnostics')}))
