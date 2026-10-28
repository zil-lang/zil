//! Resolve one self-contained concrete environment behind a public opaque
//! callable return. This module does not instantiate or inline factory bodies
//! at call sites; direct calls retain their ordinary function identities.
use super::*;
use std::collections::BTreeSet;

/// Schedule concrete results without speculative type checking: a failed
/// Builder is allowed to have reserved environment identities. The borrowed
/// walk resolves direct calls and respects every local shadowing scope.
pub(super) fn dependencies(
    function: &CoreFunction,
    signatures: &BTreeMap<String, Signature>,
    imports: Option<&program::Imports<'_>>,
    work: &mut usize,
    limits: &Limits,
) -> R<BTreeSet<FunctionId>> {
    Ok(direct_dependencies(function, signatures, imports, work, limits)?.factories)
}

#[derive(Default)]
struct DirectDependencies {
    factories: BTreeSet<FunctionId>,
    helpers: BTreeSet<helpers::TemplateKey>,
}

fn direct_dependencies(
    function: &CoreFunction,
    signatures: &BTreeMap<String, Signature>,
    imports: Option<&program::Imports<'_>>,
    work: &mut usize,
    limits: &Limits,
) -> R<DirectDependencies> {
    spend(work, function.parameters.len(), function.span)?;
    for parameter in &function.parameters {
        spend(work, parameter.name.len(), parameter.span)?;
    }
    let mut walk = Dependencies {
        signatures,
        imports,
        scopes: vec![function.parameters.iter().map(|p| p.name.clone()).collect()],
        found: DirectDependencies::default(),
        work,
        limits,
    };
    walk.block(&function.body, 0)?;
    Ok(walk.found)
}

/// An instantiated helper body is checked while its caller factory is being
/// grounded. Its factory calls therefore precede that caller too. Walk source
/// definitions in their own lexical/module context without creating instances.
pub(super) fn factory_dependencies(
    function: &CoreFunction,
    signatures: &BTreeMap<String, Signature>,
    imports: Option<&program::Imports<'_>>,
    state: &closures::State,
    work: &mut usize,
    limits: &Limits,
) -> R<BTreeSet<FunctionId>> {
    let DirectDependencies {
        mut factories,
        helpers,
    } = direct_dependencies(function, signatures, imports, work, limits)?;
    spend(work, helpers.len(), function.span)?;
    let mut pending = helpers.into_iter().collect::<Vec<_>>();
    let mut visited = BTreeSet::new();
    while let Some(key) = pending.pop() {
        spend(work, 1, function.span)?;
        if !visited.insert(key) {
            continue;
        }
        let template = state
            .helper_templates
            .get(&key)
            .ok_or_else(|| unsupported(function.span, "callable template dependency is absent"))?;
        let span = template.function.span;
        spend(work, template.context.imports.len(), span)?;
        for alias in template.context.imports.keys() {
            spend(work, alias.len(), span)?;
        }
        let imports = program::Imports {
            aliases: template.context.imports.clone(),
            signatures: &state.helper_tables,
        };
        let signatures = state
            .helper_tables
            .get(&template.context.module)
            .ok_or_else(|| unsupported(span, "callable template dependency lookup is absent"))?;
        let reached =
            direct_dependencies(&template.function, signatures, Some(&imports), work, limits)?;
        spend(
            work,
            reached
                .factories
                .len()
                .saturating_add(reached.helpers.len()),
            span,
        )?;
        factories.extend(reached.factories);
        pending.extend(reached.helpers);
    }
    Ok(factories)
}

