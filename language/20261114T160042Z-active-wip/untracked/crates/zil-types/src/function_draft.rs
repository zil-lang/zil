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
        self.return_type.as_ref().ok_or_else(|| error(
            "E-CLOSURE-INFERENCE", self.span,
            "actual Task result awaits its real return emission",
        ))
    }
    pub(crate) fn publish(self) -> R<TypedFunction> {
        let return_type = self.return_type.ok_or_else(|| error(
            "E-CLOSURE-INFERENCE", self.span,
            "actual function result remains unresolved after all return events",
        ))?;
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
        self.return_type.ok_or_else(|| error(
            "E-CLOSURE-INFERENCE", self.span,
            "this actual directory query requires the unfinished function result",
        ))
    }
}
