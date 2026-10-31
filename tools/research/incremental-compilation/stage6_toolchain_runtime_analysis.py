#!/usr/bin/env python3
"""Audit the frozen nine-arm runtime observations and retain every comparison cell."""
import argparse
import csv
import hashlib
import json
import math
from pathlib import Path
import random
import shutil
import statistics

COMPARISONS = [
    ('candidate_original', 'entry_after', 'original_after'),
    ('candidate_original_closed', 'entry_after_closed', 'original_after_closed'),
    ('candidate_rust', 'entry_after', 'rust'),
    ('candidate_rust_closed', 'entry_after_closed', 'rust'),
    ('incremental_candidate', 'entry_incremental_after', 'entry_after'),
    ('incremental_candidate_closed', 'entry_incremental_after_closed', 'entry_after_closed'),
    ('incremental_rust', 'entry_incremental_after', 'rust'),
    ('incremental_rust_closed', 'entry_incremental_after_closed', 'rust'),
    ('candidate_baseline', 'entry_after', 'baseline_off'),
    ('candidate_baseline_closed', 'entry_after_closed', 'baseline_off_closed'),
]


def identity(path):
    path = Path(path).resolve(strict=True)
    with path.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    return {'path': str(path), 'sha256': digest, 'bytes': path.stat().st_size}


def check_identity(record):
    actual = identity(record['path'])
    if actual['sha256'] != record['sha256'] or actual['bytes'] != record['bytes']:
        raise ValueError('input identity changed: ' + record['path'])


def load(path):
    return json.loads(path.read_text())


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def gm(values):
    return math.exp(statistics.mean(math.log(value) for value in values))


def percentile(values, p):
    values = sorted(values)
    index = (len(values) - 1) * p
    lower = math.floor(index)
    upper = math.ceil(index)
    return values[lower] * (upper - index) + values[upper] * (index - lower) if lower != upper else values[lower]


def ci(values):
    return [percentile(values, 0.025), percentile(values, 0.975)]


