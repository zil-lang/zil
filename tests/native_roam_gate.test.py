#!/usr/bin/env python3
"""Mutate a real run's report in memory; the original report stays immutable."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
from native_roam_gate import check, read_check


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--commit', required=True)
    args = parser.parse_args()
    before = args.report.read_bytes()
    original = json.loads(before)
    assert read_check(args.report, args.commit)['status'] == 'Pass', 'actual campaign is not passing'
    observed = []
    def reject(name, mutate):
        report = copy.deepcopy(original)
        mutate(report)
        result = check(report, args.commit)
        assert result['status'] == 'Fail', name
        observed.append({'case': name, 'status': 'Pass', 'rejected_report': True})
    for field, value in [('sourceDirty', True), ('sourceDirty', 0), ('sourceCommit', 'wrong'),
                         ('status', 'incomplete'), ('normal_paths', 83), ('normal_actions', 1310),
                         ('required_normal_actions', 0), ('checks', []), ('checks', '120')]:
        reject(field + '=' + repr(value), lambda r, k=field, v=value: r.__setitem__(k, v))
    reject('wrong failure property', lambda r: next(c for c in r['checks'] if c['status'] == 'fail')['failure'].__setitem__('propertyId', 'different'))
    reject('normal corpus error', lambda r: r['checks'][0].__setitem__('status', 'error'))
    reject('inflated aggregate', lambda r: r['checks'][0].__setitem__('steps', r['checks'][0]['steps'] + 1))
    reject('duplicate run identity', lambda r: r['checks'][1].__setitem__('name', r['checks'][0]['name']))
    reject('missing control', lambda r: r['injection'].__setitem__('clean_control', {}))
    reject('false exhaustion claim', lambda r: r['bounded_bfs'].__setitem__('complete', True))
    reject('changed original', lambda r: r['injection'].__setitem__('original_unchanged', False))
    reject('missing historical replays', lambda r: r.__setitem__('checks', [c for c in r['checks'] if not c['name'].endswith('-historical-replay')]))
    reject('missing clean run', lambda r: r.__setitem__('checks', [c for c in r['checks'] if not c['name'].endswith('-clean-control')]))
    reject('reduced replay did not fail', lambda r: next(c for c in r['checks'] if c['name'].endswith('-reduced-replay')).update(status='pass', failure=None, issues=[]))
    reject('missing discovery', lambda r: r['injection'].__setitem__('discovery', {}))
    reject('pass carries cleanup error', lambda r: r['checks'][0].__setitem__('issues', [{'phase': 'cleanup', 'name': 'Error'}]))
    reject('missing discovery witness', lambda r: r['injection']['discovery'].__setitem__('bad', []))
    reject('shrink count does not match runs', lambda r: r['injection'].__setitem__('shrink_runs', 0))
    assert check([], args.commit)['status'] == 'Fail'
    assert args.report.read_bytes() == before
    result = {'status': 'Pass', 'executed': len(observed) + 2, 'assertions': len(observed) + 3,
              'scope': 'Actual campaign plus in-memory bad-report controls, not additional compiler executions',
              'source_report_sha256': hashlib.sha256(before).hexdigest(), 'original_unchanged': True,
              'controls': observed}
    print(json.dumps(result))
    return 0


if __name__ == '__main__':
    sys.exit(main())
