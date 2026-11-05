//! Bind owned original-bound rows to the actual function/role lowering event.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct CallableContextEmission {
    pub(super) creator: NodeId,
    pub(super) item: usize,
    pub(super) parameter: u32,
    pub(super) generic: u32,
    pub(super) bound: u32,
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
                let item = module.items.get(emission.item).ok_or_else(|| {
                    Diagnostic::unsupported(
                        Phase::Expand,
                        "source helper output placement is absent",
                        None,
                    )
                })?;
                let origin = origins.in_module(module.id, emission.item, &mut budget, item.span)?;
                let Some(row) = self.callable_contexts.consume(
                    origin,
                    emission.parameter,
                    emission.generic,
                    emission.bound,
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
                    .filter(|f| f.id == emission.creator);
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
                    .find(|r| r.index == emission.parameter)
                    .ok_or_else(|| {
                        Diagnostic::unsupported(
                            Phase::Expand,
                            "source helper actual role is absent",
                            Some(item.span),
                        )
                    })?;
                if role.source_life.is_some()
                    || role.generic_index != 0
                    || !matches!(role.kind, CallableKind::Fn | CallableKind::FnMut)
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
        let result = self.callable_contexts.finish(&mut budget, Span::default());
        self.allowance = Some(budget.remaining_limits());
        result.map_err(|e| vec![e])
    }
}
