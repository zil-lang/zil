#!/usr/bin/env python3
"""Paired complete O2 requests on six opaque-input families, including real edits.

This is a compilation experiment. Five native oracle checks follow each build
outside its timer. The separately prepared runtime experiment measures code quality.
"""
import argparse
from contextlib import contextmanager
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import statistics
import subprocess
import sys
import time

from stage6_merge_runtime import CASES, SEEDS, generate

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
REQUESTS = [('cold', 0), ('unchanged', 0), ('early_edit', 1),
            ('late_edit', 2), ('restore', 0)]
ISOLATED_ENV = ['LD_LIBRARY_PATH', 'LD_PRELOAD', 'LD_AUDIT', 'LD_DEBUG',
                'LD_DEBUG_OUTPUT', 'ZIL_CLANG', 'ZIL_LLVM_LIB_DIR']
ROUNDS = 20000


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def identity(path):
    path = Path(path).resolve(strict=True)
    return {'path': str(path), 'sha256': sha(path), 'bytes': path.stat().st_size}


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def measured(command, run, label, timeout):
    """Same full-CLI clock/wait4 boundary, with interruption cleanup and receipts."""
    def expired(_signal, _frame):
        raise TimeoutError('compiler timeout')

    prior_handler = signal.getsignal(signal.SIGALRM)
    child = None
    result = {'exit': None, 'timeout': False, 'cpu_seconds': None, 'max_rss_kib': None}
    with (run / (label + '.stdout')).open('wb') as stdout, \
            (run / (label + '.stderr')).open('wb') as stderr:
        started = time.perf_counter_ns()

        def reap(kill=False):
            if kill:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            _, status, usage = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
            result.update(exit=child.returncode, cpu_seconds=usage.ru_utime + usage.ru_stime,
                          max_rss_kib=usage.ru_maxrss)

        try:
            try:
                child = subprocess.Popen(command, stdout=stdout, stderr=stderr, start_new_session=True)
            except OSError as error:
                result['launch_error'] = str(error)
                return result
            signal.signal(signal.SIGALRM, expired)
            signal.alarm(timeout)
            try:
                reap()
            except TimeoutError:
                signal.alarm(0)
                result['timeout'] = True
                reap(kill=True)
        except BaseException as error:
            signal.alarm(0)
            if child is not None and child.returncode is None:
                try:
                    reap(kill=True)
                except ChildProcessError:
                    # A signal can arrive just after wait4 reaped the child,
                    # before its Python returncode assignment. The group was
                    # still killed; preserve the interruption in that race.
                    pass
            result.update(elapsed_ns=time.perf_counter_ns() - started, status='Interrupted',
                          error=repr(error), command=[str(x) for x in command],
                          pid=child.pid if child is not None else None)
            save(run / (label + '.interrupted.json'), result)
            raise
        finally:
            signal.alarm(0)
            signal.signal(signal.SIGALRM, prior_handler)
            result['elapsed_ns'] = time.perf_counter_ns() - started
    return result


def checked(spec):
    value = identity(spec['path'])
    if value['sha256'] != spec['sha256']:
        raise ValueError('identity mismatch: ' + value['path'])
    return value


def clean_environment():
    env = os.environ.copy()
    for name in ISOLATED_ENV:
        env.pop(name, None)
    return env


@contextmanager
def isolated_process_environment():
    """The sequential CLI timer inherits this explicitly clean environment."""
    old = {name: os.environ[name] for name in ISOLATED_ENV if name in os.environ}
    for name in ISOLATED_ENV:
        os.environ.pop(name, None)
    try:
        yield
    finally:
        for name in ISOLATED_ENV:
            os.environ.pop(name, None)
        os.environ.update(old)


