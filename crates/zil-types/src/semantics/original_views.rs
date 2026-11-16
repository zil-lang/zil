//! Directed projected-loan bounds. These retain the original input and each
//! real operation; no edge establishes equality of lifetime classes.
use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum ViewBorrowStepKind {
    Parameter,
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
#[derive(Clone)]
pub(crate) struct BorrowRole {
    pub root: ViewBorrowRoot,
    pub source_lifetime: SemanticOuterLifeRecipe,
    pub target_lifetime: SemanticOuterLifeRecipe,
    pub occurrence_lifetime: SemanticOuterLifeRecipe,
    pub inner: SemanticTypeRecipe,
    pub steps: Vec<ViewBorrowStep>,
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
    if let Some(lifetime) = &value.lifetime {
        crate::dyn_metadata::semantic_operand_snapshot(lifetime, limits, left, 1)?;
    }
    Ok(value.clone())
}
impl Check<'_, '_> {
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
                if binding.origin_owner != owner {
                    return Ok(());
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
                    return Ok(());
                };
                let Some(argument) = arguments.first() else {
                    return Err(bad("projected Store loan omits its real descriptor"));
                };
                step = Some(ViewBorrowStepKind::StoreBorrow {
                    site: site.id,
                    mutable: *mutable,
                });
                Query::Expression(argument.id)
            }
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                operand,
            } => {
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
        if wanted == occurrence {
            return Ok(wanted.clone());
        }
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
            return Err(bad(
                "original contextual borrow has no proved projected input loan",
            ));
        };
        let bound = snapshot_bound(bound, self.limits, self.left, depth + 1)?;
        let region = self.region_key(owner, wanted, depth + 1)?;
        if region.is_some_and(|(region_owner, _)| region_owner == owner) {
            self.record_region_source(
                owner,
                wanted,
                expression,
                bound.lifetime.as_ref().unwrap_or(occurrence),
                variables,
                depth + 1,
            )?;
        }
        let allowed = match (&bound.lifetime, wanted) {
            (
                None,
                SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
                    scope: SemanticScopeRef::Ancestor(0),
                    ..
                }),
            ) if matches!(bound.root, ViewBorrowRoot::Local(_)) => true,
            (Some(SemanticOuterLifeRecipe::Static), _) => true,
            (
                Some(SemanticOuterLifeRecipe::Class(source)),
                SemanticOuterLifeRecipe::Class(target),
            ) => self.life_outlives(owner, source, target, depth + 1)?,
            _ => false,
        };
        if !allowed {
            return Err(bad(
                "original projected borrow exceeds its complete source lifetime bound",
            ));
        }
        crate::dyn_metadata::semantic_operand_snapshot(occurrence, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(wanted, self.limits, self.left, 2)?;
        let inner = copy_recipe(inner, self.limits, self.left)?;
        self.pay(1, depth)?;
        self.proof.proof_mut()?.borrow_roles.insert(
            (owner, expression.id),
            BorrowRole {
                root: bound.root,
                source_lifetime: bound.lifetime.unwrap_or_else(|| occurrence.clone()),
                target_lifetime: wanted.clone(),
                occurrence_lifetime: occurrence.clone(),
                inner,
                steps: bound.steps,
            },
        );
        Ok(wanted.clone())
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
            CoreExprKind::Member { object, .. } => {
                self.place_borrow_bound(owner, object, variables, depth + 1)?;
                self.view_bound(owner, expression, variables, depth + 1)
            }
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
