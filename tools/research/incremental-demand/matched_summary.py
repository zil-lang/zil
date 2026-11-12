#!/usr/bin/env python3
"""Summarize completed matched-three-arm-v1 runs. Never compiles or measures.

python3 /tmp/matched_summary.py --check RUN_CHECK --o2 RUN_O2 \
  --output-prefix /tmp/matched-summary \
  --expected-candidate-source FULL_SHA --expected-candidate-binary SHA256

Validation defaults to RUN_CHECK-validation.json and RUN_O2-validation.json.
All original observations remain in raw files addressed by path and SHA256;
this helper writes only a compact JSON/Markdown summary and no raw data.
"""
import argparse
import collections
import hashlib
import itertools
import json
from pathlib import Path
import re
import statistics

import push_counters as pc

ARMS = ('baseline', 'candidate', 'rust')
CASES = ('cold', 'unchanged', 'private', 'multi', 'broad', 'restore', 'error', 'recovery')
PARENTS = {case: ('error' if case == 'recovery' else 'private' if case == 'restore' else 'cold') for case in CASES if case != 'cold'}
SCOPES = [
    'Each mode has its own sequential six-block window with all six rotated arm permutations; check and native O2 are separate experiments. OS caches are not flushed; six observations are not a significance test.',
    pc.NATIVE_WORK_SCOPE,
    'Zil wall is resident send-to-response and excludes resident startup; Rust wall includes each new Cargo process and rustc startup. Source writes, object/symbol inventory and native-oracle execution are outside request wall.',
    'Zil resident_cpu_seconds is a root /proc user+system tick delta; resident_and_waited_cpu_seconds additionally includes waited children. Rust tree_cpu_seconds_wait4 is Cargo wait4 user+system including waited descendants. These scopes are explicitly different.',
    'resident_rss_kib is response-time Zil root RSS; resident_peak_kib is cumulative process high water across prior requests. cargo_peak_sampled_rss_kib samples Cargo root only; wait4_maxrss_kib is a largest-process statistic, not simultaneous process-tree RSS. Do not infer cross-language memory superiority.',
    'Rust uses offline/locked Cargo with incremental=true; O2 uses opt-level=2, CGU16, jobs1, lto=false. Cargo check uses its dev profile (default opt-level=0) and performs no native LLVM object generation, while Zil check executes current independent Verify. Requested CGU counts do not prove any actual partition equivalence.',
    'Rust 1.99.1 is pinned to commit 58cf8f9c35b355199727627d07ae34a4e2846d67; its embedded LLVM is reported by rustc -vV. Actual Zil runtime LLVM versions and clang/environment paths are recorded independently in native receipts/manifests. Different compilers, optimizers, safety obligations and partitions are not equivalent work.',
    'Zil object hit/miss states are current compiler receipts. Rust same inode/device/mtime/content is physical carry-forward relative to the immediately preceding parent request, not rustc red-green query counters or proof of skipped optimization. Historical Rust sessions are excluded from current object counts.',
    'nm definitions are actual emitted object contents after optimization, not a complete source dependency graph. Equal bytes alone do not prove skipped work. Raw object hashes, full symbol tables, old sessions and all preparation requests remain in the raw evidence.',
    'Absent or null instrumentation is not a zero-work observation.',
    'For explicit --push-reuse runs only, the structural Verify identity can change the strict second-line LLVM verified-fingerprint comment. Raw LLVM bytes/hashes and raw equality remain recorded; a separate comparison excludes only that hex64 value. All other LLVM bytes and complete ELF bytes must agree.',
    'This report compares this candidate only to the same-window main and Rust arms. It does not compare earlier candidates, attribute whole-pipeline timings to an isolated source slice, establish language parity or claim universally minimal work.',
]

def fail(message):
    raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def reference(path):
    path = path.resolve()
    return {'path': str(path), 'sha256': digest(path), 'bytes': path.stat().st_size}


def pick(data, keys):
    return {key: data[key] for key in keys if key in data}


def compress(observations):
    groups = []
    for block, observation in enumerate(observations):
        old = next((group for group in groups if group['value'] == observation), None)
        if old is None:
            groups.append({'blocks': [block], 'value': observation})
        else:
            old['blocks'].append(block)
    return groups


