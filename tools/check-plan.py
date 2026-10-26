#!/usr/bin/env python3
"""Validate construction contracts only; never claims ZIL implementation success."""
from pathlib import Path
import argparse
import copy
import hashlib
import json
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = {'B0', *(f'T{i:02}' for i in range(1, 10)), *(f'W{i}' for i in range(1, 5))}
STATES = {'Planned', 'Ready', 'Doing', 'Review', 'Done', 'Cancelled'}


def load_json(path):
    return json.loads(path.read_text(encoding='utf-8'))


def hashes(root, interfaces):
    entries = []
    for name in sorted(interfaces['normative_inputs']):
        p = root / name
        if not p.resolve().is_relative_to(root.resolve()) or not p.is_file():
            raise ValueError(f'invalid normative input: {name}')
        entries.append(name + '\0' + hashlib.sha256(p.read_bytes()).hexdigest() + '\n')
    return {
        'spec_hash': hashlib.sha256((root / '_zil_design.md').read_bytes()).hexdigest(),
        'normative_hash': hashlib.sha256(''.join(entries).encode('utf-8')).hexdigest(),
    }


def read_tasks(root):
    tasks = []
    for p in sorted((root / '.task').glob('*.md')):
        if p.stem == 'TEMPLATE':
            continue
        found = re.search(r'```json\s*\n(.*?)\n```', p.read_text(encoding='utf-8'), re.S)
        if not found:
            raise ValueError(f'missing task JSON: {p.name}')
        task = json.loads(found.group(1))
        if task.get('id') != p.stem:
            raise ValueError(f'filename/task ID mismatch: {p.name}')
        tasks.append(task)
    return tasks


def literal_prefix(pattern):
    """Conservative path-prefix overlap. False positives need narrower write sets."""
    return re.split(r'[?*\[]', pattern, maxsplit=1)[0]


def overlaps(a, b):
    pa, pb = literal_prefix(a), literal_prefix(b)
    wa, wb = pa != a, pb != b
    if not wa and not wb:
        return a == b
    if wa and b.startswith(pa):
        return True
    if wb and a.startswith(pb):
        return True
    return (wa and wb) and (pa.startswith(pb) or pb.startswith(pa))


