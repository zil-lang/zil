//! Infer the invocation of an unannotated lexical value from all original uses.
//! A call constrains its parameter and result types, but cannot independently
//! establish its own callee's ABI.
use super::*;

pub(super) struct DeferredCodeCall {
    pub owner: SemanticDefinitionId,
    pub span: zil_base::Span,
    pub callee: Ty,
    pub arguments: Vec<Ty>,
    pub result: Ty,
}

impl CheckedOriginal {
    pub(super) fn solve_code_calls(&mut self, b: &mut Budget) -> Check<()> {
        let mut pending = std::mem::take(&mut self.deferred_code_calls);
        while !pending.is_empty() {
            let previous_len = pending.len();
            let mut next = Vec::new();
            for call in pending {
                b.spend(1)?;
                let callee = self.resolved(&call.callee, b)?;
                if matches!(callee, Ty::Infer(_)) {
                    next.push(call);
                    continue;
                }
                let previous = self.current.replace(call.owner);
                let checked = self.check_code_call(&call, callee, b);
                self.current = previous;
                checked.map_err(|mut error| {
                    error.span = Some(call.span);
                    error
                })?;
            }
            if next.len() == previous_len {
                return Err(unsupported(
                    "original indirect invocation has no independently established callee signature",
                    Some(next[0].span),
                ));
            }
            pending = next;
        }
        Ok(())
    }

    fn check_code_call(
        &mut self,
        call: &DeferredCodeCall,
        callee: Ty,
        b: &mut Budget,
    ) -> Check<()> {
        let Ty::Code {
            kind: SemanticCodeKind::FunctionPointer,
            parameters,
            result,
            modes,
            result_mode: PassingMode::Infer,
            call_kind: CallKind::Direct,
            bound_lifetimes: 0,
            invocation_lifetimes: 0,
        } = callee
        else {
            return Err(unsupported(
                "inferred indirect invocation requires a direct value-only function signature",
                Some(call.span),
            ));
        };
        b.spend(parameters.len().saturating_add(modes.len()))?;
        if parameters.len() != call.arguments.len()
            || parameters.len() != modes.len()
            || modes.iter().any(|mode| *mode != PassingMode::Infer)
        {
            return Err(bad(
                "original inferred invocation disagrees with its complete arity or passing modes",
            ));
        }
        for parameter in parameters.iter().chain(std::iter::once(result.as_ref())) {
            self.map_invocation_type(
                parameter,
                &mut |_, _| Err(bad("inferred direct value invocation retains a lifetime")),
                0,
                b,
            )?;
        }
        for (argument, parameter) in call.arguments.iter().zip(&parameters) {
            self.unify(argument, parameter, b)?;
        }
        self.unify(&call.result, &result, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_calls_require_an_independent_callee_and_relate_every_operand() {
        let limits = Limits::default();
        for fault in 0..6 {
            let mut b = Budget::new(&limits);
            let mut original = CheckedOriginal::default();
            let callee = original.fresh(&mut b).unwrap();
            let input = original.fresh_integer(&mut b).unwrap();
            let result = original.fresh(&mut b).unwrap();
            original.deferred_code_calls.push(DeferredCodeCall {
                owner: SemanticDefinitionId {
                    module: zil_base::ModuleId(1),
                    index: 4,
                },
                span: zil_base::Span::default(),
                callee: callee.clone(),
                arguments: vec![input.clone()],
                result: result.clone(),
            });
            let real = Ty::Code {
                kind: SemanticCodeKind::FunctionPointer,
                parameters: if fault == 2 {
                    vec![]
                } else {
                    vec![Ty::Atom(Type::I64)]
                },
                result: Box::new(Ty::Atom(Type::Bool)),
                modes: vec![if fault == 3 {
                    PassingMode::Move
                } else {
                    PassingMode::Infer
                }],
                result_mode: PassingMode::Infer,
                call_kind: if fault == 4 {
                    CallKind::Suspend
                } else {
                    CallKind::Direct
                },
                bound_lifetimes: 0,
                invocation_lifetimes: 0,
            };
            if fault != 1 {
                original.unify(&callee, &real, &mut b).unwrap();
            }
            if fault == 5 {
                original
                    .unify(&result, &Ty::Atom(Type::I64), &mut b)
                    .unwrap();
            }
            assert_eq!(original.solve_code_calls(&mut b).is_ok(), fault == 0);
            if fault == 0 {
                assert_eq!(
                    original.resolved(&input, &mut b).unwrap(),
                    Ty::Atom(Type::I64)
                );
                assert_eq!(
                    original.resolved(&result, &mut b).unwrap(),
                    Ty::Atom(Type::Bool)
                );
            }
        }
    }
}
