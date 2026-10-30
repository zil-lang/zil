#!/usr/bin/env python3
"""Read-only, independent arithmetic and byte-level review of compile4 run 1.

Does not import or execute a benchmark runner, compile code, execute artifacts,
rewrite a plan, or change evidence. Only this review's JSON/Markdown are written.
"""
from collections import Counter, defaultdict
from pathlib import Path, PurePosixPath
import hashlib
import io
import itertools
import json
import lzma
import math
import os
import random
import re
import stat
import subprocess
import tarfile
import time

ROOT = Path('/workspace/scratch/87d454884917')
E = ROOT / 'recovery/compile4-run1/compile4-evidence'
REPO = ROOT / 'zil'
OUT = ROOT / 'recovery/reviews/COMPILE4-INDEPENDENT-BENCH.json'
MD = OUT.with_suffix('.md')
BASE = '64072793a0f41994f216a6207bf49ee1e2d224f8'
HEAD = '0393660f7dc8414b0ab8c77cd3d576395f4fe095'
TREES = {'A': 'dca1c046f0e03cfa037ac49e7275c5b114c3d3cc', 'B': '0221fa705e8dc926c54f6b85f2dc9ef3af98559a'}
WEIGHTS = {'tree4x2': .05, 'tree32x8': .15, 'tree256x8': .30, 'tree1024x8': .30, 'retained': .20}
REQUESTS = {'cold_process': .1, 'unchanged': .35, 'private_edit': .4, 'public_edit': .15}
SNAPSHOT = {'cold_process': 'base', 'unchanged': 'base', 'private_edit': 'private', 'public_edit': 'public'}
TAR_SHA = 'd56bb7d139bee4cf0fba8c11ccb5a7815827cf12031336445ae599503938fbc3'
ARCHIVE = REPO / 'evidence/research/compile-performance/COMPILE4/round1/raw-evidence.tar.xz'
ARCHIVE_SHA = 'ab3c783d01494c95f794855598e5e832f592e7b9994aa2634a545431aac6fc4a'
TAR_STREAM_SHA = 'a8d98528c59e7f00ed559d3bc2458f04dd86b27c2537a8d3eb63a2b5d9f5ad0f'
EMPTY_SHA = hashlib.sha256(b'').hexdigest()
checks, failures, differences = 0, [], []
hash_cache, git_cache = {}, {}
counts = Counter()
cache_observations = defaultdict(lambda: defaultdict(Counter))
usage_observations = defaultdict(set)
unit_observations = defaultdict(set)
query_keys = defaultdict(dict)
plans, comparisons = {}, {}


def check(condition, where):
    global checks
    checks += 1
    if not condition:
        failures.append(where)


def digest(path):
    path = Path(path)
    s = path.stat()
    key = (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns)
    if key not in hash_cache:
        with path.open('rb') as stream:
            hash_cache[key] = hashlib.file_digest(stream, 'sha256').hexdigest()
    return hash_cache[key]


def j(path):
    return json.loads(Path(path).read_text())


def rows(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines()]


def gitbytes(path, revision=HEAD):
    key = (revision, path)
    if key not in git_cache:
        git_cache[key] = subprocess.check_output(['git', '-C', str(REPO), 'show', revision + ':' + path])
    return git_cache[key]


result = j(E / 'result.json')
OLDROOT = PurePosixPath(result['original_evidence_root'])


def relocated(original):
    return E / PurePosixPath(original).relative_to(OLDROOT)


def same_number(actual, expected, where):
    difference = abs(actual - expected)
    differences.append(difference)
    check(math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-12), where)


def compare_numbers(actual, expected, where):
    for key, value in expected.items():
        if key not in actual:
            check(False, where + '/' + key + ': missing')
        elif isinstance(value, dict):
            compare_numbers(actual[key], value, where + '/' + key)
        elif isinstance(value, (float, int)):
            same_number(actual[key], value, where + '/' + key)


def native_check(row, logs, planroot, inp, expected, executable=None, bridge=False):
    label = str(logs.relative_to(E)) + '/' + row['stdout']
    counts['native_oracle_startup_warmup_or_measured_checks'] += 1
    check(row.get('exit') == 0 and not row.get('timeout', False), label + ': native exit')
    check(row.get('matches' if bridge else 'pass') is True, label + ': native reported pass')
    check(row['input'] == inp and row['expected'] == expected, label + ': native input binding')
    check(digest(logs / row['stdout']) == digest(planroot / expected) == row['stdout_sha256'], label + ': native oracle bytes')
    check(digest(logs / row['stderr']) == EMPTY_SHA == row['stderr_sha256'], label + ': native stderr')
    if not bridge:
        check(row['input_sha256'] == row['input_after_sha256'] == digest(planroot / inp), label + ': input stability')
        check(row['expected_stdout_sha256'] == digest(planroot / expected), label + ': expected binding')
        check(row['executable'] == row['executable_after'], label + ': executable in-window identity')
        if executable is not None:
            check(row['executable'] == executable, label + ': executable cross-record identity')


