from pathlib import Path
import gzip
import hashlib
import json
import re
import subprocess
import sys

REPO = Path('/workspace/scratch/994319f2ba02/zil-review11')
CI = Path('/workspace/scratch/994319f2ba02/z11-ci41195906918')
CANDIDATE = 'c0418f532e750e7df2b00d9b7d76da82baa5086c'
CHECKOUT = 'db07200ba221bdb67f3f1b7bef7f8e9851592a1a'
PHASE = sys.argv[1]
assert PHASE in ('workspace', 'reference')
REPORTS = CI / PHASE
NATIVE = CI / (PHASE + '-native')
assert (REPORTS / 'commit.txt').read_text().strip() == CHECKOUT
tree = (REPORTS / 'source-tree.txt').read_bytes()
assert tree == subprocess.check_output(['git', 'ls-tree', '-r', CANDIDATE], cwd=REPO)
native_manifest_path = REPORTS / (PHASE + '-native-manifest.json')
native_manifest = json.loads(native_manifest_path.read_text())
manifest = {x['path']: x for x in native_manifest['files']}
log_path = REPORTS / (PHASE + '.log')
log = log_path.read_text()
rows = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;', log)
counts = [sum(int(r[i]) for r in rows) for i in range(5)]
assert counts[1:] == [0, 0, 0, 0]
toolchain = json.loads((REPORTS / 'llvm-api-toolchain.json').read_text())
assert toolchain['status'] == 'Pass'
assert toolchain['checks']['llvm']['version'] == '23.1.4'
assert toolchain['checks']['clang']['version'] == '23.1.4'
assert toolchain['checks']['rustc']['version'] == '1.99.1'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def find_one(prefix):
    values = [p for p in NATIVE.iterdir() if p.is_dir() and p.name.startswith(prefix)]
    assert len(values) == 1, (prefix, values)
    return values[0]


def archived_bytes(path):
    if path.exists():
        return path.read_bytes()
    compressed = Path(str(path) + '.gz')
    assert compressed.exists(), str(path)
    return gzip.decompress(compressed.read_bytes())


def case_reports(runtime, local, expected_native, expected_exit=0):
    sources = list(runtime.rglob('*.zil'))
    for f in sources:
        assert f.read_bytes() == archived_bytes(local / f.relative_to(runtime)), str(f)
    records = []
    for f in sorted(runtime.glob('*/compile.json')):
        r = json.loads(f.read_text())
        old = json.loads((local / f.relative_to(runtime)).read_text())
        assert [r['status'], r['phase'], r['verification_runs']] == [old['status'], old['phase'], old['verification_runs']], str(f)
        assert [d['code'] for d in r['diagnostics']] == [d['code'] for d in old['diagnostics']], str(f)
        ng = r.get('native_codegen')
        if ng:
            records.append(native_report(f, expected_exit, archived_bytes(local / f.parent.name / 'stdout')))
    assert len(records) == expected_native
    return {'source_files_equal_local_review': len(sources), 'native': records}


def native_report(path, expected_exit, expected_stdout, expected_backend=None):
    r = json.loads(path.read_text())
    assert r['status'] == 'Success' and r['verification_runs'] == 1, str(path)
    ng = r['native_codegen']
    assert ng['fallback'] is None
    expected_backend = expected_backend or ('llvm-api' if PHASE == 'workspace' else 'clang')
    if expected_backend == 'llvm-api':
        assert ng['backend'] == 'llvm-api'
        assert ng['object_reports']
        for obj in ng['object_reports']:
            assert obj['backend'] == 'llvm-api' and obj['llvm_version'] == '23.1.4'
            assert obj['input_kind'] == 'builder' and obj['verify_before_after'] is True
            assert obj['parse_ns'] == 0
    else:
        assert ng['backend'] == 'clang'
        assert any('llvm-23.1.4' in cmd[0] and 'clang' in cmd[0] for cmd in r['commands'])
    assert (path.parent / 'exit.txt').read_text().strip() == f'Some({expected_exit})'
    assert (path.parent / 'stdout').read_bytes() == expected_stdout
    assert (path.parent / 'stderr').read_bytes() == b''
    return {'case': path.parent.name, 'backend': ng['backend'], 'fallback': ng['fallback'], 'verification_runs': r['verification_runs'], 'object_reports': ng['object_reports'], 'commands': r['commands'], 'expected_exit': expected_exit, 'stdout_hex': expected_stdout.hex(), 'report_sha256': sha(path.read_bytes())}


