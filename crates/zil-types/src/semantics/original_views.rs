//! Directed projected-loan bounds. These retain the original input and each
//! real operation; no edge establishes equality of lifetime classes.
use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum ViewBorrowStepKind {
    Parameter,
    ParameterStorage,
    Slice,
    Index {
        constant: Option<u32>,
    },
    Dereference,
    CallResult {
        site: SemanticSiteId,
    },
    /// Complete declaration-proved returned input occurrences, never inputs
    /// selected by equality of their caller lifetime images.
    CallInputs { site: SemanticSiteId, inputs: Vec<(u32, NodeId)> },
    CallableReceiver { site: SemanticSiteId, source: NodeId },
    BytesBorrow { site: SemanticSiteId, mutable: bool },
    Capture {
        name: String,
    },
    ProjectedLocal {
        statement: NodeId,
        pattern: Vec<u32>,
        read: bool,
    },
    Local {
        binding: NodeId,
    },
    Alias {
        binding: NodeId,
    },
    ReadLocal {
        binding: NodeId,
    },
    PreparedShared,
    Field {
        definition: SemanticDefinitionId,
        ordinal: u32,
    },
    StoreBorrow {
        site: SemanticSiteId,
        mutable: bool,
    },
    Borrow,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewBorrowRoot {
    Parameter(u32),
    Local(NodeId),
    Result(NodeId),
    Capture(NodeId),
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ViewBorrowStep {
    pub node: NodeId,
    pub kind: ViewBorrowStepKind,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct ViewBorrowBound {
    pub root: ViewBorrowRoot,
    pub lifetime: Option<SemanticOuterLifeRecipe>,
    pub steps: Vec<ViewBorrowStep>,
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct BorrowRole {
    pub root: ViewBorrowRoot,
    pub source_lifetime: SemanticOuterLifeRecipe,
    pub target_lifetime: SemanticOuterLifeRecipe,
    pub occurrence_lifetime: SemanticOuterLifeRecipe,
    pub inner: SemanticTypeRecipe,
    pub steps: Vec<ViewBorrowStep>,
}
pub(super) fn receiver_upper() -> SemanticOuterLifeRecipe {
    SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::OuterOccurrence {
        operand: SemanticRecipeOperand::Receiver,
        path: Vec::new(),
        mode: StoredBorrowMode::SharedAddress,
    })
}
pub(super) fn is_receiver_upper(life: &SemanticOuterLifeRecipe) -> bool {
    matches!(life, SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::OuterOccurrence {
        operand: SemanticRecipeOperand::Receiver, path, mode: StoredBorrowMode::SharedAddress,
    }) if path.is_empty())
}
pub(super) fn snapshot_bound(
    value: &ViewBorrowBound,
    limits: &Limits,
    left: &mut usize,
    depth: usize,
) -> R<ViewBorrowBound> {
    if depth > limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "projected loan source depth exceeded",
            None,
        ));
    }
    *left = left
        .checked_sub(value.steps.len().saturating_add(1))
        .ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "projected loan source work exceeded", None)
        })?;
    for step in &value.steps {
        if let ViewBorrowStepKind::Capture { name } = &step.kind {
            *left = left.checked_sub(name.len()).ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "capture loan source work exceeded", None)
            })?;
        }
        if let ViewBorrowStepKind::CallInputs { inputs, .. } = &step.kind {
            if inputs.len() > limits.max_nodes {
                return Err(Diagnostic::limit(Phase::Types, "returned call input count exceeded", None));
            }
            *left = left.checked_sub(inputs.len().saturating_mul(2)).ok_or_else(||
                Diagnostic::limit(Phase::Types, "returned call input snapshot exceeded", None))?;
        }
        if let ViewBorrowStepKind::ProjectedLocal { pattern, .. } = &step.kind {
            if pattern.len() > limits.max_call_depth {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "projected binding loan depth exceeded",
                    None,
                ));
            }
            *left = left.checked_sub(pattern.len()).ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "projected binding loan work exceeded", None)
            })?;
        }
    }
    if let Some(lifetime) = &value.lifetime {
        crate::dyn_metadata::semantic_operand_snapshot(lifetime, limits, left, 1)?;
    }
    Ok(value.clone())
}
/// Snapshot the complete existing directed receipt before a deep comparison.
pub(super) fn snapshot_role(value: &BorrowRole, limits: &Limits, left: &mut usize) -> R<BorrowRole> {
    for lifetime in [&value.source_lifetime, &value.target_lifetime, &value.occurrence_lifetime] {
        crate::dyn_metadata::semantic_operand_snapshot(lifetime, limits, left, 1)?;
    }
    crate::dyn_metadata::semantic_operand_snapshot(&value.inner, limits, left, 1)?;
    // The immutable path is not cloned until all of its payload is paid.
    *left = left.checked_sub(value.steps.len().saturating_add(1)).ok_or_else(||
        Diagnostic::limit(Phase::Types, "receiver receipt snapshot exceeded", None))?;
    for step in &value.steps {
        match &step.kind {
            ViewBorrowStepKind::Capture { name } => {
                *left = left.checked_sub(name.len()).ok_or_else(||
                    Diagnostic::limit(Phase::Types, "receiver source name snapshot exceeded", None))?;
            }
            ViewBorrowStepKind::CallInputs { inputs, .. } => {
                if inputs.len() > limits.max_nodes {
                    return Err(Diagnostic::limit(Phase::Types, "receiver return input count exceeded", None));
                }
                *left = left.checked_sub(inputs.len().saturating_mul(2)).ok_or_else(||
                    Diagnostic::limit(Phase::Types, "receiver return input snapshot exceeded", None))?;
            }
            ViewBorrowStepKind::ProjectedLocal { pattern, .. } => {
                if pattern.len() > limits.max_call_depth {
                    return Err(Diagnostic::limit(Phase::Types, "receiver receipt path depth exceeded", None));
                }
                *left = left.checked_sub(pattern.len()).ok_or_else(||
                    Diagnostic::limit(Phase::Types, "receiver receipt path snapshot exceeded", None))?;
            }
            _ => {}
        }
    }
    Ok(value.clone())
}

