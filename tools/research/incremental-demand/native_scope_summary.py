#!/usr/bin/env python3
"""Summarize retained native profiles; does not compile or establish speedup."""
import argparse
import hashlib
import json
from pathlib import Path

from push_counters import inspect_native_products, NATIVE_WORK_SCOPE


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run_dir', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    raw = (args.run_dir / 'rows.jsonl').read_bytes()
    manifest_raw = (args.run_dir / 'manifest.json').read_bytes()
    manifest = json.loads(manifest_raw)
    rows = []
    for row in map(json.loads, raw.splitlines()):
        if row.get('valid') is not True:
            raise ValueError('invalid retained request')
        report = row['resident']['report']
        view = inspect_native_products(report)
        units = [] if view is None else view['units']
        observed = [unit['stats'] for unit in units if unit['stats'] is not None]
        # Unknown fields in legacy observers remain unknown in the aggregate.
        fields = set().union(*(stats.keys() for stats in observed))
        totals = {
            field: sum(stats[field] for stats in observed)
            if all(type(stats.get(field)) is int for stats in observed) else None
            for field in sorted(fields - {'observer_schema'})
        }
        if observed:
            for field in units[0]['ir_scope_ns']:
                totals[field] = (sum(unit['ir_scope_ns'][field] for unit in units if unit['stats'] is not None)
                                 if all(unit['ir_scope_ns'][field] is not None for unit in units if unit['stats'] is not None)
                                 else None)
        objects = (report.get('native_codegen') or {}).get('object_reports', [])
        objects = [json.loads(unit) if isinstance(unit, str) else unit for unit in objects]
        rows.append({
            'arm': row['arm'], 'block': row['block'], 'request': row['request'],
            'measured': row['measured'], 'transition_from': row['transition_from'],
            'wall_ms': row['wall_ms'], 'status': row['status'],
            'observer_schemas': sorted({stats.get('observer_schema', 1) for stats in observed}),
            'observed_units': len(observed), 'unobserved_units': len(units) - len(observed),
            'native_observed_totals': totals or None,
            'object_receipt_scope_ns': {
                field: sum(unit[field] for unit in objects) if objects and all(type(unit.get(field)) is int for unit in objects) else None
                for field in ['builder_ns', 'parse_ns', 'optimize_ns', 'emit_ns', 'total_ns']
            },
            'phase_timings_ns': report.get('phase_timings_ns'),
            'backend_work': report.get('backend_work'),
            'ct_plain_work': report.get('ct_plain_work'),
        })
    result = {
        'schema': 'native-profile-scopes-v1', 'source_shas': manifest['source_shas'],
        'binary_receipts': manifest['binaries'], 'arguments': manifest['arguments'],
        'rows_sha256': hashlib.sha256(raw).hexdigest(),
        'manifest_sha256': hashlib.sha256(manifest_raw).hexdigest(),
        'scope': NATIVE_WORK_SCOPE,
        'limits': 'All preparation and measured rows retained. Diagnostic only; no cross-window ratio, causal attribution or formal acceptance. Retained bytes are a per-request receipt sum, not peak RSS or a lifetime bound.',
        'rows': rows,
    }
    args.output.write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
