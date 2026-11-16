#!/usr/bin/env python3
"""Frozen protocol: closed-project-runtime-v1 (root schedules all execution).

  python3 -B tools/research/incremental-demand/closed_project_matrix.py gen \
    --out /tmp/closed-gen
  python3 -B tools/research/incremental-demand/closed_project_matrix.py plan \
    --out /tmp/closed-plan --pins /absolute/pins.json \
    --candidate-policy closed --function-merging off
  # Review plan/manifest.json, freeze tools/compiler receipts, then root runs:
  python3 -B tools/research/incremental-demand/closed_project_matrix.py measure \
    --out /absolute/new-window --pins /absolute/pins.json \
    --candidate-policy closed --function-merging off

ONE candidate, actual main9170 and pinned Rust1.99.1, O2/CGU16/jobs1/cacheall,
default budgets. No compiler builds. No policy is enabled by default: measure
and plan require explicit closed and off. Other policies are not admitted. Candidate
alone uses push reuse, pinned native linker and the selected closed contract.
Main receives no research switches. Original large_project_matrix and its ELF
gate are imported unchanged; old failures never become this protocol's passes.

Explicit --complete-imports selects the distinct closed-import-project-runtime-v1
profile and adds the existing --incremental-o2 policy to the candidate only.
Complete transitive imports retain the unchanged compiler budget and ordinary
fallback; actual CGU16 is still mandatory. Source and runtime gates are identical.
The closed-only profile's NoGo results remain bound to their original tool SHA.

Original1000 is SHORT; original2500 is SIMPLE (both wide/100 modules).
Cold/unchanged/restore/restart source identities must match COLD_IDENTITIES,
including both languages' original formal file counts, byte counts and SHA256.
Identity is SHA256(JSON(sorted relative-path -> exact file SHA256)), using the
frozen source_metadata encoding. Every edited snapshot retains its full vector
of file hashes in source-equivalence.json; wrappers have separate identities.
Original5105 is 100x50x1, WITHOUT new unused definitions: exactly 102 files,
843875 bytes, identity 892fbc29...33627, checked before any measurement.
Sequential history: cold/unchanged, 8 (or32) private edits, unchanged, positions,
interface (new value argument and matching caller), broad, real name error,
new-body recovery, original restore, process restart retaining physical caches.
Every success requires current Verify, physical units/exports and all256 bytes
plus EOF. Every failure preserves prior publication and never executes it.
Main5105 Expand ResourceLimit is a failed observation, never a time reference;
candidate/Rust cells may qualify independently, explicitly excluding main.

Independent sustained profile: every successful request per size; only main is
wrapped, every original helper/import/io byte is retained.
10000 dynamic iterations, seed period251, checksum modulo1000003. This new
finite profile declares its own iteration count; the earlier 100000-iteration
profile is separate. Numerical
oracle evaluates a whole period independently. One warmup and six balanced
formal samples per available arm, all six A/B/R permutations. Existing gates:
B/A<=1.10, R/B>=0.90 per case, geomean R/B>=0.95. RSS and nearest-rank p95
retained; six samples cannot establish stable tails. Baseline ResourceLimit
leaves main regression acceptance UNKNOWN, never Pass. Original compile cells
remain diagnostic until the separately named runtime profile qualifies them.

All raw stdout/stderr/commands/results retained, content addressed via existing
matched blob helpers; oracle temporary directories are packed after execution.
Functional oracles always execute; there is no memoized execution witness.
Only the imports profile permits explicit --native-validation-jobs 8: independent
functional processes run after the compile timer, then join and archive in input
order. Compiler jobs and formal sustained runtime sampling remain serial. These
functional durations are not runtime-performance samples.
One shared blob/runtime store for the entire batch, not an SDK/compiler copy per
request. Source writes, hashing, native runs, disk scans and archiving are outside
compile timers. Disk uses frozen lstat/st_blocks snapshots and global inode
deduplication over explicit user roots. Peaks are observed boundaries, never
transient peaks. No OS-cache flush or same-process Rust claim. No main merge,
universal Rust lead, new numeric threshold, or historical-gate override implied.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import math
from pathlib import Path
import sys
import types

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parent))
import large_project_matrix as large
import closed_project_quality as quality

rc, matched, pc = large.rc, large.matched, large.pc
require = quality.require
ROOT = large.ROOT
SIZES = ('1000', '2500', '5105')
ORIGINAL5105 = '892fbc295a4cd51abec61c1cd600079126b1d473e1a85671f506353dfe333627'
SHAPES = {'1000': 'short', '2500': 'simple', '5105': 'short'}
# Original formal cold fixtures: large1000-edits-06 and large2500-06; original
# short5000 capacity fixture for 5105. These pins do not import old acceptance.
COLD_IDENTITIES = {
    '1000': {
        'zil': (102, 148770, 'dcb7d753d49cee5f38c4c9cee739b5df5e6e8f05304bb0e0049cc5dc824a8a58'),
        'rust': (102, 156733, '04fb2e8b2797305008649b8e00952e59b490e10f12a9d65b3b39e88f8ea4cfbd')},
    '2500': {
        'zil': (102, 209034, '01ad1fb5f38147ef90d866ad7eb067591b18c83482c826f5aac8a7ccde5b8e9b'),
        'rust': (102, 214397, '3efe3b6d11c3e12db9895c3ca9b3ae5686b5e96e32bd15619ce842f9388fe2fc')},
    '5105': {'zil': (102, 843875, ORIGINAL5105)}}


def select_preset(args, size):
    args.preset, args.shape = size, SHAPES[size]


def compose(function, **bindings):
    """Rebind this call's dependencies; never monkeypatch a frozen module."""
    return types.FunctionType(function.__code__, {**function.__globals__, **bindings},
                              function.__name__, function.__defaults__, function.__closure__)


