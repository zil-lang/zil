//! A mixed call can copy a pure record into the caller's already declared
//! return context. Both original schemas and source bindings remain intact.
use super::*;
use cfg::{Builder, Value};

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

impl Compiler<'_> {
    fn record_contexts_match(
        &mut self,
        source: CtRecordSchemaId,
        target: CtRecordSchemaId,
        span: Span,
    ) -> R<bool> {
        let left = &self.records[source.0 as usize];
        let right = &self.records[target.0 as usize];
        self.budget.work(
            left.arguments
                .len()
                .saturating_add(right.arguments.len())
                .saturating_add(left.fields.len())
                .saturating_add(right.fields.len())
                .saturating_add(64),
            span,
        )?;
        for argument in left.arguments.iter().chain(&right.arguments) {
            if let CtSchemaArgument::Type(graph) = argument {
                self.budget.metadata(graph, span)?;
            }
        }
        if left.origin != CtSchemaOrigin::UserPure
            || right.origin != CtSchemaOrigin::UserPure
            || left.name != right.name
            || left.fields.is_empty()
            || left
                .fields
                .iter()
                .chain(&right.fields)
                .any(|ty| *ty != CtTy::Scalar(CtScalarTy::I32))
            || left.arguments != right.arguments
            || left.fields != right.fields
        {
            return Ok(false);
        }
        let (Some(left), Some(right)) = (&left.life_context, &right.life_context) else {
            return Ok(false);
        };
        self.budget.metadata(left, span)?;
        self.budget.metadata(right, span)?;
        if left.root != right.root
            || left.nodes != right.nodes
            || left.ambient_classes.is_empty()
            || left.ambient_classes.len() != right.ambient_classes.len()
        {
            return Ok(false);
        }
        for (a, b) in left.ambient_classes.iter().zip(&right.ambient_classes) {
            if a.owner != left.root
                || b.owner != right.root
                || a.owner != b.owner
                || a.class != b.class
            {
                return Ok(false);
            }
            if endpoint(&self.ambient_life_scopes, &mut self.budget, a.source, span)?
                != endpoint(&self.ambient_life_scopes, &mut self.budget, b.source, span)?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl Builder<'_, '_> {
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
            || !self.compiler.record_contexts_match(source, target, span)?
        {
            return Ok(value);
        }
        let fields = self.compiler.records[source.0 as usize].fields.len();
        // The private row is also a physical backing allocation. Account the
        // entire new exact capacity and relocation work before any growth.
        self.compiler.budget.node(span)?;
        self.compiler
            .budget
            .work(self.compiler.record_relocations.len(), span)?;
        Budget::take(
            &mut self.compiler.budget.bytes,
            self.compiler
                .record_relocations
                .len()
                .saturating_add(1)
                .saturating_mul(std::mem::size_of::<SourceRecordRelocation>()),
            span,
            "CT record relocation ledger exhausted",
        )?;
        self.compiler.record_relocations.reserve_exact(1);
        let block = self.reserve_record_relocation(fields, span)?;
        let mut copied = Vec::with_capacity(fields);
        for index in 0..fields {
            copied.push(
                self.emit(
                    CtInstructionKind::ExtractField {
                        aggregate: value.id,
                        index: index as u32,
                    },
                    CtTy::Scalar(CtScalarTy::I32),
                    span,
                )?
                .id,
            );
        }
        let result = self.emit(
            CtInstructionKind::MakeRecord {
                schema: target,
                fields: copied,
            },
            CtTy::Record(target),
            span,
        )?;
        self.compiler
            .record_relocations
            .push(SourceRecordRelocation {
                caller: self.function_id(),
                block,
                input: value.id,
                source,
                target,
                output: result.id,
            });
        Ok(result)
    }
}
