//! Initialization and complete function candidate queries. No cached value
//! carries a diagnostic or Verify authority.
use super::*;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnalysisStats {
    pub reused_functions: usize,
    pub executed_functions: usize,
    pub retained_queries: usize,
    pub candidate_reused_functions: usize,
    pub candidate_executed_functions: usize,
    pub equal_function_results: usize,
    pub changed_function_results: usize,
    pub shared_program_reuses: usize,
    pub initialization_inputs_built: usize,
    pub retained_candidate_queries: usize,
    pub candidate_inputs_built: usize,
    pub dependency_signatures_checked: usize,
    pub resource_transfers_executed: usize,
    pub resource_transfers_reused: usize,
    pub product_admission_functions_checked: usize,
    pub product_function_queries_executed: usize,
    pub product_function_queries_reused: usize,
    pub product_global_obligations_executed: usize,
    pub product_global_obligations_reused: usize,
    pub product_plan_rows_replaced: usize,
}

#[derive(Clone)]
struct SharedEntry {
    program: Arc<TypedProgram>,
    limits: Limits,
    candidate: Arc<CandidatePlan>,
}

/// Retains the most recent successful candidate epoch. Function queries bind
/// actual signature dependencies; domains without complete local dependencies
/// retain the original whole-module checks. Verify remains independent.
#[derive(Clone, Default)]
pub struct AnalysisSession {
    entries: Arc<BTreeMap<Input, Arc<Entry>>>,
    candidate_entries: Arc<BTreeMap<FunctionId, Arc<candidate_queries::Entry>>>,
    resource_entries: resources::incremental::State,
    initialization_items: Arc<BTreeMap<FunctionId, Arc<candidate_queries::Initialized>>>,
    pub(super) function_results: Arc<Vec<AnalysisFunctionResult>>,
    shared: Option<Arc<SharedEntry>>,
    pub(super) stats: AnalysisStats,
    pub(super) product: Option<crate::product::Epoch>,
    pub(super) product_complete: Option<Arc<crate::product::Complete>>,
    pub(super) product_capture: Option<crate::product::Recording>,
    pub(super) product_changes: Vec<AnalysisFunctionResult>,
}

impl AnalysisSession {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn last_stats(&self) -> AnalysisStats {
        self.stats
    }

    pub fn last_function_results(&self) -> &[AnalysisFunctionResult] {
        &self.function_results
    }

    /// Reuse the complete raw candidate only for the same retained immutable
    /// program and exact limits. This includes program interfaces and all
    /// resource/generic checks; no Verified capability is cached or created.
    pub fn analyze_shared_program(
        &mut self,
        program: &Arc<TypedProgram>,
        limits: &Limits,
    ) -> CompileResult<Arc<CandidatePlan>> {
        if let Some(shared) = &self.shared
            && Arc::ptr_eq(&shared.program, program)
            && shared.limits == *limits
        {
            self.stats = AnalysisStats {
                retained_queries: self.entries.len(),
                retained_candidate_queries: self.candidate_entries.len(),
                candidate_reused_functions: program.module.functions.len(),
                equal_function_results: program.module.functions.len(),
                shared_program_reuses: 1,
                ..AnalysisStats::default()
            };
            self.function_results = Arc::new(
                self.function_results
                    .iter()
                    .map(|result| AnalysisFunctionResult {
                        reused: true,
                        ..*result
                    })
                    .collect(),
            );
            return Ok(shared.candidate.clone());
        }
        let candidate =
            Arc::new(self.analyze_input(&program.module, Some(program), limits, true, false)?);
        self.shared = Some(Arc::new(SharedEntry {
            program: program.clone(),
            limits: limits.clone(),
            candidate: candidate.clone(),
        }));
        Ok(candidate)
    }

    pub fn analyze(
        &mut self,
        module: &TypedModule,
        limits: &Limits,
    ) -> CompileResult<CandidatePlan> {
        self.analyze_input(module, None, limits, false, false)
    }

    pub fn analyze_program(
        &mut self,
        program: &TypedProgram,
        limits: &Limits,
    ) -> CompileResult<CandidatePlan> {
        self.analyze_input(&program.module, Some(program), limits, false, false)
    }

