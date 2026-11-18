#!/usr/bin/env python3
"""Measure original-body visits and Linux peak RSS on a frozen callable campaign."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time


def read(path):
    return json.loads(path.read_bytes())


def run(cli, source, directory, profile):
    env = os.environ.copy()
    env.pop('ZIL_VERIFY_PROFILE_COSTS', None)
    env.pop('ZIL_VERIFY_PROFILE_SCANS', None)
    if profile:
        env['ZIL_VERIFY_PROFILE_SCANS'] = '1'
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        start = time.monotonic_ns()
        with subprocess.Popen([cli, 'check', source, '--json', '--work-profile', 'standard'],
                              cwd=directory, env=env, stdout=output, stderr=errors) as process:
            _, status, usage = os.wait4(process.pid, 0)
            process.returncode = os.waitstatus_to_exitcode(status)
            elapsed_ns = time.monotonic_ns() - start
        output.seek(0)
        errors.seek(0)
        result = json.load(output)
        logs = errors.read().decode()
        if process.returncode or result['status'] != 'Success' or result['verification_runs'] != 1:
            raise ValueError(f'{source.name}: {result.get("diagnostics")}\n{logs}')
    if not profile:
        return {'peak_rss_kib': usage.ru_maxrss, 'elapsed_ns': elapsed_ns}
    pattern = (r'^verify-original-body completed=true walks=(\d+) revisits=(\d+) '
               r'statement_and_expression_visits=(\d+) cfg_block_visits=(\d+)$')
    rows = [dict(zip(('walks', 'revisits', 'statement_and_expression_visits', 'cfg_block_visits'),
                     map(int, row))) for row in re.findall(pattern, logs, re.MULTILINE)]
    if not rows or not any(row['walks'] for row in rows):
        raise ValueError(f'{source.name}: missing completed Verify scan counters')
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--receipt', type=Path, required=True)
    parser.add_argument('--campaign', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    cli = args.cli.resolve()
    receipt = read(args.receipt)
    matrix = read(args.campaign / 'report.json')
    # wait4 can include the forked measurement process's pre-exec RSS. Stream
    # the compiler digest so that a binary-sized Python buffer cannot set the
    # same artificial RSS floor for every independently executed program.
    with cli.open('rb') as binary:
        compiler_digest = hashlib.file_digest(binary, 'sha256').hexdigest()
    if (receipt['exit_code'] or receipt['source_commit'] != matrix['candidate'] or
            compiler_digest != receipt['cli_sha256']):
        parser.error('The CLI, build receipt and campaign must identify the same candidate')
    samples = []
    with tempfile.TemporaryDirectory() as directory:
        for shape in matrix['bound']['shapes']:
            case = (shape or 'direct') + '-early'
            record = read(args.campaign / case / 'rule-result.json')
            if record['result']['status'] != 'pass':
                raise ValueError(f'{case} has no passing native control')
            data = record['result']['nativeEvidence'][0]['source'].encode()
            source = Path(directory) / 'case.zil'
            source.write_bytes(data)
            scans = run(cli, source, directory, True)
            runs = [run(cli, source, directory, False) for _ in range(3)]
            samples.append({'shape': shape, 'depth': len(shape), 'source_sha256': hashlib.sha256(data).hexdigest(),
                            'verify_invocations': scans,
                            'peak_compiler_rss_kib': [r['peak_rss_kib'] for r in runs],
                            'compiler_elapsed_ns': [r['elapsed_ns'] for r in runs]})
    with args.out.open('x', encoding='utf-8') as output:
        json.dump({'candidate': matrix['candidate'], 'cli_sha256': receipt['cli_sha256'],
                   'work_profile': 'standard', 'cases': samples}, output, indent=2)
        output.write('\n')


if __name__ == '__main__':
    main()
