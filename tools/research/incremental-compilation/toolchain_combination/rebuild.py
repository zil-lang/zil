#!/usr/bin/env python3
"""Rebuild the pinned four-TU LLVM pair using only this repository and its SDK."""
import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


def identity(path):
    with path.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    return {'path': str(path.resolve()), 'sha256': digest}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--llvm-prefix', type=Path, required=True)
    parser.add_argument('--headers-prefix', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--jobs', type=int, choices=[1, 2], default=2)
    parser.add_argument('--build-cli', action='store_true')
    parser.add_argument('--revision', default='HEAD', help='Common source for optional private CLIs')
    parser.add_argument('--offline', action='store_true', help='Require existing Cargo dependency cache')
    args = parser.parse_args()
    repo = args.repo.resolve(strict=True)
    llvm = args.llvm_prefix.resolve(strict=True)
    headers = args.headers_prefix.resolve(strict=True)
    out = args.out.resolve()
    tools = repo / 'tools/research/incremental-compilation'
    assert Path(__file__).resolve() == tools/'toolchain_combination/rebuild.py'
    out.mkdir(parents=True, exist_ok=False)
    (out/'logs').mkdir()
    revision = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', args.revision], text=True).strip()
    receipt = {'schema': 1, 'status': 'Running', 'scope': __doc__, 'revision': revision,
               'producer': identity(Path(__file__)), 'max_jobs': args.jobs, 'commands': [],
               'existing_prefix_untouched': True, 'absolute_workspace_inputs_required': False}

    def save():
        (out/'rebuild-receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')

    def run(label, command, environment=None, overrides=None):
        record = {'label': label, 'argv': [str(x) for x in command],
                  'environment_overrides': overrides or {}}
        receipt['commands'].append(record)
        save()
        with (out/'logs'/(label+'.stdout')).open('wb') as stdout, \
                (out/'logs'/(label+'.stderr')).open('wb') as stderr:
            try:
                result = subprocess.run(record['argv'], env=environment, cwd=repo,
                                        stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr)
                record['exit'] = result.returncode
                assert result.returncode == 0, 'failed step: ' + label
            except Exception as error:
                record['error'] = repr(error)
                raise
            finally:
                save()

    def python(label, script, *arguments):
        run(label, [sys.executable, tools/script, *arguments])

    save()
    try:
        python('entry-source', 'jump_threading_plugin/prepare_native_patch.py', '--out', out/'entry-source')
        python('ssa-v1-source', 'ssa_update/prepare.py', '--out', out/'ssa-v1-source')
        python('ssa-v4-source', 'ssa_update/prepare_grouped_bulk.py',
               '--jump-threading-source', out/'ssa-v1-source', '--out', out/'ssa-v4-source')
        python('lvi-source', 'lvi_cache/prepare_patch.py', '--out', out/'lvi-source')
        python('cfg-source', 'post_ipsccp_cfg/prepare.py', '--out', out/'cfg-source')
        python('combined-source', 'toolchain_combination/prepare.py',
               '--entry-source', out/'entry-source', '--ssa-source', out/'ssa-v4-source',
               '--lvi-source', out/'lvi-source', '--cfg-source', out/'cfg-source', '--out', out/'source')
        receipt['toolchains'] = {}
        spec = {'schema': 1, 'clang': str(llvm/'bin/clang'), 'arms': []}
        for arm in ['original', 'candidate']:
            prefix = out/arm
            python(arm+'-toolchain', 'jump_threading_plugin/build_component_toolchain.py',
                   '--llvm-prefix', llvm, '--headers-prefix', headers,
                   '--recipe', out/'source'/(arm+'-recipe.json'), '--out', prefix,
                   '--jobs', str(args.jobs), '--execute')
            native = prefix/'native-toolchain-receipt.json'
            receipt['toolchains'][arm] = identity(native)
            if not args.build_cli:
                continue
            snapshot = out/(arm+'-source')
            python(arm+'-compiler-source', 'toolchain_combination/prepare_api.py',
                   '--repo', repo, '--revision', revision, '--toolchain-receipt', native, '--out', snapshot)
            source = json.loads((snapshot/'RESEARCH.json').read_text())
            overrides = dict(source['build_environment'], CARGO_BUILD_JOBS=str(args.jobs),
                             CARGO_TARGET_DIR=str(out/(arm+'-target')))
            environment = os.environ.copy()
            environment.update(overrides)
            command = ['cargo', 'build', '--locked', '--release', '-p', 'zil-cli',
                       '--manifest-path', str(snapshot/'Cargo.toml')]
            if args.offline:
                command.insert(2, '--offline')
            run(arm+'-compiler-build', command, environment, overrides)
            spec['arms'].append({'name': arm, 'snapshot': str(snapshot),
                                  'compiler': str(out/(arm+'-target/release/zil')),
                                  'build_command': command, 'build_environment': overrides,
                                  'build_log': str(out/'logs'/(arm+'-compiler-build.stderr'))})
        if args.build_cli:
            spec_path = out/'pair-spec.json'
            spec_path.write_text(json.dumps(spec, indent=2)+'\n')
            python('finalize-pair', 'toolchain_combination/finalize_pair.py',
                   '--spec', spec_path, '--out', out/'pair')
            receipt['pair'] = identity(out/'pair/api-pair-receipt.json')
        receipt['status'] = 'Pass'
    except Exception as error:
        receipt['status'] = 'Fail'
        receipt['error'] = repr(error)
        raise
    finally:
        save()
        print(json.dumps({'status': receipt['status'], 'receipt': str(out/'rebuild-receipt.json'),
                          'candidate': str(out/'candidate/lib/libLLVM.so.23.1')}))


if __name__ == '__main__':
    main()