def report_check(raw, row, plan, window, sample_kind):
    label = window + '/' + sample_kind + '/' + str(row['index'])
    counts['zil_cli_reports'] += 1
    n, q = raw['native_codegen'], raw['query_cache']
    cache = n['object_cache']
    enabled = window == 'o0-bridge' or plan.get('cache_mode') == 'incremental'
    query = plan.get('query_cache', False)
    closed = plan.get('closed_executable', False)
    check(raw['status'] == 'Success' and raw['verification_runs'] == 1 and raw['timings'] is None, label + ': success/current Verify/no timings')
    check(raw['diagnostics'] == [], label + ': no diagnostics')
    check(n['backend'] == 'llvm-api' and n['requested_units'] == 8, label + ': LLVM API/requested CGU')
    check(type(n['closed_executable']) is bool and n['closed_executable'] == closed, label + ': closed mode')
    check(type(n['internalized_functions']) is int and n['internalized_functions'] >= 0 and (closed or n['internalized_functions'] == 0), label + ': internalization count')
    check(cache['requested'] is enabled and cache['enabled'] is enabled and cache['disabled_reason'] is None, label + ': object cache mode')
    for key in ('hits', 'misses', 'corrupt', 'stored', 'write_failures', 'key_ns', 'lookup_ns', 'current_preflight_ns'):
        check(type(cache[key]) is int and cache[key] >= 0, label + ': object counter ' + key)
    check(cache['corrupt'] == cache['write_failures'] == 0, label + ': object cache failures')
    objects = n['object_reports']
    check(len(objects) == n['actual_units'] and 1 <= n['actual_units'] <= 8, label + ': actual object coverage')
    if enabled:
        check(cache['hits'] + cache['misses'] == n['actual_units'] and cache['stored'] == cache['misses'], label + ': object totals')
        if row['request'] == 'cold_process':
            check(cache['hits'] == 0, label + ': cold object hits')
        if row['request'] == 'unchanged':
            check(cache['hits'] == n['actual_units'], label + ': unchanged all objects reused')
    else:
        check(all(cache[k] == 0 for k in ('hits', 'misses', 'corrupt', 'stored', 'write_failures')), label + ': cold objects disabled')
    for index, obj in enumerate(objects):
        olabel = label + '/unit' + str(index)
        check(obj['backend'] == 'llvm-api' and obj['llvm_version'] == '23.1.4' and obj['triple'] == 'x86_64-unknown-linux-gnu', olabel + ': target/version')
        if obj.get('cache', {}).get('state') == 'hit':
            check(obj['input_kind'] == 'object-cache' and obj['producer_verified_before_after'] is True and obj['llvm_verification_this_request'] is False, olabel + ': hit verification accounting')
            check(obj['preflight_ns'] > 0 and obj['cache']['current_preflight_ns'] == obj['preflight_ns'], olabel + ': current machine preflight')
            check(all(obj[k] == 0 for k in ('builder_ns', 'parse_ns', 'optimize_ns', 'emit_ns')), olabel + ': hit skips LLVM generation only')
        else:
            check(obj['input_kind'] == 'builder' and obj['verify_before_after'] is True and obj['cpu'] == 'x86-64' and obj['features'] == '' and obj['pipeline'] == 'default<' + plan['profile'] + '>', olabel + ': builder pipeline')
        if enabled:
            check(obj['cache']['state'] in ('hit', 'miss') and obj['cache']['corrupt'] is False and re.fullmatch('[0-9a-f]{64}', obj['cache']['action_key']) is not None, olabel + ': action cache identity')
    if enabled:
        check(sum(o['cache']['state'] == 'hit' for o in objects) == cache['hits'], label + ': object detail hits')
    check(q['requested'] is query and q['reason'] is None and q['write_reason'] is None, label + ': query mode/reasons')
    for key in ('ct_steps', 'ct_nodes', 'ct_functions'):
        check(type(q[key]) is int and q[key] >= 0, label + ': bounded query usage ' + key)
    if query:
        hit = row['request'] == 'unchanged'
        check(q['state'] == ('hit' if hit else 'miss') and q['stored'] is (not hit) and re.fullmatch('[0-9a-f]{64}', q['key']) is not None, label + ': query hit/store/key')
        case = next(c for c in plan['cases'] if c['id'] == row['case'])
        check(q['ct_steps'] == 0 and q['ct_nodes'] > 0 and q['ct_functions'] == case['source_sizes']['zil']['functions'], label + ': recorded CT usage')
        usage_observations[(row['case'], row['request'])].add(tuple(q[k] for k in ('ct_steps', 'ct_nodes', 'ct_functions')))
        key = (window, sample_kind, row['work'])
        query_keys[key][row['request']] = {'key': q['key'], 'usage': [q[k] for k in ('ct_steps', 'ct_nodes', 'ct_functions')]}
    else:
        check(q['state'] == 'disabled' and q['stored'] is False and q['key'] is None and all(q[k] == 0 for k in ('ct_steps', 'ct_nodes', 'ct_functions')), label + ': disabled query counters')
    check(raw['disk_cache'] in ('missing', 'hit', 'disabled'), label + ': source cache status')
    if not enabled:
        check(raw['disk_cache'] == 'disabled', label + ': no source candidates in cold mode')
    for command in raw['commands']:
        check(command[0] == plan['binaries']['clang']['path'] and '-O' + plan['profile'][1:] in command and '--target=x86_64-unknown-linux-gnu' in command, label + ': actual link driver/profile')
        check('--ld-path=/home/runner/work/_temp/compile4-llvm-23.1.4/bin/ld.lld' in command, label + ': actual frozen LLD')
    check(len(raw['commands']) == 1, label + ': complete single link')
    if sample_kind == 'build':
        for key in ('hits', 'misses', 'stored', 'corrupt', 'write_failures'):
            cache_observations[window][row['arm'] + '/' + row['request']][key] += cache[key]
        cache_observations[window][row['arm'] + '/' + row['request']]['rows'] += 1
        unit_observations[(window, row['case'])].add((n['actual_units'], n['jobs'], n['fallback'], n['internalized_functions']))


