#!/usr/bin/env python3
"""Freeze an explicit common-source compiler pair and create per-arm launchers."""
import argparse
import hashlib
import json
import os
import re
import shlex
import subprocess
from pathlib import Path


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def identity(path):
    return {'path': str(path.resolve()), 'sha256': sha(path)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec', type=Path, required=True,
                        help='JSON: schema=1, clang path, arms [{name,snapshot,compiler,build_log}]')
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    spec_path = args.spec.resolve(strict=True)
    spec = json.loads(spec_path.read_text())
    assert spec['schema'] == 1 and len(spec['arms']) == 2
    assert {arm['name'] for arm in spec['arms']} == {'original', 'candidate'}
    clang = Path(spec['clang']).absolute()
    assert clang.is_file() and os.access(clang, os.X_OK)
    clang_version = subprocess.check_output([str(clang), '--version'], text=True)
    assert 'clang version 23.1.4' in clang_version
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    receipt = {'schema': 1, 'status': 'Building', 'scope': __doc__,
               'producer': identity(Path(__file__)), 'spec': identity(spec_path),
               'clang': identity(clang), 'clang_version': clang_version, 'arms': [],
               'boundary': 'Loader help probes establish process library selection, not semantic correctness. Separate author and independent semantic/cache controls remain required. Timings are not performance evidence.'}
    target = out / 'api-pair-receipt.json'
    try:
        for arm_spec in spec['arms']:
            arm = arm_spec['name']
            snapshot = Path(arm_spec['snapshot']).resolve(strict=True)
            source_path = snapshot / 'RESEARCH.json'
            source = json.loads(source_path.read_text())
            assert source['production_mutated'] is False
            originals = snapshot / 'SOURCE-ORIGINALS.json'
            assert sha(originals) == source['original_files_manifest_sha256']
            for record in json.loads(originals.read_text()):
                expected = (source['patched_sha256'] if record['path'] == source['modified_file']
                            else record['sha256'])
                assert sha(snapshot / record['path']) == expected, record['path']
            assert sha(snapshot / 'RESEARCH.patch') == source['patch_sha256']
            toolchain_path = Path(source['toolchain_receipt']['path'])
            assert sha(toolchain_path) == source['toolchain_receipt']['sha256']
            toolchain = json.loads(toolchain_path.read_text())
            assert toolchain['status'] == 'Pass' and toolchain['arm'] == source['arm']
            library = Path(toolchain['library']['path']).resolve(strict=True)
            assert sha(library) == toolchain['library']['sha256']
            compiler = Path(arm_spec['compiler']).resolve(strict=True)
            assert compiler.read_bytes()[:6] == b'\x7fELF\x02\x01'
            assert source['pipeline_identity_tag'].encode() in compiler.read_bytes()
            build_log = Path(arm_spec['build_log']).resolve(strict=True)
            assert 'Finished `release` profile' in build_log.read_text()
            build_environment = dict(source['build_environment'], CARGO_TARGET_DIR=str(compiler.parent.parent))
            build_environment.update(arm_spec.get('build_environment', {}))
            for name in ['ZIL_LLVM_LIB_DIR', 'LD_LIBRARY_PATH', 'RUSTFLAGS']:
                assert build_environment[name] == source['build_environment'][name]
            build_command = arm_spec.get('build_command', [
                'cargo', 'build', '--offline', '--locked', '--release', '-p', 'zil-cli',
                '--manifest-path', str(snapshot / 'Cargo.toml')])
            wrapper = out / ('zil-' + arm)
            wrapper.write_text('#!/bin/sh\nexport ZIL_CLANG=' + shlex.quote(str(clang))
                               + '\nexport LD_LIBRARY_PATH=' + shlex.quote(str(library.parent))
                               + '\nexec ' + shlex.quote(str(compiler)) + ' "$@"\n')
            wrapper.chmod(0o755)
            env = os.environ.copy()
            assert not env.get('LD_PRELOAD') and not env.get('LD_AUDIT'), 'interposed launch unsupported'
            env.update(LD_DEBUG='libs', LD_DEBUG_OUTPUT=str(out / (arm + '-loader')))
            process = subprocess.Popen([str(wrapper), '--help'], env=env, stdin=subprocess.DEVNULL,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            stdout, stderr = process.communicate(timeout=30)
            (out / (arm + '-help.stdout')).write_bytes(stdout)
            (out / (arm + '-help.stderr')).write_bytes(stderr)
            assert process.returncode == 0
            loader_path = out / (arm + '-loader.' + str(process.pid))
            paths = re.findall(r'calling init:\s*(/\S*libLLVM\S*)', loader_path.read_text())
            loaded = {str(Path(path).resolve()) for path in paths}
            assert loaded == {str(library)}, loaded
            receipt['arms'].append({
                'name': arm, 'toolchain_arm': source['arm'], 'language': 'zil', 'compiler': identity(compiler),
                'launcher': identity(wrapper), 'env': dict(source['runtime_environment'], ZIL_CLANG=str(clang)),
                'source_receipt': identity(source_path), 'production_revision': source['revision'],
                'original_files_manifest_sha256': source['original_files_manifest_sha256'],
                'pipeline_identity_tag': source['pipeline_identity_tag'],
                'llvm': {'library': identity(library), 'receipt': identity(toolchain_path)},
                'build': {'log': identity(build_log), 'environment': build_environment,
                          'command': build_command,
                          'target_directory': str(compiler.parent.parent)},
                'loader_probe': {'command': [str(wrapper), '--help'], 'pid': process.pid,
                                 'returncode': process.returncode, 'log': identity(loader_path),
                                 'actual_library': identity(library)}})
        assert len({arm['production_revision'] for arm in receipt['arms']}) == 1
        assert len({arm['original_files_manifest_sha256'] for arm in receipt['arms']}) == 1
        assert len({arm['llvm']['library']['sha256'] for arm in receipt['arms']}) == 2
        receipt['status'] = 'Pass'
    except Exception as error:
        receipt['status'] = 'Fail'
        receipt['error'] = repr(error)
        raise
    finally:
        target.write_text(json.dumps(receipt, indent=2) + '\n')
        print(json.dumps({'status': receipt['status'], 'receipt': str(target)}))


if __name__ == '__main__':
    main()
