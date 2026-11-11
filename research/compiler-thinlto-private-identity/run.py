#!/usr/bin/env python3
"""Two-case quality probe; never executes without --execute and protocol hash."""
import argparse
import json
import os
from pathlib import Path
import shutil
import traceback
from common import *
from screen import validate_zil, artifact_identity
from hook_checks import ENVIRONMENT

ACTIVE = {}

def main():
    q = argparse.ArgumentParser(description=__doc__)
    q.add_argument('--protocol', type=Path, required=True)
    q.add_argument('--protocol-sha256', required=True)
    q.add_argument('--out', type=Path, required=True)
    q.add_argument('--execute', action='store_true')
    a = q.parse_args()
    a.protocol = a.protocol.resolve(strict=True)
    if sha(a.protocol) != a.protocol_sha256:
        raise ValueError('protocol hash mismatch')
    p = json.loads(a.protocol.read_text())
    validate(p)
    for arm, receipt in p['build_receipts'].items():
        if identity(receipt['path']) != receipt:
            raise ValueError('build receipt changed')
        binding = json.loads(Path(receipt['path']).read_text())
        tool = p['tools']['baseline' if arm == 'A' else 'exporter']
        if binding['source_commit'] != p['source_claims'][arm] or binding['binary_sha256'] != tool['sha256']:
            raise ValueError('source/binary binding absent')
    if set(p['cases']) != {'n64/base', 'n256/base'}:
        raise ValueError('two fixed base cases required')
    for case, fixture in p['cases'].items():
        for name, digest in fixture['source_hashes'].items():
            if sha(a.protocol.parent / case / name) != digest:
                raise ValueError('frozen source changed')
        if fixture['selected_indices'] or fixture['runtime_oracle']['iterations'] != 100000:
            raise ValueError('expected unchanged base with fixed runtime loop')
    if not a.execute:
        print(json.dumps({'status': 'not-executed', 'protocol_sha256': sha(a.protocol)}))
        return
    a.out = a.out.resolve()
    a.out.mkdir(parents=True, exist_ok=False)
    ACTIVE.update(out=str(a.out), phase='tool-validation', compilations_completed=0, native_rows_written=0)
    write(a.out / 'execution-manifest.json', {'status': 'running', 'protocol_sha256': sha(a.protocol)})
    shutil.copy2(a.protocol, a.out / 'protocol.json')
    os.sched_setaffinity(0, p['cpus'])
    # Prevent any ambient research opt-in from contaminating A or Rust.
    controlled = ['ZIL_RESEARCH_THINLTO_RECEIPT', 'ZIL_RESEARCH_THINLTO_COMPLETE', 'ZIL_THINLTO_PROTOCOL', 'ZIL_THINLTO_PROTOCOL_SHA', 'ZIL_THINLTO_LOGDIR'] + ENVIRONMENT
    for name in controlled:
        os.environ.pop(name, None)
    os.environ['ZIL_CLANG'] = p['tools']['clang']['path']
    for name in ['clang', 'lld', 'opt', 'bcanalyzer', 'dis', 'rustc']:
        flag = '-vV' if name == 'rustc' else '--version'
        run([p['tools'][name]['path'], flag], a.out / ('version-' + name), timeout=30)
        version = (a.out / ('version-' + name) / 'stdout').read_text()
        if ('release: 1.99.1' if name == 'rustc' else '23.1.4') not in version:
            raise ValueError('required actual tool version mismatch: ' + name)
    compilations, rows, summaries = [], [], []
    for case, fixture in p['cases'].items():
        ACTIVE.update(phase='compilation', case=case)
        source, case_root = a.protocol.parent / case, a.out / case
        programs, artifacts = {}, {}
        for arm in ['A', 'B', 'R']:
            ACTIVE.update(phase='compile-' + arm, arm=arm)
            root = case_root / ('compile-' + arm)
            output = root / 'program'
            for name in controlled:
                os.environ.pop(name, None)
            if arm == 'R':
                command = [p['tools']['rustc']['path'], source / 'main.rs', '--edition=2024', '-A', 'dead_code',
                           '-C', 'opt-level=2', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                           '-C', 'codegen-units=8', '-C', 'linker=' + p['tools']['clang']['path'], '-o', output]
            else:
                tool = p['tools']['baseline' if arm == 'A' else 'exporter']['path']
                clang = p['tools']['clang']['path'] if arm == 'A' else str(ROOT / 'clang_wrapper.py')
                command = [tool, 'build', source / 'main.zil', '--json', '--backend',
                           'llvm-api' if arm == 'A' else 'clang', '--opt', 'O2',
                           '--incremental-o2', '--function-merging', 'after',
                           '--codegen-units', '8', '--codegen-jobs', '2', '--clang', clang, '-o', output]
                if arm == 'B':
                    if (root / 'wrapper').exists():
                        raise ValueError('a compiler request must start without any identity stamp or wrapper directory')
                    os.environ.update(ZIL_RESEARCH_THINLTO_RECEIPT=str(root / 'preflight.json'),
                                      ZIL_RESEARCH_THINLTO_COMPLETE='1',
                                      ZIL_THINLTO_PROTOCOL=str(a.protocol),
                                      ZIL_THINLTO_PROTOCOL_SHA=sha(a.protocol),
                                      ZIL_THINLTO_LOGDIR=str(root / 'wrapper'))
            metric = run(command, root, timeout=600)
            # Metrics/process attempts are already durable before decoding.
            evidence = {}
            if arm != 'R':
                report = json.loads((root / 'stdout').read_text())
                if report.get('status') != 'Success' or report.get('verification_runs') != 1 or len(report.get('source_files', {})) != fixture['total_files_per_language']:
                    raise ValueError('current Verify/source closure incomplete')
                if arm == 'A':
                    errors, evidence = validate_zil(report, fixture['total_files_per_language'],
                        {'policy': 'incremental-o2-after', 'fixed_toolchains_required': {'llvm': '23.1.4'}})
                    if errors:
                        raise ValueError(errors)
                    for receipt in report['native_codegen']['object_reports']:
                        if receipt.get('cpu') != 'x86-64' or receipt.get('features') != '' or receipt.get('relocation') != 'PIC' or receipt.get('code_model') != 'default':
                            raise ValueError('A target profile differs from frozen API')
                else:
                    evidence = preflight(root / 'preflight.json')
                    native = report['native_codegen']
                    if native['backend'] != 'clang' or native['actual_units'] != 8 or native['fallback'] is not None:
                        raise ValueError('canonical partition fallback or wrong backend')
                    if native['object_reports'] != [evidence]:
                        raise ValueError('external/current structured model receipts differ')
                    receipts = [json.loads(v.read_text()) for v in (root / 'wrapper').glob('*/receipt.json')]
                    units = [v for v in receipts if v['kind'] == 'unit-prelink']
                    links = [v for v in receipts if v['kind'] == 'thin-link']
                    if len(units) != 8 or len(links) != 1 or not links[0]['cache_was_initially_empty']:
                        raise ValueError('incomplete real ThinLTO compile/link or nonclean cache')
                    if links[0]['visibility']['post_import_complete_checked'] is not True:
                        raise ValueError('actual prelink/.3 complete visibility proof missing')
                    if links[0]['private_imports']['all_eight_actual_hooks_checked'] is not True:
                        raise ValueError('actual private import transformation was not witnessed')
                    evidence = {'machine': evidence, 'actual_thinlto': receipts}
                artifacts[arm] = artifact_identity(report)
            programs[arm] = identity(output)
            record = {'case': case, 'arm': arm, 'program': programs[arm], 'metrics': metric,
                      'verification_evidence': evidence, 'compile_performance_claim': False}
            write(root / 'validated.json', record)
            compilations.append(record)
            ACTIVE['compilations_completed'] = len(compilations)
        for name in controlled:
            os.environ.pop(name, None)
        write(case_root / 'artifacts.json', {'arms': artifacts, 'different_code_allowed_only_with_actual_runtime_gate': True})
        vectors = {v['stdin_hex']: v['stdout_hex'] for v in fixture['native_vectors']}
        schedule = [('functional', i, arm, byte) for byte in ['00', 'ff'] for i, arm in enumerate('ABR')]
        schedule += [('warmup', 0, arm, '41') for arm in 'ABR']
        schedule += [('formal', i, arm, '41') for i, order in enumerate(p['orders']) for arm in order]
        for position, (kind, round_index, arm, byte) in enumerate(schedule):
            ACTIVE.update(phase='native-' + kind, arm=arm, position=position, round=round_index)
            if identity(programs[arm]['path']) != programs[arm]:
                raise ValueError('artifact changed after compilation')
            log = case_root / 'native' / f'{position:03d}-{kind}-{arm}'
            # Do not throw on invalid output: preserve all formal observations,
            # then fail the entire case. Launch/timeout exceptions remain fatal.
            metrics = measured([programs[arm]['path']], log, 120, bytes.fromhex(byte))
            write(log / 'raw-metrics.json', metrics)
            stdout, stderr = (log / 'stdout').read_bytes(), (log / 'stderr').read_bytes()
            row = {'case': case, 'kind': kind, 'arm': arm, 'round': round_index, 'position': position,
                   'stdin_hex': byte, 'expected_stdout_hex': vectors[byte], 'stdout_hex': stdout.hex(),
                   'stderr_hex': stderr.hex(), 'artifact': programs[arm], **metrics,
                   'valid': metrics['exit'] == 0 and not metrics['timeout'] and metrics['exception'] is None
                            and not metrics['residual_descendants']
                            and not stderr and stdout.hex() == vectors[byte]}
            write(log / 'record.json', row)
            rows.append(row)
            ACTIVE['native_rows_written'] = len(rows)
            with (a.out / 'native-rows.jsonl').open('a') as f:
                f.write(json.dumps(row) + '\n')
        arms = {}
        for arm in 'ABR':
            formal = [v for v in rows if v['case'] == case and v['arm'] == arm and v['kind'] == 'formal']
            if len(formal) != 6:
                raise ValueError('incomplete formal cell')
            arms[arm] = {metric: stats([v[metric] for v in formal]) for metric in ['wall_ms', 'cpu_ms', 'rss_kib']}
        av, bv, rv = [arms[arm]['wall_ms']['mean'] for arm in 'ABR']
        ratios = {'B_over_A': bv / av if av and bv else None, 'R_over_B': rv / bv if rv and bv else None}
        valid = all(v['valid'] for v in rows if v['case'] == case)
        passed = valid and ratios['B_over_A'] is not None and ratios['R_over_B'] is not None and ratios['B_over_A'] <= 1.10 and ratios['R_over_B'] >= .90
        summary = {'case': case, 'arms': arms, 'ratios': ratios, 'all_outputs_valid': valid, 'gate_pass': passed}
        summaries.append(summary)
        write(case_root / 'runtime-summary.json', summary)
    if len(compilations) != 6 or len(rows) != 54 or sum(v['kind'] == 'formal' for v in rows) != 36:
        raise ValueError('fixed complete counts missing')
    passed = all(v['gate_pass'] for v in summaries)
    result = {'format': 'zil-thinlto-private-identity-runtime-result/1', 'status': 'Pass' if passed else 'NoGo',
              'gate_pass': passed, 'protocol_sha256': sha(a.protocol), 'counts': {'clean_compilations': 6, 'native_total': 54, 'native_formal': 36},
              'cases': summaries, 'outliers_removed': 0, 'scope': p['scope'],
              'RSS_caveat': 'wait4 ru_maxrss is not aggregate peak and may include launcher/fork floor',
              'old_eight_case_expansion_authorized_by_result': passed,
              'product_adoption': False, 'compile_speed_gate': False}
    write(a.out / 'summary.json', result)
    write(a.out / 'execution-manifest.json', {'status': result['status'], 'completed': True, 'protocol_sha256': sha(a.protocol)})
    print(json.dumps(result, indent=2))
    if not passed:
        raise SystemExit(1)

if __name__ == '__main__':
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        if ACTIVE.get('out'):
            root = Path(ACTIVE['out'])
            write(root / 'fatal-failure.json', {**ACTIVE, 'exception': repr(error), 'traceback': traceback.format_exc(),
                  'remaining_work': 'not completed; no partial case or summary is Pass'})
            write(root / 'execution-manifest.json', {'status': 'Incomplete', 'completed': False, 'failure': 'fatal-failure.json'})
        raise
