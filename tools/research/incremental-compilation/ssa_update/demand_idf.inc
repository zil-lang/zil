// Research: demand-selected shared IDF, only for a stable threadEdge CFG.
static bool tryBulkThreadEdgeSSA(BasicBlock *BB, BasicBlock *NewBB,
                                 ValueToValueMapTy &ValueMapping,
                                 DomTreeUpdater &DTU) {
  struct Station {
    BasicBlock *Block = nullptr;
    bool IsPhi = false;
  };
  struct Rewrite {
    Instruction *Original;
    Value *Cloned;
    SmallVector<Use *, 4> Uses;
    SmallVector<Station, 4> Roots;
    SmallVector<BasicBlock *, 4> NeededPhis;
  };
  SmallVector<Rewrite, 8> Rewrites;
  SmallVector<DbgVariableRecord *, 4> DebugValues;
  size_t NumUses = 0;
  for (Instruction &I : *BB) {
    findDbgValues(&I, DebugValues);
    if (any_of(DebugValues, [BB](const DbgVariableRecord *DVR) {
          return DVR->getParent() != BB;
        }))
      return false;
    DebugValues.clear();
    Rewrite R{&I, nullptr, {}, {}, {}};
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

  // Compute the shared frontier once, but do not create every frontier PHI.
  // A trap/return or loop header can be in that frontier without the value
  // being live there. Actual use stations drive a predecessor-PHI closure.
  DominatorTree &DT = DTU.getDomTree();
  SmallPtrSet<BasicBlock *, 2> DefBlocks{BB, NewBB};
  ForwardIDFCalculator IDF(DT);
  IDF.setDefiningBlocks(DefBlocks);
  SmallVector<BasicBlock *, 8> Frontier;
  IDF.calculate(Frontier);
  SmallPtrSet<BasicBlock *, 8> IsFrontier;
  for (BasicBlock *Block : Frontier)
    IsFrontier.insert(Block);

  // Stations distinguish a live-in PHI from the live-out definition even
  // when a defining block itself occurs in the iterated frontier (loops).
  DenseMap<BasicBlock *, Station> IncomingStation;
  auto FindStation = [&](BasicBlock *Block, bool AtEnd) -> Station {
    if (AtEnd && DefBlocks.contains(Block))
      return {Block, false};
    auto Cached = IncomingStation.find(Block);
    if (Cached != IncomingStation.end())
      return Cached->second;
    if (IsFrontier.contains(Block))
      return {Block, true};
    SmallVector<BasicBlock *, 8> Path;
    Station Result;
    while (true) {
      Path.push_back(Block);
      auto *Node = DT.getNode(Block);
      if (!Node || !Node->getIDom())
        break;
      Block = Node->getIDom()->getBlock();
      if (DefBlocks.contains(Block)) {
        Result = {Block, false};
        break;
      }
      if (IsFrontier.contains(Block)) {
        Result = {Block, true};
        break;
      }
      Cached = IncomingStation.find(Block);
      if (Cached != IncomingStation.end()) {
        Result = Cached->second;
        break;
      }
    }
    // Only stable block keys cross DenseMap growth, never bucket pointers.
    for (BasicBlock *Visited : Path)
      IncomingStation[Visited] = Result;
    return Result;
  };

  using Incoming = std::pair<BasicBlock *, Station>;
  DenseMap<BasicBlock *, SmallVector<Incoming, 4>> PhiInputs;
  size_t PlannedPhis = 0;
  for (Rewrite &R : Rewrites) {
    SmallPtrSet<BasicBlock *, 8> Needed;
    SmallVector<BasicBlock *, 8> Work;
    for (Use *U : R.Uses) {
      auto *User = cast<Instruction>(U->getUser());
      Station S;
      if (auto *PN = dyn_cast<PHINode>(User))
        S = FindStation(PN->getIncomingBlock(*U), true);
      else
        S = FindStation(User->getParent(), false);
      R.Roots.push_back(S);
      if (S.IsPhi)
        Work.push_back(S.Block);
    }
    while (!Work.empty()) {
      BasicBlock *Block = Work.pop_back_val();
      if (!Needed.insert(Block).second)
        continue;
      R.NeededPhis.push_back(Block);
      ++PlannedPhis;
      // Entire plan is still pure bookkeeping. Keep the old updater when
      // needed PHIs exceed actual uses, without partially mutating any IR.
      if (PlannedPhis > NumUses)
        return false;
      auto It = PhiInputs.find(Block);
      if (It == PhiInputs.end()) {
        SmallVector<Incoming, 4> Inputs;
        for (BasicBlock *Pred : predecessors(Block))
          Inputs.emplace_back(Pred, FindStation(Pred, true));
        It = PhiInputs.try_emplace(Block, std::move(Inputs)).first;
      }
      for (auto [Pred, S] : It->second)
        if (S.IsPhi)
          Work.push_back(S.Block);
    }
  }

  SmallVector<PHINode *, 16> NewPhis;
  for (Rewrite &R : Rewrites) {
    DenseMap<BasicBlock *, PHINode *> Phis;
    // Placeholders precede filling, so cyclic predecessor closures are safe.
    for (BasicBlock *Block : R.NeededPhis) {
      PHINode *PN = PHINode::Create(R.Original->getType(), pred_size(Block),
                                   R.Original->getName(), Block->begin());
      Phis[Block] = PN;
      NewPhis.push_back(PN);
    }
    auto Materialize = [&](Station S) -> Value * {
      if (!S.Block)
        return PoisonValue::get(R.Original->getType());
      if (S.IsPhi) {
        PHINode *PN = Phis.lookup(S.Block);
        assert(PN && "SSA station closure must contain every needed PHI");
        return PN;
      }
      assert((S.Block == BB || S.Block == NewBB) && "unknown definition");
      return S.Block == BB ? R.Original : R.Cloned;
    };
    for (BasicBlock *Block : R.NeededPhis) {
      PHINode *PN = Phis.lookup(Block);
      for (auto [Pred, S] : PhiInputs.lookup(Block))
        PN->addIncoming(Materialize(S), Pred);
    }
    for (size_t I = 0, E = R.Uses.size(); I != E; ++I)
      R.Uses[I]->set(Materialize(R.Roots[I]));
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

