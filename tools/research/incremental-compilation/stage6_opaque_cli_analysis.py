#!/usr/bin/env python3
"""Audit retained six-family opaque CLI records without executing their programs."""
import argparse
import ast
import datetime
import json
from pathlib import Path
import re
import shutil
import sys

# Importing shared analyzers must not create files beside frozen copies.
sys.dont_write_bytecode = True
import stage6_cli_analysis as common
import stage6_generated_cli_analysis as generated

CASES = ['512-arithmetic', '512-branch', '2048-arithmetic',
         '2048-branch', '512-body', '8192-arithmetic']
SEEDS = [0, 1, 7, 42, 255]
ROUNDS = 20000
TOOLS = ['stage6_opaque_bench.py', 'stage6_bench.py', 'stage6_merge_runtime.py']
REQUESTS = generated.REQUESTS
require, read, write = common.require, common.read, common.write
identity, checked, digest = common.identity, common.checked, common.digest


def independent_oracles(n, flavor):
    """Direct helper sums and cyclic visit counts, not the producer's residue sums."""
    require(flavor in ['arithmetic', 'branch', 'body'], 'Unknown opaque family')
    values = []
    for x in range(1009):
        if flavor == 'body':
            value = sum((x + 64 + helper % 97) % 1009 for helper in range(n))
        elif flavor == 'branch' and x % 3 != 0:
            value = sum((x + helper % 31) % 1009 for helper in range(n))
        else:
            value = sum((x * 3 + helper % 97) % 1009 for helper in range(n))
        values.append(value)
    baseline = {}
    cycles, remainder = divmod(ROUNDS, 1009)
    for seed in SEEDS:
        visits = [cycles] * 1009
        for step in range(remainder):
            visits[(seed + 17 * step) % 1009] += 1
        baseline[str(seed)] = sum(count * value for count, value in zip(visits, values)) % 1000003
    return {str(edit): {seed: (value + edit * ROUNDS) % 1000003 for seed, value in baseline.items()}
            for edit in [0, 1, 2]}


def source_reproduction_namespace(root):
    # Load only these two audited generator functions. No imports, top-level
    # benchmark code, bytecode loader, compiler, or native executable is run.
    namespace = {'__name__': 'frozen_source_reproduction', 're': re, 'ROUNDS': ROUNDS}
    for file, function in [('stage6_merge_runtime.py', 'generate'),
                           ('stage6_opaque_bench.py', 'generate_variant')]:
        nodes = [node for node in ast.parse((root / file).read_bytes(), filename=str(root / file)).body
                 if isinstance(node, ast.FunctionDef) and node.name == function]
        require(len(nodes) == 1, 'Frozen source generator function is not unique')
        exec(compile(ast.Module(body=nodes, type_ignores=[]), str(root / file), 'exec'), namespace)
    return namespace


def source_audit(root, output, manifest):
    namespace = source_reproduction_namespace(root)
    reproduce = output / 'source-reproduction'
    reproduce.mkdir()
    sources, oracles = {}, {}
    require(set(manifest['sources']) == {case + '/' + str(edit) for case in CASES for edit in [0, 1, 2]},
            'Incomplete or excess original source variants')
    for case in CASES:
        n, flavor = case.split('-', 1)
        oracles[case] = independent_oracles(int(n), flavor)
        for edit in [0, 1, 2]:
            key = case + '/' + str(edit)
            recorded = manifest['sources'][key]
            require(recorded['expected_by_seed'] == oracles[case][str(edit)], 'Independent opaque oracle differs')
            require(set(recorded['files']) == set(recorded['metrics']) == {'main.zil', 'main.rs'},
                    'Unexpected original variant file set')
            folder = reproduce / (case + '-' + str(edit))
            namespace['generate_variant'](folder, int(n), flavor, edit)
            variants = {}
            for name, record in recorded['files'].items():
                original = root / ('input-' + case + '-' + str(edit)) / name
                require(record['path'] == str(original), 'Original source path differs')
                actual = checked(record)
                require(digest(folder / name) == actual['sha256'], 'Original source differs from frozen reproduction')
                metrics = {'bytes': original.stat().st_size, 'lines': len(original.read_text().splitlines())}
                require(recorded['metrics'][name] == metrics, 'Original source metrics differ')
                variants[name] = actual
                if edit == 0:
                    require(digest(root / ('source-' + case) / name) == actual['sha256'],
                            'Retained final restore source differs from original')
            sources[key] = {'files': variants, 'metrics': recorded['metrics'],
                            'independently_expected_by_seed': oracles[case][str(edit)]}
            shutil.rmtree(folder)
    reproduce.rmdir()
    write(output / 'source-identities.json', sources)
    return sources, oracles


