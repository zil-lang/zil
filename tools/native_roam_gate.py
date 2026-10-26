#!/usr/bin/env python3
"""Check a freshly executed native session campaign, without changing it."""
import json
from pathlib import Path


def check(report, commit):
    errors = []
    if not isinstance(report, dict):
        return {'status': 'Fail', 'executed': 0, 'errors': ['report is not an object']}
    expected = {
        'format': 'zil-native-session-gate/1', 'status': 'pass',
        'sourceCommit': commit, 'sourceDirty': False,
        'normal_paths': 84, 'required_normal_paths': 84,
        'normal_actions': 1311, 'required_normal_actions': 1311,
    }
    for key, value in expected.items():
        actual = report.get(key)
        if type(actual) is not type(value) or actual != value:
            errors.append('incorrect or missing ' + key)
    checks = report.get('checks')
    if not isinstance(checks, list):
        checks = []
    if len(checks) < 117:
        errors.append('fewer than 117 supervised runs')
    names = set()
    normal = []
    failures = 0
    for index, row in enumerate(checks):
        if not isinstance(row, dict):
            errors.append(f'run {index} is not an object')
            continue
        name = row.get('name')
        if not isinstance(name, str) or not name or name in names:
            errors.append(f'run {index} has missing or duplicate identity')
        else:
            names.add(name)
        if type(row.get('fault')) is not bool:
            errors.append(f'run {index} lacks explicit fault profile')
        if type(row.get('steps')) is not int or row['steps'] < 0:
            errors.append(f'run {index} has invalid completed-step count')
        if row.get('status') == 'fail':
            failures += 1
            failure = row.get('failure')
            if row.get('fault') is not True or not isinstance(failure, dict) or failure.get('propertyId') != 'session.warm-cold':
                errors.append(f'run {index} has unexpected failure')
        elif row.get('status') != 'pass':
            errors.append(f'run {index} is not pass or expected property failure')
        elif row.get('failure') is not None or row.get('issues') != [] or row.get('reason') is not None:
            errors.append(f'passing run {index} still contains failure, issues or reason')
        # The normal corpus precedes injection, replay and shrink controls.
        if isinstance(name, str) and any(label in name for label in ('-fixed-', '-bounded-bfs', '-seed-')):
            normal.append(row)
            if row.get('status') != 'pass' or row.get('fault') is not False:
                errors.append(f'normal run {index} did not pass without injection')
    if len(normal) != 84 or sum(r.get('steps', 0) for r in normal if type(r.get('steps')) is int) != 1311:
        errors.append('normal run records do not account for 84 paths / 1311 actions')
    if not failures:
        errors.append('missing actual injected property failure')
    def group(label):
        return [row for row in checks if isinstance(row, dict) and isinstance(row.get('name'), str) and row['name'].split('-', 1)[-1] == label]
    for label, count, status, steps, fault in [('historical-replay', 3, 'fail', 8, True), ('reduced-replay', 1, 'fail', 3, True), ('clean-control', 1, 'pass', 3, False)]:
        rows = group(label)
        if len(rows) != count or any(row.get('status') != status or row.get('steps') != steps or row.get('fault') is not fault for row in rows):
            errors.append('missing or inconsistent actual ' + label + ' runs')
    bfs = report.get('bounded_bfs')
    if not isinstance(bfs, dict) or any(type(bfs.get(k)) is not type(v) or bfs.get(k) != v for k, v in {'edges': 56, 'states': 57, 'complete': False, 'stopReason': 'depth', 'fail': 0}.items()):
        errors.append('bounded BFS corpus or completeness claim changed')
    injection = report.get('injection')
    if not isinstance(injection, dict):
        injection = {}
    for key, value in {'production_defect': False, 'property': 'session.warm-cold', 'original_steps': 8, 'replays': 3, 'reduced_steps': 3, 'shrink_stopped': 'complete', 'original_unchanged': True}.items():
        if type(injection.get(key)) is not type(value) or injection.get(key) != value:
            errors.append('incorrect or missing injection.' + key)
    clean = injection.get('clean_control')
    if not isinstance(clean, dict) or clean.get('status') != 'pass' or type(clean.get('steps')) is not int or clean.get('steps') != 3:
        errors.append('missing three-step clean control')
    elif any(clean.get(k) != row.get(k) for row in group('clean-control') for k in ('status', 'steps', 'failure', 'issues', 'reason')):
        errors.append('clean-control summary disagrees with actual run')
    discovery = injection.get('discovery')
    if not isinstance(discovery, dict):
        discovery = {}
    discovered = group('injected-bfs')
    discovered_failures = [row for row in discovered if row.get('status') == 'fail']
    bad = discovery.get('bad')
    if (type(discovery.get('edges')) is not int or discovery.get('edges', -1) < 1
            or len(discovered) != discovery.get('edges', -1) + 1
            or type(discovery.get('fail')) is not int or discovery.get('fail') != len(discovered_failures)
            or not discovered_failures or discovery.get('stopReason') != 'failure'
            or discovery.get('complete') is not False or not isinstance(bad, list)
            or len(bad) != len(discovered_failures)):
        errors.append('discovery summary does not account for actual injected BFS runs')
    else:
        for witness, row in zip(bad, discovered_failures):
            if not isinstance(witness, dict) or not isinstance(witness.get('path'), list) or len(witness['path']) != row.get('steps') or not isinstance(witness.get('why'), dict) or witness['why'].get('status') != 'fail' or witness['why'].get('failure') != row.get('failure'):
                errors.append('discovery witness differs from actual property failure')
    if type(injection.get('shrink_runs')) is not int or injection.get('shrink_runs', 0) < 1 or injection.get('shrink_runs') != len(group('shrink-attempt')):
        errors.append('shrink summary does not account for actual attempts')
    return {'status': 'Pass' if not errors else 'Fail', 'executed': len(checks),
            'normal_paths': len(normal), 'expected_property_failures': failures, 'errors': errors}


def read_check(path, commit):
    try:
        with Path(path).open('rb') as handle:
            data = handle.read(4 * 1024 * 1024 + 1)
        if len(data) > 4 * 1024 * 1024:
            raise ValueError('campaign report exceeds 4 MiB')
        return check(json.loads(data), commit)
    except (OSError, ValueError, TypeError) as error:
        return {'status': 'Fail', 'executed': 0, 'errors': [str(error)]}
