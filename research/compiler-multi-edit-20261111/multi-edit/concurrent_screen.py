#!/usr/bin/env python3
"""Draft: independent candidate batches under one fixed four-CPU budget.

prepare freezes helpers/tools and distinct candidate bytes; run requires
--execute. No compile, test or native invocation occurs during preparation.
Each cell is separate from the single-request and original weighted protocols.
"""
import argparse
import collections
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time

FORMAT = 'zil-distinct-candidate-concurrency/1'
CPUS = [0, 1, 2, 3]
ORDER = ['A', 'B', 'B', 'A']
RUST_ORDER = ['A', 'R', 'B', 'B', 'R', 'A']  # Optional, separately frozen Rust lane.


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n')


def helpers(root):
    sys.path.insert(0, str(root))
    modules = []
    for name in ['fixture', 'screen']:
        spec = importlib.util.spec_from_file_location(name, root / (name + '.py'))
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        modules.append(module)
    return modules


def vectors(n, k, functions, salt, fixture):
    # Independent numerical execution; never derives expected output from ZIL,
    # Rust, a cached artifact, or the mutated source text.
    selected = set(fixture.selected_files(n, k))
    result = []
    for seed in [0, 65, 255]:
        total = 3 * salt if 0 in selected else 0
        for i in range(1, n):
            for j in range(functions):
                delta = salt * (1 if j == 0 else 2 if j == functions - 1 else 0) if i in selected else 0
                y = seed
                for step in range(4):
                    factor = 2 + (i * 3 + j + step * 2) % 11
                    offset = 1 + (i * 7 + j * 3 + step) % 37
                    y = (y * factor + offset + (delta if step == 0 else 0)) % 251
                total += y
        result.append({'stdin_hex': bytes([seed]).hex(), 'stdout_hex': total.to_bytes(4, 'little').hex()})
    return result


def candidate(root, n, k, salt, functions, fixture):
    manifest = fixture.generate(root, n, k, functions, 'dynamic-body')
    for ext in ['zil', 'rs']:
        for i in fixture.selected_files(n, k):
            path = root / ('main.' + ext if i == 0 else f'm{i}.{ext}')
            text = path.read_text()
            if i == 0:
                for name, old in [('own_a', 1), ('own_b', 2)]:
                    before = f'fn {name}()->i32{{return {old};}}'
                    assert text.count(before) == 1
                    text = text.replace(before, f'fn {name}()->i32{{return {old * salt};}}')
            else:
                for j, delta in [(0, 1), (functions - 1, 2)]:
                    start = text.index(f'fn h{j}(')
                    end = text.find('fn ', start + 3)
                    if end < 0:
                        end = len(text)
                    body = text[start:end]
                    factor, offset = fixture.body_parameters(i, j, 0)
                    before = f'y=(y*{factor}+{offset + delta})%251;'
                    assert body.count(before) == 1
                    body = body.replace(before, f'y=(y*{factor}+{offset + delta * salt})%251;')
                    text = text[:start] + body + text[end:]
            path.write_text(text)
    manifest.update(candidate_salt=salt, native_vectors=vectors(n, k, functions, salt, fixture),
                    source_hashes={p.name: sha(p) for p in sorted(root.iterdir()) if p.suffix in ['.zil', '.rs']})
    manifest['expected_stdout_hex'] = manifest['native_vectors'][1]['stdout_hex']
    manifest['expected_u32'] = int.from_bytes(bytes.fromhex(manifest['expected_stdout_hex']), 'little')
    save(root / 'fixture.json', manifest)
    return manifest


