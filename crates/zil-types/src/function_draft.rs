//! Private construction state. An unresolved result never enters Typed/Wire IR.
use super::*;

pub(crate) struct FunctionDraft {
    pub(crate) dyn_instance: Option<DynFunctionInstance>,
    pub(crate) retained_return_sources: Option<Vec<StorageSourceBinding>>,
    pub(crate) storage_life: Option<StorageLifeContract>,
    pub(crate) call_kind: CallKind,
    pub(crate) resume_bound: TaskResumeBound,
    pub(crate) reference_return_bound: Option<Vec<u32>>,
    pub(crate) callable_return_contract: Option<CallableReturnContract>,
    pub(crate) callable_contract: Option<CallableParameterContract>,
    pub(crate) protocol_contracts: Vec<FunctionProtocolContract>,
    pub(crate) id: FunctionId,
    pub(crate) name: String,
    pub(crate) foreign: Option<ForeignDeclaration>,
    pub(crate) environment: Option<ClosureId>,
    pub(crate) parameters: Vec<LocalId>,
    pub(crate) return_type: Option<Type>,
    pub(crate) result_mode: PassingMode,
    pub(crate) locals: Vec<LocalDecl>,
    pub(crate) values: Vec<Type>,
    pub(crate) blocks: Vec<BasicBlock>,
    pub(crate) entry: BlockId,
    pub(crate) span: Span,
}
impl FunctionDraft {
    pub(crate) fn result(&self) -> R<&Type> {
        self.return_type.as_ref().ok_or_else(|| {
            error(
                "E-CLOSURE-INFERENCE",
                self.span,
                "actual Task result awaits its real return emission",
            )
        })
    }
    pub(crate) fn publish(self) -> R<TypedFunction> {
        let return_type = self.return_type.ok_or_else(|| {
            error(
                "E-CLOSURE-INFERENCE",
                self.span,
                "actual function result remains unresolved after all return events",
            )
        })?;
        Ok(TypedFunction {
            dyn_instance: self.dyn_instance,
            retained_return_sources: self.retained_return_sources,
            storage_life: self.storage_life,
            call_kind: self.call_kind,
            resume_bound: self.resume_bound,
            reference_return_bound: self.reference_return_bound,
            callable_return_contract: self.callable_return_contract,
            callable_contract: self.callable_contract,
            protocol_contracts: self.protocol_contracts,
            id: self.id,
            name: self.name,
            foreign: self.foreign,
            environment: self.environment,
            parameters: self.parameters,
            return_type: return_type,
            result_mode: self.result_mode,
            locals: self.locals,
            values: self.values,
            blocks: self.blocks,
            entry: self.entry,
            span: self.span,
        })
    }
    pub(crate) fn view(&self) -> FunctionRef<'_> {
        FunctionRef {
            dyn_instance: &self.dyn_instance,
            retained_return_sources: &self.retained_return_sources,
            storage_life: &self.storage_life,
            call_kind: self.call_kind,
            resume_bound: self.resume_bound,
            reference_return_bound: &self.reference_return_bound,
            callable_return_contract: &self.callable_return_contract,
            callable_contract: &self.callable_contract,
            protocol_contracts: &self.protocol_contracts,
            id: self.id,
            name: &self.name,
            foreign: &self.foreign,
            environment: self.environment,
            parameters: &self.parameters,
            return_type: self.return_type.as_ref(),
            result_mode: self.result_mode,
            locals: &self.locals,
            values: &self.values,
            blocks: &self.blocks,
            entry: self.entry,
            span: self.span,
        }
    }
}

