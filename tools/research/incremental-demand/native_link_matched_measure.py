#!/usr/bin/env python3
"""Nine-scenario main9170/candidate/Rust experiment; generation is the default.

All three arms share a sequential request window. No profiling flags, budget
changes, artificial optimization barriers, or hidden reduced workload are used.
Legacy matched_validate.py intentionally does not qualify this new schema. Actual object bytes and symbol tables
are archived outside request timing; flags alone never establish partitioning.
"""
import argparse
import copy
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import select
import shutil
import signal
import struct
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True
HELPER_DIRECTORY = Path(__file__).parent
sys.path.insert(0, str(HELPER_DIRECTORY))
import rust_compare as rc
import push_counters as pc

MAIN = '9170bb4210554392e36eb7528596612596ebef80'
LABELS = ['cold', 'unchanged', 'private', 'multi', 'broad', 'positions', 'error', 'recovery', 'restore']
RUST = '58cf8f9c35b355199727627d07ae34a4e2846d67'
CARGO = '5f94df4789f005f9a352888e8355ffc645b7ed0e'
ARMS = ['baseline', 'candidate', 'rust']
ORDERS = [ARMS, list(reversed(ARMS)), ['candidate', 'rust', 'baseline'],
          ['baseline', 'rust', 'candidate'], ['rust', 'baseline', 'candidate'],
          ['candidate', 'baseline', 'rust']]
QUALITY_POLICY = {
    'schema': 'native-link-exact-elf-reviewed-ir-v1',
    'llvm': 'Full raw LLVM bytes and exact second-line-fingerprint normalization remain observations. Different IR representations are permitted for the reviewed early SSA and proven arithmetic rewrite in this candidate; LLVM textual equality is not an acceptance prerequisite.',
    'machine': 'Every paired main/candidate successful native request must have identical complete ELF bytes. Different executable bytes stop the run and require independent quality review; no normalized machine-code exception is enabled.',
    'execution': 'Every successful request in all three arms must match all fixed native input/output/exit oracles. A failed request cannot execute or claim an earlier executable.',
    'units': 'Each Zil native request requires sixteen actual current object receipts, complete physical objects/symbols, and a physical layout whose exact ownership matches current diagnostic function names. Rust inventories retain actual current sessions; CGU flags alone are not evidence of equal partitions.',
    'authority': 'IR-change permission is specific to the independently reviewed early SSA/proven rewrite and its real O0/O2 native oracle support. It does not grant permission to accept different final ELF bytes or omit current independent Verify.',
}
STAGE_BODY_FIELDS = ('captures', 'replays', 'declines', 'captured_bytes', 'capture_ns', 'prepare_ns', 'restore_ns', 'failed_replays')


def candidate_flags(flags):
    if flags != ['--push-reuse']:
        raise ValueError('this protocol requires exactly --push-reuse; profile path is a separate pinned argument')
    return flags


def rust_files(workloads, label, modules, functions, steps):
    files = rc.rust_files(workloads, label, modules, functions, steps)
    if label == 'positions':
        prefix = '// Source position edit; semantic body is unchanged.\n\n'
        files = {name: prefix + value for name, value in files.items()}
    return files


def zil_command(args, binary, source, work, arm):
    command = pc.zil_command(binary=binary, mode=args.mode, source=source, work=work,
                             arm=arm, extras=candidate_flags(args.candidate_extra_arg))
    if arm == 'candidate' and args.mode == 'o2':
        command += ['--native-link-profile', str(args.native_link_profile)]
    return command


def typed_stage_observations(report):
    native = report.get('native_codegen')
    if native is None:
        return None
    result = []
    for index, unit in enumerate(native['object_reports']):
        unit = json.loads(unit) if isinstance(unit, str) else unit
        stats = unit.get('native_products')
        value = stats.get('stage_body') if stats is not None else None
        if value is not None:
            if (not isinstance(value, dict) or type(value.get('observer_schema')) is not int
                    or value['observer_schema'] != 1 or any(type(value.get(field)) is not int or value[field] < 0 for field in STAGE_BODY_FIELDS)):
                raise ValueError('malformed typed stage_body V1 observation')
        result.append({'unit_index': index, 'stage_body': value})
    return result


def function_symbols(data):
    if len(data) < 64 or data[:6] != b'\x7fELF\x02\x01':
        raise ValueError('current object lacks a complete native ELF header')
    offset = struct.unpack_from('<Q', data, 40)[0]
    width, count = struct.unpack_from('<HH', data, 58)
    if width != 64 or not count or offset + count * width > len(data):
        raise ValueError('current object ELF section bounds')
    sections = [struct.unpack_from('<IIQQQQIIQQ', data, offset + i * width) for i in range(count)]
    result, tables = set(), 0
    for section in sections:
        if section[1] != 2:
            continue
        tables += 1
        if section[6] >= count or section[9] != 24 or section[5] % 24 or section[4] + section[5] > len(data):
            raise ValueError('current object ELF symbol-table bounds')
        strings = sections[section[6]]
        if strings[1] != 3 or strings[4] + strings[5] > len(data):
            raise ValueError('current object ELF string-table bounds')
        table = data[strings[4]:strings[4]+strings[5]]
        for at in range(section[4], section[4]+section[5], 24):
            name, info, other, index, value, size = struct.unpack_from('<IBBHQQ', data, at)
            if info & 15 == 2 and index:
                end = table.find(b'\0', name)
                if name >= len(table) or end < 0:
                    raise ValueError('current object ELF function-name bounds')
                result.add(table[name:end].decode())
    if not tables:
        raise ValueError('current object has no physical function symbol table')
    return result


