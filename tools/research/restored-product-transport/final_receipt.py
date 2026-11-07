#!/usr/bin/env python3
"""Read restored R1 products only; never compile or execute a native program.

Absolute paths in original reports are mapped through the archive's logical
index. Original JSON and observations remain byte-for-byte unchanged.
"""
import argparse
from collections import Counter
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess

ROOT = Path(__file__).resolve().parents[3]
COUNTERS = ('alloc_calls', 'allocated', 'frees', 'failures', 'live', 'peak_live')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def file_sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def git(*arguments):
    return subprocess.check_output(['git', *arguments], cwd=ROOT)


def load_validator(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def collect(args):
    commit = git('rev-parse', args.source_commit).decode().strip()
    index = json.loads(args.index.read_text())
    files = {entry['path']: entry for entry in index['files']}
    assert len(files) == len(index['files'])
    assert index['checkout'] == commit
    producer = git('show', commit + ':tools/ci/evidence_transport.py')
    assert index['producer_sha256'] == sha(producer)

    def data(name):
        path = args.runner / name
        entry = files[name]
        value = path.read_bytes()
        assert len(value) == entry['bytes'] and sha(value) == entry['sha256'], name
        assert stat.S_IMODE(path.stat().st_mode) == entry['mode'], name
        return value

    def obj(name):
        return json.loads(data(name))

    chosen = [name for name in files if name.startswith('receipts/') or (
        name.startswith('native-evidence/') and any(
            part.startswith('consumer256-') for part in PurePosixPath(name).parts))]
    for name in chosen:
        data(name)
    check = obj('receipts/check.json')
    assert check['source_commit'] == commit and check['status'] == 'Pass'
    assert check['exit_code'] == 0 and not check['timeout']
    log = data('receipts/output.log')
    assert sha(log) == check['log_sha256']
    source_files = {
        name: git('show', commit + ':' + name)
        for name in git('ls-tree', '-r', '--name-only', commit, 'tests/callable-instance-forwarding').decode().splitlines()
    }
    oracle = json.loads(source_files['tests/callable-instance-forwarding/ORACLE.json'])
    supplement = json.loads(source_files['tests/callable-instance-forwarding/supplement/ORACLE.json'])
    core_tool = args.validators / 'consumer256.py'
    supplement_tool = args.validators / 'consumer256_supplement.py'
    core = load_validator(core_tool, 'returned_callback_contracts_final_core').verify(
        index['files'], data, oracle, source_files, 'native-evidence/',
        dict(backend='llvm-api', native_runs=256, consumer_native_runs=254,
             pair_native_runs=2, allocator_runs=190, actual_allocation_failures=90))
    extra = load_validator(supplement_tool, 'returned_callback_contracts_final_supplement').verify(
        index['files'], data, supplement, source_files, 'native-evidence/',
        dict(backend='llvm-api', native_runs=32, normal_runs=8,
             allocator_runs=24, actual_allocation_failures=8, verify_only_compiles=2))

    suites = {}
    current = None
    for line in log.decode().splitlines():
        match = re.search(r'Running tests/(returned_callback_contracts_\w+)\.rs ', line)
        if match:
            current = match[1]
            assert current not in suites
            suites[current] = dict(test_names=[])
        elif 'Running ' in line or 'Doc-tests ' in line:
            current = None
        elif current:
            match = re.fullmatch(r'test (\S+) \.\.\. ok', line)
            if match:
                suites[current]['test_names'].append(match[1])
            match = re.fullmatch(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;.*', line)
            if match:
                suites[current]['counts'] = dict(zip(
                    ['passed', 'failed', 'ignored', 'measured', 'filtered'], map(int, match.groups())))
    expected_suites = dict(returned_callback_contracts_consumer=16, returned_callback_contracts_boundaries=1,
                          returned_callback_contracts_contracts=1, returned_callback_contracts_pair=1)
    assert set(suites) == set(expected_suites)
    for name, count in expected_suites.items():
        assert suites[name]['counts'] == dict(passed=count, failed=0, ignored=0, measured=0, filtered=0)
        assert len(suites[name]['test_names']) == count

    reports = []
    executions = []
    boundaries = []
    artifact_map = []
    grouping = Counter()
    report_names = sorted(name for name in chosen if name.endswith('/report.json'))
    for name in report_names:
        report = obj(name)
        directory = name.rsplit('/', 1)[0]
        label = PurePosixPath(directory).name
        native = report['status'] == 'Success' and report['phase'] == 'link'
        artifacts = {}
        for kind, original in report['artifacts'].items():
            parts = PurePosixPath(original).parts
            position = parts.index('native-evidence')
            logical = PurePosixPath(*parts[position:]).as_posix()
            assert logical.startswith(directory + '/') and logical in files
            data(logical)
            artifacts[kind] = files[logical]
            artifact_map.append(dict(report=name, original=original, logical=logical))
        reports.append(dict(path=name, status=report['status'], phase=report['phase'],
                            verification_runs=report['verification_runs'], artifacts=artifacts,
                            native_codegen=report.get('native_codegen')))
        if '/consumer256-negatives-' in name:
            assert not native
            if label == 'mut_declared_actual_fn_shared_call':
                gap = obj(directory + '/out-of-profile.json')
                assert gap['actual'] == report['status'] == 'Unsupported'
                boundaries.append(dict(case=label, status=report['status'], phase=report['phase'],
                    current_semantic_pass=False, historical_unfinished_gap=gap,
                    diagnostics=report['diagnostics']))
            else:
                expected = obj(directory + '/expected-diagnostic.json')
                assert report['status'] == expected['category']
                assert any(d['category'] == expected['category'] and d['phase'] == expected['phase']
                           and d['code'] == expected['code'] and all(
                               text in d['message'] for text in expected['message_fragments'])
                           for d in report['diagnostics'])
                boundaries.append(dict(case=label, status=report['status'], phase=report['phase'],
                    current_semantic_pass=report['status'] == 'SafeReject',
                    current_profile_boundary=report['status'] == 'Unsupported',
                    expected=expected, diagnostics=report['diagnostics']))
        if not native:
            continue
        mapping = report['source_files']
        source = mapping['0'].rsplit('/tests/callable-instance-forwarding/', 1)[1]
        if source.startswith('supplement/'):
            group = 'supplement_' + source.split('/')[1].split('_')[0]
        elif source.startswith('extended/'):
            group = 'core_extended'
        elif source == 'frozen-prep/project/main.zil':
            group = 'core_legacy'
        elif source.startswith('paired/'):
            group = 'core_pair'
        else:
            group = 'core_standalone'
        if group == 'core_pair':
            expected = json.loads(source_files['tests/callable-instance-forwarding/paired/EXPECTED.json'])['positive']
            row = dict(compile=name, group=group, case=directory, stdin_hex='',
                       expected=expected, actual=dict(
                           exit=int(re.fullmatch(rb'Some\((-?\d+)\)\s*', data(directory + '/exit'))[1]),
                           stdout=data(directory + '/stdout').decode()),
                       stderr=data(directory + '/stderr').decode())
            assert row['actual']['exit'] == expected['exit'] and row['actual']['stdout'] == expected['stdout']
            assert not row['stderr']
            executions.append(row)
            grouping[group] += 1
            continue
        for expectation in sorted(p for p in chosen if p.startswith(directory + '/') and p.endswith('/expected.json')):
            case = expectation.rsplit('/', 1)[0]
            expected = obj(expectation)
            stderr = data(case + '/stderr').decode()
            allocator = bool(stderr)
            actual = dict(exit=int(re.fullmatch(rb'Some\((-?\d+)\)\s*', data(case + '/exit.txt'))[1]),
                          stdout=data(case + '/stdout').decode())
            assert actual['exit'] == expected['exit'] and actual['stdout'] == expected['stdout']
            if allocator:
                actual.update(json.loads(stderr))
                assert all(actual[k] == expected[k] for k in COUNTERS)
            row_group = group + ('_allocator' if allocator else '_normal')
            executions.append(dict(compile=name, group=row_group, case=case,
                                   stdin_hex=data(case + '/stdin').hex(), expected=expected,
                                   actual=actual, stderr=stderr))
            grouping[row_group] += 1
    assert dict(grouping) == dict(core_standalone_normal=14, core_legacy_normal=26,
        core_legacy_allocator=82, core_extended_normal=24, core_extended_allocator=108,
        core_pair=2, supplement_ordinary_normal=4, supplement_ordinary_allocator=16,
        supplement_mixed_normal=4, supplement_mixed_allocator=8)
    assert len(reports) == 47 and len(boundaries) == 12
    assert sum(b.get('current_semantic_pass', False) for b in boundaries) == 9
    assert sum(b.get('current_profile_boundary', False) for b in boundaries) == 2
    contracts = [p for p in chosen if re.fullmatch(r'native-evidence/consumer256-contracts-\d+/candidate.wire', p)]
    assert len(contracts) == 1
    contracts = {name: files[name] for name in [contracts[0], contracts[0].rsplit('/', 1)[0] + '/typed.txt']}
    allocator_rows = [e for e in executions if e['stderr']]
    assert len(executions) == 288 and len(allocator_rows) == 214
    assert sum(e['actual']['failures'] for e in allocator_rows) == 98
    assert all(e['actual']['live'] == 0 for e in allocator_rows)
    return dict(verdict='Pass final R1 local actual consumer and source controls; remote CI separate',
        compiler_commit=commit, collector_commit=git('rev-parse', 'HEAD').decode().strip(),
        source_archive=dict(path=str(args.archive), sha256=file_sha(args.archive),
                            bytes=args.archive.stat().st_size, checkout=index['checkout']),
        recovery=dict(index_sha256=sha(args.index.read_bytes()), selected_files=len(chosen),
                      selected_bytes=sum(files[p]['bytes'] for p in chosen), all_selected_bytes_sha256_modes_exact=True,
                      original_report_json_not_modified=True, artifacts_resolved_only_through_index=True),
        oracle_sha256=sha(source_files['tests/callable-instance-forwarding/ORACLE.json']),
        supplement_oracle_sha256=sha(source_files['tests/callable-instance-forwarding/supplement/ORACLE.json']),
        independent_validators={str(p): sha(p.read_bytes()) for p in [core_tool, supplement_tool]},
        core=core, supplement=extra, grouping=dict(grouping), suites=suites,
        counts=dict(tests=19, source_compile_reports=len(reports), native_runs=len(executions),
                    allocator_runs=len(allocator_rows), actual_allocation_failures=98,
                    native_harness_assertions=sum(9 if e['stderr'] else 3 for e in executions),
                    core_semantic_source_rejections=9, pair_semantic_source_rejections=1,
                    profile_unsupported_boundaries=2, unfinished_historical_gaps=1,
                    historical_gap_semantic_passes=0),
        public_raw_wire_contract_artifacts=contracts, boundaries=boundaries,
        compiles=reports, executions=executions, artifact_path_map=artifact_map,
        no_additional_compiler_or_native_execution=True,
        historical_replays_added_to_current_counts=False)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--runner', type=Path, required=True)
    parser.add_argument('--index', type=Path, required=True)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--source-commit', required=True)
    parser.add_argument('--validators', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not args.out.exists()
    receipt = collect(args)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps({k: receipt[k] for k in ['verdict', 'compiler_commit', 'grouping', 'counts']}))
