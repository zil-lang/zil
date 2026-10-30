#!/usr/bin/env python3
"""Exact-binary native-output/temp-collision regression and recovery control."""
import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--sha256', required=True)
p.add_argument('--commit', required=True)
p.add_argument('--kind', choices=['parse', 'query', 'object'], required=True)
p.add_argument('--directory', type=Path, required=True)
p.add_argument('--clang', type=Path, required=True)
args = p.parse_args()
root = args.directory.resolve()
root.mkdir()
sha = lambda path: hashlib.sha256(Path(path).read_bytes()).hexdigest()
assert sha(args.binary) == args.sha256
source = root/'main.zil'
source.write_text('fn main()->i32=>0\n')
cache = root/'cache'
rows = []

def run(name, command, native=False):
    proc = subprocess.run([str(x) for x in command], capture_output=True)
    (root/f'{name}.json').write_bytes(proc.stdout)
    (root/f'{name}.stderr').write_bytes(proc.stderr)
    assert proc.returncode == 0, (name, proc.returncode, proc.stderr, proc.stdout)
    report = json.loads(proc.stdout)
    assert report['status'] == 'Success' and report['verification_runs'] == 1, report
    row = {'name':name, 'command':[str(x) for x in command], 'status':report['status'], 'verification_runs':report['verification_runs'], 'stdout_sha256':sha(root/f'{name}.json'), 'stderr_sha256':sha(root/f'{name}.stderr'), 'query_cache':report.get('query_cache'), 'parse_cache':report.get('parse_cache'), 'object_cache':(report.get('native_codegen') or {}).get('object_cache')}
    if native:
        output = Path(report['artifacts']['executable'])
        llvm = Path(str(output)+'.ll')
        assert output.is_file() and llvm.is_file(), (name, output)
        row.update({'output':str(output), 'output_sha256':sha(output), 'llvm_sha256':sha(llvm)})
        executed = subprocess.run([str(output)],capture_output=True)
        (root/f'{name}.native.stdout').write_bytes(executed.stdout)
        (root/f'{name}.native.stderr').write_bytes(executed.stderr)
        assert executed.returncode == 0 and not executed.stdout and not executed.stderr
        row['native_exit'] = executed.returncode
    rows.append(row)
    return report

base = [args.binary,'build',source,'--json','--clang',args.clang]
flag = '--object-cache' if args.kind == 'object' else '--query-cache'
key = ''
namespace = cache/'frontend-cp12-v1'
if args.kind == 'parse':
    run('seed', [args.binary,'check',source,'--query-cache',cache,'--json'])
    namespace = cache/'parse-cp22-v1'
    entries = list(namespace.glob('*.psyn'))
    assert len(entries) == 1
    key = entries[0].stem
    entries[0].write_bytes(b'force parse repair')
elif args.kind == 'object':
    run('seed', base+[flag,cache,'-o',root/'seed'],native=True)
    namespaces = list((cache/'objects-v1').iterdir())
    assert len(namespaces) == 1
    namespace = namespaces[0]
    entries = list(namespace.iterdir())
    assert len(entries) == 1
    entries[0].write_bytes(b'force object repair')

helper = root/'exec-same-pid.py'
helper.write_text('''import os,sys\nfrom pathlib import Path\nbinary,source,cache,namespace,key,clang,kind,enabled=sys.argv[1:]\nname=f'.{key}.{os.getpid()}.0.tmp' if kind=='parse' else f'.pending-{os.getpid()}-0'\noutput=Path(namespace)/name\nargv=[binary,'build',source,'--json','--clang',clang,'-o',str(output)]\nif enabled=='yes':argv+=['--object-cache' if kind=='object' else '--query-cache',cache]\nos.execv(binary,argv)\n''')
for enabled in ['no','yes']:
    report = run('collision-'+enabled, [sys.executable,helper,args.binary,source,cache,namespace,key,args.clang,args.kind,enabled],native=True)
    if enabled == 'yes':
        if args.kind == 'parse':
            status = report['parse_cache']
            assert status['stored'] == 0 and status['corrupt'] == 1
            assert 'File exists' in status['write_reason'] or 'os error 17' in status['write_reason']
        elif args.kind == 'query':
            status = report['query_cache']
            assert status['stored'] is False
            assert 'File exists' in status['write_reason'] or 'os error 17' in status['write_reason']
        else:
            status = report['native_codegen']['object_cache']
            assert status['enabled'] and status['stored'] == 0 and status['write_failures'] == 1

recovery_command = base+[flag,cache,'-o',root/'recovered']
recovered = run('recovery',recovery_command,native=True)
warm = run('warm',recovery_command,native=True)
if args.kind == 'parse':
    assert recovered['parse_cache']['stored'] == 1
    assert warm['parse_cache']['hits'] == 1 and warm['parse_cache']['parsed'] == 0
elif args.kind == 'query':
    assert recovered['query_cache']['stored'] is True
    assert warm['query_cache']['state'] == 'hit'
else:
    assert recovered['native_codegen']['object_cache']['stored'] == 1
    assert warm['native_codegen']['object_cache']['hits'] == 1
native_rows = [row for row in rows if 'native_exit' in row]
assert len({row['output_sha256'] for row in native_rows}) == 1
assert len({row['llvm_sha256'] for row in native_rows}) == 1
assert sha(args.binary) == args.sha256
result = {'verdict':'Pass', 'scope':args.kind, 'source_commit':args.commit, 'binary':str(args.binary), 'binary_sha256':args.sha256, 'script_sha256':sha(__file__), 'cli_requests':len(rows), 'native_controls':len(native_rows), 'rows':rows, 'conclusion':'Preexisting native output survives create_new collision, remains runnable and byte-equivalent to cache-off. Cache failure is advisory; later repair and warm reuse succeed with fresh Verify.'}
(root/'result.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k!='rows'}))