struct ReceiverReturns<'a, 'b> {
    proof: &'a Proof,
    owner: SemanticDefinitionId,
    limits: &'b Limits,
    left: &'b mut usize,
    count: usize,
    capture: bool,
    valid: bool,
}
impl super::super::original_walk::Context for ReceiverReturns<'_, '_> {
    fn descend_closure(&self) -> bool { false }
    fn node(&mut self, depth: usize) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(Phase::Types, "receiver return proof depth exceeded", None));
        }
        *self.left = self.left.checked_sub(1).ok_or_else(||
            Diagnostic::limit(Phase::Types, "receiver return proof work exceeded", None))?;
        Ok(())
    }
    fn ty(&mut self, _: &mut SemanticTypeRecipe, depth: usize) -> R<()> { self.node(depth) }
    fn statement(&mut self, _: NodeId, kind: &SemanticOriginalStatementKind, depth: usize) -> R<()> {
        self.node(depth)?;
        let CoreStatementKind::Return(value) = kind else { return Ok(()); };
        self.count = self.count.checked_add(1).ok_or_else(|| bad("receiver return count overflows"))?;
        if self.count > self.limits.max_nodes {
            return Err(Diagnostic::limit(Phase::Types, "receiver return count exceeded", None));
        }
        let Some(value) = value else { self.valid = false; return Ok(()); };
        *self.left = self.left.checked_sub(self.proof.types.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "receiver return lookup exceeded", None))?;
        let Some(SemanticTypeRecipe::Address { mutable: false, lifetime, inner }) =
            self.proof.types.get(&(self.owner, Query::Expression(value.id)))
        else { self.valid = false; return Ok(()); };
        crate::dyn_metadata::semantic_operand_snapshot(lifetime, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(inner.as_ref(), self.limits, self.left, 1)?;
        if matches!(lifetime, SemanticOuterLifeRecipe::Static) { return Ok(()); }
        *self.left = self.left.checked_sub(self.proof.return_borrows.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "receiver return source lookup exceeded", None))?;
        let Some(role) = self.proof.return_borrows.get(&(self.owner, value.id)) else {
            self.valid = false; return Ok(());
        };
        let role = snapshot_role(role, self.limits, self.left)?;
        let sources = self.proof.view_sources(self.owner, role.root, &role.steps, self.limits, self.left,
            &mut BTreeSet::new(), depth + 1)?;
        if !sources.is_some_and(|roots| !roots.is_empty() && roots.iter().all(|root| matches!(root, ViewBorrowRoot::Capture(_))))
            || !is_receiver_upper(&role.target_lifetime)
            || &role.occurrence_lifetime != lifetime || role.inner != **inner
        { self.valid = false; }
        self.capture = true;
        Ok(())
    }
}
struct ReturnedInputs<'a, 'b> {
    proof: &'a Proof, owner: SemanticDefinitionId,
    limits: &'b Limits, left: &'b mut usize,
    inputs: BTreeSet<u32>, count: usize, valid: bool,
}
impl super::super::original_walk::Context for ReturnedInputs<'_, '_> {
    fn descend_closure(&self) -> bool { false }
    fn node(&mut self, depth: usize) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(Phase::Types, "returned input source depth exceeded", None));
        }
        *self.left = self.left.checked_sub(1).ok_or_else(||
            Diagnostic::limit(Phase::Types, "returned input source work exceeded", None))?;
        Ok(())
    }
    fn ty(&mut self, _: &mut SemanticTypeRecipe, depth: usize) -> R<()> { self.node(depth) }
    fn statement(&mut self, _: NodeId, kind: &SemanticOriginalStatementKind, depth: usize) -> R<()> {
        self.node(depth)?;
        let CoreStatementKind::Return(value) = kind else { return Ok(()); };
        self.count = self.count.checked_add(1).ok_or_else(|| bad("returned source count overflows"))?;
        if self.count > self.limits.max_nodes {
            return Err(Diagnostic::limit(Phase::Types, "returned source count exceeded", None));
        }
        let Some(value) = value else { self.valid = false; return Ok(()); };
        *self.left = self.left.checked_sub(self.proof.types.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "returned source type lookup exceeded", None))?;
        let Some(SemanticTypeRecipe::Address { mutable: false, lifetime, .. }) =
            self.proof.types.get(&(self.owner, Query::Expression(value.id)))
        else { self.valid = false; return Ok(()); };
        if matches!(lifetime, SemanticOuterLifeRecipe::Static) { return Ok(()); }
        *self.left = self.left.checked_sub(self.proof.view_bounds.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "returned source bound lookup exceeded", None))?;
        let Some(bound) = self.proof.view_bounds.get(&(self.owner, Query::Expression(value.id))) else {
            self.valid = false; return Ok(());
        };
        let Some(roots) = self.proof.view_sources(self.owner, bound.root, &bound.steps, self.limits, self.left,
            &mut BTreeSet::new(), depth + 1)? else { self.valid = false; return Ok(()); };
        for root in roots {
            let ViewBorrowRoot::Parameter(parameter) = root else { self.valid = false; continue; };
            *self.left = self.left.checked_sub(self.inputs.len().saturating_add(1).ilog2() as usize + 2)
                .ok_or_else(|| Diagnostic::limit(Phase::Types, "returned input directory work exceeded", None))?;
            if self.inputs.len() >= self.limits.max_nodes {
                return Err(Diagnostic::limit(Phase::Types, "returned input directory limit exceeded", None));
            }
            self.inputs.insert(parameter);
        }
        Ok(())
    }
}

