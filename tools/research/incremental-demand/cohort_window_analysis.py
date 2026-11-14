#!/usr/bin/env python3
"""Read completed original425 matched windows with strict per-block structure.

Example, after root's unified gates and both formal windows:
  python3 -B tools/research/incremental-demand/cohort_window_analysis.py --pins RUN-pins.json \
    --check RUN-check --o2 RUN-o2 --output UNIQUE-analysis.json
Optional --verify-blobs rehashes archived ELF/LLVM/object bytes; use after timing.
Raw resident reports are authoritative. typed_stage_observations is only a
stage_body convenience projection and does not carry the optional cohort stats.
This is evidence aggregation, not an independent compiler/source qualification.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import statistics
import sys

ARMS = ('baseline', 'candidate', 'rust')
CELLS = ('cold', 'unchanged', 'private', 'multi', 'broad', 'positions', 'error', 'recovery', 'restore')
COHORT = ('ir_candidates', 'ir_replays', 'machine_candidates', 'machine_replays', 'declines', 'expired_entries', 'indexed_entries', 'disabled')
MAIN = '9170bb4210554392e36eb7528596612596ebef80'
PAIR_SCHEMA = 'native-link-exact-elf-reviewed-ir-v1'
# native_link_matched_measure.py ORDERS and native_workloads.py INPUTS.
# These identify existing receipts; they do not replace or execute an oracle.
ORDERS = (ARMS, tuple(reversed(ARMS)), ('candidate', 'rust', 'baseline'),
          ('baseline', 'rust', 'candidate'), ('rust', 'baseline', 'candidate'),
          ('candidate', 'baseline', 'rust'))
NATIVE_INPUT_HEX = ('', '00', '31', '41', '80', 'ff')


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    require(path.is_file() and not path.is_symlink(), 'missing/symlink input: '+str(path))
    return json.loads(path.read_text())


def uint(value):
    return type(value) is int and value >= 0


def parent_request(label):
    # rust_compare.py parent(), including restore's PRIVATE parent.
    return None if label == 'cold' else 'error' if label == 'recovery' else 'private' if label == 'restore' else 'cold'


def protocol_blocks(manifest, completion, mode):
    """Pure metadata helper; no files, subprocesses, imports or blob hashes."""
    args = manifest['arguments']
    blocks = args['blocks']
    require(mode in ('check', 'o2') and args['mode'] == mode
            and type(blocks) is int and blocks in (2, 6), 'original425 mode/block protocol')
    require(type(args.get('native_repetitions')) is int and args['native_repetitions'] == 2,
            'original425 native_repetitions must be exactly 2')
    require(type(manifest['fixed']['blocks']) is int and manifest['fixed']['blocks'] == blocks
            and manifest['fixed']['requests'] == list(CELLS), 'fixed block/request protocol')
    require(completion.get('valid') is True, 'completion validity')
    for key, per_block in (('rows', 51), ('measured', 27), ('preparations', 24)):
        require(type(completion.get(key)) is int and completion[key] == per_block * blocks,
                'completion matrix: '+key)
    return blocks


def require_matching_blocks(check, o2):
    require(check['mode'] == 'check' and o2['mode'] == 'o2'
            and type(check['blocks']) is int and type(o2['blocks']) is int
            and check['blocks'] in (2, 6) and check['blocks'] == o2['blocks'],
            'check/o2 must have the same original425 block count')


def native_row_count(row, mode):
    outcomes = row.get('native', [])
    require(isinstance(outcomes, list), 'native outcomes must be a list when present')
    successful_native = mode == 'o2' and row['request'] != 'error'
    require(len(outcomes) == (12 if successful_native else 0), 'per-row native count: '+row['directory'])
    if successful_native:
        # Every repeat/input gets its own exact original directory. Merely
        # copying an outcome or moving it between rows cannot fill a gap.
        for index, outcome in enumerate(outcomes):
            require(isinstance(outcome, dict), 'native outcome shape')
            repeat, number = divmod(index, len(NATIVE_INPUT_HEX))
            require(outcome.get('directory') == row['directory']+f'/native-{repeat:02d}-{number:02d}'
                    and outcome.get('stdin_hex') == NATIVE_INPUT_HEX[number], 'native outcome identity')
            require(outcome.get('valid') is True and isinstance(outcome.get('expected'), dict)
                    and isinstance(outcome.get('actual'), dict)
                    and outcome['expected'] == outcome['actual'], 'native outcome validity')
    return len(outcomes)


def validate_quality_structure(manifest, completion, rows, pair_entries, mode):
    """Validate copied metadata only, suitable for in-memory negative tests.

    pair_entries is a LIST of (exact window-relative path string, JSON object),
    not a dict: duplicate receipt paths must remain observable and be refused.
    The caller retains the original archive_check/fullhash responsibilities.
    """
    blocks = protocol_blocks(manifest, completion, mode)
    require(isinstance(rows, list) and len(rows) == 51 * blocks, 'raw row count')
    require(isinstance(pair_entries, list), 'pair entries must preserve duplicates')
    expected_order, expected_pairs = [], {}
    for block in range(blocks):
        for index, target in enumerate(CELLS):
            parent = parent_request(target)
            for current, measured in ([(parent, False)] if parent else [])+[(target, True)]:
                directory = f'block-{block:02d}/requests/{index:02d}-{current}-'+('measured' if measured else 'parent')
                keys = [(block, index, target, current, measured, arm) for arm in ORDERS[(block+index) % 6]]
                expected_order.extend(keys)
                expected_pairs[directory+'/pair-validation.json'] = keys
    expected_keys = set(expected_order)
    by_key, actual_order = {}, []
    per_block = [{'block': block, 'rows': 0, 'measured': 0, 'preparations': 0,
                  'pair_records': 0, 'native_outcomes': 0, 'full_ELF_equal_pairs': 0,
                  'aggregate_exports_equal_pairs': 0} for block in range(blocks)]
    for row in rows:
        require(isinstance(row, dict) and row.get('valid') is True and row.get('mode') == mode, 'raw row validity')
        require(type(row.get('block')) is int and 0 <= row['block'] < blocks, 'row block identity')
        require(type(row.get('request_index')) is int and 0 <= row['request_index'] < len(CELLS), 'row request_index identity')
        require(type(row.get('measured')) is bool, 'row measured must be bool')
        require(type(row.get('target_request')) is str and type(row.get('request')) is str
                and type(row.get('arm')) is str, 'row request/arm identity')
        block, index, target = row['block'], row['request_index'], row['target_request']
        key = (block, index, target, row['request'], row['measured'], row['arm'])
        require(key in expected_keys and key not in by_key, 'unexpected/duplicate complete row key: '+str(key))
        parent = parent_request(target)
        require('transition_from' in row and row['transition_from'] == (parent if row['measured'] else None),
                'parent transition: '+str(key))
        directory = f'block-{block:02d}/requests/{index:02d}-{row["request"]}-'+('measured' if row['measured'] else 'parent')+'/'+row['arm']
        require(row.get('directory') == directory, 'row directory identity: '+str(key))
        order = ORDERS[(block+index) % 6]
        require(row.get('order') == list(order) and type(row.get('position')) is int
                and row['position'] == order.index(row['arm']), 'original arm order/position')
        by_key[key] = row
        actual_order.append(key)
        counts = per_block[block]
        counts['rows'] += 1
        counts['measured' if row['measured'] else 'preparations'] += 1
        counts['native_outcomes'] += native_row_count(row, mode)
    require(set(by_key) == expected_keys, 'missing complete row keys')
    # This is the actual runner's parent-before-target sequence in rows.jsonl,
    # not just a well-formed transition_from label on an arbitrary row set.
    require(actual_order == expected_order, 'original parent/target execution sequence')
    seen_paths = set()
    for entry in pair_entries:
        require(isinstance(entry, (tuple, list)) and len(entry) == 2, 'pair entry shape')
        rel, pair = entry
        require(type(rel) is str and rel in expected_pairs and rel not in seen_paths,
                'missing/extra/misplaced/duplicate pair path: '+str(rel))
        seen_paths.add(rel)
        triplet = [by_key[key] for key in expected_pairs[rel]]
        row = triplet[0]
        require(isinstance(pair, dict) and pair.get('schema') == PAIR_SCHEMA
                and pair.get('valid') is True and pair.get('semantics_equal') is True,
                'pair receipt validity: '+rel)
        require(type(pair.get('measured')) is bool and pair['measured'] is row['measured']
                and pair.get('request') == row['request'], 'pair request/measured identity: '+rel)
        require(pair.get('llvm_equality_is_acceptance_prerequisite') is False, 'original reviewed IR policy')
        # Recheck the same metadata comparisons as runner quality(), without
        # executing its native or physical-inventory producers again.
        arms = {item['arm']: item for item in triplet}
        left, right = (arms[arm]['resident']['report'] for arm in ('baseline', 'candidate'))
        require(all(left[field] == right[field] for field in
                    ('status', 'phase', 'diagnostics', 'input_fingerprint', 'source_files', 'scalar_interfaces')),
                'pair semantic metadata: '+rel)
        require(left['status'] == ('SafeReject' if row['request'] == 'error' else 'Success'), 'pair status: '+rel)
        counts = per_block[row['block']]
        counts['pair_records'] += 1
        if mode == 'o2' and row['request'] != 'error':
            require(all(pair.get(field) is True for field in
                        ('executable_equal', 'aggregate_zil_function_exports_equal', 'native_oracles_valid', 'physical_zil_units_valid')),
                    'successful O2 pair quality: '+rel)
            require(arms['baseline']['executable']['sha256'] == arms['candidate']['executable']['sha256']
                    and arms['baseline']['executable']['bytes'] == arms['candidate']['executable']['bytes'],
                    'pair complete ELF references differ: '+rel)
            for arm in ('baseline', 'candidate'):
                inv = arms[arm]['inventory']
                require(type(inv['object_count']) is int and inv['object_count'] == len(inv['objects']) == 16
                        and type(inv['compiler_summary']['actual_units']) is int
                        and inv['compiler_summary']['actual_units'] == 16 and bool(inv['current_layout_matches']),
                        'pair physical CGU16 metadata: '+rel)
            exports = [{name for obj in arms[arm]['inventory']['objects'] for name in obj['defined_functions']}
                       for arm in ('baseline', 'candidate')]
            require(exports[0] == exports[1], 'pair aggregate exports metadata: '+rel)
            counts['full_ELF_equal_pairs'] += 1
            counts['aggregate_exports_equal_pairs'] += 1
        else:
            require(all(pair.get(field) is None for field in
                        ('executable_equal', 'aggregate_zil_function_exports_equal', 'native_oracles_valid', 'physical_zil_units_valid')),
                    'check/error pair cannot claim native quality: '+rel)
    require(seen_paths == set(expected_pairs), 'missing pair receipt paths')
    expected_counts = {'rows': 51, 'measured': 27, 'preparations': 24, 'pair_records': 17,
                       'native_outcomes': 540 if mode == 'o2' else 0,
                       'full_ELF_equal_pairs': 15 if mode == 'o2' else 0,
                       'aggregate_exports_equal_pairs': 15 if mode == 'o2' else 0}
    for counts in per_block:
        require(all(counts[field] == value for field, value in expected_counts.items()),
                'per-block quality matrix: '+str(counts))
    return per_block


def native_observation(row):
    report = (row.get('resident') or {}).get('report') or {}
    cg = report.get('native_codegen')
    if cg is None:
        return {'scope': 'no-current-native-report', 'units': [], 'final_artifact': None}
    require(isinstance(cg, dict) and isinstance(cg.get('object_reports'), list), 'native unit reports shape')
    units = []
    for index, item in enumerate(cg['object_reports']):
        item = json.loads(item) if isinstance(item, str) else item
        require(isinstance(item, dict), 'object report shape')
        stats = item.get('native_products')
        require(stats is None or isinstance(stats, dict), 'native_products shape')
        cohort = None if stats is None else stats.get('cohort')
        if cohort is not None:
            require(isinstance(cohort, dict) and cohort.get('scope') == 'current-build-completed-products', 'cohort scope')
            require(all(uint(cohort.get(k)) for k in COHORT), 'cohort missing/invalid V1 counter')
        mc = None if stats is None else stats.get('machine_emission')
        require(mc is None or isinstance(mc, dict), 'MC observation shape')
        fallback = None if stats is None else stats.get('fresh_context_fallbacks')
        eligible = (stats is not None and type(stats.get('available')) is int and stats['available'] == 1
                    and uint(fallback) and fallback == 0
                    and stats.get('failed_replays') == 0 and item.get('verify_before_after') is True
                    and (cohort is None or cohort['disabled'] == 0))
        units.append({'unit_index': index, 'native_products': stats,
                      'verify_before_after': item.get('verify_before_after'),
                      'input_kind': item.get('input_kind'), 'cache': item.get('cache'),
                      'surviving_hook_observation': eligible,
                      'cohort_observed': cohort is not None,
                      'fallback_scope': 'discarded hooked attempt; ordinary fallback work is unobserved' if uint(fallback) and fallback else None})
    return {'scope': 'current-unit-reports' if units else 'no-current-unit-observations',
            'actual_units': cg.get('actual_units'), 'requested_units': cg.get('requested_units'),
            'object_cache': cg.get('object_cache'), 'final_artifact': cg.get('final_artifact'),
            'units': units}


def numeric_leaves(value, prefix=()):
    if isinstance(value, dict):
        for key, child in value.items():
            if key in ('observer_schema', 'available'):
                continue
            yield from numeric_leaves(child, prefix+(key,))
    elif uint(value):
        yield prefix, value


def aggregate(units):
    # Optional/missing subtrees remain unknown: never materialize zero for an
    # absent cohort/MC/native_products tree. Totals describe observed units only.
    fields = sorted({path for u in units for path, _ in numeric_leaves(u['native_products'])})
    result = {}
    for path in fields:
        values = []
        surviving = []
        for unit in units:
            leaves = dict(numeric_leaves(unit['native_products']))
            if path in leaves:
                values.append(leaves[path])
                if unit['surviving_hook_observation']:
                    surviving.append(leaves[path])
        result['.'.join(path)] = {'observed_sum': sum(values) if values else None,
                                 'observed_units': len(values), 'missing_units': len(units)-len(values),
                                 'surviving_observed_sum': sum(surviving) if surviving else None,
                                 'surviving_observed_units': len(surviving)}
    return {'unit_reports': len(units), 'native_products_present': sum(u['native_products'] is not None for u in units),
            'cohort_present': sum(u['cohort_observed'] for u in units),
            'surviving_hook_units': sum(u['surviving_hook_observation'] for u in units), 'fields': result}


def archive_check(root, ref, verify, cache, *, symbols=False):
    require(isinstance(ref, dict), 'artifact reference')
    # The runner preserves binary/LLVM blobs as archive+bytes+sha256, but
    # symbol inventories as path+sha256. These are distinct recorded schemas;
    # symbols have no recorded byte count and must not receive a fabricated one.
    field = 'path' if symbols else 'archive'
    require(type(ref.get(field)) is str, 'missing '+field+' artifact reference')
    rel = Path(ref[field])
    if symbols:
        require(rel.parts and rel.parts[0] == 'symbols' and rel.suffix == '.txt',
                'symbol inventory path schema')
    require(not rel.is_absolute() and '..' not in rel.parts, 'archive path escape')
    path = root/rel
    require(not any(p.is_symlink() for p in [path, *list(path.parents)[:len(rel.parts)]]), 'archive symlink')
    require(path.is_file() and path.resolve().is_relative_to(root.resolve()), 'archive missing/escape')
    if not symbols:
        require(uint(ref.get('bytes')) and path.stat().st_size == ref['bytes'], 'archive size mismatch')
    require(re.fullmatch('[0-9a-f]{64}', ref.get('sha256', '')) is not None, 'artifact hash shape')
    key = str(path)
    if verify and key not in cache:
        data = path.read_bytes()
        require(hashlib.sha256(data).hexdigest() == ref['sha256'], 'archive hash mismatch')
        if path.suffix in ('.elf', '.o'):
            require(data[:6] == b'\x7fELF\x02\x01' and len(data) >= 64, 'ELF magic/length')
        cache[key] = ref['sha256']
    if verify:
        require(cache[key] == ref['sha256'], 'same blob with conflicting hash')


def window(path, mode, pins, verify):
    require(path.is_dir() and not path.is_symlink(), 'window directory')
    manifest = read(path/'manifest.json')
    completion = read(path/'completion.json')
    args = manifest['arguments']
    blocks = protocol_blocks(manifest, completion, mode)
    for key, expected in [('candidate_source', pins['candidate']['source']), ('baseline_source', MAIN),
                          ('tool_source', pins['tool_source']), ('expected_candidate_binary_sha256', pins['candidate']['binary_sha256']),
                          ('expected_baseline_binary_sha256', pins['baseline']['binary_sha256'])]:
        require(args.get(key) == expected, 'manifest pin mismatch: '+key)
    exit_path = path.with_suffix('.exit')
    require(exit_path.read_text().strip() == '0', 'controller exit not zero')
    rows_path = path/'rows.jsonl'
    rows = [json.loads(line) for line in rows_path.read_text().splitlines()]
    # Retain paths along with JSON. A list preserves duplicate identities for
    # the pure validator instead of silently overwriting them in a dict.
    pair_entries = []
    for f in sorted(path.rglob('pair-validation.json')):
        rel = f.relative_to(path)
        require(not any(p.is_symlink() for p in [f, *list(f.parents)[:len(rel.parts)]]), 'pair path symlink')
        pair_entries.append((rel.as_posix(), read(f)))
    per_block_quality = validate_quality_structure(manifest, completion, rows, pair_entries, mode)
    pairs = [pair for _, pair in pair_entries]
    cells = []
    for cell in CELLS:
        selected = {a: sorted([x for x in rows if x['arm'] == a and x.get('measured') is True
                              and x['request'] == x['target_request'] == cell], key=lambda x:x['block']) for a in ARMS}
        require(all([x['block'] for x in selected[a]] == list(range(blocks)) for a in ARMS), 'measured target_request matching')
        samples = {a:[x['measurement']['wall_ms'] for x in selected[a]] for a in ARMS}
        require(all(all(type(v) in (int,float) and math.isfinite(v) and v > 0 for v in vs) for vs in samples.values()), 'wall samples')
        cells.append({'request':cell, 'wall_ms_by_block':samples,
                      'wall_median_ms':{a:statistics.median(vs) for a,vs in samples.items()},
                      'candidate_over_main_paired_GM':math.exp(statistics.mean(math.log(c/b) for c,b in zip(samples['candidate'],samples['baseline']))),
                      'candidate_over_Rust_paired_GM':math.exp(statistics.mean(math.log(c/b) for c,b in zip(samples['candidate'],samples['rust'])))})
    cache = {}; observations = []
    for row in rows:
        rel = Path(row['directory'])
        directory = path/rel
        require(directory.is_dir() and directory.resolve().is_relative_to(path.resolve())
                and not any(p.is_symlink() for p in [directory, *list(directory.parents)[:len(rel.parts)]]),
                'missing/unsafe row directory: '+str(directory))
        native = native_observation(row)
        refs = [(row[k], False) for k in ('executable','llvm') if k in row]
        inv = row.get('inventory') or {}
        for obj in inv.get('objects', []):
            if 'object' in obj:
                refs.append((obj['object'], False))
            if 'symbols' in obj:
                refs.append((obj['symbols'], True))
        for ref, symbols in refs:
            archive_check(path, ref, verify, cache, symbols=symbols)
        report = (row.get('resident') or {}).get('report') or {}
        observations.append({'block':row['block'], 'request_index':row['request_index'], 'request':row['request'], 'target_request':row['target_request'],
                             'measured':row['measured'], 'arm':row['arm'], 'directory':row['directory'],
                             'transition_from':row.get('transition_from'), 'source_identity_sha256':row['source_identity_sha256'],
                             'measurement':row['measurement'], 'phase_timings_ns':report.get('phase_timings_ns'),
                             'driver_timings_ns':report.get('driver_timings_ns'), 'frontend_stats':report.get('frontend_stats'),
                             'backend_work':report.get('backend_work'), 'verification_work':report.get('verification_work'),
                             'lowering_work':report.get('lowering_work'), 'typed_stage_observations':row.get('typed_stage_observations'),
                             'actual_inventory_units':inv.get('object_count'), 'native_oracle_outcomes':len(row.get('native', [])),
                             'executable':row.get('executable'), 'LLVM':row.get('llvm'),
                             'native_work':native, 'native_sums_observed':aggregate(native['units'])})
    quality = {'pair_records':pairs, 'native_outcomes':sum(len(x.get('native', [])) for x in rows),
               'full_ELF_equal_pairs':sum(x.get('executable_equal') is True for x in pairs),
               'aggregate_exports_equal_pairs':sum(x.get('aggregate_zil_function_exports_equal') is True for x in pairs),
               'pair_paths':[rel for rel, _ in pair_entries], 'per_block':per_block_quality,
               'strict_quality_structure_valid':True, 'native_repetitions':args['native_repetitions'],
               'artifact_rehash_executed':verify, 'unique_blobs_rehashed':len(cache),
               'scope':'Existing runner current independent Verify/native/ELF/CGU quality receipts; optional blob rehash does not execute native code.'}
    if mode == 'o2':
        require(quality['native_outcomes'] == 540 * blocks and quality['full_ELF_equal_pairs'] == 15 * blocks, 'full original425 native quality count')
    # Measured and parent summaries stay separate; no cumulative arena totals
    # are differenced or mistaken for current-request publications.
    cohort_cells = []
    for measured in (True,False):
        for target in CELLS:
            for arm in ARMS:
                selected = [x for x in observations if x['measured'] is measured and x['target_request'] == target and x['arm'] == arm]
                if selected:
                    cohort_cells.append({'target_request':target,'measured':measured,'arm':arm,
                                         'observed_requests':len(selected),
                                         'sum_current_unit_observations':aggregate([u for x in selected for u in x['native_work']['units']])})
    return {'mode':mode,'blocks':blocks,'raw':str(path),'raw_hashes':{f:digest(path/f) for f in ('manifest.json','completion.json','rows.jsonl')},
            'manifest':manifest,'completion':completion,'cells':cells,'quality':quality,
            'cohort_request_sums':cohort_cells,'requests':observations}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pins',type=Path,required=True)
    parser.add_argument('--check',type=Path,required=True)
    parser.add_argument('--o2',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--verify-blobs',action='store_true')
    args = parser.parse_args()
    require(not args.output.exists(), 'unique output required')
    result = {'schema':'completed-matched-cohort-evidence-strict-v3', 'valid':False,
              'script_sha256':digest(Path(__file__)), 'scope':'Read-only evidence aggregation; no build, native execution, measurement or source qualification.',
              'interpretation':['Missing optional cohort/MC/native_products fields remain unknown; observed zero is preserved.',
                'cohort counters are current-build completed-product events, not arena live-slot totals or prior-history counters.',
                'Fallback hook observations describe a discarded attempt; uninstrumented ordinary fallback counts remain unknown.',
                'MC counters are total AsmPrinter observations; no direct per-cohort MC attribution is fabricated.',
                'Zero MIR/IR cohort replays is a valid observation, not proof of a working reuse consumer.',
                'A finite two/six-block window does not establish universal stable superiority, main merge or default flag authorization.']}
    try:
        pins = read(args.pins)
        require(pins['baseline']['source'] == MAIN, 'exact main9170 pin')
        require(all(re.fullmatch('[0-9a-f]{40}', s) for s in (pins['candidate']['source'], pins['tool_source'])), 'exact source/tool pin')
        result['pins'] = pins
        # Refuse mixed two/six-block windows before optional expensive hashes.
        require_matching_blocks(
            {'mode':'check', 'blocks':read(args.check/'manifest.json')['arguments']['blocks']},
            {'mode':'o2', 'blocks':read(args.o2/'manifest.json')['arguments']['blocks']})
        result['windows'] = [window(args.check,'check',pins,args.verify_blobs),window(args.o2,'o2',pins,args.verify_blobs)]
        require_matching_blocks(*result['windows'])
        result['valid'] = True
    except (ValueError,KeyError,TypeError,OSError,json.JSONDecodeError) as error:
        result['first_error'] = type(error).__name__+': '+str(error)
    with args.output.open('x') as stream:
        json.dump(result,stream,ensure_ascii=False,indent=2,allow_nan=False)
        stream.write('\n')
    print(json.dumps({'output':str(args.output),'sha256':digest(args.output),'valid':result['valid']}))
    return 0 if result['valid'] else 2


if __name__ == '__main__':
    raise SystemExit(main())
