#!/usr/bin/env python3
"""Run only the original short5000 capacity diagnostic in root's heavy window.

Required: --candidate /absolute/compiler --source FULL_SHA --out /new/directory
Optional --repo selects root's frozen helper checkout (default ZIL_FROZEN_REPO
or /workspace/zil-push-main). No Git, compiler build, budget overrides, source
edits, error injection, formal ratios, or timing superiority verdicts. Running
this script DOES invoke the supplied compilers and native programs. Preparation
of the script does not run them. Rust check reuses the hash-bound original
feasibility receipt; Rust O2 is built here with CGU16/O2/jobs1.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import select
import shutil
import signal
import subprocess
import sys
import time

sys.dont_write_bytecode = True
HOME_REPO = Path('/workspace/zil-push-main')
ORIGINAL = HOME_REPO / 'out/runs/large-project-initial-probe-02/short5000'
RUST_CHECK = HOME_REPO / 'out/runs/large-project-rust-ceiling-probe-01/short5000'
PINS = HOME_REPO / 'out/runs/large-project-research-pins-02.json'
BASE_SHA = '9170bb4210554392e36eb7528596612596ebef80'
IDENTITY = '892fbc295a4cd51abec61c1cd600079126b1d473e1a85671f506353dfe333627'
MODULES, FUNCTIONS, STEPS = 100, 50, 1
SCOPE = ('Original short5000 capacity diagnostic only. No formal ratio, speedup, '
         'memory advantage or language-wide claim. A main rejection is not '
         'candidate success. Missing observations are unknown, never zero.')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def file_sha(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def load(path):
    return json.loads(path.read_text())


def source_metadata(directory, suffix):
    paths = sorted(directory.rglob('*' + suffix))
    require(all(p.is_file() and not p.is_symlink() for p in paths), 'source symlink/non-file')
    hashes = {p.relative_to(directory).as_posix(): file_sha(p) for p in paths}
    return {'file_hashes': hashes, 'source_files': len(paths),
            'source_bytes': sum(p.stat().st_size for p in paths),
            'identity_sha256': sha(json.dumps(hashes, sort_keys=True).encode())}


def verify_zil(directory, expected):
    actual = source_metadata(directory, '.zil')
    require(actual == expected, 'full 102-file original fixture differs: ' + str(directory))
    return actual


def import_helper(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def native_runs(program, directory, env, w, rc):
    observations = []
    inputs = w.inputs('scalar')
    require(inputs == [b'', b'\x00', b'1', b'A', b'\x80', b'\xff'], 'six original scalar inputs changed')
    for repeat in range(2):
        for index, data in enumerate(inputs):
            leaf = directory / f'native-{repeat:02d}-{index:02d}'
            expected = w.oracle('scalar', data, 'cold', MODULES, FUNCTIONS, STEPS)
            result = rc.run_process([str(program)], directory, leaf, 10, 20, data, env)
            actual = {'exit': result['returncode'],
                      'stdout_hex': (leaf / 'stdout.bin').read_bytes().hex(),
                      'stderr_hex': (leaf / 'stderr.bin').read_bytes().hex()}
            row = {'repetition': repeat, 'stdin_hex': data.hex(), 'expected': expected,
                   'actual': actual, 'result': result,
                   'valid': not result['timed_out'] and actual == expected}
            observations.append(row)
            save(leaf / 'oracle.json', row)
    require(len(observations) == 12, 'native execution must be six inputs times two')
    return observations


class Resident:
    def __init__(self, command, directory, env, timeout, cwd):
        self.directory, self.timeout, self.buffer = directory, timeout, b''
        directory.mkdir(parents=True, exist_ok=False)
        save(directory / 'command.json', {'argv': command, 'cwd': str(cwd),
                                         'LD_LIBRARY_PATH': env.get('LD_LIBRARY_PATH')})
        self.stderr = (directory / 'stderr.bin').open('wb')
        self.stdout = (directory / 'stdout.jsonl').open('wb')
        self.stdin = (directory / 'stdin.bin').open('wb')
        try:
            self.process = subprocess.Popen(command, cwd=cwd, env=env,
                                            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                            stderr=self.stderr, start_new_session=True)
        except BaseException:
            self.stderr.close()
            self.stdout.close()
            self.stdin.close()
            raise

    def read(self):
        deadline = time.monotonic() + self.timeout
        while b'\n' not in self.buffer:
            left = deadline - time.monotonic()
            if left <= 0 or not select.select([self.process.stdout], [], [], max(0, left))[0]:
                raise TimeoutError('resident external request timeout; no compiler budget changed')
            chunk = os.read(self.process.stdout.fileno(), 65536)
            if not chunk:
                raise RuntimeError('resident exited before JSON line: ' + str(self.process.poll()))
            self.stdout.write(chunk)
            self.stdout.flush()
            self.buffer += chunk
        line, self.buffer = self.buffer.split(b'\n', 1)
        return json.loads(line)

    def send(self, line):
        self.stdin.write(line)
        self.stdin.flush()
        self.process.stdin.write(line)
        self.process.stdin.flush()

    def close(self):
        killed = False
        try:
            if self.process.poll() is None:
                self.send(b'quit\n')
                self.process.wait(timeout=3)
        except (OSError, subprocess.TimeoutExpired):
            killed = True
        finally:
            # Also reap helpers left in this private group after a crash/timeout.
            try:
                os.killpg(self.process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            self.process.wait()
            self.process.stdin.close()
            self.process.stdout.close()
            self.stderr.close()
            self.stdout.close()
            self.stdin.close()
            (self.directory / 'buffer-at-close.bin').write_bytes(self.buffer)
            save(self.directory / 'exit.json', {'returncode': self.process.returncode,
                                               'forced_shutdown': killed})


def zil_arm(args, arm, binary, lib, mode, project, expected, pc, rc, w, emit):
    work = args.out / arm / mode
    work.mkdir(parents=True, exist_ok=False)
    command = pc.zil_command(binary=binary, mode=mode, source=project / 'main.zil',
                             work=work, arm=arm, extras=['--push-reuse'])
    if arm == 'candidate' and mode == 'o2':
        command += ['--native-link-profile', str(args.native_link_profile)]
    # pc fixes all flags: no stop-after, compile deadline, max-* or Verify bypass.
    env = os.environ.copy()
    env['LD_LIBRARY_PATH'] = str(lib)
    worker = None
    try:
        worker = Resident(command, work / 'resident', env, args.timeout, args.repo)
        ready = worker.read()
        save(work / 'ready.json', ready)
        require(ready.get('ready') is True and ready.get('protocol') == 1, 'resident not ready')
        for index, label in enumerate(('cold', 'warm'), 1):
            verify_zil(project, expected)
            leaf = work / label
            leaf.mkdir()
            start = time.perf_counter_ns()
            worker.send(b'compile\n')
            response = worker.read()
            wall_ms = (time.perf_counter_ns() - start) / 1e6
            save(leaf / 'response.json', response)  # complete report, including every diagnostic/counter
            report = response['report']
            require(all(key in report for key in ('status', 'phase', 'diagnostics', 'verification_runs', 'source_files')),
                    'report missing required observations')
            require(isinstance(report['source_files'], dict), 'source inventory is not a map')
            expected_paths = {str(project / name) for name in expected['file_hashes']}
            source_ok = (len(report['source_files']) == 102
                         and set(report['source_files'].values()) == expected_paths)
            protocol_ok = (source_ok and response.get('request') == index
                           and isinstance(report['diagnostics'], list)
                           and type(report['verification_runs']) is int)
            current_verify = sum(event == {'phase': 'verify', 'disposition': 'executed'}
                                 for event in report.get('query_events', []))
            success = (protocol_ok and report['status'] == 'Success'
                       and report['verification_runs'] == 1 and current_verify == 1
                       and report['diagnostics'] == [])
            row = {'arm': arm, 'mode': mode, 'label': label,
                   'status': report['status'], 'phase': report['phase'],
                   'diagnostics': report['diagnostics'], 'verification_runs': report['verification_runs'],
                   'current_verify_events': current_verify, 'wall_ms': wall_ms,
                   'source_inventory_matches_all_102': source_ok, 'protocol_valid': protocol_ok,
                   'source': expected, 'response': str(leaf / 'response.json'),
                   'native': [], 'compiler_success': bool(success),
                   'capacity_success': False, 'scope': SCOPE}
            # Save the first response before artifact validation/native execution.
            save(leaf / 'summary.json', row)
            if success and mode == 'o2':
                codegen = report.get('native_codegen') or {}
                require(codegen.get('requested_units') == 16
                        and codegen.get('actual_units') == 16
                        and codegen.get('jobs') == 1,
                        'current native codegen summary differs from CGU16/jobs1')
                row['observed_codegen'] = {key: codegen[key] for key in
                                           ('requested_units', 'actual_units', 'jobs')}
                program = work / 'program'
                require(report.get('artifacts', {}).get('executable') == str(program), 'unexpected executable path')
                require(program.is_file() and not program.is_symlink(), 'missing native executable')
                archive = leaf / 'program.elf'
                shutil.copyfile(program, archive)
                row['executable'] = {'path': str(program), 'archive': str(archive),
                                     'sha256': file_sha(archive), 'bytes': archive.stat().st_size}
                row['native'] = native_runs(program, leaf, env, w, rc)
                success &= len(row['native']) == 12 and all(item['valid'] for item in row['native'])
            verify_zil(project, expected)
            row['capacity_success'] = bool(success)
            save(leaf / 'summary.json', row)
            emit(row)
    except Exception as error:
        failure = {'arm': arm, 'mode': mode, 'protocol_valid': False,
                   'capacity_success': False, 'error': repr(error),
                   'scope': 'Infrastructure/protocol failure; not a compiler capacity result.'}
        save(work / 'failure.json', failure)
        emit(failure)
    finally:
        if worker is not None:
            worker.close()


def reuse_rust_check(rust_meta, pc, emit, out):
    receipt = load(RUST_CHECK / 'summary.json')
    require(receipt['case'] == 'short5000' and receipt['arm'] == 'Rust', 'wrong Rust feasibility receipt')
    for key in rust_meta:
        require(receipt['source'][key] == rust_meta[key], 'Rust check receipt is for different full source')
    require(source_metadata(RUST_CHECK / 'project', '.rs') == rust_meta, 'historical Rust source bytes changed')
    result = load(RUST_CHECK / 'cold/result.json')
    require(receipt['result'] == result and result['returncode'] == 0 and result['timed_out'] is False,
            'Rust historical check was not successful')
    command = load(RUST_CHECK / 'cold/command.json')
    require(command == {'argv': pc.rust_command(cargo='/workspace/toolchains/cargo/bin/cargo',
                    mode='check', target=RUST_CHECK / 'target'), 'cwd': str(RUST_CHECK / 'project')},
            'Rust feasibility command differs from CGU16/check/jobs1 protocol')
    project_manifest = (RUST_CHECK / 'project/Cargo.toml').read_text()
    require('codegen-units = 16' in project_manifest, 'historical Rust CGU policy differs')
    archive = out / 'rust/check-reused'
    archive.mkdir(parents=True)
    for relative in ['summary.json', 'cold/command.json', 'cold/result.json',
                     'cold/stdout.bin', 'cold/stderr.bin', 'project/Cargo.toml', 'project/Cargo.lock']:
        path = RUST_CHECK / relative
        if relative in ('cold/stdout.bin', 'cold/stderr.bin'):
            field = 'stdout_sha256' if relative.endswith('stdout.bin') else 'stderr_sha256'
            require(file_sha(path) == result[field], 'historical Rust raw log hash differs')
        destination = archive / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, destination)
    messages = [json.loads(line) for line in (archive / 'cold/stdout.bin').read_text().splitlines() if line.strip()]
    finished = [message for message in messages if message.get('reason') == 'build-finished']
    require(len(finished) == 1 and finished[0].get('success') is True, 'Rust historical build-finished missing')
    emit({'arm': 'rust', 'mode': 'check', 'label': 'historical-cold', 'reused_receipt': True,
          'receipt': str(RUST_CHECK / 'summary.json'), 'receipt_sha256': file_sha(RUST_CHECK / 'summary.json'),
          'archive': str(archive), 'source': rust_meta, 'status': 'Success',
          'verification_runs': None, 'diagnostics': [m['message'] for m in messages if m.get('reason') == 'compiler-message'],
          'protocol_valid': True, 'capacity_success': True,
          'scope': 'Historical equivalent Rust check feasibility only; not a current timed request or independent ZIL Verify.'})


def rust_o2(args, rust_files, rust_meta, pc, rc, w, emit):
    work = args.out / 'rust/o2'
    project, target = work / 'project', work / 'target'
    rc.write_files(project, rust_files, '.rs')
    (project / 'Cargo.toml').write_text(rc.manifest_text(16))
    (project / 'Cargo.lock').write_text(rc.lock_text())
    env = os.environ.copy()
    for key in list(env):
        if key in ('CARGO_TARGET_DIR', 'CARGO_INCREMENTAL', 'LD_LIBRARY_PATH', 'RUSTFLAGS',
                   'CARGO_ENCODED_RUSTFLAGS', 'RUSTC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
                   'CARGO_BUILD_TARGET', 'CARGO_BUILD_RUSTFLAGS') or key.startswith(('CARGO_PROFILE_', 'CARGO_TARGET_')):
            env.pop(key, None)
    env['RUSTC'] = '/workspace/toolchains/cargo/bin/rustc'
    command = pc.rust_command(cargo='/workspace/toolchains/cargo/bin/cargo', mode='o2', target=target)
    for label in ('cold', 'warm'):
        leaf = work / label
        require(source_metadata(project, '.rs') == rust_meta, 'Rust full source changed')
        result = rc.run_process(command, project, leaf, args.timeout, 20, env=env)
        messages = rc.cargo_messages(leaf / 'stdout.bin')
        finished = [m for m in messages if m.get('reason') == 'build-finished']
        success = (result['returncode'] == 0 and not result['timed_out'] and len(finished) == 1
                   and finished[0].get('success') is True)
        row = {'arm': 'rust', 'mode': 'o2', 'label': label, 'source': rust_meta,
               'status': 'Success' if success else 'BuildFailure', 'verification_runs': None,
               'diagnostics': [m['message'] for m in messages if m.get('reason') == 'compiler-message'],
               'measurement': result, 'protocol_valid': len(finished) == 1,
               'native': [], 'compiler_success': bool(success),
               'capacity_success': False, 'scope': SCOPE}
        save(leaf / 'cargo-messages.json', messages)
        save(leaf / 'summary.json', row)
        if success:
            artifacts = [m for m in messages if m.get('reason') == 'compiler-artifact'
                         and m.get('target', {}).get('name') == rc.PACKAGE and m.get('executable')]
            require(len(artifacts) == 1, 'Rust executable artifact missing/ambiguous')
            program = Path(artifacts[0]['executable'])
            archive = leaf / 'program.elf'
            shutil.copyfile(program, archive)
            row['executable'] = {'path': str(program), 'archive': str(archive), 'sha256': file_sha(archive)}
            row['native'] = native_runs(program, leaf, env, w, rc)
            success &= len(row['native']) == 12 and all(item['valid'] for item in row['native'])
        require(source_metadata(project, '.rs') == rust_meta, 'Rust full source changed after build')
        row['capacity_success'] = bool(success)
        save(leaf / 'summary.json', row)
        emit(row)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', required=True, type=Path)
    parser.add_argument('--source', required=True)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--repo', type=Path, default=Path(os.environ.get('ZIL_FROZEN_REPO', str(HOME_REPO))))
    parser.add_argument('--timeout', type=float, default=180)
    parser.add_argument('--native-link-profile', type=Path,
                        default=Path('/workspace/.zil-environment/native-link-v1/manifest'))
    args = parser.parse_args()
    require(re.fullmatch('[0-9a-f]{40}', args.source) is not None, '--source must be the full lowercase source SHA')
    for path in (args.candidate, args.out, args.repo, args.native_link_profile):
        require(path.is_absolute() and '..' not in path.parts, 'paths must be absolute without parent traversal')
    require(args.timeout > 0, 'timeout must be positive')
    # Refuse existing output roots, including symlinks, rather than reuse caches.
    require(not args.out.exists() and not args.out.is_symlink(), '--out must be a new path')
    args.out.mkdir(parents=True, exist_ok=False)
    rows = []

    def emit(row):
        rows.append(row)
        with (args.out / 'rows.jsonl').open('a') as stream:
            stream.write(json.dumps(row, ensure_ascii=False) + '\n')
        save(args.out / 'summary.json', {'scope': SCOPE, 'source': args.source,
                                         'complete': False, 'rows': rows})
        print(json.dumps({k: row.get(k) for k in ('arm', 'mode', 'label', 'status',
                      'verification_runs', 'capacity_success', 'error')}), flush=True)

    try:
        fixture = load(ORIGINAL / 'fixture.json')
        expected = {key: fixture['source'][key] for key in
                    ('file_hashes', 'source_files', 'source_bytes', 'identity_sha256')}
        names = {'main.zil', 'io.zil', *(f'm{i}.zil' for i in range(MODULES))}
        require((fixture['modules'], fixture['functions_per_module'], fixture['steps']) == (100, 50, 1), 'original shape differs')
        require(set(expected['file_hashes']) == names and expected['source_files'] == 102
                and expected['source_bytes'] == 843875 and expected['identity_sha256'] == IDENTITY,
                'original full short5000 pin differs')
        verify_zil(ORIGINAL / 'sources', expected)
        pins = load(PINS)
        baseline = pins['baseline']
        require(baseline['source'] == BASE_SHA, 'baseline is not main9170')
        require(file_sha(baseline['binary']) == baseline['binary_sha256'], 'restored baseline binary differs')
        require(load(Path(baseline['build_receipt']))['source'] == BASE_SHA, 'baseline build receipt source differs')
        require(args.candidate.is_file() and os.access(args.candidate, os.X_OK), 'candidate executable missing')
        require(file_sha(args.native_link_profile) == pins['native_link_profile_sha256'], 'native link profile pin differs')
        policy = args.repo / 'crates/zil-backend-llvm/src/native_link.policy'
        require(policy.read_bytes() == args.native_link_profile.read_bytes(), 'frozen helper checkout native link policy differs')
        implementation = args.out / 'implementation'
        implementation.mkdir()
        helper_hashes = {}
        for name, relative in [('rust_compare', 'tools/research/incremental-demand/rust_compare.py'),
                               ('push_counters', 'tools/research/incremental-demand/push_counters.py'),
                               ('capacity_workloads', 'tools/research/reactive-query-graph/native_workloads.py')]:
            path = args.repo / relative
            helper_hashes[relative] = file_sha(path)
            shutil.copyfile(path, implementation / (name + '.py'))
        rc = import_helper('rust_compare', implementation / 'rust_compare.py')
        pc = import_helper('push_counters', implementation / 'push_counters.py')
        w = import_helper('capacity_workloads', implementation / 'capacity_workloads.py')
        w.ROOT = args.repo
        generated = w.snapshot('scalar', 'cold', MODULES, FUNCTIONS, STEPS)
        generated_hashes = {name: sha(text.encode()) for name, text in generated.items()}
        require(generated_hashes == expected['file_hashes'], 'frozen generator differs from original 102 hashes')
        require(sum(len(re.findall(r'\bfn\s+[A-Za-z_]\w*\s*\(', text)) for text in generated.values()) == 5105,
                'original declaration count differs')
        rust_files = rc.rust_files(w, 'cold', MODULES, FUNCTIONS, STEPS)
        equivalence = rc.validate_equivalence(generated, rust_files, MODULES, FUNCTIONS, STEPS, 'cold')
        require(equivalence == fixture['equivalence'], 'full Rust/ZIL equivalence receipt differs')
        project = args.out / 'sources'
        project.mkdir()
        for name in sorted(names):
            shutil.copyfile(ORIGINAL / 'sources' / name, project / name)
        verify_zil(project, expected)
        save(args.out / 'fixture.json', {'original': str(ORIGINAL / 'fixture.json'),
              'original_fixture_sha256': file_sha(ORIGINAL / 'fixture.json'),
              'modules': 100, 'functions_per_module': 50, 'steps': 1,
              'source': expected, 'equivalence': equivalence})
        candidate_lib = Path(pins['candidate']['llvm_library_path'])
        save(args.out / 'binding.json', {'candidate_source': args.source,
              'candidate_source_binding': 'Root-supplied full SHA; no Git lookup or inferred build attestation.',
              'candidate_binary': str(args.candidate), 'candidate_binary_sha256': file_sha(args.candidate),
              'baseline': baseline, 'repo': str(args.repo), 'helper_hashes': helper_hashes,
              'candidate_LD_LIBRARY_PATH': str(candidate_lib),
              'native_link_profile': str(args.native_link_profile),
              'native_link_profile_sha256': file_sha(args.native_link_profile),
              'compiler_limits': 'defaults; no budget or compile-timeout overrides',
              'request_sequence': ['cold', 'warm'], 'error_injection': False,
              'native_inputs': [data.hex() for data in w.inputs('scalar')],
              'native_repetitions_per_successful_o2_request': 2,
              'scope': SCOPE})
        for arm, binary, library in [('candidate', args.candidate, candidate_lib),
                                     ('baseline', Path(baseline['binary']), Path(baseline['llvm_library_path']))]:
            for mode in ('check', 'o2'):
                zil_arm(args, arm, binary, library, mode, project, expected, pc, rc, w, emit)
        rust_hashes = {name: sha(text.encode()) for name, text in sorted(rust_files.items())}
        rust_meta = {'file_hashes': rust_hashes, 'source_files': len(rust_files),
                     'source_bytes': sum(len(text.encode()) for text in rust_files.values()),
                     'identity_sha256': sha(json.dumps(rust_hashes, sort_keys=True).encode())}
        reuse_rust_check(rust_meta, pc, emit, args.out)
        rust_o2(args, rust_files, rust_meta, pc, rc, w, emit)
        verify_zil(ORIGINAL / 'sources', expected)
        verify_zil(project, expected)
        require(all(file_sha(args.repo / path) == digest for path, digest in helper_hashes.items()),
                'frozen helper checkout changed during the window')
        require(file_sha(args.native_link_profile) == pins['native_link_profile_sha256'], 'link profile changed during run')
        candidates = [r for r in rows if r['arm'] == 'candidate']
        rust = [r for r in rows if r['arm'] == 'rust']
        candidate_ok = (len(candidates) == 4 and all(r['capacity_success'] for r in candidates)
                        and sum(len(r.get('native', [])) for r in candidates) == 24)
        rust_ok = len(rust) == 3 and all(r['capacity_success'] for r in rust)
        protocol_ok = all(r.get('protocol_valid') is True for r in rows)
        success = candidate_ok and rust_ok and protocol_ok
        save(args.out / 'summary.json', {'scope': SCOPE, 'source': args.source, 'complete': True,
              'candidate_full_source_verify_native_success': candidate_ok,
              'rust_check_receipt_and_o2_success': rust_ok, 'protocol_valid': protocol_ok,
              'diagnostic_success': success, 'formal_ratio': None, 'rows': rows})
        return 0 if success else 1
    except Exception as error:
        save(args.out / 'failure.json', {'error': repr(error), 'scope': SCOPE})
        save(args.out / 'summary.json', {'scope': SCOPE, 'source': args.source, 'complete': False,
                                        'diagnostic_success': False, 'error': repr(error), 'rows': rows})
        print(repr(error), file=sys.stderr, flush=True)
        return 1


if __name__ == '__main__':
    sys.exit(main())
