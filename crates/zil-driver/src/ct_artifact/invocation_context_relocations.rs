//! Independently reconcile fresh source-call events and exact record copies.
//! Producer endpoint/type predicates are deliberately not called here.
use super::*;
use crate::ct_artifact::compiler::{SourceInstanceCall, SourceRecordRelocation};

impl FrozenInvocationContext {
    pub(in crate::ct_artifact) fn validate_record_relocations(
        &self,
        rows: &[SourceRecordRelocation],
        calls: &[SourceInstanceCall],
        raw: &CtArtifactProgram,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        if rows.is_empty() {
            return Ok(());
        }
        budget.reserve_analysis_work(rows.len() as u64)?;
        for (position, row) in rows.iter().enumerate() {
            budget.reserve_analysis_work(position as u64 + 1)?;
            if rows[..position]
                .iter()
                .any(|prior| prior.caller == row.caller && prior.input == row.input)
            {
                return Err(unsupported("CT record relocation repeats a call result"));
            }
            let function = raw
                .functions
                .get(row.caller.0 as usize)
                .ok_or_else(|| unsupported("CT record relocation caller is absent"))?;
            budget.reserve_analysis_work(function.blocks.len() as u64)?;
            let block = function
                .blocks
                .iter()
                .find(|block| block.id == row.block)
                .ok_or_else(|| unsupported("CT record relocation block is absent"))?;
            budget.reserve_analysis_work(block.instructions.len() as u64)?;
            let at = block
                .instructions
                .iter()
                .position(|instruction| instruction.result == Some(row.input))
                .ok_or_else(|| {
                    unsupported("CT record relocation input has no actual definition")
                })?;
            let CtInstructionKind::Call {
                function: instance, ..
            } = block.instructions[at].kind
            else {
                return Err(unsupported(
                    "CT record relocation input is not an actual call",
                ));
            };
            budget.reserve_analysis_work(calls.len() as u64)?;
            let call = calls
                .iter()
                .find(|call| call.caller == row.caller && call.instance == instance)
                .ok_or_else(|| {
                    unsupported("CT record relocation call has no authentic source event")
                })?;
            let callee = raw
                .functions
                .get(instance.0 as usize)
                .ok_or_else(|| unsupported("CT record relocation callee is absent"))?;
            if callee.definition != call.callee
                || function.definition != call.caller_definition
                || callee.result_type != CtTy::Record(row.source)
                || function.result_type != CtTy::Record(row.target)
                || function.values.get(row.input.0 as usize) != Some(&CtTy::Record(row.source))
                || function.values.get(row.output.0 as usize) != Some(&CtTy::Record(row.target))
            {
                return Err(unsupported(
                    "CT record relocation differs from actual call and return types",
                ));
            }
            let source = raw
                .record_schemas
                .get(row.source.0 as usize)
                .ok_or_else(|| unsupported("CT record relocation source schema is absent"))?;
            let target = raw
                .record_schemas
                .get(row.target.0 as usize)
                .ok_or_else(|| unsupported("CT record relocation target schema is absent"))?;
            budget.reserve_analysis_work(
                source.fields.len() as u64
                    + target.fields.len() as u64
                    + source.arguments.len() as u64
                    + target.arguments.len() as u64
                    + 64,
            )?;
            if source.fields.is_empty()
                || source
                    .fields
                    .iter()
                    .chain(&target.fields)
                    .any(|ty| *ty != CtTy::Scalar(CtScalarTy::I32))
                || source.origin != CtSchemaOrigin::UserPure
                || target.origin != CtSchemaOrigin::UserPure
                || source.name != target.name
                || source.fields != target.fields
            {
                return Err(unsupported(
                    "CT record relocation is not the same pure i32-field nominal",
                ));
            }
            let (Some(left), Some(right)) = (&source.life_context, &target.life_context) else {
                return Err(unsupported(
                    "CT record relocation has no two actual life contexts",
                ));
            };
            let cost = super::super::execute::value_context::graph_cost(left, budget)?
                .saturating_add(super::super::execute::value_context::graph_cost(
                    right, budget,
                )?);
            budget.reserve_analysis_work(cost)?;
            for argument in source.arguments.iter().chain(&target.arguments) {
                match argument {
                    CtSchemaArgument::Type(graph) => {
                        let cost = super::super::execute::value_context::graph_cost(graph, budget)?;
                        budget.reserve_analysis_work(cost)?;
                    }
                    CtSchemaArgument::Scalar(zil_ir::Constant::WideString(_)) => {
                        return Err(unsupported(
                            "CT record relocation const actual is not a pure scalar",
                        ));
                    }
                    _ => {}
                }
            }
            if source.arguments != target.arguments
                || left.nodes != right.nodes
                || left.root != right.root
                || left.ambient_classes.is_empty()
                || left.ambient_classes.len() != right.ambient_classes.len()
            {
                return Err(unsupported(
                    "CT record relocation changes complete nominal arguments or context shape",
                ));
            }
            for (a, b) in left.ambient_classes.iter().zip(&right.ambient_classes) {
                if a.owner != left.root
                    || b.owner != right.root
                    || a.owner != b.owner
                    || a.class != b.class
                    || b.source.scope != call.caller_scope_id
                {
                    return Err(unsupported(
                        "CT record relocation changes actual class positions or caller scope",
                    ));
                }
                self.validate_reference(&raw.ambient_life_scopes, a.source, budget)?;
                self.validate_reference(&raw.ambient_life_scopes, b.source, budget)?;
                let scope = &raw.ambient_life_scopes[a.source.scope.0 as usize];
                let CtScopeInstance::Instantiated(actuals) = &scope.instance else {
                    return Err(unsupported(
                        "CT record relocation source is not the actual callee instance",
                    ));
                };
                let cost = super::super::execute::value_context::ambient_cost(
                    std::slice::from_ref(scope),
                    budget,
                )?;
                budget.reserve_analysis_work(cost)?;
                budget.reserve_analysis_work(
                    call.actuals.len() as u64 * std::mem::size_of::<CtScopeActual>() as u64,
                )?;
                for actual in &call.actuals {
                    match actual {
                        CtScopeActual::Type(graph) => {
                            let cost =
                                super::super::execute::value_context::graph_cost(graph, budget)?;
                            budget.reserve_analysis_work(cost)?;
                        }
                        CtScopeActual::Scalar(zil_ir::Constant::WideString(text)) => {
                            budget.reserve_analysis_work(text.len() as u64 * 2)?;
                        }
                        _ => {}
                    }
                }
                if scope.declaration != call.callee
                    || !scope.binder_site.is_empty()
                    || actuals != &call.actuals
                {
                    return Err(unsupported(
                        "CT record relocation source full instance differs from actual call",
                    ));
                }
                if self.resolve_reference(&raw.ambient_life_scopes, a.source, budget)?
                    != self.resolve_reference(&raw.ambient_life_scopes, b.source, budget)?
                {
                    return Err(unsupported(
                        "CT record relocation changes original life source or full ordinal",
                    ));
                }
            }
            let count = source.fields.len();
            let end = at
                .checked_add(count)
                .and_then(|n| n.checked_add(1))
                .ok_or_else(|| unsupported("CT record relocation instruction range overflows"))?;
            let made = block
                .instructions
                .get(end)
                .ok_or_else(|| unsupported("CT record relocation omits the reconstruction"))?;
            let CtInstructionKind::MakeRecord { schema, fields } = &made.kind else {
                return Err(unsupported(
                    "CT record relocation output is not a reconstruction",
                ));
            };
            if *schema != row.target || fields.len() != count || made.result != Some(row.output) {
                return Err(unsupported(
                    "CT record relocation reconstruction identity differs",
                ));
            }
            budget.reserve_analysis_work(count as u64)?;
            for (index, field) in fields.iter().enumerate() {
                let extracted = &block.instructions[at + index + 1];
                if extracted.result != Some(*field)
                    || extracted.kind
                        != (CtInstructionKind::ExtractField {
                            aggregate: row.input,
                            index: index as u32,
                        })
                {
                    return Err(unsupported(
                        "CT record relocation does not copy each actual field exactly once in order",
                    ));
                }
            }
        }
        Ok(())
    }
}
