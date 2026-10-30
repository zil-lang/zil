#!/usr/bin/env python3
"""Reconstruct expected parse reuse from retained source bytes and file IDs.

This is an additional raw cache-engagement audit, not a timing estimator or a
replacement for the frozen runner's complete-row/native-oracle validation.
"""
import collections
import hashlib
import json
from pathlib import Path
import sys

PLAN_PATH, RECORDS, OUTPUT = map(Path, sys.argv[1:4])
PLAN = json.loads(PLAN_PATH.read_text())
ROWS = [json.loads(s) for s in RECORDS.read_text().splitlines()]
assert PLAN['query_cache'] is True
LOGS = RECORDS.parent / 'logs'
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
states, groups = {}, {}
checked = 0
missing_parse = collections.Counter()
failures = []
for index, row in enumerate(ROWS):
    if PLAN['schema'].startswith('zil-rust-') and row['arm'] == 'A':
        continue
    try:
        raw_path = LOGS / row['stdout']
        assert sha(raw_path) == row['stdout_sha256']
        report = json.loads(raw_path.read_text())
        assert row['pass'] is True and report['status'] == 'Success' and report['verification_runs'] == 1
        seq = row['work']
        state = states.setdefault(seq, {'requests': [], 'syntax': set(), 'query': set()})
        assert row['request'] == list(PLAN['request_weights'])[len(state['requests'])]
        state['requests'].append(row['request'])
        query = report['query_cache']
        assert query['requested'] is True
        if row.get('phase') == 'check':
            assert query['state'] == 'bypass' and query['stored'] is False
        else:
            key = query['key']
            assert isinstance(key, str) and len(key) == 64
            expected_query_hit = key in state['query']
            assert query['state'] == ('hit' if expected_query_hit else 'miss')
            assert query['stored'] is (not expected_query_hit)
            assert query['reason'] is None and query['write_reason'] is None
            state['query'].add(key)
        cache = report.get('parse_cache')
        if cache is None:
            # Older ZIL baseline has no per-module syntax layer. The CP22 arm
            # must have this report; requested query-cache alone is insufficient.
            assert row['arm'] != 'B'
            missing_parse[row['arm']] += 1
            continue
        assert cache['requested'] is True
        assert cache['reason'] is None and cache['write_reason'] is None
        assert all(type(cache[field]) is int and cache[field] >= 0 for field in
                   ('hits','misses','corrupt','quota_misses','stored','session_hits','parsed'))
        assert cache['session_hits'] == cache['corrupt'] == cache['quota_misses'] == 0
        source_root = Path(row['work']) / 'src'
        identities = set()
        for file_id, path in report['source_files'].items():
            rel = str(Path(path).relative_to(source_root))
            digest = row['source_before'][rel]
            assert row['source_after'][rel] == digest
            # Filesystem adapter assigns equal FileId/ModuleId discovery ordinals.
            identities.add((int(file_id), rel, digest))
        assert len(identities) == len(report['source_files'])
        expected_hits = len(identities & state['syntax'])
        expected_misses = len(identities) - expected_hits
        assert cache['hits'] == expected_hits, (cache, expected_hits)
        assert cache['misses'] == cache['parsed'] == cache['stored'] == expected_misses, (cache, expected_misses)
        state['syntax'].update(identities)
        key = '/'.join((row['arm'], row['case'], row['request']))
        group = groups.setdefault(key, {'rows':0,'modules':set(),'hits':set(),'parsed':set(),'stored':set(),'query_states':collections.Counter()})
        group['rows'] += 1
        group['modules'].add(len(identities))
        for field in ('hits','parsed','stored'): group[field].add(cache[field])
        group['query_states'][query['state']] += 1
        checked += 1
    except (AssertionError, OSError, ValueError, KeyError, TypeError) as error:
        failures.append({'index': index, 'case':row.get('case'), 'request':row.get('request'),'arm':row.get('arm'),'error':repr(error)})
for seq, state in states.items():
    if state['requests'] != list(PLAN['request_weights']):
        failures.append({'work':seq,'error':'incomplete request sequence'})
assert checked > 0
for group in groups.values():
    for field in ('modules','hits','parsed','stored'): group[field] = sorted(group[field])
result = {'reviewer':'/root/incremental_review','pass':not failures,
          'scope':'Additional raw cache engagement audit; not a timing estimator or complete runner validation',
          'plan_sha256':sha(PLAN_PATH),'records_sha256':sha(RECORDS),'script_sha256':sha(Path(__file__)),
          'candidate_parse_reports_checked':checked,'older_baseline_without_parse_report':missing_parse,
          'prediction':'For each isolated four-request sequence, retain identities (file/module discovery ordinal, relative path, source SHA256) from all preceding successes; hits equal current identities already retained, misses/parsed/stored equal new identities.',
          'preconditions':['Explicit trusted local query cache enabled on the frozen relevant arms',
                           'All current requests successful and fresh Verify=1',
                           'No bypass, quota miss, corrupt entry or session hit in candidate syntax reports',
                           'All current generated modules fit codec bounds and publish successfully; this is checked via stored counts and absent write_reason',
                           'FileId equals ModuleId in the filesystem adapter; source/compiler/rule/options identity is fixed within one sequence'],
          'by_arm_case_request':groups,'failures':failures}
OUTPUT.write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'pass':not failures,'checked':checked,'groups':len(groups),'failures':len(failures),'output':str(OUTPUT)}))
if failures: raise SystemExit(1)
