#!/usr/bin/env python3
"""Prepare a shared-IDF SSA repair candidate; keep the bulk experiment intact."""
import argparse
import difflib
import json
from pathlib import Path
from prepare import HERE, once, sha

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    upstream = HERE.parent / 'jump_threading_plugin/upstream'
    provenance = json.loads((upstream / 'PROVENANCE.json').read_text())
    assert provenance['commit'] == '7a941dfb2877a0b8cf30323fefa85ad019e0b0f5'
    for item in provenance['files']:
        assert sha((upstream / item['name']).read_bytes()) == item['sha256']
    original = (upstream / 'JumpThreading.cpp').read_text()
    header = (upstream / 'JumpThreading.h').read_bytes()
    helper = (HERE / 'shared_idf.inc').read_text()
    cpp = once(original, '#include "llvm/Analysis/InstructionSimplify.h"\n',
               '#include "llvm/Analysis/InstructionSimplify.h"\n#include "llvm/Analysis/IteratedDominanceFrontier.h"\n')
    cpp = once(cpp, '#include "llvm/Transforms/Utils/SSAUpdater.h"\n',
               '#include "llvm/Transforms/Utils/SSAUpdater.h"\n#include "llvm/Transforms/Utils/SSAUpdaterBulk.h"\n')
    anchor = '/// Update the SSA form.  NewBB contains instructions that are copied from BB.\n'
    cpp = once(cpp, anchor, helper + anchor)
    old = '''  remapSourceAtoms(ValueMapping, NewBB->begin(), NewBB->end());
  updateSSA(BB, NewBB, ValueMapping);

  // At this point, the IR is fully up to date and consistent.'''
    new = '''  remapSourceAtoms(ValueMapping, NewBB->begin(), NewBB->end());
  if (!tryBulkThreadEdgeSSA(BB, NewBB, ValueMapping, *DTU))
    updateSSA(BB, NewBB, ValueMapping);

  // At this point, the IR is fully up to date and consistent.'''
    cpp = once(cpp, old, new)
    for arm, contents in [('original', original), ('bulk', cpp)]:
        (args.out / arm).mkdir()
        (args.out / arm / 'JumpThreading.cpp').write_text(contents)
    (args.out / 'JumpThreading.h').write_bytes(header)
    patch = ''.join(difflib.unified_diff(original.splitlines(True), cpp.splitlines(True),
                    fromfile='a/llvm/lib/Transforms/Scalar/JumpThreading.cpp',
                    tofile='b/llvm/lib/Transforms/Scalar/JumpThreading.cpp'))
    (args.out / 'shared-idf.patch').write_text(patch)
    manifest = {'scope': __doc__, 'candidate': 'threadEdge-only shared unpruned IDF, bounded inserted PHIs; no entry traversal change',
                'upstream': provenance, 'class_header_layout_unchanged': True,
                'header_sha256': sha(header), 'generator_sha256': sha(Path(__file__).read_bytes()),
                'helper_sha256': sha(helper.encode()), 'support_script_sha256': sha((HERE / 'prepare.py').read_bytes()),
                'patch_sha256': sha(patch.encode()),
                'files': {str(p.relative_to(args.out)): sha(p.read_bytes())
                          for p in sorted(args.out.rglob('*')) if p.is_file()}}
    (args.out / 'source-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps({'manifest': str(args.out / 'source-manifest.json'), 'patch_sha256': sha(patch.encode())}))

if __name__ == '__main__':
    main()
