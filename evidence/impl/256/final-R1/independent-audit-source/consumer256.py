"""Independently replay frozen consumer256 evidence, without executing programs.

The caller verifies archive blobs and the complete Git source snapshot first.
read_bytes(path) reads a logical indexed file; source_files uses repository paths.
This module does not promote local evidence to CI or audit unrelated consumers.
"""
from collections import Counter
import hashlib
import json
from pathlib import PurePosixPath
import re

COUNTERS = ('alloc_calls', 'allocated', 'frees', 'failures', 'live', 'peak_live')


def require(ok, message):
    if not ok:
        raise ValueError('consumer256: ' + message)


def verify(index_files, read_bytes, oracle, source_files, phase_prefix, expected_counts=None):
    """Verify complete O0/O2 consumer profile in one native-evidence phase.

expected_counts optionally constrains returned integer counts and/or backend
('llvm-api' or 'clang'). Backend defaults to the actual uniform report backend;
API always requires both allocator matrices, Clang requires neither.
Pair evidence is required and counted separately. Non-pair semantic boundary
reports are listed as NotAudited, with absence of native output still checked.
"""
    expected_counts = expected_counts or {}
    phase_prefix = phase_prefix.rstrip('/') + '/'
    require(phase_prefix.startswith('native-evidence/'), 'invalid phase prefix')
    files = {e['path']: e for e in index_files}
    require(len(files) == len(index_files), 'duplicate logical file path')

    def data(path):
        require(path in files, 'missing indexed file: ' + path)
        b = read_bytes(path)
        require(isinstance(b, bytes), 'reader did not return bytes: ' + path)
        e = files[path]
        require(len(b) == e['bytes'] and hashlib.sha256(b).hexdigest() == e['sha256'],
                'indexed byte count/SHA mismatch: ' + path)
        return b

    def obj(path):
        return json.loads(data(path))

    def source(path):
        require(path in source_files and isinstance(source_files[path], bytes),
                'missing pinned source: ' + path)
        return source_files[path]

    require(json.loads(source('tests/impl256/ORACLE.json')) == oracle,
            'oracle argument differs from pinned source oracle')
    pair_expected = json.loads(source('tests/impl256/paired/EXPECTED.json'))
    frozen = {c['source']: c for c in oracle['frozen']}
    require(len(frozen) == len(oracle['frozen']) and frozen, 'duplicate/empty frozen cases')
    reports = sorted(p for p in files if p.startswith(phase_prefix)
                     and p.endswith('/report.json')
                     and not any(part.startswith('consumer256-supplement-')
                                 for part in PurePosixPath(p).parts)
                     and any(part.startswith('consumer256-') for part in PurePosixPath(p).parts))
    require(reports, 'no consumer reports in requested phase')
    counts = Counter({key: 0 for key in (
        'native_compiles', 'consumer_compiles', 'consumer_native_runs',
        'allocator_runs', 'actual_allocation_failures', 'pair_compiles',
        'pair_native_runs', 'native_runs')})
    cells = Counter()
    backends = set()
    bindings = []
    excluded = []

    def bind_sources(directory, report):
        mapping = report.get('source_files')
        require(isinstance(mapping, dict) and mapping, 'missing source_files')
        result = {}
        multiple = len(mapping) > 1
        for module, original in mapping.items():
            require(isinstance(module, str) and module.isdigit() and isinstance(original, str)
                    and '/tests/impl256/' in original, 'invalid source identity')
            suffix = original.rsplit('/tests/impl256/', 1)[1]
            require(suffix and not PurePosixPath(suffix).is_absolute()
                    and PurePosixPath(suffix).as_posix() == suffix
                    and not any(c in suffix for c in ('\\', ':', '\0', '\r', '\n'))
                    and not any(x in ('.', '..') for x in PurePosixPath(suffix).parts),
                    'unsafe source suffix')
            pinned = 'tests/impl256/' + suffix
            copy = directory + ('/module' + module + '.zil' if multiple else '/source.zil')
            require(data(copy) == source(pinned), 'source copy differs from frozen input: ' + copy)
            result[module] = suffix
        require(set(result) == {str(i) for i in range(len(result))}, 'non-contiguous source modules')
        bindings.append({'directory': directory, 'sources': result})
        return result

    def native(directory, report):
        require(report.get('status') == 'Success' and report.get('phase') == 'link'
                and report.get('verification_runs') == 1, 'build status/Verify mismatch: ' + directory)
        cg = report.get('native_codegen')
        require(isinstance(cg, dict) and cg.get('fallback') is None, 'missing codegen or fallback')
        backend = cg.get('backend')
        require(backend in ('llvm-api', 'clang'), 'unexpected backend')
        backends.add(backend)
        opts = {a for cmd in report.get('commands', []) for a in cmd if a in ('-O0', '-O2')}
        require(len(opts) == 1, 'optimization identity missing/ambiguous')
        opt = next(iter(opts))[1:]
        if backend == 'llvm-api':
            objects = cg.get('object_reports', [])
            require(type(cg.get('actual_units')) is int and cg['actual_units'] > 0
                    and len(objects) == cg['actual_units'], 'missing API object reports')
            require(all(o.get('input_kind') == 'builder' and o.get('verify_before_after') is True
                        and o.get('pipeline') == 'default<' + opt + '>' for o in objects),
                    'API object input/Verify/pipeline mismatch')
        for suffix in ('program', 'program.ll', 'phases/verified.txt'):
            require(directory + '/' + suffix in files, 'native product missing: ' + suffix)
        require(files[directory + '/program']['mode'] & 0o111, 'program executable mode lost')
        require(PurePosixPath(report['artifacts'].get('executable', '')).name == 'program'
                and PurePosixPath(report['artifacts'].get('llvm', '')).name == 'program.ll',
                'unexpected report executable/LLVM identity')
        counts['native_compiles'] += 1
        return backend, opt

    def exit_value(path):
        match = re.fullmatch(rb'Some\((-?\d+)\)\s*', data(path))
        require(match is not None, 'missing numeric exit: ' + path)
        return int(match[1])

    def execution(directory, expected, stdin, allocator):
        require(data(directory + '/stdin') == stdin, 'stdin differs from frozen oracle')
        require(obj(directory + '/expected.json') == expected,
                'recorded expectation differs from frozen oracle: ' + directory)
        require(exit_value(directory + '/exit.txt') == expected['exit'], 'exit differs from frozen oracle')
        require(data(directory + '/stdout') == expected['stdout'].encode(), 'stdout differs from frozen oracle')
        stderr = data(directory + '/stderr')
        if allocator:
            actual = json.loads(stderr)
            require(isinstance(actual, dict) and set(COUNTERS) <= set(actual)
                    and set(actual) <= set(COUNTERS) | {'peak_bytes'},
                    'allocator stderr has missing or unknown counter keys')
            # The shared probe also reports peak_bytes. The source-operation
            # oracle freezes six counters only; this seventh measurement is
            # preserved and shape-checked, not claimed as an oracle comparison.
            require('peak_bytes' not in actual or (type(actual['peak_bytes']) is int
                    and actual['peak_bytes'] >= 0), 'invalid allocator peak_bytes')
            require(all(type(actual.get(k)) is int and actual[k] >= 0
                        and actual[k] == expected[k] for k in COUNTERS),
                    'allocator six counters differ from frozen oracle')
            require(actual['live'] == 0 and actual['allocated'] == actual['frees'],
                    'allocator responsibility not balanced')
            counts['allocator_runs'] += 1
            counts['actual_allocation_failures'] += actual['failures']
        else:
            require(stderr == b'', 'unexpected stderr in ordinary execution')
        counts['consumer_native_runs'] += 1

    for path in reports:
        directory = path.rsplit('/', 1)[0]
        report = obj(path)
        parts = PurePosixPath(directory).parts
        if any(p.startswith('consumer256-negatives-') for p in parts):
            require('executable' not in report.get('artifacts', {})
                    and directory + '/program' not in files,
                    'boundary rejection produced executable')
            excluded.append({'path': path, 'status': 'NotAudited',
                             'scope': 'No executable checked; semantic rejection reason not audited here.'})
            continue
        sources = bind_sources(directory, report)
        main = sources['0']
        pair = any(p.startswith('consumer256-pair-') for p in parts)
        if pair:
            require(len(sources) == 1 and main in (
                'paired/' + pair_expected['positive']['source'],
                'paired/' + pair_expected['negative']['source']), 'unknown pair source')
            counts['pair_compiles'] += 1
            if main == 'paired/' + pair_expected['negative']['source']:
                require(report.get('status') == pair_expected['negative']['expected_category']
                        and report.get('verification_runs') == 0
                        and any(d.get('category') == 'SafeReject' and d.get('phase') == 'expand'
                                and d.get('code') == 'E-GENERIC-ARGUMENT'
                                and 'FnMut forwarding call requires a mutable binding' in d.get('message', '')
                                for d in report.get('diagnostics', [])), 'pair negative did not safely reject')
                require('executable' not in report.get('artifacts', {}) and report.get('native_codegen') is None
                        and all(directory + '/' + s not in files for s in ('program', 'stdout', 'stderr', 'exit')),
                        'pair negative contains native execution')
                cells[('pair-negative', 'O0')] += 1
                continue
            _, opt = native(directory, report)
            expected = pair_expected['positive']
            require(exit_value(directory + '/exit') == expected['exit']
                    and data(directory + '/stdout') == expected['stdout'].encode()
                    and data(directory + '/stderr') == b'', 'pair123M native observation mismatch')
            cells[('pair-positive', opt)] += 1
            counts['pair_native_runs'] += 1
            continue
        allocator = any(p.startswith('consumer256-allocator-') for p in parts)
        backend, opt = native(directory, report)
        if len(sources) == 1:
            require(not allocator and main in frozen, 'unknown frozen standalone source')
            case = frozen[main]
            key = 'frozen:' + case['name']
            rows = [('case-' + str(i), e, e['input'].encode())
                    for i, e in enumerate(case['native_if_source_verified'])]
        else:
            group = 'cases' if main == 'frozen-prep/project/main.zil' else 'extensions' if main == 'extended/main.zil' else None
            require(group is not None and len(sources) == 3, 'unknown multi-module consumer')
            base = main.rsplit('/', 1)[0]
            require(sources == {'0': base + '/main.zil', '1': base + '/model.zil', '2': base + '/relay.zil'},
                    'module identity mismatch')
            key = group + (':allocator' if allocator else ':normal')
            if allocator:
                require(backend == 'llvm-api', 'allocator consumer used non-API backend')
                for suffix in ('api.o', 'probe', 'capture-object.sh', 'link.stdout', 'link.stderr'):
                    require(directory + '/' + suffix in files, 'allocator native original missing: ' + suffix)
                require(files[directory + '/probe']['mode'] & 0o111, 'probe executable mode lost')
                rows = [('input-' + c['input'].encode().hex() + '-fail-' + str(e['fail_at']), e, c['input'].encode())
                        for c in oracle[group] for e in c['allocator']]
            else:
                rows = [('case-' + str(i), c['normal'], c['input'].encode())
                        for i, c in enumerate(oracle[group])]
        expected_paths = {directory + '/' + r[0] + '/expected.json' for r in rows}
        actual_paths = {p for p in files if p.startswith(directory + '/') and p.endswith('/expected.json')}
        require(expected_paths == actual_paths, 'missing/extra execution rows: ' + directory)
        for name, expected, stdin in rows:
            execution(directory + '/' + name, expected, stdin, allocator)
        cells[(key, opt)] += 1
        counts['consumer_compiles'] += 1

    require(len(backends) == 1, 'missing/mixed backend profile')
    backend = next(iter(backends))
    require(expected_counts.get('backend', backend) == backend, 'backend differs from requested phase')
    groups = ['frozen:' + c['name'] for c in oracle['frozen']] + ['cases:normal', 'extensions:normal', 'pair-positive']
    if backend == 'llvm-api':
        groups += ['cases:allocator', 'extensions:allocator']
    expected_cells = Counter({(group, opt): 1 for group in groups for opt in ('O0', 'O2')})
    expected_cells[('pair-negative', 'O0')] = 1
    require(cells == expected_cells, 'missing/duplicate O0/O2 compilation cells: ' + repr(dict(cells)))
    counts['native_runs'] = counts['consumer_native_runs'] + counts['pair_native_runs']
    for key, value in expected_counts.items():
        if key != 'backend':
            require(key in counts and counts[key] == value, 'count differs for ' + key)
    return {'status': 'Pass', 'phase_prefix': phase_prefix, 'backend': backend, **dict(counts),
            'oracle_sha256': hashlib.sha256(source('tests/impl256/ORACLE.json')).hexdigest(),
            'pair_expected_sha256': hashlib.sha256(source('tests/impl256/paired/EXPECTED.json')).hexdigest(),
            'compiled_source_bindings': bindings,
            'cells': [{'group': group, 'opt': opt, 'count': n} for (group, opt), n in sorted(cells.items())],
            'excluded_boundary_reports': excluded, 'supplement': 'NotAudited',
            'scope': 'Frozen native observations and paired source control only; no execution, full-CI or product acceptance claim.'}
