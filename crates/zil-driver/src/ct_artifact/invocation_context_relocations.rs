//! Independent source-call, schema-pair and actual SSA/CFG copy reconstruction.
//! No producer pairing predicate or producer conversion plan is consulted.
use super::*;
use crate::ct_artifact::compiler::{SourceInstanceCall, SourceRecordRelocation};
use zil_ir::{BinaryOp, Constant};

#[derive(Clone, Copy)]
struct Cursor {
    block: CtBlockId,
    instruction: usize,
}
type Locals = Vec<(CtLocalId, usize, [usize; 4])>;

fn block(function: &CtFunction, id: CtBlockId) -> CompileResult<&CtBlock> {
    function
        .blocks
        .get(id.0 as usize)
        .filter(|block| block.id == id)
        .ok_or_else(|| unsupported("CT relocation block is not the actual indexed block"))
}
fn next<'a>(
    function: &'a CtFunction,
    cursor: &mut Cursor,
    budget: &mut CtExecutionBudget,
) -> CompileResult<&'a CtInstruction> {
    budget.reserve_analysis_work(1)?;
    let instruction = block(function, cursor.block)?
        .instructions
        .get(cursor.instruction)
        .ok_or_else(|| unsupported("CT relocation omits an actual copy instruction"))?;
    cursor.instruction += 1;
    Ok(instruction)
}
fn type_work(ty: &CtTy, budget: &mut CtExecutionBudget, depth: usize) -> CompileResult<()> {
    budget.reserve_analysis_work(1)?;
    if depth > budget.remaining().max_call_depth as usize {
        return Err(vec![Diagnostic::limit(
            Phase::Ct,
            "CT relocation type depth exceeded",
            None,
        )]);
    }
    match ty {
        CtTy::Tuple(fields) => {
            for field in fields {
                type_work(field, budget, depth + 1)?
            }
        }
        CtTy::Array { element, .. } => type_work(element, budget, depth + 1)?,
        _ => {}
    }
    Ok(())
}
fn result(
    function: &CtFunction,
    instruction: &CtInstruction,
    ty: &CtTy,
    budget: &mut CtExecutionBudget,
) -> CompileResult<CtValueId> {
    type_work(ty, budget, 0)?;
    let id = instruction
        .result
        .ok_or_else(|| unsupported("CT relocation copy has no actual SSA result"))?;
    let actual = function
        .values
        .get(id.0 as usize)
        .ok_or_else(|| unsupported("CT relocation SSA result is outside its actual table"))?;
    type_work(actual, budget, 0)?;
    if actual != ty {
        return Err(unsupported(
            "CT relocation copy changes the complete actual SSA type",
        ));
    }
    Ok(id)
}
fn end(function: &CtFunction, cursor: Cursor) -> CompileResult<&CtTerminator> {
    let block = block(function, cursor.block)?;
    if cursor.instruction != block.instructions.len() {
        return Err(unsupported(
            "CT relocation branch contains extra unvalidated instructions",
        ));
    }
    Ok(&block.terminator)
}
fn constant(
    function: &CtFunction,
    raw: &CtArtifactProgram,
    cursor: &mut Cursor,
    value: Constant,
    ty: CtScalarTy,
    budget: &mut CtExecutionBudget,
) -> CompileResult<CtValueId> {
    let instruction = next(function, cursor, budget)?;
    let CtInstructionKind::Constant(id) = instruction.kind else {
        return Err(unsupported(
            "CT relocation omits an exact index/tag constant",
        ));
    };
    if raw.constants.get(id.0 as usize) != Some(&CtConstant::Scalar(value)) {
        return Err(unsupported(
            "CT relocation changes the exact array index or variant tag",
        ));
    }
    result(function, instruction, &CtTy::Scalar(ty), budget)
}

// Keep the source, candidate, scope and request meter explicit at this proof boundary.

