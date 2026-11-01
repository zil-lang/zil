//! Complete ordinary Product replacement preserves its existing lexical shell.
//! All ownership, origins and loans still pass through ordinary checked IR.
use super::*;

impl Builder<'_, '_> {
    pub(super) fn replace_product(
        &mut self,
        local: LocalId,
        ty: &Type,
        rhs: &CoreExpr,
        span: Span,
    ) -> R<bool> {
        if !matches!(ty, Type::Package { .. }) {
            return Ok(false);
        }
        // The bounded type walk reserves its own visits before descent. Pay
        // both that inventory and the new qualification pass before inspecting
        // Product eligibility; scalar/field assignments never enter this path.
        let qualification = inference::type_snapshot_work(ty, span, self.limits)?;
        self.closures
            .snapshot(qualification.saturating_mul(2), span)?;
        if !packages::product::owned_supported(ty) {
            return Ok(false);
        }
        self.closures
            .snapshot(self.function.locals[local.0 as usize].name.len(), span)?;
        let name = self.function.locals[local.0 as usize].name.clone();
        let references = References {
            target: &name,
            state: self.closures,
            limits: self.limits,
        }
        .expr(rhs, false, 0)?;
        let rewritten;
        let rhs = if references {
            let CoreExprKind::Call { callee, arguments } = &rhs.kind else {
                return Err(unsupported(
                    rhs.span,
                    "Product self replacement requires a static call with one direct whole-target argument",
                ));
            };
            // Retained ordinary type templates use the same call dispatcher
            // as other source calls after the single self argument is rebuilt.
            let static_callee = match &callee.kind {
                CoreExprKind::TypeApply { callee, .. } => callee.as_ref(),
                _ => callee.as_ref(),
            };
            let direct = match &static_callee.kind {
                CoreExprKind::Name(name) => self.lookup(name).is_none(),
                CoreExprKind::Member { object, .. } => {
                    matches!(&object.kind, CoreExprKind::Name(name) if self.lookup(name).is_none())
                }
                _ => false,
            };
            if !direct {
                return Err(unsupported(
                    rhs.span,
                    "Product self replacement requires an ordinary static function",
                ));
            }
            let signature = self.resolve_callee(static_callee)?;
            if signature.foreign
                || signature.cleanup
                || signature.receiver.is_some()
                || !signature.callable.is_empty()
                || signature.state_parameter.is_some()
            {
                return Err(unsupported(
                    rhs.span,
                    "Product self replacement requires an ordinary static function without callable or State roles",
                ));
            }
            let mut position = None;
            for (index, argument) in arguments.iter().enumerate() {
                let direct_target = matches!(&argument.kind, CoreExprKind::Name(value) if value == &name && self.lookup(value) == Some(local));
                if direct_target {
                    if position.replace(index).is_some() {
                        return Err(unsupported(
                            argument.span,
                            "Product self replacement cannot use its target more than once",
                        ));
                    }
                } else if (References {
                    target: &name,
                    state: self.closures,
                    limits: self.limits,
                })
                .expr(argument, false, 0)?
                {
                    return Err(unsupported(
                        argument.span,
                        "other Product replacement arguments cannot reference the target binding",
                    ));
                }
            }
            let Some(position) = position else {
                return Err(unsupported(
                    rhs.span,
                    "Product self replacement needs exactly one direct whole-target argument",
                ));
            };
            let work = inference::type_snapshot_work(ty, span, self.limits)?;
            let Type::Package { fields, .. } = ty else {
                unreachable!()
            };
            self.closures.snapshot(
                work.saturating_add(fields.len().saturating_mul(name.len().saturating_add(8))),
                span,
            )?;
            let argument = &arguments[position];
            let mut values = Vec::with_capacity(fields.len());
            for (field, field_type) in fields {
                let place = CoreExpr {
                    id: argument.id,
                    span: argument.span,
                    kind: CoreExprKind::Member {
                        object: Box::new(CoreExpr {
                            id: argument.id,
                            span: argument.span,
                            kind: CoreExprKind::Name(name.clone()),
                        }),
                        member: field.clone(),
                    },
                };
                let value = if matches!(field_type, Type::Managed(_)) {
                    CoreExpr {
                        id: argument.id,
                        span: argument.span,
                        kind: CoreExprKind::Unary {
                            op: UnaryOp::Give,
                            operand: Box::new(place),
                        },
                    }
                } else {
                    place
                };
                values.push((field.clone(), value));
            }
            // The bounded full traversal above paid for this source snapshot.
            let mut call = rhs.clone();
            let CoreExprKind::Call { arguments, .. } = &mut call.kind else {
                unreachable!()
            };
            arguments[position] = CoreExpr {
                id: argument.id,
                span: argument.span,
                kind: CoreExprKind::Record {
                    ty: ty.clone(),
                    fields: values,
                },
            };
            rewritten = call;
            &rewritten
        } else {
            rhs
        };
        let work = inference::type_snapshot_work(ty, span, self.limits)?;
        self.closures.snapshot(work.saturating_mul(3), span)?;
        let (value, actual) = self.transfer_expr(rhs, Some(ty))?;
        self.same(&actual, ty, rhs.span)?;
        // This independently checked full value protects all RHS obligations
        // while old fields execute their ordinary declared-order cleanup.
        let prepared = self.temp(ty.clone(), span)?;
        self.store(prepared, value, span)?;
        let Type::Package { fields, .. } = ty else {
            unreachable!()
        };
        for (index, (_, field)) in fields.iter().enumerate() {
            if matches!(field, Type::Managed(_) | Type::Package { .. }) {
                self.emit(
                    InstructionKind::DropPackageField {
                        local,
                        index: index as u32,
                    },
                    None,
                    span,
                )?;
            }
        }
        // No callback, recoverable failure or suspension occurs in publication.
        // Copy fields are published only after every old owning field is clear.
        for (index, (_, field)) in fields.iter().enumerate() {
            let index = index as u32;
            let kind = if matches!(field, Type::Managed(_) | Type::Package { .. }) {
                InstructionKind::TakePackageField {
                    local: prepared,
                    index,
                }
            } else {
                InstructionKind::ReadPackageField {
                    local: prepared,
                    index,
                }
            };
            let value = self.value(kind, field.clone(), span)?;
            self.emit(
                InstructionKind::StorePackageField {
                    local,
                    index,
                    value,
                },
                None,
                span,
            )?;
        }
        Ok(true)
    }
}

