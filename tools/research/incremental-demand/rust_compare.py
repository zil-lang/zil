#!/usr/bin/env python3
"""Reproduce equivalent scalar Rust requests in an exclusive build window.

Default execution only generates and validates fixtures. --run explicitly runs
Cargo and native programs. Cargo request wall time includes process startup;
these observations must not be labelled paired with separately timed resident
Zil blocks. No compiler performance or language-wide acceptance is inferred.

The measured v1 runner is archived separately with its original raw artifacts:
SHA256 a1b2df047a31aebfc59ba954964e28ef33ea44e246dcbf82b9a7879065389e9d.
This source reproducer preserves its generators, flags, request history, timing,
diagnostics and native oracles, but explicitly samples only the Cargo/root PID.
The measurement environment did not expose /proc/<pid>/task/<pid>/children, so
v1's misleading tree_* RSS fields observed only that root PID. Preserve those
raw fields without relabelling measured files. New RSS fields are cargo_rss_kib
and cargo_peak_sampled_rss_kib; they exclude rustc/linker children. wait4 maxrss
still describes the largest-process lifetime peak. No Rust/Zil memory advantage
is established by either format. For tool/native commands, the same sampler
observes their direct root process rather than Cargo.
"""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import signal
import shutil
import statistics
import subprocess
import sys
import threading
import time