/// A read-only borrow of real published or in-progress emitter products.
/// Missing result is explicit; consumers requiring it must wait.
#[derive(Clone, Copy)]
pub(crate) struct FunctionRef<'a> {
    pub(crate) dyn_instance: &'a Option<DynFunctionInstance>,
    pub(crate) retained_return_sources: &'a Option<Vec<StorageSourceBinding>>,
    pub(crate) storage_life: &'a Option<StorageLifeContract>,
    pub(crate) call_kind: CallKind,
    pub(crate) resume_bound: TaskResumeBound,
    pub(crate) reference_return_bound: &'a Option<Vec<u32>>,
    pub(crate) callable_return_contract: &'a Option<CallableReturnContract>,
    pub(crate) callable_contract: &'a Option<CallableParameterContract>,
    pub(crate) protocol_contracts: &'a Vec<FunctionProtocolContract>,
    pub(crate) id: FunctionId,
    pub(crate) name: &'a String,
    pub(crate) foreign: &'a Option<ForeignDeclaration>,
    pub(crate) environment: Option<ClosureId>,
    pub(crate) parameters: &'a Vec<LocalId>,
    pub(crate) return_type: Option<&'a Type>,
    pub(crate) result_mode: PassingMode,
    pub(crate) locals: &'a Vec<LocalDecl>,
    pub(crate) values: &'a Vec<Type>,
    pub(crate) blocks: &'a Vec<BasicBlock>,
    pub(crate) entry: BlockId,
    pub(crate) span: Span,
}
impl<'a> From<&'a TypedFunction> for FunctionRef<'a> {
    fn from(function: &'a TypedFunction) -> Self {
        Self {
            dyn_instance: &function.dyn_instance,
            retained_return_sources: &function.retained_return_sources,
            storage_life: &function.storage_life,
            call_kind: function.call_kind,
            resume_bound: function.resume_bound,
            reference_return_bound: &function.reference_return_bound,
            callable_return_contract: &function.callable_return_contract,
            callable_contract: &function.callable_contract,
            protocol_contracts: &function.protocol_contracts,
            id: function.id,
            name: &function.name,
            foreign: &function.foreign,
            environment: function.environment,
            parameters: &function.parameters,
            return_type: Some(&function.return_type),
            result_mode: function.result_mode,
            locals: &function.locals,
            values: &function.values,
            blocks: &function.blocks,
            entry: function.entry,
            span: function.span,
        }
    }
}
impl<'a> FunctionRef<'a> {
    pub(crate) fn result(self) -> R<&'a Type> {
        self.return_type.ok_or_else(|| {
            error(
                "E-CLOSURE-INFERENCE",
                self.span,
                "this actual directory query requires the unfinished function result",
            )
        })
    }
}

