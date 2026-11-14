#!/usr/bin/env python3
"""Generate or measure ONE scalar size/graph/shape window; never build a compiler.

Examples (each output must be new; measurement is an explicit separate action):
  python3 -B tools/research/incremental-demand/large_project_matrix.py gen \
    --out /tmp/large-gen --preset 2500 --shape short --epochs 8
  python3 -B tools/research/incremental-demand/large_project_matrix.py measure \
    --out /tmp/large-check --preset 1000 --shape short --epochs 32 \
    --mode check --pins /absolute/frozen-pins.json
Repeat measure with --mode o2 and a NEW output. Root schedules the exclusive
execution window. This script never changes limits, compiler sources or pins.
--candidate-policy push (default) opts only the candidate into push reuse and
the O2 native-link profile. stock sends neither flag and uses the same pinned
stock LLVM DSO as main. Each window still has exactly ONE candidate.

short is ONE BRANCH, not straight-line code. simple is explicitly one arithmetic
expression. small425/wide/branchy cold is the original 20x20x16 scalar source.
Larger cases keep >=100 modules, including unused definitions; error injects an
unknown name into an unused definition. No source/import closure is trimmed.
The history is sequential, not the fixed nine-cell runner's parent protocol.

Disk is measured outside compile timers at request boundaries, over EXPLICIT
user roots only. Peaks are observed boundary peaks, NOT transient build peaks.
Source, SDK, runtime maps, evidence, nm/native logs and fixture archives are not
user-cache roots. Disk scans may warm OS caches; no OS cold-cache claim is made.
Failure (including ResourceLimit/Unsupported) retains all three arms' evidence,
stops this window, returns 2 and emits NO performance/disk comparison summary.
"""
import argparse
import json
import math
import os
from pathlib import Path
import re
import shutil
import stat
import statistics
import subprocess
import sys
import types

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parent))
import rust_compare as rc
import push_counters as pc
import native_link_matched_measure as matched
import native_link_matched_controller as controller

ROOT = Path(__file__).resolve().parents[3]
ARMS = ('baseline', 'candidate', 'rust')
BRANCH = r'if y % 3 == 0 \{ y = \(y \* 2 \+ (\d+)\) % 251;? \} else \{ y = \(y \+ (\d+)\) % 251;? \}'
METRICS = ('logical_bytes', 'physical_bytes', 'files', 'unique_files', 'directories', 'symlinks', 'path_logical_bytes')


def require(value, message):
    if not value:
        raise ValueError(message)


def dimensions(args):
    if args.preset == 'small425':
        return 20, 20, 16 if args.shape == 'branchy' else 1
    modules = args.modules
    require(modules >= 100, 'large presets require at least 100 modules')
    # Each module also has value() and one deliberately unused definition.
    helpers = max(1, math.ceil((int(args.preset)-5)/modules)-2)
    return modules, helpers, 16 if args.shape == 'branchy' else 1


def graph(modules, kind):
    if kind == 'wide':
        return [[] for _ in range(modules)], list(range(modules))
    if kind == 'chain':
        return [[i+1] if i+1 < modules else [] for i in range(modules)], [0]
    require(modules >= 4, 'diamond requires at least four modules')
    # One real diamond followed by a chain: each downstream value executes
    # twice, not exponentially many times in a many-layer two-wide DAG.
    edges = [[i+1] if i+1 < modules else [] for i in range(modules)]
    edges[0], edges[1], edges[2] = [1, 2], [3], [3]
    return edges, [0]


def history(epochs):
    def request(name, epoch=0, broad=False, positions=False):
        return dict(name=name, epoch=epoch, broad=broad, positions=positions)
    return ([request('cold'), request('unchanged')]
            + [request(f'private-{i:03d}', i) for i in range(1, epochs+1)]
            + [request('unchanged-after-edits', epochs), request('positions', epochs, positions=True),
               request('broad', broad=True), request('error', broad=True),
               request('recovery', epochs+1, broad=True), request('restore'), request('warm-restart')])


def edit_delta(request, i, j, _modules, _functions):
    return int(request['broad']) + (request['epoch'] if i == j == 0 else 0)