def layout_units(data):
    offset = 72

    def word():
        nonlocal offset
        if offset + 4 > len(data):
            raise ValueError('truncated physical partition layout')
        value = int.from_bytes(data[offset:offset+4], 'little')
        offset += 4
        return value

    if word() != 16:
        raise ValueError('physical partition layout is not sixteen units')
    units = []
    for _ in range(16):
        unit = {'owner': word()}
        for group in ('prelude', 'declarations', 'imports', 'definitions'):
            size = word()
            if size > 1_048_576:
                raise ValueError('physical layout reference limit')
            unit[group] = [word() for _ in range(size)]
        units.append(unit)
    if offset != len(data):
        raise ValueError('trailing physical partition layout bytes')
    return units


def final_artifact_scope(args, report):
    cg = report.get('native_codegen') or {}
    artifact = cg.get('final_artifact')
    if artifact is None:
        return None
    required = {'reused_program', 'retained_files', 'before_codegen', 'current_preflight_checked_units',
                'current_preflight_reused_units', 'profile_id', 'llvm_verification_scope', 'current_llvm_verifier_executed'}
    if (not isinstance(artifact, dict) or not required <= set(artifact) or artifact['reused_program'] is not True
            or type(artifact['retained_files']) is not bool or type(artifact['before_codegen']) is not bool
            or artifact['profile_id'] != args.native_link_profile_sha256
            or artifact['llvm_verification_scope'] != 'retained-producer'
            or (report.get('backend_work') or {}).get('reused_link') is not True):
        raise ValueError('malformed/unbound final artifact reuse scope')
    if artifact['before_codegen']:
        counts = [artifact['current_preflight_checked_units'], artifact['current_preflight_reused_units']]
        if (any(type(value) is not int or value < 0 for value in counts) or sum(counts) != cg.get('actual_units')
                or artifact['current_llvm_verifier_executed'] is not False or cg.get('object_reports') != []
                or report.get('commands') != []):
            raise ValueError('before-codegen reuse lacks complete current preflight or fabricates current object/native work')
        cache = cg.get('object_cache') or {}
        if any(type(cache.get(field)) is not int or cache[field] != 0 for field in ('hits', 'misses', 'stored')):
            raise ValueError('before-codegen terminal reuse must retain its actual zero object-cache lookups')
    elif (artifact['current_preflight_checked_units'] is not None or artifact['current_preflight_reused_units'] is not None
          or artifact['current_llvm_verifier_executed'] is not None or artifact['retained_files'] is not False
          or len(cg.get('object_reports', [])) != cg.get('actual_units')):
        raise ValueError('late final reuse must keep null aggregate preflight counts and actual current unit reports')
    return artifact


def recheck_archived(out, item):
    relative = item['archive']
    if Path(relative).is_absolute() or '..' in Path(relative).parts:
        raise ValueError('unsafe retained producer artifact path')
    path = out/relative
    if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(out.resolve()):
        raise ValueError('missing/unsafe retained producer artifact')
    data = path.read_bytes()
    if len(data) != item['bytes'] or rc.sha(data) != item['sha256']:
        raise ValueError('retained producer artifact bytes changed')
    return data


def retained_inventory(args, report, cache, llvm, executable_sha256, artifact):
    key = (str(cache.resolve()), pc.llvm_comparison_sha256(llvm), executable_sha256)
    prior = getattr(args, '_zil_producer_inventories', {}).get(key)
    if prior is None:
        raise ValueError('terminal reuse has no same-worker exact LLVM/full-ELF physical producer provenance')
    result = copy.deepcopy(prior)
    for item in result['objects']:
        payload = recheck_archived(args.out, item['object'])
        container = recheck_archived(args.out, item['cache_container'])
        if (container[:8] != b'ZILOBJ01' or container[8:40].hex() != item['action_key']
                or container[40:72].hex() != item['object']['sha256'] or container[80:] != payload
                or int.from_bytes(container[72:80], 'little') != len(payload)
                or sorted(function_symbols(payload)) != item['defined_functions']):
            raise ValueError('retained physical producer object/key/exports no longer match')
        symbols = item['symbols']
        path = args.out/symbols['path']
        if path.is_symlink() or not path.resolve().is_relative_to(args.out.resolve()) or file_hash(path) != symbols['sha256']:
            raise ValueError('retained producer symbol evidence changed')
        item['producer_cache_observation'] = item.pop('cache_observation')
        item['cache_observation'] = None
        item['producer_file_stamp'] = {name: item.pop(name) for name in ('inode', 'device', 'mtime_ns')}
        item['physical_scope'] = 'Prior real producer object archived in this same worker; no current object-cache lookup or native compilation is asserted.'
    for layout in result['partition_layouts']:
        data = recheck_archived(args.out, layout)
        if data[:8] != b'ZILCGU01' or data[8:40].hex() != layout['key'] or rc.sha(data[72:]) != layout['payload_sha256'] or layout_units(data) != layout['units']:
            raise ValueError('retained physical producer layout changed')
    result['producer_compiler_summary'] = result.pop('compiler_summary')
    result.update({'compiler_summary': report['native_codegen'], 'current_object_report_count': 0,
                   'provenance_kind': 'before-codegen-retained-producer', 'final_artifact': artifact,
                   'current_preflight_checked_units': artifact['current_preflight_checked_units'],
                   'current_preflight_reused_units': artifact['current_preflight_reused_units'],
                   'scope': 'Sixteen physical prior producer CGUs/exports/layout are carried forward only after complete current machine preflight and exact same-worker normalized LLVM/full-ELF binding. Real current cache hits/misses stay zero; no fabricated per-unit current receipt.'})
    return result


