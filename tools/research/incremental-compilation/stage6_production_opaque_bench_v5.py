#!/usr/bin/env python3
"""New production-quality opaque CLI profile; old pair runner remains unchanged."""
import argparse
import datetime
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

sys.dont_write_bytecode = True
import stage6_opaque_bench as frozen
from stage6_production_identity_v5 import PIPELINE, load_production_manifest, validate_binding, load_rust_reference
from stage6_production_ledger import request_ledger

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
TOOLS = ['stage6_production_opaque_bench_v5.py', 'stage6_production_identity_v5.py', 'stage6_toolchain_runtime.py',
         'stage6_opaque_bench.py', 'stage6_merge_runtime.py', 'stage6_bench.py', 'stage6_production_ledger.py']
REQUESTS = frozen.REQUESTS


def validate_report(report):
    if report.get('status') != 'Success' or report.get('verification_runs') != 1:
        raise ValueError('current Program Verify must succeed once')
    native = report['native_codegen']
    if 'fallback' not in native or (native['fallback'] is not None and not isinstance(native['fallback'], str)):
        raise ValueError('missing or malformed explicit fallback outcome')
    if native['backend'] != 'llvm-api' or native['requested_units'] != 8 or not (0 < native['jobs'] <= 2):
        raise ValueError('unexpected production backend or requested parallelism')
    if native['actual_units'] <= 0 or len(native['object_reports']) != native['actual_units']:
        raise ValueError('incomplete actual object reports')
    for item in native['object_reports']:
        if item['pipeline'] != PIPELINE or item['backend'] != 'llvm-api' or item['llvm_version'] != '23.1.4':
            raise ValueError('wrong production object pipeline/producer')
        if item['input_kind'] == 'object-cache':
            if (item['cache']['state'] != 'hit' or item['producer_verified_before_after'] is not True
                    or item['llvm_verification_this_request'] is not False):
                raise ValueError('cache hit lacks verified producer boundary')
        elif item['input_kind'] != 'builder' or item.get('verify_before_after') is not True:
            raise ValueError('new object lacks pre/post LLVM verification')
    cache = native['object_cache']
    if (cache['requested'] is not True or cache['enabled'] is not True or cache['disabled_reason'] is not None
            or cache['hits'] + cache['misses'] != native['actual_units']
            or cache['hits'] != sum(item['cache']['state'] == 'hit' for item in native['object_reports'])
            or cache['misses'] != sum(item['cache']['state'] == 'miss' for item in native['object_reports'])):
        raise ValueError('unexpected object cache state/accounting')
    return native


def command_for(manifest, root, case, block, position, arm):
    run = root / f'{case}-{block}-{position}-{arm}'
    source = root / ('source-' + case) / ('main.rs' if arm == 'rust' else 'main.zil')
    exe = run / 'program'
    if arm == 'rust':
        command = [manifest['rustc']['path'], str(source), '--edition=2024', '-A', 'warnings',
                   '-C', 'opt-level=2', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                   '-C', 'codegen-units=8', '-C', 'incremental=' + str(run / 'incremental'), '-o', str(exe)]
    else:
        command = [manifest['arms']['candidate']['launcher']['path'], 'build', str(source), '--opt', 'O2',
                   '--codegen-units', '8', '--codegen-jobs', '2', '--function-merging', 'after',
                   '--semantic-cache', str(run / 'semantics'), '--object-cache', str(run / 'objects'),
                   '--json', '-o', str(exe), '--backend', 'llvm-api', '--incremental-o2']
        if manifest['closed_executable']:
            command.append('--closed-executable')
    return command, run, source, exe