    pub(super) fn analyze_input(
        &mut self,
        module: &TypedModule,
        program: Option<&TypedProgram>,
        limits: &Limits,
        function_queries: bool,
        structural: bool,
    ) -> CompileResult<CandidatePlan> {
        let mut next = BTreeMap::new();
        let mut stats = AnalysisStats::default();
        let mut candidates = candidate_queries::Queries::new(
            self.candidate_entries.clone(),
            self.function_results.clone(),
            self.resource_entries.clone(),
            self.initialization_items.clone(),
            limits,
        );
        let mut reuse = Reuse {
            prior: &self.entries,
            next: &mut next,
            stats: &mut stats,
            candidates: function_queries.then_some(&mut candidates),
        };
        let mut budget = Budget::new(limits);
        budget.product_mode = structural;
        let result =
            analyze_budgeted(module, limits, &mut budget, Some(&mut reuse)).and_then(|plan| {
                if let Some(program) = program {
                    tasks::interfaces(program, &mut budget)?;
                }
                Ok(plan)
            });
        // A later resource/effect error must not publish partially computed
        // initialization queries. The previous complete epoch remains intact.
        if let Ok(plan) = &result {
            if !structural {
                self.product = None;
                self.product_complete = None;
                self.product_changes.clear();
            }
            self.product_capture = structural.then(|| crate::product::Recording {
                costs: std::mem::take(&mut budget.product_costs),
                cost: limits.max_analysis_steps - budget.remaining(),
            });
            self.entries = Arc::new(next);
            if function_queries {
                (
                    self.candidate_entries,
                    self.function_results,
                    self.resource_entries,
                    self.initialization_items,
                ) = candidates.finish(module, plan, &mut stats);
            } else {
                self.candidate_entries = Arc::default();
                self.function_results = Arc::default();
                self.resource_entries = resources::incremental::State::default();
                self.initialization_items = Arc::default();
            }
            self.shared = None;
        } else if function_queries {
            candidates.record_stats(&mut stats);
        }
        stats.retained_queries = self.entries.len();
        stats.retained_candidate_queries = self.candidate_entries.len();
        self.stats = stats;
        result
    }
}

pub(super) struct Reuse<'a> {
    prior: &'a BTreeMap<Input, Arc<Entry>>,
    next: &'a mut BTreeMap<Input, Arc<Entry>>,
    stats: &'a mut AnalysisStats,
    pub(super) candidates: Option<&'a mut candidate_queries::Queries>,
}

impl Reuse<'_> {
    pub(super) fn initialize(
        &mut self,
        function: &TypedFunction,
        budget: &mut Budget,
    ) -> CompileResult<Initialization> {
        if let Some(candidates) = self.candidates.as_deref_mut()
            && let Some(result) = candidates.initialization(function, None, budget)?
        {
            self.stats.reused_functions += 1;
            return Ok(result);
        }
        // Whole-module preflight has already bounded the raw input before this
        // projection can allocate or compare anything.
        let input = Input::of(function);
        self.stats.initialization_inputs_built += 1;
        if let Some(candidates) = self.candidates.as_deref_mut()
            && let Some(result) = candidates.initialization(function, Some(&input), budget)?
        {
            self.stats.reused_functions += 1;
            return Ok(result);
        }
        if let Some(entry) = self.next.get(&input).or_else(|| self.prior.get(&input))
            && entry.cost <= budget.remaining()
        {
            budget.spend(entry.cost, "cached initialization")?;
            let result = entry.result.clone();
            let entry = entry.clone();
            if let Some(candidates) = self.candidates.as_deref_mut() {
                candidates.save_initialization(function, input.clone(), entry.clone());
            }
            self.next.entry(input).or_insert(entry);
            self.stats.reused_functions += 1;
            return Ok(result);
        }
        // If the old success does not fit today's balance, execute the original
        // algorithm so its exact first budget context and current span survive.
        let before = budget.remaining();
        self.stats.executed_functions += 1;
        let result = initialize(function, budget)?;
        let entry = Arc::new(Entry {
            cost: before - budget.remaining(),
            result: result.clone(),
        });
        if let Some(candidates) = self.candidates.as_deref_mut() {
            candidates.save_initialization(function, input.clone(), entry.clone());
        }
        self.next.insert(input, entry);
        Ok(result)
    }
}

pub(super) struct Entry {
    pub(super) cost: usize,
    pub(super) result: Initialization,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Input {
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
            InstructionKind::UnpackVariant { local, fields, .. }
            | InstructionKind::UnpackTuple { local, fields } => Self::Unpack {
                source: *local,
                fields: fields.clone(),
            },
            _ => Self::single(instruction),
        }
    }
    fn single(instruction: &InstructionKind) -> Self {
        match *instruction {
            InstructionKind::BorrowPlaceLocal { local, .. }
            | InstructionKind::PlaceTagLocal { local, .. }
            | InstructionKind::TakeTupleField { local, .. }
            | InstructionKind::TakeVariantField { local, .. }
            | InstructionKind::VariantTagLocal { local, .. }
            | InstructionKind::ReadVariantLocal { local, .. }
            | InstructionKind::BorrowVariantLocal { local, .. }
            | InstructionKind::BorrowFieldLocal { local, .. }
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
            | InstructionKind::DropTuple { local }
            | InstructionKind::DropEnum { local }
            | InstructionKind::DropPackage { local }
            | InstructionKind::DropOwnedArray { local }
            | InstructionKind::EndPackageCapture { local, .. }
            | InstructionKind::DropClosure { local } => Self::Kill(local),
            _ => Self::Other,
        }
    }
}