#[allow(clippy::too_many_arguments)]
fn fields_trace(
    function: &CtFunction,
    raw: &CtArtifactProgram,
    cursor: &mut Cursor,
    input: CtValueId,
    source: &[CtTy],
    target: &[CtTy],
    variant: Option<u32>,
    locals: &mut Locals,
    budget: &mut CtExecutionBudget,
    depth: usize,
) -> CompileResult<Vec<CtValueId>> {
    budget.reserve_analysis_work(source.len() as u64)?;
    budget
        .reserve_temporary_bytes(source.len() as u64 * std::mem::size_of::<CtValueId>() as u64)?;
    let mut fields = Vec::with_capacity(source.len());
    for (index, (source, target)) in source.iter().zip(target).enumerate() {
        let instruction = next(function, cursor, budget)?;
        let expected = match variant {
            None => CtInstructionKind::ExtractField {
                aggregate: input,
                index: index as u32,
            },
            Some(tag) => CtInstructionKind::VariantField {
                value: input,
                variant: tag,
                field: index as u32,
            },
        };
        if instruction.kind != expected {
            return Err(unsupported(
                "CT relocation does not extract each exact source field in order",
            ));
        }
        let field = result(function, instruction, source, budget)?;
        fields.push(copy_trace(
            function,
            raw,
            cursor,
            field,
            source,
            target,
            locals,
            budget,
            depth + 1,
        )?);
    }
    Ok(fields)
}
// Keep the source, candidate, scope and request meter explicit at this proof boundary.
#[allow(clippy::too_many_arguments)]
fn copy_trace(
    function: &CtFunction,
    raw: &CtArtifactProgram,
    cursor: &mut Cursor,
    input: CtValueId,
    source: &CtTy,
    target: &CtTy,
    locals: &mut Locals,
    budget: &mut CtExecutionBudget,
    depth: usize,
) -> CompileResult<CtValueId> {
    type_work(source, budget, depth)?;
    type_work(target, budget, depth)?;
    if source == target {
        return Ok(input);
    }
    match (source, target) {
        (CtTy::Record(a), CtTy::Record(b)) => {
            let fields = fields_trace(
                function,
                raw,
                cursor,
                input,
                &raw.record_schemas[a.0 as usize].fields,
                &raw.record_schemas[b.0 as usize].fields,
                None,
                locals,
                budget,
                depth,
            )?;
            let made = next(function, cursor, budget)?;
            if made.kind != (CtInstructionKind::MakeRecord { schema: *b, fields }) {
                return Err(unsupported(
                    "CT relocation record does not reconstruct the exact copied fields",
                ));
            }
            result(function, made, target, budget)
        }
        (CtTy::Tuple(a), CtTy::Tuple(b)) => {
            let fields = fields_trace(
                function, raw, cursor, input, a, b, None, locals, budget, depth,
            )?;
            let made = next(function, cursor, budget)?;
            if made.kind != CtInstructionKind::MakeTuple(fields) {
                return Err(unsupported(
                    "CT relocation tuple changes copied field order",
                ));
            }
            result(function, made, target, budget)
        }
        (CtTy::Array { element: a, length }, CtTy::Array { element: b, .. }) => {
            budget.reserve_analysis_work(*length as u64)?;
            budget.reserve_temporary_bytes(
                *length as u64 * std::mem::size_of::<CtValueId>() as u64,
            )?;
            let mut fields = Vec::with_capacity(*length as usize);
            for index in 0..*length {
                let index = constant(
                    function,
                    raw,
                    cursor,
                    Constant::U64(index as u64),
                    CtScalarTy::U64,
                    budget,
                )?;
                let instruction = next(function, cursor, budget)?;
                if instruction.kind
                    != (CtInstructionKind::Extract {
                        aggregate: input,
                        index,
                    })
                {
                    return Err(unsupported(
                        "CT relocation array does not extract its actual indexed element",
                    ));
                }
                let extracted = result(function, instruction, a, budget)?;
                fields.push(copy_trace(
                    function,
                    raw,
                    cursor,
                    extracted,
                    a,
                    b,
                    locals,
                    budget,
                    depth + 1,
                )?);
            }
            let made = next(function, cursor, budget)?;
            if made.kind != CtInstructionKind::MakeArray(fields) {
                return Err(unsupported(
                    "CT relocation array does not reconstruct every exact copied element",
                ));
            }
            result(function, made, target, budget)
        }
        (CtTy::Enum(a), CtTy::Enum(b)) => {
            let source = &raw.enum_schemas[a.0 as usize].variants;
            let target_variants = &raw.enum_schemas[b.0 as usize].variants;
            let live = next(function, cursor, budget)?;
            let CtInstructionKind::StorageLive(local) = live.kind else {
                return Err(unsupported("CT relocation enum has no fresh merge local"));
            };
            if live.result.is_some() || function.locals.get(local.0 as usize) != Some(target) {
                return Err(unsupported(
                    "CT relocation enum merge local has the wrong complete type",
                ));
            }
            budget.reserve_analysis_work(locals.len() as u64 + 1)?;
            if locals.iter().any(|(prior, _, _)| *prior == local) {
                return Err(unsupported("CT relocation reuses an enum merge local"));
            }
            if locals.len() == locals.capacity() {
                let capacity = locals.len().checked_add(1).ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Ct,
                        "CT relocation merge-local capacity overflow",
                        None,
                    )]
                })?;
                let bytes = capacity
                    .checked_mul(std::mem::size_of::<(CtLocalId, usize, [usize; 4])>())
                    .ok_or_else(|| {
                        vec![Diagnostic::limit(
                            Phase::Ct,
                            "CT relocation merge-local storage overflow",
                            None,
                        )]
                    })?;
                budget.reserve_temporary_bytes(bytes as u64)?;
                budget.reserve_analysis_work(locals.len() as u64)?;
                locals.reserve_exact(capacity - locals.len());
            }
            locals.push((local, source.len(), [0; 4]));
            let tagged = next(function, cursor, budget)?;
            if tagged.kind != CtInstructionKind::VariantTag(input) {
                return Err(unsupported(
                    "CT relocation enum dispatch tags another value",
                ));
            }
            let tag = result(function, tagged, &CtTy::Scalar(CtScalarTy::I32), budget)?;
            let mut join = None;
            for (ordinal, (source, target_fields)) in source.iter().zip(target_variants).enumerate()
            {
                let expected = constant(
                    function,
                    raw,
                    cursor,
                    Constant::I32(ordinal as i32),
                    CtScalarTy::I32,
                    budget,
                )?;
                let compared = next(function, cursor, budget)?;
                if compared.kind
                    != (CtInstructionKind::Binary {
                        op: BinaryOp::Equal,
                        left: tag,
                        right: expected,
                    })
                {
                    return Err(unsupported(
                        "CT relocation enum dispatch changes the exact tag test",
                    ));
                }
                let condition =
                    result(function, compared, &CtTy::Scalar(CtScalarTy::Bool), budget)?;
                let CtTerminator::Branch {
                    condition: actual,
                    then_block,
                    else_block,
                } = *end(function, *cursor)?
                else {
                    return Err(unsupported(
                        "CT relocation enum tag has no actual dispatch branch",
                    ));
                };
                if condition != actual || then_block == else_block {
                    return Err(unsupported(
                        "CT relocation enum branch does not follow its exact tag test",
                    ));
                }
                let mut body = Cursor {
                    block: then_block,
                    instruction: 0,
                };
                let fields = fields_trace(
                    function,
                    raw,
                    &mut body,
                    input,
                    source,
                    target_fields,
                    Some(ordinal as u32),
                    locals,
                    budget,
                    depth,
                )?;
                let made = next(function, &mut body, budget)?;
                if made.kind
                    != (CtInstructionKind::MakeVariant {
                        schema: *b,
                        tag: ordinal as u32,
                        fields,
                    })
                {
                    return Err(unsupported(
                        "CT relocation enum changes active tag or copied payload",
                    ));
                }
                let made = result(function, made, target, budget)?;
                let stored = next(function, &mut body, budget)?;
                if stored.result.is_some()
                    || stored.kind != (CtInstructionKind::Store { local, value: made })
                {
                    return Err(unsupported(
                        "CT relocation enum branch does not store its actual reconstruction",
                    ));
                }
                let CtTerminator::Jump(destination) = *end(function, body)? else {
                    return Err(unsupported(
                        "CT relocation enum branch does not join its result",
                    ));
                };
                if join
                    .replace(destination)
                    .is_some_and(|previous| previous != destination)
                {
                    return Err(unsupported(
                        "CT relocation enum variants join different merge blocks",
                    ));
                }
                *cursor = Cursor {
                    block: else_block,
                    instruction: 0,
                };
            }
            if *end(function, *cursor)? != CtTerminator::Trap {
                return Err(unsupported(
                    "CT relocation enum has an unvalidated extra tag path",
                ));
            }
            *cursor = Cursor {
                block: join.ok_or_else(|| unsupported("CT relocation enum has no variant"))?,
                instruction: 0,
            };
            let loaded = next(function, cursor, budget)?;
            if loaded.kind != CtInstructionKind::Load(local) {
                return Err(unsupported("CT relocation enum merge loads another local"));
            }
            let output = result(function, loaded, target, budget)?;
            let dead = next(function, cursor, budget)?;
            if dead.result.is_some() || dead.kind != CtInstructionKind::StorageDead(local) {
                return Err(unsupported("CT relocation enum merge local is not closed"));
            }
            Ok(output)
        }
        _ => Err(unsupported(
            "CT relocation SSA has no independently checked pure conversion",
        )),
    }
}

