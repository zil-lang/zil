#!/usr/bin/env python3
"""Prepare a narrow SSAUpdaterBulk candidate on the pinned upstream LLVM source."""
import argparse
import difflib
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent

BULK_HELPER = '''// Research: only call with a consistent CFG and queued DT edge updates.
// Other updateSSA callers can temporarily have two terminators / an old DT.
static bool tryBulkThreadEdgeSSA(BasicBlock *BB, BasicBlock *NewBB,
                                 ValueToValueMapTy &ValueMapping,
                                 DomTreeUpdater &DTU) {
  // Preserve the exact original debug and lifetime cleanup paths. Do this
  // before any rewrite, so a fallback cannot observe a partially changed batch.
  SmallVector<DbgVariableRecord *, 4> DebugValues;
  for (Instruction &I : *BB) {
    findDbgValues(&I, DebugValues);
    if (any_of(DebugValues, [BB](const DbgVariableRecord *DVR) {
          return DVR->getParent() != BB;
        }))
      return false;
    DebugValues.clear();
    if (any_of(I.users(), [](User *U) {
          return cast<Instruction>(U)->isLifetimeStartOrEnd();
        }))
      return false;
  }

  SSAUpdaterBulk Bulk;
  unsigned Variables = 0;
  SmallVector<Use *, 16> Uses;
  for (Instruction &I : *BB) {
    for (Use &U : I.uses()) {
      auto *User = cast<Instruction>(U.getUser());
      if (auto *PN = dyn_cast<PHINode>(User)) {
        if (PN->getIncomingBlock(U) == BB)
          continue;
      } else if (User->getParent() == BB) {
        continue;
      }
      Uses.push_back(&U);
    }
    if (Uses.empty())
      continue;
    unsigned Var = Bulk.AddVariable(I.getName(), I.getType());
    ++Variables;
    Bulk.AddAvailableValue(Var, BB, &I);
    Bulk.AddAvailableValue(Var, NewBB, ValueMapping[&I]);
    for (Use *U : Uses)
      Bulk.AddUse(Var, U);
    Uses.clear();
  }
  if (Variables)
    Bulk.RewriteAndOptimizeAllUses(DTU.getDomTree());
  return true;
}

'''

def sha(data):
    return hashlib.sha256(data).hexdigest()

def once(text, old, new):
    assert text.count(old) == 1, (old[:100], text.count(old))
    return text.replace(old, new)

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
    cpp = once(original, '#include "llvm/Transforms/Utils/SSAUpdater.h"\n',
               '#include "llvm/Transforms/Utils/SSAUpdater.h"\n#include "llvm/Transforms/Utils/SSAUpdaterBulk.h"\n')
    anchor = '/// Update the SSA form.  NewBB contains instructions that are copied from BB.\n'
    cpp = once(cpp, anchor, BULK_HELPER + anchor)
    # This context uniquely selects threadEdge after its current CFG updates.
    old = '''  remapSourceAtoms(ValueMapping, NewBB->begin(), NewBB->end());
  updateSSA(BB, NewBB, ValueMapping);

  // At this point, the IR is fully up to date and consistent.'''
    new = '''  remapSourceAtoms(ValueMapping, NewBB->begin(), NewBB->end());
  if (!tryBulkThreadEdgeSSA(BB, NewBB, ValueMapping, *DTU))
    updateSSA(BB, NewBB, ValueMapping);

  // At this point, the IR is fully up to date and consistent.'''
    cpp = once(cpp, old, new)
    assert cpp.count('JumpThreadingPass::JumpThreadingPass(int T)') == 1
    for arm, contents in [('original', original), ('bulk', cpp)]:
        (args.out / arm).mkdir()
        (args.out / arm / 'JumpThreading.cpp').write_text(contents)
    (args.out / 'JumpThreading.h').write_bytes(header)
    patch = ''.join(difflib.unified_diff(original.splitlines(True), cpp.splitlines(True),
                    fromfile='a/llvm/lib/Transforms/Scalar/JumpThreading.cpp',
                    tofile='b/llvm/lib/Transforms/Scalar/JumpThreading.cpp'))
    (args.out / 'bulk.patch').write_text(patch)
    manifest = {'scope': __doc__, 'candidate': 'threadEdge-only SSAUpdaterBulk; no entry traversal change',
                'upstream': provenance, 'class_header_layout_unchanged': True,
                'header_sha256': sha(header), 'generator_sha256': sha(Path(__file__).read_bytes()),
                'patch_sha256': sha(patch.encode()),
                'files': {str(p.relative_to(args.out)): sha(p.read_bytes())
                          for p in sorted(args.out.rglob('*')) if p.is_file()}}
    (args.out / 'source-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps({'manifest': str(args.out / 'source-manifest.json'), 'patch_sha256': sha(patch.encode())}))

if __name__ == '__main__':
    main()
