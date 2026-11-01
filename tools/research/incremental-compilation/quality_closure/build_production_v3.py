#!/usr/bin/env python3
"""Build committed opt-in O2 scalar-leaf sources against one explicit LLVM prefix (v3)."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from stage6_toolchain_runtime import clean_environment, command_run, identity, save

SOURCE_PATHS = ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates', 'tools/llvm/version.txt']


def inventory(repo, revision):
    rows = []
    tree = subprocess.check_output(['git', 'ls-tree', '-rz', revision, '--', *SOURCE_PATHS], cwd=repo)
    entries = []
    for item in tree.split(b'\0'):
        if item:
            header, path = item.split(b'\t', 1)
            mode, kind, oid = header.decode().split()
            assert kind == 'blob' and mode in ['100644', '100755']
            entries.append((mode, oid, path.decode()))
    assert entries
    blobs = subprocess.run(['git', 'cat-file', '--batch'], cwd=repo,
        input=''.join(oid + '\n' for _, oid, _ in entries).encode(), stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, check=True).stdout
    stream = io.BytesIO(blobs)
    for mode, oid, path in entries:
        actual_oid, kind, size = stream.readline().decode().split()
        assert actual_oid == oid and kind == 'blob'
        expected = stream.read(int(size))
        assert stream.read(1) == b'\n'
        actual = (repo / path).read_bytes()
        assert actual == expected, ('source differs from requested revision', path)
        rows.append(dict(path=path, sha256=hashlib.sha256(actual).hexdigest(), bytes=len(actual), git_oid=oid, mode=mode))
    assert not stream.read()
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--revision', required=True)
    parser.add_argument('--toolchain-receipt', type=Path, required=True)
    parser.add_argument('--target', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--clang', type=Path, required=True)
    args = parser.parse_args()
    repo = args.repo.resolve()
    revision = subprocess.check_output(['git', 'rev-parse', args.revision], cwd=repo, text=True).strip()
    assert not args.target.exists(), 'use a fresh target; never overwrite a reviewed compiler'
    args.out.mkdir(parents=True, exist_ok=False)
    source_files = inventory(repo, revision)
    source_manifest = args.out / 'SOURCE.json'
    save(source_manifest, source_files)
    toolchain = json.loads(args.toolchain_receipt.read_text())
    assert toolchain['status'] == 'Pass'
    library = identity(toolchain['library']['path'])
    assert library['sha256'] == toolchain['library']['sha256']
    clang = identity(args.clang)
    protocol_source = (repo / 'crates/zil-backend-llvm/src/object_cache.rs').read_text()
    protocol = re.search(r'const ACTION_PROTOCOL: &str = "([^"]+)";', protocol_source).group(1)
    assert protocol == 'ZIL-OBJECT-ACTION-212-v3-complete-call-closures-checked-scalar-v3'
    hook_source = (repo / 'crates/zil-backend-llvm/src/build.rs').read_text()
    assert re.search(r'if options\.optimization == Optimization::O2 && options\.incremental_o2\s*\{\s*'
                     r'crate::machine_scalar::fold_known_leaf_calls\(&mut machine, lir\);\s*\}', hook_source)
    machine_transform = dict(name='checked-scalar-leaves-v3', optimization='O2', requires_incremental_o2=True,
        preflight='nonempty-plan-complete-original-model-before-mutation')
    api = (repo / 'crates/zil-llvm-api/src/lib.rs').read_text()
    pipeline = 'default<O2>|merge-functions=after|aliases=false'
    assert '"' + pipeline + '"' in api and '|research-' not in api
    environment = clean_environment()
    overrides = dict(ZIL_LLVM_LIB_DIR=str(Path(library['path']).parent),
        LD_LIBRARY_PATH=str(Path(library['path']).parent),
        RUSTFLAGS='-C link-arg=-Wl,-rpath,' + str(Path(library['path']).parent),
        CARGO_TARGET_DIR=str(args.target.resolve()), CARGO_BUILD_JOBS='2', ZIL_CLANG=clang['path'])
    environment.update(overrides)
    receipt = dict(schema=1, status='Running', scope=__doc__, producer=identity(__file__),
        helper=identity(HERE.parent / 'stage6_toolchain_runtime.py'),
        source=dict(revision=revision, root=str(repo), manifest=identity(source_manifest), source_matches_revision=True),
        llvm=dict(library=library, receipt=identity(args.toolchain_receipt), arm=toolchain['arm']),
        clang=clang, action_protocol=protocol, pipeline=pipeline, machine_transform=machine_transform)
    save(args.out / 'BUILD.json', receipt)
    try:
        command = ['cargo', 'build', '--release', '--locked', '--offline', '-p', 'zil-cli', '--features', 'llvm-api']
        previous = Path.cwd()
        try:
            os.chdir(repo)
            receipt['build'] = command_run(command, args.out / 'cargo', environment, 300)
        finally:
            os.chdir(previous)
        receipt['build'].update(cwd=str(repo), environment_overrides=overrides)
        assert receipt['build']['exit'] == 0
        assert inventory(repo, revision) == source_files
        receipt['source']['source_matches_revision_after_build'] = True
        compiler = (args.target / 'release/zil').resolve()
        receipt['compiler'] = identity(compiler)
        runtime = clean_environment()
        runtime.update(LD_LIBRARY_PATH=overrides['LD_LIBRARY_PATH'], ZIL_CLANG=clang['path'], LD_DEBUG='libs')
        probe = command_run([compiler, '--help'], args.out / 'loader', runtime, 20)
        receipt['loader_probe'] = probe
        assert probe['exit'] == 0
        loaded = {str(Path(line.split('calling init:', 1)[1].strip()).resolve())
            for line in Path(probe['stderr_path']).read_text().splitlines()
            if re.match(r'\s*' + str(probe['pid']) + r':', line) and 'calling init:' in line and 'libLLVM' in line}
        assert loaded == {library['path']}
        wrapper = args.out / 'zil-quality'
        wrapper.write_text('#!/bin/sh\nexport LD_LIBRARY_PATH=' + shlex.quote(overrides['LD_LIBRARY_PATH']) +
            '\nexport ZIL_CLANG=' + shlex.quote(clang['path']) + '\nexec ' + shlex.quote(str(compiler)) + ' "$@"\n')
        wrapper.chmod(0o755)
        receipt.update(status='Pass', actual_loader_paths=sorted(loaded), wrapper=identity(wrapper),
            boundary='This compiler was built against the separately reconstructed four-TU LLVM prefix. The ordinary SDK installation is unchanged. Source/ELF/library/build binding and loader probe are verified; runtime and compile-quality controls remain separate.')
    except BaseException as error:
        receipt.update(status='Fail', error=repr(error))
        raise
    finally:
        save(args.out / 'BUILD.json', receipt)


if __name__ == '__main__':
    main()
