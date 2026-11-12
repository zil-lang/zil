#!/usr/bin/env python3
"""Fixed six-block main/candidate/Rust experiment; generation is the default.

All three arms share a sequential request window. No profiling flags, budget
changes, artificial optimization barriers, or hidden reduced workload are used.
Run matched_validate.py after completion. Actual object bytes and symbol tables
are archived outside request timing; flags alone never establish partitioning.
"""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import select
import shutil
import signal
import subprocess
import sys
import threading
import time

import rust_compare as rc

MAIN = '99035931d358add0eaed6d4535035c38413701dd'
RUST = '58cf8f9c35b355199727627d07ae34a4e2846d67'
CARGO = '5f94df4789f005f9a352888e8355ffc645b7ed0e'
ARMS = ['baseline', 'candidate', 'rust']
ORDERS = [ARMS, list(reversed(ARMS)), ['candidate', 'rust', 'baseline'],
          ['baseline', 'rust', 'candidate'], ['rust', 'baseline', 'candidate'],
          ['candidate', 'baseline', 'rust']]


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def file_hash(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def blob(out, data, suffix=''):
    identity = rc.sha(data)
    target = out / 'blobs' / (identity + suffix)
    target.parent.mkdir(exist_ok=True)
    if not target.exists():
        target.write_bytes(data)
    return {'sha256': identity, 'bytes': len(data), 'archive': str(target.relative_to(out))}


def preserve(out, path, suffix=''):
    return blob(out, Path(path).read_bytes(), suffix)


def publication_state(paths):
    """Final publication history; source/cache/intermediate files are separate."""
    result = {}
    for path in paths:
        path = Path(path)
        if path.exists():
            stat = path.stat()
            result[str(path)] = {'exists': True, 'sha256': file_hash(path),
                                 'bytes': stat.st_size, 'mtime_ns': stat.st_mtime_ns,
                                 'inode': stat.st_ino, 'device': stat.st_dev}
        else:
            result[str(path)] = {'exists': False}
    return result


def source_identity(repo, commit, out, arm):
    def git(*args):
        return subprocess.check_output(['git', '-C', str(repo), *args])
    head = git('rev-parse', 'HEAD').decode().strip()
    if head != commit:
        raise ValueError(f'{arm}: worktree HEAD {head} differs from source pin {commit}')
    status = git('status', '--porcelain=v1', '--untracked-files=all').decode()
    # Research tools/reports can be edited while the compiler source stays frozen.
    changed = git('diff', '--name-only', 'HEAD').decode().splitlines()
    changed += git('ls-files', '--others', '--exclude-standard').decode().splitlines()
    compiler_paths = ('crates/', 'src/', 'Cargo.toml', 'Cargo.lock', 'build.rs', '.cargo/')
    if any(p.startswith(compiler_paths) for p in changed):
        raise ValueError(f'{arm}: compiler source differs from pinned commit')
    listing = git('ls-tree', '-r', '--full-tree', commit)
    diff = git('diff', '--binary', 'HEAD')
    return {'commit': commit, 'tree': git('rev-parse', commit + '^{tree}').decode().strip(),
            'worktree': str(repo), 'status': status,
            'source_tree_inventory': blob(out, listing, '.git-ls-tree'),
            'worktree_diff': blob(out, diff, '.patch')}


class Worker:
    """Resident boundary with explicitly root-only RSS and raw JSONL transport."""
    def __init__(self, command, directory, timeout, sample_ms, native):
        self.directory, self.timeout, self.sample_ms = directory, timeout, sample_ms
        self.native = native
        self.buffer, self.count = b'', 0
        self.err = (directory / 'stderr.bin').open('wb')
        self.raw = (directory / 'stdout.jsonl').open('wb')
        rc.save(directory / 'command.json', {'argv': command, 'cwd': os.getcwd()})
        start = time.perf_counter_ns()
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.err, start_new_session=True, bufsize=0)
        try:
            ready = self.read()
            if ready.get('ready') is not True or ready.get('protocol') != 1:
                raise ValueError('invalid resident ready')
            startup_wall_ms = (time.perf_counter_ns()-start)/1e6
            self.raw.write(self.last_line + b'\n')
            self.raw.flush()
            rc.save(directory / 'startup.json', {'ready': ready, 'pid': self.process.pid, 'wall_ms': startup_wall_ms})
        except BaseException:
            self.close()
            raise

    def read(self):
        deadline = time.monotonic() + self.timeout
        while b'\n' not in self.buffer:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([self.process.stdout], [], [], remaining)[0]:
                raise TimeoutError('resident response timeout')
            chunk = os.read(self.process.stdout.fileno(), 65536)
            if not chunk:
                raise RuntimeError(f'resident exited: {self.process.poll()}')
            self.buffer += chunk
        line, self.buffer = self.buffer.split(b'\n', 1)
        self.last_line = line
        return json.loads(line)

    def request(self, directory):
        samples, stop = [], threading.Event()
        (directory / 'stdin.bin').write_bytes(b'compile\n')
        before = self.native.proc_stat(self.process.pid)
        start = time.perf_counter_ns()
        def sample():
            while not stop.is_set():
                samples.append({'offset_ms': (time.perf_counter_ns()-start)/1e6,
                                'resident_pid': self.process.pid, 'resident_rss_kib': rc.memory_sample(self.process.pid)})
                stop.wait(self.sample_ms / 1000)
        thread = threading.Thread(target=sample, daemon=True)
        thread.start()
        try:
            self.process.stdin.write(b'compile\n')
            self.process.stdin.flush()
            response = self.read()
            wall_ms = (time.perf_counter_ns()-start)/1e6
            self.raw.write(self.last_line + b'\n')
            self.raw.flush()
        finally:
            stop.set()
            thread.join()
        after = self.native.proc_stat(self.process.pid)
        self.count += 1
        rc.save(directory / 'response.json', response)
        rc.save(directory / 'memory-samples.json', samples)
        self.native.validate_response(response, self.count)
        if before is None or after is None:
            raise ValueError('resident process accounting unavailable')
        result = {'wall_ms': wall_ms, 'resident_cpu_seconds': after['own_cpu_seconds']-before['own_cpu_seconds'],
                  'resident_and_waited_cpu_seconds': after['cpu_seconds']-before['cpu_seconds'],
                  'resident_rss_kib': response['rss_kib'], 'resident_peak_kib': response['peak_kib']}
        rc.save(directory / 'result.json', result)
        return result, response

    def close(self):
        if self.process.poll() is None:
            try:
                self.process.stdin.write(b'quit\n')
                self.process.stdin.flush()
                self.process.wait(timeout=3)
            except (OSError, subprocess.TimeoutExpired):
                os.killpg(self.process.pid, signal.SIGKILL)
                self.process.wait()
        self.err.close()
        self.raw.close()