def metrics(rows):
    measurements = [row['measurement'] for row in rows]
    fields = sorted({key for row in measurements for key in row if 'cpu' in key or 'rss' in key or key.endswith('_kib') or key == 'memory_sample_count'})
    walls = [row['wall_ms'] for row in measurements]
    return {'wall_ms': walls, 'median_ms': statistics.median(walls),
            'cpu_rss_sampling_by_block': {key: [row.get(key) for row in measurements] for key in fields}}


def reuse(row):
    report = row.get('resident', {}).get('report', {})
    out = pick(report, ('status', 'phase', 'verification_runs', 'disk_cache', 'verification_work', 'backend_work'))
    # Preserve unavailable receipts explicitly, without fabricated zero counters.
    out['types_push_work'] = report.get('types_push_work')
    out['raw_inference_work'] = report.get('raw_inference_work')
    out['native_products'] = pc.inspect_native_products(report)
    out['lowering_work'] = report.get('lowering_work')
    if report:
        out['query_events'] = dict(collections.Counter(event['phase'] + ':' + event['disposition'] for event in report['query_events']))
        front = report.get('frontend_stats') or {}
        keys = {
            'function_types': ('executed_functions', 'reused_functions', 'body_key_executions', 'body_key_reuses', 'producer_version_reuses', 'cfg_same_span_reuses', 'cfg_relocated_functions'),
            'initialization': ('candidate_executed_functions', 'candidate_reused_functions', 'initialization_inputs_built', 'candidate_inputs_built', 'dependency_signatures_checked', 'resource_transfers_executed', 'resource_transfers_reused'),
            'producer': ('classified_modules', 'lower_modules', 'reused_lower_modules', 'function_input_hashes', 'function_input_reuses', 'function_input_text_comparisons', 'function_input_binding_hashes', 'environment_imports_changed'),
            'materialization': ('executed_modules', 'reused_modules', 'cloned_modules'),
        }
        for group, fields in keys.items():
            stats = front.get(group)
            out[group] = pick(stats, fields) if stats is not None else None
            if group == 'initialization' and stats is not None:
                out[group].update({field: stats.get(field) for field in pc.FIELDS['initialization']})
            if group == 'function_types' and stats is not None:
                out[group]['graph_executed'] = (stats.get('graph') or {}).get('executed')
        for name in ('function_cache_encoding', 'function_plan_encoding', 'function_cache_publication', 'semantic_cache_publication'):
            if report.get(name) is not None:
                out[name] = {key: value for key, value in report[name].items() if not key.endswith('_ns')}
    inv = row.get('inventory')
    if inv:
        out['actual_current_objects'] = inv['object_count']
        if row['arm'] == 'rust':
            out['physical_carry_forward_objects'] = sum(item.get('same_inode_bytes_mtime_as_previous_request', False) for item in inv['objects'])
            out['same_bytes_objects'] = sum(item.get('same_bytes_as_previous_request', False) for item in inv['objects'])
            out['historical_objects_excluded'] = len(inv.get('historical_objects', []))
        else:
            out['object_cache_states'] = dict(collections.Counter(item['cache_observation']['state'] for item in inv['objects']))
            out['cache_miss_unit_indexes'] = [item['unit_index'] for item in inv['objects'] if item['cache_observation']['state'] == 'miss']
    return out


def numeric_ranges(numbers):
    groups = []
    for number in sorted(set(numbers)):
        if groups and groups[-1][-1] + 1 == number:
            groups[-1].append(number)
        else:
            groups.append([number])
    return ','.join(str(group[0]) if len(group) == 1 else f'{group[0]}-{group[-1]}' for group in groups)


def unit_key(item):
    return item.get('unit_index', item['name'])


def symbols(run, item, cache):
    key = item['object']['sha256']
    if key not in cache:
        path = run / item['symbols']['path']
        if digest(path) != item['symbols']['sha256']:
            fail(f'Changed nm evidence: {path}')
        cache[key] = sorted(line.split(maxsplit=3)[-1] for line in path.read_text().splitlines() if line.strip())
    return cache[key]


