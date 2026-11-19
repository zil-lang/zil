//! Task ownership is attached to real input, capture, constructor and checked
//! return occurrences. A Task type alone never creates a close-state fact.
use super::*;

impl Engine<'_, '_> {
    /// A reusable summary can refine a surviving occupant only when the body
    /// preserved that exact input occurrence. Ordinary replacement remains
    /// valid, but needs its own take/refill correspondence before transport.
    pub(super) fn same_input_occurrence(
        &mut self,
        expected: &Value,
        actual: &Value,
        depth: usize,
    ) -> R<bool> {
        self.pay(expected.work().saturating_add(actual.work()), depth)?;
        if let Value::Choice(values) = actual {
            for value in values {
                if !self.same_input_occurrence(expected, value, depth + 1)? {
                    return Ok(false);
                }
            }
            return Ok(!values.is_empty());
        }
        if let Value::Choice(values) = expected {
            for value in values {
                if self.same_input_occurrence(value, actual, depth + 1)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        Ok(match (expected, actual) {
            (Value::Plain | Value::Bool(_), _) => true,
            (
                Value::Root(root, true, owning, None),
                Value::Root(other, true, other_owning, None),
            ) => root == other && owning == other_owning,
            (
                Value::Cleanup {
                    owner,
                    ty,
                    source: Some(source),
                },
                Value::Cleanup {
                    owner: other,
                    ty: other_ty,
                    source: Some(other_source),
                },
            ) => owner == other && ty == other_ty && source == other_source,
            (Value::Tuple(a), Value::Tuple(b)) if a.len() == b.len() => {
                for (a, b) in a.iter().zip(b) {
                    if !self.same_input_occurrence(a, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Value::Fields(a), Value::Fields(b)) => self.same_input_fields(a, b, depth + 1)?,
            (
                Value::Closure {
                    definition: a,
                    captures: ac,
                },
                Value::Closure {
                    definition: b,
                    captures: bc,
                },
            ) if a == b => self.same_input_fields(ac, bc, depth + 1)?,
            (
                Value::Error {
                    is_error: a,
                    payload: ap,
                },
                Value::Error {
                    is_error: b,
                    payload: bp,
                },
            ) if a == b => self.same_input_occurrence(ap, bp, depth + 1)?,
            (Value::Boxed(a), Value::Boxed(b)) => self.same_input_occurrence(a, b, depth + 1)?,
            (
                Value::Variant {
                    name: a,
                    fields: af,
                },
                Value::Variant {
                    name: b,
                    fields: bf,
                },
            ) if a == b && af.len() == bf.len() => {
                for (a, b) in af.iter().zip(bf) {
                    if !self.same_input_occurrence(a, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            _ => false,
        })
    }
    fn same_input_fields(
        &mut self,
        a: &BTreeMap<String, Value>,
        b: &BTreeMap<String, Value>,
        depth: usize,
    ) -> R<bool> {
        if a.len() != b.len() {
            return Ok(false);
        }
        for (name, value) in a {
            self.pay(
                name.len()
                    .saturating_mul(lookup_work(b.len()))
                    .saturating_add(1),
                depth,
            )?;
            let Some(other) = b.get(name) else {
                return Ok(false);
            };
            if !self.same_input_occurrence(value, other, depth + 1)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn task_set_sources(
        &mut self,
        bindings: &mut SourceBindings,
        query: Query,
        source: Option<Vec<TaskResultSource>>,
        depth: usize,
    ) -> R<()> {
        self.pay(
            query
                .copy_work()
                .saturating_add(bindings.len().saturating_add(1).ilog2() as usize + 2),
            depth,
        )?;
        if let Some(source) = &source {
            task_results::source_snapshot(source, self.limits, self.left)?;
        }
        if bindings.len() >= self.limits.max_nodes && !bindings.contains_key(&query) {
            return Err(Diagnostic::limit(
                Phase::Types,
                "Task source binder inventory limit exceeded",
                None,
            ));
        }
        bindings.insert(query, source);
        Ok(())
    }
    fn task_union_sources(
        &mut self,
        a: &Option<Vec<TaskResultSource>>,
        b: &Option<Vec<TaskResultSource>>,
        depth: usize,
    ) -> R<Option<Vec<TaskResultSource>>> {
        let (Some(a), Some(b)) = (a, b) else {
            return Ok(None);
        };
        task_results::source_snapshot(a, self.limits, self.left)?;
        task_results::source_snapshot(b, self.limits, self.left)?;
        let mut result = a.clone();
        for source in b {
            task_results::source_snapshot(&result, self.limits, self.left)?;
            self.pay(result.len().saturating_add(1), depth)?;
            if !result.contains(source) {
                if result.len() >= self.limits.max_nodes {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "Task source relation inventory limit exceeded",
                        None,
                    ));
                }
                result.push(source.clone());
            }
        }
        Ok(Some(result))
    }
    pub(super) fn task_join_sources(
        &mut self,
        a: Option<&SourceBindings>,
        b: Option<&SourceBindings>,
        depth: usize,
    ) -> R<Option<SourceBindings>> {
        let (Some(a), Some(b)) = (a, b) else {
            return Ok(None);
        };
        self.pay(a.len().saturating_add(b.len()), depth)?;
        let mut result = BTreeMap::new();
        for (query, source) in a {
            let joined = match b.get(query) {
                Some(other) => self.task_union_sources(source, other, depth + 1)?,
                None => None,
            };
            self.task_set_sources(&mut result, query.clone(), joined, depth + 1)?;
        }
        for query in b.keys() {
            if !result.contains_key(query) {
                self.task_set_sources(&mut result, query.clone(), None, depth + 1)?;
            }
        }
        Ok(Some(result))
    }
    pub(super) fn task_assign_sources(
        &mut self,
        owner: SemanticDefinitionId,
        target: &SemanticOriginalExpr,
        source: Option<Vec<TaskResultSource>>,
        state: &mut State,
        depth: usize,
    ) -> R<()> {
        let Some(bindings) = &mut state.task_sources else {
            return Ok(());
        };
        let mut base = target;
        let mut whole = true;
        loop {
            self.pay(1, depth)?;
            match &base.kind {
                CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } => {
                    base = inner
                }
                CoreExprKind::Member { object, .. } | CoreExprKind::Index { object, .. } => {
                    base = object;
                    whole = false;
                }
                CoreExprKind::Unary {
                    op: UnaryOp::Dereference,
                    operand,
                } => {
                    base = operand;
                    whole = false;
                }
                _ => break,
            }
        }
        // Writes through an external pointer cannot mint a new parameter
        // origin. A projected write invalidates the old whole-value relation
        // until its complete field correspondence is independently rebuilt.
        let Some(name) = self.proof.task_name_sources.get(&(owner, base.id)) else {
            return Ok(());
        };
        if name.owner != owner {
            return Ok(());
        }
        let work = name.origin.copy_work();
        self.pay(work, depth)?;
        let query = self.proof.task_name_sources[&(owner, base.id)]
            .origin
            .clone();
        self.task_set_sources(
            bindings,
            query,
            if whole { source } else { None },
            depth + 1,
        )
    }
    pub(super) fn task_record_sources(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        source: Option<Vec<TaskResultSource>>,
        depth: usize,
    ) -> R<()> {
        self.pay(
            self.task_source_returns.len().saturating_add(1).ilog2() as usize + 2,
            depth,
        )?;
        let key = (owner, statement);
        let source = if self.task_source_returns.contains(&key) {
            let previous = self
                .proof
                .task_returns
                .get(&key)
                .ok_or_else(|| failure("Task CFG return loses its original endpoint"))?;
            if let Some(previous) = &previous.sources {
                task_results::source_snapshot(previous, self.limits, self.left)?;
            }
            let previous = previous.sources.clone();
            self.task_union_sources(&previous, &source, depth + 1)?
        } else {
            source
        };
        if let Some(source) = &source {
            task_results::source_snapshot(source, self.limits, self.left)?;
        }
        // None is absorbing across every real reaching state. One successful
        // branch never fills a missing relation on another path or backedge.
        self.proof
            .task_returns
            .get_mut(&key)
            .ok_or_else(|| failure("Task CFG return has no checked original endpoint"))?
            .sources = source;
        self.task_source_returns.insert(key);
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Requirement {
    Drop(Root),
    Call(Root),
    Consume(Root),
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct GraphCall {
    callee: Value,
    arguments: Vec<Value>,
    state: State,
}
impl GraphCall {
    fn work(&self) -> usize {
        self.arguments.iter().fold(self.callee.work().saturating_add(self.state.work()).saturating_add(1),
            |work, value| work.saturating_add(value.work()))
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Exit {
    pub value: Value,
    pub phases: BTreeMap<Root, u8>,
    pub synchronous: BTreeMap<Root, BTreeSet<Requirement>>,
}

struct ReturnInputs<'a> {
    target: SemanticDefinitionId,
    arguments: &'a [Value],
    captures: Option<&'a BTreeMap<String, Value>>,
    ordinary_call: Option<(SemanticDefinitionId, SemanticSiteId)>,
}

impl Engine<'_, '_> {
    pub(super) fn record_graph_call(
        &mut self,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        callee: Value,
        arguments: &[Value],
        state: &State,
        depth: usize,
    ) -> R<()> {
        let mut snapshot = self.copy(state, depth + 1)?;
        // The call's real generation facts are immutable evidence. Later
        // replacement must never make an old occurrence synchronously closed.
        snapshot.names.clear();
        snapshot.temporaries.clear();
        snapshot.deferred.clear();
        snapshot.abrupt.clear();
        snapshot.returned = Value::Plain;
        snapshot.task_sources = None;
        let arguments = arguments.iter().map(|value| self.value_copy(value, depth + 1))
            .collect::<R<Vec<_>>>()?;
        let row = GraphCall { callee, arguments, state: snapshot };
        self.pay(lookup_work(self.graph_calls.len()), depth)?;
        if let Some(previous) = self.graph_calls.get(&(owner, site)) {
            self.pay(previous.iter().fold(row.work(), |work, old|
                work.saturating_add(old.work()).saturating_add(row.work())), depth)?;
            if self.graph_calls[&(owner, site)].contains(&row) { return Ok(()); }
        }
        if self.graph_call_count >= self.limits.max_nodes {
            return Err(Diagnostic::limit(Phase::Types, "original Task call occurrence inventory limit exceeded", None));
        }
        self.pay(row.work().saturating_add(1), depth)?;
        self.graph_call_count += 1;
        self.graph_calls.entry((owner, site)).or_default().push(row);
        Ok(())
    }

    /// Summarize the complete original graph under its own Gamma. Invocation
    /// of an incoming callable remains an occurrence requirement, discharged
    /// only against the real argument/capture at a later construction boundary.
    fn synchronous_task_worker(
        &mut self,
        owner: SemanticDefinitionId,
        active: &mut BTreeSet<SemanticDefinitionId>,
        depth: usize,
    ) -> R<Option<BTreeSet<Requirement>>> {
        self.pay(active.len().saturating_add(lookup_work(self.synchronous_workers.len())), depth)?;
        if let Some(known) = self.synchronous_workers.get(&owner) {
            self.pay(known.as_ref().map_or(1, |r| r.len().saturating_add(1)), depth)?;
            return Ok(self.synchronous_workers[&owner].clone());
        }
        if self.active.contains(&owner) || !active.insert(owner) { return Ok(None); }
        // A frozen cleanup result does not contain this Engine's call operand
        // receipts. Rebuild those from the original body rather than trusting
        // a concrete type or another occurrence's invocation graph.
        if !self.graph_complete.contains(&owner) {
            self.pay(1, depth)?;
            self.graph_requested.insert(owner);
            self.proof.protocol_complete.remove(&owner);
            self.proof.cleanup_complete.remove(&owner);
            self.function(owner, depth + 1)?;
        }
        let result = self.synchronous_task_worker_inner(owner, active, depth + 1)?;
        active.remove(&owner);
        self.pay(result.as_ref().map_or(1, |r| r.len().saturating_add(1)), depth)?;
        self.synchronous_workers.insert(owner, result.clone());
        Ok(result)
    }

    fn synchronous_task_worker_inner(
        &mut self,
        owner: SemanticDefinitionId,
        active: &mut BTreeSet<SemanticDefinitionId>,
        depth: usize,
    ) -> R<Option<BTreeSet<Requirement>>> {
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature, foreign, body, sites, ..
        } = &definition.body else { return Ok(None); };
        if foreign.is_some() {
            // The independently checked concrete C declaration cannot acquire
            // a Task or its environment through primitive scalar operands.
            // Addresses, handles, code and aggregate/abstract types remain
            // outside this leaf domain, even if their machine layout is scalar.
            self.pay(signature.parameters.len().saturating_add(1), depth)?;
            let scalar = |ty: &SemanticTypeRecipe| matches!(ty,
                SemanticTypeRecipe::Atom(zil_ir::Type::Unit | zil_ir::Type::Bool
                    | zil_ir::Type::I8 | zil_ir::Type::I16 | zil_ir::Type::I32 | zil_ir::Type::I64
                    | zil_ir::Type::U8 | zil_ir::Type::U16 | zil_ir::Type::U32 | zil_ir::Type::U64
                    | zil_ir::Type::Isize | zil_ir::Type::Usize | zil_ir::Type::F32 | zil_ir::Type::F64));
            return Ok((signature.call_kind == CallKind::Direct
                && signature.bound_lifetimes == 0 && signature.invocation_lifetimes == 0
                && foreign.as_ref().is_some_and(|declaration| declaration.abi == "C" && !declaration.symbol.is_empty())
                && matches!(body, SemanticOriginalFunctionBody::External)
                && scalar(&signature.result)
                && signature.parameters.iter().all(|parameter| scalar(&parameter.ty)))
                .then(BTreeSet::new));
        }
        if !matches!(body, SemanticOriginalFunctionBody::Defined(_)) { return Ok(None); }
        self.pay(sites.len().saturating_add(self.proof.types.len()), depth)?;
        let mut types = Vec::new();
        for ((domain, _), ty) in &self.proof.types {
            if *domain == owner { types.push(copy_recipe(ty, self.limits, self.left)?); }
        }
        for ty in types {
            if !self.synchronous_graph_type(owner, &ty, depth + 1)? { return Ok(None); }
        }
        let mut requirements = BTreeSet::new();
        for site in sites {
            self.pay(1, depth)?;
            if !matches!(site.kind, SemanticSiteRecipeKind::OriginalCall)
                || self.proof.compile_time_calls.contains(&(owner, site.id)) { continue; }
            let intrinsic = self.proof.intrinsics.get(&(owner, site.id)).map(|call| call.kind.clone());
            let (target, skip_provider) = match intrinsic {
                Some(IntrinsicKind::TaskCreate { worker, .. }) => (Some(worker), true),
                Some(IntrinsicKind::TaskYield | IntrinsicKind::TaskOperation(_)
                    | IntrinsicKind::NominalConstructor { .. }
                    | IntrinsicKind::MutexCreate | IntrinsicKind::MutexLock
                    | IntrinsicKind::MutexValue | IntrinsicKind::MutexUnlock
                    | IntrinsicKind::StoreNew | IntrinsicKind::StoreReserve
                    | IntrinsicKind::StorePush | IntrinsicKind::StorePop
                    | IntrinsicKind::StoreLen | IntrinsicKind::StoreCapacity
                    | IntrinsicKind::StoreBorrow { .. }
                    | IntrinsicKind::BytesNew | IntrinsicKind::BytesReserve
                    | IntrinsicKind::BytesTruncate | IntrinsicKind::BytesPush
                    | IntrinsicKind::BytesLen | IntrinsicKind::BytesCapacity
                    | IntrinsicKind::BytesBorrow { .. } | IntrinsicKind::Length
                    | IntrinsicKind::Numeric { .. }) => continue,
                Some(_) => return Ok(None),
                None => (self.proof.call_targets.get(&(owner, site.id)).copied(), false),
            };
            self.pay(lookup_work(self.graph_calls.len()), depth)?;
            let Some(rows) = self.graph_calls.get(&(owner, site.id)) else { return Ok(None); };
            self.pay(rows.iter().fold(1usize, |work, row| work.saturating_add(row.work())), depth)?;
            let rows = self.graph_calls[&(owner, site.id)].clone();
            for row in rows {
                let incoming = if let Some(target) = target {
                    let arguments = if skip_provider {
                        let Some((Value::Plain, arguments)) = row.arguments.split_first() else { return Ok(None); };
                        arguments
                    } else { row.arguments.as_slice() };
                    let Some(needed) = self.synchronous_task_worker(target, active, depth + 1)? else { return Ok(None); };
                    let Some(mut incoming) = self.instantiate_graph_requirements(
                        target, &needed, arguments, None, &row.state, active, depth + 1,
                    )? else { return Ok(None); };
                    if skip_provider {
                        let Some(children) = self.synchronous_task_inputs(arguments, &row.state, depth + 1)? else { return Ok(None); };
                        self.pay(children.len(), depth)?;
                        incoming.extend(children);
                    }
                    Some(incoming)
                } else if self.proof.callable_calls.contains_key(&(owner, site.id)) {
                    self.synchronous_callable_value(&row.callee, &row.arguments, &row.state, active, depth + 1)?
                } else { None };
                let Some(incoming) = incoming else { return Ok(None); };
                self.pay(incoming.len().saturating_add(1), depth)?;
                requirements.extend(incoming);
            }
        }
        Ok(Some(requirements))
    }

    fn synchronous_callable_value(
        &mut self,
        value: &Value,
        arguments: &[Value],
        state: &State,
        active: &mut BTreeSet<SemanticDefinitionId>,
        depth: usize,
    ) -> R<Option<BTreeSet<Requirement>>> {
        self.pay(value.work().saturating_add(arguments.len()), depth)?;
        match value {
            Value::Cleanup { source: Some(source), .. } => {
                // The deferred invocation has no owning/callable input whose
                // identity would need a further application environment.
                if !arguments.iter().all(|value| matches!(value, Value::Plain | Value::Bool(_))) {
                    return Ok(None);
                }
                Ok(Some(BTreeSet::from([Requirement::Call(*source)])))
            }
            Value::Closure { definition, captures } => {
                let Some(needed) = self.synchronous_task_worker(*definition, active, depth + 1)? else { return Ok(None); };
                self.instantiate_graph_requirements(*definition, &needed, arguments, Some(captures), state, active, depth + 1)
            }
            Value::Choice(values) => {
                let mut requirements = BTreeSet::new();
                for value in values {
                    let Some(incoming) = self.synchronous_callable_value(value, arguments, state, active, depth + 1)? else { return Ok(None); };
                    self.pay(incoming.len(), depth)?;
                    requirements.extend(incoming);
                }
                Ok(Some(requirements))
            }
            _ => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn instantiate_graph_requirements(
        &mut self,
        target: SemanticDefinitionId,
        requirements: &BTreeSet<Requirement>,
        arguments: &[Value],
        captures: Option<&BTreeMap<String, Value>>,
        state: &State,
        active: &mut BTreeSet<SemanticDefinitionId>,
        depth: usize,
    ) -> R<Option<BTreeSet<Requirement>>> {
        self.pay(requirements.len(), depth)?;
        let mut result = BTreeSet::new();
        for requirement in requirements {
            let (Requirement::Drop(root) | Requirement::Call(root) | Requirement::Consume(root)) = requirement;
            let Some(value) = self.original_input(*root, target, arguments, captures, depth + 1)? else {
                // A proved inactive sum alternative has no runtime invocation.
                continue;
            };
            let incoming = match requirement {
                Requirement::Drop(_) => self.remaining_consumption(&value, state, depth + 1)?,
                Requirement::Call(_) => self.synchronous_callable_value(&value, &[], state, active, depth + 1)?,
                Requirement::Consume(_) => self.synchronous_consumption(&value, &[], state, active, depth + 1)?,
            };
            let Some(incoming) = incoming else { return Ok(None); };
            self.pay(incoming.len().saturating_add(1), depth)?;
            result.extend(incoming);
        }
        Ok(Some(result))
    }
    fn synchronous_graph_type(
        &mut self, owner: SemanticDefinitionId, ty: &SemanticTypeRecipe, depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        if self.task_head(owner, ty, depth + 1)?.is_some() || self.can_drop(owner, ty, depth + 1)? {
            return Ok(true);
        }
        if let Some((error, result)) = cleanup::error_arms(self.program, self.proof, owner, ty, self.limits, self.left)? {
            return Ok(self.synchronous_graph_type(owner, &error, depth + 1)?
                && self.synchronous_graph_type(owner, &result, depth + 1)?);
        }
        if let Some(fields) = cleanup::fields(self.program, self.proof, owner, ty, self.limits, self.left)? {
            for (_, field) in fields {
                if !self.synchronous_graph_type(owner, &field, depth + 1)? { return Ok(false); }
            }
            return Ok(true);
        }
        Ok(false)
    }
    fn synchronous_task_inputs(
        &mut self, values: &[Value], state: &State, depth: usize,
    ) -> R<Option<BTreeSet<Requirement>>> {
        let mut requirements = BTreeSet::new();
        for value in values {
            let Some(incoming) = self.synchronous_task_value(value, state, depth + 1)? else { return Ok(None); };
            self.pay(incoming.len().saturating_add(1), depth)?;
            requirements.extend(incoming);
        }
        Ok(Some(requirements))
    }
    pub(super) fn synchronous_task_value(
        &mut self, value: &Value, state: &State, depth: usize,
    ) -> R<Option<BTreeSet<Requirement>>> {
        self.pay(1, depth)?;
        match value {
            Value::Root(root, true, _, None) if state.tasks.contains(root) => {
                if state.roots.get(root) == Some(&CLOSED) { return Ok(Some(BTreeSet::new())); }
                if let Some(requirements) = state.task_sync.get(root) {
                    self.pay(requirements.len().saturating_add(1), depth)?;
                    Ok(Some(requirements.clone()))
                } else { Ok(None) }
            }
            Value::Root(..) => Ok(None),
            Value::Cleanup { owner, ty, .. } => Ok(self.can_drop(*owner, ty, depth + 1)?.then(BTreeSet::new)),
            Value::Error { payload, .. } | Value::Boxed(payload) => self.synchronous_task_value(payload, state, depth + 1),
            Value::Tuple(values) | Value::Choice(values) | Value::Variant { fields: values, .. } => self.synchronous_task_inputs(values, state, depth + 1),
            Value::Fields(fields) | Value::Closure { captures: fields, .. } => {
                let mut requirements = BTreeSet::new();
                for value in fields.values() {
                    let Some(incoming) = self.synchronous_task_value(value, state, depth + 1)? else { return Ok(None); };
                    self.pay(incoming.len().saturating_add(1), depth)?;
                    requirements.extend(incoming);
                }
                Ok(Some(requirements))
            }
            Value::Plain | Value::Bool(_) => Ok(Some(BTreeSet::new())),
            _ => Ok(None),
        }
    }
    fn instantiate_task_requirements(
        &mut self, target: SemanticDefinitionId, requirements: &BTreeSet<Requirement>,
        arguments: &[Value], state: &State, depth: usize,
    ) -> R<Option<BTreeSet<Requirement>>> {
        self.instantiate_graph_requirements(target, requirements, arguments, None, state, &mut BTreeSet::new(), depth + 1)
    }
    pub(super) fn task_head(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Option<bool>> {
        self.pay(1, depth)?;
        cleanup::task_head(self.program, self.proof, owner, ty, self.limits, self.left)
    }
    pub(super) fn task_input(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        root: Root,
        state: &mut State,
        depth: usize,
    ) -> R<Option<Value>> {
        let Some(owning) = self.task_head(owner, ty, depth + 1)? else {
            return Ok(None);
        };
        self.pay(2, depth)?;
        state.roots.insert(root, UNKNOWN);
        state.tasks.insert(root);
        // A parameter is a symbolic requirement, never a type-level promise.
        state.task_sync.insert(root, BTreeSet::from([Requirement::Drop(root)]));
        Ok(Some(Value::Root(root, true, owning, None)))
    }
    pub(super) fn task_drop(
        &mut self,
        root: Root,
        state: &State,
        prior: Option<u8>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        let phase = prior.unwrap_or_else(|| state.roots.get(&root).copied().unwrap_or(0));
        if phase == CLOSED {
            return Ok(());
        }
        if prior.is_some() {
            return Err(failure(
                "original Task cleanup loses an earlier live task generation",
            ));
        }
        self.pay(state.task_sync.len().saturating_add(1), depth)?;
        if state.task_sync.get(&root).is_some_and(BTreeSet::is_empty) {
            return Ok(());
        }
        let original = self.definition(state.owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            foreign: None,
            ..
        } = &original.body
        else {
            return Err(failure(
                "original Task cleanup has no language execution root",
            ));
        };
        if signature.call_kind != CallKind::Suspend {
            return Err(error(
                "E-ORIGINAL-CLEANUP",
                Span::default(),
                "live original Task default Drop cleanup may suspend and lacks a Closed fact or a Suspend execution root",
            ));
        }
        // A genuine initialized/incoming owner preserves its frame's cleanup
        // invariant. This validates a resumable cleanup edge only: it does not
        // assert Closed at a subsequent source operation or SyncDrop(Task/F).
        Ok(())
    }
    pub(super) fn task_call(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        site: SemanticSiteId,
        kind: &IntrinsicKind,
        values: &[Value],
        state: &mut State,
        depth: usize,
    ) -> R<Option<Value>> {
        self.pay(1, depth)?;
        match kind {
            IntrinsicKind::TaskCreate {
                worker,
                owned,
                await_result,
            } => {
                if !self.active.contains(worker) {
                    self.function(*worker, depth + 1)?;
                }
                if !self.proof.cleanup_complete.contains(worker) {
                    return Err(pending(
                        "Task construction requires its complete original worker cleanup invariant",
                    ));
                }
                let arity = self
                    .proof
                    .intrinsics
                    .get(&(owner, site))
                    .ok_or_else(|| failure("Task worker scheme disappeared"))?
                    .parameters
                    .len();
                if values.len() != arity.saturating_add(1)
                    || !matches!(values.first(), Some(Value::Plain))
                {
                    return Err(failure(
                        "Task worker occurrence changes its static provider or complete runtime input arity",
                    ));
                }
                let values = &values[1..];
                self.check_consuming_target(*worker, values, None, state, depth + 1)?;
                if *await_result {
                    return self.task_return_call(owner, node, *worker, values, state, None, depth + 1);
                }
                let synchronous = if let Some(needed) = self.synchronous_task_worker(*worker, &mut BTreeSet::new(), depth + 1)? {
                    match (self.instantiate_task_requirements(*worker, &needed, values, state, depth + 1)?,
                        self.synchronous_task_inputs(values, state, depth + 1)?) {
                        (Some(mut graph), Some(inputs)) => {
                            self.pay(inputs.len(), depth)?;
                            graph.extend(inputs);
                            Some(graph)
                        }
                        _ => None,
                    }
                } else { None };
                let root = Root::Created(owner, node);
                self.change(state, root, OPEN, true, depth + 1)?;
                if let Some(requirements) = synchronous {
                    state.task_sync.insert(root, requirements);
                }
                self.pay(1, depth)?;
                state.tasks.insert(root);
                let failure = if *owned {
                    // The canonical failure carrier physically owns exactly the
                    // prepared inputs; its frame type argument is phantom.
                    let mut inputs = Vec::with_capacity(values.len());
                    for value in values {
                        inputs.push(self.value_copy(value, depth + 1)?)
                    }
                    self.pay(14, depth)?;
                    Value::Fields(BTreeMap::from([
                        ("reason".into(), Value::Plain),
                        ("inputs".into(), Value::Tuple(inputs)),
                    ]))
                } else {
                    for value in values {
                        self.drop_value(value, state, depth + 1)?;
                    }
                    Value::Plain
                };
                self.choice(
                    vec![
                        Value::Error {
                            is_error: true,
                            payload: Box::new(failure),
                        },
                        Value::Error {
                            is_error: false,
                            payload: Box::new(Value::Root(root, true, true, None)),
                        },
                    ],
                    depth + 1,
                )
                .map(Some)
            }
            IntrinsicKind::TaskOperation(op) => {
                let [value] = values else {
                    return Err(failure(
                        "Task operation changes its original one-owner arity",
                    ));
                };
                let root = self.root(value, state, depth + 1)?;
                self.pay(
                    state.tasks.len().saturating_add(1).ilog2() as usize + 1,
                    depth,
                )?;
                if !state.tasks.contains(&root) {
                    return Err(failure(
                        "Task operation has no real initialized Task owner occurrence",
                    ));
                }
                let phase = state.roots[&root];
                match op {
                    TaskOp::Valid => Ok(Some(Value::Plain)),
                    TaskOp::FinishDestroy => {
                        if !matches!(value, Value::Root(_, true, true, _)) {
                            return Err(failure("Task destruction lacks its current unique owning occurrence"));
                        }
                        // Only the successful runtime Closed check returns and consumes the head.
                        self.change(state, root, RETIRED, true, depth + 1)?;
                        Ok(Some(Value::Plain))
                    }
                    TaskOp::RequestCancel => {
                        let closing = if phase & (OPEN | CLOSING) != 0 {
                            CLOSING
                        } else {
                            0
                        };
                        self.change(state, root, closing | (phase & CLOSED), false, depth + 1)?;
                        Ok(Some(Value::Plain))
                    }
                    TaskOp::PollClose => {
                        let closing = if phase & (OPEN | CLOSING) != 0 {
                            CLOSING
                        } else {
                            0
                        };
                        self.change(state, root, closing | CLOSED, false, depth + 1)?;
                        let mut outcomes = Vec::new();
                        if closing != 0 {
                            outcomes.push(Outcome {
                                exit: ProtocolExit::PollPending,
                                phases: BTreeMap::from([(root, CLOSING)]),
                            })
                        }
                        outcomes.push(Outcome {
                            exit: ProtocolExit::PollReady,
                            phases: BTreeMap::from([(root, CLOSED)]),
                        });
                        self.pay(outcomes.len().saturating_mul(3), depth)?;
                        Ok(Some(Value::Poll {
                            fresh: true,
                            outcomes,
                        }))
                    }
                    TaskOp::Poll => {
                        let result = {
                            let call = self
                                .proof
                                .intrinsics
                                .get(&(owner, site))
                                .ok_or_else(|| failure("Task poll scheme disappeared"))?;
                            copy_recipe(&call.result, self.limits, self.left)?
                        };
                        let Some((error, poll)) = cleanup::error_arms(
                            self.program,
                            self.proof,
                            owner,
                            &result,
                            self.limits,
                            self.left,
                        )?
                        else {
                            return Err(failure(
                                "Task poll result changes its original ErrorUnion scheme",
                            ));
                        };
                        let error = self.typed(owner, &error, Value::Plain, depth + 1)?;
                        let mut outcomes = vec![Value::Error {
                            is_error: true,
                            payload: Box::new(error),
                        }];
                        if phase & (OPEN | CLOSING) != 0 {
                            let fields = cleanup::variant(
                                self.program,
                                self.proof,
                                owner,
                                &poll,
                                "Ready",
                                self.limits,
                                self.left,
                            )?
                            .ok_or_else(|| {
                                failure("Task poll has no canonical complete Ready payload")
                            })?;
                            let [payload] = fields.as_slice() else {
                                return Err(failure(
                                    "Task Ready changes its complete payload arity",
                                ));
                            };
                            let ready = self.typed(owner, payload, Value::Plain, depth + 1)?;
                            let poll = self.choice(
                                vec![
                                    Value::Variant {
                                        name: "Pending".into(),
                                        fields: Vec::new(),
                                    },
                                    Value::Variant {
                                        name: "Ready".into(),
                                        fields: vec![ready],
                                    },
                                ],
                                depth + 1,
                            )?;
                            outcomes.push(Value::Error {
                                is_error: false,
                                payload: Box::new(poll),
                            });
                        }
                        self.pay(outcomes.len().saturating_add(14), depth)?;
                        // Ready moves the payload once. It does not prove that
                        // a borrowed result, child, waker or external user has
                        // drained; only the separate close operation does that.
                        self.choice(outcomes, depth + 1).map(Some)
                    }
                }
            }
            _ => Ok(None),
        }
    }
    pub(super) fn task_record_exit(
        &mut self,
        owner: SemanticDefinitionId,
        state: &State,
        depth: usize,
    ) -> R<()> {
        let mut roots = BTreeSet::new();
        self.pay(state.returned.work(), depth)?;
        state.returned.roots(&mut roots);
        // A reusable invocation leaves its input owners in place. Their real
        // exit states belong to this invocation even when the result is Copy.
        // These are conditional exit facts, never cleanup facts of the type.
        self.pay(lookup_work(self.proof.kinds.len()), depth)?;
        if self.definition(owner)?.kind == SemanticDefinitionKind::Closure
            && matches!(self.proof.kinds.get(&owner), Some(CallableKind::Fn | CallableKind::FnMut)) {
            self.pay(state.tasks.len().saturating_mul(lookup_work(roots.len().saturating_add(state.tasks.len())).saturating_add(1)), depth)?;
            for root in &state.tasks {
                if matches!(root, Root::Parameter(domain, _) | Root::Capture(domain, _) | Root::Projection(domain, _) if *domain == owner) {
                    roots.insert(*root);
                }
            }
        }
        let mut phases: BTreeMap<Root, u8> = BTreeMap::new();
        for root in roots {
            if state.tasks.contains(&root) {
                self.pay(2, depth)?;
                phases.insert(root, state.roots[&root]);
            }
        }
        let value = self.value_copy(&state.returned, depth + 1)?;
        let mut synchronous = BTreeMap::new();
        for root in phases.keys() {
            if let Some(requirements) = state.task_sync.get(root) {
                self.pay(requirements.len().saturating_add(1), depth)?;
                synchronous.insert(*root, requirements.clone());
            }
        }
        let exit = Exit { value, phases, synchronous };
        self.pay(
            self.task_exits.len().saturating_add(1).ilog2() as usize + 1,
            depth,
        )?;
        if let Some(previous) = self.task_exits.get(&owner) {
            let work = previous.iter().fold(0usize, |work, old| {
                work.saturating_add(old.value.work())
                    .saturating_add(exit.value.work())
                    .saturating_add(old.phases.len())
                    .saturating_add(exit.phases.len())
                    .saturating_add(old.synchronous.values().fold(0usize, |n, r| n.saturating_add(r.len()).saturating_add(1)))
                    .saturating_add(exit.synchronous.values().fold(0usize, |n, r| n.saturating_add(r.len()).saturating_add(1)))
            });
            self.pay(work, depth)?;
            if self.task_exits[&owner].iter().any(|old| old == &exit) {
                return Ok(());
            }
        }
        if self.task_exit_count >= self.limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original Task return inventory limit exceeded",
                None,
            ));
        }
        self.pay(1, depth)?;
        self.task_exits.entry(owner).or_default().push(exit);
        self.task_exit_count += 1;
        Ok(())
    }
    fn has_return_facts(&mut self, value: &Value, depth: usize) -> R<bool> {
        self.pay(1, depth)?;
        Ok(match value {
            // A tag selects which owning projections exist at the caller.
            // Even a fieldless arm has an observable cleanup fact.
            Value::Closure { .. } | Value::Cleanup { source: Some(_), .. }
            | Value::Variant { .. } | Value::Error { .. } => true,
            Value::Boxed(payload) => self.has_return_facts(payload, depth + 1)?,
            Value::Tuple(values) | Value::Choice(values) => {
                let mut any = false;
                for value in values { any |= self.has_return_facts(value, depth + 1)?; }
                any
            }
            Value::Fields(values) => {
                let mut any = false;
                for value in values.values() { any |= self.has_return_facts(value, depth + 1)?; }
                any
            }
            _ => false,
        })
    }

    pub(super) fn task_return_call(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        target: SemanticDefinitionId,
        arguments: &[Value],
        state: &mut State,
        incoming: Option<&State>,
        depth: usize,
    ) -> R<Option<Value>> {
        self.task_return_application(owner, node, target, arguments, None, state, incoming, depth + 1)
    }

    /// Only the actual invoked closure supplies this environment. No type,
    /// matching layout or unrelated creator can stand in for these captures.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn task_return_callable(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        callee_node: NodeId,
        callee: &Value,
        arguments: &[Value],
        state: &mut State,
        depth: usize,
    ) -> R<Option<Value>> {
        let Value::Closure { definition, captures } = callee else { return Ok(None); };
        let Some(site) = self.site(owner, node, depth + 1)? else { return Ok(None); };
        self.pay(lookup_work(self.proof.callable_calls.len()).saturating_add(lookup_work(self.proof.kinds.len())), depth)?;
        let Some(call) = self.proof.callable_calls.get(&(owner, site)) else { return Ok(None); };
        if call.callee_node != callee_node || !matches!((self.proof.kinds.get(definition), call.kind),
            (Some(CallableKind::Fn), CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce)
            | (Some(CallableKind::FnMut), CallableKind::FnMut | CallableKind::FnOnce)
            | (Some(CallableKind::FnOnce), CallableKind::FnOnce)) { return Ok(None); }
        let incoming = self.copy(state, depth + 1)?;
        let mut outgoing = self.copy(state, depth + 1)?;
        let mut exposed = BTreeSet::new();
        self.pay(callee.work(), depth)?;
        callee.roots(&mut exposed);
        for argument in arguments {
            self.pay(argument.work(), depth)?;
            argument.roots(&mut exposed);
        }
        for root in exposed {
            self.change(&mut outgoing, root, UNKNOWN, false, depth + 1)?;
            outgoing.task_sync.remove(&root);
        }
        let result = self.task_return_application(owner, node, *definition, arguments,
            Some(captures), &mut outgoing, Some(&incoming), depth + 1)?;
        if result.is_some() { *state = outgoing; }
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    fn task_return_application(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        target: SemanticDefinitionId,
        arguments: &[Value],
        captures: Option<&BTreeMap<String, Value>>,
        state: &mut State,
        incoming: Option<&State>,
        depth: usize,
    ) -> R<Option<Value>> {
        if self.active.contains(&target) {
            return Ok(None);
        }
        if !self.local_complete.contains(&target) {
            self.proof.protocol_complete.remove(&target);
            self.proof.cleanup_complete.remove(&target);
        }
        self.function(target, depth + 1)?;
        let Some(exits) = self.task_exits.get(&target) else {
            return Ok(None);
        };
        let work = exits.iter().fold(exits.len(), |work, exit| {
            work.saturating_add(exit.value.work())
                .saturating_add(exit.phases.len())
                .saturating_add(exit.synchronous.values().fold(0usize, |n, r| n.saturating_add(r.len()).saturating_add(1)))
        });
        self.pay(work, depth)?;
        let exits = self.task_exits[&target].clone();
        self.pay(lookup_work(self.proof.kinds.len()), depth)?;
        let reusable = self.definition(target)?.kind == SemanticDefinitionKind::Closure
            && matches!(self.proof.kinds.get(&target), Some(CallableKind::Fn | CallableKind::FnMut));
        let mut transported = reusable || exits.iter().any(|exit| !exit.phases.is_empty());
        for exit in &exits { transported |= self.has_return_facts(&exit.value, depth + 1)?; }
        if !transported { return Ok(None); }
        self.pay(lookup_work(self.proof.call_targets.len()), depth)?;
        let ordinary_call = self.site(owner, node, depth + 1)?
            .map(|site| (owner, site))
            .filter(|key| self.proof.call_targets.get(key) == Some(&target));
        let inputs = ReturnInputs { target, arguments, captures, ordinary_call };
        // A generic body may return different input projections from different
        // arms. A concrete argument rules out only arms whose original input
        // tag is absent. Filter before merging their generation/cleanup effects.
        self.pay(exits.len(), depth)?;
        let mut applicable = Vec::with_capacity(exits.len());
        for mut exit in exits {
            if self.retain_return_sources(&mut exit.value, &inputs, depth + 1)? {
                applicable.push(exit);
            }
        }
        if applicable.is_empty() {
            return Err(pending("no original return alternative applies to this invocation"));
        }
        let exits = applicable;
        // This summary transports Task generations and ordinary callable
        // occurrences. A registration-only result retains its independent
        // protocol consumer; do not invent a Task root for that carrier.
        for exit in &exits {
            let mut roots = BTreeSet::new();
            self.pay(exit.value.work(), depth)?;
            exit.value.roots(&mut roots);
            if roots.iter().any(|root| !exit.phases.contains_key(root)) { return Ok(None); }
        }
        for exit in &exits {
            self.charge_value(&exit.value, depth + 1)?;
        }
        let mut roots = BTreeMap::new();
        let mut phases: BTreeMap<Root, u8> = BTreeMap::new();
        for exit in &exits {
            let mut returned = BTreeSet::new();
            self.pay(exit.value.work(), depth)?;
            exit.value.roots(&mut returned);
            for (root, phase) in &exit.phases {
                if matches!(root, Root::Capture(..)) && captures.is_none() { return Ok(None); }
                let input = matches!(root, Root::Parameter(..) | Root::Capture(..) | Root::Projection(..));
                let mapped = if input {
                    let value = self.original_input(*root, target, arguments, captures, depth + 1)?
                        .ok_or_else(|| failure("Task return input is an inactive original alternative"))?;
                    let mapped = self.root(&value, incoming.unwrap_or(state), depth + 1)?;
                    if !matches!(value, Value::Root(_, true, _, None))
                        || returned.contains(root) && !matches!(value, Value::Root(_, true, true, None)) {
                        return Err(pending(
                            "Task owning return through a borrowed head requires its exact storage take/refill relation",
                        ));
                    }
                    if !state.tasks.contains(&mapped) {
                        return Err(failure(
                            "Task return input has no initialized owner invariant",
                        ));
                    }
                    mapped
                } else {
                    if !matches!(root, Root::Created(domain, _) | Root::Result(domain, _, _) if *domain == target) {
                        return Err(failure("Task return mint is not its original constructor or call result"));
                    }
                    let slot = u32::try_from(roots.len())
                        .map_err(|_| failure("Task return root index overflow"))?;
                    *roots
                        .entry(*root)
                        .or_insert(Root::Result(owner, node, slot))
                };
                phases
                    .entry(mapped)
                    .and_modify(|joined| *joined |= *phase)
                    .or_insert(*phase);
                roots.insert(*root, mapped);
            }
        }
        let mut synchronous: BTreeMap<Root, Option<BTreeSet<Requirement>>> = BTreeMap::new();
        for exit in &exits {
            for root in exit.phases.keys() {
                let mapped = roots[root];
                let requirements = match exit.synchronous.get(root) {
                    Some(requirements) => self.instantiate_graph_requirements(target, requirements, arguments, captures,
                        incoming.unwrap_or(state), &mut BTreeSet::new(), depth + 1)?,
                    None => None,
                };
                self.pay(requirements.as_ref().map_or(1, |r| r.len().saturating_add(1)), depth)?;
                match synchronous.entry(mapped) {
                    std::collections::btree_map::Entry::Vacant(entry) => { entry.insert(requirements); }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        match (entry.get_mut(), requirements) {
                            (Some(previous), Some(incoming)) => previous.extend(incoming),
                            (previous, _) => *previous = None,
                        }
                    }
                }
            }
        }
        for (root, phase) in phases {
            let fresh = matches!(root,Root::Result(definition,at,_) if definition==owner&&at==node);
            self.change(state, root, phase, fresh, depth + 1)?;
            state.tasks.insert(root);
            state.task_sync.remove(&root);
            if let Some(Some(requirements)) = synchronous.remove(&root) {
                state.task_sync.insert(root, requirements);
            }
        }
        let mut values = Vec::with_capacity(exits.len());
        for exit in exits {
            values.push(self.task_rehome(exit.value, &roots, &inputs, depth + 1)?)
        }
        self.choice(values, depth + 1).map(Some)
    }
    /// Remove alternatives contradicted by an actual enum/error discriminant.
    /// A malformed field or unproved source is still an error in original_input.
    fn retain_return_sources(
        &mut self,
        value: &mut Value,
        inputs: &ReturnInputs<'_>,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        match value {
            Value::Cleanup { source: Some(source), .. } => Ok(self
                .original_input(*source, inputs.target, inputs.arguments, inputs.captures, depth + 1)?
                .is_some()),
            Value::Choice(values) => {
                self.pay(values.len(), depth)?;
                let mut active = Vec::with_capacity(values.len());
                for mut branch in std::mem::take(values) {
                    if self.retain_return_sources(&mut branch, inputs, depth + 1)? {
                        active.push(branch);
                    }
                }
                *values = active;
                Ok(!values.is_empty())
            }
            Value::Boxed(inner) | Value::Error { payload: inner, .. } => {
                self.retain_return_sources(inner, inputs, depth + 1)
            }
            Value::Tuple(fields) | Value::Variant { fields, .. } => {
                for field in fields {
                    if !self.retain_return_sources(field, inputs, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Value::Fields(fields) | Value::Closure { captures: fields, .. } => {
                for field in fields.values_mut() {
                    if !self.retain_return_sources(field, inputs, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    fn task_rehome(
        &mut self,
        value: Value,
        roots: &BTreeMap<Root, Root>,
        inputs: &ReturnInputs<'_>,
        depth: usize,
    ) -> R<Value> {
        self.pay(1, depth)?;
        Ok(match value {
            Value::Cleanup { source: Some(source), .. } => {
                self.original_input(source, inputs.target, inputs.arguments, inputs.captures, depth + 1)?
                    .ok_or_else(|| pending("returned callable source is in an inactive original input alternative"))?
            }
            Value::Cleanup { owner, ty, source: None } if owner == inputs.target => {
                if let Some(call) = inputs.ordinary_call {
                    let actual = cleanup::instantiate_call_type(
                        self.program, self.proof, call, owner, &ty, self.limits, self.left,
                    )?;
                    self.typed(call.0, &actual, Value::Plain, depth + 1)?
                } else {
                    Value::Cleanup { owner, ty, source: None }
                }
            }
            Value::Root(root, live, owning, prior) => Value::Root(
                *roots.get(&root).ok_or_else(|| {
                    pending("Task return has an unmapped original owning projection")
                })?,
                live,
                owning,
                prior,
            ),
            Value::Error { is_error, payload } => Value::Error {
                is_error,
                payload: Box::new(self.task_rehome(*payload, roots, inputs, depth + 1)?),
            },
            Value::Boxed(value) => {
                Value::Boxed(Box::new(self.task_rehome(*value, roots, inputs, depth + 1)?))
            }
            Value::Tuple(values) => Value::Tuple(
                values
                    .into_iter()
                    .map(|value| self.task_rehome(value, roots, inputs, depth + 1))
                    .collect::<R<_>>()?,
            ),
            Value::Choice(values) => Value::Choice(
                values
                    .into_iter()
                    .map(|value| self.task_rehome(value, roots, inputs, depth + 1))
                    .collect::<R<_>>()?,
            ),
            Value::Variant { name, fields } => Value::Variant {
                name,
                fields: fields
                    .into_iter()
                    .map(|value| self.task_rehome(value, roots, inputs, depth + 1))
                    .collect::<R<_>>()?,
            },
            Value::Fields(values) => Value::Fields(
                values
                    .into_iter()
                    .map(|(name, value)| Ok((name, self.task_rehome(value, roots, inputs, depth + 1)?)))
                    .collect::<R<_>>()?,
            ),
            Value::Closure {
                definition,
                captures: environment,
            } => Value::Closure {
                definition,
                captures: environment
                    .into_iter()
                    .map(|(name, value)| Ok((name, self.task_rehome(value, roots, inputs, depth + 1)?)))
                    .collect::<R<_>>()?,
            },
            Value::Poll { fresh, outcomes } => {
                let mut mapped = Vec::new();
                for outcome in outcomes {
                    self.pay(outcome.phases.len().saturating_mul(
                        lookup_work(roots.len()).saturating_add(lookup_work(outcome.phases.len())).saturating_add(2),
                    ).saturating_add(1), depth)?;
                    let mut phases = BTreeMap::new();
                    for (root, phase) in outcome.phases {
                        let root = *roots.get(&root).ok_or_else(|| pending(
                            "callable Poll result has no complete original Task input correspondence",
                        ))?;
                        phases.entry(root).and_modify(|joined| *joined |= phase).or_insert(phase);
                    }
                    mapped.push(Outcome { exit: outcome.exit, phases });
                }
                Value::Poll { fresh, outcomes: mapped }
            }
            Value::Valid { .. } => {
                return Err(pending(
                    "Task return also carries an independently correlated protocol discriminator",
                ));
            }
            other => other,
        })
    }
}