def history(epochs):
    requests = large.history(epochs)
    index = next(i for i, r in enumerate(requests) if r['name'] == 'broad')
    requests.insert(index, dict(name='interface', epoch=epochs, broad=False, positions=False, interface=True))
    return requests


def dimensions(args):
    return (100, 50, 1) if args.preset == '5105' else large.dimensions(args)


def fixture(args, workloads, request, sustained=False):
    args.shape = 'simple' if args.preset == '2500' else 'short'
    require(args.shape == SHAPES[args.preset], 'shape must match the original formal preset')
    if args.preset == '5105':
        delta = lambda label, i, j, m, f: large.edit_delta(request, i, j, m, f)
        generate = compose(workloads.generated, delta=delta)
        label = 'error' if request['name'] == 'error' else 'cold'
        zil = generate(100, 50, 1, label, False)
        rust = rc.rust_files(types.SimpleNamespace(delta=delta), label, 100, 50, 1)
        equality = rc.validate_equivalence(zil, rust, 100, 50, 1, label)
        if request['positions']:
            for files in (zil, rust):
                for name in files:
                    files[name] = '// Source position edit; semantic body is unchanged.\n\n'+files[name]
        files = {'zil': zil, 'rust': rust}
    else:
        files, equality = large.fixture(args, workloads, request)
    if request.get('interface'):
        for lang, module, main, separator in (
                ('zil', 'm0.zil', 'main.zil', '.'),
                ('rust', 'src/m0.rs', 'src/main.rs', '::')):
            text = files[lang][module]
            require(text.count('pub fn value(x: i32) -> i32') == 1, 'one genuine public interface edit')
            files[lang][module] = text.replace('pub fn value(x: i32) -> i32', 'pub fn value(x: i32, offset: i32) -> i32')
            files[lang][module] = files[lang][module].replace('    var total: i32 = x\n' if lang == 'zil' else '    let mut total: i32 = x;\n',
                '    var total: i32 = x + offset\n' if lang == 'zil' else '    let mut total: i32 = x + offset;\n')
            call = f'm0{separator}value(x)'
            require(files[lang][main].count(call) == 1, 'one matching interface caller')
            files[lang][main] = files[lang][main].replace(call, f'm0{separator}value(x, 0)')
    identities = {lang: quality.source_metadata(source) for lang, source in files.items()}
    if request['name'] in ('cold', 'unchanged', 'restore', 'warm-restart'):
        for language, expected in COLD_IDENTITIES[args.preset].items():
            meta = identities[language]
            require((meta['source_files'], meta['source_bytes'], meta['identity_sha256']) == expected,
                    'all original'+args.preset+' '+language+' files/bytes/hash must remain unchanged')
    equality.update(source=identities, preset=args.preset, dimensions=dimensions(args),
                    shape=args.shape, interface=request.get('interface', False), request=request, sustained=sustained)
    if sustained:
        before = {lang: dict(source) for lang, source in files.items()}
        files = runtime_wrapper(files)
        equality['original_source'] = identities
        equality['source'] = {lang: quality.source_metadata(source) for lang, source in files.items()}
        equality['wrapper_only_main_changed'] = all(
            all(text == before[lang][name] for name, text in source.items()
                if name not in ('main.zil', 'src/main.rs')) for lang, source in files.items())
        require(equality['wrapper_only_main_changed'], 'all original helper bytes must survive runtime wrapping')
    return files, equality