def describe_unit(run, item, cache):
    names = symbols(run, item, cache)
    ids = [int(match.group(1)) for name in names if (match := re.fullmatch(r'zil\.fn\.(\d+)', name))]
    other = [name for name in names if not re.fullmatch(r'zil\.fn\.\d+', name)]
    out = {'unit': unit_key(item), 'object_sha256': item['object']['sha256'], 'bytes': item['object']['bytes'],
           'defined_symbol_count': len(names), 'nm': item['symbols']}
    if ids:
        out['zil_fn_id_ranges'] = numeric_ranges(ids)
    if other:
        out['other_defined_symbols'] = other
    return out


def content_summary(run, rows):
    cache, examples, changes = {}, {}, {}
    for arm in ARMS:
        cold = next(row for row in rows if row['block'] == 0 and row['measured'] and row['request'] == 'cold' and row['arm'] == arm)
        inv = cold['inventory']
        examples[arm] = {'raw_directory': cold['directory'], 'scope': 'block 0 cold actual current session',
                         'namespace_or_session': inv.get('namespace', inv.get('current_session')),
                         'units': [describe_unit(run, item, cache) for item in inv['objects']]}
        old = {unit_key(item): item for item in inv['objects']}
        changes[arm] = {}
        for case in CASES:
            row = next(row for row in rows if row['block'] == 0 and row['measured'] and row['request'] == case and row['arm'] == arm)
            items = (row.get('inventory') or {}).get('objects', [])
            modified = []
            for item in items:
                prior = old.get(unit_key(item))
                if prior is not None and prior['object']['sha256'] == item['object']['sha256']:
                    continue
                same_members = prior is not None and symbols(run, prior, cache) == symbols(run, item, cache)
                unit = {'unit': unit_key(item), 'object_sha256': item['object']['sha256'],
                        'members_same_as_cold_unit': same_members, 'defined_symbol_count': len(symbols(run, item, cache))}
                if not same_members:
                    unit['actual_contents'] = describe_unit(run, item, cache)
                modified.append(unit)
            changes[arm][case] = {'raw_directory': row['directory'], 'contents_changed_from_cold': modified,
                                  'no_current_codegen_inventory': not row.get('inventory')}
    return {'representative_current_unit_contents': examples, 'representative_content_changes': changes,
            'scope': 'Representative block 0 contents only. Six-block object counts and reuse observations are retained in cells; every unit/hash/session is in hash-addressed raw rows. Missing error inventories are not zero-work claims.'}