impl Proof {
    /// Resolve only already checked operation-source edges. A call result is
    /// not an input alias unless its complete original returns say so.
    fn view_sources(
        &self, owner: SemanticDefinitionId, root: ViewBorrowRoot, steps: &[ViewBorrowStep],
        limits: &Limits, left: &mut usize, active: &mut BTreeSet<NodeId>, depth: usize,
    ) -> R<Option<Vec<ViewBorrowRoot>>> {
        if depth > limits.max_call_depth {
            return Err(Diagnostic::limit(Phase::Types, "original return-source depth exceeded", None));
        }
        *left = left.checked_sub(steps.len()).ok_or_else(|| Diagnostic::limit(
            Phase::Types, "original return-source path work exceeded", None))?;
        let node = match root {
            ViewBorrowRoot::Result(node) => node,
            ViewBorrowRoot::Local(binding) if steps.iter().any(|step|
                matches!(step.kind, ViewBorrowStepKind::ReadLocal { binding: selected } if selected == binding)) => {
                let Some(sources) = self.view_local_sources(owner, binding, limits, left)? else {
                    return Ok(None);
                };
                *left = left.checked_sub(active.len().saturating_add(1).ilog2() as usize + 1)
                    .ok_or_else(|| Diagnostic::limit(Phase::Types, "original local source cycle lookup exceeded", None))?;
                if !active.insert(binding) { return Err(bad("original local return-source cycle")); }
                let result = (|| {
                    let mut roots = Vec::new();
                    for source in sources {
                        *left = left.checked_sub(self.view_bounds.len().saturating_add(1).ilog2() as usize + 1)
                            .ok_or_else(|| Diagnostic::limit(Phase::Types, "local return-source lookup exceeded", None))?;
                        let Some(bound) = self.view_bounds.get(&(owner, Query::Expression(source))) else { return Ok(None); };
                        let Some(next) = self.view_sources(owner, bound.root, &bound.steps, limits, left, active, depth + 1)?
                        else { return Ok(None); };
                        if roots.len().saturating_add(next.len()) > limits.max_nodes {
                            return Err(Diagnostic::limit(Phase::Types, "original local source domain node limit exceeded", None));
                        }
                        *left = left.checked_sub(next.len()).ok_or_else(|| Diagnostic::limit(
                            Phase::Types, "original local return-source snapshot exceeded", None))?;
                        roots.extend(next);
                    }
                    Ok(Some(roots))
                })();
                active.remove(&binding);
                return result;
            }
            _ => {
                if matches!(root, ViewBorrowRoot::Parameter(_))
                    && !steps.iter().any(|step| matches!(step.kind, ViewBorrowStepKind::Parameter))
                { return Ok(None); }
                *left = left.checked_sub(1).ok_or_else(|| Diagnostic::limit(
                    Phase::Types, "original return-source leaf work exceeded", None))?;
                return Ok(Some(vec![root]));
            }
        };
        *left = left.checked_sub(active.len().saturating_add(1).ilog2() as usize + steps.len() + 1)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "original return-source work exceeded", None))?;
        if !active.insert(node) { return Err(bad("original return-source operation cycle")); }
        let mut sources = None;
        for step in steps {
            if step.node != node { continue; }
            let selected = match &step.kind {
                ViewBorrowStepKind::CallInputs { inputs, .. } => {
                    if inputs.len() > limits.max_nodes {
                        return Err(Diagnostic::limit(Phase::Types, "original returned argument count exceeded", None));
                    }
                    *left = left.checked_sub(inputs.len()).ok_or_else(|| Diagnostic::limit(
                        Phase::Types, "original returned arguments work exceeded", None))?;
                    inputs.iter().map(|(_, node)| *node).collect::<Vec<_>>()
                }
                ViewBorrowStepKind::CallableReceiver { source, .. } => {
                    *left = left.checked_sub(1).ok_or_else(|| Diagnostic::limit(
                        Phase::Types, "original returned receiver work exceeded", None))?;
                    vec![*source]
                }
                _ => continue,
            };
            if sources.replace(selected).is_some() {
                return Err(bad("call result repeats its original source selection"));
            }
        }
        let result = if let Some(sources) = sources {
            if sources.is_empty() { None } else {
                let mut roots = Vec::new();
                for source in sources {
                    *left = left.checked_sub(self.view_bounds.len().saturating_add(1).ilog2() as usize + 1)
                        .ok_or_else(|| Diagnostic::limit(Phase::Types, "original return-source lookup exceeded", None))?;
                    let Some(bound) = self.view_bounds.get(&(owner, Query::Expression(source))) else {
                        active.remove(&node); return Ok(None);
                    };
                    let Some(children) = self.view_sources(owner, bound.root, &bound.steps, limits, left, active, depth + 1)? else {
                        active.remove(&node); return Ok(None);
                    };
                    if roots.len().saturating_add(children.len()) > limits.max_nodes {
                        return Err(Diagnostic::limit(Phase::Types, "original return-source domain limit exceeded", None));
                    }
                    *left = left.checked_sub(roots.len().saturating_add(children.len()))
                        .ok_or_else(|| Diagnostic::limit(Phase::Types, "original return-source union work exceeded", None))?;
                    roots.extend(children);
                }
                Some(roots)
            }
        } else { None };
        active.remove(&node);
        Ok(result)
    }
    fn returned_input_parameters(
        &self, program: &SemanticModule, owner: SemanticDefinitionId,
        limits: &Limits, left: &mut usize,
    ) -> R<Option<Vec<u32>>> {
        *left = left.checked_sub(program.definitions.len()
            .saturating_add(self.complete.len().saturating_add(1).ilog2() as usize + 1))
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "returned input declaration lookup exceeded", None))?;
        if !self.complete.contains(&owner) { return Ok(None); }
        let definition = program.definitions.iter().find(|row| row.id == owner)
            .ok_or_else(|| bad("returned inputs have no original definition"))?;
        let SemanticDefinitionBody::FunctionWithBody {
            body: SemanticOriginalFunctionBody::Defined(body), .. } = &definition.body
        else { return Ok(None); };
        if definition.kind != SemanticDefinitionKind::Function { return Ok(None); }
        crate::dyn_metadata::semantic_operand_snapshot(body, limits, left, 1)?;
        let mut body = body.clone();
        let mut returned = ReturnedInputs { proof: self, owner, limits, left,
            inputs: BTreeSet::new(), count: 0, valid: true };
        super::super::original_walk::block(&mut body, &mut returned, 0)?;
        if !returned.valid || returned.count == 0 || returned.inputs.is_empty() { return Ok(None); }
        *returned.left = returned.left.checked_sub(returned.inputs.len()).ok_or_else(||
            Diagnostic::limit(Phase::Types, "returned input ordinal snapshot exceeded", None))?;
        Ok(Some(returned.inputs.into_iter().collect()))
    }
    /// Complete declaration-proved source roots, including captures and
    /// activation-local roots. Consumers must retain the entire result;
    /// filtering it to parameter ordinals cannot establish an outside root.
    pub(in crate::semantics) fn view_source_roots(
        &self, owner: SemanticDefinitionId, root: ViewBorrowRoot, steps: &[ViewBorrowStep],
        limits: &Limits, left: &mut usize,
    ) -> R<Option<Vec<ViewBorrowRoot>>> {
        self.view_sources(owner, root, steps, limits, left, &mut BTreeSet::new(), 0)
    }
    pub(in crate::semantics) fn view_input_referents(
        &self, owner: SemanticDefinitionId, root: ViewBorrowRoot, steps: &[ViewBorrowStep],
        limits: &Limits, left: &mut usize,
    ) -> R<Vec<u32>> {
        let Some(roots) = self.view_sources(owner, root, steps, limits, left, &mut BTreeSet::new(), 0)?
        else { return Ok(Vec::new()); };
        *left = left.checked_sub(roots.len()).ok_or_else(|| Diagnostic::limit(
            Phase::Types, "returned input referent directory exceeded", None))?;
        Ok(roots.into_iter().filter_map(|root| match root {
            ViewBorrowRoot::Parameter(parameter) => Some(parameter), _ => None,
        }).collect())
    }
    pub(in crate::semantics) fn view_local_sources(
        &self, owner: SemanticDefinitionId, binding: NodeId,
        limits: &Limits, left: &mut usize,
    ) -> R<Option<Vec<NodeId>>> {
        *left = left.checked_sub(self.local_roles.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "local source directory lookup exceeded", None))?;
        let Some(role) = self.local_roles.get(&(owner, binding)) else { return Ok(None); };
        if role.sources.len() > limits.max_nodes {
            return Err(Diagnostic::limit(Phase::Types, "local source directory node limit exceeded", None));
        }
        *left = left.checked_sub(role.sources.len()).ok_or_else(|| Diagnostic::limit(
            Phase::Types, "local source directory snapshot exceeded", None))?;
        let mut sources = Vec::new();
        for source in &role.sources {
            let SemanticOriginalNode::Expression(node) = source.at else { return Ok(None); };
            *left = left.checked_sub(self.view_bounds.len().saturating_add(1).ilog2() as usize + 1)
                .ok_or_else(|| Diagnostic::limit(Phase::Types, "local source expression lookup exceeded", None))?;
            if !self.view_bounds.contains_key(&(owner, Query::Expression(node))) { return Ok(None); }
            sources.push(node);
        }
        Ok((!sources.is_empty()).then_some(sources))
    }
    pub(in crate::semantics) fn view_source_steps(
        &self, owner: SemanticDefinitionId, node: NodeId, limits: &Limits, left: &mut usize,
    ) -> R<Option<(ViewBorrowRoot, Vec<ViewBorrowStep>)>> {
        *left = left.checked_sub(self.view_bounds.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "actual returned source lookup exceeded", None))?;
        let Some(bound) = self.view_bounds.get(&(owner, Query::Expression(node))) else { return Ok(None); };
        let bound = snapshot_bound(bound, limits, left, 0)?;
        Ok(Some((bound.root, bound.steps)))
    }
    /// A declaration-domain theorem, not a type equality or a runtime source.
    /// Its actual caller must still check every returned capture derivation.
    pub(in crate::semantics) fn receiver_result_upper(
        &self, program: &SemanticModule, owner: SemanticDefinitionId,
        limits: &Limits, left: &mut usize,
    ) -> R<bool> {
        *left = left.checked_sub(program.definitions.len()
            .saturating_add(self.complete.len().saturating_add(1).ilog2() as usize + 1)
            .saturating_add(self.kinds.len().saturating_add(1).ilog2() as usize + 1))
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "receiver entry lookup exceeded", None))?;
        if !self.complete.contains(&owner)
            || !matches!(self.kinds.get(&owner), Some(CallableKind::Fn | CallableKind::FnMut))
        { return Ok(false); }
        let definition = program.definitions.iter().find(|row| row.id == owner)
            .ok_or_else(|| bad("receiver result has no original closure definition"))?;
        let SemanticDefinitionBody::FunctionWithBody { signature,
            body: SemanticOriginalFunctionBody::Defined(body), .. } = &definition.body
        else { return Ok(false); };
        if definition.kind != SemanticDefinitionKind::Closure || signature.call_kind != CallKind::Direct {
            return Ok(false);
        }
        crate::dyn_metadata::semantic_operand_snapshot(body, limits, left, 1)?;
        let mut body = body.clone();
        let mut returns = ReceiverReturns { proof: self, owner, limits, left,
            count: 0, capture: false, valid: true };
        super::super::original_walk::block(&mut body, &mut returns, 0)?;
        Ok(returns.valid && returns.capture && returns.count != 0)
    }
}

