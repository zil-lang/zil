#!/usr/bin/env python3
"""Build a private quality-closure compiler with an explicit LLVM/cache identity."""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from stage6_toolchain_runtime import clean_environment, command_run, identity, save


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--target', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--clang', type=Path, required=True)
    parser.add_argument('--jobs', type=int, default=2)
    parser.add_argument('--offline', action='store_true')
    args = parser.parse_args()
    assert args.jobs in [1, 2]
    args.out.mkdir(parents=True, exist_ok=False)
    source_receipt = json.loads((args.source / 'RESEARCH.json').read_text())
    assert source_receipt['status'] == 'Prepared' and source_receipt['policy'] == 'complete-direct-internal-v1'
    original_manifest = args.source / 'SOURCE-ORIGINALS.json'
    assert identity(original_manifest)['sha256'] == source_receipt['original_files_manifest_sha256']
    modified = {item['path'] for item in source_receipt['modified_files']}
    for item in json.loads(original_manifest.read_text()):
        if item['path'] not in modified:
            assert identity(args.source / item['path'])['sha256'] == item['sha256']
    for item in source_receipt['modified_files']:
        assert identity(args.source / item['path'])['sha256'] == item['patched_sha256']
    library = Path(source_receipt['library']['path']).resolve()
    assert identity(library)['sha256'] == source_receipt['library']['sha256']
    clang = args.clang.resolve()
    environment = clean_environment()
    environment.update(source_receipt['build_environment'])
    environment.update(CARGO_TARGET_DIR=str(args.target.resolve()), CARGO_BUILD_JOBS=str(args.jobs), ZIL_CLANG=str(clang))
    command = ['cargo', 'build', '--release', '-p', 'zil-cli', '--features', 'llvm-api',
               '--manifest-path', str(args.source / 'Cargo.toml')]
    if args.offline:
        command.append('--offline')
    receipt = dict(status='Running', scope=__doc__, producer=identity(__file__),
        helper=identity(HERE.parent / 'stage6_toolchain_runtime.py'), source=identity(args.source / 'RESEARCH.json'),
        library=source_receipt['library'], clang=identity(clang),
        environment_overrides={name: environment[name] for name in ['CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS',
            'ZIL_LLVM_LIB_DIR', 'LD_LIBRARY_PATH', 'RUSTFLAGS', 'ZIL_CLANG']})
    save(args.out / 'BUILD.json', receipt)
    try:
        receipt['build'] = command_run(command, args.out / 'cargo', environment, 300)
        assert receipt['build']['exit'] == 0
        compiler = (args.target / 'release/zil').resolve()
        receipt['compiler'] = identity(compiler)
        assert source_receipt['pipeline_identity_tag'].encode() in compiler.read_bytes()
        runtime = clean_environment()
        runtime.update(LD_LIBRARY_PATH=str(library.parent), ZIL_CLANG=str(clang), LD_DEBUG='libs')
        receipt['loader_probe'] = command_run([compiler, '--help'], args.out / 'loader', runtime, 20)
        assert receipt['loader_probe']['exit'] == 0
        pid = receipt['loader_probe']['pid']
        loaded = {str(Path(line.split('calling init:', 1)[1].strip()).resolve())
            for line in Path(receipt['loader_probe']['stderr_path']).read_text().splitlines()
            if re.match(r'\s*' + str(pid) + r':', line) and 'calling init:' in line and 'libLLVM' in line}
        assert loaded == {str(library)}
        receipt['actual_loader_paths'] = sorted(loaded)
        wrapper = args.out / 'zil-quality'
        wrapper.write_text('#!/bin/sh\nexport LD_LIBRARY_PATH=' + shlex.quote(str(library.parent)) +
            '\nexport ZIL_CLANG=' + shlex.quote(str(clang)) + '\nexec ' + shlex.quote(str(compiler)) + ' "$@"\n')
        wrapper.chmod(0o755)
        receipt.update(status='Pass', wrapper=identity(wrapper),
            boundary='Successful private release build, embedded policy identity and actual compiler-PID LLVM loader check only; semantic/cache/runtime controls are separate.')
    except BaseException as error:
        receipt.update(status='Fail', error=repr(error))
        raise
    finally:
        save(args.out / 'BUILD.json', receipt)


if __name__ == '__main__':
    main()
