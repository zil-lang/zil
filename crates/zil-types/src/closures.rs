//! Local Copy aggregate, resource and shared-address closures. Inference variables are private compiler state, never
//! forged `Type::Unsupported` values in the public IR.
use super::*;
use std::collections::BTreeSet;
mod methods;
mod package_effects;
mod patterns;
mod slots;
mod source_context;

pub(super) fn source_owned_capture(ty: &Type) -> bool {
    source_context::owned_scalar_resource(ty)
}
pub(super) fn reserve_source_contexts(
    rows: &[CoreClosureLifeContext],
    limits: &Limits,
    left: &mut usize,
    copies: usize,
) -> R<()> {
    source_context::reserve(rows, limits, left, copies)
}

#[derive(Clone)]
pub(super) struct Capture {
    pub name: String,
    pub source: LocalId,
    pub ty: Type,
    pub borrowed: bool,
    pub written: bool,
    // A mutable address can change the activation local without using Store.
    // Decide this over the complete source body before generating any exit.
    pub address_written: bool,
    pub moved: bool,
    pub consumed: bool,
    pub declared_kind: Option<CallableKind>,
    pub span: Span,
}

#[cfg(test)]
mod thread_lease_regressions {
    use super::*;

    // Internal accounting fixtures only: they carry no source or CFG proof.
    fn pending_state(result: Option<Type>, count: usize) -> State {
        let limits = Limits::default();
        let mut state = State::new(0, &limits);
        for index in 0..count {
            let mut graph = Graph::new(&limits);
            let parameter = graph.fresh(I32, false);
            let value = graph.fresh(VALUE, false);
            if let Some(result) = &result {
                graph.bind(value, result, Span::default()).unwrap();
            }
            state.pending.push(Pending {
                id: ClosureId(index as u32),
                function: FunctionId(index as u32),
                parameters: vec![CoreClosureParameter {
                    name: "value".into(),
                    ty: Some(Type::I32),
                    mode: PassingMode::Infer,
                    span: Span::default(),
                }],
                body: CoreBlock {
                    statements: vec![],
                    span: Span::default(),
                },
                span: Span::default(),
                captures: vec![],
                owns_leaf_store: false,
                defer_action: false,
                inference: Inference {
                    graph,
                    parameters: vec![parameter],
                    result: value,
                },
                typed: None,
                source_context: None,
                requires_source_context: false,
            });
        }
        state
    }

    fn exhausted<T>(result: R<T>) {
        let error = result
            .err()
            .expect("a fresh private Graph quota bypassed shared exhaustion");
        assert_eq!(error.phase, Phase::Types);
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
    }

    #[test]
    fn two_pending_graphs_share_the_current_remainder_and_cannot_renew_it() {
        let mut state = pending_state(Some(Type::I32), 2);
        state.work_left = 2;
        let before0 = state.pending[0].inference.graph.work_left.get();
        let before1 = state.pending[1].inference.graph.work_left.get();
        assert_eq!(state.thread_result(ClosureId(0)).unwrap(), Some(Type::I32));
        assert_eq!(state.work_left, 1);
        assert_eq!(
            state.thread_parameter(ClosureId(1), 0).unwrap(),
            Some(Type::I32)
        );
        assert_eq!(state.work_left, 0);
        assert_eq!(
            state.pending[0].inference.graph.work_left.get(),
            before0 - 1
        );
        assert_eq!(
            state.pending[1].inference.graph.work_left.get(),
            before1 - 1
        );
        exhausted(state.thread_result(ClosureId(1)));
        exhausted(state.thread_parameter(ClosureId(0), 0));
        assert_eq!(state.work_left, 0);
        assert_eq!(
            state.pending[0].inference.graph.work_left.get(),
            before0 - 1
        );
        assert_eq!(
            state.pending[1].inference.graph.work_left.get(),
            before1 - 1
        );
        let mut limited = pending_state(Some(Type::I32), 1);
        limited.pending[0].inference.graph.work_left.set(1);
        limited.work_left = 1000;
        limited.thread_result(ClosureId(0)).unwrap();
        assert_eq!(limited.work_left, 999);
        exhausted(limited.thread_result(ClosureId(0)));
        assert_eq!(limited.work_left, 999);
        assert_eq!(limited.pending[0].inference.graph.work_left.get(), 0);
    }

    #[test]
    fn actual_pending_signature_clone_receipt_has_exact_n_and_failed_n_minus_one() {
        let ty = Type::Tuple(vec![Type::I32, Type::I32]);
        let mut ample = pending_state(Some(ty.clone()), 1);
        let before_shared = ample.work_left;
        let before_graph = ample.pending[0].inference.graph.work_left.get();
        assert_eq!(ample.thread_result(ClosureId(0)).unwrap(), Some(ty.clone()));
        let work = before_graph - ample.pending[0].inference.graph.work_left.get();
        assert!(work > 1);
        assert_eq!(before_shared - ample.work_left, work);
        for amount in [work, work - 1, work] {
            let mut current = pending_state(Some(ty.clone()), 1);
            current.work_left = amount;
            let before = current.pending[0].inference.graph.work_left.get();
            let result = current.thread_result(ClosureId(0));
            if amount == work {
                assert_eq!(result.unwrap(), Some(ty.clone()));
                assert_eq!(current.work_left, 0);
            } else {
                exhausted(result);
            }
            assert_eq!(
                amount - current.work_left,
                before - current.pending[0].inference.graph.work_left.get()
            );
            assert!(current.work_left <= amount);
        }
    }

    #[test]
    fn mutable_pending_expectation_debits_the_same_graph_work_once_even_on_failure() {
        let mut ample = pending_state(None, 1);
        let shared = ample.work_left;
        let graph = ample.pending[0].inference.graph.work_left.get();
        ample
            .thread_expect_result(ClosureId(0), &Type::I32, Span::default())
            .unwrap();
        let work = graph - ample.pending[0].inference.graph.work_left.get();
        assert!(work > 0);
        assert_eq!(shared - ample.work_left, work);
        for amount in [work, work - 1, work] {
            let mut current = pending_state(None, 1);
            current.work_left = amount;
            let graph = current.pending[0].inference.graph.work_left.get();
            let result = current.thread_expect_result(ClosureId(0), &Type::I32, Span::default());
            if amount == work {
                result.unwrap();
                assert_eq!(current.work_left, 0);
            } else {
                exhausted(result);
            }
            assert_eq!(
                amount - current.work_left,
                graph - current.pending[0].inference.graph.work_left.get()
            );
            assert!(current.work_left <= amount);
        }
        let mut zero = pending_state(None, 1);
        zero.work_left = 0;
        let before = zero.pending[0].inference.graph.work_left.get();
        exhausted(zero.thread_expect_result(ClosureId(0), &Type::I32, Span::default()));
        assert_eq!(zero.work_left, 0);
        assert_eq!(zero.pending[0].inference.graph.work_left.get(), before);
    }
    #[test]
    fn qualified_pending_snapshots_pay_storage_and_metadata_before_copying() {
        let mut costs = Vec::new();
        for width in [4, 32] {
            let ty = Type::BorrowedStorage {
                storage: Box::new(Type::Record {
                    name: "ScopedRecord".into(),
                    fields: (0..width)
                        .map(|i| {
                            (
                                format!("field_{i}"),
                                Type::Address {
                                    mutable: false,
                                    inner: Box::new(Type::I32),
                                },
                            )
                        })
                        .collect(),
                }),
                life: StorageLifeShape {
                    binder_count: 1,
                    slots: (0..width)
                        .map(|i| StorageLifeSlot {
                            path: vec![PlaceProjection::Field(i)],
                            binder: 0,
                            mode: StoredBorrowMode::SharedAddress,
                        })
                        .collect(),
                    retained: vec![],
                    nominal_arguments: vec![],
                },
            };
            let mut ample = pending_state(Some(ty.clone()), 1);
            let before = ample.work_left;
            assert_eq!(ample.thread_result(ClosureId(0)).unwrap(), Some(ty.clone()));
            let cost = before - ample.work_left;
            assert!(cost > width as usize * 2);
            for allowance in [cost, cost - 1, cost] {
                let mut state = pending_state(Some(ty.clone()), 1);
                state.work_left = allowance;
                let result = state.thread_result(ClosureId(0));
                if allowance == cost {
                    assert_eq!(result.unwrap(), Some(ty.clone()));
                    assert_eq!(state.work_left, 0);
                } else {
                    exhausted(result);
                }
            }
            costs.push(cost);
        }
        assert!(costs[1] > costs[0]);
        let limits = Limits {
            max_call_depth: 2,
            ..Limits::default()
        };
        let graph = Graph::new(&limits);
        let mut deep = Type::I32;
        for _ in 0..4 {
            deep = Type::BorrowedStorage {
                storage: Box::new(deep),
                life: StorageLifeShape {
                    binder_count: 0,
                    slots: vec![],
                    retained: vec![],
                    nominal_arguments: vec![],
                },
            };
        }
        exhausted(graph.reserve_type(&deep, 0, Span::default()));
    }
}
impl Capture {
    pub fn stored_type(&self) -> Type {
        if let Type::Managed(payload) = &self.ty {
            if self.moved {
                self.ty.clone()
            } else {
                Type::Desc(payload.clone())
            }
        } else if self.borrowed {
            Type::Address {
                mutable: self.written,
                inner: Box::new(self.ty.clone()),
            }
        } else if !self.moved {
            match &self.ty {
                Type::Address { mutable, inner } => Type::Address {
                    mutable: *mutable && self.written,
                    inner: inner.clone(),
                },
                _ => self.ty.clone(),
            }
        } else {
            self.ty.clone()
        }
    }
    pub fn mutable_call(&self) -> bool {
        // The stored type's outer mode is determined by these flags; querying
        // it does not need to clone the captured payload tree.
        if matches!(self.ty, Type::Desc(_)) || (!self.moved && matches!(self.ty, Type::Managed(_)))
        {
            return false;
        }
        self.written
            || (self.moved
                && !self.borrowed
                && matches!(self.ty, Type::Address { mutable: true, .. }))
    }
}

pub(super) struct Pending {
    pub id: ClosureId,
    pub function: FunctionId,
    pub parameters: Vec<CoreClosureParameter>,
    pub body: CoreBlock,
    pub span: Span,
    pub captures: Vec<Capture>,
    // Private source classification. It only restricts this resolved result;
    // Analyze and Verify independently inspect actual captures and values.
    pub owns_leaf_store: bool,
    pub defer_action: bool,
    pub inference: Inference,
    pub typed: Option<TypedFunction>,
    source_context: Option<source_context::Formation>,
    requires_source_context: bool,
}
impl Pending {
    pub fn callable_kind(&self) -> CallableKind {
        if self.defer_action || self.captures.iter().any(|capture| capture.consumed) {
            CallableKind::FnOnce
        } else if self.captures.iter().any(Capture::mutable_call) {
            CallableKind::FnMut
        } else {
            CallableKind::Fn
        }
    }
    pub fn owns_resources(&self) -> bool {
        self.captures.iter().any(|capture| {
            if matches!(capture.ty, Type::Managed(_)) {
                capture.moved
            } else {
                !capture.borrowed
                    && (matches!(capture.ty, Type::Package { .. } | Type::Closure(_))
                        || (capture.moved && owned_shapes::sum(&capture.ty)))
            }
        })
    }
}

fn tuple_pattern_takes_owner(
    pattern: &CorePattern,
    ty: &Type,
    work: &mut usize,
    depth: usize,
    depth_limit: usize,
) -> R<bool> {
    if depth > depth_limit {
        return Err(limit("owned Tuple pattern depth exceeded", pattern.span));
    }
    *work = work
        .checked_sub(1)
        .ok_or_else(|| limit("owned Tuple pattern budget exceeded", pattern.span))?;
    match (&pattern.kind, ty) {
        // UnpackTuple creates an owned hidden target for a discarded field;
        // its cleanup also consumes the captured environment's owner.
        (CorePatternKind::Name(_) | CorePatternKind::Wildcard, ty) => {
            Ok(!ty.is_copy_value() && dynamic_store::contains_inline_carrier(ty))
        }
        (CorePatternKind::Tuple(patterns), Type::Tuple(fields))
            if patterns.len() == fields.len() =>
        {
            let mut takes = false;
            for (pattern, field) in patterns.iter().zip(fields) {
                takes |= tuple_pattern_takes_owner(pattern, field, work, depth + 1, depth_limit)?;
            }
            Ok(takes)
        }
        // A reference binding keeps the child in its actual parent place.
        (CorePatternKind::Reference { .. }, _) => Ok(false),
        _ => Ok(false),
    }
}

pub(super) struct Input<'a> {
    pub defer_action: bool,
    pub move_capture: bool,
    pub parameters: &'a [CoreClosureParameter],
    pub result: Option<&'a Type>,
    pub body: &'a CoreBlock,
    pub span: Span,
}

pub(super) struct State {
    pub resource_writes: resource_effects::Writes,
    next_function: usize,
    pub helper_templates: BTreeMap<helpers::TemplateKey, helpers::Template>,
    pub active_helpers: std::collections::BTreeSet<helpers::TemplateKey>,
    pub helper_tables: std::rc::Rc<helpers::Tables>,
    pub helper_instances: BTreeMap<helpers::Instance, FunctionId>,
    pub forward_callers: BTreeSet<helpers::Instance>,
    pub forward_results: BTreeMap<FunctionId, u32>,
    pub store_instances: Vec<store_templates::Instance>,
    pub products: aggregate_templates::Products,
    // Private compiler provenance; public Core/Typed/Wire cannot supply it.
    pub internal_product_fields: BTreeMap<FunctionId, BTreeMap<(BlockId, usize), InstructionKind>>,
    pub helper_functions: Vec<TypedFunction>,
    pub helper_exports: Vec<helpers::Export>,
    pub pending: Vec<Pending>,
    // A converted closure owns a normal function slot, not an environment ID.
    // Reserve its slot before checking the body, which can create more entries.
    pub thin: Vec<Option<TypedFunction>>,
    limits: Limits,
    work_left: usize,
    nodes_left: usize,
    blocks: usize,
    locals: usize,
    values: usize,
    instructions: usize,
    source_contexts: source_context::Ledger,
}

impl State {
    pub(super) fn inserted_instruction(
        &mut self,
        function: FunctionId,
        block: BlockId,
        position: usize,
        span: Span,
    ) -> R<()> {
        if self.internal_product_fields.is_empty() {
            return Ok(());
        }
        self.snapshot(
            self.internal_product_fields.len().saturating_add(1).ilog2() as usize + 1,
            span,
        )?;
        let Some(receipts) = self.internal_product_fields.get(&function) else {
            return Ok(());
        };
        let count = receipts.len();
        self.snapshot(
            count
                .saturating_mul(count.saturating_add(1).ilog2() as usize + 3)
                .saturating_add(
                    self.internal_product_fields.len().saturating_add(1).ilog2() as usize + 1,
                ),
            span,
        )?;
        let receipts = self
            .internal_product_fields
            .get_mut(&function)
            .expect("existing receipts");
        *receipts = std::mem::take(receipts)
            .into_iter()
            .map(|((owner, index), kind)| {
                (
                    (
                        owner,
                        if owner == block && index >= position {
                            index + 1
                        } else {
                            index
                        },
                    ),
                    kind,
                )
            })
            .collect();
        Ok(())
    }

    // Private work-product receipts record only shared accounting, never proof.
    pub(super) fn accounting(&self) -> [usize; 6] {
        [
            self.blocks,
            self.locals,
            self.values,
            self.instructions,
            self.limits.max_analysis_steps - self.work_left,
            self.limits.max_nodes - self.nodes_left,
        ]
    }

    pub(super) fn replay_accounting(&mut self, cost: [usize; 6]) -> bool {
        let before = self.accounting();
        let maxima = [
            self.limits.max_blocks,
            self.limits.max_locals,
            self.limits.max_values,
            self.limits.max_instructions,
            self.limits.max_analysis_steps,
            self.limits.max_nodes,
        ];
        let mut after = [0; 6];
        for i in 0..6 {
            let Some(value) = before[i].checked_add(cost[i]).filter(|n| *n <= maxima[i]) else {
                return false;
            };
            after[i] = value;
        }
        [self.blocks, self.locals, self.values, self.instructions] =
            [after[0], after[1], after[2], after[3]];
        self.work_left = maxima[4] - after[4];
        self.nodes_left = maxima[5] - after[5];
        true
    }

    pub(super) fn factory_self_contained(&mut self, id: ClosureId, span: Span) -> R<()> {
        retained::self_contained(&self.pending, id, &mut self.work_left, span)
    }

    pub fn new(ordinary_functions: usize, limits: &Limits) -> Self {
        Self {
            resource_writes: BTreeMap::new(),
            next_function: ordinary_functions,
            helper_templates: BTreeMap::new(),
            active_helpers: std::collections::BTreeSet::new(),
            helper_tables: std::rc::Rc::new(BTreeMap::new()),
            helper_instances: BTreeMap::new(),
            forward_callers: BTreeSet::new(),
            forward_results: BTreeMap::new(),
            store_instances: Vec::new(),
            products: None,
            internal_product_fields: BTreeMap::new(),
            helper_functions: Vec::new(),
            helper_exports: Vec::new(),
            pending: vec![],
            thin: vec![],
            limits: limits.clone(),
            work_left: limits.max_analysis_steps,
            nodes_left: limits.max_nodes,
            blocks: 0,
            locals: 0,
            values: 0,
            instructions: 0,
            source_contexts: source_context::Ledger::default(),
        }
    }

    pub fn block(&mut self, span: Span) -> R<()> {
        charge(&mut self.blocks, self.limits.max_blocks, "block", span)
    }
    pub fn local(&mut self, span: Span) -> R<()> {
        charge(&mut self.locals, self.limits.max_locals, "local", span)
    }
    pub fn instruction(&mut self, value: bool, span: Span) -> R<()> {
        charge(
            &mut self.instructions,
            self.limits.max_instructions,
            "instruction",
            span,
        )?;
        if value {
            charge(&mut self.values, self.limits.max_values, "value", span)?;
        }
        Ok(())
    }
    pub fn snapshot(&mut self, entries: usize, span: Span) -> R<()> {
        self.work_left = self
            .work_left
            .checked_sub(entries)
            .ok_or_else(|| limit("closure lookup snapshot budget exceeded", span))?;
        Ok(())
    }

    pub(super) fn snapshot_type(
        &mut self,
        ty: &Type,
        copies: usize,
        extra: usize,
        span: Span,
    ) -> R<()> {
        self.snapshot(extra, span)?;
        inference::charge_type_snapshot(ty, span, &self.limits, &mut self.work_left, copies)
    }

    /// Retained factories cannot borrow the internal work ledger directly.
    /// Recheck a newly admitted complete Store parent on the same request.
    pub(super) fn ordinary_store_capture(&mut self, ty: &Type, span: Span) -> R<bool> {
        dynamic_store::owned_capture(ty, &mut self.work_left, &self.limits, span)
    }

    pub(super) fn source_helper_bounds(
        &mut self,
        function: &CoreFunction,
    ) -> R<Vec<CoreCallableParameter>> {
        inference::snapshot_callable_parameters(
            &function.callable_parameters,
            &self.limits,
            &mut self.work_left,
            function.span,
        )
    }
    pub(super) fn source_helper_context(
        &mut self,
        function: &CoreFunction,
        bound: &CoreCallableParameter,
    ) -> R<()> {
        helpers::source_life::reserve_function(
            function,
            bound,
            &self.limits,
            &mut self.work_left,
            2,
        )?;
        helpers::source_life::validate(function, bound)
    }
    pub(super) fn has_source_context(&self, id: ClosureId) -> bool {
        self.pending
            .get(id.0 as usize)
            .is_some_and(|pending| pending.source_context.is_some())
    }

    pub(super) fn source_helper_joint(
        &mut self,
        original: &CoreFunction,
        partials: &[(u32, StorageLifeContract)],
        span: Span,
    ) -> R<StorageLifeContract> {
        helpers::source_life::joint_header(original, partials, &mut self.work_left, span)
    }
    pub(super) fn source_helper_relation(
        &mut self,
        original: &CoreFunction,
        bound: &CoreCallableParameter,
        id: ClosureId,
        span: Span,
    ) -> R<(StorageLifeContract, StorageLifeContract)> {
        self.source_helper_context(original, bound)?;
        let pending = self.pending.get(id.0 as usize).ok_or_else(|| {
            error(
                "E-CALLABLE-CONTEXT",
                span,
                "source helper actual environment is absent",
            )
        })?;
        let typed = pending.typed.as_ref().ok_or_else(|| {
            error(
                "E-CALLABLE-CONTEXT",
                span,
                "source helper actual environment entry is unresolved",
            )
        })?;
        for capture in &pending.captures {
            inference::charge_type_snapshot(
                &capture.ty,
                span,
                &self.limits,
                &mut self.work_left,
                3,
            )?;
        }
        for local in &typed.parameters {
            inference::charge_type_snapshot(
                &typed.locals[local.0 as usize].ty,
                span,
                &self.limits,
                &mut self.work_left,
                3,
            )?;
        }
        inference::charge_type_snapshot(
            &typed.return_type,
            span,
            &self.limits,
            &mut self.work_left,
            3,
        )?;
        if let Some(life) = &typed.storage_life {
            self.work_left = self
                .work_left
                .checked_sub(borrowed_fp::metadata_work(life).saturating_mul(3))
                .ok_or_else(|| {
                    limit("source helper environment relation snapshot exceeded", span)
                })?;
        }
        let definition = ClosureDefinition {
            storage_life: typed.storage_life.clone(),
            id,
            function: typed.id,
            captures: pending.captures.iter().map(Capture::stored_type).collect(),
            parameters: typed
                .parameters
                .iter()
                .map(|local| typed.locals[local.0 as usize].ty.clone())
                .collect(),
            result: typed.return_type.clone(),
            kind: pending.callable_kind(),
            lending_captures: Vec::new(),
        };
        helpers::source_life::form(original, bound, &definition, &mut self.work_left, span)
    }