def fixture(args, workloads, request):
    modules, helpers, steps = dimensions(args)
    delta = lambda label, i, j, m, f: edit_delta(request, i, j, m, f)
    # Rebind only this generator function's globals; never monkeypatch a shared
    # module or alter the original fixed workload/oracle implementation.
    generate = types.FunctionType(workloads.generated.__code__,
                                  {**vars(workloads), 'delta': delta}, argdefs=workloads.generated.__defaults__)
    unused = args.preset != 'small425'
    label = 'error' if request['name'] == 'error' and not unused else 'cold'
    zil = generate(modules, helpers, steps, label, False)
    rust = rc.rust_files(types.SimpleNamespace(delta=delta), label, modules, helpers, steps)
    equality = rc.validate_equivalence(zil, rust, modules, helpers, steps, label)
    edges, roots = graph(modules, args.graph)
    for i in range(modules):
        zn, rn = f'm{i}.zil', f'src/m{i}.rs'
        if args.shape == 'simple':
            for files, name, language in ((zil, zn, 'zil'), (rust, rn, 'rust')):
                pattern = r'fn h(\d+)\(x: i32\) -> i32 \{.*?\n\}\n'
                def straight(match):
                    j = int(match[1])
                    value = 'absent_name' if label == 'error' and i == j == 0 else f'(x + {(i+j) % 13+1+delta(label, i, j, modules, helpers)}) % 251'
                    return f'fn h{j}(x: i32) -> i32 {{ return {value}'+(';' if language == 'rust' else '')+' }\n'
                files[name], count = re.subn(pattern, straight, files[name], flags=re.S)
                require(count == helpers, 'complete simple helper inventory')
        zil[zn] = ''.join(f'import m{child}\n' for child in edges[i]) + zil[zn]
        rust[rn] = ''.join(f'use crate::m{child};\n' for child in edges[i]) + rust[rn]
        zcalls = ''.join(f'    total = (total + m{child}.value(x)) % 65521\n' for child in edges[i])
        rcalls = ''.join(f'    total = (total + m{child}::value(x)) % 65521;\n' for child in edges[i])
        require(zil[zn].count('    return total\n') == rust[rn].count('    return total;\n') == 1, 'module value body')
        zil[zn] = zil[zn].replace('    return total\n', zcalls+'    return total\n')
        rust[rn] = rust[rn].replace('    return total;\n', rcalls+'    return total;\n')
        if unused:
            value = 'absent_name' if request['name'] == 'error' and i == 0 else str(i % 251)
            zil[zn] += f'fn unused_checked() -> i32 {{ return {value} }}\n'
            rust[rn] += f'fn unused_checked() -> i32 {{ return {value}; }}\n'
        # Recheck final graph and complete local definitions, including unused.
        require(re.findall(r'^import m(\d+)$', zil[zn], re.M) ==
                re.findall(r'^use crate::m(\d+);$', rust[rn], re.M) == [str(c) for c in edges[i]], 'import graph equivalence')
        require(re.findall(r'm(\d+)\.value\(x\)', zil[zn]) ==
                re.findall(r'm(\d+)::value\(x\)', rust[rn]) == [str(c) for c in edges[i]], 'cross-module call graph equivalence')
        require(re.findall(r'fn h(\d+)\(', zil[zn]) == re.findall(r'fn h(\d+)\(', rust[rn]) == [str(j) for j in range(helpers)], 'final complete helper inventory')
        require(re.findall(BRANCH, zil[zn]) == re.findall(BRANCH, rust[rn]), 'final branch coefficients')
        if args.shape == 'simple':
            require(re.findall(r'return \(x \+ (\d+)\) % 251', zil[zn]) == re.findall(r'return \(x \+ (\d+)\) % 251', rust[rn]), 'straight-line coefficients')
    for files, name, language in ((zil, 'main.zil', 'zil'), (rust, 'src/main.rs', 'rust')):
        if args.graph == 'wide':
            continue  # Preserve the original425 cold source bytes exactly.
        if language == 'zil':
            files[name] = re.sub(r'^import m\d+\n', '', files[name], flags=re.M)
            files[name] = ''.join(f'import m{i}\n' for i in roots) + files[name]
            pattern = r'^    total = \(total \+ m\d+\.value\(x\)\) % 65521\n'
            calls = ''.join(f'    total = (total + m{i}.value(x)) % 65521\n' for i in roots)
        else:
            # Rust's root module declarations retain ALL files. The matching
            # dependency edges are the use/call graph, not mod declarations.
            pattern = r'^    total = \(total \+ m\d+::value\(x\)\) % 65521;\n'
            calls = ''.join(f'    total = (total + m{i}::value(x)) % 65521;\n' for i in roots)
        files[name], count = re.subn(pattern, '', files[name], flags=re.M)
        require(count == modules, 'complete original main call inventory')
        files[name] = files[name].replace('    io.emit', calls+'    io.emit', 1) if language == 'zil' else files[name].replace('    io::emit', calls+'    io::emit', 1)
    # Require reachability of the ENTIRE module graph, never a trimmed closure.
    reached, pending = set(), list(roots)
    while pending:
        node = pending.pop()
        if node not in reached:
            reached.add(node); pending.extend(edges[node])
    require(reached == set(range(modules)), 'all declared modules must be imported')
    require(sum(text.count('absent_name') for text in zil.values()) ==
            sum(text.count('absent_name') for text in rust.values()) == int(request['name'] == 'error'), 'one real name error')
    if request['positions']:
        for files in (zil, rust):
            for name in files:
                files[name] = '// Source position edit; semantic body is unchanged.\n\n'+files[name]
    equality.update(modules=modules, helpers_per_module=helpers, shape=args.shape, steps=steps,
                    graph=args.graph, roots=roots, import_edges=edges, unused_definitions=modules if unused else 0,
                    named_declarations=modules*(helpers+1+int(unused))+5,
                    function_definitions=modules*(helpers+1+int(unused))+3,
                    bounded_arithmetic=f'0<=x,y<=250; y*2+a<={513+request["epoch"]+int(request["broad"])}; each aggregate addition is reduced modulo 65521; i32 cannot overflow',
                    small425_cold_source_control=args.preset == 'small425' and args.graph == 'wide' and args.shape == 'branchy' and request['name'] == 'cold',
                    error_scope='unused definition' if unused else 'original h0 definition')
    return {'zil': zil, 'rust': rust}, equality


