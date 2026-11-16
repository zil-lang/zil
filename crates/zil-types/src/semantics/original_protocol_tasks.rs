//! Task ownership is attached to real input, capture, constructor and checked
//! return occurrences. A Task type alone never creates a close-state fact.
use super::*;

impl Engine<'_, '_> {
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

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Exit {
    pub value: Value,
    pub phases: BTreeMap<Root, u8>,
    pub synchronous: BTreeMap<Root, BTreeSet<Root>>,
}
impl Engine<'_, '_> {
    /// A finite original worker graph is independent of its actual Task input
    /// owners. Those owners are checked separately and carried as symbolic
    /// requirements through factories. Unknown calls and external providers
    /// cannot be hidden by a matching result type or a completed cleanup pass.
    fn synchronous_task_worker(
        &mut self,
        owner: SemanticDefinitionId,
        active: &mut BTreeSet<SemanticDefinitionId>,
        depth: usize,
    ) -> R<bool> {
        self.pay(active.len().saturating_add(lookup_work(self.synchronous_workers.len())), depth)?;
        if let Some(known) = self.synchronous_workers.get(&owner) {
            return Ok(*known);
        }
        if !active.insert(owner) {
            return Ok(false);
        }
        let result = self.synchronous_task_worker_inner(owner, active, depth + 1)?;
        active.remove(&owner);
        self.pay(1, depth)?;
        self.synchronous_workers.insert(owner, result);
        Ok(result)
    }
    fn synchronous_task_worker_inner(
        &mut self,
        owner: SemanticDefinitionId,
        active: &mut BTreeSet<SemanticDefinitionId>,
        depth: usize,
    ) -> R<bool> {
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            foreign: None, body: SemanticOriginalFunctionBody::Defined(_), sites, ..
        } = &definition.body else { return Ok(false); };
        self.pay(sites.len().saturating_add(self.proof.types.len()), depth)?;
        // The complete local/parameter/result inventory also covers unused
        // branches and resource teardown, rather than just the worker's name.
        let mut types = Vec::new();
        for ((domain, _), ty) in &self.proof.types {
            if *domain == owner {
                types.push(copy_recipe(ty, self.limits, self.left)?);
            }
        }
        for ty in types {
            if !self.synchronous_graph_type(owner, &ty, depth + 1)? {
                return Ok(false);
            }
        }
        for site in sites {
            self.pay(1, depth)?;
            if !matches!(site.kind, SemanticSiteRecipeKind::OriginalCall) {
                continue;
            }
            if self.proof.compile_time_calls.contains(&(owner, site.id)) {
                continue;
            }
            if let Some(call) = self.proof.intrinsics.get(&(owner, site.id)) {
                let kind = call.kind.clone();
                match kind {
                    IntrinsicKind::TaskCreate { worker, .. } => {
                        if !self.synchronous_task_worker(worker, active, depth + 1)? { return Ok(false); }
                    }
                    IntrinsicKind::TaskYield | IntrinsicKind::TaskOperation(_)
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
                    | IntrinsicKind::Numeric { .. } => {}
                    _ => return Ok(false),
                }
            } else if let Some(target) = self.proof.call_targets.get(&(owner, site.id)).copied() {
                if !self.synchronous_task_worker(target, active, depth + 1)? { return Ok(false); }
            } else {
                return Ok(false);
            }
        }
        Ok(true)
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
    ) -> R<Option<BTreeSet<Root>>> {
        let mut requirements = BTreeSet::new();
        for value in values {
            let Some(incoming) = self.synchronous_task_value(value, state, depth + 1)? else { return Ok(None); };
            self.pay(incoming.len().saturating_add(1), depth)?;
            requirements.extend(incoming);
        }
        Ok(Some(requirements))
    }
    fn synchronous_task_value(
        &mut self, value: &Value, state: &State, depth: usize,
    ) -> R<Option<BTreeSet<Root>>> {
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
            Value::Cleanup { owner, ty } => Ok(self.can_drop(*owner, ty, depth + 1)?.then(BTreeSet::new)),
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
        &mut self, target: SemanticDefinitionId, requirements: &BTreeSet<Root>,
        arguments: &[Value], state: &State, depth: usize,
    ) -> R<Option<BTreeSet<Root>>> {
        self.pay(requirements.len(), depth)?;
        let mut result = BTreeSet::new();
        for requirement in requirements {
            let Root::Parameter(owner, ordinal) = requirement else { return Ok(None); };
            if *owner != target { return Ok(None); }
            let argument = arguments.get(*ordinal as usize).ok_or_else(|| failure("Task cleanup graph omits its actual worker input"))?;
            let Some(incoming) = self.synchronous_task_value(argument, state, depth + 1)? else { return Ok(None); };
            self.pay(incoming.len().saturating_add(1), depth)?;
            result.extend(incoming);
        }
        Ok(Some(result))
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
        state.task_sync.insert(root, BTreeSet::from([root]));
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
                if *await_result {
                    return self.task_return_call(owner, node, *worker, values, state, None, depth + 1);
                }
                let synchronous = if self.synchronous_task_worker(*worker, &mut BTreeSet::new(), depth + 1)? {
                    self.synchronous_task_inputs(values, state, depth + 1)?
                } else {
                    None
                };
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
                        if !matches!(value, Value::Root(_, true, true, _)) || phase != CLOSED {
                            return Err(failure(
                                "Task destruction lacks its unique owning occurrence and Closed fact",
                            ));
                        }
                        self.change(state, root, CLOSED, true, depth + 1)?;
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
        if self.active.contains(&target) {
            return Ok(None);
        }
        self.function(target, depth + 1)?;
        let Some(exits) = self.task_exits.get(&target) else {
            return Ok(None);
        };
        if !exits.iter().any(|exit| !exit.phases.is_empty()) {
            return Ok(None);
        }
        let work = exits.iter().fold(exits.len(), |work, exit| {
            work.saturating_add(exit.value.work())
                .saturating_add(exit.phases.len())
                .saturating_add(exit.synchronous.values().fold(0usize, |n, r| n.saturating_add(r.len()).saturating_add(1)))
        });
        self.pay(work, depth)?;
        let exits = self.task_exits[&target].clone();
        for exit in &exits {
            self.charge_value(&exit.value, depth + 1)?;
        }
        let mut roots = BTreeMap::new();
        let mut phases: BTreeMap<Root, u8> = BTreeMap::new();
        for exit in &exits {
            for (root, phase) in &exit.phases {
                let mapped = if let Root::Parameter(definition, ordinal) = root {
                    if *definition != target {
                        return Err(failure("Task return changes its original parameter domain"));
                    }
                    let value = arguments
                        .get(*ordinal as usize)
                        .ok_or_else(|| failure("Task return omits its real input owner"))?;
                    let mapped = self.root(value, state, depth + 1)?;
                    if !matches!(value, Value::Root(_, true, true, _)) {
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
                    if matches!(root, Root::Capture(_, _))
                        && self.proof.kinds.get(&target) != Some(&CallableKind::FnOnce)
                    {
                        return Err(pending(
                            "Task return from reusable capture requires its complete take/refill correspondence",
                        ));
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
        let mut synchronous: BTreeMap<Root, Option<BTreeSet<Root>>> = BTreeMap::new();
        for exit in &exits {
            for root in exit.phases.keys() {
                let mapped = roots[root];
                let requirements = match exit.synchronous.get(root) {
                    Some(requirements) => self.instantiate_task_requirements(target, requirements, arguments, incoming.unwrap_or(state), depth + 1)?,
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
            values.push(self.task_rehome(exit.value, &roots, depth + 1)?)
        }
        self.choice(values, depth + 1).map(Some)
    }
    fn task_rehome(
        &mut self,
        value: Value,
        roots: &BTreeMap<Root, Root>,
        depth: usize,
    ) -> R<Value> {
        self.pay(1, depth)?;
        Ok(match value {
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
                payload: Box::new(self.task_rehome(*payload, roots, depth + 1)?),
            },
            Value::Boxed(value) => {
                Value::Boxed(Box::new(self.task_rehome(*value, roots, depth + 1)?))
            }
            Value::Tuple(values) => Value::Tuple(
                values
                    .into_iter()
                    .map(|value| self.task_rehome(value, roots, depth + 1))
                    .collect::<R<_>>()?,
            ),
            Value::Choice(values) => Value::Choice(
                values
                    .into_iter()
                    .map(|value| self.task_rehome(value, roots, depth + 1))
                    .collect::<R<_>>()?,
            ),
            Value::Variant { name, fields } => Value::Variant {
                name,
                fields: fields
                    .into_iter()
                    .map(|value| self.task_rehome(value, roots, depth + 1))
                    .collect::<R<_>>()?,
            },
            Value::Fields(values) => Value::Fields(
                values
                    .into_iter()
                    .map(|(name, value)| Ok((name, self.task_rehome(value, roots, depth + 1)?)))
                    .collect::<R<_>>()?,
            ),
            Value::Closure {
                definition,
                captures,
            } => Value::Closure {
                definition,
                captures: captures
                    .into_iter()
                    .map(|(name, value)| Ok((name, self.task_rehome(value, roots, depth + 1)?)))
                    .collect::<R<_>>()?,
            },
            Value::Poll { .. } | Value::Valid { .. } => {
                return Err(pending(
                    "Task return also carries an independently correlated protocol discriminator",
                ));
            }
            other => other,
        })
    }
}