def validate(root, tasks, interfaces, plans, conformance=None, allow_pending=False):
    errors, warnings = [], []

    def require(condition, text):
        if not condition:
            errors.append(text)

    by = {t.get('id'): t for t in tasks}
    require(len(by) == len(tasks), 'duplicate task ID')
    require(set(by) == EXPECTED, 'expected exactly B0, T01..T09, W1..W4')
    for tid, t in by.items():
        require(t.get('status') in STATES, f'{tid}: invalid status')
        require(isinstance(t.get('task_revision'), int) and t['task_revision'] > 0, f'{tid}: missing revision')
        require(set(t.get('blocked_by', [])) <= set(by), f'{tid}: unknown dependency')
        require(tid not in t.get('blocked_by', []), f'{tid}: self dependency')
        require(bool(t.get('write_set')), f'{tid}: empty write set')
        require(bool(t.get('done_when')), f'{tid}: no completion criteria')
        require(bool(t.get('requirements')) and bool(t.get('acceptance')), f'{tid}: missing rule/acceptance references')
        require(bool(t.get('plan_case_ids')), f'{tid}: no required protocol fixtures')
        require(t.get('test_plan_manifest') == 'validation/task-test-plans.json', f'{tid}: wrong test-plan manifest')
        require(bool(t.get('test_commands')), f'{tid}: missing executable runner contract')
        for command in t.get('test_commands', []):
            require('tools/run-cases.py' in command and f'--task {tid} ' in command and '--manifest validation/task-test-plans.json' in command and f'--out-root evidence/{tid}' in command,
                    f'{tid}: invalid runner command contract')
        for name in t.get('write_set', []):
            require(not name.startswith('/') and '..' not in Path(name).parts, f'{tid}: unsafe write-set path {name}')

    visiting, visited, ancestor_map = set(), set(), {}

    def ancestors(tid):
        if tid in visiting:
            raise ValueError('task dependency cycle at ' + str(tid))
        if tid in visited:
            return ancestor_map[tid]
        visiting.add(tid)
        found = set()
        for dep in by[tid].get('blocked_by', []):
            if dep in by:
                found.add(dep)
                found.update(ancestors(dep))
        visiting.remove(tid)
        visited.add(tid)
        ancestor_map[tid] = found
        return found

    dag_ok = True
    try:
        for tid in by:
            ancestors(tid)
    except ValueError as ex:
        errors.append(str(ex))
        dag_ok = False

    artifacts = interfaces.get('artifacts', [])
    amap = {a.get('id'): a for a in artifacts}
    require(len(amap) == len(artifacts) and bool(amap), 'duplicate/empty artifact registry')
    actual_producers = {}
    actual_consumers = {aid: [] for aid in amap}
    for tid, t in by.items():
        for aid in t.get('provided_artifacts', []):
            require(aid not in actual_producers, f'{aid}: multiple producing tasks')
            actual_producers[aid] = tid
            require(aid in amap and amap[aid].get('producer') == tid, f'{tid}: artifact producer mismatch for {aid}')
        for aid in t.get('required_artifacts', []):
            require(aid in amap, f'{tid}: unknown required artifact {aid}')
            if aid in amap:
                actual_consumers[aid].append(tid)
                producer = amap[aid].get('producer')
                require(producer in by, f'{aid}: unknown producer')
                if dag_ok:
                    require(producer in ancestor_map[tid], f'{tid}: artifact {aid} producer is not an ancestor')
    require(set(actual_producers) == set(amap), 'unowned or undeclared artifact')
    for aid, artifact in amap.items():
        require(sorted(artifact.get('consumers', [])) == sorted(actual_consumers[aid]), f'{aid}: consumer registry differs from task requirements')
        require(bool(artifact.get('handoff_required')), f'{aid}: missing handoff requirements')

    parallel_pairs = []
    if dag_ok:
        names = sorted(by)
        for i, a in enumerate(names):
            for b in names[i + 1:]:
                if a in ancestor_map[b] or b in ancestor_map[a]:
                    continue
                parallel_pairs.append([a, b])
                conflicts = [(x, y) for x in by[a]['write_set'] for y in by[b]['write_set'] if overlaps(x, y)]
                require(not conflicts, f'parallel write-set conflict {a}/{b}: {conflicts[:3]}')

    cases = plans.get('cases', [])
    cmap = {c.get('id'): c for c in cases}
    require(len(cmap) == len(cases) and bool(cmap), 'duplicate/empty protocol case registry')
    runner = plans.get('runner_contract', {})
    for key in ['selected_min', 'executed_min', 'assertions_min']:
        require(isinstance(runner.get(key), int) and runner[key] >= 1, f'runner: {key} must be positive')
    for key in ['zero_selected', 'zero_executed', 'missing_fixture', 'unknown_status']:
        require(runner.get(key) == 'fail', f'runner: {key} must fail')
    for key in ['unsupported', 'resource_limit', 'timeout', 'infrastructure_failure']:
        require(runner.get(key) == 'non-pass', f'runner: {key} must not pass')
    require(runner.get('all_selected_must_execute') is True, 'runner: selected cases must actually execute')
    claimed = []
    for tid, t in by.items():
        for cid in t.get('plan_case_ids', []):
            claimed.append(cid)
            require(cid in cmap and cmap[cid].get('owner') == tid, f'{tid}: missing/foreign protocol case {cid}')
    require(len(claimed) == len(set(claimed)) and set(claimed) == set(cmap), 'protocol case mapping is incomplete/duplicated')
    for cid, case in cmap.items():
        require(case.get('owner') in by, f'{cid}: unknown owner')
        require(bool(case.get('required_fixture')) and bool(case.get('required_observations')), f'{cid}: missing fixture/observable oracle')
        require(case.get('status') in {'NotImplemented', 'Implemented'}, f'{cid}: implementation status must not pretend execution')
        require(case.get('minimum_executed', 0) >= 1 and case.get('minimum_assertions', 0) >= 1, f'{cid}: empty execution or assertions allowed')

    conformance_by = {}
    if conformance is not None:
        source_cases = conformance.get('cases', [])
        conformance_by = {c.get('id'): c for c in source_cases}
        require(len(conformance_by) == len(source_cases), 'duplicate conformance ID')
        claims = []
        for tid, t in by.items():
            for cid in t.get('case_ids', []):
                claims.append(cid)
                require(cid in conformance_by and conformance_by[cid].get('owner') == tid, f'{tid}: missing/foreign conformance case {cid}')
        require(len(claims) == len(set(claims)) and set(claims) == set(conformance_by), 'conformance case owner mapping incomplete/duplicated')
    elif any(t.get('case_ids') for t in tasks):
        errors.append('task case_ids exist but conformance manifest is missing')

    wanted = hashes(root, interfaces)
    for tid, t in by.items():
        for key in ['spec_hash', 'normative_hash']:
            value = t.get(key)
            if allow_pending and value == 'PENDING':
                warnings.append(f'{tid}: draft {key} remains PENDING')
            else:
                require(value == wanted[key], f'{tid}: stale/missing {key}')
        if t.get('status') in {'Ready', 'Doing', 'Review', 'Done'}:
            require(bool(re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', str(t.get('base_revision') or ''))), f'{tid}: active task lacks precise Git baseline')
            require(all(by[d].get('status') == 'Done' for d in t.get('blocked_by', []) if d in by), f'{tid}: active task has incomplete dependency')
            require(t.get('spec_hash') != 'PENDING' and t.get('normative_hash') != 'PENDING', f'{tid}: active task has unfrozen inputs')
        if t.get('status') == 'Cancelled':
            require(bool(t.get('cancelled_reason')) and bool(t.get('reassigned_to')), f'{tid}: cancellation must retain capability ownership')
        if t.get('status') == 'Done':
            require(bool(t.get('acceptance_record')), f'{tid}: Done lacks integrator decision')
            require(bool(t.get('evidence')), f'{tid}: Done lacks actual evidence')
            observed = set()
            for ref in t.get('evidence', []):
                name = ref if isinstance(ref, str) else ref.get('result_path', '')
                path = root / name
                if not name or not path.resolve().is_relative_to(root.resolve()) or not path.is_file():
                    errors.append(f'{tid}: missing evidence result {name}')
                    continue
                try:
                    run = load_json(path)
                    require(run.get('task') == tid and run.get('status') == 'Pass', f'{tid}: wrong/nonpassing evidence {name}')
                    require(run.get('normative_hash') == t['normative_hash'], f'{tid}: stale evidence {name}')
                    require(run.get('selected', 0) > 0 and run.get('executed') == run.get('selected') and run.get('assertions', 0) > 0, f'{tid}: zero/partial evidence {name}')
                    for c in run.get('cases', []):
                        if c.get('status') == 'Pass' and c.get('executed', 0) > 0 and c.get('assertions', 0) > 0:
                            observed.add(c.get('id'))
                except (ValueError, TypeError, KeyError) as ex:
                    errors.append(f'{tid}: invalid evidence {name}: {ex}')
            require(set(t['plan_case_ids'] + t.get('case_ids', [])) <= observed, f'{tid}: Done has missing actual case executions')
    return {
        'scope': 'Construction plan consistency only; not compiler, language-safety or product acceptance.',
        'ok': not errors,
        'tasks': len(tasks),
        'protocol_case_requirements': len(cases),
        'conformance_sources': len(conformance_by),
        'registered_artifacts': len(amap),
        'unordered_task_pairs_checked': len(parallel_pairs),
        'ready_tasks': [t['id'] for t in tasks if t.get('status') == 'Ready'],
        'errors': errors,
        'warnings': warnings,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--allow-pending', action='store_true', help='allow draft hashes only; never makes a task Ready')
    parser.add_argument('--print-hashes', action='store_true')
    parser.add_argument('--self-test', action='store_true', help='mutate in-memory plan fixtures; does not test ZIL')
    args = parser.parse_args()
    interfaces = load_json(ROOT / 'spec/interfaces.json')
    if args.print_hashes:
        print(json.dumps(hashes(ROOT, interfaces), indent=2))
        return 0
    tasks = read_tasks(ROOT)
    plans = load_json(ROOT / 'validation/task-test-plans.json')
    conformance_path = ROOT / 'conformance/index.json'
    conformance = load_json(conformance_path) if conformance_path.exists() else None
    report = validate(ROOT, tasks, interfaces, plans, conformance, args.allow_pending)
    if args.self_test:
        if not report['ok']:
            print(json.dumps(report, ensure_ascii=False, indent=2))
            return 1
        results = []
        def mutation(name, edit):
            ts, ins, ps = copy.deepcopy(tasks), copy.deepcopy(interfaces), copy.deepcopy(plans)
            edit(ts, ins, ps)
            result = validate(ROOT, ts, ins, ps, conformance, args.allow_pending)
            results.append({'name': name, 'rejected': not result['ok']})
        def task(ts, tid):
            return next(t for t in ts if t['id'] == tid)
        mutation('dependency cycle', lambda ts, ins, ps: task(ts, 'B0')['blocked_by'].append('W4'))
        mutation('missing artifact owner', lambda ts, ins, ps: ins['artifacts'][0].update(producer='MISSING'))
        mutation('parallel write conflict', lambda ts, ins, ps: task(ts, 'T04')['write_set'].append('crates/zil-syntax/**'))
        mutation('Ready without baseline', lambda ts, ins, ps: task(ts, 'B0').update(status='Ready', base_revision=None))
        mutation('zero execution minimum', lambda ts, ins, ps: ps['runner_contract'].update(executed_min=0))
        mutation('unmapped protocol case', lambda ts, ins, ps: task(ts, 'B0')['plan_case_ids'].pop())
        mutation('Done without actual results', lambda ts, ins, ps: task(ts, 'B0').update(status='Done', base_revision='a'*40))
        report['self_tests'] = results
        report['ok'] = report['ok'] and all(x['rejected'] for x in results)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report['ok'] else 1


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (OSError, ValueError, TypeError, KeyError) as ex:
        print(json.dumps({'ok': False, 'scope': 'construction-plan-check', 'errors': [str(ex)]}, ensure_ascii=False))
        sys.exit(2)
