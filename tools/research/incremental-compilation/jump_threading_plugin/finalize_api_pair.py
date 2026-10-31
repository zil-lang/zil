#!/usr/bin/env python3
"""Freeze two built private API compilers and create explicit per-arm launchers."""
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
    parser.add_argument('--research-root', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    root = args.research_root.resolve()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    receipt = {'schema': 1, 'status': 'Building', 'scope': __doc__,
               'producer': identity(Path(__file__)), 'arms': [],
               'boundary': 'Loader help probes establish process library selection, not semantic correctness. Separate author and independent semantic/cache controls remain required. Timings are not performance evidence.'}
    target = out / 'api-pair-receipt.json'
    try:
        for arm in ['original', 'entry']:
            snapshot = root / ('stage6/jump-threading-api-' + arm + '-snapshot')
            source_path = snapshot / 'RESEARCH.json'
            source = json.loads(source_path.read_text())
            assert source['arm'] == arm and source['production_mutated'] is False
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
            assert toolchain['status'] == 'Pass' and toolchain['arm'] == arm
            library = Path(toolchain['library']['path']).resolve(strict=True)
            assert sha(library) == toolchain['library']['sha256']
            compiler = root / ('target-stage6-api-jt-' + arm + '/release/zil')
            assert compiler.read_bytes()[:6] == b'\x7fELF\x02\x01'
            build_log = root / ('stage6/jump-threading-api-' + arm + '-build.log')
            assert 'Finished `release` profile' in build_log.read_text()
            wrapper = out / ('zil-' + arm)
            wrapper.write_text('#!/bin/sh\nexport LD_LIBRARY_PATH=' + shlex.quote(str(library.parent))
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
                'name': arm, 'language': 'zil', 'compiler': identity(compiler),
                'launcher': identity(wrapper), 'env': source['runtime_environment'],
                'source_receipt': identity(source_path), 'production_revision': source['revision'],
                'original_files_manifest_sha256': source['original_files_manifest_sha256'],
                'pipeline_identity_tag': source['pipeline_identity_tag'],
                'llvm': {'library': identity(library), 'receipt': identity(toolchain_path)},
                'build': {'log': identity(build_log), 'environment': source['build_environment'],
                          'command': ['cargo', 'build', '--offline', '--locked', '--release', '-p', 'zil-cli',
                                      '--manifest-path', str(snapshot / 'Cargo.toml')],
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