class Oracle:
    """Independent integer DAG evaluation, not a parse of generated source.

    The scalar arithmetic and six inputs come from native_workloads. Wide
    branchy/short uses its existing independent interpreter directly, with this
    explicit edit delta. DAG/simple extends that integer interpreter only.
    The unused definitions never contribute a fabricated runtime value.
    """
    def __init__(self, args, workloads, request):
        self.args, self.workloads, self.request = args, workloads, request

    def inputs(self, case):
        return self.workloads.inputs(case)

    def oracle(self, _case, data, _label, *_fixed_runner_dimensions):
        require(self.request['name'] != 'error', 'a rejected request has no native oracle execution')
        modules, helpers, steps = dimensions(self.args)
        delta = lambda label, i, j, m, f: edit_delta(self.request, i, j, m, f)
        if self.args.graph == 'wide' and self.args.shape != 'simple':
            evaluate = types.FunctionType(self.workloads.expected_generated.__code__,
                                          {**vars(self.workloads), 'delta': delta}, argdefs=self.workloads.expected_generated.__defaults__)
            return evaluate(data, modules, helpers, steps, 'cold', False)
        edges, roots = graph(modules, self.args.graph)
        x = (data[0] if data else 0) % 251
        values = [0]*modules
        for i in reversed(range(modules)):
            total = x
            for j in range(helpers):
                y = x
                change = delta('', i, j, modules, helpers)
                for k in range(steps):
                    a, b = (i+j+k) % 13+1+change, (j*3+k) % 11+2
                    y = (y+a) % 251 if self.args.shape == 'simple' else (y*2+a) % 251 if y % 3 == 0 else (y+b) % 251
                total = (total+y) % 65521
            values[i] = (total+sum(values[c] for c in edges[i])) % 65521
        total = sum(values[i] for i in roots) % 65521
        return {'exit': 0, 'stdout_hex': total.to_bytes(4, 'little').hex(), 'stderr_hex': ''}


def disk_roots(directory):
    roots = []
    for arm in ARMS[:2]:
        work = directory/arm
        roots += [(arm+'.semanticcache', work/'semantic'), (arm+'.objectcache', work/'objects'),
                  (arm+'.IR', work/'program.ll'), (arm+'.exe', work/'program')]
    return roots + [('rust.target', directory/'rust-target')]