def runtime_wrapper(files):
    result = {lang: dict(source) for lang, source in files.items()}
    for lang, name, declaration, emit in (
            ('zil', 'main.zil', '    var total: i32 = 0\n', '    io.emit'),
            ('rust', 'src/main.rs', '    let mut total: i32 = 0;\n', '    io::emit')):
        text = result[lang][name]
        require(text.count(declaration) == 1 and emit in text, 'known original scalar main wrapper site')
        if lang == 'zil':
            opening = ('    var accumulated: i32 = 0\n    var iteration: i32 = 0\n'
                       f'    while iteration < {quality.RUNTIME_GATES["iterations"]} {{\n'+declaration)
            closing = ('    accumulated = (accumulated + total) % 1000003\n'
                       '    x = (x + 1) % 251\n    iteration = iteration + 1\n'
                       '    }\n    let total: i32 = accumulated\n')
        else:
            opening = ('    let mut accumulated: i32 = 0;\n    let mut iteration: i32 = 0;\n'
                       f'    while iteration < {quality.RUNTIME_GATES["iterations"]} {{\n'+declaration)
            closing = ('    accumulated = (accumulated + total) % 1000003;\n'
                       '    x = (x + 1) % 251;\n    iteration = iteration + 1;\n'
                       '    }\n    let total: i32 = accumulated;\n')
        text = text.replace(declaration, opening, 1)
        position = text.index(emit)
        result[lang][name] = text[:position]+closing+text[position:]
    return result


class Oracle(large.Oracle):
    def __init__(self, args, workloads, request, sustained=False):
        super().__init__(args, workloads, request)
        self.sustained = sustained
        self.period = None

    def inputs(self, case):
        return (b'A',) if self.sustained else quality.INPUTS

    def scalar(self, data):
        if self.args.preset != '5105':
            return super().oracle('scalar', data, '')
        delta = lambda label, i, j, m, f: large.edit_delta(self.request, i, j, m, f)
        return compose(self.workloads.expected_generated, delta=delta)(data, 100, 50, 1, 'cold', False)

    def oracle(self, _case, data, _label, *_dimensions):
        if not self.sustained:
            return self.scalar(data)
        if self.period is None:
            self.period = [int.from_bytes(bytes.fromhex(self.scalar(bytes([i]))['stdout_hex']), 'little') for i in range(251)]
        seed = (data[0] if data else 0) % 251
        cycles, remainder = divmod(quality.RUNTIME_GATES['iterations'], 251)
        total = (cycles*sum(self.period)+sum(self.period[(seed+i) % 251] for i in range(remainder))) % 1000003
        return {'exit': 0, 'stdout_hex': total.to_bytes(4, 'little').hex(), 'stderr_hex': ''}


def packed_directory(out, directory):
    """Retain every raw byte before retiring ONLY this private temporary leaf."""
    files = {}
    for path in sorted(directory.iterdir()):
        require(path.is_file() and not path.is_symlink(), 'pack only regular evidence leaves')
        files[path.name] = matched.preserve(out, path)
    receipt = matched.blob(out, json.dumps(files, sort_keys=True).encode(), '.raw.json')
    for path in directory.iterdir():
        path.unlink()
    directory.rmdir()
    return receipt


def native_outcomes(args, oracle, program, label, directory, env):
    require(not program.is_symlink() and program.is_file() and program.read_bytes().startswith(b'\x7fELF'), 'real ELF before native execution')
    tasks = [(repeat, index, data) for repeat in range(args.native_repetitions)
             for index, data in enumerate(oracle.inputs('scalar'))]
    def execute(task):
        repeat, index, data = task
        leaf = directory/f'native-{repeat:02d}-{index:03d}'
        result = rc.run_process([str(program)], directory, leaf, args.native_timeout, args.sample_ms, data, env)
        expected = oracle.oracle('scalar', data, label)
        actual = {'exit': result['returncode'], 'stdout_hex': (leaf/'stdout.bin').read_bytes().hex(),
                  'stderr_hex': (leaf/'stderr.bin').read_bytes().hex()}
        return leaf, {'stdin_hex': data.hex(), 'expected': expected, 'actual': actual,
                      'valid': not result['timed_out'] and expected == actual, 'measurement': result}
    jobs = 1 if getattr(oracle, 'sustained', False) else getattr(args, 'native_validation_jobs', 1)
    if jobs == 1:
        results = [execute(task) for task in tasks]
    else:
        # Only functional oracles overlap, after the compile timer has stopped.
        # Every process owns its result directory and wait4 PID. map preserves
        # the complete declared input order; exit joins all work before another
        # compile or any formal sustained-runtime sample can start.
        with ThreadPoolExecutor(max_workers=jobs) as pool:
            results = list(pool.map(execute, tasks))
    outcomes = []
    # Hash-addressed archive writes remain serialized, including equal blobs.
    for leaf, result in results:
        result['raw'] = packed_directory(args.out, leaf)
        outcomes.append(result)
    return outcomes


