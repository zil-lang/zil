//! Queries for the ordinary, closure-free resource transfer. The original
//! frontier, convergence operations and final validation order remain intact.
//! Packages, sums, Stores, callbacks, replacements and 32+ function constraint
//! extraction conservatively keep their existing fresh implementation.
use super::*;
use std::hash::Hash;
use std::sync::Arc;
use zil_base::{Fingerprint, FingerprintBuilder};

#[derive(Clone, Default)]
pub(crate) struct State(Arc<BTreeMap<FunctionId, Arc<Entry>>>);

struct Entry {
    function: TypedFunction,
    context: Fingerprint,
    limits: Limits,
    callees: Arc<Vec<FunctionId>>,
    calls: Vec<Arc<Call>>,
}
struct Call {
    check: bool,
    reachable: Vec<bool>,
    reads: Vec<(FunctionId, Effect)>,
    result: Effect,
    cost: usize,
}

pub(crate) struct Request {
    prior: State,
    next: BTreeMap<FunctionId, Entry>,
    limits: Limits,
    context: Option<Fingerprint>,
    executed: usize,
    reused: usize,
}

impl Request {
    pub(crate) fn new(prior: State, limits: &Limits) -> Self {
        Self {
            prior,
            next: BTreeMap::new(),
            limits: limits.clone(),
            context: None,
            executed: 0,
            reused: 0,
        }
    }

    pub(crate) fn prepare(&mut self, module: &TypedModule, budget: &Budget) {
        if module.resources.is_empty()
            || module.functions.len() >= 32
            || budget.static_utf8
            || budget.pod_stores
            || budget.generic_mutex
            || budget.tasks
            || budget.registrations
            || budget.async_registrations
            || budget.stored_borrows
            || budget.task_borrowed_results
            || budget.store_option_results
            || budget.borrowed_callback_inputs
            || budget.invocation_callbacks
            || budget.borrowed_function_pointers
            || budget.scoped_foreign
            || budget.scoped_threads
            || budget.shared_product_capture
            || budget.address_capture
            || budget.owning_address_captures
            || budget.affine_packages
            || budget.addressed_sums
            || budget.mixed_address_sums
            || !budget.replacement_functions.is_empty()
            || !budget.owned_callbacks.is_empty()
            || !module.closures.is_empty()
            || !module.life_declarations.is_empty()
            || !module.life_instances.is_empty()
            || !module.dyn_declarations.is_empty()
            || !module.dyn_interfaces.is_empty()
            || !module.dyn_vtables.is_empty()
            || !module.unsupported.is_empty()
            || module
                .resources
                .iter()
                .any(|resource| resource.teardown.is_some() || !payload(&resource.payload))
            || !module.functions.iter().all(|function| {
                self.prior.0.get(&function.id).is_some_and(|entry| {
                    crate::candidate_queries::semantic_input_eq(&entry.function, function)
                }) || profile(function)
            })
        {
            return;
        }
        // This profile never enters the physical-slot or addressed-Sum memo
        // paths. Its transfer changes only Budget.remaining; all scheduling
        // flags and all callable/resource headers it can observe are bound.
        let mut context = FingerprintBuilder::new();
        "zil-analyze-resource-context-v1".hash(&mut context);
        module.schema_version.hash(&mut context);
        module.module_id.hash(&mut context);
        module.target.fingerprint().hash(&mut context);
        budget.head_descriptions.hash(&mut context);
        module.resources.len().hash(&mut context);
        for resource in &module.resources {
            (&resource.name, &resource.payload, resource.teardown).hash(&mut context);
        }
        module.functions.len().hash(&mut context);
        for function in &module.functions {
            (
                function.id,
                &function.name,
                function.call_kind,
                &function.return_type,
                function.result_mode,
            )
                .hash(&mut context);
            function.parameters.hash(&mut context);
            for parameter in &function.parameters {
                let Some(local) = function.locals.get(parameter.0 as usize) else {
                    return;
                };
                local.ty.hash(&mut context);
            }
        }
        self.context = Some(context.digest());
    }