def zil_error(report, source, files):
    diagnostics = report.get('diagnostics', [])
    if len(diagnostics) != 1:
        return False
    error = diagnostics[0]
    span = error.get('span') or {}
    offset = files['m0.zil'].encode().index(b'absent_name')
    mapping = report.get('source_files', {})
    return (error.get('code') == 'E-UNKNOWN-NAME'
            and span.get('start') == offset and span.get('end') == offset + len(b'absent_name')
            and mapping.get(str(span.get('file'))) == str(source / 'm0.zil'))


def symbols(args, object_blob):
    path = args.out / object_blob['archive']
    destination = args.out / 'symbols' / (object_blob['sha256'] + '.txt')
    destination.parent.mkdir(exist_ok=True)
    if not destination.exists():
        result = subprocess.run([args.nm, '--defined-only', '--print-size', str(path)],
                                capture_output=True, timeout=30)
        destination.write_bytes(result.stdout)
        destination.with_suffix('.stderr').write_bytes(result.stderr)
        rc.save(destination.with_suffix('.command.json'), {'argv': [args.nm, '--defined-only', '--print-size', str(path)],
                                                          'returncode': result.returncode})
        if result.returncode:
            raise ValueError(f'object symbol inventory failed: {path}')
    return {'path': str(destination.relative_to(args.out)), 'sha256': file_hash(destination)}


def object_record(args, path, root, payload=None):
    stat = path.stat()
    archived = blob(args.out, payload, '.o') if payload is not None else preserve(args.out, path, '.o')
    return {'path': str(path.relative_to(root)), 'name': path.name,
            'inode': stat.st_ino, 'device': stat.st_dev, 'mtime_ns': stat.st_mtime_ns,
            'object': archived, 'symbols': symbols(args, archived)}