LABELS = ['cold', 'unchanged', 'private', 'multi', 'broad', 'restore', 'error', 'recovery']
PACKAGE = 'zil_scalar_equivalent'


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load_workloads(repo, path=None):
    path = path or repo / 'tools/research/reactive-query-graph/native_workloads.py'
    spec = importlib.util.spec_from_file_location('comparison_workloads', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.ROOT = repo
    return module


def rust_files(workloads, label, modules, functions, steps):
    files = {'src/io.rs': '''unsafe extern "C" {
    fn getchar() -> i32;
    fn putchar(value: i32) -> i32;
}

pub fn read() -> i32 {
    unsafe { getchar() }
}

pub fn emit(value: i32) {
    unsafe { putchar(value); }
}
'''}
    for i in range(modules):
        code = []
        for j in range(functions):
            change = workloads.delta(label, i, j, modules, functions)
            code.append(f'fn h{j}(x: i32) -> i32 {{\n    let mut y: i32 = x;\n')
            for k in range(steps):
                a = (i + j + k) % 13 + 1 + change
                b = (j * 3 + k) % 11 + 2
                code.append(f'    if y % 3 == 0 {{ y = (y * 2 + {a}) % 251; }} else {{ y = (y + {b}) % 251; }}\n')
            code.append('    return absent_name;\n}\n' if label == 'error' and i == j == 0 else '    return y;\n}\n')
        code.append('pub fn value(x: i32) -> i32 {\n    let mut total: i32 = x;\n')
        code.extend(f'    total = (total + h{j}(x)) % 65521;\n' for j in range(functions))
        code.append('    return total;\n}\n')
        files[f'src/m{i}.rs'] = ''.join(code)
    main = ['mod io;\n', *(f'mod m{i};\n' for i in range(modules))]
    main.append('fn main() -> std::process::ExitCode {\n    let mut x: i32 = io::read();\n    if x < 0 { x = 0; }\n    x = x % 251;\n    let mut total: i32 = 0;\n')
    main.extend(f'    total = (total + m{i}::value(x)) % 65521;\n' for i in range(modules))
    main.extend(f'    io::emit((total >> {shift}) & 255);\n' for shift in [0, 8, 16, 24])
    main.append('    std::process::ExitCode::SUCCESS\n}\n')
    files['src/main.rs'] = ''.join(main)
    return files


def manifest_text(codegen_units=1):
    return f'''[package]
name = "{PACKAGE}"
version = "0.1.0"
edition = "2024"

[workspace]

[profile.dev]
incremental = true
codegen-units = {codegen_units}

[profile.release]
opt-level = 2
codegen-units = {codegen_units}
incremental = true
lto = false
'''


def lock_text():
    return f'''# No third-party dependencies; standard library is toolchain-provided.
version = 4

[[package]]
name = "{PACKAGE}"
version = "0.1.0"
'''


def write_files(directory, files, suffix):
    directory.mkdir(parents=True, exist_ok=True)
    changed = []
    for name, text in sorted(files.items()):
        path, data = directory / name, text.encode()
        if not path.exists() or path.read_bytes() != data:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            changed.append(name)
    for path in directory.rglob(f'*{suffix}'):
        name = path.relative_to(directory).as_posix()
        if name not in files:
            path.unlink()
            changed.append(name)
    hashes = {name: sha(text.encode()) for name, text in sorted(files.items())}
    return {'changed_files': sorted(changed), 'file_hashes': hashes,
            'identity_sha256': sha(json.dumps(hashes, sort_keys=True).encode()),
            'source_files': len(files), 'source_bytes': sum(len(s.encode()) for s in files.values())}


def validate_equivalence(zil, rust, modules, functions, steps, label):
    pattern = r'if y % 3 == 0 \{ y = \(y \* 2 \+ (\d+)\) % 251;? \} else \{ y = \(y \+ (\d+)\) % 251;? \}'
    pairs = []
    for i in range(modules):
        z, r = zil[f'm{i}.zil'], rust[f'src/m{i}.rs']
        left, right = re.findall(pattern, z), re.findall(pattern, r)
        if left != right or len(left) != functions * steps:
            raise ValueError(f'{label}: arithmetic source mismatch in module {i}')
        if re.findall(r'h(\d+)\(x\)\) % 65521', z) != re.findall(r'h(\d+)\(x\)\) % 65521', r):
            raise ValueError(f'{label}: helper call graph differs')
        if [int(n) for n in re.findall(r'fn h(\d+)\(', z)] != list(range(functions)):
            raise ValueError('unexpected Zil helper inventory')
        pairs.extend(left)
    zmain = re.findall(r'm(\d+)\.value\(x\)', zil['main.zil'])
    rmain = re.findall(r'm(\d+)::value\(x\)', rust['src/main.rs'])
    if zmain != rmain or [int(n) for n in zmain] != list(range(modules)):
        raise ValueError(f'{label}: module call graph differs')
    if len(zil) != modules + 2 or len(rust) != modules + 2:
        raise ValueError('source file inventory mismatch')
    if ('absent_name' in ''.join(rust.values())) != (label == 'error'):
        raise ValueError('invalid error fixture')
    return {'arithmetic_step_count': len(pairs), 'coefficient_sha256': sha(json.dumps(pairs).encode()),
            'named_declarations': modules * (functions + 1) + 5,
            'function_definitions': modules * (functions + 1) + 3,
            'extern_c_declarations': 2, 'source_files_each_language': modules + 2,
            'bounded_arithmetic': '0<=x,y<=250; y*2+a<=515; total+helper<=65770; total+module_value<=131040; i32 cannot overflow',
            'io': 'Same libc getchar/putchar; one input byte or EOF; four raw output bytes; no println, allocation, formatting, or Rust buffering shim'}


def memory_sample(pid):
    """Sample the direct root PID only; this is Cargo for compile requests."""
    try:
        fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
        return int(fields[21]) * os.sysconf('SC_PAGE_SIZE') / 1024
    except (OSError, ValueError, IndexError):
        return None


def run_process(command, cwd, directory, timeout, sample_ms, stdin=b'', env=None):
    """wait4 includes reaped child CPU; files avoid pipe deadlocks and log loss."""
    directory.mkdir(parents=True, exist_ok=False)
    (directory / 'stdin.bin').write_bytes(stdin)
    save(directory / 'command.json', {'argv': command, 'cwd': str(cwd)})
    samples, timed_out = [], [False]
    with (directory / 'stdin.bin').open('rb') as inp, (directory / 'stdout.bin').open('wb') as out, (directory / 'stderr.bin').open('wb') as err:
        start = time.perf_counter_ns()
        process = subprocess.Popen(command, cwd=cwd, stdin=inp, stdout=out, stderr=err,
                                   start_new_session=True, env=env)
        stop = threading.Event()

        def sample():
            while not stop.is_set():
                sampled = memory_sample(process.pid)
                samples.append({'offset_ms': (time.perf_counter_ns() - start) / 1e6,
                                'cargo_pid': process.pid, 'cargo_rss_kib': sampled})
                stop.wait(sample_ms / 1000)

        def terminate():
            try:
                os.killpg(process.pid, signal.SIGKILL)
                timed_out[0] = True
            except ProcessLookupError:
                pass

        sampler = threading.Thread(target=sample, daemon=True)
        timer = threading.Timer(timeout, terminate)
        timer.daemon = True
        sampler.start()
        timer.start()
        try:
            _, status, usage = os.wait4(process.pid, 0)
            wall_ms = (time.perf_counter_ns() - start) / 1e6
            process.returncode = os.waitstatus_to_exitcode(status)
        except BaseException:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            try:
                _, status, _ = os.wait4(process.pid, 0)
                process.returncode = os.waitstatus_to_exitcode(status)
            except ChildProcessError:
                pass
            raise
        finally:
            timer.cancel()
            stop.set()
            sampler.join()
            timer.join()
    result = {'returncode': process.returncode, 'timed_out': timed_out[0], 'wall_ms': wall_ms,
              'tree_cpu_seconds_wait4': usage.ru_utime + usage.ru_stime,
              'tree_user_seconds_wait4': usage.ru_utime, 'tree_system_seconds_wait4': usage.ru_stime,
              'wait4_maxrss_kib': usage.ru_maxrss,
              'cargo_peak_sampled_rss_kib': max((s['cargo_rss_kib'] for s in samples if s['cargo_rss_kib'] is not None), default=0),
              'memory_sample_count': len(samples),
              'stdout_sha256': sha((directory / 'stdout.bin').read_bytes()),
              'stderr_sha256': sha((directory / 'stderr.bin').read_bytes())}
    save(directory / 'memory-samples.json', samples)
    save(directory / 'result.json', result)
    return result


def parent(label):
    return None if label == 'cold' else 'error' if label == 'recovery' else 'private' if label == 'restore' else 'cold'


def bind_tool(command, name, args, env):
    path = shutil.which(command, path=env.get('PATH'))
    if not path:
        raise RuntimeError(f'{name} executable not found: {command}')
    launcher = Path(path).resolve()
    actual = launcher
    rustup_path = shutil.which('rustup', path=env.get('PATH'))
    rustup = Path(rustup_path).resolve() if rustup_path else launcher.with_name('rustup')
    proxy = launcher.name == 'rustup' or (rustup.is_file() and os.path.samefile(launcher, rustup))
    if proxy:
        evidence = args.out / f'{name}-resolution'
        result = run_process([str(rustup), 'which', name], args.repo, evidence, 30, args.sample_ms, env=env)
        if result['returncode'] or result['timed_out']:
            raise RuntimeError(f'could not resolve {name} rustup proxy')
        actual = Path((evidence / 'stdout.bin').read_text().strip()).resolve(strict=True)
        if os.path.samefile(actual, rustup):
            raise RuntimeError(f'{name} still resolves to a rustup proxy')
    return {'requested': command, 'launcher_path': str(launcher), 'launcher_sha256': sha(launcher.read_bytes()),
            'actual_path': str(actual), 'actual_sha256': sha(actual.read_bytes())}


def expected_error(diagnostics, rust):
    semantic = [d for d in diagnostics if d.get('level') == 'error' and d.get('code')]
    extra = [d for d in diagnostics if d.get('level') == 'error' and not d.get('code')
             and d.get('message') != 'aborting due to 1 previous error']
    if len(semantic) != 1 or extra:
        return False
    error = semantic[0]
    offset = rust['src/m0.rs'].encode().index(b'absent_name')
    spans = [s for s in error.get('spans', []) if s.get('is_primary')]
    return (error.get('code', {}).get('code') == 'E0425' and 'absent_name' in error.get('message', '')
            and len(spans) == 1 and spans[0].get('file_name') == 'src/m0.rs'
            and spans[0].get('byte_start') == offset and spans[0].get('byte_end') == offset + len(b'absent_name'))


def cargo_messages(path):
    messages = []
    for line in path.read_text(errors='replace').splitlines():
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if isinstance(message, dict):
            messages.append(message)
    return messages


def compile_request(args, workloads, fixtures, block, mode, index, label, measured, transition, project, target, evidence, env):
    source = write_files(project, fixtures[label]['rust'], '.rs')
    command = [args.cargo, 'check' if mode == 'check' else 'build', '--offline', '--locked',
               '--jobs', '1', '--message-format=json', '--target-dir', str(target)]
    if mode == 'o2':
        command.append('--release')
    directory = evidence / f'{index:02d}-{label}-{"measured" if measured else "parent"}'
    result = run_process(command, project, directory, args.timeout, args.sample_ms, env=env)
    messages = cargo_messages(directory / 'stdout.bin')
    finished = [m for m in messages if m.get('reason') == 'build-finished']
    diagnostics = [m.get('message', {}) for m in messages if m.get('reason') == 'compiler-message']
    success = label != 'error'
    valid = not result['timed_out'] and result['returncode'] == (0 if success else 101)
    valid &= all(math.isfinite(result[k]) and result[k] > 0 for k in
                 ['wall_ms', 'tree_cpu_seconds_wait4', 'wait4_maxrss_kib', 'cargo_peak_sampled_rss_kib'])
    valid &= result['memory_sample_count'] > 0
    valid &= len(finished) == 1 and finished[0].get('success') is success
    if not success:
        valid &= expected_error(diagnostics, fixtures[label]['rust'])
    native = []
    if success and valid and mode == 'o2':
        artifacts = [m for m in messages if m.get('reason') == 'compiler-artifact' and m.get('target', {}).get('name') == PACKAGE and m.get('executable')]
        if len(artifacts) != 1:
            valid = False
        else:
            program = Path(artifacts[0]['executable'])
            result['executable_sha256'] = sha(program.read_bytes())
            for repeat in range(args.native_repetitions):
                for number, data in enumerate(workloads.inputs('scalar')):
                    expected = workloads.oracle('scalar', data, label, args.modules, args.functions, args.steps)
                    native_dir = directory / f'native-{repeat:02d}-{number:02d}'
                    outcome = run_process([str(program)], project, native_dir, args.native_timeout, args.sample_ms, data, env)
                    actual = {'exit': outcome['returncode'], 'stdout_hex': (native_dir / 'stdout.bin').read_bytes().hex(),
                              'stderr_hex': (native_dir / 'stderr.bin').read_bytes().hex()}
                    okay = not outcome['timed_out'] and actual == expected
                    native.append({'repetition': repeat, 'stdin_hex': data.hex(), 'expected': expected, 'actual': actual, 'valid': okay})
                    valid &= okay
    row = {'block': block, 'mode': mode, 'index': index, 'label': label, 'measured': measured,
           'transition_from': transition, 'source': source, 'zil_identity_sha256': fixtures[label]['zil_metadata']['identity_sha256'],
           'measurement': result, 'diagnostic_codes': [d.get('code') for d in diagnostics],
           'native': native, 'valid': bool(valid), 'directory': str(directory)}
    save(directory / 'request.json', row)
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--run', action='store_true', help='Explicitly execute Cargo/native programs; otherwise fixture generation only')
    parser.add_argument('--cargo', default='cargo')
    parser.add_argument('--rustc', default='rustc')
    parser.add_argument('--blocks', type=int, default=6)
    parser.add_argument('--modules', type=int, default=20)
    parser.add_argument('--functions', type=int, default=20)
    parser.add_argument('--steps', type=int, default=16)
    parser.add_argument('--codegen-units', type=int, default=1,
                        help='Rust CGUs in actual Cargo profiles: 1 controlled; 256 normal incremental-CGU observation')
    parser.add_argument('--request', action='append', choices=LABELS)
    parser.add_argument('--mode', action='append', choices=['check', 'o2'])
    parser.add_argument('--sample-ms', type=float, default=5)
    parser.add_argument('--timeout', type=float, default=300)
    parser.add_argument('--native-timeout', type=float, default=10)
    parser.add_argument('--native-repetitions', type=int, default=2)
    args = parser.parse_args()
    if min(args.blocks, args.modules, args.functions, args.steps, args.codegen_units, args.sample_ms, args.timeout, args.native_timeout, args.native_repetitions) <= 0:
        parser.error('dimensions, blocks, repetitions, sampling interval and timeouts must be positive')
    labels, modes = args.request or LABELS, args.mode or ['check', 'o2']
    if labels[0] != 'cold' or len(labels) != len(set(labels)) or len(modes) != len(set(modes)):
        parser.error('requests must begin with cold; requests and modes must be unique')
    if args.modules < 2 and 'multi' in labels:
        parser.error('multi requires at least two modules')
    args.repo, args.out = args.repo.resolve(), args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    archive = args.out / 'implementation'
    archive.mkdir()
    (archive / 'rust_compare.py').write_bytes(Path(__file__).read_bytes())
    generator = archive / 'native_workloads.py'
    generator.write_bytes((args.repo / 'tools/research/reactive-query-graph/native_workloads.py').read_bytes())
    workloads, fixtures = load_workloads(args.repo, generator), {}
    for label in dict.fromkeys([*labels, *(parent(label) for label in labels if parent(label))]):
        zil = workloads.snapshot('scalar', label, args.modules, args.functions, args.steps)
        rust = rust_files(workloads, label, args.modules, args.functions, args.steps)
        equal = validate_equivalence(zil, rust, args.modules, args.functions, args.steps, label)
        directory = args.out / 'fixtures' / label
        zmeta = write_files(directory / 'zil', zil, '.zil')
        rmeta = write_files(directory / 'rust', rust, '.rs')
        (directory / 'rust' / 'Cargo.toml').write_text(manifest_text(args.codegen_units))
        (directory / 'rust' / 'Cargo.lock').write_text(lock_text())
        fixtures[label] = {'rust': rust, 'zil_metadata': zmeta, 'rust_metadata': rmeta}
        save(directory / 'equivalence.json', {**equal, 'zil': zmeta, 'rust': rmeta,
            'native_oracles': [{'stdin_hex': data.hex(), **workloads.oracle('scalar', data, label, args.modules, args.functions, args.steps)} for data in workloads.inputs('scalar')] if label != 'error' else []})
    manifest = {'arguments': {k: str(v) if isinstance(v, Path) else v for k, v in vars(args).items()},
        'requests': labels, 'modes': modes, 'run_compiler': args.run,
        'runner_sha256': sha((archive / 'rust_compare.py').read_bytes()),
        'workload_generator_sha256': sha(generator.read_bytes()),
        'cargo_manifest_text': manifest_text(args.codegen_units),
        'cargo_manifest_sha256': sha(manifest_text(args.codegen_units).encode()),
        'target_policy': 'One initially nonexistent application target per block and mode; same target for all parent/measured requests within that block. Toolchain/std and OS page cache are not cold.',
        'source_policy': 'Only changed source bytes are written; unchanged files retain mtimes. Same complete snapshot and named parent as Zil. Source generation is excluded from request timing.',
        'scope': {'check': 'Cargo check default dev metadata/type/borrow checking, no linking/native oracle; not a Zil Verify-equivalent proof.',
                  'o2': f'Cargo release build opt-level=2, codegen-units={args.codegen_units}, jobs=1, incremental=true, lto=false; Zil compiler and verifier remain different work.',
                  'limits': 'Zil default logical budgets have no Rust analogue; none are fabricated or raised.'},
        'timing_policy': 'Cargo fresh-process request wall includes startup and filesystem planning; Zil resident request excludes startup. Show these as separate latency contracts; do not subtract a cargo --version baseline. Blocking wait4 determines completion; sampling does not quantize elapsed wall time to the sampling period.',
        'cpu_policy': 'wait4 child usage includes CPU of waited descendants; user/system retained separately.',
        'memory_policy': 'Only the direct Cargo/root PID RSS is sampled; rustc/linker children are excluded. wait4 maxrss is largest-process lifetime peak, not summed tree peak. No cross-language memory advantage claim.',
        'measurement_format': 'v2-cargo-root-rss',
        'measured_v1_runner_sha256': 'a1b2df047a31aebfc59ba954964e28ef33ea44e246dcbf82b9a7879065389e9d',
        'historical_artifact_policy': 'Measured v1 raw artifacts are archived separately and remain unchanged. Its tree_* RSS fields observed only root because /proc children was unavailable; this source reproducer explicitly names the same root-only observation.',
        'comparison_policy': 'When Rust and Zil run in separate windows, report separate distributions and relative medians, not paired-block confidence intervals. Six blocks do not support stable p95 claims. No assumed workload frequency weights.',
        'optimization_policy': 'No inline(never), black_box or artificial optimization barriers. All helpers consume live input and contribute to output. CGU1 is controlled, CGU256 permits normal incremental-CGU reuse; each setting is a separately identified run, not a claim of optimal Rust flags.',
        'mode_order': 'Alternate check/o2 order between blocks; requests retain the same eight-label order within each target.',
        'fixture_identity': {label: {'zil': f['zil_metadata']['identity_sha256'], 'rust': f['rust_metadata']['identity_sha256']} for label, f in fixtures.items()}}
    save(args.out / 'manifest.json', manifest)
    if not args.run:
        print(json.dumps({'generated_only': True, 'out': str(args.out), 'labels': labels, 'fixture_count': len(fixtures), 'compiler_commands_executed': 0}))
        return
    env = os.environ.copy()
    remove = ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_TARGET', 'CARGO_INCREMENTAL', 'CARGO_BUILD_RUSTFLAGS', 'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER']
    remove.extend(key for key in env if key.startswith(('CARGO_PROFILE_', 'CARGO_TARGET_')))
    removed = {key: env.pop(key) for key in remove if key in env}
    tools = {name: bind_tool(getattr(args, name), name, args, env) for name in ['cargo', 'rustc']}
    args.cargo, args.rustc = tools['cargo']['actual_path'], tools['rustc']['actual_path']
    # With no dependencies Cargo needs no shared registry/cache. Pin real tools
    # and an empty Cargo home so user config/proxy cwd cannot change the run.
    isolated_home = args.out / 'cargo-home'
    isolated_home.mkdir()
    inherited = [p for directory in [args.out, *args.out.parents]
                 for name in ['config.toml', 'config'] if (p := directory / '.cargo' / name).is_file()]
    if inherited:
        save(args.out / 'failure.json', {'unexpected_ancestor_cargo_configs': [str(p) for p in inherited]})
        raise RuntimeError('ancestor Cargo config would alter the isolated comparison')
    env['CARGO_HOME'] = str(isolated_home)
    env.update({'RUSTC': args.rustc, 'CARGO_INCREMENTAL': '1', 'CARGO_BUILD_JOBS': '1', 'CARGO_TERM_COLOR': 'never'})
    manifest['removed_environment_overrides'] = removed
    manifest['tools_before'] = tools
    manifest['cargo_configuration'] = {'isolated_cargo_home': str(isolated_home), 'ancestor_configs': [],
                                       'toolchain_pinning': 'Real absolute cargo/rustc binaries resolved in --repo context before isolation; identical tools in every generated-project cwd'}
    manifest['explicit_environment'] = {key: env[key] for key in ['RUSTC', 'CARGO_INCREMENTAL', 'CARGO_BUILD_JOBS', 'CARGO_TERM_COLOR']}
    for name, command in [('rustc-version', [args.rustc, '-vV']), ('cargo-version', [args.cargo, '-vV'])]:
        version = run_process(command, args.repo, args.out / name, 30, args.sample_ms, env=env)
        if version['returncode'] != 0 or version['timed_out']:
            raise RuntimeError(f'toolchain identity failed: {name}')
    save(args.out / 'manifest.json', manifest)
    rows = []
    with (args.out / 'rows.jsonl').open('w') as stream:
        for block in range(args.blocks):
            for mode in modes if block % 2 == 0 else list(reversed(modes)):
                directory = args.out / f'block-{block:02d}' / mode
                project, target = directory / 'project', directory / 'target'
                project.mkdir(parents=True)
                (project / 'Cargo.toml').write_text(manifest_text(args.codegen_units))
                (project / 'Cargo.lock').write_text(lock_text())
                if target.exists():
                    raise RuntimeError('cold target is not initially empty/nonexistent')
                for index, label in enumerate(labels):
                    transition = parent(label)
                    for current, measured in ([(transition, False)] if transition else []) + [(label, True)]:
                        row = compile_request(args, workloads, fixtures, block, mode, index, current, measured,
                                              transition if measured else None, project, target, directory / 'requests', env)
                        rows.append(row)
                        stream.write(json.dumps(row) + '\n')
                        stream.flush()
                        if not row['valid']:
                            save(args.out / 'failure.json', row)
                            raise RuntimeError(f'invalid request: block={block}, mode={mode}, label={current}; raw evidence retained')
    after = {name: sha(Path(identity['actual_path']).read_bytes()) for name, identity in tools.items()}
    manifest['tools_after_sha256'] = after
    save(args.out / 'manifest.json', manifest)
    if any(after[name] != identity['actual_sha256'] for name, identity in tools.items()):
        save(args.out / 'failure.json', {'toolchain_bytes_changed': after})
        raise RuntimeError('toolchain changed during comparison')
    summary = {'valid': True, 'measured_requests': args.blocks * len(modes) * len(labels),
               'preparation_requests': sum(not r['measured'] for r in rows), 'cells': {}}
    for mode in modes:
        summary['cells'][mode] = {}
        for label in labels:
            selected = [r for r in rows if r['measured'] and r['mode'] == mode and r['label'] == label]
            cell = {'n': len(selected), 'native_executions': sum(len(r['native']) for r in selected)}
            for metric in ['wall_ms', 'tree_cpu_seconds_wait4', 'wait4_maxrss_kib', 'cargo_peak_sampled_rss_kib']:
                values = [r['measurement'][metric] for r in selected]
                cell[metric] = {'observations': values, 'median': statistics.median(values), 'min': min(values), 'max': max(values)}
            summary['cells'][mode][label] = cell
    save(args.out / 'summary.json', summary)
    print(json.dumps({'valid': True, 'out': str(args.out), 'measured_requests': summary['measured_requests']}))


if __name__ == '__main__':
    main()