def freeze_pair(path, out):
    pair = json.loads(path.read_text())
    if pair.get('schema') != 1 or pair.get('status') != 'Pass':
        raise ValueError('pair receipt must have passed')
    supplied = pair['arms']
    if len(supplied) != 2 or {a['name'] for a in supplied} != {'original', 'candidate'}:
        raise ValueError('requires one original and one candidate')
    clang = checked(pair['clang'])
    arms = {}
    for value in supplied:
        arm = dict(value)
        for field in ['compiler', 'launcher', 'source_receipt']:
            arm[field] = checked(value[field])
        arm['llvm'] = {key: checked(value['llvm'][key]) for key in ['library', 'receipt']}
        native = json.loads(Path(arm['llvm']['receipt']['path']).read_text())
        source = json.loads(Path(arm['source_receipt']['path']).read_text())
        if native.get('status') != 'Pass' or native['arm'] != arm['toolchain_arm']:
            raise ValueError('wrong native toolchain arm')
        if checked(native['library']) != arm['llvm']['library']:
            raise ValueError('native library identity differs')
        if (source['revision'] != arm['production_revision'] or
                source['arm'] != arm['toolchain_arm'] or source['production_mutated'] is not False or
                source['pipeline_identity_tag'] != arm['pipeline_identity_tag'] or
                source['original_files_manifest_sha256'] != arm['original_files_manifest_sha256'] or
                checked(source['toolchain_receipt']) != arm['llvm']['receipt']):
            raise ValueError('source or pipeline identity differs')
        if arm['pipeline_identity_tag'].encode() not in Path(arm['compiler']['path']).read_bytes():
            raise ValueError('compiler lacks its pipeline identity tag')
        if (Path(arm['env']['LD_LIBRARY_PATH']).resolve() != Path(arm['llvm']['library']['path']).parent or
                identity(arm['env']['ZIL_CLANG']) != clang):
            raise ValueError('launcher environment differs')
        # A fresh actual compiler PID probe, outside the compilation timer.
        env = clean_environment()
        env.update(LD_DEBUG='libs', LD_DEBUG_OUTPUT=str(out / (arm['name'] + '-loader')))
        command = [arm['launcher']['path'], '--help']
        with (out / (arm['name'] + '-help.stdout')).open('wb') as stdout, \
                (out / (arm['name'] + '-help.stderr')).open('wb') as stderr:
            process = subprocess.Popen(command, env=env, stdin=subprocess.DEVNULL,
                                       stdout=stdout, stderr=stderr, start_new_session=True)
            try:
                result = process.wait(timeout=30)
            except BaseException:
                import signal
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
                raise
        loader = out / (arm['name'] + '-loader.' + str(process.pid))
        matches = re.findall(r'calling init:\s*(/\S*libLLVM\S*)', loader.read_text())
        actual = {str(Path(p).resolve(strict=True)) for p in matches}
        arm['fresh_loader'] = {'pid': process.pid, 'command': command, 'exit': result,
                               'log': identity(loader), 'actual_libraries': sorted(actual)}
        save(out / (arm['name'] + '-binding.json'), arm)
        if result != 0 or actual != {arm['llvm']['library']['path']}:
            raise ValueError('actual compiler loaded another library')
        arms[arm['name']] = arm
    if (len({a['production_revision'] for a in arms.values()}) != 1 or
            len({a['original_files_manifest_sha256'] for a in arms.values()}) != 1 or
            len({a['llvm']['library']['sha256'] for a in arms.values()}) != 2):
        raise ValueError('isolation requires common source and two distinct toolchains')
    return arms


def generate_variant(root, n, flavor, edit):
    """Reuse the frozen runtime body; edit exactly one real helper in both languages."""
    root.mkdir(parents=True, exist_ok=False)
    generate(root, n, flavor, ROUNDS)
    if edit not in [0, 1, 2]:
        raise ValueError('unknown edit')
    if edit:
        index = 0 if edit == 1 else n - 1
        for ext in ['zil', 'rs']:
            path = root / ('main.' + ext)
            lines = path.read_text().splitlines(keepends=True)
            matched = [i for i, line in enumerate(lines) if line.startswith(f'fn h{index}(')]
            if len(matched) != 1:
                raise ValueError('helper match is not unique')
            at = matched[0]
            replacement, count = re.subn(r'return ([^;]+);',
                                         lambda m: f'return ({m[1]})+{edit};', lines[at])
            if count != (2 if flavor == 'branch' else 1):
                raise ValueError('unexpected number of helper exits')
            lines[at] = replacement
            path.write_text(''.join(lines))


