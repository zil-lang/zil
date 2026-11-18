//! A concrete cleanup occurrence is not a structural SyncDrop premise.
//! Bind it to every actual owner/closure instance and defer only the terminal
//! worker/frame proof until raw preflight and the resource ledger complete.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(in crate::semantic) enum Event { Scope, Replace(NodeId), Backedge(NodeId), CaptureExit(CallKind), Discard(NodeId) }
impl Event {
    fn node(self) -> Option<NodeId> {
        match self { Self::Replace(node) | Self::Discard(node) | Self::Backedge(node) => Some(node), _ => None }
    }
}
#[derive(Clone, Copy)]
pub(in crate::semantic) enum Source { Local(LocalAnchor), Expression(NodeId), Unknown }
#[derive(Clone)]
pub(super) struct Origin {
    pub(in crate::semantic) definition: SemanticDefinitionId,
    pub(in crate::semantic) source: Source,
    pub(in crate::semantic) path: Vec<u32>,
    pub(super) event: Event,
}
impl Origin {
    pub(super) fn local(definition: SemanticDefinitionId, anchor: Option<LocalAnchor>, event: Event) -> Self {
        Self { definition, source: anchor.map(Source::Local).unwrap_or(Source::Unknown), path: Vec::new(), event }
    }
    pub(super) fn expression(definition: SemanticDefinitionId, node: NodeId) -> Self {
        Self { definition, source: Source::Expression(node), path: Vec::new(), event: Event::Discard(node) }
    }
}
pub(super) struct Occurrence {
    pub(super) term: Term,
    pub(super) origin: Origin,
}

/// Successfully evaluated operands waiting for one actual original call.
/// This is the same cleanup responsibility as a lexical owner, named by its
/// genuine expression occurrence instead of a fabricated local declaration.
pub(super) struct Preparation {
    pub(super) definition: SemanticDefinitionId,
    pub(super) call: NodeId,
    pub(super) values: Vec<Occurrence>,
}
pub(in crate::semantic) struct Pending {
    pub(in crate::semantic) definition: SemanticDefinitionId,
    pub(in crate::semantic) source: Source,
    pub(in crate::semantic) path: Vec<u32>,
    pub(in crate::semantic) event: Event,
    term: Term,
}
#[derive(PartialEq, Eq)]
pub(in crate::semantic) struct Write {
    pub(in crate::semantic) anchor: LocalAnchor,
    pub(in crate::semantic) path: Vec<u32>,
    pub(in crate::semantic) value: NodeId,
}

impl Checked {
    pub(in crate::semantic) fn cleanup_write(&self, definition: SemanticDefinitionId,
        node: NodeId, b: &mut Budget) -> CompileResult<Option<&Write>> {
        b.lookup(self.cleanup_writes.len(), WORK)?;
        Ok(self.cleanup_writes.get(&(definition, node)))
    }
    pub(in crate::semantic) fn pending_cleanups(&self) -> &[Pending] { &self.pending_cleanups }

    pub(in crate::semantic) fn cleanup_value(&self, ordinal: usize,
        context: &SemanticInstance, program: &SemanticModule, inventory: &Inventory<'_>,
        b: &mut Budget) -> CompileResult<crate::semantic::value::Value> {
        b.lookup(self.pending_cleanups.len(), WORK)?;
        let pending = self.pending_cleanups.get(ordinal)
            .filter(|pending| pending.definition == context.definition)
            .ok_or_else(|| fail("original cleanup occurrence changes its declaration scope"))?;
        self.value(pending.term, context, program, inventory, b, 0)
    }
}

impl Solver<'_, '_> {
    pub(super) fn prepare_call_value(
        &mut self, definition: SemanticDefinitionId, node: NodeId,
        term: Term, b: &mut Budget,
    ) -> CompileResult<()> {
        let Some(preparation) = self.preparations.last() else {
            return Err(fail("prepared operand has no original call transaction"));
        };
        if preparation.definition != definition {
            return Err(fail("prepared operand crosses its original call owner"));
        }
        b.spend(preparation.values.len(), WORK)?;
        for value in &preparation.values {
            if matches!(value.origin.source, Source::Expression(actual) if actual == node) {
                if value.term != term {
                    return Err(fail("prepared original expression has competing value identities"));
                }
                return Ok(());
            }
        }
        let origin = Origin::expression(definition, node);
        Self::push(&mut self.preparations.last_mut().expect("checked transaction").values,
            Occurrence { term, origin }, b)
    }