def quality(pair, args):
    if len(pair) != 3 or {row['arm'] for row in pair} != set(ARMS):
        raise ValueError('quality requires the complete three-arm request')
    zil = [row for row in pair if row['arm'] != 'rust']
    if len(zil) != 2 or {row['arm'] for row in zil} != {'baseline', 'candidate'}:
        raise ValueError('quality requires both actual Zil arms')
    left, right = [row['resident']['report'] for row in zil]
    equal = all(left[key] == right[key] for key in ['status', 'phase', 'diagnostics', 'input_fingerprint', 'source_files', 'scalar_interfaces'])
    native = args.mode == 'o2' and left['status'] == right['status'] == 'Success'
    executable_equal = llvm_equal = normalized_equal = exports_equal = aggregate_exports_equal = None
    oracles_valid = units_valid = True
    if native:
        elf_bytes = []
        for row in zil:
            item = row['executable']
            data = (args.out/item['archive']).read_bytes()
            if len(data) != item['bytes'] or rc.sha(data) != item['sha256'] or not data.startswith(b'\x7fELF'):
                raise ValueError('complete current executable archive length/hash/ELF identity failed')
            elf_bytes.append(data)
            inv = row['inventory']
            units_valid &= inv['object_count'] == len(inv['objects']) == 16 and inv['compiler_summary']['actual_units'] == 16 and bool(inv['current_layout_matches'])
        executable_equal = elf_bytes[0] == elf_bytes[1]
        llvm_equal = zil[0]['llvm']['sha256'] == zil[1]['llvm']['sha256']
        normalized_equal = zil[0]['llvm']['comparison_sha256_excluding_identity_comment'] == zil[1]['llvm']['comparison_sha256_excluding_identity_comment']
        exports_equal = [item['defined_functions'] for item in zil[0]['inventory']['objects']] == [item['defined_functions'] for item in zil[1]['inventory']['objects']]
        aggregate_exports_equal = {name for item in zil[0]['inventory']['objects'] for name in item['defined_functions']} == {name for item in zil[1]['inventory']['objects'] for name in item['defined_functions']}
        for row in pair:
            outcomes = row.get('native', [])
            oracles_valid &= len(outcomes) == 6 * args.native_repetitions and all(outcome['valid'] is True and outcome['expected'] == outcome['actual'] for outcome in outcomes)
    valid = all(row['valid'] for row in pair) and equal and (not native or executable_equal and oracles_valid and units_valid and aggregate_exports_equal)
    return {'schema': QUALITY_POLICY['schema'], 'valid': bool(valid), 'semantics_equal': equal,
            'executable_equal': executable_equal, 'llvm_text_equal': llvm_equal, 'raw_llvm_equal': llvm_equal,
            'llvm_equal_excluding_identity_comment': normalized_equal, 'per_unit_exports_equal_observed': exports_equal,
            'aggregate_zil_function_exports_equal': aggregate_exports_equal,
            'native_oracles_valid': bool(oracles_valid) if native else None,
            'physical_zil_units_valid': bool(units_valid) if native else None,
            'llvm_equality_is_acceptance_prerequisite': False}


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
    if target.is_symlink():
        raise ValueError('symlink in hash-addressed artifact archive')
    if target.exists():
        existing = target.read_bytes()
        if existing != data or len(existing) != len(data) or rc.sha(existing) != identity:
            raise ValueError('existing content-addressed artifact differs')
    else:
        with target.open('xb') as stream:
            stream.write(data)
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
        committed = git('diff', '--name-only', commit, 'HEAD').decode().splitlines()
        if not committed or any(not name.startswith('tools/') for name in committed):
            raise ValueError(f'{arm}: checkout differs from compiled source beyond committed tools')
        if subprocess.run(['git', 'merge-base', '--is-ancestor', commit, 'HEAD'], cwd=repo, capture_output=True).returncode:
            raise ValueError(f'{arm}: compiled source is not an ancestor of the tools checkout')
    status = git('status', '--porcelain=v1', '--untracked-files=all').decode()
    # Research tools/reports can be edited while the compiler source stays frozen.
    changed = git('diff', '--name-only', 'HEAD').decode().splitlines()
    changed += git('ls-files', '--others', '--exclude-standard').decode().splitlines()
    compiler_paths = ('crates/', 'src/', 'Cargo.toml', 'Cargo.lock', 'build.rs', '.cargo/')
    if any(p.startswith(compiler_paths) for p in changed):
        raise ValueError(f'{arm}: compiler source differs from pinned commit')
    listing = git('ls-tree', '-r', '--full-tree', commit)
    diff = git('diff', '--binary', 'HEAD')
    return {'commit': commit, 'checkout_commit': head, 'tree': git('rev-parse', commit + '^{tree}').decode().strip(),
            'worktree': str(repo), 'status': status,
            'source_tree_inventory': blob(out, listing, '.git-ls-tree'),
            'worktree_diff': blob(out, diff, '.patch')}


