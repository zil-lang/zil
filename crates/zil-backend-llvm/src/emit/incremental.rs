//! Immutable, current-input-bound scalar lowering work products. These are raw
//! machine values, never verifier capabilities. Unsupported inputs use the
//! original emitter, including its budget and diagnostic behavior.
use super::*;
use std::sync::Arc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Session {
    context: Option<Context>,
    entries: BTreeMap<FunctionId, Arc<Entry>>,
    pub(crate) hits: usize,
    pub(crate) misses: usize,
    context_identity: Arc<()>,
    pub(crate) products: Vec<(Arc<()>, Arc<()>)>,
    pub(crate) eligible: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Context {
    target: TargetProfile,
    limits: zil_base::Limits,
    entry: EntryPoint,
    scalar_policy: bool,
    headers: Vec<Header>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Header {
    id: FunctionId,
    name: String,
    foreign: Option<zil_ir::ForeignDeclaration>,
    parameters: Vec<(LocalId, Type)>,
    result: Type,
    result_mode: zil_ir::PassingMode,
    file: zil_base::FileId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    input: TypedFunction,
    facts: Vec<(FunctionId, Option<Constant>)>,
    collected: bool,
    output: Arc<FunctionBatch>,
    output_identity: Arc<()>,
    source_identity: Arc<()>,
    source: Vec<Vec<Option<FunctionId>>>,
    cost: usize,
}
fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit | Type::Bool | Type::I32 | Type::I64 | Type::U8 | Type::U16
    )
}
fn eligible(f: &TypedFunction, previous: Option<&TypedFunction>) -> bool {
    f.environment.is_none()
        && f.storage_life.is_none()
        && f.call_kind == zil_ir::CallKind::Direct
        && f.reference_return_bound.is_none()
        && f.callable_return_contract.is_none()
        && f.callable_contract.is_none()
        && f.protocol_contract.is_none()
        && scalar(&f.return_type)
        && (previous.is_some_and(|previous| previous.locals.ptr_eq(&f.locals))
            || f.locals.iter().all(|l| scalar(&l.ty)))
        && (previous.is_some_and(|previous| previous.values.ptr_eq(&f.values))
            || f.values.iter().all(scalar))
        && (previous.is_some_and(|previous| previous.blocks.ptr_eq(&f.blocks))
            || f.blocks.iter().all(|b| {
                b.instructions.iter().all(|i| match &i.kind {
                    InstructionKind::Constant(c) => scalar(&c.ty()),
                    InstructionKind::Load(_)
                    | InstructionKind::Store { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::UnsafeEnter { .. }
                    | InstructionKind::UnsafeExit { .. }
                    | InstructionKind::Call { .. }
                    | InstructionKind::ForeignCall { .. }
                    | InstructionKind::Unary { .. }
                    | InstructionKind::Binary { .. }
                    | InstructionKind::ConvertInteger { .. } => true,
                    _ => false,
                })
            }))
}
// Exhaustive destructuring prevents an added raw function field from silently
// escaping the dependency proof. Source locations only affect diagnostics;
// fresh binding/eligibility run before this successful machine-value lookup.
fn same_input(previous: &TypedFunction, current: &TypedFunction) -> bool {
    let TypedFunction {
        storage_life,
        call_kind,
        reference_return_bound,
        callable_return_contract,
        callable_contract,
        protocol_contract,
        id,
        name,
        foreign,
        environment,
        parameters,
        return_type,
        result_mode,
        locals,
        values,
        blocks,
        entry,
        span: _,
    } = current;
    previous.storage_life == *storage_life
        && previous.call_kind == *call_kind
        && previous.reference_return_bound == *reference_return_bound
        && previous.callable_return_contract == *callable_return_contract
        && previous.callable_contract == *callable_contract
        && previous.protocol_contract == *protocol_contract
        && previous.id == *id
        && previous.name == *name
        && previous.foreign == *foreign
        && previous.environment == *environment
        && previous.parameters == *parameters
        && previous.return_type == *return_type
        && previous.result_mode == *result_mode
        && previous.values == *values
        && previous.entry == *entry
        && (previous.locals.ptr_eq(locals)
            || (previous.locals.len() == locals.len()
                && previous.locals.iter().zip(locals).all(|(old, new)| {
                    let zil_ir::LocalDecl {
                        id,
                        name,
                        ty,
                        mutable,
                        span: _,
                    } = new;
                    old.id == *id && old.name == *name && old.ty == *ty && old.mutable == *mutable
                })))
        && (previous.blocks.ptr_eq(blocks)
            || (previous.blocks.len() == blocks.len()
                && previous.blocks.iter().zip(blocks).all(|(old, new)| {
                    let zil_ir::BasicBlock {
                        id,
                        instructions,
                        terminator,
                    } = new;
                    old.id == *id
                        && old.terminator == *terminator
                        && old.instructions.len() == instructions.len()
                        && old.instructions.iter().zip(instructions).all(|(old, new)| {
                            let zil_ir::Instruction {
                                result,
                                kind,
                                span: _,
                            } = new;
                            old.result == *result && old.kind == *kind
                        })
                })))
}
impl Session {
    pub(super) fn prepare(
        &mut self,
        lir: &LirView<'_>,
        verified: &VerifiedModule,
        options: &EmitOptions,
        scalar_policy: bool,
    ) -> bool {
        self.hits = 0;
        self.misses = 0;
        self.eligible = false;
        self.products.clear();
        if !lir.closures.is_empty()
            || !lir.resources.is_empty()
            || !verified.module().dyn_declarations.is_empty()
            || !verified.module().dyn_interfaces.is_empty()
            || !verified.module().dyn_vtables.is_empty()
            || !verified.module().life_declarations.is_empty()
            || !verified.module().life_instances.is_empty()
            || !verified.task_interfaces().is_empty()
            || verified.uses_scalar_tasks()
            || verified.uses_retained_registration()
            || !lir.functions.iter().all(|function| {
                eligible(
                    function,
                    self.entries.get(&function.id).map(|entry| &entry.input),
                )
            })
        {
            self.context = None;
            self.entries.clear();
            return false;
        }
        let context = Context {
            target: lir.target.clone(),
            limits: verified.limits().clone(),
            entry: options.entry,
            scalar_policy,
            headers: lir
                .functions
                .iter()
                .map(|f| Header {
                    id: f.id,
                    name: f.name.clone(),
                    foreign: f.foreign.clone(),
                    parameters: f
                        .parameters
                        .iter()
                        .map(|id| (*id, f.locals[id.0 as usize].ty.clone()))
                        .collect(),
                    result: f.return_type.clone(),
                    result_mode: f.result_mode,
                    file: f.span.file,
                })
                .collect(),
        };
        if self.context.as_ref() != Some(&context) {
            self.entries.clear();
            self.context = Some(context);
            self.context_identity = Arc::new(());
        }
        self.eligible = true;
        // A single current epoch, with no deleted-function or historical tail.
        true
    }
    pub(crate) fn identity(&self) -> Option<crate::incremental::ScalarIdentity> {
        self.eligible
            .then(|| (self.context_identity.clone(), self.products.clone()))
    }
    pub(super) fn lower<F>(
        &mut self,
        function: &TypedFunction,
        summaries: Option<&BTreeMap<FunctionId, Constant>>,
        collected: Option<&BTreeSet<FunctionId>>,
        meter: &tasks::Meter,
        lower: F,
    ) -> CompileResult<FunctionBatch>
    where
        F: FnOnce() -> CompileResult<FunctionBatch>,
    {
        // Only observed direct calls can consume a scalar result. Capturing the
        // actual fact values stops propagation when a callee recomputes equally.
        let observed = if let Some(entry) = self
            .entries
            .get(&function.id)
            .filter(|entry| entry.input.blocks.ptr_eq(&function.blocks))
        {
            entry
                .facts
                .iter()
                .map(|(id, _)| *id)
                .collect::<BTreeSet<_>>()
        } else {
            let mut observed = BTreeSet::from([function.id]);
            for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
                if let InstructionKind::Call { function, .. } = &instruction.kind {
                    observed.insert(*function);
                }
            }
            observed
        };
        let facts = observed
            .into_iter()
            .map(|id| (id, summaries.and_then(|s| s.get(&id)).cloned()))
            .collect::<Vec<_>>();
        let collected = collected.is_none_or(|needed| needed.contains(&function.id));
        if let Some(entry) = self.entries.get(&function.id)
            && same_input(&entry.input, function)
            && entry.facts == facts
            && entry.collected == collected
        {
            meter.spend(entry.cost)?;
            self.hits += 1;
            self.products
                .push((entry.output_identity.clone(), entry.source_identity.clone()));
            return Ok(entry.output.as_ref().clone());
        }
        let before = meter.remaining();
        let output = lower()?;
        let cost = before.saturating_sub(meter.remaining());
        self.misses += 1;
        let source = function
            .blocks
            .iter()
            .map(|b| {
                b.instructions
                    .iter()
                    .map(|i| match &i.kind {
                        InstructionKind::Call { function, .. } => Some(*function),
                        _ => None,
                    })
                    .collect()
            })
            .collect::<Vec<Vec<Option<FunctionId>>>>();
        let old = self.entries.get(&function.id);
        let (output, output_identity) =
            if let Some(old) = old.filter(|old| old.output.as_ref() == &output) {
                (old.output.clone(), old.output_identity.clone())
            } else {
                (Arc::new(output), Arc::new(()))
            };
        let source_identity = old
            .filter(|old| old.source == source)
            .map_or_else(|| Arc::new(()), |old| old.source_identity.clone());
        self.products
            .push((output_identity.clone(), source_identity.clone()));
        self.entries.insert(
            function.id,
            Arc::new(Entry {
                input: function.clone(),
                facts,
                collected,
                output: output.clone(),
                output_identity,
                source_identity,
                source,
                cost,
            }),
        );
        let output = output.as_ref().clone();
        Ok(output)
    }
}
