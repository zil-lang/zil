#!/usr/bin/env python3
"""Compose frozen entry, SSA, LVI and O2 CFG source changes into four pinned TUs."""
import argparse
import difflib
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'jump_threading_plugin'))
from prepare_native_patch import ENTRY_MERGE, FIXED_LOOP


def sha(data):
    return hashlib.sha256(data).hexdigest()


def identity(path):
    return {'path': str(path.resolve()), 'sha256': sha(path.read_bytes())}


def checked_manifest(directory):
    path = directory / 'source-manifest.json'
    manifest = json.loads(path.read_text())
    assert manifest['upstream']['commit'] == '7a941dfb2877a0b8cf30323fefa85ad019e0b0f5'
    for name, digest in manifest['files'].items():
        assert sha((directory / name).read_bytes()) == digest, name
    return manifest


def entry_patch(text):
    anchor = '#include "llvm/IR/Value.h"\n'
    assert text.count(anchor) == 1 and '#include "llvm/IR/ValueHandle.h"' not in text
    text = text.replace(anchor, anchor + '#include "llvm/IR/ValueHandle.h"\n')
    begin = text.index('    for (auto &BB : *F) {', text.index('  do {\n    Changed = false;'))
    end = text.index('    EverChanged |= Changed;', begin)
    text = text[:begin] + FIXED_LOOP + text[end:]
    anchor = '  // If SinglePred was a loop header, BB becomes one.\n'
    assert text.count(anchor) == 1
    return text.replace(anchor, ENTRY_MERGE + anchor)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['entry', 'ssa', 'lvi', 'cfg']:
        parser.add_argument('--' + name + '-source', type=Path, required=True)
    parser.add_argument('--variant', default='entry-ssa-v4-lvi-cfg')
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert args.variant == 'entry-ssa-v4-lvi-cfg', 'freeze a new tool epoch for a different composition'
    directories = {name: getattr(args, name + '_source').resolve(strict=True)
                   for name in ['entry', 'ssa', 'lvi', 'cfg']}
    manifests = {name: checked_manifest(path) for name, path in directories.items()}
    assert manifests['entry']['class_header_layout_unchanged'] is True
    assert all(manifests[name]['public_headers_unchanged'] is True for name in ['ssa', 'lvi', 'cfg'])
    original_jt = (directories['entry'] / 'original/JumpThreading.cpp').read_text()
    assert original_jt == (directories['ssa'] / 'original/JumpThreading.cpp').read_text()
    assert entry_patch(original_jt) == (directories['entry'] / 'entry/JumpThreading.cpp').read_text()
    candidate_jt = entry_patch((directories['ssa'] / 'grouped/JumpThreading.cpp').read_text())
    units = [
        ('JumpThreading.cpp', 'lib/libLLVMScalarOpts.a', original_jt, candidate_jt),
        ('SSAUpdaterBulk.cpp', 'lib/libLLVMTransformUtils.a',
         (directories['ssa']/'original/SSAUpdaterBulk.cpp').read_text(),
         (directories['ssa']/'grouped/SSAUpdaterBulk.cpp').read_text()),
        ('LazyValueInfo.cpp', 'lib/libLLVMAnalysis.a',
         (directories['lvi']/'original/LazyValueInfo.cpp').read_text(),
         (directories['lvi']/'indexed/LazyValueInfo.cpp').read_text()),
        ('PassBuilderPipelines.cpp', 'lib/libLLVMPasses.a',
         (directories['cfg']/'original/PassBuilderPipelines.cpp').read_text(),
         (directories['cfg']/'cfg/PassBuilderPipelines.cpp').read_text()),
    ]
    args.out.mkdir(parents=True, exist_ok=False)
    files = {}
    patches = []
    for filename, _, original, candidate in units:
        assert 'SPDX-License-Identifier: Apache-2.0 WITH LLVM-exception' in candidate
        for arm, contents in [('original', original), ('candidate', candidate)]:
            path = args.out / arm / filename
            path.parent.mkdir(exist_ok=True)
            path.write_text(contents)
            files[str(path.relative_to(args.out))] = sha(path.read_bytes())
        patches.append(''.join(difflib.unified_diff(original.splitlines(True), candidate.splitlines(True),
            fromfile='a/' + filename, tofile='b/' + filename)))
    patch = args.out / 'combined.patch'
    patch.write_text(''.join(patches))
    manifest = {'scope': __doc__, 'upstream': {'commit': '7a941dfb2877a0b8cf30323fefa85ad019e0b0f5'},
                'variant': args.variant, 'public_headers_unchanged': True,
                'input_manifests': {name: identity(path/'source-manifest.json') for name, path in directories.items()},
                'entry_transform_tool': identity(Path(__file__).resolve().parents[1]/'jump_threading_plugin/prepare_native_patch.py'),
                'generator': identity(Path(__file__)), 'files': files, 'patch': identity(patch)}
    manifest_path = args.out / 'source-manifest.json'
    manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
    for arm in ['original', 'candidate']:
        recipe = {'schema': 1, 'upstream_commit': manifest['upstream']['commit'],
                  'public_headers_unchanged': True,
                  'arm': 'four-tu-original' if arm == 'original' else args.variant,
                  'replacements': [{'component': component, 'member': filename+'.o',
                       'source': identity(args.out/arm/filename), 'source_manifest': identity(manifest_path)}
                       for filename, component, _, _ in units]}
        (args.out / (arm+'-recipe.json')).write_text(json.dumps(recipe, indent=2) + '\n')
    print(json.dumps({'status': 'Prepared', 'manifest': str(manifest_path), 'variant': args.variant}))


if __name__ == '__main__':
    main()
