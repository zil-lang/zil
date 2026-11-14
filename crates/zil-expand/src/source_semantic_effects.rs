//! Untrusted ABI upper candidates over the complete original operation graph.
//! This pass does not validate effects, callable premises or actual code. In
//! particular, an explicit Direct declaration is never upgraded to repair its
//! body. The independent consumers must check that body and its actual ABI.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Definition(usize),
    Kind(CallKind),
    TraitReceiver(LifeDeclarationId),
    NominalReceiver(SemanticDefinitionId),
    FormalReceiver { owner: usize, ordinal: u32 },
    FallibleTrait(LifeDeclarationId),
    FallibleNominal(SemanticDefinitionId),
    LengthReceiver,
    Unknown,
}
struct Vertex {
    explicit: Option<CallKind>,
    suspend: bool,
    edges: Vec<usize>,
}
struct Scan<'a, 'd, 'b, 'l> {
    definitions: &'d [&'a SemanticDefinition],
    owner: usize,
    locals: Vec<(&'a str, Target)>,
    vertex: &'b mut Vertex,
    budget: &'b mut Budget<'l>,
}
fn signature(definition: &SemanticDefinition) -> Option<&SemanticSignatureRecipe> {
    match &definition.body {
        SemanticDefinitionBody::FunctionWithBody { signature, .. }
        | SemanticDefinitionBody::Function { signature, .. } => Some(signature),
        _ => None,
    }
}
fn declaration_index(
    definitions: &[&SemanticDefinition],
    id: SemanticDefinitionId,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<Option<usize>> {
    budget.identity_work(definitions.len(), span)?;
    Ok(definitions
        .iter()
        .position(|definition| definition.id == id))
}
impl<'a> Scan<'a, '_, '_, '_> {
    fn edge(&mut self, target: Target, span: Span) -> R<()> {
        match target {
            Target::Kind(CallKind::Direct) => {}
            Target::Kind(CallKind::Suspend)
            | Target::Unknown
            | Target::TraitReceiver(_)
            | Target::NominalReceiver(_)
            | Target::FormalReceiver { .. }
            | Target::FallibleTrait(_)
            | Target::FallibleNominal(_)
            | Target::LengthReceiver => self.vertex.suspend = true,
            Target::Definition(index) => {
                self.budget.identity_work(self.vertex.edges.len(), span)?;
                if !self.vertex.edges.contains(&index) {
                    self.budget
                        .lineage_push(&mut self.vertex.edges, index, span)?;
                }
            }
        }
        Ok(())
    }
    fn bind(&mut self, name: &'a str, target: Target, span: Span) -> R<()> {
        self.budget
            .lineage_push(&mut self.locals, (name, target), span)
    }
    fn scope(
        &mut self,
        mut owner: usize,
        reference: &SemanticScopeRef,
        span: Span,
    ) -> R<Option<usize>> {
        let SemanticScopeRef::Ancestor(distance) = reference;
        for _ in 0..*distance {
            self.budget.node(span, 0)?;
            self.budget.identity_work(self.definitions.len(), span)?;
            let Some(parent) = self.definitions[owner].parent else {
                return Ok(None);
            };
            // A lexical walk is bounded by the complete original inventory.
            let Some(next) = self
                .definitions
                .iter()
                .position(|definition| definition.id == parent)
            else {
                return Err(error(span, "effect candidate lexical parent is absent"));
            };
            owner = next;
        }
        Ok(Some(owner))
    }
    fn callable_type(
        &mut self,
        owner: usize,
        ty: &SemanticTypeRecipe,
        depth: usize,
        span: Span,
    ) -> R<Target> {
        self.budget.node(span, depth)?;
        Ok(match ty {
            SemanticTypeRecipe::Code { signature, .. } => Target::Kind(signature.call_kind),
            SemanticTypeRecipe::Managed(inner)
            | SemanticTypeRecipe::Address { inner, .. }
            | SemanticTypeRecipe::Desc { inner, .. }
            | SemanticTypeRecipe::Qualified { storage: inner, .. }
            | SemanticTypeRecipe::Opaque { hidden: inner, .. } => {
                self.callable_type(owner, inner, depth + 1, span)?
            }
            SemanticTypeRecipe::Closure { definition, .. } => {
                declaration_index(self.definitions, *definition, self.budget, span)?
                    .map_or(Target::Unknown, Target::Definition)
            }
            SemanticTypeRecipe::Formal(reference) => {
                let Some(owner) = self.scope(owner, &reference.scope, span)? else {
                    return Ok(Target::Unknown);
                };
                let definition = self.definitions[owner];
                let local_type = SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: reference.ordinal,
                });
                let mut found = None;
                if let Some(SemanticFormal::Type { requirements }) =
                    definition.parameters.get(reference.ordinal as usize)
                {
                    self.callable_requirements(
                        owner,
                        &local_type,
                        requirements,
                        &mut found,
                        depth + 1,
                        span,
                    )?;
                }
                self.callable_requirements(
                    owner,
                    &local_type,
                    &definition.requirements,
                    &mut found,
                    depth + 1,
                    span,
                )?;
                found.map_or(
                    Target::FormalReceiver {
                        owner,
                        ordinal: reference.ordinal,
                    },
                    Target::Kind,
                )
            }
            SemanticTypeRecipe::Array { .. } | SemanticTypeRecipe::Slice(_) => {
                Target::LengthReceiver
            }
            SemanticTypeRecipe::Dyn { interface, .. } => {
                Target::TraitReceiver(interface.application.declaration)
            }
            SemanticTypeRecipe::Nominal { definition, .. } => Target::NominalReceiver(*definition),
            SemanticTypeRecipe::ErrorUnion { result, .. } => {
                match self.callable_type(owner, result, depth + 1, span)? {
                    Target::TraitReceiver(id) => Target::FallibleTrait(id),
                    Target::NominalReceiver(id) => Target::FallibleNominal(id),
                    _ => Target::Unknown,
                }
            }
            SemanticTypeRecipe::SelfType { scope } => {
                let Some(mut owner) = self.scope(owner, scope, span)? else {
                    return Ok(Target::Unknown);
                };
                loop {
                    self.budget.node(span, depth + 1)?;
                    match &self.definitions[owner].body {
                        SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait {
                            declaration,
                        }) => break Target::TraitReceiver(*declaration),
                        SemanticDefinitionBody::Scope(
                            SemanticScopeRecipe::InherentImpl { self_type, .. }
                            | SemanticScopeRecipe::TraitImpl { self_type, .. }
                            | SemanticScopeRecipe::BuiltinImpl { self_type, .. },
                        ) => break self.callable_type(owner, self_type, depth + 1, span)?,
                        _ => {}
                    }
                    let Some(parent) = self.definitions[owner].parent else {
                        break Target::Unknown;
                    };
                    let Some(next) =
                        declaration_index(self.definitions, parent, self.budget, span)?
                    else {
                        break Target::Unknown;
                    };
                    owner = next;
                }
            }
            _ => Target::Unknown,
        })
    }
    fn callable_requirements(
        &mut self,
        owner: usize,
        ty: &SemanticTypeRecipe,
        requirements: &[SemanticRequirementRecipe],
        found: &mut Option<CallKind>,
        depth: usize,
        span: Span,
    ) -> R<()> {
        for requirement in requirements {
            self.budget.node(span, depth)?;
            prepay_recipe_work(requirement, self.budget, span, depth)?;
            match requirement {
                SemanticRequirementRecipe::Callable {
                    ty: receiver,
                    signature,
                    ..
                } if receiver == ty => {
                    // Multiple roles remain separate obligations. Taking the
                    // larger ABI upper is conservative and proves no role.
                    *found = Some(
                        if *found == Some(CallKind::Suspend)
                            || signature.call_kind == CallKind::Suspend
                        {
                            CallKind::Suspend
                        } else {
                            CallKind::Direct
                        },
                    );
                }
                SemanticRequirementRecipe::ForAll { requirements, .. } => {
                    self.callable_requirements(owner, ty, requirements, found, depth + 1, span)?
                }
                _ => {}
            }
        }
        let _ = owner;
        Ok(())
    }
    fn names(&self) -> &'a [SemanticOriginalNameBinding] {
        match &self.definitions[self.owner].body {
            SemanticDefinitionBody::FunctionWithBody { names, .. } => names,
            _ => &[],
        }
    }
    fn path(
        &mut self,
        expression: &'a SemanticOriginalExpr,
        parts: &mut Vec<&'a str>,
        depth: usize,
    ) -> R<bool> {
        self.budget.node(expression.span, depth)?;
        match &expression.kind {
            CoreExprKind::Name(name) => {
                self.budget
                    .lineage_push(parts, name.as_str(), expression.span)?;
                Ok(true)
            }
            CoreExprKind::Member { object, member, .. } => {
                if !self.path(object, parts, depth + 1)? {
                    return Ok(false);
                }
                self.budget
                    .lineage_push(parts, member.as_str(), expression.span)?;
                Ok(true)
            }
            CoreExprKind::TypeApply { callee, .. }
            | CoreExprKind::GenericApply { callee, .. }
            | CoreExprKind::Group(callee) => self.path(callee, parts, depth + 1),
            _ => Ok(false),
        }
    }
    fn target(&mut self, expression: &'a SemanticOriginalExpr, depth: usize) -> R<Target> {
        self.budget.node(expression.span, depth)?;
        match &expression.kind {
            CoreExprKind::Record { ty, .. } => {
                return self.callable_type(self.owner, ty, depth + 1, expression.span);
            }
            CoreExprKind::RecordConstructor {
                definition: Some(definition),
                ..
            } => return Ok(Target::NominalReceiver(*definition)),
            CoreExprKind::Try(value) => {
                return Ok(match self.target(value, depth + 1)? {
                    Target::FallibleTrait(id) => Target::TraitReceiver(id),
                    Target::FallibleNominal(id) => Target::NominalReceiver(id),
                    _ => Target::Unknown,
                });
            }
            CoreExprKind::Call { callee, .. } => {
                if let CoreExprKind::GenericApply {
                    callee: factory,
                    arguments,
                } = &callee.kind
                    && matches!(&factory.kind,CoreExprKind::Name(name) if name=="owned_dyn")
                    && let [CoreGenericArgument::Type(SemanticTypeRecipe::Dyn { interface, .. })] =
                        arguments.as_slice()
                {
                    return Ok(Target::FallibleTrait(interface.application.declaration));
                }
                if let Target::Definition(owner) = self.target(callee, depth + 1)?
                    && let Some(signature) = signature(self.definitions[owner])
                {
                    return self.callable_type(
                        owner,
                        &signature.result,
                        depth + 1,
                        expression.span,
                    );
                }
                return Ok(Target::Unknown);
            }
            CoreExprKind::Ascribed { ty, .. } => {
                return self.callable_type(self.owner, ty, depth + 1, expression.span);
            }
            CoreExprKind::TypeApply { callee, .. }
            | CoreExprKind::GenericApply { callee, .. }
            | CoreExprKind::Group(callee) => return self.target(callee, depth + 1),
            CoreExprKind::Name(name) => {
                self.budget
                    .identity_work(self.locals.len(), expression.span)?;
                if let Some((_, target)) =
                    self.locals.iter().rev().find(|(local, _)| *local == name)
                {
                    return Ok(*target);
                }
            }
            CoreExprKind::Closure { .. } => {
                let SemanticDefinitionBody::FunctionWithBody {
                    sites, site_nodes, ..
                } = &self.definitions[self.owner].body
                else {
                    return Ok(Target::Unknown);
                };
                self.budget.identity_work(
                    site_nodes.len().saturating_add(sites.len()),
                    expression.span,
                )?;
                for node in site_nodes {
                    if node.node != SemanticOriginalNode::Expression(expression.id) {
                        continue;
                    }
                    if let Some(SemanticSiteRecipe {
                        kind: SemanticSiteRecipeKind::Closure { definition, .. },
                        ..
                    }) = sites.iter().find(|site| site.id == node.site)
                    {
                        return Ok(declaration_index(
                            self.definitions,
                            *definition,
                            self.budget,
                            expression.span,
                        )?
                        .map_or(Target::Unknown, Target::Definition));
                    }
                }
                return Ok(Target::Unknown);
            }
            _ => {}
        }
        let mut path = Vec::new();
        let is_path = self.path(expression, &mut path, depth + 1)?;
        let local_root = if let Some(root) = path.first() {
            self.budget
                .identity_work(self.locals.len(), expression.span)?;
            self.locals.iter().rev().any(|(name, _)| name == root)
        } else {
            false
        };
        let target = if is_path && !local_root {
            self.budget.identity_work(
                self.names().len().saturating_mul(path.len() + 1),
                expression.span,
            )?;
            let binding = self.names().iter().find(|binding| {
                binding.namespace == SemanticOriginalNamespace::Value
                    && binding
                        .path
                        .iter()
                        .map(String::as_str)
                        .eq(path.iter().copied())
            });
            if let Some(SemanticOriginalNameBinding {
                value: SemanticOriginalNameValue::Definition(id),
                ..
            }) = binding
            {
                declaration_index(self.definitions, *id, self.budget, expression.span)?
                    .map_or(Target::Unknown, Target::Definition)
            } else if let [name] = path.as_slice() {
                if crate::primitive_name(name) {
                    Target::Kind(if matches!(*name, "task_yield" | "task_await") {
                        CallKind::Suspend
                    } else {
                        CallKind::Direct
                    })
                } else {
                    Target::Unknown
                }
            } else {
                Target::Unknown
            }
        } else {
            Target::Unknown
        };
        self.budget.drop_lineage_vec(path, expression.span)?;
        Ok(target)
    }
    fn method_scope(
        &mut self,
        scope: SemanticDefinitionId,
        spelling: &str,
        depth: usize,
        span: Span,
    ) -> R<bool> {
        self.budget.node(span, depth)?;
        self.budget.identity_work(self.definitions.len(), span)?;
        let mut found = false;
        for (index, definition) in self.definitions.iter().enumerate() {
            if definition.parent == Some(scope) && definition.kind == SemanticDefinitionKind::Method
            {
                self.budget.identity_work(
                    definition
                        .identity
                        .declaration_path
                        .last()
                        .map_or(0, String::len)
                        .saturating_add(spelling.len()),
                    span,
                )?;
                if definition
                    .identity
                    .declaration_path
                    .last()
                    .is_some_and(|name| name == spelling)
                {
                    self.edge(Target::Definition(index), span)?;
                    found = true;
                }
            }
        }
        Ok(found)
    }
    fn trait_methods(
        &mut self,
        declaration: LifeDeclarationId,
        spelling: &str,
        depth: usize,
        span: Span,
        active: &mut Vec<SemanticDefinitionId>,
    ) -> R<bool> {
        self.budget.node(span, depth)?;
        let mut found = false;
        for definition in self.definitions {
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait {
                declaration: source,
            }) = &definition.body
            else {
                continue;
            };
            if *source != declaration {
                continue;
            }
            self.budget.identity_work(active.len(), span)?;
            if active.contains(&definition.id) {
                self.vertex.suspend = true;
                return Ok(true);
            }
            self.budget.lineage_push(active, definition.id, span)?;
            found |= self.method_scope(definition.id, spelling, depth + 1, span)?;
            for requirement in &definition.requirements {
                self.budget.node(span, depth)?;
                if let SemanticRequirementRecipe::Trait {
                    ty: SemanticTypeRecipe::SelfType { .. },
                    application,
                } = requirement
                {
                    found |= self.trait_methods(
                        application.declaration,
                        spelling,
                        depth + 1,
                        span,
                        active,
                    )?;
                }
            }
            active.pop();
        }
        Ok(found)
    }
    fn receiver_method(
        &mut self,
        target: Target,
        spelling: &str,
        depth: usize,
        span: Span,
    ) -> R<bool> {
        self.budget.node(span, depth)?;
        match target {
            Target::LengthReceiver if spelling == "len" => {
                self.edge(Target::Kind(CallKind::Direct), span)?;
                Ok(true)
            }
            Target::TraitReceiver(declaration) => {
                let mut active = Vec::new();
                let found =
                    self.trait_methods(declaration, spelling, depth + 1, span, &mut active)?;
                self.budget.drop_lineage_vec(active, span)?;
                Ok(found)
            }
            Target::NominalReceiver(owner) => {
                let Some(index) = declaration_index(self.definitions, owner, self.budget, span)?
                else {
                    return Ok(false);
                };
                if let SemanticDefinitionBody::Nominal {
                    body: SemanticNominalBody::Product { fields },
                    ..
                } = &self.definitions[index].body
                {
                    self.budget.identity_work(fields.len(), span)?;
                    if let Some(field) = fields.iter().find(|field| field.name == spelling) {
                        let target = self.callable_type(index, &field.ty, depth + 1, span)?;
                        self.edge(target, span)?;
                        return Ok(true);
                    }
                }
                let mut found = false;
                for definition in self.definitions {
                    let self_type = match &definition.body {
                        SemanticDefinitionBody::Scope(
                            SemanticScopeRecipe::InherentImpl { self_type, .. }
                            | SemanticScopeRecipe::TraitImpl { self_type, .. }
                            | SemanticScopeRecipe::BuiltinImpl { self_type, .. },
                        ) => self_type,
                        _ => continue,
                    };
                    self.budget.node(span, depth)?;
                    if matches!(self_type,SemanticTypeRecipe::Nominal {definition,..} if *definition==owner)
                    {
                        found |= self.method_scope(definition.id, spelling, depth + 1, span)?;
                    } else if !matches!(self_type, SemanticTypeRecipe::Nominal { .. }) {
                        // An unresolved blanket/projection receiver is not
                        // discarded merely because its image is not known.
                        if self.method_scope(definition.id, spelling, depth + 1, span)? {
                            self.vertex.suspend = true;
                            found = true;
                        }
                    }
                }
                Ok(found)
            }
            Target::FormalReceiver { owner, ordinal } => {
                let definition = self.definitions[owner];
                let mut found = false;
                let empty = [];
                let own = if let Some(SemanticFormal::Type { requirements }) =
                    definition.parameters.get(ordinal as usize)
                {
                    requirements.as_slice()
                } else {
                    &empty
                };
                for requirements in [own, definition.requirements.as_slice()] {
                    for requirement in requirements {
                        self.budget.node(span, depth)?;
                        if let SemanticRequirementRecipe::Trait {
                            ty: SemanticTypeRecipe::Formal(reference),
                            application,
                        } = requirement
                            && reference.scope == SemanticScopeRef::Ancestor(0)
                            && reference.ordinal == ordinal
                        {
                            found |= self.receiver_method(
                                Target::TraitReceiver(application.declaration),
                                spelling,
                                depth + 1,
                                span,
                            )?;
                        }
                    }
                }
                Ok(found)
            }
            _ => Ok(false),
        }
    }
    // Construction executes its argument expressions, not an unknown method.
    // Resolve the original type namespace and actual enum variant; spelling
    // alone (including compiler-prelude spelling) never selects this case.
    fn enum_constructor(
        &mut self,
        callee: &'a SemanticOriginalExpr,
        arity: usize,
        depth: usize,
    ) -> R<bool> {
        let mut path = Vec::new();
        let is_path = self.path(callee, &mut path, depth + 1)?;
        let result = (|| {
            let Some((variant, owner)) = path.split_last() else {
                return Ok(false);
            };
            if !is_path || owner.is_empty() {
                return Ok(false);
            }
            self.budget.identity_work(self.locals.len(), callee.span)?;
            if self
                .locals
                .iter()
                .rev()
                .any(|(local, _)| *local == owner[0])
            {
                return Ok(false);
            }
            self.budget.identity_work(
                self.names().len().saturating_mul(owner.len() + 1),
                callee.span,
            )?;
            let mut selected = None;
            for binding in self.names() {
                if binding.namespace != SemanticOriginalNamespace::Type
                    || !binding
                        .path
                        .iter()
                        .map(String::as_str)
                        .eq(owner.iter().copied())
                {
                    continue;
                }
                let id = match &binding.value {
                    SemanticOriginalNameValue::Definition(id) => *id,
                    SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Type(
                        SemanticTypeRecipe::Nominal { definition, .. },
                    )) => *definition,
                    _ => return Ok(false),
                };
                // A duplicate/ambiguous original namespace is not repaired by
                // choosing the first possible declaration.
                if selected.replace(id).is_some() {
                    return Ok(false);
                }
            }
            let Some(id) = selected else { return Ok(false) };
            let Some(index) = declaration_index(self.definitions, id, self.budget, callee.span)?
            else {
                return Ok(false);
            };
            let SemanticDefinitionBody::Nominal {
                body: SemanticNominalBody::Enum { variants },
                ..
            } = &self.definitions[index].body
            else {
                return Ok(false);
            };
            self.budget.identity_work(
                variants.len().saturating_mul(variant.len() + 1),
                callee.span,
            )?;
            let mut found = false;
            for row in variants {
                if row.name == *variant {
                    if found || row.fields.len() != arity {
                        return Ok(false);
                    }
                    found = true;
                }
            }
            Ok(found)
        })();
        self.budget.drop_lineage_vec(path, callee.span)?;
        result
    }
    fn invoke(
        &mut self,
        callee: &'a SemanticOriginalExpr,
        arity: Option<usize>,
        depth: usize,
    ) -> R<()> {
        let target = self.target(callee, depth + 1)?;
        if target != Target::Unknown {
            return self.edge(target, callee.span);
        }
        if let Some(arity) = arity
            && self.enum_constructor(callee, arity, depth + 1)?
        {
            return self.edge(Target::Kind(CallKind::Direct), callee.span);
        }
        let receiver = match &callee.kind {
            CoreExprKind::Member { object, member, .. } => {
                Some((self.target(object, depth + 1)?, member.as_str()))
            }
            CoreExprKind::ReceiverMember(member) => {
                self.budget.identity_work(self.locals.len(), callee.span)?;
                self.locals
                    .iter()
                    .rev()
                    .find(|(name, _)| *name == "self")
                    .map(|(_, target)| (*target, member.as_str()))
            }
            CoreExprKind::GenericApply { callee, .. }
            | CoreExprKind::TypeApply { callee, .. }
            | CoreExprKind::Group(callee) => return self.invoke(callee, arity, depth + 1),
            _ => None,
        };
        if let Some((target, member)) = receiver
            && self.receiver_method(target, member, depth + 1, callee.span)?
        {
            return Ok(());
        }
        self.edge(Target::Unknown, callee.span)
    }
    fn pattern(&mut self, pattern: &'a SemanticOriginalPattern, depth: usize) -> R<()> {
        self.budget.node(pattern.span, depth)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => self.bind(name, Target::Unknown, pattern.span)?,
            CorePatternKind::Tuple(fields)
            | CorePatternKind::Variant { fields, .. }
            | CorePatternKind::Constructor { fields, .. } => {
                for field in fields {
                    self.pattern(field, depth + 1)?
                }
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                for (_, field) in fields {
                    self.pattern(field, depth + 1)?
                }
            }
            CorePatternKind::Reference { pattern, .. }
            | CorePatternKind::ErrorArm { pattern, .. } => self.pattern(pattern, depth + 1)?,
            CorePatternKind::Wildcard
            | CorePatternKind::Integer(_)
            | CorePatternKind::Bool(_)
            | CorePatternKind::Float(_)
            | CorePatternKind::String(_)
            | CorePatternKind::Character(_) => {}
        }
        Ok(())
    }
    fn block(&mut self, block: &'a SemanticOriginalBlock, depth: usize) -> R<()> {
        self.budget.node(block.span, depth)?;
        let base = self.locals.len();
        for statement in &block.statements {
            self.statement(statement, depth + 1)?
        }
        self.locals.truncate(base);
        Ok(())
    }
    fn statement(&mut self, statement: &'a SemanticOriginalStatement, depth: usize) -> R<()> {
        self.budget.node(statement.span, depth)?;
        use CoreStatementKind as S;
        match &statement.kind {
            S::Binding {
                name,
                ty,
                initializer,
                ..
            } => {
                if let Some(value) = initializer {
                    self.expression(value, depth + 1)?
                }
                let target = if let Some(ty) = ty {
                    self.callable_type(self.owner, ty, depth + 1, statement.span)?
                } else if let Some(value) = initializer {
                    self.target(value, depth + 1)?
                } else {
                    Target::Unknown
                };
                self.bind(name, target, statement.span)?;
            }
            S::TupleBinding {
                pattern,
                initializer,
                ..
            } => {
                self.expression(initializer, depth + 1)?;
                self.pattern(pattern, depth + 1)?;
            }
            S::Assign { target, value } => {
                self.expression(target, depth + 1)?;
                self.expression(value, depth + 1)?;
                if let CoreExprKind::Name(name) = &target.kind {
                    let target = self.target(value, depth + 1)?;
                    self.budget
                        .identity_work(self.locals.len(), statement.span)?;
                    if let Some((_, old)) = self
                        .locals
                        .iter_mut()
                        .rev()
                        .find(|(local, _)| *local == name)
                    {
                        if *old != target {
                            *old = Target::Unknown
                        }
                    }
                }
            }
            S::CompoundAssign { target, value, .. } => {
                self.expression(target, depth + 1)?;
                self.expression(value, depth + 1)?;
            }
            S::Expression(value) | S::Yield(value) => self.expression(value, depth + 1)?,
            S::Return(value) => {
                if let Some(value) = value {
                    self.expression(value, depth + 1)?
                }
            }
            S::If {
                condition,
                then_body,
                else_body,
            } => {
                self.expression(condition, depth + 1)?;
                self.block(then_body, depth + 1)?;
                if let Some(body) = else_body {
                    self.block(body, depth + 1)?
                }
            }
            S::While { condition, body } => {
                self.expression(condition, depth + 1)?;
                self.block(body, depth + 1)?;
            }
            S::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                self.expression(value, depth + 1)?;
                let base = self.locals.len();
                self.pattern(pattern, depth + 1)?;
                self.block(then_body, depth + 1)?;
                self.locals.truncate(base);
                if let Some(body) = else_body {
                    self.block(body, depth + 1)?
                }
            }
            S::WhileLet {
                pattern,
                value,
                body,
            }
            | S::ForEach {
                pattern,
                iterator: value,
                body,
            } => {
                self.expression(value, depth + 1)?;
                let base = self.locals.len();
                self.pattern(pattern, depth + 1)?;
                self.block(body, depth + 1)?;
                self.locals.truncate(base);
                if matches!(statement.kind, S::ForEach { .. }) {
                    self.vertex.suspend = true;
                }
            }
            S::ForRange {
                name,
                start,
                end,
                body,
            } => {
                self.expression(start, depth + 1)?;
                self.expression(end, depth + 1)?;
                let base = self.locals.len();
                self.bind(name, Target::Unknown, statement.span)?;
                self.block(body, depth + 1)?;
                self.locals.truncate(base);
            }
            S::Match { value, arms } => {
                self.expression(value, depth + 1)?;
                for arm in arms {
                    let base = self.locals.len();
                    self.pattern(&arm.pattern, depth + 1)?;
                    self.block(&arm.body, depth + 1)?;
                    self.locals.truncate(base);
                }
            }
            S::Block(body) | S::Defer(body) | S::Unsafe(body) | S::Loop(body) => {
                self.block(body, depth + 1)?
            }
            S::With { name, value, body } => {
                self.expression(value, depth + 1)?;
                let base = self.locals.len();
                let target = self.target(value, depth + 1)?;
                self.bind(name, target, statement.span)?;
                self.block(body, depth + 1)?;
                self.locals.truncate(base);
            }
            S::CompileTime(_) => {}
            S::Break | S::Continue => {}
            S::Unsupported(_) => self.vertex.suspend = true,
        }
        Ok(())
    }
    fn expression(&mut self, expression: &'a SemanticOriginalExpr, depth: usize) -> R<()> {
        self.budget.node(expression.span, depth)?;
        use CoreExprKind as E;
        match &expression.kind {
            E::Call { callee, arguments }
            | E::CallDynSlot {
                callee, arguments, ..
            } => {
                self.expression(callee, depth + 1)?;
                for argument in arguments {
                    self.expression(argument, depth + 1)?
                }
                self.budget
                    .identity_work(arguments.len(), expression.span)?;
                let arity = (!arguments
                    .iter()
                    .any(|argument| matches!(argument.kind, E::Spread(_))))
                .then_some(arguments.len());
                self.invoke(callee, arity, depth + 1)?;
            }
            E::Pipeline { input, target } => {
                self.expression(input, depth + 1)?;
                self.expression(target, depth + 1)?;
                self.invoke(target, Some(1), depth + 1)?;
            }
            E::MakeDynBorrow { source: value, .. }
            | E::MakeDynOwned { source: value, .. }
            | E::Ascribed { value, .. }
            | E::NumericConvert { value, .. }
            | E::IntegerConvert { value, .. }
            | E::Unary { operand: value, .. }
            | E::Try(value)
            | E::Spread(value)
            | E::Group(value) => self.expression(value, depth + 1)?,
            E::Binary { left, right, .. }
            | E::Range {
                start: left,
                end: right,
            }
            | E::Index {
                object: left,
                index: right,
            } => {
                self.expression(left, depth + 1)?;
                self.expression(right, depth + 1)?;
            }
            E::TypeApply { callee, .. } | E::GenericApply { callee, .. } => {
                self.expression(callee, depth + 1)?
            }
            E::Tuple(values) | E::Array(values) => {
                for value in values {
                    self.expression(value, depth + 1)?
                }
            }
            E::Slice { object, start, end } => {
                self.expression(object, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    self.expression(value, depth + 1)?
                }
            }
            E::Member { object, .. } => self.expression(object, depth + 1)?,
            E::Record { fields, .. } | E::RecordConstructor { fields, .. } => {
                for (_, value) in fields {
                    self.expression(value, depth + 1)?
                }
            }
            E::RecordUpdate { base, fields, .. } => {
                self.expression(base, depth + 1)?;
                for (_, value) in fields {
                    self.expression(value, depth + 1)?
                }
            }
            E::Sequence { bindings, value } => {
                let base = self.locals.len();
                for (name, value) in bindings {
                    self.expression(value, depth + 1)?;
                    let target = self.target(value, depth + 1)?;
                    self.bind(name, target, expression.span)?
                }
                self.expression(value, depth + 1)?;
                self.locals.truncate(base);
            }
            E::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expression(condition, depth + 1)?;
                self.expression(then_value, depth + 1)?;
                self.expression(else_value, depth + 1)?
            }
            // Creation does not execute the child's body. Each authored child
            // definition is a separate graph vertex with its own full body.
            E::Closure { .. }
            | E::CompileTime(_)
            | E::CompileTimeBlock(_)
            | E::SyntaxQuote { .. }
            | E::SyntaxIsland { .. }
            | E::Splice(_) => {}
            E::Literal(_)
            | E::Integer(_)
            | E::Float(_)
            | E::Bool(_)
            | E::String(_)
            | E::Unit
            | E::Name(_)
            | E::ReceiverMember(_)
            | E::Character(_) => {}
            E::Unsupported(_) => self.vertex.suspend = true,
        }
        Ok(())
    }
}