def invocation_audit(manifest, root):
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument('--pair-receipt', required=True)
    parser.add_argument('--comparison', required=True, choices=['isolation', 'rust'])
    parser.add_argument('--rustc')
    parser.add_argument('--out', required=True)
    parser.add_argument('--blocks', required=True, type=int)
    parser.add_argument('--case', action='append')
    parser.add_argument('--closed-executable', action='store_true')
    parser.add_argument('--incremental-o2', action='store_true')
    parser.add_argument('--timeout', type=int, default=180)
    parser.add_argument('--control-only', action='store_true')
    invocation = manifest['invocation']
    require(Path(invocation[0]).resolve() == Path(manifest['tools']['stage6_opaque_bench.py']['path']).resolve(),
            'Invocation runner identity differs')
    args = parser.parse_args(invocation[1:])
    require(Path(args.out).resolve() == root and Path(args.pair_receipt).resolve() == Path(manifest['pair']['path']),
            'Invocation output/pair differs')
    require(args.blocks == manifest['blocks'] == 6 and args.control_only is manifest['control_only'] is False,
            'Not a formal six-block protocol')
    require(args.comparison == manifest['comparison'] and args.timeout > 0, 'Invocation comparison/timeout differs')
    require(args.closed_executable is manifest['closed_executable'] and args.incremental_o2 is manifest['incremental_o2'],
            'Invocation export/incremental contract differs')
    require(args.case is None or args.case == CASES, 'Invocation does not select the full fixed corpus')
    if manifest['reference'] == 'rust':
        require(args.rustc is not None and Path(args.rustc).resolve() == Path(manifest['rustc']['path']),
                'Invocation Rust compiler differs')
    return {'timeout_seconds': args.timeout, 'command': invocation}


