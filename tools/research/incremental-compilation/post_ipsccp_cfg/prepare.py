#!/usr/bin/env python3
"""Prepare one pinned O2 pipeline scheduling change and unchanged-source control."""
import argparse
import difflib
import hashlib
import json
from pathlib import Path


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    upstream = Path(__file__).with_name('upstream')
    provenance = json.loads((upstream / 'PROVENANCE.json').read_text())
    assert provenance['commit'] == '7a941dfb2877a0b8cf30323fefa85ad019e0b0f5'
    original = (upstream / 'PassBuilderPipelines.cpp').read_bytes()
    assert sha(original) == provenance['files'][0]['sha256']
    before = original.decode()
    anchor = '  GlobalCleanupPM.addPass(PromotePass());\n  GlobalCleanupPM.addPass(InstCombinePass());\n'
    assert before.count(anchor) == 1
    change = '''  GlobalCleanupPM.addPass(PromotePass());
  // IPSCCP can leave long chains of unconditional blocks after eliminating
  // checked branches. Clean those edges before InstCombine repeatedly sinks
  // and reanalyzes single-use expressions along the chain. Retain the regular
  // cleanup below and use its existing options; other levels are unchanged.
  if (Level == OptimizationLevel::O2)
    GlobalCleanupPM.addPass(
        SimplifyCFGPass(SimplifyCFGOptions().convertSwitchRangeToICmp(true)));
  GlobalCleanupPM.addPass(InstCombinePass());
'''
    after = before.replace(anchor, change)
    patch = ''.join(difflib.unified_diff(before.splitlines(True), after.splitlines(True),
        fromfile='a/llvm/lib/Passes/PassBuilderPipelines.cpp',
        tofile='b/llvm/lib/Passes/PassBuilderPipelines.cpp'))
    (args.out / 'native.patch').write_text(patch)
    files = {}
    for arm, text in [('original', before), ('cfg', after)]:
        destination = args.out / arm / 'PassBuilderPipelines.cpp'
        destination.parent.mkdir()
        destination.write_text(text)
        files[str(destination.relative_to(args.out))] = sha(destination.read_bytes())
    manifest = {'scope': __doc__, 'upstream': provenance, 'public_headers_unchanged': True,
                'optimization_level': 'O2 only', 'original_passes_removed': 0,
                'added_pass': 'SimplifyCFGPass(SimplifyCFGOptions().convertSwitchRangeToICmp(true))',
                'insertion': 'GlobalCleanupPM after PromotePass and before InstCombinePass',
                'files': files, 'patch_sha256': sha(patch.encode()),
                'generator_sha256': sha(Path(__file__).read_bytes())}
    manifest_path = args.out / 'source-manifest.json'
    manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
    for arm in ['original', 'cfg']:
        cpp = args.out / arm / 'PassBuilderPipelines.cpp'
        recipe = {'schema': 1, 'upstream_commit': provenance['commit'],
                  'public_headers_unchanged': True, 'arm': 'post-ipsccp-' + arm,
                  'replacements': [{'component': 'lib/libLLVMPasses.a',
                     'member': 'PassBuilderPipelines.cpp.o',
                     'source': {'path': str(cpp.resolve()), 'sha256': sha(cpp.read_bytes())},
                     'source_manifest': {'path': str(manifest_path.resolve()),
                                         'sha256': sha(manifest_path.read_bytes())}}]}
        (args.out / (arm + '-recipe.json')).write_text(json.dumps(recipe, indent=2) + '\n')
    print(json.dumps({'status': 'Prepared', 'manifest': str(manifest_path),
                      'patch_sha256': manifest['patch_sha256']}))


if __name__ == '__main__':
    main()
