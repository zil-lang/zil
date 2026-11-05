//! Fresh source calls may copy finite ordinary values into an independently
//! declared return context. Both original schemas and full scopes remain.
use super::*;
use cfg::{Builder, Value};

fn copy_types(budget: &mut Budget<'_>, fields: &[CtTy], span: Span) -> R<()> {
    budget.work(fields.len(), span)?;
    for ty in fields {
        budget.copy_ty(ty, 0, span)?;
    }
    Ok(())
}

fn endpoint(
    scopes: &[CtAmbientLifeScope],
    budget: &mut Budget<'_>,
    mut reference: CtAmbientLifeReference,
    span: Span,
) -> R<(CtSymbolId, u32)> {
    for depth in 0..=budget.limits.max_call_depth {
        budget.depth(depth, span)?;
        budget.work(32, span)?;
        let scope = scopes
            .get(reference.scope.0 as usize)
            .ok_or_else(|| error(span, "CT relocation life scope is absent"))?;
        match &scope.instance {
            CtScopeInstance::Definition => {
                return Ok((scope.declaration.clone(), reference.parameter));
            }
            CtScopeInstance::Instantiated(actuals) => {
                let Some(CtScopeActual::Lifetime(next)) = actuals.get(reference.parameter as usize)
                else {
                    return Err(error(span, "CT relocation selects a non-life actual"));
                };
                reference = *next;
            }
        }
    }
    Err(vec![Diagnostic::limit(
        Phase::Ct,
        "CT relocation life depth exceeded",
        Some(span),
    )])
}

