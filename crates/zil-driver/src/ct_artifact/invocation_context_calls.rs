//! Admission of the private source-call ledger. Expected raw tables are only
//! reconciled after a real call event establishes the corresponding instance.
use super::*;
use crate::ct_artifact::compiler::SourceInstanceCall;

impl FrozenInvocationContext {
    pub(in crate::ct_artifact) fn extend_calls(
        &mut self,
        calls: &[SourceInstanceCall],
        source_scopes: &[CtAmbientLifeScope],
        expected: &CtArtifactProgram,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        budget.reserve_analysis_work(calls.len() as u64)?;
        for (position, call) in calls.iter().enumerate() {
            self.validate_scope(&call.caller_scope, budget)?;
            budget.reserve_analysis_work(index.symbol_lookup_work().saturating_mul(3))?;
            if !index.contains(&call.callee)
                || call.callee.generation_path.is_some()
                || !index.visible(&call.caller_scope.declaration, &call.callee)
            {
                return Err(unsupported(
                    "CT factory call has no authentic visible original callee",
                ));
            }
            let caller = expected
                .functions
                .get(call.caller.0 as usize)
                .ok_or_else(|| unsupported("CT call ledger caller placement is absent"))?;
            let callee = expected
                .functions
                .get(call.instance.0 as usize)
                .ok_or_else(|| unsupported("CT call ledger instance placement is absent"))?;
            if caller.definition != call.caller_definition || callee.definition != call.callee {
                return Err(unsupported(
                    "CT call ledger declaration differs from actual placement",
                ));
            }
            if let Some(pool) = source_scopes.get(call.caller_scope_id.0 as usize)
                && matches!(pool.instance, CtScopeInstance::Instantiated(_))
                && matches!(call.caller_scope.instance, CtScopeInstance::Instantiated(_))
            {
                let left = super::super::execute::value_context::ambient_cost(
                    std::slice::from_ref(pool),
                    budget,
                )?;
                let right = super::super::execute::value_context::ambient_cost(
                    std::slice::from_ref(&call.caller_scope),
                    budget,
                )?;
                budget.reserve_analysis_work(left.saturating_add(right))?;
            }
            if source_scopes.get(call.caller_scope_id.0 as usize) != Some(&call.caller_scope) {
                return Err(unsupported("CT call ledger caller scope placement differs"));
            }
            // Consume call occurrences, not just the existence of one matching
            // function. Repeated source calls to a reused instance stay distinct.
            let mut actual = 0usize;
            for block in &caller.blocks {
                budget.reserve_analysis_work(block.instructions.len() as u64 + 1)?;
                actual += block.instructions.iter().filter(|instruction| {
                    matches!(&instruction.kind, CtInstructionKind::Call { function, .. } if *function == call.instance)
                }).count();
            }
            let mut prior = 0usize;
            for earlier in &calls[..position] {
                budget.reserve_analysis_work(2)?;
                prior +=
                    usize::from(earlier.caller == call.caller && earlier.instance == call.instance);
            }
            if prior >= actual {
                return Err(unsupported(
                    "CT source call ledger has no unconsumed actual call",
                ));
            }
            for value in &call.actuals {
                budget.reserve_analysis_work(1)?;
                match value {
                    CtScopeActual::Lifetime(reference) => {
                        if reference.scope != call.caller_scope_id {
                            return Err(unsupported(
                                "CT life actual is not from the authentic caller scope",
                            ));
                        }
                        self.validate_reference(source_scopes, *reference, budget)?;
                    }
                    CtScopeActual::Type(graph) => {
                        budget.reserve_analysis_work(graph.ambient_classes.len() as u64)?;
                        for row in &graph.ambient_classes {
                            self.validate_reference(
                                &expected.ambient_life_scopes,
                                row.source,
                                budget,
                            )?;
                        }
                    }
                    CtScopeActual::Scalar(_) => {}
                }
            }
            self.validate_source_call_actuals(call, source_scopes, expected, index, budget)?;
            let row = self.call_scope(call, budget)?;
            budget.reserve_analysis_work(self.active.len() as u64 * 32)?;
            self.charge_instance_lookup(&row.raw, budget)?;
            if !self.active.iter().any(|active| active.raw == row.raw) {
                self.active.reserve_exact(1);
                self.active.push(row);
            } else {
                // The fresh row was allocated only to compare independently
                // with the already authenticated original instance.
                let bytes = super::super::execute::value_context::ambient_cost(
                    std::slice::from_ref(&row.raw),
                    budget,
                )? + identity_cost(&row.identity, budget)?
                    + row.parameters.len() as u64
                        * std::mem::size_of::<CtScopeParameterKind>() as u64
                    + std::mem::size_of::<ActiveScope>() as u64;
                budget.release_temporary_bytes(bytes);
                self.bytes = self.bytes.saturating_sub(bytes);
            }
        }
        // Every actual call to a ledger-owned instance is consumed exactly.
        for (position, call) in calls.iter().enumerate() {
            let mut count = 0usize;
            for event in calls {
                budget.reserve_analysis_work(2)?;
                count +=
                    usize::from(event.caller == call.caller && event.instance == call.instance);
            }
            if calls[..position]
                .iter()
                .any(|event| event.caller == call.caller && event.instance == call.instance)
            {
                continue;
            }
            let mut actual = 0usize;
            for block in &expected.functions[call.caller.0 as usize].blocks {
                budget.reserve_analysis_work(block.instructions.len() as u64 + 1)?;
                actual += block.instructions.iter().filter(|instruction| {
                    matches!(&instruction.kind, CtInstructionKind::Call { function, .. } if *function == call.instance)
                }).count();
            }
            if count != actual {
                return Err(unsupported(
                    "CT actual call has no corresponding source binding event",
                ));
            }
        }
        // Raw rows never create authority. Each must match a scope admitted
        // above from its private actual-call event, or the original entry.
        for scope in &expected.ambient_life_scopes {
            self.validate_scope(scope, budget)?;
        }
        Ok(())
    }