def expected_command(plan, row, bridge):
    if bridge:
        work = PurePosixPath(row['argv'][2]).parent.parent
        return [plan['binaries'][row['arm']]['path'], 'build', str(work / 'src/main.zil'), '--json', '--cache-dir', str(work / 'source-candidates'), '--clang', plan['binaries']['clang']['path'], '--opt', 'O0', '--codegen-units', '8', '--codegen-jobs', '4', '-o', str(work / 'program'), '--object-cache', str(work / 'objects')]
    work = PurePosixPath(row['work'])
    if row['arm'] == 'A':
        command = [plan['binaries']['A']['path'], str(work / 'src/main.rs'), '--edition=2024', '--crate-name', 'zil_cp08_ref', '-Awarnings', '--target', 'x86_64-unknown-linux-gnu', '-C', 'target-cpu=x86-64', '-C', 'opt-level=' + plan['profile'][1:], '-C', 'debuginfo=0', '-C', 'overflow-checks=on', '-C', 'panic=abort', '-C', 'embed-bitcode=no', '-o', str(work / 'program')]
        if plan['cache_mode'] == 'incremental':
            command += ['-C', 'incremental=' + str(work / 'rust-incremental')]
        return command
    command = [plan['binaries']['B']['path'], 'build', str(work / 'src/main.zil'), '-o', str(work / 'program'), '--json', '--backend', 'llvm-api', '--clang', plan['binaries']['clang']['path'], '--opt', plan['profile'], '--codegen-units', '8', '--codegen-jobs', '4']
    if plan['cache_mode'] == 'incremental':
        command += ['--cache-dir', str(work / 'source-candidates'), '--object-cache', str(work / 'objects'), '--query-cache', str(work / 'queries')]
    if plan['closed_executable']:
        command += ['--closed-executable']
    return command


def validate_build(folder, plan, sample, bridge=False):
    data = rows(folder / sample / 'records.jsonl')
    planroot, logs = folder / 'plan', folder / sample / 'logs'
    receipt = j(folder / sample / ('result.json' if sample == 'preflight' else 'complete.json'))
    expected_order = plan['order'] if sample == 'build' else [dict(block=0, case=c['id'], position=i, arm=a, **({'phase': 'build'} if bridge else {})) for c in plan['cases'] for i, a in enumerate('AB')]
    expected = [{**seq, 'request': request} for seq in expected_order for request in plan['request_weights']]
    key = ('block', 'case', 'position', 'arm', 'request') + (('phase',) if bridge else ())
    check([tuple(r[k] for k in key) for r in data] == [tuple(r[k] for k in key) for r in expected], folder.name + '/' + sample + ': full ordered coverage')
    check(receipt['pass'] is True and receipt['failures'] == [] and receipt['rows'] == len(data) and receipt['plan_sha256'] == digest(planroot / 'plan.json') and receipt['records_sha256'] == digest(folder / sample / 'records.jsonl'), folder.name + '/' + sample + ': completion/preflight binding')
    if sample == 'build':
        check(receipt['preflight_sha256'] == digest(folder / 'preflight/result.json'), folder.name + ': matched preflight')
    cases = {c['id']: c for c in plan['cases']}
    for index, row in enumerate(data):
        label = folder.name + '/' + sample + '/' + str(index)
        counts['measured_build_rows' if sample == 'build' else 'preflight_build_rows'] += 1
        check(row['index'] == index and row['pass'] is True and row['exit'] == 0 and row['timeout'] is False and row['plan_sha256'] == digest(planroot / 'plan.json'), label + ': row state/identity')
        for k in ('wall_seconds', 'cpu_user', 'cpu_system'):
            check(type(row[k]) in (int, float) and math.isfinite(row[k]) and row[k] >= 0, label + ': numeric ' + k)
        check(row['wall_seconds'] > 0, label + ': positive wall')
        check(row['snapshot'] == SNAPSHOT[row['request']] and row['argv'] == expected_command(plan, row, bridge), label + ': exact argv/snapshot')
        if not bridge:
            check(row['sample_kind'] == ('build' if sample == 'build' else 'preflight-unscored'), label + ': sample kind')
            for stream in ('stdout', 'stderr'):
                check(digest(logs / row[stream]) == row[stream + '_sha256'], label + ': raw ' + stream)
        check((logs / row['stderr']).stat().st_size == 0, label + ': compiler stderr empty')
        case = cases[row['case']]
        language = 'zil' if bridge or row['arm'] == 'B' else 'rust'
        prefix = case['root'] + '/' + row['snapshot'] + '/' + language + '/'
        if bridge:
            prefix = prefix.removeprefix('corpus/')
        expected_sources = {k[len(prefix):]: v for k, v in plan['fixtures_sha256'].items() if k.startswith(prefix)}
        check(bool(expected_sources) and row['source_before'] == row['source_after'] == expected_sources, label + ': exact source bytes before/after')
        if bridge or row['arm'] == 'B':
            raw = j(logs / row['stdout'])
            for field, value in row.items():
                if bridge and field.startswith('reported_'):
                    check(raw.get(field[len('reported_'):]) == value, label + ': raw reported ' + field)
            if not bridge:
                check(row['reported'] == {k: raw.get(k) for k in row['reported']}, label + ': raw reported JSON')
            report_check(raw, row, plan, folder.name, sample)
        else:
            check((logs / row['stdout']).stat().st_size == 0, label + ': rust compiler stdout empty')
        check(len(row['checks']) == len(case['inputs']), label + ': oracle count')
        for obs, inp in zip(row['checks'], case['inputs']):
            native_check(obs, logs, planroot, inp['path'], inp['expected'][row['snapshot']], row.get('executable'), bridge)
    return data


def validate_artifacts(folder, plan):
    artifacts = j(folder / 'preflight/artifacts.json')
    receipt = j(folder / 'preflight/result.json')
    check(receipt['warmup'] is True and receipt['artifacts_sha256'] == digest(folder / 'preflight/artifacts.json'), folder.name + ': warmup artifact receipt')
    check([(r['case'], r['arm']) for r in artifacts] == [(c['id'], a) for c in plan['cases'] for a in 'AB'], folder.name + ': saved executable coverage')
    cases = {c['id']: c for c in plan['cases']}
    for artifact in artifacts:
        exe = artifact['executable']; path = relocated(exe['path'])
        check(path.stat().st_size == exe['size'] and digest(path) == exe['sha256'] and os.access(path, os.X_OK), folder.name + ': saved executable bytes/mode')
        case = cases[artifact['case']]
        native_check(artifact['startup'], folder / 'preflight/logs', folder / 'plan', 'inputs/empty.bin', 'expected/empty.bin', exe)
        native_check(artifact['warmup'], folder / 'preflight/logs', folder / 'plan', case['input'], case['expected'], exe)
    return {(a['case'], a['arm']): a['executable'] for a in artifacts}