pub(super) fn propose(
    modules: &mut [(ModuleId, SemanticModule)],
    originals: &SourceDeclarationInventory<'_>,
    budget: &mut Budget<'_>,
) -> R<()> {
    let mut definitions = Vec::new();
    for (_, module) in modules.iter() {
        for definition in &module.definitions {
            budget.lineage_push(&mut definitions, definition, Span::default())?
        }
    }
    let mut vertices = Vec::new();
    for (index, definition) in definitions.iter().enumerate() {
        let mut explicit = None;
        budget.identity_work(originals.declarations.len(), Span::default())?;
        if let Some(row) = originals.declarations.iter().find(|row| {
            row.module == definition.id.module && row.declaration.0 == definition.id.index
        }) && let Some(function) = row.function()
        {
            let mut remaining = budget.remaining_limits();
            explicit = crate::tasks::selection(function, &mut remaining.max_analysis_steps)?.0;
            budget.adopt_remaining(&remaining);
        }
        let mut vertex = Vertex {
            explicit,
            suspend: explicit == Some(CallKind::Suspend),
            edges: Vec::new(),
        };
        if let SemanticDefinitionBody::FunctionWithBody {
            signature,
            parameters,
            body,
            ..
        } = &definition.body
        {
            let mut scan = Scan {
                definitions: &definitions,
                owner: index,
                locals: Vec::new(),
                vertex: &mut vertex,
                budget,
            };
            for (ordinal, parameter) in parameters.iter().enumerate() {
                let target = signature
                    .parameters
                    .get(ordinal)
                    .map(|parameter| &parameter.ty);
                let target = if let Some(ty) = target {
                    scan.callable_type(index, ty, 0, parameter.span)?
                } else {
                    Target::Unknown
                };
                scan.bind(&parameter.name, target, parameter.span)?
            }
            if let SemanticOriginalFunctionBody::Defined(body) = body {
                scan.block(body, 0)?
            }
            let locals = std::mem::take(&mut scan.locals);
            scan.budget.drop_lineage_vec(locals, Span::default())?;
        }
        budget.lineage_push(&mut vertices, vertex, Span::default())?;
    }
    // Monotone propagation reaches a fixed point after at most one promotion
    // per vertex. Closed recursive SCCs without a Suspend edge remain Direct.
    loop {
        let mut changed = false;
        for index in 0..vertices.len() {
            budget.node(Span::default(), 0)?;
            if vertices[index].explicit.is_some() || vertices[index].suspend {
                continue;
            }
            budget.identity_work(vertices[index].edges.len(), Span::default())?;
            if vertices[index].edges.iter().any(|target| {
                vertices[*target].explicit == Some(CallKind::Suspend)
                    || (vertices[*target].explicit.is_none() && vertices[*target].suspend)
            }) {
                vertices[index].suspend = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    budget.drop_lineage_vec(definitions, Span::default())?;
    let mut index = 0;
    for (_, module) in modules {
        for definition in &mut module.definitions {
            let vertex = &vertices[index];
            index += 1;
            let kind = vertex.explicit.unwrap_or(if vertex.suspend {
                CallKind::Suspend
            } else {
                CallKind::Direct
            });
            if let SemanticDefinitionBody::FunctionWithBody { signature, .. }
            | SemanticDefinitionBody::Function { signature, .. } = &mut definition.body
            {
                signature.call_kind = kind;
            }
        }
    }
    for vertex in &mut vertices {
        budget.drop_lineage_vec(std::mem::take(&mut vertex.edges), Span::default())?;
    }
    budget.drop_lineage_vec(vertices, Span::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_context::{self, DeclarationPlacement, SourceIdentityContext};
    use std::collections::BTreeMap;
    use zil_base::{FileId, SourceUnit};

    #[test]
    fn original_effect_candidates_follow_calls_and_preserve_explicit_bounds() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "original-effects.zil",
            "fn leaf(){task_yield();}fn relay(){leaf();}fn a(){b();}fn b(){a();}fn declared_direct() contract{effects(io);}{task_yield();}fn declared_suspend() contract{effects(suspend);}{}fn creator(){let worker=()=>task_yield();}fn invoked(){let worker=()=>task_yield();worker();}fn through<F:Fn()->void>(worker:F){worker();}fn deferred(){defer{task_yield();}}fn unknown(){unresolved();}fn length(values:&mut[u8])->i64{return values.len()}",
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&parsed],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let mut remaining = limits.clone();
        let (_, program) =
            super::super::program::prepare(&[&parsed], &origins, &[Vec::new()], &mut remaining)
                .unwrap();
        let definitions = &program.modules[0].1.definitions;
        let kind = |index: usize| signature(&definitions[index]).unwrap().call_kind;
        assert_eq!(kind(0), CallKind::Suspend);
        assert_eq!(kind(1), CallKind::Suspend);
        assert_eq!(kind(2), CallKind::Direct);
        assert_eq!(kind(3), CallKind::Direct);
        assert_eq!(kind(4), CallKind::Direct); // Invalid body is left for consumers.
        assert_eq!(kind(5), CallKind::Suspend); // Empty body does not narrow a declaration.
        assert_eq!(kind(6), CallKind::Direct); // Making a child does not execute it.
        assert_eq!(kind(7), CallKind::Suspend);
        assert_eq!(kind(8), CallKind::Direct); // Original callable premise, not actual F.
        assert_eq!(kind(9), CallKind::Suspend);
        assert_eq!(kind(10), CallKind::Suspend); // Unresolved is not silently Direct.
        assert_eq!(kind(11), CallKind::Direct); // Registered slice method, from original annotation.
    }
    #[test]
    fn method_candidates_use_original_receiver_bindings_not_same_spelling() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "original-method-effects.zil",
            "trait Read{fn read(self:&Self)->i32;}struct Provider{n:i32}impl Read for Provider{fn read(self:&Self)->i32{return self.n}}fn declared_receiver(value:&Provider){value.read();}fn unknown_receiver<F>(value:F){value.read();}fn bounded_receiver<F:Read>(value:F){value.read();}",
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&parsed],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let mut remaining = limits.clone();
        let (_, program) =
            super::super::program::prepare(&[&parsed], &origins, &[Vec::new()], &mut remaining)
                .unwrap();
        let kind = |node| {
            signature(
                program.modules[0]
                    .1
                    .definitions
                    .iter()
                    .find(|definition| {
                        definition.id
                            == SemanticDefinitionId {
                                module: source.module_id,
                                index: node,
                            }
                    })
                    .unwrap(),
            )
            .unwrap()
            .call_kind
        };
        assert_eq!(kind(3), CallKind::Direct);
        assert_eq!(kind(4), CallKind::Suspend);
        assert_eq!(kind(5), CallKind::Direct);
    }
    #[test]
    fn enum_constructor_candidates_use_original_type_bindings_and_preserve_argument_effects() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "enum-constructor-effects.zil",
            "enum Choice<T>{Empty,Value(T)}fn empty(){Choice::<i32>.Empty();}fn value(){Choice::<i32>.Value(42);}fn argument(){Choice::<void>.Value(task_yield());}fn wrong_arity(){Choice::<i32>.Value();}fn shadow< F >(Choice:F){Choice.Value(42);}fn unknown(){Other.Value(42);}",
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&parsed],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let (_, program) = super::super::program::prepare(
            &[&parsed],
            &origins,
            &[Vec::new()],
            &mut limits.clone(),
        )
        .unwrap();
        let kind = |index| {
            signature(
                program.modules[0]
                    .1
                    .definitions
                    .iter()
                    .find(|row| row.id.index == index)
                    .unwrap(),
            )
            .unwrap()
            .call_kind
        };
        assert_eq!(kind(1), CallKind::Direct);
        assert_eq!(kind(2), CallKind::Direct);
        assert_eq!(kind(3), CallKind::Suspend);
        assert_eq!(kind(4), CallKind::Suspend);
        assert_eq!(kind(5), CallKind::Suspend);
        assert_eq!(kind(6), CallKind::Suspend);
    }
}