def rust_inventory(args, target, previous):
    root = target / ('release' if args.mode == 'o2' else 'debug') / 'incremental'
    objects, metadata = [], []
    previous_objects = previous.get('objects', [])
    if root.exists():
        for path in sorted(root.rglob('*')):
            if not path.is_file():
                continue
            if path.suffix == '.o':
                item = object_record(args, path, root)
                # Sessions are renamed on commit. Identical bytes alone cannot
                # establish that rustc skipped a code generation operation.
                old = [r for r in previous_objects if r['name'] == item['name']]
                item['same_bytes_as_previous_request'] = any(r['object']['sha256'] == item['object']['sha256'] for r in old)
                item['same_inode_bytes_mtime_as_previous_request'] = any(
                    all(r[k] == item[k] for k in ['inode', 'device', 'mtime_ns'])
                    and r['object']['sha256'] == item['object']['sha256'] for r in old)
                objects.append(item)
            else:
                metadata.append({'path': str(path.relative_to(root)), **preserve(args.out, path, '.incremental')})
    sessions = sorted(set(str(Path(r['path']).parent) for r in objects))
    committed = [session for session in sessions if not Path(session).name.endswith('-working')
                 and Path(session).parts[0].startswith(rc.PACKAGE + '-')
                 and (root/session/'work-products.bin').is_file()]
    old_sessions = set(previous.get('sessions', []))
    new_sessions = [session for session in committed if session not in old_sessions]
    if len(new_sessions) == 1:
        current = new_sessions[0]
        basis = 'unique newly committed object session since immediately preceding request'
    elif not new_sessions and previous.get('current_session') in committed:
        current = previous['current_session']
        basis = 'previous committed session remains current; no new committed object session appeared'
    elif not committed:
        current, basis = None, 'no committed native object session (expected for cargo check)'
    else:
        raise ValueError('ambiguous current Rust object session; all raw evidence retained')
    active = [r for r in objects if str(Path(r['path']).parent) == current]
    historical = [r for r in objects if str(Path(r['path']).parent) != current]
    return {'objects': active, 'historical_objects': historical, 'metadata': metadata,
            'object_count': len(active), 'sessions': sessions, 'current_session': current,
            'current_session_selection': basis, 'previous_sessions': previous.get('sessions', []),
            'previous_current_session': previous.get('current_session'),
            'reuse_observation': 'same inode/device/mtime/content records physical carry-forward, not rustc query red-green counters; equal content alone cannot establish skipped codegen',
            'scope': 'Actual current committed crate-session objects, selected by unique new session or retained previous session, with older sessions listed separately. Ambiguity is fatal, not a count inferred from CGU flags.'}


