// Research: a shared two-definition IDF, only for a stable threadEdge CFG.
static bool tryBulkThreadEdgeSSA(BasicBlock *BB, BasicBlock *NewBB,
                                 ValueToValueMapTy &ValueMapping,
                                 DomTreeUpdater &DTU) {
  struct Rewrite {
    Instruction *Original;
    Value *Cloned;
    SmallVector<Use *, 4> Uses;
  };
  SmallVector<Rewrite, 8> Rewrites;
  SmallVector<DbgVariableRecord *, 4> DebugValues;
  unsigned NumUses = 0;
  for (Instruction &I : *BB) {
    findDbgValues(&I, DebugValues);
    if (any_of(DebugValues, [BB](const DbgVariableRecord *DVR) {
          return DVR->getParent() != BB;
        }))
      return false;
    DebugValues.clear();
    Rewrite R{&I, nullptr, {}};
    for (Use &U : I.uses()) {
      auto *User = cast<Instruction>(U.getUser());
      if (User->isLifetimeStartOrEnd())
        return false;
      if (auto *PN = dyn_cast<PHINode>(User)) {
        if (PN->getIncomingBlock(U) == BB)
          continue;
      } else if (User->getParent() == BB) {
        continue;
      }
      R.Uses.push_back(&U);
    }
    if (R.Uses.empty())
      continue;
    R.Cloned = ValueMapping[&I];
    NumUses += R.Uses.size();
    Rewrites.push_back(std::move(R));
  }
  if (Rewrites.empty())
    return true;

  // Both definitions are identical across the batch. A full frontier avoids
  // rebuilding the backwards live region for each instruction and each use.
  DominatorTree &DT = DTU.getDomTree();
  SmallPtrSet<BasicBlock *, 2> DefBlocks{BB, NewBB};
  ForwardIDFCalculator IDF(DT);
  IDF.setDefiningBlocks(DefBlocks);
  SmallVector<BasicBlock *, 8> Frontier;
  IDF.calculate(Frontier);
  // Guard before mutation: speculative PHIs cannot outnumber actual uses.
  if (Frontier.size() > NumUses / Rewrites.size())
    return false;

  SmallVector<PHINode *, 16> NewPhis;
  for (Rewrite &R : Rewrites) {
    DenseMap<BasicBlock *, Value *> LiveIn;
    for (BasicBlock *Block : Frontier) {
      PHINode *PN = PHINode::Create(R.Original->getType(), pred_size(Block),
                                   R.Original->getName(), Block->begin());
      LiveIn[Block] = PN;
      NewPhis.push_back(PN);
    }

    auto LiveOutDefinition = [&](BasicBlock *Block) -> Value * {
      if (Block == BB)
        return R.Original;
      if (Block == NewBB)
        return R.Cloned;
      return nullptr;
    };
    auto ValueAt = [&](BasicBlock *Block, bool AtEnd) -> Value * {
      if (AtEnd)
        if (Value *V = LiveOutDefinition(Block))
          return V;
      if (Value *V = LiveIn.lookup(Block))
        return V;
      // Keep block keys, never pointers into a DenseMap that can reallocate.
      SmallVector<BasicBlock *, 8> Path;
      Value *V = nullptr;
      while (true) {
        Path.push_back(Block);
        auto *Node = DT.getNode(Block);
        if (!Node || !Node->getIDom())
          break;
        Block = Node->getIDom()->getBlock();
        V = LiveOutDefinition(Block);
        if (!V)
          V = LiveIn.lookup(Block);
        if (V)
          break;
      }
      if (!V)
        V = PoisonValue::get(R.Original->getType());
      for (BasicBlock *Visited : Path)
        LiveIn[Visited] = V;
      return V;
    };

    for (BasicBlock *Block : Frontier) {
      auto *PN = cast<PHINode>(LiveIn.lookup(Block));
      for (BasicBlock *Pred : predecessors(Block))
        PN->addIncoming(ValueAt(Pred, true), Pred);
    }
    for (Use *U : R.Uses) {
      auto *User = cast<Instruction>(U->getUser());
      if (auto *PN = dyn_cast<PHINode>(User))
        U->set(ValueAt(PN->getIncomingBlock(*U), true));
      else
        U->set(ValueAt(User->getParent(), false));
    }
  }

  // Same post-construction simplification/deduplication contract as the bulk
  // utility. Only inserted PHIs enter this list; originals retain their order.
  SmallDenseMap<BasicBlock *, unsigned, 8> Remaining;
  for (PHINode *PN : NewPhis) {
    if (Value *V = simplifyInstruction(PN, BB->getDataLayout())) {
      PN->replaceAllUsesWith(V);
      PN->eraseFromParent();
    } else {
      ++Remaining[PN->getParent()];
    }
  }
  for (auto [Block, Count] : Remaining)
    EliminateNewDuplicatePHINodes(Block,
                                  std::next(Block->phis().begin(), Count));
  return true;
}