def validate_runtime(folder, plan, artifacts):
    data = rows(folder / 'runtime/records.jsonl')
    key = ('block', 'case', 'position', 'arm')
    check([tuple(r[k] for k in key) for r in data] == [tuple(r[k] for k in key) for r in plan['order']], folder.name + ': runtime ordered coverage')
    receipt = j(folder / 'runtime/complete.json')
    check(receipt['pass'] is True and receipt['failures'] == [] and receipt['rows'] == len(data) and receipt['records_sha256'] == digest(folder / 'runtime/records.jsonl') and receipt['plan_sha256'] == digest(folder / 'plan/plan.json') and receipt['preflight_sha256'] == digest(folder / 'preflight/result.json') and receipt['sample_kind'] == 'runtime', folder.name + ': runtime completion')
    cases = {c['id']: c for c in plan['cases']}
    for index, row in enumerate(data):
        counts['measured_runtime_rows'] += 1
        check(row['index'] == index and row['sample_kind'] == 'runtime' and row['plan_sha256'] == digest(folder / 'plan/plan.json') and row['preflight_sha256'] == digest(folder / 'preflight/result.json'), folder.name + ': runtime row identity')
        check(all(type(row[k]) in (int, float) and math.isfinite(row[k]) and row[k] >= 0 for k in ('wall_seconds', 'cpu_user', 'cpu_system')) and row['wall_seconds'] > 0, folder.name + ': runtime timing values')
        case = cases[row['case']]
        native_check(row, folder / 'runtime/logs', folder / 'plan', case['input'], case['expected'], artifacts[(row['case'], row['arm'])])
    return data


