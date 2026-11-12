//! Immutable, current-input-bound scalar lowering work products. These are raw
//! machine values, never verifier capabilities. Unsupported inputs use the
//! original emitter, including its budget and diagnostic behavior.
use super::*;
use std::sync::Arc;
mod whole;
pub(super) use whole::WholeOptions;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Session {
    context: Option<Context>,
    entries: BTreeMap<FunctionId, Arc<Entry>>,
    pub(crate) hits: usize,
    pub(crate) misses: usize,
    pub(crate) source_projection_traversals: usize,
    pub(crate) source_projection_instruction_visits: usize,
    pub(crate) source_projection_reuses: usize,
    context_identity: Arc<()>,
    pub(crate) products: Vec<(Arc<()>, Arc<()>)>,
    pub(crate) eligible: bool,
    admitted: BTreeMap<FunctionId, Vec<FunctionId>>,
    whole_machine: Option<Arc<whole::MachineProduct>>,
    pub(crate) reused_machine_module: bool,
    pub(crate) whole_machine_input_functions_compared: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Context {
    target: TargetProfile,
    limits: zil_base::Limits,
    entry: EntryPoint,
    scalar_policy: bool,
    headers: Vec<Header>,
    globals: Globals,
}
/// Complete conservative inventory for mixed modules. Only the current checked
/// snapshot supplies roles; these raw values never authorize an emitter branch.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Globals {
    schema: u32,
    module: zil_base::ModuleId,
    resources: Vec<ResourceDefinition>,
    closures: Vec<ClosureDefinition>,
    dyn_declarations: Vec<zil_ir::DynDeclaration>,
    dyn_interfaces: Vec<zil_ir::DynInterface>,
    dyn_vtables: Vec<zil_ir::DynVtable>,
    life_declarations: Vec<zil_ir::LifeNominalDeclaration>,
    life_instances: Vec<zil_ir::LifeNominalInstance>,
    task_interfaces: Vec<zil_ir::TaskInterface>,
    workers: BTreeSet<FunctionId>,
    roles: Vec<(FunctionId, bool, bool, bool)>,
    flags: [bool; 4],
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
    storage_life: Option<zil_ir::StorageLifeContract>,
    call_kind: zil_ir::CallKind,
    reference_return_bound: Option<Vec<u32>>,
    callable_return_contract: Option<zil_ir::CallableReturnContract>,
    callable_contract: Option<zil_ir::CallableParameterContract>,
    protocol_contract: Option<zil_ir::FunctionProtocolContract>,
    environment: Option<ClosureId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    input: TypedFunction,
    facts: Vec<(FunctionId, Option<Constant>)>,
    collected: bool,
    output: Arc<FunctionBatch>,
    output_identity: Arc<()>,
    source_identity: Arc<()>,
    source: Option<Arc<SourceProjection>>,
    callees: Vec<FunctionId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceProjection {
    slots: Vec<Vec<Option<FunctionId>>>,
    observed: Vec<FunctionId>,
}
fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit | Type::Bool | Type::I32 | Type::I64 | Type::U8 | Type::U16
    )
}
fn eligible(f: &TypedFunction, previous: Option<&Entry>) -> Option<Vec<FunctionId>> {
    if f.environment.is_some()
        || f.storage_life.is_some()
        || f.call_kind != zil_ir::CallKind::Direct
        || f.reference_return_bound.is_some()
        || f.callable_return_contract.is_some()
        || f.callable_contract.is_some()
        || f.protocol_contract.is_some()
        || !scalar(&f.return_type)
        || !(previous.is_some_and(|previous| previous.input.locals.ptr_eq(&f.locals))
            || f.locals.iter().all(|l| scalar(&l.ty)))
        || !(previous.is_some_and(|previous| previous.input.values.ptr_eq(&f.values))
            || f.values.iter().all(scalar))
    {
        return None;
    }
    if let Some(previous) = previous.filter(|previous| previous.input.blocks.ptr_eq(&f.blocks)) {
        return Some(previous.callees.clone());
    }
    let mut callees = BTreeSet::new();
    for instruction in f.blocks.iter().flat_map(|block| &block.instructions) {
        match &instruction.kind {
            InstructionKind::Constant(c) if scalar(&c.ty()) => {}
            InstructionKind::Call { function, .. }
            | InstructionKind::ForeignCall { function, .. } => {
                callees.insert(*function);
            }
            InstructionKind::Load(_)
            | InstructionKind::Store { .. }
            | InstructionKind::StorageLive(_)
            | InstructionKind::StorageDead(_)
            | InstructionKind::ScopeEnter { .. }
            | InstructionKind::ScopeExit { .. }
            | InstructionKind::UnsafeEnter { .. }
            | InstructionKind::UnsafeExit { .. }
            | InstructionKind::Unary { .. }
            | InstructionKind::Binary { .. }
            | InstructionKind::ConvertInteger { .. } => {}
            _ => return None,
        }
    }
    Some(callees.into_iter().collect())
}
fn scalar_abi(id: FunctionId, plans: &large_copy_abi::Plans) -> bool {
    plans
        .functions
        .get(&id)
        .is_some_and(|abi| !abi.versioned && !abi.large())
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
        copy_plans: &large_copy_abi::Plans,
    ) -> bool {
        // Any rebuilding of the per-function identity starts a new epoch.
        self.whole_machine = None;
        self.hits = 0;
        self.misses = 0;
        self.source_projection_traversals = 0;
        self.source_projection_instruction_visits = 0;
        self.source_projection_reuses = 0;
        self.eligible = false;
        self.products.clear();
        // The source fingerprint is diagnostic identity; the function consumer
        // reads the exact CFG plus the following complete current inventories.
        // Exhaustive matching makes new module domains require a new audit.
        let zil_ir::TypedModule {
            dyn_declarations,
            dyn_interfaces,
            dyn_vtables,
            life_declarations,
            life_instances,
            schema_version,
            module_id,
            source_fingerprint: _,
            target: _,
            functions: _,
            closures: _,
            resources: _,
            unsupported: _,
        } = verified.module();
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
                    storage_life: f.storage_life.clone(),
                    call_kind: f.call_kind,
                    reference_return_bound: f.reference_return_bound.clone(),
                    callable_return_contract: f.callable_return_contract.clone(),
                    callable_contract: f.callable_contract.clone(),
                    protocol_contract: f.protocol_contract.clone(),
                    environment: f.environment,
                })
                .collect(),
            globals: Globals {
                schema: *schema_version,
                module: *module_id,
                resources: lir.resources.to_vec(),
                closures: lir.closures.to_vec(),
                dyn_declarations: dyn_declarations.clone(),
                dyn_interfaces: dyn_interfaces.clone(),
                dyn_vtables: dyn_vtables.clone(),
                life_declarations: life_declarations.clone(),
                life_instances: life_instances.clone(),
                task_interfaces: verified.task_interfaces().to_vec(),
                workers: verified.task_worker_ids().clone(),
                roles: lir
                    .functions
                    .iter()
                    .map(|f| {
                        (
                            f.id,
                            verified.is_suspend(f.id),
                            verified.is_task_export(f.id),
                            verified.task_ownership(f.id).is_some(),
                        )
                    })
                    .collect(),
                flags: [
                    verified.uses_scalar_tasks(),
                    verified.uses_retained_registration(),
                    verified.uses_stored_borrows(),
                    verified.uses_owned_sum_extension(),
                ],
            },
        };
        if self.context.as_ref() != Some(&context) {
            self.entries.clear();
            self.context = Some(context);
            self.context_identity = Arc::new(());
        }
        let functions = lir
            .functions
            .iter()
            .map(|f| (f.id, f))
            .collect::<BTreeMap<_, _>>();
        self.admitted = lir
            .functions
            .iter()
            .filter_map(|function| {
                if verified.task_worker_ids().contains(&function.id)
                    || verified.is_suspend(function.id)
                    || verified.is_task_export(function.id)
                    || verified.task_ownership(function.id).is_some()
                    || !scalar_abi(function.id, copy_plans)
                    || copy_plans
                        .values
                        .get(&function.id)
                        .is_some_and(|values| !values.is_empty())
                    || copy_plans
                        .indirect
                        .range((function.id, ValueId(0))..=(function.id, ValueId(u32::MAX)))
                        .next()
                        .is_some()
                {
                    return None;
                }
                let callees = eligible(function, self.entries.get(&function.id).map(Arc::as_ref))?;
                if !callees.iter().all(|id| {
                    functions.get(id).is_some_and(|callee| {
                        callee.environment.is_none()
                            && callee.call_kind == zil_ir::CallKind::Direct
                            && scalar(&callee.return_type)
                            && callee
                                .parameters
                                .iter()
                                .all(|id| scalar(&callee.locals[id.0 as usize].ty))
                            && scalar_abi(callee.id, copy_plans)
                    })
                }) {
                    return None;
                }
                Some((function.id, callees))
            })
            .collect();
        // A partial function cache cannot certify equality of current generated
        // helpers, runtimes, tables or worker machines. Keep that old proof's
        // entire closed scalar domain, independently of per-function admission.
        self.eligible = self.admitted.len() == lir.functions.len()
            && lir.closures.is_empty()
            && lir.resources.is_empty()
            && verified.module().dyn_declarations.is_empty()
            && verified.module().dyn_interfaces.is_empty()
            && verified.module().dyn_vtables.is_empty()
            && verified.module().life_declarations.is_empty()
            && verified.module().life_instances.is_empty()
            && verified.task_interfaces().is_empty()
            && !verified.uses_scalar_tasks()
            && !verified.uses_retained_registration();
        // Exactly one current epoch; neither deleted nor unsupported functions
        // retain historical machine values.
        self.entries.retain(|id, _| self.admitted.contains_key(id));
        !self.admitted.is_empty()
    }
    pub(crate) fn identity(&self) -> Option<crate::incremental::ScalarIdentity> {
        self.eligible
            .then(|| (self.context_identity.clone(), self.products.clone()))
    }
    pub(super) fn lower<F, R>(
        &mut self,
        function: &TypedFunction,
        summaries: Option<&BTreeMap<FunctionId, Constant>>,
        collected: Option<&BTreeSet<FunctionId>>,
        reserve_current: R,
        lower: F,
    ) -> CompileResult<FunctionBatch>
    where
        F: FnOnce() -> CompileResult<FunctionBatch>,
        R: FnOnce() -> CompileResult<()>,
    {
        if !self.admitted.contains_key(&function.id) {
            self.misses += 1;
            return lower();
        }
        // With ordinary scalar policy there is no summary consumer, complete
        // implementation import or source-call grouping. ABI/header inventory
        // and each actual machine output remain bound independently. In the
        // explicit scalar policy, preserve ordered slots and observed callees:
        // an absent current fact can become present after a callee body edit.
        let consumes_calls = summaries.is_some()
            || self
                .context
                .as_ref()
                .is_some_and(|context| context.scalar_policy);
        let source = if !consumes_calls {
            None
        } else if let Some(source) = self
            .entries
            .get(&function.id)
            .filter(|entry| entry.input.blocks.ptr_eq(&function.blocks))
            .and_then(|entry| entry.source.as_ref())
        {
            self.source_projection_reuses += 1;
            Some(source.clone())
        } else {
            self.source_projection_traversals += 1;
            let mut observed = BTreeSet::from([function.id]);
            let slots = function
                .blocks
                .iter()
                .map(|block| {
                    block
                        .instructions
                        .iter()
                        .map(|instruction| {
                            self.source_projection_instruction_visits += 1;
                            if let InstructionKind::Call { function, .. } = &instruction.kind {
                                observed.insert(*function);
                                Some(*function)
                            } else {
                                None
                            }
                        })
                        .collect()
                })
                .collect();
            Some(Arc::new(SourceProjection {
                slots,
                observed: observed.into_iter().collect(),
            }))
        };
        let facts = if let (Some(source), Some(summaries)) = (&source, summaries) {
            source
                .observed
                .iter()
                .map(|id| (*id, summaries.get(id).cloned()))
                .collect()
        } else {
            Vec::new()
        };
        let collected = collected.is_none_or(|needed| needed.contains(&function.id));
        if let Some(entry) = self.entries.get(&function.id)
            && same_input(&entry.input, function)
            && entry.facts == facts
            && entry.collected == collected
        {
            // Execute the same current reservation consumer as a fresh body.
            // In particular task/registration project flags may charge scans
            // even though this function itself has no task operations.
            reserve_current()?;
            self.hits += 1;
            self.products
                .push((entry.output_identity.clone(), entry.source_identity.clone()));
            return Ok(entry.output.as_ref().clone());
        }
        let output = lower()?;
        self.misses += 1;
        let old = self.entries.get(&function.id);
        let (output, output_identity) =
            if let Some(old) = old.filter(|old| old.output.as_ref() == &output) {
                (old.output.clone(), old.output_identity.clone())
            } else {
                (Arc::new(output), Arc::new(()))
            };
        let source_identity = old
            .filter(|old| match (&old.source, &source) {
                (Some(old), Some(current)) => {
                    Arc::ptr_eq(old, current) || old.slots == current.slots
                }
                (None, None) => true,
                _ => false,
            })
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
                callees: self.admitted[&function.id].clone(),
            }),
        );
        let output = output.as_ref().clone();
        Ok(output)
    }
}

