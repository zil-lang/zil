//! Types checks the Core cleanup graph; typed kernels reconstruct it again.
use super::*;
use zil_base::NodeId;

fn fee(left: &mut usize, count: usize, span: Span) -> R<()> {
    *left = left
        .checked_sub(count)
        .ok_or_else(|| Diagnostic::limit(Phase::Types, "dyn cleanup work exceeded", Some(span)))?;
    Ok(())
}
fn effect(bound: &DynCleanupContract, e: DynEffect, span: Span) -> R<()> {
    if bound.effects.contains(&e) {
        Ok(())
    } else {
        Err(unsupported(
            span,
            "owned dyn concrete cleanup exceeds its declared effects",
        ))
    }
}

pub(super) fn prove(
    ty: &Type,
    modules: &[&CoreModule],
    bound: &DynCleanupContract,
    left: &mut usize,
    limits: &Limits,
    depth: usize,
) -> R<()> {
    if depth > limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "dyn cleanup type depth exceeded",
            None,
        ));
    }
    fee(left, 1, Span::default())?;
    match ty {
        Type::Managed(payload) => match payload.as_ref() {
            Type::ByteBuffer => Ok(()),
            Type::OwnedStore { element } => prove(element, modules, bound, left, limits, depth + 1),
            Type::Record { name, .. } => {
                for module in modules {
                    fee(left, module.aggregates.len(), Span::default())?;
                    for a in &module.aggregates {
                        fee(
                            left,
                            name.len().saturating_add(a.name.len()).saturating_add(32),
                            a.span,
                        )?;
                        if *name == format!("@module{}:{}", module.module_id.0, a.name)
                            && a.kind == AggregateKind::Resource
                        {
                            if let Some(drop) = &a.teardown {
                                fee(left, module.functions.len(), a.span)?;
                                let f = module
                                    .functions
                                    .iter()
                                    .find(|f| f.name == *drop)
                                    .ok_or_else(|| {
                                        unsupported(
                                            a.span,
                                            "owned dyn concrete teardown body absent",
                                        )
                                    })?;
                                function(module, f, bound, left, limits, &mut Vec::new())?;
                            }
                            return Ok(());
                        }
                    }
                }
                Err(unsupported(
                    Span::default(),
                    "owned dyn cleanup lacks its exact concrete resource identity",
                ))
            }
            _ => Err(unsupported(
                Span::default(),
                "owned dyn cleanup resource is unknown",
            )),
        },
        Type::Package { fields, .. } | Type::Record { fields, .. } => {
            for (_, field) in fields {
                prove(field, modules, bound, left, limits, depth + 1)?;
            }
            Ok(())
        }
        Type::Tuple(fields) => {
            for field in fields {
                prove(field, modules, bound, left, limits, depth + 1)?;
            }
            Ok(())
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    prove(field, modules, bound, left, limits, depth + 1)?;
                }
            }
            Ok(())
        }
        Type::Array { element, .. } => prove(element, modules, bound, left, limits, depth + 1),
        _ if ty.is_b0_scalar() || *ty == Type::Unit => Ok(()),
        _ => Err(unsupported(
            Span::default(),
            "owned dyn cleanup retains unproved state",
        )),
    }
}
fn function(
    module: &CoreModule,
    f: &CoreFunction,
    bound: &DynCleanupContract,
    left: &mut usize,
    limits: &Limits,
    active: &mut Vec<NodeId>,
) -> R<()> {
    fee(left, active.len().saturating_add(1), f.span)?;
    if active.len() > limits.max_call_depth || active.contains(&f.id) {
        return Err(unsupported(
            f.span,
            "owned dyn cleanup call graph is cyclic or unknown",
        ));
    }
    if f.call_kind.is_some_and(|k| k != CallKind::Direct)
        || !f.type_parameters.is_empty()
        || !f.callable_parameters.is_empty()
        || !f.state_parameters.is_empty()
    {
        return Err(unsupported(
            f.span,
            "owned dyn cleanup target has unknown callable or suspension semantics",
        ));
    }
    if f.foreign.is_some() {
        for e in [
            DynEffect::Alloc,
            DynEffect::Ffi,
            DynEffect::Io,
            DynEffect::Block,
            DynEffect::Reenter,
            DynEffect::Panic,
        ] {
            effect(bound, e, f.span)?;
        }
        return Ok(());
    }
    if !f.type_parameters.is_empty()
        || !f.callable_parameters.is_empty()
        || !f.state_parameters.is_empty()
    {
        return Err(unsupported(
            f.span,
            "owned dyn cleanup has an unfixed callable",
        ));
    }
    active.push(f.id);
    block(module, &f.body, bound, left, limits, active, 0)?;
    active.pop();
    Ok(())
}
fn block(
    module: &CoreModule,
    b: &CoreBlock,
    bound: &DynCleanupContract,
    left: &mut usize,
    limits: &Limits,
    active: &mut Vec<NodeId>,
    depth: usize,
) -> R<()> {
    if depth > limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "dyn cleanup body depth exceeded",
            Some(b.span),
        ));
    }
    for s in &b.statements {
        fee(left, 1, s.span)?;
        match &s.kind {
            CoreStatementKind::Expression(e) | CoreStatementKind::Return(Some(e)) => {
                expr(module, e, bound, left, limits, active, depth + 1)?
            }
            CoreStatementKind::Return(None)
            | CoreStatementKind::Break
            | CoreStatementKind::Continue => {}
            CoreStatementKind::Binding { initializer, .. } => {
                if let Some(e) = initializer {
                    expr(module, e, bound, left, limits, active, depth + 1)?;
                }
            }
            CoreStatementKind::Assign { target, value } => {
                expr(module, target, bound, left, limits, active, depth + 1)?;
                expr(module, value, bound, left, limits, active, depth + 1)?;
            }
            CoreStatementKind::Block(b) | CoreStatementKind::Unsafe(b) => {
                block(module, b, bound, left, limits, active, depth + 1)?
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                expr(module, condition, bound, left, limits, active, depth + 1)?;
                block(module, then_body, bound, left, limits, active, depth + 1)?;
                if let Some(b) = else_body {
                    block(module, b, bound, left, limits, active, depth + 1)?;
                }
            }
            CoreStatementKind::While { condition, body } => {
                expr(module, condition, bound, left, limits, active, depth + 1)?;
                block(module, body, bound, left, limits, active, depth + 1)?;
            }
            _ => {
                return Err(unsupported(
                    s.span,
                    "owned dyn cleanup statement has unknown effects",
                ));
            }
        }
    }
    Ok(())
}
fn expr(
    module: &CoreModule,
    e: &CoreExpr,
    bound: &DynCleanupContract,
    left: &mut usize,
    limits: &Limits,
    active: &mut Vec<NodeId>,
    depth: usize,
) -> R<()> {
    if depth > limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "dyn cleanup expression depth exceeded",
            Some(e.span),
        ));
    }
    fee(left, 1, e.span)?;
    match &e.kind {
        CoreExprKind::Unit
        | CoreExprKind::Integer(_)
        | CoreExprKind::Float(_)
        | CoreExprKind::Bool(_)
        | CoreExprKind::Literal(_)
        | CoreExprKind::Name(_)
        | CoreExprKind::ReceiverMember(_) => Ok(()),
        CoreExprKind::Member { object, .. } | CoreExprKind::Ascribed { value: object, .. } => {
            expr(module, object, bound, left, limits, active, depth + 1)
        }
        CoreExprKind::Binary {
            left: a, right: b, ..
        } => {
            effect(bound, DynEffect::Panic, e.span)?;
            expr(module, a, bound, left, limits, active, depth + 1)?;
            expr(module, b, bound, left, limits, active, depth + 1)
        }
        CoreExprKind::Unary {
            op: UnaryOp::Negate | UnaryOp::Not | UnaryOp::BitNot,
            operand,
        } => {
            effect(bound, DynEffect::Panic, e.span)?;
            expr(module, operand, bound, left, limits, active, depth + 1)
        }
        CoreExprKind::Call { callee, arguments } => {
            for arg in arguments {
                expr(module, arg, bound, left, limits, active, depth + 1)?;
            }
            let CoreExprKind::Name(name) = &callee.kind else {
                return Err(unsupported(
                    callee.span,
                    "owned dyn cleanup call needs an exact direct target",
                ));
            };
            fee(left, module.functions.len(), e.span)?;
            let mut matches = module.functions.iter().filter(|f| f.name == *name);
            let target = matches
                .next()
                .ok_or_else(|| unsupported(e.span, "owned dyn cleanup direct target is absent"))?;
            if matches.next().is_some() {
                return Err(unsupported(
                    e.span,
                    "owned dyn cleanup direct target is ambiguous",
                ));
            }
            function(module, target, bound, left, limits, active)
        }
        CoreExprKind::Tuple(fields) => {
            for field in fields {
                expr(module, field, bound, left, limits, active, depth + 1)?;
            }
            Ok(())
        }
        _ => Err(unsupported(
            e.span,
            "owned dyn cleanup expression has unknown effects",
        )),
    }
}
