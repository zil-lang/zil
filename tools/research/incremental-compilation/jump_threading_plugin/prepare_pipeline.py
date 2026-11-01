#!/usr/bin/env python3
"""Expand the pinned target's O2 pipeline, replacing only whole JT pass nodes."""
import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path


def identity(path):
    path = path.resolve()
    return {'path': str(path), 'bytes': path.stat().st_size,
            'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--opt', type=Path, required=True)
    parser.add_argument('--plugin', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    source = args.out/'shape.ll'
    source.write_text('target triple = "x86_64-unknown-linux-gnu"\ndefine i32 @pipeline_shape(i32 %x) { ret i32 %x }\n')
    target_flags = ['-mtriple=x86_64-unknown-linux-gnu', '-mcpu=x86-64', '-relocation-model=pic',
                    '-enable-merge-functions=true', '-mergefunc-use-aliases=false']
    command = [str(args.opt.resolve()), '-load-pass-plugin='+str(args.plugin.resolve()),
               *target_flags, '-passes=default<O2>', '-print-pipeline-passes', '-disable-output', str(source)]
    run = subprocess.run(command, capture_output=True)
    (args.out/'printer.stdout').write_bytes(run.stdout)
    (args.out/'printer.stderr').write_bytes(run.stderr)
    assert run.returncode == 0, run.stderr.decode()
    builtin = run.stdout.decode().strip()
    assert '\n' not in builtin and builtin
    # Match pass nodes only, not arbitrary substrings or options.
    pattern = r'(?<=[,(])jump-threading(?=[,)])'
    nodes = len(re.findall(pattern, builtin))
    assert nodes > 0
    variants = {'builtin': builtin,
                'original': re.sub(pattern, 'zil-jt-original', builtin),
                'entry': re.sub(pattern, 'zil-jt-entry', builtin)}
    checks = {}
    for name, pipeline in variants.items():
        (args.out/(name+'.pipeline')).write_text(pipeline+'\n')
        cmd = [str(args.opt.resolve()), '-load-pass-plugin='+str(args.plugin.resolve()),
               *target_flags, '-verify-each', '-passes='+pipeline, '-S', str(source),
               '-o', str(args.out/(name+'.ll'))]
        check = subprocess.run(cmd, capture_output=True)
        (args.out/(name+'.stderr')).write_bytes(check.stderr)
        checks[name] = {'command': cmd, 'exit': check.returncode,
                        'pipeline_sha256': hashlib.sha256(pipeline.encode()).hexdigest()}
        assert check.returncode == 0, check.stderr.decode()
    def normalized(path):
        return '\n'.join(x for x in path.read_text().splitlines() if not x.startswith('; ModuleID ='))
    assert normalized(args.out/'builtin.ll') == normalized(args.out/'original.ll')
    receipt = {'scope': __doc__, 'opt': identity(args.opt), 'plugin': identity(args.plugin),
               'target_flags': target_flags, 'merge_policy': 'after', 'target': 'Linux/x86-64/PIC',
               'printer_command': command, 'jump_threading_nodes': nodes,
               'pipelines': variants, 'smoke_checks': checks, 'builtin_clone_smoke_equal': True,
               'tool': identity(Path(__file__)), 'source': identity(source)}
    (args.out/'manifest.json').write_text(json.dumps(receipt, indent=2)+'\n')
    print('Pipeline nodes replaced:', nodes)


if __name__ == '__main__':
    main()