    fn call_scope(
        &mut self,
        call: &SourceInstanceCall,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<ActiveScope> {
        budget.reserve_analysis_work(self.modules.len() as u64)?;
        let module = self
            .modules
            .iter()
            .find(|module| module.module.id == call.callee.module)
            .ok_or_else(|| unsupported("CT factory has no original module"))?;
        let original = super::super::compiler::source_scope::original_function(
            &module.module,
            call.callee.declaration,
            budget.remaining().max_call_depth as usize,
            |work| budget.reserve_analysis_work(work as u64),
        )?
        .ok_or_else(|| unsupported("CT factory has no original function"))?;
        if original.generic_host || !matches!(original.item.kind, s::ItemKind::Function(_)) {
            return Err(unsupported(
                "CT full instance requires an original nongenerated factory",
            ));
        }
        let function = original.function;
        let host = &module.module.items[original.top];
        if !std::ptr::eq(original.item, host) {
            let s::ItemKind::Aggregate(aggregate) = &host.kind else {
                return Err(unsupported("CT member factory has no actual struct host"));
            };
            budget.reserve_analysis_work(
                aggregate.methods.len() as u64 + function.generics.len() as u64 * 3,
            )?;
            for parameter in &function.params {
                budget.reserve_analysis_work(parameter.name.len().min(4) as u64 + 1)?;
            }
            if !aggregate.generics.is_empty()
                || aggregate.kind != s::AggregateKind::Struct
                || aggregate
                    .methods
                    .iter()
                    .filter(|item| std::ptr::eq(*item, original.item))
                    .count()
                    != 1
                || function
                    .params
                    .iter()
                    .any(|parameter| parameter.name == "self")
                || !function
                    .generics
                    .iter()
                    .any(|parameter| matches!(parameter.kind, s::GenericParamKind::Type(_)))
                || !function
                    .generics
                    .iter()
                    .any(|parameter| matches!(parameter.kind, s::GenericParamKind::Const(_)))
                || !function
                    .generics
                    .iter()
                    .any(|parameter| matches!(parameter.kind, s::GenericParamKind::Lifetime))
            {
                return Err(unsupported(
                    "CT member factory requires an actual nongeneric struct, static entry and complete mixed parameters",
                ));
            }
        }
        if function.generics.len() != call.actuals.len() {
            return Err(unsupported("CT call ledger full actual arity differs"));
        }
        let origins = self
            .origins
            .as_ref()
            .ok_or_else(|| unsupported("CT factory original lineage absent"))?;
        budget.reserve_analysis_work(origins.len() as u64)?;
        let origin = origins
            .iter()
            .find(|row| row.module == call.callee.module)
            .and_then(|row| row.items.get(original.top))
            .ok_or_else(|| unsupported("CT factory original source identity absent"))?;
        if origin.declaration.0 as usize != original.top {
            return Err(unsupported(
                "CT factory original declaration ordinal differs",
            ));
        }
        budget.reserve_analysis_work(function.generics.len() as u64)?;
        let bytes = std::mem::size_of::<ActiveScope>() as u64
            + function.generics.len() as u64 * std::mem::size_of::<CtScopeParameterKind>() as u64
            + identity_cost(&origin.identity, budget)?;
        budget.reserve_temporary_bytes(bytes)?;
        let mut parameters = Vec::with_capacity(function.generics.len());
        for (parameter, actual) in function.generics.iter().zip(&call.actuals) {
            budget.reserve_analysis_work(1)?;
            parameters.push(match (&parameter.kind, actual) {
                (s::GenericParamKind::Type(_), CtScopeActual::Type(_)) => {
                    CtScopeParameterKind::Type
                }
                (s::GenericParamKind::Lifetime, CtScopeActual::Lifetime(_)) => {
                    CtScopeParameterKind::Lifetime
                }
                (s::GenericParamKind::Const(ty), CtScopeActual::Scalar(value)) => {
                    let s::TypeKind::Named { path, args } = &ty.kind else {
                        return Err(unsupported(
                            "CT factory scalar domain requires an explicit scalar",
                        ));
                    };
                    budget.reserve_analysis_work(path.len() as u64 + args.len() as u64)?;
                    let [name] = path.as_slice() else {
                        return Err(unsupported(
                            "CT factory scalar domain is not a direct builtin",
                        ));
                    };
                    budget.reserve_analysis_work(name.len() as u64)?;
                    let domain = super::super::compiler::scalar_type(name)
                        .ok_or_else(|| unsupported("CT factory scalar domain is not a scalar"))?;
                    if !args.is_empty()
                        || super::super::compiler::constant_type(value) != CtTy::Scalar(domain)
                    {
                        return Err(unsupported("CT factory actual scalar domain differs"));
                    }
                    CtScopeParameterKind::Scalar(domain)
                }
                _ => {
                    return Err(unsupported(
                        "CT call ledger actual kind differs from original full ordinal",
                    ));
                }
            });
        }
        let raw = CtAmbientLifeScope {
            declaration: call.callee.clone(),
            binder_site: Vec::new(),
            instance: CtScopeInstance::Instantiated(Vec::new()),
        };
        let mut cost =
            super::super::execute::value_context::ambient_cost(std::slice::from_ref(&raw), budget)?;
        cost += call.actuals.len() as u64 * std::mem::size_of::<CtScopeActual>() as u64;
        for actual in &call.actuals {
            budget.reserve_analysis_work(1)?;
            if let CtScopeActual::Type(graph) = actual {
                cost += super::super::execute::value_context::graph_cost(graph, budget)?;
            }
        }
        budget.reserve_temporary_bytes(cost)?;
        self.bytes = self.bytes.saturating_add(bytes).saturating_add(cost);
        Ok(ActiveScope {
            raw: CtAmbientLifeScope {
                instance: CtScopeInstance::Instantiated(call.actuals.clone()),
                ..raw
            },
            identity: origin.identity.clone(),
            parameters,
        })
    }
}
