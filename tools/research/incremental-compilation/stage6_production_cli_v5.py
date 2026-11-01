#!/usr/bin/env python3
"""Run one new production-quality CLI window under an already active lease."""
import argparse
import datetime
import json
import os
from pathlib import Path
import signal
import subprocess
import sys

sys.dont_write_bytecode = True
from stage6_production_identity_v5 import create_binding, load_production_manifest, load_rust_reference
from stage6_production_process import enable_subreaper, interrupt_and_reap
from stage6_production_ledger import request_ledger
from stage6_toolchain_runtime import clean_environment, identity, save

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
WINDOWS = ['generated-default', 'generated-closed', 'opaque-default', 'opaque-closed']


def completion_records(out, window, reason):
    from stage6_bench import shapes
    from stage6_opaque_bench import CASES, REQUESTS
    cases = [case['name'] for case in shapes()] if window.startswith('generated') else [f'{n}-{flavor}' for n, flavor in CASES]
    candidate = 'zil' if window.startswith('generated') else 'candidate'
    ledger = request_ledger(out, cases, 6, candidate, REQUESTS, reason, generated=window.startswith('generated'))
    path = out.with_name(out.name + '-not-run.json')
    incomplete_path = out.with_name(out.name + '-attempted-incomplete.json')
    save(path, ledger.pop('not_run_records'))
    save(incomplete_path, ledger.pop('attempted_incomplete_records'))
    return dict(ledger, not_run=identity(path), attempted_incomplete=identity(incomplete_path))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--production', type=Path, required=True)
    parser.add_argument('--rust-reference', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--lease', type=Path, required=True)
    parser.add_argument('--window', choices=WINDOWS, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    receipt_path = out.with_name(out.name + '-execution.json')
    log_path = out.with_name(out.name + '.log')
    binding_path = out.with_name(out.name + '-binding')
    active_lease_path = out.with_name(out.name + '-active-lease.json')
    if any(path.exists() for path in [out, receipt_path, log_path, binding_path, active_lease_path,
                                    out.with_name(out.name + '-not-run.json'), out.with_name(out.name + '-attempted-incomplete.json')]):
        parser.error('refusing to overwrite any prior window, binding, receipt or log')
    planned = 1380 if args.window.startswith('generated') else 360
    out.parent.mkdir(parents=True, exist_ok=True)
    record = {'schema': 1, 'kind': 'production-quality-cli-v5', 'status': 'Preparing', 'window': args.window,
              'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'out': str(out), 'production_manifest_requested': str(args.production),
              'rust_reference_requested': str(args.rust_reference), 'lease_requested': str(args.lease),
              'planned_complete_cli_requests': planned,
              'scope': 'New production-quality epoch; exact build/source/LLVM binding. Full compiler CLI timing only. Native/source/identity work outside each timer; all fallbacks and refusals retained.'}
    save(receipt_path, record)
    child = None
    previous_handlers = {number: signal.getsignal(number) for number in [signal.SIGINT, signal.SIGTERM]}

    def terminate(number, _frame):
        record['received_signal'] = number
        raise KeyboardInterrupt('coordinator received termination signal')

    signal.signal(signal.SIGTERM, terminate)
    try:
        lease_path = args.lease.resolve(strict=True)
        lease_bytes = lease_path.read_bytes()
        lease = json.loads(lease_bytes)
        active_lease_path.write_bytes(lease_bytes)
        record.update(production_manifest=identity(args.production), exclusive_lease_at_start=identity(active_lease_path),
                      exclusive_lease_path=str(lease_path),
                      rust_reference=identity(args.rust_reference), coordinator=identity(__file__),
                      identity_tool=identity(HERE / 'stage6_production_identity_v5.py'),
                      process_tool=identity(HERE / 'stage6_production_process.py'),
                      ledger_tool=identity(HERE / 'stage6_production_ledger.py'),
                      source_commit_at_start=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip())
        if (lease.get('status') != 'Active' or lease.get('planned_cli_requests') != planned
                or lease.get('window') != args.window or lease.get('out') != str(out)
                or lease.get('production_manifest') != record['production_manifest']
                or lease.get('rust_reference') != record['rust_reference']):
            raise ValueError('exclusive lease does not bind this exact window/output/production/Rust reference')
        profile = load_production_manifest(args.production)
        record['production_profile'] = profile
        if lease.get('production_revision') != profile['revision']:
            raise ValueError('lease must name the actual production compiler revision')
        if lease.get('source_commit') != record['source_commit_at_start']:
            raise ValueError('lease benchmark-tool checkout revision differs')
        binding = create_binding(profile, binding_path)
        record['binding'] = identity(binding_path / 'binding.json')
        record['binding_contents'] = binding
        reference = load_rust_reference(args.rust_reference)
        record['rust_reference_contents'] = reference
        rustc = reference['compiler']
        record['rustc'] = rustc
        closed = args.window.endswith('-closed')
        if args.window.startswith('generated'):
            runner = HERE / 'stage6_bench.py'
            command = [sys.executable, str(runner), '--zil', binding['launcher']['path'], '--rustc', rustc['path'],
                       '--out', str(out), '--mode', 'O2', '--blocks', '6', '--semantic-cache',
                       '--zil-arg=--function-merging', '--zil-arg=after', '--zil-arg=--backend',
                       '--zil-arg=llvm-api', '--zil-arg=--incremental-o2']
            if closed:
                command.append('--closed-executable')
        else:
            runner = HERE / 'stage6_production_opaque_bench_v5.py'
            command = [sys.executable, str(runner), '--production', profile['_manifest_identity']['path'],
                       '--binding', str(binding_path / 'binding.json'), '--rust-reference', reference['_reference_identity']['path'],
                       '--out', str(out), '--blocks', '6']
            if closed:
                command.append('--closed-executable')
        record.update(status='Running', command=command, runner=identity(runner))
        save(receipt_path, record)
        enable_subreaper()
        record['owned_child_subreaper'] = True
        with log_path.open('x') as log:
            child = subprocess.Popen(command, cwd=REPO, env=clean_environment(), stdin=subprocess.DEVNULL,
                                     stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            record['runner_pid'] = child.pid
            save(receipt_path, record)
            result = child.wait()
        record['post_runner_cleanup'] = interrupt_and_reap(child, grace_seconds=0)
        record.update(exit=result, runner_exit=result, status='CompletedRunner' if result == 0 else 'FailedRunner')
        if (record['post_runner_cleanup']['captured'] or not record['post_runner_cleanup']['all_owned_processes_reaped']
                or not record['post_runner_cleanup']['no_running_descendants']):
            record.update(status='RunnerDescendantLeak', exit=1)
            result = 1
    except BaseException as error:
        if child is not None:
            signal.signal(signal.SIGINT, signal.SIG_IGN)
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
            record['interruption_cleanup'] = interrupt_and_reap(child)
        record.update(status='InterruptedOrLaunchFailure', error=repr(error), exit=child.returncode if child else None)
        raise
    finally:
        try:
            record.update(completion_records(out, args.window, record.get('error', 'runner did not reach this request')))
            if record['status'] == 'CompletedRunner' and (record['observed_requests'] != planned or record['not_run_requests']
                    or record['interrupted_requests'] or record['attempted_incomplete_requests'] or record['duplicate_request_keys']
                    or record['unexpected_request_keys'] or record['raw_row_parse_errors']):
                record.update(status='IncompleteRunner', exit=1)
                result = 1
        except BaseException as error:
            record.update(completion_error=repr(error), status='ReceiptCompletionFailed', exit=1)
            result = 1
        record['ended_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        save(receipt_path, record)
        for number, handler in previous_handlers.items():
            signal.signal(number, handler)
    raise SystemExit(result)


if __name__ == '__main__':
    main()
