#!/usr/bin/env python3
"""Reconstruct all frozen stage-5 cells; paired bootstrap is exploratory only."""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path
import random
import statistics

WINDOWS = {
    'generated-default': 4140,
    'generated-closed': 1380,
    'implementation-ablation': 960,
    'real-default': 1008,
    'real-closed': 336,
}
median = statistics.median


def read(path):
    return json.loads(path.read_text())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--window', action='append', help='NAME:EXPECTED_ROWS; defaults to the original five windows')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    result = {'scope': __doc__, 'bootstrap_draws': 2000, 'seed': 20261031,
              'statistic': 'ratio of arm medians; both arms resampled with the same block indices',
              'script_sha256': sha(Path(__file__)), 'windows': {}}
    table = ['# 第五阶段完整单元表', '',
             '单位ms。Z/R为双臂中位时间之比，低于1表示ZIL更快；ablation的R是旧ZIL。',
             '每臂6次，AB/BA交错。探索95%区间为同区组配对bootstrap（2000次）；不是产品门或多重比较保证。',
             'RSS为wait4峰值的中位数（MiB），不是进程树同时占用的总和。失败保留耗时/RSS，但不计算速度比。', '']
    windows = dict((name, int(count)) for name, count in (entry.rsplit(':', 1) for entry in args.window)) if args.window else WINDOWS
    for name, expected_rows in windows.items():
        folder = args.root / name
        manifest = read(folder / 'manifest.json')
        rows = [json.loads(line) for line in (folder / 'rows.jsonl').read_text().splitlines()]
        assert len(rows) == expected_rows
        reference = 'baseline' if 'baseline' in manifest['binaries'] else 'rust'
        grouped = defaultdict(dict)
        invalid = Counter()
        native_runs = verify = 0
        for row in rows:
            key = (row['case'], row['mode'], row['request'])
            slot = (row['arm'], row['block'])
            assert slot not in grouped[key]
            assert row['position'] == (0 if (row['arm'] == 'zil') == (row['block'] % 2 == 0) else 1)
            grouped[key][slot] = row
            if not row['valid']:
                invalid[(row['case'], row['arm'], row.get('status', 'invalid'))] += 1
            if row['valid'] and row['arm'] in ('zil', 'baseline'):
                assert row['verification_runs'] == 1
                verify += 1
            native = row.get('native')
            if native is not None:
                native_runs += len(native) if isinstance(native, list) else 1
        original = read(folder / 'summary.json')
        cells = []
        assert len(original['cells']) == len(grouped)
        table += [f'## {name}', '',
                  '| 样本 | 模式 | 请求 | ZIL ms | R ms | Z/R [探索95%区间] | RSS Z/R MiB | 失败 Z/R |',
                  '|---|---|---|---:|---:|---|---|---|']
        for old in original['cells']:
            key = tuple(old[k] for k in ('case', 'mode', 'request'))
            by_slot = grouped[key]
            assert set(by_slot) == {(arm, block) for arm in ('zil', reference) for block in range(6)}
            cell = dict(zip(('case', 'mode', 'request'), key))
            cell['arms'] = {}
            for arm in ('zil', reference):
                arm_rows = [by_slot[(arm, block)] for block in range(6)]
                values = [r['elapsed_ns'] / 1e6 for r in arm_rows]
                failures = sum(not r['valid'] for r in arm_rows)
                rss = [r['max_rss_kib'] / 1024 for r in arm_rows if r['max_rss_kib'] is not None]
                detail = {'median_ms': median(values), 'values_ms': values, 'failures': failures,
                          'median_rss_mib': median(rss) if rss else None}
                for field in ('median_ms', 'values_ms', 'failures'):
                    assert old['arms'][arm][field] == detail[field]
                cg = [r.get('native_codegen') for r in arm_rows]
                if all(isinstance(c, dict) and c.get('object_cache') is not None for c in cg):
                    detail['actual_units'] = [c['actual_units'] for c in cg]
                    detail['hits'] = [c['object_cache']['hits'] for c in cg]
                    detail['misses'] = [c['object_cache']['misses'] for c in cg]
                    detail['backend_component_sum_ms'] = {
                        field: median(sum(o.get(field, 0) for o in c['object_reports']) / 1e6 for c in cg)
                        for field in ('preflight_ns', 'builder_ns', 'optimize_ns', 'emit_ns', 'total_ns')}
                    detail['current_hit_preflight_ms'] = median(c['object_cache']['current_preflight_ns'] / 1e6 for c in cg)
                cell['arms'][arm] = detail
            z, r = cell['arms']['zil'], cell['arms'][reference]
            valid = not (z['failures'] or r['failures'])
            ratio = z['median_ms'] / r['median_ms'] if valid else None
            assert old['zil_over_' + reference] == ratio
            cell['ratio'] = ratio
            cell['exploratory_95'] = None
            if valid:
                seed = 20261031 + int.from_bytes(hashlib.sha256((name + repr(key)).encode()).digest()[:8], 'big')
                rng = random.Random(seed)
                boot = []
                for _ in range(2000):
                    indices = [rng.randrange(6) for _ in range(6)]
                    boot.append(median(z['values_ms'][i] for i in indices) / median(r['values_ms'][i] for i in indices))
                boot.sort()
                cell['exploratory_95'] = [boot[49], boot[1949]]
            ci = cell['exploratory_95']
            ratio_text = f'{ratio:.3f} [{ci[0]:.3f}, {ci[1]:.3f}]' if valid else '无效'
            rss_text = '/'.join(f'{arm["median_rss_mib"]:.1f}' if arm['median_rss_mib'] is not None else 'NA' for arm in (z, r))
            table.append(f'| {key[0]} | {key[1]} | {key[2]} | {z["median_ms"]:.3f} | {r["median_ms"]:.3f} | {ratio_text} | {rss_text} | {z["failures"]}/{r["failures"]} |')
            cells.append(cell)
        aggregates = []
        for mode in manifest['modes']:
            for request in dict.fromkeys(c['request'] for c in cells):
                selected = [c for c in cells if c['mode'] == mode and c['request'] == request]
                valid = [c for c in selected if c['ratio'] is not None]
                aggregates.append({'mode': mode, 'request': request, 'valid_cases': len(valid),
                                   'failed_cases': len(selected) - len(valid),
                                   'equal_case_geomean': math.exp(statistics.mean(math.log(c['ratio']) for c in valid)) if valid else None})
        coverage = []
        for entry in manifest['cases']:
            case = entry if isinstance(entry, str) else entry['name']
            source = folder / ('source-' + case)
            metrics = {}
            for ext in ('zil', 'rs'):
                files = list(source.rglob('*.' + ext))
                metrics[ext] = {'copied_files': len(files), 'bytes': sum(f.stat().st_size for f in files),
                                'lines': sum(len(f.read_text().splitlines()) for f in files),
                                'max_file_bytes': max((f.stat().st_size for f in files), default=0),
                                'max_file_lines': max((len(f.read_text().splitlines()) for f in files), default=0)}
            coverage.append({'case': case, 'metrics': metrics})
        oracle = [read(p) for p in sorted(folder.glob('oracle-*/record.json'))]
        assert all(x['oracle_valid'] and len(x['native']) == 15 for x in oracle)
        result['windows'][name] = {'rows': len(rows), 'cells': cells, 'reference': reference,
                                  'invalid_rows': [{'case': c, 'arm': a, 'status': s, 'count': n} for (c, a, s), n in sorted(invalid.items())],
                                  'valid_verify_requests': verify, 'native_runs': native_runs,
                                  'oracle_builds': len(oracle), 'oracle_native_runs': sum(len(o['native']) for o in oracle),
                                  'coverage': coverage, 'aggregates': aggregates,
                                  'rows_sha256': sha(folder / 'rows.jsonl'), 'manifest_sha256': sha(folder / 'manifest.json')}
        table.append('')
    (args.out / 'analysis.json').write_text(json.dumps(result, indent=2) + '\n')
    (args.out / 'DISTRIBUTION-TABLES.md').write_text('\n'.join(table) + '\n')
    print(json.dumps({n: {k: w[k] for k in ('rows', 'invalid_rows', 'valid_verify_requests', 'native_runs', 'oracle_builds')} for n, w in result['windows'].items()}, indent=2))


if __name__ == '__main__':
    main()