def zil_inventory(args, report, cache):
    summary = report.get('native_codegen') or {}
    reports = summary.get('object_reports', [])
    if len(reports) != summary.get('actual_units') or not reports:
        raise ValueError('missing current Zil unit receipts')
    # This root was created empty for exactly one pinned arm/block. The
    # compiler's schema places objects under a tool/target-specific namespace;
    # require that namespace to be unique and stable rather than searching for
    # an action key recursively across unrelated compiler identities.
    schema = cache / 'objects-v1'
    if cache.is_symlink() or schema.is_symlink() or not schema.is_dir():
        raise ValueError('missing or unsafe Zil objects-v1 cache schema')
    entries = list(schema.iterdir())
    if len(entries) != 1:
        raise ValueError('fresh Zil arm cache does not have exactly one namespace')
    namespace_path = entries[0]
    namespace = namespace_path.name
    if (len(namespace) != 64 or any(c not in '0123456789abcdef' for c in namespace)
            or namespace_path.is_symlink() or not namespace_path.is_dir()
            or namespace_path.resolve().parent != schema.resolve()):
        raise ValueError('invalid or unsafe Zil object cache namespace')
    bindings = getattr(args, '_zil_namespace_bindings', {})
    cache_identity = str(cache.resolve())
    if cache_identity in bindings and bindings[cache_identity] != namespace:
        raise ValueError('Zil arm cache namespace changed during the block')
    bindings[cache_identity] = namespace
    args._zil_namespace_bindings = bindings
    objects = []
    for index, receipt in enumerate(reports):
        if isinstance(receipt, str):
            receipt = json.loads(receipt)
        access = receipt.get('cache', {})
        key = access.get('action_key')
        if not isinstance(key,str) or len(key)!=64 or any(c not in '0123456789abcdef' for c in key):
            raise ValueError('unit receipt lacks a valid current object action key')
        path = namespace_path / (key + '.zobj')
        if path.is_symlink() or not path.is_file() or path.resolve().parent != namespace_path.resolve():
            raise ValueError('missing or unsafe current Zil cached object')
        data = path.read_bytes()
        if (len(data) <= 80 or data[:8] != b'ZILOBJ01' or data[8:40].hex() != key
                or data[40:72].hex() != rc.sha(data[80:])
                or int.from_bytes(data[72:80], 'little') != len(data)-80):
            raise ValueError('invalid current Zil cached object header/key/payload checksum/length')
        item = object_record(args, path, cache, data[80:])
        item.update({'unit_index': index, 'action_key': key, 'cache_observation': access,
                     'cache_container': preserve(args.out, path, '.zobj')})
        objects.append(item)
    return {'object_count': len(objects), 'objects': objects, 'compiler_summary': summary,
            'cache_schema': 'objects-v1', 'namespace': namespace,
            'namespace_binding': 'Unique immediate hex64 namespace in a fresh private arm/block root, stable for the pinned compiler/library/target; symlinks and path escapes rejected',
            'scope': 'Current request unit receipts map exact action keys to archived objects and symbols; not the cumulative cache directory count'}


def native_outcomes(args, workloads, program, label, directory, env):
    outcomes = []
    for repeat in range(args.native_repetitions):
        for index, data in enumerate(workloads.inputs('scalar')):
            current = directory / f'native-{repeat:02d}-{index:02d}'
            result = rc.run_process([str(program)], directory, current, args.native_timeout, args.sample_ms, data, env)
            expected = workloads.oracle('scalar', data, label, 20, 20, 16)
            actual = {'exit': result['returncode'], 'stdout_hex': (current/'stdout.bin').read_bytes().hex(),
                      'stderr_hex': (current/'stderr.bin').read_bytes().hex()}
            outcomes.append({'stdin_hex': data.hex(), 'expected': expected, 'actual': actual,
                             'valid': not result['timed_out'] and actual == expected,
                             'directory': str(current.relative_to(args.out))})
    return outcomes