def prepare(a):
    root = a.protocol.parent.resolve()
    if a.protocol.exists() or root.exists():
        raise ValueError('Use a new nonexistent protocol directory')
    if not set(CPUS) <= os.sched_getaffinity(0):
        raise ValueError('The four declared CPUs 0..3 must all be available')
    root.mkdir(parents=True)
    fixture, _ = helpers(a.helpers.resolve())
    fixtures = {}
    for n in a.files:
        rel = f'n{n}/base'
        fixtures[rel] = fixture.generate(root / rel, n, 0, 8, 'dynamic-body')
        for k in [16, 64]:
            identities = []
            for salt in [1, 2, 3, 4]:
                rel = f'n{n}/edit{k}/candidate{salt}'
                fixtures[rel] = candidate(root / rel, n, k, salt, 8, fixture)
                identities.append(tuple(fixtures[rel]['source_hashes'].items()))
                base = fixtures[f'n{n}/base']['source_hashes']
                for ext in ['zil', 'rs']:
                    changed = [name for name, digest in fixtures[rel]['source_hashes'].items()
                               if name.endswith('.' + ext) and digest != base[name]]
                    assert sorted(changed) == sorted(fixture.changed_names(n, k, ext))
            assert len(set(identities)) == 4, 'Candidate final bytes must differ'
    tools = {arm: {'path': str(path.resolve()), 'sha256': sha(path)}
             for arm, path in [('A', a.baseline), ('B', a.candidate), ('R', a.rustc), ('clang', a.clang)]}
    pins = {str(path.resolve()): sha(path) for path in [Path(__file__), a.helpers / 'fixture.py', a.helpers / 'screen.py']}
    plan = {'format': FORMAT, 'status': 'prepared-not-run', 'helpers': str(a.helpers.resolve()),
            'pins': pins, 'tools': tools, 'source_identities': {'A': a.baseline_source, 'B': a.candidate_source},
            'cpus': CPUS, 'files': a.files, 'edits': [16, 64], 'concurrency': [1, 2, 4],
            'blocks': a.blocks, 'order': RUST_ORDER if a.include_rust else ORDER, 'policy': 'incremental-o2-after',
            'fixed_toolchains_required': {'llvm': '23.1.4', 'rust': '1.99.1'},
            'timeout_seconds': 300, 'native_timeout_seconds': 10,
            'fixtures': fixtures, 'out': str(a.out.resolve()),
            'method': 'Each arm-position gets isolated source/output/cache for every candidate. Clean final oracle, then sequential base-cache seeds, then one simultaneous final batch; no source writes while any compiler runs.',
            'budget': 'Exactly CPUs0..3 total at every c; contiguous disjoint 4/c CPU slices; ZIL jobs=4/c and Rust inherits the identical slice; 8CGU fixed, normal Rust incremental.',
            'metrics': {'batch_wall': 'Before first launch through last wait4 completion; includes launch skew',
                        'candidate_latency': 'Both batch-start-to-reap and individual-launch-to-reap',
                        'cpu': 'Sum wait4 user+system per completed child including OS-accounted reaped descendants',
                        'rss': 'Per-child wait4 ru_maxrss: maximum individual-process RSS, never aggregate process-tree or concurrent-batch peak'},
            'validation': 'Each current ZIL seed/oracle/final Verify=1 and full per-unit preflight; final ELF/materialIR equals baseline-A clean current final oracle at the same candidate source path. Rust output checked against independent model; ELF identity recorded, not forced across clean/incremental Rust codegen.',
            'scope': 'Concurrency screen only; no original-frequency aggregation, no c4 extra-core comparison, no runtime Rust90 claim; strict functional failures stop the attempt.'}
    save(a.protocol, plan)


def command(arm, tool, source, output, jobs, cache, clang):
    exe = output / 'program'
    if arm == 'R':
        argv = [tool, str(source / 'main.rs'), '--edition=2024', '--crate-name', 'candidate_batch',
                '-A', 'warnings', '-C', 'opt-level=2', '-C', 'codegen-units=8', '-C', 'debuginfo=0',
                '-C', 'overflow-checks=yes', '-C', 'panic=abort', '-C', 'embed-bitcode=no', '-C', 'linker=' + clang, '-o', str(exe)]
        if cache:
            argv += ['-C', 'incremental=' + str(output / 'incremental')]
        return argv
    argv = [tool, 'build', str(source / 'main.zil'), '--json', '--backend', 'llvm-api', '--opt', 'O2',
            '--codegen-units', '8', '--codegen-jobs', str(jobs), '--clang', clang, '--incremental-o2', '--function-merging', 'after', '-o', str(exe)]
    if cache:
        argv += ['--object-cache', str(output / 'objects'), '--semantic-cache', str(output / 'semantic')]
    return argv