    /// A possible language exit does not destroy the success-path preparation
    /// state. Record independent obligations for every still-owned operand,
    /// newest successful initialization first, within this exact definition.
    pub(super) fn cleanup_preparations(
        &mut self, definition: SemanticDefinitionId, b: &mut Budget,
    ) -> CompileResult<()> {
        for preparation in self.preparations.iter().rev() {
            b.spend(1, WORK)?;
            if preparation.definition != definition { continue; }
            for value in preparation.values.iter().rev() {
                b.spend(value.origin.path.len().saturating_add(1), WORK)?;
                Self::push(&mut self.cleanup_terms,
                    Occurrence { term: value.term, origin: value.origin.clone() }, b)?;
            }
        }
        Ok(())
    }

    pub(super) fn record_cleanup_write(&mut self, definition: SemanticDefinitionId,
        node: NodeId, anchor: Option<LocalAnchor>, path: &[u32], value: NodeId,
        b: &mut Budget) -> CompileResult<()> {
        let Some(anchor) = anchor else { return Ok(()); };
        b.spend(path.len(), WORK)?;
        let write = Write { anchor, path: path.to_vec(), value };
        b.lookup(self.state.cleanup_writes.len(), WORK)?;
        if let Some(previous) = self.state.cleanup_writes.get(&(definition, node)) {
            b.spend(previous.path.len().saturating_add(path.len() + 1), WORK)?;
            if *previous != write { return Err(fail("original assignment changes its exact cleanup source")); }
        } else {
            self.state.cleanup_writes.insert((definition, node), write);
        }
        Ok(())
    }
    pub(super) fn record_cleanup(&mut self, term: Term, origin: &Origin,
        path: &[u32], b: &mut Budget) -> CompileResult<()> {
        b.spend(path.len().saturating_add(origin.path.len() + 1), WORK)?;
        let mut source = origin.clone();
        source.path = path.to_vec();
        Self::push(&mut self.cleanup_terms, Occurrence { term, origin: source }, b)
    }
    fn task_cleanup_environment(
        &self,
        term: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        let term = self.root(term, b)?;
        match &self.state.arena[term] {
            Kind::CallResult { hidden, .. } => {
                self.task_cleanup_environment(*hidden, b, depth + 1)
            }
            Kind::Wrapped(Wrapper::Managed, payload) => {
                Ok(matches!(self.state.arena[self.root(*payload, b)?], Kind::Task(_)))
            }
            Kind::Closure { environment, .. } => {
                let mut task = false;
                for captured in environment {
                    let root = self.root(*captured, b)?;
                    if matches!(self.state.arena[root], Kind::Life(_) | Kind::LifeVariable(_)
                        | Kind::ConstRigid { .. } | Kind::ConstValue(_))
                    {
                        continue;
                    }
                    if self.satisfies(root, Predicate::SyncDrop, b, depth + 1)? {
                        continue;
                    }
                    if !self.task_cleanup_environment(root, b, depth + 1)? {
                        return Ok(false);
                    }
                    task = true;
                }
                Ok(task)
            }
            // An abstract F or arbitrary resource does not acquire a cleanup
            // theorem from one of its concrete callers.
            _ => Ok(false),
        }
    }

    fn suspend_inline_cleanup(&self, term: Term, b: &mut Budget, depth: usize) -> CompileResult<bool> {
        self.step(depth, b)?;
        let term = self.root(term, b)?;
        let fields = match &self.state.arena[term] {
            Kind::Wrapped(Wrapper::Managed, payload) => return Ok(matches!(
                self.state.arena[self.root(*payload, b)?],
                Kind::Task(_) | Kind::Wrapped(Wrapper::AsyncRegistration, _)
            )),
            Kind::Tuple(fields) => fields,
            Kind::Nominal { definition, fields, .. } if matches!(
                self.inventory.definition(*definition, b)?.body,
                SemanticDefinitionBody::Nominal { body: SemanticNominalBody::Enum { .. }, .. }
            ) => fields,
            _ => return Ok(false),
        };
        let mut resumable = false;
        for field in fields {
            if self.satisfies(*field, Predicate::SyncDrop, b, depth + 1)? {
                continue;
            }
            if !self.suspend_inline_cleanup(*field, b, depth + 1)? {
                return Ok(false);
            }
            resumable = true;
        }
        Ok(resumable)
    }