class Worker:
    """Resident boundary with explicitly root-only RSS and raw JSONL transport."""
    def __init__(self, command, directory, timeout, sample_ms, native, env, runtime_root):
        self.directory, self.timeout, self.sample_ms = directory, timeout, sample_ms
        self.native = native
        self.env, self.runtime_root, self.runtime_cache = env, runtime_root, {}
        self.buffer, self.count = b'', 0
        self.err = (directory / 'stderr.bin').open('wb')
        self.raw = (directory / 'stdout.jsonl').open('wb')
        rc.save(directory / 'command.json', {'argv': command, 'cwd': os.getcwd(), 'environment': native.runtime_environment(env)})
        start = time.perf_counter_ns()
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.err, start_new_session=True, bufsize=0, env=env)
        try:
            ready = self.read()
            if ready.get('ready') is not True or ready.get('protocol') != 1:
                raise ValueError('invalid resident ready')
            startup_wall_ms = (time.perf_counter_ns()-start)/1e6
            self.raw.write(self.last_line + b'\n')
            self.raw.flush()
            rc.save(directory / 'startup.json', {'ready': ready, 'pid': self.process.pid, 'wall_ms': startup_wall_ms})
            self.runtime = native.capture_runtime(self.process.pid, runtime_root, env, self.runtime_cache)
            rc.save(directory / 'runtime-startup.json', self.runtime)
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
        current_runtime = self.native.capture_runtime(self.process.pid, self.runtime_root, self.env, self.runtime_cache)
        rc.save(directory / 'runtime.json', current_runtime)
        if current_runtime['llvm_dsos'] != self.runtime['llvm_dsos']:
            raise ValueError('resident LLVM DSO changed between requests')
        self.runtime = current_runtime
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


def zil_inventory(args, report, cache, llvm, executable_sha256, request_directory=None):
    summary = report.get('native_codegen') or {}
    reports = summary.get('object_reports', [])
    if (summary.get('backend') != 'llvm-api' or summary.get('requested_units') != 16
            or summary.get('actual_units') != 16 or summary.get('jobs') != 1 or summary.get('fallback') is not None):
        raise ValueError('actual native partition/backend differs from CGU16/O2/jobs1 contract')
    artifact = final_artifact_scope(args, report)
    if artifact is not None and artifact['before_codegen']:
        return retained_inventory(args, report, cache, llvm, executable_sha256, artifact)
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
        if receipt.get('backend') != 'llvm-api' or receipt.get('pipeline') != 'default<O2>':
            raise ValueError('actual native unit pipeline/backend differs')
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
        if receipt.get('object_bytes') != len(data)-80:
            raise ValueError('current object receipt byte count differs from physical payload')
        item = object_record(args, path, cache, data[80:])
        item.update({'unit_index': index, 'action_key': key, 'cache_observation': access,
                     'cache_container': preserve(args.out, path, '.zobj'),
                     'defined_functions': sorted(function_symbols(data[80:]))})
        objects.append(item)
    layouts = []
    for path in sorted((cache / 'partition-layout-v1').glob('*.layout')):
        if path.is_symlink() or not path.is_file():
            raise ValueError('unsafe partition layout artifact')
        data = path.read_bytes()
        if (len(data) < 76 or data[:8] != b'ZILCGU01' or data[8:40].hex() != path.stem
                or rc.sha(data[72:]) != data[40:72].hex() or int.from_bytes(data[72:76], 'little') != 16):
            raise ValueError('invalid complete partition layout frame/key/hash')
        layouts.append({'key': path.stem, 'payload_sha256': rc.sha(data[72:]), 'units': layout_units(data), **preserve(args.out, path, '.layout')})
    if not layouts:
        raise ValueError('current CGU object evidence lacks physical partition layouts')
    items = [(match[1].decode(), match[2].decode()) for match in re.finditer(rb'(?m)^(declare|define) [^\n]*?@([^ (]+)\(', llvm)]
    definitions = [index for index, (kind, name) in enumerate(items) if kind == 'define']
    matches = []
    for layout in layouts:
        units = layout['units']
        referenced = [index for unit in units for group in ('prelude', 'declarations', 'imports', 'definitions') for index in unit[group]]
        if (not items or not definitions or any(index >= len(items) for index in referenced)
                or sorted(index for unit in units for index in unit['definitions']) != definitions
                or any(unit['imports'] for unit in units)):
            continue
        if all(set(item['defined_functions']) == {items[index][1] for index in unit['definitions']}
               for item, unit in zip(objects, units)):
            matches.append(layout)
    if not matches or len({layout['payload_sha256'] for layout in matches}) != 1:
        raise ValueError('actual current object function exports do not uniquely bind a physical sixteen-unit layout/current LLVM item ownership')
    states = {name: sum(item['cache_observation'].get('state') == name for item in objects) for name in ('hit', 'miss')}
    aggregate = summary.get('object_cache') or {}
    if (sum(states.values()) != 16 or aggregate.get('hits') != states['hit'] or aggregate.get('misses') != states['miss']
            or aggregate.get('corrupt') != 0 or aggregate.get('write_failures') != 0):
        raise ValueError('actual current per-unit cache observations disagree with aggregate counts')
    result = {'object_count': len(objects), 'objects': objects, 'partition_layouts': layouts, 'compiler_summary': summary,
            'current_layout_matches': [item['key'] for item in matches],
            'current_object_report_count': len(reports), 'provenance_kind': 'current-object-reports',
            'producer_request_directory': request_directory, 'final_artifact': artifact,
            'cache_schema': 'objects-v1', 'namespace': namespace,
            'namespace_binding': 'Unique immediate hex64 namespace in a fresh private arm/block root, stable for the pinned compiler/library/target; symlinks and path escapes rejected',
            'scope': 'Current request unit receipts map exact action keys to archived objects and symbols; not the cumulative cache directory count'}
    histories = getattr(args, '_zil_producer_inventories', {})
    histories[(str(cache.resolve()), pc.llvm_comparison_sha256(llvm), executable_sha256)] = copy.deepcopy(result)
    args._zil_producer_inventories = histories
    return result