def load_run(mode, run, validation, args):
    run = run.resolve()
    manifest = json.loads((run / 'manifest.json').read_text())
    receipt = json.loads(validation.read_text())
    completion = json.loads((run / 'completion.json').read_text())
    rows = [json.loads(line) for line in (run / 'rows.jsonl').read_text().splitlines() if line]
    if not receipt.get('valid') or not completion.get('valid') or len(rows) != 270 or not all(row.get('valid') for row in rows):
        fail(f'{mode}: run incomplete or validation failed')
    if receipt.get('rows') != 270 or receipt.get('measured_requests') != 144 or receipt.get('preparation_requests') != 126:
        fail(f'{mode}: expected 270 total / 144 measured / 126 preparation requests')
    if manifest['arguments']['mode'] != mode:
        fail(f'{mode}: manifest mode disagrees with requested input role')
    fixed = manifest['fixed']
    if fixed['blocks'] != 6 or tuple(fixed['requests']) != CASES or fixed['codegen_units_requested'] != 16 or fixed['codegen_jobs'] != 1 or fixed['native_opt'] != 'O2':
        fail(f'{mode}: protocol is not the required six-block CGU16/O2/jobs1 matrix')
    if any(fixed.get(key) != value for key, value in {'modules': 20, 'functions': 20, 'steps': 16, 'cache': 'all', 'capacity': 256}.items()):
        fail(f'{mode}: workload/cache/capacity differs from the full 20x20x16 matched fixture')
    if not manifest.get('completed_utc'):
        fail(f'{mode}: missing completed manifest')
    identities = pick(manifest, ('sources', 'binaries', 'tools', 'implementation', 'tools_after_sha256', 'binary_after_sha256', 'implementation_after_sha256'))
    for arm in ('baseline', 'candidate'):
        if manifest['binaries'][arm]['sha256'] != manifest['binary_after_sha256'][arm]:
            fail(f'{mode}: {arm} compiler binary changed within window')
        build_ref = manifest['binaries'][arm]['build_receipt']
        build_path = run / build_ref['archive']
        if digest(build_path) != build_ref['sha256']:
            fail(f'{mode}: changed build receipt for {arm}')
        build = json.loads(build_path.read_text())
        if build['source'] != manifest['sources'][arm]['commit'] or build['tree'] != manifest['sources'][arm]['tree'] or build['sha256'] != manifest['binaries'][arm]['sha256'] or build['exit_code'] != 0:
            fail(f'{mode}: {arm} source/tree/binary/build receipt mismatch')
        identities.setdefault('build_receipts', {})[arm] = build
    if manifest['implementation'] != manifest['implementation_after_sha256']:
        fail(f'{mode}: measurement implementation changed within window')
    for name, tool in manifest['tools'].items():
        if tool['actual_sha256'] != manifest['tools_after_sha256'][name]:
            fail(f'{mode}: {name} tool changed within window')
    extras = pc.candidate_flags(manifest['arguments'].get('candidate_extra_arg', []))
    pc.inspect_commands(run, manifest)
    for row in rows:
        if row['arm'] == 'candidate' and '--push-reuse' in extras:
            pc.inspect_report(row['resident']['report'], mode=mode, cold=row['request'] == 'cold' and row['measured'])
        fixture = manifest['fixture_identity'][row['request']]['rust' if row['arm'] == 'rust' else 'zil']
        if row['source']['identity_sha256'] != fixture:
            fail(f'{mode}: measured or named-parent source differs from the declared fixture')
        inv = row.get('inventory')
        if inv and inv['object_count'] != len(inv['objects']):
            fail(f'{mode}: current inventory count disagrees with actual objects')

    pc.baseline_pin(manifest['sources']['baseline']['commit'], manifest['arguments']['baseline_source'])
    if args.expected_baseline_source and manifest['sources']['baseline']['commit'] != args.expected_baseline_source:
        fail(f'{mode}: unexpected baseline source')
    if args.expected_candidate_source and manifest['sources']['candidate']['commit'] != args.expected_candidate_source:
        fail(f'{mode}: unexpected candidate source')
    if args.expected_candidate_binary and manifest['binaries']['candidate']['sha256'] != args.expected_candidate_binary:
        fail(f'{mode}: unexpected candidate binary')
    version_files = [run / name / 'stdout.bin' for name in ('rustc-version', 'cargo-version')]
    rust_version = version_files[0].read_text()
    if 'release: 1.99.1' not in rust_version or '58cf8f9c35b355199727627d07ae34a4e2846d67' not in rust_version:
        fail(f'{mode}: unexpected Rust pin')
    evidence = {name: reference(path) for name, path in [('manifest', run / 'manifest.json'), ('rows', run / 'rows.jsonl'), ('validation', validation), ('completion', run / 'completion.json'), ('rustc_version', version_files[0]), ('cargo_version', version_files[1])]}
    result = {'raw_directory': str(run), 'evidence': evidence, 'validation_receipt': {key: value for key, value in receipt.items() if key != 'cells'},
              'protocol': pick(manifest, ('fixed', 'fixture_identity', 'timing_policy', 'parent_policy', 'inventory_policy', 'memory_policy', 'explicit_environment', 'runtime_environment', 'host', 'started_utc', 'completed_utc')),
              'rustc_version': rust_version, 'cargo_version': version_files[1].read_text(), 'cells': {}, 'named_parents': {}, 'orders_by_block': {}}
    for case in CASES:
        cell = result['cells'][case] = {}
        orders = []
        for arm in ARMS:
            six = sorted((row for row in rows if row['measured'] and row['request'] == case and row['arm'] == arm), key=lambda row: row['block'])
            if [row['block'] for row in six] != list(range(6)):
                fail(f'{mode}/{case}/{arm}: six blocks missing')
            cell[arm] = metrics(six)
            if cell[arm]['wall_ms'] != receipt['cells'][case][arm]['wall_ms'] or cell[arm]['median_ms'] != receipt['cells'][case][arm]['median_ms']:
                fail(f'{mode}/{case}/{arm}: raw/validation timing mismatch')
            cell[arm]['reuse_by_block'] = compress([reuse(row) for row in six])
            cell[arm]['actual_current_object_counts'] = [(row.get('inventory') or {}).get('object_count') for row in six]
            if arm == 'baseline':
                orders = [row['order'] for row in six]
        if {tuple(order) for order in orders} != set(itertools.permutations(ARMS)):
            fail(f'{mode}/{case}: arm order is not six balanced permutations')
        result['orders_by_block'][case] = orders
        if case != 'cold':
            parent = PARENTS[case]
            result['named_parents'][case] = {'parent': parent, 'arms': {}}
            for arm in ARMS:
                six = sorted((row for row in rows if not row['measured'] and row['target_request'] == case and row['arm'] == arm), key=lambda row: row['block'])
                if len(six) != 6 or any(row['request'] != parent for row in six):
                    fail(f'{mode}/{case}/{arm}: wrong/missing named parent')
                result['named_parents'][case]['arms'][arm] = metrics(six)
    if sum(len(row.get('native', [])) for row in rows) != receipt['native_executions'] or any(not native['valid'] for row in rows for native in row.get('native', [])):
        fail(f'{mode}: native oracle/validation mismatch')
    if mode == 'o2':
        result['actual_llvm_versions'] = sorted({item['llvm_version'] for row in rows for item in (row.get('resident', {}).get('report', {}).get('native_codegen') or {}).get('object_reports', [])})
        paired = {}
        for row in rows:
            if row['arm'] in ('baseline', 'candidate') and row.get('executable'):
                key = (row['block'], row['request_index'], row['request'], row['target_request'], row['measured'])
                raw = row['llvm']['sha256']
                comparison = raw
                if '--push-reuse' in extras:
                    data = (run / row['llvm']['archive']).read_bytes()
                    if hashlib.sha256(data).hexdigest() != raw:
                        fail('O2: changed raw LLVM archive')
                    comparison = pc.llvm_comparison_sha256(data)
                    if comparison != row['llvm']['comparison_sha256_excluding_identity_comment']:
                        fail('O2: changed LLVM comparison receipt')
                paired.setdefault(key, {})[row['arm']] = (row['executable']['sha256'], comparison, raw)
        if not paired or any(set(pair) != {'baseline', 'candidate'} or pair['baseline'][:2] != pair['candidate'][:2] for pair in paired.values()):
            fail('O2: same-request main/candidate executable or LLVM text hash differs')
        raw_equal = all(pair['baseline'][2] == pair['candidate'][2] for pair in paired.values())
        result['zil_codegen_equality'] = {'successful_pairs_including_named_parents': len(paired), 'executable_sha256_equal': True, 'raw_llvm_equal': raw_equal, 'llvm_equal_excluding_identity_comment': True if '--push-reuse' in extras else None, 'llvm_comparison_policy': pc.LLVM_PUSH if '--push-reuse' in extras else pc.LLVM_RAW, 'rust_byte_or_partition_equality_claimed': False}
        result['actual_unit_contents'] = content_summary(run, rows)
    return result, identities