    pub(super) fn check_cleanups(&mut self, _module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
        for ordinal in 0..self.cleanup_terms.len() {
            let occurrence = &self.cleanup_terms[ordinal];
            let term = occurrence.term;
            if self.satisfies(term, Predicate::SyncDrop, b, 0)? { continue; }
            let origin = &occurrence.origin;
            if let Some(node) = origin.event.node() {
                b.lookup(self.indexes.len(), WORK)?;
                self.indexes.get(&origin.definition)
                    .ok_or_else(|| fail("original cleanup event has no declaration body"))?.node(node, b)?;
            }
            let SemanticDefinitionBody::FunctionWithBody { signature, .. } =
                &self.inventory.definition(origin.definition, b)?.body else {
                    return Err(fail("original cleanup occurrence has no complete declaration ABI"));
                };
            let suspend = match origin.event {
                Event::CaptureExit(operation) => operation == CallKind::Suspend,
                _ => signature.call_kind == CallKind::Suspend,
            };
            if !matches!(origin.source, Source::Unknown)
                && (suspend && self.suspend_inline_cleanup(term, b, 0)?
                    || self.task_cleanup_environment(term, b, 0)?
                    || suspend && self.satisfies(term,
                        Predicate::Capability(SemanticBuiltinTrait::ClosedEnvironment), b, 0)?)
            {
                b.spend(origin.path.len(), WORK)?;
                let pending = Pending { definition: origin.definition, source: origin.source,
                    path: origin.path.clone(), event: origin.event, term };
                Self::push(&mut self.state.pending_cleanups, pending, b)?;
                continue;
            }
            b.spend(160, WORK)?;
            let mut errors = fail("original live cleanup edge requires SyncDrop or its complete actual Task cleanup proof");
            errors[0].notes.push(format!("original cleanup definition {:?}, event {:?}, subject {}, selector {:?}, whole kind {:?}",
                origin.definition, origin.event, self.type_root(term, b)?, origin.path,
                std::mem::discriminant(&self.state.arena[self.type_root(term, b)?])));
            return Err(errors);
        }
        Ok(())
    }
}