def disk_snapshot(roots):
    """lstat only; exact allocated blocks, globally unique inodes across roots.

    Logical bytes are unique FILE inode sizes. path_logical_bytes separately
    exposes aliases; it must not be used as additional user-cache storage.
    Directory blocks and symlink storage are counted, never symlink targets.
    Shared inodes are charged to the first lexicographic root/path category.
    """
    categories, seen, paths = {}, {}, set()
    for base, root in sorted(roots, key=lambda entry: (entry[0], str(entry[1]))):
        pending = [root]
        while pending:
            path = pending.pop()
            if path in paths:
                continue
            paths.add(path)
            try:
                info = path.lstat()
            except FileNotFoundError:
                continue
            category = base
            if base == 'rust.target':
                parts = path.relative_to(root).parts
                category = 'rust.'+('incremental' if 'incremental' in parts else 'deps' if 'deps' in parts else
                                    'binary' if path.name == rc.PACKAGE else 'target-other')
            count = categories.setdefault(category, dict.fromkeys(METRICS, 0))
            regular, directory, link = stat.S_ISREG(info.st_mode), stat.S_ISDIR(info.st_mode), stat.S_ISLNK(info.st_mode)
            require(regular or directory or link, 'unexpected special file in user cache: '+str(path))
            count['files'] += int(regular)
            count['symlinks'] += int(link)
            count['directories'] += int(directory)
            count['path_logical_bytes'] += info.st_size if regular else 0
            identity = (info.st_dev, info.st_ino)
            if identity not in seen:
                seen[identity] = (info.st_size, info.st_blocks, category)
                count['logical_bytes'] += info.st_size if regular else 0
                count['physical_bytes'] += info.st_blocks*512
                count['unique_files'] += int(regular)
            else:
                require(seen[identity][:2] == (info.st_size, info.st_blocks), 'file changed during disk observation')
            if directory:
                pending.extend(sorted(path.iterdir(), reverse=True))
    total = {key: sum(count[key] for count in categories.values()) for key in METRICS}
    return {'categories': categories, 'total': total, 'unique_inodes': len(seen),
            'roots': [{'category': c, 'path': str(p)} for c, p in roots],
            'scope': 'post-request boundary; lstat/st_blocks*512; globally deduplicated across explicit roots; no source/evidence/SDK traversal'}


def clean_environment():
    require(not os.environ.get('LD_PRELOAD') and not os.environ.get('LD_AUDIT'), 'loader interposition is unsupported')
    env = dict(os.environ)
    for key in list(env):
        if key in {'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
                   'CARGO_BUILD_TARGET', 'CARGO_INCREMENTAL', 'CARGO_BUILD_RUSTFLAGS', 'CARGO_TARGET_DIR',
                   'LLVM_CONFIG', 'ZIL_LLVM_CONFIG', 'LD_LIBRARY_PATH', 'ZIL_LLVM_LIB_DIR'} or key.startswith(('CARGO_PROFILE_', 'CARGO_TARGET_')):
            del env[key]
    env['ZIL_CLANG'] = '/workspace/.zil-environment/llvm-23.1.4/bin/clang'
    return env


def implementation_paths(repo):
    return [Path(__file__), Path(rc.__file__), Path(pc.__file__), Path(matched.__file__), Path(controller.__file__),
            repo/'tools/research/reactive-query-graph/native_workloads.py', repo/'tools/research/reactive-query-graph/native_measure.py']


def verify_frozen_tools(args, source):
    for path in implementation_paths(args.repo):
        relative = path.resolve().relative_to(args.repo)
        frozen = subprocess.check_output(['git', 'show', source+':'+str(relative)], cwd=args.repo)
        require(path.read_bytes() == frozen, 'implementation differs from frozen tool source: '+str(relative))