impl Check<'_, '_> {
    pub(super) fn bind_receiver_call_result(
        &mut self,
        owner: SemanticDefinitionId,
        call: &SemanticOriginalExpr,
        callee: &SemanticOriginalExpr,
        callee_type: &SemanticTypeRecipe,
        signature: &mut SemanticSignatureRecipe,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<bool> {
        let SemanticTypeRecipe::Address { mutable: false, lifetime, .. } = &mut signature.result else {
            return Ok(false);
        };
        if !is_receiver_upper(lifetime) { return Ok(false); }
        let source = match callee_type {
            SemanticTypeRecipe::Address { lifetime, .. } => {
                crate::dyn_metadata::semantic_operand_snapshot(lifetime, self.limits, self.left, 1)?;
                lifetime.clone()
            }
            _ => self.borrow_occurrence(owner, call.id, true)?,
        };
        let bound = self.borrow_occurrence(owner, call.id, true)?;
        *lifetime = bound.clone();
        {
            let SemanticOuterLifeRecipe::Class(life) = bound else {
                return Err(bad("owned callable receiver lacks its original occurrence"));
            };
            let escape = if matches!(callee_type, SemanticTypeRecipe::Address { .. }) {
                self.role_source(owner, callee, &source, variables, depth + 1)?
            } else { OriginalRoleEscape::LocalPlace { owner, node: callee.id } };
            let role = LocalRole {
                kind: LocalRoleKind::Address { mutable: false },
                binding: call.id,
                lifetime: life.clone(),
                ty: copy_recipe(&signature.result, self.limits, self.left)?,
                sources: vec![LocalRoleSource {
                    at: SemanticOriginalNode::Expression(callee.id),
                    lifetime: source,
                    escape,
                }],
                escapes: false,
                result_upper: None,
            };
            self.pay(self.proof.local_roles.len().saturating_add(1).ilog2() as usize + 3, depth)?;
            if let Some(previous) = self.proof.local_roles.get(&(owner, call.id)) {
                crate::dyn_metadata::semantic_operand_snapshot(&previous.ty, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&role.ty, self.limits, self.left, 1)?;
                for sources in [&previous.sources, &role.sources] {
                    for source in sources {
                        crate::dyn_metadata::semantic_operand_snapshot(&source.lifetime, self.limits, self.left, 1)?;
                        // The exact occurrence and escape discriminator carry
                        // only fixed-size node/owner/ordinal identities.
                        *self.left = self.left.checked_sub(2).ok_or_else(||
                            Diagnostic::limit(Phase::Types, "receiver source identity snapshot exceeded", None))?;
                    }
                }
                if previous.binding != role.binding || previous.kind != role.kind
                    || previous.lifetime != role.lifetime || previous.ty != role.ty
                    || previous.sources != role.sources {
                    return Err(bad("receiver occurrence changes its exact original call sources"));
                }
            } else { self.proof.proof_mut()?.local_roles.insert((owner, call.id), role); }
        }
        Ok(true)
    }
    /// A logical shared result upper for an actual environment projection.
    /// The expression keeps its original sigma and complete inner Type; this
    /// records a directed view, not a lifetime equality or a new Fixed class.
    pub(super) fn receiver_return_value(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        actual: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        if self.definition(owner)?.kind != SemanticDefinitionKind::Closure {
            return Ok(None);
        }
        let SemanticTypeRecipe::Address { mutable: false, lifetime, inner } = actual else {
            return Ok(None);
        };
        if matches!(lifetime, SemanticOuterLifeRecipe::Static) || is_receiver_upper(lifetime) {
            return Ok(None);
        }
        self.view_bound(owner, expression, variables, depth + 1)?;
        self.pay(self.proof.view_bounds.len().saturating_add(1).ilog2() as usize + 1, depth)?;
        let Some(bound) = self.proof.view_bounds.get(&(owner, Query::Expression(expression.id))) else {
            return Ok(None);
        };
        let sources = self.proof.view_sources(owner, bound.root, &bound.steps, self.limits, self.left,
            &mut BTreeSet::new(), depth + 1)?;
        if !sources.is_some_and(|roots| !roots.is_empty() && roots.iter().all(|root| matches!(root, ViewBorrowRoot::Capture(_)))) {
            return Ok(None);
        }
        let bound = snapshot_bound(bound, self.limits, self.left, depth + 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(lifetime, self.limits, self.left, 2)?;
        let role = BorrowRole {
            root: bound.root,
            source_lifetime: bound.lifetime.unwrap_or_else(|| lifetime.clone()),
            target_lifetime: receiver_upper(),
            occurrence_lifetime: lifetime.clone(),
            inner: copy_recipe(inner, self.limits, self.left)?,
            steps: bound.steps,
        };
        self.pay(self.proof.return_borrows.len().saturating_add(1).ilog2() as usize + 1, depth)?;
        let rows = &mut self.proof.proof_mut()?.return_borrows;
        if let Some(old) = rows.get(&(owner, expression.id)) {
            let old = snapshot_role(old, self.limits, self.left)?;
            let current = snapshot_role(&role, self.limits, self.left)?;
            if old != current { return Err(bad("receiver result changes its exact original view")); }
        } else {
            rows.insert((owner, expression.id), role);
        }
        if matches!(lifetime, SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
            scope: SemanticScopeRef::Ancestor(0), ..
        })) && self.input_lifetime(owner, lifetime, depth + 1)?.is_none() {
            Ok(Some(SemanticTypeRecipe::Address {
                mutable: false, lifetime: receiver_upper(),
                inner: Box::new(copy_recipe(inner, self.limits, self.left)?),
            }))
        } else {
            // An existing rigid outside lifetime stays an outside lifetime.
            Ok(None)
        }
    }
    /// An explicit shared binding/ascription observes an exclusive input;
    /// it does not transfer or copy that input. Only the root mode changes.
    /// Its original read keeps its exact exclusive sigma and whole pointee.
    pub(super) fn shared_input_view(
        &mut self,
        owner: SemanticDefinitionId,
        target: Query,
        expression: &SemanticOriginalExpr,
        expected: &SemanticTypeRecipe,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        let Some(shared) =
            self.observe_shared_input(owner, expression, expected, variables, depth + 1)?
        else {
            return Ok(None);
        };
        self.pay(
            target.copy_work().saturating_add(
                self.proof.shared_views.len().saturating_add(1).ilog2() as usize + 1,
            ),
            depth,
        )?;
        let rows = &mut self.proof.proof_mut()?.shared_views;
        match rows.entry((owner, target)) {
            std::collections::btree_map::Entry::Occupied(old) if *old.get() != expression.id => {
                return Err(bad("shared input view changes its exact original read"));
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
            std::collections::btree_map::Entry::Vacant(row) => {
                row.insert(expression.id);
            }
        }
        Ok(Some(shared))
    }

    /// A solved call parameter can borrow a root-shared view without changing
    /// its original expression equation or registering an ascription there.
    /// Only the caller's complete expected type selects this boundary view.
    pub(super) fn observe_shared_input(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        expected: &SemanticTypeRecipe,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        let SemanticTypeRecipe::Address { mutable: false, .. } = expected else {
            return Ok(None);
        };
        let mut root = expression;
        while let CoreExprKind::Group(value) = &root.kind {
            self.pay(1, depth)?;
            root = value;
        }
        let CoreExprKind::Name(name) = &root.kind else {
            return Ok(None);
        };
        self.pay(name.len(), depth)?;
        let Some(binding) = variables.get(name) else {
            return Ok(None);
        };
        let source = copy_recipe(&binding.ty, self.limits, self.left)?;
        let source = self.observation_type(owner, &source, depth + 1)?;
        let SemanticTypeRecipe::Address {
            mutable: true,
            lifetime,
            inner,
        } = &source
        else {
            return Ok(None);
        };
        crate::dyn_metadata::semantic_operand_snapshot(lifetime, self.limits, self.left, 1)?;
        let shared = SemanticTypeRecipe::Address {
            mutable: false,
            lifetime: lifetime.clone(),
            inner: Box::new(copy_recipe(inner, self.limits, self.left)?),
        };
        // This is root permission attenuation, not lifetime shortening or
        // a conversion of an intrinsic/nominal child class.
        self.same(&shared, expected)?;
        let observed = self.place(owner, expression, variables, depth + 1)?;
        self.same(&observed, &source)?;
        Ok(Some(shared))
    }

    pub(super) fn alias_view_bound(
        &mut self,
        owner: SemanticDefinitionId,
        target: &Query,
        source: &Query,
        depth: usize,
    ) -> R<()> {
        let Some(value) = self.proof.view_bounds.get(&(owner, source.clone())) else {
            return Ok(());
        };
        let value = snapshot_bound(value, self.limits, self.left, depth + 1)?;
        self.insert_view_bound(owner, target.clone(), value, depth + 1)
    }
    fn insert_view_bound(
        &mut self,
        owner: SemanticDefinitionId,
        target: Query,
        value: ViewBorrowBound,
        depth: usize,
    ) -> R<()> {
        self.pay(
            target
                .copy_work()
                .saturating_add(value.steps.len())
                .saturating_add(1),
            depth,
        )?;
        if let Some(old) = self.proof.view_bounds.get(&(owner, target.clone())) {
            let old = snapshot_bound(old, self.limits, self.left, depth + 1)?;
            let current = snapshot_bound(&value, self.limits, self.left, depth + 1)?;
            if old != current {
                return Err(pending("original projected loan has several unmerged operation sources"));
            }
        }
        self.proof
            .proof_mut()?
            .view_bounds
            .insert((owner, target), value);
        Ok(())
    }
    /// `&x` borrows x's slot when x itself is an address/descriptor. Its
    /// referent lifetime is an inner class and cannot bound the local slot.
    fn address_slot_bound(
        &mut self,
        owner: SemanticDefinitionId,
        mut place: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<Option<ViewBorrowBound>> {
        loop {
            self.pay(1, depth)?;
            match &place.kind {
                CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } => {
                    place = inner
                }
                _ => break,
            }
        }
        let CoreExprKind::Name(name) = &place.kind else {
            return Ok(None);
        };
        let Some(binding) = variables.get(name) else {
            return Ok(None);
        };
        if !matches!(
            binding.ty.as_ref(),
            SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. }
        ) {
            return Ok(None);
        }
        self.pay(binding.origin.copy_work().saturating_add(name.len()), depth)?;
        let (root, kind) = if binding.origin_owner != owner {
            if !binding.capture {
                return Ok(None);
            }
            (
                ViewBorrowRoot::Capture(place.id),
                ViewBorrowStepKind::Capture { name: name.clone() },
            )
        } else {
            match &binding.origin {
                Query::Parameter(parameter) => (
                    ViewBorrowRoot::Parameter(*parameter),
                    ViewBorrowStepKind::ParameterStorage,
                ),
                Query::Binding(binding) => (
                    ViewBorrowRoot::Local(*binding),
                    ViewBorrowStepKind::Local { binding: *binding },
                ),
                Query::BindingProjection { statement, pattern } => (
                    ViewBorrowRoot::Local(*statement),
                    ViewBorrowStepKind::ProjectedLocal {
                        statement: *statement,
                        pattern: pattern.clone(),
                        read: false,
                    },
                ),
                _ => return Ok(None),
            }
        };
        Ok(Some(ViewBorrowBound {
            root,
            lifetime: None,
            steps: vec![ViewBorrowStep {
                node: place.id,
                kind,
            }],
        }))
    }
    /// A resource view carries the loan from the selected manager. Preserve
    /// the intrinsic's original result and prove each contextual shortening
    /// against the input place before recording its use at this occurrence.
    pub(super) fn contextual_resource_view(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        manager: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        result: SemanticTypeRecipe,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let Some(wanted) = wanted else { return Ok(result); };
        if let (
            SemanticTypeRecipe::Address { mutable, lifetime: source, inner },
            SemanticTypeRecipe::Address { mutable: requested, lifetime: target, inner: expected },
        ) = (&result, wanted) {
            self.same(inner, expected)?;
            if mutable != requested {
                return Err(bad("resource view changes the requested access mode"));
            }
            if matches!(target, SemanticOuterLifeRecipe::Class(
                SemanticLifeRecipe::Implicit { scope: SemanticScopeRef::Ancestor(u32::MAX), .. }
                | SemanticLifeRecipe::Environment { scope: SemanticScopeRef::Ancestor(u32::MAX), .. }
            )) {
                return Ok(result);
            }
            if source != target {
                self.place_borrow_bound(owner, manager, variables, depth + 1)?;
                self.view_bound(owner, expression, variables, depth + 1)?;
                let lifetime = self.contextual_borrow_role(
                    owner, expression, expression, variables, inner, source, target, depth + 1,
                )?;
                return Ok(SemanticTypeRecipe::Address {
                    mutable: *mutable,
                    lifetime,
                    inner: Box::new(copy_recipe(inner, self.limits, self.left)?),
                });
            }
        }
        self.same(&result, wanted)?;
        Ok(result)
    }
    fn call_view_bound(
        &mut self, owner: SemanticDefinitionId, expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>, depth: usize,
    ) -> R<Option<ViewBorrowBound>> {
        let CoreExprKind::Call { callee, arguments } = &expression.kind else { return Ok(None); };
        let Some(site) = self.site(owner, SemanticOriginalNode::Expression(expression.id))? else { return Ok(None); };
        let site = site.id;
        self.pay(self.proof.intrinsics.len().saturating_add(1).ilog2() as usize + 1, depth)?;
        if let Some(IntrinsicCall { kind: IntrinsicKind::BytesBorrow { mutable }, .. }) =
            self.proof.intrinsics.get(&(owner, site))
        {
            let mutable = *mutable;
            let [argument] = arguments.as_slice() else { return Err(bad("byte view changes its exact original input arity")); };
            self.place_borrow_bound(owner, argument, variables, depth + 1)?;
            let Some(bound) = self.proof.view_bounds.get(&(owner, Query::Expression(argument.id))) else { return Ok(None); };
            let mut bound = snapshot_bound(bound, self.limits, self.left, depth + 1)?;
            self.pay(1, depth)?;
            bound.steps.push(ViewBorrowStep { node: expression.id,
                kind: ViewBorrowStepKind::BytesBorrow { site, mutable } });
            return Ok(Some(bound));
        }
        let Some(ty) = self.proof.types.get(&(owner, Query::Expression(expression.id))) else { return Ok(None); };
        let lifetime = match ty {
            SemanticTypeRecipe::Address { lifetime, .. } | SemanticTypeRecipe::Desc { lifetime, .. } => {
                crate::dyn_metadata::semantic_operand_snapshot(lifetime, self.limits, self.left, 1)?;
                lifetime.clone()
            }
            _ => return Ok(None),
        };
        self.pay(self.proof.callable_calls.len().saturating_add(1).ilog2() as usize + 1, depth)?;
        if let Some(call) = self.proof.callable_calls.get(&(owner, site))
            && matches!(&call.signature.result, SemanticTypeRecipe::Address { lifetime, .. } if is_receiver_upper(lifetime))
        {
            let source = call.callee_node;
            self.pay(self.proof.local_roles.len().saturating_add(1).ilog2() as usize + 1, depth)?;
            let access_lifetime = if let Some(role) = self.proof.local_roles.get(&(owner, expression.id)) {
                let [source] = role.sources.as_slice() else { return Err(bad("receiver call has no unique original access bound")); };
                crate::dyn_metadata::semantic_operand_snapshot(&source.lifetime, self.limits, self.left, 1)?;
                source.lifetime.clone()
            } else { return Err(bad("receiver call has no original access bound")); };
            let mut base = callee.as_ref();
            while let CoreExprKind::Group(inner) | CoreExprKind::TypeApply { callee: inner, .. }
                | CoreExprKind::GenericApply { callee: inner, .. } = &base.kind
            { self.pay(1, depth)?; base = inner; }
            if base.id != source { return Err(bad("receiver call changes its original callee source node")); }
            self.place_borrow_bound(owner, base, variables, depth + 1)?;
            self.pay(1, depth)?;
            return Ok(Some(ViewBorrowBound { root: ViewBorrowRoot::Result(expression.id),
                lifetime: Some(access_lifetime), steps: vec![ViewBorrowStep { node: expression.id,
                    kind: ViewBorrowStepKind::CallableReceiver { site, source } }] }));
        }
        self.pay(self.proof.call_targets.len().saturating_add(1).ilog2() as usize + 1, depth)?;
        let Some(target) = self.proof.call_targets.get(&(owner, site)).copied() else { return Ok(None); };
        let Some(parameters) = self.proof.returned_input_parameters(self.program, target, self.limits, self.left)? else { return Ok(None); };
        let mut inputs = Vec::new();
        for parameter in parameters {
            let argument = arguments.get(parameter as usize)
                .ok_or_else(|| bad("returned input leaves its exact original Call arguments"))?;
            self.place_borrow_bound(owner, argument, variables, depth + 1)?;
            self.pay(self.proof.view_bounds.len().saturating_add(1).ilog2() as usize + 3, depth)?;
            if !self.proof.view_bounds.contains_key(&(owner, Query::Expression(argument.id))) { return Ok(None); }
            inputs.push((parameter, argument.id));
        }
        self.pay(1, depth)?;
        Ok(Some(ViewBorrowBound { root: ViewBorrowRoot::Result(expression.id),
            lifetime: Some(lifetime), steps: vec![ViewBorrowStep { node: expression.id,
                kind: ViewBorrowStepKind::CallInputs { site, inputs } }] }))
    }
    pub(super) fn view_bound(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        let mut step = None;
        let source = match &expression.kind {
            CoreExprKind::Name(name) => {
                let Some(binding) = variables.get(name) else {
                    return Ok(());
                };
                if binding.origin_owner == owner {
                    self.pay(binding.origin.copy_work().saturating_add(1), depth)?;
                    let source = match &binding.origin {
                        Query::Binding(statement) => {
                            Some(SemanticOriginalTypeSource::Binding(*statement))
                        }
                        Query::BindingProjection { statement, pattern } => {
                            Some(SemanticOriginalTypeSource::BindingProjection {
                                statement: *statement,
                                pattern: pattern.clone(),
                            })
                        }
                        _ => None,
                    };
                    if let Some(source) = source {
                        self.pay(
                            self.proof.borrow_bindings.len().saturating_add(1).ilog2() as usize + 1,
                            depth,
                        )?;
                        match self
                            .proof
                            .proof_mut()?
                            .borrow_bindings
                            .entry((owner, expression.id))
                        {
                            std::collections::btree_map::Entry::Occupied(row)
                                if row.get() != &source =>
                            {
                                return Err(bad(
                                    "one original loan read changes its lexical binding identity",
                                ));
                            }
                            std::collections::btree_map::Entry::Occupied(_) => {}
                            std::collections::btree_map::Entry::Vacant(row) => {
                                row.insert(source);
                            }
                        }
                    }
                }
                if binding.origin_owner != owner {
                    if !binding.capture {
                        return Ok(());
                    }
                    let lifetime = match binding.ty.as_ref() {
                        SemanticTypeRecipe::Address { lifetime, .. }
                        | SemanticTypeRecipe::Desc { lifetime, .. } => Some(lifetime.clone()),
                        _ => None,
                    };
                    if let Some(lifetime) = &lifetime {
                        crate::dyn_metadata::semantic_operand_snapshot(
                            lifetime,
                            self.limits,
                            self.left,
                            1,
                        )?;
                    }
                    self.pay(name.len().saturating_add(1), depth)?;
                    return self.insert_view_bound(
                        owner,
                        Query::Expression(expression.id),
                        ViewBorrowBound {
                            root: ViewBorrowRoot::Capture(expression.id),
                            lifetime,
                            steps: vec![ViewBorrowStep {
                                node: expression.id,
                                kind: ViewBorrowStepKind::Capture { name: name.clone() },
                            }],
                        },
                        depth + 1,
                    );
                }
                if let Query::Parameter(parameter) = binding.origin {
                    if binding.mutable || binding.written {
                        return Ok(());
                    }
                    let Some(ty) = self.proof.types.get(&(owner, Query::Parameter(parameter)))
                    else {
                        return Ok(());
                    };
                    let lifetime = match ty {
                        SemanticTypeRecipe::Address { lifetime, .. }
                        | SemanticTypeRecipe::Desc { lifetime, .. } => lifetime,
                        _ => return Ok(()),
                    };
                    crate::dyn_metadata::semantic_operand_snapshot(
                        lifetime,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    let lifetime = lifetime.clone();
                    self.pay(2, depth)?;
                    return self.insert_view_bound(
                        owner,
                        Query::Expression(expression.id),
                        ViewBorrowBound {
                            root: ViewBorrowRoot::Parameter(parameter),
                            lifetime: Some(lifetime),
                            steps: vec![ViewBorrowStep {
                                node: expression.id,
                                kind: ViewBorrowStepKind::Parameter,
                            }],
                        },
                        depth + 1,
                    );
                }
                if let Query::BindingProjection { statement, pattern } = &binding.origin {
                    let lifetime = match binding.ty.as_ref() {
                        SemanticTypeRecipe::Address { lifetime, .. }
                        | SemanticTypeRecipe::Desc { lifetime, .. } => lifetime,
                        _ => return Ok(()),
                    };
                    crate::dyn_metadata::semantic_operand_snapshot(
                        lifetime,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    self.pay(pattern.len().saturating_add(1), depth)?;
                    return self.insert_view_bound(
                        owner,
                        Query::Expression(expression.id),
                        ViewBorrowBound {
                            root: ViewBorrowRoot::Local(*statement),
                            lifetime: Some(lifetime.clone()),
                            steps: vec![ViewBorrowStep {
                                node: expression.id,
                                kind: ViewBorrowStepKind::ProjectedLocal {
                                    statement: *statement,
                                    pattern: pattern.clone(),
                                    read: true,
                                },
                            }],
                        },
                        depth + 1,
                    );
                }
                let Query::Binding(binding_node) = binding.origin else {
                    return Ok(());
                };
                if !matches!(
                    binding.ty.as_ref(),
                    SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. }
                ) {
                    return Ok(());
                }
                if binding.mutable || binding.written {
                    let lifetime = match binding.ty.as_ref() {
                        SemanticTypeRecipe::Address { lifetime, .. }
                        | SemanticTypeRecipe::Desc { lifetime, .. } => lifetime,
                        _ => return Ok(()),
                    };
                    crate::dyn_metadata::semantic_operand_snapshot(
                        lifetime,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    return self.insert_view_bound(
                        owner,
                        Query::Expression(expression.id),
                        ViewBorrowBound {
                            root: ViewBorrowRoot::Local(binding_node),
                            lifetime: Some(lifetime.clone()),
                            steps: vec![ViewBorrowStep {
                                node: expression.id,
                                kind: ViewBorrowStepKind::ReadLocal {
                                    binding: binding_node,
                                },
                            }],
                        },
                        depth + 1,
                    );
                }
                step = Some(ViewBorrowStepKind::Alias {
                    binding: binding_node,
                });
                Query::Binding(binding_node)
            }
            CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                Query::Expression(value.id)
            }
            CoreExprKind::Member { object, member, .. } => {
                self.place_borrow_bound(owner, object, variables, depth + 1)?;
                let Some(base) = self.proof.types.get(&(owner, Query::Expression(object.id)))
                else {
                    return Ok(());
                };
                let base = copy_recipe(base, self.limits, self.left)?;
                let mut base = self.observation_type(owner, &base, depth + 1)?;
                loop {
                    self.pay(1, depth)?;
                    match base {
                        SemanticTypeRecipe::Address { inner, .. }
                        | SemanticTypeRecipe::Desc { inner, .. }
                        | SemanticTypeRecipe::Managed(inner) => base = *inner,
                        _ => break,
                    }
                }
                let SemanticTypeRecipe::Nominal { definition, .. } = base else {
                    return Ok(());
                };
                let declaration = self.definition(definition)?;
                let SemanticDefinitionBody::Nominal {
                    body: SemanticNominalBody::Product { fields },
                    ..
                } = &declaration.body
                else {
                    return Ok(());
                };
                self.pay(fields.len().saturating_add(member.len()), depth)?;
                let Some(ordinal) = fields.iter().position(|field| &field.name == member) else {
                    return Ok(());
                };
                step = Some(ViewBorrowStepKind::Field {
                    definition,
                    ordinal: ordinal as u32,
                });
                Query::Expression(object.id)
            }
            CoreExprKind::Call { arguments, .. } => {
                if let Some(bound) = self.call_view_bound(owner, expression, variables, depth + 1)? {
                    return self.insert_view_bound(owner, Query::Expression(expression.id), bound, depth + 1);
                }
                let Some(site) =
                    self.site(owner, SemanticOriginalNode::Expression(expression.id))?
                else {
                    return Ok(());
                };
                let Some(IntrinsicCall {
                    kind: IntrinsicKind::StoreBorrow { mutable },
                    ..
                }) = self.proof.intrinsics.get(&(owner, site.id))
                else {
                    let site = site.id;
                    let checked = self.proof.call_targets.contains_key(&(owner, site))
                        || self.proof.callable_calls.contains_key(&(owner, site))
                        || self.proof.intrinsics.contains_key(&(owner, site));
                    if !checked {
                        return Ok(());
                    }
                    let Some(ty) = self
                        .proof
                        .types
                        .get(&(owner, Query::Expression(expression.id)))
                    else {
                        return Ok(());
                    };
                    let mut lifetime = match ty {
                        SemanticTypeRecipe::Address { lifetime, .. }
                        | SemanticTypeRecipe::Desc { lifetime, .. } => lifetime,
                        _ => return Ok(()),
                    };
                    if self.proof.callable_calls.get(&(owner, site)).is_some_and(|call| {
                        matches!(&call.signature.result, SemanticTypeRecipe::Address { lifetime, .. }
                            if is_receiver_upper(lifetime))
                    }) && let Some(role) = self.proof.local_roles.get(&(owner, expression.id))
                    {
                        if role.sources.len() != 1 {
                            return Err(bad("receiver call has no unique original access source"));
                        }
                        lifetime = &role.sources[0].lifetime;
                    }
                    crate::dyn_metadata::semantic_operand_snapshot(
                        lifetime,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    return self.insert_view_bound(
                        owner,
                        Query::Expression(expression.id),
                        ViewBorrowBound {
                            root: ViewBorrowRoot::Result(expression.id),
                            lifetime: Some(lifetime.clone()),
                            steps: vec![ViewBorrowStep {
                                node: expression.id,
                                kind: ViewBorrowStepKind::CallResult { site },
                            }],
                        },
                        depth + 1,
                    );
                };
                let Some(argument) = arguments.first() else {
                    return Err(bad("projected Store loan omits its real descriptor"));
                };
                let site = site.id;
                let mutable = *mutable;
                self.place_borrow_bound(owner, argument, variables, depth + 1)?;
                step = Some(ViewBorrowStepKind::StoreBorrow { site, mutable });
                Query::Expression(argument.id)
            }
            CoreExprKind::Slice { object, .. } => {
                self.place_borrow_bound(owner, object, variables, depth + 1)?;
                step = Some(ViewBorrowStepKind::Slice);
                Query::Expression(object.id)
            }
            CoreExprKind::Index { object, index } => {
                self.place_borrow_bound(owner, object, variables, depth + 1)?;
                let literal = match &index.kind {
                    CoreExprKind::Integer(text) => {
                        self.pay(text.len(), depth)?;
                        Some(integer(text, Some(&Type::I64), false, index.span)?)
                    }
                    CoreExprKind::Literal(value) => Some(value.clone()),
                    _ => None,
                };
                let constant = literal.and_then(|value| match value {
                    Constant::I8(v) => u32::try_from(v).ok(),
                    Constant::I16(v) => u32::try_from(v).ok(),
                    Constant::I32(v) => u32::try_from(v).ok(),
                    Constant::I64(v) | Constant::Isize(v) => u32::try_from(v).ok(),
                    Constant::U8(v) => Some(v as u32),
                    Constant::U16(v) => Some(v as u32),
                    Constant::U32(v) => Some(v),
                    _ => None,
                });
                step = Some(ViewBorrowStepKind::Index { constant });
                Query::Expression(object.id)
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                self.place_borrow_bound(owner, operand, variables, depth + 1)?;
                step = Some(ViewBorrowStepKind::Dereference);
                Query::Expression(operand.id)
            }
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                operand,
            } => {
                if let Some(bound) =
                    self.address_slot_bound(owner, operand, variables, depth + 1)?
                {
                    return self.insert_view_bound(
                        owner,
                        Query::Expression(expression.id),
                        bound,
                        depth + 1,
                    );
                }
                self.place_borrow_bound(owner, operand, variables, depth + 1)?;
                step = Some(ViewBorrowStepKind::Borrow);
                Query::Expression(operand.id)
            }
            _ => return Ok(()),
        };
        let Some(value) = self.proof.view_bounds.get(&(owner, source)) else {
            return Ok(());
        };
        let mut value = snapshot_bound(value, self.limits, self.left, depth + 1)?;
        if let Some(kind) = step {
            self.pay(1, depth)?;
            value.steps.push(ViewBorrowStep {
                node: expression.id,
                kind,
            });
        }
        self.insert_view_bound(owner, Query::Expression(expression.id), value, depth + 1)
    }
    pub(super) fn contextual_borrow_role(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        operand: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        inner: &SemanticTypeRecipe,
        occurrence: &SemanticOuterLifeRecipe,
        wanted: &SemanticOuterLifeRecipe,
        depth: usize,
    ) -> R<SemanticOuterLifeRecipe> {
        if let Some(role) = self.projected_borrow_role(
            owner, expression, operand, variables, inner, occurrence, wanted, true, true, depth,
        )? {
            self.pay(1, depth)?;
            self.proof
                .proof_mut()?
                .borrow_roles
                .insert((owner, expression.id), role);
        }
        Ok(wanted.clone())
    }

    /// A result view is a boundary loan, not a new type for its source
    /// expression. In particular a retained field may be viewed for the
    /// receiver loan while its original Fixed class remains unchanged.
    pub(super) fn return_view(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        actual: &SemanticTypeRecipe,
        expected: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        let (
            SemanticTypeRecipe::Address {
                mutable: false,
                lifetime: source,
                inner,
            },
            SemanticTypeRecipe::Address {
                mutable: false,
                lifetime: target,
                inner: target_inner,
            },
        ) = (actual, expected)
        else {
            return Ok(false);
        };
        if source == target || matches!(target, SemanticOuterLifeRecipe::Static) {
            return Ok(false);
        }
        self.same(inner, target_inner)?;
        if is_receiver_upper(target) {
            self.pay(self.proof.return_borrows.len().saturating_add(1).ilog2() as usize + 1, depth)?;
            if let Some(role) = self.proof.return_borrows.get(&(owner, expression.id)) {
                let role = snapshot_role(role, self.limits, self.left)?;
                crate::dyn_metadata::semantic_operand_snapshot(source, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(inner.as_ref(), self.limits, self.left, 1)?;
                let sources = self.proof.view_sources(owner, role.root, &role.steps, self.limits, self.left,
                    &mut BTreeSet::new(), depth + 1)?;
                if sources.is_some_and(|roots| !roots.is_empty() && roots.iter().all(|root| matches!(root, ViewBorrowRoot::Capture(_))))
                    && is_receiver_upper(&role.target_lifetime)
                    && &role.occurrence_lifetime == source && role.inner == **inner
                { return Ok(true); }
            }
        }
        let Some(role) = self.projected_borrow_role(
            owner,
            expression,
            expression,
            variables,
            inner,
            source,
            target,
            false,
            false,
            depth + 1,
        )?
        else {
            return Ok(false);
        };
        self.pay(
            self.proof.return_borrows.len().saturating_add(1).ilog2() as usize + 1,
            depth,
        )?;
        let rows = &mut self.proof.proof_mut()?.return_borrows;
        match rows.entry((owner, expression.id)) {
            std::collections::btree_map::Entry::Occupied(old) => {
                let previous = snapshot_role(old.get(), self.limits, self.left)?;
                let current = snapshot_role(&role, self.limits, self.left)?;
                if previous != current {
                    return Err(bad("one original result view changes its complete source loan"));
                }
            }
            std::collections::btree_map::Entry::Vacant(row) => {
                row.insert(role);
            }
        }
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn projected_borrow_role(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        operand: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        inner: &SemanticTypeRecipe,
        occurrence: &SemanticOuterLifeRecipe,
        wanted: &SemanticOuterLifeRecipe,
        local_region: bool,
        extend_region: bool,
        depth: usize,
    ) -> R<Option<BorrowRole>> {
        if wanted == occurrence {
            return Ok(None);
        }
        let slot = if matches!(&expression.kind, CoreExprKind::Unary { op: UnaryOp::Borrow | UnaryOp::BorrowMut, operand: original } if original.id == operand.id)
        {
            self.address_slot_bound(owner, operand, variables, depth + 1)?
        } else {
            None
        };
        let bound = if let Some(slot) = slot {
            slot
        } else {
            if !self
                .proof
                .view_bounds
                .contains_key(&(owner, Query::Expression(operand.id)))
            {
                self.place_borrow_bound(owner, operand, variables, depth + 1)?;
            }
            let Some(bound) = self
                .proof
                .view_bounds
                .get(&(owner, Query::Expression(operand.id)))
            else {
                if !local_region {
                    return Ok(None);
                }
                return Err(bad(
                    "original contextual borrow has no proved projected input loan",
                ));
            };
            snapshot_bound(bound, self.limits, self.left, depth + 1)?
        };
        let region = self.region_key(owner, wanted, depth + 1)?;
        if extend_region && region.is_some_and(|(region_owner, _)| region_owner == owner) {
            self.record_region_source(
                owner,
                wanted,
                expression,
                bound.lifetime.as_ref().unwrap_or(occurrence),
                variables,
                depth + 1,
            )?;
        }
        let existing_local_role = if local_region && bound.lifetime.is_none() {
            self.pay(self.proof.local_roles.len(), depth)?;
            self.proof.local_roles.iter().any(|((role_owner, _), role)| {
                *role_owner == owner
                    && matches!(wanted, SemanticOuterLifeRecipe::Class(life) if life == &role.lifetime)
            })
        } else {
            false
        };
        let allowed = match (&bound.lifetime, wanted) {
            (
                None,
                SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
                    scope: SemanticScopeRef::Ancestor(0),
                    ..
                }),
            ) if local_region
                // A fresh direct annotation retains its existing lexical
                // place proof. An already-declared local role cannot acquire
                // an unrecorded indirect write through its old source bounds.
                && (!existing_local_role
                    || region.is_some_and(|(region_owner, _)| region_owner == owner))
                && matches!(
                    bound.root,
                    ViewBorrowRoot::Local(_)
                        | ViewBorrowRoot::Capture(_)
                        | ViewBorrowRoot::Parameter(_)
                ) =>
            {
                true
            }
            (Some(SemanticOuterLifeRecipe::Static), _) => true,
            (
                Some(SemanticOuterLifeRecipe::Class(source)),
                SemanticOuterLifeRecipe::Class(target),
            ) => {
                let current_write=if local_source_copy(bound.root,&bound.steps) && !self.proof.return_writers.is_empty() {
                    self.pay(self.proof.return_writers.len().ilog2() as usize+2,depth)?;
                    self.proof.return_writers.get(&(owner,expression.id)).is_some_and(|(_,life)|life==wanted)
                } else {false};
                let returned_local = match (bound.root,target) {
                    (ViewBorrowRoot::Local(binding),SemanticLifeRecipe::Implicit {
                        scope: SemanticScopeRef::Ancestor(0), ordinal
                    }) if local_source_copy(bound.root,&bound.steps) => {
                        self.pay(self.proof.local_roles.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                        self.proof.local_roles.get(&(owner,binding)).is_some_and(|role|
                            role.result_upper == Some(*ordinal) && &role.lifetime == source)
                    }
                    _ => false,
                };
                current_write || returned_local || self.life_outlives(owner, source, target, depth + 1)?
            }
            _ => false,
        };
        if !allowed {
            if !local_region {
                return Ok(None);
            }
            return Err(bad(
                "original projected borrow exceeds its complete source lifetime bound",
            ));
        }
        crate::dyn_metadata::semantic_operand_snapshot(occurrence, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(wanted, self.limits, self.left, 2)?;
        let inner = copy_recipe(inner, self.limits, self.left)?;
        Ok(Some(BorrowRole {
            root: bound.root,
            source_lifetime: bound.lifetime.unwrap_or_else(|| occurrence.clone()),
            target_lifetime: wanted.clone(),
            occurrence_lifetime: occurrence.clone(),
            inner,
            steps: bound.steps,
        }))
    }
    pub(super) fn place_borrow_bound(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        match &expression.kind {
            CoreExprKind::Name(name) => {
                self.view_bound(owner, expression, variables, depth + 1)?;
                if self
                    .proof
                    .view_bounds
                    .contains_key(&(owner, Query::Expression(expression.id)))
                {
                    return Ok(());
                }
                let Some(binding) = variables.get(name) else {
                    return Ok(());
                };
                if let Query::Parameter(parameter) = binding.origin {
                    if binding.origin_owner != owner
                        || matches!(
                            binding.ty.as_ref(),
                            SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. }
                        )
                    {
                        return Ok(());
                    }
                    return self.insert_view_bound(
                        owner,
                        Query::Expression(expression.id),
                        ViewBorrowBound {
                            root: ViewBorrowRoot::Parameter(parameter),
                            lifetime: None,
                            steps: vec![ViewBorrowStep {
                                node: expression.id,
                                kind: ViewBorrowStepKind::ParameterStorage,
                            }],
                        },
                        depth + 1,
                    );
                }
                if let Query::BindingProjection { statement, pattern } = &binding.origin {
                    if binding.origin_owner != owner
                        || matches!(
                            binding.ty.as_ref(),
                            SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. }
                        )
                    {
                        return Ok(());
                    }
                    self.pay(pattern.len().saturating_add(1), depth)?;
                    return self.insert_view_bound(
                        owner,
                        Query::Expression(expression.id),
                        ViewBorrowBound {
                            root: ViewBorrowRoot::Local(*statement),
                            lifetime: None,
                            steps: vec![ViewBorrowStep {
                                node: expression.id,
                                kind: ViewBorrowStepKind::ProjectedLocal {
                                    statement: *statement,
                                    pattern: pattern.clone(),
                                    read: false,
                                },
                            }],
                        },
                        depth + 1,
                    );
                }
                let Query::Binding(node) = binding.origin else {
                    return Ok(());
                };
                if binding.origin_owner != owner
                    || matches!(
                        binding.ty.as_ref(),
                        SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. }
                    )
                {
                    return Ok(());
                }
                // This is the storage explicitly borrowed at this operation;
                // observing or moving a whole value never creates such a loan.
                self.insert_view_bound(
                    owner,
                    Query::Expression(expression.id),
                    ViewBorrowBound {
                        root: ViewBorrowRoot::Local(node),
                        lifetime: None,
                        steps: vec![ViewBorrowStep {
                            node: expression.id,
                            kind: ViewBorrowStepKind::Local { binding: node },
                        }],
                    },
                    depth + 1,
                )
            }
            CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                self.place_borrow_bound(owner, value, variables, depth + 1)?;
                self.view_bound(owner, expression, variables, depth + 1)
            }
            // Each of these view rules establishes its operand's place bound
            // before appending its own step. Walk that operand once per level.
            CoreExprKind::Member { .. }
            | CoreExprKind::Slice { .. }
            | CoreExprKind::Index { .. }
            | CoreExprKind::Unary { op: UnaryOp::Dereference, .. }
            | CoreExprKind::Call { .. } => self.view_bound(owner, expression, variables, depth + 1),
            _ => Ok(()),
        }
    }
    /// Preparing the left operand does not create a new source lifetime.
    /// Carry the verified original place and bound into this argument use.
    pub(super) fn prepared_input_view_bound(
        &mut self,
        owner: SemanticDefinitionId,
        source: &SemanticOriginalExpr,
        argument: NodeId,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        self.place_borrow_bound(owner, source, variables, depth + 1)?;
        self.pay(self.proof.view_bounds.len().saturating_add(1).ilog2() as usize + 1, depth)?;
        let Some(bound) = self.proof.view_bounds.get(&(owner, Query::Expression(source.id))) else {
            return Ok(());
        };
        let mut bound = snapshot_bound(bound, self.limits, self.left, depth + 1)?;
        self.pay(1, depth)?;
        bound.steps.push(ViewBorrowStep { node: argument, kind: ViewBorrowStepKind::PreparedShared });
        self.insert_view_bound(owner, Query::Expression(argument), bound, depth + 1)
    }

    pub(super) fn life_outlives(
        &mut self,
        owner: SemanticDefinitionId,
        source: &SemanticLifeRecipe,
        target: &SemanticLifeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.call_region_outlives(owner, source, target, &mut Vec::new(), depth)
    }
}