def arithmetic(data, request_weights, metric):
    """Independent implementation: no imports/calls to original estimators."""
    cells = defaultdict(list)
    for row in data:
        value = row['cpu_user'] + row['cpu_system'] if metric == 'cpu' else row['wall_seconds']
        cells[(row['case'], row.get('request', 'runtime'), row['arm'], row['block'])].append(value)
    means = {}
    for key, values in cells.items():
        check(len(values) == 2, 'arithmetic: each block/case/request/arm has exactly two observations')
        means[key] = math.fsum(values) / len(values)
    def estimates(blocks, weights=WEIGHTS):
        per_cell = {}
        for case in weights:
            for request in request_weights:
                v = {arm: sorted(means[(case, request, arm, block)] for block in blocks)[len(blocks) // 2] for arm in 'AB'}
                per_cell[case + '/' + request] = {**v, 'B_over_A': v['B'] / v['A']}
        by_case, by_request = {}, {}
        for case in weights:
            v = {a: math.fsum(request_weights[r] * per_cell[case + '/' + r][a] for r in request_weights) for a in 'AB'}
            by_case[case] = {**v, 'B_over_A': v['B'] / v['A']}
        for request in request_weights:
            v = {a: math.fsum(weights[c] * per_cell[c + '/' + request][a] for c in weights) for a in 'AB'}
            by_request[request] = {**v, 'B_over_A': v['B'] / v['A']}
        total = {a: math.fsum(weights[c] * by_case[c][a] for c in weights) for a in 'AB'}
        return {**total, 'B_over_A': total['B'] / total['A'], 'by_case': by_case, 'by_request': by_request, 'by_case_request': per_cell}
    point = estimates([0, 1, 2])
    rng = random.Random(20261030)
    draws = [estimates(rng.choices([0, 1, 2], k=3)) for _ in range(2000)]
    def interval(get):
        ordered = sorted(get(draw) for draw in draws)
        return [ordered[49], ordered[1949]]
    point['bootstrap95'] = interval(lambda v: v['B_over_A'])
    for category in ('by_case', 'by_request', 'by_case_request'):
        for key, value in point[category].items():
            value['bootstrap95'] = interval(lambda v: v[category][key]['B_over_A'])
            value['block_ratios'] = [estimates([b])[category][key]['B_over_A'] for b in range(3)]
    blocks = [estimates([b]) for b in range(3)]
    point['block_ratios'] = [b['B_over_A'] for b in blocks]
    point['paired_block_geometric_ratio_sensitivity'] = math.exp(math.fsum(math.log(b['B_over_A']) for b in blocks) / 3)
    exhaustive = [estimates(b)['B_over_A'] for b in itertools.product(range(3), repeat=3)]
    point['all_27_ordered_block_resamples'] = {'minimum': min(exhaustive), 'maximum': max(exhaustive), 'below_one': sum(x < 1 for x in exhaustive), 'total': 27}
    point['leave_one_case_out_sensitivity'] = {}
    for omitted in WEIGHTS:
        weights = {c: w / (1 - WEIGHTS[omitted]) for c, w in WEIGHTS.items() if c != omitted}
        point['leave_one_case_out_sensitivity'][omitted] = estimates([0, 1, 2], weights)['B_over_A']
    point['weighted_cost_share_by_case'] = {c: {a: WEIGHTS[c] * point['by_case'][c][a] / point[a] for a in 'AB'} for c in WEIGHTS}
    return point


def audit_summary(folder, kind, plan, data):
    bridge = folder.name == 'o0-bridge'
    path = folder / ('summary.json' if bridge else kind + '-summary.json')
    summary = j(path)
    check(summary['rows'] == len(data) and summary['coverage_complete'] is True and summary['failures'] == [] and summary['product_gates'] == 'NotRun' and summary['verdict'] == 'ExploratoryComplete_NotAProductGate', folder.name + '/' + kind + ': summary completeness')
    check(summary['plan_sha256'] == digest(folder / 'plan/plan.json') and summary['records_sha256'] == digest(folder / kind / 'records.jsonl'), folder.name + '/' + kind + ': summary byte binding')
    if not bridge:
        check(summary['completion_receipt_matches'] is True and summary['preflight_sha256'] == digest(folder / 'preflight/result.json'), folder.name + '/' + kind + ': summary preflight')
    reference = summary['by_phase']['build'] if bridge else summary
    weights = plan['request_weights'] if kind == 'build' else {'runtime': 1.0}
    recomputed = {metric: arithmetic(data, weights, metric) for metric in ('wall', 'cpu')}
    for metric in ('wall', 'cpu'):
        compare_numbers(recomputed[metric], reference[metric], folder.name + '/' + kind + '/' + metric)
    for actual, expected in zip(recomputed['wall']['bootstrap95'], reference['exploratory_paired_block_bootstrap95']):
        same_number(actual, expected, folder.name + '/' + kind + ': bootstrap interval')
    if bridge:
        compare_numbers(recomputed['wall']['by_case_request'], reference['by_case_request'], folder.name + ': case/request medians')
        digests = defaultdict(set)
        for row in data:
            digests[(row['case'], row['snapshot'])].add(row['executable_sha256'])
        same = all(len(d) == 1 for d in digests.values())
        check(same == reference['same_binary_for_each_case_snapshot'], 'bridge: binary equality summary')
        recomputed['same_binary_for_each_case_snapshot'] = same
        recomputed['distinct_binary_counts_case_snapshot'] = {c + '/' + s: len(d) for (c, s), d in digests.items()}
    recomputed.update(summary_sha256=digest(path), records_sha256=digest(folder / kind / 'records.jsonl'), rows=len(data), arm_A='baseline' if bridge else 'Rust', arm_B='candidate', profile=plan['profile'], cache_mode='source-object-no-query' if bridge else plan['cache_mode'], query_cache=plan.get('query_cache', False), closed_executable=plan.get('closed_executable', False))
    comparisons[folder.name + '/' + kind] = recomputed


def main():
    check(result['candidate_revision'] == HEAD and result['base_revision'] == BASE and result['candidate_crates_tree'] == TREES['B'] and result['base_crates_tree'] == TREES['A'], 'top-level exact source identity')
    check(result['status'] == 'Complete_Exploratory_NotAProductGate' and result['product_gates'] == 'NotRun', 'top-level exploratory status')
    check(result['runner_sha256'] == hashlib.sha256(gitbytes('tools/research/compile-performance/compile4_ci.py')).hexdigest(), 'top-level exact orchestration source')
    check(len(result['windows']) == 8, 'eight completed named summaries')
    for window in result['windows']:
        check(digest(E / window['summary']) == window['sha256'], 'top-level summary binding:' + window['name'])
    check(digest(E / 'manifest.json') == result['manifest_sha256'] == hashlib.sha256(gitbytes('tools/research/compile-performance/compile4-run.json')).hexdigest(), 'committed manifest exact bytes')
    check((E.parent / 'compile4-setup/commit.txt').read_text().strip() == HEAD, 'Actions setup source SHA')
    manifest = j(E / 'manifest.json')
    check(manifest['blocks'] == 3 and manifest['seed'] == 20261030 and manifest['llvm_version'] == '23.1.4' and manifest['rust_version'] == '1.99.1', 'manifest fixed parameters')
    inventory = j(E / 'files.json')
    observed = {str(p.relative_to(E)) for p in E.rglob('*') if p.is_file() and p.name != 'files.json'}
    check(observed == set(inventory), 'all files exactly match inventory membership')
    for name, entry in inventory.items():
        path = E / name
        check(path.stat().st_size == entry['bytes'] and digest(path) == entry['sha256'], 'inventory:' + name)
    archive = ARCHIVE
    archive_names = set()
    archive_available = archive.stat().st_size > 0
    if archive_available:
        check(digest(archive) == ARCHIVE_SHA, 'recompressed xz SHA256')
        stream_hash, stream_bytes = hashlib.sha256(), 0
        with lzma.open(archive, 'rb') as stream:
            while chunk := stream.read(1024 * 1024):
                stream_hash.update(chunk)
                stream_bytes += len(chunk)
        check(stream_hash.hexdigest() == TAR_STREAM_SHA and stream_bytes == 364175360, 'identical uncompressed original tar byte stream')
        with tarfile.open(archive, 'r|xz') as tar:
            for member in tar:
                if member.isdir():
                    continue
                check(member.isfile(), 'tar contains regular files only: ' + member.name)
                relative = PurePosixPath(member.name).relative_to('compile4-evidence').as_posix()
                path = E / relative
                archive_names.add(relative)
                check(member.size == path.stat().st_size and stat.S_IMODE(path.stat().st_mode) == member.mode, 'tar size/POSIX mode:' + relative)
                with tar.extractfile(member) as stream:
                    check(hashlib.file_digest(stream, 'sha256').hexdigest() == digest(path), 'tar bytes:' + relative)
        check(archive_names == observed | {'files.json'}, 'tar/extracted exact file membership')
    print('all extracted file hashes checked', len(inventory), 'original tar available', archive_available, flush=True)
    builds = j(E / 'compiler-builds.json')
    for arm, key, revision in (('A', 'baseline', BASE), ('B', 'candidate', HEAD)):
        entry = builds[key]; path = relocated(entry['path'])
        check(entry['source_commit'] == revision and digest(path) == entry['sha256'] and path.stat().st_size == entry['bytes'] and os.access(path, os.X_OK), 'compiler binary:' + key)
    command_receipts = [j(p) for p in sorted((E / 'logs').glob('*.json'))]
    for command in command_receipts:
        check(command['status'] == 'Pass' and command['exit'] == 0 and not command.get('timeout'), 'orchestration command:' + command['label'])
        for stream in ('stdout', 'stderr'):
            check(digest(E / command[stream]) == command[stream + '_sha256'], 'orchestration log:' + command['label'] + '/' + stream)
    for left, right in zip(command_receipts, command_receipts[1:]):
        check(left['started_unix'] + left['seconds'] <= right['started_unix'] + .05, 'orchestration seriality:' + left['label'])
    first_measurement = min(c['started_unix'] for c in command_receipts if 'measure' in c['label'])
    check(all(c['started_unix'] + c['seconds'] < first_measurement for c in command_receipts if c['label'] in ('baseline-build', 'candidate-build', 'candidate-check') or c['label'].startswith('candidate-tests-')), 'compiler builds/tests precede timing')
    test_counts = []
    for command in command_receipts:
        if command['label'].startswith('candidate-tests-'):
            outcomes = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;', (E / command['stdout']).read_text())
            check(bool(outcomes) and all(int(p) > 0 and all(int(v) == 0 for v in (f, i, m, x)) for p, f, i, m, x in outcomes), command['label'] + ': nonzero/all-success test targets')
            test_counts.append({'label': command['label'], 'targets': len(outcomes), 'passed': sum(int(r[0]) for r in outcomes)})
    toolchain = j(E / 'logs/001-toolchain-check.stdout')
    check(toolchain['status'] == 'Pass' and toolchain['checks']['llvm']['version'] == '23.1.4' and toolchain['checks']['rustc']['version'] == '1.99.1', 'actual toolchain preflight')
    for window in ('o0-bridge', 'rust-o0-cold', 'rust-o0-incremental', 'rust-o2-closed'):
        folder = E / 'windows' / window
        plan = j(folder / 'plan/plan.json'); plans[window] = plan
        bridge = window == 'o0-bridge'
        check(plan['schema'] == ('zil-compile-performance/1' if bridge else 'zil-rust-compile-performance/3'), window + ': schema')
        check({c['id']: c['weight'] for c in plan['cases']} == WEIGHTS and len(plan['cases']) == 5 and plan['selected_original_weight'] == 1.0 and plan['blocks'] == 3 and plan['seed'] == 20261030, window + ': frozen weights/coverage/blocks')
        check(plan['units'] == 8 and plan['jobs'] == 4 and plan['cpus'] == [0, 1], window + ': requested schedule/affinity')
        check(plan['runner_source_commit'] == HEAD, window + ': runner source')
        helper = 'bench.py' if bridge else 'rust_bench.py'
        check(hashlib.sha256(gitbytes('tools/research/compile-performance/' + helper)).hexdigest() == plan['runner_sha256'], window + ': original runner bytes')
        for path, expected in plan['dependencies_sha256' if bridge else 'helper_sha256'].items():
            check(hashlib.sha256(gitbytes(path)).hexdigest() == expected, window + ': helper/oracle source ' + path)
        for arm, identity in plan['source_identity'].items():
            check(identity == {'commit': BASE if arm == 'A' else HEAD, 'crates_tree': TREES[arm]}, window + ': compiler source ' + arm)
        for arm, build in ((('A', 'baseline'), ('B', 'candidate')) if bridge else (('B', 'candidate'),)):
            check(plan['binaries'][arm]['sha256'] == builds[build]['sha256'] and plan['binaries'][arm]['size'] == builds[build]['bytes'], window + ': measured compiler binary ' + arm)
        if bridge:
            check(plan['object_cache'] is True and plan['phases'] == ['build'] and plan['profile'] == 'O0', window + ': bridge contract')
        else:
            check(plan['rust_profile'] == plan['rust_lto'] == 'native' and plan['rust_linker'] is None and plan['target'] == 'x86_64-unknown-linux-gnu', window + ': native Rust flags')
            check(plan['profile'] == ('O2' if window == 'rust-o2-closed' else 'O0') and plan['cache_mode'] == ('incremental' if window == 'rust-o0-incremental' else 'cold') and plan['query_cache'] is (window == 'rust-o0-incremental') and plan['closed_executable'] is (window == 'rust-o2-closed'), window + ': explicit cache/closed config')
            check(plan['cpu'] == 0 and plan['versions']['rustc']['exit'] == 0 and 'release: 1.99.1' in plan['versions']['rustc']['stdout'] and 'LLVM version: 23.1.1' in plan['versions']['rustc']['stdout'], window + ': Rust toolchain version')
        check(plan['request_weights'] == (REQUESTS if bridge or plan['cache_mode'] == 'incremental' else {'cold_process': 1.0}), window + ': request weights')
        for name, expected in plan['fixtures_sha256'].items():
            path = folder / 'plan' / ('corpus' if bridge else '') / name
            check(digest(path) == expected, window + ': frozen source/input/oracle ' + name)
        rng, expected_order = random.Random(20261030), []
        for block in range(3):
            cases = list(WEIGHTS); rng.shuffle(cases)
            for case in cases:
                for position, arm in enumerate('ABBA' if block % 2 == 0 else 'BAAB'):
                    expected_order.append(dict(block=block, case=case, position=position, arm=arm, **({'phase': 'build'} if bridge else {})))
        check(plan['order'] == expected_order, window + ': independently reconstructed random paired order')
        validate_build(folder, plan, 'preflight', bridge)
        build_rows = validate_build(folder, plan, 'build', bridge)
        audit_summary(folder, 'build', plan, build_rows)
        if not bridge:
            artifacts = validate_artifacts(folder, plan)
            runtime_rows = validate_runtime(folder, plan, artifacts)
            audit_summary(folder, 'runtime', plan, runtime_rows)
        print('completed independent window', window, 'current issues', len(failures), flush=True)
    for sequence, values in query_keys.items():
        check(set(values) == set(REQUESTS), str(sequence) + ': complete query sequence')
        check(values['cold_process'] == values['unchanged'], str(sequence) + ': exact hit usage/key replay')
        check(len({values[r]['key'] for r in ('cold_process', 'private_edit', 'public_edit')}) == 3, str(sequence) + ': edits change query key')
    for cell, values in usage_observations.items():
        check(len(values) == 1, str(cell) + ': consistent recorded usage across all blocks and preflight')
    check(counts['measured_build_rows'] == 600 and counts['measured_runtime_rows'] == 180 and counts['preflight_build_rows'] == 100, '780 measured plus 100 unscored build coverage')
    check(sum(t['targets'] for t in test_counts) == 10, 'ten selected Rust harnesses executed')
    hosts = [p['host'] for p in plans.values()]
    check(all(h == hosts[0] for h in hosts), 'all windows same recorded host')
    check(all(p['environment'] == next(iter(plans.values()))['environment'] for p in plans.values()), 'all windows same recorded environment')
    rust_plans = [p for name, p in plans.items() if name != 'o0-bridge']
    check(all(p['binaries']['A'] == rust_plans[0]['binaries']['A'] and p['extra_tools'] == rust_plans[0]['extra_tools'] for p in rust_plans), 'all Rust windows bind identical rustc/sysroot/linker inventory')
    check(all(p['binaries']['clang']['sha256'] == toolchain['checks']['clang']['sha256'] for p in plans.values()), 'all windows actual checked Clang hash')
    check(all(any(t['sha256'] == toolchain['checks']['llvm']['sha256'] for t in p['extra_tools']) for p in plans.values()), 'all windows actual checked LLVM library hash')
    old_archive = 'evidence/research/compile-performance/LLVM23-ROUND2/raw-records-plans-preflights.tar.xz'
    previous = {}
    with tarfile.open(fileobj=io.BytesIO(gitbytes(old_archive)), mode='r:xz') as old_tar:
        for member in old_tar:
            if member.isfile() and member.name == 'round2/rust-o0-cold-plan/plan.json':
                old = json.load(old_tar.extractfile(member))
                previous = {k: old[k] for k in ('source_identity', 'host', 'cpus', 'blocks', 'units', 'jobs', 'profile')}
                previous.update(archive_git_path=old_archive, member=member.name)
                break
    review = {'schema': 'zil-compile4-independent-bench-review/1', 'status': 'Pass_ExploratoryEvidenceOnly' if not failures else 'Fail_NoAdoption', 'reviewer': '/root/benchmark_review', 'run_id': 41068328117, 'candidate_revision': HEAD, 'baseline_revision': BASE, 'candidate_crates_tree': TREES['B'], 'baseline_crates_tree': TREES['A'], 'tar_sha256': digest(E.parent / 'compile4-evidence.tar.gz'), 'zip_sha256_from_extraction_receipt': j(E.parent / 'EXTRACTION.json')['zip_sha256'], 'manifest_sha256': digest(E / 'manifest.json'), 'script_sha256': digest(__file__), 'checked_at_unix': time.time(), 'checks': checks, 'failures': failures, 'counts': dict(counts), 'inventory': {'files': len(inventory), 'tar_members': len(archive_names), 'unique_inodes_hashed': len(hash_cache), 'all_original_tar_bytes_and_POSIX_modes_checked': True}, 'max_summary_numeric_difference': max(differences, default=0), 'host': hosts[0], 'compile_affinity': [0, 1], 'runtime_affinity': [0], 'previous_host_context_only': previous, 'test_counts': test_counts, 'comparisons': comparisons, 'object_cache_measured_totals': {w: {r: dict(v) for r, v in req.items()} for w, req in cache_observations.items()}, 'query_recorded_usage': {c + '/' + r: sorted(values) for (c, r), values in usage_observations.items()}, 'units_jobs_fallback_internalized': {w + '/' + c: sorted(values, key=str) for (w, c), values in unit_observations.items()}, 'scope': ['600 measured compiler builds + 180 Rust/ZIL native runtime rows independently recalculated; direct ZIL runtime-quality arithmetic belongs to a separate review.', 'All 100 associated unscored compiler preflight rows, saved native binaries, startup/warmup and per-build oracles checked.', 'No original runner import or aggregation call; block means, median of block means, weighted absolute costs and ratio implemented here.', 'Per-case/request intervals, all-27 resampling support, geometric paired-block ratio and leave-one-case-out are additional descriptive sensitivities, never replacement scores.', 'Three paired blocks do not establish a product gate or general performance superiority.', 'Bridge has source/object caches and query disabled; Rust incremental has query enabled. No pooled score.', 'Recorded CT usage and cache/preflight accounting checked; logs do not expose all internal budget decrements and this review is not a full budget-semantics proof.', 'Extracted ctime/path differ by design; original identities checked for internal consistency, actual bytes and POSIX modes matched directly to original tar.', 'Rust/LLVM SDKs are not retained in this artifact; their frozen hashes, versions and original toolchain receipts are checked for consistency, not freshly re-executed.', 'Old 22.93% result is not migrated. Host model, CPU affinity, source and block count differ; changed host does not establish a causal explanation of performance.'], 'product_gates': 'NotRun', 'performance_adoption': 'NotEvaluated'}
    review['inventory']['all_original_tar_bytes_and_POSIX_modes_checked'] = archive_available
    review['tar_sha256_from_original_extraction_receipt'] = TAR_SHA
    review.pop('tar_sha256', None)
    review['archive_transport'] = {'path': str(archive), 'xz_sha256': digest(archive), 'original_tar_stream_sha256': stream_hash.hexdigest(), 'original_tar_stream_bytes': stream_bytes, 'note': 'Root reclaimed the 68 MB duplicate gzip during the first review read after another independent archive check, causing a transparent read interruption. The replacement xz carries the unchanged tar stream; all members were independently reread here. This transport incident is not a CI/measurement failure.'}
    review['conclusions'] = [
        'Within this host/window, bridge weighted wall is 0.8879857% lower and every block aggregate is below one; gain is small and not universal across cases.',
        'Bridge cold and unchanged request intervals cross one; private/public intervals are below one. tree4x2 compile is slower in all three block aggregates; retained point estimate is slower with an interval crossing one.',
        'O0 Rust cold is effectively tied in the main weighted estimate, interval crosses one; two of three block ratios and the paired geometric sensitivity exceed one. No confirmed overall Rust lead.',
        'Rust incremental is 20.0135% slower in wall and 43.3699% more CPU; unchanged/private/public are slower in all three blocks. Cache-enabled first builds are a distinct request and are slightly faster.',
        'O2 closed complete compile is 9.6820% slower than Rust, with all three aggregate block ratios above one; improved runtime quality must not be called compile-time superiority.',
        'Rust comparisons remain faster for small cases but largest-tree compile is slower in every main Rust window; removing that case is a diagnostic sensitivity, not a replacement result.',
        'O0 native runtime weighted lead is dominated by tree1024x8; tree4x2 remains slower than Rust. O2 native runtime weighted ratio is about 0.97985, with tree4x2 about 1.01080.',
        'No old 22.93% number, no bridge/query-enabled incremental pooled score, and no formal product gate follows from these three blocks.'
    ]
    if not archive_available:
        review['status'] = 'Pass_ExtractedEvidence_ArchiveUnavailable' if not failures else 'Fail_NoAdoption'
        review['scope'].append('Original tar was truncated to zero bytes during this review by an external actor/process. No original evidence was modified by this reviewer. All extracted files still match frozen inventory bytes; original tar mode/byte comparison is unavailable and not claimed. Parent was notified.')
    OUT.write_text(json.dumps(review, ensure_ascii=False, indent=2) + '\n')
    lines = ['# COMPILE4 独立基准复核', '', f"状态：{review['status']}；检查 {checks} 项，失败 {len(failures)} 项。", '', f"源：{BASE} → {HEAD}；Actions run 41068328117。", '', '不调用原 runner 聚合函数；逐文件对照原 tar 的字节与 POSIX mode，保持 artifact 不变。600 条构建与 180 条原生运行记录，以及 100 条预检构建完成复核。', '', '| 窗口 | A ms | B ms | B/A | 原口径 bootstrap 95% | 三个逐区块比值 |', '| --- | ---: | ---: | ---: | --- | --- |']
    for name, value in comparisons.items():
        w = value['wall']; lines.append(f"| {name} | {w['A']*1000:.3f} | {w['B']*1000:.3f} | {w['B_over_A']:.6f} | [{w['bootstrap95'][0]:.6f}, {w['bootstrap95'][1]:.6f}] | " + ', '.join(f'{r:.6f}' for r in w['block_ratios']) + ' |')
    lines += ['', '所有区间只描述三个配对区块；不是正式 Gate。bridge 的 B/A 为 candidate/baseline，Rust 窗口为 ZIL/Rust。', '', f"新主机：{hosts[0]['cpu_model']}，编译 affinity [0,1]，请求 8 CGU/4 jobs；旧主机为 {previous['host']['cpu_model']}、affinity {previous['cpus']}、{previous['blocks']} blocks。旧 22.93% 不迁移。", '', '## 构建请求与逐语料结果', '']
    for name, value in comparisons.items():
        if not name.endswith('/build'):
            continue
        w = value['wall']; lines += ['### ' + name, '', '| 请求 | A ms | B ms | B/A | 区间 |', '| --- | ---: | ---: | ---: | --- |']
        for request, cell in w['by_request'].items():
            lines.append(f"| {request} | {cell['A']*1000:.3f} | {cell['B']*1000:.3f} | {cell['B_over_A']:.6f} | [{cell['bootstrap95'][0]:.6f}, {cell['bootstrap95'][1]:.6f}] |")
        lines += ['', '| 语料 | B/A | 区间 | 三个逐区块比值 |', '| --- | ---: | --- | --- |']
        for case, cell in w['by_case'].items():
            lines.append(f"| {case} | {cell['B_over_A']:.6f} | [{cell['bootstrap95'][0]:.6f}, {cell['bootstrap95'][1]:.6f}] | " + ', '.join(f'{r:.6f}' for r in cell['block_ratios']) + ' |')
        lines += ['']
    lines += ['完整逐 case/request、CPU、附加敏感性、缓存计数与 CT usage 见同名 JSON。', '', '## 结论边界', '', '桥接加权墙钟改善约 0.888%，三个区块总体同向，但小语料并非全部改善。Rust O0 cold 总体接近持平；增量墙钟慢约 20.01%、CPU 多约 43.37%；O2 closed 完整编译墙钟慢约 9.68%。这些结论只属于本轮主机与冻结语料。', '', 'O0 原生运行的加权优势主要来自最大树；tree4x2 仍慢于 Rust。O2 原生运行加权 B/A 约 0.97985，tree4x2 约 1.01080；独立 ZIL baseline/candidate 质量窗口由另一份审查处理。', '', '内部预算逐步扣费无法只凭日志完整证明；已检查所有公开 CT usage、当前 Verify 次数、object 命中当前 machine preflight，以及 query hit 的 usage/key 重放一致性。缓存命中报告中的 llvm_verification_this_request=false 属于明确报告的 object 重用，不意味着跳过当前前端 Verify。', '', '归档运输说明：root 在首次读取过程中回收了重复 gzip，随后提供保留原 tar 字节流的 xz。此审查已重新验证 xz SHA、原 tar 流 SHA/长度以及全部成员字节和 POSIX mode；这次运输层读取中断不是 CI 或样本失败。']
    if failures:
        lines += ['', '## 未通过项', ''] + ['- ' + error for error in failures]
    if not archive_available:
        lines[6] = '不调用原 runner 聚合函数；全部已解包文件与冻结 files.json 清单逐一核对。原 tar 在审查中被外部进程清空，原 tar 字节与 POSIX mode 重核未完成，不声称通过。600 条构建与 180 条原生运行，以及 100 条预检构建已核对。'
    MD.write_text('\n'.join(lines) + '\n')
    print(json.dumps({'status': review['status'], 'checks': checks, 'failures': len(failures), 'max_numeric_difference': review['max_summary_numeric_difference'], 'outputs': [str(OUT), str(MD)]}, ensure_ascii=False), flush=True)


if __name__ == '__main__':
    main()
