#!/usr/bin/env python3
"""Final v5 production binding for the unchanged 14-case, 336-request real runner."""
import argparse
import datetime
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys

sys.dont_write_bytecode = True
import stage6_cli_analysis as real
from stage6_production_identity_v5 import create_binding, load_production_manifest, load_rust_reference
from stage6_production_ledger import request_ledger
from stage6_production_process import enable_subreaper, interrupt_and_reap
from stage6_toolchain_runtime import clean_environment, identity, save

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
CORPUS_REVISION = 'df04d4920b9d3ec618e660f8149063eb6019b317'
WINDOW = 'real'
KIND = 'production-quality-real-cli-v5'
FLAGS = ['--function-merging', 'after', '--backend', 'llvm-api', '--incremental-o2']
TOOLS = ['stage6_production_real_cli_v5.py', 'stage6_production_identity_v5.py',
         'stage6_production_ledger.py', 'stage6_production_process.py', 'stage6_cli_analysis.py',
         'stage6_toolchain_runtime.py', 'stage6_merge_runtime.py', 'stage6_bench.py',
         'stage6_real_pair_bench.py', 'distribution_bench.py']


def reference_binding(reference, rust, checkout):
    """Bind old raw oracles and prove the current copied corpus has identical bytes."""
    reference = reference.resolve(strict=True)
    manifest = real.read(reference / 'manifest.json')
    real.require(manifest['source'] == CORPUS_REVISION and manifest['cases'] == real.CASES
                 and manifest['modes'] == ['O2'] and manifest['blocks'] == 6
                 and manifest['stdin_hex'] == real.INPUTS and manifest['closed_executable'] is False
                 and manifest['semantic_cache'] is True
                 and manifest['zil_args'] == ['--function-merging', 'after'], 'Wrong original real corpus')
    real.require(manifest['binaries']['rust']['path'] == rust['compiler']['path']
                 and manifest['binaries']['rust']['sha256'] == rust['compiler']['sha256'],
                 'Original real corpus used another Rust compiler')
    sources = real.source_audit(reference, manifest)
    current_git = real.git_archive_hashes(checkout, ['examples/mainline'])
    cases = {}
    for case in real.CASES:
        current = REPO / 'examples/mainline' / case
        actual = {str(p.relative_to(current)): real.digest(p) for p in sorted(current.rglob('*')) if p.is_file()}
        prefix = 'examples/mainline/' + case + '/'
        committed = {name.removeprefix(prefix): value for name, value in current_git.items() if name.startswith(prefix)}
        real.require(actual == committed == sources[case]['files'], 'Current/reference corpus differs: ' + case)
        oracle_path = reference / ('oracle-' + case) / 'record.json'
        oracle = real.read(oracle_path)
        command = [rust['compiler']['path'], str(reference / ('source-' + case) / 'main.rs'),
                   '--edition=2024', '-C', 'overflow-checks=yes', '-C', 'panic=abort',
                   '-C', 'opt-level=0', '-o', str(reference / ('oracle-' + case) / 'program')]
        real.require(oracle['command'] == command and oracle['exit'] == 0 and oracle['timeout'] is False
                     and oracle.get('launch_error') is None and oracle['oracle_valid'] is True
                     and oracle['native_arguments'] == (['--check'] if case == 'app' else [])
                     and real.native_valid(oracle['native']), 'Invalid original raw native oracle: ' + case)
        cases[case] = {'sources': sources[case],
                       'source_manifest': identity(reference / (case + '-sources.json')),
                       'oracle_record': identity(oracle_path), 'native': oracle['native']}
    return {'scope': 'Fixed old real corpus/raw outcomes only; no old compiler timing reused.',
            'root': str(reference), 'manifest': identity(reference / 'manifest.json'),
            'corpus_revision': CORPUS_REVISION, 'checkout_revision': checkout, 'cases': cases}