impl FrozenInvocationContext {
    fn relocation_context_pair(
        &self,
        left: &Option<CtTypeRef>,
        right: &Option<CtTypeRef>,
        call: &SourceInstanceCall,
        raw: &CtArtifactProgram,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        let (left, right) = match (left, right) {
            (None, None) => return Ok(()),
            (Some(a), Some(b)) => (a, b),
            _ => {
                return Err(unsupported(
                    "CT relocation changes lifetime context presence",
                ));
            }
        };
        for graph in [left, right] {
            let cost = super::super::execute::value_context::graph_cost(graph, budget)?;
            budget.reserve_analysis_work(cost)?;
        }
        if left.nodes != right.nodes
            || left.root != right.root
            || left.ambient_classes.len() != right.ambient_classes.len()
        {
            return Err(unsupported(
                "CT relocation changes the complete nominal context graph",
            ));
        }
        for (a, b) in left.ambient_classes.iter().zip(&right.ambient_classes) {
            if a.owner != b.owner || a.class != b.class {
                return Err(unsupported(
                    "CT relocation changes an actual owner/class position",
                ));
            }
            self.validate_reference(&raw.ambient_life_scopes, a.source, budget)?;
            self.validate_reference(&raw.ambient_life_scopes, b.source, budget)?;
            if self.resolve_reference(&raw.ambient_life_scopes, a.source, budget)?
                != self.resolve_reference(&raw.ambient_life_scopes, b.source, budget)?
            {
                return Err(unsupported(
                    "CT relocation changes the original source lifetime endpoint/full ordinal",
                ));
            }
            self.relocation_source_scope(a.source, call, raw, budget)?;
            if !self.relocation_scope_admits(b.source, call.caller_scope_id, raw, budget)? {
                return Err(unsupported(
                    "CT relocation target reference is outside the authentic caller/host actuals",
                ));
            }
        }
        Ok(())
    }

