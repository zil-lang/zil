#!/usr/bin/env python3
"""Derive a diagnostic use-group/PHI-plan epoch from a frozen demand candidate."""
import argparse
import difflib
import json
import shutil
from pathlib import Path
from prepare import once, sha


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source', required=True, type=Path)
    p.add_argument('--out', required=True, type=Path)
    a = p.parse_args()
    manifest_path = a.source / 'source-manifest.json'
    manifest = json.loads(manifest_path.read_text())
    for name, expected in manifest['files'].items():
        assert sha((a.source / name).read_bytes()) == expected
    shutil.copytree(a.source, a.out)
    path = a.out / 'bulk/JumpThreading.cpp'
    original = path.read_text()
    text = once(original, '  // Compute the shared frontier once,', '''  static unsigned long long Calls = 0, Vars = 0, Singleton = 0, Groups = 0;
  SmallPtrSet<BasicBlock *, 32> SingleUseBlocks;
  ++Calls;
  Vars += Rewrites.size();
  for (Rewrite &R : Rewrites) {
    if (R.Uses.size() != 1)
      continue;
    ++Singleton;
    Use *U = R.Uses.front();
    auto *User = cast<Instruction>(U->getUser());
    SingleUseBlocks.insert(isa<PHINode>(User) ?
        cast<PHINode>(User)->getIncomingBlock(*U) : User->getParent());
  }
  Groups += SingleUseBlocks.size();
  if ((Calls & 63) == 1)
    errs() << "[ssa-use-groups] calls=" << Calls << " vars=" << Vars
           << " singleton=" << Singleton << " groups=" << Groups << "\\n";

  // Compute the shared frontier once,''')
    text = once(text, '      if (PlannedPhis > NumUses)\n        return false;', '''      if (PlannedPhis > NumUses) {
        if (Calls == 1) {
          errs() << "[ssa-plan] defs=" << BB->getName() << "," << NewBB->getName()
                 << " original=" << R.Original->getName() << " uses="
                 << R.Uses.size() << " needed=" << R.NeededPhis.size() << "\\n";
          for (Station S : R.Roots)
            errs() << " root=" << (S.Block ? S.Block->getName() : "poison")
                   << " phi=" << S.IsPhi << "\\n";
          for (BasicBlock *B : R.NeededPhis) {
            errs() << " phi=" << B->getName() << "\\n";
            for (BasicBlock *P : predecessors(B)) {
              Station S = FindStation(P, true);
              errs() << "  pred=" << P->getName() << " station="
                     << (S.Block ? S.Block->getName() : "poison")
                     << " phi=" << S.IsPhi << "\\n";
            }
          }
        }
        return false;
      }''')
    path.write_text(text)
    patch = ''.join(difflib.unified_diff(original.splitlines(True), text.splitlines(True), fromfile='v3/JumpThreading.cpp', tofile='v3-plan/JumpThreading.cpp'))
    (a.out / 'plan-only.patch').write_text(patch)
    manifest['candidate'] += '; diagnostic use groups and first rejected plan only'
    manifest['parent_source_manifest_sha256'] = sha(manifest_path.read_bytes())
    manifest['instrumentation_generator_sha256'] = sha(Path(__file__).read_bytes())
    manifest['files'] = {str(p.relative_to(a.out)): sha(p.read_bytes()) for p in sorted(a.out.rglob('*')) if p.is_file() and p.name != 'source-manifest.json'}
    (a.out / 'source-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')


if __name__ == '__main__':
    main()