    pub(super) fn snapshot_helper(
        &mut self,
        function: &CoreFunction,
        copies: usize,
    ) -> R<(usize, bool)> {
        let allowance = self
            .work_left
            .checked_div(copies)
            .ok_or_else(|| limit("invalid helper snapshot multiplier", function.span))?;
        let (work, shapes) = inference::helper_snapshot_work(
            function,
            &Limits {
                max_analysis_steps: allowance,
                ..self.limits.clone()
            },
        )?;
        self.snapshot(work.saturating_mul(copies), function.span)?;
        Ok((work, shapes))
    }

    pub(super) fn snapshot_signature(
        &mut self,
        name: &str,
        signature: &Signature,
        span: Span,
    ) -> R<()> {
        let work = inference::signature_snapshot_work(
            name,
            signature,
            span,
            &Limits {
                max_analysis_steps: self.work_left / 2,
                ..self.limits.clone()
            },
        )?;
        self.snapshot(work.saturating_mul(2), span)
    }

    pub(super) fn snapshot_capture(
        &mut self,
        closure: ClosureId,
        index: usize,
        span: Span,
    ) -> R<()> {
        let capture = self
            .pending
            .get(closure.0 as usize)
            .and_then(|p| p.captures.get(index))
            .ok_or_else(|| {
                error(
                    "E-CLOSURE-INFERENCE",
                    span,
                    "missing actual capture snapshot",
                )
            })?;
        inference::charge_type_snapshot(&capture.ty, span, &self.limits, &mut self.work_left, 2)
    }

    pub(super) fn borrowed_task_result(
        &self,
        id: ClosureId,
        left: &mut usize,
        span: Span,
    ) -> R<Option<&Type>> {
        let Some(pending) = self.pending.get(id.0 as usize) else {
            return Ok(None);
        };
        pending
            .inference
            .graph
            .borrowed_task_type(pending.inference.result, left, span)
    }

    // Thread-only queries join this request's existing work counter. General
    // expression inference keeps its historical independent Graph profile.
    pub(super) fn thread_result(&mut self, id: ClosureId) -> R<Option<Type>> {
        let inference = &self.pending[id.0 as usize].inference;
        inference
            .graph
            .thread_query(&mut self.work_left, |graph| graph.ty(inference.result))
    }
    pub(super) fn thread_parameter(&mut self, id: ClosureId, index: usize) -> R<Option<Type>> {
        let inference = &self.pending[id.0 as usize].inference;
        inference.graph.thread_query(&mut self.work_left, |graph| {
            graph.ty(inference.parameters[index])
        })
    }
    pub(super) fn thread_expect_result(&mut self, id: ClosureId, ty: &Type, span: Span) -> R<()> {
        let inference = &mut self.pending[id.0 as usize].inference;
        let result = inference.result;
        inference.graph.thread_work(&mut self.work_left, |graph| {
            graph.bind(result, ty, span)?;
            graph.solve()
        })
    }

    pub fn allocate_function(&mut self, span: Span) -> R<FunctionId> {
        let index = self.next_function;
        if index >= self.limits.max_functions {
            return Err(limit("generated function budget exceeded", span));
        }
        self.next_function = index
            .checked_add(1)
            .ok_or_else(|| limit("generated function identity overflow", span))?;
        Ok(FunctionId(u32::try_from(index).map_err(|_| {
            limit("generated function identity overflow", span)
        })?))
    }

    pub fn create(
        &mut self,
        input: Input<'_>,
        outer: &BTreeMap<String, (LocalId, Type)>,
        signatures: &BTreeMap<String, Signature>,
        imports: Option<&program::Imports<'_>>,
        enums: Option<&BTreeMap<String, Type>>,
        declared: Option<&BTreeMap<LocalId, CallableKind>>,
    ) -> R<ClosureId> {
        let function = self.allocate_function(input.span)?;
        let id = ClosureId(
            u32::try_from(self.pending.len())
                .map_err(|_| limit("closure identity overflow", input.span))?,
        );
        // The lookup snapshot is bounded before its entries are visited. Only
        // referenced LocalIds become capture fields, in first-use source order.
        self.work_left = self
            .work_left
            .checked_sub(outer.len())
            .ok_or_else(|| limit("closure inference work budget exceeded", input.span))?;
        let mut checker = InferBuilder::new(
            &input,
            outer,
            Functions {
                signatures,
                imports,
                resource_writes: &self.resource_writes,
                closures: &self.pending,
                declared,
            },
            &self.limits,
            &mut self.work_left,
            &mut self.nodes_left,
        )?;
        checker.enums = enums;
        checker.block(input.body, 0)?;
        checker.graph.solve()?;
        package_effects::classify(
            input.body,
            input.parameters,
            &mut checker.captures,
            &checker.nested_package_moves,
            &self.limits,
            checker.work_left,
        )?;
        *checker.work_left = checker
            .work_left
            .checked_sub(checker.captures.len())
            .ok_or_else(|| limit("byte capture admission budget exceeded", input.span))?;
        let mut owns_leaf_store = false;
        let mut requires_source_context = false;
        for capture in &mut checker.captures {
            requires_source_context |= matches!(&capture.ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::BorrowedStorage { .. }));
            if dynamic_store::is_carrier(&capture.stored_type()) {
                if !capture.moved
                    || !dynamic_store::owned_capture(
                        &capture.ty,
                        checker.work_left,
                        &self.limits,
                        capture.span,
                    )?
                {
                    return Err(unsupported(
                        capture.span,
                        "Store capture requires an owned carrier with finite leaf Package elements",
                    ));
                }
                owns_leaf_store = true;
            }
            if packages::has_store_fields(&capture.stored_type()) {
                if !capture.moved
                    || !dynamic_store::owned_capture(
                        &capture.ty,
                        checker.work_left,
                        &self.limits,
                        capture.span,
                    )?
                {
                    return Err(unsupported(
                        capture.span,
                        "Package capture with Store fields requires a complete owning leaf Store transfer",
                    ));
                }
                owns_leaf_store = true;
            }
            if owned_shapes::sum(&capture.stored_type())
                && dynamic_store::owned_capture(
                    &capture.ty,
                    checker.work_left,
                    &self.limits,
                    capture.span,
                )?
            {
                if !capture.moved || capture.borrowed {
                    return Err(unsupported(
                        capture.span,
                        "owned Sum capture requires an explicit complete move",
                    ));
                }
                owns_leaf_store = true;
            }
            if packages::has_array_fields(&capture.stored_type()) {
                return Err(unsupported(
                    capture.span,
                    "Package capture with owning array fields requires a retained nested-field protocol",
                ));
            }
            if packages::owned_array_supported(&capture.stored_type()) {
                return Err(unsupported(
                    capture.span,
                    "owned array capture requires a separate retained array protocol",
                ));
            }
            if let Type::Closure(id) = capture.stored_type() {
                retained::self_contained(&self.pending, id, checker.work_left, capture.span)?;
                if self.pending[id.0 as usize].owns_leaf_store {
                    *checker.work_left = checker.work_left.checked_sub(1).ok_or_else(|| {
                        limit("owned Store child profile budget exceeded", capture.span)
                    })?;
                    owns_leaf_store = true;
                }
            }
            if matches!(capture.ty, Type::Package { .. }) && !capture.moved {
                // Resolve the complete body's ownership uses before choosing
                // a shared window. Reading a child path must not take it, and
                // a later real take must not be hidden by an earlier read.
                packages::product::validate(
                    &capture.ty,
                    checker.work_left,
                    self.limits.max_call_depth,
                    capture.span,
                )?;
                if capture.written || capture.consumed {
                    return Err(unsupported(
                        capture.span,
                        "mutable Product capture is not implemented",
                    ));
                }
                capture.borrowed = true;
            }
            // A local byte-storage Desc uses the same actual capture-source
            // and dynamic access checks as a direct synchronous operation.
            // Retained results still independently require self containment.
        }
        if !checker.has_return {
            checker
                .graph
                .bind(checker.result, &Type::Unit, input.span)?;
        }
        let inference = Inference {
            graph: checker.graph,
            parameters: checker.parameters,
            result: checker.result,
        };
        self.pending.push(Pending {
            id,
            function,
            parameters: input.parameters.to_vec(),
            body: input.body.clone(),
            span: input.span,
            captures: checker.captures,
            owns_leaf_store,
            defer_action: input.defer_action,
            inference,
            typed: None,
            source_context: None,
            requires_source_context,
        });
        Ok(id)
    }

    pub fn take_thin(&mut self, id: ClosureId, span: Span) -> R<(usize, Pending)> {
        let pending = self.pending.last().ok_or_else(|| {
            error(
                "E-CLOSURE-INFERENCE",
                span,
                "missing closure conversion entry",
            )
        })?;
        if pending.id != id || pending.typed.is_some() {
            return Err(error(
                "E-CLOSURE-INFERENCE",
                span,
                "only the fresh closure entry can become a normal function",
            ));
        }
        if !pending.captures.is_empty() {
            return Err(error(
                "E-CAPTURED-FN-POINTER",
                span,
                "function pointer conversion requires a closure with no captures",
            ));
        }
        let pending = self.pending.pop().expect("checked fresh entry");
        let slot = self.thin.len();
        self.thin.push(None);
        Ok((slot, pending))
    }

    pub(super) fn require_source_context(&self, id: ClosureId, span: Span) -> R<()> {
        let pending = &self.pending[id.0 as usize];
        if pending.requires_source_context && pending.source_context.is_none() {
            return Err(unsupported(
                span,
                "qualified capture requires its genuine actual creator context",
            ));
        }
        Ok(())
    }
    pub(super) fn remaining_work(&self) -> usize {
        self.work_left
    }

    pub fn finish(mut self) -> R<(Vec<ClosureDefinition>, Vec<TypedFunction>, usize)> {
        self.finish_source_contexts()?;
        let mut remaining = self.work_left;
        let mut definitions = Vec::new();
        let mut functions = Vec::new();
        for mut pending in self.pending {
            let (parameters, result) =
                pending
                    .inference
                    .resolve_shared(true, pending.span, &mut remaining)?;
            remaining = remaining
                .checked_sub(pending.captures.len().saturating_add(1))
                .ok_or_else(|| limit("completed closure metadata budget exceeded", pending.span))?;
            for capture in &pending.captures {
                pending
                    .inference
                    .graph
                    .thread_query(&mut remaining, |graph| {
                        graph.reserve_type(&capture.ty, 0, capture.span)
                    })?;
            }
            let kind = pending.callable_kind();
            let typed = pending.typed.ok_or_else(|| {
                error(
                    "E-CLOSURE-INFERENCE",
                    pending.span,
                    "closure body was not resolved",
                )
            })?;
            let captures = pending
                .captures
                .iter()
                .map(Capture::stored_type)
                .collect::<Vec<_>>();
            if let Some(life) = &typed.storage_life {
                remaining = remaining
                    .checked_sub(
                        borrowed_fp::metadata_work(life)
                            .saturating_add(parameters.len())
                            .saturating_add(captures.len())
                            .saturating_add(1),
                    )
                    .ok_or_else(|| {
                        limit("completed capture relation budget exceeded", pending.span)
                    })?;
                borrowed_fp::qualified_inputs(life, &parameters, &result, |work| {
                    remaining = remaining.checked_sub(work).ok_or_else(|| {
                        limit("completed parameter class budget exceeded", pending.span)
                    })?;
                    Ok(())
                })?;
                borrowed_fp::qualified_inputs(life, &captures, &Type::Unit, |work| {
                    remaining = remaining.checked_sub(work).ok_or_else(|| {
                        limit("completed capture class budget exceeded", pending.span)
                    })?;
                    Ok(())
                })?;
                if pending.source_context.is_some() {
                    for capture in &captures {
                        if let Type::Managed(payload) = capture
                            && let Type::Record { fields, .. } = payload.as_ref()
                        {
                            remaining = remaining
                                .checked_sub(fields.len().saturating_mul(2))
                                .ok_or_else(|| {
                                    limit(
                                        "owned source capture classification work exceeded",
                                        pending.span,
                                    )
                                })?;
                        }
                    }
                    borrowed_fp_qualified::source_capture_relation(
                        &parameters,
                        &captures,
                        if matches!(&result, Type::Address { mutable:false, inner } if inner.is_b0_scalar())
                        {
                            &Type::Unit
                        } else {
                            &result
                        },
                        life,
                        pending.span,
                    )?;
                } else {
                    borrowed_fp_qualified::capture_relation(
                        &parameters,
                        &captures,
                        &result,
                        life,
                        pending.span,
                    )?;
                }
            }
            definitions.push(ClosureDefinition {
                storage_life: typed.storage_life.clone(),
                lending_captures: Vec::new(),
                id: pending.id,
                function: pending.function,
                captures,
                parameters,
                result,
                kind,
            });
            functions.push(typed);
        }
        for function in self.thin {
            functions.push(function.ok_or_else(|| {
                error(
                    "E-CLOSURE-INFERENCE",
                    Span::default(),
                    "function pointer body was not resolved",
                )
            })?);
        }
        functions.extend(self.helper_functions);
        functions.sort_by_key(|function| function.id);
        Ok((definitions, functions, remaining))
    }
}

fn charge(used: &mut usize, maximum: usize, what: &str, span: Span) -> R<()> {
    *used = used
        .checked_add(1)
        .filter(|n| *n <= maximum)
        .ok_or_else(|| limit(format!("project {what} budget exceeded"), span))?;
    Ok(())
}

fn limit(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::limit(Phase::Types, message, Some(span))
}

pub(super) type Variable = usize;
pub(super) fn signature_parameter(ty: &Type) -> bool {
    ty.is_copy_value()
        || ty.is_shared_sync_cell_address()
        || stored::shared_address(ty)
        || matches!(ty, Type::Address { inner, .. } if inner.is_address_pointee() || owned_shapes::tuple(inner) || owned_shapes::enumeration(inner) || errors::owned(inner))
}

pub(super) fn supported_signature(parameters: &[Type], result: &Type) -> bool {
    parameters.iter().all(signature_parameter)
        && (result.is_copy_value()
            || (borrowed_fp::invocation(parameters, result)
                && matches!(result,Type::Address{inner,..} if inner.is_address_pointee()))
            || (parameters.iter().all(Type::is_copy_value) && lending::supported_result(result)))
}
/// Ordinary owned data signatures are deliberately separate from the retained
/// factory, State and address profiles. This grants no capture/container right.
pub(super) fn owned_signature_value(ty: &Type) -> bool {
    fn data(ty: &Type) -> bool {
        if ty.is_copy_value() {
            return true;
        }
        match ty {
            Type::Package { fields, .. } => fields.iter().all(|(_, ty)| data(ty)),
            Type::Enum { variants, .. } => {
                variants.iter().all(|(_, fields)| fields.iter().all(data))
            }
            Type::ErrorUnion { error, result } => data(error) && data(result),
            Type::Managed(payload) => match payload.as_ref() {
                Type::OwnedStore { element } => data(element),
                Type::ByteBuffer => true,
                payload => payload.is_copy_value(),
            },
            _ => false,
        }
    }
    ty.is_copy_value()
        || (matches!(
            ty,
            Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
        ) && owned_shapes::value(ty)
            && data(ty))
}

pub(super) fn owned_signature_head(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Package { .. } | Type::Enum { .. } | Type::ErrorUnion { .. }
    )
}

pub(super) fn value_signature(parameters: &[Type], result: &Type) -> bool {
    supported_signature(parameters, result)
        || (parameters.iter().all(owned_signature_value) && owned_signature_value(result))
}
pub(super) const UNIT: u16 = 1;
pub(super) const BOOL: u16 = 2;
pub(super) const I32: u16 = 4;
pub(super) const I64: u16 = 8;
pub(super) const U8: u16 = 32;
pub(super) const U16: u16 = 64;
pub(super) const HANDLE: u16 = 128;
pub(super) const U64: u16 = 256;
pub(super) const USIZE: u16 = 512;
pub(super) const INTEGER: u16 = U8 | U16 | U64 | USIZE | I32 | I64;
pub(super) const F32: u16 = 1024;
pub(super) const F64: u16 = 2048;
pub(super) const FLOAT: u16 = F32 | F64;
pub(super) const NUMBER: u16 = INTEGER | FLOAT;
pub(super) const SCALAR: u16 = UNIT | BOOL | NUMBER | HANDLE;
const AGGREGATE: u16 = 16;
pub(super) const VALUE: u16 = SCALAR | AGGREGATE;

#[derive(Clone)]
enum Shape {
    Tuple(Vec<Variable>),
    Array {
        element: Variable,
        length: u32,
    },
    Slice {
        element: Variable,
    },
    Address {
        inner: Variable,
        mutable: Option<bool>,
    },
    FunctionPointer {
        parameters: Vec<Variable>,
        result: Variable,
    },
}
#[derive(Clone)]
pub(super) enum Projection {
    Field(String),
    Element,
}
struct Node {
    parent: Variable,
    rank: u8,
    domain: u16,
    literal: bool,
    concrete: Option<Type>,
    shape: Option<Shape>,
}
pub(super) struct Graph {
    nodes: Vec<Node>,
    projections: Vec<(Variable, Projection, Variable, Span)>,
    work_left: std::cell::Cell<usize>,
    depth_limit: usize,
    node_limit: usize,
    store_parameter: Option<String>,
    source_callback_parameters: Vec<String>,
}
impl Graph {
    pub(super) fn new(limits: &Limits) -> Self {
        Self {
            nodes: vec![],
            projections: vec![],
            work_left: std::cell::Cell::new(limits.max_analysis_steps),
            depth_limit: limits.max_call_depth,
            node_limit: limits.max_nodes,
            store_parameter: None,
            source_callback_parameters: Vec::new(),
        }
    }
    // A graph-only operation leases at most the current shared remainder.
    // Always debit actual work, including partial work before an error, while
    // preserving the Graph's own historical quota minus that same work.
    fn thread_query<T>(&self, remaining: &mut usize, query: impl FnOnce(&Self) -> R<T>) -> R<T> {
        let original = self.work_left.get();
        let allowance = original.min(*remaining);
        self.work_left.set(allowance);
        let result = query(self);
        self.thread_finish(original, allowance, remaining, result)
    }
    fn thread_work<T>(
        &mut self,
        remaining: &mut usize,
        query: impl FnOnce(&mut Self) -> R<T>,
    ) -> R<T> {
        let original = self.work_left.get();
        let allowance = original.min(*remaining);
        self.work_left.set(allowance);
        let result = query(self);
        self.thread_finish(original, allowance, remaining, result)
    }
    fn thread_finish<T>(
        &self,
        original: usize,
        allowance: usize,
        remaining: &mut usize,
        result: R<T>,
    ) -> R<T> {
        let spent = allowance - self.work_left.get();
        *remaining -= spent;
        self.work_left.set(original - spent);
        result
    }

