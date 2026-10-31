#!/usr/bin/env python3
"""Run one exclusive six-block CLI window with an identity-bound LLVM pair."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def identity(path):
    path = Path(path).resolve(strict=True)
    with path.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    return {'path': str(path), 'sha256': digest}


def check(record):
    actual = identity(record['path'])
    if actual['sha256'] != record['sha256']:
        raise ValueError('frozen identity changed: ' + record['path'])
    return actual


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pair', type=Path, required=True)
    parser.add_argument('--rustc', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--window', choices=['isolation', 'default', 'closed', 'incremental', 'real'], required=True)
    args = parser.parse_args()
    pair = json.loads(args.pair.read_text())
    if pair['status'] != 'Pass' or {a['name'] for a in pair['arms']} != {'original', 'candidate'}:
        raise ValueError('requires the complete original/candidate compiler pair')
    arms = {a['name']: a for a in pair['arms']}
    if len({a['production_revision'] for a in arms.values()}) != 1:
        raise ValueError('isolation pair must share production source revision')
    if len({a['original_files_manifest_sha256'] for a in arms.values()}) != 1:
        raise ValueError('isolation pair must share original source bytes')
    if len({a['llvm']['library']['sha256'] for a in arms.values()}) != 2:
        raise ValueError('original and candidate libraries must be distinct')
    for arm in arms.values():
        for key in ['compiler', 'launcher', 'source_receipt']:
            check(arm[key])
        check(arm['llvm']['library'])
        check(arm['llvm']['receipt'])
    here = Path(__file__).resolve().parent
    repo = here.parents[2]
    script = here / ('stage6_real_pair_bench.py' if args.window == 'real' else 'stage6_bench.py')
    destination = args.out.resolve()
    receipt_path = destination.with_name(destination.name + '-execution.json')
    log_path = destination.with_name(destination.name + '.log')
    if any(p.exists() for p in [destination, receipt_path, log_path]):
        parser.error('refusing to overwrite a previous window, receipt, or log')
    destination.parent.mkdir(parents=True, exist_ok=True)
    command = [sys.executable, str(script), '--zil', arms['candidate']['launcher']['path'],
               '--rustc', str(args.rustc.resolve(strict=True)), '--out', str(destination),
               '--mode', 'O2', '--blocks', '6', '--semantic-cache',
               '--zil-arg=--function-merging', '--zil-arg=after']
    if args.window == 'isolation':
        command += ['--reference-zil', arms['original']['launcher']['path'],
                    '--reference-semantic-cache', '--reference-arg=--function-merging', '--reference-arg=after']
    elif args.window == 'closed':
        command += ['--closed-executable']
    elif args.window == 'incremental':
        command += ['--zil-arg=--incremental-o2']
    environment = os.environ.copy()
    isolated_keys = ['LD_LIBRARY_PATH', 'LD_PRELOAD', 'LD_AUDIT', 'LD_DEBUG',
                     'LD_DEBUG_OUTPUT', 'ZIL_CLANG', 'ZIL_LLVM_LIB_DIR']
    for key in isolated_keys:
        environment.pop(key, None)
    record = {'schema': 1, 'window': args.window, 'command': command,
              'cleared_inherited_environment_keys': isolated_keys,
              'planned_complete_cli_requests': 336 if args.window == 'real' else 1380,
              'pair': identity(args.pair), 'pair_contents': pair, 'rustc': identity(args.rustc),
              'runner': identity(script), 'coordinator': identity(Path(__file__)),
              'source_commit_at_start': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=repo, text=True).strip(),
              'status': 'Running', 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'scope': 'Exclusive window; no concurrent local builds, tests, profiling, or archives. Cache-cold is not OS-page-cache-cold. All refusals and unfavorable cells remain in the result.'}

    def save():
        receipt_path.write_text(json.dumps(record, indent=2) + '\n')

    save()
    try:
        with log_path.open('x') as stream:
            result = subprocess.run(command, cwd=repo, env=environment, stdin=subprocess.DEVNULL,
                                    stdout=stream, stderr=subprocess.STDOUT)
        record.update(exit=result.returncode, status='CompletedRunner' if result.returncode == 0 else 'FailedRunner')
    except BaseException as error:
        record.update(status='InterruptedOrLaunchFailure', error=repr(error))
        raise
    finally:
        record['ended_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        save()
    # Runner completion is distinct from per-cell correctness and performance.
    raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()
