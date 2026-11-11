#!/usr/bin/env python3
"""Balanced complete-request compilation screen; no execution without a hash."""
import argparse
import json
import os
from pathlib import Path
import shutil
import traceback
from common import *
from screen import validate_zil, artifact_identity

ACTIVE = {}
CONTROLLED = CONTROLLED_ENV


def clear_research():
    for name in CONTROLLED:
        os.environ.pop(name, None)


def validate_compilation(root, arm, fixture, p):
    if arm == 'R':
        return {'rust_compilation_exit_verified': True}
    report = json.loads((root / 'stdout').read_text())
    if (report.get('status') != 'Success' or report.get('verification_runs') != 1
            or len(report.get('source_files', {})) != fixture['total_files_per_language']
            or sum(v == {'phase': 'verify', 'disposition': 'executed'} for v in report.get('query_events', [])) != 1):
        raise ValueError('current independent Verify/source closure incomplete')
    if arm == 'A':
        errors, evidence = validate_zil(report, fixture['total_files_per_language'],
            {'policy': 'incremental-o2-after', 'fixed_toolchains_required': {'llvm': '23.1.4'}})
        if errors:
            raise ValueError(errors)
        for obj in report['native_codegen']['object_reports']:
            if (obj.get('cpu') != 'x86-64' or obj.get('features') != ''
                    or obj.get('relocation') != 'PIC' or obj.get('code_model') != 'default'):
                raise ValueError('baseline target differs from frozen profile')
    else:
        evidence = validate_candidate_receipt(root / 'preflight.json', report, fixture, p)
    return {'verification': evidence, 'artifact': artifact_identity(report)}


