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
            _ => self.borrow_life(owner, call.id)?,
        };
        let bound = self.borrow_life(owner, call.id)?;
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
            };
            self.pay(self.proof.local_roles.len().saturating_add(1).ilog2() as usize + 3, depth)?;
            if let Some(previous) = self.proof.local_roles.get(&(owner, call.id)) {
                crate::dyn_metadata::semantic_operand_snapshot(&previous.ty, self.limits, self.left, 1)?;
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
        if !matches!(bound.root, ViewBorrowRoot::Capture(_)) {
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
            if old != &role { return Err(bad("receiver result changes its exact original view")); }
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
        if self
            .proof
            .view_bounds
            .get(&(owner, target.clone()))
            .is_some_and(|old| old != &value)
        {
            return Err(pending(
                "original projected loan has several unmerged operation sources",
            ));
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
            binding.ty,
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
                    let lifetime = match &binding.ty {
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
                    let lifetime = match &binding.ty {
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
                    binding.ty,
                    SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. }
                ) {
                    return Ok(());
                }
                if binding.mutable || binding.written {
                    let lifetime = match &binding.ty {
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
            owner, expression, operand, variables, inner, occurrence, wanted, true, depth,
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
        let Some(role) = self.projected_borrow_role(
            owner,
            expression,
            expression,
            variables,
            inner,
            source,
            target,
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
            std::collections::btree_map::Entry::Occupied(old) if old.get() != &role => {
                return Err(bad(
                    "one original result view changes its complete source loan",
                ));
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
            std::collections::btree_map::Entry::Vacant(row) => {
                row.insert(role);
            }
        }
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    fn projected_borrow_role(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        operand: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        inner: &SemanticTypeRecipe,
        occurrence: &SemanticOuterLifeRecipe,
        wanted: &SemanticOuterLifeRecipe,
        local_region: bool,
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
        if local_region && region.is_some_and(|(region_owner, _)| region_owner == owner) {
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
            ) => self.life_outlives(owner, source, target, depth + 1)?,
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
                            binding.ty,
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
                            binding.ty,
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
                        binding.ty,
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
            CoreExprKind::Member { object, .. }
            | CoreExprKind::Slice { object, .. }
            | CoreExprKind::Index { object, .. } => {
                self.place_borrow_bound(owner, object, variables, depth + 1)?;
                self.view_bound(owner, expression, variables, depth + 1)
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                self.place_borrow_bound(owner, operand, variables, depth + 1)?;
                self.view_bound(owner, expression, variables, depth + 1)
            }
            CoreExprKind::Call { .. } => self.view_bound(owner, expression, variables, depth + 1),
            _ => Ok(()),
        }
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