    pub(super) fn set_store_parameter(&mut self, name: &str, span: Span) -> R<()> {
        self.spend_count(name.len().saturating_add(1), span)?;
        self.store_parameter = Some(name.to_owned());
        Ok(())
    }
    pub(super) fn symbolic_store_type(&self, ty: &Type) -> bool {
        self.store_parameter
            .as_ref()
            .is_some_and(|name| store_templates::symbolic_type(ty, name))
    }
    pub(super) fn is_store_parameter(&self, ty: &Type) -> bool {
        matches!(ty, Type::Named(name) if self.store_parameter.as_ref() == Some(name))
    }
    pub(super) fn source_callback_parameter(&mut self, name: &str, span: Span) -> R<()> {
        self.spend_count(
            self.source_callback_parameters
                .len()
                .saturating_mul(name.len().saturating_add(1))
                .saturating_add(name.len())
                .saturating_add(1),
            span,
        )?;
        if !self
            .source_callback_parameters
            .iter()
            .any(|old| old == name)
        {
            if self.source_callback_parameters.len() >= 2 {
                return Err(unsupported(
                    span,
                    "source callback parameter bound exceeded",
                ));
            }
            self.source_callback_parameters.push(name.to_owned());
        }
        Ok(())
    }
    fn source_callback_package(&self, ty: &Type) -> bool {
        !self.source_callback_parameters.is_empty()
            && dynamic_store::callback_element(
                ty,
                |ty| matches!(ty, Type::Named(name) if self.source_callback_parameters.contains(name)),
            )
    }
    fn spend(&self, span: Span) -> R<()> {
        self.spend_count(1, span)
    }
    fn spend_count(&self, count: usize, span: Span) -> R<()> {
        self.work_left.set(
            self.work_left
                .get()
                .checked_sub(count)
                .ok_or_else(|| limit("closure aggregate constraint budget exceeded", span))?,
        );
        Ok(())
    }
    fn reserve_type_uses(&self, ty: &Type, uses: usize, span: Span) -> R<()> {
        let original = self.work_left.get();
        let allowance = original
            .checked_div(uses)
            .ok_or_else(|| limit("invalid type formation reservation", span))?;
        self.work_left.set(allowance);
        let result = self.reserve_type(ty, 0, span);
        let consumed = allowance - self.work_left.get();
        // consumed <= original / uses; arithmetic cannot exceed original.
        self.work_left.set(original - consumed * uses);
        result
    }
    pub(super) fn reserve_type(&self, ty: &Type, depth: usize, span: Span) -> R<()> {
        tasks::require_closed_context(ty, span)?;
        self.spend(span)?;
        if depth > self.depth_limit {
            return Err(limit("closure aggregate type depth exceeded", span));
        }
        match ty {
            Type::BorrowedStorage { storage, life } => {
                self.spend_count(
                    life.slots
                        .len()
                        .saturating_add(life.retained.len())
                        .saturating_add(life.nominal_arguments.len()),
                    span,
                )?;
                for slot in &life.slots {
                    self.spend_count(slot.path.len(), span)?;
                }
                for row in &life.retained {
                    self.spend_count(row.path.len(), span)?;
                }
                for row in &life.nominal_arguments {
                    self.spend_count(row.path.len(), span)?;
                }
                self.reserve_type(storage, depth + 1, span)?;
            }

            Type::Tuple(fields) => {
                for field in fields {
                    self.reserve_type(field, depth + 1, span)?;
                }
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                self.spend_count(name.len(), span)?;
                for (name, field) in fields {
                    self.spend_count(name.len(), span)?;
                    self.reserve_type(field, depth + 1, span)?;
                }
            }
            Type::Enum { name, variants } => {
                self.spend_count(name.len(), span)?;
                for (name, fields) in variants {
                    self.spend_count(name.len(), span)?;
                    for field in fields {
                        self.reserve_type(field, depth + 1, span)?;
                    }
                }
            }
            Type::Task {
                result: element, ..
            }
            | Type::Array { element, .. }
            | Type::Slice { element }
            | Type::Registration {
                environment: element,
            }
            | Type::AsyncRegistration {
                environment: element,
            }
            | Type::OwnedStore { element }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element)
            | Type::Opaque {
                hidden: element, ..
            } => self.reserve_type(element, depth + 1, span)?,
            Type::ErrorUnion { error, result } => {
                self.reserve_type(error, depth + 1, span)?;
                self.reserve_type(result, depth + 1, span)?;
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for parameter in parameters {
                    self.reserve_type(parameter, depth + 1, span)?;
                }
                self.reserve_type(result, depth + 1, span)?;
            }
            Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                self.spend_count(borrowed_fp::metadata_work(life), span)?;
                for parameter in parameters {
                    self.reserve_type(parameter, depth + 1, span)?;
                }
                self.reserve_type(result, depth + 1, span)?;
                borrowed_fp::qualified_inputs(life, parameters, result, |work| {
                    self.spend_count(work, span)
                })?;
                borrowed_fp::relation(parameters, result, life, span)?;
            }
            Type::Named(name) | Type::Unsupported(name) => self.spend_count(name.len(), span)?,
            _ => {}
        }
        Ok(())
    }
    /// Core types may already contain inferred local storage binders. Source
    /// lexical annotations are checked in Expand, before that distinction is
    /// erased. A callable's free Fixed domain still needs an enclosing context.
    pub(super) fn annotation_context(&self, ty: &Type, depth: usize, span: Span) -> R<()> {
        self.spend(span)?;
        if depth > self.depth_limit {
            return Err(limit("closure annotation depth exceeded", span));
        }
        match ty {
            Type::BorrowedStorage { storage, .. } => {
                // StorageLifeShape binders are local to this value, not proof
                // of a reference to an outer source definition. Inferred Core
                // passes through this walk again before ordinary Types.
                self.annotation_context(storage, depth + 1, span)?;
            }
            Type::BorrowedFunctionPointer { life, .. } => {
                // Its Invocation domain belongs to the code signature. A free
                // Fixed domain still needs an actual retained outer context.
                if life.fixed_binder_count != 0 {
                    return Err(unsupported(
                        span,
                        "explicit callable closure annotation requires its retained enclosing life context",
                    ));
                }
            }
            Type::Tuple(fields) => {
                for field in fields {
                    self.annotation_context(field, depth + 1, span)?;
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, field) in fields {
                    self.annotation_context(field, depth + 1, span)?;
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    self.spend(span)?;
                    for field in fields {
                        self.annotation_context(field, depth + 1, span)?;
                    }
                }
            }
            Type::Array { element, .. }
            | Type::Slice { element }
            | Type::OwnedStore { element }
            | Type::Task {
                result: element, ..
            }
            | Type::Registration {
                environment: element,
            }
            | Type::AsyncRegistration {
                environment: element,
            }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element)
            | Type::Opaque {
                hidden: element, ..
            } => {
                self.annotation_context(element, depth + 1, span)?;
            }
            Type::ErrorUnion { error, result } => {
                self.annotation_context(error, depth + 1, span)?;
                self.annotation_context(result, depth + 1, span)?;
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                for parameter in parameters {
                    self.annotation_context(parameter, depth + 1, span)?;
                }
                self.annotation_context(result, depth + 1, span)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn clone_type(&self, ty: &Option<Type>, span: Span) -> R<Option<Type>> {
        if let Some(ty) = ty {
            self.reserve_type(ty, 0, span)?;
        }
        Ok(ty.clone())
    }
    fn clone_shape(&self, shape: &Option<Shape>, span: Span) -> R<Option<Shape>> {
        let count = match shape {
            Some(Shape::Tuple(fields)) => fields.len(),
            Some(Shape::FunctionPointer { parameters, .. }) => parameters.len() + 1,
            Some(Shape::Array { .. } | Shape::Slice { .. } | Shape::Address { .. }) => 1,
            None => 0,
        };
        self.spend_count(count, span)?;
        Ok(shape.clone())
    }
    pub(super) fn len(&self) -> usize {
        self.nodes.len()
    }
    pub(super) fn is_slice(&self, variable: Variable) -> bool {
        let node = &self.nodes[self.root(variable)];
        matches!(node.shape, Some(Shape::Slice { .. }))
            || matches!(node.concrete, Some(Type::Slice { .. }))
    }
    pub(super) fn fresh(&mut self, domain: u16, literal: bool) -> Variable {
        let variable = self.nodes.len();
        self.nodes.push(Node {
            parent: variable,
            rank: 0,
            domain,
            literal,
            concrete: None,
            shape: None,
        });
        variable
    }
    fn borrowed_task_type(
        &self,
        mut variable: Variable,
        left: &mut usize,
        span: Span,
    ) -> R<Option<&Type>> {
        loop {
            *left = left
                .checked_sub(1)
                .ok_or_else(|| limit("task closure hint work budget exceeded", span))?;
            let node = &self.nodes[variable];
            if node.parent == variable {
                return Ok(node.concrete.as_ref());
            }
            variable = node.parent;
        }
    }
    fn root(&self, mut variable: Variable) -> Variable {
        while self.nodes[variable].parent != variable {
            variable = self.nodes[variable].parent;
        }
        variable
    }
    pub(super) fn constrain(&mut self, variable: Variable, domain: u16, span: Span) -> R<()> {
        self.spend(span)?;
        let root = self.root(variable);
        let narrowed = self.nodes[root].domain & domain;
        if narrowed == 0 {
            return Err(error(
                "E-TYPE-MISMATCH",
                span,
                "conflicting closure type constraints",
            ));
        }
        self.nodes[root].domain = narrowed;
        Ok(())
    }
    pub(super) fn bind(&mut self, variable: Variable, ty: &Type, span: Span) -> R<()> {
        self.bind_source(variable, ty, None, span)
    }
    pub(super) fn bind_source(
        &mut self,
        variable: Variable,
        ty: &Type,
        source_callbacks: Option<&BTreeMap<String, usize>>,
        span: Span,
    ) -> R<()> {
        self.reserve_type(ty, 0, span)?;
        let domain = match ty {
            Type::Unit => UNIT,
            Type::Bool => BOOL,
            Type::U8 => U8,
            Type::U16 => U16,
            Type::ForeignHandle => HANDLE,
            Type::I32 => I32,
            Type::I64 => I64,
            Type::F32 => F32,
            Type::F64 => F64,
            Type::U64 => U64,
            Type::Usize => USIZE,
            ty if self.symbolic_store_type(ty) => AGGREGATE,
            ty if self.source_callback_package(ty) => AGGREGATE,
            ty if matches!(ty, Type::TaskI32 | Type::Task { .. }) || ty.is_exclusive_task_head_address() || resources::head_address(ty) || matches!(ty, Type::SyncCell(_)) || ty.is_shared_sync_cell_address() || *ty == Type::ByteBuffer
                || matches!(ty, Type::OwnedStore { .. } | Type::Registration { .. } | Type::AsyncRegistration { .. })
                || ty.is_copy_value()
                || stored::shared_value(ty)
                || stored::shared_address(ty)
                || errors::owned(ty)
                || owned_shapes::enumeration(ty)
                || owned_shapes::tuple(ty)
                || owned_shapes::sum(ty)
                || packages::ordinary(ty)
                || packages::owned_array_supported(ty)
                || lending::supported_result(ty)
                || matches!(ty, Type::Address { inner, .. } if inner.is_address_pointee() || owned_shapes::tuple(inner) || owned_shapes::enumeration(inner) || errors::owned(inner))
                // This is only a pointee constraint node for a shared head.
                // It does not make a Managed value Copy or grant a capture.
                || matches!(ty, Type::Managed(payload)
                    if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value())
                // A variant/error arm can bind the actual owned Store leaf.
                // The pattern still takes its owning Sum in the Builder; this
                // graph node only carries the already resolved leaf type.
                || matches!(ty, Type::Managed(_)
                    if dynamic_store::element(ty).is_some_and(|element|
                        dynamic_store::inline_element(element)
                            && dynamic_store::supported_element(element)))
                || matches!(ty, Type::Slice { element } if element.is_copy_value()) =>
            {
                AGGREGATE
            }
            _ => {
                if let Some(callbacks) = source_callbacks {
                    // Only the new unresolved shell pays for this extra walk.
                    // Ordinary concrete/Copy paths returned above unchanged.
                    self.reserve_type(ty, 0, span)?;
                    if dynamic_store::callback_element(
                        ty,
                        |ty| matches!(ty, Type::Named(name) if callbacks.contains_key(name)),
                    ) {
                        for name in callbacks.keys() {
                            self.source_callback_parameter(name, span)?;
                        }
                        return self.bind(variable, ty, span);
                    }
                }
                return Err(unsupported(
                    span,
                    "closure value requires a finite Copy type",
                ));
            }
        };
        self.constrain(variable, domain, span)?;
        if domain == AGGREGATE {
            let root = self.root(variable);
            if self.nodes[root]
                .concrete
                .as_ref()
                .is_some_and(|known| known != ty)
            {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "different concrete aggregate types",
                ));
            }
            let shape = self.clone_shape(&self.nodes[root].shape, span)?;
            match (shape, ty) {
                (Some(Shape::Tuple(variables)), Type::Tuple(fields))
                    if variables.len() == fields.len() =>
                {
                    for (v, t) in variables.into_iter().zip(fields) {
                        self.bind(v, t, span)?;
                    }
                }
                (
                    Some(Shape::Array { element, length }),
                    Type::Array {
                        element: ty_element,
                        length: ty_length,
                    },
                ) if length == *ty_length => self.bind(element, ty_element, span)?,
                (
                    Some(Shape::Slice { element }),
                    Type::Slice {
                        element: ty_element,
                    },
                ) => self.bind(element, ty_element, span)?,
                (
                    Some(Shape::Address { inner, mutable }),
                    Type::Address {
                        mutable: actual_mutable,
                        inner: actual,
                    },
                ) if mutable.is_none_or(|wanted| wanted == *actual_mutable) => {
                    self.bind(inner, actual, span)?;
                    self.nodes[root].shape = Some(Shape::Address {
                        inner,
                        mutable: Some(*actual_mutable),
                    });
                }
                (
                    Some(Shape::FunctionPointer { parameters, result }),
                    Type::FunctionPointer {
                        parameters: types,
                        result: result_type,
                    }
                    | Type::BorrowedFunctionPointer {
                        parameters: types,
                        result: result_type,
                        ..
                    },
                ) if parameters.len() == types.len() => {
                    for (parameter, ty) in parameters.into_iter().zip(types) {
                        self.bind(parameter, ty, span)?;
                    }
                    self.bind(result, result_type, span)?;
                }
                (Some(_), _) => {
                    return Err(error(
                        "E-TYPE-MISMATCH",
                        span,
                        "aggregate shape differs from its context",
                    ));
                }
                (None, _) => {}
            }
            self.nodes[root].concrete = Some(ty.clone());
        }
        Ok(())
    }
    fn occurs(&mut self, needle: Variable, base: Variable, span: Span) -> R<bool> {
        let mut pending = vec![base];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(v) = pending.pop() {
            self.spend(span)?;
            let v = self.root(v);
            if v == needle {
                return Ok(true);
            }
            if !seen.insert(v) {
                continue;
            }
            match &self.nodes[v].shape {
                Some(Shape::Tuple(fields)) => {
                    self.spend_count(fields.len(), span)?;
                    pending.extend(fields);
                }
                Some(Shape::Array { element, .. } | Shape::Slice { element }) => {
                    pending.push(*element)
                }
                Some(Shape::Address { inner, .. }) => pending.push(*inner),
                Some(Shape::FunctionPointer { parameters, result }) => {
                    self.spend_count(parameters.len() + 1, span)?;
                    pending.extend(parameters);
                    pending.push(*result);
                }
                None => {}
            }
        }
        Ok(false)
    }
    pub(super) fn unify(&mut self, a: Variable, b: Variable, span: Span) -> R<()> {
        self.unify_depth(a, b, span, 0)
    }
    fn unify_depth(&mut self, a: Variable, b: Variable, span: Span, depth: usize) -> R<()> {
        self.spend(span)?;
        if depth > self.depth_limit {
            return Err(limit("closure constraint unification depth exceeded", span));
        }
        let mut a = self.root(a);
        let mut b = self.root(b);
        if a == b {
            return Ok(());
        }
        let domain = self.nodes[a].domain & self.nodes[b].domain;
        if domain == 0 {
            return Err(error(
                "E-TYPE-MISMATCH",
                span,
                "conflicting closure type constraints",
            ));
        }
        if self.occurs(a, b, span)? || self.occurs(b, a, span)? {
            return Err(error(
                "E-INFINITE-LAYOUT",
                span,
                "recursive aggregate type constraint",
            ));
        }
        let a_concrete = self.clone_type(&self.nodes[a].concrete, span)?;
        let b_concrete = self.clone_type(&self.nodes[b].concrete, span)?;
        if a_concrete.is_some() && b_concrete.is_some() && a_concrete != b_concrete {
            return Err(error(
                "E-TYPE-MISMATCH",
                span,
                "different concrete aggregate types",
            ));
        }
        let mut a_shape = self.clone_shape(&self.nodes[a].shape, span)?;
        let b_shape = self.clone_shape(&self.nodes[b].shape, span)?;
        match (&a_shape, &b_shape) {
            (Some(Shape::Tuple(left)), Some(Shape::Tuple(right))) if left.len() == right.len() => {
                for (a, b) in left.iter().zip(right) {
                    self.unify_depth(*a, *b, span, depth + 1)?;
                }
            }
            (
                Some(Shape::Array {
                    element: a,
                    length: n,
                }),
                Some(Shape::Array {
                    element: b,
                    length: m,
                }),
            ) if n == m => self.unify_depth(*a, *b, span, depth + 1)?,
            (Some(Shape::Slice { element: a }), Some(Shape::Slice { element: b })) => {
                self.unify_depth(*a, *b, span, depth + 1)?
            }
            (
                Some(Shape::Address {
                    inner: a,
                    mutable: am,
                }),
                Some(Shape::Address {
                    inner: b,
                    mutable: bm,
                }),
            ) if am.is_none() || bm.is_none() || am == bm => {
                self.unify_depth(*a, *b, span, depth + 1)?;
                a_shape = Some(Shape::Address {
                    inner: *a,
                    mutable: am.or(*bm),
                });
            }
            (
                Some(Shape::FunctionPointer {
                    parameters: left,
                    result: a,
                }),
                Some(Shape::FunctionPointer {
                    parameters: right,
                    result: b,
                }),
            ) if left.len() == right.len() => {
                for (left, right) in left.iter().zip(right) {
                    self.unify_depth(*left, *right, span, depth + 1)?;
                }
                self.unify_depth(*a, *b, span, depth + 1)?;
            }
            (Some(_), Some(_)) => {
                return Err(error("E-TYPE-MISMATCH", span, "different aggregate shapes"));
            }
            _ => {}
        }
        if self.nodes[a].rank < self.nodes[b].rank {
            std::mem::swap(&mut a, &mut b);
        }
        self.nodes[b].parent = a;
        self.nodes[a].domain = domain;
        self.nodes[a].literal |= self.nodes[b].literal;
        self.nodes[a].shape = a_shape.or(b_shape);
        self.nodes[a].concrete = a_concrete.or(b_concrete);
        if self.nodes[a].rank == self.nodes[b].rank {
            self.nodes[a].rank += 1;
        }
        if let Some(ty) = self.clone_type(&self.nodes[a].concrete, span)? {
            self.bind(a, &ty, span)?;
        }
        Ok(())
    }
    pub(super) fn tuple(&mut self, result: Variable, fields: Vec<Variable>, span: Span) -> R<()> {
        self.constrain(result, AGGREGATE, span)?;
        let root = self.root(result);
        self.nodes[root].shape = Some(Shape::Tuple(fields));
        Ok(())
    }
    pub(super) fn array(
        &mut self,
        result: Variable,
        element: Variable,
        length: u32,
        span: Span,
    ) -> R<()> {
        self.constrain(result, AGGREGATE, span)?;
        let root = self.root(result);
        self.nodes[root].shape = Some(Shape::Array { element, length });
        Ok(())
    }
    pub(super) fn slice(&mut self, result: Variable, element: Variable, span: Span) -> R<()> {
        self.constrain(result, AGGREGATE, span)?;
        let root = self.root(result);
        self.nodes[root].shape = Some(Shape::Slice { element });
        Ok(())
    }
    /// Inference shapes carry no storage ownership or lifetime proof. An
    /// unknown permission is resolved by the signature/call constraints.
    pub(super) fn address(
        &mut self,
        result: Variable,
        inner: Variable,
        mutable: Option<bool>,
        span: Span,
    ) -> R<()> {
        self.constrain(result, AGGREGATE, span)?;
        let root = self.root(result);
        match self.clone_shape(&self.nodes[root].shape, span)? {
            Some(Shape::Address {
                inner: previous,
                mutable: prior,
            }) if prior.is_none() || mutable.is_none() || prior == mutable => {
                self.unify(previous, inner, span)?;
                self.nodes[root].shape = Some(Shape::Address {
                    inner: previous,
                    mutable: prior.or(mutable),
                });
            }
            Some(_) => {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "address shape or permission differs from its context",
                ));
            }
            None => {
                if self.occurs(root, inner, span)? {
                    return Err(error(
                        "E-INFINITE-LAYOUT",
                        span,
                        "recursive address constraint",
                    ));
                }
                self.nodes[root].shape = Some(Shape::Address { inner, mutable });
            }
        }
        if let Some(ty) = self.clone_type(&self.nodes[root].concrete, span)? {
            self.bind(root, &ty, span)?;
        }
        Ok(())
    }

    pub(super) fn address_shape(
        &self,
        variable: Variable,
        span: Span,
    ) -> R<Option<(Variable, Option<bool>)>> {
        self.spend(span)?;
        Ok(match self.nodes[self.root(variable)].shape {
            Some(Shape::Address { inner, mutable }) => Some((inner, mutable)),
            _ => None,
        })
    }

    pub(super) fn shared_address(
        &mut self,
        result: Variable,
        inner: Variable,
        span: Span,
    ) -> R<()> {
        self.address(result, inner, Some(false), span)
    }
    pub(super) fn function_pointer(
        &mut self,
        callee: Variable,
        parameters: Vec<Variable>,
        result: Variable,
        span: Span,
    ) -> R<()> {
        self.constrain(callee, AGGREGATE, span)?;
        let root = self.root(callee);
        match self.clone_shape(&self.nodes[root].shape, span)? {
            Some(Shape::FunctionPointer {
                parameters: previous,
                result: previous_result,
            }) => {
                if previous.len() != parameters.len() {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "function pointer call argument count mismatch",
                    ));
                }
                for (a, b) in previous.into_iter().zip(parameters) {
                    self.unify(a, b, span)?;
                }
                self.unify(previous_result, result, span)?;
            }
            Some(_) => {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    span,
                    "called value is not a function pointer",
                ));
            }
            None => {
                for variable in parameters.iter().chain(std::iter::once(&result)) {
                    if self.occurs(root, *variable, span)? {
                        return Err(error(
                            "E-INFINITE-LAYOUT",
                            span,
                            "recursive function pointer constraint",
                        ));
                    }
                }
                self.nodes[root].shape = Some(Shape::FunctionPointer { parameters, result });
            }
        }
        if let Some(ty) = self.clone_type(&self.nodes[root].concrete, span)? {
            self.bind(root, &ty, span)?;
        }
        Ok(())
    }
    pub(super) fn project(
        &mut self,
        base: Variable,
        projection: Projection,
        result: Variable,
        span: Span,
    ) -> R<()> {
        self.spend(span)?;
        self.projections.push((base, projection, result, span));
        self.solve()
    }
    pub(super) fn solve(&mut self) -> R<()> {
        // Every resolved projection is removed; only unresolved relations persist.
        // A successful pass resolves at least one relation, so work is finite.
        loop {
            let mut pending = std::mem::take(&mut self.projections);
            let mut progressed = false;
            for (base, projection, result, span) in pending.drain(..) {
                self.spend(span)?;
                let base = self
                    .address_shape(base, span)?
                    .map_or(base, |(inner, _)| inner);
                let base_root = self.root(base);
                let projected_variable = match (&self.nodes[base_root].shape, &projection) {
                    (Some(Shape::Tuple(fields)), Projection::Field(name)) => name
                        .parse::<usize>()
                        .ok()
                        .and_then(|n| fields.get(n))
                        .copied(),
                    (
                        Some(Shape::Array { element, .. } | Shape::Slice { element }),
                        Projection::Element,
                    ) => Some(*element),
                    _ => None,
                };
                if let Some(field) = projected_variable {
                    self.unify(result, field, span)?;
                    progressed = true;
                    continue;
                }
                if let Some(ty) = self.ty(base)? {
                    let ty = match ty {
                        Type::Address { inner, .. }
                            if inner.is_address_pointee() || stored::shared_value(&inner) =>
                        {
                            *inner
                        }
                        other => other,
                    };
                    let field = match &projection {
                        Projection::Field(name) => aggregates::field(&ty, name, span)?.1,
                        Projection::Element => match ty {
                            Type::Array { element, .. } | Type::Slice { element } => *element,
                            _ => {
                                return Err(error(
                                    "E-INDEX-TYPE",
                                    span,
                                    "index requires a fixed array",
                                ));
                            }
                        },
                    };
                    self.bind(result, &field, span)?;
                    progressed = true;
                } else {
                    self.projections.push((base, projection, result, span));
                }
            }
            if !progressed {
                return Ok(());
            }
        }
    }
    pub(super) fn ty(&self, variable: Variable) -> R<Option<Type>> {
        self.ty_depth(variable, 0)
    }
    fn ty_depth(&self, variable: Variable, depth: usize) -> R<Option<Type>> {
        // Repeated variables form a DAG: expanding (T,T) can otherwise double
        // type storage at each step. Charge every output node before allocating.
        self.spend(Span::default())?;
        if depth > self.depth_limit {
            return Err(limit(
                "closure aggregate type depth exceeded",
                Span::default(),
            ));
        }
        let node = &self.nodes[self.root(variable)];
        let ty = match node.domain {
            UNIT => Some(Type::Unit),
            BOOL => Some(Type::Bool),
            U8 => Some(Type::U8),
            U16 => Some(Type::U16),
            HANDLE => Some(Type::ForeignHandle),
            I32 => Some(Type::I32),
            I64 => Some(Type::I64),
            F32 => Some(Type::F32),
            F64 => Some(Type::F64),
            U64 => Some(Type::U64),
            USIZE => Some(Type::Usize),
            AGGREGATE => {
                if let Some(ty) = &node.concrete {
                    self.reserve_type(ty, depth, Span::default())?;
                    Some(ty.clone())
                } else {
                    match &node.shape {
                        Some(Shape::Tuple(fields)) => {
                            let mut types = Vec::new();
                            for field in fields {
                                let Some(ty) = self.ty_depth(*field, depth + 1)? else {
                                    return Ok(None);
                                };
                                types.push(ty);
                            }
                            Some(Type::Tuple(types))
                        }
                        Some(Shape::Array { element, length }) => self
                            .ty_depth(*element, depth + 1)?
                            .map(|element| Type::Array {
                                element: Box::new(element),
                                length: *length,
                            }),
                        Some(Shape::Slice { element }) => {
                            self.ty_depth(*element, depth + 1)?
                                .map(|element| Type::Slice {
                                    element: Box::new(element),
                                })
                        }
                        Some(Shape::Address {
                            inner,
                            mutable: Some(mutable),
                        }) => self
                            .ty_depth(*inner, depth + 1)?
                            .map(|inner| Type::Address {
                                mutable: *mutable,
                                inner: Box::new(inner),
                            }),
                        Some(Shape::Address { mutable: None, .. }) => None,
                        Some(Shape::FunctionPointer { parameters, result }) => {
                            let mut types = Vec::new();
                            for parameter in parameters {
                                let Some(ty) = self.ty_depth(*parameter, depth + 1)? else {
                                    return Ok(None);
                                };
                                types.push(ty);
                            }
                            let Some(result) = self.ty_depth(*result, depth + 1)? else {
                                return Ok(None);
                            };
                            Some(Type::FunctionPointer {
                                parameters: types,
                                result: Box::new(result),
                            })
                        }
                        None => None,
                    }
                }
            }
            _ => None,
        };
        Ok(ty)
    }
    pub(super) fn injection_type(&mut self, variable: Variable, span: Span) -> R<Type> {
        self.solve()?;
        if let Some(ty) = self.ty(variable)? {
            return Ok(ty);
        }
        let root = self.root(variable);
        if self.nodes[root].literal && self.nodes[root].domain == INTEGER {
            self.bind(variable, &Type::I32, span)?;
            return Ok(Type::I32);
        }
        if self.nodes[root].literal && self.nodes[root].domain == FLOAT {
            self.bind(variable, &Type::F64, span)?;
            return Ok(Type::F64);
        }
        Err(error(
            "E-ERROR-INFERENCE",
            span,
            "error-union injection requires a uniquely known payload type; add a type annotation",
        ))
    }
    pub(super) fn default_literals(&mut self) {
        for node in &mut self.nodes {
            if node.literal && node.domain == INTEGER {
                node.domain = I32;
            } else if node.literal && node.domain == FLOAT {
                node.domain = F64;
            }
        }
    }
}

