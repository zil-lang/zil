//! Bind owned original-bound rows to the actual function/role lowering event.
use super::*;

#[derive(Clone, Debug)]
pub(super) enum CallableContextEmission {
    Parameter {
        creator: NodeId,
        item: usize,
        parameter: u32,
        generic: u32,
        bound: u32,
    },
    Return {
        creator: NodeId,
        item: usize,
    },
}

impl generics::products::Products {
    pub(crate) fn bind_callable_contexts(
        &mut self,
        module: &s::Module,
        expanded: &mut ExpandedModule,
        limits: &Limits,
    ) -> CompileResult<()> {
        if expanded.callable_context_emissions.is_empty() {
            return Ok(());
        }
        let mut budget = generics::Budget::new(self.allowance.as_ref().unwrap_or(limits));
        let result = (|| {
            let origins = self.origins.as_ref().ok_or_else(|| {
                Diagnostic::unsupported(
                    Phase::Expand,
                    "source helper original lineage is absent",
                    None,
                )
            })?;
            for emission in std::mem::take(&mut expanded.callable_context_emissions) {
                budget.node(module.span, 0)?;
                let (creator, item_ordinal) = match &emission {
                    CallableContextEmission::Parameter { creator, item, .. }
                    | CallableContextEmission::Return { creator, item } => (*creator, *item),
                };
                let item = module.items.get(item_ordinal).ok_or_else(|| {
                    Diagnostic::unsupported(
                        Phase::Expand,
                        "source helper output placement is absent",
                        None,
                    )
                })?;
                let origin = origins.in_module(module.id, item_ordinal, &mut budget, item.span)?;
                if matches!(emission, CallableContextEmission::Return { .. }) {
                    if let Some(row) =
                        self.callable_returns
                            .consume(origin, &mut budget, item.span)?
                    {
                        budget.identity_work(
                            expanded.core.functions.len().saturating_add(1),
                            item.span,
                        )?;
                        let mut actual = expanded
                            .core
                            .functions
                            .iter_mut()
                            .filter(|function| function.id == creator);
                        let function = actual.next().ok_or_else(|| {
                            Diagnostic::unsupported(
                                Phase::Expand,
                                "opaque return actual creator is absent",
                                Some(item.span),
                            )
                        })?;
                        if actual.next().is_some() || function.source_scope.is_some() {
                            return Err(Diagnostic::unsupported(
                                Phase::Expand,
                                "opaque return actual owner is ambiguous",
                                Some(item.span),
                            ));
                        }
                        let Type::Opaque { identity, hidden } = &mut function.return_type else {
                            return Err(Diagnostic::unsupported(
                                Phase::Expand,
                                "opaque return actual creator changed its result kind",
                                Some(item.span),
                            ));
                        };
                        if *identity != function.id
                            || function
                                .callable_return
                                .as_ref()
                                .is_none_or(|bound| bound.kind != row.returned.kind)
                        {
                            return Err(Diagnostic::unsupported(
                                Phase::Expand,
                                "opaque return actual creator changed its declared bound",
                                Some(item.span),
                            ));
                        }
                        crate::stored_types::pay_type(
                            &row.returned.result,
                            &mut budget,
                            item.span,
                            0,
                        )?;
                        for ty in &row.returned.parameters {
                            crate::stored_types::pay_type(ty, &mut budget, item.span, 0)?;
                        }
                        *hidden = Box::new(Type::FunctionPointer {
                            parameters: row.returned.parameters.clone(),
                            result: Box::new(row.returned.result.clone()),
                        });
                        function.source_scope = Some(row.scope);
                        function.callable_return = Some(row.returned);
                    }
                    continue;
                }
                let CallableContextEmission::Parameter {
                    parameter,
                    generic,
                    bound,
                    ..
                } = emission
                else {
                    unreachable!()
                };
                let Some(row) = self.callable_contexts.consume(
                    origin,
                    parameter,
                    generic,
                    bound,
                    &mut budget,
                    item.span,
                )?
                else {
                    continue;
                };
                budget.identity_work(expanded.core.functions.len().saturating_add(1), item.span)?;
                let mut actual = expanded
                    .core
                    .functions
                    .iter_mut()
                    .filter(|f| f.id == creator);
                let function = actual.next().ok_or_else(|| {
                    Diagnostic::unsupported(
                        Phase::Expand,
                        "source helper actual creator is absent",
                        Some(item.span),
                    )
                })?;
                if actual.next().is_some() {
                    return Err(Diagnostic::unsupported(
                        Phase::Expand,
                        "source helper actual creator is ambiguous",
                        Some(item.span),
                    ));
                }
                budget.identity_work(
                    function.callable_parameters.len().saturating_add(1),
                    item.span,
                )?;
                let role = function
                    .callable_parameters
                    .iter_mut()
                    .find(|r| r.index == parameter)
                    .ok_or_else(|| {
                        Diagnostic::unsupported(
                            Phase::Expand,
                            "source helper actual role is absent",
                            Some(item.span),
                        )
                    })?;
                if role.source_life.is_some()
                    || role.generic_index != row.generic_index
                    || !matches!(
                        role.kind,
                        CallableKind::Fn | CallableKind::FnMut | CallableKind::FnOnce
                    )
                {
                    return Err(Diagnostic::unsupported(
                        Phase::Expand,
                        "source helper actual role changed",
                        Some(item.span),
                    ));
                }
                role.life = Some(row.life);
                role.source_life = Some(row.context);
                role.parameters = row.parameters;
                role.result = row.result;
            }
            Ok(())
        })();
        self.allowance = Some(budget.remaining_limits());
        result.map_err(|e| vec![e])
    }
    pub(crate) fn finish_callable_contexts(&mut self, limits: &Limits) -> CompileResult<()> {
        let mut budget = generics::Budget::new(self.allowance.as_ref().unwrap_or(limits));
        let result = self
            .callable_contexts
            .finish(&mut budget, Span::default())
            .and_then(|()| self.callable_returns.finish(&mut budget, Span::default()));
        self.allowance = Some(budget.remaining_limits());
        result.map_err(|e| vec![e])
    }
}