def main():
    q = argparse.ArgumentParser(description=__doc__)
    q.add_argument('--protocol', type=Path, required=True)
    q.add_argument('--protocol-sha256', required=True)
    q.add_argument('--out', type=Path, required=True)
    q.add_argument('--execute', action='store_true')
    a = q.parse_args()
    a.protocol = a.protocol.resolve(strict=True)
    if sha(a.protocol) != a.protocol_sha256:
        raise ValueError('protocol SHA mismatch')
    p = json.loads(a.protocol.read_text())
    validate(p)
    expected_counts = {'cases': 2, 'formal_compilations': 36, 'native_total': 108,
                       'native_formal': 36, 'native_functional': 72}
    if (p.get('compile_screen_counts') != expected_counts
            or p.get('identity_validation_policy') != 'symmetric-process-external-prevalidation; complete-native-compiler-request-timed'
            or list(p['cases']) != ['n64/base', 'n256/base']):
        raise ValueError('unknown compile screen counts or identity policy')
    for case, fixture in p['cases'].items():
        if fixture['selected_indices'] or fixture['runtime_oracle']['iterations'] != 100000:
            raise ValueError('fixed base runtime fixture required')
        for name, digest in fixture['source_hashes'].items():
            if sha(a.protocol.parent / case / name) != digest:
                raise ValueError('actual frozen source changed')
    if not a.execute:
        print(json.dumps({'status': 'not-executed', 'protocol_sha256': sha(a.protocol)}))
        return
    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    ACTIVE.update(out=str(out), phase='versions', compilations=0, native=0)
    write(out / 'execution-manifest.json', {'status': 'Running', 'protocol_sha256': sha(a.protocol)})
    shutil.copy2(a.protocol, out / 'protocol.json')
    os.sched_setaffinity(0, p['cpus'])
    clear_research()
    os.environ['ZIL_CLANG'] = p['tools']['clang']['path']
    for name in ['clang', 'rustc']:
        run([p['tools'][name]['path'], '-vV' if name == 'rustc' else '--version'], out / ('version-' + name), timeout=30)
        version = (out / ('version-' + name) / 'stdout').read_text()
        if ('release: 1.99.1' if name == 'rustc' else '23.1.4') not in version:
            raise ValueError('actual tool version mismatch')
        if name == 'rustc' and 'LLVM version: 23.1.1' not in version:
            raise ValueError('actual Rust bundled LLVM version differs from pinned reference')
    compiles, native_rows, summaries = [], [], []
    for case, fixture in p['cases'].items():
        source = a.protocol.parent / case
        vectors = {v['stdin_hex']: v['stdout_hex'] for v in fixture['native_vectors']}
        for round_index, order in enumerate(p['orders']):
            for position, arm in enumerate(order):
                ACTIVE.update(phase='compile', case=case, round=round_index, arm=arm)
                root = out / case / f'round-{round_index}' / ('compile-' + arm)
                output = root / 'program'
                clear_research()
                if arm == 'R':
                    command = [p['tools']['rustc']['path'], source / 'main.rs', '--edition=2024', '-A', 'dead_code',
                        '-C', 'opt-level=2', '-C', 'overflow-checks=yes', '-C', 'panic=abort', '-C', 'codegen-units=8',
                        '-C', 'linker=' + p['tools']['clang']['path'], '-o', output]
                else:
                    command = [p['tools']['baseline' if arm == 'A' else 'candidate']['path'], 'build',
                        source / 'main.zil', '--json', '--backend', 'llvm-api',
                        '--opt', 'O2', '--incremental-o2', '--function-merging', 'after', '--codegen-units', '8',
                        '--codegen-jobs', '2', '--clang', p['tools']['clang']['path'],
                        '-o', output]
                    if arm == 'B':
                        receipt = (root / 'preflight.json').absolute()
                        if receipt.exists():
                            raise ValueError('fresh native current receipt required')
                        os.environ['ZIL_RESEARCH_PRELINK_RECEIPT'] = str(receipt)
                # Every arm's actual compiler bytes are checked OUTSIDE the
                # measured request; no wrapper/image hashing inside B.
                selected_tool = p['tools']['rustc' if arm == 'R' else 'baseline' if arm == 'A' else 'candidate']
                if identity(selected_tool['path']) != selected_tool:
                    raise ValueError('actual compiler changed before request')
                metric = run(command, root, timeout=600)
                if identity(selected_tool['path']) != selected_tool:
                    raise ValueError('actual compiler changed during request; raw retained')
                proof = validate_compilation(root, arm, fixture, p)
                program = identity(output)
                record = {'case': case, 'arm': arm, 'round': round_index, 'position': position,
                          'program': program, 'metrics': metric, 'evidence': proof}
                write(root / 'validated.json', record)
                compiles.append(record)
                ACTIVE['compilations'] = len(compiles)
                with (out / 'compile-rows.jsonl').open('a') as stream:
                    stream.write(json.dumps(record) + '\n')
                clear_research()
                for byte in ['00', 'ff', '41']:
                    ACTIVE.update(phase='native', byte=byte)
                    if identity(output) != program:
                        raise ValueError('compiled executable changed')
                    log = root / ('native-' + byte)
                    metrics = measured([str(output)], log, 120, bytes.fromhex(byte))
                    write(log / 'raw-metrics.json', metrics)
                    stdout, stderr = (log / 'stdout').read_bytes(), (log / 'stderr').read_bytes()
                    row = {'case': case, 'arm': arm, 'round': round_index, 'position': position,
                           'kind': 'formal' if byte == '41' else 'functional', 'stdin_hex': byte,
                           'expected_stdout_hex': vectors[byte], 'stdout_hex': stdout.hex(), 'stderr_hex': stderr.hex(),
                           'artifact': program, **metrics,
                           'valid': metrics['exit'] == 0 and not metrics['timeout'] and metrics['exception'] is None
                                    and not metrics['residual_descendants'] and not stderr and stdout.hex() == vectors[byte]}
                    write(log / 'record.json', row)
                    native_rows.append(row)
                    ACTIVE['native'] = len(native_rows)
                    with (out / 'native-rows.jsonl').open('a') as stream:
                        stream.write(json.dumps(row) + '\n')
        compiler, runtime, identities = {}, {}, {}
        for arm in 'ABR':
            selected = [v for v in compiles if v['case'] == case and v['arm'] == arm]
            formal = [v for v in native_rows if v['case'] == case and v['arm'] == arm and v['kind'] == 'formal']
            if len(selected) != 6 or len(formal) != 6:
                raise ValueError('incomplete balanced sample cell')
            compiler[arm] = {key: stats([v['metrics'][key] for v in selected]) for key in ['wall_ms', 'cpu_ms', 'rss_kib']}
            runtime[arm] = {key: stats([v[key] for v in formal]) for key in ['wall_ms', 'cpu_ms', 'rss_kib']}
            identities[arm] = sorted({v['program']['sha256'] for v in selected})
            if len(identities[arm]) != 1:
                raise ValueError('fixed-source artifact differs across six rounds; all raw retained')
            if arm != 'R' and len({v['evidence']['artifact']['sidecar_without_verified_fingerprint_sha256'] for v in selected}) != 1:
                raise ValueError('fixed-source material IR differs across six rounds; all raw retained')
        ratios = {'compile_B_over_A_wall': compiler['B']['wall_ms']['mean'] / compiler['A']['wall_ms']['mean'],
                  'compile_B_over_A_CPU': compiler['B']['cpu_ms']['mean'] / compiler['A']['cpu_ms']['mean'],
                  'compile_R_over_B_wall': compiler['R']['wall_ms']['mean'] / compiler['B']['wall_ms']['mean'],
                  'runtime_B_over_A': runtime['B']['wall_ms']['mean'] / runtime['A']['wall_ms']['mean'],
                  'runtime_R_over_B': runtime['R']['wall_ms']['mean'] / runtime['B']['wall_ms']['mean']}
        valid = all(v['valid'] for v in native_rows if v['case'] == case)
        passed = valid and ratios['runtime_B_over_A'] <= 1.10 and ratios['runtime_R_over_B'] >= .90
        summaries.append({'case': case, 'compile': compiler, 'runtime': runtime, 'ratios': ratios,
                          'all_outputs_valid': valid, 'runtime_gate_pass': passed,
                          'observed_whole_program_identities': identities,
                          'compile_wall_and_CPU_both_lower_than_main': ratios['compile_B_over_A_wall'] < 1 and ratios['compile_B_over_A_CPU'] < 1})
    if (len(compiles) != 36 or len(native_rows) != 108
            or sum(v['kind'] == 'formal' for v in native_rows) != 36):
        raise ValueError('incomplete fixed screen')
    result = {'format': 'zil-manual-prelink-compile-screen-result/1', 'status': 'Complete',
              'protocol_sha256': sha(a.protocol), 'counts': expected_counts, 'cases': summaries,
              'runtime_gate_pass': all(v['runtime_gate_pass'] for v in summaries), 'outliers_removed': 0,
              'scope': p['scope'], 'cold_request_only': True, 'production_adoption': False,
              'many_edit_or_concurrency_claim': False, 'weighted_product_gate': False,
              'compiler_identity_prevalidation_outside_timed_requests': True,
              'actual_prelink_postlink_checks_inside_B': True,
              'RSS_caveat': 'wait4 high-water is not aggregate peak and may include launcher/fork floor'}
    # End-of-screen full images/scripts/build/reference recheck remains outside all compiler/native timing.
    clear_research()
    validate(p)
    write(out / 'summary.json', result)
    write(out / 'execution-manifest.json', {'status': 'Complete', 'completed': True, 'protocol_sha256': sha(a.protocol)})
    print(json.dumps(result, indent=2))
    if not result['runtime_gate_pass']:
        raise SystemExit(1)


if __name__ == '__main__':
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        if ACTIVE.get('out'):
            out = Path(ACTIVE['out'])
            write(out / 'fatal-failure.json', {**ACTIVE, 'exception': repr(error), 'traceback': traceback.format_exc()})
            write(out / 'execution-manifest.json', {'status': 'Incomplete', 'completed': False})
        raise