pub(super) struct Inference {
    graph: Graph,
    parameters: Vec<Variable>,
    result: Variable,
}
impl Inference {
    pub fn arity(&self) -> usize {
        self.parameters.len()
    }
    pub fn parameter(&self, index: usize) -> R<Option<Type>> {
        self.graph.ty(self.parameters[index])
    }
    pub fn result(&self) -> R<Option<Type>> {
        self.graph.ty(self.result)
    }
    pub fn argument(&mut self, index: usize, ty: &Type, span: Span) -> R<()> {
        self.graph.bind(self.parameters[index], ty, span)?;
        self.graph.solve()
    }
    pub fn expect_result(&mut self, ty: &Type, span: Span) -> R<()> {
        self.graph.bind(self.result, ty, span)?;
        self.graph.solve()
    }
    pub fn ready(&self) -> R<bool> {
        if self.result()?.is_none() {
            return Ok(false);
        }
        for i in 0..self.arity() {
            if self.parameter(i)?.is_none() {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn resolve_shared(
        &mut self,
        defaults: bool,
        span: Span,
        remaining: &mut usize,
    ) -> R<(Vec<Type>, Type)> {
        let original = self.graph.work_left.get();
        let allowance = original.min(*remaining);
        self.graph.work_left.set(allowance);
        let result = self.resolve(defaults, span);
        self.graph
            .thread_finish(original, allowance, remaining, result)
    }
    pub fn resolve(&mut self, defaults: bool, span: Span) -> R<(Vec<Type>, Type)> {
        self.resolve_inner(defaults, None, span)
    }
    pub fn resolve_borrowed(
        &mut self,
        life: &StorageLifeContract,
        span: Span,
    ) -> R<(Vec<Type>, Type)> {
        self.resolve_inner(false, Some(life), span)
    }
    fn resolve_inner(
        &mut self,
        defaults: bool,
        life: Option<&StorageLifeContract>,
        span: Span,
    ) -> R<(Vec<Type>, Type)> {
        self.graph.solve()?;
        if defaults {
            self.graph.default_literals();
            self.graph.solve()?;
        }
        let parameters = self
            .parameters
            .iter()
            .map(|p| self.graph.ty(*p))
            .collect::<R<Vec<_>>>()?
            .into_iter()
            .collect::<Option<Vec<_>>>();
        let signature = parameters.zip(self.result()?).ok_or_else(|| error(
            "E-CLOSURE-INFERENCE", span,
            "closure parameter or result type is not uniquely constrained; annotate it or supply a uniquely typed call",
        ))?;
        let old = supported_signature(&signature.0, &signature.1);
        if let Some(life) = life {
            self.graph
                .spend_count(borrowed_fp::metadata_work(life), span)?;
            for ty in signature.0.iter().chain(std::iter::once(&signature.1)) {
                self.graph.reserve_type(ty, 0, span)?;
            }
            borrowed_fp::qualified_inputs(life, &signature.0, &signature.1, |work| {
                self.graph.spend_count(work, span)
            })?;
            borrowed_fp::relation(&signature.0, &signature.1, life, span)?;
            return Ok(signature);
        }
        if !old {
            // The owned shape and hidden-callable checks are new traversals.
            // Pay them only after the old profile failed, before classification.
            for ty in signature.0.iter().chain(std::iter::once(&signature.1)) {
                self.graph.reserve_type(ty, 0, span)?;
                self.graph.reserve_type(ty, 0, span)?;
            }
        }
        if !old && !value_signature(&signature.0, &signature.1) {
            return Err(unsupported(
                span,
                "closure signatures require finite Copy or one-layer Copy/slice-address arguments; address arguments require a Copy result",
            ));
        }
        Ok(signature)
    }
}

/// One shared-address layer reuses the pointee's scalar variable. It introduces
/// neither a public placeholder type nor recursive type unification.
#[derive(Clone, Copy)]
enum LocalType {
    Scalar(Variable),
    Address(Variable, bool),
    Resource(Variable, bool),
    Closure(ClosureId, Option<bool>, Option<CallableKind>),
    // A resolved nested source closure, not a public thin pointer or Copy
    // value. The ordinary Builder still constructs its actual environment.
    Nested(usize, Option<bool>),
    SliceLength,
}
impl LocalType {
    fn scalar(self, span: Span) -> R<Variable> {
        match self {
            Self::Scalar(variable) => Ok(variable),
            Self::Address(..)
            | Self::Resource(..)
            | Self::Closure(..)
            | Self::Nested(..)
            | Self::SliceLength => Err(unsupported(
                span,
                "address/resource cannot be used in a Copy closure signature or operation",
            )),
        }
    }
    fn bind(self, graph: &mut Graph, ty: &Type, span: Span) -> R<()> {
        match (self, ty) {
            (Self::Closure(actual, None, _), Type::Closure(expected)) if actual == *expected => {
                Ok(())
            }
            (Self::Closure(actual, Some(available), _), Type::Address { mutable, inner })
                if matches!(inner.as_ref(), Type::Closure(expected) if actual == *expected)
                    && (available || !mutable) =>
            {
                Ok(())
            }
            (Self::Scalar(variable), _) => graph.bind(variable, ty, span),
            (Self::Resource(variable, _), Type::Desc(payload))
            | (Self::Resource(variable, true), Type::Managed(payload)) => {
                graph.bind(variable, payload, span)
            }
            (
                Self::Address(variable, mutable),
                Type::Address {
                    mutable: wanted,
                    inner,
                },
            ) if (inner.is_address_pointee()
                || stored::shared_address(ty)
                || ty.is_shared_sync_cell_address()
                || ty.is_shared_resource_head_address()
                || ty.is_exclusive_task_head_address()
                || resources::head_address(ty)
                || ty.is_package_address())
                && (mutable || !wanted) =>
            {
                graph.bind(variable, inner, span)
            }
            (Self::Address(_, false), Type::Address { mutable: true, .. }) => Err(error(
                "E-ADDRESS-PERMISSION",
                span,
                "a shared address cannot grant exclusive access",
            )),
            (Self::Address(..), Type::Address { .. }) => Err(unsupported(
                span,
                "only shared finite Copy addresses are implemented",
            )),
            _ => Err(error(
                "E-TYPE-MISMATCH",
                span,
                "shared address does not match scalar type",
            )),
        }
    }
    fn unify(self, other: Self, graph: &mut Graph, span: Span) -> R<()> {
        match (self, other) {
            (Self::Nested(a, am), Self::Nested(b, bm)) if a == b && am == bm => graph.spend(span),
            (Self::Closure(a, am, ak), Self::Closure(b, bm, bk))
                if a == b && am == bm && ak == bk =>
            {
                Ok(())
            }
            (Self::Scalar(a), Self::Scalar(b)) => graph.unify(a, b, span),
            (Self::Address(a, am), Self::Address(b, bm)) if am == bm => graph.unify(a, b, span),
            (Self::Resource(a, am), Self::Resource(b, bm)) if am == bm => graph.unify(a, b, span),
            _ => Err(error(
                "E-TYPE-MISMATCH",
                span,
                "shared address and scalar types differ",
            )),
        }
    }
}

struct Functions<'a> {
    closures: &'a [Pending],
    signatures: &'a BTreeMap<String, Signature>,
    imports: Option<&'a program::Imports<'a>>,
    resource_writes: &'a resource_effects::Writes,
    declared: Option<&'a BTreeMap<LocalId, CallableKind>>,
}
struct InferBuilder<'a, 'b> {
    closures: &'a [Pending],
    resource_writes: &'a resource_effects::Writes,
    graph: Graph,
    nested_signatures: Vec<Type>,
    parameters: Vec<Variable>,
    result: Variable,
    scopes: Vec<BTreeMap<String, LocalType>>,
    prepared: std::collections::BTreeSet<String>,
    outer: &'a BTreeMap<String, (LocalId, Type)>,
    declared: Option<&'a BTreeMap<LocalId, CallableKind>>,
    captures: Vec<Capture>,
    captured_variables: BTreeMap<LocalId, LocalType>,
    package_descriptions: Vec<(Variable, LocalId)>,
    signatures: &'a BTreeMap<String, Signature>,
    imports: Option<&'a program::Imports<'a>>,
    enums: Option<&'a BTreeMap<String, Type>>,
    move_capture: bool,
    // A move boundary copies an ordinary Copy capture but transfers an owned
    // carrier. This belongs to capture inference, not to the safety proof.
    nested_moves: usize,
    nested_depth: usize,
    nested_root: Option<zil_base::NodeId>,
    nested_package_moves: BTreeMap<zil_base::NodeId, BTreeSet<String>>,
    has_return: bool,
    limits: &'a Limits,
    work_left: &'b mut usize,
    nodes_left: &'b mut usize,
}
impl<'a, 'b> InferBuilder<'a, 'b> {
    fn owner_type(&mut self, actual: LocalType, owner: Option<&Type>, span: Span) -> R<()> {
        let Some(expected) = owner else {
            return Ok(());
        };
        self.graph.reserve_type(expected, 0, span)?;
        let variable = match actual {
            LocalType::Scalar(v) | LocalType::Address(v, _) | LocalType::Resource(v, _) => Some(v),
            _ => None,
        };
        if let Some(variable) = variable
            && let Some(ty) = self.graph.ty(variable)?
        {
            self.graph.reserve_type(&ty, 0, span)?;
            if member_owner::concrete(&ty, &mut || self.graph.spend(span))?
                && member_owner::concrete(expected, &mut || self.graph.spend(span))?
            {
                self.graph.reserve_type(expected, 0, span)?;
                self.graph.spend(span)?;
                self.graph.spend(span)?;
                member_owner::check_member_owner(&ty, Some(expected), span)?;
            }
        }
        Ok(())
    }

    fn new(
        input: &Input<'_>,
        outer: &'a BTreeMap<String, (LocalId, Type)>,
        functions: Functions<'a>,
        limits: &'a Limits,
        work_left: &'b mut usize,
        nodes_left: &'b mut usize,
    ) -> R<Self> {
        let mut graph = Graph::new(limits);
        let result = graph.fresh(VALUE, false);
        if let Some(ty) = input.result {
            graph.annotation_context(ty, 0, input.span)?;
            if matches!(ty, Type::Slice { .. }) {
                return Err(unsupported(
                    input.span,
                    "slice closure signatures are not implemented",
                ));
            }
            graph.bind(result, ty, input.span)?;
        }
        let mut this = Self {
            closures: functions.closures,
            resource_writes: functions.resource_writes,
            graph,
            nested_signatures: vec![],
            parameters: vec![],
            result,
            scopes: vec![BTreeMap::new()],
            prepared: std::collections::BTreeSet::new(),
            outer,
            declared: functions.declared,
            captures: vec![],
            captured_variables: BTreeMap::new(),
            package_descriptions: Vec::new(),
            signatures: functions.signatures,
            imports: functions.imports,
            enums: None,
            move_capture: input.move_capture,
            nested_moves: 0,
            nested_depth: 0,
            nested_root: None,
            nested_package_moves: BTreeMap::new(),
            has_return: false,
            limits,
            work_left,
            nodes_left,
        };
        for parameter in input.parameters {
            this.visit(0, parameter.span)?;
            if !matches!(
                parameter.mode,
                PassingMode::Infer | PassingMode::Copy | PassingMode::Move
            ) {
                return Err(unsupported(
                    parameter.span,
                    "non-Value closure parameter mode",
                ));
            }
            let variable = this.graph.fresh(VALUE, false);
            if let Some(ty) = &parameter.ty {
                this.graph.annotation_context(ty, 0, parameter.span)?;
                if !signature_parameter(ty) && !owned_signature_head(ty) {
                    return Err(unsupported(
                        parameter.span,
                        "closure parameters require a finite Copy type or one Copy/slice address layer",
                    ));
                }
                this.graph.bind(variable, ty, parameter.span)?;
            }
            let local = if let Some(ty @ Type::Address { .. }) = &parameter.ty {
                let local = this.known(ty, parameter.span)?;
                if let LocalType::Address(inner, mutable) = local {
                    this.graph
                        .address(variable, inner, Some(mutable), parameter.span)?;
                }
                local
            } else {
                LocalType::Scalar(variable)
            };
            if this.scopes[0]
                .insert(parameter.name.clone(), local)
                .is_some()
            {
                return Err(error(
                    "E-NAME-DUPLICATE",
                    parameter.span,
                    format!("duplicate closure parameter {}", parameter.name),
                ));
            }
            this.parameters.push(variable);
        }
        Ok(this)
    }
    fn visit(&mut self, depth: usize, span: Span) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(limit("closure inference depth budget exceeded", span));
        }
        *self.work_left = self
            .work_left
            .checked_sub(1)
            .ok_or_else(|| limit("closure inference work budget exceeded", span))?;
        *self.nodes_left = self
            .nodes_left
            .checked_sub(1)
            .ok_or_else(|| limit("closure inference node budget exceeded", span))?;
        Ok(())
    }
    fn lookup(&self, name: &str) -> Option<LocalType> {
        self.scopes.iter().rev().find_map(|s| s.get(name).copied())
    }
    fn name(&mut self, name: &str, write: bool, span: Span) -> R<LocalType> {
        if let Some(variable) = self.lookup(name) {
            return Ok(match variable {
                LocalType::Resource(variable, _) => LocalType::Resource(variable, false),
                other => other,
            });
        }
        let Some((local, ty)) = self.outer.get(name) else {
            if !write && let Some(signature) = self.signatures.get(name) {
                let ty = self.pointer_type(signature, span)?;
                return self.known(&ty, span);
            }
            return Err(error(
                "E-UNKNOWN-NAME",
                span,
                format!("unknown value {name}"),
            ));
        };
        if let Type::Address { inner, .. } = ty
            && matches!(
                inner.as_ref(),
                Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
            )
        {
            owned_shapes::validate(inner, self.work_left, self.limits.max_call_depth, span)?;
        }
        if ty.is_package_address() {
            let Type::Address { inner, .. } = ty else {
                unreachable!()
            };
            // This is shape admission, not origin authority. Analyze and Verify
            // independently exclude dynamic Store and unknown incoming roots.
            packages::product::validate(inner, self.work_left, self.limits.max_call_depth, span)?;
        }
        if ty.is_shared_resource_head_address() {
            return Err(unsupported(
                span,
                "resource-head address capture requires a separate retained head-window contract",
            ));
        }
        // A write behind a nested move changes that inner snapshot, not the
        // outer position. Managed/Package captures instead change ownership at
        // creation, even if the nested environment is never invoked.
        let write = write && !(self.nested_moves != 0 && ty.is_copy_value());
        let owned_sum_capture = (self.move_capture || self.nested_moves != 0)
            && matches!(
                ty,
                Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
            )
            && dynamic_store::owned_capture(ty, self.work_left, self.limits, span)?;
        let nested_owned = self.nested_moves != 0
            && matches!(
                ty,
                Type::Managed(_) | Type::Package { .. } | Type::Closure(_)
            )
            || (self.nested_moves != 0 && owned_sum_capture);
        let nested_writeback =
            write && self.nested_depth != 0 && self.move_capture && ty.is_copy_value();
        if nested_owned && matches!(ty, Type::Package { .. }) {
            *self.work_left = self
                .work_left
                .checked_sub(name.len().saturating_add(1))
                .ok_or_else(|| limit("nested Package capture budget exceeded", span))?;
            self.nested_package_moves
                .entry(self.nested_root.expect("nested move root"))
                .or_default()
                .insert(name.into());
        }
        // Package slots may be restored after the nested creation. Their flow
        // classification below decides FnMut/FnOnce at each actual exit.
        let nested_consumed = nested_owned
            && (matches!(ty, Type::Managed(_) | Type::Closure(_)) || owned_sum_capture);
        if self.nested_moves != 0 && matches!(ty, Type::Address { mutable: true, .. }) {
            return Err(unsupported(
                span,
                "moving a captured exclusive address into a nested environment requires an explicit transfer contract",
            ));
        }
        if write && matches!(ty, Type::Address { .. }) {
            return Err(unsupported(
                span,
                "rebinding a captured address requires a nested address window",
            ));
        }
        if let Type::Closure(id) = ty {
            let moved = self.move_capture || nested_owned;
            if let Some(variable) = self.captured_variables.get(local) {
                let capture = self
                    .captures
                    .iter_mut()
                    .find(|c| c.source == *local)
                    .expect("capture");
                capture.written |= write;
                capture.moved |= nested_owned;
                capture.borrowed &= !nested_owned;
                capture.consumed |= nested_consumed;
                return Ok(*variable);
            }
            let declared_kind = if let Some(declared) = self.declared {
                *self.work_left = self
                    .work_left
                    .checked_sub(helpers::plan::tree_work(declared.len()))
                    .ok_or_else(|| limit("factory capture declaration budget exceeded", span))?;
                declared.get(local).copied()
            } else {
                None
            };
            let value = LocalType::Closure(*id, None, declared_kind);
            self.captures.push(Capture {
                name: name.into(),
                source: *local,
                ty: ty.clone(),
                borrowed: !moved,
                written: write,
                address_written: false,
                moved,
                consumed: nested_consumed,
                declared_kind,
                span,
            });
            self.captured_variables.insert(*local, value);
            return Ok(value);
        }
        let (inner, address) = match ty {
            ty if ty.is_copy_value() => (ty, None),
            ty @ Type::Package { .. }
                if packages::supported(ty)
                    || packages::product::nested(ty)
                    || ((self.move_capture || nested_owned) && packages::ordinary(ty)) =>
            {
                if packages::product::nested(ty) && packages::product::supported(ty) {
                    packages::product::validate(
                        ty,
                        self.work_left,
                        self.limits.max_call_depth,
                        span,
                    )?;
                } else if packages::product::nested(ty) {
                    packages::product::validate_owned(
                        ty,
                        self.work_left,
                        self.limits.max_call_depth,
                        span,
                    )?;
                } else if !self.move_capture
                    && !nested_owned
                    && !dynamic_store::owned_capture(ty, self.work_left, self.limits, span)?
                {
                    // Flat ordinary Products share the same window profile as
                    // recursive Products. Other Package kinds still fail this
                    // complete qualifier; Store/array/callback ownership is not
                    // granted by the new default-read path.
                    packages::product::validate(
                        ty,
                        self.work_left,
                        self.limits.max_call_depth,
                        span,
                    )?;
                }
                (ty, None)
            }
            ty if owned_sum_capture => (ty, None),
            Type::Managed(payload) | Type::Desc(payload) if resources::supported(ty) => {
                (payload.as_ref(), None)
            }
            Type::Address {
                mutable: false,
                inner,
            } if matches!(inner.as_ref(), Type::BorrowedStorage { storage, life }
                    if matches!(storage.as_ref(), Type::Record { .. } | Type::Tuple(_)) && life.retained.is_empty()) =>
            {
                (inner.as_ref(), Some(false))
            }
            Type::Address { mutable, inner }
                if inner.is_copy_value() || ty.is_shared_sync_cell_address() =>
            {
                (inner.as_ref(), Some(*mutable))
            }
            Type::Address { mutable, inner }
                if matches!(
                    inner.as_ref(),
                    Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
                ) =>
            {
                (inner.as_ref(), Some(*mutable))
            }
            Type::Address { mutable, inner } if matches!(inner.as_ref(), Type::Package { .. }) => {
                // Complete Product qualification was charged above. Capturing
                // the address keeps a loan; it never creates an owning slot.
                (inner.as_ref(), Some(*mutable))
            }
            _ => {
                return Err(unsupported(
                    span,
                    "capture requires a finite Copy value, finite resource, explicit-move Package or one address layer",
                ));
            }
        };
        if let Some(variable) = self.captured_variables.get(local) {
            let capture = self
                .captures
                .iter_mut()
                .find(|c| c.source == *local)
                .expect("capture index");
            capture.written |= write;
            capture.address_written |= nested_writeback;
            capture.moved |= nested_owned;
            capture.consumed |= nested_consumed;
            return Ok(*variable);
        }
        let variable = self.graph.fresh(VALUE, false);
        self.graph.bind(variable, inner, span)?;
        self.captures.push(Capture {
            name: name.into(),
            source: *local,
            ty: ty.clone(),
            borrowed: !self.move_capture && address.is_none() && ty.is_copy_value(),
            written: write,
            address_written: nested_writeback,
            moved: self.move_capture || nested_owned,
            consumed: nested_consumed,
            declared_kind: None,
            span,
        });
        let value = if matches!(ty, Type::Managed(_) | Type::Desc(_)) {
            LocalType::Resource(variable, false)
        } else if let Some(mutable) = address {
            LocalType::Address(variable, mutable)
        } else {
            LocalType::Scalar(variable)
        };
        self.captured_variables.insert(*local, value);
        Ok(value)
    }
    fn pattern_needs_mutable(&mut self, pattern: &CorePattern, depth: usize) -> R<bool> {
        self.visit(depth, pattern.span)?;
        match &pattern.kind {
            CorePatternKind::Reference { mutable, pattern } => {
                Ok(*mutable || self.pattern_needs_mutable(pattern, depth + 1)?)
            }
            CorePatternKind::ErrorArm { pattern, .. } => {
                self.pattern_needs_mutable(pattern, depth + 1)
            }
            CorePatternKind::Tuple(fields) | CorePatternKind::Variant { fields, .. } => {
                for field in fields {
                    if self.pattern_needs_mutable(field, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }
    fn pattern_local(&mut self, variable: Variable, span: Span) -> R<LocalType> {
        if let Some((inner, Some(mutable))) = self.graph.address_shape(variable, span)? {
            return Ok(LocalType::Address(inner, mutable));
        }
        // Pattern matching takes an owned Sum, but an ordinary use of a bound
        // resource name describes its leaf. Keep that distinction in closure
        // inference as the Builder does for a managed local.
        if let Some(ty @ (Type::Managed(_) | Type::Desc(_))) = self.graph.ty(variable)? {
            return self.known(&ty, span);
        }
        Ok(LocalType::Scalar(variable))
    }
    fn block(&mut self, block: &CoreBlock, depth: usize) -> R<()> {
        self.visit(depth, block.span)?;
        self.scopes.push(BTreeMap::new());
        for statement in &block.statements {
            self.visit(depth + 1, statement.span)?;
            match &statement.kind {
                CoreStatementKind::TupleBinding {
                    pattern,
                    ty,
                    initializer,
                    ..
                } => {
                    let variable = self.expr_expected(initializer, ty.as_ref(), depth + 1)?;
                    if let Some(ty) = ty {
                        variable.bind(&mut self.graph, ty, statement.span)?;
                    }
                    let value = variable.scalar(statement.span)?;
                    if self.pattern_needs_mutable(pattern, depth + 1)? {
                        self.mark_address_write(initializer, depth + 1)?;
                    }
                    // Classify each actual child: `ref` can coexist with a
                    // by-value owner binding in the same Tuple pattern.
                    if let CoreExprKind::Name(name) = &initializer.kind
                        && self.lookup(name).is_none()
                        && self.outer.get(name).is_some_and(|(_, ty)| {
                            matches!(ty, Type::Tuple(_))
                                && dynamic_store::contains_inline_carrier(ty)
                        })
                    {
                        *self.work_left = self
                            .work_left
                            .checked_sub(
                                self.captures
                                    .len()
                                    .saturating_mul(name.len().saturating_add(1)),
                            )
                            .ok_or_else(|| {
                                limit(
                                    "owned Tuple capture transfer budget exceeded",
                                    initializer.span,
                                )
                            })?;
                        if let Some(capture) = self.captures.iter_mut().find(|c| c.name == *name) {
                            inference::charge_type_snapshot(
                                &capture.ty,
                                initializer.span,
                                self.limits,
                                self.work_left,
                                2,
                            )?;
                            if owned_shapes::tuple(&capture.ty)
                                && tuple_pattern_takes_owner(
                                    pattern,
                                    &capture.ty,
                                    self.work_left,
                                    0,
                                    self.limits.max_call_depth,
                                )?
                            {
                                capture.moved = true;
                                capture.borrowed = false;
                                capture.consumed = true;
                            }
                        }
                    }
                    for (name, variable, span) in self.graph.pattern_bind(pattern, value)? {
                        let value = self.pattern_local(variable, span)?;
                        if self
                            .scopes
                            .last_mut()
                            .expect("scope")
                            .insert(name.clone(), value)
                            .is_some()
                        {
                            return Err(error(
                                "E-NAME-DUPLICATE",
                                span,
                                format!("duplicate local {name}"),
                            ));
                        }
                    }
                }
                CoreStatementKind::Binding {
                    name,
                    ty,
                    initializer,
                    ..
                } => {
                    let Some(expression) = initializer else {
                        return Err(unsupported(statement.span, "uninitialized closure binding"));
                    };
                    let variable = self.expr_expected(expression, ty.as_ref(), depth + 1)?;
                    self.transferred_closure(expression, variable);
                    if let Some(ty) = ty {
                        variable.bind(&mut self.graph, ty, statement.span)?;
                    }
                    if self
                        .scopes
                        .last_mut()
                        .expect("scope")
                        .insert(name.clone(), variable)
                        .is_some()
                    {
                        return Err(error(
                            "E-NAME-DUPLICATE",
                            statement.span,
                            format!("duplicate local {name}"),
                        ));
                    }
                }
                CoreStatementKind::Assign { target, value } => {
                    let target_type = self.place(target, depth + 1)?;
                    let hint = match target_type {
                        LocalType::Scalar(v) => self.graph.ty(v)?,
                        _ => None,
                    };
                    let value_type = self.expr_expected(value, hint.as_ref(), depth + 1)?;
                    self.transferred_closure(value, value_type);
                    target_type.unify(value_type, &mut self.graph, value.span)?;
                }
                CoreStatementKind::Expression(expression) => {
                    self.expr(expression, depth + 1)?;
                }
                CoreStatementKind::Return(expression) => {
                    self.has_return = true;
                    if let Some(expression) = expression {
                        let hint = self.graph.ty(self.result)?;
                        let value = self.expr_expected(expression, hint.as_ref(), depth + 1)?;
                        match value {
                            LocalType::Address(inner, false) => {
                                self.graph
                                    .shared_address(self.result, inner, expression.span)?;
                            }
                            LocalType::Address(inner, true) => {
                                self.graph.address(
                                    self.result,
                                    inner,
                                    Some(true),
                                    expression.span,
                                )?;
                            }
                            _ => self.graph.unify(
                                self.result,
                                value.scalar(expression.span)?,
                                expression.span,
                            )?,
                        }
                    } else if let Some(ty @ Type::ErrorUnion { .. }) = self.graph.ty(self.result)? {
                        errors::injection(&Type::Unit, &ty, statement.span)?;
                    } else {
                        self.graph.bind(self.result, &Type::Unit, statement.span)?;
                    }
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    let condition_type = self.expr(condition, depth + 1)?;
                    condition_type.bind(&mut self.graph, &Type::Bool, condition.span)?;
                    self.block(then_body, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    let condition_type = self.expr(condition, depth + 1)?;
                    condition_type.bind(&mut self.graph, &Type::Bool, condition.span)?;
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    let variable = self.expr(value, depth + 1)?.scalar(value.span)?;
                    if self.pattern_needs_mutable(pattern, depth + 1)? {
                        self.mark_address_write(value, depth + 1)?;
                    }
                    self.scopes.push(BTreeMap::new());
                    for (name, variable, span) in self.graph.pattern_bind(pattern, variable)? {
                        let value = self.pattern_local(variable, span)?;
                        self.scopes
                            .last_mut()
                            .expect("pattern scope")
                            .insert(name, value);
                    }
                    self.block(then_body, depth + 1)?;
                    self.scopes.pop();
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    let variable = self.expr(value, depth + 1)?.scalar(value.span)?;
                    if self.pattern_needs_mutable(pattern, depth + 1)? {
                        self.mark_address_write(value, depth + 1)?;
                    }
                    self.scopes.push(BTreeMap::new());
                    for (name, variable, span) in self.graph.pattern_bind(pattern, variable)? {
                        let value = self.pattern_local(variable, span)?;
                        self.scopes
                            .last_mut()
                            .expect("pattern scope")
                            .insert(name, value);
                    }
                    self.block(body, depth + 1)?;
                    self.scopes.pop();
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    let start_type = self.expr(start, depth + 1)?;
                    let end_type = self.expr(end, depth + 1)?;
                    start_type.unify(end_type, &mut self.graph, statement.span)?;
                    self.graph.constrain(
                        start_type.scalar(statement.span)?,
                        INTEGER,
                        statement.span,
                    )?;
                    self.scopes
                        .push(BTreeMap::from([(name.clone(), start_type)]));
                    self.block(body, depth + 1)?;
                    self.scopes.pop();
                }
                CoreStatementKind::Match { value, arms } => {
                    let variable = self.expr(value, depth + 1)?.scalar(value.span)?;
                    for arm in arms {
                        if self.pattern_needs_mutable(&arm.pattern, depth + 1)? {
                            self.mark_address_write(value, depth + 1)?;
                        }
                        self.scopes.push(BTreeMap::new());
                        for (name, variable, span) in
                            self.graph.pattern_bind(&arm.pattern, variable)?
                        {
                            let value = self.pattern_local(variable, span)?;
                            self.scopes
                                .last_mut()
                                .expect("pattern scope")
                                .insert(name, value);
                        }
                        self.block(&arm.body, depth + 1)?;
                        self.scopes.pop();
                    }
                }
                CoreStatementKind::Defer(body) => {
                    // A deferred mutation reaches this closure's activation
                    // local through a nested borrowed environment. Like a
                    // non-move nested closure, it needs final Copy capture
                    // writeback even when no outer Store executes.
                    self.nested_depth += 1;
                    self.block(body, depth + 1)?;
                    self.nested_depth -= 1;
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    self.block(body, depth + 1)?
                }
                CoreStatementKind::Break | CoreStatementKind::Continue => {}
                CoreStatementKind::Unsupported(feature) => {
                    return Err(unsupported(statement.span, feature));
                }
            }
        }
        self.scopes.pop();
        Ok(())
    }
    fn name_if_capture_written(&mut self, expression: &CoreExpr) {
        if let CoreExprKind::Name(name) = &expression.kind
            && self.lookup(name).is_none()
            && let Some(capture) = self.captures.iter_mut().find(|c| c.name == *name)
        {
            capture.written = true;
        }
    }
    fn transferred_closure(&mut self, expression: &CoreExpr, term: LocalType) {
        if matches!(term, LocalType::Closure(_, None, _))
            && let CoreExprKind::Name(name) = &expression.kind
            && self.lookup(name).is_none()
            && let Some(capture) = self.captures.iter_mut().find(|c| c.name == *name)
        {
            capture.moved = true;
            capture.borrowed = false;
            capture.consumed = true;
        }
    }
    fn transferred_factory_branch(&mut self, expression: &CoreExpr, term: LocalType) -> R<()> {
        let (LocalType::Closure(_, None, Some(_)), CoreExprKind::Name(name)) =
            (term, &expression.kind)
        else {
            return Ok(());
        };
        // Reserve the new scope walk and both lexical/capture searches before
        // inspecting them. The old direct-alias path keeps its existing fee.
        *self.work_left = self
            .work_left
            .checked_sub(self.scopes.len())
            .ok_or_else(|| {
                limit(
                    "factory conditional transfer budget exceeded",
                    expression.span,
                )
            })?;
        let searches = self
            .scopes
            .iter()
            .fold(self.captures.len().saturating_add(1), |work, scope| {
                work.saturating_add(helpers::plan::tree_work(scope.len()))
            });
        *self.work_left = self
            .work_left
            .checked_sub(searches.saturating_mul(name.len().saturating_add(1)))
            .ok_or_else(|| {
                limit(
                    "factory conditional transfer budget exceeded",
                    expression.span,
                )
            })?;
        self.transferred_closure(expression, term);
        Ok(())
    }
    fn helper_argument(
        &mut self,
        expression: &CoreExpr,
        bound: &CoreCallableParameter,
        mode: Option<bool>,
        depth: usize,
    ) -> R<LocalType> {
        let span = expression.span;
        self.graph.spend(span)?;
        let value = self.expr(expression, depth)?;
        let mismatch = || {
            error(
                "E-TYPE-MISMATCH",
                span,
                "helper requires its concrete environment passing mode and callable signature",
            )
        };
        match (value, mode) {
            (LocalType::Nested(index, actual), wanted)
                if actual == wanted || (actual == Some(true) && wanted == Some(false)) =>
            {
                let signature = self.nested_signature(index, span)?;
                self.graph.reserve_type(&signature, 0, span)?;
                let Type::FunctionPointer { parameters, result } = signature else {
                    unreachable!("private nested signature")
                };
                if parameters != bound.parameters || result.as_ref() != &bound.result {
                    return Err(mismatch());
                }
                // Capture inference does not grant a bound from this private
                // signature. Builder checks the actual closure mode and origin.
                if wanted == Some(true) {
                    self.mark_address_write(expression, depth)?;
                }
            }
            (LocalType::Scalar(variable), None)
            | (LocalType::Address(variable, false), Some(false))
            | (LocalType::Address(variable, true), Some(_)) => {
                // Nested closure bodies have already received concrete source
                // signatures. This capture walk represents them using a private
                // graph signature only; it must not bind an address to Named(F).
                // Builder later checks actual ClosureId, mode and role origin.
                self.graph.spend(span)?;
                for ty in bound
                    .parameters
                    .iter()
                    .chain(std::iter::once(&bound.result))
                {
                    self.graph.reserve_type(ty, 0, span)?;
                }
                self.graph.bind(
                    variable,
                    &Type::FunctionPointer {
                        parameters: bound.parameters.clone(),
                        result: Box::new(bound.result.clone()),
                    },
                    span,
                )?;
                if mode == Some(true) {
                    self.mark_address_write(expression, depth)?;
                }
            }
            (LocalType::Closure(id, actual, declared), wanted)
                if actual == wanted || (actual == Some(true) && wanted == Some(false)) =>
            {
                let pending = self.closures.get(id.0 as usize).ok_or_else(mismatch)?;
                if !helpers::compatible(
                    declared.unwrap_or_else(|| pending.callable_kind()),
                    bound.kind,
                ) || pending.inference.arity() != bound.parameters.len()
                {
                    return Err(mismatch());
                }
                for (index, ty) in bound.parameters.iter().enumerate() {
                    self.graph.reserve_type(ty, 0, span)?;
                    if pending.inference.parameter(index)?.as_ref() != Some(ty) {
                        return Err(mismatch());
                    }
                }
                self.graph.reserve_type(&bound.result, 0, span)?;
                if pending.inference.result()?.as_ref() != Some(&bound.result) {
                    return Err(mismatch());
                }
                if wanted.is_none() {
                    self.transferred_closure(expression, value);
                }
            }
            _ => return Err(mismatch()),
        }
        Ok(value)
    }
    fn known(&mut self, ty: &Type, span: Span) -> R<LocalType> {
        if let Type::Closure(id) = ty {
            return Ok(LocalType::Closure(*id, None, None));
        }
        if let Type::Address { mutable, inner } = ty
            && let Type::Closure(id) = inner.as_ref()
        {
            return Ok(LocalType::Closure(*id, Some(*mutable), None));
        }
        let variable = self.graph.fresh(VALUE, false);
        if let Type::Managed(payload) | Type::Desc(payload) = ty {
            if !resources::supported(ty) {
                return Err(unsupported(span, "resource requires a finite Copy payload"));
            }
            self.graph.bind(variable, payload, span)?;
            Ok(LocalType::Resource(
                variable,
                matches!(ty, Type::Managed(_)),
            ))
        } else if let Type::Address { mutable, inner } = ty {
            self.graph.bind(variable, inner, span)?;
            Ok(LocalType::Address(variable, *mutable))
        } else {
            self.graph.bind(variable, ty, span)?;
            Ok(LocalType::Scalar(variable))
        }
    }
    fn nested_signature(&self, index: usize, span: Span) -> R<Type> {
        let signature = &self.nested_signatures[index];
        self.graph.reserve_type(signature, 0, span)?;
        Ok(signature.clone())
    }
    fn project(&mut self, value: LocalType, projection: Projection, span: Span) -> R<LocalType> {
        let base = match value {
            LocalType::Address(variable, _) | LocalType::Resource(variable, _) => variable,
            _ => value.scalar(span)?,
        };
        let result = self.graph.fresh(VALUE, false);
        if let Some(ty @ Type::Package { .. }) = self.graph.ty(base)?
            && let Projection::Field(name) = &projection
        {
            let field = aggregates::field(&ty, name, span)?.1;
            let projected = self.known(&packages::projection_type(field), span)?;
            if let LocalType::Resource(variable, _) = projected {
                let base = self.graph.root(base);
                *self.work_left = self
                    .work_left
                    .checked_sub(self.captured_variables.len())
                    .ok_or_else(|| limit("Package description origin budget exceeded", span))?;
                for (source, captured) in &self.captured_variables {
                    if let LocalType::Scalar(root) = captured
                        && self.graph.root(*root) == base
                    {
                        self.package_descriptions.push((variable, *source));
                    }
                }
            }
            return Ok(projected);
        }
        self.graph.project(base, projection, result, span)?;
        Ok(LocalType::Scalar(result))
    }
    fn give(&mut self, operand: &CoreExpr, value: LocalType, span: Span) -> R<LocalType> {
        if let CoreExprKind::Member { object, member, .. } = &operand.kind
            && let CoreExprKind::Name(name) = &object.kind
            && let Some(LocalType::Scalar(variable)) = self.lookup(name).or_else(|| {
                self.outer
                    .get(name)
                    .and_then(|(source, _)| self.captured_variables.get(source).copied())
            })
            && let Some(ty @ Type::Package { .. }) = self.graph.ty(variable)?
        {
            let field = aggregates::field(&ty, member, span)?.1;
            if !matches!(field, Type::Managed(_)) {
                return Err(error(
                    "E-GIVE-TYPE",
                    span,
                    "give requires a Managed package field",
                ));
            }
            return self.known(&field, span);
        }
        let CoreExprKind::Name(name) = &operand.kind else {
            return Err(unsupported(
                span,
                "give requires a whole managed local or capture",
            ));
        };
        if let Some(local) = self.lookup(name) {
            return match local {
                LocalType::Resource(variable, true) => Ok(LocalType::Resource(variable, true)),
                LocalType::Resource(_, false) => Err(error(
                    "E-GIVE-DESCRIPTOR",
                    span,
                    "a descriptor does not own the resource carrier",
                )),
                _ => Err(error(
                    "E-GIVE-TYPE",
                    span,
                    "give requires a managed resource carrier",
                )),
            };
        }
        if let Some((source, Type::Managed(_))) = self.outer.get(name)
            && let LocalType::Resource(variable, _) = value
        {
            let capture = self
                .captures
                .iter_mut()
                .find(|c| c.source == *source)
                .expect("capture");
            // A body transfer is a creation-time ownership capture even when
            // the closure has no outer `move` qualifier.
            capture.moved = true;
            capture.consumed = true;
            return Ok(LocalType::Resource(variable, true));
        }
        Err(error(
            "E-GIVE-DESCRIPTOR",
            span,
            "a descriptor does not own the resource carrier",
        ))
    }
    fn mark_resource_write(&mut self, value: LocalType, span: Span) -> R<()> {
        let LocalType::Resource(variable, _) = value else {
            return Ok(());
        };
        let root = self.graph.root(variable);
        *self.work_left = self
            .work_left
            .checked_sub(self.captures.len())
            .ok_or_else(|| limit("resource capture effect work budget exceeded", span))?;
        for capture in &mut self.captures {
            if let Some(LocalType::Resource(variable, _)) =
                self.captured_variables.get(&capture.source)
                && self.graph.root(*variable) == root
            {
                capture.written = true;
            }
        }
        *self.work_left = self
            .work_left
            .checked_sub(
                self.package_descriptions
                    .len()
                    .saturating_mul(self.captures.len().saturating_add(1)),
            )
            .ok_or_else(|| limit("Package description write budget exceeded", span))?;
        for (variable, source) in &self.package_descriptions {
            if self.graph.root(*variable) == root
                && let Some(capture) = self.captures.iter_mut().find(|c| c.source == *source)
            {
                capture.written = true;
            }
        }
        Ok(())
    }
    fn mark_address_write(&mut self, expression: &CoreExpr, depth: usize) -> R<()> {
        self.visit(depth, expression.span)?;
        match &expression.kind {
            CoreExprKind::Name(name) => {
                if let Some(LocalType::Address(variable, mutable)) = self.lookup(name) {
                    if !mutable {
                        return Err(error(
                            "E-ADDRESS-PERMISSION",
                            expression.span,
                            "shared address cannot grant exclusive access",
                        ));
                    }
                    let root = self.graph.root(variable);
                    for capture in &mut self.captures {
                        if let Some(LocalType::Address(v, true)) =
                            self.captured_variables.get(&capture.source)
                            && self.graph.root(*v) == root
                        {
                            capture.written = true;
                        }
                    }
                } else if let Some((local, ty)) = self.outer.get(name).cloned() {
                    if ty.is_copy_value() {
                        self.name(name, true, expression.span)?;
                        if self.move_capture && self.nested_moves == 0 {
                            for capture in &mut self.captures {
                                *self.work_left =
                                    self.work_left.checked_sub(1).ok_or_else(|| {
                                        limit(
                                            "capture address-write scan budget exceeded",
                                            expression.span,
                                        )
                                    })?;
                                if capture.source == local {
                                    capture.address_written = true;
                                    break;
                                }
                            }
                        }
                    } else if matches!(ty, Type::Package { .. }) {
                        // The existing Package capture path requires explicit
                        // ownership transfer and records FnMut environment use.
                        // Its field address does not turn the Package into Copy.
                        self.name(name, true, expression.span)?;
                    } else {
                        if !matches!(ty, Type::Address { mutable: true, .. }) {
                            return Err(error(
                                "E-ADDRESS-PERMISSION",
                                expression.span,
                                "shared capture cannot grant exclusive access",
                            ));
                        }
                        self.name(name, false, expression.span)?;
                        self.captures
                            .iter_mut()
                            .find(|c| c.source == local)
                            .expect("capture")
                            .written = true;
                    }
                }
            }
            CoreExprKind::Unary { operand, .. } => self.mark_address_write(operand, depth + 1)?,
            CoreExprKind::Member { object, .. }
            | CoreExprKind::Index { object, .. }
            | CoreExprKind::Slice { object, .. } => self.mark_address_write(object, depth + 1)?,
            CoreExprKind::Ascribed { value, .. } => self.mark_address_write(value, depth + 1)?,
            _ => {}
        }
        Ok(())
    }
    fn place(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        self.visit(depth, expression.span)?;
        match &expression.kind {
            CoreExprKind::Name(name) => self.name(name, true, expression.span),
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                self.mark_address_write(operand, depth + 1)?;
                match self.expr(operand, depth + 1)? {
                    LocalType::Address(variable, true) => Ok(LocalType::Scalar(variable)),
                    _ => Err(error(
                        "E-ADDRESS-PERMISSION",
                        expression.span,
                        "indirect write requires an exclusive address",
                    )),
                }
            }
            CoreExprKind::Member {
                object,
                member,
                owner,
            } => {
                let probe = self.expr(object, depth + 1)?;
                self.owner_type(probe, owner.as_ref(), expression.span)?;
                if let LocalType::Scalar(variable) = probe
                    && let Some(ty @ Type::Package { .. }) = self.graph.ty(variable)?
                {
                    // Assignment targets the owning slot, while a value
                    // projection of the same field produces a descriptor.
                    let _ = self.place(object, depth + 1)?;
                    let field = aggregates::field(&ty, member, expression.span)?.1;
                    return self.known(&field, expression.span);
                }
                if let LocalType::Address(variable, _) = probe
                    && let Some(ty @ Type::Package { .. }) = self.graph.ty(variable)?
                {
                    self.graph.reserve_type(&ty, 0, expression.span)?;
                    let field = aggregates::field(&ty, member, expression.span)?.1;
                    if matches!(&field, Type::Managed(payload) if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value())
                    {
                        // A replacement target is the actual owning slot.
                        // Ordinary member reads still project a descriptor.
                        self.mark_address_write(object, depth + 1)?;
                        return self.known(&field, expression.span);
                    }
                }
                let base = if matches!(probe, LocalType::Resource(..)) {
                    self.mark_resource_write(probe, expression.span)?;
                    probe
                } else if matches!(probe, LocalType::Address(..)) {
                    self.mark_address_write(object, depth + 1)?;
                    probe
                } else {
                    self.place(object, depth + 1)?
                };
                self.project(base, Projection::Field(member.clone()), expression.span)
            }
            CoreExprKind::Index { object, index } => {
                let probe = self.expr(object, depth + 1)?;
                let base = if matches!(probe, LocalType::Address(..)) {
                    self.mark_address_write(object, depth + 1)?;
                    probe
                } else {
                    self.place(object, depth + 1)?
                };
                let index_type = self.expr(index, depth + 1)?.scalar(index.span)?;
                self.graph.constrain(index_type, INTEGER, index.span)?;
                self.project(base, Projection::Element, expression.span)
            }
            _ => Err(unsupported(
                expression.span,
                "closure assignment requires a local or Copy projection",
            )),
        }
    }
    fn expr(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        self.expr_expected(expression, None, depth)
    }
    fn expr_expected(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
    ) -> R<LocalType> {
        if let Some(sum) = expected.filter(|ty| enums::anonymous(ty))
            && !matches!(
                &expression.kind,
                CoreExprKind::If { .. } | CoreExprKind::Sequence { .. }
            )
        {
            self.graph.reserve_type(sum, 0, expression.span)?;
            self.graph.reserve_type(sum, 0, expression.span)?;
            let value = self.expr_raw(expression, None, depth)?;
            let actual = self
                .graph
                .injection_type(value.scalar(expression.span)?, expression.span)?;
            enums::anonymous_injection(&actual, sum, expression.span)?;
            return self.known(sum, expression.span);
        }
        if let Some(union @ Type::ErrorUnion { .. }) = expected
            && !matches!(
                &expression.kind,
                CoreExprKind::If { .. } | CoreExprKind::Sequence { .. }
            )
        {
            let value = self.expr_raw(expression, None, depth)?;
            let actual = self
                .graph
                .injection_type(value.scalar(expression.span)?, expression.span)?;
            errors::injection(&actual, union, expression.span)?;
            return self.known(union, expression.span);
        }
        self.expr_raw(expression, expected, depth)
    }
    // Match the source prepass: recursive variants retain only their own
    // temporaries, so nested projections do not accumulate giant frames.
    fn expr_raw(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
    ) -> R<LocalType> {
        self.visit(depth, expression.span)?;
        let span = expression.span;
        match &expression.kind {
            CoreExprKind::Literal(value) => {
                foreign::check_literal(value, span)?;
                self.known(&value.ty(), span)
            }
            CoreExprKind::Float(text) => {
                self.graph
                    .spend_count(zil_base::numeric::decimal_work(text.len()), span)?;
                let domain = if text.ends_with("f32") {
                    F32
                } else if text.ends_with("f64") {
                    F64
                } else {
                    FLOAT
                };
                let variable = self.graph.fresh(domain, domain == FLOAT);
                if let Some(expected) = expected.filter(|ty| ty.is_float()) {
                    self.graph.bind(variable, expected, span)?;
                }
                Ok(LocalType::Scalar(variable))
            }
            CoreExprKind::NumericConvert { value, target } => {
                if !target.is_numeric() {
                    return Err(error(
                        "E-NUMERIC-CONVERSION",
                        span,
                        "numeric target required",
                    ));
                }
                let value = self.expr(value, depth + 1)?.scalar(value.span)?;
                self.graph.constrain(value, NUMBER, span)?;
                self.known(target, span)
            }
            CoreExprKind::IntegerConvert { .. } => self.expr_integer_convert(expression, depth),
            CoreExprKind::Ascribed { .. } => self.expr_ascribed(expression, depth),
            CoreExprKind::Try(..) => self.expr_try(expression, depth),
            CoreExprKind::Integer(text) => {
                let variable = self.graph.fresh(
                    INTEGER,
                    !text.ends_with("u8")
                        && !text.ends_with("u16")
                        && !text.ends_with("i32")
                        && !text.ends_with("i64")
                        && !text.ends_with("u64")
                        && !text.ends_with("usize"),
                );
                if text.ends_with("u8") {
                    self.graph.bind(variable, &Type::U8, span)?;
                }
                if text.ends_with("u16") {
                    self.graph.bind(variable, &Type::U16, span)?;
                }
                if text.ends_with("i32") {
                    self.graph.bind(variable, &Type::I32, span)?;
                }
                if text.ends_with("i64") {
                    self.graph.bind(variable, &Type::I64, span)?;
                }
                if text.ends_with("u64") {
                    self.graph.bind(variable, &Type::U64, span)?;
                }
                if text.ends_with("usize") {
                    self.graph.bind(variable, &Type::Usize, span)?;
                }
                Ok(LocalType::Scalar(variable))
            }
            CoreExprKind::String(_) => self.known(&utf8_literals::ty(), span),
            CoreExprKind::Bool(_) => Ok(LocalType::Scalar(self.graph.fresh(BOOL, false))),
            CoreExprKind::Unit => Ok(LocalType::Scalar(self.graph.fresh(UNIT, false))),
            CoreExprKind::Name(name) => {
                let value = self.name(name, false, span)?;
                if matches!(expected, Some(Type::Managed(_)))
                    && self.prepared.contains(name)
                    && let Some(LocalType::Resource(variable, true)) = self.lookup(name)
                {
                    return Ok(LocalType::Resource(variable, true));
                }
                Ok(value)
            }
            CoreExprKind::Unary { .. } => self.expr_unary(expression, depth),
            CoreExprKind::Binary { .. } => self.expr_binary(expression, depth),
            CoreExprKind::Call { .. } => self.expr_call(expression, expected, depth),
            CoreExprKind::Tuple(..) => self.expr_tuple(expression, depth),
            CoreExprKind::Array(..) => self.expr_array(expression, depth),
            CoreExprKind::Record { .. } => self.expr_record(expression, depth),
            CoreExprKind::RecordUpdate { .. } => self.expr_record_update(expression, depth),
            CoreExprKind::Member { .. } => self.expr_member(expression, depth),
            CoreExprKind::Index { .. } => self.expr_index(expression, depth),
            CoreExprKind::Slice { .. } => self.expr_slice(expression, depth),
            CoreExprKind::If { .. } => self.expr_if(expression, expected, depth),
            CoreExprKind::Sequence { .. } => self.expr_sequence(expression, expected, depth),
            CoreExprKind::Closure { .. } => self.expr_closure(expression, depth),
            CoreExprKind::Unsupported(feature) => Err(unsupported(span, feature)),
            _ => Err(unsupported(span, "non-scalar expression in closure body")),
        }
    }
    #[inline(never)]
    fn expr_integer_convert(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::IntegerConvert { value, target } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        if !target.is_integer() {
            return Err(error(
                "E-INTEGER-CONVERSION",
                span,
                "conversion target must be an integer",
            ));
        }
        let source = self.expr(value, depth + 1)?.scalar(value.span)?;
        self.graph.constrain(source, INTEGER, value.span)?;
        self.known(target, span)
    }

    #[inline(never)]
    fn expr_ascribed(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::Ascribed { ty, value } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let term = self.expr_expected(value, Some(ty), depth + 1)?;
        term.bind(&mut self.graph, ty, span)?;
        Ok(term)
    }

    #[inline(never)]
    fn expr_try(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::Try(operand) = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let ty = self.expr(operand, depth + 1)?.scalar(span)?;
        let Type::ErrorUnion {
            error: failure,
            result,
        } = self.graph.injection_type(ty, span)?
        else {
            return Err(error(
                "E-TRY-NOT-ERROR",
                span,
                "? requires an error-union operand",
            ));
        };
        let Some(Type::ErrorUnion {
            error: expected, ..
        }) = self.graph.ty(self.result)?
        else {
            return Err(error(
                "E-TRY-RETURN-TYPE",
                span,
                "? requires an explicitly known enclosing error-union result",
            ));
        };
        if failure != expected {
            return Err(error(
                "E-TRY-ERROR-TYPE",
                span,
                "? error type must exactly match the enclosing error type",
            ));
        }
        self.known(&result, span)
    }

    #[inline(never)]
    fn expr_unary(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::Unary { op, operand } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let value = self.expr(operand, depth + 1)?;
        if *op == UnaryOp::Give {
            return self.give(operand, value, span);
        }
        if matches!(op, UnaryOp::Borrow | UnaryOp::BorrowMut) {
            if !matches!(
                operand.kind,
                CoreExprKind::Name(_)
                    | CoreExprKind::Member { .. }
                    | CoreExprKind::Index { .. }
                    | CoreExprKind::Slice { .. }
                    | CoreExprKind::Unary {
                        op: UnaryOp::Dereference,
                        ..
                    }
            ) {
                return Err(unsupported(span, "shared borrow requires a place"));
            }
            if let LocalType::Closure(id, None, declared) = value {
                if *op == UnaryOp::BorrowMut {
                    self.name_if_capture_written(operand);
                }
                return Ok(LocalType::Closure(
                    id,
                    Some(*op == UnaryOp::BorrowMut),
                    declared,
                ));
            }
            if let LocalType::Nested(index, None) = value {
                if *op == UnaryOp::BorrowMut {
                    self.name_if_capture_written(operand);
                }
                return Ok(LocalType::Nested(index, Some(*op == UnaryOp::BorrowMut)));
            }
            if *op == UnaryOp::BorrowMut
                && let LocalType::Resource(variable, _) = value
            {
                let payload = self
                    .graph
                    .ty(variable)?
                    .ok_or_else(|| unsupported(span, "task head payload is unresolved"))?;
                if !matches!(payload, Type::TaskI32 | Type::Task { .. }) {
                    return Err(unsupported(
                        span,
                        "exclusive resource head requires TaskI32",
                    ));
                }
                self.mark_address_write(operand, depth + 1)?;
                *self.work_left = self
                    .work_left
                    .checked_sub(1)
                    .ok_or_else(|| limit("task head graph allocation budget exceeded", span))?;
                let head = self.graph.fresh(VALUE, false);
                self.graph.bind(head, &tasks::owner(), span)?;
                return Ok(LocalType::Address(head, true));
            }
            if matches!(value, LocalType::Address(..)) {
                return Err(unsupported(
                    span,
                    "nested shared addresses are not implemented",
                ));
            }
            if *op == UnaryOp::BorrowMut {
                self.mark_address_write(operand, depth + 1)?;
            }
            return Ok(LocalType::Address(
                value.scalar(span)?,
                *op == UnaryOp::BorrowMut,
            ));
        }
        if *op == UnaryOp::Dereference {
            return match value {
                LocalType::Address(variable, _) => Ok(LocalType::Scalar(variable)),
                LocalType::Scalar(variable) => {
                    let inner = self.graph.fresh(VALUE, false);
                    self.graph.address(variable, inner, None, span)?;
                    Ok(LocalType::Scalar(inner))
                }
                _ => Err(error(
                    "E-OPERATOR-TYPE",
                    span,
                    "dereference requires a shared address",
                )),
            };
        }
        let variable = value.scalar(span)?;
        match op {
            UnaryOp::Not => self.graph.constrain(variable, BOOL, span)?,
            UnaryOp::Negate => self.graph.constrain(variable, NUMBER, span)?,
            UnaryOp::BitNot => self.graph.constrain(variable, INTEGER, span)?,
            _ => return Err(unsupported(span, "address/resource operation in closure")),
        }
        Ok(value)
    }

    #[inline(never)]
    fn expr_binary(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::Binary { op, left, right } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let left = self.expr(left, depth + 1)?.scalar(span)?;
        let right = self.expr(right, depth + 1)?.scalar(span)?;
        self.graph.unify(left, right, span)?;
        let comparison = matches!(
            op,
            BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
        );
        let domain = if matches!(op, BinaryOp::And | BinaryOp::Or) {
            BOOL
        } else if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual) {
            BOOL | NUMBER | HANDLE
        } else {
            NUMBER
        };
        self.graph.constrain(left, domain, span)?;
        Ok(LocalType::Scalar(if comparison {
            self.graph.fresh(BOOL, false)
        } else {
            left
        }))
    }

    #[inline(never)]
    fn expr_call(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
    ) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::Call { callee, arguments } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        if let Some(name) = dynamic_store::primitive(callee) {
            self.visit(depth + 1, callee.span)?;
            let arity = match name {
                "store_new" => 0,
                "store_reserve" | "store_push" | "store_at" | "store_at_mut" => 2,
                _ => 1,
            };
            if arguments.len() != arity {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "Store primitive argument count mismatch",
                ));
            }
            if name == "store_new" {
                let element = expected.and_then(dynamic_store::element).ok_or_else(|| {
                    error(
                        "E-OWNED-STORE-TYPE",
                        span,
                        "store_new requires a concrete Store context",
                    )
                })?;
                self.graph.reserve_type(element, 0, span)?;
                return self.known(&dynamic_store::result_type(name, element), span);
            }
            let first = self.expr(&arguments[0], depth + 1)?;
            let variable = match first {
                LocalType::Resource(variable, _) => variable,
                LocalType::Address(head, _) => {
                    let Some(Type::Managed(payload)) = self.graph.ty(head)? else {
                        return Err(error(
                            "E-OWNED-STORE-TYPE",
                            span,
                            "Store primitive requires a resource description",
                        ));
                    };
                    inference::charge_type_snapshot(
                        &payload,
                        span,
                        self.limits,
                        self.work_left,
                        2,
                    )?;
                    if !matches!(payload.as_ref(), Type::OwnedStore { element }
                        if dynamic_store::inline_element(element)
                            && dynamic_store::supported_element(element))
                    {
                        return Err(error(
                            "E-OWNED-STORE-TYPE",
                            span,
                            "Store primitive requires a resource description",
                        ));
                    }
                    let variable = self.graph.fresh(VALUE, false);
                    self.graph.bind(variable, &payload, span)?;
                    variable
                }
                _ => {
                    return Err(error(
                        "E-OWNED-STORE-TYPE",
                        span,
                        "Store primitive requires a resource description",
                    ));
                }
            };
            let Some(Type::OwnedStore { element }) = self.graph.ty(variable)? else {
                return Err(error(
                    "E-OWNED-STORE-TYPE",
                    span,
                    "Store primitive requires a concrete Store element",
                ));
            };
            self.graph.reserve_type(&element, 0, span)?;
            if matches!(
                name,
                "store_reserve" | "store_push" | "store_at" | "store_at_mut"
            ) {
                let wanted = if matches!(name, "store_reserve" | "store_at" | "store_at_mut") {
                    Type::I64
                } else {
                    (*element).clone()
                };
                let value = self.expr_expected(&arguments[1], Some(&wanted), depth + 1)?;
                value.bind(&mut self.graph, &wanted, arguments[1].span)?;
            }
            if matches!(
                name,
                "store_reserve" | "store_push" | "store_pop" | "store_at_mut"
            ) {
                self.mark_resource_write(first, arguments[0].span)?;
            }
            return self.known(&dynamic_store::result_type(name, &element), span);
        }
        if let Some(name) = tasks::primitive(callee) {
            self.visit(depth + 1, callee.span)?;
            self.graph.spend_count(12, span)?;
            if matches!(name, "task_create" | "task_await") {
                let Some(worker) = arguments.first() else {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "task_create requires a static worker",
                    ));
                };
                self.visit(depth + 1, worker.span)?;
                if !self.direct(worker) {
                    return Err(error(
                        "E-TASK-WORKER",
                        span,
                        "task_create requires a static worker",
                    ));
                }
                let signature = self.callee(worker)?.clone();
                if arguments.len() != signature.parameters.len() + 1 {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "task_create actual input count mismatch",
                    ));
                }
                for (argument, wanted) in arguments[1..].iter().zip(&signature.parameters) {
                    let term = self.expr_expected(argument, Some(wanted), depth + 1)?;
                    term.bind(&mut self.graph, wanted, argument.span)?;
                }
                for _ in 0..4 {
                    self.graph.reserve_type(&signature.result, 0, span)?;
                }
                return self.known(
                    &if name == "task_await" {
                        signature.result
                    } else {
                        tasks::create_type(&signature.result)
                    },
                    span,
                );
            }
            let offset = usize::from(matches!(name, "task_start" | "task_await"));
            let parameters = tasks::parameters(name);
            if arguments.len() != parameters.len() + offset {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "task primitive argument count mismatch",
                ));
            }
            if offset != 0 {
                self.visit(depth + 1, arguments[0].span)?;
                if !self.direct(&arguments[0]) {
                    return Err(error(
                        "E-TASK-WORKER",
                        span,
                        "task worker must be a static function",
                    ));
                }
                let signature = self.callee(&arguments[0])?;
                if !signature.scalar_thread_worker()
                    || signature.parameters.as_slice() != [Type::I32]
                {
                    return Err(error(
                        "E-TASK-WORKER",
                        span,
                        "task worker requires fn(i32)->i32",
                    ));
                }
            }
            let mut returned = tasks::result(name);
            for (argument, wanted) in arguments[offset..].iter().zip(&parameters) {
                let term = self.expr_expected(argument, None, depth + 1)?;
                let variable = match term {
                    LocalType::Address(v, _) | LocalType::Resource(v, _) | LocalType::Scalar(v) => {
                        v
                    }
                    _ => {
                        return Err(unsupported(
                            span,
                            "task operand has unresolved callable shape",
                        ));
                    }
                };
                if let Some(actual) = self.graph.ty(variable)?
                    && let Some(task @ Type::Task { .. }) = tasks::payload(&actual)
                {
                    for _ in 0..6 {
                        self.graph.reserve_type(task, 0, span)?;
                    }
                    let parameter = tasks::parameter_for(name, task);
                    term.bind(&mut self.graph, &parameter, argument.span)?;
                    returned = tasks::result_for(name, task);
                } else {
                    term.bind(&mut self.graph, wanted, argument.span)?;
                }
            }
            return self.known(&returned, span);
        }
        if let Some(name) = byte_buffer::primitive(callee) {
            self.visit(depth + 1, callee.span)?;
            let parameters = byte_buffer::parameters(name);
            if parameters.len() != arguments.len() {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "byte storage primitive argument count mismatch",
                ));
            }
            for (index, (argument, ty)) in arguments.iter().zip(&parameters).enumerate() {
                let value = self.expr_expected(argument, Some(ty), depth + 1)?;
                value.bind(&mut self.graph, ty, argument.span)?;
                if index == 0
                    && matches!(
                        name,
                        "bytes_reserve" | "bytes_truncate" | "bytes_push" | "bytes_view_mut"
                    )
                {
                    self.mark_resource_write(value, argument.span)?;
                }
            }
            return self.known(
                &byte_buffer::result_type(name).expect("byte primitive"),
                span,
            );
        }
        if matches!(&callee.kind,CoreExprKind::Name(name) if name=="atomic_compare_exchange") {
            self.visit(depth + 1, callee.span)?;
            self.graph.spend_count(8, span)?;
            if arguments.len() != 3 {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "atomic_compare_exchange requires three arguments",
                ));
            }
            let address = scoped_thread::sync_address(SyncCellKind::AtomicI32);
            for (index, argument) in arguments.iter().enumerate() {
                let wanted = if index == 0 { &address } else { &Type::I32 };
                let term = self.expr_expected(argument, Some(wanted), depth + 1)?;
                self.graph.thread_work(self.work_left, |graph| {
                    term.bind(graph, wanted, argument.span)
                })?;
            }
            return self.known(&Type::Tuple(vec![Type::I32, Type::Bool]), span);
        }
        if let CoreExprKind::Name(name) = &callee.kind
            && let Some((kind, op)) = scoped_thread::sync_primitive(name)
        {
            self.visit(depth + 1, callee.span)?;
            *self.work_left = self
                .work_left
                .checked_sub(8)
                .ok_or_else(|| limit("cell operation type construction budget exceeded", span))?;
            let address_ty = scoped_thread::sync_address(kind);
            if let Some(op) = op {
                let arity = if op == SyncI32Op::Load { 1 } else { 2 };
                if arguments.len() != arity {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "incorrect synchronization operation arity",
                    ));
                }
                for (index, argument) in arguments.iter().enumerate() {
                    let ty = if index == 0 { &address_ty } else { &Type::I32 };
                    let term = self.expr_expected(argument, Some(ty), depth + 1)?;
                    self.graph
                        .thread_work(self.work_left, |graph| term.bind(graph, ty, argument.span))?;
                }
                return self.known(
                    &if op == SyncI32Op::Store {
                        Type::Unit
                    } else {
                        Type::I32
                    },
                    span,
                );
            }
            let [initial, body] = arguments.as_slice() else {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "cell scope requires initial i32 and body",
                ));
            };
            let term = self.expr_expected(initial, Some(&Type::I32), depth + 1)?;
            self.graph.thread_work(self.work_left, |graph| {
                term.bind(graph, &Type::I32, initial.span)
            })?;
            let term = self.expr(body, depth + 1)?;
            let signature = match term {
                LocalType::Nested(index, None) => self.nested_signature(index, span)?,
                LocalType::Scalar(variable) => self
                    .graph
                    .thread_query(self.work_left, |graph| graph.ty(variable))?
                    .ok_or_else(|| {
                        error(
                            "E-SYNC-BODY",
                            body.span,
                            "cell body signature is unresolved",
                        )
                    })?,
                LocalType::Closure(id, None, _) => {
                    let pending = &self.closures[id.0 as usize];
                    if pending.inference.arity() != 1 {
                        return Err(error(
                            "E-SYNC-BODY",
                            body.span,
                            "cell body takes one sealed address",
                        ));
                    }
                    pending
                        .inference
                        .graph
                        .thread_query(self.work_left, |graph| {
                            let parameter =
                                graph.ty(pending.inference.parameters[0])?.ok_or_else(|| {
                                    error(
                                        "E-SYNC-BODY",
                                        body.span,
                                        "cell body parameter is unresolved",
                                    )
                                })?;
                            let result = graph.ty(pending.inference.result)?.ok_or_else(|| {
                                error("E-SYNC-BODY", body.span, "cell body result is unresolved")
                            })?;
                            Ok(Type::FunctionPointer {
                                parameters: vec![parameter],
                                result: Box::new(result),
                            })
                        })?
                }
                _ => {
                    return Err(error(
                        "E-SYNC-BODY",
                        body.span,
                        "cell scope requires a concrete closure",
                    ));
                }
            };
            if !matches!(&signature, Type::FunctionPointer { parameters, result } if parameters.as_slice() == [address_ty] && result.as_ref() == &Type::I32)
            {
                return Err(error(
                    "E-SYNC-BODY",
                    body.span,
                    "cell body has wrong sealed cell signature",
                ));
            }
            self.transferred_closure(body, term);
            return self.known(&Type::Tuple(vec![Type::Bool, Type::I32]), span);
        }
        if matches!(&callee.kind,CoreExprKind::Name(name) if name=="thread_scope") {
            self.visit(depth + 1, callee.span)?;
            let [worker, argument, body] = arguments.as_slice() else {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "thread_scope requires a static worker, i32 payload and body",
                ));
            };
            self.visit(depth + 1, worker.span)?;
            self.thread_worker_lookup(worker)?;
            if !self.direct(worker) {
                return Err(error(
                    "E-THREAD-WORKER",
                    worker.span,
                    "thread worker requires an actual static ordinary function symbol",
                ));
            }
            *self.work_left = self
                .work_left
                .checked_sub(1)
                .ok_or_else(|| limit("thread worker signature budget exceeded", span))?;
            let worker_signature = self.callee(worker)?;
            if !worker_signature.scalar_thread_worker() {
                return Err(error(
                    "E-THREAD-WORKER",
                    worker.span,
                    "thread worker must be a concrete ordinary fn(i32)->i32 or fn(&i32)->i32",
                ));
            }
            let shared_payload = matches!(&worker_signature.parameters[0], Type::Address { .. });
            let cell_kind = match &worker_signature.parameters[0] {
                Type::Address { inner, .. } => match inner.as_ref() {
                    Type::SyncCell(kind) => Some(*kind),
                    _ => None,
                },
                _ => None,
            };
            if shared_payload {
                *self.work_left = self.work_left.checked_sub(2).ok_or_else(|| {
                    limit("thread shared payload construction budget exceeded", span)
                })?;
            }
            let payload_type = if shared_payload {
                Type::Address {
                    mutable: false,
                    inner: Box::new(cell_kind.map_or(Type::I32, Type::SyncCell)),
                }
            } else {
                Type::I32
            };
            // The accepted shapes are closed. Shared construction is
            // prepaid above; graph traversal reserves the actual type.
            self.graph.thread_query(self.work_left, |graph| {
                graph.reserve_type(&payload_type, 0, worker.span)?;
                graph.reserve_type(&Type::I32, 0, worker.span)
            })?;
            let value = self.expr_expected(argument, Some(&payload_type), depth + 1)?;
            self.graph.thread_work(self.work_left, |graph| {
                value.bind(graph, &payload_type, argument.span)
            })?;
            let value = self.expr(body, depth + 1)?;
            let signature = match value {
                LocalType::Nested(index, None) => {
                    let signature = &self.nested_signatures[index];
                    self.graph.thread_query(self.work_left, |graph| {
                        graph.reserve_type(signature, 0, span)?;
                        Ok(signature.clone())
                    })?
                }
                // The legacy Copy inference path keeps a private
                // function signature for nested source closures.
                // Builder must still prove an actual Closure value;
                // this signature never grants a thin pointer authority.
                LocalType::Scalar(variable) => self
                    .graph
                    .thread_query(self.work_left, |graph| graph.ty(variable))?
                    .ok_or_else(|| {
                        error(
                            "E-CLOSURE-INFERENCE",
                            span,
                            "thread body signature is unresolved",
                        )
                    })?,
                LocalType::Closure(id, None, _) => {
                    let pending = &self.closures[id.0 as usize];
                    if pending.inference.arity() != 0 {
                        return Err(error(
                            "E-THREAD-BODY",
                            body.span,
                            "thread body signature is ()->i32",
                        ));
                    }
                    *self.work_left = self.work_left.checked_sub(1).ok_or_else(|| {
                        limit("thread signature construction budget exceeded", span)
                    })?;
                    let result = pending
                        .inference
                        .graph
                        .thread_query(self.work_left, |graph| graph.ty(pending.inference.result))?;
                    Type::FunctionPointer {
                        parameters: vec![],
                        result: Box::new(result.ok_or_else(|| {
                            error(
                                "E-CLOSURE-INFERENCE",
                                span,
                                "thread body result is unresolved",
                            )
                        })?),
                    }
                }
                _ => {
                    return Err(error(
                        "E-THREAD-BODY",
                        body.span,
                        "thread body must be a concrete closure value",
                    ));
                }
            };
            *self.work_left = self
                .work_left
                .checked_sub(2)
                .ok_or_else(|| limit("thread signature comparison budget exceeded", span))?;
            if !matches!(&signature, Type::FunctionPointer { parameters, result } if parameters.is_empty() && result.as_ref() == &Type::I32)
            {
                return Err(error(
                    "E-THREAD-BODY",
                    body.span,
                    "thread body signature is ()->i32",
                ));
            }
            // The prepared body is transferred once, even when create
            // fails and only its environment cleanup executes.
            if let CoreExprKind::Name(name) = &body.kind {
                let width = name.len().saturating_add(1);
                *self.work_left = self
                    .work_left
                    .checked_sub(self.scopes.len())
                    .ok_or_else(|| limit("thread body lookup budget exceeded", span))?;
                let lookup = self.scopes.iter().fold(0usize, |work, scope| {
                    work.saturating_add(helpers::plan::tree_work(scope.len()).saturating_mul(width))
                });
                *self.work_left = self
                    .work_left
                    .checked_sub(lookup)
                    .ok_or_else(|| limit("thread body lookup budget exceeded", span))?;
                if self.lookup(name).is_none() {
                    let scan = self.captures.len().saturating_mul(width);
                    *self.work_left = self
                        .work_left
                        .checked_sub(scan)
                        .ok_or_else(|| limit("thread capture scan budget exceeded", span))?;
                    if let Some(capture) = self
                        .captures
                        .iter_mut()
                        .find(|capture| capture.name == *name)
                    {
                        capture.moved = true;
                        capture.borrowed = false;
                        capture.consumed = true;
                    }
                }
            }
            return self.graph.thread_work(self.work_left, |graph| {
                graph.spend_count(4, span)?;
                let ty = Type::Tuple(vec![Type::Bool, Type::I32, Type::I32]);
                let variable = graph.fresh(VALUE, false);
                graph.bind(variable, &ty, span)?;
                Ok(LocalType::Scalar(variable))
            });
        }
        if matches!(&callee.kind,CoreExprKind::Name(name) if name=="c_with_callback") {
            self.visit(depth + 1, callee.span)?;
            let [register, close, callback, body] = arguments.as_slice() else {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "c_with_callback requires four arguments",
                ));
            };
            for (expression, role) in [
                (register, ScopedCallbackRole::Register),
                (close, ScopedCallbackRole::Close),
            ] {
                self.visit(depth + 1, expression.span)?;
                let signature = self.callee(expression)?;
                self.reserve_signature(signature, expression.span)?;
                if signature.scoped_callback != Some(role) {
                    return Err(error(
                        "E-FFI-SCOPED-ROLE",
                        expression.span,
                        "scoped callback role differs from its actual declaration",
                    ));
                }
            }
            let value = self.expr(callback, depth + 1)?;
            let signature = match value {
                LocalType::Nested(index, Some(true)) => self.nested_signature(index, span)?,
                LocalType::Closure(id, Some(true), declared) => {
                    let pending = &self.closures[id.0 as usize];
                    if declared == Some(CallableKind::FnOnce)
                        || pending.callable_kind() == CallableKind::FnOnce
                    {
                        return Err(error(
                            "E-CLOSURE-MODE",
                            span,
                            "scoped callback cannot consume its environment",
                        ));
                    }
                    Type::FunctionPointer {
                        parameters: (0..pending.inference.arity())
                            .map(|i| {
                                pending.inference.parameter(i)?.ok_or_else(|| {
                                    error(
                                        "E-CLOSURE-INFERENCE",
                                        span,
                                        "scoped callback signature is unresolved",
                                    )
                                })
                            })
                            .collect::<R<Vec<_>>>()?,
                        result: Box::new(pending.inference.result()?.ok_or_else(|| {
                            error(
                                "E-CLOSURE-INFERENCE",
                                span,
                                "scoped callback result is unresolved",
                            )
                        })?),
                    }
                }
                _ => {
                    return Err(error(
                        "E-FFI-SCOPED-CALLBACK",
                        span,
                        "scoped callback requires a concrete exclusive closure position",
                    ));
                }
            };
            if signature
                != (Type::FunctionPointer {
                    parameters: vec![Type::I32],
                    result: Box::new(Type::I32),
                })
            {
                return Err(error(
                    "E-FFI-SCOPED-CALLBACK",
                    span,
                    "scoped callback signature is (i32)->i32",
                ));
            }
            self.mark_address_write(callback, depth + 1)?;
            let value = self.expr(body, depth + 1)?;
            let signature = match value {
                LocalType::Nested(index, None) => self.nested_signature(index, span)?,
                LocalType::Closure(id, None, _) => {
                    let pending = &self.closures[id.0 as usize];
                    Type::FunctionPointer {
                        parameters: (0..pending.inference.arity())
                            .map(|i| {
                                pending.inference.parameter(i)?.ok_or_else(|| {
                                    error(
                                        "E-CLOSURE-INFERENCE",
                                        span,
                                        "scoped body signature is unresolved",
                                    )
                                })
                            })
                            .collect::<R<Vec<_>>>()?,
                        result: Box::new(pending.inference.result()?.ok_or_else(|| {
                            error(
                                "E-CLOSURE-INFERENCE",
                                span,
                                "scoped body result is unresolved",
                            )
                        })?),
                    }
                }
                _ => {
                    return Err(error(
                        "E-FFI-SCOPED-BODY",
                        span,
                        "scoped body must be a concrete closure value",
                    ));
                }
            };
            if signature
                != (Type::FunctionPointer {
                    parameters: vec![],
                    result: Box::new(Type::I32),
                })
            {
                return Err(error(
                    "E-FFI-SCOPED-BODY",
                    span,
                    "scoped body signature is ()->i32",
                ));
            }
            if let CoreExprKind::Name(name) = &body.kind
                && self.lookup(name).is_none()
                && let Some(capture) = self
                    .captures
                    .iter_mut()
                    .find(|capture| capture.name == *name)
            {
                capture.moved = true;
                capture.borrowed = false;
                capture.consumed = true;
            }
            return self.known(&Type::Tuple(vec![Type::Bool, Type::I32]), span);
        }
        if foreign::callback_constructor(callee) {
            self.visit(depth + 1, callee.span)?;
            let [target] = arguments.as_slice() else {
                return Err(error(
                    "E-FFI-CALLBACK",
                    span,
                    "c_callback requires one static ordinary function symbol",
                ));
            };
            if !self.direct(target) {
                return Err(error(
                    "E-FFI-CALLBACK",
                    target.span,
                    "stored callables and closures cannot become C callbacks",
                ));
            }
            self.visit(depth + 1, target.span)?;
            let signature = self.callee(target)?;
            self.reserve_signature(signature, span)?;
            let ty = signature.foreign_callback_type().ok_or_else(|| {
                error(
                    "E-FFI-CALLBACK",
                    target.span,
                    "C callback target has an unsupported signature or contract",
                )
            })?;
            return self.known(&ty, span);
        }
        if let Some((ty, variant)) = self.constructor(callee)? {
            self.visit(depth + 1, callee.span)?;
            self.visit(depth + 2, callee.span)?;
            return self.variant(&ty, variant, arguments, depth, span);
        }
        if !self.direct(callee) {
            // Infer the receiver once. Function-valued fields and slice
            // len keep their existing precedence over nominal methods.
            let callee_type = if let CoreExprKind::Member {
                object,
                member,
                owner,
            } = &callee.kind
            {
                self.visit(depth + 1, callee.span)?;
                let base = self.expr(object, depth + 2)?;
                self.owner_type(base, owner.as_ref(), callee.span)?;
                if let Some(result) =
                    self.nominal_method_call(base, object, member, arguments, depth, span)?
                {
                    return Ok(result);
                }
                if member == "len"
                    && matches!(base, LocalType::Address(v, _) if self.graph.is_slice(v))
                {
                    LocalType::SliceLength
                } else {
                    self.project(base, Projection::Field(member.clone()), callee.span)?
                }
            } else {
                self.expr(callee, depth + 1)?
            };
            if matches!(callee_type, LocalType::SliceLength) {
                if !arguments.is_empty() {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "slice len requires no arguments",
                    ));
                }
                return self.known(&Type::I64, span);
            }
            if let LocalType::Closure(id, permission, declared) = callee_type {
                let pending = self.closures.get(id.0 as usize).ok_or_else(|| {
                    error(
                        "E-CLOSURE-INFERENCE",
                        span,
                        "concrete captured closure is absent",
                    )
                })?;
                let kind = declared.unwrap_or_else(|| pending.callable_kind());
                if permission.is_some()
                    && (kind == CallableKind::FnOnce
                        || (kind == CallableKind::FnMut && permission == Some(false)))
                {
                    return Err(error(
                        "E-CLOSURE-MODE",
                        span,
                        "borrowed closure lacks its call permission",
                    ));
                }
                if arguments.len() != pending.inference.arity() {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "captured callback argument count differs",
                    ));
                }
                let parameters = (0..arguments.len())
                    .map(|i| pending.inference.parameter(i))
                    .collect::<R<Vec<_>>>()?;
                let result = pending.inference.result()?.ok_or_else(|| {
                    error(
                        "E-CLOSURE-INFERENCE",
                        span,
                        "captured closure result is unresolved",
                    )
                })?;
                if let CoreExprKind::Name(name) = &callee.kind
                    && self.lookup(name).is_none()
                    && let Some(capture) = self
                        .captures
                        .iter_mut()
                        .find(|capture| capture.name == *name)
                {
                    if kind == CallableKind::FnOnce {
                        capture.moved = true;
                        capture.borrowed = false;
                        capture.consumed = true;
                    } else if kind == CallableKind::FnMut {
                        capture.written = true;
                    }
                }
                for (argument, parameter) in arguments.iter().zip(parameters) {
                    let parameter = parameter.ok_or_else(|| {
                        error(
                            "E-CLOSURE-INFERENCE",
                            span,
                            "captured closure parameter is unresolved",
                        )
                    })?;
                    let value = self.expr_expected(argument, Some(&parameter), depth + 1)?;
                    value.bind(&mut self.graph, &parameter, argument.span)?;
                }
                return self.known(&result, span);
            }
            if let LocalType::Nested(index, _) = callee_type {
                let Type::FunctionPointer { parameters, result } =
                    self.nested_signature(index, span)?
                else {
                    unreachable!("private nested signature")
                };
                if arguments.len() != parameters.len() {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "nested callback argument count differs",
                    ));
                }
                for (argument, parameter) in arguments.iter().zip(&parameters) {
                    self.expr_expected(argument, Some(parameter), depth + 1)?
                        .bind(&mut self.graph, parameter, argument.span)?;
                }
                return self.known(&result, span);
            }
            let callee_type = callee_type.scalar(callee.span)?;
            if let Some(Type::BorrowedFunctionPointer {
                parameters, result, ..
            }) = self.graph.ty(callee_type)?
            {
                if parameters.len() != arguments.len() {
                    return Err(error(
                        "E-CALL-ARITY",
                        span,
                        "indirect callback arity differs from its signature",
                    ));
                }
                for (argument, parameter) in arguments.iter().zip(&parameters) {
                    self.expr_expected(argument, Some(parameter), depth + 1)?
                        .bind(&mut self.graph, parameter, argument.span)?;
                    if matches!(parameter, Type::Address { mutable: true, .. }) {
                        self.mark_address_write(argument, depth + 1)?;
                    }
                }
                return self.known(&result, span);
            }
            let mut parameters = Vec::new();
            for argument in arguments {
                parameters.push(self.expr(argument, depth + 1)?.scalar(argument.span)?);
            }
            let result = self.graph.fresh(VALUE, false);
            self.graph
                .function_pointer(callee_type, parameters, result, span)?;
            return Ok(LocalType::Scalar(result));
        }
        let signature = self.callee(callee)?;
        self.reserve_signature(signature, span)?;
        let signature = signature.clone();
        if arguments.len() != signature.parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "closure body call argument count mismatch",
            ));
        }
        let mut nested_result = None;
        for (index, (argument, parameter)) in
            arguments.iter().zip(&signature.parameters).enumerate()
        {
            self.graph.spend_count(signature.callable.len(), span)?;
            if let Some(bound) = signature
                .callable
                .iter()
                .find(|bound| bound.index as usize == index)
                && let Some(mode) = helpers::parameter_mode(parameter, &bound.generic_name)
            {
                let value = self.helper_argument(argument, bound, mode, depth + 1)?;
                if matches!(value, LocalType::Nested(_, None))
                    && mode.is_none()
                    && matches!(&signature.result, Type::Named(name) if name == &bound.generic_name)
                {
                    nested_result = Some(value);
                }
                continue;
            }
            let variable = self.expr_expected(argument, Some(parameter), depth + 1)?;
            if matches!(parameter, Type::Address { mutable: true, .. }) {
                self.mark_address_write(argument, depth + 1)?;
            }
            if matches!(parameter, Type::Desc(_))
                && self
                    .resource_writes
                    .get(&resource_effects::target(&signature))
                    .is_some_and(|writes| writes.contains(&index))
            {
                self.mark_resource_write(variable, argument.span)?;
            }
            variable.bind(&mut self.graph, parameter, argument.span)?;
            if matches!(
                parameter,
                Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
            ) && let CoreExprKind::Name(name) = &argument.kind
                && self.lookup(name).is_none()
            {
                *self.work_left = self
                    .work_left
                    .checked_sub(
                        self.captures
                            .len()
                            .saturating_mul(name.len().saturating_add(1)),
                    )
                    .ok_or_else(|| {
                        limit("owned Sum capture transfer budget exceeded", argument.span)
                    })?;
                if let Some(capture) = self
                    .captures
                    .iter_mut()
                    .find(|capture| capture.name == *name)
                {
                    inference::charge_type_snapshot(
                        &capture.ty,
                        argument.span,
                        self.limits,
                        self.work_left,
                        2,
                    )?;
                    if owned_shapes::sum(&capture.ty)
                        && dynamic_store::contains_inline_carrier(&capture.ty)
                    {
                        capture.moved = true;
                        capture.borrowed = false;
                        capture.consumed = true;
                    }
                }
            }
        }
        if let Some(value) = nested_result {
            return Ok(value);
        }
        self.known(&signature.result, span)
    }

    #[inline(never)]
    fn expr_tuple(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::Tuple(items) = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let mut fields = Vec::new();
        for item in items {
            let value = self.expr(item, depth + 1)?;
            let field = if let LocalType::Resource(payload, true) = value {
                if let Some(ty) = self.graph.ty(payload)?
                    && matches!(&ty, Type::OwnedStore { element }
                        if dynamic_store::inline_element(element)
                            && dynamic_store::supported_element(element))
                {
                    let variable = self.graph.fresh(VALUE, false);
                    self.graph
                        .bind(variable, &Type::Managed(Box::new(ty)), item.span)?;
                    variable
                } else {
                    return Err(unsupported(
                        item.span,
                        "ordinary Tuple capture requires a complete inline Store owner",
                    ));
                }
            } else {
                value.scalar(item.span)?
            };
            fields.push(field);
        }
        let result = self.graph.fresh(VALUE, false);
        self.graph.tuple(result, fields, span)?;
        Ok(LocalType::Scalar(result))
    }

    #[inline(never)]
    fn expr_array(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::Array(items) = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let element = self.graph.fresh(VALUE, false);
        for item in items {
            let value = self.expr(item, depth + 1)?.scalar(item.span)?;
            self.graph.unify(element, value, item.span)?;
        }
        let result = self.graph.fresh(VALUE, false);
        self.graph.array(
            result,
            element,
            u32::try_from(items.len()).map_err(|_| limit("array length overflow", span))?,
            span,
        )?;
        Ok(LocalType::Scalar(result))
    }

    #[inline(never)]
    fn expr_record(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::Record { ty, fields } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let payload = match ty {
            Type::Managed(payload) => payload.as_ref(),
            _ => ty,
        };
        for (name, value) in fields {
            let expected = aggregates::field(payload, name, value.span)?.1;
            let actual = self.expr_expected(value, Some(&expected), depth + 1)?;
            actual.bind(&mut self.graph, &expected, value.span)?;
        }
        self.known(ty, span)
    }

    #[inline(never)]
    fn expr_record_update(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::RecordUpdate { ty, base, fields } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let base = self.expr(base, depth + 1)?;
        if let Some(ty) = ty {
            base.bind(&mut self.graph, ty, span)?;
        }
        for (name, value) in fields {
            let target = self.project(base, Projection::Field(name.clone()), value.span)?;
            let value = self.expr(value, depth + 1)?;
            target.unify(value, &mut self.graph, span)?;
        }
        Ok(base)
    }

    #[inline(never)]
    fn expr_member(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::Member {
            object,
            member,
            owner,
        } = &expression.kind
        else {
            unreachable!("expression dispatcher variant");
        };
        if let Some((ty, variant)) = self.constructor(expression)? {
            self.visit(depth + 1, object.span)?;
            return self.variant(&ty, variant, &[], depth, span);
        }
        if self.direct(expression) {
            if owner.is_some() {
                return Err(error(
                    "E-MEMBER-OWNER",
                    span,
                    "a constrained member requires a value receiver, not a static namespace",
                ));
            }
            let ty = self.pointer_type(self.callee(expression)?, span)?;
            return self.known(&ty, span);
        }
        let base = self.expr(object, depth + 1)?;
        self.owner_type(base, owner.as_ref(), span)?;
        if member == "len"
            && matches!(base, LocalType::Address(variable, _) if self.graph.is_slice(variable))
        {
            return Ok(LocalType::SliceLength);
        }
        self.project(base, Projection::Field(member.clone()), span)
    }

    #[inline(never)]
    fn expr_index(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::Index { object, index } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let base = self.expr(object, depth + 1)?;
        let index_type = self.expr(index, depth + 1)?.scalar(index.span)?;
        self.graph.constrain(index_type, INTEGER, index.span)?;
        self.project(base, Projection::Element, span)
    }

    #[inline(never)]
    fn expr_slice(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::Slice { object, start, end } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        let base = self.expr(object, depth + 1)?;
        for endpoint in [start, end].into_iter().flatten() {
            let variable = self.expr(endpoint, depth + 1)?.scalar(endpoint.span)?;
            self.graph.constrain(variable, INTEGER, endpoint.span)?;
        }
        let element = self
            .project(base, Projection::Element, span)?
            .scalar(span)?;
        let result = self.graph.fresh(VALUE, false);
        self.graph.slice(result, element, span)?;
        Ok(LocalType::Scalar(result))
    }

    #[inline(never)]
    fn expr_if(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
    ) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::If {
            condition,
            then_value,
            else_value,
        } = &expression.kind
        else {
            unreachable!("expression dispatcher variant");
        };
        let condition_type = self.expr(condition, depth + 1)?;
        condition_type.bind(&mut self.graph, &Type::Bool, condition.span)?;
        let then_type = self.expr_expected(then_value, expected, depth + 1)?;
        let else_type = self.expr_expected(else_value, expected, depth + 1)?;
        then_type.unify(else_type, &mut self.graph, span)?;
        // An owned declared F leaves the selected captured position
        // before the conditional value is bound or called. Nested
        // conditionals handle their own arms during the visit above.
        self.transferred_factory_branch(then_value, then_type)?;
        self.transferred_factory_branch(else_value, else_type)?;
        Ok(then_type)
    }

    #[inline(never)]
    fn expr_sequence(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
        depth: usize,
    ) -> R<LocalType> {
        let CoreExprKind::Sequence { bindings, value } = &expression.kind else {
            unreachable!("expression dispatcher variant");
        };
        self.scopes.push(BTreeMap::new());
        for (name, expression) in bindings {
            let variable = self.expr(expression, depth + 1)?;
            self.scopes
                .last_mut()
                .expect("scope")
                .insert(name.clone(), variable);
            self.prepared.insert(name.clone());
        }
        let result = self.expr_expected(value, expected, depth + 1)?;
        self.scopes.pop();
        Ok(result)
    }

    #[inline(never)]
    fn expr_closure(&mut self, expression: &CoreExpr, depth: usize) -> R<LocalType> {
        let span = expression.span;
        let CoreExprKind::Closure {
            move_capture,
            parameters,
            return_type,
            body,
        } = &expression.kind
        else {
            unreachable!("expression dispatcher variant");
        };
        // Source inference has resolved each nested signature. Visit
        // its body as well so free paths cross every enclosing layer.
        // The graph uses a private callable signature only; Builder
        // still creates the actual environment and ordinary entry.
        let Some(result) = return_type else {
            return Err(unsupported(
                span,
                "nested closure requires a known fn context",
            ));
        };
        let mut types = Vec::with_capacity(parameters.len());
        let outer_result = self.result;
        let outer_has_return = self.has_return;
        self.result = self.graph.fresh(VALUE, false);
        self.graph.bind(self.result, result, span)?;
        self.has_return = false;
        self.scopes.push(BTreeMap::new());
        for parameter in parameters {
            self.visit(depth + 1, parameter.span)?;
            let Some(ty) = &parameter.ty else {
                return Err(unsupported(
                    parameter.span,
                    "nested closure requires a known fn context",
                ));
            };
            self.graph.reserve_type(ty, 0, parameter.span)?;
            types.push(ty.clone());
            let value = self.known(ty, parameter.span)?;
            self.scopes
                .last_mut()
                .expect("nested parameter scope")
                .insert(parameter.name.clone(), value);
        }
        self.nested_moves += usize::from(*move_capture);
        self.nested_depth += 1;
        let outer_nested_root = self.nested_root;
        self.nested_root = self.nested_root.or(Some(expression.id));
        self.block(body, depth + 1)?;
        self.nested_root = outer_nested_root;
        self.nested_depth -= 1;
        self.nested_moves -= usize::from(*move_capture);
        self.scopes.pop();
        self.result = outer_result;
        self.has_return = outer_has_return;
        self.graph.reserve_type(result, 0, span)?;
        let signature = Type::FunctionPointer {
            parameters: types,
            result: Box::new(result.clone()),
        };
        match self.known(&signature, span) {
            // Preserve the old Copy inference path and its budget.
            Ok(value) => Ok(value),
            Err(rejected) if rejected.category == zil_base::ResultCategory::Unsupported => {
                // A private source closure signature can contain owned
                // data. It grants neither public pointer nor Copy rights.
                self.graph.reserve_type(&signature, 0, span)?;
                self.graph.reserve_type(&signature, 0, span)?;
                let Type::FunctionPointer { parameters, result } = &signature else {
                    unreachable!("nested signature")
                };
                if !value_signature(parameters, result) {
                    return Err(rejected);
                }
                self.graph.spend(span)?;
                let index = self.nested_signatures.len();
                self.nested_signatures.push(signature);
                Ok(LocalType::Nested(index, None))
            }
            Err(rejected) => Err(rejected),
        }
    }

    fn constructor(&self, expression: &CoreExpr) -> R<Option<(Type, usize)>> {
        let CoreExprKind::Member {
            object,
            member,
            owner,
        } = &expression.kind
        else {
            return Ok(None);
        };
        let CoreExprKind::Name(name) = &object.kind else {
            return Ok(None);
        };
        if self.lookup(name).is_some() || self.outer.contains_key(name) {
            return Ok(None);
        }
        let Some(types) = self.enums else {
            return Ok(None);
        };
        let Some(ty) = enums::constructor_type(types, name, expression.span)? else {
            return Ok(None);
        };
        let Type::Enum { variants, .. } = stored::physical(ty) else {
            return Ok(None);
        };
        if owner.is_some() {
            return Err(error(
                "E-MEMBER-OWNER",
                expression.span,
                "a constrained member cannot denote an enum constructor namespace",
            ));
        }
        self.graph.reserve_type(ty, 0, expression.span)?;
        let index = variants
            .iter()
            .position(|(name, _)| name == member)
            .ok_or_else(|| error("E-ENUM-VARIANT", expression.span, "unknown enum variant"))?;
        Ok(Some((ty.clone(), index)))
    }
    fn variant(
        &mut self,
        ty: &Type,
        index: usize,
        arguments: &[CoreExpr],
        depth: usize,
        span: Span,
    ) -> R<LocalType> {
        let Type::Enum { variants, .. } = stored::physical(ty) else {
            return Err(error(
                "E-ENUM-TYPE",
                span,
                "enum constructor lacks its actual physical variant layout",
            ));
        };
        let fields = &variants[index].1;
        if fields.len() != arguments.len() {
            return Err(error(
                "E-ENUM-ARITY",
                span,
                "enum variant payload arity mismatch",
            ));
        }
        for (argument, expected) in arguments.iter().zip(fields) {
            self.expr(argument, depth + 1)?
                .bind(&mut self.graph, expected, argument.span)?;
        }
        self.known(ty, span)
    }
    // Both direct() and callee() look through the source scope/outer tables.
    // Reserve their bounded ordered-key paths before either lookup executes.
    fn thread_worker_lookup(&mut self, expression: &CoreExpr) -> R<()> {
        let (name, member) = match &expression.kind {
            CoreExprKind::Name(name) => (name, None),
            CoreExprKind::Member { object, member, .. } => {
                let CoreExprKind::Name(name) = &object.kind else {
                    return Ok(());
                };
                (name, Some(member))
            }
            _ => return Ok(()),
        };
        let span = expression.span;
        *self.work_left = self
            .work_left
            .checked_sub(self.scopes.len())
            .ok_or_else(|| limit("thread worker lookup budget exceeded", span))?;
        let width = name.len().saturating_add(1);
        let scopes = self.scopes.iter().fold(0usize, |sum, scope| {
            sum.saturating_add(helpers::plan::tree_work(scope.len()))
        });
        let mut work = scopes
            .saturating_add(helpers::plan::tree_work(self.outer.len()))
            .saturating_mul(width)
            .saturating_mul(2);
        let signature_width = member.map_or(width, |member| {
            width.saturating_add(member.len()).saturating_add(1)
        });
        work = work.saturating_add(
            helpers::plan::tree_work(self.signatures.len()).saturating_mul(signature_width),
        );
        if let Some(member) = member {
            // The qualified lookup constructs alias.member before consulting
            // the local signature map, then can enter the imported module map.
            work = work.saturating_add(signature_width);
            if let Some(imports) = self.imports {
                *self.work_left = self
                    .work_left
                    .checked_sub(imports.signatures.len())
                    .ok_or_else(|| limit("thread module lookup inventory budget exceeded", span))?;
                let largest = imports
                    .signatures
                    .values()
                    .map(BTreeMap::len)
                    .max()
                    .unwrap_or(0);
                work = work
                    .saturating_add(
                        helpers::plan::tree_work(imports.aliases.len()).saturating_mul(width),
                    )
                    .saturating_add(helpers::plan::tree_work(imports.signatures.len()))
                    .saturating_add(
                        helpers::plan::tree_work(largest)
                            .saturating_mul(member.len().saturating_add(1)),
                    );
            }
        }
        *self.work_left = self
            .work_left
            .checked_sub(work)
            .ok_or_else(|| limit("thread worker ordered-key lookup budget exceeded", span))?;
        Ok(())
    }
    fn direct(&self, expression: &CoreExpr) -> bool {
        let name = match &expression.kind {
            CoreExprKind::Name(name) => name,
            CoreExprKind::Member { object, .. } => {
                let CoreExprKind::Name(name) = &object.kind else {
                    return false;
                };
                name
            }
            _ => return false,
        };
        self.lookup(name).is_none() && !self.outer.contains_key(name)
    }
    fn reserve_signature(&self, signature: &Signature, span: Span) -> R<()> {
        if let Some(life) = &signature.storage_life {
            borrowed_fp::charge_contract(life, |work| self.graph.spend_count(work, span), 1)?;
        }
        if let Some(life) = &signature.life {
            self.graph
                .spend_count(life.parameter_lives.len().saturating_add(1), span)?;
        }
        for ty in signature
            .parameters
            .iter()
            .chain(std::iter::once(&signature.result))
        {
            self.graph.reserve_type(ty, 0, span)?;
        }
        self.graph.spend_count(signature.callable.len(), span)?;
        for bound in &signature.callable {
            self.graph.spend_count(bound.generic_name.len(), span)?;
            for ty in bound
                .parameters
                .iter()
                .chain(std::iter::once(&bound.result))
            {
                self.graph.reserve_type(ty, 0, span)?;
            }
        }
        Ok(())
    }
    fn pointer_type(&self, signature: &Signature, span: Span) -> R<Type> {
        self.reserve_signature(signature, span)?;
        if signature.storage_life.is_some() {
            for ty in &signature.parameters {
                self.graph.reserve_type_uses(ty, 4, span)?;
            }
            self.graph.reserve_type_uses(&signature.result, 5, span)?;
            borrowed_fp::static_pointer_slots(signature, |work| {
                self.graph.spend_count(work, span)
            })?;
        }
        self.graph
            .spend_count(borrowed_fp::pointer_work(signature), span)?;
        signature.pointer_type().ok_or_else(|| unsupported(span,
            "function pointers require pure Copy parameters and result; indirect address contracts are not implemented"))
    }
    fn callee(&self, callee: &CoreExpr) -> R<&Signature> {
        match &callee.kind {
            CoreExprKind::Name(name) => {
                if self.lookup(name).is_some() || self.outer.contains_key(name) {
                    return Err(unsupported(
                        callee.span,
                        "calling a captured or nested callable is not implemented in C1",
                    ));
                }
                self.signatures.get(name).ok_or_else(|| {
                    error(
                        "E-UNKNOWN-NAME",
                        callee.span,
                        format!("unknown function {name}"),
                    )
                })
            }
            CoreExprKind::Member { object, member, .. } => {
                let CoreExprKind::Name(alias) = &object.kind else {
                    return Err(unsupported(callee.span, "indirect member call in closure"));
                };
                if self.lookup(alias).is_some() || self.outer.contains_key(alias) {
                    return Err(unsupported(callee.span, "stored member call in closure"));
                }
                if let Some(signature) = self.signatures.get(&format!("{alias}.{member}")) {
                    return Ok(signature);
                }
                let imports = self.imports.ok_or_else(|| {
                    unsupported(callee.span, "module-qualified call requires a project")
                })?;
                let module = imports.aliases.get(alias).ok_or_else(|| {
                    error(
                        "E-UNKNOWN-NAME",
                        object.span,
                        format!("unknown module alias {alias}"),
                    )
                })?;
                let signature = imports.signatures[module].get(member).ok_or_else(|| {
                    error(
                        "E-MODULE-MEMBER",
                        callee.span,
                        format!("module {alias} has no function {member}"),
                    )
                })?;
                if !signature.public {
                    return Err(error(
                        "E-MODULE-PRIVATE",
                        callee.span,
                        format!("function {alias}.{member} is private"),
                    ));
                }
                Ok(signature)
            }
            _ => Err(unsupported(callee.span, "indirect call in closure body")),
        }
    }
}

