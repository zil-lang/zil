#!/usr/bin/env python3
"""Read completed original425 matched windows; never build or measure.

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
    require(args['mode'] == mode and args['blocks'] == 2, 'original two-block mode')
    for key, expected in [('candidate_source', pins['candidate']['source']), ('baseline_source', MAIN),
                          ('tool_source', pins['tool_source']), ('expected_candidate_binary_sha256', pins['candidate']['binary_sha256']),
                          ('expected_baseline_binary_sha256', pins['baseline']['binary_sha256'])]:
        require(args.get(key) == expected, 'manifest pin mismatch: '+key)
    require(completion.get('valid') is True and completion['rows'] == 102
            and completion['measured'] == 54 and completion['preparations'] == 48, 'completion matrix')
    exit_path = path.with_suffix('.exit')
    require(exit_path.read_text().strip() == '0', 'controller exit not zero')
    rows_path = path/'rows.jsonl'
    rows = [json.loads(line) for line in rows_path.read_text().splitlines()]
    require(len(rows) == 102 and all(x.get('valid') is True and x['mode'] == mode for x in rows), 'raw row validity')
    require(sum(x.get('measured') is True for x in rows) == 54, 'measured count')
    cells = []
    for cell in CELLS:
        selected = {a: sorted([x for x in rows if x['arm'] == a and x.get('measured') is True
                              and x['request'] == x['target_request'] == cell], key=lambda x:x['block']) for a in ARMS}
        require(all([x['block'] for x in selected[a]] == [0, 1] for a in ARMS), 'measured target_request matching')
        samples = {a:[x['measurement']['wall_ms'] for x in selected[a]] for a in ARMS}
        require(all(all(type(v) in (int,float) and math.isfinite(v) and v > 0 for v in vs) for vs in samples.values()), 'wall samples')
        cells.append({'request':cell, 'wall_ms_block0_1':samples,
                      'wall_median_ms':{a:statistics.median(vs) for a,vs in samples.items()},
                      'candidate_over_main_paired_GM':math.exp(statistics.mean(math.log(c/b) for c,b in zip(samples['candidate'],samples['baseline']))),
                      'candidate_over_Rust_paired_GM':math.exp(statistics.mean(math.log(c/b) for c,b in zip(samples['candidate'],samples['rust'])))})
    cache = {}; observations = []
    for row in rows:
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
        observations.append({'block':row['block'], 'request':row['request'], 'target_request':row['target_request'],
                             'measured':row['measured'], 'arm':row['arm'], 'directory':row['directory'],
                             'transition_from':row.get('transition_from'), 'source_identity_sha256':row['source_identity_sha256'],
                             'measurement':row['measurement'], 'phase_timings_ns':report.get('phase_timings_ns'),
                             'driver_timings_ns':report.get('driver_timings_ns'), 'frontend_stats':report.get('frontend_stats'),
                             'backend_work':report.get('backend_work'), 'verification_work':report.get('verification_work'),
                             'lowering_work':report.get('lowering_work'), 'typed_stage_observations':row.get('typed_stage_observations'),
                             'actual_inventory_units':inv.get('object_count'), 'native_oracle_outcomes':len(row.get('native', [])),
                             'executable':row.get('executable'), 'LLVM':row.get('llvm'),
                             'native_work':native, 'native_sums_observed':aggregate(native['units'])})
    pairs = [read(f) for f in sorted(path.glob('block-*/requests/*/pair-validation.json'))]
    require(len(pairs) == 34 and all(x.get('valid') is True for x in pairs), 'all parent/measured pair receipts')
    quality = {'pair_records':pairs, 'native_outcomes':sum(len(x.get('native', [])) for x in rows),
               'full_ELF_equal_pairs':sum(x.get('executable_equal') is True for x in pairs),
               'aggregate_exports_equal_pairs':sum(x.get('aggregate_zil_function_exports_equal') is True for x in pairs),
               'artifact_rehash_executed':verify, 'unique_blobs_rehashed':len(cache),
               'scope':'Existing runner current independent Verify/native/ELF/CGU quality receipts; optional blob rehash does not execute native code.'}
    if mode == 'o2':
        require(quality['native_outcomes'] == 1080 and quality['full_ELF_equal_pairs'] == 30, 'full original425 native quality count')
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
    return {'mode':mode,'raw':str(path),'raw_hashes':{f:digest(path/f) for f in ('manifest.json','completion.json','rows.jsonl')},
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
    result = {'schema':'completed-two-block-cohort-evidence-v1', 'valid':False,
              'script_sha256':digest(Path(__file__)), 'scope':'Read-only evidence aggregation; no build, native execution, measurement or source qualification.',
              'interpretation':['Missing optional cohort/MC/native_products fields remain unknown; observed zero is preserved.',
                'cohort counters are current-build completed-product events, not arena live-slot totals or prior-history counters.',
                'Fallback hook observations describe a discarded attempt; uninstrumented ordinary fallback counts remain unknown.',
                'MC counters are total AsmPrinter observations; no direct per-cohort MC attribution is fabricated.',
                'Zero MIR/IR cohort replays is a valid observation, not proof of a working reuse consumer.',
                'Two blocks do not establish significance, stable superiority, main merge or default flag authorization.']}
    try:
        pins = read(args.pins)
        require(pins['baseline']['source'] == MAIN, 'exact main9170 pin')
        require(all(re.fullmatch('[0-9a-f]{40}', s) for s in (pins['candidate']['source'], pins['tool_source'])), 'exact source/tool pin')
        result['pins'] = pins
        result['windows'] = [window(args.check,'check',pins,args.verify_blobs),window(args.o2,'o2',pins,args.verify_blobs)]
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