def zil_command(args, arm, source, work):
    command = pc.zil_command(binary=getattr(args, arm), mode='o2', source=source,
                             work=work, arm=arm, extras=['--push-reuse'])
    if arm == 'candidate':
        require(args.candidate_policy in ('closed', 'late') and args.function_merging in ('off', 'after', 'before-and-after'), 'explicit candidate policy')
        command += ['--native-link-profile', str(args.native_link_profile),
                    '--closed-executable' if args.candidate_policy == 'closed' else '--late-closed-executable',
                    '--function-merging', args.function_merging]
        if getattr(args, 'complete_imports', False):
            command += ['--incremental-o2']
    return command


def implementation_paths(repo):
    return [*large.implementation_paths(repo), Path(__file__), Path(quality.__file__), Path(quality.late.__file__),
            repo/'validation/product-gates.json']


def initialize(args, manifest):
    pins = manifest['pins']
    # The existing admission verifies the frozen source/tree/build receipt,
    # binary, version, native profile and complete implementation of old tools.
    admission = types.SimpleNamespace(**{**vars(args), 'candidate_policy': 'push'})
    env = large.clean_environment()
    manifest.update(large.pin_inputs(admission, pins, env))
    for name in ('baseline', 'candidate', 'baseline_source', 'candidate_source', 'cargo', 'rustc', 'nm',
                 'native_link_profile', 'native_link_profile_sha256'):
        setattr(args, name, getattr(admission, name))
    # Also freeze this separately named protocol, not just its imported tools.
    for path in implementation_paths(args.repo):
        relative = path.resolve().relative_to(args.repo)
        frozen = large.subprocess.check_output(['git', 'show', pins['tool_source']+':'+str(relative)], cwd=args.repo)
        require(path.read_bytes() == frozen, 'new protocol implementation differs from frozen tool source: '+str(relative))
    manifest['candidate_policy'] = args.candidate_policy
    manifest['function_merging'] = args.function_merging
    manifest['complete_imports'] = args.complete_imports
    manifest['runtime_gate_authority'] = {
        'product': 'validation/product-gates.json:runtime',
        'regression_and_sampling_precedent': 'goal:records/compiler-multi-edit-continuation.json:482-509',
        'regression_precedent': 'goal:records/compiler-next-optimization.json:216-244'}
    return env


def start_worker(args, native, environments, arm, source, user, directory):
    directory.mkdir(parents=True)
    worker = matched.Worker(zil_command(args, arm, source/'main.zil', user/arm), directory,
                            args.timeout, args.sample_ms, native, environments[arm], args.out)
    try:
        require([d['sha256'] for d in worker.runtime['llvm_dsos']] ==
                [args.pins_data[arm]['llvm_dso_sha256']], 'actual mapped LLVM DSO pin')
    except Exception:
        worker.close()
        raise
    return worker


def close_worker(args, native, worker):
    try:
        require(worker.process.poll() is None, 'resident died before final identity check')
        final = native.capture_runtime(worker.process.pid, args.out, worker.env, {})
        rc.save(worker.directory/'runtime-final.json', final)
        require(final['llvm_dsos'] == worker.runtime['llvm_dsos'], 'mapped runtime changed at final rehash')
    finally:
        worker.close()


def slim(row, out):
    raw = matched.blob(out, json.dumps(row, sort_keys=True).encode(), '.row.json')
    keep = ('arm', 'preset', 'block', 'index', 'request', 'valid', 'main_resource_limit', 'measurement',
            'source_identity_sha256', 'disk_before', 'disk_after', 'failure', 'directory')
    return {**{key: row[key] for key in keep if key in row}, 'raw': raw}


def runtime_samples(args, rows, oracle, leaf, environments, env):
    by_arm = {r['arm']: r for r in rows}
    main_limit = quality.resource_limit(by_arm['baseline'])
    active = ('candidate', 'rust') if main_limit else matched.ARMS
    samples = {a: [] for a in active}
    # native_outcomes already executed exactly one warmup/arm for this wrapper.
    require(args.native_repetitions == 1 and all(len(by_arm[a]['native']) == 1 for a in active), 'one wrapper warmup per available arm')
    expected = oracle.oracle('scalar', b'A', '')
    for repeat, order in enumerate(matched.ORDERS):
        for arm in order:
            if arm not in active:
                continue
            row = by_arm[arm]
            # Execute the exact published path; archived ELF is rechecked too.
            matched.recheck_archived(args.out, row['executable'])
            program = (Path(row['resident']['report']['artifacts']['executable']) if arm != 'rust'
                       else Path(next(iter(row['publication_after']))))
            require(matched.file_hash(program) == row['executable']['sha256'], 'runtime publication identity')
            directory = leaf/f'runtime-{repeat:02d}-{arm}'
            result = rc.run_process([str(program)], leaf, directory, args.native_timeout, args.sample_ms, b'A',
                                    env if arm == 'rust' else environments[arm])
            actual = {'exit': result['returncode'], 'stdout_hex': (directory/'stdout.bin').read_bytes().hex(),
                      'stderr_hex': (directory/'stderr.bin').read_bytes().hex()}
            result.update(valid=not result['timed_out'] and actual == expected, expected=expected, actual=actual,
                          order=order, repeat=repeat, raw=packed_directory(args.out, directory))
            samples[arm].append(result)
    return {'case': args.preset+'/'+oracle.request['name'], 'main_resource_limit': main_limit,
            'iterations': quality.RUNTIME_GATES['iterations'], 'samples': samples}


