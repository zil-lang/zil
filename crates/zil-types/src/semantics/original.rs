//! Symbolic facts obtained from the immutable original bodies. This module has
//! no TypedModule, ValueId, closure runtime ID or instance actuals as input.
use crate::*;
use std::collections::{BTreeMap, BTreeSet};
use zil_base::NodeId;
fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-ORIGINAL", Span::default(), message)
}
fn pending(message: &str) -> Diagnostic {
    Diagnostic::unsupported(Phase::Types, message, None)
}
fn copy_recipe(
    ty: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeRecipe> {
    crate::dyn_metadata::semantic_operand_snapshot(ty, limits, left, 1)?;
    Ok(ty.clone())
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Query {
    Expression(NodeId),
    Binding(NodeId),
    Parameter(u32),
    Result,
    CallConstCarrier {
        call: NodeId,
        formal: u32,
    },
    BindingProjection {
        statement: NodeId,
        pattern: Vec<u32>,
    },
    NameAt {
        expression: bool,
        node: NodeId,
        name: String,
    },
}
impl From<SemanticOriginalTypeSource> for Query {
    fn from(source: SemanticOriginalTypeSource) -> Self {
        match source {
            SemanticOriginalTypeSource::Expression(id) => Self::Expression(id),
            SemanticOriginalTypeSource::Binding(id) => Self::Binding(id),
            SemanticOriginalTypeSource::Parameter(i) => Self::Parameter(i),
            SemanticOriginalTypeSource::Result => Self::Result,
            SemanticOriginalTypeSource::CallConstCarrier { call, formal } => {
                Self::CallConstCarrier { call, formal }
            }
            SemanticOriginalTypeSource::BindingProjection { statement, pattern } => {
                Self::BindingProjection { statement, pattern }
            }
            SemanticOriginalTypeSource::NameAt { at, name } => {
                let (expression, node) = match at {
                    SemanticOriginalNode::Expression(node) => (true, node),
                    SemanticOriginalNode::Statement(node) => (false, node),
                };
                Self::NameAt {
                    expression,
                    node,
                    name,
                }
            }
        }
    }
}
#[derive(Default)]
pub(super) struct Proof {
    pub types: BTreeMap<(SemanticDefinitionId, Query), SemanticTypeRecipe>,
    pub calls: BTreeMap<(SemanticDefinitionId, SemanticSiteId), Vec<SemanticActualRecipe>>,
    pub complete: BTreeSet<SemanticDefinitionId>,
    pub kinds: BTreeMap<SemanticDefinitionId, CallableKind>,
    pub call_targets: BTreeMap<(SemanticDefinitionId, SemanticSiteId), SemanticDefinitionId>,
    environments: BTreeMap<(SemanticDefinitionId, u32), (SemanticDefinitionId, SemanticTypeRecipe)>,
}
#[derive(Clone)]
struct Binding {
    ty: SemanticTypeRecipe,
    mutable: bool,
    used: bool,
    observed: bool,
    capture: bool,
    borrowed: bool,
    origin: Query,
    initialized: bool,
    written: bool,
}
struct Check<'a, 'b> {
    program: &'a SemanticModule,
    limits: &'b Limits,
    left: &'b mut usize,
    proof: Proof,
    active: BTreeSet<SemanticDefinitionId>,
    captures: BTreeMap<SemanticDefinitionId, Vec<String>>,
}
impl<'a> Check<'a, '_> {
    fn pay(&mut self, n: usize, depth: usize) -> R<()> {
        *self.left = self.left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "original symbolic typing work exceeded", None)
        })?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original symbolic typing depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn definition(&mut self, id: SemanticDefinitionId) -> R<&'a SemanticDefinition> {
        self.pay(self.program.definitions.len(), 0)?;
        self.program
            .definitions
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| bad("symbolic type references an absent original definition"))
    }
    fn same(&mut self, a: &SemanticTypeRecipe, b: &SemanticTypeRecipe) -> R<()> {
        crate::dyn_metadata::semantic_operand_snapshot(a, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(b, self.limits, self.left, 1)?;
        if a != b {
            if let SemanticTypeRecipe::Opaque { hidden, .. } = b {
                return self.same(a, hidden);
            }
            return Err(bad(
                "original body changes a rigid whole type or lifetime anchor",
            ));
        }
        Ok(())
    }
    fn copy_type(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        Ok(match ty {
            SemanticTypeRecipe::Atom(ty) => ty.is_copy_value(),
            SemanticTypeRecipe::Address { mutable: false, .. } => true,
            SemanticTypeRecipe::Tuple(fields) => {
                let mut all = true;
                for field in fields {
                    all &= self.copy_type(owner, field, depth + 1)?;
                }
                all
            }
            SemanticTypeRecipe::Environment {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            } => {
                let (parent, ty) = self
                    .proof
                    .environments
                    .get(&(owner, *ordinal))
                    .ok_or_else(|| pending("sealed whole-Type Copy premise is not established"))?;
                let parent = *parent;
                let ty = copy_recipe(ty, self.limits, self.left)?;
                self.copy_type(parent, &ty, depth + 1)?
            }
            SemanticTypeRecipe::Formal(reference)
                if reference.scope == SemanticScopeRef::Ancestor(0) =>
            {
                let definition = self.definition(owner)?;
                let mut found = false;
                if let Some(SemanticFormal::Type { requirements }) =
                    definition.parameters.get(reference.ordinal as usize)
                {
                    for requirement in requirements {
                        self.pay(1, depth)?;
                        if matches!(requirement, SemanticRequirementRecipe::Structural { ty: actual, bound: DynStructuralBound::Copy } | SemanticRequirementRecipe::Builtin { ty: actual, trait_kind: SemanticBuiltinTrait::Copy } if actual == ty)
                        {
                            found = true;
                        }
                    }
                }
                for requirement in &definition.requirements {
                    self.pay(1, depth)?;
                    if matches!(requirement, SemanticRequirementRecipe::Structural { ty: actual, bound: DynStructuralBound::Copy } | SemanticRequirementRecipe::Builtin { ty: actual, trait_kind: SemanticBuiltinTrait::Copy } if actual == ty)
                    {
                        found = true;
                    }
                }
                found
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let definition = self.definition(*definition)?;
                match &definition.body {
                    SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Record,
                        body: SemanticNominalBody::Product { fields },
                        ..
                    } => {
                        let mut all = true;
                        for field in fields {
                            let field = super::instantiate::ty(
                                &field.ty,
                                arguments,
                                self.limits,
                                self.left,
                            )?;
                            all &= self.copy_type(owner, &field, depth + 1)?;
                        }
                        all
                    }
                    _ => false,
                }
            }
            _ => false,
        })
    }
    fn place(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        match &expression.kind {
            CoreExprKind::Name(name) => {
                let value = variables
                    .get_mut(name)
                    .ok_or_else(|| bad("original place has no lexical binder"))?;
                if value.used || !value.initialized {
                    return Err(bad("original place was already transferred"));
                }
                value.observed = true;
                copy_recipe(&value.ty, self.limits, self.left)
            }
            CoreExprKind::Member { object, member, .. } => {
                let owner_type = self.place(owner, object, variables, depth + 1)?;
                self.field(owner, &owner_type, member, depth + 1)
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let pointer = self.place(owner, operand, variables, depth + 1)?;
                let SemanticTypeRecipe::Address { inner, .. } = pointer else {
                    return Err(bad("original dereference place is not an address"));
                };
                Ok(*inner)
            }
            _ => self.expr(owner, expression, variables, None, depth + 1),
        }
    }
    fn field(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        name: &str,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(name.len(), depth)?;
        let ty = self.resolve(owner, ty, depth + 1)?;
        if let SemanticTypeRecipe::Environment {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        } = &ty
        {
            let (parent, actual) =
                self.proof
                    .environments
                    .get(&(owner, *ordinal))
                    .ok_or_else(|| {
                        pending("sealed whole-Type field awaits its parent symbolic source")
                    })?;
            let parent = *parent;
            let actual = copy_recipe(actual, self.limits, self.left)?;
            let field = self.field(parent, &actual, name, depth + 1)?;
            let child = self.definition(owner)?;
            return self.rehome(parent, child, &field, depth + 1);
        }
        let SemanticTypeRecipe::Nominal {
            definition,
            arguments,
        } = ty
        else {
            return Err(pending(
                "original field requires a visible complete nominal shape",
            ));
        };
        let definition = self.definition(definition)?;
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Product { fields },
            ..
        } = &definition.body
        else {
            return Err(bad("original field selects another nominal constructor"));
        };
        self.pay(fields.len(), depth)?;
        let field = fields
            .iter()
            .find(|field| field.name == name)
            .ok_or_else(|| bad("original field is absent from its declared nominal"))?;
        super::instantiate::ty(&field.ty, &arguments, self.limits, self.left)
    }
    fn rehome(
        &mut self,
        parent: SemanticDefinitionId,
        child: &SemanticDefinition,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let parent_definition = self.definition(parent)?;
        self.pay(
            child
                .environment
                .len()
                .saturating_add(parent_definition.parameters.len()),
            depth,
        )?;
        let mut actuals = Vec::new();
        for (ordinal, formal) in parent_definition.parameters.iter().enumerate() {
            let selector = SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: ordinal as u32,
            };
            let source = match formal {
                SemanticFormal::Lifetime => {
                    SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(selector))
                }
                SemanticFormal::Type { .. } => {
                    SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(selector))
                }
                SemanticFormal::Const { carrier } => SemanticActualRecipe::Const {
                    carrier: carrier.clone(),
                    value: SemanticConstRecipe::Formal(selector),
                },
            };
            let mut replacement = None;
            for (slot, environment) in child.environment.iter().enumerate() {
                self.pay(1, depth)?;
                if environment.source == source {
                    replacement = Some(match formal {
                        SemanticFormal::Lifetime => {
                            SemanticActualRecipe::Life(SemanticLifeRecipe::Environment {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: slot as u32,
                            })
                        }
                        SemanticFormal::Type { .. } => {
                            SemanticActualRecipe::Type(SemanticTypeRecipe::Environment {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: slot as u32,
                            })
                        }
                        SemanticFormal::Const { carrier } => SemanticActualRecipe::Const {
                            carrier: carrier.clone(),
                            value: SemanticConstRecipe::Environment {
                                scope: SemanticScopeRef::Ancestor(0),
                                ordinal: slot as u32,
                            },
                        },
                    });
                }
            }
            actuals.push(replacement.unwrap_or(source));
        }
        super::instantiate::ty(ty, &actuals, self.limits, self.left)
    }
    fn clone_variables(
        &mut self,
        variables: &BTreeMap<String, Binding>,
    ) -> R<BTreeMap<String, Binding>> {
        self.pay(variables.len(), 0)?;
        for (name, value) in variables {
            self.pay(name.len(), 0)?;
            crate::dyn_metadata::semantic_operand_snapshot(&value.ty, self.limits, self.left, 1)?;
        }
        Ok(variables.clone())
    }
    fn snapshot(
        &mut self,
        owner: SemanticDefinitionId,
        at: SemanticOriginalNode,
        variables: &BTreeMap<String, Binding>,
    ) -> R<()> {
        self.pay(self.program.definitions.len(), 0)?;
        for child in &self.program.definitions {
            if child.parent != Some(owner) {
                continue;
            }
            self.pay(child.environment.len(), 0)?;
            for formal in &child.environment {
                let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source:
                        SemanticOriginalTypeSource::NameAt {
                            at: requested,
                            name,
                        },
                }) = &formal.source
                else {
                    continue;
                };
                if *requested != at {
                    continue;
                }
                self.pay(name.len().saturating_add(variables.len()), 0)?;
                let value = variables.get(name).ok_or_else(|| {
                    bad("original NameAt has no lexical binder at its exact source point")
                })?;
                let value = copy_recipe(&value.ty, self.limits, self.left)?;
                let (expression, node) = match at {
                    SemanticOriginalNode::Expression(node) => (true, node),
                    SemanticOriginalNode::Statement(node) => (false, node),
                };
                self.proof.types.insert(
                    (
                        owner,
                        Query::NameAt {
                            expression,
                            node,
                            name: name.clone(),
                        },
                    ),
                    value,
                );
            }
        }
        Ok(())
    }
    fn resolve(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        if let SemanticTypeRecipe::OriginalType { scope, source } = ty {
            let mut target = owner;
            let SemanticScopeRef::Ancestor(hops) = scope;
            for step in 0..*hops {
                self.pay(1, depth + step as usize)?;
                target = self
                    .definition(target)?
                    .parent
                    .ok_or_else(|| bad("original type query escapes its lexical parent"))?;
            }
            let value = self
                .proof
                .types
                .get(&(target, Query::from(source.clone())))
                .ok_or_else(|| {
                    pending("original type query awaits its symbolic binding/expression equation")
                })?;
            let value = copy_recipe(value, self.limits, self.left)?;
            if target != owner {
                let current = self.definition(owner)?;
                if current.parent != Some(target) {
                    return Err(pending(
                        "original ancestor query needs every checked sealed lexical boundary",
                    ));
                }
                self.pay(current.environment.len(), depth)?;
                for (slot, formal) in current.environment.iter().enumerate() {
                    if matches!(&formal.source, SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: actual }) if actual == source)
                    {
                        return Ok(SemanticTypeRecipe::Environment {
                            scope: SemanticScopeRef::Ancestor(0),
                            ordinal: slot as u32,
                        });
                    }
                }
                return self.rehome(target, current, &value, depth + 1);
            }
            return Ok(value);
        }
        let mut value = copy_recipe(ty, self.limits, self.left)?;
        match &mut value {
            SemanticTypeRecipe::Opaque { hidden, .. } => {
                **hidden = self.resolve(owner, hidden, depth + 1)?
            }
            SemanticTypeRecipe::Address { inner, .. } | SemanticTypeRecipe::Desc { inner, .. } => {
                **inner = self.resolve(owner, inner, depth + 1)?
            }
            _ => {}
        }
        Ok(value)
    }
    // Unfold only a whole-Type environment already sealed from the immutable
    // lexical creator. This is a logical view, not a runtime read or a new premise.
    fn logical(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        if let SemanticTypeRecipe::Environment {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        } = value
        {
            let (parent, value) =
                self.proof
                    .environments
                    .get(&(owner, *ordinal))
                    .ok_or_else(|| {
                        pending("original operation needs its checked whole-Type environment")
                    })?;
            let parent = *parent;
            let value = copy_recipe(value, self.limits, self.left)?;
            return self.logical(parent, &value, depth + 1);
        }
        self.resolve(owner, value, depth + 1)
    }
    fn resolved_actual(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticActualRecipe,
        depth: usize,
    ) -> R<SemanticActualRecipe> {
        self.pay(1, depth)?;
        Ok(match value {
            SemanticActualRecipe::Type(value) => {
                SemanticActualRecipe::Type(self.resolve(owner, value, depth + 1)?)
            }
            SemanticActualRecipe::Life(value) => {
                crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)?;
                SemanticActualRecipe::Life(value.clone())
            }
            SemanticActualRecipe::Const { carrier, value } => {
                crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)?;
                SemanticActualRecipe::Const {
                    carrier: self.resolve(owner, carrier, depth + 1)?,
                    value: value.clone(),
                }
            }
            SemanticActualRecipe::Infer => {
                return Err(bad(
                    "original closure environment contains unresolved inference",
                ));
            }
        })
    }
    fn site(
        &mut self,
        owner: SemanticDefinitionId,
        node: SemanticOriginalNode,
    ) -> R<Option<&'a SemanticSiteRecipe>> {
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &definition.body
        else {
            return Err(bad("original operation has no complete body directory"));
        };
        self.pay(site_nodes.len().saturating_add(sites.len()), 0)?;
        let mut found = None;
        for row in site_nodes {
            if row.node == node {
                let site = sites
                    .iter()
                    .find(|site| site.id == row.site)
                    .ok_or_else(|| bad("original node directory references an absent recipe"))?;
                // One expression may have a type-use and its actual operation.
                if matches!(
                    site.kind,
                    SemanticSiteRecipeKind::Call { .. }
                        | SemanticSiteRecipeKind::CallableCall { .. }
                        | SemanticSiteRecipeKind::Closure { .. }
                        | SemanticSiteRecipeKind::Construct { .. }
                ) {
                    if found.replace(site).is_some() {
                        return Err(bad("original expression has multiple operation recipes"));
                    }
                }
            }
        }
        Ok(found)
    }
    fn borrow_life(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
    ) -> R<SemanticOuterLifeRecipe> {
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &definition.body
        else {
            return Err(bad("borrow has no original body"));
        };
        self.pay(
            site_nodes
                .len()
                .saturating_add(sites.len())
                .saturating_add(definition.implicit_lifetimes.len()),
            0,
        )?;
        let mut selected = None;
        for row in site_nodes {
            if row.node == SemanticOriginalNode::Expression(node) {
                if let Some(site) = sites.iter().find(|site| {
                    site.id == row.site
                        && matches!(site.kind, SemanticSiteRecipeKind::TypeUse { .. })
                }) {
                    for implicit in &definition.implicit_lifetimes {
                        if implicit.occurrence_path == site.occurrence_path {
                            if selected.replace(implicit.ordinal).is_some() {
                                return Err(bad(
                                    "one original borrow has ambiguous declaration occurrences",
                                ));
                            }
                        }
                    }
                }
            }
        }
        Ok(SemanticOuterLifeRecipe::Class(
            SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: selected.ok_or_else(|| {
                    pending("original borrow needs its own explicit declaration occurrence")
                })?,
            },
        ))
    }
    fn unify(
        &mut self,
        template: &SemanticTypeRecipe,
        actual: &SemanticTypeRecipe,
        formals: &[SemanticFormal],
        solutions: &mut [Option<SemanticActualRecipe>],
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        match (template, actual) {
            (
                SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal,
                }),
                _,
            ) if matches!(
                formals.get(*ordinal as usize),
                Some(SemanticFormal::Type { .. })
            ) =>
            {
                let slot = solutions
                    .get_mut(*ordinal as usize)
                    .ok_or_else(|| bad("call inference formal is absent"))?;
                if let Some(SemanticActualRecipe::Type(old)) = slot {
                    self.same(old, actual)?;
                } else if slot.is_none() {
                    *slot = Some(SemanticActualRecipe::Type(copy_recipe(
                        actual,
                        self.limits,
                        self.left,
                    )?));
                } else {
                    return Err(bad("call inference changes mixed formal kind"));
                }
            }
            (
                SemanticTypeRecipe::Address {
                    mutable: requested,
                    lifetime,
                    inner,
                },
                SemanticTypeRecipe::Address {
                    mutable: available,
                    lifetime: actual_life,
                    inner: actual_inner,
                },
            ) => {
                if *requested && !available {
                    return Err(bad(
                        "original call grants exclusive access through a shared actual",
                    ));
                }
                self.unify(inner, actual_inner, formals, solutions, depth + 1)?;
                if let (
                    SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Formal(SemanticFormalRef {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal,
                    })),
                    SemanticOuterLifeRecipe::Class(actual),
                ) = (lifetime, actual_life)
                {
                    if !matches!(
                        formals.get(*ordinal as usize),
                        Some(SemanticFormal::Lifetime)
                    ) {
                        return Err(bad("call lifetime inference selects a non-Life formal"));
                    }
                    let slot = solutions
                        .get_mut(*ordinal as usize)
                        .ok_or_else(|| bad("call lifetime formal is absent"))?;
                    let value = SemanticActualRecipe::Life(actual.clone());
                    if slot.as_ref().is_some_and(|old| old != &value) {
                        return Err(pending(
                            "original call requires its complete outer-role shortening relation",
                        ));
                    }
                    *slot = Some(value);
                } else if lifetime != actual_life {
                    return Err(pending(
                        "original call needs its scoped outer lifetime correspondence",
                    ));
                }
            }
            (SemanticTypeRecipe::Tuple(left), SemanticTypeRecipe::Tuple(right))
                if left.len() == right.len() =>
            {
                for (left, right) in left.iter().zip(right) {
                    self.unify(left, right, formals, solutions, depth + 1)?;
                }
            }
            _ => self.same(template, actual)?,
        }
        Ok(())
    }
    fn ordinary_value(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        let ty = self.logical(owner, ty, depth + 1)?;
        Ok(match &ty {
            SemanticTypeRecipe::Atom(ty) => ty.is_b0_scalar(),
            SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Closure { .. } => true,
            SemanticTypeRecipe::Tuple(fields) => {
                let mut all = true;
                for field in fields {
                    all &= self.ordinary_value(owner, field, depth + 1)?;
                }
                all
            }
            SemanticTypeRecipe::Formal(reference)
                if reference.scope == SemanticScopeRef::Ancestor(0) =>
            {
                let original = self.definition(owner)?;
                let mut found = false;
                if let Some(SemanticFormal::Type { requirements }) =
                    original.parameters.get(reference.ordinal as usize)
                {
                    for requirement in requirements.iter().chain(&original.requirements) {
                        self.pay(1, depth)?;
                        found |= matches!(requirement, SemanticRequirementRecipe::Builtin { ty:actual,trait_kind:SemanticBuiltinTrait::Value } if actual==&ty);
                    }
                }
                found
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let original = self.definition(*definition)?;
                let SemanticDefinitionBody::Nominal {
                    body: SemanticNominalBody::Product { fields },
                    ..
                } = &original.body
                else {
                    return Err(pending(
                        "ordinary Value sum requires complete original alternatives",
                    ));
                };
                let mut all = true;
                for field in fields {
                    let ty = super::instantiate::ty(&field.ty, arguments, self.limits, self.left)?;
                    all &= self.ordinary_value(owner, &ty, depth + 1)?;
                }
                all
            }
            _ => {
                return Err(pending(
                    "ordinary Value premise needs complete original carrier semantics",
                ));
            }
        })
    }
    fn call_requirements(
        &mut self,
        owner: SemanticDefinitionId,
        target: &SemanticDefinition,
        actuals: &[SemanticActualRecipe],
        depth: usize,
    ) -> R<()> {
        for requirement in target
            .requirements
            .iter()
            .chain(target.parameters.iter().flat_map(|formal| match formal {
                SemanticFormal::Type { requirements } => requirements.as_slice(),
                _ => &[],
            }))
        {
            self.pay(1, depth)?;
            match requirement {
                SemanticRequirementRecipe::Structural {
                    ty,
                    bound: DynStructuralBound::Copy,
                }
                | SemanticRequirementRecipe::Builtin {
                    ty,
                    trait_kind: SemanticBuiltinTrait::Copy,
                } => {
                    let ty = super::instantiate::ty(ty, actuals, self.limits, self.left)?;
                    if !self.copy_type(owner, &ty, depth + 1)? {
                        return Err(bad(
                            "original caller lacks required whole-Type Copy premise",
                        ));
                    }
                }
                SemanticRequirementRecipe::Builtin {
                    ty,
                    trait_kind: SemanticBuiltinTrait::Value,
                } => {
                    let ty = super::instantiate::ty(ty, actuals, self.limits, self.left)?;
                    if !self.ordinary_value(owner, &ty, depth + 1)? {
                        return Err(bad(
                            "original caller lacks required whole-Type Value premise",
                        ));
                    }
                }
                SemanticRequirementRecipe::TypeEqual { left, right } => {
                    let left = super::instantiate::ty(left, actuals, self.limits, self.left)?;
                    let right = super::instantiate::ty(right, actuals, self.limits, self.left)?;
                    self.same(&left, &right)?;
                }
                SemanticRequirementRecipe::Callable {
                    ty:
                        SemanticTypeRecipe::OriginalType {
                            scope: SemanticScopeRef::Ancestor(0),
                            source: SemanticOriginalTypeSource::Result,
                        },
                    ..
                } if self.proof.complete.contains(&target.id) => {}
                _ => {
                    return Err(pending(
                        "original call requires complete symbolic where satisfaction",
                    ));
                }
            }
        }
        Ok(())
    }
    fn written(
        &mut self,
        owner: SemanticDefinitionId,
        callee: &SemanticOriginalExpr,
        formals: &[SemanticFormal],
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<Vec<Option<SemanticActualRecipe>>> {
        self.pay(formals.len(), depth)?;
        let mut result = vec![None; formals.len()];
        if let CoreExprKind::GenericApply { arguments, .. } = &callee.kind {
            if arguments.len() != formals.len() {
                return Err(pending(
                    "written mixed arguments need complete original arity/inference positions",
                ));
            }
            for (index, (argument, formal)) in arguments.iter().zip(formals).enumerate() {
                result[index] = Some(match (argument, formal) {
                    (CoreGenericArgument::Type(ty), SemanticFormal::Type { .. }) => {
                        SemanticActualRecipe::Type(self.resolve(owner, ty, depth + 1)?)
                    }
                    (CoreGenericArgument::Lifetime(name), SemanticFormal::Lifetime) => {
                        let definition = self.definition(owner)?;
                        let SemanticDefinitionBody::FunctionWithBody { names, .. } =
                            &definition.body
                        else {
                            return Err(bad("written life has no original name table"));
                        };
                        self.pay(names.len().saturating_add(name.len()), depth)?;
                        let binding = names
                            .iter()
                            .find(|binding| {
                                binding.namespace == SemanticOriginalNamespace::Lifetime
                                    && binding.path.len() == 1
                                    && binding.path[0] == *name
                            })
                            .ok_or_else(|| {
                                bad("written lifetime does not name an original lexical formal")
                            })?;
                        let SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Life(life)) =
                            &binding.value
                        else {
                            return Err(bad("written lifetime names another original kind"));
                        };
                        crate::dyn_metadata::semantic_operand_snapshot(
                            life,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        SemanticActualRecipe::Life(life.clone())
                    }
                    (CoreGenericArgument::Const(value), SemanticFormal::Const { carrier }) => {
                        let mut arguments = Vec::new();
                        self.pay(formals.len(), depth)?;
                        for (ordinal, formal) in formals.iter().enumerate() {
                            if let Some(actual) = result.get(ordinal).and_then(|r| r.as_ref()) {
                                crate::dyn_metadata::semantic_operand_snapshot(
                                    actual,
                                    self.limits,
                                    self.left,
                                    1,
                                )?;
                                arguments.push(actual.clone());
                            } else if matches!(formal, SemanticFormal::Type { .. }) {
                                arguments.push(SemanticActualRecipe::Type(
                                    SemanticTypeRecipe::Formal(SemanticFormalRef {
                                        scope: SemanticScopeRef::Ancestor(0),
                                        ordinal: ordinal as u32,
                                    }),
                                ));
                            } else {
                                arguments.push(SemanticActualRecipe::Infer);
                            }
                        }
                        let carrier =
                            super::instantiate::ty(carrier, &arguments, self.limits, self.left)?;
                        let actual_ty =
                            self.expr(owner, value, variables, Some(&carrier), depth + 1)?;
                        self.same(&carrier, &actual_ty)?;
                        let SemanticTypeRecipe::Atom(ty) = actual_ty else {
                            return Err(pending(
                                "written const needs its original symbolic value/carrier equation",
                            ));
                        };
                        let value = match &value.kind {
                            CoreExprKind::Literal(value) => value.clone(),
                            CoreExprKind::Integer(text) => {
                                integer(text, Some(&ty), false, value.span)?
                            }
                            _ => {
                                return Err(pending(
                                    "written const needs its complete original const evaluator",
                                ));
                            }
                        };
                        SemanticActualRecipe::Const {
                            carrier: SemanticTypeRecipe::Atom(ty),
                            value: SemanticConstRecipe::Value(value),
                        }
                    }
                    _ => {
                        return Err(bad(
                            "written argument changes its original mixed formal kind",
                        ));
                    }
                });
            }
        } else if let CoreExprKind::TypeApply { type_arguments, .. } = &callee.kind {
            if type_arguments.len() != formals.len()
                || formals
                    .iter()
                    .any(|f| !matches!(f, SemanticFormal::Type { .. }))
            {
                return Err(bad(
                    "type-only application omits original mixed argument positions",
                ));
            }
            for (slot, ty) in result.iter_mut().zip(type_arguments) {
                *slot = Some(SemanticActualRecipe::Type(self.resolve(
                    owner,
                    ty,
                    depth + 1,
                )?));
            }
        }
        Ok(result)
    }
    fn expr(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        self.snapshot(
            owner,
            SemanticOriginalNode::Expression(expression.id),
            variables,
        )?;
        let result = match &expression.kind {
            CoreExprKind::Literal(value) => SemanticTypeRecipe::Atom(value.ty()),
            CoreExprKind::Integer(text) => {
                let expected = wanted.and_then(|ty| {
                    if let SemanticTypeRecipe::Atom(ty) = ty {
                        Some(ty)
                    } else {
                        None
                    }
                });
                SemanticTypeRecipe::Atom(integer(text, expected, false, expression.span)?.ty())
            }
            CoreExprKind::Float(text) => {
                let expected = wanted.and_then(|ty| {
                    if let SemanticTypeRecipe::Atom(ty) = ty {
                        Some(ty)
                    } else {
                        None
                    }
                });
                SemanticTypeRecipe::Atom(floats::literal(text, expected, expression.span)?.ty())
            }
            CoreExprKind::Bool(_) => SemanticTypeRecipe::Atom(Type::Bool),
            CoreExprKind::Unit => SemanticTypeRecipe::Atom(Type::Unit),
            CoreExprKind::Name(name) => {
                self.pay(name.len().saturating_add(variables.len()), depth)?;
                let value = variables
                    .get_mut(name)
                    .ok_or_else(|| bad("original name has no lexical value binder"))?;
                if value.used || !value.initialized {
                    return Err(bad(
                        "original body reads a value after its affine whole-type transfer",
                    ));
                }
                value.observed = true;
                if !self.copy_type(owner, &value.ty, depth + 1)? {
                    if value.borrowed {
                        return Err(bad(
                            "original body moves a non-Copy value from a shared capture",
                        ));
                    }
                    value.used = true;
                }
                copy_recipe(&value.ty, self.limits, self.left)?
            }
            CoreExprKind::Ascribed { ty, value } => {
                let expected = self.resolve(owner, ty, depth + 1)?;
                let actual = self.expr(owner, value, variables, Some(&expected), depth + 1)?;
                self.same(&actual, &expected)?;
                expected
            }
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                operand,
            } => {
                let inner = self.place(owner, operand, variables, depth + 1)?;
                SemanticTypeRecipe::Address {
                    mutable: matches!(
                        expression.kind,
                        CoreExprKind::Unary {
                            op: UnaryOp::BorrowMut,
                            ..
                        }
                    ),
                    lifetime: self.borrow_life(owner, expression.id)?,
                    inner: Box::new(inner),
                }
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let value = self.expr(owner, operand, variables, None, depth + 1)?;
                let SemanticTypeRecipe::Address { inner, .. } = value else {
                    return Err(bad("original dereference has no address type"));
                };
                *inner
            }
            CoreExprKind::Unary { op, operand } => {
                let value = self.expr(owner, operand, variables, wanted, depth + 1)?;
                let SemanticTypeRecipe::Atom(ty) = &value else {
                    return Err(pending(
                        "original unary operation lacks its declared symbolic primitive premise",
                    ));
                };
                let valid = match op {
                    UnaryOp::Not => *ty == Type::Bool,
                    UnaryOp::BitNot => ty.is_integer(),
                    UnaryOp::Negate => ty.is_integer() || ty.is_float(),
                    _ => false,
                };
                if !valid {
                    return Err(bad(
                        "original unary operator has incompatible primitive endpoint",
                    ));
                }
                value
            }
            CoreExprKind::Binary { op, left, right } => {
                let left = self.expr(owner, left, variables, wanted, depth + 1)?;
                let right = self.expr(owner, right, variables, Some(&left), depth + 1)?;
                let left = self.logical(owner, &left, depth + 1)?;
                let right = self.logical(owner, &right, depth + 1)?;
                self.same(&left, &right)?;
                let SemanticTypeRecipe::Atom(ty) = left else {
                    return Err(bad(
                        "original generic operation lacks a primitive type equality premise",
                    ));
                };
                let boolean = matches!(op, BinaryOp::And | BinaryOp::Or);
                if boolean && ty != Type::Bool || !boolean && !ty.is_integer() && !ty.is_float() {
                    return Err(bad(
                        "original binary operator has incompatible primitive endpoint",
                    ));
                }
                SemanticTypeRecipe::Atom(
                    if boolean
                        || matches!(
                            op,
                            BinaryOp::Equal
                                | BinaryOp::NotEqual
                                | BinaryOp::Less
                                | BinaryOp::LessEqual
                                | BinaryOp::Greater
                                | BinaryOp::GreaterEqual
                        )
                    {
                        Type::Bool
                    } else {
                        ty
                    },
                )
            }
            CoreExprKind::Member { object, member, .. } => {
                let owner_type = self.place(owner, object, variables, depth + 1)?;
                let field = self.field(owner, &owner_type, member, depth + 1)?;
                if !self.copy_type(owner, &field, depth + 1)? {
                    return Err(pending(
                        "original affine field observation needs its explicit view/transfer rule",
                    ));
                }
                field
            }
            CoreExprKind::Record { ty, fields } => {
                let ty = self.resolve(owner, ty, depth + 1)?;
                let SemanticTypeRecipe::Nominal {
                    definition,
                    arguments,
                } = &ty
                else {
                    return Err(bad(
                        "original record constructor has no nominal declaration",
                    ));
                };
                let original = self.definition(*definition)?;
                let SemanticDefinitionBody::Nominal {
                    body:
                        SemanticNominalBody::Product {
                            fields: declarations,
                        },
                    ..
                } = &original.body
                else {
                    return Err(bad(
                        "original record constructor changes its declaration kind",
                    ));
                };
                if fields.len() != declarations.len() {
                    return Err(bad("original record constructor omits or adds fields"));
                }
                for ((name, value), field) in fields.iter().zip(declarations) {
                    if name != &field.name {
                        return Err(bad(
                            "original record constructor changes field identity/order",
                        ));
                    }
                    let expected =
                        super::instantiate::ty(&field.ty, arguments, self.limits, self.left)?;
                    let actual = self.expr(owner, value, variables, Some(&expected), depth + 1)?;
                    self.same(&actual, &expected)?;
                }
                ty
            }
            CoreExprKind::Closure {
                move_capture,
                parameters,
                body,
                ..
            } => {
                let site = self
                    .site(owner, SemanticOriginalNode::Expression(expression.id))?
                    .ok_or_else(|| {
                        bad("original closure has no explicit child declaration site")
                    })?;
                let SemanticSiteRecipeKind::Closure {
                    definition,
                    environment,
                } = &site.kind
                else {
                    return Err(bad("original closure site changes operation kind"));
                };
                let child = self.definition(*definition)?;
                if child.parent != Some(owner) {
                    return Err(bad("original closure has a different lexical creator"));
                }
                let SemanticDefinitionBody::FunctionWithBody {
                    parameters: declared,
                    body: SemanticOriginalFunctionBody::Defined(child_body),
                    ..
                } = &child.body
                else {
                    return Err(bad(
                        "original closure omits its complete parameter/body declaration",
                    ));
                };
                if parameters.len() != declared.len()
                    || parameters
                        .iter()
                        .zip(declared)
                        .any(|(a, b)| a.name != b.name)
                {
                    return Err(bad(
                        "closure original parameter binders differ across its real source site",
                    ));
                }
                let _ = (body, child_body); // Full operation correspondence is indexed per original owner.

                let mut captured = BTreeMap::new();
                for (name, value) in variables.iter() {
                    self.pay(name.len(), depth)?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        &value.ty,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    let mut value = value.clone();
                    value.capture = true;
                    value.borrowed = !move_capture;
                    value.observed = false;
                    let mut sealed = None;
                    for (slot, formal) in child.environment.iter().enumerate() {
                        if let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                            scope: SemanticScopeRef::Ancestor(0),
                            source,
                        }) = &formal.source
                        {
                            let matches = match source {
                                SemanticOriginalTypeSource::NameAt {
                                    at: SemanticOriginalNode::Expression(at),
                                    name: source_name,
                                } => *at == expression.id && source_name == name,
                                _ => Query::from(source.clone()) == value.origin,
                            };
                            if matches {
                                let parent_type = copy_recipe(&value.ty, self.limits, self.left)?;
                                self.proof
                                    .environments
                                    .insert((child.id, slot as u32), (owner, parent_type));
                                sealed = Some(SemanticTypeRecipe::Environment {
                                    scope: SemanticScopeRef::Ancestor(0),
                                    ordinal: slot as u32,
                                });
                            }
                        }
                    }
                    value.ty = if let Some(sealed) = sealed {
                        sealed
                    } else {
                        self.rehome(owner, child, &value.ty, depth + 1)?
                    };
                    captured.insert(name.clone(), value);
                }
                self.function(child, Some(captured))?;
                if *move_capture {
                    let captures = self.captures.get(definition).cloned().ok_or_else(|| {
                        bad("original closure lacks a completed lexical capture set")
                    })?;
                    for name in captures {
                        let value = variables
                            .get_mut(&name)
                            .ok_or_else(|| bad("original capture lost its creator binder"))?;
                        if !self.copy_type(owner, &value.ty, depth + 1)? {
                            if value.used || !value.initialized {
                                return Err(bad(
                                    "original closure captures an already transferred affine value",
                                ));
                            }
                            value.used = true;
                        }
                    }
                }
                let mut sealed_environment = Vec::new();
                for actual in environment {
                    sealed_environment.push(self.resolved_actual(owner, actual, depth + 1)?);
                }
                SemanticTypeRecipe::Closure {
                    definition: *definition,
                    arguments: Vec::new(),
                    environment: sealed_environment,
                }
            }
            CoreExprKind::Tuple(fields) => {
                let mut types = Vec::new();
                for field in fields {
                    types.push(self.expr(owner, field, variables, None, depth + 1)?);
                }
                SemanticTypeRecipe::Tuple(types)
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.expr(
                    owner,
                    condition,
                    variables,
                    Some(&SemanticTypeRecipe::Atom(Type::Bool)),
                    depth + 1,
                )?;
                self.same(&condition, &SemanticTypeRecipe::Atom(Type::Bool))?;
                let a = self.expr(owner, then_value, variables, wanted, depth + 1)?;
                let b = self.expr(owner, else_value, variables, Some(&a), depth + 1)?;
                self.same(&a, &b)?;
                a
            }
            CoreExprKind::Call {
                callee: _,
                arguments,
            } => {
                let site = self
                    .site(owner, SemanticOriginalNode::Expression(expression.id))?
                    .ok_or_else(|| {
                        pending("original call has no resolved original target recipe")
                    })?;
                let (target, actuals) = match &site.kind {
                    SemanticSiteRecipeKind::Call { target, actuals } => (*target, actuals.clone()),
                    SemanticSiteRecipeKind::CallableCall { .. } => {
                        let CoreExprKind::Call { callee, .. } = &expression.kind else {
                            unreachable!()
                        };
                        let value = self.place(owner, callee, variables, depth + 1)?;
                        let value = if let SemanticTypeRecipe::Opaque { hidden, .. } = value {
                            *hidden
                        } else {
                            value
                        };
                        let SemanticTypeRecipe::Closure {
                            definition,
                            arguments,
                            ..
                        } = value
                        else {
                            return Err(pending(
                                "abstract callable needs its complete declared original role premise",
                            ));
                        };
                        (definition, arguments)
                    }
                    SemanticSiteRecipeKind::OriginalCall => {
                        let CoreExprKind::Call { callee, .. } = &expression.kind else {
                            unreachable!()
                        };
                        let mut callee = callee.as_ref();
                        while let CoreExprKind::TypeApply { callee: inner, .. }
                        | CoreExprKind::GenericApply { callee: inner, .. } = &callee.kind
                        {
                            callee = inner;
                        }
                        if let CoreExprKind::Name(name) = &callee.kind {
                            if variables.contains_key(name) {
                                let value = self.place(owner, callee, variables, depth + 1)?;
                                let value = if let SemanticTypeRecipe::Opaque { hidden, .. } = value
                                {
                                    *hidden
                                } else {
                                    value
                                };
                                let SemanticTypeRecipe::Closure {
                                    definition,
                                    arguments,
                                    ..
                                } = value
                                else {
                                    return Err(pending(
                                        "original callable requires its declared complete role premise",
                                    ));
                                };
                                (definition, arguments)
                            } else {
                                let current = self.definition(owner)?;
                                let SemanticDefinitionBody::FunctionWithBody { names, .. } =
                                    &current.body
                                else {
                                    unreachable!()
                                };
                                self.pay(names.len(), depth)?;
                                let mut targets = names.iter().filter(|binding| {
                                    binding.namespace == SemanticOriginalNamespace::Value
                                        && binding.path.len() == 1
                                        && binding.path[0] == *name
                                });
                                let binding = targets.next().ok_or_else(|| pending("original call name lacks its real declaration import binding"))?;
                                if targets.next().is_some() {
                                    return Err(bad(
                                        "original name has ambiguous declaration bindings",
                                    ));
                                }
                                let SemanticOriginalNameValue::Definition(target) = binding.value
                                else {
                                    return Err(pending(
                                        "original callee recipe requires its complete callable interpretation",
                                    ));
                                };
                                let target_definition = self.definition(target)?;
                                (
                                    target,
                                    vec![
                                        SemanticActualRecipe::Infer;
                                        target_definition.parameters.len()
                                    ],
                                )
                            }
                        } else {
                            return Err(pending(
                                "original member/builtin call needs its complete declared scheme",
                            ));
                        }
                    }
                    _ => return Err(bad("original call site has a different operation kind")),
                };
                let definition = self.definition(target)?;
                if !self.proof.complete.contains(&target)
                    && !self.active.contains(&target)
                    && definition.kind != SemanticDefinitionKind::Closure
                {
                    self.function(definition, None)?;
                }
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
                else {
                    return Err(bad(
                        "original call target omits its complete body/signature",
                    ));
                };
                if signature.parameters.len() != arguments.len()
                    || actuals.len() != definition.parameters.len()
                {
                    return Err(bad(
                        "original call omits complete parameters or mixed actuals",
                    ));
                }
                let CoreExprKind::Call { callee, .. } = &expression.kind else {
                    unreachable!()
                };
                let mut solutions =
                    self.written(owner, callee, &definition.parameters, variables, depth + 1)?;
                for (slot, actual) in solutions.iter_mut().zip(&actuals) {
                    if !matches!(actual, SemanticActualRecipe::Infer) {
                        crate::dyn_metadata::semantic_operand_snapshot(
                            actual,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        if slot.as_ref().is_some_and(|old| old != actual) {
                            return Err(bad(
                                "written actual differs from the original call recipe",
                            ));
                        }
                        *slot = Some(actual.clone());
                    }
                }
                for (argument, parameter) in arguments.iter().zip(&signature.parameters) {
                    let actual = self.expr(owner, argument, variables, None, depth + 1)?;
                    self.unify(
                        &parameter.ty,
                        &actual,
                        &definition.parameters,
                        &mut solutions,
                        depth + 1,
                    )?;
                }
                let result_template = self.resolve(target, &signature.result, depth + 1)?;
                if let Some(wanted) = wanted {
                    self.unify(
                        &result_template,
                        wanted,
                        &definition.parameters,
                        &mut solutions,
                        depth + 1,
                    )?;
                }
                let actuals: Vec<_> = solutions.into_iter().collect::<Option<_>>().ok_or_else(|| pending("original call's fresh mixed inference variables are not fully determined"))?;
                self.call_requirements(owner, definition, &actuals, depth + 1)?;
                self.pay(actuals.len(), depth)?;
                self.proof.call_targets.insert((owner, site.id), target);
                for (ordinal, actual) in actuals.iter().enumerate() {
                    if let SemanticActualRecipe::Const { carrier, .. } = actual {
                        let carrier = copy_recipe(carrier, self.limits, self.left)?;
                        self.proof.types.insert(
                            (
                                owner,
                                Query::CallConstCarrier {
                                    call: expression.id,
                                    formal: ordinal as u32,
                                },
                            ),
                            carrier,
                        );
                    }
                }
                self.proof.calls.insert((owner, site.id), actuals.clone());
                super::instantiate::ty(&result_template, &actuals, self.limits, self.left)?
            }
            _ => {
                return Err(pending(
                    "original operation awaits its complete symbolic typing rule",
                ));
            }
        };
        let copy = copy_recipe(&result, self.limits, self.left)?;
        self.proof
            .types
            .insert((owner, Query::Expression(expression.id)), copy);
        Ok(result)
    }
    fn block(
        &mut self,
        owner: SemanticDefinitionId,
        block: &SemanticOriginalBlock,
        variables: &mut BTreeMap<String, Binding>,
        result: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(block.statements.len(), depth)?;
        for statement in &block.statements {
            self.snapshot(
                owner,
                SemanticOriginalNode::Statement(statement.id),
                variables,
            )?;
            match &statement.kind {
                CoreStatementKind::Binding {
                    name,
                    mutable,
                    ty,
                    initializer,
                } => {
                    let annotation = ty
                        .as_ref()
                        .map(|ty| self.resolve(owner, ty, depth + 1))
                        .transpose()?;
                    let actual = initializer
                        .as_ref()
                        .map(|value| {
                            self.expr(owner, value, variables, annotation.as_ref(), depth + 1)
                        })
                        .transpose()?;
                    if let (Some(a), Some(b)) = (&annotation, &actual) {
                        self.same(a, b)?;
                    }
                    let value = annotation.or(actual).ok_or_else(|| {
                        bad("original uninitialized binding has no declared symbolic type")
                    })?;
                    let copy = copy_recipe(&value, self.limits, self.left)?;
                    self.proof
                        .types
                        .insert((owner, Query::Binding(statement.id)), copy);
                    variables.insert(
                        name.clone(),
                        Binding {
                            ty: value,
                            mutable: *mutable,
                            used: false,
                            observed: false,
                            capture: false,
                            borrowed: false,
                            origin: Query::Binding(statement.id),
                            initialized: initializer.is_some(),
                            written: false,
                        },
                    );
                }
                CoreStatementKind::Return(Some(value)) => {
                    let target = self.resolve(owner, result, depth + 1).ok();
                    let value = self.expr(owner, value, variables, target.as_ref(), depth + 1)?;
                    let infer = matches!(
                        result,
                        SemanticTypeRecipe::OriginalType {
                            scope: SemanticScopeRef::Ancestor(0),
                            source: SemanticOriginalTypeSource::Result
                        }
                    ) || matches!(result, SemanticTypeRecipe::Opaque { hidden, .. } if matches!(hidden.as_ref(), SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result }));
                    if infer {
                        if let Some(old) = self.proof.types.get(&(owner, Query::Result)) {
                            let old = copy_recipe(old, self.limits, self.left)?;
                            self.same(&old, &value)?;
                        } else {
                            let copy = copy_recipe(&value, self.limits, self.left)?;
                            self.proof.types.insert((owner, Query::Result), copy);
                        }
                    }
                    let expected = self.resolve(owner, result, depth + 1)?;
                    self.same(&value, &expected)?;
                }
                CoreStatementKind::Return(None) => {
                    self.same(result, &SemanticTypeRecipe::Atom(Type::Unit))?
                }
                CoreStatementKind::Expression(value) => {
                    self.expr(owner, value, variables, None, depth + 1)?;
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    let condition = self.expr(
                        owner,
                        condition,
                        variables,
                        Some(&SemanticTypeRecipe::Atom(Type::Bool)),
                        depth + 1,
                    )?;
                    self.same(&condition, &SemanticTypeRecipe::Atom(Type::Bool))?;
                    let mut left = self.clone_variables(variables)?;
                    let mut right = self.clone_variables(variables)?;
                    self.block(owner, then_body, &mut left, result, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(owner, body, &mut right, result, depth + 1)?;
                    }
                    for (name, value) in variables.iter_mut() {
                        value.used |= left.get(name).is_some_and(|v| v.used)
                            || right.get(name).is_some_and(|v| v.used);
                        value.observed |= left.get(name).is_some_and(|v| v.observed)
                            || right.get(name).is_some_and(|v| v.observed);
                    }
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    let mut inner = self.clone_variables(variables)?;
                    self.block(owner, body, &mut inner, result, depth + 1)?;
                    for (name, value) in variables.iter_mut() {
                        if let Some(inner) = inner.get(name) {
                            value.used |= inner.used;
                            value.observed |= inner.observed;
                        }
                    }
                }
                CoreStatementKind::Defer(_) => {
                    return Err(pending(
                        "original deferred cleanup requires its complete exit phase proof",
                    ));
                }
                CoreStatementKind::Assign { target, value } => {
                    let CoreExprKind::Name(name) = &target.kind else {
                        return Err(pending(
                            "original assignment requires its full product/borrow place proof",
                        ));
                    };
                    let binding = variables
                        .get(name)
                        .ok_or_else(|| bad("original assignment has no lexical target"))?;
                    if !binding.mutable {
                        return Err(bad("original assignment targets an immutable binder"));
                    }
                    let expected = copy_recipe(&binding.ty, self.limits, self.left)?;
                    let actual = self.expr(owner, value, variables, Some(&expected), depth + 1)?;
                    self.same(&actual, &expected)?;
                    let target = variables
                        .get_mut(name)
                        .ok_or_else(|| bad("original assignment target disappeared"))?;
                    target.used = false;
                    target.initialized = true;
                    target.written = true;
                }
                _ => {
                    return Err(pending(
                        "original statement needs its complete symbolic assignment/pattern/control-flow rule",
                    ));
                }
            }
        }
        Ok(())
    }
    fn function(
        &mut self,
        definition: &'a SemanticDefinition,
        parent: Option<BTreeMap<String, Binding>>,
    ) -> R<()> {
        if self.proof.complete.contains(&definition.id) {
            return Ok(());
        }
        self.pay(self.active.len().saturating_add(1), 0)?;
        if !self.active.insert(definition.id) {
            return Err(pending(
                "recursive original inference needs a complete declared signature",
            ));
        }
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            parameters,
            body,
            ..
        } = &definition.body
        else {
            return Err(pending(
                "executable original definition omits its complete immutable operation body",
            ));
        };
        let mut variables = parent.unwrap_or_default();
        if parameters.len() != signature.parameters.len() {
            return Err(bad(
                "original parameter identity table differs from signature",
            ));
        }
        for (index, (binder, parameter)) in parameters.iter().zip(&signature.parameters).enumerate()
        {
            let ty = self.resolve(definition.id, &parameter.ty, 0)?;
            if let Some(annotation) = &binder.ty {
                let annotation = self.resolve(definition.id, annotation, 0)?;
                self.same(&ty, &annotation)?;
            }
            let copy = copy_recipe(&ty, self.limits, self.left)?;
            self.proof
                .types
                .insert((definition.id, Query::Parameter(index as u32)), copy);
            variables.insert(
                binder.name.clone(),
                Binding {
                    ty,
                    mutable: false,
                    used: false,
                    observed: false,
                    capture: false,
                    borrowed: false,
                    origin: Query::Parameter(index as u32),
                    initialized: true,
                    written: false,
                },
            );
        }
        let result = copy_recipe(&signature.result, self.limits, self.left)?;
        match body {
            SemanticOriginalFunctionBody::Defined(body) => {
                self.block(definition.id, body, &mut variables, &result, 0)?
            }
            SemanticOriginalFunctionBody::External => {}
            SemanticOriginalFunctionBody::Declaration => {
                return Err(pending(
                    "abstract original method needs its complete trait member witness",
                ));
            }
        }
        let names = variables
            .iter()
            .filter(|(_, binding)| binding.capture && binding.observed)
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        let mut kind = CallableKind::Fn;
        for value in variables
            .values()
            .filter(|value| value.capture && value.observed)
        {
            if value.used && !self.copy_type(definition.id, &value.ty, 0)? {
                kind = CallableKind::FnOnce;
            } else if value.written && kind != CallableKind::FnOnce {
                kind = CallableKind::FnMut;
            }
        }
        self.proof.kinds.insert(definition.id, kind);
        self.captures.insert(definition.id, names);
        if !self
            .proof
            .types
            .contains_key(&(definition.id, Query::Result))
        {
            let result = self.resolve(definition.id, &result, 0)?;
            self.proof
                .types
                .insert((definition.id, Query::Result), result);
        }
        for requirement in &definition.requirements {
            if let SemanticRequirementRecipe::Callable {
                ty,
                kind: required,
                signature: wanted,
            } = requirement
            {
                if matches!(
                    ty,
                    SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0),
                        source: SemanticOriginalTypeSource::Result
                    }
                ) {
                    let actual = self.resolve(definition.id, ty, 0)?;
                    let SemanticTypeRecipe::Closure {
                        definition: child, ..
                    } = actual
                    else {
                        return Err(pending(
                            "opaque result callable goal requires its completed original code witness",
                        ));
                    };
                    let actual_kind =
                        *self.proof.kinds.get(&child).ok_or_else(|| {
                            bad("opaque result has no original capture-effect kind")
                        })?;
                    let compatible = matches!(
                        (actual_kind, *required),
                        (
                            CallableKind::Fn,
                            CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
                        ) | (
                            CallableKind::FnMut,
                            CallableKind::FnMut | CallableKind::FnOnce
                        ) | (CallableKind::FnOnce, CallableKind::FnOnce)
                    );
                    if !compatible {
                        return Err(bad(
                            "opaque result callable kind is not valid in the original generic world",
                        ));
                    }
                    let child_definition = self.definition(child)?;
                    let SemanticDefinitionBody::FunctionWithBody {
                        signature: actual, ..
                    } = &child_definition.body
                    else {
                        return Err(bad("opaque result child lacks original code"));
                    };
                    if actual.parameters.len() != wanted.parameters.len()
                        || actual.bound_lifetimes != wanted.bound_lifetimes
                        || actual.invocation_lifetimes != wanted.invocation_lifetimes
                    {
                        return Err(pending(
                            "opaque callable result needs its complete scoped code alpha correspondence",
                        ));
                    }
                    for (a, b) in actual.parameters.iter().zip(&wanted.parameters) {
                        let a = self.resolve(child, &a.ty, 0)?;
                        let b = self.resolve(definition.id, &b.ty, 0)?;
                        self.same(&a, &b)?;
                    }
                    let a = self.resolve(child, &actual.result, 0)?;
                    let b = self.resolve(definition.id, &wanted.result, 0)?;
                    self.same(&a, &b)?;
                }
            }
        }
        self.active.remove(&definition.id);
        self.proof.complete.insert(definition.id);
        Ok(())
    }
}
pub(super) fn check(program: &SemanticModule, limits: &Limits, left: &mut usize) -> R<Proof> {
    let mut check = Check {
        program,
        limits,
        left,
        proof: Proof::default(),
        active: BTreeSet::new(),
        captures: BTreeMap::new(),
    };
    for definition in &program.definitions {
        if matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
                | SemanticDefinitionBody::Function { .. }
        ) {
            if definition.kind != SemanticDefinitionKind::Closure {
                check.function(definition, None)?;
            }
        }
    }
    for definition in &program.definitions {
        if matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
                | SemanticDefinitionBody::Function { .. }
        ) && !check.proof.complete.contains(&definition.id)
        {
            return Err(bad(
                "original code definition has no checked lexical creation context",
            ));
        }
    }
    Ok(check.proof)
}