#[cfg(test)]
mod prepared_owner_budget {
    use super::*;
    use zil_base::ResultCategory;
    #[test]
    fn remaining_allowance_stops_owner_planning_before_deep_tail_and_is_not_refunded() {
        let limits = Limits {
            max_call_depth: 8,
            ..Limits::default()
        };
        let mut owner = Type::I32;
        for _ in 0..32 {
            owner = Type::Record {
                name: "R".into(),
                fields: vec![("x".into(), owner)],
            };
        }
        let uncapped = inference::type_snapshot_work(&owner, Span::default(), &limits).unwrap_err();
        assert!(uncapped.message.contains("depth"), "{uncapped:?}");
        let mut state = State::new(0, &limits);
        state
            .snapshot(limits.max_analysis_steps - 3, Span::default())
            .unwrap();
        let error = state
            .snapshot_type(&owner, 1, 0, Span::default())
            .unwrap_err();
        assert_eq!(error.category, ResultCategory::ResourceLimit);
        assert!(error.message.contains("work"), "{error:?}");
        assert_eq!(
            state.remaining_work(),
            0,
            "failed visits refunded the caller allowance"
        );
    }
    #[test]
    fn complete_owner_comparison_preserves_exact_n_and_fresh_retry() {
        let limits = Limits::default();
        let owner = Type::Record {
            name: "Owner".into(),
            fields: vec![("nested".into(), Type::Tuple(vec![Type::I32, Type::I64]))],
        };
        let work = inference::type_snapshot_work(&owner, Span::default(), &limits).unwrap();
        let boundary = work * 2 + 2;
        for (allowance, success) in [(boundary, true), (boundary - 1, false), (boundary, true)] {
            let mut state = State::new(0, &limits);
            state
                .snapshot(limits.max_analysis_steps - allowance, Span::default())
                .unwrap();
            let checked = state.snapshot_type(&owner, 2, 2, Span::default());
            assert_eq!(checked.is_ok(), success, "{checked:?}");
            if success {
                assert_eq!(state.remaining_work(), 0);
            } else {
                assert_eq!(checked.unwrap_err().category, ResultCategory::ResourceLimit);
            }
        }
    }
}