#[cfg(test)]
mod mixed_tests {
    use super::*;
    use zil_base::{Fingerprint, Limits, ModuleId};
    use zil_ir::*;

    fn function(id: u32, kind: InstructionKind, values: Vec<Type>) -> TypedFunction {
        TypedFunction {
            storage_life: None,
            call_kind: CallKind::Direct,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contract: None,
            id: FunctionId(id),
            name: format!("function{id}"),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            locals: vec![].into(),
            values: values.into(),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction {
                    result: Some(ValueId(0)),
                    kind,
                    span: Span::default(),
                }],
                terminator: Terminator::Return(Some(ValueId(0))),
            }]
            .into(),
            entry: BlockId(0),
            span: Span::default(),
        }
    }
    fn fixture() -> VerifiedModule {
        let mut worker = function(0, InstructionKind::Load(LocalId(0)), vec![Type::I32]);
        worker.parameters.push(LocalId(0));
        worker.locals.push(LocalDecl {
            id: LocalId(0),
            name: "x".into(),
            ty: Type::I32,
            mutable: false,
            span: Span::default(),
        });
        let mut root = function(
            1,
            InstructionKind::Constant(Constant::I32(7)),
            vec![
                Type::I32,
                Type::Managed(Box::new(Type::TaskI32)),
                Type::Unit,
            ],
        );
        root.blocks[0].instructions.extend([
            Instruction {
                result: Some(ValueId(1)),
                kind: InstructionKind::TaskStart {
                    worker: FunctionId(0),
                    argument: ValueId(0),
                },
                span: Span::default(),
            },
            Instruction {
                // Task operations publish Unit SSA results as well. This is
                // the same shape emitted by Types::tasks through self.value.
                result: Some(ValueId(2)),
                kind: InstructionKind::TaskFinishDestroy { task: ValueId(1) },
                span: Span::default(),
            },
        ]);
        let leaf = function(
            2,
            InstructionKind::Constant(Constant::I32(42)),
            vec![Type::I32],
        );
        let module = TypedModule {
            dyn_declarations: vec![],
            dyn_interfaces: vec![],
            dyn_vtables: vec![],
            life_declarations: vec![],
            life_instances: vec![],
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::of(b"mixed-task-reservation"),
            target: TargetProfile::host_llvm(),
            functions: vec![worker, root, leaf],
            closures: vec![],
            resources: vec![ResourceDefinition {
                name: TASK_I32_RESOURCE_NAME.into(),
                payload: Type::TaskI32,
                teardown: None,
            }],
            unsupported: vec![],
        };
        let mut plan = CandidatePlan::empty_for(&module);
        plan.functions = module
            .functions
            .iter()
            .map(|f| FunctionPlan {
                function: f.id,
                holds: (0..f.values.len())
                    .map(|id| ValueHold {
                        value: ValueId(id as u32),
                        roots: vec![],
                    })
                    .collect(),
                blocks: vec![BlockPlan {
                    block: BlockId(0),
                    initialized_in: f.parameters.clone(),
                    initialized_out: f.parameters.clone(),
                }],
            })
            .collect();
        let target = module.target.clone();
        zil_verify::verify_module(module, plan, &target, &Limits::default()).unwrap()
    }

    #[test]
    fn mixed_task_leaf_reuse_runs_current_reservations_at_n_and_n_minus_one() {
        let verified = fixture();
        assert!(verified.uses_scalar_tasks());
        assert!(verified.task_worker_ids().contains(&FunctionId(0)));
        let module = verified.module();
        let raw = LirModule {
            schema_version: module.schema_version,
            module_id: module.module_id,
            verified_fingerprint: verified.fingerprint(),
            target: module.target.clone(),
            functions: module.functions.clone(),
            closures: module.closures.clone(),
            resources: module.resources.clone(),
            layouts: vec![],
        };
        let view = raw.as_view();
        let limits = verified.limits();
        let plans = copy_aggregates::plan(&view, limits, limits.max_analysis_steps).unwrap();
        let options = EmitOptions {
            entry: EntryPoint::None,
        };
        let functions = raw.functions.iter().map(|f| (f.id, f)).collect();
        let closures = BTreeMap::new();
        let leaf = &raw.functions[2];
        let mut session = Session::default();
        assert!(session.prepare(&view, &verified, &options, false, &plans));
        assert!(
            !session.admitted.contains_key(&FunctionId(0)),
            "scalar source is a current worker"
        );
        assert!(session.admitted.contains_key(&FunctionId(2)));
        assert!(
            session.identity().is_none(),
            "mixed module is never a scalar identity"
        );
        let fresh = |meter: &tasks::Meter| {
            lower_function_batch(
                std::slice::from_ref(leaf),
                &view,
                &options,
                false,
                &functions,
                &closures,
                None,
                None,
                None,
                &plans,
                &verified,
                meter,
            )
        };
        let full = tasks::Meter::new(limits.max_analysis_steps);
        let original = fresh(&full).unwrap();
        let n = limits.max_analysis_steps - full.remaining();
        assert!(n > 0, "project task charges apply even to a scalar leaf");
        session
            .lower(
                leaf,
                None,
                None,
                || unreachable!("cold miss"),
                || fresh(&tasks::Meter::new(n)),
            )
            .unwrap();
        for budget in [n, n - 1, n] {
            assert!(session.prepare(&view, &verified, &options, false, &plans));
            let meter = tasks::Meter::new(budget);
            let cached = session.lower(
                leaf,
                None,
                None,
                || {
                    reserve_function_emission(leaf, &view, &closures, &functions, &verified, &meter)
                        .map(|_| ())
                },
                || panic!("unchanged scalar machine must hit"),
            );
            let fresh_meter = tasks::Meter::new(budget);
            let expected = fresh(&fresh_meter);
            assert_eq!(cached, expected);
            assert_eq!(meter.remaining(), fresh_meter.remaining());
            if budget == n {
                assert_eq!(cached.unwrap(), original);
                assert_eq!(session.hits, 1);
            } else {
                assert_eq!(
                    cached.unwrap_err()[0].category,
                    zil_base::ResultCategory::ResourceLimit
                );
            }
        }
    }
}