/// Only a transparent local-pointer read/alias transports the same referent.
/// Group/ascription add no path step. Field/index/deref/borrow and call edges
/// must form their own source receipt after the complete actual trace.
pub(in crate::semantics) fn local_source_copy(
    root: ViewBorrowRoot, steps: &[ViewBorrowStep],
) -> bool {
    let ViewBorrowRoot::Local(binding) = root else { return false; };
    let mut read = false;
    for step in steps {
        match &step.kind {
            ViewBorrowStepKind::ReadLocal { binding: selected } if *selected == binding => read = true,
            ViewBorrowStepKind::Alias { .. } => {}
            ViewBorrowStepKind::ReadLocal { .. }
            | ViewBorrowStepKind::Parameter | ViewBorrowStepKind::ParameterStorage
            | ViewBorrowStepKind::Slice | ViewBorrowStepKind::Index { .. }
            | ViewBorrowStepKind::Dereference | ViewBorrowStepKind::CallResult { .. }
            | ViewBorrowStepKind::CallInputs { .. } | ViewBorrowStepKind::CallableReceiver { .. }
            | ViewBorrowStepKind::BytesBorrow { .. } | ViewBorrowStepKind::Capture { .. }
            | ViewBorrowStepKind::ProjectedLocal { .. } | ViewBorrowStepKind::Local { .. }
            | ViewBorrowStepKind::Field { .. } | ViewBorrowStepKind::StoreBorrow { .. }
            | ViewBorrowStepKind::Borrow
            | ViewBorrowStepKind::PreparedShared => return false,
        }
    }
    read
}
