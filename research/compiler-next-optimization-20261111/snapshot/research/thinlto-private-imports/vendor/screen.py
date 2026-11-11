#!/usr/bin/env python3
"""Execute a pre-frozen full-O2 multi-file screen using existing ZIL CLI paths.

This file is a prepared draft. No compilation occurs without --execute. Import
the repository's existing distribution_bench.measured for complete CLI waits.
"""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import selectors
import shutil
import signal
import statistics
import subprocess
import time
import traceback
from fixture import sha, changed_names

ACTIVE = {'phase': 'arguments', 'rows_written': 0, 'oracles_completed': 0}


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def validate_clang_pin(protocol):
    pin = protocol['clang']
    path = Path(pin['path']).resolve(strict=True)
    env_path = os.environ.get('ZIL_CLANG')
    if not env_path or Path(env_path).resolve(strict=True) != path:
        raise ValueError('current ZIL_CLANG does not resolve to the frozen clang')
    if sha(path) != pin['sha256']:
        raise ValueError('frozen clang bytes changed')
    return {'path': str(path), 'sha256': pin['sha256']}


def zil_command(binary, source, root, protocol, cache=True):
    command = [str(binary), 'build', str(source / 'main.zil'), '--json', '--backend', 'llvm-api',
               '--opt', 'O2', '--codegen-units', '8', '--codegen-jobs', '2', '-o', str(root / 'program')]
    if cache:
        command += ['--object-cache', str(root / 'objects')]
    if 'clang' in protocol:
        command += ['--clang', protocol['clang']['path']]
    if protocol['policy'].startswith('incremental-o2'):
        command += ['--incremental-o2']
    if protocol['policy'].endswith('after'):
        command += ['--function-merging', 'after']
    return command


def artifact_identity(report):
    artifact = report.get('artifacts') or {}
    executable = Path(artifact['executable'])
    llvm = Path(artifact['llvm'])
    lines = llvm.read_bytes().splitlines(keepends=True)
    fingerprint = [line for line in lines if line.startswith(b'; verified-fingerprint: ')]
    if len(fingerprint) != 1:
        raise ValueError('sidecar must contain exactly one known verified-fingerprint line')
    # Exactly this one diagnostic line is excluded. Paths, symbols, definitions,
    # attributes and every other byte remain in the comparison.
    normalized = b''.join(line for line in lines if not line.startswith(b'; verified-fingerprint: '))
    return {'elf_sha256': sha(executable), 'sidecar_raw_sha256': sha(llvm),
            'sidecar_without_verified_fingerprint_sha256': hashlib.sha256(normalized).hexdigest(),
            'excluded_lines': 1}


def native_checks(program, fixture, timeout):
    native, errors = [], []
    for vector in fixture['native_vectors']:
        result = subprocess.run([str(program)], input=bytes.fromhex(vector['stdin_hex']),
                                capture_output=True, timeout=timeout)
        native.append({'stdin_hex': vector['stdin_hex'], 'exit': result.returncode,
                       'stdout_hex': result.stdout.hex(), 'stderr_hex': result.stderr.hex()})
        if result.returncode != 0 or result.stderr or result.stdout.hex() != vector['stdout_hex']:
            errors.append('native output differs from independent arithmetic oracle')
    return native, errors


def layout_recipe_files(objects):
    """Read existing recipe artifacts outside every request's timer."""
    found = {}
    for version in ['partition-layout-v1', 'partition-layout-v2']:
        directory = objects / version
        found[version] = [{'name': str(path.relative_to(directory)), 'sha256': sha(path),
                           'bytes': path.stat().st_size}
                          for path in sorted(directory.rglob('*')) if path.is_file()] if directory.exists() else []
    return found