r232 = find_one('review232-')
r233 = find_one('review233-')
r077 = find_one('review077-')
local232 = REPO / 'evidence/reviews/232/candidate-426bbd20/final'
local233 = REPO / 'evidence/reviews/233/candidate-6d087050/final'
audit232 = case_reports(r232, local232, 14)
audit233 = case_reports(r233, local233, 12)
wire_matches = []
for f in sorted(r232.rglob('*.wire')):
    assert f.read_bytes() == archived_bytes(local232 / f.relative_to(r232)), str(f)
    wire_matches.append({'path': str(f.relative_to(r232)), 'sha256': sha(f.read_bytes())})
audit232['wire_equal_local_review'] = wire_matches
for label in ['dynamic-origin-forgery', 'same-type-retarget', 'same-type-explicit226-retarget']:
    for name in ['analyze.txt', 'raw.txt', 'wire.txt']:
        p = r232 / label / name
        assert p.read_bytes() == archived_bytes(local232 / label / name), str(p)
audit232['raw_diagnostics_equal_local_review'] = True
raw077 = {}
local077 = REPO / 'evidence/reviews/232/ci41194080948-077/candidate'
for case in ['original077-selector', 'explicit-descriptor-attack']:
    for name in ['source.zil', 'original.wire', 'mutated.wire', 'OBSERVATION.json', 'analyze.txt', 'raw.txt', 'wire.txt']:
        p = r077 / case / name
        assert p.read_bytes() == archived_bytes(local077 / case / name), str(p)
    raw077[case] = json.loads((r077 / case / 'OBSERVATION.json').read_text())
audit232['raw077_exact_archived_bytes'] = raw077

expected_budget = {
    'budget225-legacy': [835, 7129, 12161], 'budget225-nested': [3889, 36339, 56097],
    'budget226-legacy': [835, 7129, 12161], 'budget226-nested': [1787, 15791, 30761],
    'budget232-flat': [1226, 7896, 16608], 'budget232-owned': [1762, 12234, 17419],
    'budget232-recursive': [1788, 14756, 29096],
}
budgets = {}
for name, values in expected_budget.items():
    r = json.loads((NATIVE / name / 'minimum.json').read_text())
    assert [r['types'], r['analyze'], r['verify']] == values
    assert all(r[k] is True for k in ['n', 'n_minus_one_resource_limit', 'fresh_n'])
    budgets[name] = r
audit232['budgets'] = budgets
impl233 = find_one('impl233-')
budgets233 = {}
for name, minimum in [('counter', 16012), ('trait-library', 2003)]:
    r = json.loads((impl233 / 'budget' / (name + '.json')).read_text())
    assert r == {'minimum_analysis_steps': minimum, 'cold_warm_fresh': 'Pass'}
    budgets233[name] = r
audit233['budgets'] = budgets233

local_migration = json.loads((REPO / 'evidence/reviews/232/migration-507e650d/RESULT.json').read_text())
expected_sources = {k: local_migration['original_inputs'][k]['source_sha256'] for k in ['072', '077']}
migration = []
migration_roots = sorted(NATIVE.glob('migration232-*'))
for p in migration_roots:
    r = json.loads((p / 'report.json').read_text())
    source_hash = sha((p / 'source.zil').read_bytes())
    if '072-owned-original' in p.name:
        assert source_hash == expected_sources['072']
    if '077-shared-original' in p.name:
        assert source_hash == expected_sources['077']
    row = {'case': p.name, 'status': r['status'], 'phase': r['phase'], 'codes': [d['code'] for d in r['diagnostics']], 'source_sha256': source_hash}
    if r.get('native_codegen'):
        row['native'] = native_report(p / 'report.json', 20, b'')
    if 'owner-after-creation' in p.name or 'second-call' in p.name:
        assert (r['status'], r['phase'], row['codes']) == ('SafeReject', 'analyze', ['E-UNINITIALIZED'])
    if '077-live-' in p.name:
        assert (r['status'], r['phase'], row['codes']) == ('SafeReject', 'verify', ['E-BORROW-CONFLICT'])
    if '077-default-mutable' in p.name:
        assert (r['status'], r['phase'], row['codes']) == ('Unsupported', 'types', ['E-UNSUPPORTED'])
    migration.append(row)
