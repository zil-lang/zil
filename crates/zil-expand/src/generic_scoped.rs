//! Symbolic checking for the structured foreign callback primitive.
use super::*;
impl Check<'_, '_, '_> {
    pub(super) fn scoped_callback_block(&mut self, block: &Block, depth: usize) -> R<bool> {
        self.budget.node(block.span, depth)?;
        for statement in &block.statements {
            self.budget.node(statement.span, depth + 1)?;
            match &statement.kind {
                StmtKind::Return(Some(e))
                | StmtKind::Expression(e)
                | StmtKind::Binding { value: e, .. }
                    if matches!(&e.kind,ExprKind::Call{callee,..} if super::super::super::foreign::bare_name(callee)==Some("c_with_callback")) =>
                {
                    return Ok(true);
                }
                StmtKind::Unsafe(body)
                | StmtKind::Loop(body)
                | StmtKind::While { body, .. }
                | StmtKind::For { body, .. } => {
                    if self.scoped_callback_block(body, depth + 1)? {
                        return Ok(true);
                    }
                }
                StmtKind::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    if self.scoped_callback_block(then_body, depth + 1)? {
                        return Ok(true);
                    }
                    if let Some(ElseBranch::Block(body)) = else_body
                        && self.scoped_callback_block(body, depth + 1)?
                    {
                        return Ok(true);
                    }
                }
                _ => {}
            }
        }
        Ok(false)
    }
    pub(super) fn scoped_callback_call(
        &mut self,
        arguments: &[Argument],
        state: &mut State,
        span: Span,
    ) -> R<TypeRef> {
        let [register, close, callback, body] = arguments else {
            return Err(error(
                span,
                "c_with_callback requires four ordered arguments",
            ));
        };
        if arguments.iter().any(|a| a.spread) {
            return Err(error(
                span,
                "c_with_callback does not accept spread arguments",
            ));
        }
        if !self.unsafe_scope {
            return Err(error(
                span,
                "c_with_callback requires an explicit unsafe block",
            ));
        }
        for (argument, role) in [
            (register, zil_ir::ScopedCallbackRole::Register),
            (close, zil_ir::ScopedCallbackRole::Close),
        ] {
            self.tick(argument.span)?;
            let name = super::super::super::foreign::bare_name(&argument.value)
                .filter(|n| !state.locals.contains_key(*n))
                .ok_or_else(|| {
                    error(
                        argument.span,
                        "scoped callback role requires a static foreign declaration symbol",
                    )
                })?;
            self.budget.text(name, argument.span)?;
            let (_, function) = self.foreign_signatures.get(name).ok_or_else(|| {
                error(
                    argument.span,
                    "scoped callback role has no actual foreign declaration",
                )
            })?;
            for clause in &function.contract {
                self.budget.node(clause.span, 0)?;
                for term in &clause.terms {
                    for part in &term.path {
                        self.budget.text(part, clause.span)?;
                    }
                }
            }
            let (_, static_callbacks, actual) =
                super::super::super::foreign::noescape_contract(function)?;
            if actual != Some(role) || !static_callbacks {
                return Err(error(
                    argument.span,
                    "scoped callback role differs from its actual foreign declaration",
                ));
            }
        }
        let callback_type = self.expr(&callback.value, state, None, true)?;
        let TypeKind::Reference {
            mutable: true,
            inner,
            ..
        } = &group(&callback_type).kind
        else {
            return Err(error(
                callback.span,
                "scoped callback requires an exclusive callable address",
            ));
        };
        let bound = self
            .callable
            .as_ref()
            .and_then(|bounds| {
                bounds.bindings().find(|bound| {
                    callable::parameter_mode(inner, &bound.generic_name) == Some(None)
                })
            })
            .ok_or_else(|| {
                error(
                    callback.span,
                    "scoped callback must have a declared callable bound",
                )
            })?;
        if !matches!(
            bound.kind,
            zil_ir::CallableKind::Fn | zil_ir::CallableKind::FnMut
        ) || bound.parameters.len() != 1
        {
            return Err(error(
                callback.span,
                "scoped callback requires Fn or FnMut(i32)->i32",
            ));
        }
        same(&bound.parameters[0], &named("i32", span), span, self.budget)?;
        same(&bound.result, &named("i32", span), span, self.budget)?;
        let body_type = self.expr(&body.value, state, None, false)?;
        let bound = self
            .callable
            .as_ref()
            .and_then(|bounds| {
                bounds.bindings().find(|bound| {
                    callable::parameter_mode(&body_type, &bound.generic_name) == Some(None)
                })
            })
            .ok_or_else(|| error(body.span, "scoped body must have an owned callable bound"))?;
        if !bound.parameters.is_empty() {
            return Err(error(body.span, "scoped body requires ()->i32"));
        }
        same(&bound.result, &named("i32", span), span, self.budget)?;
        Ok(TypeRef {
            span,
            kind: TypeKind::Tuple(vec![named("bool", span), named("i32", span)]),
        })
    }
}