def native_outcomes(args, workloads, program, label, directory, env):
    if program.is_symlink() or not program.is_file() or not program.read_bytes().startswith(b'\x7fELF'):
        raise ValueError('refuse to execute missing/symlink/non-ELF publication')
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
    command = pc.rust_command(cargo=args.cargo, mode=args.mode, target=target)
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


def zil_request(args, workloads, fixtures, current, source, source_meta, directory, arm, workers, published_llvm, leaf, environments, cold=False):
    leaf.mkdir()
    program = directory/arm/'program'
    publication_paths = [program] + ([published_llvm[arm]] if arm in published_llvm else [])
    before_publication = publication_state(publication_paths)
    result,response = workers[arm].request(leaf)
    report = response['report']
    if arm == 'candidate' and '--push-reuse' in args.candidate_extra_arg:
        pc.inspect_report(report, mode=args.mode, cold=cold)
    valid = set(report['source_files'].values()) == {str(source/name) for name in fixtures[current]['zil']}
    if current=='error':
        valid &= report['status']=='SafeReject' and report['phase']=='types' and report['verification_runs']==0 and zil_error(report,source,fixtures[current]['zil'])
    else:
        valid &= report['status']=='Success' and report['verification_runs']==1 and report['diagnostics']==[] and sum(e=={'phase':'verify','disposition':'executed'} for e in report['query_events'])==1
    after_publication = publication_state(publication_paths)
    if current=='error':
        valid &= before_publication == after_publication
    row = {'measurement':result,'resident':response,'valid':bool(valid),'source':source_meta,
           'runtime': workers[arm].runtime,
           'typed_stage_observations': typed_stage_observations(report),
           'publication_before':before_publication,'publication_after':after_publication}
    if valid and current!='error' and args.mode=='o2':
        if report['artifacts']['executable'] != str(program):
            raise ValueError('unexpected Zil executable path')
        row['executable'] = preserve(args.out,program,'.elf')
        published_llvm[arm] = Path(report['artifacts']['llvm'])
        row['llvm'] = preserve(args.out,published_llvm[arm],'.ll')
        if '--push-reuse' in args.candidate_extra_arg:
            row['llvm']['comparison_sha256_excluding_identity_comment'] = pc.llvm_comparison_sha256((args.out / row['llvm']['archive']).read_bytes())
        row['inventory'] = zil_inventory(args,report,directory/arm/'objects', (args.out/row['llvm']['archive']).read_bytes(),
                                          row['executable']['sha256'], str(leaf.relative_to(args.out)))
        row['native'] = native_outcomes(args,workloads,program,current,leaf,environments[arm])
        row['valid'] &= all(r['valid'] for r in row['native'])
    return row


def archive_native_profile(args):
    """After the entire native matrix, retain the actual compiled-policy SDK inputs."""
    manifest = args.native_link_profile
    data = manifest.read_bytes()
    if file_hash(manifest) != args.native_link_profile_sha256:
        raise ValueError('native link manifest changed during the matrix')
    compiled = subprocess.check_output(['git', 'show', args.candidate_source+':crates/zil-backend-llvm/src/native_link.policy'], cwd=args.repo)
    if compiled != data:
        raise ValueError('actual link manifest differs from the frozen compiled policy')
    root = manifest.parent / 'root'
    file_table, directories, files = {}, set(), []
    for line in data.decode('ascii').splitlines():
        fields = line.split()
        if fields[0] == 'FILE':
            _, name, size, mode, identity = fields
            path = os.fsdecode(bytes.fromhex(name))
            if not path.startswith('/') or '..' in Path(path).parts or path in file_table:
                raise ValueError('unsafe/duplicate native SDK policy input')
            file_table[path] = (int(size), int(mode, 8), identity)
        elif fields[0] == 'DIR':
            directories.add(os.fsdecode(bytes.fromhex(fields[1])))
    actual_dirs, actual_files = {'/'}, set()
    for path in root.rglob('*'):
        if path.is_symlink():
            raise ValueError('symlink in actual native SDK root')
        name = '/'+str(path.relative_to(root))
        if path.is_dir():
            actual_dirs.add(name)
            continue
        if not path.is_file() or name not in file_table:
            raise ValueError('extra or unknown native SDK file')
        content = path.read_bytes()
        size, mode, identity = file_table[name]
        if len(content) != size or path.stat().st_mode & 0o7777 != mode or rc.sha(content) != identity:
            raise ValueError('actual SDK input mode/size/bytes differ from compiled policy')
        files.append({'path': name, 'mode': mode, **blob(args.out, content, '.sdk-input')})
        actual_files.add(name)
    if actual_files != set(file_table) or actual_dirs != directories or not files:
        raise ValueError('actual native SDK complete file/directory inventory differs')
    return {'manifest': preserve(args.out, manifest, '.native-link-policy'), 'snapshot_root': str(root),
            'files': files, 'directories': sorted(directories),
            'capture_boundary': 'Full SDK files are archived only after every measured native request; this does not prewarm the first candidate cold profile load.'}


