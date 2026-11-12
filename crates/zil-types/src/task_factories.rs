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
        && f.parameters.iter().any(|p| retained_input(&p.ty))
}

fn retained_input(ty: &Type) -> bool {
    matches!(ty, Type::Address { inner, .. } if tasks::retained_pointee(inner))
}

pub(super) fn signature_factory(signature: &Signature) -> bool {
    !signature.foreign
        && signature.template.is_none()
        && result_task(&signature.result).is_some()
        && signature.parameters.iter().any(retained_input)
}

pub(super) fn replace_task(ty: &mut Type, actual: &Type, span: Span) -> R<()> {
    match ty {
        Type::Task {
            result,
            retained_life,
        } => {
            let Type::Task {
                result: wanted,
                retained_life: context,
            } = actual
            else {
                unreachable!()
            };
            if result != wanted
                || (*retained_life != tasks::empty_context() && retained_life != context)
            {
                return Err(error(
                    "E-TASK-RETURN-CONTEXT",
                    span,
                    "Task result or explicit retained context differs from its actual producer",
                ));
            }
            *retained_life = context.clone();
        }
        Type::Managed(inner) | Type::Desc(inner) => replace_task(inner, actual, span)?,
        Type::ErrorUnion { result, .. } => replace_task(result, actual, span)?,
        _ => {
            return Err(unsupported(
                span,
                "Task return context has no actual carrier",
            ));
        }
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
        *self.work = self.work.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "Task factory context budget exceeded",
                Some(span),
            )
        })?;
        Ok(())
    }
    fn snapshot(&mut self, ty: &Type, span: Span) -> R<Type> {
        self.pay(inference::type_snapshot_work(ty, span, self.limits)?, span)?;
        Ok(ty.clone())
    }
    fn callee(
        &self,
        e: &CoreExpr,
        locals: &BTreeMap<String, Option<Type>>,
    ) -> Option<&'a Signature> {
        match &e.kind {
            CoreExprKind::Name(name) if !locals.contains_key(name) => self.signatures.get(name),
            CoreExprKind::Member { object, member, .. } => {
                let CoreExprKind::Name(alias) = &object.kind else {
                    return None;
                };
                if locals.contains_key(alias) {
                    return None;
                }
                let imports = self.imports?;
                imports
                    .signatures
                    .get(imports.aliases.get(alias)?)?
                    .get(member)
            }
            _ => None,
        }
    }
    fn expr(
        &mut self,
        e: &mut CoreExpr,
        locals: &mut BTreeMap<String, Option<Type>>,
        depth: usize,
    ) -> R<Option<Type>> {
        self.pay(1, e.span)?;
        if depth > self.limits.max_call_depth {
            return Err(unsupported(e.span, "Task factory context depth exceeded"));
        }
        match &mut e.kind {
            CoreExprKind::Name(name) => {
                let ty = locals.get(name).and_then(Option::as_ref);
                ty.map(|ty| self.snapshot(ty, e.span)).transpose()
            }
            CoreExprKind::Call { callee, arguments } => {
                if let CoreExprKind::Name(name) = &callee.kind
                    && matches!(name.as_str(), "task_create" | "task_create_owned")
                    && !locals.contains_key(name)
                {
                    let worker = arguments
                        .first()
                        .and_then(|worker| self.callee(worker, locals))
                        .ok_or_else(|| {
                            unsupported(
                                e.span,
                                "Task factory worker must resolve to an actual static signature",
                            )
                        })?;
                    let cost = inference::signature_snapshot_work("", worker, e.span, self.limits)?;
                    self.pay(cost, e.span)?;
                    let result = worker.result.clone();
                    let parameters = worker.parameters.clone();
                    if result_task(&result).is_some() {
                        return Err(unsupported(
                            e.span,
                            "nested Task factory results require their complete dependency profile",
                        ));
                    }
                    return Ok(Some(tasks::call_create_type(
                        name,
                        &result,
                        tasks::borrowed_context_with_life(
                            &parameters,
                            worker
                                .life
                                .clone()
                                .or_else(|| {
                                    matches!(result, Type::Address { mutable: false, .. })
                                        .then(|| {
                                            worker
                                                .storage_life
                                                .as_ref()
                                                .and_then(|life| life.to_flat(parameters.len()))
                                        })
                                        .flatten()
                                })
                                .as_ref(),
                            &result,
                        ),
                        &parameters,
                    )));
                }
                let Some(signature) = self.callee(callee, locals) else {
                    return Ok(None);
                };
                let cost = inference::type_snapshot_work(&signature.result, e.span, self.limits)?;
                self.pay(cost, e.span)?;
                let result = signature.result.clone();
                Ok(Some(result))
            }
            CoreExprKind::Try(value) => Ok(match self.expr(value, locals, depth + 1)? {
                Some(Type::ErrorUnion { result, .. }) => Some(*result),
                _ => None,
            }),
            CoreExprKind::Unary {
                op: UnaryOp::Give,
                operand,
            } => self.expr(operand, locals, depth + 1),
            CoreExprKind::Ascribed { ty, value } => {
                let actual = self.expr(value, locals, depth + 1)?;
                if let Some(task) = actual.as_ref().and_then(result_task)
                    && result_task(ty).is_some()
                {
                    replace_task(ty, task, e.span)?;
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
            CoreExprKind::If {
                then_value,
                else_value,
                ..
            } => {
                let left = self.expr(then_value, locals, depth + 1)?;
                let right = self.expr(else_value, locals, depth + 1)?;
                match (left, right) {
                    (Some(a), Some(b)) if a == b => Ok(Some(a)),
                    _ => Err(unsupported(
                        e.span,
                        "conditional Task factory values need one complete result context",
                    )),
                }
            }
            _ => Ok(None),
        }
    }
    fn copy_locals(
        &mut self,
        locals: &BTreeMap<String, Option<Type>>,
        span: Span,
    ) -> R<BTreeMap<String, Option<Type>>> {
        for (name, ty) in locals {
            self.pay(name.len() + 1, span)?;
            if let Some(ty) = ty {
                self.pay(inference::type_snapshot_work(ty, span, self.limits)?, span)?;
            }
        }
        Ok(locals.clone())
    }
    fn block(
        &mut self,
        block: &mut CoreBlock,
        locals: &mut BTreeMap<String, Option<Type>>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, block.span)?;
        if depth > self.limits.max_call_depth {
            return Err(unsupported(block.span, "Task factory block depth exceeded"));
        }
        for statement in &mut block.statements {
            self.pay(1, statement.span)?;
            match &mut statement.kind {
                CoreStatementKind::Binding {
                    name,
                    ty,
                    initializer,
                    ..
                } => {
                    let actual = initializer
                        .as_mut()
                        .map(|e| self.expr(e, locals, depth + 1))
                        .transpose()?
                        .flatten();
                    if let (Some(annotation), Some(task)) =
                        (ty.as_mut(), actual.as_ref().and_then(result_task))
                        && result_task(annotation).is_some()
                    {
                        replace_task(annotation, task, statement.span)?;
                    }
                    self.pay(name.len() + 1, statement.span)?;
                    locals.insert(name.clone(), actual);
                }
                CoreStatementKind::Return(Some(e)) => {
                    if let Some(actual) = self.expr(e, locals, depth + 1)?
                        && let Some(task) = result_task(&actual)
                    {
                        if self.returned.as_ref().is_some_and(|old| old != task) {
                            return Err(unsupported(
                                statement.span,
                                "all Task factory returns require one complete retained context",
                            ));
                        }
                        self.returned = Some(self.snapshot(task, statement.span)?);
                    }
                }
                CoreStatementKind::If {
                    then_body,
                    else_body,
                    ..
                } => {
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
                CoreStatementKind::Assign { .. }
                | CoreStatementKind::IfLet { .. }
                | CoreStatementKind::While { .. }
                | CoreStatementKind::WhileLet { .. }
                | CoreStatementKind::ForRange { .. }
                | CoreStatementKind::Match { .. }
                | CoreStatementKind::TupleBinding { .. } => {
                    return Err(unsupported(
                        statement.span,
                        "Task factory context inference currently requires straight-line bindings and ordinary if returns",
                    ));
                }
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
    // Closed factories retain their ordinary body profile. An external frame
    // port must originate in an actual, qualified address parameter.
    let mut context = tasks::borrowed_context_with_life(
        &function
            .parameters
            .iter()
            .map(|p| p.ty.clone())
            .collect::<Vec<_>>(),
        function.life.as_ref().map(borrowed_fp::from_life).as_ref(),
        &function.return_type,
    );
    if let Some(Type::Task { retained_life, .. }) = result_task(&function.return_type) {
        context.retained = retained_life.retained.clone();
    }
    if context.ports.is_empty() {
        return Ok(resolved);
    }
    let before = state.remaining_work();
    let mut remaining = before;
    let result = (|| {
        let mut scan = Scan {
            signatures,
            imports,
            work: &mut remaining,
            limits,
            returned: None,
        };
        // A rich Task return cannot always be projected to the old flat life
        // contract. Recover only the actual address-input classes from the full
        // signature; the complete inferred contract is still compared below.
        if let Some(life) = &function.storage_life {
            scan.pay(
                life.bindings
                    .len()
                    .saturating_add(life.classes.len())
                    .saturating_add(function.parameters.len()),
                function.span,
            )?;
            for row in &life.bindings {
                scan.pay(row.position.path.len(), function.span)?;
            }
            for row in &life.classes {
                scan.pay(row.position.path.len(), function.span)?;
            }
            let mut classes = BTreeMap::new();
            let mut port = 0;
            for (index, parameter) in function.parameters.iter().enumerate() {
                scan.pay(
                    inference::type_snapshot_work(&parameter.ty, parameter.span, limits)?,
                    parameter.span,
                )?;
                if !retained_input(&parameter.ty) {
                    continue;
                }
                scan.pay(
                    life.bindings
                        .len()
                        .saturating_add(classes.len())
                        .saturating_add(1),
                    parameter.span,
                )?;
                let mut rows = life.bindings.iter().filter(|row| {
                    row.position.base == zil_ir::StorageBase::Parameter(index as u32)
                        && row.position.path.is_empty()
                });
                let source = match (rows.next(), rows.next()) {
                    (Some(row), None) => match row.binder {
                        zil_ir::StorageBinder::Invocation(class)
                            if class < life.invocation_binder_count =>
                        {
                            class
                        }
                        _ => {
                            return Err(unsupported(
                                parameter.span,
                                "Task factory input requires its actual invocation class",
                            ));
                        }
                    },
                    _ => {
                        return Err(unsupported(
                            parameter.span,
                            "Task factory input requires one complete declared class",
                        ));
                    }
                };
                if function
                    .life
                    .as_ref()
                    .is_some_and(|old| old.parameter_lives.get(index) != Some(&Some(source)))
                {
                    return Err(unsupported(
                        parameter.span,
                        "Task factory flat and full input classes differ",
                    ));
                }
                let next = classes.len() as u32;
                let binder = *classes.entry(source).or_insert(next);
                context.ports[port].binder = binder;
                port += 1;
            }
            context.binder_count = classes.len() as u32;
        }
        let mut locals = BTreeMap::new();
        for parameter in &function.parameters {
            scan.pay(parameter.name.len() + 1, parameter.span)?;
            locals.insert(
                parameter.name.clone(),
                Some(scan.snapshot(&parameter.ty, parameter.span)?),
            );
        }
        scan.block(&mut resolved.body, &mut locals, 0)?;
        let Some(task) = scan.returned else {
            return Ok(());
        };
        replace_task(&mut resolved.return_type, &task, function.span)?;
        let Type::Task { retained_life, .. } = task else {
            unreachable!()
        };
        if retained_life == tasks::empty_context() {
            return Ok(());
        }
        let inputs = function
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(index, parameter)| retained_input(&parameter.ty).then_some(index))
            .collect::<Vec<_>>();
        if retained_life != context || inputs.len() != retained_life.ports.len() {
            return Err(unsupported(
                function.span,
                "borrowed Task factory result differs from its actual input ports",
            ));
        }
        if let Some(life) = &function.life
            && (life.result_life.is_some()
                || life.parameter_lives.len() != function.parameters.len()
                || inputs.iter().enumerate().any(|(port, input)| {
                    life.parameter_lives[*input] != Some(retained_life.ports[port].binder)
                }))
        {
            return Err(unsupported(
                function.span,
                "Task factory input lifetime classes differ from retained ports",
            ));
        }
        let mut port_path = Vec::new();
        let mut class_path = Vec::new();
        if matches!(resolved.return_type, Type::ErrorUnion { .. }) {
            port_path.push(zil_ir::ContractProjection::Place(
                zil_ir::PlaceProjection::ErrorArm(false),
            ));
            class_path.push(zil_ir::LifeTypeProjection::Place(
                zil_ir::PlaceProjection::ErrorArm(false),
            ));
        }
        class_path.push(zil_ir::LifeTypeProjection::Pointee);
        let mut bindings = Vec::new();
        let mut classes = Vec::new();
        for (port, input) in inputs.iter().enumerate() {
            let binder = zil_ir::StorageBinder::Invocation(retained_life.ports[port].binder);
            bindings.push(zil_ir::StorageLifeBinding {
                position: zil_ir::StoragePosition {
                    base: zil_ir::StorageBase::Parameter(*input as u32),
                    path: vec![],
                },
                binder,
            });
        }
        for (port, _) in inputs.iter().enumerate() {
            let binder = zil_ir::StorageBinder::Invocation(retained_life.ports[port].binder);
            let mut path = port_path.clone();
            path.push(zil_ir::ContractProjection::RetainedPort(port as u32));
            bindings.push(zil_ir::StorageLifeBinding {
                position: zil_ir::StoragePosition {
                    base: zil_ir::StorageBase::Result,
                    path,
                },
                binder,
            });
        }
        for class in 0..retained_life.binder_count {
            classes.push(zil_ir::SignatureLifeClassBinding {
                position: zil_ir::SignatureLifeClass {
                    base: zil_ir::StorageBase::Result,
                    path: class_path.clone(),
                    class,
                },
                binder: zil_ir::StorageBinder::Invocation(class),
            });
        }
        let inferred = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: retained_life.binder_count,
            bindings,
            classes,
        };
        if function
            .storage_life
            .as_ref()
            .is_some_and(|actual| actual != &inferred)
        {
            return Err(unsupported(
                function.span,
                "Task factory declared relation differs from actual port sources",
            ));
        }
        resolved.storage_life = Some(inferred);
        resolved.life = None;
        Ok(())
    })();
    state.snapshot(before - remaining, function.span)?;
    result?;
    Ok(resolved)
}