assert len(migration) == 15 and sum('native' in r for r in migration) == 6
audit232['migration'] = migration

expected192 = {r['source_sha256']: r for r in local_migration['192_original_controls']}
roots192 = []
rows192 = []
for p in NATIVE.glob('fields192-*'):
    source = p / 'main.zil'
    if not source.exists():
        continue
    digest = sha(source.read_bytes())
    if digest not in expected192:
        continue
    expected = expected192[digest]
    r = json.loads((p / 'compile.json').read_text())
    assert [r['status'], r['phase'], [d['code'] for d in r['diagnostics']]] == [expected['status'], expected['phase'], expected['codes']]
    row = {'case': p.name, 'variant': expected['variant'], 'source_sha256': digest, 'status': r['status'], 'phase': r['phase'], 'codes': expected['codes']}
    if r.get('native_codegen'):
        row['native'] = native_report(p / 'compile.json', 0, b'', expected_backend='clang')
    roots192.append(p)
    rows192.append(row)
assert len(rows192) == 5
audit232['192_original_controls'] = rows192
audit232['192_backend_accounting'] = 'The unchanged192 native helper uses CompileOptions::default; NativeBackend::default is Clang. Its two Clang23.1.4 runs are recorded separately even within the default workspace run, never counted as Builder/API executions. New232/233 review and migration natives explicitly select the required backend.'

scope232 = ['crates/zil-types/src', 'crates/zil-driver/tests/impl232_budget.rs', 'crates/zil-driver/tests/impl232_readonly_product_capture.rs', 'crates/zil-driver/tests/impl072_packages.rs', 'crates/zil-driver/tests/impl077_package_captures.rs', 'crates/zil-driver/tests/impl192_copy_fields.rs', 'crates/zil-driver/tests/impl232_fixture_support', 'examples/mainline/readonly_product232']
scope233 = ['crates/zil-expand/src', 'crates/zil-driver/tests/impl233_modules.rs', 'crates/zil-expand/tests/impl233_project_traits.rs', 'examples/mainline/trait_modules233']
impl232 = '507e650d526653ccc79146366e4bb7c30b55ac84'
impl233_sha = '6d087050f232e8e8939aff297af0acc89e11bd20'
for commit, scope in [(impl232, scope232), (impl233_sha, scope233)]:
    assert subprocess.check_output(['git', 'diff', '--name-only', commit, CANDIDATE, '--', *scope], cwd=REPO).strip() == b''

