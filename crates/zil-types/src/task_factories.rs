//! Resolve hidden Task contexts before ordinary strict body checking.
//! This is only a type/upper-bound proposal. Both kernels reconstruct roots.
use super::*;

pub(super) fn result_task(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Task { .. } => Some(ty),
        Type::Managed(inner) | Type::Desc(inner) => result_task(inner),
        Type::ErrorUnion { result, .. } => result_task(result),
        _ => None,
    }
}

pub(super) fn factory(f: &CoreFunction) -> bool {
    f.foreign.is_none()
        && f.callable_parameters.is_empty()
        && f.type_parameters.is_empty()
        && f.state_parameters.is_empty()
        && result_task(&f.return_type).is_some()
        && f.parameters.len() == 1
        && shared_input(&f.parameters[0].ty)
}

fn shared_input(ty: &Type) -> bool {
    matches!(ty, Type::Address { mutable: false, inner } if **inner == Type::I32)
}

pub(super) fn signature_factory(signature: &Signature) -> bool {
    !signature.foreign && signature.template.is_none()
        && result_task(&signature.result).is_some()
        && signature.parameters.len() == 1 && shared_input(&signature.parameters[0])
}

fn replace_task(ty: &mut Type, actual: &Type, span: Span) -> R<()> {
    match ty {
        Type::Task { result, retained_life } => {
            let Type::Task { result: wanted, retained_life: context } = actual else {
                unreachable!()
            };
            if result != wanted
                || (*retained_life != tasks::empty_context() && retained_life != context)
            {
                return Err(error("E-TASK-RETURN-CONTEXT", span, "Task result or explicit retained context differs from its actual producer"));
            }
            *retained_life = context.clone();
        }
        Type::Managed(inner) | Type::Desc(inner) => replace_task(inner, actual, span)?,
        Type::ErrorUnion { result, .. } => replace_task(result, actual, span)?,
        _ => return Err(unsupported(span, "Task return context has no actual carrier")),
    }
    Ok(())
}

struct Scan<'a, 'b> {
    signatures: &'a BTreeMap<String, Signature>,
    imports: Option<&'a program::Imports<'a>>,
    work: &'b mut usize,
    limits: &'a Limits,
    returned: Option<Type>,
}