def frequency(cpus, policy):
    found = {}
    for cpu in cpus:
        root = Path(f'/sys/devices/system/cpu/cpu{cpu}/cpufreq')
        def read(name):
            try:
                return (root / name).read_text().strip()
            except OSError:
                return None
        item = {k: read(k) for k in ['scaling_governor', 'scaling_min_freq', 'scaling_max_freq', 'scaling_cur_freq']}
        if policy['policy'] == 'fixed':
            target = str(policy['fixed_khz'])
            if item['scaling_min_freq'] != target or item['scaling_max_freq'] != target:
                raise RuntimeError(f'CPU{cpu} has no verified fixed min=max={target} kHz: {item}')
        found[str(cpu)] = item
    return found


def cpu_ns(pid):
    # After the final ')' the first token is field 3; fields 14..17 are
    # utime, stime, cutime and cstime. Children have already been reaped by ZIL.
    fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
    return sum(int(x) for x in fields[11:15]) * 1_000_000_000 // os.sysconf('SC_CLK_TCK')


class Resident:
    def __init__(self, command, root, timeout):
        self.timeout = timeout
        self.log = (root / 'resident.stderr').open('wb')
        self.stdout_log = (root / 'resident.stdout.jsonl').open('ab')
        start = time.perf_counter_ns()
        self.process = subprocess.Popen(command + ['--resident', '--resident-capacity', '256',
                                                  '--resident-reset-after', '64'],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.log, start_new_session=True, bufsize=0)
        self.pending = bytearray()
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        try:
            self.ready = self.line()
            if self.ready.get('ready') is not True or self.ready.get('protocol') != 1:
                raise RuntimeError(f'not the resident protocol: {self.ready}')
        except BaseException:
            self.close()
            raise
        self.startup_ns = time.perf_counter_ns() - start
        write(root / 'resident-ready.json', {'command': command, 'startup_ns': self.startup_ns, 'ready': self.ready})

    def line(self):
        deadline = time.monotonic() + self.timeout
        while b'\n' not in self.pending:
            left = deadline - time.monotonic()
            if left <= 0 or not self.selector.select(max(0, left)):
                raise TimeoutError('resident response deadline')
            data = os.read(self.process.stdout.fileno(), 65536)
            if not data:
                raise RuntimeError(f'resident EOF, exit={self.process.poll()}')
            self.pending.extend(data)
            if len(self.pending) > 128 * 1024 * 1024:
                raise RuntimeError('resident response exceeds 128 MiB')
        line, _, tail = self.pending.partition(b'\n')
        self.pending = bytearray(tail)
        self.stdout_log.write(line + b'\n')
        self.stdout_log.flush()
        return json.loads(line)

    def compile(self, root, label):
        before_cpu = cpu_ns(self.process.pid)
        start = time.perf_counter_ns()
        self.process.stdin.write(b'compile\n')
        self.process.stdin.flush()
        envelope = self.line()
        wall = time.perf_counter_ns() - start
        after_cpu = cpu_ns(self.process.pid)
        write(root / f'{label}.resident.json', envelope)
        report = envelope.get('report', {})
        write(root / f'{label}.stdout.json', report)
        result = {'elapsed_ns': wall, 'cpu_seconds': (after_cpu - before_cpu) / 1e9,
                  'exit': 0, 'timeout': False, 'resident_internal_elapsed_ms': envelope.get('elapsed_ms'),
                  'rss_kib': envelope.get('rss_kib'), 'lifetime_peak_rss_kib': envelope.get('peak_kib'),
                  'active_queries': envelope.get('active_queries'), 'session_epoch': envelope.get('session_epoch')}
        ACTIVE.update(phase='resident-envelope-validation', last_metrics=result)
        if envelope.get('session_cleared') or envelope.get('active_queries') != 0:
            raise RuntimeError(f'unexpected resident reset/active query: {envelope}')
        return result, report

    def close(self):
        try:
            if self.process.poll() is None:
                self.process.stdin.write(b'quit\n')
                self.process.stdin.flush()
                self.process.wait(timeout=5)
        finally:
            # This process group was created by this runner; never signal outside it.
            try:
                os.killpg(self.process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            self.process.wait()
            self.selector.close()
            self.log.close()
            self.stdout_log.close()


def validate_zil(report, n, protocol):
    errors = []
    if report.get('status') != 'Success' or report.get('verification_runs') != 1:
        errors.append('current independent Verify did not execute exactly once successfully')
    if len(report.get('source_files', {})) != n:
        errors.append('actual source file count differs from frozen N')
    native = report.get('native_codegen') or {}
    receipts = native.get('object_reports') or []
    if native.get('backend') != 'llvm-api' or len(receipts) != native.get('actual_units') or not receipts:
        errors.append('missing actual LLVM API unit receipts')
    wanted_pipeline = ('default<O2>' if not protocol['policy'].endswith('after')
                       else 'default<O2>|merge-functions=after|aliases=false')
    checked = []
    for receipt in receipts:
        item = {'input_kind': receipt.get('input_kind'), 'pipeline': receipt.get('pipeline'),
                'llvm_version': receipt.get('llvm_version'), 'preflight_ns': receipt.get('preflight_ns')}
        if receipt.get('pipeline') != wanted_pipeline:
            errors.append('O2 pipeline identity mismatch')
        if str(receipt.get('llvm_version', '')).split()[0] != protocol['fixed_toolchains_required']['llvm']:
            errors.append('LLVM version differs from required toolchain')
        if not isinstance(receipt.get('preflight_ns'), int) or receipt['preflight_ns'] <= 0:
            errors.append('current full machine preflight receipt absent')
        if receipt.get('input_kind') == 'object-cache':
            if receipt.get('producer_verified_before_after') is not True:
                errors.append('cache producer verification evidence absent')
        elif receipt.get('verify_before_after') is not True:
            errors.append('LLVM before/after verification absent')
        if receipt.get('input_kind') == 'object-patch' and receipt.get('current_full_machine_preflight') is not True:
            errors.append('object patch skipped current full machine preflight')
        checked.append(item)
    return errors, {'verification_runs': report.get('verification_runs'), 'units': checked,
                    'actual_units': native.get('actual_units'), 'object_cache': native.get('object_cache')}


def source_snapshot(source, destination, ext, changed=None):
    destination.mkdir(parents=True, exist_ok=True)
    names = changed if changed is not None else [p.name for p in source.glob('*.' + ext)]
    # All writes/renames end before the single compile request. One reader only;
    # this is a batch publication boundary, not an atomic multi-file filesystem API.
    for name in names:
        temporary = destination / (name + '.pending')
        temporary.write_bytes((source / name).read_bytes())
        temporary.replace(destination / name)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--protocol', type=Path, required=True)
    p.add_argument('--baseline', type=Path, required=True)
    p.add_argument('--candidate', type=Path, required=True)
    p.add_argument('--baseline-source', required=True)
    p.add_argument('--candidate-source', required=True)
    p.add_argument('--rustc', type=Path)
    p.add_argument('--runtime-summary', type=Path,
                   help='mandatory successful exact-source/runtime-binary receipt before --execute')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--execute', action='store_true')
    a = p.parse_args()
    protocol = json.loads(a.protocol.read_text())
    if not a.execute:
        print(json.dumps({'status': 'not-run', 'protocol_sha256': sha(a.protocol),
                          'note': 'add --execute only after checking protocol/tool identities and exclusive resource window'}, indent=2))
        return
    if protocol['status'] != 'prepared-not-run' or protocol['format'] != 'zil-bounded-imports-multi-file-screen/1':
        raise ValueError('unsupported or unfrozen protocol')
    if not a.runtime_summary:
        raise ValueError('matching completed runtime quality gate is required before compile screen execution')
    runtime = json.loads(a.runtime_summary.read_text())
    if runtime.get('format') != 'zil-bounded-imports-runtime-result/1' or runtime.get('gate_pass') is not True \
            or runtime.get('all_native_outputs_valid') is not True or len(runtime.get('cases', [])) != 8 \
            or not all(case.get('gate_pass') is True for case in runtime['cases']) \
            or runtime.get('counts') != {'clean_compilations': 24, 'native_total': 216, 'native_formal': 144}:
        raise ValueError('runtime gate is failed, incomplete or from another protocol')
    for name, digest in protocol['tools'].items():
        if sha(Path(__file__).with_name(name)) != digest:
            raise ValueError(f'runner changed since preparation: {name}; freeze a new protocol')
    reference = protocol['reference_protocol']
    if sha(Path(reference['path'])) != reference['sha256']:
        raise ValueError('original v2 reference protocol changed')
    repo = Path(protocol['repo'])
    measured_path = repo / 'tools/research/incremental-compilation/distribution_bench.py'
    if sha(measured_path) != protocol['existing_measurer_sha256']:
        raise ValueError('existing CLI timing implementation changed')
    spec = importlib.util.spec_from_file_location('zil_existing_distribution', measured_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    measured = module.measured
    a.out = a.out.resolve()
    a.out.mkdir(parents=True, exist_ok=False)
    ACTIVE.update(out=str(a.out), phase='preflight')
    cpus = protocol['cpus']
    if not set(cpus).issubset(os.sched_getaffinity(0)):
        raise ValueError('declared CPU affinity unavailable')
    os.sched_setaffinity(0, cpus)
    compilers = {'A': a.baseline.resolve(strict=True), 'B': a.candidate.resolve(strict=True)}
    if a.rustc:
        compilers['R'] = a.rustc.resolve(strict=True)
    identities = {arm: {'path': str(path), 'sha256': sha(path)} for arm, path in compilers.items()}
    for arm in ['A', 'B']:
        if runtime['binaries'][arm]['sha256'] != identities[arm]['sha256']:
            raise ValueError('runtime receipt compiler binary differs from compile screen')
    if runtime['source_claims'] != {'A': a.baseline_source, 'B': a.candidate_source}:
        raise ValueError('runtime receipt source identities differ from compile screen')
    if runtime['reference_protocol_sha256'] != protocol['reference_protocol']['sha256']:
        raise ValueError('runtime receipt belongs to a different source corpus')
    clang = validate_clang_pin(protocol)
    if runtime['clang'] != clang:
        raise ValueError('runtime receipt clang differs from compile screen')
    if 'R' in identities and runtime['binaries']['R']['sha256'] != identities['R']['sha256']:
        raise ValueError('runtime Rust binary differs from compile screen Rust')
    if a.rustc:
        version = subprocess.check_output([str(compilers['R']), '-vV'], text=True)
        (a.out / 'rustc-version.txt').write_text(version)
        if f"release: {protocol['fixed_toolchains_required']['rust']}" not in version:
            raise ValueError('Rust version differs from required toolchain')
    shutil.copy2(a.protocol, a.out / 'protocol.json')
    shutil.copy2(a.runtime_summary, a.out / 'runtime-quality-receipt.json')
    initial_frequency = frequency(cpus, protocol['frequency'])
    write(a.out / 'execution-manifest.json', {'status': 'running', 'protocol_sha256': sha(a.protocol),
          'binaries': identities, 'source_claims': {'A': a.baseline_source, 'B': a.candidate_source},
          'cpus': cpus, 'frequency_at_start': initial_frequency, 'runner_sha256': sha(__file__),
          'environment': {k: v for k, v in os.environ.items() if k in ['ZIL_CLANG', 'LLVM_CONFIG', 'RUSTFLAGS']},
          'runtime_quality_receipt_sha256': sha(a.runtime_summary),
          'clang': clang,
          'binary_source_binding': 'source identities are explicit claims; attach build receipts for verified provenance'})
    for rel, fixture in protocol['fixtures'].items():
        ACTIVE.update(phase='frozen-source-validation', snapshot=rel, arm='A', cell=None)
        for name, digest in fixture['source_hashes'].items():
            if sha(a.protocol.parent / rel / name) != digest:
                raise ValueError(f'frozen fixture changed: {rel}/{name}')
    # Different machine code is an intended possible result of bounded imports.
    # Each arm therefore proves incremental transparency against its OWN clean
    # full-O2 oracle. Cross-arm byte differences are classified, never hidden.
    oracles = {'A': {}, 'B': {}}
    for arm, rel, fixture in [(arm, rel, fixture) for arm in ['A', 'B']
                              for rel, fixture in protocol['fixtures'].items()]:
        ACTIVE.update(phase='clean-oracle-setup', snapshot=rel, arm=arm, cell=None, last_metrics=None)
        root = a.out / 'clean-oracles' / arm / rel
        ACTIVE['attempt'] = str(root)
        root.mkdir(parents=True)
        source = root / 'source'
        source_snapshot(a.protocol.parent / rel, source, 'zil')
        command = zil_command(compilers[arm], source, root, protocol, cache=False)
        before_frequency = frequency(cpus, protocol['frequency'])
        ACTIVE['phase'] = 'clean-oracle-compile'
        metrics = measured(command, root, 'oracle', protocol['timeout_seconds'])
        ACTIVE.update(phase='clean-oracle-validation', attempt=str(root), last_metrics=metrics)
        report = json.loads((root / 'oracle.stdout').read_text()) if metrics['exit'] == 0 else {}
        errors, evidence = validate_zil(report, fixture['total_files_per_language'], protocol)
        if metrics['exit'] != 0 or metrics['timeout']:
            errors.append('uncached oracle build failed')
        native, identity = [], None
        if not errors:
            ACTIVE['phase'] = 'clean-oracle-native-and-artifact'
            native, extra = native_checks(root / 'program', fixture, protocol['native_timeout_seconds'])
            errors.extend(extra)
            identity = artifact_identity(report)
        write(root / 'oracle.json', {'command': command, **metrics, 'errors': errors,
              'valid': not errors, 'native': native, 'identity': identity,
              'verify_preflight_evidence': evidence, 'frequency_before': before_frequency,
              'performance_sample': False, 'object_cache_requested': False})
        if errors:
            raise RuntimeError(f'clean O2 oracle failed: {rel}: {errors}')
        oracles[arm][rel] = identity
        ACTIVE['oracles_completed'] += 1
    write(a.out / 'clean-oracle-identities.json', oracles)
    cross_arm = {rel: {key: oracles['A'][rel][key] == oracles['B'][rel][key]
                      for key in ['elf_sha256', 'sidecar_without_verified_fingerprint_sha256']}
                 for rel in protocol['fixtures']}
    write(a.out / 'cross-arm-clean-identity.json', {'classification': cross_arm,
          'identity_difference_allowed': True,
          'required': 'every arm matches its own clean artifact; every independent native oracle passes; runtime quality gate is separate and mandatory'})
    rows = []
    for n in protocol['files']:
        for mutation, k in [*(('edit-' + str(k), k) for k in protocol['edits']),
                            *(('control-' + kind, 1) for kind in protocol['controls'])]:
            for state in protocol['states']:
                for block, order in enumerate(protocol['order']):
                    arms = list(order)
                    if a.rustc and state != 'resident-cli':
                        arms = ['R', *arms] if block % 2 == 0 else [*arms, 'R']
                    for position, arm in enumerate(arms):
                        ACTIVE.update(phase='cell-setup', snapshot=f'n{n}/{mutation}', arm=arm,
                                      cell={'files': n, 'mutation': mutation, 'state': state,
                                            'block': block, 'position': position}, last_metrics=None)
                        root = a.out / f'n{n}-{mutation}-{state}-b{block}-p{position}-{arm}'
                        ACTIVE['attempt'] = str(root)
                        root.mkdir()
                        source = root / 'source'
                        ext = 'rs' if arm == 'R' else 'zil'
                        base = a.protocol.parent / f'n{n}' / 'base'
                        edited = a.protocol.parent / f'n{n}' / mutation
                        source_snapshot(base, source, ext)
                        program = root / 'program'
                        if arm == 'R':
                            command = [str(compilers[arm]), str(source / 'main.rs'), '--edition=2024', '-A', 'dead_code',
                                       '-C', 'opt-level=2', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                                       '-C', 'codegen-units=8', '-C', f'linker={clang["path"]}',
                                       '-C', f'incremental={root / "incremental"}', '-o', str(program)]
                        else:
                            command = zil_command(compilers[arm], source, root, protocol)
                        resident = None
                        try:
                            if state == 'resident-cli':
                                ACTIVE['phase'] = 'resident-startup'
                                resident = Resident(command, root, protocol['timeout_seconds'])
                            labels = ['edit'] if state == 'cold-cli' else ['seed', 'edit']
                            for label in labels:
                                ACTIVE.update(phase='snapshot-publication', request=label)
                                if label == 'edit':
                                    source_snapshot(edited, source, ext, changed_names(n, k, ext))
                                program.unlink(missing_ok=True)
                                recipes_before = layout_recipe_files(root / 'objects') if arm != 'R' else None
                                before_frequency = frequency(cpus, protocol['frequency'])
                                ACTIVE['phase'] = 'request-compile'
                                if resident:
                                    metrics, report = resident.compile(root, label)
                                else:
                                    metrics = measured(command, root, label, protocol['timeout_seconds'])
                                    ACTIVE['last_metrics'] = metrics
                                    ACTIVE['phase'] = 'request-report-decode'
                                    report = json.loads((root / f'{label}.stdout').read_text()) if arm != 'R' and metrics['exit'] == 0 else {}
                                ACTIVE.update(phase='request-validation', last_metrics=metrics)
                                recipes_after = layout_recipe_files(root / 'objects') if arm != 'R' else None
                                errors = []
                                evidence = None
                                if metrics['exit'] != 0 or metrics['timeout']:
                                    errors.append('compiler failed or timed out')
                                if arm != 'R':
                                    more, evidence = validate_zil(report, n, protocol)
                                    errors.extend(more)
                                fixture = protocol['fixtures'][f'n{n}/base' if label == 'seed' else f'n{n}/{mutation}']
                                native = []
                                artifact = None
                                if not errors:
                                    ACTIVE['phase'] = 'request-native-and-artifact'
                                    native, extra = native_checks(program, fixture, protocol['native_timeout_seconds'])
                                    errors.extend(extra)
                                    if arm != 'R':
                                        artifact = artifact_identity(report)
                                        oracle_key = f'n{n}/base' if label == 'seed' else f'n{n}/{mutation}'
                                        reference = oracles[arm][oracle_key]
                                        for key in ['elf_sha256', 'sidecar_without_verified_fingerprint_sha256']:
                                            if artifact[key] != reference[key]:
                                                errors.append(f'{key} differs from SAME ARM clean no-cache O2 oracle; native output cannot override this failure')
                                hashes = {x.name: sha(x) for x in sorted(source.glob('*.' + ext))}
                                row = {'files': n, 'edited_files': k, 'mutation': mutation,
                                       'edit_sites_per_file': 2 if mutation.startswith('edit-') else None, 'state': state,
                                       'block': block, 'position': position, 'arm': arm, 'request': label,
                                       'measured': label == 'edit', 'command': command, **metrics,
                                       'frequency_before': before_frequency, 'source_hashes': hashes,
                                       'source_identity_sha256': __import__('hashlib').sha256(json.dumps(hashes, sort_keys=True).encode()).hexdigest(),
                                       'native': native, 'verify_preflight_evidence': evidence,
                                       'artifact_identity': artifact,
                                       'cross_arm_clean_identity': cross_arm[f'n{n}/base' if label == 'seed' else f'n{n}/{mutation}'] if arm != 'R' else None,
                                       'layout_recipes_before': recipes_before, 'layout_recipes_after': recipes_after,
                                       'valid': not errors, 'errors': errors}
                                write(root / f'{label}.row.json', row)
                                with (a.out / 'rows.jsonl').open('a') as stream:
                                    stream.write(json.dumps(row) + '\n')
                                rows.append(row)
                                ACTIVE['rows_written'] = len(rows)
                                if errors:
                                    raise RuntimeError(f'screen failed at {root.name}/{label}: {errors}')
                        finally:
                            if resident:
                                resident.close()
    summary = []
    ACTIVE['phase'] = 'summary'
    for n in protocol['files']:
        for mutation, k in [*(('edit-' + str(k), k) for k in protocol['edits']),
                            *(('control-' + kind, 1) for kind in protocol['controls'])]:
            for state in protocol['states']:
                cells = {}
                for arm in compilers:
                    selected = [r for r in rows if r['measured'] and (r['files'], r['mutation'], r['state'], r['arm']) == (n, mutation, state, arm)]
                    if not selected:
                        continue
                    means = [statistics.mean(r['elapsed_ns'] for r in selected if r['block'] == b) for b in range(protocol['blocks'])]
                    walls = sorted(r['elapsed_ns'] / 1e6 for r in selected)
                    cells[arm] = {'median_block_mean_ms': statistics.median(means) / 1e6,
                                  'arithmetic_mean_wall_ms': statistics.mean(r['elapsed_ns'] for r in selected) / 1e6,
                                  'observed_p95_ms': walls[max(0, math.ceil(.95 * len(walls)) - 1)],
                                  'samples': len(selected), 'median_cpu_seconds': statistics.median(r['cpu_seconds'] for r in selected),
                                  'rss_fields': 'see rows; CLI ru_maxrss and resident current/lifetime peak have different scopes'}
                summary.append({'files': n, 'edited_files': k, 'mutation': mutation, 'state': state, 'arms': cells,
                                'B_over_A_wall': cells['B']['median_block_mean_ms'] / cells['A']['median_block_mean_ms']})
    weighted = {}
    for arm in ['A', 'B']:
        weighted[arm] = sum(protocol['file_count_weights'][str(cell['files'])]
                            * protocol['edit_weights'][str(cell['edited_files'])]
                            * protocol['scenario_weights'][cell['state']]
                            * cell['arms'][arm]['arithmetic_mean_wall_ms']
                            for cell in summary if cell['mutation'].startswith('edit-'))
    write(a.out / 'summary.json', {'status': 'screen-only', 'valid': all(r['valid'] for r in rows),
          'total_compilations_including_seeds': len(rows), 'cells': summary,
          'fixed_frequency_weighted_mean_wall_ms': weighted,
          'weighted_B_over_A': weighted['B'] / weighted['A'],
          'weight_meaning': 'predeclared request occurrence frequencies, absolute wall means weighted before ratios; structural controls excluded',
          'cross_arm_clean_identity': cross_arm,
          'runtime_performance_gate': 'not-executed-by-this-screen', 'parallel_candidate_scenario': 'not-executed'})
    manifest = json.loads((a.out / 'execution-manifest.json').read_text())
    manifest['status'] = 'screen-complete'
    write(a.out / 'execution-manifest.json', manifest)
    print(json.dumps({'status': 'screen-complete', 'rows': len(rows), 'out': str(a.out)}, indent=2))
    ACTIVE['phase'] = 'complete'


if __name__ == '__main__':
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        if ACTIVE.get('out'):
            out = Path(ACTIVE['out'])
            failure = {**ACTIVE, 'status': 'Incomplete' if isinstance(error, KeyboardInterrupt) else 'Failed',
                       'valid': False, 'time_utc': __import__('datetime').datetime.now(__import__('datetime').timezone.utc).isoformat(),
                       'exception': {'type': type(error).__name__, 'message': str(error)},
                       'traceback': traceback.format_exc(),
                       'remaining_requests': 'not-executed; never counted as Pass',
                       'summary': 'no successful aggregate or partial-result performance ratio is emitted after fatal failure'}
            write(out / 'fatal-failure.json', failure)
            manifest_path = out / 'execution-manifest.json'
            manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else {}
            manifest.update(status=failure['status'], valid=False, failure_receipt='fatal-failure.json',
                            rows_written=ACTIVE['rows_written'], oracles_completed=ACTIVE['oracles_completed'])
            write(manifest_path, manifest)
        raise