fn context_pair(
    left: &Option<CtTypeRef>,
    right: &Option<CtTypeRef>,
    scopes: &[CtAmbientLifeScope],
    budget: &mut Budget<'_>,
    span: Span,
) -> R<bool> {
    let (left, right) = match (left, right) {
        (None, None) => return Ok(true),
        (Some(a), Some(b)) => (a, b),
        _ => return Ok(false),
    };
    budget.metadata(left, span)?;
    budget.metadata(right, span)?;
    if left.root != right.root
        || left.nodes != right.nodes
        || left.ambient_classes.len() != right.ambient_classes.len()
    {
        return Ok(false);
    }
    for (a, b) in left.ambient_classes.iter().zip(&right.ambient_classes) {
        if a.owner != b.owner
            || a.class != b.class
            || endpoint(scopes, budget, a.source, span)?
                != endpoint(scopes, budget, b.source, span)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn arguments_pair(
    left: &[CtSchemaArgument],
    right: &[CtSchemaArgument],
    budget: &mut Budget<'_>,
    span: Span,
) -> R<bool> {
    budget.work(left.len().saturating_add(right.len()), span)?;
    for argument in left.iter().chain(right) {
        match argument {
            CtSchemaArgument::Type(graph) => budget.metadata(graph, span)?,
            CtSchemaArgument::Scalar(Constant::WideString(_)) => return Ok(false),
            _ => {}
        }
    }
    Ok(left == right)
}

fn pure_pair(
    left: &CtTy,
    right: &CtTy,
    records: &[CtRecordSchema],
    enums: &[CtEnumSchema],
    scopes: &[CtAmbientLifeScope],
    budget: &mut Budget<'_>,
    depth: usize,
    span: Span,
) -> R<bool> {
    budget.depth(depth, span)?;
    budget.work(1, span)?;
    match (left, right) {
        (CtTy::Scalar(a), CtTy::Scalar(b)) => Ok(a == b),
        (CtTy::Tuple(a), CtTy::Tuple(b)) => {
            if a.len() != b.len() {
                return Ok(false);
            }
            for (a, b) in a.iter().zip(b) {
                if !pure_pair(a, b, records, enums, scopes, budget, depth + 1, span)? {
                    return Ok(false);
                }
            }
            Ok(true)
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
        ) => Ok(na == nb && pure_pair(a, b, records, enums, scopes, budget, depth + 1, span)?),
        (CtTy::Record(a), CtTy::Record(b)) => {
            let (Some(a), Some(b)) = (records.get(a.0 as usize), records.get(b.0 as usize)) else {
                return Ok(false);
            };
            budget.work(64 + a.fields.len() + b.fields.len(), span)?;
            if a.origin != CtSchemaOrigin::UserPure
                || b.origin != CtSchemaOrigin::UserPure
                || a.name != b.name
                || a.fields.len() != b.fields.len()
                || !arguments_pair(&a.arguments, &b.arguments, budget, span)?
                || !context_pair(&a.life_context, &b.life_context, scopes, budget, span)?
            {
                return Ok(false);
            }
            for (a, b) in a.fields.iter().zip(&b.fields) {
                if !pure_pair(a, b, records, enums, scopes, budget, depth + 1, span)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (CtTy::Enum(a), CtTy::Enum(b)) => {
            let (Some(a), Some(b)) = (enums.get(a.0 as usize), enums.get(b.0 as usize)) else {
                return Ok(false);
            };
            budget.work(64 + a.variants.len() + b.variants.len(), span)?;
            if a.origin != CtSchemaOrigin::UserPure
                || b.origin != CtSchemaOrigin::UserPure
                || a.name != b.name
                || a.variants.is_empty()
                || a.variants.len() != b.variants.len()
                || !arguments_pair(&a.arguments, &b.arguments, budget, span)?
                || !context_pair(&a.life_context, &b.life_context, scopes, budget, span)?
            {
                return Ok(false);
            }
            for (a, b) in a.variants.iter().zip(&b.variants) {
                budget.work(a.len() + b.len() + 1, span)?;
                if a.len() != b.len() {
                    return Ok(false);
                }
                for (a, b) in a.iter().zip(b) {
                    if !pure_pair(a, b, records, enums, scopes, budget, depth + 1, span)? {
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

impl Builder<'_, '_> {
    fn relocation_fields(
        &mut self,
        input: CtValueId,
        source: &[CtTy],
        target: &[CtTy],
        variant: Option<u32>,
        span: Span,
        depth: usize,
    ) -> R<Vec<CtValueId>> {
        Budget::take(
            &mut self.compiler.budget.bytes,
            source
                .len()
                .saturating_mul(std::mem::size_of::<CtValueId>()),
            span,
            "CT recursive relocation field vector exhausted",
        )?;
        let mut fields = Vec::with_capacity(source.len());
        for (index, (source, target)) in source.iter().zip(target).enumerate() {
            self.compiler.budget.copy_ty(source, 0, span)?;
            let kind = match variant {
                Some(tag) => CtInstructionKind::VariantField {
                    value: input,
                    variant: tag,
                    field: index as u32,
                },
                None => CtInstructionKind::ExtractField {
                    aggregate: input,
                    index: index as u32,
                },
            };
            let field = self.relocation_emit(kind, source.clone(), span)?;
            fields.push(self.relocation_copy(field, target, span, depth + 1)?.id);
        }
        Ok(fields)
    }

    fn relocation_copy(
        &mut self,
        value: Value,
        target: &CtTy,
        span: Span,
        depth: usize,
    ) -> R<Value> {
        self.compiler.budget.depth(depth, span)?;
        self.compiler.budget.work(1, span)?;
        self.compiler.budget.ty(&value.ty, 0, span)?;
        self.compiler.budget.ty(target, 0, span)?;
        if &value.ty == target {
            return Ok(value);
        }
        match (&value.ty, target) {
            (CtTy::Record(source), CtTy::Record(target)) => {
                let (source, target) = (*source, *target);
                let source_fields = &self.compiler.records[source.0 as usize].fields;
                let target_fields = &self.compiler.records[target.0 as usize].fields;
                copy_types(&mut self.compiler.budget, source_fields, span)?;
                copy_types(&mut self.compiler.budget, target_fields, span)?;
                let (source_fields, target_fields) = (source_fields.clone(), target_fields.clone());
                let fields = self.relocation_fields(
                    value.id,
                    &source_fields,
                    &target_fields,
                    None,
                    span,
                    depth,
                )?;
                self.relocation_emit(
                    CtInstructionKind::MakeRecord {
                        schema: target,
                        fields,
                    },
                    CtTy::Record(target),
                    span,
                )
            }
            (CtTy::Tuple(source), CtTy::Tuple(target)) => {
                copy_types(&mut self.compiler.budget, source, span)?;
                copy_types(&mut self.compiler.budget, target, span)?;
                let (source, target) = (source.clone(), target.clone());
                let fields =
                    self.relocation_fields(value.id, &source, &target, None, span, depth)?;
                self.relocation_emit(
                    CtInstructionKind::MakeTuple(fields),
                    CtTy::Tuple(target),
                    span,
                )
            }
            (
                CtTy::Array {
                    element: source,
                    length,
                },
                CtTy::Array {
                    element: target, ..
                },
            ) => {
                let (source, target, length) = (source.as_ref(), target.as_ref(), *length);
                self.compiler.budget.work(length as usize, span)?;
                self.compiler.budget.copy_ty(source, 0, span)?;
                self.compiler.budget.copy_ty(target, 0, span)?;
                let (source, target) = (source.clone(), target.clone());
                Budget::take(
                    &mut self.compiler.budget.bytes,
                    (length as usize).saturating_mul(std::mem::size_of::<CtValueId>()),
                    span,
                    "CT relocation array field vector exhausted",
                )?;
                let mut fields = Vec::with_capacity(length as usize);
                for index in 0..length {
                    let index = self.relocation_constant(
                        Constant::U64(index as u64),
                        CtScalarTy::U64,
                        span,
                    )?;
                    self.compiler.budget.copy_ty(&source, 0, span)?;
                    let field = self.relocation_emit(
                        CtInstructionKind::Extract {
                            aggregate: value.id,
                            index: index.id,
                        },
                        source.clone(),
                        span,
                    )?;
                    fields.push(self.relocation_copy(field, &target, span, depth + 1)?.id);
                }
                self.relocation_emit(
                    CtInstructionKind::MakeArray(fields),
                    CtTy::Array {
                        element: Box::new(target),
                        length,
                    },
                    span,
                )
            }
            (CtTy::Enum(source), CtTy::Enum(target)) => {
                let (source, target) = (*source, *target);
                let source_variants = &self.compiler.enums[source.0 as usize].variants;
                let target_variants = &self.compiler.enums[target.0 as usize].variants;
                self.compiler
                    .budget
                    .work(source_variants.len() + target_variants.len(), span)?;
                Budget::take(
                    &mut self.compiler.budget.bytes,
                    (source_variants.len() + target_variants.len())
                        .saturating_mul(std::mem::size_of::<Vec<CtTy>>()),
                    span,
                    "CT relocation variant inventory exhausted",
                )?;
                for fields in source_variants.iter().chain(target_variants) {
                    copy_types(&mut self.compiler.budget, fields, span)?;
                }
                let (source_variants, target_variants) =
                    (source_variants.clone(), target_variants.clone());
                let local = self.relocation_local(CtTy::Enum(target), span)?;
                self.relocation_op(CtInstructionKind::StorageLive(local), span)?;
                let tag = self.relocation_emit(
                    CtInstructionKind::VariantTag(value.id),
                    CtTy::Scalar(CtScalarTy::I32),
                    span,
                )?;
                let join = self.relocation_block(span)?;
                for (ordinal, (source, target_fields)) in
                    source_variants.iter().zip(&target_variants).enumerate()
                {
                    let body = self.relocation_block(span)?;
                    let next = self.relocation_block(span)?;
                    let expected = self.relocation_constant(
                        Constant::I32(ordinal as i32),
                        CtScalarTy::I32,
                        span,
                    )?;
                    let condition = self.relocation_emit(
                        CtInstructionKind::Binary {
                            op: BinaryOp::Equal,
                            left: tag.id,
                            right: expected.id,
                        },
                        CtTy::Scalar(CtScalarTy::Bool),
                        span,
                    )?;
                    self.terminate(
                        CtTerminator::Branch {
                            condition: condition.id,
                            then_block: body,
                            else_block: next,
                        },
                        span,
                    )?;
                    self.relocation_select(body);
                    let fields = self.relocation_fields(
                        value.id,
                        source,
                        target_fields,
                        Some(ordinal as u32),
                        span,
                        depth,
                    )?;
                    let made = self.relocation_emit(
                        CtInstructionKind::MakeVariant {
                            schema: target,
                            tag: ordinal as u32,
                            fields,
                        },
                        CtTy::Enum(target),
                        span,
                    )?;
                    self.relocation_op(
                        CtInstructionKind::Store {
                            local,
                            value: made.id,
                        },
                        span,
                    )?;
                    self.terminate(CtTerminator::Jump(join), span)?;
                    self.relocation_select(next);
                }
                self.terminate(CtTerminator::Trap, span)?;
                self.relocation_select(join);
                let output =
                    self.relocation_emit(CtInstructionKind::Load(local), CtTy::Enum(target), span)?;
                self.relocation_op(CtInstructionKind::StorageDead(local), span)?;
                Ok(output)
            }
            _ => Err(error(
                span,
                "CT recursive relocation lacks a proved pure type pair",
            )),
        }
    }

    pub(super) fn relocate_record_result(
        &mut self,
        value: Value,
        expected: Option<&CtTy>,
        span: Span,
    ) -> R<Value> {
        let (CtTy::Record(source), Some(CtTy::Record(target))) = (&value.ty, expected) else {
            return Ok(value);
        };
        let (source, target) = (*source, *target);
        if source == target
            || expected != Some(self.result_type())
            || !pure_pair(
                &value.ty,
                &CtTy::Record(target),
                &self.compiler.records,
                &self.compiler.enums,
                &self.compiler.ambient_life_scopes,
                &mut self.compiler.budget,
                0,
                span,
            )?
        {
            return Ok(value);
        }
        self.compiler.budget.node(span)?;
        self.compiler
            .budget
            .work(self.compiler.record_relocations.len(), span)?;
        Budget::take(
            &mut self.compiler.budget.bytes,
            (self.compiler.record_relocations.len() + 1)
                .saturating_mul(std::mem::size_of::<SourceRecordRelocation>()),
            span,
            "CT record relocation ledger exhausted",
        )?;
        self.compiler.record_relocations.reserve_exact(1);
        let block = self.reserve_record_relocation(0, span)?;
        let input = value.id;
        let instance = self.relocation_call_instance(block, input, span)?;
        let site = self.compiler.current_expression_site(span)?;
        let caller = self.function_id();
        let call = self
            .compiler
            .source_instance_calls
            .last_mut()
            .ok_or_else(|| error(span, "CT relocation has no fresh source call event"))?;
        if call.caller != caller
            || call.instance != instance
            || call.site != site
            || call.occurrence.is_some()
        {
            return Err(error(
                span,
                "CT relocation differs from its exact fresh source call occurrence",
            ));
        }
        call.occurrence = Some((block, input));
        let result = self.relocation_copy(value, &CtTy::Record(target), span, 0)?;
        self.compiler
            .record_relocations
            .push(SourceRecordRelocation {
                caller: self.function_id(),
                block,
                input,
                source,
                target,
                output: result.id,
            });
        Ok(result)
    }
}