impl Builder<'_, '_> {
    fn draft_return_result(&mut self, actual: &Type, span: Span) -> R<()> {
        if self.function.return_type.is_some() {
            return Ok(());
        }
        let id = self.draft_result_owner.ok_or_else(|| {
            error(
                "E-CLOSURE-INFERENCE",
                span,
                "deferred result has no original inference variable",
            )
        })?;
        let before = self.closures.remaining_work();
        let mut left = before;
        let resolved = self.closures.pending[id.0 as usize]
            .inference
            .emitted_return(actual, span, &mut left);
        self.closures.snapshot(before - left, span)?;
        if let Some(result) = resolved? {
            self.closures.array_snapshot(&result, 1, span)?;
            self.function.return_type = Some(result);
        }
        Ok(())
    }
    fn draft_error_type(&self, span: Span) -> R<Option<Type>> {
        let Some(id) = self.draft_result_owner else {
            return Ok(None);
        };
        self.closures.pending[id.0 as usize]
            .inference
            .draft_error(span)
    }
    fn bind_draft_error(&mut self, actual: &Type, span: Span) -> R<()> {
        let id = self.draft_result_owner.ok_or_else(|| {
            error(
                "E-TRY-RETURN-TYPE",
                span,
                "? requires an enclosing error-union result",
            )
        })?;
        let before = self.closures.remaining_work();
        let mut left = before;
        let result = self.closures.pending[id.0 as usize]
            .inference
            .emitted_error(actual, span, &mut left);
        self.closures.snapshot(before - left, span)?;
        result
    }
    fn defer_error_return(
        &mut self,
        payload: ValueId,
        ty: Type,
        span: Span,
        exit: CleanupExitKind,
    ) -> R<()> {
        let block = self
            .current
            .ok_or_else(|| unsupported(span, "deferred return has no live block"))?;
        self.closures
            .snapshot(self.draft_error_returns.len().saturating_add(1), span)?;
        self.end_unsafe_regions(0, span)?;
        self.end_scopes_kind(0, exit, span)?;
        // Unreachable is only an internal unfinished block terminator. It is
        // replaced before publication, never used as a completed exit proof.
        self.terminate(Terminator::Unreachable);
        self.draft_error_returns.push(DeferredErrorReturn {
            block,
            payload,
            ty,
            span,
        });
        Ok(())
    }
    fn finish_draft_result(&mut self, span: Span) -> R<()> {
        if self.draft_result_owner.is_none() && self.draft_error_returns.is_empty() {
            return Ok(());
        }
        let result = self.function.result()?;
        self.closures.array_snapshot(result, 1, span)?;
        let result = result.clone();
        let current = self.current;
        let returns = std::mem::take(&mut self.draft_error_returns);
        for pending in returns {
            if !matches!(&result,Type::ErrorUnion { error, .. } if **error==pending.ty) {
                return Err(error(
                    "E-TRY-ERROR-TYPE",
                    pending.span,
                    "deferred error return differs from the completed result error arm",
                ));
            }
            let block = self
                .function
                .blocks
                .get(pending.block.0 as usize)
                .ok_or_else(|| {
                    unsupported(pending.span, "deferred return lost its actual block")
                })?;
            if !matches!(block.terminator, Terminator::Unreachable) {
                return Err(error(
                    "E-CLOSURE-INFERENCE",
                    pending.span,
                    "deferred return block acquired a different terminator",
                ));
            }
            self.current = Some(pending.block);
            let value = self.value(
                InstructionKind::MakeErrorUnion {
                    value: pending.payload,
                    is_error: true,
                },
                result.clone(),
                pending.span,
            )?;
            self.terminate(Terminator::Return(Some(value)));
        }
        self.current = current;
        let Some(id) = self.draft_result_owner else {
            return Ok(());
        };
        let before = self.closures.remaining_work();
        let mut left = before;
        let state = &mut *self.closures;
        let (earlier, tail) = state.pending.split_at_mut(id.0 as usize);
        let (pending, later) = tail
            .split_first_mut()
            .ok_or_else(|| unsupported(span, "closure result owner disappeared"))?;
        left = left
            .checked_sub(earlier.len().saturating_add(later.len()))
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "actual callable directory work exceeded",
                    Some(span),
                )
            })?;
        let entries = earlier.iter().chain(later.iter()).collect::<Vec<_>>();
        let completed = pending.inference.complete_actual_tasks(
            self.function.view(),
            &state.semantic_emission,
            &entries,
            &mut left,
        );
        self.closures.snapshot(before - left, span)?;
        completed?;
        let (parameters, expected) = self.closures.pending[id.0 as usize]
            .inference
            .resolve(true, span)?;
        if expected != result
            || parameters.len() != self.function.parameters.len()
            || parameters
                .iter()
                .zip(&self.function.parameters)
                .any(|(ty, id)| {
                    self.function
                        .locals
                        .get(id.0 as usize)
                        .is_none_or(|local| &local.ty != ty)
                })
        {
            return Err(error(
                "E-CLOSURE-INFERENCE",
                span,
                "actual complete closure signature differs from its original inference constraints",
            ));
        }
        let header = self
            .closures
            .form_source_contract(id, &parameters, &result, span)?;
        if self.function.storage_life.is_some() && self.function.storage_life != header {
            return Err(error(
                "E-CLOSURE-INFERENCE",
                span,
                "actual result changed the formed closure storage relation",
            ));
        }
        self.function.storage_life = header;
        self.closures.check_capture_result(id, &result, span)?;
        Ok(())
    }
}

impl closures::State {
    pub(super) fn check_capture_result(
        &mut self,
        id: ClosureId,
        result: &Type,
        span: Span,
    ) -> R<()> {
        let captures = self.pending[id.0 as usize].captures.len();
        self.snapshot(captures.saturating_add(1), span)?;
        let pending = &self.pending[id.0 as usize];
        if closures::owned_signature_head(result)
            && !result.is_copy_value()
            && pending.captures.iter().any(|capture| {
                (capture.borrowed && matches!(capture.ty, Type::Package { .. }))
                    || capture.ty.is_shared_package_address()
            })
        {
            return Err(unsupported(
                span,
                "shared Product capture environments require Copy results",
            ));
        }
        if pending.owns_leaf_store && !result.is_copy_value() {
            return Err(unsupported(
                span,
                "owned Store capture environments require Copy results",
            ));
        }
        Ok(())
    }
}