def setup(root, output, lease_path):
    manifest, lease = read(root / 'manifest.json'), read(lease_path)
    require(manifest['status'] == 'Completed' and lease['status'] == 'Released', 'Window or exclusive lease incomplete')
    require(manifest['cases'] == CASES and manifest['seeds'] == SEEDS and manifest['rounds'] == ROUNDS,
            'Unexpected fixed opaque corpus or native inputs')
    require(manifest['requests'] == [list(value) for value in REQUESTS], 'Unexpected request/edit sequence')
    require(manifest['planned_requests'] == manifest['observed_requests'] == manifest['completed_requests']
            == lease['planned_cli_requests'] == lease['observed_requests'] == 360, 'Incomplete or excess request grid')
    require(manifest['interrupted_requests'] == manifest['not_run_requests'] == 0 and read(root / 'not-run.json') == [],
            'Incomplete or interrupted window')
    require(manifest['source_commit'] == lease['source_commit'], 'Manifest and lease checkout identities differ')
    at = datetime.datetime.fromisoformat
    require(at(lease['started_utc']) <= at(manifest['started_utc']) < at(manifest['finished_utc']) <= at(lease['finished_utc']),
            'Runner is outside the recorded exclusive lease')
    require(len(manifest['cpus']) == 2 and len(set(manifest['cpus'])) == 2, 'Unexpected CPU affinity')
    require(manifest['comparison'] in ['isolation', 'rust'], 'Unsupported comparison')
    reference = 'original' if manifest['comparison'] == 'isolation' else 'rust'
    require(manifest['reference'] == reference and set(manifest['arms']) == {'original', 'candidate'}, 'Wrong pair/reference')
    require(set(manifest['tools']) == set(TOOLS), 'Unexpected frozen tools')
    tool_paths = ['tools/research/incremental-compilation/' + name for name in TOOLS]
    git_tools = common.git_archive_hashes(manifest['source_commit'], tool_paths)
    for name in TOOLS:
        record = manifest['tools'][name]
        require(digest(root / name) == record['sha256'] == git_tools['tools/research/incremental-compilation/' + name],
                'Frozen tool differs from its recorded source revision: ' + name)
        require((root / name).stat().st_size == record['bytes'], 'Frozen tool size differs')
    invocation = invocation_audit(manifest, root)
    if reference == 'rust':
        checked(manifest['rustc'])
    pair, compiler_audit = common.audit_pair({'pair': manifest['pair'], 'pair_contents': read(manifest['pair']['path'])})
    pair_arms = {arm['name']: arm for arm in pair['arms']}
    fresh_loaders = []
    for name, arm in manifest['arms'].items():
        original = pair_arms[name]
        require(read(root / (name + '-binding.json')) == arm, 'Fresh arm binding differs from manifest')
        for key in ['compiler', 'launcher', 'source_receipt']:
            require(checked(arm[key]) == checked(original[key]), 'Bound arm identity differs: ' + key)
        for key in ['library', 'receipt']:
            require(checked(arm['llvm'][key]) == checked(original['llvm'][key]), 'Bound LLVM identity differs')
        for key in ['name', 'toolchain_arm', 'production_revision', 'pipeline_identity_tag',
                    'original_files_manifest_sha256', 'env']:
            require(arm[key] == original[key], 'Bound arm metadata differs: ' + key)
        require(set(arm['env']) == {'LD_LIBRARY_PATH', 'ZIL_CLANG'}
                and Path(arm['env']['LD_LIBRARY_PATH']).resolve() == Path(arm['llvm']['library']['path']).parent,
                'Bound loader search environment differs')
        require(identity(arm['env']['ZIL_CLANG']) == checked(pair['clang']), 'Bound clang identity differs')
        probe = arm['fresh_loader']
        require(type(probe['pid']) is int and probe['pid'] > 0 and probe['exit'] == 0
                and probe['command'] == [arm['launcher']['path'], '--help'], 'Fresh loader probe failed or differs')
        log = root / (name + '-loader.' + str(probe['pid']))
        require(probe['log']['path'] == str(log), 'Fresh loader log path differs')
        checked(probe['log'])
        loaded = sorted({str(Path(path).resolve(strict=True))
                         for path in re.findall(r'calling init:\s*(/\S*libLLVM\S*)', log.read_text())})
        require(loaded == probe['actual_libraries'] == [arm['llvm']['library']['path']], 'Fresh loader selected another DSO')
        fresh_loaders.append({'arm': name, 'probe': probe,
                              'stdout': identity(root / (name + '-help.stdout')),
                              'stderr': identity(root / (name + '-help.stderr'))})
    sources, oracles = source_audit(root, output, manifest)
    return manifest, lease, reference, pair_arms, compiler_audit, fresh_loaders, invocation, sources, oracles