impl Proof {
    pub(super) fn type_recipe(
        &self,
        program: &SemanticModule,
        current: SemanticDefinitionId,
        owner: SemanticDefinitionId,
        source: &SemanticOriginalTypeSource,
        limits: &Limits,
        left: &mut usize,
    ) -> R<SemanticTypeRecipe> {
        crate::dyn_metadata::semantic_operand_snapshot(source, limits, left, 1)?;
        let query = Query::from(source.clone());
        if !self.complete.contains(&owner) {
            return Err(bad(
                "original query owner has no complete symbolic body proof",
            ));
        }
        let value = self
            .types
            .get(&(owner, query))
            .ok_or_else(|| bad("original type query has no derived symbolic solution"))?;
        if owner != current {
            *left = left.checked_sub(program.definitions.len()).ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "original query rehoming work exceeded", None)
            })?;
            let definition = program
                .definitions
                .iter()
                .find(|d| d.id == current)
                .ok_or_else(|| bad("original query current definition is absent"))?;
            if definition.parent != Some(owner) {
                return Err(pending(
                    "original query crosses unproved lexical environment boundaries",
                ));
            }
            for (slot, formal) in definition.environment.iter().enumerate() {
                if matches!(&formal.source, SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: expected }) if expected == source)
                {
                    return Ok(SemanticTypeRecipe::Environment {
                        scope: SemanticScopeRef::Ancestor(0),
                        ordinal: slot as u32,
                    });
                }
            }
            if !matches!(value, SemanticTypeRecipe::Atom(_)) {
                return Err(pending(
                    "original parent type requires complete checked environment sealing",
                ));
            }
        }
        crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
        Ok(value.clone())
    }
    pub(super) fn call_actual(
        &self,
        owner: SemanticDefinitionId,
        call: SemanticSiteId,
        formal: u32,
        limits: &Limits,
        left: &mut usize,
    ) -> R<SemanticActualRecipe> {
        if !self.complete.contains(&owner) {
            return Err(bad(
                "original inference owner has no complete symbolic proof",
            ));
        }
        let value = self
            .calls
            .get(&(owner, call))
            .and_then(|values| values.get(formal as usize))
            .ok_or_else(|| bad("original call/formal has no derived inference solution"))?;
        crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
        Ok(value.clone())
    }
}