def rust_request(args, workloads, fixtures, label, project, target, directory, env, previous):
    source = rc.write_files(project, fixtures[label]['rust'], '.rs')
    command = [args.cargo, 'check' if args.mode == 'check' else 'build', '--offline', '--locked',
               '--jobs', '1', '--verbose', '--message-format=json', '--target-dir', str(target)]
    if args.mode == 'o2':
        command.append('--release')
    program_path = target / ('release' if args.mode == 'o2' else 'debug') / rc.PACKAGE
    before_publication = publication_state([program_path])
    # run_process creates the leaf directory itself.
    result = rc.run_process(command, project, directory, args.timeout, args.sample_ms, env=env)
    messages = rc.cargo_messages(directory / 'stdout.bin')
    diagnostics = [m.get('message', {}) for m in messages if m.get('reason') == 'compiler-message']
    finished = [m for m in messages if m.get('reason') == 'build-finished']
    success = label != 'error'
    valid = (not result['timed_out'] and result['returncode'] == (0 if success else 101)
             and len(finished) == 1 and finished[0].get('success') is success)
    valid &= rc.expected_error(diagnostics, fixtures[label]['rust']) if not success else not any(d.get('level') == 'error' for d in diagnostics)
    after_publication = publication_state([program_path])
    if not success:
        valid &= before_publication == after_publication
    row = {'source': source, 'measurement': result, 'diagnostics': diagnostics, 'valid': bool(valid),
           'publication_before': before_publication, 'publication_after': after_publication}
    if valid and success and args.mode == 'o2':
        artifacts = [m for m in messages if m.get('reason') == 'compiler-artifact' and m.get('target', {}).get('name') == rc.PACKAGE and m.get('executable')]
        if len(artifacts) != 1:
            raise ValueError('Rust executable publication missing/ambiguous')
        program = Path(artifacts[0]['executable'])
        row['executable'] = preserve(args.out, program, '.elf')
        row['native'] = native_outcomes(args, workloads, program, label, directory, env)
        row['valid'] &= all(r['valid'] for r in row['native'])
    row['inventory'] = rust_inventory(args, target, previous)
    if valid and success and args.mode == 'o2' and not row['inventory']['objects']:
        row['valid'] = False
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--mode', choices=['check', 'o2'], required=True)
    parser.add_argument('--baseline', type=Path)
    parser.add_argument('--candidate', type=Path)
    parser.add_argument('--baseline-worktree', type=Path)
    parser.add_argument('--candidate-worktree', type=Path)
    parser.add_argument('--baseline-source', default=MAIN, choices=[MAIN])
    parser.add_argument('--candidate-source')
    parser.add_argument('--baseline-build-receipt', type=Path)
    parser.add_argument('--candidate-build-receipt', type=Path)
    parser.add_argument('--cargo', default='cargo')
    parser.add_argument('--rustc', default='rustc')
    parser.add_argument('--nm', default='nm')
    parser.add_argument('--candidate-extra-arg', action='append', default=[])
    parser.add_argument('--timeout', type=float, default=900)
    parser.add_argument('--native-timeout', type=float, default=10)
    parser.add_argument('--sample-ms', type=float, default=5)
    parser.add_argument('--native-repetitions', type=int, default=2)
    args = parser.parse_args()
    if min(args.timeout, args.native_timeout, args.sample_ms, args.native_repetitions) <= 0:
        parser.error('timeouts, sample interval and repetitions must be positive')
    # These are the complete permitted research switches; no budget/backend/opt overrides.
    allowed = {'--producer-reuse', '--reactive-queries'}
    if not set(args.candidate_extra_arg) <= allowed:
        parser.error('candidate extras must be research switches, never optimization/cache/budget overrides')
    args.repo, args.out = args.repo.resolve(), args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=False)
    archive = args.out / 'implementation'
    archive.mkdir()
    generator = args.repo / 'tools/research/reactive-query-graph/native_workloads.py'
    native_path = generator.with_name('native_measure.py')
    implementation = [Path(__file__), Path(rc.__file__), generator, native_path,
                      Path(__file__).with_name('matched_validate.py')]
    for path in implementation:
        shutil.copyfile(path, archive / path.name)
    # native_measure imports native_workloads by name; bind the archived copy.
    sys.path.insert(0, str(archive))
    workloads = rc.load_workloads(args.repo, archive / generator.name)
    sys.modules['native_workloads'] = workloads
    native = load_module('matched_native', archive / native_path.name)
    fixtures = {}
    for label in rc.LABELS:
        zil = workloads.snapshot('scalar', label, 20, 20, 16)
        rust = rc.rust_files(workloads, label, 20, 20, 16)
        equality = rc.validate_equivalence(zil, rust, 20, 20, 16, label)
        directory = args.out / 'fixtures' / label
        zmeta = rc.write_files(directory/'zil', zil, '.zil')
        rmeta = rc.write_files(directory/'rust', rust, '.rs')
        (directory/'rust'/'Cargo.toml').write_text(rc.manifest_text(16))
        (directory/'rust'/'Cargo.lock').write_text(rc.lock_text())
        fixtures[label] = {'zil': zil, 'rust': rust, 'zil_metadata': zmeta, 'rust_metadata': rmeta}
        rc.save(directory/'equivalence.json', {**equality, 'zil': zmeta, 'rust': rmeta})
    manifest = {'schema': 'matched-three-arm-v1', 'arguments': {k: str(v) if isinstance(v, Path) else v for k,v in vars(args).items()},
                'fixed': {'blocks': 6, 'requests': rc.LABELS, 'modules': 20, 'functions': 20, 'steps': 16,
                          'codegen_units_requested': 16, 'codegen_jobs': 1, 'native_opt': 'O2', 'cache': 'all', 'capacity': 256},
                'implementation': {p.name: file_hash(archive/p.name) for p in implementation},
                'timing_policy': 'Sequential three-arm requests, six balanced arm permutations rotated per block/request. Zil resident send-to-response excludes startup; Rust full Cargo process includes startup. Source writes, object inventory and native execution occur outside request wall timing. No profiling. OS cache not flushed.',
                'parent_policy': 'Every non-cold request is immediately preceded for all three arms by its validated named parent: recovery<-error, restore<-private, all others<-cold. Preparations retained and validated, no cache reset within block.',
                'inventory_policy': 'Archive actual object bytes and defined symbol tables, Rust incremental metadata and inode/content/mtime observations; Zil current per-unit action-key/cache-state receipts. Hash equality is not proof of skipped work. No partition equivalence implied by requested CGU16.',
                'memory_policy': 'Root-only RSS; resident Zil root vs Cargo root are different scopes. wait4 maxrss is largest process, not tree sum; no cross-language memory advantage.',
                'fixture_identity': {k: {lang: v[lang+'_metadata']['identity_sha256'] for lang in ['zil','rust']} for k,v in fixtures.items()},
                'started_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}
    rc.save(args.out/'manifest.json', manifest)
    if not args.run:
        print(json.dumps({'generated_only': True, 'out': str(args.out), 'compiler_commands_executed': 0}))
        return
    for key in ['baseline', 'candidate', 'baseline_worktree', 'candidate_worktree', 'candidate_source', 'baseline_build_receipt', 'candidate_build_receipt']:
        if getattr(args, key) is None:
            parser.error('--run requires --' + key.replace('_','-'))
    if len(args.candidate_source) != 40 or any(c not in '0123456789abcdef' for c in args.candidate_source):
        parser.error('candidate source must be an exact git commit')
    env = os.environ.copy()
    # Prevent inherited Cargo config from changing optimization, wrapper or target.
    remove = [k for k in env if k in {'RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','CARGO_BUILD_TARGET','CARGO_INCREMENTAL','CARGO_BUILD_RUSTFLAGS','CARGO_BUILD_RUSTC_WRAPPER','CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER'} or k.startswith(('CARGO_PROFILE_','CARGO_TARGET_'))]
    manifest['removed_environment_override_names'] = sorted(remove)
    for key in remove:
        env.pop(key)
    tools = {name: rc.bind_tool(getattr(args,name), name, args, env) for name in ['cargo','rustc']}
    args.cargo, args.rustc = tools['cargo']['actual_path'], tools['rustc']['actual_path']
    args.nm = str(Path(shutil.which(args.nm) or args.nm).resolve(strict=True))
    tools['nm'] = {'actual_path': args.nm, 'actual_sha256': file_hash(args.nm)}
    cargo_home = args.out/'cargo-home'
    cargo_home.mkdir()
    configs = [p for directory in [args.out,*args.out.parents] for name in ['config','config.toml'] if (p:=directory/'.cargo'/name).exists()]
    if configs:
        raise ValueError('ancestor Cargo config detected: '+str(configs))
    env.update({'CARGO_HOME':str(cargo_home), 'RUSTC':args.rustc, 'CARGO_INCREMENTAL':'1', 'CARGO_BUILD_JOBS':'1', 'CARGO_TERM_COLOR':'never'})
    manifest['explicit_environment'] = {k:env[k] for k in ['CARGO_HOME','RUSTC','CARGO_INCREMENTAL','CARGO_BUILD_JOBS','CARGO_TERM_COLOR']}
    for name, commit, version in [('rustc',RUST,'release: 1.99.1'),('cargo',CARGO,None)]:
        result = rc.run_process([getattr(args,name),'-vV'], args.repo, args.out/(name+'-version'), 30, args.sample_ms, env=env)
        output = (args.out/(name+'-version')/'stdout.bin').read_text()
        if result['returncode'] or commit not in output or (version and version not in output):
            raise ValueError('incorrect pinned toolchain: '+name)
    manifest['tools'] = tools
    manifest['sources'] = {}
    manifest['binaries'] = {}
    for arm in ['baseline','candidate']:
        path = getattr(args,arm).resolve(strict=True)
        setattr(args,arm,path)
        manifest['sources'][arm] = source_identity(getattr(args,arm+'_worktree').resolve(),getattr(args,arm+'_source'),args.out,arm)
        receipt = getattr(args,arm+'_build_receipt')
        binding = json.loads(receipt.read_text())
        if binding.get('source') != getattr(args,arm+'_source') or binding.get('sha256') != file_hash(path) or binding.get('exit_code') != 0 or not binding.get('command'):
            raise ValueError(arm + ': build receipt source/binary/success binding failed')
        if binding.get('tree') != manifest['sources'][arm]['tree']:
            raise ValueError(arm + ': build receipt tree differs from pinned source')
        manifest['binaries'][arm] = {'path':str(path), **preserve(args.out,path,'.compiler'), 'build_receipt':preserve(args.out,receipt,'.build-receipt')}
    manifest['runtime_environment'] = {key:os.environ.get(key) for key in ['ZIL_CLANG','ZIL_LLVM_CONFIG','LLVM_CONFIG','LD_LIBRARY_PATH','LD_PRELOAD','LD_AUDIT','CARGO_TARGET_DIR','RUSTUP_TOOLCHAIN']}
    manifest['host'] = {'affinity':sorted(os.sched_getaffinity(0)), 'load_average':os.getloadavg(), 'uname':list(os.uname())}
    rc.save(args.out/'manifest.json',manifest)
    rows = []
    try:
        with (args.out/'rows.jsonl').open('w') as stream:
            for block in range(6):
                directory = args.out/f'block-{block:02d}'
                directory.mkdir()
                source, project, target = directory/'source',directory/'rust-project',directory/'rust-target'
                rc.write_files(source,fixtures['cold']['zil'],'.zil')
                project.mkdir()
                (project/'Cargo.toml').write_text(rc.manifest_text(16))
                (project/'Cargo.lock').write_text(rc.lock_text())
                workers, previous_inventory, published_llvm = {}, {}, {}
                try:
                    for arm in ORDERS[block]:
                        if arm == 'rust':
                            continue
                        work = directory/arm
                        work.mkdir()
                        command = [str(getattr(args,arm)),'check' if args.mode=='check' else 'build',str(source/'main.zil'),
                                   '--json','--resident','--resident-capacity','256','--resident-reset-after','4096',
                                   '--semantic-cache',str(work/'semantic')]
                        if args.mode=='o2':
                            command += ['--backend','llvm-api','--opt','O2','--codegen-units','16','--codegen-jobs','1',
                                        '--object-cache',str(work/'objects'),'-o',str(work/'program')]
                        if arm=='candidate':
                            command += args.candidate_extra_arg
                        workers[arm] = Worker(command,work,args.timeout,args.sample_ms,native)
                    for index,label in enumerate(rc.LABELS):
                        parent = rc.parent(label)
                        for current,measured in ([(parent,False)] if parent else [])+[(label,True)]:
                            source_meta = rc.write_files(source,fixtures[current]['zil'],'.zil')
                            pair = []
                            order = ORDERS[(block+index)%6]
                            for position,arm in enumerate(order):
                                leaf = directory/'requests'/f'{index:02d}-{current}-{"measured" if measured else "parent"}'/arm
                                leaf.parent.mkdir(parents=True,exist_ok=True)
                                common = {'block':block,'request_index':index,'request':current,'target_request':label,'measured':measured,
                                          'transition_from':parent if measured else None,'position':position,'order':order,'arm':arm,'mode':args.mode,
                                          'started_utc_ns':time.time_ns(), 'directory':str(leaf.relative_to(args.out)),'source_identity_sha256':source_meta['identity_sha256']}
                                if arm=='rust':
                                    row = rust_request(args,workloads,fixtures,current,project,target,leaf,env,previous_inventory)
                                    previous_inventory = row['inventory']
                                else:
                                    leaf.mkdir()
                                    program = directory/arm/'program'
                                    publication_paths = [program] + ([published_llvm[arm]] if arm in published_llvm else [])
                                    before_publication = publication_state(publication_paths)
                                    result,response = workers[arm].request(leaf)
                                    report = response['report']
                                    valid = set(report['source_files'].values()) == {str(source/name) for name in fixtures[current]['zil']}
                                    if current=='error':
                                        valid &= report['status']=='SafeReject' and report['phase']=='types' and report['verification_runs']==0 and zil_error(report,source,fixtures[current]['zil'])
                                    else:
                                        valid &= report['status']=='Success' and report['verification_runs']==1 and report['diagnostics']==[] and sum(e=={'phase':'verify','disposition':'executed'} for e in report['query_events'])==1
                                    after_publication = publication_state(publication_paths)
                                    if current=='error':
                                        valid &= before_publication == after_publication
                                    row = {'measurement':result,'resident':response,'valid':bool(valid),'source':source_meta,
                                           'publication_before':before_publication,'publication_after':after_publication}
                                    if valid and current!='error' and args.mode=='o2':
                                        if report['artifacts']['executable'] != str(program):
                                            raise ValueError('unexpected Zil executable path')
                                        row['executable'] = preserve(args.out,program,'.elf')
                                        published_llvm[arm] = Path(report['artifacts']['llvm'])
                                        row['llvm'] = preserve(args.out,published_llvm[arm],'.ll')
                                        row['inventory'] = zil_inventory(args,report,directory/arm/'objects')
                                        row['native'] = native_outcomes(args,workloads,program,current,leaf,env)
                                        row['valid'] &= all(r['valid'] for r in row['native'])
                                row.update(common)
                                row['ended_utc_ns'] = time.time_ns()
                                pair.append(row)
                                rc.save(leaf/'request.json',row)
                                stream.write(json.dumps(row)+'\n')
                                stream.flush()
                                if not row['valid']:
                                    raise ValueError(f'invalid request {block}/{index}/{current}/{arm}')
                            zil_rows = [r for r in pair if r['arm']!='rust']
                            left,right = [r['resident']['report'] for r in zil_rows]
                            equal = all(left[k]==right[k] for k in ['status','phase','diagnostics','input_fingerprint','source_files','scalar_interfaces'])
                            native_pair = args.mode=='o2' and current!='error'
                            executable_equal = zil_rows[0].get('executable',{}).get('sha256') == zil_rows[1].get('executable',{}).get('sha256') if native_pair else None
                            llvm_text_equal = zil_rows[0].get('llvm',{}).get('sha256') == zil_rows[1].get('llvm',{}).get('sha256') if native_pair else None
                            pair_valid = equal and (not native_pair or (executable_equal and llvm_text_equal))
                            rc.save(leaf.parent/'pair-validation.json',{'valid':pair_valid,'request':current,'measured':measured,
                                                                      'semantics_equal':equal,'executable_equal':executable_equal,
                                                                      'llvm_text_equal':llvm_text_equal})
                            if not pair_valid:
                                raise ValueError('main/candidate current semantics or executable/LLVM content differ; raw evidence retained for diagnosis')
                            rows.extend(pair)
                            print(json.dumps({'block':block,'request':current,'measured':measured,'order':order,'wall_ms':{r['arm']:r['measurement']['wall_ms'] for r in pair}}),flush=True)
                finally:
                    for worker in workers.values():
                        worker.close()
        after = {name:file_hash(identity['actual_path']) for name,identity in tools.items()}
        manifest['tools_after_sha256'] = after
        manifest['binary_after_sha256'] = {arm:file_hash(getattr(args,arm)) for arm in ['baseline','candidate']}
        manifest['implementation_after_sha256'] = {p.name:file_hash(p) for p in implementation}
        if manifest['implementation_after_sha256'] != manifest['implementation']:
            raise ValueError('measurement implementation changed during the run')
        for arm in ['baseline','candidate']:
            source_identity(getattr(args,arm+'_worktree').resolve(),getattr(args,arm+'_source'),args.out,arm)
        manifest['completed_utc'] = time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime())
        rc.save(args.out/'manifest.json',manifest)
        if any(after[name]!=identity['actual_sha256'] for name,identity in tools.items()) or any(manifest['binary_after_sha256'][arm]!=manifest['binaries'][arm]['sha256'] for arm in ['baseline','candidate']):
            raise ValueError('tool or compiler bytes changed during run')
        rc.save(args.out/'completion.json',{'valid':True,'rows':len(rows),'measured':sum(r['measured'] for r in rows),'preparations':sum(not r['measured'] for r in rows)})
    except BaseException as error:
        rc.save(args.out/'failure.json',{'type':type(error).__name__,'message':str(error),'completed_rows':len(rows)})
        raise


if __name__=='__main__':
    main()