groups = {}
for name in ['review232_independent.rs', 'review232_ci077.rs', 'review233_independent.rs', 'impl232_budget.rs', 'impl225_budget.rs', 'impl226_budget.rs', 'impl233_modules.rs', 'impl233_project_traits.rs', 'impl077_package_captures.rs', 'impl072_packages.rs', 'impl192_copy_fields.rs']:
    m = re.search(r'Running tests/' + re.escape(name) + r'.*?(?=\n\s*Running |\n\s*Doc-tests |\Z)', log, re.S)
    assert m, name
    group = m.group(0)
    result = re.search(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;', group)
    assert result and result.group(1) == 'ok'
    groups[name] = {'counts': [int(result.group(i)) for i in range(2, 7)], 'raw_log': group}


def archive_slice(number, roots, audit, implementation, scope):
    out = REPO / f'evidence/reviews/{number}/ci41195906918' / PHASE
    out.mkdir(parents=True, exist_ok=True)
    retained = []
    checked = []
    for root in roots:
        for p in sorted(root.rglob('*')):
            if not p.is_file():
                continue
            relative = str(p.relative_to(NATIVE))
            data = p.read_bytes()
            original = manifest[relative]
            assert len(data) == original['bytes'] and sha(data) == original['sha256'], relative
            checked.append(original)
            record = {'original_path': relative, 'sha256': original['sha256'], 'bytes': original['bytes']}
            if p.name == 'program' or p.suffix in ('.o', '.obj'):
                record['archive'] = None
                record['representation'] = 'binary digest; original ZIP retained by integrator'
            else:
                target = out / 'raw' / relative
                compressed = len(data) > 2000 and p.suffix not in ('.json', '.zil', '.wire')
                if compressed:
                    target = Path(str(target) + '.gz')
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(gzip.compress(data, mtime=0) if compressed else data)
                record['archive'] = str(target.relative_to(out))
            retained.append(record)
    (out / 'MANIFEST.json').write_text(json.dumps(retained, indent=2) + '\n')
    (out / 'CI-MANIFEST-SUBSET.json').write_text(json.dumps(checked, indent=2) + '\n')
    (out / 'commit.txt').write_bytes((REPORTS / 'commit.txt').read_bytes())
    (out / 'llvm-api-toolchain.json').write_bytes((REPORTS / 'llvm-api-toolchain.json').read_bytes())
    relevant = list(groups) if number == 232 else ['review233_independent.rs', 'impl233_modules.rs', 'impl233_project_traits.rs']
    (out / 'relevant-tests.log').write_text('\n'.join(groups[n]['raw_log'] for n in relevant))
    result = {
        'reviewer': '/root/handoff_integrity', 'slice': str(number),
        'review_stage': 'DefaultApiEvidenceAudited' if PHASE == 'workspace' else 'ReferenceEvidenceAudited',
        'formal_acceptance': 'Pending final required gate verification; no FINAL verdict issued here',
        'implementation_commit': implementation,
        'original_production_commit': '426bbd2066e70acdb5b635ea90694c050c78f764' if number == 232 else implementation,
        'candidate_commit': CANDIDATE, 'actual_checkout': CHECKOUT, 'run_id': 41195906918,
        'phase': PHASE, 'source_tree_equal_candidate': True, 'source_tree_sha256': sha(tree),
        'source_scope_equal_implementation': scope, 'source_scope_diff': [],
        'toolchain': {'llvm': '23.1.4', 'clang': '23.1.4', 'rust': '1.99.1'},
        'original_log_sha256': sha(log_path.read_bytes()), 'original_manifest_sha256': sha(native_manifest_path.read_bytes()),
        'run_counts': dict(zip(['passed', 'failed', 'ignored', 'measured', 'filtered'], counts)), 'run_groups': len(rows),
        'relevant_tests': {n: groups[n]['counts'] for n in relevant},
        'checked_original_artifact_files': len(checked), 'manifest_entries_all_match': True,
        'checks': audit,
        'previous_failed_run': {'run_id': 41194080948, 'workspace': [3020, 4, 0], 'reference': [2976, 4, 0], 'status': 'Fail retained'},
        'accounting': 'Original author, local independent, default API, reference and later main replays remain separate; no summed new coverage.',
    }
    (out / 'EVIDENCE-AUDIT.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    return {'slice': number, 'artifact_files_checked': len(checked), 'native_independent_replays': len(audit['native']), 'source_files_equal_local_review': audit['source_files_equal_local_review'], 'output': str(out)}


result232 = archive_slice(232, [r232, r077, *migration_roots, *roots192, *[NATIVE / name for name in expected_budget]], audit232, impl232, scope232)
result233 = archive_slice(233, [r233, impl233 / 'budget'], audit233, impl233_sha, scope233)
print(json.dumps({'phase': PHASE, 'counts': counts, 'source_tree_equal': True, 'results': [result232, result233]}, ensure_ascii=False))