pub(super) fn finish(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    // Operation, remaining-capture and local cleanup obligations can occur
    // independently in one module. Keep all evidence until every family has
    // passed; a capture-free module must still consume its operation proofs.
    if b.semantic_cleanup_sites.is_empty() && !b.semantic_cleanup_proofs.is_empty() {
        return Err(fail("actual Task cleanup proof has no original pending occurrence"));
    }
    if (!b.semantic_cleanup_sites.is_empty() || !b.semantic_capture_cleanups.is_empty())
        && b.task_input_scopes.is_none() {
        return Err(fail("original concrete cleanup has no complete actual Task input scopes"));
    }
    finish_consuming_operations(module, b)?;
    finish_capture_cleanups(module, b)?;
    b.spend(b.semantic_cleanup_sites.len().saturating_mul(5), WORK)?;
    let sites: Vec<_> = b.semantic_cleanup_sites.iter().copied().collect();
    let mut frames = BTreeSet::new();
    for (function, block, position, local, closure) in sites {
        if !b.task_origin_roots || !b.tasks {
            return Err(fail("original concrete cleanup reached no complete actual Task preflight"));
        }
        let function = module.functions.get(function.0 as usize)
            .filter(|actual| actual.id == function)
            .ok_or_else(|| fail("original concrete cleanup loses its actual function"))?;
        let instruction = function.blocks.get(block.0 as usize)
            .filter(|actual| actual.id == block)
            .and_then(|actual| actual.instructions.get(position))
            .ok_or_else(|| fail("original concrete cleanup loses its actual instruction"))?;
        let declaration = function.locals.get(local.0 as usize)
            .filter(|actual| actual.id == local)
            .ok_or_else(|| fail("original concrete cleanup loses its actual local"))?;
        let slot = match (closure, &declaration.ty) {
            (Some(closure), Type::Array { element, length: 1 }) if element.as_ref() == &Type::Closure(closure) =>
                matches!(instruction.kind, InstructionKind::DropOwnedArrayElement { local: actual, index: 0 }
                    | InstructionKind::DropOwnedArray { local: actual } if actual == local),
            _ => false,
        };
        let registration = function.call_kind == CallKind::Suspend && matches!(&declaration.ty,
            Type::Managed(head) if matches!(head.as_ref(), Type::AsyncRegistration { .. }));
        let inline = function.call_kind == CallKind::Suspend && match (&declaration.ty, &instruction.kind) {
            (Type::Tuple(_), InstructionKind::DropTuple { local: actual })
            | (Type::Enum { .. }, InstructionKind::DropEnum { local: actual }) => *actual == local,
            _ => false,
        };
        let exact = slot || match closure {
            Some(closure) => matches!(instruction.kind, InstructionKind::DropClosure { local: actual } if actual == local)
                && declaration.ty == Type::Closure(closure),
            None => inline || matches!(instruction.kind, InstructionKind::DropManaged { local: actual } if actual == local)
                && (registration || matches!(&declaration.ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. }))),
        };
        if !exact || !(inline || registration || crate::tasks::needs_synchronous_proof(module, &declaration.ty, b)?) {
            return Err(fail("original concrete cleanup changes its exact actual Task-bearing site"));
        }
        b.spend(function.name.len().saturating_add(128), WORK)?;
        // Every conditional local drop, including a concrete Closure,
        // consumes its converged live/vacant resource proof. Original strict
        // Direct checks run there only on live debt; Suspend still needs the
        // actual frame graph below. No vacant value is read here.
        {
            b.lookup(b.semantic_cleanup_proofs.len(), WORK)?;
            if !b.semantic_cleanup_proofs.contains(&(function.id, block, position, local)) {
                let mut errors = fail("original Task cleanup lacks its complete actual resource ledger proof");
                errors[0].notes.push(format!("function {} {}, block {} instruction {}, original-bound local {}",
                    function.id.0, function.name, block.0, position, local.0));
                return Err(errors);
            }
            if function.call_kind == CallKind::Suspend && frames.insert(function.id) {
                crate::tasks::cleanup_frame_profile(module, function, b)?;
            }
            continue;
        }
    }
    // Called only after tasks::check and resources::check_module succeeded.
    // A failure above leaves every pending site intact in this request.
    // All exact original sites and their converged resource/frame proofs
    // succeeded. Drain once, together: neither a skipped family nor a failed
    // later obligation can silently discard another family's pending proof.
    b.semantic_cleanup_sites.clear();
    b.semantic_cleanup_proofs.clear();
    b.semantic_capture_cleanups.clear();
    b.semantic_consuming_operations.clear();
    b.semantic_consuming_cleanups.clear();
    Ok(())
}

fn finish_consuming_operations(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    b.spend(b.semantic_consuming_operations.len().saturating_mul(5), WORK)?;
    let pending: Vec<_> = b.semantic_consuming_operations.iter().map(|(point, upper)| (*point, *upper)).collect();
    for ((function, block, position, closure), upper) in pending {
        let function = module.functions.get(function.0 as usize).filter(|actual| actual.id == function)
            .ok_or_else(|| fail("original consuming operation loses its actual caller"))?;
        let instruction = function.blocks.get(block.0 as usize).filter(|actual| actual.id == block)
            .and_then(|block| block.instructions.get(position))
            .ok_or_else(|| fail("original consuming operation loses its exact actual opcode"))?;
        let InstructionKind::CallClosureOnce { callee, .. } = instruction.kind else {
            return Err(fail("original consuming operation changes its actual receiver mode"));
        };
        if function.values.get(callee.0 as usize) != Some(&Type::Closure(closure))
            || upper == CallKind::Suspend && function.call_kind != CallKind::Suspend {
            return Err(fail("original consuming operation changes its receiver or execution upper"));
        }
        b.lookup(b.semantic_consuming_cleanups.len(), WORK)?;
        let proof = b.semantic_consuming_cleanups.get(&(function.id, block, position, closure))
            .ok_or_else(|| fail("original consuming operation lacks complete actual remaining-environment cleanup"))?;
        if *proof == CallKind::Suspend && (upper != CallKind::Suspend || !b.task_origin_roots || !b.tasks) {
            return Err(fail("Direct consuming operation cannot use a resumable cleanup theorem"));
        }
    }
    // No orphan completion may be discarded by the terminal drain. Resource
    // checks produce a completion only for an exact operation obligation or a
    // remaining-capture occurrence; preserve that invariant at consumption.
    b.spend(b.semantic_consuming_cleanups.len().saturating_mul(4), WORK)?;
    let completed: Vec<_> = b.semantic_consuming_cleanups.keys().copied().collect();
    for point in completed {
        b.lookup(b.semantic_consuming_operations.len(), WORK)?;
        if b.semantic_consuming_operations.contains_key(&point) { continue; }
        b.spend(b.semantic_capture_cleanups.len(), WORK)?;
        if !b.semantic_capture_cleanups.iter().any(|(closure, _, _, _)| *closure == point.3) {
            return Err(fail("actual consuming cleanup proof has no original operation or capture obligation"));
        }
    }
    Ok(())
}