impl<'a> Scan<'a, '_> {
    fn pay(&mut self, n: usize, span: Span) -> R<()> {
        *self.work = self.work.checked_sub(n).ok_or_else(|| Diagnostic::limit(Phase::Types, "Task factory context budget exceeded", Some(span)))?;
        Ok(())
    }
    fn snapshot(&mut self, ty: &Type, span: Span) -> R<Type> {
        self.pay(inference::type_snapshot_work(ty, span, self.limits)?, span)?;
        Ok(ty.clone())
    }
    fn callee(&self, e: &CoreExpr, locals: &BTreeMap<String, Option<Type>>) -> Option<&'a Signature> {
        match &e.kind {
            CoreExprKind::Name(name) if !locals.contains_key(name) => self.signatures.get(name),
            CoreExprKind::Member { object, member, .. } => {
                let CoreExprKind::Name(alias) = &object.kind else { return None };
                if locals.contains_key(alias) { return None; }
                let imports = self.imports?;
                imports.signatures.get(imports.aliases.get(alias)?)?.get(member)
            }
            _ => None,
        }
    }
    fn expr(&mut self, e: &mut CoreExpr, locals: &mut BTreeMap<String, Option<Type>>, depth: usize) -> R<Option<Type>> {
        self.pay(1, e.span)?;
        if depth > self.limits.max_call_depth { return Err(unsupported(e.span, "Task factory context depth exceeded")); }
        match &mut e.kind {
            CoreExprKind::Name(name) => {
                let ty = locals.get(name).and_then(Option::as_ref);
                ty.map(|ty| self.snapshot(ty, e.span)).transpose()
            }
            CoreExprKind::Call { callee, arguments } => {
                if matches!(&callee.kind, CoreExprKind::Name(name) if name == "task_create" && !locals.contains_key(name)) {
                    let worker = arguments.first().and_then(|worker| self.callee(worker, locals))
                        .ok_or_else(|| unsupported(e.span, "Task factory worker must resolve to an actual static signature"))?;
                    let cost = inference::signature_snapshot_work("", worker, e.span, self.limits)?;
                    self.pay(cost, e.span)?;
                    let result = worker.result.clone();
                    let parameters = worker.parameters.clone();
                    if result_task(&result).is_some() { return Err(unsupported(e.span, "nested Task factory results require their complete dependency profile")); }
                    return Ok(Some(tasks::create_type(&result, tasks::borrowed_context(&parameters))));
                }
                let Some(signature) = self.callee(callee, locals) else { return Ok(None) };
                let cost = inference::type_snapshot_work(&signature.result, e.span, self.limits)?;
                self.pay(cost, e.span)?;
                let result = signature.result.clone();
                Ok(Some(result))
            }
            CoreExprKind::Try(value) => {
                Ok(match self.expr(value, locals, depth + 1)? {
                    Some(Type::ErrorUnion { result, .. }) => Some(*result),
                    _ => None,
                })
            }
            CoreExprKind::Unary { op: UnaryOp::Give, operand } => self.expr(operand, locals, depth + 1),
            CoreExprKind::Ascribed { ty, value } => {
                let actual = self.expr(value, locals, depth + 1)?;
                if let Some(task) = actual.as_ref().and_then(result_task) {
                    if result_task(ty).is_some() { replace_task(ty, task, e.span)?; }
                }
                Ok(actual)
            }
            CoreExprKind::Sequence { bindings, value } => {
                let mut nested = self.copy_locals(locals, e.span)?;
                for (name, initializer) in bindings {
                    let ty = self.expr(initializer, &mut nested, depth + 1)?;
                    self.pay(name.len() + 1, e.span)?;
                    nested.insert(name.clone(), ty);
                }
                self.expr(value, &mut nested, depth + 1)
            }
            CoreExprKind::If { then_value, else_value, .. } => {
                let left = self.expr(then_value, locals, depth + 1)?;
                let right = self.expr(else_value, locals, depth + 1)?;
                match (left, right) {
                    (Some(a), Some(b)) if a == b => Ok(Some(a)),
                    _ => Err(unsupported(e.span, "conditional Task factory values need one complete result context")),
                }
            }
            _ => Ok(None),
        }
    }
    fn copy_locals(&mut self, locals: &BTreeMap<String, Option<Type>>, span: Span) -> R<BTreeMap<String, Option<Type>>> {
        for (name, ty) in locals {
            self.pay(name.len() + 1, span)?;
            if let Some(ty) = ty { self.pay(inference::type_snapshot_work(ty, span, self.limits)?, span)?; }
        }
        Ok(locals.clone())
    }
    fn block(&mut self, block: &mut CoreBlock, locals: &mut BTreeMap<String, Option<Type>>, depth: usize) -> R<()> {
        self.pay(1, block.span)?;
        if depth > self.limits.max_call_depth { return Err(unsupported(block.span, "Task factory block depth exceeded")); }
        for statement in &mut block.statements {
            self.pay(1, statement.span)?;
            match &mut statement.kind {
                CoreStatementKind::Binding { name, ty, initializer, .. } => {
                    let actual = initializer.as_mut().map(|e| self.expr(e, locals, depth + 1)).transpose()?.flatten();
                    if let (Some(annotation), Some(task)) = (ty.as_mut(), actual.as_ref().and_then(result_task)) {
                        if result_task(annotation).is_some() { replace_task(annotation, task, statement.span)?; }
                    }
                    self.pay(name.len() + 1, statement.span)?;
                    locals.insert(name.clone(), actual);
                }
                CoreStatementKind::Return(Some(e)) => {
                    if let Some(actual) = self.expr(e, locals, depth + 1)? {
                        if let Some(task) = result_task(&actual) {
                            if self.returned.as_ref().is_some_and(|old| old != task) {
                                return Err(unsupported(statement.span, "all Task factory returns require one complete retained context"));
                            }
                            self.returned = Some(self.snapshot(task, statement.span)?);
                        }
                    }
                }
                CoreStatementKind::If { then_body, else_body, .. } => {
                    let mut nested = self.copy_locals(locals, statement.span)?;
                    self.block(then_body, &mut nested, depth + 1)?;
                    if let Some(body) = else_body {
                        let mut nested = self.copy_locals(locals, statement.span)?;
                        self.block(body, &mut nested, depth + 1)?;
                    }
                }
                CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                    let mut nested = self.copy_locals(locals, statement.span)?;
                    self.block(body, &mut nested, depth + 1)?;
                }
                CoreStatementKind::Assign { .. } | CoreStatementKind::IfLet { .. }
                | CoreStatementKind::While { .. } | CoreStatementKind::WhileLet { .. }
                | CoreStatementKind::ForRange { .. } | CoreStatementKind::Match { .. }
                | CoreStatementKind::TupleBinding { .. } => return Err(unsupported(statement.span, "Task factory context inference currently requires straight-line bindings and ordinary if returns")),
                _ => {}
            }
        }
        Ok(())
    }
}