/// Only the target binding matters; lexical shadowing can be represented by
/// one bit. Every node is visited even after a reference is found, so cloning
/// the self-call never precedes its budget reservation.
struct References<'a, 's> {
    target: &'a str,
    state: &'s mut closures::State,
    limits: &'a Limits,
}
impl References<'_, '_> {
    fn charge(&mut self, depth: usize, span: Span, extra: usize) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "Product replacement expression depth exceeded",
                Some(span),
            ));
        }
        self.state.snapshot(extra.saturating_add(1), span)
    }
    fn ty(&mut self, ty: &Type, span: Span) -> R<()> {
        let work = inference::type_snapshot_work(ty, span, self.limits)?;
        self.state.snapshot(work, span)
    }
    fn pattern(&mut self, pattern: &CorePattern, depth: usize) -> R<bool> {
        self.charge(depth, pattern.span, 0)?;
        let mut shadows = false;
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                self.state.snapshot(name.len(), pattern.span)?;
                shadows = name == self.target;
            }
            CorePatternKind::Tuple(fields) => {
                for field in fields {
                    shadows |= self.pattern(field, depth + 1)?;
                }
            }
            CorePatternKind::Variant {
                ty,
                variant,
                fields,
            } => {
                self.ty(ty, pattern.span)?;
                self.state.snapshot(variant.len(), pattern.span)?;
                for field in fields {
                    shadows |= self.pattern(field, depth + 1)?;
                }
            }
            CorePatternKind::Integer(text) => self.state.snapshot(text.len(), pattern.span)?,
            CorePatternKind::Wildcard | CorePatternKind::Bool(_) => {}
        }
        Ok(shadows)
    }
    fn block(&mut self, block: &CoreBlock, mut shadow: bool, depth: usize) -> R<bool> {
        self.charge(depth, block.span, 0)?;
        let mut found = false;
        for statement in &block.statements {
            self.charge(depth + 1, statement.span, 0)?;
            match &statement.kind {
                CoreStatementKind::Binding {
                    name,
                    ty,
                    initializer,
                    ..
                } => {
                    self.state.snapshot(name.len(), statement.span)?;
                    if let Some(ty) = ty {
                        self.ty(ty, statement.span)?;
                    }
                    if let Some(value) = initializer {
                        found |= self.expr(value, shadow, depth + 1)?;
                    }
                    shadow |= name == self.target;
                }
                CoreStatementKind::Assign { target, value } => {
                    found |= self.expr(target, shadow, depth + 1)?;
                    found |= self.expr(value, shadow, depth + 1)?;
                }
                CoreStatementKind::Expression(value) | CoreStatementKind::Return(Some(value)) => {
                    found |= self.expr(value, shadow, depth + 1)?
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    found |= self.expr(condition, shadow, depth + 1)?;
                    found |= self.block(then_body, shadow, depth + 1)?;
                    if let Some(body) = else_body {
                        found |= self.block(body, shadow, depth + 1)?;
                    }
                }
                CoreStatementKind::IfLet {
                    pattern,
                    value,
                    then_body,
                    else_body,
                } => {
                    found |= self.expr(value, shadow, depth + 1)?;
                    let branch_shadow = shadow | self.pattern(pattern, depth + 1)?;
                    found |= self.block(then_body, branch_shadow, depth + 1)?;
                    if let Some(body) = else_body {
                        found |= self.block(body, shadow, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    found |= self.expr(condition, shadow, depth + 1)?;
                    found |= self.block(body, shadow, depth + 1)?;
                }
                CoreStatementKind::WhileLet {
                    pattern,
                    value,
                    body,
                } => {
                    found |= self.expr(value, shadow, depth + 1)?;
                    let body_shadow = shadow | self.pattern(pattern, depth + 1)?;
                    found |= self.block(body, body_shadow, depth + 1)?;
                }
                CoreStatementKind::ForRange {
                    name,
                    start,
                    end,
                    body,
                } => {
                    self.state.snapshot(name.len(), statement.span)?;
                    found |= self.expr(start, shadow, depth + 1)?;
                    found |= self.expr(end, shadow, depth + 1)?;
                    found |= self.block(body, shadow | (name == self.target), depth + 1)?;
                }
                CoreStatementKind::Match { value, arms } => {
                    found |= self.expr(value, shadow, depth + 1)?;
                    for arm in arms {
                        let branch_shadow = shadow | self.pattern(&arm.pattern, depth + 1)?;
                        found |= self.block(&arm.body, branch_shadow, depth + 1)?;
                    }
                }
                CoreStatementKind::Block(body)
                | CoreStatementKind::Unsafe(body)
                | CoreStatementKind::Defer(body) => found |= self.block(body, shadow, depth + 1)?,
                CoreStatementKind::Unsupported(text) => {
                    self.state.snapshot(text.len(), statement.span)?
                }
                CoreStatementKind::Return(None)
                | CoreStatementKind::Break
                | CoreStatementKind::Continue => {}
            }
        }
        Ok(found)
    }
    fn expr(&mut self, value: &CoreExpr, shadow: bool, depth: usize) -> R<bool> {
        self.charge(depth, value.span, 0)?;
        let mut found = false;
        match &value.kind {
            CoreExprKind::Name(name) => {
                self.state.snapshot(name.len(), value.span)?;
                found = !shadow && name == self.target;
            }
            CoreExprKind::Integer(text)
            | CoreExprKind::String(text)
            | CoreExprKind::ReceiverMember(text)
            | CoreExprKind::Unsupported(text) => self.state.snapshot(text.len(), value.span)?,
            CoreExprKind::Literal(Constant::WideString(text)) => {
                self.state.snapshot(text.len(), value.span)?
            }
            CoreExprKind::Ascribed { ty, value: inner }
            | CoreExprKind::IntegerConvert {
                target: ty,
                value: inner,
            } => {
                self.ty(ty, value.span)?;
                found |= self.expr(inner, shadow, depth + 1)?;
            }
            CoreExprKind::TypeApply {
                callee,
                type_arguments,
            } => {
                for ty in type_arguments {
                    self.ty(ty, value.span)?;
                }
                found |= self.expr(callee, shadow, depth + 1)?;
            }
            CoreExprKind::Unary { operand, .. }
            | CoreExprKind::Try(operand)
            | CoreExprKind::CompileTime(operand) => {
                found |= self.expr(operand, shadow, depth + 1)?
            }
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Index {
                object: left,
                index: right,
            } => {
                found |= self.expr(left, shadow, depth + 1)?;
                found |= self.expr(right, shadow, depth + 1)?;
            }
            CoreExprKind::Call { callee, arguments } => {
                found |= self.expr(callee, shadow, depth + 1)?;
                for arg in arguments {
                    found |= self.expr(arg, shadow, depth + 1)?;
                }
            }
            CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
                for value in values {
                    found |= self.expr(value, shadow, depth + 1)?;
                }
            }
            CoreExprKind::Member { object, member } => {
                self.state.snapshot(member.len(), value.span)?;
                found |= self.expr(object, shadow, depth + 1)?;
            }
            CoreExprKind::Slice { object, start, end } => {
                found |= self.expr(object, shadow, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    found |= self.expr(value, shadow, depth + 1)?;
                }
            }
            CoreExprKind::Record { ty, fields } => {
                self.ty(ty, value.span)?;
                for (name, value) in fields {
                    self.state.snapshot(name.len(), value.span)?;
                    found |= self.expr(value, shadow, depth + 1)?;
                }
            }
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                if let Some(ty) = ty {
                    self.ty(ty, value.span)?;
                }
                found |= self.expr(base, shadow, depth + 1)?;
                for (name, value) in fields {
                    self.state.snapshot(name.len(), value.span)?;
                    found |= self.expr(value, shadow, depth + 1)?;
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                let mut shadow = shadow;
                for (name, value) in bindings {
                    self.state.snapshot(name.len(), value.span)?;
                    found |= self.expr(value, shadow, depth + 1)?;
                    shadow |= name == self.target;
                }
                found |= self.expr(value, shadow, depth + 1)?;
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                found |= self.expr(condition, shadow, depth + 1)?;
                found |= self.expr(then_value, shadow, depth + 1)?;
                found |= self.expr(else_value, shadow, depth + 1)?;
            }
            CoreExprKind::Closure {
                parameters,
                return_type,
                body,
                ..
            } => {
                let mut shadow = shadow;
                for parameter in parameters {
                    self.state.snapshot(parameter.name.len(), parameter.span)?;
                    if let Some(ty) = &parameter.ty {
                        self.ty(ty, parameter.span)?;
                    }
                    shadow |= parameter.name == self.target;
                }
                if let Some(ty) = return_type {
                    self.ty(ty, value.span)?;
                }
                found |= self.block(body, shadow, depth + 1)?;
            }
            CoreExprKind::Literal(_) | CoreExprKind::Bool(_) | CoreExprKind::Unit => {}
        }
        Ok(found)
    }
}
