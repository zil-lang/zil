#!/usr/bin/env python3
"""Run the frozen helper-partition acceptance protocol and retain every outcome."""
from pathlib import Path
import argparse
import datetime
import hashlib
import json
import subprocess
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--base', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--kind', choices=['resource', 'normalized'], default='resource')
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    here = Path(__file__).resolve().parent
    protocol = (here / 'protocols/helper-current' if args.kind == 'resource'
                else here.parent / 'normalized-merging')
    repo = args.repo.resolve()
    identity = json.loads((here / 'plan.json').read_text())
    tree = subprocess.check_output(['git', '-C', repo, 'rev-parse', 'HEAD^{tree}'], text=True).strip()
    expected_tree = identity[args.kind + '_tree']
    assert tree == expected_tree, (tree, expected_tree)
    identity.update(
        comparison=args.kind,
        actual_baseline=identity['baseline'] if args.kind == 'resource' else identity['resource_remote_commit'],
        actual_tree=tree,
        binaries={name: {'path': str(path.resolve()), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
                  for name, path in [('base', args.base), ('candidate', args.candidate)]},
        phases=[],
        status='Running',
    )
    def save():
        (out / 'receipt.json').write_text(json.dumps(identity, indent=2) + '\n')
    save()
    try:
        phases = [('multi', 'multimodule.py'), ('primary', 'bench/measure.py')]
        if args.kind == 'resource':
            phases += [('multi-controls', 'multi_controls.py')]
        phases += [('controls', 'controls.py'), ('quality', 'quality.py'), ('neighbors', 'neighbors/check.py')]
        if args.kind == 'normalized':
            phases += [('holdout', '../normalized-consumers/holdout.py')]
        for name, script in phases:
            command = ['python3', str(protocol / script), '--base', str(args.base.resolve()),
                       '--candidate', str(args.candidate.resolve()), '--out', str(out / name)]
            if name in ('multi', 'primary'):
                command += ['--rounds', '5']
            phase = {'name': name, 'command': command,
                     'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat()}
            identity['phases'].append(phase)
            save()
            print(json.dumps({'starting': name}), flush=True)
            start = time.monotonic()
            with (out / (name + '.log')).open('w') as log:
                completed = subprocess.run(command, cwd=repo, stdout=log, stderr=subprocess.STDOUT)
            phase.update(exit_code=completed.returncode, seconds=time.monotonic() - start)
            save()
            print(json.dumps(phase), flush=True)
            if completed.returncode:
                raise RuntimeError(f'{name} failed; first failure retained')
            if name == 'multi':
                summary = json.loads((out / name / 'summary.json').read_text())
                assert summary['complete'] and summary['quality_complete'], summary
                if not summary['no_new_runtime_regression']:
                    raise RuntimeError('multi runtime gate failed; numerical acceptance remains blocked')
        identity['status'] = 'Measurements complete; independent numerical review pending'
    except Exception as error:
        identity.update(status='Failed or blocked', error=repr(error))
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
