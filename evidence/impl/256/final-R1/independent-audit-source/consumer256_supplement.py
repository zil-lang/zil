"""Audit separate 256 supplement observations against pinned source expectations.

No compiler execution. Complete blob/source snapshot verification belongs to the
caller. Planned counts are derived expectations, never observations by themselves.
"""
from collections import Counter
import hashlib
import json
from pathlib import PurePosixPath
import re

COUNTERS = ('alloc_calls', 'allocated', 'frees', 'failures', 'live', 'peak_live')
GROUPS = {
    'ordinary': ('supplement/ordinary_main.zil', 'frozen-prep/project/model.zil'),
    'mixed': ('supplement/mixed_main.zil', 'frozen-prep/project/model.zil',
              'frozen-prep/project/relay.zil'),
}


def require(ok, message):
    if not ok:
        raise ValueError('consumer256 supplement: ' + message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def verify(index_files, read_bytes, oracle, source_files, phase_prefix, expected_counts=None):
    """Same interface as consumer256.verify, separate results and coverage.

expected_counts accepts backend plus observed integer counters returned below.
Both groups require source Verify evidence and O0/O2 normal builds. API also
requires O0/O2 allocator builds; Clang requires no allocator builds.
"""
    expected_counts = expected_counts or {}
    phase_prefix = phase_prefix.rstrip('/') + '/'
    require(phase_prefix.startswith('native-evidence/'), 'invalid phase prefix')
    files = {e['path']: e for e in index_files}
    require(len(files) == len(index_files), 'duplicate logical file path')

    def source(name):
        require(name in source_files and isinstance(source_files[name], bytes),
                'missing pinned source: ' + name)
        return source_files[name]

    def data(name):
        require(name in files, 'missing indexed file: ' + name)
        b = read_bytes(name)
        require(isinstance(b, bytes) and len(b) == files[name]['bytes']
                and sha(b) == files[name]['sha256'], 'indexed bytes/SHA mismatch: ' + name)
        return b

    def obj(name):
        return json.loads(data(name))

    oracle_path = 'tests/impl256/supplement/ORACLE.json'
    require(json.loads(source(oracle_path)) == oracle, 'oracle differs from pinned supplement source')
    require(sha(source('tests/impl256/ORACLE.json')) == oracle['frozen_core_oracle_sha256'],
            'frozen core oracle binding changed')
    source_set = set(s for group in GROUPS.values() for s in group)
    require(set(oracle['source_sha256']) == source_set, 'supplement source hash set differs')
    for name in source_set:
        require(sha(source('tests/impl256/' + name)) == oracle['source_sha256'][name],
                'supplement oracle/source SHA mismatch: ' + name)
    normal_modes = sum(len(oracle[g]) for g in GROUPS)
    allocator_rows = sum(len(c['allocator']) for g in GROUPS for c in oracle[g])
    planned_failures = 2 * sum(e['failures'] for g in GROUPS for c in oracle[g] for e in c['allocator'])
    planned = {'normal_modes': normal_modes, 'native_O0_O2_runs': 2 * (normal_modes + allocator_rows),
               'normal_runs': 2 * normal_modes, 'allocator_runs': 2 * allocator_rows,
               'real_allocator_failures': planned_failures}
    require(planned == oracle['planned_separate_group'], 'planned counts do not derive from oracle rows')
    for group in GROUPS:
        rows = oracle[group]
        require(rows and len({c['input'] for c in rows}) == len(rows), 'empty/duplicate input modes')
        for c in rows:
            require([e['fail_at'] for e in c['allocator']] == list(range(c['normal']['alloc_calls'] + 2)),
                    'allocator oracle omits disabled, fail point or one-past control')

    reports = sorted(p for p in files if p.startswith(phase_prefix) and p.endswith('/report.json')
                     and any(x.startswith('consumer256-supplement-') for x in PurePosixPath(p).parts))
    require(reports, 'missing complete supplement evidence: no reports')
    counts = Counter({k: 0 for k in ('verify_only_compiles', 'normal_compiles', 'allocator_compiles',
                                    'native_compiles', 'verification_runs', 'normal_runs',
                                    'allocator_runs', 'actual_allocation_failures', 'native_runs')})
    cells = Counter()
    backends = set()
    bindings = []

    def bind(directory, report, group):
        mapping = report.get('source_files')
        names = GROUPS[group]
        require(isinstance(mapping, dict) and set(mapping) == {str(i) for i in range(len(names))},
                'source module identity mismatch')
        for i, suffix in enumerate(names):
            original = mapping[str(i)]
            require(isinstance(original, str) and original.endswith('/tests/impl256/' + suffix),
                    'source report identity differs: ' + str(original))
            copy = directory + '/module' + str(i) + '.zil'
            require(data(copy) == source('tests/impl256/' + suffix), 'preserved source differs: ' + copy)
        bindings.append({'directory': directory, 'group': group, 'sources': list(names)})

    def exit_value(name):
        match = re.fullmatch(rb'Some\((-?\d+)\)\s*', data(name))
        require(match is not None, 'missing numeric process exit')
        return int(match[1])

    for path in reports:
        directory = path.rsplit('/', 1)[0]
        label = PurePosixPath(directory).name
        match = re.fullmatch(r'consumer256-supplement-(ordinary|mixed)_main-(verify|matrix|allocator)-\d+-\d+', label)
        require(match is not None, 'unrecognized supplement report directory: ' + directory)
        group, kind = match.groups()
        report = obj(path)
        bind(directory, report, group)
        require(report.get('status') == 'Success' and report.get('verification_runs') == 1,
                'compile status/Verify mismatch')
        require(directory + '/phases/verified.txt' in files, 'verified stage product missing')
        counts['verification_runs'] += 1
        if kind == 'verify':
            require(report.get('phase') == 'verify' and report.get('native_codegen') is None
                    and not report.get('commands') and 'executable' not in report.get('artifacts', {}),
                    'Verify-only report contains native work')
            require(not any(p.startswith(directory + '/') and (
                p.endswith(('/expected.json', '/exit.txt', '/stdout', '/stderr'))
                or PurePosixPath(p).name in ('program', 'probe', 'api.o')) for p in files),
                'Verify-only evidence contains native observations')
            counts['verify_only_compiles'] += 1
            cells[(group, 'verify', 'none')] += 1
            continue

        require(report.get('phase') == 'link', 'native compilation did not reach link')
        cg = report.get('native_codegen')
        require(isinstance(cg, dict) and cg.get('fallback') is None, 'missing codegen or fallback')
        backend = cg.get('backend')
        require(backend in ('llvm-api', 'clang'), 'unknown native backend')
        backends.add(backend)
        opts = {a for command in report.get('commands', []) for a in command if a in ('-O0', '-O2')}
        require(len(opts) == 1, 'optimization identity missing/ambiguous')
        opt = next(iter(opts))[1:]
        if backend == 'llvm-api':
            objects = cg.get('object_reports', [])
            require(type(cg.get('actual_units')) is int and cg['actual_units'] > 0
                    and len(objects) == cg['actual_units'], 'missing API object reports')
            require(all(o.get('input_kind') == 'builder' and o.get('verify_before_after') is True
                        and o.get('pipeline') == 'default<' + opt + '>' for o in objects),
                    'API builder/Verify/optimization mismatch')
        for suffix in ('program', 'program.ll'):
            require(directory + '/' + suffix in files, 'missing machine product: ' + suffix)
        require(files[directory + '/program']['mode'] & 0o111, 'program executable mode lost')
        require(PurePosixPath(report['artifacts'].get('executable', '')).name == 'program'
                and PurePosixPath(report['artifacts'].get('llvm', '')).name == 'program.ll',
                'unexpected executable/LLVM identity')
        allocator = kind == 'allocator'
        if allocator:
            require(backend == 'llvm-api' and cg.get('actual_units') == 1,
                    'allocator requires one API object')
            for suffix in ('api.o', 'probe', 'capture-object.sh', 'link.stdout', 'link.stderr'):
                require(directory + '/' + suffix in files, 'missing allocator product: ' + suffix)
            require(files[directory + '/probe']['mode'] & 0o111, 'probe executable mode lost')
            rows = [('input-' + c['input'].encode().hex() + '-fail-' + str(e['fail_at']), c['input'].encode(), e)
                    for c in oracle[group] for e in c['allocator']]
        else:
            rows = [('case-' + str(i), c['input'].encode(), c['normal']) for i, c in enumerate(oracle[group])]
        required_rows = {directory + '/' + name + '/expected.json' for name, _, _ in rows}
        observed_rows = {p for p in files if p.startswith(directory + '/') and p.endswith('/expected.json')}
        require(required_rows == observed_rows, 'missing/extra supplement execution rows')
        for name, stdin, expected in rows:
            run = directory + '/' + name
            require(obj(run + '/expected.json') == expected, 'recorded expectation differs from pinned oracle')
            require(data(run + '/stdin') == stdin and exit_value(run + '/exit.txt') == expected['exit']
                    and data(run + '/stdout') == expected['stdout'].encode(), 'observations differ from pinned oracle')
            stderr = data(run + '/stderr')
            if allocator:
                actual = json.loads(stderr)
                require(isinstance(actual, dict) and set(COUNTERS) <= set(actual)
                        and set(actual) <= set(COUNTERS) | {'peak_bytes'}, 'missing/unknown allocator counters')
                require(all(type(actual[k]) is int and actual[k] >= 0 and actual[k] == expected[k]
                            for k in COUNTERS), 'allocator six counters differ from pinned oracle')
                require('peak_bytes' not in actual or (type(actual['peak_bytes']) is int
                        and actual['peak_bytes'] >= 0), 'invalid unpaired peak_bytes measurement')
                require(actual['live'] == 0 and actual['allocated'] == actual['frees'], 'unbalanced allocation')
                counts['allocator_runs'] += 1
                counts['actual_allocation_failures'] += actual['failures']
            else:
                require(stderr == b'', 'unexpected ordinary execution stderr')
                counts['normal_runs'] += 1
            counts['native_runs'] += 1
        counts['allocator_compiles' if allocator else 'normal_compiles'] += 1
        counts['native_compiles'] += 1
        cells[(group, kind, opt)] += 1

    require(len(backends) == 1, 'missing complete supplement native evidence or mixed backends')
    backend = next(iter(backends))
    require(expected_counts.get('backend', backend) == backend, 'backend differs from requested phase')
    required_cells = Counter({(group, kind, opt): 1 for group in GROUPS
                              for kind in (('matrix', 'allocator') if backend == 'llvm-api' else ('matrix',))
                              for opt in ('O0', 'O2')})
    required_cells.update({(group, 'verify', 'none'): 1 for group in GROUPS})
    require(cells == required_cells, 'missing/duplicate supplement Verify/O0/O2 cells')
    derived_counts = {'normal_runs': planned['normal_runs'],
                      'allocator_runs': planned['allocator_runs'] if backend == 'llvm-api' else 0,
                      'actual_allocation_failures': planned['real_allocator_failures'] if backend == 'llvm-api' else 0,
                      'verify_only_compiles': len(GROUPS)}
    derived_counts['native_runs'] = derived_counts['normal_runs'] + derived_counts['allocator_runs']
    require(all(counts[k] == v for k, v in derived_counts.items()), 'observed coverage differs from derived oracle profile')
    for key, expected in expected_counts.items():
        if key != 'backend':
            require(key in counts and counts[key] == expected, 'count differs for ' + key)
    return {'status': 'Pass', 'phase_prefix': phase_prefix, 'backend': backend, **dict(counts),
            'oracle_sha256': sha(source(oracle_path)), 'frozen_core_oracle_sha256': oracle['frozen_core_oracle_sha256'],
            'source_sha256': oracle['source_sha256'], 'planned_from_oracle': planned,
            'compiled_source_bindings': bindings,
            'cells': [{'group': g, 'kind': k, 'opt': o, 'count': n} for (g, k, o), n in sorted(cells.items())],
            'scope': 'Separate supplement observations only; no compiler execution and no core/pair or whole-CI acceptance claim.'}