def batch(specs, log, timeout):
    """Launch every child before waiting; never invoke Popen.poll/communicate (which reap)."""
    log.mkdir(parents=True, exist_ok=False)
    children, rows, handles = {}, [], []
    error = None
    started = time.perf_counter_ns()
    def expired(*_):
        raise TimeoutError('Concurrent batch deadline')
    previous = signal.signal(signal.SIGALRM, expired)
    signal.setitimer(signal.ITIMER_REAL, timeout)
    try:
        for index, (argv, cpus) in enumerate(specs):
            stdout = (log / f'{index}.stdout').open('wb')
            stderr = (log / f'{index}.stderr').open('wb')
            handles += [stdout, stderr]
            launch = time.perf_counter_ns()
            process = subprocess.Popen(argv, stdout=stdout, stderr=stderr, start_new_session=True,
                                       preexec_fn=lambda cpus=cpus: os.sched_setaffinity(0, cpus))
            children[process.pid] = (index, process, launch, argv, cpus)
        while children:
            pid, status, usage = os.wait4(-1, 0)
            stopped = time.perf_counter_ns()
            index, process, launch, argv, cpus = children.pop(pid)
            process.returncode = os.waitstatus_to_exitcode(status)
            rows.append({'candidate': index + 1, 'argv': argv, 'cpus': cpus, 'exit': process.returncode,
                         'batch_latency_ns': stopped - started, 'launch_latency_ns': stopped - launch,
                         'launch_offset_ns': launch - started, 'cpu_ns': int((usage.ru_utime + usage.ru_stime) * 1e9),
                         'individual_process_rss_kib': usage.ru_maxrss})
        wall = time.perf_counter_ns() - started
    except BaseException as failure:
        error = repr(failure)
        raise
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)
        stopped = time.perf_counter_ns()
        reaped = {row['candidate'] for row in rows}
        unfinished = [{'candidate': index + 1, 'argv': argv, 'cpus': cpus,
                       'launched': any(child[0] == index for child in children.values())}
                      for index, (argv, cpus) in enumerate(specs) if index + 1 not in reaped]
        cleanup_rows, cleanup_errors = [], []
        for pid, (index, process, launch, argv, cpus) in children.items():
            try:
                os.killpg(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            except OSError as failure:
                cleanup_errors.append({'candidate': index + 1, 'kill_error': repr(failure)})
            try:
                _, status, usage = os.wait4(pid, 0)
                process.returncode = os.waitstatus_to_exitcode(status)
                cleanup_rows.append({'candidate': index + 1, 'argv': argv, 'cpus': cpus,
                                     'exit': process.returncode, 'forced_cleanup': True,
                                     'cpu_ns': int((usage.ru_utime + usage.ru_stime) * 1e9),
                                     'individual_process_rss_kib': usage.ru_maxrss})
            except OSError as failure:
                cleanup_errors.append({'candidate': index + 1, 'wait_error': repr(failure)})
        for handle in handles:
            try:
                handle.close()
            except OSError as failure:
                cleanup_errors.append({'close_error': repr(failure)})
        save(log / 'batch-attempt.json', {
            'complete': error is None and len(rows) == len(specs), 'error': error,
            'wall_so_far_ns': stopped - started,
            'cleanup_wall_ns': time.perf_counter_ns() - stopped,
            'reaped_rows': sorted(rows, key=lambda row: row['candidate']),
            'reaped_sum_cpu_ns': sum(row['cpu_ns'] for row in rows),
            'unfinished_at_cleanup': unfinished, 'cleanup_rows': cleanup_rows,
            'cleanup_sum_cpu_ns': sum(row['cpu_ns'] for row in cleanup_rows),
            'cleanup_errors': cleanup_errors})
    return {'wall_ns': wall, 'sum_cpu_ns': sum(row['cpu_ns'] for row in rows),
            'candidates': sorted(rows, key=lambda row: row['candidate'])}


def validate(arm, row, output, log, index, fixture, plan, helper):
    errors = []
    if row['exit']:
        errors.append('Compiler exit nonzero')
    report = None
    identity = None
    if not errors and arm != 'R':
        report = json.loads((log / f'{index}.stdout').read_text())
        more, evidence = helper.validate_zil(report, fixture['total_files_per_language'], plan)
        errors += more
        identity = helper.artifact_identity(report)
        row['verify_preflight_evidence'] = evidence
    if not errors:
        native, more = helper.native_checks(output / 'program', fixture, plan['native_timeout_seconds'])
        errors += more
        row['native'] = native
        row['artifact'] = identity or {'elf_sha256': sha(output / 'program')}
    row['errors'] = errors
    if errors:
        raise ValueError(errors)


def run(a):
    plan = json.loads(a.protocol.read_text())
    if not a.execute:
        print(json.dumps({'status': 'not-run', 'protocol_sha256': sha(a.protocol)}))
        return
    assert plan['format'] == FORMAT and plan['status'] == 'prepared-not-run'
    for path, digest in plan['pins'].items():
        assert sha(path) == digest, ('Helper/script changed after freeze', path)
    for item in plan['tools'].values():
        assert sha(item['path']) == item['sha256']
    assert set(CPUS) <= os.sched_getaffinity(0)
    os.sched_setaffinity(0, CPUS)
    root, out = a.protocol.parent, Path(plan['out'])
    fixture_module, helper = helpers(Path(plan['helpers']))
    clang = plan['tools']['clang']['path']
    for rel, fixture in plan['fixtures'].items():
        for name, digest in fixture['source_hashes'].items():
            assert sha(root / rel / name) == digest
    out.mkdir(parents=True, exist_ok=False)
    shutil.copy2(a.protocol, out / 'protocol.json')
    version = subprocess.check_output([plan['tools']['R']['path'], '-vV'], text=True)
    (out / 'rustc-version.txt').write_text(version)
    assert 'release: ' + plan['fixed_toolchains_required']['rust'] in version
    rows, complete, failure = [], False, None
    try:
        for n in plan['files']:
            for k in plan['edits']:
                for c in plan['concurrency']:
                    jobs = 4 // c
                    slices = [CPUS[i * jobs:(i + 1) * jobs] for i in range(c)]
                    for block in range(plan['blocks']):
                        for position, arm in enumerate(plan['order']):
                            cell = out / f'n{n}-k{k}-c{c}-b{block}-p{position}-{arm}'
                            cell.mkdir()
                            ext = 'rs' if arm == 'R' else 'zil'
                            tool = plan['tools'][arm]['path']
                            specs, current = [], []
                            for index in range(c):
                                item = cell / f'candidate{index + 1}'
                                item.mkdir()
                                source, output = item / 'source', item / 'build'
                                output.mkdir()
                                rel = f'n{n}/edit{k}/candidate{index + 1}'
                                final = plan['fixtures'][rel]
                                # Own candidate path makes source-dependent Rust identity explicit.
                                helper.source_snapshot(root / rel, source, ext)
                                oracle = item / 'oracle'
                                oracle.mkdir()
                                log = item / 'oracle-log'
                                oracle_arm = 'R' if arm == 'R' else 'A'
                                oracle_tool = plan['tools'][oracle_arm]['path']
                                clean = batch([(command(oracle_arm, oracle_tool, source, oracle, jobs, False, clang), slices[index])], log, plan['timeout_seconds'])
                                clean['oracle_arm'] = oracle_arm
                                clean['logical_candidate'] = index + 1
                                save(item / 'oracle-raw.json', clean)
                                validate(arm, clean['candidates'][0], oracle, log, 0, final, plan, helper)
                                save(item / 'oracle.json', clean)
                                expected = clean['candidates'][0]['artifact']
                                helper.source_snapshot(root / f'n{n}/base', source, ext)
                                log = item / 'seed-log'
                                seeded = batch([(command(arm, tool, source, output, jobs, True, clang), slices[index])], log, plan['timeout_seconds'])
                                seeded['logical_candidate'] = index + 1
                                save(item / 'seed-raw.json', seeded)
                                validate(arm, seeded['candidates'][0], output, log, 0, plan['fixtures'][f'n{n}/base'], plan, helper)
                                save(item / 'seed.json', seeded)
                                helper.source_snapshot(root / rel, source, ext, fixture_module.changed_names(n, k, ext))
                                specs.append((command(arm, tool, source, output, jobs, True, clang), slices[index]))
                                current.append((output, final, expected))
                            log = cell / 'final-log'
                            measured = batch(specs, log, plan['timeout_seconds'])
                            measured.update(files=n, edited_files=k, concurrency=c, jobs_per_candidate=jobs,
                                            block=block, position=position, arm=arm)
                            # Persist raw batch metrics before any post-timing comparison can fail.
                            save(cell / 'batch-raw.json', measured)
                            for index, row in enumerate(measured['candidates']):
                                output, final, expected = current[index]
                                validate(arm, row, output, log, index, final, plan, helper)
                                row['clean_oracle_identity'] = expected
                                row['elf_equal_to_clean'] = row['artifact']['elf_sha256'] == expected['elf_sha256']
                                if arm != 'R':
                                    assert row['elf_equal_to_clean'], 'Final ELF differs from clean current oracle'
                                    assert row['artifact']['sidecar_without_verified_fingerprint_sha256'] == expected['sidecar_without_verified_fingerprint_sha256'], 'Current material IR differs from clean oracle'
                            rows.append(measured)
                            save(cell / 'batch.json', measured)
                            with (out / 'batches.jsonl').open('a') as stream:
                                stream.write(json.dumps(measured) + '\n')
        complete = True
    except BaseException as error:
        failure = repr(error)
        raise
    finally:
        cells = collections.defaultdict(list)
        for row in rows:
            cells[row['files'], row['edited_files'], row['concurrency'], row['arm']].append(row)
        summary = [{'files': n, 'edited_files': k, 'concurrency': c, 'arm': arm,
                    'batch_wall_ns': [r['wall_ns'] for r in values],
                    'sum_cpu_ns': [r['sum_cpu_ns'] for r in values],
                    'candidate_batch_latency_ns': [[x['batch_latency_ns'] for x in r['candidates']] for r in values],
                    'individual_process_rss_kib': [[x['individual_process_rss_kib'] for x in r['candidates']] for r in values]}
                   for (n, k, c, arm), values in cells.items()]
        save(out / 'completion.json', {'complete': complete, 'failure': failure, 'batches': len(rows),
                                      'protocol_sha256': sha(a.protocol), 'scope': 'concurrency only; no weighted aggregate'})
        save(out / 'cells.json', summary)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    prep = commands.add_parser('prepare')
    prep.add_argument('--protocol', type=Path, required=True)
    prep.add_argument('--files', type=int, nargs='+', choices=[64, 256], default=[256])
    prep.add_argument('--blocks', type=int, choices=[1, 2], default=1)
    prep.add_argument('--include-rust', action='store_true', help='Predeclare additional balanced Rust positions; default first screen is ABBA only')
    prep.add_argument('--out', type=Path, required=True)
    prep.add_argument('--helpers', type=Path, default=Path(__file__).resolve().parent)
    prep.add_argument('--baseline', type=Path, required=True)
    prep.add_argument('--candidate', type=Path, required=True)
    prep.add_argument('--rustc', type=Path, required=True)
    prep.add_argument('--clang', type=Path, required=True)
    prep.add_argument('--baseline-source', required=True)
    prep.add_argument('--candidate-source', required=True)
    execute = commands.add_parser('run')
    execute.add_argument('--protocol', type=Path, required=True)
    execute.add_argument('--execute', action='store_true')
    args = parser.parse_args()
    prepare(args) if args.command == 'prepare' else run(args)


if __name__ == '__main__':
    main()