pub(super) fn order(
    graph: BTreeMap<FunctionId, (Span, BTreeSet<FunctionId>)>,
    work: &mut usize,
) -> R<Vec<FunctionId>> {
    let mut counts = BTreeMap::new();
    let mut users = BTreeMap::<FunctionId, Vec<FunctionId>>::new();
    let mut ready = BTreeSet::new();
    for (id, (span, edges)) in &graph {
        spend(work, edges.len().saturating_add(1), *span)?;
        counts.insert(*id, edges.len());
        if edges.is_empty() {
            ready.insert(*id);
        }
        for edge in edges {
            users.entry(*edge).or_default().push(*id);
        }
    }
    let mut ordered = Vec::new();
    while let Some(id) = ready.pop_first() {
        ordered.push(id);
        if let Some(dependents) = users.get(&id) {
            spend(work, dependents.len(), graph[&id].0)?;
            for dependent in dependents {
                let count = counts.get_mut(dependent).expect("factory graph node");
                *count -= 1;
                if *count == 0 {
                    ready.insert(*dependent);
                }
            }
        }
    }
    if ordered.len() != graph.len() {
        let (id, _) = counts
            .iter()
            .find(|(_, count)| **count != 0)
            .expect("cycle");
        return Err(unsupported(
            graph[id].0,
            "recursive opaque factory result dependencies have no finite concrete identity",
        ));
    }
    Ok(ordered)
}

fn spend(work: &mut usize, count: usize, span: Span) -> R<()> {
    *work = work.checked_sub(count).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "factory dependency budget exceeded",
            Some(span),
        )
    })?;
    Ok(())
}

pub(super) fn self_contained(
    closures: &[closures::Pending],
    root: ClosureId,
    work: &mut usize,
    span: Span,
) -> R<()> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        spend(work, 1, span)?;
        if !seen.insert(id) {
            continue;
        }
        let closure = closures.get(id.0 as usize).ok_or_else(|| {
            error(
                "E-CLOSURE-INFERENCE",
                span,
                "nested closure identity is unresolved",
            )
        })?;
        spend(work, closure.captures.len(), span)?;
        for capture in &closure.captures {
            if capture.borrowed || (matches!(capture.ty, Type::Managed(_)) && !capture.moved) {
                return Err(unsupported(
                    span,
                    "retained callable environments cannot capture borrowed positions or descriptions",
                ));
            }
            match &capture.ty {
                Type::Closure(child) => pending.push(*child),
                Type::Managed(_) | Type::Package { .. } => {}
                ty if ty.is_copy_value() => {}
                _ => {
                    return Err(unsupported(
                        span,
                        "owned nested closure captures require self-contained environments; borrowed dependencies cannot escape",
                    ));
                }
            }
        }
    }
    Ok(())
}