fn finish_capture_cleanups(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    if b.semantic_capture_cleanups.is_empty() { return Ok(()); }
    if !b.task_origin_roots || !b.tasks {
        return Err(fail("original remaining capture cleanup reached no complete actual Task proof"));
    }
    let work = b.semantic_capture_cleanups.iter().fold(0usize,
        |work, (_, _, path, _)| work.saturating_add(path.len().saturating_add(1)));
    b.spend(work, WORK)?;
    let pending: Vec<_> = b.semantic_capture_cleanups.iter().cloned().collect();
    for (id, capture, path, upper) in pending {
        let closure = module.closures.get(id.0 as usize).filter(|closure| closure.id == id
            && matches!(closure.kind, CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce))
            .ok_or_else(|| fail("original remaining capture cleanup changes its actual entry"))?;
        // These are unconditional actual entry theorems over every return,
        // independent of one caller's concrete input or a Static/header claim.
        if crate::tasks::once_cleanup_has_no_task(module, id, b)?
            || crate::tasks::once_cleanup_closed(module, id, b)? { continue; }
        // Fn/FnMut entries may implement a requested FnOnce invocation. Its
        // post-return destruction belongs to the actual consuming operation,
        // never to every ordinary reusable call or to a global SyncDrop bound.
        let mut calls = 0usize;
        for function in &module.functions {
            for block in &function.blocks {
                for (position, instruction) in block.instructions.iter().enumerate() {
                    b.spend(1, WORK)?;
                    let InstructionKind::CallClosureOnce { callee, .. } = instruction.kind else { continue; };
                    if function.values.get(callee.0 as usize) != Some(&Type::Closure(id)) { continue; }
                    b.lookup(b.semantic_consuming_cleanups.len(), WORK)?;
                    let proof = b.semantic_consuming_cleanups.get(&(function.id, block.id, position, id));
                    if proof.is_none() || upper == CallKind::Direct && proof != Some(&CallKind::Direct) {
                        return Err(fail("original remaining capture cleanup lacks its complete actual consuming-call proof at the promised execution upper"));
                    }
                    calls += 1;
                }
            }
        }
        if calls != 0 { continue; }
        if !path.is_empty() {
            return Err(unknown("remaining capture cleanup needs its complete synchronous selected child source"));
        }
        let expected = closure.captures.get(capture as usize)
            .ok_or_else(|| fail("original remaining cleanup selects no actual capture"))?;
        let mut constructors = 0usize;
        for function in &module.functions {
            for block in &function.blocks {
                for instruction in &block.instructions {
                    b.spend(1, WORK)?;
                    let InstructionKind::MakeClosure { closure: actual, captures } = &instruction.kind else { continue; };
                    if *actual != id { continue; }
                    if captures.len() != closure.captures.len() {
                        return Err(fail("remaining capture cleanup constructor omits its complete original environment"));
                    }
                    let value = *captures.get(capture as usize)
                        .ok_or_else(|| fail("remaining capture cleanup constructor has no selected operand"))?;
                    let ty = function.values.get(value.0 as usize)
                        .ok_or_else(|| fail("remaining capture cleanup operand has no complete actual type"))?;
                    crate::dyn_metadata::ty(ty, b)?;
                    crate::dyn_metadata::ty(expected, b)?;
                    if ty != expected {
                        return Err(fail("remaining capture cleanup constructor changes its complete selected type"));
                    }
                    crate::tasks::require_original_synchronous_value(module, function, value, b, Some(instruction.span))?;
                    constructors += 1;
                }
            }
        }
        if constructors == 0 {
            return Err(fail("remaining capture cleanup has no complete actual constructor sources"));
        }
    }
    Ok(())
}