def command_for(root, manifest, row):
    case, arm, block, position = [row[key] for key in ['case', 'arm', 'block', 'position']]
    run = root / f'{case}-{block}-{position}-{arm}'
    source = root / ('source-' + case) / ('main.rs' if arm == 'rust' else 'main.zil')
    exe = run / 'program'
    if arm == 'rust':
        command = [manifest['rustc']['path'], str(source), '--edition=2024', '-A', 'warnings',
                   '-C', 'opt-level=2', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                   '-C', 'codegen-units=8', '-C', 'incremental=' + str(run / 'incremental'), '-o', str(exe)]
    else:
        command = [manifest['arms'][arm]['launcher']['path'], 'build', str(source), '--opt', 'O2',
                   '--codegen-units', '8', '--codegen-jobs', '2', '--function-merging', 'after',
                   '--semantic-cache', str(run / 'semantics'), '--object-cache', str(run / 'objects'), '--json', '-o', str(exe)]
        if manifest['closed_executable']:
            command.append('--closed-executable')
        if manifest['incremental_o2']:
            command.append('--incremental-o2')
    return command, run, source, exe


def audit_rows(root, output, manifest, lease, reference, pair_arms, sources, oracles):
    rows = [json.loads(line) for line in (root / 'rows.jsonl').read_text().splitlines()]
    grid = [(case, block, position, arm, request, edit) for case in CASES for block in range(6)
            for position, arm in enumerate(['candidate', reference] if block % 2 == 0 else [reference, 'candidate'])
            for request, edit in REQUESTS]
    require(len(rows) == len(grid) == 360, 'Incomplete or excess retained row grid')
    history, statuses, semantics, audited = {}, {}, {}, []
    native_count = verify_count = hits = misses = retained_elfs = 0
    for row, key in zip(rows, grid):
        require(tuple(row[field] for field in ['case', 'block', 'position', 'arm', 'request', 'edit']) == key,
                'Row grid or AB/BA sequence differs')
        case, block, position, arm, request, edit = key
        command, run, source, exe = command_for(root, manifest, row)
        require(row['command'] == command, 'Actual compiler options differ')
        source_record = sources[case + '/' + str(edit)]['files'][source.name]
        require(row['source'] == dict(source_record, path=str(source)), 'Row source identity differs from original variant')
        require(type(row['elapsed_ns']) is int and row['elapsed_ns'] > 0, 'Invalid full CLI time')
        require(row.get('max_rss_kib') is None or row['max_rss_kib'] > 0, 'Invalid peak RSS')
        stdout, stderr = run / (request + '.stdout'), run / (request + '.stderr')
        require(stdout.is_file() and stderr.is_file(), 'Missing compiler stdout/stderr')
        compiler_ok = type(row.get('exit')) is int and row['exit'] == 0 and row.get('timeout') is False and 'launch_error' not in row
        report, cache, report_problem = None, None, None
        if arm != 'rust':
            try:
                report = read(stdout)
                require(isinstance(report, dict), 'Malformed compiler report')
            except (ValueError, KeyError, TypeError) as error:
                report_problem = repr(error)
            if report_problem is None:
                # Row/raw identity mismatches are audit failures even when the
                # original compilation also failed for an unrelated reason.
                for field in ['status', 'verification_runs']:
                    require(row.get(field) == report.get(field), 'Retained report field differs: ' + field)
                status = case + '/' + arm + '/' + str(report.get('status'))
                statuses[status] = statuses.get(status, 0) + 1
                semantic = request + '/' + arm + '/' + str(report.get('disk_cache'))
                semantics[semantic] = semantics.get(semantic, 0) + 1
                if report.get('status') != 'Success' or report.get('verification_runs') != 1:
                    report_problem = 'Current compiler success/Verify contract failed'
                else:
                    try:
                        cache = common.object_audit(report['native_codegen'], pair_arms[arm]['pipeline_identity_tag'])
                    except (ValueError, KeyError, TypeError) as error:
                        report_problem = repr(error)
                    if report_problem is None:
                        require(row.get('native_codegen') == report['native_codegen'], 'Retained native report differs')
                        verify_count += 1
                        hits += cache['hits']
                        misses += cache['misses']
            if report_problem:
                require('report_error' in row, 'Independent report failure was not recorded by the producer')
                compiler_ok = False
            else:
                require('report_error' not in row, 'Producer rejected a report accepted by this audit')
        artifact = row.get('executable')
        if artifact is not None:
            require(artifact['path'] == str(exe) and re.fullmatch(r'[0-9a-f]{64}', artifact['sha256'])
                    and type(artifact['bytes']) is int and artifact['bytes'] > 0, 'Malformed historical ELF identity')
            require(compiler_ok, 'Executable identity recorded after a failed compiler/report')
            if request == 'restore':
                checked(artifact)
                retained_elfs += 1
        native = row.get('native')
        native_ok = isinstance(native, list) and len(native) == len(SEEDS)
        if native is not None:
            require(isinstance(native, list) and len(native) == len(SEEDS) and artifact is not None,
                    'Incomplete native oracle records')
            for outcome, seed in zip(native, SEEDS):
                expected = oracles[case][str(edit)][str(seed)]
                require(outcome['command'] == [str(exe)] and outcome['input_hex'] == bytes([seed]).hex()
                        and outcome['expected'] == expected, 'Native invocation or oracle identity differs')
                native_stdout = run / f'{request}-seed-{seed}.stdout'
                native_stderr = run / f'{request}-seed-{seed}.stderr'
                actual_stdout, actual_stderr = native_stdout.read_bytes(), native_stderr.read_bytes()
                require(outcome['stdout_hex'] == actual_stdout.hex() and outcome['stderr_hex'] == actual_stderr.hex(),
                        'Native row differs from retained original output')
                valid = (type(outcome.get('exit')) is int and outcome['exit'] == 0 and outcome.get('timeout') is False
                         and 'error' not in outcome and actual_stderr == b'' and actual_stdout == expected.to_bytes(4, 'little'))
                require(outcome['valid'] is valid, 'Native validity differs')
                native_ok = native_ok and valid
                native_count += 1
        valid = compiler_ok and artifact is not None and native_ok
        sequence = case, block, arm
        prior = history.get(sequence, True)
        require(row['execution_valid'] is bool(valid) and row['sequence_history_valid'] is prior
                and row['valid'] is (bool(valid) and prior), 'Execution/cache-history validity differs')
        history[sequence] = row['valid']
        audited.append({'case': case, 'block': block, 'position': position, 'arm': arm, 'request': request, 'edit': edit,
                        'execution_valid': bool(valid), 'sequence_history_valid': prior, 'comparison_valid': row['valid'],
                        'status': row.get('status'), 'exit': row.get('exit'), 'timeout': row.get('timeout'),
                        'launch_error': row.get('launch_error'), 'report_error': row.get('report_error'),
                        'independent_report_error': report_problem, 'source': row['source'], 'executable': artifact,
                        'native_outcomes': len(native) if native is not None else 0,
                        'elapsed_ns': row['elapsed_ns'], 'cpu_seconds': row.get('cpu_seconds'), 'max_rss_kib': row.get('max_rss_kib'),
                        'semantic_cache': report.get('disk_cache') if isinstance(report, dict) else None,
                        'verification_runs': row.get('verification_runs'), 'object_hits': cache['hits'] if cache else None,
                        'object_misses': cache['misses'] if cache else None, 'stdout': identity(stdout), 'stderr': identity(stderr)})
    invalid = sum(not row['valid'] for row in rows)
    require(invalid == lease['invalid_requests'], 'Lease failure count differs')
    with (output / 'request-audit.jsonl').open('w') as stream:
        for row in audited:
            stream.write(json.dumps(row) + '\n')
    return rows, {'requests': len(rows), 'valid_requests': len(rows) - invalid, 'invalid_requests': invalid,
                  'native_outcomes': native_count, 'current_successful_verify_requests': verify_count,
                  'directly_rehashed_final_restore_elfs': retained_elfs, 'status_counts': statuses,
                  'semantic_cache_counts': semantics, 'object_hits': hits, 'object_misses': misses,
                  'scope': 'Read-only retained-record audit; no compiler or native program was executed. All 18 original input variants and all retained native stdout/stderr files are checked. Historical elapsed/CPU/RSS and process exit/timeout values remain row observations, not independently re-proven historical process events. Historical ELF identities are retained row records; only final restore paths are directly rehashed. Both the frozen pair loader probes and this window\'s fresh compiler-PID probes are checked; this is not per-request loader tracing. Rust incremental cache contents are not inspected and cannot establish historical intermediate states.'}