def expected_values(n, flavor, edit):
    """Arithmetic oracle independent of generated source parsing or either compiler."""
    values = []
    for x in range(1009):
        if flavor == 'body':
            total = sum(((x + 64 + residue) % 1009) * ((n + 96 - residue) // 97)
                        for residue in range(97))
        elif flavor == 'branch' and x % 3 != 0:
            total = sum(((x + residue) % 1009) * ((n + 30 - residue) // 31)
                        for residue in range(31))
        else:
            total = sum(((3 * x + residue) % 1009) * ((n + 96 - residue) // 97)
                        for residue in range(97))
        values.append(total + edit)
    expected = {}
    for seed in SEEDS:
        result, x = 0, seed
        for _ in range(ROUNDS):
            result = (result + values[x]) % 1000003
            x = (x + 17) % 1009
        expected[seed] = result
    return expected


def native_check(exe, prefix, expected, timeout=60):
    record = {'command': [str(exe)], 'input_hex': bytes([expected[0]]).hex(),
              'expected': expected[1], 'valid': False}
    try:
        result = subprocess.run([str(exe)], input=bytes([expected[0]]),
                                capture_output=True, env=clean_environment(), timeout=timeout)
        stdout, stderr = result.stdout, result.stderr
        record.update(exit=result.returncode, timeout=False)
        record['valid'] = result.returncode == 0 and not stderr and stdout == expected[1].to_bytes(4, 'little')
    except (OSError, subprocess.TimeoutExpired) as error:
        stdout, stderr = getattr(error, 'stdout', None) or b'', getattr(error, 'stderr', None) or b''
        record.update(error=repr(error), timeout=isinstance(error, subprocess.TimeoutExpired))
    Path(str(prefix) + '.stdout').write_bytes(stdout)
    Path(str(prefix) + '.stderr').write_bytes(stderr)
    record.update(stdout_hex=stdout.hex(), stderr_hex=stderr.hex())
    return record


def validate_zil_report(report, expected_tag):
    if report['status'] != 'Success' or report['verification_runs'] != 1:
        raise ValueError('current Verify must succeed exactly once')
    native = report['native_codegen']
    if native['backend'] != 'llvm-api' or native['actual_units'] <= 0:
        raise ValueError('native LLVM API generation required')
    objects = native['object_reports']
    if len(objects) != native['actual_units']:
        raise ValueError('one object report per actual unit required')
    for item in objects:
        if (item['backend'] != 'llvm-api' or item['pipeline'] !=
                'default<O2>|merge-functions=after|aliases=false' + expected_tag):
            raise ValueError('wrong object pipeline identity')
        if item['input_kind'] == 'object-cache':
            if (item['cache']['state'] != 'hit' or item['producer_verified_before_after'] is not True or
                    item['llvm_verification_this_request'] is not False):
                raise ValueError('cached object lacks verified producer')
        elif item.get('verify_before_after') is not True:
            raise ValueError('new object lacks LLVM pre/post verification')
    cache = native['object_cache']
    if cache['requested'] is not True or cache['enabled'] is not True:
        raise ValueError('object cache unexpectedly disabled')
    return native


def summarize(rows, cases, blocks, reference):
    cells = []
    for case in cases:
        for label, _ in REQUESTS:
            arms = {}
            for arm in ['candidate', reference]:
                selected = [r for r in rows if (r['case'], r['request'], r['arm']) == (case, label, arm)]
                values = [r['elapsed_ns'] / 1e6 for r in selected if 'elapsed_ns' in r]
                memory = [r['max_rss_kib'] for r in selected if r.get('max_rss_kib') is not None]
                arms[arm] = {'observations': len(selected), 'valid': sum(r['valid'] for r in selected),
                             'median_ms': statistics.median(values) if values else None,
                             'median_max_rss_kib': statistics.median(memory) if memory else None,
                             'values_ms': values}
            valid = all(a['observations'] == blocks and a['valid'] == blocks for a in arms.values())
            cells.append({'case': case, 'request': label, 'arms': arms, 'valid': valid,
                          'candidate_over_' + reference: arms['candidate']['median_ms'] / arms[reference]['median_ms'] if valid else None})
    return {'rows': len(rows), 'cells': cells, 'failures': sum(not r['valid'] for r in rows)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pair-receipt', type=Path, required=True)
    parser.add_argument('--comparison', choices=['isolation', 'rust'], required=True)
    parser.add_argument('--rustc', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--blocks', type=int, required=True)
    parser.add_argument('--case', action='append', choices=[f'{n}-{f}' for n, f in CASES])
    parser.add_argument('--closed-executable', action='store_true')
    parser.add_argument('--incremental-o2', action='store_true')
    parser.add_argument('--timeout', type=int, default=180)
    parser.add_argument('--control-only', action='store_true', help='Label a non-six-block functional control explicitly')
    args = parser.parse_args()
    if args.blocks <= 0 or args.timeout <= 0 or (args.blocks != 6 and not args.control_only):
        parser.error('formal windows require six blocks; positive --blocks/--timeout always required')
    if args.comparison == 'rust' and not args.rustc:
        parser.error('--rustc required for Rust comparison')
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    cpus = sorted(os.sched_getaffinity(0))[:2]
    os.sched_setaffinity(0, cpus)
    cases = [(n, f) for n, f in CASES if not args.case or f'{n}-{f}' in args.case]
    reference = 'original' if args.comparison == 'isolation' else 'rust'
    rows = []
    manifest = {'schema': 1, 'status': 'Preparing', 'scope': __doc__,
                'control_only': args.control_only, 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                'cpus': cpus, 'blocks': args.blocks, 'cases': [f'{n}-{f}' for n, f in cases],
                'comparison': args.comparison, 'reference': reference, 'requests': REQUESTS,
                'rounds': ROUNDS, 'seeds': SEEDS, 'closed_executable': args.closed_executable,
                'incremental_o2': args.incremental_o2, 'planned_requests': len(cases) * args.blocks * 10,
                'pair': identity(args.pair_receipt), 'tools': {name: identity(HERE / name) for name in
                    ['stage6_opaque_bench.py', 'stage6_bench.py', 'stage6_merge_runtime.py']},
                'invocation': sys.argv,
                'order': 'AB/BA alternating complete five-request sequences; three first positions per arm in six blocks',
                'timing_boundary': 'Full compiler CLI only. Identity checks, source generation/copy/hash, oracle calculation and all five native checks are outside the timer. Cold means empty compiler caches, not empty OS page cache.',
                'cache_contract': 'Independent per-case/block/arm object+semantic or rustc incremental directory. Both ZIL arms use identical after, semantic-cache, O2, 8 requested CGUs and 2 requested jobs.',
                'sources': {}}
    save(out / 'manifest.json', manifest)
    for name in manifest['tools']:
        shutil.copy2(HERE / name, out / name)
    try:
        arms = freeze_pair(args.pair_receipt.resolve(strict=True), out)
        manifest['arms'] = arms
        if reference == 'rust':
            manifest['rustc'] = identity(args.rustc)
            manifest['rustc_version'] = subprocess.check_output([str(args.rustc.resolve()), '-vV'],
                                                                 env=clean_environment(), text=True)
        manifest['status'] = 'Running'
        save(out / 'manifest.json', manifest)
        for n, flavor in cases:
            case = f'{n}-{flavor}'
            source = out / ('source-' + case)
            source.mkdir()
            variants, oracles = {}, {}
            for edit in [0, 1, 2]:
                variant = out / ('input-' + case + '-' + str(edit))
                generate_variant(variant, n, flavor, edit)
                variants[edit], oracles[edit] = variant, expected_values(n, flavor, edit)
                manifest['sources'][case + '/' + str(edit)] = {
                    'files': {p.name: identity(p) for p in sorted(variant.iterdir())},
                    'expected_by_seed': oracles[edit],
                    'metrics': {p.name: {'bytes': p.stat().st_size, 'lines': len(p.read_text().splitlines())}
                                for p in sorted(variant.iterdir())}}
            save(out / 'manifest.json', manifest)
            for block in range(args.blocks):
                order = ['candidate', reference] if block % 2 == 0 else [reference, 'candidate']
                for position, arm in enumerate(order):
                    run = out / f'{case}-{block}-{position}-{arm}'
                    run.mkdir()
                    history_valid = True
                    for label, edit in REQUESTS:
                        for ext in ['zil', 'rs']:
                            shutil.copyfile(variants[edit] / ('main.' + ext), source / ('main.' + ext))
                        ext = 'rs' if arm == 'rust' else 'zil'
                        source_file = source / ('main.' + ext)
                        source_identity = identity(source_file)
                        if source_identity['sha256'] != manifest['sources'][case + '/' + str(edit)]['files'][source_file.name]['sha256']:
                            raise ValueError('source copy mismatch')
                        exe = run / 'program'
                        exe.unlink(missing_ok=True)
                        if arm == 'rust':
                            command = [manifest['rustc']['path'], str(source_file), '--edition=2024', '-A', 'warnings',
                                       '-C', 'opt-level=2', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                                       '-C', 'codegen-units=8', '-C', 'incremental=' + str(run / 'incremental'), '-o', str(exe)]
                        else:
                            command = [arms[arm]['launcher']['path'], 'build', str(source_file), '--opt', 'O2',
                                       '--codegen-units', '8', '--codegen-jobs', '2', '--function-merging', 'after',
                                       '--semantic-cache', str(run / 'semantics'), '--object-cache', str(run / 'objects'),
                                       '--json', '-o', str(exe)]
                            if args.closed_executable:
                                command.append('--closed-executable')
                            if args.incremental_o2:
                                command.append('--incremental-o2')
                        row = {'case': case, 'block': block, 'position': position, 'arm': arm,
                               'request': label, 'edit': edit, 'command': command, 'source': source_identity}
                        try:
                            with isolated_process_environment():
                                result = measured(command, run, label, args.timeout)
                        except BaseException:
                            interrupted = run / (label + '.interrupted.json')
                            if interrupted.is_file():
                                row.update(json.loads(interrupted.read_text()), valid=False,
                                           execution_valid=False, sequence_history_valid=history_valid)
                                rows.append(row)
                                with (out / 'rows.jsonl').open('a') as stream:
                                    stream.write(json.dumps(row) + '\n')
                            raise
                        row.update(result)
                        row['valid'] = result['exit'] == 0 and not result['timeout'] and exe.is_file()
                        if arm != 'rust':
                            try:
                                report = json.loads((run / (label + '.stdout')).read_text())
                                row['status'] = report.get('status')
                                row['verification_runs'] = report.get('verification_runs')
                                row['native_codegen'] = validate_zil_report(report, arms[arm]['pipeline_identity_tag'])
                            except (OSError, ValueError, KeyError, TypeError) as error:
                                row['valid'] = False
                                row['report_error'] = repr(error)
                        if row['valid']:
                            row['executable'] = identity(exe)
                            row['native'] = [native_check(exe, run / f'{label}-seed-{seed}',
                                                         (seed, oracles[edit][seed])) for seed in SEEDS]
                            row['valid'] = all(item['valid'] for item in row['native'])
                        row['execution_valid'] = bool(row['valid'])
                        row['sequence_history_valid'] = history_valid
                        row['valid'] = row['execution_valid'] and history_valid
                        history_valid = row['valid']
                        rows.append(row)
                        with (out / 'rows.jsonl').open('a') as stream:
                            stream.write(json.dumps(row) + '\n')
                        if not row['valid']:
                            print('INVALID', case, block, arm, label, flush=True)
                print(case, 'block', block + 1, '/', args.blocks, flush=True)
        manifest['status'] = 'Completed'
    except BaseException as error:
        manifest.update(status='FailedOrInterrupted', error=repr(error))
        raise
    finally:
        manifest['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        manifest['observed_requests'] = len(rows)
        manifest['interrupted_requests'] = sum(row.get('status') == 'Interrupted' for row in rows)
        manifest['completed_requests'] = len(rows) - manifest['interrupted_requests']
        manifest['not_run_requests'] = manifest['planned_requests'] - len(rows)
        completed = {(row['case'], row['block'], row['arm'], row['request']) for row in rows}
        not_run = [{'case': case, 'block': block, 'arm': arm, 'request': label,
                    'status': 'NotRun', 'reason': manifest.get('error', 'runner did not reach this request')}
                   for case in manifest['cases'] for block in range(args.blocks)
                   for arm in ['candidate', reference] for label, _ in REQUESTS
                   if (case, block, arm, label) not in completed]
        save(out / 'not-run.json', not_run)
        save(out / 'manifest.json', manifest)
        save(out / 'summary.json', summarize(rows, manifest['cases'], args.blocks, reference))
    print(json.dumps({'requests': len(rows), 'invalid': sum(not r['valid'] for r in rows),
                      'summary': str(out / 'summary.json')}))


if __name__ == '__main__':
    main()
