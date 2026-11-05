//! Revisit a successful request's actual source call before admitting its host
//! as an uninstantiated namespace. The retained event is private, never a flag
//! supplied by Expand or an artifact cache.
use super::*;

impl FrozenInvocationContext {
    pub(in crate::ct_artifact) fn static_host_source<'a>(
        &'a self,
        call: &SourceInstanceCall,
        scopes: &[CtAmbientLifeScope],
        raw: &CtArtifactProgram,
        target: &CtSymbolId,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
        completed_calls: &[zil_ct::CtCompletedCall],
        source_calls: &[SourceInstanceCall],
    ) -> CompileResult<Option<(&'a s::Item, &'a LifeDeclarationIdentity)>> {
        let Some(enclosing) = call.enclosing else {
            return Ok(None);
        };
        budget.reserve_analysis_work(33)?;
        let host_scope = scopes
            .get(enclosing.0 as usize)
            .ok_or_else(|| unsupported("CT namespace host instance is absent"))?;
        if host_scope.declaration != *target {
            return Ok(None);
        }
        self.validate_scope(&call.caller_scope, budget)?;
        self.validate_scope(host_scope, budget)?;
        self.validate_source_call_actuals(call, scopes, raw, index, budget)?;
        if let Some(catalog) = &self.generated_catalog {
            super::super::generated::validate_catalog(catalog, &self.modules, budget)?;
        }
        let caller = raw
            .functions
            .get(call.caller.0 as usize)
            .ok_or_else(|| unsupported("CT namespace actual caller absent"))?;
        let callee = raw
            .functions
            .get(call.instance.0 as usize)
            .ok_or_else(|| unsupported("CT namespace actual callee absent"))?;
        if caller.definition != call.caller_definition || callee.definition != call.callee {
            return Err(unsupported(
                "CT namespace call definition differs from the successful artifact",
            ));
        }
        let (block, position) = call
            .execution
            .ok_or_else(|| unsupported("CT namespace needs its exact source execution position"))?;
        budget.reserve_analysis_work(source_calls.len() as u64)?;
        if source_calls.iter().any(|other| {
            !std::ptr::eq(other, call)
                && other.caller == call.caller
                && other.execution == call.execution
        }) {
            return Err(unsupported(
                "CT namespace source execution occurrence is repeated",
            ));
        }
        budget.reserve_analysis_work(caller.blocks.len() as u64 + 1)?;
        let actual = caller
            .blocks
            .iter()
            .find(|row| row.id == block)
            .and_then(|row| row.instructions.get(position as usize))
            .ok_or_else(|| unsupported("CT namespace exact source instruction is absent"))?;
        if !matches!(&actual.kind,CtInstructionKind::Call{function,..} if *function==call.instance)
            || call
                .occurrence
                .is_some_and(|(b, v)| b != block || actual.result != Some(v))
        {
            return Err(unsupported(
                "CT namespace source execution position is not its actual Call",
            ));
        }
        budget.reserve_analysis_work(completed_calls.len() as u64)?;
        if !completed_calls.iter().any(|event| {
            event.caller == call.caller
                && event.block == block
                && event.instruction == position
                && event.callee == call.instance
                && event.result == actual.result
        }) {
            return Ok(None);
        }
        // Independently recover the callee's real lexical parent, including
        // the generated catalog's local-to-global declaration correspondence.
        let (module, original, tree) = self.source_function_source(&call.callee, budget)?;
        let parent = original
            .lexical_parent
            .ok_or_else(|| unsupported("CT namespace call is not an actual member"))?;
        let parent_global = if let Some(tree) = tree {
            *tree
                .declarations
                .get(parent.0 as usize)
                .ok_or_else(|| unsupported("CT namespace generated parent inventory absent"))?
        } else {
            parent
        };
        if parent_global != target.declaration
            || module.id != target.module
            || tree.and_then(|t| t.generation_fingerprint()) != target.generation_path
        {
            return Err(unsupported(
                "CT namespace call belongs to another actual lexical host",
            ));
        }
        let source = super::super::super::compiler::source_scope::original_declaration(
            module,
            parent,
            budget.remaining().max_call_depth as usize,
            |work| budget.reserve_analysis_work(work as u64),
        )?
        .ok_or_else(|| unsupported("CT namespace actual parent declaration absent"))?;
        let s::ItemKind::Aggregate(aggregate) = &source.item.kind else {
            return Err(unsupported(
                "CT namespace lexical parent is not an aggregate",
            ));
        };
        if aggregate.kind != s::AggregateKind::Struct || source.lexical_parent.is_some() {
            return Err(unsupported(
                "CT namespace requires its actual top-level ordinary struct",
            ));
        }
        // Replay every host actual directly from the original Call under its
        // actual caller environment. Matching an admitted scope alone is not
        // enough to pick a host or its parameter order.
        let location = self.source_location(call.site, budget)?;
        let SourceNode::Expr(s::Expr {
            kind: s::ExprKind::Call { callee, .. },
            ..
        }) = location.node
        else {
            return Err(unsupported(
                "CT namespace source site is not an actual Call",
            ));
        };
        let mut member = callee.as_ref();
        while let s::ExprKind::Group(inner) = &member.kind {
            step(budget, 0)?;
            member = inner;
        }
        if let s::ExprKind::TypeApply { callee, .. } = &member.kind {
            member = callee;
        }
        let s::ExprKind::Member { object, .. } = &member.kind else {
            return Err(unsupported(
                "CT namespace source callee is not a static member",
            ));
        };
        let s::ExprKind::TypeApply {
            callee: path,
            arguments,
        } = &object.kind
        else {
            return Err(unsupported("CT namespace source omits full host actuals"));
        };
        let before = budget.live_temporary_bytes();
        let mut names = Vec::new();
        let result = (|| {
            self.source_path(path, &mut names, budget, 0)?;
            let (symbol, item) = self.source_symbol(
                &call.caller_scope.declaration,
                call.site.module,
                &names,
                index,
                budget,
            )?;
            if symbol != *target || !std::ptr::eq(item, source.item) {
                return Err(unsupported(
                    "CT namespace actual source path selects another host",
                ));
            }
            let CtScopeInstance::Instantiated(actuals) = &host_scope.instance else {
                return Err(unsupported(
                    "CT namespace host lacks a complete actual instance",
                ));
            };
            budget.reserve_analysis_work(
                (arguments.len() + actuals.len() + aggregate.generics.len()) as u64,
            )?;
            if arguments.len() != aggregate.generics.len()
                || actuals.len() != arguments.len()
                || host_scope.enclosing.is_some()
                || !host_scope.binder_site.is_empty()
            {
                return Err(unsupported("CT namespace full host actual shape differs"));
            }
            let env =
                self.source_environment(&call.caller_scope, call.caller_scope_id, scopes, budget)?;
            for ((parameter, arg), actual) in aggregate.generics.iter().zip(arguments).zip(actuals)
            {
                step(budget, 0)?;
                match (&parameter.kind, arg, actual) {
                    (
                        s::GenericParamKind::Lifetime,
                        s::GenericArg::Lifetime(name),
                        CtScopeActual::Lifetime(actual),
                    ) => {
                        if self.source_lifetime(name, &env, budget)? != *actual {
                            return Err(unsupported(
                                "CT namespace host life actual differs from source",
                            ));
                        }
                    }
                    (
                        s::GenericParamKind::Type(_),
                        s::GenericArg::Type(ty),
                        CtScopeActual::Type(graph),
                    ) => self.source_type(ty, graph, graph.root, &env, raw, index, budget, 0)?,
                    (s::GenericParamKind::Const(domain), _, CtScopeActual::Scalar(actual)) => {
                        let value = self.source_scalar(
                            arg,
                            scalar_domain(domain, budget)?,
                            &env,
                            budget,
                            0,
                        )?;
                        if value != *actual {
                            return Err(unsupported(
                                "CT namespace host Const actual differs from source",
                            ));
                        }
                    }
                    _ => {
                        return Err(unsupported(
                            "CT namespace host actual kind differs from source",
                        ));
                    }
                }
            }
            Ok(())
        })();
        budget.release_temporary_bytes(budget.live_temporary_bytes().saturating_sub(before));
        result?;
        for active in &self.active {
            let cost = super::super::super::execute::value_context::ambient_cost(
                std::slice::from_ref(&active.raw),
                budget,
            )?;
            budget.reserve_analysis_work(cost.saturating_add(32))?;
            if active.raw == *host_scope {
                return Ok(Some((source.item, &active.identity)));
            }
        }
        Err(unsupported(
            "CT namespace host is outside the actual independently admitted invocation",
        ))
    }
}
