//! Directed projected-loan bounds. These retain the original input and each
//! real operation; no edge establishes equality of lifetime classes.
use super::*;

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum ViewBorrowStepKind {
    Parameter,
    Alias {
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
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ViewBorrowStep {
    pub node: NodeId,
    pub kind: ViewBorrowStepKind,
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct ViewBorrowBound {
    pub parameter: u32,
    pub lifetime: SemanticOuterLifeRecipe,
    pub steps: Vec<ViewBorrowStep>,
}
#[derive(Clone)]
pub(crate) struct BorrowRole {
    pub parameter: u32,
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
    crate::dyn_metadata::semantic_operand_snapshot(&value.lifetime, limits, left, 1)?;
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
        self.proof.view_bounds.insert((owner, target), value);
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
                if binding.origin_owner != owner || binding.mutable || binding.written {
                    return Ok(());
                }
                if let Query::Parameter(parameter) = binding.origin {
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
                            parameter,
                            lifetime,
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
        inner: &SemanticTypeRecipe,
        occurrence: &SemanticOuterLifeRecipe,
        wanted: &SemanticOuterLifeRecipe,
        depth: usize,
    ) -> R<SemanticOuterLifeRecipe> {
        if wanted == occurrence {
            return Ok(wanted.clone());
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
        let allowed = match (&bound.lifetime, wanted) {
            (SemanticOuterLifeRecipe::Static, _) => true,
            (SemanticOuterLifeRecipe::Class(source), SemanticOuterLifeRecipe::Class(target)) => {
                self.life_outlives(owner, source, target, depth + 1)?
            }
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
        self.proof.borrow_roles.insert(
            (owner, expression.id),
            BorrowRole {
                parameter: bound.parameter,
                source_lifetime: bound.lifetime,
                target_lifetime: wanted.clone(),
                occurrence_lifetime: occurrence.clone(),
                inner,
                steps: bound.steps,
            },
        );
        Ok(wanted.clone())
    }
    fn life_outlives(
        &mut self,
        owner: SemanticDefinitionId,
        source: &SemanticLifeRecipe,
        target: &SemanticLifeRecipe,
        depth: usize,
    ) -> R<bool> {
        let requirements = self.original_requirements(owner, depth + 1)?;
        self.pay(
            requirements.len().saturating_mul(4).saturating_add(4),
            depth,
        )?;
        let mut frontier = vec![source];
        let mut seen = Vec::new();
        while let Some(current) = frontier.pop() {
            if self.same_life(current, target)? {
                return Ok(true);
            }
            self.pay(seen.len().saturating_add(1), depth)?;
            if seen.contains(&current) {
                continue;
            }
            seen.push(current);
            for requirement in &requirements {
                if let SemanticRequirementRecipe::Outlives { longer, shorter } = requirement {
                    if self.same_life(current, longer)? {
                        self.pay(1, depth)?;
                        frontier.push(shorter);
                    }
                }
            }
        }
        Ok(false)
    }
}