def restarts_worker(request):
    return request['name'] == 'warm-restart'


def run_window(args, workloads, native, env, environments, sustained=False):
    requests = history(args.epochs)
    if sustained:
        requests = [r for r in requests if r['name'] != 'error']
    args.native_repetitions = 1
    prefix = args.out/('runtime-profile' if sustained else 'workflow')/args.preset
    rows, pairs, runtime = [], [], []
    zil_request = compose(matched.zil_request, native_outcomes=native_outcomes, zil_inventory=quality.inventory)
    rust_request = compose(matched.rust_request, native_outcomes=native_outcomes)
    for block in range(1 if sustained else args.blocks):
        base = prefix/f'block-{block:02d}'
        source, project, user = base/'sources'/'zil', base/'sources'/'rust', base/'user'
        project.mkdir(parents=True)
        (project/'Cargo.toml').write_text(rc.manifest_text(16))
        (project/'Cargo.lock').write_text(rc.lock_text())
        for arm in matched.ARMS[:2]:
            (user/arm).mkdir(parents=True)
        initial, _ = fixture(args, workloads, requests[0], sustained)
        rc.write_files(source, initial['zil'], '.zil')
        workers, previous, published = {}, {}, {}
        try:
            for arm in matched.ARMS[:2]:
                workers[arm] = start_worker(args, native, environments, arm, source, user, base/'resident'/arm/'0')
            for index, request in enumerate(requests):
                name = request['name']
                files, equivalence = fixture(args, workloads, request, sustained)
                source_meta = rc.write_files(source, files['zil'], '.zil')
                leaf = base/'requests'/f'{index:03d}-{name}'
                leaf.mkdir(parents=True)
                rc.save(leaf/'source-equivalence.json', equivalence)
                if restarts_worker(request):
                    for arm in matched.ARMS[:2]:
                        close_worker(args, native, workers.pop(arm))
                        workers[arm] = start_worker(args, native, environments, arm, source, user, base/'resident'/arm/'1')
                oracle = Oracle(args, workloads, request, sustained)
                triplet = []
                for arm in matched.ORDERS[(block+index) % 6]:
                    directory = leaf/arm
                    before = large.disk_snapshot(large.disk_roots(user))
                    try:
                        if arm == 'rust':
                            row = rust_request(args, oracle, {name: files}, name, project, user/'rust-target', directory, env, previous)
                            previous = row['inventory']
                        else:
                            row = zil_request(args, oracle, {name: files}, name, source, source_meta, user, arm,
                                              workers, published, directory, environments, cold=index == 0)
                    except Exception as error:
                        row = {'valid': False, 'failure': type(error).__name__+': '+str(error)}
                    row.update(arm=arm, block=block, index=index, request=name, preset=args.preset,
                               directory=str(directory.relative_to(args.out)), source_identity_sha256=source_meta['identity_sha256'],
                               disk_before=before, disk_after=large.disk_snapshot(large.disk_roots(user)))
                    row['main_resource_limit'] = args.preset == '5105' and quality.resource_limit(row)
                    # A failed main5105 still has its current source map checked.
                    if row['main_resource_limit']:
                        require(set(row['resident']['report']['source_files'].values()) == {str(source/n) for n in files['zil']}, 'ResourceLimit complete original source map')
                    triplet.append(row)
                    saved = slim(row, args.out)
                    rows.append(saved)
                    with (args.out/'rows.jsonl').open('a') as stream:
                        stream.write(json.dumps(dict(saved, sustained=sustained))+'\n')
                    rc.save(directory/'closed-request.json', saved)
                try:
                    pair = quality.quality(triplet, args, oracle, allow_main_limit=args.preset == '5105')
                except Exception as error:
                    pair = {'valid': False, 'error': type(error).__name__+': '+str(error)}
                pair.update(request=name, preset=args.preset, block=block, index=index)
                pairs.append(pair)
                rc.save(leaf/'quality.json', pair)
                require(pair['valid'], 'new protocol functional quality failed: '+str(pair))
                print(json.dumps({'preset': args.preset, 'block': block, 'request': name,
                                  'sustained': sustained, 'functional_quality_valid': True,
                                  'wall_ms': {r['arm']: r.get('measurement', {}).get('wall_ms') for r in triplet}}), flush=True)
                if sustained:
                    record = runtime_samples(args, triplet, oracle, leaf, environments, env)
                    record['original_source'] = equivalence['original_source']
                    record['wrapped_source'] = equivalence['source']
                    runtime.append(record)
                    rc.save(leaf/'sustained-runtime.json', record)
        finally:
            errors = []
            for worker in workers.values():
                try:
                    close_worker(args, native, worker)
                except Exception as error:
                    errors.append(str(error))
            require(not errors, 'resident finalization: '+str(errors))
    return rows, pairs, runtime