def pin_inputs(args, pins, env):
    controller.validate_pins(pins)
    verify_frozen_tools(args, pins['tool_source'])
    manifest = {'pins': pins, 'sources': {}, 'binaries': {}}
    manifest['tool_source'] = matched.source_identity(args.repo, pins['tool_source'], args.out, 'tools')
    for arm in ARMS[:2]:
        item = pins[arm]
        require(matched.file_hash(item['binary']) == item['binary_sha256'], arm+' binary SHA pin')
        receipt = json.loads(Path(item['build_receipt']).read_text())
        require(receipt.get('source') == item['source'] and receipt.get('sha256') == item['binary_sha256']
                and receipt.get('exit_code') == 0 and receipt.get('command'), arm+' successful source/binary build receipt')
        manifest['sources'][arm] = matched.source_identity(Path(item['worktree']), item['source'], args.out, arm)
        require(receipt['tree'] == manifest['sources'][arm]['tree'], arm+' build receipt tree')
        manifest['binaries'][arm] = matched.preserve(args.out, Path(item['binary']), '.compiler')
        setattr(args, arm, Path(item['binary']))
        setattr(args, arm+'_source', item['source'])
    args.native_link_profile = Path(pins['native_link_profile'])
    args.native_link_profile_sha256 = pins['native_link_profile_sha256']
    if args.candidate_policy == 'push':
        require(matched.file_hash(args.native_link_profile) == args.native_link_profile_sha256, 'native link profile SHA pin')
    else:
        require(pins['candidate']['llvm_dso_sha256'] == pins['baseline']['llvm_dso_sha256'],
                'stock candidate must use the same pinned stock LLVM DSO as main')
    manifest['candidate_policy'] = args.candidate_policy
    manifest['native_link_profile_used'] = args.candidate_policy == 'push' and args.mode == 'o2'
    manifest['native_link_profile_scope'] = 'candidate O2 only' if args.candidate_policy == 'push' else 'unused pin; stock command does not load or stat this profile'
    require(not any((parent/'.cargo'/name).exists() for parent in (args.out, *args.out.parents)
                    for name in ('config', 'config.toml')), 'ambient ancestor Cargo config')
    tools = {name: rc.bind_tool(getattr(args, name), name, args, env) for name in ('cargo', 'rustc')}
    for name in tools:
        setattr(args, name, tools[name]['actual_path'])
    args.nm = str(Path(shutil.which(args.nm) or args.nm).resolve(strict=True))
    home = args.out/'cargo-home'
    home.mkdir()
    env.update(CARGO_HOME=str(home), RUSTC=args.rustc, CARGO_INCREMENTAL='1', CARGO_BUILD_JOBS='1', CARGO_TERM_COLOR='never')
    for name, commit in (('cargo', matched.CARGO), ('rustc', matched.RUST)):
        directory = args.out/(name+'-version')
        result = rc.run_process([getattr(args, name), '-vV'], args.repo, directory, 30, args.sample_ms, env=env)
        require(result['returncode'] == 0 and not result['timed_out'] and commit in (directory/'stdout.bin').read_text(), name+' exact pinned version')
    manifest['tools'] = tools
    manifest['explicit_environment'] = {key: env.get(key) for key in
                                        ('CARGO_HOME', 'RUSTC', 'CARGO_INCREMENTAL', 'CARGO_BUILD_JOBS', 'ZIL_CLANG')}
    return manifest


