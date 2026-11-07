#!/usr/bin/env python3
"""Check static engineering contracts and registries without inferring task state."""
import argparse
import copy
import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = {'B0', *(f'T{i:02}' for i in range(1, 10)), *(f'W{i}' for i in range(1, 5))}
FIELDS = {'source_id', 'title', 'requirements', 'acceptance', 'depends_on',
          'required_artifacts', 'provided_artifacts', 'plan_case_ids',
          'conformance_case_ids', 'profile', 'test_plan_manifest',
          'test_commands', 'conformance_command', 'done_when',
          'input_obligations', 'implementation_obligations', 'boundary_obligations'}

def load(path):
    return json.loads(path.read_text(encoding='utf-8'))

def hashes(root, interfaces):
    entries = []
    for name in sorted(interfaces['normative_inputs']):
        path = (root / name).resolve()
        if not path.is_relative_to(root.resolve()) or not path.is_file():
            raise ValueError(f'invalid normative input: {name}')
        entries.append(name + '\0' + hashlib.sha256(path.read_bytes()).hexdigest() + '\n')
    return {'spec_hash': hashlib.sha256((root / '_zil_design.md').read_bytes()).hexdigest(),
            'normative_hash': hashlib.sha256(''.join(entries).encode('utf-8')).hexdigest()}

def read_contracts(root=ROOT):
    data = load(root / 'validation/engineering-contracts.json')
    if (data.get('schema_version') != 1 or not isinstance(data.get('contracts'), dict)
            or not isinstance(data.get('handoff_protocol'), str) or not data['handoff_protocol'].strip()
            or not isinstance(data.get('acceptance_evidence'), str) or not data['acceptance_evidence'].strip()):
        raise ValueError('missing or invalid static engineering contracts')
    return data['contracts']