pub(super) fn prepare(
    function: &CoreFunction,
    signatures: &BTreeMap<String, Signature>,
    imports: Option<&program::Imports<'_>>,
    state: &mut closures::State,
    limits: &Limits,
) -> R<CoreFunction> {
    let cost = inference::helper_snapshot_work(function, limits)?.0;
    state.snapshot(cost, function.span)?;
    let mut resolved = function.clone();
    // Existing closed factories retain their ordinary body profile. The new
    // inferred borrowed boundary has exactly one possible external input.
    let shared = Type::Address { mutable: false, inner: Box::new(Type::I32) };
    if function.parameters.len() != 1 || function.parameters[0].ty != shared {
        return Ok(resolved);
    }
    let before = state.remaining_work();
    let mut remaining = before;
    let result = (|| {
        let mut scan = Scan { signatures, imports, work: &mut remaining, limits, returned: None };
        let mut locals = BTreeMap::new();
        for parameter in &function.parameters {
            scan.pay(parameter.name.len() + 1, parameter.span)?;
            locals.insert(parameter.name.clone(), Some(scan.snapshot(&parameter.ty, parameter.span)?));
        }
        scan.block(&mut resolved.body, &mut locals, 0)?;
        let Some(task) = scan.returned else { return Ok(()) };
        replace_task(&mut resolved.return_type, &task, function.span)?;
        let Type::Task { retained_life, .. } = task else { unreachable!() };
        if retained_life == tasks::empty_context() { return Ok(()); }
        let shared = Type::Address { mutable: false, inner: Box::new(Type::I32) };
        if function.parameters.len() != 1 || function.parameters[0].ty != shared {
            return Err(unsupported(function.span, "borrowed Task factory requires its one shared i32 source parameter"));
        }
        if function.storage_life.is_some() {
            return Err(unsupported(function.span, "Task factory inference cannot replace an existing rich source contract"));
        }
        if let Some(life) = &function.life {
            if life.binders.len() != 1 || life.parameter_lives != [Some(0)] || life.result_life.is_some() {
                return Err(unsupported(function.span, "Task factory requires one declared input lifetime class"));
            }
        }
        let mut port_path = Vec::new();
        let mut class_path = Vec::new();
        if matches!(resolved.return_type, Type::ErrorUnion { .. }) {
            port_path.push(zil_ir::ContractProjection::Place(zil_ir::PlaceProjection::ErrorArm(false)));
            class_path.push(zil_ir::LifeTypeProjection::Place(zil_ir::PlaceProjection::ErrorArm(false)));
        }
        port_path.push(zil_ir::ContractProjection::RetainedPort(0));
        class_path.push(zil_ir::LifeTypeProjection::Pointee);
        resolved.storage_life = Some(StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 1,
            bindings: vec![
                zil_ir::StorageLifeBinding { position: zil_ir::StoragePosition { base: zil_ir::StorageBase::Parameter(0), path: vec![] }, binder: zil_ir::StorageBinder::Invocation(0) },
                zil_ir::StorageLifeBinding { position: zil_ir::StoragePosition { base: zil_ir::StorageBase::Result, path: port_path }, binder: zil_ir::StorageBinder::Invocation(0) },
            ],
            classes: vec![zil_ir::SignatureLifeClassBinding { position: zil_ir::SignatureLifeClass { base: zil_ir::StorageBase::Result, path: class_path, class: 0 }, binder: zil_ir::StorageBinder::Invocation(0) }],
        });
        resolved.life = None;
        Ok(())
    })();
    state.snapshot(before - remaining, function.span)?;
    result?;
    Ok(resolved)
}