def prepare(args, out):
    profile = load_production_manifest(args.production)
    binding_path = args.binding.resolve(strict=True)
    binding = validate_binding(profile, binding_path)
    reference = load_rust_reference(args.rust_reference)
    launcher = frozen.checked(binding['launcher'])
    cpus = sorted(os.sched_getaffinity(0))[:2]
    os.sched_setaffinity(0, cpus)
    cases = [(n, flavor) for n, flavor in frozen.CASES if not args.case or f'{n}-{flavor}' in args.case]
    manifest = {'schema': 1, 'kind': 'production-quality-opaque-cli-v5', 'status': 'Preparing', 'control_only': args.control_only,
                'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(),
                'production': profile['_manifest_identity'], 'production_profile': profile, 'binding': frozen.identity(binding_path),
                'binding_contents': binding, 'cpus': cpus, 'blocks': args.blocks, 'cases': [f'{n}-{flavor}' for n, flavor in cases],
                'comparison': 'production-quality-rust-v5', 'reference': 'rust', 'requests': REQUESTS,
                'rounds': frozen.ROUNDS, 'seeds': frozen.SEEDS, 'closed_executable': args.closed_executable,
                'incremental_o2': True, 'planned_requests': len(cases) * args.blocks * 10,
                'arms': {'candidate': {'launcher': launcher}}, 'rustc': reference['compiler'],
                'rust_reference': reference['_reference_identity'], 'rust_reference_contents': reference,
                'tools': {name: frozen.identity(HERE / name) for name in TOOLS}, 'invocation': sys.argv,
                'sources': {}, 'scope': __doc__,
                'timing_boundary': 'Full compiler CLI only; generation, identity checks, oracle and native execution outside timer.',
                'order': 'Six alternating AB/BA complete five-request sequences; independent per-case/block/arm caches.'}
    frozen.save(out / 'manifest.json', manifest)
    for name in TOOLS:
        shutil.copy2(HERE / name, out / name)
    return cases, manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--production', type=Path, required=True)
    parser.add_argument('--binding', type=Path, required=True)
    parser.add_argument('--rust-reference', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--blocks', type=int, required=True)
    parser.add_argument('--closed-executable', action='store_true')
    parser.add_argument('--timeout', type=int, default=180)
    parser.add_argument('--control-only', action='store_true')
    parser.add_argument('--case', action='append', choices=[f'{n}-{flavor}' for n, flavor in frozen.CASES])
    args = parser.parse_args()
    if args.blocks <= 0 or args.timeout <= 0 or (args.blocks != 6 and not args.control_only):
        parser.error('formal windows require six blocks; positive block/timeout required')
    if args.case and not args.control_only:
        parser.error('formal production opaque windows require all six families')
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    frozen.save(out / 'manifest.json', {'schema': 1, 'kind': 'production-quality-opaque-cli-v5', 'status': 'Preparing',
                'invocation': sys.argv, 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat()})
    try:
        cases, manifest = prepare(args, out)
    except BaseException as error:
        cases = [f'{n}-{flavor}' for n, flavor in frozen.CASES if not args.case or f'{n}-{flavor}' in args.case]
        failed = {'schema': 1, 'kind': 'production-quality-opaque-cli-v5', 'status': 'PreparationFailed',
                  'invocation': sys.argv, 'error': repr(error), 'observed_requests': 0,
                  'planned_requests': len(cases) * args.blocks * 10,
                  'finished_utc': datetime.datetime.now(datetime.timezone.utc).isoformat()}
        frozen.save(out / 'manifest.json', failed)
        frozen.save(out / 'not-run.json', [{'case': case, 'block': block, 'arm': arm, 'request': request,
                     'status': 'NotRun', 'reason': failed['error']} for case in cases for block in range(args.blocks)
                     for arm in ['candidate', 'rust'] for request, _ in REQUESTS])
        frozen.save(out / 'attempted-incomplete.json', [])
        raise
    rows = []
    try:
        manifest['status'] = 'Running'
        frozen.save(out / 'manifest.json', manifest)
        for n, flavor in cases:
            case = f'{n}-{flavor}'
            source_root = out / ('source-' + case)
            source_root.mkdir()
            variants, oracles = {}, {}
            for edit in [0, 1, 2]:
                folder = out / ('input-' + case + '-' + str(edit))
                frozen.generate_variant(folder, n, flavor, edit)
                variants[edit], oracles[edit] = folder, frozen.expected_values(n, flavor, edit)
                manifest['sources'][case + '/' + str(edit)] = {
                    'files': {path.name: frozen.identity(path) for path in sorted(folder.iterdir())},
                    'expected_by_seed': oracles[edit],
                    'metrics': {path.name: {'bytes': path.stat().st_size, 'lines': len(path.read_text().splitlines())}
                                for path in sorted(folder.iterdir())}}
            frozen.save(out / 'manifest.json', manifest)
            for block in range(args.blocks):
                for position, arm in enumerate(['candidate', 'rust'] if block % 2 == 0 else ['rust', 'candidate']):
                    command, run, source, exe = command_for(manifest, out, case, block, position, arm)
                    run.mkdir()
                    history = True
                    for request, edit in REQUESTS:
                        for extension in ['zil', 'rs']:
                            shutil.copyfile(variants[edit] / ('main.' + extension), source_root / ('main.' + extension))
                        source_identity = frozen.identity(source)
                        if source_identity['sha256'] != manifest['sources'][case + '/' + str(edit)]['files'][source.name]['sha256']:
                            raise ValueError('source copy differs')
                        exe.unlink(missing_ok=True)
                        row = {'case': case, 'block': block, 'position': position, 'arm': arm,
                               'request': request, 'edit': edit, 'command': command, 'source': source_identity}
                        try:
                            with frozen.isolated_process_environment():
                                measured = frozen.measured(command, run, request, args.timeout)
                        except BaseException:
                            interrupted = run / (request + '.interrupted.json')
                            if interrupted.is_file():
                                row.update(json.loads(interrupted.read_text()), valid=False, execution_valid=False,
                                           sequence_history_valid=history)
                                rows.append(row)
                                with (out / 'rows.jsonl').open('a') as stream:
                                    stream.write(json.dumps(row) + '\n')
                            raise
                        row.update(measured)
                        valid = measured['exit'] == 0 and not measured['timeout'] and exe.is_file()
                        if arm == 'candidate':
                            try:
                                report = json.loads((run / (request + '.stdout')).read_text())
                                row['status'], row['verification_runs'] = report.get('status'), report.get('verification_runs')
                                row['native_codegen'] = validate_report(report)
                            except (OSError, ValueError, KeyError, TypeError) as error:
                                valid = False
                                row['report_error'] = repr(error)
                        if valid:
                            row['executable'] = frozen.identity(exe)
                            row['native'] = [frozen.native_check(exe, run / f'{request}-seed-{seed}', (seed, oracles[edit][seed]))
                                             for seed in frozen.SEEDS]
                            valid = all(item['valid'] for item in row['native'])
                        row.update(execution_valid=bool(valid), sequence_history_valid=history, valid=bool(valid) and history)
                        history = row['valid']
                        rows.append(row)
                        with (out / 'rows.jsonl').open('a') as stream:
                            stream.write(json.dumps(row) + '\n')
                        if not row['valid']:
                            print('INVALID', case, block, arm, request, flush=True)
                print(case, 'block', block + 1, '/', args.blocks, flush=True)
        manifest['status'] = 'Completed'
    except BaseException as error:
        manifest.update(status='FailedOrInterrupted', error=repr(error))
        raise
    finally:
        manifest['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        ledger = request_ledger(out, manifest['cases'], args.blocks, 'candidate', REQUESTS,
                                manifest.get('error', 'request not reached'))
        frozen.save(out / 'not-run.json', ledger.pop('not_run_records'))
        frozen.save(out / 'attempted-incomplete.json', ledger.pop('attempted_incomplete_records'))
        manifest.update(ledger)
        frozen.save(out / 'manifest.json', manifest)
        frozen.save(out / 'summary.json', frozen.summarize(rows, manifest['cases'], args.blocks, 'rust'))
    print(json.dumps({'requests': len(rows), 'invalid': sum(not row['valid'] for row in rows)}))


if __name__ == '__main__':
    main()
