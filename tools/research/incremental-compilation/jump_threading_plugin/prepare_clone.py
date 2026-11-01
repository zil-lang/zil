#!/usr/bin/env python3
"""Materialize a pinned upstream clone plus an explicitly gated entry-preserving arm."""
import argparse
import difflib
import hashlib
import json
from pathlib import Path


def once(text, old, new):
    assert text.count(old) == 1, (old[:100], text.count(old))
    return text.replace(old, new)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    upstream = Path(__file__).with_name('upstream')
    provenance = json.loads((upstream/'PROVENANCE.json').read_text())
    for item in provenance['files']:
        assert hashlib.sha256((upstream/item['name']).read_bytes()).hexdigest() == item['sha256']
    original_cpp = (upstream/'JumpThreading.cpp').read_text()
    original_h = (upstream/'JumpThreading.h').read_text()
    cpp = original_cpp.replace('"llvm/Transforms/Scalar/JumpThreading.h"', '"ResearchJumpThreading.h"')
    header = original_h.replace('LLVM_TRANSFORMS_SCALAR_JUMPTHREADING_H', 'ZIL_RESEARCH_JUMPTHREADING_H')
    cpp = cpp.replace('JumpThreadingPass', 'ZilResearchJumpThreadingPass').replace('jumpthreading', 'zil_research_jumpthreading').replace('jump-threading', 'zil-research-jump-threading')
    header = header.replace('JumpThreadingPass', 'ZilResearchJumpThreadingPass').replace('jumpthreading', 'zil_research_jumpthreading')
    header = once(header, '  unsigned BBDupThreshold;', '''  bool PreserveEntry;
  uint64_t ResearchRounds = 0;
  uint64_t ResearchBlockCalls = 0;
  uint64_t ResearchOriginalEntryMerges = 0;
  uint64_t ResearchPreservedEntryMerges = 0;

  unsigned BBDupThreshold;''')
    header = once(header, 'ZilResearchJumpThreadingPass(int T = -1);', 'ZilResearchJumpThreadingPass(int T = -1, bool PreserveEntry = false);')
    cpp = once(cpp, 'ZilResearchJumpThreadingPass::ZilResearchJumpThreadingPass(int T) {', '''static cl::opt<bool> ResearchAudit("zil-jt-audit", cl::init(false), cl::Hidden,
                                   cl::desc("Print research JT transformation counters"));

ZilResearchJumpThreadingPass::ZilResearchJumpThreadingPass(int T, bool PreserveEntry_)
    : PreserveEntry(PreserveEntry_) {''')
    cpp = once(cpp, '  F = &F_;', '''  ResearchRounds = ResearchBlockCalls = ResearchOriginalEntryMerges =
      ResearchPreservedEntryMerges = 0;
  F = &F_;''')
    cpp = once(cpp, 'bool ZilResearchJumpThreadingPass::processBlock(BasicBlock *BB) {', '''bool ZilResearchJumpThreadingPass::processBlock(BasicBlock *BB) {
  ++ResearchBlockCalls;''')
    begin = cpp.index('    for (auto &BB : *F) {', cpp.index('  do {\n    Changed = false;'))
    end = cpp.index('    EverChanged |= Changed;', begin)
    original_loop = cpp[begin:end]
    fixed_loop = '''    ++ResearchRounds;
    if (PreserveEntry) {
      // Tracking handles follow BB->replaceAllUsesWith(entry), and null on
      // deletion. Unlike a range-for reference they survive a lazy DTU flush.
      // Process the surviving entry immediately instead of needing one full
      // function round for each folded checked-success edge.
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
    } else {
'''+original_loop+'''    }
'''
    cpp = cpp[:begin] + fixed_loop + cpp[end:]
    cpp = once(cpp, '  LoopHeaders.clear();\n  return EverChanged;', '''  LoopHeaders.clear();
  if (ResearchAudit && (ResearchOriginalEntryMerges || ResearchPreservedEntryMerges))
    errs() << "[zil-jt-audit] function=" << F->getName()
           << " arm=" << (PreserveEntry ? "entry" : "original")
           << " original_entry_merges=" << ResearchOriginalEntryMerges
           << " preserved_entry_merges=" << ResearchPreservedEntryMerges
           << " rounds=" << ResearchRounds
           << " block_calls=" << ResearchBlockCalls << "\\n";
  return EverChanged;''')
    anchor = '''  // If SinglePred was a loop header, BB becomes one.
  if (LoopHeaders.erase(SinglePred))'''
    replacement = '''  if (PreserveEntry && SinglePred == &F->getEntryBlock()) {
    // All original eligibility checks above still apply. The opposite merge
    // orientation retains entry identity and avoids unconditional full DT
    // recalculation. Invalidate both sides before moving their instructions.
    LVI->eraseBlock(SinglePred);
    LVI->eraseBlock(BB);
    // Remove an AssertingVH set key before the utility RAUWs BB to entry;
    // mutating a hashed key in place would corrupt LoopHeaders bookkeeping.
    bool WasLoopHeader = LoopHeaders.erase(BB);
    if (MergeBlockIntoPredecessor(BB, DTU.get())) {
      if (WasLoopHeader)
        LoopHeaders.insert(SinglePred);
      ++ResearchPreservedEntryMerges;
      // Current work handle now tracks SinglePred. Do not access emptied BB.
      return true;
    }
    // The utility's stricter address/PHI/terminator checks can refuse. They
    // have not mutated IR at that point; preserve original merge fallback.
    if (WasLoopHeader)
      LoopHeaders.insert(BB);
  }
  if (SinglePred == &F->getEntryBlock())
    ++ResearchOriginalEntryMerges;

  // If SinglePred was a loop header, BB becomes one.
  if (LoopHeaders.erase(SinglePred))'''
    cpp = once(cpp, anchor, replacement)
    cpp += '''
// Research registration. All LLVM source above retains its upstream license.
#include "llvm/Passes/PassBuilder.h"
#include "llvm/Plugins/PassPlugin.h"
extern "C" LLVM_ATTRIBUTE_WEAK PassPluginLibraryInfo llvmGetPassPluginInfo() {
  return {LLVM_PLUGIN_API_VERSION, "zil-jump-threading-research", "23.1.4",
          [](PassBuilder &PB) {
            PB.registerPipelineParsingCallback(
                [](StringRef Name, FunctionPassManager &FPM,
                   ArrayRef<PassBuilder::PipelineElement>) {
                  if (Name != "zil-jt-original" && Name != "zil-jt-entry")
                    return false;
                  FPM.addPass(ZilResearchJumpThreadingPass(-1, Name == "zil-jt-entry"));
                  return true;
                });
          }};
}
'''
    (args.out/'ResearchJumpThreading.h').write_text(header)
    (args.out/'ResearchJumpThreading.cpp').write_text(cpp)
    patch = ''.join(difflib.unified_diff(original_h.splitlines(True), header.splitlines(True), fromfile='upstream/JumpThreading.h', tofile='ResearchJumpThreading.h'))
    patch += ''.join(difflib.unified_diff(original_cpp.splitlines(True), cpp.splitlines(True), fromfile='upstream/JumpThreading.cpp', tofile='ResearchJumpThreading.cpp'))
    (args.out/'research.patch').write_text(patch)
    manifest = {'scope': __doc__, 'upstream': provenance,
                'generator_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                'patch_sha256': hashlib.sha256(patch.encode()).hexdigest(),
                'files': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in args.out.iterdir()}}
    (args.out/'source-manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    print(json.dumps(manifest, indent=2))


if __name__ == '__main__':
    main()
