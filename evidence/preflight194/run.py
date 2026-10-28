#!/usr/bin/env python3
"""Read-only language admission probes, not native or performance testing."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import time

ROOT = Path('/workspace/scratch/39be3faa4442')
OUT = Path(__file__).resolve().parent
CLI = ROOT / 'targets178/author193/debug/zil'
SOURCE = 'f594c57665aaf34d0c1d54df11d5047d4d042f0d'
CLI_SHA = '3830f924b7fa1223aeec88ecfd406f0a3012095ebd5be42a10aae0a7357ae33e'
MAIN = 'df4546b15d98647a0a677aa9ac279b55868bfe46'
REPO = ROOT / 'language14/wt192'

CASES = [
 ('01-ordinary-unbound-control', 'ordinary single-use generic baseline',
  'fn id<T>(x:T)->T=>x;fn main()->i32{let a:i32=id(7);let b:i64=id(i64(9));return a+i32(b)-16}'),
 ('02-one-callable-control', 'existing one-environment helper baseline',
  'fn apply<F:Fn(i32)->i32>(f:&F,x:i32)->i32=>f(x);fn main()->i32{let f=move(n:i32)=>n+1;return apply(&f,6)-7}'),
 ('03-copy-bound-alone', 'Copy bound independent prerequisite',
  'fn id<T:Copy>(x:T)->T=>x;fn main()->i32{return id(7)-7}'),
 ('04-map-copy-f', 'unchanged historical audit input',
  'fn apply<T:Copy,F:Fn(T)->T>(f:&F,x:T)->T=>f(x);fn main()->i32{let f=move(n:i32)=>n+1;return apply(&f,6)-7}\n'),
 ('05-map-f-copy', 'generic order must not determine inference or identity',
  'fn apply<F:Fn(T)->T,T:Copy>(f:&F,x:T)->T=>f(x);fn main()->i32{let f=move(n:i32)=>n+1;return apply(&f,6)-7}'),
 ('06-extra-i32-i64', 'same closure identity, T absent from callable signature, differing scalar widths',
  'fn keep<T:Copy,F:Fn()->i32>(f:&F,x:T)->T{f();return x}fn main()->i32{let f=move()=>1;let a:i32=keep(&f,i32(7));let b:i64=keep(&f,i64(9));return a+i32(b)-16}'),
 ('07-extra-nominal', 'same closure identity and layout, distinct nominal Copy arguments',
  'struct A{n:i32}struct B{n:i32}fn keep<T:Copy,F:Fn()->i32>(f:&F,x:T)->T{f();return x}fn main()->i32{let f=move()=>1;let a=keep(&f,A{n:7});let b=keep(&f,B{n:9});return a.n+b.n-16}'),
 ('08-nested-map', 'symbolic T substitution through existing acyclic helper composition',
  'fn leaf<T:Copy,F:Fn(T)->T>(f:&F,x:T)->T=>f(x);fn outer<T:Copy,F:Fn(T)->T>(f:&F,x:T)->T=>leaf(f,x);fn main()->i32{let f=move(n:i32)=>n+1;return outer(&f,6)-7}'),
 ('09-fnmut-copy', 'existing mutable environment mode plus Copy data',
  'fn apply<T:Copy,F:FnMut(T)->T>(f:&mut F,x:T)->T=>f(x);fn main()->i32{var total=0;var f=(n:i32)=>{total+=n;return total};return apply(&mut f,7)-7}'),
 ('10-fnonce-copy', 'existing owned environment mode plus Copy data',
  'fn apply<T:Copy,F:FnOnce(T)->T>(f:F,x:T)->T=>f(x);fn main()->i32{let offset=1;let f=move(n:i32)=>n+offset;return apply(f,6)-7}'),
 ('11-copy-address-parameter', '189 explicit address shape must survive substitution',
  'fn apply<T:Copy,F:Fn(&mut T)->i32>(f:&F,x:&mut T)->i32=>f(x);fn main()->i32{let f=move(n:&mut i32)=>{*n+=1;return *n};var n=6;return apply(&f,&mut n)-7}'),
 ('12-unused-bad-copy-body', 'unused Copy bound grants no numeric capability',
  'fn bad<T:Copy,F:Fn(T)->T>(f:&F,x:T)->T=>x+1;fn main()->i32=>0;'),
 ('13-unbounded-data-type', 'proposed slice does not infer a missing Copy bound from one scalar use',
  'fn apply<T,F:Fn(T)->T>(f:&F,x:T)->T=>f(x);fn main()->i32{let f=move(n:i32)=>n+1;return apply(&f,6)-7}'),
 ('14-owned-package-data', 'Copy generic must not admit an owning Package',
  'resource R{n:i32}package P{manages r:R}fn inspect<T:Copy,F:Fn()->i32>(f:&F,x:T)->i32=>f();fn main()->i32{let f=move()=>0;let p=P{r:R{n:7}};return inspect(&f,p)}'),
 ('15-two-parameters-one-f', 'unchanged historical audit input; two runtime environment roles',
  'fn twice<F:Fn()->i32>(a:&F,b:&F)->i32=>a()+b();fn main()->i32{let f=move()=>7;return twice(&f,&f)-14}\n'),
 ('16-two-callable-types', 'unchanged historical audit input; independent environments',
  'fn apply<F:Fn(i32)->i32,G:Fn(i32)->i32>(f:&F,g:&G,x:i32)->i32=>g(f(x));fn main()->i32{let f=move(n:i32)=>n+1;let g=move(n:i32)=>n*2;return apply(&f,&g,3)-8}\n'),
 ('17-nested-mixed-mode', 'existing explicit boundary between parent and child declared modes',
  'fn inner<F:FnMut()->i32>(f:&mut F)->i32=>f();fn outer<F:Fn()->i32>(f:&mut F)->i32=>inner(f);fn main()->i32{var f=move()=>7;return outer(&mut f)-7}'),
 ('18-mode-adaptation-control', 'one concrete Fn environment may instantiate separate Fn and FnMut helpers',
  'fn read<F:Fn()->i32>(f:&F)->i32=>f();fn update<F:FnMut()->i32>(f:&mut F)->i32=>f();fn main()->i32{var f=move()=>7;let a=read(&f);return a+update(&mut f)-14}'),
 ('19-runtime-loop-copy', 'existing Flow loop with ordinary Copy carrier',
  'fn repeat<T:Copy,F:Fn(T)->T>(f:&F,x:T,n:i32)->T{var value=x;var i=n;while i>0{value=f(value);i-=1}return value}fn main()->i32{let f=move(n:i32)=>n+1;return repeat(&f,4,3)-7}'),
]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def git(*args):
    return subprocess.check_output(['git', *args], cwd=REPO)


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def main():
    assert len(CASES) <= 20
    assert digest(CLI.read_bytes()) == CLI_SHA
    assert not (OUT / 'SUMMARY.json').exists(), 'do not overwrite an earlier probe record'
    receipt_source = ROOT / 'run-logs/193-author/final-build01/command'
    receipt = json.loads((receipt_source / 'command.json').read_text())
    assert receipt['source_commit'] == SOURCE and receipt['git_status'] == ''
    assert receipt['status'] == 'Pass' and receipt['exit_code'] == 0
    receipt_out = OUT / 'build193'
    receipt_out.mkdir(exist_ok=True)
    for name in ['command.json', 'stdout.log', 'stderr.log']:
        shutil.copyfile(receipt_source / name, receipt_out / name)
    related = [
        'crates/zil-expand/src/generics.rs', 'crates/zil-expand/src/generic_callable.rs',
        'crates/zil-expand/src/generic_check.rs', 'crates/zil-expand/src/generic_trait_bounds.rs',
        'crates/zil-expand/src/generic_store.rs', 'crates/zil-expand/src/receivers.rs',
        'crates/zil-types/src/helpers.rs', 'crates/zil-types/src/helper_plan.rs',
        'crates/zil-types/src/closures.rs', 'crates/zil-ir/src/lib.rs',
        'crates/zil-ir/src/wire.rs', 'crates/zil-ir/src/program.rs',
    ]
    snapshots = {}
    for label, revision in [('observed_cli_source', SOURCE), ('read_main', MAIN)]:
        registry = json.loads(git('show', revision + ':data/normative-inputs.json'))
        snapshots[label] = {
            'commit': revision,
            'design_sha256': digest(git('show', revision + ':_zil_design.md')),
            'normative_hash_from_committed_registry': registry['normative_hash'],
            'source_sha256': {p: digest(git('show', revision + ':' + p)) for p in related},
        }
    identity = {
        'kind': 'read-only-preflight', 'cli': str(CLI), 'cli_sha256': CLI_SHA,
        'cli_size': CLI.stat().st_size, 'native_executed': 0,
        'evidence_worktree_head_before_probes': git('rev-parse', 'HEAD').decode().strip(),
        'evidence_worktree_status_before_probes': git('status', '--porcelain').decode(),
        'build_receipt_origin': str(receipt_source), 'snapshots': snapshots,
        'note': 'Probe results belong to clean f594 build, not main or the evidence worktree. No production source changed.',
    }
    write_json(OUT / 'IDENTITY.json', identity)
    rows = []
    for name, purpose, source in CASES:
        case = OUT / 'cases' / name
        case.mkdir(parents=True, exist_ok=False)
        input_path = case / 'main.zil'
        input_path.write_text(source)
        argv = [str(CLI), 'check', str(input_path), '--stop-after', 'verify', '--json', '--emit-dir', str(case / 'phases')]
        start = time.time()
        result = subprocess.run(argv, cwd=REPO, capture_output=True, timeout=30)
        end = time.time()
        (case / 'stdout.json').write_bytes(result.stdout)
        (case / 'stderr.log').write_bytes(result.stderr)
        report = json.loads(result.stdout)
        command = {'argv': argv, 'cwd': str(REPO), 'source_commit': SOURCE,
                   'cli_sha256': CLI_SHA, 'exit_code': result.returncode,
                   'start': start, 'end': end, 'input_sha256': digest(input_path.read_bytes()),
                   'stdout_sha256': digest(result.stdout), 'stderr_sha256': digest(result.stderr)}
        write_json(case / 'command.json', command)
        row = {'name': name, 'purpose': purpose, 'input': f'cases/{name}/main.zil',
               'input_sha256': command['input_sha256'], 'exit_code': result.returncode,
               'requested_stop_after': 'verify', 'status': report['status'], 'stop_phase': report['phase'],
               'diagnostics': report['diagnostics'], 'verification_runs': report['verification_runs'],
               'cache_hit': report.get('cache_hit'), 'query_events': report.get('query_events'),
               'native_executed': 0}
        rows.append(row)
        print(name, row['status'], row['stop_phase'], 'verify=' + str(row['verification_runs']), flush=True)
    assert digest(CLI.read_bytes()) == CLI_SHA, 'CLI changed during probing'
    write_json(OUT / 'SUMMARY.json', {
        'kind': 'read-only-preflight', 'observed_source_commit': SOURCE, 'cli_sha256': CLI_SHA,
        'executed': len(rows), 'native_executed': 0, 'verification_runs': sum(r['verification_runs'] for r in rows),
        'outcomes': {s: sum(r['status'] == s for r in rows) for s in sorted({r['status'] for r in rows})},
        'rows': rows,
        'note': 'Observed acceptance and rejection, not a 194 implementation or acceptance verdict. All original failures retained.',
    })


if __name__ == '__main__':
    main()