struct Dependencies<'a, 'b> {
    signatures: &'a BTreeMap<String, Signature>,
    imports: Option<&'a program::Imports<'a>>,
    scopes: Vec<BTreeSet<String>>,
    found: DirectDependencies,
    work: &'b mut usize,
    limits: &'a Limits,
}
impl Dependencies<'_, '_> {
    fn node(&mut self, depth: usize, span: Span) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "factory dependency depth exceeded",
                Some(span),
            ));
        }
        spend(self.work, 1, span)
    }
    fn bound(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }
    fn bind(&mut self, name: &str, span: Span) -> R<()> {
        spend(self.work, name.len().saturating_add(1), span)?;
        self.scopes.last_mut().expect("scope").insert(name.into());
        Ok(())
    }
    fn pattern(&mut self, pattern: &CorePattern, depth: usize) -> R<()> {
        self.node(depth, pattern.span)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => self.bind(name, pattern.span)?,
            CorePatternKind::Tuple(fields) | CorePatternKind::Variant { fields, .. } => {
                for field in fields {
                    self.pattern(field, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn patterned(&mut self, pattern: &CorePattern, body: &CoreBlock, depth: usize) -> R<()> {
        self.scopes.push(BTreeSet::new());
        self.pattern(pattern, depth + 1)?;
        self.block(body, depth + 1)?;
        self.scopes.pop();
        Ok(())
    }
    fn block(&mut self, block: &CoreBlock, depth: usize) -> R<()> {
        self.node(depth, block.span)?;
        self.scopes.push(BTreeSet::new());
        for statement in &block.statements {
            self.node(depth + 1, statement.span)?;
            match &statement.kind {
                CoreStatementKind::Binding {
                    name, initializer, ..
                } => {
                    if let Some(value) = initializer {
                        self.expr(value, depth + 1)?;
                    }
                    self.bind(name, statement.span)?;
                }
                CoreStatementKind::Assign { target, value } => {
                    self.expr(target, depth + 1)?;
                    self.expr(value, depth + 1)?;
                }
                CoreStatementKind::Expression(e) | CoreStatementKind::Return(Some(e)) => {
                    self.expr(e, depth + 1)?
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expr(condition, depth + 1)?;
                    self.block(then_body, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    self.expr(value, depth + 1)?;
                    self.patterned(pattern, then_body, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(body, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    self.expr(condition, depth + 1)?;
                    self.block(body, depth + 1)?;
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    self.expr(value, depth + 1)?;
                    self.patterned(pattern, body, depth + 1)?;
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    self.expr(start, depth + 1)?;
                    self.expr(end, depth + 1)?;
                    self.scopes.push(BTreeSet::new());
                    self.bind(name, statement.span)?;
                    self.block(body, depth + 1)?;
                    self.scopes.pop();
                }
                CoreStatementKind::Match { value, arms } => {
                    self.expr(value, depth + 1)?;
                    for arm in arms {
                        self.patterned(&arm.pattern, &arm.body, depth + 1)?;
                    }
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    self.block(body, depth + 1)?
                }
                _ => {}
            }
        }
        self.scopes.pop();
        Ok(())
    }
    fn expr(&mut self, expression: &CoreExpr, depth: usize) -> R<()> {
        self.node(depth, expression.span)?;
        match &expression.kind {
            CoreExprKind::Call { callee, arguments } => {
                let signature = match &callee.kind {
                    CoreExprKind::Name(name) if !self.bound(name) => self.signatures.get(name),
                    CoreExprKind::Member { object, member } => {
                        if let CoreExprKind::Name(alias) = &object.kind {
                            if self.bound(alias) {
                                None
                            } else {
                                self.imports.and_then(|imports| {
                                    imports
                                        .aliases
                                        .get(alias)
                                        .and_then(|module| imports.signatures.get(module))
                                        .and_then(|table| table.get(member))
                                })
                            }
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(signature) = signature {
                    if signature.callable_return.is_some() {
                        self.found.factories.insert(signature.id);
                    }
                    if let Some(key) = signature.template {
                        self.found.helpers.insert(key);
                    }
                }
                self.expr(callee, depth + 1)?;
                for argument in arguments {
                    self.expr(argument, depth + 1)?;
                }
            }
            CoreExprKind::Ascribed { value, .. }
            | CoreExprKind::IntegerConvert { value, .. }
            | CoreExprKind::CompileTime(value)
            | CoreExprKind::Try(value)
            | CoreExprKind::Unary { operand: value, .. }
            | CoreExprKind::Member { object: value, .. } => self.expr(value, depth + 1)?,
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Index {
                object: left,
                index: right,
            } => {
                self.expr(left, depth + 1)?;
                self.expr(right, depth + 1)?;
            }
            CoreExprKind::Tuple(fields) | CoreExprKind::Array(fields) => {
                for field in fields {
                    self.expr(field, depth + 1)?;
                }
            }
            CoreExprKind::Slice { object, start, end } => {
                self.expr(object, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    self.expr(value, depth + 1)?;
                }
            }
            CoreExprKind::Record { fields, .. } => {
                for (_, field) in fields {
                    self.expr(field, depth + 1)?;
                }
            }
            CoreExprKind::RecordUpdate { base, fields, .. } => {
                self.expr(base, depth + 1)?;
                for (_, field) in fields {
                    self.expr(field, depth + 1)?;
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                self.scopes.push(BTreeSet::new());
                for (name, initializer) in bindings {
                    self.expr(initializer, depth + 1)?;
                    self.bind(name, expression.span)?;
                }
                self.expr(value, depth + 1)?;
                self.scopes.pop();
            }
            CoreExprKind::Closure {
                parameters, body, ..
            } => {
                self.scopes.push(BTreeSet::new());
                for parameter in parameters {
                    self.bind(&parameter.name, parameter.span)?;
                }
                self.block(body, depth + 1)?;
                self.scopes.pop();
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, depth + 1)?;
                self.expr(then_value, depth + 1)?;
                self.expr(else_value, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
}

pub(super) fn validate_factory(function: &CoreFunction) -> R<()> {
    let Some(bound) = &function.callable_return else {
        return Ok(());
    };
    let signature = Type::FunctionPointer {
        parameters: bound.parameters.clone(),
        result: Box::new(bound.result.clone()),
    };
    if function.foreign.is_some()
        || function.receiver.is_some()
        || function.callable_parameter.is_some()
        || function.result_mode != PassingMode::Infer
        || bound.kind == CallableKind::FunctionPointer
        || !closures::supported_signature(&bound.parameters, &bound.result)
        || (lending::supported_result(&bound.result) && bound.kind == CallableKind::FnOnce)
        || function.return_type
            != (Type::Opaque {
                identity: function.id,
                hidden: Box::new(signature),
            })
        || function.parameters.iter().any(|parameter| {
            !(parameter.ty.is_copy_value()
                || matches!(&parameter.ty, Type::Managed(_) if resources::supported(&parameter.ty))
                || packages::supported(&parameter.ty))
        })
    {
        return Err(unsupported(
            function.span,
            "opaque callable factories require ordinary Copy/owned Managed/Package inputs, finite Copy or Copy/slice-address parameters, a Copy or shared environment result and a matching opaque identity",
        ));
    }
    Ok(())
}

impl Builder<'_, '_> {
    pub(super) fn factory_return(&mut self, expression: Option<&CoreExpr>, span: Span) -> R<()> {
        let expression = expression.ok_or_else(|| {
            error(
                "E-TYPE-MISMATCH",
                span,
                "callable factory must return its concrete environment",
            )
        })?;
        let (value, ty) = self.transfer_expr(expression, None)?;
        let Type::Closure(id) = ty else {
            return Err(error(
                "E-TYPE-MISMATCH",
                span,
                "opaque callable return requires a closure environment",
            ));
        };
        let bound = self.factory_bound.as_ref().expect("factory return").clone();
        let pending = &mut self.closures.pending[id.0 as usize];
        if pending.inference.arity() != bound.parameters.len() {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "returned closure differs from the public callable signature",
            ));
        }
        for (index, ty) in bound.parameters.iter().enumerate() {
            pending.inference.argument(index, ty, span)?;
        }
        pending.inference.expect_result(&bound.result, span)?;
        if pending.callable_kind() != bound.kind {
            return Err(unsupported(
                span,
                "opaque callable kind adaptation is not implemented; declared and actual Fn/FnMut/FnOnce kinds must match",
            ));
        }
        let mut cleanup = Vec::new();
        for (index, capture) in pending.captures.iter().enumerate() {
            let ty = capture.stored_type();
            if matches!(&ty, Type::Managed(_) if resources::supported(&ty))
                || packages::supported(&ty)
                || matches!(ty, Type::Closure(_))
            {
                cleanup.push(index as u32);
            } else if !ty.is_copy_value() {
                return Err(unsupported(
                    span,
                    "returned closure captures must be self-contained Copy snapshots or owned Managed/Package values; escaping Address/Desc captures are not implemented",
                ));
            }
        }
        if let Some(previous) = &self.function.callable_return_contract
            && previous.closure != id
        {
            return Err(error(
                "E-TYPE-MISMATCH",
                span,
                "all opaque callable returns must have one hidden closure identity",
            ));
        }
        self.function.return_type = Type::Closure(id);
        self.function.callable_return_contract = Some(CallableReturnContract {
            lending_captures: Vec::new(),
            closure: id,
            kind: bound.kind,
            hold: Vec::new(),
            call: Vec::new(),
            writes: Vec::new(),
            cleanup,
        });
        self.materialize_closure(id)?;
        self.return_value(Some(value), span)
    }
}
