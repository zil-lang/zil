"""Run the separately frozen compile5 composite-type mechanism window.

Uses the committed CP20 full-CLI/oracle primitive. Keeps every sample and
native product, scores each case separately, and never alters the main matrix.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import statistics
import sys
import time

sys.dont_write_bytecode = True


def save(path, value):
    with path.open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')


def main():
    parser = argparse.ArgumentParser()
    for name in ('repo', 'fixtures', 'baseline', 'candidate', 'clang', 'llvm', 'linker', 'compiler_builds', 'out'):
        parser.add_argument('--' + name.replace('_', '-'), type=Path, required=True)
    parser.add_argument('--base-revision', required=True)
    parser.add_argument('--candidate-revision', required=True)
    args = parser.parse_args()
    linker_invocation = args.linker.absolute().parent.resolve() / args.linker.name
    for name in ('repo', 'fixtures', 'baseline', 'candidate', 'clang', 'llvm', 'linker', 'compiler_builds'):
        setattr(args, name, getattr(args, name).resolve(strict=True))
    if any(os.environ.get(name) for name in ('LD_PRELOAD', 'LD_AUDIT')):
        raise ValueError('Loader interposition is outside the frozen probe environment')
    # Preserve the ld.lld invocation alias; its resolved file may be named lld.
    os.environ['LD_LIBRARY_PATH'] = str(args.llvm.parent)
    os.environ['ZIL_CLANG'] = str(args.clang)
    os.environ['ZIL_LINUX_LINKER'] = str(linker_invocation)
    helper = args.repo / 'tools/research/compile-performance/bench.py'
    spec = importlib.util.spec_from_file_location('compile5_frozen_common', helper)
    common = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(common)
    def host_identity():
        return {**common.host(), 'hostname': os.uname().nodename}
    meta = json.loads((args.fixtures / 'meta.json').read_text())
    if meta['formal_compile_rows'] != 36 or meta['blocks'] != 3:
        raise ValueError('Frozen probe count changed')
    for relative, expected in meta['files'].items():
        path = args.fixtures / relative
        if common.sha(path) != expected['sha256'] or path.stat().st_size != expected['bytes']:
            raise ValueError('Frozen input changed: ' + relative)
    if not {0, 1} <= os.sched_getaffinity(0):
        raise ValueError('The frozen CPU set is unavailable')
    builds = json.loads(args.compiler_builds.read_text())
    # Bind the supplied binaries to the actual same-CI compiler-build receipt.
    observed_builds = builds.get('builds', builds)
    identities = {'A': common.source_identity(args.base_revision),
                  'B': common.source_identity(args.candidate_revision)}
    binaries = {'A': common.inventory(args.baseline), 'B': common.inventory(args.candidate),
                'clang': common.inventory(args.clang)}
    def loaded_llvm(binary):
        result = common.capture(['ldd', str(binary)])
        paths = [Path(line.split('=>', 1)[1].split()[0]).resolve()
                 for line in result.get('stdout', '').splitlines()
                 if line.lstrip().startswith('libLLVM') and '=>' in line]
        if result.get('exit') != 0 or paths != [args.llvm]:
            raise ValueError('The actual loaded LLVM differs from the frozen file: ' + str(binary))
        return result
    loader_receipts = {arm: loaded_llvm(path) for arm, path in (('A', args.baseline), ('B', args.candidate))}
    # Query the actual DSO in an exited child so its mappings cannot inflate
    # the later fork/exec RSS or launch cost of measured compiler children.
    llvm_version = common.capture([sys.executable, '-c',
        'import ctypes,sys; lib=ctypes.CDLL(sys.argv[1]); f=lib.LLVMGetVersion; '
        'f.argtypes=[ctypes.POINTER(ctypes.c_uint)]*3; f.restype=None; '
        'v=[ctypes.c_uint() for _ in range(3)]; f(*(ctypes.byref(x) for x in v)); '
        'print(".".join(str(x.value) for x in v))', str(args.llvm)])
    versions = {'A': common.capture([str(args.baseline), 'version']),
                'B': common.capture([str(args.candidate), 'version']),
                'clang': common.capture([str(args.clang), '--version']),
                'linker': common.capture([str(linker_invocation), '--version']),
                'llvm': {**llvm_version, 'path': str(args.llvm), 'api': 'LLVMGetVersion'}}
    if any(versions[name].get('exit') != 0 for name in ('A', 'B', 'clang', 'linker', 'llvm')):
        raise ValueError('A required version invocation failed')
    for arm, label, revision in (('A', 'baseline', args.base_revision), ('B', 'candidate', args.candidate_revision)):
        receipt = observed_builds[label]
        if receipt['source_commit'] != revision or receipt['sha256'] != binaries[arm]['sha256'] or receipt['bytes'] != binaries[arm]['size']:
            raise ValueError('Supplied compiler/source identity does not match the CI receipt: ' + label)
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    root = out / 'plan'
    root.mkdir()
    cases = []
    for case in meta['cases']:
        folder = root / 'corpus' / case['id']
        source = folder / 'base/zil'
        source.mkdir(parents=True)
        for name in ('main.zil', 'work.zil'):
            shutil.copyfile(args.fixtures / case['id'] / name, source / name)
        for name in ('stdin.bin', 'stdout.bin', 'stderr.bin'):
            shutil.copyfile(args.fixtures / case['id'] / name, folder / name)
        prefix = 'corpus/' + case['id']
        cases.append({'id': case['id'], 'root': prefix,
                      'inputs': [{'path': prefix + '/stdin.bin',
                                  'expected': {'base': prefix + '/stdout.bin'}}]})
    plan = {
        'schema': 'zil-compile5-auxiliary-execution/1', 'created_unix': time.time(),
        'meta_sha256': common.sha(args.fixtures / 'meta.json'), 'meta': meta,
        'runner_sha256': common.sha(Path(__file__)), 'common_runner_sha256': common.sha(helper),
        'dependencies_sha256': {p: common.sha(args.repo / p) for p in common.DEPENDENCIES},
        'compiler_build_receipt_sha256': common.sha(args.compiler_builds),
        'compiler_build_receipt': builds, 'source_identity': identities,
        'binaries': binaries, 'extra_tools': [common.inventory(args.llvm), common.inventory(args.linker)],
        'linker_invocation': str(linker_invocation), 'loader_receipts': loader_receipts,
        'versions': versions,
        'host': host_identity(), 'environment': common.environment(), 'cpus': [0, 1],
        'profile': 'O0', 'units': 8, 'jobs': 4, 'object_cache': True,
        'timeout_seconds': 90, 'native_timeout_seconds': 30, 'cases': cases,
        'fixtures_sha256': common.file_hashes(root / 'corpus'),
        'order': [{**item, 'phase': 'build'} for item in meta['order']],
        'scope': 'Independent local auxiliary A/B window; no pooled score; no comparison with CI absolute times.',
    }
    save(root / 'plan.json', plan)

    def verify():
        if common.sha(Path(__file__)) != plan['runner_sha256'] or common.sha(helper) != plan['common_runner_sha256']:
            raise ValueError('Runner identity drift')
        for name, digest in plan['dependencies_sha256'].items():
            if common.sha(args.repo / name) != digest:
                raise ValueError('Helper identity drift')
        for item in [*plan['binaries'].values(), *plan['extra_tools']]:
            if common.inventory(item['path']) != item:
                raise ValueError('Binary/tool identity drift')
        if linker_invocation.resolve() != args.linker or common.sha(args.compiler_builds) != plan['compiler_build_receipt_sha256']:
            raise ValueError('Linker alias or compiler build receipt drift')
        for binary in (args.baseline, args.candidate):
            loaded_llvm(binary)
        if common.file_hashes(root / 'corpus') != plan['fixtures_sha256'] or common.environment() != plan['environment']:
            raise ValueError('Input/environment drift')
        host = host_identity()
        if any(host[k] != plan['host'][k] for k in ('hostname', 'cpu_model', 'cpu_max', 'memory_max', 'allowed_cpus')):
            raise ValueError('Host identity drift')

    def execute(label, order):
        verify()
        folder = out / label
        folder.mkdir()
        logs = folder / 'logs'
        logs.mkdir()
        products = folder / 'products'
        products.mkdir()
        records = []
        with (folder / 'records.jsonl').open('x') as stream:
            for index, seq in enumerate(order):
                verify()
                work = folder / ('work-' + str(index))
                work.mkdir()
                case = next(c for c in cases if c['id'] == seq['case'])
                row = common.one(plan, root, case, seq, 'cold_process', work, logs, f'{index:03d}')
                if 'cpu_user' in row and 'cpu_system' in row:
                    row['cpu_seconds'] = row['cpu_user'] + row['cpu_system']
                commands = row.get('reported_commands', [])
                row['actual_linker_contract'] = (isinstance(commands, list) and len(commands) == 1
                                                and isinstance(commands[0], list) and bool(commands[0])
                                                and commands[0][0] == str(args.clang)
                                                and '--ld-path=' + str(linker_invocation) in commands[0])
                row['pass'] = row['pass'] and row['actual_linker_contract']
                row.update(index=index, plan_sha256=common.sha(root / 'plan.json'))
                for suffix in ('', '.ll'):
                    source = work / ('program' + suffix)
                    if source.is_file():
                        target = products / (f'{index:03d}' + suffix)
                        source.rename(target)
                        row['retained_elf' if not suffix else 'retained_llvm'] = str(target.relative_to(out))
                stream.write(json.dumps(row) + '\n')
                stream.flush()
                records.append(row)
                shutil.rmtree(work)
                verify()
                print(label, index, seq['case'], seq['arm'], row['pass'], flush=True)
        verify()
        receipt = {'pass': all(r['pass'] for r in records), 'rows': len(records),
                   'failures': [r['index'] for r in records if not r['pass']],
                   'records_sha256': common.sha(folder / 'records.jsonl'),
                   'plan_sha256': common.sha(root / 'plan.json')}
        save(folder / 'complete.json', receipt)
        return records, receipt

    preflight = [{'block': 0, 'case': c['id'], 'position': pos, 'arm': arm, 'phase': 'build'}
                 for c in cases for pos, arm in enumerate('AB')]
    _, receipt = execute('preflight', preflight)
    if not receipt['pass']:
        save(out / 'result.json', {'status': 'PreflightFailed_NoScore', 'preflight': receipt,
                                  'scope': plan['scope']})
        return
    rows, receipt = execute('measure', plan['order'])
    result = {'status': 'Complete_AuxiliaryOnly' if receipt['pass'] and len(rows) == 36 else 'Failed_NoScore',
              'measure': receipt, 'by_case': {}, 'scope': plan['scope']}
    if result['status'] == 'Complete_AuxiliaryOnly':
        for case in cases:
            metrics = {}
            for metric in ('wall_seconds', 'cpu_seconds', 'max_process_rss_kib'):
                if not all(metric in row for row in rows):
                    continue
                blocks = {arm: [statistics.mean(r[metric] for r in rows
                                                if r['case'] == case['id'] and r['arm'] == arm and r['block'] == block)
                                for block in range(3)] for arm in 'AB'}
                a, b = (statistics.median(blocks[arm]) for arm in 'AB')
                metrics[metric] = {'A': a, 'B': b, 'B_over_A': b / a, 'block_means': blocks}
            result['by_case'][case['id']] = metrics
    save(out / 'result.json', result)
    print(json.dumps(result), flush=True)


if __name__ == '__main__':
    main()