    fn relocation_scope_admits(
        &self,
        reference: CtAmbientLifeReference,
        mut scope: CtAmbientScopeId,
        raw: &CtArtifactProgram,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<bool> {
        let expected = self.resolve_reference(&raw.ambient_life_scopes, reference, budget)?;
        for depth in 0..=budget.remaining().max_call_depth as usize {
            budget.reserve_analysis_work(1)?;
            let current = raw
                .ambient_life_scopes
                .get(scope.0 as usize)
                .ok_or_else(|| unsupported("CT relocation actual-scope chain is absent"))?;
            if scope == reference.scope {
                return Ok(true);
            }
            if let CtScopeInstance::Instantiated(actuals) = &current.instance {
                budget.reserve_analysis_work(actuals.len() as u64)?;
                for actual in actuals {
                    match actual {
                        CtScopeActual::Lifetime(actual) => {
                            self.validate_reference(&raw.ambient_life_scopes, *actual, budget)?;
                            if self.resolve_reference(&raw.ambient_life_scopes, *actual, budget)?
                                == expected
                            {
                                return Ok(true);
                            }
                        }
                        CtScopeActual::Type(graph) => {
                            let cost =
                                super::super::execute::value_context::graph_cost(graph, budget)?;
                            budget.reserve_analysis_work(cost)?;
                            for class in &graph.ambient_classes {
                                self.validate_reference(
                                    &raw.ambient_life_scopes,
                                    class.source,
                                    budget,
                                )?;
                                if self.resolve_reference(
                                    &raw.ambient_life_scopes,
                                    class.source,
                                    budget,
                                )? == expected
                                {
                                    return Ok(true);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            let Some(parent) = current.enclosing else {
                return Ok(false);
            };
            if depth == budget.remaining().max_call_depth as usize {
                break;
            }
            scope = parent;
        }
        Err(vec![Diagnostic::limit(
            Phase::Ct,
            "CT relocation actual scope chain depth exceeded",
            None,
        )])
    }

    fn relocation_source_scope(
        &self,
        reference: CtAmbientLifeReference,
        call: &SourceInstanceCall,
        raw: &CtArtifactProgram,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        budget.reserve_analysis_work(raw.ambient_life_scopes.len() as u64)?;
        let mut complete_instance = false;
        for (index, scope) in raw.ambient_life_scopes.iter().enumerate() {
            if scope.declaration != call.callee
                || !scope.binder_site.is_empty()
                || scope.enclosing != call.enclosing
            {
                continue;
            }
            let actuals = match &scope.instance {
                CtScopeInstance::Instantiated(actuals) => actuals.as_slice(),
                // An own header with zero parameters may be a Definition under
                // its actual instantiated host. The source call has already
                // independently established this exact empty header/vector.
                CtScopeInstance::Definition if call.actuals.is_empty() => &[],
                CtScopeInstance::Definition => continue,
            };
            let cost = super::super::execute::value_context::ambient_cost(
                std::slice::from_ref(scope),
                budget,
            )?;
            budget.reserve_analysis_work(cost)?;
            for actual in &call.actuals {
                if let CtScopeActual::Type(graph) = actual {
                    let cost = super::super::execute::value_context::graph_cost(graph, budget)?;
                    budget.reserve_analysis_work(cost)?;
                }
            }
            if actuals != call.actuals.as_slice() {
                continue;
            }
            complete_instance = true;
            if self.relocation_scope_admits(
                reference,
                CtAmbientScopeId(index as u32),
                raw,
                budget,
            )? {
                return Ok(());
            }
        }
        Err(unsupported(if complete_instance {
            "CT relocation source reference is outside the actual callee complete instance/host actuals"
        } else {
            "CT relocation source full instance differs from actual call"
        }))
    }

    fn relocation_pair(
        &self,
        source: &CtTy,
        target: &CtTy,
        call: &SourceInstanceCall,
        raw: &CtArtifactProgram,
        budget: &mut CtExecutionBudget,
        depth: usize,
    ) -> CompileResult<()> {
        budget.reserve_analysis_work(1)?;
        if depth > budget.remaining().max_call_depth as usize {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT relocation pure pair depth exceeded",
                None,
            )]);
        }
        match (source, target) {
            (CtTy::Scalar(a), CtTy::Scalar(b)) if a == b => Ok(()),
            (CtTy::Tuple(a), CtTy::Tuple(b)) if a.len() == b.len() => {
                for (a, b) in a.iter().zip(b) {
                    self.relocation_pair(a, b, call, raw, budget, depth + 1)?
                }
                Ok(())
            }
            (
                CtTy::Array {
                    element: a,
                    length: na,
                },
                CtTy::Array {
                    element: b,
                    length: nb,
                },
            ) if na == nb => self.relocation_pair(a, b, call, raw, budget, depth + 1),
            (CtTy::Record(a), CtTy::Record(b)) => {
                let a = raw
                    .record_schemas
                    .get(a.0 as usize)
                    .ok_or_else(|| unsupported("CT relocation actual source record is absent"))?;
                let b = raw
                    .record_schemas
                    .get(b.0 as usize)
                    .ok_or_else(|| unsupported("CT relocation actual target record is absent"))?;
                budget.reserve_analysis_work(64 + a.fields.len() as u64 + b.fields.len() as u64)?;
                if a.origin != CtSchemaOrigin::UserPure
                    || b.origin != CtSchemaOrigin::UserPure
                    || a.name != b.name
                    || a.fields.len() != b.fields.len()
                {
                    return Err(unsupported(
                        "CT relocation changes complete pure nominal record identity/shape",
                    ));
                }
                self.relocation_arguments(&a.arguments, &b.arguments, budget)?;
                self.relocation_context_pair(&a.life_context, &b.life_context, call, raw, budget)?;
                for (a, b) in a.fields.iter().zip(&b.fields) {
                    self.relocation_pair(a, b, call, raw, budget, depth + 1)?
                }
                Ok(())
            }
            (CtTy::Enum(a), CtTy::Enum(b)) => {
                let a = raw
                    .enum_schemas
                    .get(a.0 as usize)
                    .ok_or_else(|| unsupported("CT relocation source enum is absent"))?;
                let b = raw
                    .enum_schemas
                    .get(b.0 as usize)
                    .ok_or_else(|| unsupported("CT relocation target enum is absent"))?;
                budget.reserve_analysis_work(
                    64 + a.variants.len() as u64 + b.variants.len() as u64,
                )?;
                if a.origin != CtSchemaOrigin::UserPure
                    || b.origin != CtSchemaOrigin::UserPure
                    || a.name != b.name
                    || a.variants.is_empty()
                    || a.variants.len() != b.variants.len()
                {
                    return Err(unsupported(
                        "CT relocation changes complete pure nominal enum identity/shape",
                    ));
                }
                self.relocation_arguments(&a.arguments, &b.arguments, budget)?;
                self.relocation_context_pair(&a.life_context, &b.life_context, call, raw, budget)?;
                for (a, b) in a.variants.iter().zip(&b.variants) {
                    budget.reserve_analysis_work(a.len() as u64 + b.len() as u64 + 1)?;
                    if a.len() != b.len() {
                        return Err(unsupported("CT relocation changes actual variant arity"));
                    }
                    for (a, b) in a.iter().zip(b) {
                        self.relocation_pair(a, b, call, raw, budget, depth + 1)?
                    }
                }
                Ok(())
            }
            _ => Err(unsupported(
                "CT relocation pair is not the same finite ordinary pure value",
            )),
        }
    }
    fn relocation_arguments(
        &self,
        a: &[CtSchemaArgument],
        b: &[CtSchemaArgument],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        budget.reserve_analysis_work(a.len() as u64 + b.len() as u64)?;
        for argument in a.iter().chain(b) {
            match argument {
                CtSchemaArgument::Type(graph) => {
                    let cost = super::super::execute::value_context::graph_cost(graph, budget)?;
                    budget.reserve_analysis_work(cost)?
                }
                CtSchemaArgument::Scalar(Constant::WideString(_)) => {
                    return Err(unsupported(
                        "CT relocation const actual is not a pure scalar",
                    ));
                }
                _ => {}
            }
        }
        if a != b {
            return Err(unsupported(
                "CT relocation changes the complete ordered nominal actual vector",
            ));
        }
        Ok(())
    }

    pub(in crate::ct_artifact) fn validate_record_relocations(
        &self,
        rows: &[SourceRecordRelocation],
        calls: &[SourceInstanceCall],
        raw: &CtArtifactProgram,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        budget.reserve_analysis_work(rows.len() as u64)?;
        for (position, row) in rows.iter().enumerate() {
            budget.reserve_analysis_work(position as u64 + calls.len() as u64 + 1)?;
            if rows[..position]
                .iter()
                .any(|prior| prior.caller == row.caller && prior.input == row.input)
            {
                return Err(unsupported("CT record relocation repeats a call result"));
            }
            let function = raw
                .functions
                .get(row.caller.0 as usize)
                .ok_or_else(|| unsupported("CT relocation caller is absent"))?;
            let input_block = block(function, row.block)?;
            budget.reserve_analysis_work(input_block.instructions.len() as u64)?;
            let at = input_block
                .instructions
                .iter()
                .position(|instruction| instruction.result == Some(row.input))
                .ok_or_else(|| {
                    unsupported("CT relocation input has no actual definition in its block")
                })?;
            let CtInstructionKind::Call {
                function: instance, ..
            } = input_block.instructions[at].kind
            else {
                return Err(unsupported("CT relocation input is not an actual call"));
            };
            let mut matching = calls.iter().filter(|call| {
                call.caller == row.caller
                    && call.instance == instance
                    && call.occurrence == Some((row.block, row.input))
            });
            let call = matching.next().ok_or_else(|| {
                unsupported("CT relocation input has no authentic exact source-call occurrence")
            })?;
            if matching.next().is_some() {
                return Err(unsupported(
                    "CT relocation exact source-call occurrence is ambiguous",
                ));
            }
            let callee = raw
                .functions
                .get(instance.0 as usize)
                .ok_or_else(|| unsupported("CT relocation callee is absent"))?;
            if callee.definition != call.callee
                || function.definition != call.caller_definition
                || callee.result_type != CtTy::Record(row.source)
                || function.result_type != CtTy::Record(row.target)
                || function.values.get(row.input.0 as usize) != Some(&CtTy::Record(row.source))
                || function.values.get(row.output.0 as usize) != Some(&CtTy::Record(row.target))
                || row.source == row.target
            {
                return Err(unsupported(
                    "CT relocation differs from the actual call/return/SSA types",
                ));
            }
            let source = raw
                .record_schemas
                .get(row.source.0 as usize)
                .ok_or_else(|| unsupported("CT relocation source schema is absent"))?;
            let target = raw
                .record_schemas
                .get(row.target.0 as usize)
                .ok_or_else(|| unsupported("CT relocation target schema is absent"))?;
            if source.fields.is_empty()
                || source
                    .life_context
                    .as_ref()
                    .is_none_or(|graph| graph.ambient_classes.is_empty())
                || target
                    .life_context
                    .as_ref()
                    .is_none_or(|graph| graph.ambient_classes.is_empty())
            {
                return Err(unsupported(
                    "CT relocation has no actual contextual nominal return",
                ));
            }
            self.relocation_pair(
                &CtTy::Record(row.source),
                &CtTy::Record(row.target),
                call,
                raw,
                budget,
                0,
            )?;
            let mut cursor = Cursor {
                block: row.block,
                instruction: at + 1,
            };
            let mut locals = Locals::new();
            if copy_trace(
                function,
                raw,
                &mut cursor,
                row.input,
                &CtTy::Record(row.source),
                &CtTy::Record(row.target),
                &mut locals,
                budget,
                0,
            )? != row.output
            {
                return Err(unsupported(
                    "CT relocation output is not its independently reconstructed exact copy",
                ));
            }
            if !locals.is_empty() {
                for block in &function.blocks {
                    budget.reserve_analysis_work(block.instructions.len() as u64)?;
                    for instruction in &block.instructions {
                        let (local, kind) = match instruction.kind {
                            CtInstructionKind::StorageLive(local) => (local, 0),
                            CtInstructionKind::StorageDead(local) => (local, 1),
                            CtInstructionKind::Load(local) => (local, 2),
                            CtInstructionKind::Store { local, .. } => (local, 3),
                            _ => continue,
                        };
                        budget.reserve_analysis_work(locals.len() as u64 + 1)?;
                        if let Some((_, _, counts)) =
                            locals.iter_mut().find(|(id, _, _)| *id == local)
                        {
                            counts[kind] += 1;
                        }
                    }
                }
                for (_, stores, counts) in locals {
                    if counts != [1, 1, 1, stores] {
                        return Err(unsupported(
                            "CT relocation enum merge local has extra definitions, stores or uses",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