def completion_records(out, reason):
    ledger = request_ledger(out, real.CASES, 6, 'zil', [('cold', 0), ('unchanged', 0)], reason, generated=True)
    missing = out.with_name(out.name + '-not-run.json')
    partial = out.with_name(out.name + '-attempted-incomplete.json')
    save(missing, ledger.pop('not_run_records'))
    save(partial, ledger.pop('attempted_incomplete_records'))
    return dict(ledger, not_run=identity(missing), attempted_incomplete=identity(partial))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['production', 'rust-reference', 'reference-window', 'out', 'lease']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    receipt_path = out.with_name(out.name + '-execution.json')
    binding_path = out.with_name(out.name + '-binding')
    log_path = out.with_name(out.name + '.log')
    active_path = out.with_name(out.name + '-active-lease.json')
    reference_path = out.with_name(out.name + '-real-reference.json')
    tools_path = out.with_name(out.name + '-tools')
    protected = [out, receipt_path, binding_path, log_path, active_path, reference_path, tools_path,
                 out.with_name(out.name + '-not-run.json'), out.with_name(out.name + '-attempted-incomplete.json')]
    if any(p.exists() for p in protected):
        parser.error('refusing to overwrite a prior real window or any adjacent receipt')
    out.parent.mkdir(parents=True, exist_ok=True)
    record = {'schema': 1, 'kind': KIND, 'window': WINDOW, 'status': 'Preparing', 'out': str(out),
              'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'planned_complete_cli_requests': 336,
              'scope': 'Unchanged real runner/corpus/statistics; final production quality options. Actual compiler-PID help loader probe is outside request timing, not a per-request trace.'}
    with receipt_path.open('x') as stream:
        json.dump(record, stream, indent=2)
        stream.write('\n')
    child, result = None, 1
    handlers = {number: signal.getsignal(number) for number in [signal.SIGINT, signal.SIGTERM]}

    def terminate(number, _frame):
        record['received_signal'] = number
        raise KeyboardInterrupt('real coordinator interrupted')

    for number in handlers:
        signal.signal(number, terminate)
    try:
        lease_path = args.lease.resolve(strict=True)
        lease_bytes = lease_path.read_bytes()
        lease = json.loads(lease_bytes)
        with active_path.open('xb') as stream:
            stream.write(lease_bytes)
        checkout = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip()
        record.update(source_commit_at_start=checkout, production_manifest=identity(args.production),
                      rust_reference=identity(args.rust_reference),
                      real_reference_manifest=identity(args.reference_window / 'manifest.json'),
                      exclusive_lease_path=str(lease_path), exclusive_lease_at_start=identity(active_path))
        real.require(lease.get('status') == 'Active' and lease.get('window') == WINDOW
                     and lease.get('planned_cli_requests') == 336 and lease.get('out') == str(out)
                     and lease.get('source_commit') == checkout
                     and all(lease.get(name) == record[name] for name in
                             ['production_manifest', 'rust_reference', 'real_reference_manifest']),
                     'Lease does not bind this exact production/Rust/corpus/output')
        profile = load_production_manifest(args.production)
        real.require(lease.get('production_revision') == profile['revision'], 'Wrong leased production revision')
        rust = load_rust_reference(args.rust_reference)
        reference = reference_binding(args.reference_window, rust, checkout)
        save(reference_path, reference)
        binding = create_binding(profile, binding_path)
        tools_path.mkdir()
        tool_records = {name: identity(HERE / name) for name in TOOLS}
        real.require(tool_records['stage6_real_pair_bench.py']['sha256']
                     == real.read(reference['manifest']['path'])['tool_sha256'], 'The original real runner changed')
        git_tools = real.git_archive_hashes(checkout, ['tools/research/incremental-compilation/' + name for name in TOOLS])
        for name, tool in tool_records.items():
            real.require(tool['sha256'] == git_tools['tools/research/incremental-compilation/' + name],
                         'Tool differs from committed checkout: ' + name)
            shutil.copy2(tool['path'], tools_path / name)
        runner = HERE / 'stage6_real_pair_bench.py'
        command = [sys.executable, str(runner), '--zil', binding['launcher']['path'], '--rustc', rust['compiler']['path'],
                   '--out', str(out), '--mode', 'O2', '--blocks', '6', '--semantic-cache']
        command += ['--zil-arg=' + flag for flag in FLAGS]
        record.update(status='Running', production_profile=profile, rust_reference_contents=rust,
                      rustc=rust['compiler'], real_reference=identity(reference_path),
                      binding=identity(binding_path / 'binding.json'), binding_contents=binding,
                      tools=tool_records, frozen_tools_directory=str(tools_path),
                      runner=identity(runner), command=command)
        save(receipt_path, record)
        enable_subreaper()
        record['owned_child_subreaper'] = True
        with log_path.open('x') as stream:
            child = subprocess.Popen(command, cwd=REPO, env=clean_environment(), stdin=subprocess.DEVNULL,
                                     stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
            record['runner_pid'] = child.pid
            save(receipt_path, record)
            result = child.wait()
        record['post_runner_cleanup'] = interrupt_and_reap(child, grace_seconds=0)
        record.update(exit=result, runner_exit=result, status='CompletedRunner' if result == 0 else 'FailedRunner')
        cleanup = record['post_runner_cleanup']
        if cleanup['captured'] or not cleanup['all_owned_processes_reaped'] or not cleanup['no_running_descendants']:
            record.update(status='RunnerDescendantLeak', exit=1)
            result = 1
    except BaseException as error:
        if child is not None:
            for number in handlers:
                signal.signal(number, signal.SIG_IGN)
            record['interruption_cleanup'] = interrupt_and_reap(child)
        record.update(status='InterruptedOrLaunchFailure', error=repr(error), exit=1)
        raise
    finally:
        try:
            record.update(completion_records(out, record.get('error', 'runner did not reach this request')))
            if record['status'] == 'CompletedRunner' and (record['observed_requests'] != 336
                    or record['not_run_requests'] or record['interrupted_requests']
                    or record['attempted_incomplete_requests'] or record['duplicate_request_keys']
                    or record['unexpected_request_keys'] or record['raw_row_parse_errors']):
                record.update(status='IncompleteRunner', exit=1)
                result = 1
        except BaseException as error:
            record.update(status='ReceiptCompletionFailed', completion_error=repr(error), exit=1)
            result = 1
        record['ended_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        save(receipt_path, record)
        for number, handler in handlers.items():
            signal.signal(number, handler)
    raise SystemExit(result)


if __name__ == '__main__':
    main()