def run_and_report(args, workloads, manifest):
    pins = manifest['pins']
    sys.modules['native_workloads'] = workloads
    native = matched.load_module('large_native_runtime', args.repo/'tools/research/reactive-query-graph/native_measure.py')
    env = clean_environment()
    manifest.update(pin_inputs(args, pins, env))
    environments = {arm: native.llvm_environment(pins[arm]['llvm_library_path'], env) for arm in ARMS[:2]}
    requests = history(args.epochs)
    rows, pair_records, failure = [], [], None
    context = {}
    try:
        with (args.out/'rows.jsonl').open('x') as stream:
            for block in range(args.blocks):
                user = args.out/'user'/f'block-{block:02d}'
                source, project, target = args.out/'sources'/f'block-{block:02d}'/'zil', args.out/'sources'/f'block-{block:02d}'/'rust', user/'rust-target'
                project.mkdir(parents=True)
                (project/'Cargo.toml').write_text(rc.manifest_text(16))
                (project/'Cargo.lock').write_text(rc.lock_text())
                workers, previous_inventory, published_llvm = {}, {}, {}
                for arm in ARMS[:2]:
                    (user/arm).mkdir(parents=True)
                def start(arm, generation):
                    directory = args.out/'resident'/f'block-{block:02d}'/arm/str(generation)
                    directory.mkdir(parents=True)
                    # Compose locally: the fixed matched runner is unchanged.
                    command = pc.zil_command(binary=getattr(args, arm), mode=args.mode,
                                             source=source/'main.zil', work=user/arm, arm=arm,
                                             extras=args.candidate_extra_arg)
                    if arm == 'candidate' and args.mode == 'o2' and args.candidate_policy == 'push':
                        command += ['--native-link-profile', str(args.native_link_profile)]
                    worker = matched.Worker(command, directory, args.timeout, args.sample_ms, native, environments[arm], args.out)
                    try:
                        require([entry['sha256'] for entry in worker.runtime['llvm_dsos']] == [pins[arm]['llvm_dso_sha256']], arm+' actual mapped LLVM DSO pin')
                    except Exception:
                        worker.close()
                        raise
                    return worker
                try:
                    initial, _ = fixture(args, workloads, requests[0])
                    rc.write_files(source, initial['zil'], '.zil')
                    for arm in ARMS[:2]:
                        workers[arm] = start(arm, 0)
                    for index, request in enumerate(requests):
                        current = request['name']
                        fixtures, equality = fixture(args, workloads, request)
                        source_meta = rc.write_files(source, fixtures['zil'], '.zil')
                        # Snapshot generation/hashing, source writes and disk
                        # observation all finish outside each request timer.
                        if current == 'warm-restart':
                            for arm in ARMS[:2]:
                                workers[arm].close()
                                workers[arm] = start(arm, 1)
                        leaf = args.out/'requests'/f'block-{block:02d}'/f'{index:03d}-{current}'
                        leaf.mkdir(parents=True)
                        rc.save(leaf/'equivalence.json', equality)
                        pair = []
                        for arm in matched.ORDERS[(block+index) % 6]:
                            directory = leaf/arm
                            context = {'block': block, 'index': index, 'request': current, 'arm': arm}
                            before = disk_snapshot(disk_roots(user))
                            try:
                                adapter = Oracle(args, workloads, request)
                                if arm == 'rust':
                                    row = matched.rust_request(args, adapter, {current: fixtures}, current, project, target, directory, env, previous_inventory)
                                    previous_inventory = row['inventory']
                                else:
                                    row = matched.zil_request(args, adapter, {current: fixtures}, current, source, source_meta,
                                                              user, arm, workers, published_llvm, directory, environments, cold=index == 0)
                                    if args.candidate_policy == 'stock' and 'llvm' in row:
                                        # Fixed quality() observes this field in
                                        # both policies. It never substitutes for
                                        # the full ELF/Verify/native quality gate.
                                        row['llvm']['comparison_sha256_excluding_identity_comment'] = pc.llvm_comparison_sha256(
                                            (args.out/row['llvm']['archive']).read_bytes())
                                require(row.get('valid') is True, 'request failed; limits/unsupported are not success')
                            except Exception as error:
                                row = {'valid': False, 'failure': type(error).__name__+': '+str(error),
                                       'raw_response': str(directory/'response.json') if (directory/'response.json').exists() else None}
                            after = disk_snapshot(disk_roots(user))
                            row.update(context, measured=True, mode=args.mode, transition_from=requests[index-1]['name'] if index else None,
                                       edit=request, directory=str(directory.relative_to(args.out)), source_identity_sha256=source_meta['identity_sha256'],
                                       disk_before=before, disk_after=after)
                            rc.save(directory/'large-request.json', row)
                            stream.write(json.dumps(row)+'\n'); stream.flush()
                            rows.append(row); pair.append(row)
                        try:
                            quality = matched.quality(pair, args) if all(row['valid'] for row in pair) else {'valid': False, 'scope': 'failed request triplet; not a comparison sample'}
                        except Exception as error:
                            quality = {'valid': False, 'error': type(error).__name__+': '+str(error)}
                        quality.update(block=block, index=index, request=current)
                        rc.save(leaf/'pair-validation.json', quality)
                        pair_records.append(quality)
                        require(quality['valid'] is True, 'three-arm request/native/CGU16/full-ELF quality failed')
                finally:
                    shutdown_errors = []
                    for worker in workers.values():
                        try:
                            final_runtime = native.capture_runtime(worker.process.pid, args.out, worker.env, {}) if worker.process.poll() is None else None
                            if final_runtime is not None:
                                rc.save(worker.directory/'runtime-final.json', final_runtime)
                                require(final_runtime['llvm_dsos'] == worker.runtime['llvm_dsos'], 'LLVM DSO changed at final rehash')
                        except Exception as error:
                            shutdown_errors.append(str(error))
                        finally:
                            worker.close()
                    require(not shutdown_errors, 'runtime finalization: '+str(shutdown_errors))
        # Binary/runtime/tool identities remain bound through the end, not just
        # at admission. Large ResourceLimit runs never reach valid completion.
        for arm in ARMS[:2]:
            require(matched.file_hash(pins[arm]['binary']) == pins[arm]['binary_sha256'], arm+' binary changed during window')
        for filename, identity in manifest['implementation_sha256'].items():
            candidates = [args.repo/'tools/research/incremental-demand'/filename,
                          args.repo/'tools/research/reactive-query-graph'/filename]
            require(any(path.is_file() and matched.file_hash(path) == identity for path in candidates), 'measurement implementation changed: '+filename)
        verify_frozen_tools(args, pins['tool_source'])
        if args.candidate_policy == 'push' and args.mode == 'o2':
            manifest['native_profile_archive'] = matched.archive_native_profile(args)
    except Exception as error:
        failure = {**context, 'error': type(error).__name__+': '+str(error)}
        rc.save(args.out/'failure.json', failure)
    expected = args.blocks*len(requests)*3
    valid = failure is None and len(rows) == expected and len(pair_records) == expected//3
    summary = {'valid': valid, 'mode': args.mode, 'expected_rows': expected, 'actual_rows': len(rows),
               'failure': failure, 'unexecuted_rows': expected-len(rows), 'comparisons': []}
    if valid:
        for request in requests:
            current = request['name']
            selected = {arm: [r for r in rows if r['arm'] == arm and r['request'] == current] for arm in ARMS}
            samples = {arm: [r['measurement']['wall_ms'] for r in selected[arm]] for arm in ARMS}
            require(all(len(values) == args.blocks and all(type(v) in (int, float) and math.isfinite(v) and v > 0 for v in values)
                        for values in samples.values()), 'complete positive paired request samples')
            summary['comparisons'].append({'request': current, 'wall_ms_by_block': samples,
                'median_ms': {arm: statistics.median(values) for arm, values in samples.items()},
                'candidate_over_main_paired_GM': math.exp(statistics.mean(math.log(c/b) for c, b in zip(samples['candidate'], samples['baseline']))),
                'candidate_over_Rust_paired_GM': math.exp(statistics.mean(math.log(c/b) for c, b in zip(samples['candidate'], samples['rust'])))})
        # Per-category history stays in rows. Peaks/growth are boundary-only,
        # and are never emitted as a successful comparison for a failed run.
        summary['disk_boundaries'] = []
        for block in range(args.blocks):
            history_rows = [r for r in rows if r['block'] == block]
            first, last = history_rows[0]['disk_before'], history_rows[-1]['disk_after']
            cold = [r for r in history_rows if r['request'] == 'cold'][-1]['disk_after']
            edited = [r for r in history_rows if r['request'] == 'unchanged-after-edits'][-1]['disk_after']
            restored = [r for r in history_rows if r['request'] == 'restore'][-1]['disk_after']
            categories = sorted({key for r in history_rows for key in r['disk_after']['categories']})
            summary['disk_boundaries'].append({'block': block, 'categories': {category: {
                'observed_boundary_peak': {key: max(r['disk_after']['categories'].get(category, {}).get(key, 0) for r in history_rows) for key in METRICS},
                'growth_from_empty': {key: last['categories'].get(category, {}).get(key, 0)-first['categories'].get(category, {}).get(key, 0) for key in METRICS},
                'growth_from_cold': {key: last['categories'].get(category, {}).get(key, 0)-cold['categories'].get(category, {}).get(key, 0) for key in METRICS},
                'growth_over_private_edits': {key: edited['categories'].get(category, {}).get(key, 0)-cold['categories'].get(category, {}).get(key, 0) for key in METRICS},
                'warm_restart_delta': {key: last['categories'].get(category, {}).get(key, 0)-restored['categories'].get(category, {}).get(key, 0) for key in METRICS}
            } for category in categories}, 'transient_build_peak': None})
    manifest.update(valid=valid, failure=failure)
    rc.save(args.out/'manifest.json', manifest)
    rc.save(args.out/'summary.json', summary)
    rc.save(args.out/'completion.json', {'valid': valid, 'rows': len(rows), 'pairs': len(pair_records), 'expected_rows': expected})
    return 0 if valid else 2


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('gen', 'measure'))
    parser.add_argument('--repo', type=Path, default=ROOT)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--pins', type=Path)
    parser.add_argument('--preset', choices=('small425', '1000', '2500', '5000', '10000'), default='1000')
    parser.add_argument('--modules', type=int, default=100)
    parser.add_argument('--shape', choices=('branchy', 'short', 'simple'), help='default: branchy for small425, short otherwise')
    parser.add_argument('--graph', choices=('wide', 'chain', 'diamond'), default='wide')
    parser.add_argument('--epochs', type=int, choices=(8, 32), default=8)
    parser.add_argument('--blocks', type=int, choices=(2, 6), default=2)
    parser.add_argument('--mode', choices=('check', 'o2'), default='check')
    parser.add_argument('--candidate-policy', choices=('push', 'stock'), default='push')
    parser.add_argument('--cargo', default='cargo')
    parser.add_argument('--rustc', default='rustc')
    parser.add_argument('--nm', default='nm')
    parser.add_argument('--timeout', type=float, default=900)
    parser.add_argument('--native-timeout', type=float, default=10)
    parser.add_argument('--sample-ms', type=float, default=5)
    args = parser.parse_args()
    if args.shape is None:
        args.shape = 'branchy' if args.preset == 'small425' else 'short'
    require(min(args.timeout, args.native_timeout, args.sample_ms) > 0, 'positive timeouts/sample interval')
    args.repo, args.out = args.repo.resolve(), args.out.absolute()
    require(not args.out.exists() and not args.out.is_symlink() and '..' not in args.out.parts, 'new absolute output directory required')
    dimensions(args)
    args.out.mkdir(parents=True)
    args.native_repetitions = 2
    args.candidate_extra_arg = ['--push-reuse'] if args.candidate_policy == 'push' else []
    workloads = rc.load_workloads(args.repo)
    implementation = implementation_paths(args.repo)
    for path in implementation:
        shutil.copyfile(path, args.out/('implementation-'+path.name))
    requests = history(args.epochs)
    manifest = {'schema': 'large-scalar-three-arm-disk-v1', 'generated_only': args.action == 'gen',
                'arguments': {key: str(value) if isinstance(value, Path) else value for key, value in vars(args).items()},
                'implementation_sha256': {path.name: matched.file_hash(path) for path in implementation},
                'history': requests, 'budgets': 'unchanged compiler defaults; ResourceLimit is failure',
                'quality_policy': matched.QUALITY_POLICY, 'measurement_scope': 'resident send-to-response vs new Cargo/rustc; startup separate; disk/native/archive work outside timer; OS caches not flushed',
                'disk_scope': 'explicit semantic/objects/program/program.ll and Rust target ONLY; boundary peaks; cross-root inode deduplication; evidence/source/SDK excluded'}
    cold, equality = fixture(args, workloads, requests[0])
    for language in ('zil', 'rust'):
        rc.write_files(args.out/'fixtures'/'cold'/language, cold[language], '.zil' if language == 'zil' else '.rs')
    rc.save(args.out/'equivalence.json', equality)
    rc.save(args.out/'manifest.json', manifest)
    if args.action == 'gen':
        # Generate+validate one complete snapshot at a time, not all large
        # source histories in RAM/disk. Recipes and hashes reconstruct each.
        for request in requests:
            files, equality = fixture(args, workloads, request)
            equality['source_sha256'] = {lang: {name: rc.sha(text.encode()) for name, text in sorted(source.items())} for lang, source in files.items()}
            adapter = Oracle(args, workloads, request)
            equality['native_oracles'] = [] if request['name'] == 'error' else [dict(stdin_hex=data.hex(), **adapter.oracle('scalar', data, request['name'])) for data in adapter.inputs('scalar')]
            rc.save(args.out/'recipes'/(request['name']+'.json'), equality)
        print(json.dumps({'generated_only': True, 'out': str(args.out), 'compiler_processes_started': 0, 'dimensions': dimensions(args)}))
        return 0
    require(args.pins is not None, 'measure requires explicit frozen pins')
    manifest['pins'] = json.loads(args.pins.read_text())
    try:
        return run_and_report(args, workloads, manifest)
    except Exception as error:
        # Infrastructure/analysis failures also fail closed and retain their
        # logs; they cannot leave a successful partial comparison behind.
        failure = {'error': type(error).__name__+': '+str(error)}
        rc.save(args.out/'failure.json', failure)
        rc.save(args.out/'summary.json', {'valid': False, 'failure': failure, 'comparisons': []})
        rc.save(args.out/'completion.json', {'valid': False, 'failure': failure})
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