def disk_summary(rows):
    result = []
    for size in SIZES:
        for block in sorted({r['block'] for r in rows if r['preset'] == size}):
            selected = [r for r in rows if r['preset'] == size and r['block'] == block]
            categories = sorted({c for r in selected for c in r['disk_after']['categories']})
            boundaries = {r['request']: r['disk_after'] for r in selected}
            first, last = selected[0]['disk_before'], selected[-1]['disk_after']
            def count(snapshot, category, metric):
                return snapshot['categories'].get(category, {}).get(metric, 0)
            result.append({'preset': size, 'block': block, 'categories': {c: {
                'observed_boundary_peak': {m: max(count(r[edge], c, m) for r in selected for edge in ('disk_before', 'disk_after')) for m in large.METRICS},
                'growth_from_empty': {m: count(last, c, m)-count(first, c, m) for m in large.METRICS},
                'growth_from_cold': {m: count(last, c, m)-count(boundaries['cold'], c, m) for m in large.METRICS},
                'growth_over_private_edits': {m: count(boundaries['unchanged-after-edits'], c, m)-count(boundaries['cold'], c, m) for m in large.METRICS},
                'restart_delta': {m: count(last, c, m)-count(boundaries['restore'], c, m) for m in large.METRICS}}
                for c in categories}, 'transient_peak': None})
    return result


def report(args, manifest, rows, pairs, runtime):
    expected = len(args.sizes)*args.blocks*len(history(args.epochs))
    require(len(rows) == expected*3 and len(pairs) == expected and all(p['valid'] for p in pairs),
            'complete predeclared workflow; partial cells never qualify')
    require(len(runtime) == len({case['case'] for case in runtime}), 'unique named runtime cases')
    for case in runtime:
        size, request = case['case'].split('/')
        originals = {r['source_identity_sha256'] for r in rows if r['preset'] == size and r['request'] == request}
        require(originals == {case['original_source']['zil']['identity_sha256']},
                'runtime case must bind exact original workflow source identity')
    decision = quality.runtime_decision(runtime)
    decision['schema'] = quality.schema(args)
    comparisons, totals = [], []
    for size in args.sizes:
        relevant = [case for case in decision['cases'] if case['case'].startswith(size+'/')]
        require({c['case'] for c in relevant} == {size+'/'+r['name'] for r in history(args.epochs) if r['name'] != 'error'},
                'every successful request needs its own sustained runtime case')
        rust_accepted = decision['candidate_rust_accepted']
        main_accepted = rust_accepted and all(c['main_gate_pass'] is True for c in relevant)
        for request in history(args.epochs):
            selected = [r for r in rows if r['preset'] == size and r['request'] == request['name']]
            main_failed = any(r.get('main_resource_limit') for r in selected)
            arms = ('candidate', 'rust') if main_failed else matched.ARMS
            case = next((c for c in relevant if c['case'] == size+'/'+request['name']), None)
            accepted = (None if case is None else rust_accepted and case['rust_gate_pass']
                        and (main_failed or case['main_gate_pass'] is True))
            comparison = {'preset': size, 'request': request['name'], 'main_role': 'ResourceLimit; excluded from ratios' if main_failed else 'actual main9170',
                          'runtime_quality_accepted': accepted,
                          'runtime_case': case['case'] if case else 'not applicable: no executable on error', 'comparison_arms': arms,
                          'observed_compile_samples': quality.paired_comparison(selected, args.blocks, arms)}
            comparisons.append(comparison)
        workflow = [r for r in rows if r['preset'] == size]
        main_failed = any(r.get('main_resource_limit') for r in workflow)
        arms = ('candidate', 'rust') if main_failed else matched.ARMS
        summed = []
        for arm in arms:
            for block in range(args.blocks):
                selected = [r for r in workflow if r['arm'] == arm and r['block'] == block]
                require(len(selected) == len(history(args.epochs)) and all(r['valid'] for r in selected), 'complete workflow waiting totals')
                summed.append({'arm': arm, 'block': block, 'valid': True,
                               'measurement': {'wall_ms': sum(r['measurement']['wall_ms'] for r in selected)}})
        totals.append({'preset': size, 'runtime_quality_accepted': rust_accepted if main_failed else main_accepted,
                       'main_role': 'ResourceLimit; excluded from ratios' if main_failed else 'actual main9170',
                       'frequency_policy': 'each request occurs exactly once in the declared sequential workflow; no reweighting',
                       **quality.paired_comparison(summed, args.blocks, arms)})
    summary = {'schema': quality.schema(args), 'execution_complete': True, 'functional_quality_valid': True,
               'accepted': decision['accepted'], 'candidate_rust_accepted': decision['candidate_rust_accepted'],
               'main_regression_accepted': decision['main_regression_accepted'], 'runtime_profile': decision,
               'comparisons': [c for c in comparisons if c['runtime_quality_accepted']],
               'unaccepted_observations': [c for c in comparisons if not c['runtime_quality_accepted']],
               'workflow_waiting_totals': totals,
               'disk_boundaries': disk_summary(rows), 'rows': len(rows), 'quality_cells': len(pairs)}
    rc.save(args.out/'summary.json', summary)
    manifest.update(execution_complete=True, accepted=decision['accepted'], candidate_rust_accepted=decision['candidate_rust_accepted'])
    rc.save(args.out/'manifest.json', manifest)
    rc.save(args.out/'completion.json', {k: summary[k] for k in ('execution_complete', 'accepted', 'candidate_rust_accepted', 'rows', 'quality_cells')})
    return 0 if decision['candidate_rust_accepted'] and all(c['main_gate_pass'] is not False for c in decision['cases']) else 2


