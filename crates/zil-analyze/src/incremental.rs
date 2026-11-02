//! Content queries for the function-local initialization semantics. No cached
//! value includes a diagnostic, source position, module fingerprint or authority.
use super::*;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnalysisStats {
    pub reused_functions: usize,
    pub executed_functions: usize,
    pub retained_queries: usize,
}

/// Retains only the distinct initialization inputs used by the most recent
/// successful analysis. All other analyses and the consumer's Verify stay fresh.
#[derive(Clone, Default)]
pub struct AnalysisSession {
    entries: Arc<BTreeMap<Input, Arc<Entry>>>,
    stats: AnalysisStats,
}

impl AnalysisSession {
    pub fn clear(&mut self) {
        self.entries = Arc::default();
        self.stats = AnalysisStats::default();
    }

    pub fn last_stats(&self) -> AnalysisStats {
        self.stats
    }

    pub fn analyze(
        &mut self,
        module: &TypedModule,
        limits: &Limits,
    ) -> CompileResult<CandidatePlan> {
        let mut next = BTreeMap::new();
        let mut stats = AnalysisStats::default();
        let mut reuse = Reuse {
            prior: &self.entries,
            next: &mut next,
            stats: &mut stats,
        };
        let result = analyze_budgeted(module, limits, &mut Budget::new(limits), Some(&mut reuse));
        // A later resource/effect error must not publish partially computed
        // initialization queries. The previous complete epoch remains intact.
        if result.is_ok() {
            self.entries = Arc::new(next);
        }
        stats.retained_queries = self.entries.len();
        self.stats = stats;
        result
    }
}

pub(super) struct Reuse<'a> {
    prior: &'a BTreeMap<Input, Arc<Entry>>,
    next: &'a mut BTreeMap<Input, Arc<Entry>>,
    stats: &'a mut AnalysisStats,
}

impl Reuse<'_> {
    pub(super) fn initialize(
        &mut self,
        function: &TypedFunction,
        budget: &mut Budget,
    ) -> CompileResult<Initialization> {
        // Whole-module preflight has already bounded the raw input before this
        // projection can allocate or compare anything.
        let input = Input::of(function);
        if let Some(entry) = self.next.get(&input).or_else(|| self.prior.get(&input))
            && entry.cost <= budget.remaining()
        {
            budget.spend(entry.cost, "cached initialization")?;
            let result = entry.result.clone();
            let entry = entry.clone();
            self.next.entry(input).or_insert(entry);
            self.stats.reused_functions += 1;
            return Ok(result);
        }
        // If the old success does not fit today's balance, execute the original
        // algorithm so its exact first budget context and current span survive.
        let before = budget.remaining();
        self.stats.executed_functions += 1;
        let result = initialize(function, budget)?;
        self.next.insert(
            input,
            Arc::new(Entry {
                cost: before - budget.remaining(),
                result: result.clone(),
            }),
        );
        Ok(result)
    }
}

struct Entry {
    cost: usize,
    result: Initialization,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Input {
    locals: usize,
    parameters: Vec<LocalId>,
    entry: BlockId,
    blocks: Vec<Block>,
}

impl Input {
    fn of(function: &TypedFunction) -> Self {
        Self {
            locals: function.locals.len(),
            parameters: function.parameters.clone(),
            entry: function.entry,
            blocks: function
                .blocks
                .iter()
                .map(|block| Block {
                    id: block.id,
                    instructions: block
                        .instructions
                        .iter()
                        .map(|i| Operation::of(&i.kind))
                        .collect(),
                    exit: match block.terminator {
                        Terminator::Jump(target) => Exit::Jump(target),
                        Terminator::Branch {
                            then_block,
                            else_block,
                            ..
                        } => Exit::Branch(then_block, else_block),
                        _ => Exit::Terminal,
                    },
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Block {
    id: BlockId,
    instructions: Vec<Operation>,
    exit: Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Exit {
    Terminal,
    Jump(BlockId),
    Branch(BlockId, BlockId),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Operation {
    Other,
    Read(LocalId),
    Store(LocalId),
    Take(LocalId),
    Kill(LocalId),
    Unpack {
        source: LocalId,
        fields: Vec<LocalId>,
    },
}

impl Operation {
    pub(super) fn of(instruction: &InstructionKind) -> Self {
        match instruction {
            InstructionKind::UnpackVariant { local, fields, .. } => Self::Unpack {
                source: *local,
                fields: fields.clone(),
            },
            _ => Self::single(instruction),
        }
    }
    fn single(instruction: &InstructionKind) -> Self {
        match *instruction {
            InstructionKind::VariantTagLocal { local, .. }
            | InstructionKind::ReadVariantLocal { local, .. }
            | InstructionKind::Load(local)
            | InstructionKind::RegisterDefer { local, .. }
            | InstructionKind::BorrowLocal { local }
            | InstructionKind::BorrowLocalMut { local }
            | InstructionKind::DescribeLocal { local }
            | InstructionKind::ReadPackageField { local, .. }
            | InstructionKind::DescribePackageField { local, .. }
            | InstructionKind::TakePackageField { local, .. }
            | InstructionKind::DropPackageField { local, .. }
            | InstructionKind::StorePackageField { local, .. }
            | InstructionKind::ProjectOwnedArrayField { local, .. }
            | InstructionKind::TakeOwnedArrayElement { local, .. }
            | InstructionKind::DropOwnedArrayElement { local, .. }
            | InstructionKind::StoreOwnedArrayElement { local, .. }
            | InstructionKind::BorrowOwnedArrayElement { local, .. }
            | InstructionKind::BorrowPackageField { local, .. }
            | InstructionKind::CallClosureMut { callee: local, .. }
            | InstructionKind::CallClosureShared { callee: local, .. } => Self::Read(local),
            InstructionKind::TakeLocal { local } => Self::Take(local),
            InstructionKind::Store { local, .. }
            | InstructionKind::BindPackageCapture { local, .. } => Self::Store(local),
            InstructionKind::StorageLive(local)
            | InstructionKind::StorageDead(local)
            | InstructionKind::DropManaged { local }
            | InstructionKind::DropErrorUnion { local }
            | InstructionKind::DropEnum { local }
            | InstructionKind::DropPackage { local }
            | InstructionKind::DropOwnedArray { local }
            | InstructionKind::EndPackageCapture { local, .. }
            | InstructionKind::DropClosure { local } => Self::Kill(local),
            _ => Self::Other,
        }
    }
}
