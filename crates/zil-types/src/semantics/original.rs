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
    pub intrinsics: BTreeMap<(SemanticDefinitionId, SemanticSiteId), IntrinsicCall>,
    pub scope_calls: BTreeMap<
        (SemanticDefinitionId, SemanticSiteId, SemanticDefinitionId),
        Vec<SemanticActualRecipe>,
    >,
    environments: BTreeMap<(SemanticDefinitionId, u32), (SemanticDefinitionId, SemanticTypeRecipe)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum IntrinsicKind {
    MutexCreate,
    MutexLock,
    MutexValue,
    MutexUnlock,
    BytesNew,
    BytesReserve,
    BytesTruncate,
    BytesPush,
    BytesLen,
    BytesCapacity,
    BytesBorrow {
        mutable: bool,
    },
    Length,
    Numeric {
        target: Type,
    },
    ThreadScope {
        worker: SemanticDefinitionId,
        body: SemanticDefinitionId,
    },
}
#[derive(Clone)]
pub(super) struct IntrinsicCall {
    pub kind: IntrinsicKind,
    pub parameters: Vec<SemanticTypeRecipe>,
    pub result: SemanticTypeRecipe,
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
    loops: Vec<LoopFrame>,
}
#[derive(Default)]
struct LoopFrame {
    breaks: Vec<BTreeMap<String, Binding>>,
    continues: Vec<BTreeMap<String, Binding>>,
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
            SemanticTypeRecipe::Array { element, .. } => {
                self.copy_type(owner, element, depth + 1)?
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                self.copy_type(owner, error, depth + 1)?
                    && self.copy_type(owner, result, depth + 1)?
            }
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
                    SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Enum,
                        body: SemanticNominalBody::Enum { variants },
                        ..
                    } => {
                        let mut all = true;
                        for variant in variants {
                            for field in &variant.fields {
                                let field = super::instantiate::ty(
                                    field,
                                    arguments,
                                    self.limits,
                                    self.left,
                                )?;
                                all &= self.copy_type(owner, &field, depth + 1)?;
                            }
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
        if let SemanticTypeRecipe::Address { inner, .. }
        | SemanticTypeRecipe::Desc { inner, .. }
        | SemanticTypeRecipe::Managed(inner) = &ty
        {
            return self.field(owner, inner, name, depth + 1);
        }
        if let SemanticTypeRecipe::Tuple(fields) = &ty {
            let ordinal = name
                .parse::<usize>()
                .map_err(|_| bad("original tuple field is not an ordinal"))?;
            return copy_recipe(
                fields
                    .get(ordinal)
                    .ok_or_else(|| bad("original tuple field is absent"))?,
                self.limits,
                self.left,
            );
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
    // Only declaration paths are resolved here. A lexical value at the root
    // always wins, including when it shadows an imported module or intrinsic.
    fn declaration_path(
        &mut self,
        expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<Option<Vec<String>>> {
        self.pay(1, depth)?;
        match &expression.kind {
            CoreExprKind::Name(name) if !variables.contains_key(name) => {
                self.pay(name.len().saturating_add(1), depth)?;
                Ok(Some(vec![name.clone()]))
            }
            CoreExprKind::Member { object, member, .. } => {
                let Some(mut path) = self.declaration_path(object, variables, depth + 1)? else {
                    return Ok(None);
                };
                self.pay(member.len().saturating_add(1), depth)?;
                path.push(member.clone());
                Ok(Some(path))
            }
            CoreExprKind::TypeApply { callee, .. }
            | CoreExprKind::GenericApply { callee, .. }
            | CoreExprKind::Group(callee) => self.declaration_path(callee, variables, depth + 1),
            _ => Ok(None),
        }
    }
    fn declaration_target(
        &mut self,
        owner: SemanticDefinitionId,
        path: &[String],
        depth: usize,
    ) -> R<Option<SemanticDefinitionId>> {
        let current = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody { names, .. } = &current.body else {
            return Err(bad(
                "original declaration lookup has no complete lexical body",
            ));
        };
        self.pay(names.len().saturating_add(path.len()), depth)?;
        let mut selected = None;
        for binding in names {
            if binding.namespace != SemanticOriginalNamespace::Value || binding.path != path {
                continue;
            }
            if selected.is_some() {
                return Err(bad("original path has ambiguous declaration bindings"));
            }
            let SemanticOriginalNameValue::Definition(target) = binding.value else {
                return Err(pending(
                    "original callee recipe requires its complete callable interpretation",
                ));
            };
            selected = Some(target);
        }
        Ok(selected)
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
            SemanticTypeRecipe::Address { inner, .. }
            | SemanticTypeRecipe::Desc { inner, .. }
            | SemanticTypeRecipe::Slice(inner)
            | SemanticTypeRecipe::Managed(inner)
            | SemanticTypeRecipe::OwnedStore(inner)
            | SemanticTypeRecipe::Mutex(inner)
            | SemanticTypeRecipe::Registration(inner)
            | SemanticTypeRecipe::AsyncRegistration(inner) => {
                **inner = self.resolve(owner, inner, depth + 1)?
            }
            SemanticTypeRecipe::Tuple(fields) | SemanticTypeRecipe::AnonymousSum(fields) => {
                for field in fields {
                    *field = self.resolve(owner, field, depth + 1)?;
                }
            }
            SemanticTypeRecipe::Array { element, .. } => {
                **element = self.resolve(owner, element, depth + 1)?
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                **error = self.resolve(owner, error, depth + 1)?;
                **result = self.resolve(owner, result, depth + 1)?;
            }
            SemanticTypeRecipe::Nominal { arguments, .. }
            | SemanticTypeRecipe::Closure { arguments, .. } => {
                for actual in arguments {
                    *actual = self.resolved_actual(owner, actual, depth + 1)?;
                }
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
    fn observation_type(
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
            let (parent, actual) = self
                .proof
                .environments
                .get(&(owner, *ordinal))
                .ok_or_else(|| bad("original observation has no sealed lexical Type source"))?;
            let parent = *parent;
            let actual = copy_recipe(actual, self.limits, self.left)?;
            let actual = self.observation_type(parent, &actual, depth + 1)?;
            let child = self.definition(owner)?;
            return self.rehome(parent, child, &actual, depth + 1);
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
                        | SemanticSiteRecipeKind::OriginalCall
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
        let mut result: Vec<Option<SemanticActualRecipe>> = vec![None; formals.len()];
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
                let observed = self.observation_type(owner, &value.ty, depth + 1)?;
                if let SemanticTypeRecipe::Managed(inner) = &observed {
                    let inner = copy_recipe(inner, self.limits, self.left)?;
                    SemanticTypeRecipe::Desc {
                        lifetime: self.borrow_life(owner, expression.id)?,
                        inner: Box::new(inner),
                    }
                } else {
                    let reusable_descriptor = matches!(&value.ty, SemanticTypeRecipe::Desc { .. });
                    if !reusable_descriptor && !self.copy_type(owner, &value.ty, depth + 1)? {
                        if value.borrowed {
                            return Err(bad(
                                "original body moves a non-Copy value from a shared capture",
                            ));
                        }
                        value.used = true;
                    }
                    copy_recipe(&value.ty, self.limits, self.left)?
                }
            }
            CoreExprKind::Group(value) => self.expr(owner, value, variables, wanted, depth + 1)?,
            CoreExprKind::String(spelling) => {
                // Decode validation and byte emission are independently performed
                // by the real UTF8 instruction producer. The original type is
                // always the exact language static byte-slice address scheme.
                self.pay(spelling.len(), depth)?;
                SemanticTypeRecipe::Address {
                    mutable: false,
                    lifetime: SemanticOuterLifeRecipe::Static,
                    inner: Box::new(SemanticTypeRecipe::Slice(Box::new(
                        SemanticTypeRecipe::Atom(Type::U8),
                    ))),
                }
            }
            CoreExprKind::Array(fields) => {
                self.pay(fields.len().saturating_add(3), depth)?;
                let expected = match wanted {
                    Some(SemanticTypeRecipe::Array { element, .. }) => Some(element.as_ref()),
                    _ => None,
                };
                let mut element = expected
                    .map(|ty| copy_recipe(ty, self.limits, self.left))
                    .transpose()?;
                for field in fields {
                    let actual = self.expr(owner, field, variables, element.as_ref(), depth + 1)?;
                    if let Some(element) = &element {
                        self.same(element, &actual)?;
                    } else {
                        element = Some(actual);
                    }
                }
                let element = element.ok_or_else(|| {
                    bad("original empty array requires a complete element type context")
                })?;
                let length = u32::try_from(fields.len())
                    .map_err(|_| bad("original array exceeds its length domain"))?;
                SemanticTypeRecipe::Array {
                    element: Box::new(element),
                    length: Box::new(SemanticConstRecipe::Value(Constant::Usize(u64::from(
                        length,
                    )))),
                }
            }
            CoreExprKind::Index { object, index } => {
                let object_ty = self.place(owner, object, variables, depth + 1)?;
                let element = self.index_type(owner, &object_ty, index, variables, depth + 1)?;
                if !self.copy_type(owner, &element, depth + 1)? {
                    return Err(bad(
                        "original indexed observation requires a whole-element Copy premise",
                    ));
                }
                element
            }
            CoreExprKind::Slice { object, start, end } => {
                let object_ty = self.place(owner, object, variables, depth + 1)?;
                let element = match object_ty {
                    SemanticTypeRecipe::Array { element, .. }
                    | SemanticTypeRecipe::Slice(element) => element,
                    SemanticTypeRecipe::Address { inner, .. } => match *inner {
                        SemanticTypeRecipe::Array { element, .. }
                        | SemanticTypeRecipe::Slice(element) => element,
                        _ => return Err(bad("original slice does not select an array or slice")),
                    },
                    _ => return Err(bad("original slice does not select an array or slice")),
                };
                for index in start.iter().chain(end.iter()) {
                    let ty = self.expr(
                        owner,
                        index,
                        variables,
                        Some(&SemanticTypeRecipe::Atom(Type::I64)),
                        depth + 1,
                    )?;
                    if !matches!(ty, SemanticTypeRecipe::Atom(ty) if ty.is_integer()) {
                        return Err(bad("original slice bound is not an integer"));
                    }
                }
                SemanticTypeRecipe::Slice(element)
            }
            CoreExprKind::NumericConvert { value, target }
            | CoreExprKind::IntegerConvert { value, target } => {
                let target = self.resolve(owner, target, depth + 1)?;
                let value = self.expr(owner, value, variables, None, depth + 1)?;
                if !matches!((&value, &target), (SemanticTypeRecipe::Atom(a), SemanticTypeRecipe::Atom(b)) if (a.is_integer() || a.is_float()) && (b.is_integer() || b.is_float()))
                {
                    return Err(bad(
                        "original numeric conversion requires primitive numeric endpoints",
                    ));
                }
                target
            }
            CoreExprKind::Try(value) => {
                let value = self.expr(owner, value, variables, None, depth + 1)?;
                let SemanticTypeRecipe::ErrorUnion { error, result } = value else {
                    return Err(bad("original propagation operand is not an error union"));
                };
                let definition = self.definition(owner)?;
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
                else {
                    unreachable!()
                };
                let function_result = self.resolve(owner, &signature.result, depth + 1)?;
                let SemanticTypeRecipe::ErrorUnion {
                    error: expected, ..
                } = function_result
                else {
                    return Err(bad(
                        "original propagation escapes a function without an error result",
                    ));
                };
                self.same(&error, &expected)?;
                *result
            }
            CoreExprKind::Unary {
                op: UnaryOp::Give,
                operand,
            } => {
                let CoreExprKind::Name(name) = &operand.kind else {
                    return Err(bad("original give requires an owning lexical place"));
                };
                let value = variables
                    .get_mut(name)
                    .ok_or_else(|| bad("original give has no lexical binder"))?;
                if value.used || !value.initialized || value.borrowed {
                    return Err(bad(
                        "original give transfers an unavailable or borrowed value",
                    ));
                }
                if !matches!(value.ty, SemanticTypeRecipe::Managed(_)) {
                    return Err(bad("original give requires an owning resource head"));
                }
                value.used = true;
                value.observed = true;
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
                if matches!(
                    &expression.kind,
                    CoreExprKind::Unary {
                        op: UnaryOp::BorrowMut,
                        ..
                    }
                ) {
                    self.writable(owner, operand, variables, depth + 1)?;
                }
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
                let value = self.place(owner, operand, variables, depth + 1)?;
                let SemanticTypeRecipe::Address { inner, .. } = value else {
                    return Err(bad("original dereference has no address type"));
                };
                if !self.copy_type(owner, &inner, depth + 1)? {
                    return Err(bad(
                        "original dereference cannot transfer a non-Copy pointee",
                    ));
                }
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
                let mut left = self.clone_variables(variables)?;
                let mut right = self.clone_variables(variables)?;
                let a = self.expr(owner, then_value, &mut left, wanted, depth + 1)?;
                let b = self.expr(owner, else_value, &mut right, Some(&a), depth + 1)?;
                self.same(&a, &b)?;
                self.merge_variables(variables, &[left, right])?;
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
                if matches!(site.kind, SemanticSiteRecipeKind::OriginalCall) {
                    if let Some(result) =
                        self.intrinsic(owner, site.id, expression, variables, wanted, depth + 1)?
                    {
                        let copy = copy_recipe(&result, self.limits, self.left)?;
                        self.proof
                            .types
                            .insert((owner, Query::Expression(expression.id)), copy);
                        return Ok(result);
                    }
                }
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
                        if let Some(path) = self.declaration_path(callee, variables, depth + 1)? {
                            let target = self.declaration_target(owner, &path, depth + 1)?
                                .ok_or_else(|| pending("original call path lacks its real declaration import binding"))?;
                            let target_definition = self.definition(target)?;
                            self.pay(target_definition.parameters.len(), depth)?;
                            (
                                target,
                                vec![
                                    SemanticActualRecipe::Infer;
                                    target_definition.parameters.len()
                                ],
                            )
                        } else {
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
                                    "original callable requires its declared complete role premise",
                                ));
                            };
                            (definition, arguments)
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
    fn intrinsic(
        &mut self,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.pay(1, depth)?;
        let CoreExprKind::Call { callee, arguments } = &expression.kind else {
            unreachable!()
        };
        let path = self.declaration_path(callee, variables, depth + 1)?;
        if let Some(path) = &path {
            if self.declaration_target(owner, path, depth + 1)?.is_some() {
                return Ok(None);
            }
        }
        let mut base = callee.as_ref();
        while let CoreExprKind::GenericApply { callee, .. }
        | CoreExprKind::TypeApply { callee, .. } = &base.kind
        {
            self.pay(1, depth)?;
            base = callee;
        }
        // A builtin is selected only after exact lexical/declaration resolution;
        // a matching spelling hidden by a local remains an ordinary callable.
        let numeric = |name: &str| match name {
            "i8" => Some(Type::I8),
            "i16" => Some(Type::I16),
            "i32" => Some(Type::I32),
            "i64" => Some(Type::I64),
            "isize" => Some(Type::Isize),
            "u8" => Some(Type::U8),
            "u16" => Some(Type::U16),
            "u32" => Some(Type::U32),
            "u64" => Some(Type::U64),
            "usize" => Some(Type::Usize),
            "f32" => Some(Type::F32),
            "f64" => Some(Type::F64),
            _ => None,
        };
        let (kind, arity, receiver) = match (&base.kind, &path) {
            (CoreExprKind::Name(name), Some(path)) if path.len() == 1 => match name.as_str() {
                "mutex_create" => (IntrinsicKind::MutexCreate, 1, None),
                "mutex_lock" => (IntrinsicKind::MutexLock, 1, None),
                "mutex_value" => (IntrinsicKind::MutexValue, 1, None),
                "mutex_unlock" => (IntrinsicKind::MutexUnlock, 1, None),
                "bytes_new" => (IntrinsicKind::BytesNew, 0, None),
                "bytes_reserve" => (IntrinsicKind::BytesReserve, 2, None),
                "bytes_truncate" => (IntrinsicKind::BytesTruncate, 2, None),
                "bytes_push" => (IntrinsicKind::BytesPush, 2, None),
                "bytes_len" => (IntrinsicKind::BytesLen, 1, None),
                "bytes_capacity" => (IntrinsicKind::BytesCapacity, 1, None),
                "bytes_view" => (IntrinsicKind::BytesBorrow { mutable: false }, 1, None),
                "bytes_view_mut" => (IntrinsicKind::BytesBorrow { mutable: true }, 1, None),
                "thread_scope" => {
                    if arguments.len() != 3 {
                        return Err(bad(
                            "original thread_scope changes its three-operand scheme",
                        ));
                    }
                    let worker_path = self
                        .declaration_path(&arguments[0], variables, depth + 1)?
                        .ok_or_else(|| {
                            bad("original thread worker is not a lexical static declaration")
                        })?;
                    let worker = self
                        .declaration_target(owner, &worker_path, depth + 1)?
                        .ok_or_else(|| bad("original thread worker has no real declaration"))?;
                    let worker_definition = self.definition(worker)?;
                    if !worker_definition.parameters.is_empty()
                        || worker_definition.kind != SemanticDefinitionKind::Function
                    {
                        return Err(bad(
                            "original thread worker must have a concrete ordinary declaration",
                        ));
                    }
                    if !self.proof.complete.contains(&worker) && !self.active.contains(&worker) {
                        self.function(worker_definition, None)?;
                    }
                    let SemanticDefinitionBody::FunctionWithBody {
                        signature,
                        body: SemanticOriginalFunctionBody::Defined(_),
                        ..
                    } = &worker_definition.body
                    else {
                        return Err(bad("original thread worker must have a real language body"));
                    };
                    if signature.parameters.len() != 1 || signature.call_kind != CallKind::Direct {
                        return Err(bad(
                            "original thread worker has an incompatible call scheme",
                        ));
                    }
                    self.same(&signature.result, &SemanticTypeRecipe::Atom(Type::I32))?;
                    let payload = self.expr(
                        owner,
                        &arguments[1],
                        variables,
                        Some(&signature.parameters[0].ty),
                        depth + 1,
                    )?;
                    self.same(&payload, &signature.parameters[0].ty)?;
                    let body_ty = self.expr(owner, &arguments[2], variables, None, depth + 1)?;
                    let SemanticTypeRecipe::Closure {
                        definition: body, ..
                    } = &body_ty
                    else {
                        return Err(bad("original thread body is not a concrete closure"));
                    };
                    let body_definition = self.definition(*body)?;
                    let SemanticDefinitionBody::FunctionWithBody {
                        signature: body_signature,
                        ..
                    } = &body_definition.body
                    else {
                        return Err(bad("original thread body omits its complete scheme"));
                    };
                    if !body_signature.parameters.is_empty() {
                        return Err(bad("original thread body requires a zero-argument scheme"));
                    }
                    let body_result = self.resolve(*body, &body_signature.result, depth + 1)?;
                    self.same(&body_result, &SemanticTypeRecipe::Atom(Type::I32))?;
                    let result = SemanticTypeRecipe::Tuple(vec![
                        SemanticTypeRecipe::Atom(Type::Bool),
                        SemanticTypeRecipe::Atom(Type::I32),
                        SemanticTypeRecipe::Atom(Type::I32),
                    ]);
                    let kind = IntrinsicKind::ThreadScope {
                        worker,
                        body: *body,
                    };
                    self.record_intrinsic(
                        owner,
                        site,
                        kind,
                        vec![payload, body_ty],
                        &result,
                        wanted,
                        depth + 1,
                    )?;
                    return Ok(Some(result));
                }
                name => match numeric(name) {
                    Some(target) => (IntrinsicKind::Numeric { target }, 1, None),
                    None => return Ok(None),
                },
            },
            (CoreExprKind::Member { object, member, .. }, None) if member == "len" => {
                (IntrinsicKind::Length, 0, Some(object.as_ref()))
            }
            _ => return Ok(None),
        };
        if base.id != callee.id {
            return Err(bad(
                "original sealed intrinsic has no written generic argument scheme",
            ));
        }
        if arguments.len() != arity {
            return Err(bad("original intrinsic changes its declared operand arity"));
        }
        self.pay(arguments.len().saturating_add(8), depth)?;
        let mut parameters = Vec::new();
        if let Some(receiver) = receiver {
            parameters.push(self.place(owner, receiver, variables, depth + 1)?);
        }
        for (ordinal, argument) in arguments.iter().enumerate() {
            let expected = match (&kind, ordinal) {
                (IntrinsicKind::BytesReserve | IntrinsicKind::BytesTruncate, 1) => {
                    Some(SemanticTypeRecipe::Atom(Type::I64))
                }
                (IntrinsicKind::BytesPush, 1) => Some(SemanticTypeRecipe::Atom(Type::U8)),
                _ => None,
            };
            parameters.push(self.expr(owner, argument, variables, expected.as_ref(), depth + 1)?);
        }
        let result = match &kind {
            IntrinsicKind::MutexCreate => {
                let payload = copy_recipe(&parameters[0], self.limits, self.left)?;
                if !self.ordinary_value(owner, &payload, depth + 1)? {
                    return Err(bad(
                        "original mutex payload lacks its complete ordinary Value premise",
                    ));
                }
                SemanticTypeRecipe::ErrorUnion {
                    error: Box::new(SemanticTypeRecipe::Exact(Box::new(
                        SemanticExactTypeRecipe {
                            ty: crate::generic_mutex::create_error(),
                            life: StorageLifeShape {
                                binder_count: 0,
                                slots: Vec::new(),
                                retained: Vec::new(),
                                nominal_arguments: Vec::new(),
                            },
                            classes: Vec::new(),
                        },
                    ))),
                    result: Box::new(SemanticTypeRecipe::Managed(Box::new(
                        SemanticTypeRecipe::Mutex(Box::new(payload)),
                    ))),
                }
            }
            IntrinsicKind::MutexLock => {
                let SemanticTypeRecipe::Desc {
                    lifetime: SemanticOuterLifeRecipe::Class(lifetime),
                    inner,
                } = &parameters[0]
                else {
                    return Err(bad(
                        "original mutex lock requires a real scoped head descriptor",
                    ));
                };
                let SemanticTypeRecipe::Mutex(payload) = inner.as_ref() else {
                    return Err(bad("original mutex lock changes the sealed head kind"));
                };
                crate::dyn_metadata::semantic_operand_snapshot(
                    lifetime,
                    self.limits,
                    self.left,
                    1,
                )?;
                SemanticTypeRecipe::MutexGuard {
                    payload: Box::new(copy_recipe(payload, self.limits, self.left)?),
                    retained: Box::new(SemanticRetainedShapeRecipe {
                        domain: SemanticClassSequenceRecipe {
                            parts: vec![SemanticClassSequencePart::One(lifetime.clone())],
                        },
                        ports: vec![SemanticPortPart::One {
                            leaf: Box::new(copy_recipe(inner, self.limits, self.left)?),
                            mode: StoredBorrowMode::SharedAddress,
                            class: lifetime.clone(),
                        }],
                        retained: Vec::new(),
                        nominal: Vec::new(),
                    }),
                }
            }
            IntrinsicKind::MutexValue => {
                let SemanticTypeRecipe::Address {
                    mutable: true,
                    lifetime,
                    inner,
                } = &parameters[0]
                else {
                    return Err(bad(
                        "original mutex value requires an exclusive guard borrow",
                    ));
                };
                let SemanticTypeRecipe::MutexGuard { payload, .. } = inner.as_ref() else {
                    return Err(bad("original mutex value changes its sealed guard kind"));
                };
                SemanticTypeRecipe::Address {
                    mutable: true,
                    lifetime: lifetime.clone(),
                    inner: Box::new(copy_recipe(payload, self.limits, self.left)?),
                }
            }
            IntrinsicKind::MutexUnlock => {
                if !matches!(&parameters[0], SemanticTypeRecipe::MutexGuard { .. }) {
                    return Err(bad("original mutex unlock must consume a real guard"));
                }
                SemanticTypeRecipe::Atom(Type::Unit)
            }
            IntrinsicKind::BytesNew => {
                SemanticTypeRecipe::Managed(Box::new(SemanticTypeRecipe::Atom(Type::ByteBuffer)))
            }
            IntrinsicKind::BytesReserve
            | IntrinsicKind::BytesTruncate
            | IntrinsicKind::BytesPush
            | IntrinsicKind::BytesLen
            | IntrinsicKind::BytesCapacity
            | IntrinsicKind::BytesBorrow { .. } => {
                let SemanticTypeRecipe::Desc { lifetime, inner } = &parameters[0] else {
                    return Err(bad("original bytes operation requires a scoped descriptor"));
                };
                self.same(inner, &SemanticTypeRecipe::Atom(Type::ByteBuffer))?;
                if parameters.len() == 2 {
                    self.same(
                        &parameters[1],
                        &SemanticTypeRecipe::Atom(if kind == IntrinsicKind::BytesPush {
                            Type::U8
                        } else {
                            Type::I64
                        }),
                    )?;
                }
                match &kind {
                    IntrinsicKind::BytesLen | IntrinsicKind::BytesCapacity => {
                        SemanticTypeRecipe::Atom(Type::I64)
                    }
                    IntrinsicKind::BytesBorrow { mutable } => SemanticTypeRecipe::Address {
                        mutable: *mutable,
                        lifetime: lifetime.clone(),
                        inner: Box::new(SemanticTypeRecipe::Slice(Box::new(
                            SemanticTypeRecipe::Atom(Type::U8),
                        ))),
                    },
                    _ => SemanticTypeRecipe::Atom(Type::Bool),
                }
            }
            IntrinsicKind::Length => {
                let mut value = &parameters[0];
                while let SemanticTypeRecipe::Address { inner, .. } = value {
                    self.pay(1, depth)?;
                    value = inner;
                }
                if !matches!(
                    value,
                    SemanticTypeRecipe::Array { .. } | SemanticTypeRecipe::Slice(_)
                ) {
                    return Err(bad(
                        "original len receiver is not an array or borrowed slice",
                    ));
                }
                SemanticTypeRecipe::Atom(Type::I64)
            }
            IntrinsicKind::Numeric { target } => {
                if !matches!(&parameters[0], SemanticTypeRecipe::Atom(ty) if ty.is_integer() || ty.is_float())
                {
                    return Err(bad(
                        "original numeric intrinsic requires a primitive numeric input",
                    ));
                }
                SemanticTypeRecipe::Atom(target.clone())
            }
            IntrinsicKind::ThreadScope { .. } => unreachable!(),
        };
        self.record_intrinsic(owner, site, kind, parameters, &result, wanted, depth + 1)?;
        Ok(Some(result))
    }
    fn record_intrinsic(
        &mut self,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        kind: IntrinsicKind,
        parameters: Vec<SemanticTypeRecipe>,
        result: &SemanticTypeRecipe,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<()> {
        self.pay(2, depth)?;
        if let Some(wanted) = wanted {
            self.same(result, wanted)?;
        }
        let result = copy_recipe(result, self.limits, self.left)?;
        self.proof.intrinsics.insert(
            (owner, site),
            IntrinsicCall {
                kind,
                parameters,
                result,
            },
        );
        self.proof.calls.insert((owner, site), Vec::new());
        Ok(())
    }
    fn merge_variables(
        &mut self,
        output: &mut BTreeMap<String, Binding>,
        branches: &[BTreeMap<String, Binding>],
    ) -> R<()> {
        self.pay(output.len().saturating_mul(branches.len()), 0)?;
        for (name, value) in output {
            let mut initialized = !branches.is_empty();
            for branch in branches {
                let inner = branch
                    .get(name)
                    .filter(|inner| inner.origin == value.origin)
                    .ok_or_else(|| {
                        bad("original control edge loses its lexical binder identity")
                    })?;
                self.same(&value.ty, &inner.ty)?;
                value.used |= inner.used;
                value.observed |= inner.observed;
                value.written |= inner.written;
                initialized &= inner.initialized;
            }
            value.initialized = initialized;
        }
        Ok(())
    }
    fn writable(
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
                    .ok_or_else(|| bad("original assignment has no lexical target"))?;
                if !value.mutable {
                    return Err(bad("original assignment targets an immutable binder"));
                }
                value.written = true;
                copy_recipe(&value.ty, self.limits, self.left)
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let address = self.place(owner, operand, variables, depth + 1)?;
                let SemanticTypeRecipe::Address {
                    mutable: true,
                    inner,
                    ..
                } = address
                else {
                    return Err(bad("original write requires an exclusive address"));
                };
                Ok(*inner)
            }
            CoreExprKind::Member { object, member, .. } => {
                let object_ty = self.place(owner, object, variables, depth + 1)?;
                match &object_ty {
                    SemanticTypeRecipe::Address { mutable: true, .. } => {}
                    SemanticTypeRecipe::Address { mutable: false, .. } => {
                        return Err(bad("original field write uses a shared address"))
                    }
                    _ => {
                        self.writable(owner, object, variables, depth + 1)?;
                    }
                }
                self.field(owner, &object_ty, member, depth + 1)
            }
            CoreExprKind::Slice { object, .. } => {
                let object_ty = self.place(owner, object, variables, depth + 1)?;
                match &object_ty {
                    SemanticTypeRecipe::Address { mutable: true, .. } => {}
                    SemanticTypeRecipe::Address { mutable: false, .. } => {
                        return Err(bad("original slice write uses a shared address"))
                    }
                    _ => {
                        self.writable(owner, object, variables, depth + 1)?;
                    }
                }
                self.expr(owner, expression, variables, None, depth + 1)
            }
            CoreExprKind::Index { object, index } => {
                let object_ty = self.place(owner, object, variables, depth + 1)?;
                match &object_ty {
                    SemanticTypeRecipe::Address { mutable: true, .. } => {}
                    SemanticTypeRecipe::Address { mutable: false, .. } => {
                        return Err(bad("original index write uses a shared address"))
                    }
                    _ => {
                        self.writable(owner, object, variables, depth + 1)?;
                    }
                }
                self.index_type(owner, &object_ty, index, variables, depth + 1)
            }
            _ => Err(bad("original assignment does not select a writable place")),
        }
    }
    fn index_type(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        index: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        let index_ty = self.expr(
            owner,
            index,
            variables,
            Some(&SemanticTypeRecipe::Atom(Type::I64)),
            depth + 1,
        )?;
        let SemanticTypeRecipe::Atom(index_ty) = index_ty else {
            return Err(bad("original index is not a primitive integer"));
        };
        if !index_ty.is_integer() {
            return Err(bad("original index is not an integer"));
        }
        let mut ty = self.logical(owner, ty, depth + 1)?;
        while let SemanticTypeRecipe::Address { inner, .. } = ty {
            self.pay(1, depth)?;
            ty = *inner;
        }
        match ty {
            SemanticTypeRecipe::Array { element, .. } | SemanticTypeRecipe::Slice(element) => {
                Ok(*element)
            }
            _ => Err(bad(
                "original indexing requires a complete array or borrowed slice",
            )),
        }
    }
    fn pattern(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        path: &mut Vec<u32>,
        pattern: &SemanticOriginalPattern,
        ty: &SemanticTypeRecipe,
        mutable: bool,
        variables: &mut BTreeMap<String, Binding>,
        names: &mut BTreeSet<String>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                self.pay(name.len().saturating_add(path.len()), depth)?;
                if !names.insert(name.clone()) {
                    return Err(bad("original pattern repeats a lexical binder"));
                }
                let origin = Query::BindingProjection {
                    statement,
                    pattern: path.clone(),
                };
                let copy = copy_recipe(ty, self.limits, self.left)?;
                self.proof.types.insert((owner, origin.clone()), copy);
                variables.insert(
                    name.clone(),
                    Binding {
                        ty: copy_recipe(ty, self.limits, self.left)?,
                        mutable,
                        used: false,
                        observed: false,
                        capture: false,
                        borrowed: false,
                        origin,
                        initialized: true,
                        written: false,
                    },
                );
            }
            CorePatternKind::Wildcard => {}
            CorePatternKind::Tuple(patterns) => {
                let ty = self.logical(owner, ty, depth + 1)?;
                let SemanticTypeRecipe::Tuple(fields) = ty else {
                    return Err(bad("original tuple pattern changes subject shape"));
                };
                if patterns.len() != fields.len() {
                    return Err(bad("original tuple pattern changes subject arity"));
                }
                for (ordinal, (pattern, ty)) in patterns.iter().zip(&fields).enumerate() {
                    self.pay(1, depth)?;
                    path.push(ordinal as u32);
                    self.pattern(
                        owner,
                        statement,
                        path,
                        pattern,
                        ty,
                        mutable,
                        variables,
                        names,
                        depth + 1,
                    )?;
                    path.pop();
                }
            }
            CorePatternKind::Reference {
                mutable: requested,
                pattern,
            } => {
                let ty = self.logical(owner, ty, depth + 1)?;
                let SemanticTypeRecipe::Address {
                    mutable: available,
                    inner,
                    ..
                } = ty
                else {
                    return Err(bad("original reference pattern has no borrowed subject"));
                };
                if *requested && !available {
                    return Err(bad("original pattern strengthens shared access"));
                }
                self.pay(1, depth)?;
                path.push(0);
                self.pattern(
                    owner,
                    statement,
                    path,
                    pattern,
                    &inner,
                    mutable,
                    variables,
                    names,
                    depth + 1,
                )?;
                path.pop();
            }
            CorePatternKind::ErrorArm { is_error, pattern } => {
                let ty = self.logical(owner, ty, depth + 1)?;
                let SemanticTypeRecipe::ErrorUnion { error, result } = ty else {
                    return Err(bad(
                        "original error pattern requires the actual error union",
                    ));
                };
                self.pay(1, depth)?;
                path.push(0);
                self.pattern(
                    owner,
                    statement,
                    path,
                    pattern,
                    if *is_error { &error } else { &result },
                    mutable,
                    variables,
                    names,
                    depth + 1,
                )?;
                path.pop();
            }
            CorePatternKind::Record {
                ty: annotation,
                fields,
            } => {
                let annotation = self.resolve(owner, annotation, depth + 1)?;
                self.same(ty, &annotation)?;
                self.record_pattern(
                    owner,
                    statement,
                    path,
                    fields,
                    &annotation,
                    mutable,
                    variables,
                    names,
                    depth + 1,
                )?;
            }
            CorePatternKind::RecordConstructor { definition, fields } => {
                let ty = self.logical(owner, ty, depth + 1)?;
                if let Some(definition) = definition {
                    if !matches!(&ty, SemanticTypeRecipe::Nominal { definition: actual, .. } if definition == actual)
                    {
                        return Err(bad("original record pattern changes nominal identity"));
                    }
                }
                self.record_pattern(
                    owner,
                    statement,
                    path,
                    fields,
                    &ty,
                    mutable,
                    variables,
                    names,
                    depth + 1,
                )?;
            }
            CorePatternKind::Variant {
                ty: annotation,
                variant,
                fields,
            } => {
                let annotation = self.resolve(owner, annotation, depth + 1)?;
                self.same(ty, &annotation)?;
                self.variant_pattern(
                    owner,
                    statement,
                    path,
                    variant,
                    fields,
                    &annotation,
                    mutable,
                    variables,
                    names,
                    depth + 1,
                )?;
            }
            CorePatternKind::Constructor {
                definition,
                variant,
                fields,
            } => {
                let ty = self.logical(owner, ty, depth + 1)?;
                if let Some(definition) = definition {
                    if !matches!(&ty, SemanticTypeRecipe::Nominal { definition: actual, .. } if actual == definition)
                    {
                        return Err(bad("original variant pattern changes nominal identity"));
                    }
                }
                self.variant_pattern(
                    owner,
                    statement,
                    path,
                    variant,
                    fields,
                    &ty,
                    mutable,
                    variables,
                    names,
                    depth + 1,
                )?;
            }
            CorePatternKind::Bool(_) => self.same(ty, &SemanticTypeRecipe::Atom(Type::Bool))?,
            CorePatternKind::Integer(text) => {
                let SemanticTypeRecipe::Atom(expected) = ty else {
                    return Err(bad("original integer pattern has a nonprimitive subject"));
                };
                if !expected.is_integer() {
                    return Err(bad("original integer pattern has a noninteger subject"));
                }
                integer(text, Some(expected), false, pattern.span)?;
            }
            CorePatternKind::Float(text) => {
                let SemanticTypeRecipe::Atom(expected) = ty else {
                    return Err(bad("original float pattern has a nonprimitive subject"));
                };
                if !expected.is_float() {
                    return Err(bad("original float pattern has a nonfloat subject"));
                }
                floats::literal(text, Some(expected), pattern.span)?;
            }
            CorePatternKind::String(_) | CorePatternKind::Character(_) => {
                return Err(pending(
                    "original text pattern requires its checked comparison scheme",
                ));
            }
        }
        Ok(())
    }
    fn record_pattern(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        path: &mut Vec<u32>,
        fields: &[(String, SemanticOriginalPattern)],
        ty: &SemanticTypeRecipe,
        mutable: bool,
        variables: &mut BTreeMap<String, Binding>,
        names: &mut BTreeSet<String>,
        depth: usize,
    ) -> R<()> {
        let SemanticTypeRecipe::Nominal {
            definition,
            arguments: _,
        } = ty
        else {
            return Err(bad("original record pattern requires a nominal product"));
        };
        let definition = self.definition(*definition)?;
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Product { fields: declared },
            ..
        } = &definition.body
        else {
            return Err(bad("original record pattern selects another constructor"));
        };
        if fields.len() != declared.len() {
            return Err(bad("original record pattern omits declared fields"));
        }
        self.pay(fields.len(), depth)?;
        let mut labels = BTreeSet::new();
        for (ordinal, (name, pattern)) in fields.iter().enumerate() {
            self.pay(name.len(), depth)?;
            if !labels.insert(name.clone()) {
                return Err(bad("original record pattern repeats a field"));
            }
            let field = self.field(owner, ty, name, depth + 1)?;
            self.pay(1, depth)?;
            path.push(ordinal as u32);
            self.pattern(
                owner,
                statement,
                path,
                pattern,
                &field,
                mutable,
                variables,
                names,
                depth + 1,
            )?;
            path.pop();
        }
        Ok(())
    }
    fn variant_pattern(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        path: &mut Vec<u32>,
        name: &str,
        patterns: &[SemanticOriginalPattern],
        ty: &SemanticTypeRecipe,
        mutable: bool,
        variables: &mut BTreeMap<String, Binding>,
        names: &mut BTreeSet<String>,
        depth: usize,
    ) -> R<()> {
        let SemanticTypeRecipe::Nominal {
            definition,
            arguments,
        } = ty
        else {
            return Err(bad("original variant pattern requires its nominal sum"));
        };
        let definition = self.definition(*definition)?;
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Enum { variants },
            ..
        } = &definition.body
        else {
            return Err(bad(
                "original variant pattern selects another declaration kind",
            ));
        };
        self.pay(variants.len(), depth)?;
        let variant = variants
            .iter()
            .find(|v| v.name == name)
            .ok_or_else(|| bad("original variant pattern names an absent constructor"))?;
        if patterns.len() != variant.fields.len() {
            return Err(bad("original variant pattern changes payload arity"));
        }
        for (ordinal, (pattern, field)) in patterns.iter().zip(&variant.fields).enumerate() {
            let field = super::instantiate::ty(field, arguments, self.limits, self.left)?;
            self.pay(1, depth)?;
            path.push(ordinal as u32);
            self.pattern(
                owner,
                statement,
                path,
                pattern,
                &field,
                mutable,
                variables,
                names,
                depth + 1,
            )?;
            path.pop();
        }
        Ok(())
    }
    fn loop_body(
        &mut self,
        owner: SemanticDefinitionId,
        body: &SemanticOriginalBlock,
        variables: &mut BTreeMap<String, Binding>,
        result: &SemanticTypeRecipe,
        may_skip: bool,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        let entry = self.clone_variables(variables)?;
        let mut inner = self.clone_variables(variables)?;
        self.loops.push(LoopFrame::default());
        let falls = self.block(owner, body, &mut inner, result, depth + 1)?;
        let mut frame = self
            .loops
            .pop()
            .ok_or_else(|| bad("original loop frame disappeared"))?;
        if falls {
            self.pay(1, depth)?;
            frame.continues.push(inner);
        }
        // Every actual back edge must be usable again in the same rigid Gamma.
        // A later iteration cannot restore a value transferred on this edge.
        for edge in &frame.continues {
            for (name, before) in &entry {
                self.pay(1, depth)?;
                let after = edge
                    .get(name)
                    .filter(|after| after.origin == before.origin)
                    .ok_or_else(|| bad("original loop edge changes its outer lexical binder"))?;
                self.same(&before.ty, &after.ty)?;
                if !before.used && after.used || before.initialized && !after.initialized {
                    return Err(bad("original loop back edge consumes an affine value needed by another iteration"));
                }
            }
        }
        // Condition-false exits may happen after zero or any number of iterations.
        if may_skip {
            self.pay(1, depth)?;
            frame.breaks.push(entry);
            for edge in frame.continues {
                self.pay(1, depth)?;
                frame.breaks.push(edge);
            }
        }
        let falls = !frame.breaks.is_empty();
        if falls {
            self.merge_variables(variables, &frame.breaks)?;
        }
        Ok(falls)
    }
    fn block(
        &mut self,
        owner: SemanticDefinitionId,
        block: &SemanticOriginalBlock,
        variables: &mut BTreeMap<String, Binding>,
        result: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(block.statements.len(), depth)?;
        let mut live = true;
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
                    live = false;
                }
                CoreStatementKind::Return(None) => {
                    self.same(result, &SemanticTypeRecipe::Atom(Type::Unit))?;
                    live = false;
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
                    let mut branches = Vec::new();
                    self.pay(2, depth)?;
                    let mut left = self.clone_variables(variables)?;
                    if self.block(owner, then_body, &mut left, result, depth + 1)? {
                        branches.push(left);
                    }
                    let mut right = self.clone_variables(variables)?;
                    if let Some(body) = else_body {
                        if self.block(owner, body, &mut right, result, depth + 1)? {
                            branches.push(right);
                        }
                    } else {
                        branches.push(right);
                    }
                    if branches.is_empty() {
                        live = false;
                    } else {
                        self.merge_variables(variables, &branches)?;
                    }
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    let mut inner = self.clone_variables(variables)?;
                    if self.block(owner, body, &mut inner, result, depth + 1)? {
                        self.merge_variables(variables, &[inner])?;
                    } else {
                        live = false;
                    }
                }
                CoreStatementKind::Defer(body) => {
                    // Check the cleanup at its declaration environment. Reserving
                    // its affine transfers also prevents another transfer before exit.
                    let mut cleanup = self.clone_variables(variables)?;
                    let outer_loops = std::mem::take(&mut self.loops);
                    let cleanup_result = self.block(
                        owner,
                        body,
                        &mut cleanup,
                        &SemanticTypeRecipe::Atom(Type::Unit),
                        depth + 1,
                    );
                    self.loops = outer_loops;
                    if !cleanup_result? {
                        return Err(bad(
                            "original deferred cleanup escapes its own exit boundary",
                        ));
                    }
                    self.merge_variables(variables, &[cleanup])?;
                }
                CoreStatementKind::Assign { target, value }
                | CoreStatementKind::CompoundAssign { target, value, .. } => {
                    let expected = self.writable(owner, target, variables, depth + 1)?;
                    if matches!(&statement.kind, CoreStatementKind::CompoundAssign { .. }) {
                        let observed = self.place(owner, target, variables, depth + 1)?;
                        self.same(&expected, &observed)?;
                        let SemanticTypeRecipe::Atom(primitive) = &expected else {
                            return Err(bad(
                                "original compound assignment lacks a primitive endpoint premise",
                            ));
                        };
                        let CoreStatementKind::CompoundAssign { op, .. } = &statement.kind else {
                            unreachable!()
                        };
                        if !primitive.is_integer() && !primitive.is_float()
                            || matches!(
                                op,
                                BinaryOp::Equal
                                    | BinaryOp::NotEqual
                                    | BinaryOp::Less
                                    | BinaryOp::LessEqual
                                    | BinaryOp::Greater
                                    | BinaryOp::GreaterEqual
                                    | BinaryOp::And
                                    | BinaryOp::Or
                            )
                            || primitive.is_float()
                                && matches!(
                                    op,
                                    BinaryOp::BitAnd
                                        | BinaryOp::BitOr
                                        | BinaryOp::BitXor
                                        | BinaryOp::ShiftLeft
                                        | BinaryOp::ShiftRight
                                        | BinaryOp::AddWrapping
                                        | BinaryOp::SubtractWrapping
                                        | BinaryOp::MultiplyWrapping
                                )
                        {
                            return Err(bad(
                                "original compound assignment has an incompatible operator",
                            ));
                        }
                    }
                    let actual = self.expr(owner, value, variables, Some(&expected), depth + 1)?;
                    self.same(&actual, &expected)?;
                    if let CoreExprKind::Name(name) = &target.kind {
                        let target = variables
                            .get_mut(name)
                            .ok_or_else(|| bad("original assignment target disappeared"))?;
                        target.used = false;
                        target.initialized = true;
                        target.written = true;
                    }
                }
                CoreStatementKind::TupleBinding {
                    pattern,
                    mutable,
                    ty,
                    initializer,
                } => {
                    let annotation = ty
                        .as_ref()
                        .map(|ty| self.resolve(owner, ty, depth + 1))
                        .transpose()?;
                    let actual = self.expr(
                        owner,
                        initializer,
                        variables,
                        annotation.as_ref(),
                        depth + 1,
                    )?;
                    if let Some(annotation) = &annotation {
                        self.same(annotation, &actual)?;
                    }
                    let copy = copy_recipe(&actual, self.limits, self.left)?;
                    self.proof
                        .types
                        .insert((owner, Query::Binding(statement.id)), copy);
                    self.pattern(
                        owner,
                        statement.id,
                        &mut Vec::new(),
                        pattern,
                        &actual,
                        *mutable,
                        variables,
                        &mut BTreeSet::new(),
                        depth + 1,
                    )?;
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    let ty = self.expr(owner, value, variables, None, depth + 1)?;
                    self.pay(2, depth)?;
                    let mut branches = Vec::new();
                    let mut matched = self.clone_variables(variables)?;
                    self.pattern(
                        owner,
                        statement.id,
                        &mut Vec::new(),
                        pattern,
                        &ty,
                        false,
                        &mut matched,
                        &mut BTreeSet::new(),
                        depth + 1,
                    )?;
                    if self.block(owner, then_body, &mut matched, result, depth + 1)? {
                        branches.push(matched);
                    }
                    let mut other = self.clone_variables(variables)?;
                    if let Some(body) = else_body {
                        if self.block(owner, body, &mut other, result, depth + 1)? {
                            branches.push(other);
                        }
                    } else {
                        branches.push(other);
                    }
                    if branches.is_empty() {
                        live = false;
                    } else {
                        self.merge_variables(variables, &branches)?;
                    }
                }
                CoreStatementKind::Match { value, arms } => {
                    let ty = self.expr(owner, value, variables, None, depth + 1)?;
                    self.pay(arms.len(), depth)?;
                    let mut branches = Vec::new();
                    for (ordinal, arm) in arms.iter().enumerate() {
                        let mut arm_variables = self.clone_variables(variables)?;
                        self.pay(1, depth)?;
                        let mut path = vec![ordinal as u32];
                        self.pattern(
                            owner,
                            statement.id,
                            &mut path,
                            &arm.pattern,
                            &ty,
                            false,
                            &mut arm_variables,
                            &mut BTreeSet::new(),
                            depth + 1,
                        )?;
                        if self.block(owner, &arm.body, &mut arm_variables, result, depth + 1)? {
                            branches.push(arm_variables);
                        }
                    }
                    self.exhaustive(owner, &ty, arms, depth + 1)?;
                    if branches.is_empty() {
                        live = false;
                    } else {
                        self.merge_variables(variables, &branches)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    let condition = self.expr(
                        owner,
                        condition,
                        variables,
                        Some(&SemanticTypeRecipe::Atom(Type::Bool)),
                        depth + 1,
                    )?;
                    self.same(&condition, &SemanticTypeRecipe::Atom(Type::Bool))?;
                    self.loop_body(owner, body, variables, result, true, depth + 1)?;
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    let ty = self.expr(owner, value, variables, None, depth + 1)?;
                    let mut iteration = self.clone_variables(variables)?;
                    self.pattern(
                        owner,
                        statement.id,
                        &mut Vec::new(),
                        pattern,
                        &ty,
                        false,
                        &mut iteration,
                        &mut BTreeSet::new(),
                        depth + 1,
                    )?;
                    self.loop_body(owner, body, &mut iteration, result, true, depth + 1)?;
                    self.merge_variables(variables, &[iteration])?;
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    let ty = self.expr(owner, start, variables, None, depth + 1)?;
                    let end = self.expr(owner, end, variables, Some(&ty), depth + 1)?;
                    self.same(&ty, &end)?;
                    if !matches!(&ty, SemanticTypeRecipe::Atom(ty) if ty.is_integer()) {
                        return Err(bad(
                            "original range loop endpoints must be primitive integers",
                        ));
                    }
                    let mut iteration = self.clone_variables(variables)?;
                    self.bind_iteration(owner, statement.id, name, ty, &mut iteration, depth + 1)?;
                    self.loop_body(owner, body, &mut iteration, result, true, depth + 1)?;
                    self.merge_variables(variables, &[iteration])?;
                }
                CoreStatementKind::Loop(body) => {
                    if !self.loop_body(owner, body, variables, result, false, depth + 1)? {
                        live = false;
                    }
                }
                CoreStatementKind::Break | CoreStatementKind::Continue => {
                    if self.loops.is_empty() {
                        return Err(bad("original control transfer has no lexical loop target"));
                    }
                    let edge = self.clone_variables(variables)?;
                    self.pay(1, depth)?;
                    let frame = self
                        .loops
                        .last_mut()
                        .ok_or_else(|| bad("original loop control frame disappeared"))?;
                    if matches!(&statement.kind, CoreStatementKind::Break) {
                        frame.breaks.push(edge);
                    } else {
                        frame.continues.push(edge);
                    }
                    live = false;
                }
                CoreStatementKind::With { name, value, body } => {
                    let ty = self.expr(owner, value, variables, None, depth + 1)?;
                    let mut inner = self.clone_variables(variables)?;
                    self.bind_iteration(owner, statement.id, name, ty, &mut inner, depth + 1)?;
                    if self.block(owner, body, &mut inner, result, depth + 1)? {
                        self.merge_variables(variables, &[inner])?;
                    } else {
                        live = false;
                    }
                }
                CoreStatementKind::ForEach {
                    pattern,
                    iterator,
                    body,
                } => {
                    let ty = self.expr(owner, iterator, variables, None, depth + 1)?;
                    let ty = self.logical(owner, &ty, depth + 1)?;
                    let element = match ty {
                        SemanticTypeRecipe::Array { element, .. } => *element,
                        SemanticTypeRecipe::Address {
                            mutable,
                            lifetime,
                            inner,
                        } => {
                            let (SemanticTypeRecipe::Array { element, .. }
                            | SemanticTypeRecipe::Slice(element)) = *inner
                            else {
                                return Err(pending("original iteration requires its complete iterator protocol scheme"));
                            };
                            SemanticTypeRecipe::Address {
                                mutable,
                                lifetime,
                                inner: element,
                            }
                        }
                        _ => {
                            return Err(pending(
                                "original iteration requires its complete iterator protocol scheme",
                            ))
                        }
                    };
                    let mut iteration = self.clone_variables(variables)?;
                    self.pattern(
                        owner,
                        statement.id,
                        &mut Vec::new(),
                        pattern,
                        &element,
                        false,
                        &mut iteration,
                        &mut BTreeSet::new(),
                        depth + 1,
                    )?;
                    self.loop_body(owner, body, &mut iteration, result, true, depth + 1)?;
                    self.merge_variables(variables, &[iteration])?;
                }
                CoreStatementKind::CompileTime(_)
                | CoreStatementKind::Yield(_)
                | CoreStatementKind::Unsupported(_) => {
                    return Err(pending("original staged or resumable statement requires its executable stage scheme"));
                }
            }
        }
        Ok(live)
    }
    fn bind_iteration(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        name: &str,
        ty: SemanticTypeRecipe,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        self.pay(name.len(), depth)?;
        let origin = Query::Binding(statement);
        let copy = copy_recipe(&ty, self.limits, self.left)?;
        self.proof.types.insert((owner, origin.clone()), copy);
        variables.insert(
            name.to_owned(),
            Binding {
                ty,
                mutable: false,
                used: false,
                observed: false,
                capture: false,
                borrowed: false,
                origin,
                initialized: true,
                written: false,
            },
        );
        Ok(())
    }
    fn irrefutable(&mut self, pattern: &SemanticOriginalPattern, depth: usize) -> R<bool> {
        self.pay(1, depth)?;
        Ok(match &pattern.kind {
            CorePatternKind::Name(_) | CorePatternKind::Wildcard => true,
            CorePatternKind::Tuple(fields) => {
                let mut all = true;
                for field in fields {
                    all &= self.irrefutable(field, depth + 1)?;
                }
                all
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                let mut all = true;
                for (_, field) in fields {
                    all &= self.irrefutable(field, depth + 1)?;
                }
                all
            }
            CorePatternKind::Reference { pattern, .. } => self.irrefutable(pattern, depth + 1)?,
            _ => false,
        })
    }
    fn exhaustive(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        arms: &[SemanticOriginalMatchArm],
        depth: usize,
    ) -> R<()> {
        self.pay(arms.len(), depth)?;
        if arms.iter().any(|arm| {
            matches!(
                &arm.pattern.kind,
                CorePatternKind::Wildcard | CorePatternKind::Name(_)
            )
        }) {
            return Ok(());
        }
        let ty = self.logical(owner, ty, depth + 1)?;
        let complete = match ty {
            SemanticTypeRecipe::ErrorUnion { .. } => {
                let mut tags = [false; 2];
                for arm in arms {
                    if let CorePatternKind::ErrorArm { is_error, pattern } = &arm.pattern.kind {
                        tags[usize::from(*is_error)] |= self.irrefutable(pattern, depth + 1)?;
                    }
                }
                tags.into_iter().all(|tag| tag)
            }
            SemanticTypeRecipe::Atom(Type::Bool) => {
                let mut tags = [false; 2];
                for arm in arms {
                    self.pay(1, depth)?;
                    if let CorePatternKind::Bool(value) = &arm.pattern.kind {
                        tags[usize::from(*value)] = true;
                    }
                }
                tags.into_iter().all(|tag| tag)
            }
            SemanticTypeRecipe::Nominal { definition, .. } => {
                let definition = self.definition(definition)?;
                let SemanticDefinitionBody::Nominal {
                    body: SemanticNominalBody::Enum { variants },
                    ..
                } = &definition.body
                else {
                    let mut complete = false;
                    for arm in arms {
                        complete |= self.irrefutable(&arm.pattern, depth + 1)?;
                    }
                    return if complete {
                        Ok(())
                    } else {
                        Err(bad("original product match needs an irrefutable arm"))
                    };
                };
                let mut complete = true;
                for variant in variants {
                    let mut covered = false;
                    for arm in arms {
                        self.pay(1, depth)?;
                        if let CorePatternKind::Variant {
                            variant: name,
                            fields,
                            ..
                        }
                        | CorePatternKind::Constructor {
                            variant: name,
                            fields,
                            ..
                        } = &arm.pattern.kind
                        {
                            if name == &variant.name {
                                let mut all = true;
                                for field in fields {
                                    all &= self.irrefutable(field, depth + 1)?;
                                }
                                covered |= all;
                            }
                        }
                    }
                    complete &= covered;
                }
                complete
            }
            _ => {
                let mut complete = false;
                for arm in arms {
                    complete |= self.irrefutable(&arm.pattern, depth + 1)?;
                }
                complete
            }
        };
        if !complete {
            return Err(bad(
                "original match omits a possible symbolic constructor or payload",
            ));
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
        let outer_loops = std::mem::take(&mut self.loops);
        match body {
            SemanticOriginalFunctionBody::Defined(body) => {
                self.block(definition.id, body, &mut variables, &result, 0)?;
            }
            SemanticOriginalFunctionBody::External => {}
            SemanticOriginalFunctionBody::Declaration => {
                return Err(pending(
                    "abstract original method needs its complete trait member witness",
                ));
            }
        }
        self.loops = outer_loops;
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
        loops: Vec::new(),
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