def publication_recovery(args, workloads, fixtures, source, directory, project, target,
                         workers, published_llvm, environments, env, previous_inventory):
    """Optional correctness phase after the last measured block; never append performance rows."""
    rows = []
    for action in ('delete', 'overwrite'):
        for arm in ARMS:
            leaf = args.out/'publication-correctness'/action/arm
            leaf.parent.mkdir(parents=True, exist_ok=True)
            program = (target/'release'/rc.PACKAGE) if arm == 'rust' else directory/arm/'program'
            before = publication_state([program])
            saved = preserve(args.out, program, '.elf') if program.is_file() else None
            if action == 'delete':
                program.unlink(missing_ok=True)
            else:
                program.write_bytes(b'ZIL-OUTPUT-RECOVERY-SENTINEL\n')
            corrupted = publication_state([program])
            try:
                if arm == 'rust':
                    row = rust_request(args, workloads, fixtures, 'restore', project, target, leaf, env, previous_inventory)
                    previous_inventory = row['inventory']
                else:
                    source_meta = rc.write_files(source, fixtures['restore']['zil'], '.zil')
                    row = zil_request(args, workloads, fixtures, 'restore', source, source_meta, directory, arm,
                                      workers, published_llvm, leaf, environments)
            except (ValueError, OSError, RuntimeError, TimeoutError) as error:
                row = {'valid': False, 'error': str(error)}
            row.update({'arm': arm, 'scenario': action, 'request': 'restore', 'measured': False,
                        'publication_before_mutation': before, 'publication_after_mutation': corrupted,
                        'prior_executable': saved, 'scope': 'Separate output recovery correctness; excluded from all nine scenario timings and performance rows.'})
            leaf.mkdir(parents=True, exist_ok=True)
            rc.save(leaf/'correctness.json', row)
            rows.append(row)
    with (args.out/'publication-correctness.jsonl').open('x') as stream:
        stream.write(''.join(json.dumps(row)+'\n' for row in rows))
    return {'executed': True, 'valid': all(row['valid'] for row in rows), 'requests': len(rows),
            'rows': str(args.out/'publication-correctness.jsonl'), 'scope': 'No correctness request contributes to the nine measured cells.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--run', action='store_true')
    parser.add_argument('--mode', choices=['check', 'o2'], required=True)
    parser.add_argument('--baseline', type=Path)
    parser.add_argument('--candidate', type=Path)
    parser.add_argument('--baseline-llvm-library-path', type=Path)
    parser.add_argument('--candidate-llvm-library-path', type=Path)
    parser.add_argument('--baseline-worktree', type=Path)
    parser.add_argument('--candidate-worktree', type=Path)
    parser.add_argument('--baseline-source', default=MAIN, choices=[MAIN])
    parser.add_argument('--candidate-source')
    parser.add_argument('--tool-source')
    parser.add_argument('--blocks', type=int, choices=[2, 6], default=2)
    parser.add_argument('--native-link-profile', type=Path, default=Path('/workspace/.zil-environment/native-link-v1/manifest'))
    parser.add_argument('--native-link-profile-sha256')
    parser.add_argument('--expected-baseline-binary-sha256')
    parser.add_argument('--expected-candidate-binary-sha256')
    parser.add_argument('--expected-baseline-dso-sha256')
    parser.add_argument('--expected-candidate-dso-sha256')
    parser.add_argument('--publication-recovery', action='store_true', help='after the final native block only, separate delete/overwrite correctness; never enters the nine-scenario table')
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
    if args.publication_recovery and args.mode != 'o2':
        parser.error('publication recovery is a separate native-only correctness phase')
    # These are the complete permitted research switches; no budget/backend/opt overrides.
    try:
        candidate_flags(args.candidate_extra_arg)
    except ValueError as error:
        parser.error(str(error))
    args.repo, args.out = args.repo.resolve(), args.out.resolve()
    for arm in ['baseline', 'candidate']:
        key = arm + '_llvm_library_path'
        if getattr(args, key) is not None:
            setattr(args, key, getattr(args, key).resolve(strict=True))
    args.out.mkdir(parents=True, exist_ok=False)
    archive = args.out / 'implementation'
    archive.mkdir()
    generator = args.repo / 'tools/research/reactive-query-graph/native_workloads.py'
    native_path = generator.with_name('native_measure.py')
    implementation = [Path(__file__), Path(rc.__file__), generator, native_path,
                      Path(pc.__file__).with_name('matched_validate.py'), Path(pc.__file__),
                      Path(__file__).with_name('matched_measure.py')]
    for path in implementation:
        shutil.copyfile(path, archive / path.name)
    # native_measure imports native_workloads by name; bind the archived copy.
    sys.path.insert(0, str(archive))
    workloads = rc.load_workloads(args.repo, archive / generator.name)
    sys.modules['native_workloads'] = workloads
    native = load_module('matched_native', archive / native_path.name)
    fixtures = {}
    for label in LABELS:
        zil = workloads.snapshot('scalar', label, 20, 20, 16)
        rust = rust_files(workloads, label, 20, 20, 16)
        equality = rc.validate_equivalence(zil, rust, 20, 20, 16, label)
        directory = args.out / 'fixtures' / label
        zmeta = rc.write_files(directory/'zil', zil, '.zil')
        rmeta = rc.write_files(directory/'rust', rust, '.rs')
        (directory/'rust'/'Cargo.toml').write_text(rc.manifest_text(16))
        (directory/'rust'/'Cargo.lock').write_text(rc.lock_text())
        fixtures[label] = {'zil': zil, 'rust': rust, 'zil_metadata': zmeta, 'rust_metadata': rmeta}
        rc.save(directory/'equivalence.json', {**equality, 'zil': zmeta, 'rust': rmeta})
    manifest = {'schema': 'native-link-matched-three-arm-v2', 'arguments': {k: str(v) if isinstance(v, Path) else v for k,v in vars(args).items()},
                'fixed': {'blocks': args.blocks, 'requests': LABELS, 'modules': 20, 'functions': 20, 'steps': 16,
                          'codegen_units_requested': 16, 'codegen_jobs': 1, 'native_opt': 'O2', 'cache': 'all', 'capacity': 256},
                'implementation': {p.name: file_hash(archive/p.name) for p in implementation},
                'llvm_comparison_policy': {'purpose': 'observation-only; reviewed IR representations may differ', 'normalization': pc.LLVM_PUSH},
                'native_quality_policy': QUALITY_POLICY,
                'timing_policy': 'Sequential three-arm requests, six balanced arm permutations rotated per block/request. Zil resident send-to-response excludes startup; Rust full Cargo process includes startup. Source writes, object inventory and native execution occur outside request wall timing. No profiling. OS cache not flushed.',
                'parent_policy': 'Every non-cold request is immediately preceded for all three arms by its validated named parent: recovery<-error, restore<-private, all others<-cold. Preparations retained and validated, no cache reset within block.',
                'inventory_policy': 'Archive actual object bytes and defined symbol tables, Rust incremental metadata and inode/content/mtime observations; Zil current per-unit action-key/cache-state receipts. Hash equality is not proof of skipped work. No partition equivalence implied by requested CGU16.',
                'output_policy': 'All three arms retain successful outputs between every measured and parent request. No pre-request deletion or overwrite. Error requests must preserve prior publication, never execute it and never classify it as current success. Optional delete/overwrite recovery is after the last native block in a separate correctness file and excluded from all nine cells.',
                'native_receipt_policy': 'Complete unmodified report JSON, including typed LLVM/native/MC observations, is retained. Missing fields remain missing/null. Current Zil unit action keys bind archived complete object containers/payloads/symbols and physical layout files. Full ELF bytes are content-addressed and checked on reuse.',
                'comparison_scope': 'Current main9170, this frozen candidate and Rust 1.99.1 in one sequential window only. Cargo is a new process per request; Zil is resident and startup is separate. Rust embedded LLVM and actual mapped Zil LLVM versions are recorded independently; requested CGU16 is not identical partitioning or equal safety work. No previous-candidate timing attribution.',
                'runner_derivation': {'original_matched_measure_sha256': '9a83219bfcc725efa6d1744fa34c34afbd2af03cebce74b8b7c44ff7e8377fbd', 'original_runner_retained': 'implementation/matched_measure.py', 'legacy_validator_supports_this_schema': False},
                'memory_policy': 'Root-only RSS; resident Zil root vs Cargo root are different scopes. wait4 maxrss is largest process, not tree sum; no cross-language memory advantage.',
                'fixture_identity': {k: {lang: v[lang+'_metadata']['identity_sha256'] for lang in ['zil','rust']} for k,v in fixtures.items()},
                'started_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}
    rc.save(args.out/'manifest.json', manifest)
    if not args.run:
        print(json.dumps({'generated_only': True, 'out': str(args.out), 'compiler_commands_executed': 0}))
        return
    for key in ['baseline', 'candidate', 'baseline_worktree', 'candidate_worktree', 'candidate_source', 'baseline_build_receipt', 'candidate_build_receipt', 'baseline_llvm_library_path', 'candidate_llvm_library_path', 'tool_source', 'native_link_profile_sha256', 'expected_baseline_binary_sha256', 'expected_candidate_binary_sha256', 'expected_baseline_dso_sha256', 'expected_candidate_dso_sha256']:
        if getattr(args, key) is None:
            parser.error('--run requires --' + key.replace('_','-'))
    if len(args.candidate_source) != 40 or any(c not in '0123456789abcdef' for c in args.candidate_source):
        parser.error('candidate source must be an exact git commit')
    if subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=args.repo, text=True).strip() != args.tool_source:
        raise ValueError('exact tool source differs from frozen repo')
    for path in implementation:
        relative = path.resolve().relative_to(args.repo)
        committed = subprocess.check_output(['git', 'show', args.tool_source+':'+str(relative)], cwd=args.repo)
        if committed != path.read_bytes():
            raise ValueError('measurement tool differs from the exact independent tool commit: '+str(relative))
    if file_hash(generator) != 'df6e95b48825063b718fd65a0c4ab8afaa940f6880a40215d60ac9105ced6964' or file_hash(args.repo/'examples/mainline/resource_closures/io.zil') != 'ff0523cbcc596d96efafd5361c91d7482ca52e9f8eee8ca611a486045328d89d':
        raise ValueError('original425 fixture generator/io bytes changed')
    if file_hash(args.native_link_profile) != args.native_link_profile_sha256:
        raise ValueError('native link profile differs from explicit SHA pin')
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
        if file_hash(path) != getattr(args, 'expected_'+arm+'_binary_sha256'):
            raise ValueError(arm + ': binary differs from explicit binary SHA256 pin')
        if binding.get('source') != getattr(args,arm+'_source') or binding.get('sha256') != file_hash(path) or binding.get('exit_code') != 0 or not binding.get('command'):
            raise ValueError(arm + ': build receipt source/binary/success binding failed')
        if binding.get('tree') != manifest['sources'][arm]['tree']:
            raise ValueError(arm + ': build receipt tree differs from pinned source')
        manifest['binaries'][arm] = {'path':str(path), **preserve(args.out,path,'.compiler'), 'build_receipt':preserve(args.out,receipt,'.build-receipt')}
        if binding.get('original_receipt') is not None:
            original = binding['original_receipt']
            original_path = Path(original['path'])
            if file_hash(original_path) != original['sha256']:
                raise ValueError(arm + ': original build receipt bytes differ from immutable tree binding')
            raw = json.loads(original_path.read_text())
            if any(binding.get(key) != value for key, value in raw.items()):
                raise ValueError(arm + ': tree binding changed an original build receipt field')
            manifest['binaries'][arm]['original_build_receipt'] = preserve(args.out,original_path,'.original-build-receipt')
    environments = {arm: native.llvm_environment(getattr(args, arm + '_llvm_library_path'))
                    for arm in ['baseline', 'candidate']}
    manifest['runtime_binding_schema'] = 'per-arm-resident-llvm-v1'
    manifest['arm_runtime_environments'] = {arm: native.runtime_environment(value) for arm, value in environments.items()}
    manifest['runtime_binding_policy'] = 'Explicit per-arm LLVM directory. Actual resident maps + mapped LLVM DSO bytes archived at startup and after every request outside timing; final DSO hash recomputed without stamp cache. Configuration alone is not loading evidence. DSO archiving can warm OS file cache; no OS cold-cache claim.'
    manifest['runtime_environment'] = {key:os.environ.get(key) for key in ['ZIL_CLANG','ZIL_LLVM_CONFIG','LLVM_CONFIG','LD_LIBRARY_PATH','LD_PRELOAD','LD_AUDIT','CARGO_TARGET_DIR','RUSTUP_TOOLCHAIN']}
    manifest['host'] = {'affinity':sorted(os.sched_getaffinity(0)), 'load_average':os.getloadavg(), 'uname':list(os.uname())}
    rc.save(args.out/'manifest.json',manifest)
    rows = []
    correctness = {'executed': False, 'valid': None, 'scope': 'Output mutation requests were not requested; none enter the nine measured cells.'}
    try:
        with (args.out/'rows.jsonl').open('w') as stream:
            for block in range(args.blocks):
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
                        command = zil_command(args, getattr(args,arm), source/'main.zil', work, arm)
                        workers[arm] = Worker(command,work,args.timeout,args.sample_ms,native,environments[arm],args.out)
                        if [dso['sha256'] for dso in workers[arm].runtime['llvm_dsos']] != [getattr(args, 'expected_'+arm+'_dso_sha256')]:
                            raise ValueError(arm + ': actual mapped LLVM DSO differs from explicit SHA pin')
                        native.bind_runtime_libraries(manifest, arm, workers[arm].runtime)
                        rc.save(args.out/'manifest.json', manifest)
                    for index,label in enumerate(LABELS):
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
                                    row = zil_request(args, workloads, fixtures, current, source, source_meta, directory, arm, workers, published_llvm, leaf, environments, cold=current == 'cold' and measured)
                                row.update(common)
                                row['ended_utc_ns'] = time.time_ns()
                                pair.append(row)
                                rc.save(leaf/'request.json',row)
                                stream.write(json.dumps(row)+'\n')
                                stream.flush()
                                if not row['valid']:
                                    raise ValueError(f'invalid request {block}/{index}/{current}/{arm}')
                            pair_quality = quality(pair, args)
                            pair_quality.update({'request': current, 'measured': measured})
                            rc.save(leaf.parent/'pair-validation.json', pair_quality)
                            if not pair_quality['valid']:
                                raise ValueError('main/candidate semantics, complete executable bytes, native oracle or physical CGU quality differ; raw evidence retained for independent review')
                            rows.extend(pair)
                            print(json.dumps({'block':block,'request':current,'measured':measured,'order':order,'wall_ms':{r['arm']:r['measurement']['wall_ms'] for r in pair}}),flush=True)
                    if args.publication_recovery and block == args.blocks - 1:
                        correctness = publication_recovery(args, workloads, fixtures, source, directory, project, target,
                                                            workers, published_llvm, environments, env, previous_inventory)
                finally:
                    try:
                        for worker in workers.values():
                            if worker.process.poll() is None:
                                final = native.capture_runtime(worker.process.pid, args.out, worker.env, {})
                                rc.save(worker.directory / 'runtime-final.json', final)
                                if final['llvm_dsos'] != worker.runtime['llvm_dsos']:
                                    raise ValueError('resident LLVM DSO changed during block')
                    finally:
                        for worker in workers.values():
                            worker.close()
        if args.mode == 'o2':
            manifest['native_link_profile'] = archive_native_profile(args)
        manifest['publication_recovery'] = correctness
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
        expected_counts = (args.blocks * 51, args.blocks * 27, args.blocks * 24)
        observed_counts = (len(rows), sum(r['measured'] for r in rows), sum(not r['measured'] for r in rows))
        if observed_counts != expected_counts:
            raise ValueError('incomplete three-arm original-nine/parent matrix')
        rc.save(args.out/'completion.json',{'valid':True,'rows':len(rows),'measured':sum(r['measured'] for r in rows),'preparations':sum(not r['measured'] for r in rows), 'publication_recovery': correctness})
        if correctness['executed'] and not correctness['valid']:
            return 2
    except BaseException as error:
        rc.save(args.out/'failure.json',{'type':type(error).__name__,'message':str(error),'completed_rows':len(rows)})
        raise


if __name__=='__main__':
    raise SystemExit(main())
