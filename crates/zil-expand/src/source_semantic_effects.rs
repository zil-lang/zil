//! Untrusted ABI upper candidates over the complete original operation graph.
//! This pass does not validate effects, callable premises or actual code. In
//! particular, an explicit Direct declaration is never upgraded to repair its
//! body. The independent consumers must check that body and its actual ABI.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Definition(usize),
    Kind(CallKind),
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
            Target::Kind(CallKind::Suspend) | Target::Unknown => self.vertex.suspend = true,
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
            SemanticTypeRecipe::Address { inner, .. }
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
                found.map_or(Target::Unknown, Target::Kind)
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
        let target = if is_path {
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
    fn invoke(&mut self, callee: &'a SemanticOriginalExpr, depth: usize) -> R<()> {
        let target = self.target(callee, depth + 1)?;
        if target != Target::Unknown {
            return self.edge(target, callee.span);
        }
        // Unresolved method selection is not a fabricated static target. Keep
        // every same-spelled visible declaration as an upper candidate. If
        // there is no such complete binding, retain the unknown Suspend upper.
        let spelling = match &callee.kind {
            CoreExprKind::Member { member, .. } | CoreExprKind::ReceiverMember(member) => {
                Some(member.as_str())
            }
            _ => None,
        };
        let mut found = false;
        if let Some(spelling) = spelling {
            for binding in self.names() {
                self.budget.node(callee.span, depth)?;
                if binding.namespace != SemanticOriginalNamespace::Value
                    || binding.path.last().is_none_or(|part| part != spelling)
                {
                    continue;
                }
                if let SemanticOriginalNameValue::Definition(id) = binding.value {
                    if let Some(index) =
                        declaration_index(self.definitions, id, self.budget, callee.span)?
                    {
                        if signature(self.definitions[index]).is_some() {
                            self.edge(Target::Definition(index), callee.span)?;
                            found = true;
                        }
                    }
                }
            }
        }
        if !found {
            self.edge(Target::Unknown, callee.span)?
        }
        Ok(())
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
                self.invoke(callee, depth + 1)?;
            }
            E::Pipeline { input, target } => {
                self.expression(input, depth + 1)?;
                self.expression(target, depth + 1)?;
                self.invoke(target, depth + 1)?;
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
            "fn leaf(){task_yield();}fn relay(){leaf();}fn a(){b();}fn b(){a();}fn declared_direct() contract{effects(io);}{task_yield();}fn declared_suspend() contract{effects(suspend);}{}fn creator(){let worker=()=>task_yield();}fn invoked(){let worker=()=>task_yield();worker();}fn through<F:Fn()>(worker:F){worker();}fn deferred(){defer{task_yield();}}fn unknown(){unresolved();}",
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
    }
}