def csv_write(path, rows):
    with path.open('w', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prepared', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--bootstrap', type=int, default=2000)
    parser.add_argument('--seed', type=int, default=6302026)
    args = parser.parse_args()
    if args.bootstrap < 100:
        parser.error('at least 100 bootstrap draws are required')
    args.prepared = args.prepared.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    shutil.copy2(Path(__file__), args.out / Path(__file__).name)
    manifest = load(args.prepared / 'manifest.json')
    execution = load(args.prepared / 'execution.json')
    marker = load(args.prepared / 'prepared.json')
    builds = load(args.prepared / 'builds.json')
    if marker != {'ready': True, 'manifest_sha256': identity(args.prepared / 'manifest.json')['sha256'],
                  'builds_sha256': identity(args.prepared / 'builds.json')['sha256']}:
        raise ValueError('preparation marker changed or failed')
    if (manifest['functional_smoke_only'] or manifest['blocks'] != 6 or manifest['rounds'] != 20000 or
            manifest['planned_builds'] != 54 or manifest['planned_runtime_observations'] != 1620 or
            not execution['completed'] or not execution['valid'] or execution['failed_observations'] != 0 or
            execution['observations'] != 1620 or not execution['prebuilt_artifacts_only'] or
            execution['manifest_sha256'] != marker['manifest_sha256']):
        raise ValueError('not the complete, valid frozen formal window')
    cases, seeds = manifest['cases'], manifest['seeds']
    names = [arm['name'] for arm in manifest['arms']]
    if len(cases) != 6 or len(seeds) != 5 or len(names) != 9:
        raise ValueError('unexpected protocol dimensions')
    for sources in manifest['sources'].values():
        for record in sources.values():
            check_identity(record)
    for name, record in manifest['scripts'].items():
        if identity(args.prepared / 'scripts' / name)['sha256'] != record['sha256']:
            raise ValueError('frozen script changed: ' + name)
    check_identity(manifest['config'])
    artifact_paths = {(item['case'], item['arm']): item['executable']['path'] for item in builds}
    series, rows = {}, []
    for line in (args.prepared / 'rows.jsonl').read_text().splitlines():
        row = json.loads(line)
        key = row['case'], row['seed'], row['arm'], row['block']
        if key in series or key[0] not in cases or key[1] not in seeds or key[2] not in names or key[3] not in range(6):
            raise ValueError('duplicate or unexpected observation')
        expected = manifest['expected'][key[0]][str(key[1])].to_bytes(4, 'little')
        process = row['process']
        prefix = args.prepared / 'runtime' / key[0] / f'{key[3]}-{key[1]}-{row["position"]}-{key[2]}'
        order = names if key[3] % 2 == 0 else list(reversed(names))
        if (row['position'] != order.index(key[2]) or not row['valid'] or process['exit'] != 0 or process['timeout'] or
                process['launch_error'] is not None or row['elapsed_ns'] != process['elapsed_ns'] or row['elapsed_ns'] <= 0 or
                process['command'] != [artifact_paths[key[0], key[2]]] or process['environment_overrides'] or
                row['expected'] != int.from_bytes(expected, 'little') or row['expected_hex'] != expected.hex() or
                row['stdout_hex'] != expected.hex() or row['stderr_hex'] or
                Path(str(prefix) + '.stdout').read_bytes() != expected or Path(str(prefix) + '.stderr').read_bytes() or
                load(Path(str(prefix) + '.process.json')) != process):
            raise ValueError('raw native observation is invalid: ' + str(key))
        series[key] = row['elapsed_ns']
        rows.append(row)
    if len(rows) != 1620:
        raise ValueError('incomplete observations')
    medians = []
    for case in cases:
        for seed in seeds:
            for arm in names:
                values = [series[case, seed, arm, block] for block in range(6)]
                medians.append({'case': case, 'seed': seed, 'arm': arm,
                                'median_ns': statistics.median(values), 'min_ns': min(values), 'max_ns': max(values)})
    csv_write(args.out / 'arm-medians.csv', medians)
    build_rows = []
    seen_builds = set()
    for build in builds:
        key = build['case'], build['arm']
        if key in seen_builds or key[0] not in cases or key[1] not in names or not build['valid']:
            raise ValueError('invalid or duplicated prepared build')
        seen_builds.add(key)
        check_identity(build['executable'])
        size = build['audits']['size']
        parsed_size = list(map(int, Path(size['process']['stdout_path']).read_text().splitlines()[-1].split()[:3]))
        if parsed_size != [size['text_bytes'], size['data_bytes'], size['bss_bytes']]:
            raise ValueError('size inventory changed')
        symbol_counts = {}
        for table in ['nm-global', 'nm-dynamic']:
            audit = build['audits'][table]
            raw = Path(audit['process']['stdout_path']).read_bytes()
            if hashlib.sha256(raw).hexdigest() != audit['stdout_sha256']:
                raise ValueError('symbol inventory changed')
            symbol_counts[table + '_definitions'] = len(audit['symbols'])
            symbol_counts[table + '_zil_function_definitions'] = sum(name.startswith('zil.fn.') for name in audit['symbols'])
        build_rows.append({'case': key[0], 'arm': key[1],
            'executable_bytes': build['executable']['bytes'], 'executable_sha256': build['executable']['sha256'],
            'text_bytes': size['text_bytes'], 'data_bytes': size['data_bytes'], 'bss_bytes': size['bss_bytes'],
            'actual_codegen_units': build.get('native_codegen', {}).get('actual_units'),
            'diagnostic_build_wall_seconds': build['process']['elapsed_ns'] / 1e9,
            'diagnostic_build_peak_rss_kib': build['process']['max_rss_kib'], **symbol_counts})
    if len(seen_builds) != 54:
        raise ValueError('incomplete prepared builds')
    csv_write(args.out / 'build-artifacts-diagnostic.csv', build_rows)
    random_source = random.Random(args.seed)
    # Each draw uses the same selected blocks for both arms and all five seeds
    # within a case. Keep the three forward and three reverse order strata.
    draws = [{case: [random_source.choice([0, 2, 4]) for _ in range(3)] +
                    [random_source.choice([1, 3, 5]) for _ in range(3)] for case in cases}
             for _ in range(args.bootstrap)]
    cells, comparisons = [], []
    for name, numerator, denominator in COMPARISONS:
        local, by_case = [], []
        samples_all = [[] for _ in draws]
        for case in cases:
            case_cells = []
            samples_case = [[] for _ in draws]
            for seed in seeds:
                top = [series[case, seed, numerator, block] for block in range(6)]
                bottom = [series[case, seed, denominator, block] for block in range(6)]
                value = statistics.median(top) / statistics.median(bottom)
                samples = []
                for index, draw in enumerate(draws):
                    selected = draw[case]
                    sample = statistics.median(top[i] for i in selected) / statistics.median(bottom[i] for i in selected)
                    samples.append(sample)
                    samples_all[index].append(sample)
                    samples_case[index].append(sample)
                interval = ci(samples)
                cell = {'comparison': name, 'case': case, 'seed': seed,
                        'numerator': numerator, 'denominator': denominator,
                        'numerator_median_ns': statistics.median(top), 'denominator_median_ns': statistics.median(bottom),
                        'median_ratio': value, 'median_paired_block_ratio': statistics.median(a / b for a, b in zip(top, bottom)),
                        'paired_bootstrap_95_low': interval[0], 'paired_bootstrap_95_high': interval[1],
                        'adverse_point_estimate': value > 1, 'adverse_interval': interval[0] > 1,
                        'favorable_interval': interval[1] < 1}
                cells.append(cell)
                local.append(cell)
                case_cells.append(cell)
            by_case.append({'case': case, 'geomean_ratio': gm(x['median_ratio'] for x in case_cells),
                            'paired_bootstrap_95': ci([gm(values) for values in samples_case]),
                            'adverse_cells': sum(x['adverse_point_estimate'] for x in case_cells),
                            'cells': 5})
        comparisons.append({'comparison': name, 'numerator': numerator, 'denominator': denominator,
                            'geomean_ratio': gm(x['median_ratio'] for x in local),
                            'paired_bootstrap_95': ci([gm(values) for values in samples_all]),
                            'adverse_cells': sum(x['adverse_point_estimate'] for x in local),
                            'adverse_intervals': sum(x['adverse_interval'] for x in local),
                            'favorable_intervals': sum(x['favorable_interval'] for x in local),
                            'cells': len(local), 'by_case': by_case})
    csv_write(args.out / 'cells.csv', cells)
    adverse = [cell for cell in cells if cell['adverse_point_estimate']]
    csv_write(args.out / 'all-adverse-cells.csv', adverse)
    summary = {'status': 'Pass', 'scope': __doc__, 'producer': identity(Path(__file__)),
        'inputs': {name: identity(args.prepared / name) for name in ['manifest.json', 'execution.json', 'prepared.json', 'builds.json', 'rows.jsonl']},
        'raw_validation': {'observations': 1620, 'builds': 54, 'failures': 0,
                           'checks': 'Unique complete protocol grid, balanced order, raw stdout/stderr/process records, complete output oracle, prepared executable/source/script/config identities, actual size and nm inventories'},
        'method': {'metric': 'Fresh native process elapsed_ns, including process launch, I/O and cleanup; no compilation inside the measured window',
                   'point_estimate': 'Ratio of two six-block medians for each case/seed; every cell and every case has equal geometric-mean weight',
                   'bootstrap': 'Exploratory percentile paired bootstrap; resample three forward and three reverse blocks separately; use the same block indices for both arms and all five seeds within each case; different cases sampled independently; cases themselves remain fixed',
                   'draws': args.bootstrap, 'seed': args.seed, 'confidence_level': 0.95,
                   'limits': 'Only six blocks and this fixed six-family suite. Intervals measure within-window variation, not workload-population uncertainty; no product gate or universal Rust comparison. Millisecond-scale arms include process-startup cost.'},
        'build_measurement_boundary': 'Each build was performed once in a concurrent functional window with loader instrumentation. Build time and peak RSS are diagnostic only; generated size is a specific artifact property. Rust links a different runtime, so whole-executable/text size is not a pure optimizer comparison.',
        'comparisons': comparisons, 'cells': cells, 'all_adverse_cells': adverse, 'build_artifacts_diagnostic': build_rows}
    save(args.out / 'summary.json', summary)
    lines = ['# Frozen runtime comparison', '',
             '1620 formal native observations and 54 frozen artifacts validated. A ratio below 1 means the numerator was faster. All comparisons use six-block medians per case/seed; no cell is discarded.', '',
             '| Comparison | GM ratio | Exploratory paired 95% interval | Adverse cells |',
             '|---|---:|---:|---:|']
    for comparison in comparisons:
        low, high = comparison['paired_bootstrap_95']
        lines.append(f'| {comparison["comparison"]} | {comparison["geomean_ratio"]:.6f} | [{low:.6f}, {high:.6f}] | {comparison["adverse_cells"]}/30 |')
    lines += ['', 'The incremental partition/import strategy has severe runtime regressions in this suite. Preserve it as a separate compile/runtime tradeoff, not equivalent O2 quality. The default candidate also retains the pre-existing body-family gap relative to Rust; the explicitly closed build removes that gap in this sample.', '',
              '| Family | Candidate/original | Candidate/Rust | Closed candidate/Rust | Incremental/candidate | Closed incremental/candidate |',
              '|---|---:|---:|---:|---:|---:|']
    table_names = ['candidate_original', 'candidate_rust', 'candidate_rust_closed', 'incremental_candidate', 'incremental_candidate_closed']
    mapping = {item['comparison']: {cell['case']: cell['geomean_ratio'] for cell in item['by_case']} for item in comparisons}
    for case in cases:
        lines.append('| ' + case + ' | ' + ' | '.join(f'{mapping[name][case]:.6f}' for name in table_names) + ' |')
    lines += ['', 'Full data: `arm-medians.csv` (270 arm cells), `cells.csv` (300 comparisons), `all-adverse-cells.csv` (every ratio above 1), `build-artifacts-diagnostic.csv` (all 54 artifacts), and `summary.json` (exact identities, intervals and methodology).', '',
              'Intervals are exploratory: the fixed cases are not resampled as if they represented all programs, and five seeds are not treated as independent block replications. One diagnostic build per arm cannot establish compile-time or peak-memory improvements. Rust and ZIL executable sizes include different runtime/linking contents.', '']
    (args.out / 'REPORT.md').write_text('\n'.join(lines))
    print(json.dumps({'status': summary['status'], 'cells': len(cells), 'adverse_cells': len(adverse),
                      'summary': identity(args.out / 'summary.json')}))


if __name__ == '__main__':
    main()
