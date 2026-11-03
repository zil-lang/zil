"""Independent real-source/native probes; all outputs stay in supplied scratch."""
import argparse
import hashlib
import json
from pathlib import Path
import resource
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('cli', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
fixtures = Path(__file__).resolve().parent
rows = []


def capped():
    resource.setrlimit(resource.RLIMIT_CPU, (40, 40))
    resource.setrlimit(resource.RLIMIT_AS, (2 * 1024**3, 2 * 1024**3))
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))


def command(cmd, timeout=45):
    result = subprocess.run([str(x) for x in cmd], capture_output=True,
                            text=True, timeout=timeout, preexec_fn=capped)
    return result


def run_program(path, expected):
    result = command([path], timeout=5)
    assert (result.returncode, result.stdout, result.stderr) == (expected, '', ''), result
    return {'exit': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr}


for backend in ['clang', 'llvm-api']:
    for opt in ['O0', 'O2']:
        for name, expected in [
            ('large-enum-mixed-payload.zil', 0),
            ('large-enum-copy-snapshot.zil', 0),
            ('large-nested-error-false.zil', 0),
            ('large-nested-error-true.zil', 1),
            ('large-unit-error-false.zil', 0),
            ('large-unit-error-true.zil', 1),
        ]:
            source = fixtures / name
            key = f'{source.stem}-{backend}-{opt}'
            output = args.output / key
            cmd = [args.cli, 'build', source, '--backend', backend, '--opt', opt,
                   '--emit-dir', args.output / f'{key}-ir', '-o', output, '--json']
            cache = None
            if backend == 'llvm-api' and name == 'large-nested-error-false.zil':
                cache = args.output / f'{key}-objects'
                cmd += ['--object-cache', cache]
            result = command(cmd)
            report = json.loads(result.stdout)
            assert report['status'] == 'Success', report
            assert report['verification_runs'] == 1, report
            native = report['native_codegen']
            assert native['backend'] == backend and native['fallback'] is None, report
            llvm = Path(report['artifacts']['llvm'])
            text = llvm.read_text()
            assert '.lc1' in text and '@zil.copy.bytes' in text
            assert not any(' = load { i32,' in line or ' = load { i1, { i32,' in line
                           for line in text.splitlines()), 'whole large Sum load'
            if name == 'large-enum-copy-snapshot.zil':
                assert 'define ptr @zil.fn.1.lc1(' in text, 'fnptr factory must be versioned'
            row = {'kind': 'source-native', 'input': name, 'backend': backend, 'opt': opt,
                   'command': [str(x) for x in cmd], 'verification_runs': 1,
                   'native_codegen': native, 'run': run_program(output, expected),
                   'executable_sha256': hashlib.sha256(output.read_bytes()).hexdigest(),
                   'llvm_sha256': hashlib.sha256(llvm.read_bytes()).hexdigest()}
            rows.append(row)
            print(name, backend, opt, row['run'], flush=True)
            if name == 'large-nested-error-false.zil':
                objects = []
                if cache:
                    warm = command(cmd)
                    warm_report = json.loads(warm.stdout)
                    summary = warm_report['native_codegen']['object_cache']
                    assert warm_report['verification_runs'] == 1
                    assert summary['hits'] > 0 and summary['misses'] == 0, warm_report
                    rows.append({'kind': 'object-cache-warm', 'backend': backend, 'opt': opt,
                                 'summary': summary, 'verification_runs': 1,
                                 'run': run_program(output, expected)})
                    for index, cached in enumerate(sorted(cache.rglob('*.zobj'))):
                        data = cached.read_bytes()
                        assert data[:8] == b'ZILOBJ01'
                        assert data[8:40].hex() == cached.stem
                        payload = data[80:]
                        assert len(payload) == int.from_bytes(data[72:80], 'little')
                        assert hashlib.sha256(payload).digest() == data[40:72]
                        object_path = args.output / f'{key}-{index}.o'
                        object_path.write_bytes(payload)
                        objects.append(object_path)
                    assert objects, 'current API compilation must store actual object'
                else:
                    object_path = args.output / f'{key}.o'
                    compiled = command(['clang', '-x', 'ir', f'-{opt}', '-c', llvm,
                                        '-o', object_path])
                    assert compiled.returncode == 0, compiled
                    objects.append(object_path)
                consumer = args.output / f'{key}-consumer'
                link = ['clang', f'-{opt}', fixtures / 'large-error-payload-consumer.c',
                        *objects, '-o', consumer]
                linked = command(link)
                assert linked.returncode == 0, linked
                rows.append({'kind': 'C-machine-consumer', 'backend': backend, 'opt': opt,
                             'object_origin': 'actual cached llvm-api objects' if cache else 'actual emitted LLVM compiled by Clang',
                             'command': [str(x) for x in link],
                             'object_sha256': [hashlib.sha256(p.read_bytes()).hexdigest() for p in objects],
                             'run': run_program(consumer, 0)})
                print('C ABI payload consumer', backend, opt, 'exit0', flush=True)
(args.output / 'result.json').write_text(json.dumps(rows, indent=2) + '\n')
