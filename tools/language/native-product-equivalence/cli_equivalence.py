#!/usr/bin/env python3
"""Check explicit --syntax-lock against identical live in-memory source inputs.

This performs only CLI stop-Verify requests, never native execution. Run before
the recorded input tree is retired; a missing input is an infrastructure error.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        while chunk := stream.read(1024 * 1024):
            value.update(chunk)
    return value.hexdigest()


def run(args):
    args.out.mkdir(parents=True, exist_ok=False)
    identity = json.loads(args.cli_identity.read_text())
    assert identity['compiler_commit'] == args.compiler_commit
    assert identity['binary_sha256'] == digest(args.cli)
    projects = sorted(p for p in args.memory_root.rglob('project-input.json')
                      if '-input-equivalence-' in p.parent.name)
    assert len(projects) == 3
    results = []
    for project in projects:
        request = json.loads(project.read_text())
        memory = json.loads((project.parent / 'memory/report.json').read_text())
        assert memory['status'] == 'Success' and memory['verification_runs'] == 1
        for module in request['modules']:
            path = Path(module['path'])
            assert path.is_file(), 'retired input tree is not a language failure: ' + str(path)
            assert digest(path) == module['source_sha256']
        out = args.out / request['case']
        out.mkdir()
        command = [str(args.cli.resolve()), 'check', request['entry'],
                   '--stop-after', 'verify', '--emit-dir', str(out / 'phases'), '--json']
        if request['syntax_lock']:
            assert digest(Path(request['syntax_lock'])) == request['syntax_lock_sha256']
            command += ['--syntax-lock', request['syntax_lock']]
        actual = subprocess.run(command, capture_output=True, timeout=60)
        (out / 'stdout').write_bytes(actual.stdout)
        (out / 'stderr').write_bytes(actual.stderr)
        (out / 'command.json').write_text(json.dumps(command, indent=2) + '\n')
        assert actual.returncode == 0, (command, actual.returncode, actual.stdout, actual.stderr)
        report = json.loads(actual.stdout)
        assert report['status'] == 'Success' and report['verification_runs'] == 1
        assert report['native_codegen'] is None and 'executable' not in report['artifacts']
        for key in ['input_fingerprint', 'source_files', 'scalar_interfaces']:
            assert report[key] == memory[key], (request['case'], key)
        results.append(dict(case=request['case'], command=command,
                            cli_exit=actual.returncode, input_fingerprint=report['input_fingerprint'],
                            explicit_lock=bool(request['syntax_lock']),
                            syntax_lock_sha256=request['syntax_lock_sha256'],
                            same_source_bytes_and_identity=True, verified_interfaces_equal=True))
    return dict(verdict='Pass CLI and memory source checks only; not native evidence',
                compiler_commit=args.compiler_commit, cli_sha256=digest(args.cli),
                cli_identity_sha256=digest(args.cli_identity), source_checks=len(results),
                native_runs=0, results=results)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--cli-identity', type=Path, required=True)
    parser.add_argument('--compiler-commit', required=True)
    parser.add_argument('--memory-root', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    receipt = run(args)
    (args.out / 'RESULT.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))