def generate(args, workloads, manifest):
    for size in args.sizes:
        select_preset(args, size)
        requests = history(args.epochs)
        for request in requests:
            files, equality = fixture(args, workloads, request)
            oracle = Oracle(args, workloads, request)
            equality['oracles'] = ([] if request['name'] == 'error' else
                [dict(stdin_hex=data.hex(), **oracle.oracle('scalar', data, '')) for data in quality.INPUTS])
            rc.save(args.out/'recipes'/size/(request['name']+'.json'), equality)
            if request['name'] == 'cold':
                for lang, source in files.items():
                    rc.write_files(args.out/'fixtures'/size/lang, source, '.zil' if lang == 'zil' else '.rs')
        for request in requests:
            if request['name'] != 'error':
                files, equality = fixture(args, workloads, request, True)
                rc.save(args.out/'runtime-recipes'/size/(request['name']+'.json'), equality)
                for lang, source in files.items():
                    name = 'main.zil' if lang == 'zil' else 'src/main.rs'
                    path = args.out/'runtime-wrappers'/size/request['name']/name
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text(source[name])
    manifest['generated_only'] = True
    rc.save(args.out/'manifest.json', manifest)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('gen', 'plan', 'measure'))
    parser.add_argument('--repo', type=Path, default=ROOT)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--pins', type=Path)
    parser.add_argument('--sizes', nargs='+', choices=SIZES, default=list(SIZES))
    parser.add_argument('--candidate-policy', choices=('closed', 'late'))
    parser.add_argument('--function-merging', choices=('off', 'after', 'before-and-after'))
    parser.add_argument('--complete-imports', action='store_true', help='explicit distinct profile using existing bounded incremental O2 imports')
    parser.add_argument('--native-validation-jobs', type=int, choices=(1, 8), default=1,
                        help='functional oracles only; compilation and formal runtime samples remain serial')
    parser.add_argument('--epochs', type=int, choices=(8, 32), default=8)
    parser.add_argument('--blocks', type=int, choices=(2, 6), default=2)
    parser.add_argument('--cargo', default='cargo')
    parser.add_argument('--rustc', default='rustc')
    parser.add_argument('--nm', default='nm')
    parser.add_argument('--timeout', type=float, default=900)
    parser.add_argument('--native-timeout', type=float, default=120)
    parser.add_argument('--sample-ms', type=float, default=5)
    args = parser.parse_args(argv)
    require(args.native_validation_jobs == 1 or args.complete_imports,
            'parallel functional validation is explicit and limited to the distinct imports profile')
    require(len(args.sizes) == len(set(args.sizes)), 'unique declared size cells')
    require(all(math.isfinite(v) and v > 0 for v in (args.timeout, args.native_timeout, args.sample_ms)), 'positive finite timing limits')
    if args.action != 'gen':
        require(args.pins and args.candidate_policy and args.function_merging, 'explicit pins, candidate-policy and function-merging required')
        require(args.candidate_policy == 'closed' and args.function_merging == 'off',
                'this frozen runtime protocol admits only closed executable with function merging off')
    args.repo, args.out = args.repo.resolve(), args.out.absolute()
    require(not args.out.exists() and not args.out.is_symlink() and '..' not in args.out.parts, 'new output directory required')
    args.mode, args.modules, args.graph = 'o2', 100, 'wide'
    args.candidate_extra_arg, args.native_repetitions = ['--push-reuse'], 1
    args.out.mkdir(parents=True)
    implementations = implementation_paths(args.repo)
    manifest = {'schema': quality.schema(args), 'arguments': {k: str(v) if isinstance(v, Path) else v for k, v in vars(args).items()},
                'history': history(args.epochs), 'runtime_gates': quality.RUNTIME_GATES,
                'preset_shapes': SHAPES, 'original_cold_identities': COLD_IDENTITIES,
                'expected_workflow_rows': len(args.sizes)*args.blocks*len(history(args.epochs))*3,
                'expected_runtime_cases': len(args.sizes)*(len(history(args.epochs))-1),
                'implementation_sha256': {str(p.relative_to(args.repo)): matched.file_hash(p) for p in implementations},
                'budgets': 'unchanged compiler defaults; ResourceLimit is failure',
                'quality_policy': quality.__doc__, 'protocol_freeze': __doc__, 'accepted': False}
    for path in implementations:
        matched.preserve(args.out, path, '.implementation')
    workloads = rc.load_workloads(args.repo)
    if args.action in ('gen', 'plan'):
        if args.pins:
            manifest['pins'] = json.loads(args.pins.read_text())
            large.controller.validate_pins(manifest['pins'])
            manifest['run_argv'] = [sys.executable, '-B', str(Path(__file__).resolve()), 'measure',
                '--repo', str(args.repo), '--out', str(args.out.with_name(args.out.name+'-run')),
                '--pins', str(args.pins.absolute()), '--candidate-policy', args.candidate_policy,
                '--function-merging', args.function_merging, '--native-validation-jobs', str(args.native_validation_jobs),
                '--blocks', str(args.blocks), '--epochs', str(args.epochs),
                '--sizes', *args.sizes, '--cargo', args.cargo, '--rustc', args.rustc, '--nm', args.nm,
                '--timeout', str(args.timeout), '--native-timeout', str(args.native_timeout), '--sample-ms', str(args.sample_ms)]
            if args.complete_imports:
                manifest['run_argv'] += ['--complete-imports']
        generate(args, workloads, manifest)
        print(json.dumps({'prepared_only': True, 'compiler_processes_started': 0, 'manifest': str(args.out/'manifest.json')}))
        return 0
    manifest['pins'] = args.pins_data = json.loads(args.pins.read_text())
    rows, pairs, runtime = [], [], []
    try:
        env = initialize(args, manifest)
        rc.save(args.out/'manifest.json', manifest)
        sys.modules['native_workloads'] = workloads
        native = matched.load_module('closed_project_native_runtime', args.repo/'tools/research/reactive-query-graph/native_measure.py')
        environments = {a: native.llvm_environment(args.pins_data[a]['llvm_library_path'], env) for a in matched.ARMS[:2]}
        for size in args.sizes:
            select_preset(args, size)
            current_rows, current_pairs, _ = run_window(args, workloads, native, env, environments)
            rows.extend(current_rows); pairs.extend(current_pairs)
        for size in args.sizes:
            select_preset(args, size)
            _, _, cases = run_window(args, workloads, native, env, environments, sustained=True)
            runtime.extend(cases)
        for arm in matched.ARMS[:2]:
            require(matched.file_hash(args.pins_data[arm]['binary']) == args.pins_data[arm]['binary_sha256'], 'compiler changed during window')
        for path in implementations:
            require(matched.file_hash(path) == manifest['implementation_sha256'][str(path.relative_to(args.repo))], 'measurement implementation changed during window')
        manifest['native_profile_archive'] = matched.archive_native_profile(args)
        return report(args, manifest, rows, pairs, runtime)
    except Exception as error:
        failure = {'error': type(error).__name__+': '+str(error)}
        manifest.update(execution_complete=False, accepted=False, failure=failure)
        rc.save(args.out/'manifest.json', manifest)
        rc.save(args.out/'failure.json', failure)
        rc.save(args.out/'summary.json', {'schema': quality.schema(args), 'accepted': False, 'execution_complete': False,
                                        'failure': failure, 'comparisons': [], 'raw_rows': 'rows.jsonl'})
        rc.save(args.out/'completion.json', {'accepted': False, 'execution_complete': False, 'failure': failure})
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