def compact_json(value):
    text = json.dumps(value, ensure_ascii=False, indent=2)
    def inline(match):
        values = json.loads(match.group())
        inline_numbers = all(isinstance(item, (int, float)) or item is None for item in values)
        inline_order = len(values) <= 3 and all(isinstance(item, str) for item in values) and len(json.dumps(values)) < 100
        return json.dumps(values) if inline_numbers or inline_order else match.group()
    return re.sub(r'\[\n[^\[\]{}]*?\n\s*\]', inline, text) + '\n'


def markdown(report, json_name):
    identity = report['identities']
    candidate = identity['sources']['candidate']
    lines = ['# Matched three-arm results', '',
             f"Candidate `{candidate['commit']}`, tree `{candidate['tree']}`, binary SHA256 `{identity['binaries']['candidate']['sha256']}`.",
             f"Main `{identity['sources']['baseline']['commit']}`; Rust 1.99.1 commit `58cf8f9c35b355199727627d07ae34a4e2846d67`.", '',
             f"[JSON summary]({json_name}) retains all six observations and original CPU/RSS field names, named preparations, exact raw-file/build/tool hashes, actual unit content examples and current reuse observations. Raw archive status: {report['raw_archive_status']}.", '',
             'Each mode validates 270 rows: 144 measured and 126 named parent preparations. Native execution counts and verified blobs are the exact validator receipts in JSON.', '',
             'Wall medians in ms; each cell is main / candidate / Rust. All individual six-block values remain in JSON.', '', '| Request | check | O2 |', '|---|---:|---:|']
    for case in CASES:
        values = [' / '.join(f"{report['runs'][mode]['cells'][case][arm]['median_ms']:.3f}" for arm in ARMS) for mode in ('check', 'o2')]
        lines.append(f'| {case} | {values[0]} | {values[1]} |')
    lines += ['', 'Candidate medians slower than their same-window comparator (descriptive observations, not significance claims):', '']
    lines += ['- ' + row for row in report['negative_cases']] or ['- None in this matrix; this does not establish universal superiority.']
    lines += ['', f"CGU16/O2/jobs1 is matched native configuration, not matched partitions or optimizer/safety work. Rust embedded LLVM is in rustc -vV; observed Zil native LLVM versions: {', '.join(report['runs']['o2']['actual_llvm_versions'])}. Cargo check uses its dev profile and generates no native objects; Zil check executes independent Verify. Rust uses a new Cargo invocation; Zil is resident. Check and native are separate windows. Source writes, unit inventory and native oracle execution are excluded from compile wall.", '',
              'CPU and RSS scopes differ: Zil /proc root CPU deltas and root response/cumulative-peak RSS; Rust wait4 invocation-tree CPU, Cargo-root sampled RSS, and largest-process maxrss. Do not infer cross-language memory superiority. Physical Rust object carry-forward and Zil object-cache hit receipts are distinct observations.', '',
              'These are whole-request results for the stated fixed workload and source. No prior-candidate timing comparison, isolated-slice attribution, universal minimal-work claim or cross-language parity is implied.']
    return '\n'.join(lines) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--check', required=True, type=Path)
    parser.add_argument('--o2', required=True, type=Path)
    parser.add_argument('--check-validation', type=Path)
    parser.add_argument('--o2-validation', type=Path)
    parser.add_argument('--output-prefix', required=True, type=Path)
    parser.add_argument('--expected-baseline-source', choices=pc.BASELINE_PINS)
    parser.add_argument('--expected-candidate-source')
    parser.add_argument('--expected-candidate-binary')
    parser.add_argument('--raw-archive-status', default='local raw evidence; pending remote archive')
    args = parser.parse_args()
    report = {'schema': 'matched-three-arm-summary-v2', 'generator': reference(Path(__file__)), 'raw_archive_status': args.raw_archive_status,
              'scope_and_limitations': SCOPES, 'identities': None, 'runs': {}, 'negative_cases': []}
    for mode, run, validation in [('check', args.check, args.check_validation), ('o2', args.o2, args.o2_validation)]:
        validation = validation or Path(str(run) + '-validation.json')
        summary, identity = load_run(mode, run, validation, args)
        if report['identities'] is not None and report['identities'] != identity:
            fail('Check and O2 source/binary/tool/build identities differ; do not silently pool distinct candidates')
        report['identities'] = identity
        report['runs'][mode] = summary
        for case in CASES:
            cell = summary['cells'][case]
            candidate = cell['candidate']['median_ms']
            for arm in ('baseline', 'rust'):
                comparison = cell[arm]['median_ms']
                if candidate > comparison:
                    report['negative_cases'].append(f'{mode}/{case}: candidate {candidate:.3f} ms > {"main" if arm == "baseline" else "Rust"} {comparison:.3f} ms')
    prefix = args.output_prefix
    prefix.parent.mkdir(parents=True, exist_ok=True)
    json_path, md_path = Path(str(prefix) + '.json'), Path(str(prefix) + '.md')
    json_path.write_text(compact_json(report))
    md_path.write_text(markdown(report, json_path.name))
    print(json.dumps({'json': reference(json_path), 'markdown': reference(md_path), 'validated_modes': ['check', 'o2'], 'prior_candidate_timing_comparisons': 0}))


if __name__ == '__main__':
    main()