    pub(super) fn transfer(
        &mut self,
        module: &TypedModule,
        function: &TypedFunction,
        reachable: &[bool],
        effects: &[Effect],
        check: bool,
        budget: &mut Budget,
    ) -> CompileResult<Effect> {
        let Some(context) = self.context else {
            self.executed += 1;
            return analyze_function(module, function, reachable, effects, check, budget);
        };
        let prior = self.prior.0.get(&function.id).filter(|entry| {
            entry.context == context
                && entry.limits == self.limits
                && crate::candidate_queries::semantic_input_eq(&entry.function, function)
        });
        let entry = self.next.entry(function.id).or_insert_with(|| Entry {
            function: function.clone(),
            context,
            limits: self.limits.clone(),
            callees: prior.map_or_else(
                || {
                    Arc::new(
                        function
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .filter_map(|instruction| match instruction.kind {
                                InstructionKind::Call { function, .. } => Some(function),
                                _ => None,
                            })
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect(),
                    )
                },
                |entry| entry.callees.clone(),
            ),
            calls: Vec::new(),
        });
        let Some(reads) = entry
            .callees
            .iter()
            .map(|id| {
                effects
                    .get(id.0 as usize)
                    .map(|effect| (*id, effect.clone()))
            })
            .collect::<Option<Vec<_>>>()
        else {
            return analyze_function(module, function, reachable, effects, check, budget);
        };
        let hit = entry
            .calls
            .iter()
            .chain(prior.into_iter().flat_map(|entry| &entry.calls))
            .find(|call| {
                call.check == check
                    && call.reachable == reachable
                    && call.reads == reads
                    && call.cost <= budget.remaining()
            })
            .cloned();
        if let Some(hit) = hit {
            budget.spend(hit.cost, "cached resource transfer")?;
            self.reused += 1;
            let result = hit.result.clone();
            entry.calls.push(hit);
            return Ok(result);
        }
        self.executed += 1;
        let before = budget.remaining();
        let result = analyze_function(module, function, reachable, effects, check, budget)?;
        entry.calls.push(Arc::new(Call {
            check,
            reachable: reachable.to_vec(),
            reads,
            result: result.clone(),
            cost: before - budget.remaining(),
        }));
        Ok(result)
    }

    pub(crate) fn finish(self, stats: &mut AnalysisStats) -> State {
        self.record_stats(stats);
        State(Arc::new(
            self.next
                .into_iter()
                .map(|(id, entry)| (id, Arc::new(entry)))
                .collect(),
        ))
    }

    pub(crate) fn record_stats(&self, stats: &mut AnalysisStats) {
        stats.resource_transfers_executed = self.executed;
        stats.resource_transfers_reused = self.reused;
    }
}

fn scalar(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
            | Type::U8
            | Type::U16
    )
}
fn payload(ty: &Type) -> bool {
    scalar(ty)
        || matches!(ty, Type::ByteBuffer)
        || matches!(ty, Type::Record { fields, .. } if fields.iter().all(|(_, ty)| payload(ty) && !matches!(ty, Type::ByteBuffer)))
}
fn ty(ty: &Type) -> bool {
    scalar(ty)
        || matches!(ty, Type::Managed(inner) | Type::Desc(inner) if payload(inner))
        || matches!(ty, Type::Address { inner, .. } if scalar(inner) || matches!(inner.as_ref(), Type::Managed(payload_ty) if payload(payload_ty)))
}
fn profile(function: &TypedFunction) -> bool {
    function.call_kind == CallKind::Direct
        && function.storage_life.is_none()
        && function.reference_return_bound.is_none()
        && function.callable_return_contract.is_none()
        && function.callable_contract.is_none()
        && function.protocol_contract.is_none()
        && function.foreign.is_none()
        && function.environment.is_none()
        && (scalar(&function.return_type)
            || matches!(&function.return_type, Type::Managed(inner) if payload(inner)))
        && function.locals.iter().all(|local| ty(&local.ty))
        && function.values.iter().all(ty)
        && function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .all(|instruction| {
                matches!(&instruction.kind,
                InstructionKind::Constant(value) if scalar(&value.ty()))
                    || matches!(
                        instruction.kind,
                        InstructionKind::Load(_)
                            | InstructionKind::Store { .. }
                            | InstructionKind::TakeLocal { .. }
                            | InstructionKind::StorageLive(_)
                            | InstructionKind::StorageDead(_)
                            | InstructionKind::ScopeEnter { .. }
                            | InstructionKind::ScopeExit { .. }
                            | InstructionKind::Unary { .. }
                            | InstructionKind::Binary { .. }
                            | InstructionKind::Call { .. }
                            | InstructionKind::MakeManaged { .. }
                            | InstructionKind::BorrowLocal { .. }
                            | InstructionKind::BorrowLocalMut { .. }
                            | InstructionKind::DescribeLocal { .. }
                            | InstructionKind::DescribeAddress { .. }
                            | InstructionKind::ReadDesc { .. }
                            | InstructionKind::WriteDesc { .. }
                            | InstructionKind::DropManaged { .. }
                            | InstructionKind::ReadAddress { .. }
                            | InstructionKind::WriteAddress { .. }
                            | InstructionKind::ReborrowAddress { .. }
                            | InstructionKind::ByteBufferNew
                            | InstructionKind::ByteBufferPush { .. }
                            | InstructionKind::ByteBufferLen { .. }
                            | InstructionKind::ByteBufferCapacity { .. }
                    )
                    || matches!(&instruction.kind, InstructionKind::ConvertInteger { target, .. }
                    | InstructionKind::ConvertNumber { target, .. } if scalar(target))
            })
}