def validate(contracts, interfaces, plans, conformance, rules=None, gates=None):
    errors = []
    def need(ok, message):
        if not ok:
            errors.append(message)

    need(len(contracts) == 14, 'expected 14 static contracts')
    ids = [c.get('source_id') for c in contracts.values() if isinstance(c, dict)]
    need(set(ids) == EXPECTED and len(ids) == len(set(ids)) == len(contracts), 'missing or duplicate source ID')
    need(all(re.fullmatch(r'[a-z][a-z0-9]*(?:_[a-z0-9]+)*', key) for key in contracts), 'contract keys must name capabilities')
    dependencies = {}
    for key, c in contracts.items():
        if not isinstance(c, dict):
            need(False, f'{key}: contract must be an object')
            continue
        need(set(c) == FIELDS, f'{key}: missing or unexpected fields {sorted(FIELDS ^ set(c))}')
        for field in ['requirements', 'acceptance', 'provided_artifacts', 'plan_case_ids', 'done_when', 'test_commands']:
            value = c.get(field)
            need(isinstance(value, list) and bool(value) and all(isinstance(x, str) and x.strip() for x in value) and len(set(value)) == len(value), f'{key}: invalid {field}')
        for field in ['depends_on', 'required_artifacts', 'conformance_case_ids']:
            value = c.get(field)
            need(isinstance(value, list) and all(isinstance(x, str) and x for x in value) and len(set(value)) == len(value), f'{key}: invalid {field}')
        for field in ['title', 'input_obligations', 'implementation_obligations', 'boundary_obligations']:
            need(isinstance(c.get(field), str) and bool(c[field].strip()), f'{key}: missing {field}')
        need(c.get('profile') == 'validation/product-gates.json', f'{key}: wrong product profile')
        need(c.get('test_plan_manifest') == 'validation/task-test-plans.json', f'{key}: wrong plan manifest')
        need(set(c.get('depends_on', [])) <= set(contracts) and key not in c.get('depends_on', []), f'{key}: invalid dependency')
        dependencies[key] = c.get('depends_on', [])
        tid = c.get('source_id')
        for command in c.get('test_commands', []):
            need('tools/run-cases.py' in command and f'--task {tid} ' in command and '--manifest validation/task-test-plans.json' in command and f'--out-root evidence/{tid}' in command, f'{key}: invalid plan runner command')
        command = c.get('conformance_command')
        need(isinstance(command, str) and 'tools/run-cases.py' in command and f'--task {tid} ' in command and '--manifest conformance/index.json' in command and f'--out-root evidence/{tid}' in command, f'{key}: invalid conformance runner command')
        if rules is not None:
            need(set(c.get('requirements', [])) <= rules, f'{key}: unknown rule ID')
        if gates is not None:
            need(set(c.get('acceptance', [])) <= gates, f'{key}: unknown acceptance ID')

    ancestors = {}
    visiting = set()
    def ancestor(key):
        if key in visiting:
            raise ValueError('contract dependency cycle at ' + key)
        if key in ancestors:
            return ancestors[key]
        visiting.add(key)
        found = set()
        for dep in dependencies.get(key, []):
            if dep in contracts:
                found.add(dep)
                found.update(ancestor(dep))
        visiting.remove(key)
        ancestors[key] = found
        return found
    try:
        for key in contracts:
            ancestor(key)
    except ValueError as ex:
        errors.append(str(ex))

    artifacts = interfaces.get('artifacts')
    need(isinstance(artifacts, list) and bool(artifacts), 'empty artifact registry')
    artifacts = artifacts if isinstance(artifacts, list) else []
    amap = {a.get('id'): a for a in artifacts if isinstance(a, dict)}
    need(len(amap) == len(artifacts), 'duplicate artifact ID')
    producers, consumers = {}, {key: [] for key in amap}
    for key, c in contracts.items():
        if not isinstance(c, dict):
            continue
        for aid in c.get('provided_artifacts', []):
            need(aid not in producers, f'{aid}: multiple producers')
            producers[aid] = key
            need(aid in amap and amap[aid].get('producer') == c.get('source_id'), f'{key}: artifact producer mismatch {aid}')
        for aid in c.get('required_artifacts', []):
            need(aid in amap, f'{key}: unknown required artifact {aid}')
            if aid in amap:
                consumers[aid].append(c.get('source_id'))
                producer = next((name for name, value in contracts.items() if isinstance(value, dict) and value.get('source_id') == amap[aid].get('producer')), None)
                need(producer in ancestors.get(key, set()), f'{key}: artifact producer is not an ancestor: {aid}')
    need(set(producers) == set(amap), 'unowned artifact')
    for aid, a in amap.items():
        need(sorted(a.get('consumers', [])) == sorted(consumers[aid]) and bool(a.get('handoff_required')), f'{aid}: consumer or handoff mismatch')

    for manifest, field, label in [(plans, 'plan_case_ids', 'plan'), (conformance, 'conformance_case_ids', 'conformance')]:
        cases = manifest.get('cases')
        need(isinstance(cases, list) and bool(cases), f'empty {label} cases')
        cases = cases if isinstance(cases, list) else []
        cmap = {c.get('id'): c for c in cases if isinstance(c, dict)}
        need(len(cmap) == len(cases), f'duplicate {label} ID')
        claimed = []
        for c in contracts.values():
            if not isinstance(c, dict):
                continue
            for cid in c.get(field, []):
                claimed.append(cid)
                need(cid in cmap and cmap[cid].get('owner') == c.get('source_id'), f'foreign or missing {label} case {cid}')
        need(len(claimed) == len(set(claimed)) and set(claimed) == set(cmap), f'incomplete {label} case mapping')
        for cid, case in cmap.items():
            need(case.get('owner') in ids, f'{cid}: unknown owner')
            if label == 'plan':
                need(bool(case.get('required_fixture')) and bool(case.get('required_observations')), f'{cid}: missing fixture or observations')
                need(case.get('status') in {'NotImplemented', 'Implemented'}, f'{cid}: invalid implementation state')
                need(type(case.get('minimum_executed')) is int and case['minimum_executed'] >= 1 and type(case.get('minimum_assertions')) is int and case['minimum_assertions'] >= 1, f'{cid}: zero minimum')
    need(len(plans.get('cases', [])) == 63 and len(conformance.get('cases', [])) == 93, 'case baseline differs from 63/93')
    runner = plans.get('runner_contract', {})
    for field in ['selected_min', 'executed_min', 'assertions_min']:
        need(type(runner.get(field)) is int and runner[field] >= 1, f'runner: {field} must be positive')
    for field in ['zero_selected', 'zero_executed', 'missing_fixture', 'unknown_status']:
        need(runner.get(field) == 'fail', f'runner: {field} must fail')
    for field in ['unsupported', 'resource_limit', 'timeout', 'infrastructure_failure']:
        need(runner.get(field) == 'non-pass', f'runner: {field} must not pass')
    need(runner.get('all_selected_must_execute') is True, 'runner: partial execution forbidden')
    if rules is not None:
        need(set(rules) == {r for c in contracts.values() if isinstance(c, dict) for r in c.get('requirements', [])}, 'rule coverage incomplete')
    if gates is not None:
        need(set(gates) <= {r for c in contracts.values() if isinstance(c, dict) for r in c.get('acceptance', [])}, 'acceptance coverage incomplete')
    return {'ok': not errors, 'scope': 'Static engineering contract and registry consistency; no execution or acceptance claim.',
            'contracts': len(contracts), 'plan_cases': len(plans.get('cases', [])), 'conformance_cases': len(conformance.get('cases', [])), 'artifacts': len(amap), 'errors': errors}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    parser.add_argument('--print-hashes', action='store_true', help='print current normative input hashes without inferring acceptance')
    args = parser.parse_args()
    interfaces = load(ROOT / 'spec/interfaces.json')
    if args.print_hashes:
        print(json.dumps(hashes(ROOT, interfaces), indent=2))
        return 0
    contracts = read_contracts()
    plans = load(ROOT / 'validation/task-test-plans.json')
    conformance = load(ROOT / 'conformance/index.json')
    spec = (ROOT / '_zil_design.md').read_text(encoding='utf-8')
    rules = set(re.findall(r'<a id="(ZIL-[A-Z]+(?:-[A-Z]+)*)"></a>', spec))
    gates = set(re.findall(r'\| (V\d\d) \|', spec))
    report = validate(contracts, interfaces, plans, conformance, rules, gates)
    if args.self_test and report['ok']:
        results = []
        def mutate(name, edit):
            cs, ins, ps, cf = copy.deepcopy((contracts, interfaces, plans, conformance))
            edit(cs, ins, ps, cf)
            results.append({'name': name, 'rejected': not validate(cs, ins, ps, cf, rules, gates)['ok']})
        first = next(iter(contracts))
        mutate('missing field', lambda cs, ins, ps, cf: cs[first].pop('done_when'))
        mutate('missing implementation obligation', lambda cs, ins, ps, cf: cs[first].update(implementation_obligations=''))
        mutate('empty case list', lambda cs, ins, ps, cf: cs[first].update(plan_case_ids=[]))
        mutate('redirected evidence', lambda cs, ins, ps, cf: cs[first].update(test_commands=[cs[first]['test_commands'][0].replace('--out-root evidence/B0', '--out-root tmp')]))
        mutate('dependency cycle', lambda cs, ins, ps, cf: cs[first].update(depends_on=[next(reversed(cs))]))
        mutate('missing artifact owner', lambda cs, ins, ps, cf: ins['artifacts'][0].update(producer='MISSING'))
        mutate('unmapped case', lambda cs, ins, ps, cf: ps['cases'].pop())
        mutate('zero execution minimum', lambda cs, ins, ps, cf: ps['runner_contract'].update(executed_min=0))
        report['self_tests'] = results
        report['ok'] = all(x['rejected'] for x in results)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report['ok'] else 1

if __name__ == '__main__':
    try:
        sys.exit(main())
    except (OSError, ValueError, TypeError, KeyError) as ex:
        print(json.dumps({'ok': False, 'errors': [str(ex)]}, ensure_ascii=False))
        sys.exit(2)
