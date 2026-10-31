#!/usr/bin/env python3
"""Prepare one ABI-preserving LLVM23 source patch; no compilation or extraction."""
import argparse
import difflib
import hashlib
import json
from pathlib import Path


def digest(data):
    return hashlib.sha256(data).hexdigest()


def once(text, old, new):
    assert text.count(old) == 1, (old[:100], text.count(old))
    return text.replace(old, new)


FIXED_LOOP = '''    // A merge can RAUW a block to the surviving entry, and a lazy DTU
    // flush can delete a queued block. Reacquire each block through its handle.
    SmallVector<WeakTrackingVH, 64> Work;
    for (BasicBlock &Block : *F)
      Work.emplace_back(&Block);
    for (WeakTrackingVH &Current : Work) {
      auto *BB = dyn_cast_or_null<BasicBlock>(static_cast<Value *>(Current));
      if (!BB || BB->getParent() != F || Unreachable.count(BB))
        continue;
      while (true) {
        BB = dyn_cast_or_null<BasicBlock>(static_cast<Value *>(Current));
        if (!BB || BB->getParent() != F || !processBlock(BB))
          break;
        Changed = ChangedSinceLastAnalysisUpdate = true;
      }
      BB = dyn_cast_or_null<BasicBlock>(static_cast<Value *>(Current));
      if (!BB || BB->getParent() != F || BB == &F->getEntryBlock() ||
          DTU->isBBPendingDeletion(BB))
        continue;
      if (pred_empty(BB)) {
        LoopHeaders.erase(BB);
        LVI->eraseBlock(BB);
        DeleteDeadBlock(BB, DTU.get());
        Changed = ChangedSinceLastAnalysisUpdate = true;
        continue;
      }
      if (auto *BI = dyn_cast<UncondBrInst>(BB->getTerminator())) {
        BasicBlock *Succ = BI->getSuccessor();
        if (BB->getFirstNonPHIOrDbg(true)->isTerminator() &&
            !LoopHeaders.count(BB) && !LoopHeaders.count(Succ) &&
            TryToSimplifyUncondBranchFromEmptyBlock(BB, DTU.get())) {
          LVI->eraseBlock(BB);
          Changed = ChangedSinceLastAnalysisUpdate = true;
        }
      }
    }
'''

ENTRY_MERGE = '''  if (SinglePred == &F->getEntryBlock()) {
    // Retain entry identity instead of forcing a full dominator recalculation
    // for each merged block. The original eligibility checks still apply.
    LVI->eraseBlock(SinglePred);
    LVI->eraseBlock(BB);
    // Remove the hashed key before the utility RAUWs BB to the entry.
    bool WasLoopHeader = LoopHeaders.erase(BB);
    if (MergeBlockIntoPredecessor(BB, DTU.get())) {
      if (WasLoopHeader)
        LoopHeaders.insert(SinglePred);
      // The work handle now tracks SinglePred. Do not access the emptied BB.
      return true;
    }
    // Stricter utility checks can decline without changing IR. Restore the
    // bookkeeping and retain the original merge orientation as a fallback.
    if (WasLoopHeader)
      LoopHeaders.insert(BB);
  }

'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    upstream = Path(__file__).with_name('upstream')
    provenance = json.loads((upstream / 'PROVENANCE.json').read_text())
    assert provenance['commit'] == '7a941dfb2877a0b8cf30323fefa85ad019e0b0f5'
    for item in provenance['files']:
        assert digest((upstream / item['name']).read_bytes()) == item['sha256']
    original = (upstream / 'JumpThreading.cpp').read_text()
    header = (upstream / 'JumpThreading.h').read_bytes()
    cpp = once(original, '#include "llvm/IR/Value.h"\n',
               '#include "llvm/IR/Value.h"\n#include "llvm/IR/ValueHandle.h"\n')
    begin = cpp.index('    for (auto &BB : *F) {', cpp.index('  do {\n    Changed = false;'))
    end = cpp.index('    EverChanged |= Changed;', begin)
    cpp = cpp[:begin] + FIXED_LOOP + cpp[end:]
    anchor = '  // If SinglePred was a loop header, BB becomes one.\n'
    cpp = once(cpp, anchor, ENTRY_MERGE + anchor)
    for forbidden in ['PreserveEntry', 'ResearchRounds', 'ResearchBlockCalls',
                      'ResearchOriginalEntryMerges', 'ResearchPreservedEntryMerges',
                      'ZilResearchJumpThreadingPass', 'llvmGetPassPluginInfo',
                      'zil-research-jump-threading', 'zil-jt-audit']:
        assert forbidden not in cpp, forbidden
    assert '#include "llvm/Transforms/Scalar/JumpThreading.h"' in cpp
    assert cpp.count('JumpThreadingPass::JumpThreadingPass(int T)') == 1
    assert 'class JumpThreadingPass' not in cpp
    for arm, contents in [('original', original), ('entry', cpp)]:
        (args.out / arm).mkdir()
        (args.out / arm / 'JumpThreading.cpp').write_text(contents)
    # This copy is provenance only: compilation uses the matching SDK header.
    (args.out / 'JumpThreading.h').write_bytes(header)
    patch = ''.join(difflib.unified_diff(
        original.splitlines(True), cpp.splitlines(True),
        fromfile='a/llvm/lib/Transforms/Scalar/JumpThreading.cpp',
        tofile='b/llvm/lib/Transforms/Scalar/JumpThreading.cpp'))
    (args.out / 'native.patch').write_text(patch)
    manifest = {'scope': __doc__, 'upstream': provenance,
                'class_header_layout_unchanged': True,
                'header_sha256': digest(header),
                'generator_sha256': digest(Path(__file__).read_bytes()),
                'patch_sha256': digest(patch.encode()),
                'files': {str(p.relative_to(args.out)): digest(p.read_bytes())
                          for p in sorted(args.out.rglob('*')) if p.is_file()}}
    (args.out / 'source-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps({'out': str(args.out), 'source_manifest': str(args.out / 'source-manifest.json'),
                      'patch_sha256': manifest['patch_sha256']}))


if __name__ == '__main__':
    main()