def summarize(rows, reference, count, seed):
    # Reuse exactly the frozen paired/block-stratified method without altering
    # raw arm names. Each mapped dictionary is a separate in-memory copy.
    mapped = [dict(row, arm='zil' if row['arm'] == 'candidate' else row['arm']) for row in rows]
    cells, groups = generated.summarize(mapped, CASES, reference, count, seed)
    cells = [{('candidate' + key[3:] if key.startswith('zil') else key): value for key, value in cell.items()}
             for cell in cells]
    return cells, groups


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--window', type=Path, required=True)
    parser.add_argument('--lease', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--bootstrap', type=int, default=2000)
    parser.add_argument('--seed', type=int, default=6302026)
    args = parser.parse_args()
    require(args.bootstrap >= 100, 'Use at least 100 exploratory paired draws')
    root, output = args.window.resolve(strict=True), args.out.resolve()
    require(output != root and root not in output.parents, 'Analysis output must be outside the measured input tree')
    lease_path = (args.lease or root.with_name(root.name + '-exclusive-lease.json')).resolve(strict=True)
    output.mkdir(parents=True, exist_ok=False)
    producers = [Path(__file__), Path(common.__file__), Path(generated.__file__)]
    for path in producers:
        shutil.copy2(path, output / path.name)
    result = {'status': 'Auditing', 'producers': [identity(path) for path in producers], 'window': str(root)}
    try:
        manifest, lease, reference, arms, compiler_audit, loaders, invocation, sources, oracles = setup(root, output, lease_path)
        rows, audit = audit_rows(root, output, manifest, lease, reference, arms, sources, oracles)
        cells, groups = summarize(rows, reference, args.bootstrap, args.seed)
        original = read(root / 'summary.json')
        require(original['rows'] == 360 and original['failures'] == audit['invalid_requests']
                and len(original['cells']) == len(cells) == 30, 'Runner summary dimensions/failures differ')
        for independent, recorded in zip(cells, original['cells']):
            require((independent['case'], independent['request']) == (recorded['case'], recorded['request'])
                    and independent['valid'] is recorded['valid'], 'Summary cell order/validity differs')
            for arm, label in [('candidate', 'candidate'), (reference, 'reference')]:
                require(recorded['arms'][arm]['observations'] == 6
                        and independent[label + '_valid'] == recorded['arms'][arm]['valid'], 'Runner valid count differs')
                require(common.math.isclose(independent[label + '_median_ms'], recorded['arms'][arm]['median_ms'],
                                             rel_tol=1e-12, abs_tol=1e-9), 'Runner median differs')
                require(independent[label + '_median_peak_rss_kib'] == recorded['arms'][arm]['median_max_rss_kib'],
                        'Runner median peak RSS differs')
                raw_values = [row['elapsed_ns'] / 1e6 for row in rows
                              if (row['case'], row['request'], row['arm']) ==
                              (independent['case'], independent['request'], arm)]
                recorded_values = recorded['arms'][arm]['values_ms']
                require(len(recorded_values) == len(raw_values) == 6 and all(
                    common.math.isclose(actual, value, rel_tol=1e-12, abs_tol=1e-9)
                    for actual, value in zip(raw_values, recorded_values)), 'Runner timing array differs')
            actual, expected = independent['candidate_over_reference'], recorded['candidate_over_' + reference]
            require((actual is None and expected is None) or (actual is not None and expected is not None
                    and common.math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-15)), 'Runner point ratio differs')
        fields = list(cells[0])
        common.csv_write(output / 'cells.csv', cells, fields)
        common.csv_write(output / 'all-adverse-cells.csv', [cell for cell in cells if cell['adverse_point_estimate']], fields)
        common.csv_write(output / 'all-invalid-cells.csv', [cell for cell in cells if not cell['valid']], fields)
        resource_fields = ['case', 'block', 'position', 'arm', 'request', 'edit', 'status', 'valid', 'execution_valid',
                           'sequence_history_valid', 'elapsed_ns', 'cpu_seconds', 'max_rss_kib']
        common.csv_write(output / 'request-resources.csv', [{field: row.get(field) for field in resource_fields} for row in rows], resource_fields)
        result.update(status='AuditComplete', source_commit=manifest['source_commit'], comparison=manifest['comparison'],
                      reference=reference, closed_executable=manifest['closed_executable'], incremental_o2=manifest['incremental_o2'],
                      invocation=invocation, exclusive_lease=identity(lease_path),
                      inputs={name: identity(root / name) for name in ['manifest.json', 'rows.jsonl', 'summary.json', 'not-run.json', *TOOLS]},
                      compiler_audit=compiler_audit, fresh_loader_audit=loaders, source_inventory=sources,
                      audit=audit, cells=cells, groups=groups,
                      method={'point_estimate': 'Ratio of arm six-block medians per fixed case/request.',
                              'bootstrap': 'Exploratory percentile paired bootstrap: resample three forward and three reverse blocks; share block indices across both arms and all five requests within each case. Fixed cases have independently sampled block indices.',
                              'draws': args.bootstrap, 'seed': args.seed, 'confidence_level': 0.95,
                              'aggregation': 'Equal geometric-mean weight per valid fixed cell. All failed cells remain listed without a ratio. No random workload-population interpretation.',
                              'limits': 'Only six fixed opaque families and six blocks on this machine. Native oracle executions are correctness evidence, not runtime-performance measurements. Cold compiler caches do not imply a cold OS page cache. wait4 peak RSS is not aggregate system memory.',
                              'contract': 'Explicit O2, after-MergeFunctions, independent object and semantic caches, eight requested CGUs/two requested ZIL codegen jobs. Closed/export and incremental switches are reported separately and symmetric for ZIL isolation. Rust uses O2, overflow checks, abort panic, eight CGUs and persistent per-sequence incremental caches.'})
        lines = ['# Independent opaque-source CLI audit', '',
                 f'Comparison `{manifest["comparison"]}`; closed={manifest["closed_executable"]}, incremental={manifest["incremental_o2"]}. '
                 f'{audit["requests"]} requests, {audit["valid_requests"]} valid, {audit["invalid_requests"]} invalid. All 30 cells remain visible.', '',
                 '| Request group | Candidate/reference GM | Exploratory paired 95% interval | Adverse / valid | Invalid |',
                 '|---|---:|---:|---:|---:|']
        for group in groups:
            low, high = group['paired_bootstrap_95'] or [float('nan'), float('nan')]
            ratio = group['geomean_ratio'] if group['geomean_ratio'] is not None else float('nan')
            lines.append(f'| {group["group"]} | {ratio:.6f} | [{low:.6f}, {high:.6f}] | {group["adverse_cells"]}/{group["valid_cells"]} | {group["invalid_cells"]} |')
        lines += ['', f'Reference `{reference}`. Ratios above one are slower for the candidate; invalid cells are excluded from ratios and retained in the failure table.',
                  '', audit['scope'], '', result['method']['limits'], '', result['method']['contract'], '']
        (output / 'REPORT.md').write_text('\n'.join(lines))
    except BaseException as error:
        result.update(status='AuditFailed', error=repr(error))
        raise
    finally:
        write(output / 'summary.json', result)
        print(json.dumps({'status': result['status'], 'summary': str(output / 'summary.json')}))


if __name__ == '__main__':
    main()
