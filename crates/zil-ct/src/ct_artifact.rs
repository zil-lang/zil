//! Bounded execution of independently verified CT-only artifacts.
use zil_base::{CompileResult, Diagnostic, Phase};
use zil_ir::ct_artifact::*;
use zil_ir::{Constant, Type};
use zil_verify::VerifiedCTArtifactProgram;

fn error(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-ARTIFACT", None, message)]
}
fn limit(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::limit(Phase::Ct, message, None)]
}

/// One mutable ledger is passed through the entire project request. The driver
/// derives its initial remainder from the existing Limits and never resets it
/// per provider/query. Host intrinsics reserve before allocating their output.
pub struct CtExecutionBudget {
    remaining: CtDeclaredBudget,
    live_temporary_bytes: u64,
    usage: CtUsageReceipt,
}
impl CtExecutionBudget {
    pub fn new(remaining: CtDeclaredBudget) -> Self {
        Self {
            remaining,
            live_temporary_bytes: 0,
            usage: CtUsageReceipt {
                steps: 0,
                peak_call_depth: 0,
                peak_live_cells: 0,
                peak_temporary_bytes: 0,
                generated_nodes: 0,
                instances: 0,
                analysis_work: 0,
                queries: Vec::new(),
            },
        }
    }
    pub fn remaining(&self) -> CtDeclaredBudget {
        self.remaining
    }
    pub fn usage(&self) -> &CtUsageReceipt {
        &self.usage
    }
    pub fn live_temporary_bytes(&self) -> u64 {
        self.live_temporary_bytes
    }
    /// Call after the driver has materialized and discarded a returned value.
    /// Dependency receipts and other request-owned allocations remain charged.
    pub fn release_value(&mut self, value: &CtValue) {
        self.release_temporary_bytes(
            value_bytes(value).saturating_add(std::mem::size_of::<CtValue>() as u64),
        );
    }
    /// Drop the returned normalization buffer after its data consumer finishes.
    pub fn release_type(&mut self, ty: &CtTypeRef) {
        self.release_temporary_bytes(crate::anonymous_type::retained_bytes(ty));
    }
    /// Release the separately owned receipt returned by an invocation after use.
    pub fn release_usage_receipt(&mut self, receipt: &CtUsageReceipt) {
        self.release_temporary_bytes(
            receipt.queries.capacity() as u64 * std::mem::size_of::<CtQueryDependency>() as u64,
        );
    }
    fn constrain_to_program(&mut self, declared: CtDeclaredBudget) -> CompileResult<()> {
        self.remaining.max_steps = self.remaining.max_steps.min(declared.max_steps);
        self.remaining.max_call_depth = self.remaining.max_call_depth.min(declared.max_call_depth);
        self.remaining.max_live_cells = self.remaining.max_live_cells.min(declared.max_live_cells);
        self.remaining.max_temporary_bytes = self
            .remaining
            .max_temporary_bytes
            .min(declared.max_temporary_bytes);
        self.remaining.max_generated_nodes = self
            .remaining
            .max_generated_nodes
            .min(declared.max_generated_nodes);
        self.remaining.max_instances = self.remaining.max_instances.min(declared.max_instances);
        self.remaining.max_analysis_work = self
            .remaining
            .max_analysis_work
            .min(declared.max_analysis_work);
        if self.live_temporary_bytes > self.remaining.max_temporary_bytes {
            return Err(limit(
                "CT program declared temporary-byte budget is below current live bytes",
            ));
        }
        Ok(())
    }
    pub fn reserve_steps(&mut self, count: u64) -> CompileResult<()> {
        self.remaining.max_steps = self
            .remaining
            .max_steps
            .checked_sub(count)
            .ok_or_else(|| limit("CT step budget exhausted"))?;
        self.usage.steps = self
            .usage
            .steps
            .checked_add(count)
            .ok_or_else(|| limit("CT step count overflow"))?;
        Ok(())
    }
    pub fn reserve_generated_nodes(&mut self, count: u64) -> CompileResult<()> {
        self.remaining.max_generated_nodes = self
            .remaining
            .max_generated_nodes
            .checked_sub(count)
            .ok_or_else(|| limit("CT generated-node budget exhausted"))?;
        self.usage.generated_nodes = self
            .usage
            .generated_nodes
            .checked_add(count)
            .ok_or_else(|| limit("CT generated-node count overflow"))?;
        Ok(())
    }
    pub fn reserve_instance(&mut self) -> CompileResult<()> {
        self.remaining.max_instances = self
            .remaining
            .max_instances
            .checked_sub(1)
            .ok_or_else(|| limit("CT instance budget exhausted"))?;
        self.usage.instances = self
            .usage
            .instances
            .checked_add(1)
            .ok_or_else(|| limit("CT instance count overflow"))?;
        Ok(())
    }
    pub fn reserve_analysis_work(&mut self, count: u64) -> CompileResult<()> {
        self.remaining.max_analysis_work = self
            .remaining
            .max_analysis_work
            .checked_sub(count)
            .ok_or_else(|| limit("CT analysis-work budget exhausted"))?;
        self.usage.analysis_work = self
            .usage
            .analysis_work
            .checked_add(count)
            .ok_or_else(|| limit("CT analysis-work count overflow"))?;
        Ok(())
    }
    pub fn reserve_temporary_bytes(&mut self, count: u64) -> CompileResult<()> {
        let next = self
            .live_temporary_bytes
            .checked_add(count)
            .filter(|n| *n <= self.remaining.max_temporary_bytes)
            .ok_or_else(|| limit("CT temporary-byte budget exhausted"))?;
        self.live_temporary_bytes = next;
        self.usage.peak_temporary_bytes = self.usage.peak_temporary_bytes.max(next);
        Ok(())
    }
    pub fn release_temporary_bytes(&mut self, count: u64) {
        self.live_temporary_bytes = self.live_temporary_bytes.saturating_sub(count);
    }
    pub fn observe_live_cells(&mut self, count: u64) -> CompileResult<()> {
        if count > self.remaining.max_live_cells {
            return Err(limit("CT live-cell budget exhausted"));
        }
        self.usage.peak_live_cells = self.usage.peak_live_cells.max(count);
        Ok(())
    }
    fn observe_depth(&mut self, depth: usize) -> CompileResult<()> {
        if depth as u64 > self.remaining.max_call_depth as u64 {
            return Err(limit("CT call-depth budget exhausted"));
        }
        self.usage.peak_call_depth = self.usage.peak_call_depth.max(depth as u32);
        Ok(())
    }
    fn record_query(&mut self, query: CtQueryDependency) -> CompileResult<()> {
        self.reserve_generated_nodes(1)?;
        if self.usage.queries.len() == self.usage.queries.capacity() {
            let capacity = self.usage.queries.capacity().saturating_mul(2).max(1);
            let growth = capacity.saturating_sub(self.usage.queries.capacity());
            self.reserve_analysis_work(self.usage.queries.len() as u64)?;
            self.reserve_temporary_bytes(
                (growth as u64).saturating_mul(std::mem::size_of::<CtQueryDependency>() as u64),
            )?;
            self.usage
                .queries
                .reserve_exact(capacity.saturating_sub(self.usage.queries.len()));
        }
        self.usage.queries.push(query);
        Ok(())
    }
}

/// These are closed compiler services over one frozen project view. A provider
/// supplies CT code/data, never an implementation of this trait.
pub trait CtReadView {
    fn authenticate_argument(&self, index: u32, ty: &CtTy, value: &CtValue) -> CompileResult<()>;
    fn type_name(
        &self,
        requester: &CtSymbolId,
        symbol: &CtSymbolId,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtTypeRef>;
    fn type_apply(
        &self,
        requester: &CtSymbolId,
        nominal: &CtSymbolId,
        args: &[CtValue],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtTypeRef>;
    fn definition_ident(
        &self,
        requester: &CtSymbolId,
        symbol: &CtSymbolId,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtIdent>;
    /// Parse only the contents of the provider invocation's outer delimiter.
    /// Nested groups stay intact; the original envelope remains in provenance.
    fn parse(
        &self,
        category: CtSyntaxCategory,
        input: &CtTokenTree,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtSyntaxTree>;
    fn parse_ident(
        &self,
        input: &CtTokenTree,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtIdent>;
    fn query(
        &self,
        requester: &CtSymbolId,
        kind: CtQueryKind,
        args: &[CtValue],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<(CtValue, CtQueryDependency)>;
    fn build_syntax(
        &self,
        kind: CtSyntaxBuilder,
        args: &[CtValue],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtSyntaxTree>;
    fn instantiate_quote(
        &self,
        template: &CtTemplate,
        holes: &[CtValue],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtSyntaxTree>;
    fn token_inspect(
        &self,
        source: &CtValue,
        projection: CtTokenProjection,
        index: Option<i64>,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtValue>;
}

struct Frame<'a> {
    function: &'a CtFunction,
    locals: Vec<Option<CtValue>>,
    local_bytes: Vec<u64>,
    values: Vec<Option<CtValue>>,
    value_bytes: Vec<u64>,
    block: usize,
    instruction: usize,
    destination: Option<CtValueId>,
    backing_bytes: u64,
}
impl<'a> Frame<'a> {
    fn new(
        function: &'a CtFunction,
        arguments: Vec<CtValue>,
        argument_bytes: Vec<u64>,
        destination: Option<CtValueId>,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Self> {
        let cells = function.locals.len().saturating_add(function.values.len());
        budget.reserve_analysis_work(cells as u64)?;
        let backing_bytes = (cells as u64).saturating_mul(
            (std::mem::size_of::<Option<CtValue>>() + std::mem::size_of::<u64>()) as u64,
        );
        budget.reserve_temporary_bytes(backing_bytes)?;
        let mut locals = vec![None; function.locals.len()];
        let mut local_bytes = vec![0; function.locals.len()];
        for ((id, value), bytes) in function
            .parameters
            .iter()
            .zip(arguments)
            .zip(argument_bytes)
        {
            locals[id.0 as usize] = Some(value);
            local_bytes[id.0 as usize] = bytes;
        }
        Ok(Self {
            function,
            locals,
            local_bytes,
            values: vec![None; function.values.len()],
            value_bytes: vec![0; function.values.len()],
            block: function.entry.0 as usize,
            instruction: 0,
            destination,
            backing_bytes,
        })
    }
    fn value(&self, id: CtValueId) -> &CtValue {
        self.values[id.0 as usize]
            .as_ref()
            .expect("verified CT value dominates use")
    }
    fn release(self, budget: &mut CtExecutionBudget) {
        budget.release_temporary_bytes(self.backing_bytes);
        for bytes in self.local_bytes.into_iter().chain(self.value_bytes) {
            budget.release_temporary_bytes(bytes);
        }
    }
}

#[path = "ct_value_storage.rs"]
pub(crate) mod storage;
fn type_storage(ty: &CtTy, budget: Option<&mut CtExecutionBudget>) -> CompileResult<u64> {
    storage::ty(ty, budget)
}
fn value_bytes(value: &CtValue) -> u64 {
    storage::owned(value, None).expect("admitted value storage cannot fail")
}
fn copy_value_bytes(value: &CtValue, budget: &mut CtExecutionBudget) -> CompileResult<u64> {
    storage::owned(value, Some(budget))
}
fn clone_operands(
    frame: &Frame<'_>,
    ids: &[CtValueId],
    budget: &mut CtExecutionBudget,
) -> CompileResult<(Vec<CtValue>, u64)> {
    budget.reserve_analysis_work(ids.len() as u64)?;
    let mut bytes = (ids.len() as u64).saturating_mul(std::mem::size_of::<CtValue>() as u64);
    for id in ids {
        bytes = bytes.saturating_add(copy_value_bytes(frame.value(*id), budget)?);
    }
    budget.reserve_temporary_bytes(bytes)?;
    Ok((
        ids.iter().map(|id| frame.value(*id).clone()).collect(),
        bytes,
    ))
}

fn scalar_ty(value: &Constant) -> Option<CtScalarTy> {
    match value {
        Constant::Unit => Some(CtScalarTy::Unit),
        Constant::Bool(_) => Some(CtScalarTy::Bool),
        Constant::U8(_) => Some(CtScalarTy::U8),
        Constant::U16(_) => Some(CtScalarTy::U16),
        Constant::I32(_) => Some(CtScalarTy::I32),
        Constant::I64(_) => Some(CtScalarTy::I64),
        Constant::F32(_) => Some(CtScalarTy::F32),
        Constant::F64(_) => Some(CtScalarTy::F64),
        Constant::U64(_) => Some(CtScalarTy::U64),
        Constant::Usize(_) => Some(CtScalarTy::Usize),
        _ => None,
    }
}
fn ordinary_scalar(ty: CtScalarTy) -> Type {
    match ty {
        CtScalarTy::Unit => Type::Unit,
        CtScalarTy::Bool => Type::Bool,
        CtScalarTy::U8 => Type::U8,
        CtScalarTy::U16 => Type::U16,
        CtScalarTy::I32 => Type::I32,
        CtScalarTy::I64 => Type::I64,
        CtScalarTy::F32 => Type::F32,
        CtScalarTy::F64 => Type::F64,
        CtScalarTy::U64 => Type::U64,
        CtScalarTy::Usize => Type::Usize,
    }
}
fn integer_index(value: &CtValue, len: usize) -> CompileResult<usize> {
    let n = match value {
        CtValue::Scalar(Constant::U8(n)) => i128::from(*n),
        CtValue::Scalar(Constant::U16(n)) => i128::from(*n),
        CtValue::Scalar(Constant::I32(n)) => i128::from(*n),
        CtValue::Scalar(Constant::I64(n)) => i128::from(*n),
        CtValue::Scalar(Constant::U64(n) | Constant::Usize(n)) => i128::from(*n),
        _ => unreachable!("verified CT index is integer"),
    };
    usize::try_from(n)
        .ok()
        .filter(|i| *i < len)
        .ok_or_else(|| error("CT index is outside its bounds"))
}

pub fn evaluate_ct_artifact(
    program: &VerifiedCTArtifactProgram,
    arguments: &[CtValue],
    reader: &impl CtReadView,
    budget: &mut CtExecutionBudget,
) -> CompileResult<(CtValue, CtUsageReceipt)> {
    let before = budget.live_temporary_bytes;
    let query_capacity = budget.usage.queries.capacity();
    let result = evaluate_ct_artifact_inner(program, arguments, reader, budget);
    let query_bytes = (budget
        .usage
        .queries
        .capacity()
        .saturating_sub(query_capacity) as u64)
        .saturating_mul(std::mem::size_of::<CtQueryDependency>() as u64);
    let keep = result
        .as_ref()
        .map_or(0, |(value, receipt)| {
            value_bytes(value)
                .saturating_add(std::mem::size_of::<CtValue>() as u64)
                .saturating_add(
                    receipt.queries.capacity() as u64
                        * std::mem::size_of::<CtQueryDependency>() as u64,
                )
        })
        .saturating_add(query_bytes);
    let added = budget.live_temporary_bytes.saturating_sub(before);
    budget.release_temporary_bytes(added.saturating_sub(keep));
    result
}
fn reserve_frame<'a>(
    frames: &mut Vec<Frame<'a>>,
    charged: &mut u64,
    budget: &mut CtExecutionBudget,
) -> CompileResult<()> {
    if frames.len() == frames.capacity() {
        let capacity = frames.capacity().saturating_mul(2).max(1);
        let bytes = (capacity.saturating_sub(frames.capacity()) as u64)
            .saturating_mul(std::mem::size_of::<Frame<'a>>() as u64);
        budget.reserve_analysis_work(frames.len() as u64)?;
        budget.reserve_temporary_bytes(bytes)?;
        frames.reserve_exact(capacity.saturating_sub(frames.len()));
        *charged = charged.saturating_add(bytes);
    }
    Ok(())
}
fn evaluate_ct_artifact_inner(
    program: &VerifiedCTArtifactProgram,
    arguments: &[CtValue],
    reader: &impl CtReadView,
    budget: &mut CtExecutionBudget,
) -> CompileResult<(CtValue, CtUsageReceipt)> {
    let raw = program.program();
    budget.constrain_to_program(raw.budget)?;
    let entry = &raw.functions[raw.entry.0 as usize];
    if arguments.len() != entry.parameter_types.len() {
        return Err(error("CT invocation argument count differs"));
    }
    for (index, (value, ty)) in arguments.iter().zip(&entry.parameter_types).enumerate() {
        if !value_matches_type(value, ty, raw, 0, budget)? {
            return Err(error("CT invocation argument type differs"));
        }
        reader.authenticate_argument(index as u32, ty, value)?;
    }
    budget.reserve_instance()?;
    budget.observe_depth(1)?;
    let argument_vectors = (arguments.len() as u64)
        .saturating_mul((std::mem::size_of::<CtValue>() + std::mem::size_of::<u64>()) as u64);
    budget.reserve_temporary_bytes(argument_vectors)?;
    let argument_bytes: Vec<_> = arguments
        .iter()
        .map(|v| copy_value_bytes(v, budget))
        .collect::<CompileResult<_>>()?;
    for bytes in &argument_bytes {
        budget.reserve_temporary_bytes(*bytes)?;
    }
    let mut live_cells = (entry.locals.len() + entry.values.len()) as u64;
    budget.observe_live_cells(live_cells)?;
    let mut frames = Vec::new();
    let mut frame_vector_bytes = 0u64;
    reserve_frame(&mut frames, &mut frame_vector_bytes, budget)?;
    frames.push(Frame::new(
        entry,
        arguments.to_vec(),
        argument_bytes,
        None,
        budget,
    )?);
    budget.release_temporary_bytes(argument_vectors);
    loop {
        budget.reserve_steps(1)?;
        let current_depth = frames.len();
        let frame = frames.last_mut().expect("CT root frame returns");
        let block = &frame.function.blocks[frame.block];
        if let Some(inst) = block.instructions.get(frame.instruction) {
            frame.instruction += 1;
            if let CtInstructionKind::Call {
                function,
                arguments,
            } = &inst.kind
            {
                let target = &raw.functions[function.0 as usize];
                budget.observe_depth(current_depth + 1)?;
                let incoming_cells = (target.locals.len() + target.values.len()) as u64;
                budget.observe_live_cells(live_cells.saturating_add(incoming_cells))?;
                let argument_vectors = (arguments.len() as u64).saturating_mul(
                    (std::mem::size_of::<CtValue>() + std::mem::size_of::<u64>()) as u64,
                );
                budget.reserve_temporary_bytes(argument_vectors)?;
                let argument_bytes: Vec<_> = arguments
                    .iter()
                    .map(|v| copy_value_bytes(frame.value(*v), budget))
                    .collect::<CompileResult<_>>()?;
                for bytes in &argument_bytes {
                    budget.reserve_temporary_bytes(*bytes)?;
                }
                let args = arguments.iter().map(|v| frame.value(*v).clone()).collect();
                let incoming = Frame::new(target, args, argument_bytes, inst.result, budget)?;
                budget.release_temporary_bytes(argument_vectors);
                reserve_frame(&mut frames, &mut frame_vector_bytes, budget)?;
                frames.push(incoming);
                live_cells += incoming_cells;
                continue;
            }
            let bytes_before = budget.live_temporary_bytes;
            let query_capacity_before = budget.usage.queries.capacity();
            let produced = execute_instruction(raw, frame, inst, arguments, reader, budget)?;
            if let Some(id) = inst.result {
                let value = produced.expect("verified CT value instruction has result");
                if !value_matches_type(
                    &value,
                    &frame.function.values[id.0 as usize],
                    raw,
                    0,
                    budget,
                )? {
                    return Err(error("CT read view returned a value of the wrong type"));
                }
                let query_growth = (budget
                    .usage
                    .queries
                    .capacity()
                    .saturating_sub(query_capacity_before)
                    as u64)
                    .saturating_mul(std::mem::size_of::<CtQueryDependency>() as u64);
                let mut added = budget
                    .live_temporary_bytes
                    .saturating_sub(bytes_before)
                    .saturating_sub(query_growth);
                let required = value_bytes(&value);
                if added < required {
                    budget.reserve_temporary_bytes(required - added)?;
                    added = required;
                }
                let old = frame.value_bytes[id.0 as usize];
                budget.release_temporary_bytes(old);
                frame.value_bytes[id.0 as usize] = added;
                frame.values[id.0 as usize] = Some(value);
            }
        } else {
            match block.terminator {
                CtTerminator::Return(result) => {
                    let bytes = match result {
                        Some(id) => copy_value_bytes(frame.value(id), budget)?,
                        None => 0,
                    };
                    let final_root = if current_depth == 1 {
                        std::mem::size_of::<CtValue>() as u64
                    } else {
                        0
                    };
                    budget.reserve_temporary_bytes(bytes.saturating_add(final_root))?;
                    let output = result
                        .map(|id| frame.value(id).clone())
                        .unwrap_or(CtValue::Scalar(Constant::Unit));
                    let completed = frames.pop().expect("active CT frame");
                    live_cells -=
                        (completed.function.locals.len() + completed.function.values.len()) as u64;
                    let destination = completed.destination;
                    completed.release(budget);
                    if let Some(parent) = frames.last_mut() {
                        if let Some(id) = destination {
                            let old = parent.value_bytes[id.0 as usize];
                            budget.release_temporary_bytes(old);
                            parent.value_bytes[id.0 as usize] = bytes;
                            parent.values[id.0 as usize] = Some(output);
                        }
                    } else {
                        budget.release_temporary_bytes(frame_vector_bytes);
                        budget.reserve_temporary_bytes(
                            budget.usage.queries.len() as u64
                                * std::mem::size_of::<CtQueryDependency>() as u64,
                        )?;
                        return Ok((output, budget.usage.clone()));
                    }
                }
                CtTerminator::Jump(to) => {
                    frame.block = to.0 as usize;
                    frame.instruction = 0;
                }
                CtTerminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let CtValue::Scalar(Constant::Bool(flag)) = frame.value(condition) else {
                        unreachable!("verified CT branch condition")
                    };
                    frame.block = if *flag { then_block.0 } else { else_block.0 } as usize;
                    frame.instruction = 0;
                }
                CtTerminator::Trap => return Err(error("CT execution reached an explicit trap")),
            }
        }
    }
}

fn value_matches_type(
    value: &CtValue,
    ty: &CtTy,
    raw: &CtArtifactProgram,
    depth: usize,
    budget: &mut CtExecutionBudget,
) -> CompileResult<bool> {
    if depth > raw.budget.max_call_depth as usize {
        return Ok(false);
    }
    let fields_match = |fields: &[CtValue],
                        types: &[CtTy],
                        budget: &mut CtExecutionBudget|
     -> CompileResult<bool> {
        if fields.len() != types.len() {
            return Ok(false);
        }
        for (value, ty) in fields.iter().zip(types) {
            if !value_matches_type(value, ty, raw, depth + 1, budget)? {
                return Ok(false);
            }
        }
        Ok(true)
    };
    Ok(match (value, ty) {
        (CtValue::Scalar(c), CtTy::Scalar(t)) => {
            scalar_ty(c) == Some(*t)
                && !matches!(c, Constant::Usize(v) if raw.target_pointer_width == 32 && *v > u64::from(u32::MAX))
        }
        (CtValue::Text(v), CtTy::Text) => v.len() as u64 <= raw.budget.max_temporary_bytes,
        (CtValue::Type(v), CtTy::Type) => type_ref_matches(v, raw, budget)?,
        (CtValue::Ident(v), CtTy::Ident) => {
            !v.text.is_empty()
                && v.text.len() as u64 <= raw.budget.max_temporary_bytes
                && v.member_owner.is_some() == v.member_owner_type.is_some()
                && match v.source {
                    CtNameSource::Input(_) | CtNameSource::Definition(_) => {
                        v.namespace != CtNameNamespace::Unbound
                    }
                    CtNameSource::Fresh { .. } | CtNameSource::Export => {
                        v.namespace == CtNameNamespace::Unbound
                    }
                }
                && if let Some(owner) = &v.member_owner_type {
                    type_ref_matches(owner, raw, budget)?
                } else {
                    true
                }
        }
        (CtValue::TokenTree(v), CtTy::TokenTree) => token_tree_valid(v, raw),
        (CtValue::TokenCursor(_), CtTy::TokenCursor)
        | (CtValue::ShapeInfo(_), CtTy::ShapeInfo)
        | (CtValue::FieldInfo(_), CtTy::FieldInfo)
        | (CtValue::ImplInfo(_), CtTy::ImplInfo)
        | (CtValue::LayoutInfo(_), CtTy::LayoutInfo) => true,
        (CtValue::Syntax(tree), CtTy::Syntax(category)) => {
            tree.category == *category && syntax_tree_valid(tree, raw)
        }
        (CtValue::Tuple(v), CtTy::Tuple(t)) => fields_match(v, t, budget)?,
        (CtValue::Array(v), CtTy::Array { element, length }) => {
            if v.len() != *length as usize {
                return Ok(false);
            }
            for value in v {
                if !value_matches_type(value, element, raw, depth + 1, budget)? {
                    return Ok(false);
                }
            }
            true
        }
        (CtValue::List(v), CtTy::List(element)) => {
            if v.len() as u64 > raw.budget.max_generated_nodes {
                return Ok(false);
            }
            for value in v {
                if !value_matches_type(value, element, raw, depth + 1, budget)? {
                    return Ok(false);
                }
            }
            true
        }
        (CtValue::Record { schema, fields }, CtTy::Record(id)) => {
            if schema != id {
                return Ok(false);
            }
            if let Some(schema) = raw.record_schemas.get(id.0 as usize) {
                fields_match(fields, &schema.fields, budget)?
            } else {
                false
            }
        }
        (
            CtValue::AnonymousSum {
                members,
                active,
                payload,
            },
            CtTy::AnonymousSum(expected),
        ) => {
            let actual_key = crate::artifact_sum_members_key(
                members,
                &raw.record_schemas,
                &raw.enum_schemas,
                budget,
            )?;
            let expected_key = crate::artifact_sum_members_key(
                expected,
                &raw.record_schemas,
                &raw.enum_schemas,
                budget,
            )?;
            budget.reserve_analysis_work(
                (actual_key.len() as u64).saturating_add(expected_key.len() as u64),
            )?;
            let same = actual_key == expected_key;
            budget.release_temporary_bytes(
                (actual_key.capacity() as u64).saturating_add(expected_key.capacity() as u64),
            );
            if !same {
                false
            } else if let Some(member) = expected.get(*active as usize) {
                value_matches_type(payload, member, raw, depth + 1, budget)?
            } else {
                false
            }
        }
        (
            CtValue::Variant {
                schema,
                tag,
                fields,
            },
            CtTy::Enum(id),
        ) => {
            if schema != id {
                return Ok(false);
            }
            if let Some(fields_type) = raw
                .enum_schemas
                .get(id.0 as usize)
                .and_then(|s| s.variants.get(*tag as usize))
            {
                fields_match(fields, fields_type, budget)?
            } else {
                false
            }
        }
        _ => false,
    })
}

// Detect this new profile during the existing structural validation walk.
// Old Type values acquire neither a second classifier traversal nor a new fee.
fn type_ref_matches(
    reference: &CtTypeRef,
    raw: &CtArtifactProgram,
    budget: &mut CtExecutionBudget,
) -> CompileResult<bool> {
    if !reference.ambient_classes.is_empty()
        || matches!(
            reference.nodes.get(reference.root.0 as usize),
            Some(CtTypeNode::BorrowedStorage { .. })
        )
    {
        // New contextual values can only reuse complete graphs independently
        // admitted at actual source Type sites in this verified program.
        for instruction in raw
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
        {
            budget.reserve_analysis_work(1)?;
            if let CtInstructionKind::TypeInContext { ty } = &instruction.kind {
                let cost = storage::graph(reference, Some(budget))?
                    .saturating_add(storage::graph(ty, Some(budget))?);
                budget.reserve_analysis_work(cost)?;
                if ty == reference {
                    let limits = zil_base::Limits {
                        max_nodes: raw.budget.max_generated_nodes as usize,
                        max_source_bytes: raw.budget.max_temporary_bytes as usize,
                        max_call_depth: raw.budget.max_call_depth as usize,
                        max_analysis_steps: budget.remaining().max_analysis_work as usize,
                        ..zil_base::Limits::default()
                    };
                    let mut meter =
                        zil_verify::CtVerificationMeter::new(budget.remaining().max_analysis_work);
                    let checked = zil_verify::verify_ct_type_graph_metered(
                        reference, raw, &limits, &mut meter,
                    );
                    budget.reserve_analysis_work(meter.spent())?;
                    checked?;
                    return Ok(true);
                }
            }
        }
        return Ok(false);
    }
    let mut anonymous = false;
    if !type_ref_valid(reference, raw, &mut anonymous) {
        return Ok(false);
    }
    if anonymous {
        let canonical = crate::canonicalize_anonymous_type(reference, budget)?;
        let comparison_work = canonical.nodes.iter().fold(0u64, |work, node| {
            work.saturating_add(128).saturating_add(match node {
                CtTypeNode::Nominal { arguments, .. } => {
                    (arguments.len() as u64).saturating_mul(64)
                }
                CtTypeNode::Tuple(v) | CtTypeNode::AnonymousSum(v) => {
                    (v.len() as u64).saturating_mul(8)
                }
                CtTypeNode::FunctionPointer { parameters, .. }
                | CtTypeNode::ForeignCallback { parameters, .. } => {
                    (parameters.len() as u64).saturating_mul(8)
                }
                _ => 0,
            })
        });
        budget.reserve_analysis_work(comparison_work)?;
        let valid = canonical == *reference;
        budget.release_temporary_bytes(
            16u64.saturating_add((canonical.nodes.len() as u64).saturating_mul(64)),
        );
        if !valid {
            return Err(error("anonymous Type value is not structurally canonical"));
        }
    }
    Ok(true)
}

fn type_ref_valid(reference: &CtTypeRef, raw: &CtArtifactProgram, anonymous: &mut bool) -> bool {
    if reference.nodes.len() as u64 > raw.budget.max_generated_nodes
        || reference.root.0 as usize >= reference.nodes.len()
        || !reference.ambient_classes.is_empty()
    {
        return false;
    }
    for (index, node) in reference.nodes.iter().enumerate() {
        let child = |id: CtTypeNodeId| (id.0 as usize) < index;
        let good = match node {
            CtTypeNode::BorrowedStorage { .. } => false,
            CtTypeNode::Primitive(_) | CtTypeNode::Builtin(_) | CtTypeNode::Opaque { .. } => true,
            CtTypeNode::Nominal { arguments, .. } => arguments.iter().all(|a| match a {
                CtTypeArgument::Type(id) => child(*id),
                CtTypeArgument::Scalar(c) => scalar_ty(c).is_some(),
                CtTypeArgument::Lifetime(_) => false,
            }),
            CtTypeNode::Tuple(v) => v.iter().all(|id| child(*id)),
            CtTypeNode::AnonymousSum(v) => {
                *anonymous = true;
                v.len() >= 2 && v.iter().all(|id| child(*id))
            }
            CtTypeNode::Array { element, .. }
            | CtTypeNode::Slice(element)
            | CtTypeNode::Managed(element)
            | CtTypeNode::Desc(element)
            | CtTypeNode::OwnedStore(element) => child(*element),
            CtTypeNode::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                let Some(life) = life.to_flat(parameters.len()) else {
                    return false;
                };
                let mut dense = 0;
                let inputs = parameters.len() == life.parameter_lives.len()
                    && parameters
                        .iter()
                        .zip(&life.parameter_lives)
                        .all(|(id, label)| {
                            if !child(*id) {
                                return false;
                            }
                            if matches!(reference.nodes[id.0 as usize], CtTypeNode::Address { .. })
                            {
                                let Some(tag) =
                                    label.filter(|tag| *tag < life.binder_count && *tag <= dense)
                                else {
                                    return false;
                                };
                                if tag == dense {
                                    dense += 1;
                                }
                                true
                            } else {
                                label.is_none()
                            }
                        });
                inputs && child(*result) && dense==life.binder_count && dense>0 && match &reference.nodes[result.0 as usize]{
                    CtTypeNode::Address{mutable,..}=>life.result_life.is_some_and(|tag|tag<dense&&parameters.iter().zip(&life.parameter_lives).any(|(id,label)|*label==Some(tag)&&matches!(reference.nodes[id.0 as usize],CtTypeNode::Address{mutable:authority,..}if !mutable||authority))),
                    _=>life.result_life.is_none(),
                }
            }
            CtTypeNode::FunctionPointer { parameters, result }
            | CtTypeNode::ForeignCallback { parameters, result } => {
                child(*result) && parameters.iter().all(|id| child(*id))
            }
            CtTypeNode::Address { inner, .. } => child(*inner),
            CtTypeNode::ErrorUnion { error, result } => child(*error) && child(*result),
        };
        if !good {
            return false;
        }
    }
    true
}
fn origins_valid(origins: &[CtOrigin], max: u64) -> bool {
    origins.len() as u64 <= max
        && origins
            .iter()
            .enumerate()
            .all(|(i, o)| o.parent.is_none_or(|p| (p.0 as usize) < i))
}
fn token_tree_valid(tree: &CtTokenTree, raw: &CtArtifactProgram) -> bool {
    if tree.nodes.len() as u64 > raw.budget.max_generated_nodes
        || !origins_valid(&tree.origins, raw.budget.max_generated_nodes)
        || tree.roots.iter().any(|r| r.0 as usize >= tree.nodes.len())
    {
        return false;
    }
    tree.nodes.iter().enumerate().all(|(i, n)| match n {
        CtTokenNode::Token(t) => {
            (t.origin.0 as usize) < tree.origins.len()
                && (t.text.len() as u64) <= raw.budget.max_temporary_bytes
        }
        CtTokenNode::Group {
            children, origin, ..
        } => {
            (origin.0 as usize) < tree.origins.len() && children.iter().all(|c| (c.0 as usize) < i)
        }
    })
}
fn syntax_tree_valid(tree: &CtSyntaxTree, raw: &CtArtifactProgram) -> bool {
    if tree.nodes.len() as u64 > raw.budget.max_generated_nodes
        || !origins_valid(&tree.origins, raw.budget.max_generated_nodes)
        || tree.roots.iter().any(|r| r.0 as usize >= tree.nodes.len())
    {
        return false;
    }
    tree.nodes.iter().enumerate().all(|(i, n)| match n {
        CtSyntaxNode::Token(t) => {
            (t.origin.0 as usize) < tree.origins.len()
                && (t.text.len() as u64) <= raw.budget.max_temporary_bytes
        }
        CtSyntaxNode::Group {
            children, origin, ..
        } => {
            (origin.0 as usize) < tree.origins.len() && children.iter().all(|c| (c.0 as usize) < i)
        }
        CtSyntaxNode::Hole { .. } => false,
    })
}

fn int(value: &CtValue) -> i128 {
    match value {
        CtValue::Scalar(Constant::U8(v)) => i128::from(*v),
        CtValue::Scalar(Constant::U16(v)) => i128::from(*v),
        CtValue::Scalar(Constant::I32(v)) => i128::from(*v),
        CtValue::Scalar(Constant::I64(v)) => i128::from(*v),
        CtValue::Scalar(Constant::U64(v) | Constant::Usize(v)) => i128::from(*v),
        _ => unreachable!("verified integer"),
    }
}
fn text(value: &CtValue) -> &str {
    let CtValue::Text(v) = value else {
        unreachable!("verified text")
    };
    v
}
fn append_type(out: &mut Vec<CtTypeNode>, source: &CtTypeRef) -> CompileResult<CtTypeNodeId> {
    if !source.ambient_classes.is_empty() {
        return Err(vec![Diagnostic::unsupported(
            Phase::Ct,
            "authenticated CT5 ambient graph relocation is not yet implemented",
            None,
        )]);
    }
    let offset = u32::try_from(out.len()).map_err(|_| limit("CT type node count overflow"))?;
    for node in &source.nodes {
        let plus = |id: CtTypeNodeId| -> CompileResult<CtTypeNodeId> {
            Ok(CtTypeNodeId(
                id.0.checked_add(offset)
                    .ok_or_else(|| limit("CT type node ID overflow"))?,
            ))
        };
        let mapped = crate::anonymous_type::remap(node, plus)?;
        out.push(mapped);
    }
    Ok(CtTypeNodeId(
        source
            .root
            .0
            .checked_add(offset)
            .ok_or_else(|| limit("CT type root overflow"))?,
    ))
}
fn construct_type(
    kind: &CtTypeConstructor,
    args: &[CtValue],
    budget: &mut CtExecutionBudget,
) -> CompileResult<CtTypeRef> {
    budget.reserve_analysis_work(args.len() as u64)?;
    let mut output_nodes = 1u64;
    let mut output_bytes = std::mem::size_of::<CtTypeNode>() as u64;
    let mut child_count = 0usize;
    for arg in args {
        if let CtValue::Type(graph) = arg {
            output_nodes = output_nodes
                .checked_add(graph.nodes.len() as u64)
                .ok_or_else(|| limit("CT type output node overflow"))?;
            output_bytes = output_bytes.saturating_add(storage::graph(graph, Some(budget))?);
            child_count = child_count.saturating_add(1);
        }
    }
    output_bytes = output_bytes
        .saturating_add(child_count as u64 * std::mem::size_of::<CtTypeNodeId>() as u64);
    budget.reserve_generated_nodes(output_nodes)?;
    budget.reserve_temporary_bytes(output_bytes)?;
    let mut nodes = Vec::with_capacity(
        usize::try_from(output_nodes).map_err(|_| limit("CT type output node overflow"))?,
    );
    let mut children = Vec::with_capacity(child_count);
    for arg in args {
        if let CtValue::Type(value) = arg {
            children.push(append_type(&mut nodes, value)?);
        }
    }
    let last = u32::try_from(nodes.len()).map_err(|_| limit("CT type node count overflow"))?;
    let node = match kind {
        CtTypeConstructor::BorrowedStorage { .. } => {
            return Err(error("stored-borrow CT constructor is not yet implemented"));
        }
        CtTypeConstructor::Array => {
            let length = u32::try_from(int(&args[1]))
                .map_err(|_| error("CT array type length is outside u32"))?;
            CtTypeNode::Array {
                element: children[0],
                length,
            }
        }
        CtTypeConstructor::Tuple => CtTypeNode::Tuple(children),
        CtTypeConstructor::AnonymousSum => CtTypeNode::AnonymousSum(children),
        CtTypeConstructor::BorrowedFunctionPointer { life } => {
            crate::anonymous_type::flat_life(life, children.len().saturating_sub(1), budget)?;
            budget.reserve_temporary_bytes(crate::anonymous_type::life_bytes(life))?;
            let result = children.pop().expect("verified borrowed pointer result");
            CtTypeNode::BorrowedFunctionPointer {
                parameters: children,
                result,
                life: life.clone(),
            }
        }
        CtTypeConstructor::FunctionPointer => {
            let result = children.pop().expect("verified function Type result");
            let address = |id: &CtTypeNodeId| {
                matches!(nodes.get(id.0 as usize), Some(CtTypeNode::Address { .. }))
            };
            if children.iter().any(address) || address(&result) {
                budget.reserve_analysis_work(children.len() as u64)?;
                let flat_bytes = (children.len() as u64)
                    .saturating_mul(std::mem::size_of::<Option<u32>>() as u64);
                let rich_bytes = (children.len() as u64 + 1)
                    .saturating_mul(std::mem::size_of::<zil_ir::StorageLifeBinding>() as u64);
                budget.reserve_temporary_bytes(flat_bytes.saturating_add(rich_bytes))?;
                let life = zil_ir::FunctionLifeContract {
                    binder_count: 1,
                    parameter_lives: children.iter().map(|id| address(id).then_some(0)).collect(),
                    result_life: address(&result).then_some(0),
                };
                let life =
                    zil_ir::StorageLifeContract::from_flat(life).expect("bounded legacy signature");
                budget.release_temporary_bytes(flat_bytes);
                CtTypeNode::BorrowedFunctionPointer {
                    parameters: children,
                    result,
                    life,
                }
            } else {
                CtTypeNode::FunctionPointer {
                    parameters: children,
                    result,
                }
            }
        }
        CtTypeConstructor::Address { mutable } => CtTypeNode::Address {
            mutable: *mutable,
            inner: children[0],
        },
        CtTypeConstructor::Slice => CtTypeNode::Slice(children[0]),
        CtTypeConstructor::Managed => CtTypeNode::Managed(children[0]),
        CtTypeConstructor::Desc => CtTypeNode::Desc(children[0]),
        CtTypeConstructor::OwnedStore => CtTypeNode::OwnedStore(children[0]),
        CtTypeConstructor::ErrorUnion => CtTypeNode::ErrorUnion {
            error: children[0],
            result: children[1],
        },
    };
    nodes.push(node);
    let value = CtTypeRef {
        nodes,
        root: CtTypeNodeId(last),
        ambient_classes: Vec::new(),
    };
    if *kind == CtTypeConstructor::AnonymousSum {
        let canonical = crate::canonicalize_anonymous_type(&value, budget)?;
        budget.release_temporary_bytes(output_bytes);
        Ok(canonical)
    } else {
        Ok(value)
    }
}
fn project_meta(
    value: &CtValue,
    projection: CtMetaProjection,
    requester: &CtSymbolId,
    reader: &impl CtReadView,
    budget: &mut CtExecutionBudget,
) -> CompileResult<CtValue> {
    use CtMetaProjection as P;
    let allocation = match (value, projection) {
        (CtValue::ShapeInfo(s), P::ShapeFields) => {
            budget.reserve_analysis_work(s.visible_fields.len() as u64)?;
            let mut bytes = s.visible_fields.len() as u64 * std::mem::size_of::<CtValue>() as u64;
            for field in &s.visible_fields {
                bytes = bytes.saturating_add(storage::field(field, &mut Some(budget))?);
            }
            bytes
        }
        (CtValue::FieldInfo(f), P::FieldName) => storage::ident(&f.name, &mut Some(budget))?
            .saturating_add(storage::graph(&f.owner_type, Some(budget))?),
        (CtValue::FieldInfo(f), P::FieldType) => storage::graph(&f.ty, Some(budget))?,
        (CtValue::ImplInfo(i), P::ImplType) => storage::graph(&i.for_type, Some(budget))?,
        (CtValue::LayoutInfo(l), P::LayoutFieldOffsets) => {
            budget.reserve_analysis_work(l.field_offsets.len() as u64)?;
            l.field_offsets.len() as u64 * std::mem::size_of::<CtValue>() as u64
        }
        _ => 0,
    };
    if allocation > 0 {
        budget.reserve_temporary_bytes(allocation)?;
    }
    match (value, projection) {
        (CtValue::ShapeInfo(s), P::ShapeFields) => Ok(CtValue::List(
            s.visible_fields
                .iter()
                .cloned()
                .map(CtValue::FieldInfo)
                .collect(),
        )),
        (CtValue::ShapeInfo(s), P::ShapeComplete) => {
            Ok(CtValue::Scalar(Constant::Bool(s.complete)))
        }
        (CtValue::FieldInfo(f), P::FieldName) => {
            let mut name = f.name.clone();
            name.namespace = CtNameNamespace::Member;
            name.member_owner = Some(f.owner.clone());
            name.member_owner_type = Some(f.owner_type.clone());
            Ok(CtValue::Ident(name))
        }
        (CtValue::FieldInfo(f), P::FieldType) => Ok(CtValue::Type(f.ty.clone())),
        (CtValue::FieldInfo(f), P::FieldManaged) => Ok(CtValue::Scalar(Constant::Bool(f.managed))),
        (CtValue::ImplInfo(i), P::ImplTrait) => Ok(CtValue::Ident(reader.definition_ident(
            requester,
            &i.trait_symbol,
            budget,
        )?)),
        (CtValue::ImplInfo(i), P::ImplType) => Ok(CtValue::Type(i.for_type.clone())),
        (CtValue::LayoutInfo(l), P::LayoutSize) => Ok(CtValue::Scalar(Constant::I64(
            i64::try_from(l.size).map_err(|_| error("CT layout size exceeds target i64"))?,
        ))),
        (CtValue::LayoutInfo(l), P::LayoutAlignment) => Ok(CtValue::Scalar(Constant::I64(
            i64::try_from(l.alignment)
                .map_err(|_| error("CT layout alignment exceeds target i64"))?,
        ))),
        (CtValue::LayoutInfo(l), P::LayoutFieldOffsets) => Ok(CtValue::List(
            l.field_offsets
                .iter()
                .map(|v| {
                    i64::try_from(*v)
                        .map(|n| CtValue::Scalar(Constant::I64(n)))
                        .map_err(|_| error("CT layout offset exceeds target i64"))
                })
                .collect::<CompileResult<Vec<_>>>()?,
        )),
        _ => unreachable!("verified CT metadata projection"),
    }
}

fn execute_instruction(
    raw: &CtArtifactProgram,
    frame: &mut Frame<'_>,
    inst: &CtInstruction,
    invocation: &[CtValue],
    reader: &impl CtReadView,
    budget: &mut CtExecutionBudget,
) -> CompileResult<Option<CtValue>> {
    use CtInstructionKind as K;
    let get = |id: CtValueId| frame.value(id);
    fn copy_bytes(
        frame: &Frame<'_>,
        ids: &[CtValueId],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<u64> {
        budget.reserve_analysis_work(ids.len() as u64)?;
        let mut bytes = (ids.len() as u64).saturating_mul(std::mem::size_of::<CtValue>() as u64);
        for id in ids {
            bytes = bytes.saturating_add(copy_value_bytes(frame.value(*id), budget)?);
        }
        Ok(bytes)
    }
    let planned = match &inst.kind {
        K::Constant(id) => match &raw.constants[id.0 as usize] {
            CtConstant::Scalar(_) => 0,
            CtConstant::Text(t) => {
                budget.reserve_analysis_work(t.len() as u64)?;
                t.len() as u64
            }
        },
        K::InvocationInput { index } => copy_value_bytes(&invocation[*index as usize], budget)?,
        K::Load(id) => copy_value_bytes(
            frame.locals[id.0 as usize]
                .as_ref()
                .expect("verified initialized local"),
            budget,
        )?,
        K::ConvertInteger { .. }
        | K::ConvertNumber { .. }
        | K::Unary { .. }
        | K::Binary { .. }
        | K::Length(_)
        | K::VariantTag(_) => 0,
        K::MakeTuple(ids) | K::MakeArray(ids) | K::MakeList(ids) => copy_bytes(frame, ids, budget)?,
        K::MakeRecord { fields, .. } | K::MakeVariant { fields, .. } => {
            copy_bytes(frame, fields, budget)?
        }
        K::TypePrimitive(_) | K::TypeBuiltin(_) => std::mem::size_of::<CtTypeNode>() as u64,
        K::TypeInContext { ty } => {
            budget.reserve_generated_nodes(ty.nodes.len() as u64)?;
            storage::graph(ty, Some(budget))?
        }
        K::MakeAnonymousSum {
            members, payload, ..
        } => {
            budget.reserve_analysis_work(members.len() as u64)?;
            let mut bytes = (members.len() as u64)
                .saturating_mul(std::mem::size_of::<CtTy>() as u64)
                .saturating_add(std::mem::size_of::<CtValue>() as u64);
            for member in members {
                bytes = bytes.saturating_add(type_storage(member, Some(budget))?);
            }
            bytes.saturating_add(copy_value_bytes(get(*payload), budget)?)
        }
        K::VariantField {
            value,
            variant,
            field,
        } => match get(*value) {
            CtValue::Variant { tag, fields, .. } if tag == variant => {
                copy_value_bytes(&fields[*field as usize], budget)?
            }
            CtValue::AnonymousSum {
                active, payload, ..
            } if active == variant && *field == 0 => copy_value_bytes(payload, budget)?,
            CtValue::Variant { .. } | CtValue::AnonymousSum { .. } => {
                return Err(error("CT inactive variant projection reached"));
            }
            _ => unreachable!("verified enum"),
        },
        K::Extract { aggregate, index } => match get(*aggregate) {
            CtValue::Array(fields) | CtValue::List(fields) => {
                copy_value_bytes(&fields[integer_index(get(*index), fields.len())?], budget)?
            }
            _ => unreachable!("verified homogeneous container"),
        },
        K::ExtractField { aggregate, index } => match get(*aggregate) {
            CtValue::Tuple(fields) | CtValue::Record { fields, .. } => {
                copy_value_bytes(&fields[*index as usize], budget)?
            }
            _ => unreachable!("verified static container"),
        },
        K::Replace {
            aggregate, value, ..
        }
        | K::ReplaceField {
            aggregate, value, ..
        } => copy_value_bytes(get(*aggregate), budget)?
            .saturating_add(copy_value_bytes(get(*value), budget)?),
        _ => 0,
    };
    if planned > 0 {
        budget.reserve_temporary_bytes(planned)?;
    }
    let output = match &inst.kind {
        K::Constant(id) => match &raw.constants[id.0 as usize] {
            CtConstant::Scalar(c) => Some(CtValue::Scalar(c.clone())),
            CtConstant::Text(t) => Some(CtValue::Text(t.clone())),
        },
        K::InvocationInput { index } => Some(invocation[*index as usize].clone()),
        K::StorageLive(id) | K::StorageDead(id) => {
            budget.release_temporary_bytes(frame.local_bytes[id.0 as usize]);
            frame.local_bytes[id.0 as usize] = 0;
            frame.locals[id.0 as usize] = None;
            None
        }
        K::Load(id) => Some(
            frame.locals[id.0 as usize]
                .as_ref()
                .expect("verified initialized local")
                .clone(),
        ),
        K::Store { local, value } => {
            let bytes = copy_value_bytes(get(*value), budget)?;
            budget.reserve_temporary_bytes(bytes)?;
            let copied = get(*value).clone();
            budget.release_temporary_bytes(frame.local_bytes[local.0 as usize]);
            frame.local_bytes[local.0 as usize] = bytes;
            frame.locals[local.0 as usize] = Some(copied);
            None
        }
        K::ConvertInteger { value, target } => {
            let CtValue::Scalar(c) = get(*value) else {
                unreachable!("verified scalar conversion")
            };
            Some(CtValue::Scalar(crate::convert_integer_target(
                c,
                &ordinary_scalar(*target),
                raw.target_pointer_width,
            )?))
        }
        K::ConvertNumber { value, target } => {
            let CtValue::Scalar(c) = get(*value) else {
                unreachable!("verified scalar conversion")
            };
            Some(CtValue::Scalar(crate::convert_number(
                c,
                &ordinary_scalar(*target),
                raw.target_pointer_width,
            )?))
        }
        K::Unary { op, operand } => {
            let CtValue::Scalar(c) = get(*operand) else {
                unreachable!("verified scalar unary")
            };
            Some(CtValue::Scalar(crate::unary_target(
                *op,
                c,
                raw.target_pointer_width,
            )?))
        }
        K::Binary { op, left, right } => {
            let a = get(*left);
            let b = get(*right);
            let result = match (a, b) {
                (CtValue::Scalar(a), CtValue::Scalar(b)) => {
                    crate::binary_target(*op, a, b, raw.target_pointer_width)?
                }
                (CtValue::Type(_), CtValue::Type(_))
                | (CtValue::Ident(_), CtValue::Ident(_))
                | (CtValue::Text(_), CtValue::Text(_)) => {
                    let equal = a == b;
                    match op {
                        zil_ir::BinaryOp::Equal => Constant::Bool(equal),
                        zil_ir::BinaryOp::NotEqual => Constant::Bool(!equal),
                        _ => unreachable!("verified CT metadata comparison"),
                    }
                }
                _ => unreachable!("verified CT binary types"),
            };
            Some(CtValue::Scalar(result))
        }
        K::MakeTuple(ids) => Some(CtValue::Tuple(
            ids.iter().map(|id| get(*id).clone()).collect(),
        )),
        K::MakeArray(ids) => Some(CtValue::Array(
            ids.iter().map(|id| get(*id).clone()).collect(),
        )),
        K::MakeList(ids) => Some(CtValue::List(
            ids.iter().map(|id| get(*id).clone()).collect(),
        )),
        K::MakeRecord { schema, fields } => Some(CtValue::Record {
            schema: *schema,
            fields: fields.iter().map(|id| get(*id).clone()).collect(),
        }),
        K::MakeVariant {
            schema,
            tag,
            fields,
        } => Some(CtValue::Variant {
            schema: *schema,
            tag: *tag,
            fields: fields.iter().map(|id| get(*id).clone()).collect(),
        }),
        K::MakeAnonymousSum {
            members,
            active,
            payload,
        } => Some(CtValue::AnonymousSum {
            members: members.clone(),
            active: *active,
            payload: Box::new(get(*payload).clone()),
        }),
        K::Length(id) => {
            let len = match get(*id) {
                CtValue::Array(v) | CtValue::List(v) => v.len(),
                CtValue::Text(v) => v.len(),
                CtValue::TokenTree(v) => v.roots.len(),
                _ => unreachable!("verified length operand"),
            };
            Some(CtValue::Scalar(Constant::I64(
                i64::try_from(len).map_err(|_| limit("CT container length exceeds i64"))?,
            )))
        }
        K::VariantTag(id) => {
            let tag = match get(*id) {
                CtValue::Variant { tag, .. } => *tag,
                CtValue::AnonymousSum { active, .. } => *active,
                _ => unreachable!("verified enum or anonymous sum"),
            };
            Some(CtValue::Scalar(Constant::I32(
                i32::try_from(tag).map_err(|_| limit("CT variant tag exceeds i32"))?,
            )))
        }
        K::VariantField {
            value,
            variant,
            field,
        } => match get(*value) {
            CtValue::Variant { tag, fields, .. } if tag == variant => {
                Some(fields[*field as usize].clone())
            }
            CtValue::AnonymousSum {
                active, payload, ..
            } if active == variant && *field == 0 => Some(payload.as_ref().clone()),
            _ => return Err(error("CT inactive variant projection reached")),
        },
        K::Extract { aggregate, index } => {
            let fields = match get(*aggregate) {
                CtValue::Array(v) | CtValue::List(v) => v,
                _ => unreachable!("verified homogeneous container"),
            };
            Some(fields[integer_index(get(*index), fields.len())?].clone())
        }
        K::ExtractField { aggregate, index } => {
            let at = *index as usize;
            Some(match get(*aggregate) {
                CtValue::Tuple(fields) => fields[at].clone(),
                CtValue::Record { fields, .. } => fields[at].clone(),
                _ => unreachable!("verified static projection"),
            })
        }
        K::Replace {
            aggregate,
            index,
            value,
        } => {
            let mut fields = match get(*aggregate) {
                CtValue::Array(v) | CtValue::List(v) => v.clone(),
                _ => unreachable!("verified homogeneous container"),
            };
            let at = integer_index(get(*index), fields.len())?;
            fields[at] = get(*value).clone();
            Some(if matches!(get(*aggregate), CtValue::Array(_)) {
                CtValue::Array(fields)
            } else {
                CtValue::List(fields)
            })
        }
        K::ReplaceField {
            aggregate,
            index,
            value,
        } => {
            let at = *index as usize;
            Some(match get(*aggregate) {
                CtValue::Tuple(fields) => {
                    let mut out = fields.clone();
                    out[at] = get(*value).clone();
                    CtValue::Tuple(out)
                }
                CtValue::Record { schema, fields } => {
                    let mut out = fields.clone();
                    out[at] = get(*value).clone();
                    CtValue::Record {
                        schema: *schema,
                        fields: out,
                    }
                }
                _ => unreachable!("verified static replacement"),
            })
        }
        K::Call { .. } => unreachable!("CT calls handled by explicit stack"),
        K::TypePrimitive(s) => Some(CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Primitive(*s)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
        })),
        K::TypeBuiltin(b) => Some(CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Builtin(*b)],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
        })),
        K::TypeInContext { ty } => Some(CtValue::Type(ty.clone())),
        K::TypeName(symbol) => Some(CtValue::Type(reader.type_name(
            &frame.function.definition,
            symbol,
            budget,
        )?)),
        K::TypeApply { nominal, arguments } => {
            let (args, temporary) = clone_operands(frame, arguments, budget)?;
            let answer = reader.type_apply(&frame.function.definition, nominal, &args, budget);
            budget.release_temporary_bytes(temporary);
            Some(CtValue::Type(answer?))
        }
        K::TypeConstruct {
            constructor,
            arguments,
        } => {
            let (args, temporary) = clone_operands(frame, arguments, budget)?;
            let answer = construct_type(constructor, &args, budget);
            budget.release_temporary_bytes(temporary);
            Some(CtValue::Type(answer?))
        }
        K::Quote { template, holes } => {
            let (args, temporary) = clone_operands(frame, holes, budget)?;
            let template = &raw.templates[template.0 as usize];
            for (hole, value) in template.holes.iter().zip(&args) {
                if let CtHoleKind::Ident(namespace) = hole {
                    let CtValue::Ident(name) = value else {
                        unreachable!("verified Ident hole")
                    };
                    if name.namespace == CtNameNamespace::Unbound {
                        match name.source {
                            CtNameSource::Fresh { .. } => {}
                            CtNameSource::Export
                                if matches!(
                                    template.syntax.category,
                                    CtSyntaxCategory::StmtList | CtSyntaxCategory::ItemList
                                ) => {}
                            _ => {
                                return Err(error(
                                    "CT unbound name intent is not valid in this quote",
                                ));
                            }
                        }
                    } else if name.namespace != *namespace {
                        return Err(error("CT quote name hole has wrong namespace"));
                    }
                }
            }
            let answer = reader.instantiate_quote(template, &args, budget);
            budget.release_temporary_bytes(temporary);
            Some(CtValue::Syntax(answer?))
        }
        K::Parse { category, input } => {
            let CtValue::TokenTree(tree) = get(*input) else {
                unreachable!("verified TokenTree")
            };
            Some(CtValue::Syntax(reader.parse(*category, tree, budget)?))
        }
        K::ParseIdent { input } => {
            let CtValue::TokenTree(tree) = get(*input) else {
                unreachable!("verified TokenTree")
            };
            Some(CtValue::Ident(reader.parse_ident(tree, budget)?))
        }
        K::TokenInspect {
            source,
            projection,
            index,
        } => {
            let number = index
                .map(|id| {
                    i64::try_from(int(get(id))).map_err(|_| error("CT token index exceeds i64"))
                })
                .transpose()?;
            Some(reader.token_inspect(get(*source), *projection, number, budget)?)
        }
        K::Text { op, arguments } => {
            let a = text(get(arguments[0]));
            use CtTextOp as T;
            Some(match op {
                T::Equal => CtValue::Scalar(Constant::Bool(a == text(get(arguments[1])))),
                T::Length => CtValue::Scalar(Constant::I64(
                    i64::try_from(a.len()).map_err(|_| limit("CT text length exceeds i64"))?,
                )),
                T::Slice => {
                    let start = usize::try_from(int(get(arguments[1])))
                        .map_err(|_| error("CT text slice start is negative"))?;
                    let end = usize::try_from(int(get(arguments[2])))
                        .map_err(|_| error("CT text slice end is negative"))?;
                    let fragment = a
                        .get(start..end)
                        .ok_or_else(|| error("CT text slice cuts UTF-8 or is out of range"))?;
                    budget.reserve_temporary_bytes(16u64.saturating_add(fragment.len() as u64))?;
                    CtValue::Text(fragment.to_owned())
                }
                T::Concatenate => {
                    let b = text(get(arguments[1]));
                    budget.reserve_temporary_bytes(
                        16u64.saturating_add(a.len().saturating_add(b.len()) as u64),
                    )?;
                    CtValue::Text(format!("{a}{b}"))
                }
            })
        }
        K::SyntaxBuild { kind, arguments } => {
            let (mut args, temporary) = clone_operands(frame, arguments, budget)?;
            if *kind == CtSyntaxBuilder::NameExpr {
                let CtValue::Ident(name) = &mut args[0] else {
                    unreachable!("verified name_expr Ident")
                };
                match (&name.source, name.namespace) {
                    (CtNameSource::Fresh { .. }, CtNameNamespace::Unbound) => {
                        name.namespace = CtNameNamespace::Value
                    }
                    (
                        CtNameSource::Input(_) | CtNameSource::Definition(_),
                        CtNameNamespace::Value,
                    ) => {}
                    _ => return Err(error("CT name_expr needs a value name, not export intent")),
                }
            }
            let answer = reader.build_syntax(*kind, &args, budget);
            budget.release_temporary_bytes(temporary);
            Some(CtValue::Syntax(answer?))
        }
        K::NameBuild { kind, text: source } => {
            let label = text(get(*source));
            budget.reserve_generated_nodes(1)?;
            budget.reserve_temporary_bytes(80u64.saturating_add(label.len() as u64))?;
            let ordinal = u32::try_from(budget.usage.generated_nodes)
                .map_err(|_| limit("CT fresh-name ordinal overflow"))?;
            let source = match kind {
                CtNameBuilder::Fresh => CtNameSource::Fresh {
                    invocation: raw.input_tokens.invocation_fingerprint,
                    binder: ordinal,
                },
                CtNameBuilder::Export => CtNameSource::Export,
            };
            Some(CtValue::Ident(CtIdent {
                text: label.to_owned(),
                namespace: CtNameNamespace::Unbound,
                source,
                member_owner: None,
                member_owner_type: None,
            }))
        }
        K::Query { kind, arguments } => {
            let (args, temporary) = clone_operands(frame, arguments, budget)?;
            let result = reader.query(&frame.function.definition, *kind, &args, budget);
            budget.release_temporary_bytes(temporary);
            let (answer, dependency) = result?;
            budget.record_query(dependency)?;
            Some(answer)
        }
        K::MetaProject { value, projection } => Some(project_meta(
            get(*value),
            *projection,
            &frame.function.definition,
            reader,
            budget,
        )?),
    };
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{Fingerprint, Limits, ModuleId, NodeId, Span};
    use zil_verify::{CtArtifactAuthority, CtSymbolUsage, verify_ct_artifact};

    fn symbol() -> CtSymbolId {
        CtSymbolId {
            module: ModuleId(0),
            declaration: NodeId(1),
            generation_path: None,
        }
    }
    fn snapshot() -> CtSnapshotRequest {
        CtSnapshotRequest {
            fingerprint: Fingerprint::ZERO,
            ready: CtReadyStage::Tokens,
            target_fingerprint: None,
            requested_queries: Vec::new(),
        }
    }
    struct Authority {
        snapshot: CtSnapshotRequest,
    }
    impl CtArtifactAuthority for Authority {
        fn scope_parameter_count(&self, scope: &CtAmbientLifeScope) -> Option<usize> {
            (scope.declaration == symbol()
                && scope.binder_site.is_empty()
                && matches!(scope.instance, CtScopeInstance::Definition))
            .then_some(2)
        }
        fn scope_parameter_kind(
            &self,
            scope: &CtAmbientLifeScope,
            ordinal: usize,
        ) -> Option<zil_verify::CtScopeParameterKind> {
            self.scope_parameter_count(scope)
                .filter(|n| ordinal < *n)
                .map(|_| zil_verify::CtScopeParameterKind::Lifetime)
        }

        fn source_fingerprint(&self) -> Fingerprint {
            Fingerprint::ZERO
        }
        fn snapshot(&self) -> &CtSnapshotRequest {
            &self.snapshot
        }
        fn authenticate_input_table(&self, input: &CtInputTable) -> bool {
            input.tokens.is_empty()
        }
        fn authenticate_artifact(&self, _: &CtArtifactProgram) -> bool {
            true
        }
        fn authenticate_function(&self, _: &CtFunction) -> bool {
            true
        }
        fn authorize_symbol(&self, _: &CtSymbolId, _: &CtSymbolId, _: CtSymbolUsage) -> bool {
            true
        }
        fn authorize_query(&self, _: &CtSymbolId, _: CtQueryKind) -> bool {
            true
        }
    }
    struct Reader;
    impl CtReadView for Reader {
        fn authenticate_argument(&self, _: u32, _: &CtTy, _: &CtValue) -> CompileResult<()> {
            Ok(())
        }
        fn type_name(
            &self,
            _: &CtSymbolId,
            _: &CtSymbolId,
            _: &mut CtExecutionBudget,
        ) -> CompileResult<CtTypeRef> {
            Err(error("unused test reader method"))
        }
        fn type_apply(
            &self,
            _: &CtSymbolId,
            _: &CtSymbolId,
            _: &[CtValue],
            _: &mut CtExecutionBudget,
        ) -> CompileResult<CtTypeRef> {
            Err(error("unused test reader method"))
        }
        fn definition_ident(
            &self,
            _: &CtSymbolId,
            _: &CtSymbolId,
            _: &mut CtExecutionBudget,
        ) -> CompileResult<CtIdent> {
            Err(error("unused test reader method"))
        }
        fn parse(
            &self,
            _: CtSyntaxCategory,
            _: &CtTokenTree,
            _: &mut CtExecutionBudget,
        ) -> CompileResult<CtSyntaxTree> {
            Err(error("unused test reader method"))
        }
        fn parse_ident(
            &self,
            _: &CtTokenTree,
            _: &mut CtExecutionBudget,
        ) -> CompileResult<CtIdent> {
            Err(error("unused test reader method"))
        }
        fn query(
            &self,
            _: &CtSymbolId,
            _: CtQueryKind,
            _: &[CtValue],
            _: &mut CtExecutionBudget,
        ) -> CompileResult<(CtValue, CtQueryDependency)> {
            Err(error("unused test reader method"))
        }
        fn build_syntax(
            &self,
            _: CtSyntaxBuilder,
            _: &[CtValue],
            _: &mut CtExecutionBudget,
        ) -> CompileResult<CtSyntaxTree> {
            Err(error("unused test reader method"))
        }
        fn instantiate_quote(
            &self,
            _: &CtTemplate,
            _: &[CtValue],
            _: &mut CtExecutionBudget,
        ) -> CompileResult<CtSyntaxTree> {
            Err(error("unused test reader method"))
        }
        fn token_inspect(
            &self,
            _: &CtValue,
            _: CtTokenProjection,
            _: Option<i64>,
            _: &mut CtExecutionBudget,
        ) -> CompileResult<CtValue> {
            Err(error("unused test reader method"))
        }
    }
    fn raw() -> CtArtifactProgram {
        let span = Span::default();
        CtArtifactProgram {
            schema_version: CT_ARTIFACT_SCHEMA_VERSION,
            target_pointer_width: 64,
            source_fingerprint: Fingerprint::ZERO,
            functions: vec![CtFunction {
                id: CtFunctionId(0),
                definition: symbol(),
                parameters: Vec::new(),
                parameter_types: Vec::new(),
                result_type: CtTy::Scalar(CtScalarTy::I32),
                locals: Vec::new(),
                values: vec![CtTy::Scalar(CtScalarTy::I32); 3],
                blocks: vec![CtBlock {
                    id: CtBlockId(0),
                    instructions: vec![
                        CtInstruction {
                            result: Some(CtValueId(0)),
                            kind: CtInstructionKind::Constant(CtConstantId(0)),
                            span,
                        },
                        CtInstruction {
                            result: Some(CtValueId(1)),
                            kind: CtInstructionKind::Constant(CtConstantId(1)),
                            span,
                        },
                        CtInstruction {
                            result: Some(CtValueId(2)),
                            kind: CtInstructionKind::Binary {
                                op: zil_ir::BinaryOp::Add,
                                left: CtValueId(0),
                                right: CtValueId(1),
                            },
                            span,
                        },
                    ],
                    terminator: CtTerminator::Return(Some(CtValueId(2))),
                }],
                entry: CtBlockId(0),
                span,
            }],
            entry: CtFunctionId(0),
            constants: vec![
                CtConstant::Scalar(Constant::I32(4)),
                CtConstant::Scalar(Constant::I32(5)),
            ],
            record_schemas: Vec::new(),
            enum_schemas: Vec::new(),
            templates: Vec::new(),
            input_tokens: CtInputTable {
                invocation_fingerprint: Fingerprint::ZERO,
                tokens: Vec::new(),
            },
            snapshot: snapshot(),
            budget: CtDeclaredBudget {
                max_steps: 100,
                max_call_depth: 10,
                max_live_cells: 100,
                max_temporary_bytes: 16_384,
                max_generated_nodes: 100,
                max_instances: 10,
                max_analysis_work: 1000,
            },
            ambient_life_scopes: Vec::new(),
        }
    }
    #[test]
    fn ieee_float_unsigned_artifact_conversions_observe_raw_bits_and_target_width() {
        for (width, input, target, expected) in [
            (
                64,
                Constant::U64(1 << 63),
                CtScalarTy::F64,
                Some(Constant::F64(0x43e0000000000000)),
            ),
            (
                64,
                Constant::U64(u64::MAX),
                CtScalarTy::F32,
                Some(Constant::F32(0x5f800000)),
            ),
            (
                64,
                Constant::F64(0x43efffffffffffff),
                CtScalarTy::U64,
                Some(Constant::U64(u64::MAX - 2047)),
            ),
            (
                64,
                Constant::F64(0x43efffffffffffff),
                CtScalarTy::Usize,
                Some(Constant::Usize(u64::MAX - 2047)),
            ),
            (64, Constant::F64(0x43f0000000000000), CtScalarTy::U64, None),
            (
                64,
                Constant::F64(0x43f0000000000000),
                CtScalarTy::Usize,
                None,
            ),
            (
                32,
                Constant::F64(0x41efffffffe00000),
                CtScalarTy::Usize,
                Some(Constant::Usize(u64::from(u32::MAX))),
            ),
            (
                32,
                Constant::Usize(u64::from(u32::MAX)),
                CtScalarTy::F64,
                Some(Constant::F64(0x41efffffffe00000)),
            ),
            (
                32,
                Constant::F64(0x41f0000000000000),
                CtScalarTy::Usize,
                None,
            ),
        ] {
            let mut raw = raw();
            raw.target_pointer_width = width;
            let source = scalar_ty(&input).unwrap();
            raw.constants = vec![CtConstant::Scalar(input)];
            let function = &mut raw.functions[0];
            function.result_type = CtTy::Scalar(target);
            function.values = vec![CtTy::Scalar(source), CtTy::Scalar(target)];
            function.blocks[0].instructions.truncate(1);
            function.blocks[0].instructions.push(CtInstruction {
                result: Some(CtValueId(1)),
                kind: CtInstructionKind::ConvertNumber {
                    value: CtValueId(0),
                    target,
                },
                span: Span::default(),
            });
            function.blocks[0].terminator = CtTerminator::Return(Some(CtValueId(1)));
            let authority = Authority {
                snapshot: raw.snapshot.clone(),
            };
            let verified = verify_ct_artifact(raw.clone(), &Limits::default(), &authority).unwrap();
            let mut budget = CtExecutionBudget::new(raw.budget);
            let result = evaluate_ct_artifact(&verified, &[], &Reader, &mut budget);
            if let Some(expected) = expected {
                assert_eq!(result.unwrap().0, CtValue::Scalar(expected));
            } else {
                assert!(
                    result
                        .unwrap_err()
                        .iter()
                        .all(|d| d.code == "E-NUMERIC-CONVERSION")
                );
            }
        }
    }

    #[test]
    fn ieee_float_raw_artifact_constants_and_identity_casts_preserve_bits() {
        for constant in [
            Constant::F32(0x7fc00001),
            Constant::F32(0xff800001),
            Constant::F64(0x7ff8000000000001),
            Constant::F64(0xfff0000000000001),
        ] {
            for cast in [false, true] {
                let mut raw = raw();
                let ty = if matches!(constant, Constant::F32(_)) {
                    CtScalarTy::F32
                } else {
                    CtScalarTy::F64
                };
                raw.constants = vec![CtConstant::Scalar(constant.clone())];
                let function = &mut raw.functions[0];
                function.result_type = CtTy::Scalar(ty);
                function.values = vec![CtTy::Scalar(ty); if cast { 2 } else { 1 }];
                function.blocks[0].instructions.truncate(1);
                if cast {
                    function.blocks[0].instructions.push(CtInstruction {
                        result: Some(CtValueId(1)),
                        kind: CtInstructionKind::ConvertNumber {
                            value: CtValueId(0),
                            target: ty,
                        },
                        span: Span::default(),
                    });
                }
                function.blocks[0].terminator =
                    CtTerminator::Return(Some(CtValueId(u32::from(cast))));
                let authority = Authority {
                    snapshot: raw.snapshot.clone(),
                };
                let verified =
                    verify_ct_artifact(raw.clone(), &Limits::default(), &authority).unwrap();
                let mut budget = CtExecutionBudget::new(raw.budget);
                let (value, _) =
                    evaluate_ct_artifact(&verified, &[], &Reader, &mut budget).unwrap();
                assert_eq!(value, CtValue::Scalar(constant.clone()));
            }
        }
    }

    #[test]
    fn executes_verified_scalar_cfg_with_shared_target_arithmetic() {
        let raw = raw();
        let authority = Authority {
            snapshot: raw.snapshot.clone(),
        };
        let verified = verify_ct_artifact(raw.clone(), &Limits::default(), &authority).unwrap();
        let mut budget = CtExecutionBudget::new(raw.budget);
        let (value, usage) = evaluate_ct_artifact(&verified, &[], &Reader, &mut budget).unwrap();
        assert_eq!(value, CtValue::Scalar(Constant::I32(9)));
        assert_eq!(usage.steps, 4);
    }
    #[test]
    fn unselected_late_query_branch_does_not_read_frozen_view() {
        let mut raw = raw();
        let span = Span::default();
        let function = &mut raw.functions[0];
        function.values = vec![
            CtTy::Scalar(CtScalarTy::Bool),
            CtTy::Type,
            CtTy::ShapeInfo,
            CtTy::Scalar(CtScalarTy::I32),
            CtTy::Scalar(CtScalarTy::I32),
        ];
        function.blocks = vec![
            CtBlock {
                id: CtBlockId(0),
                instructions: vec![CtInstruction {
                    result: Some(CtValueId(0)),
                    kind: CtInstructionKind::Constant(CtConstantId(0)),
                    span,
                }],
                terminator: CtTerminator::Branch {
                    condition: CtValueId(0),
                    then_block: CtBlockId(1),
                    else_block: CtBlockId(2),
                },
            },
            CtBlock {
                id: CtBlockId(1),
                instructions: vec![
                    CtInstruction {
                        result: Some(CtValueId(1)),
                        kind: CtInstructionKind::TypePrimitive(CtScalarTy::I32),
                        span,
                    },
                    CtInstruction {
                        result: Some(CtValueId(2)),
                        kind: CtInstructionKind::Query {
                            kind: CtQueryKind::Shape,
                            arguments: vec![CtValueId(1)],
                        },
                        span,
                    },
                    CtInstruction {
                        result: Some(CtValueId(3)),
                        kind: CtInstructionKind::Constant(CtConstantId(1)),
                        span,
                    },
                ],
                terminator: CtTerminator::Return(Some(CtValueId(3))),
            },
            CtBlock {
                id: CtBlockId(2),
                instructions: vec![CtInstruction {
                    result: Some(CtValueId(4)),
                    kind: CtInstructionKind::Constant(CtConstantId(1)),
                    span,
                }],
                terminator: CtTerminator::Return(Some(CtValueId(4))),
            },
        ];
        raw.constants = vec![
            CtConstant::Scalar(Constant::Bool(false)),
            CtConstant::Scalar(Constant::I32(5)),
        ];
        let authority = Authority {
            snapshot: raw.snapshot.clone(),
        };
        let verified = verify_ct_artifact(raw.clone(), &Limits::default(), &authority).unwrap();
        let (value, receipt) = evaluate_ct_artifact(
            &verified,
            &[],
            &Reader,
            &mut CtExecutionBudget::new(raw.budget),
        )
        .unwrap();
        assert_eq!(value, CtValue::Scalar(Constant::I32(5)));
        assert!(receipt.queries.is_empty());
    }
    #[test]
    fn enforces_step_limit_before_instruction() {
        let raw = raw();
        let authority = Authority {
            snapshot: raw.snapshot.clone(),
        };
        let verified = verify_ct_artifact(raw.clone(), &Limits::default(), &authority).unwrap();
        let mut budget = CtExecutionBudget::new(CtDeclaredBudget {
            max_steps: 3,
            ..raw.budget
        });
        assert_eq!(
            evaluate_ct_artifact(&verified, &[], &Reader, &mut budget).unwrap_err()[0].code,
            "E-RESOURCE-LIMIT"
        );
    }
    #[test]
    fn temporary_bytes_exact_peak_and_fresh_retry() {
        let raw = raw();
        let authority = Authority {
            snapshot: raw.snapshot.clone(),
        };
        let verified = verify_ct_artifact(raw.clone(), &Limits::default(), &authority).unwrap();
        let (_, probe) = evaluate_ct_artifact(
            &verified,
            &[],
            &Reader,
            &mut CtExecutionBudget::new(raw.budget),
        )
        .unwrap();
        let peak = probe.peak_temporary_bytes;
        assert!(
            peak > 64,
            "the historical logical-scalar limit cannot hold actual frame buffers"
        );
        let exact = CtDeclaredBudget {
            max_temporary_bytes: peak,
            ..raw.budget
        };
        let (_, receipt) =
            evaluate_ct_artifact(&verified, &[], &Reader, &mut CtExecutionBudget::new(exact))
                .unwrap();
        assert_eq!(receipt.peak_temporary_bytes, peak);
        let below = CtDeclaredBudget {
            max_temporary_bytes: peak - 1,
            ..raw.budget
        };
        assert_eq!(
            evaluate_ct_artifact(&verified, &[], &Reader, &mut CtExecutionBudget::new(below))
                .unwrap_err()[0]
                .code,
            "E-RESOURCE-LIMIT"
        );
        assert_eq!(
            evaluate_ct_artifact(&verified, &[], &Reader, &mut CtExecutionBudget::new(exact))
                .unwrap()
                .0,
            CtValue::Scalar(Constant::I32(9))
        );
    }
    #[test]
    fn actual_descriptor_buffers_have_checked_byte_neighbors_and_cleanup() {
        let mut program = raw();
        program.budget.max_temporary_bytes = 1_000_000;
        program.budget.max_analysis_work = 100_000;
        let function = &mut program.functions[0];
        function.parameters = vec![CtLocalId(0)];
        function.parameter_types = vec![CtTy::FieldInfo];
        function.locals = vec![CtTy::FieldInfo];
        function.values = vec![CtTy::FieldInfo];
        function.result_type = CtTy::FieldInfo;
        function.blocks = vec![CtBlock {
            id: CtBlockId(0),
            instructions: vec![CtInstruction {
                result: Some(CtValueId(0)),
                kind: CtInstructionKind::Load(CtLocalId(0)),
                span: Span::default(),
            }],
            terminator: CtTerminator::Return(Some(CtValueId(0))),
        }];
        let authority = Authority {
            snapshot: program.snapshot.clone(),
        };
        let verified = verify_ct_artifact(program.clone(), &Limits::default(), &authority).unwrap();
        let mut nodes = vec![CtTypeNode::Primitive(CtScalarTy::I32); 64];
        nodes.push(CtTypeNode::Tuple((0..64).map(CtTypeNodeId).collect()));
        let graph = CtTypeRef {
            nodes,
            root: CtTypeNodeId(64),
            ambient_classes: Vec::new(),
        };
        let owner = symbol();
        let mut owner_type = graph.clone();
        owner_type.nodes.push(CtTypeNode::Nominal {
            symbol: owner.clone(),
            arguments: vec![CtTypeArgument::Type(graph.root)],
        });
        owner_type.root = CtTypeNodeId(65);
        let value = CtValue::FieldInfo(CtFieldInfo {
            owner: owner.clone(),
            owner_type: owner_type.clone(),
            field: owner.clone(),
            name: CtIdent {
                text: "field".repeat(128),
                namespace: CtNameNamespace::Member,
                source: CtNameSource::Definition(owner.clone()),
                member_owner: Some(owner),
                member_owner_type: Some(owner_type),
            },
            ty: graph,
            managed: false,
        });
        let mut probe = CtExecutionBudget::new(program.budget);
        let (output, usage) =
            evaluate_ct_artifact(&verified, std::slice::from_ref(&value), &Reader, &mut probe)
                .unwrap();
        assert_eq!(output, value);
        let peak = usage.peak_temporary_bytes;
        assert!(
            peak > 32_000,
            "complete descriptors need actual inline cells and all three graphs"
        );
        probe.release_value(&output);
        probe.release_usage_receipt(&usage);
        assert_eq!(probe.live_temporary_bytes(), 0);
        for (bytes, good) in [(peak, true), (peak - 1, false), (peak, true)] {
            let mut budget = CtExecutionBudget::new(CtDeclaredBudget {
                max_temporary_bytes: bytes,
                ..program.budget
            });
            let result = evaluate_ct_artifact(
                &verified,
                std::slice::from_ref(&value),
                &Reader,
                &mut budget,
            );
            if good {
                let (output, usage) = result.unwrap();
                assert_eq!(output, value);
                budget.release_value(&output);
                budget.release_usage_receipt(&usage);
            } else {
                assert_eq!(result.unwrap_err()[0].code, "E-RESOURCE-LIMIT");
            }
            assert_eq!(
                budget.live_temporary_bytes(),
                0,
                "failed/successful invocation retains no frame or descriptor scratch"
            );
        }
        eprintln!("CT5 descriptor temporary-byte boundary {peak}");
    }
    #[test]
    fn actual_frame_buffers_are_released_after_step_failure() {
        let program = raw();
        let verified = verify_ct_artifact(
            program.clone(),
            &Limits::default(),
            &Authority {
                snapshot: program.snapshot.clone(),
            },
        )
        .unwrap();
        let mut budget = CtExecutionBudget::new(CtDeclaredBudget {
            max_steps: 1,
            ..program.budget
        });
        assert_eq!(
            evaluate_ct_artifact(&verified, &[], &Reader, &mut budget).unwrap_err()[0].code,
            "E-RESOURCE-LIMIT"
        );
        assert!(
            budget.usage().peak_temporary_bytes
                >= std::mem::size_of::<Option<CtValue>>() as u64 * 3
        );
        assert_eq!(budget.live_temporary_bytes(), 0);
    }
    #[test]
    fn executes_verified_loop_with_local_refill() {
        let mut program = raw();
        let s = Span::default();
        let f = &mut program.functions[0];
        f.locals = vec![CtTy::Scalar(CtScalarTy::I32)];
        f.values = vec![CtTy::Scalar(CtScalarTy::I32); 8];
        f.values[3] = CtTy::Scalar(CtScalarTy::Bool);
        f.blocks = vec![
            CtBlock {
                id: CtBlockId(0),
                instructions: vec![
                    CtInstruction {
                        result: None,
                        kind: CtInstructionKind::StorageLive(CtLocalId(0)),
                        span: s,
                    },
                    CtInstruction {
                        result: Some(CtValueId(0)),
                        kind: CtInstructionKind::Constant(CtConstantId(0)),
                        span: s,
                    },
                    CtInstruction {
                        result: None,
                        kind: CtInstructionKind::Store {
                            local: CtLocalId(0),
                            value: CtValueId(0),
                        },
                        span: s,
                    },
                ],
                terminator: CtTerminator::Jump(CtBlockId(1)),
            },
            CtBlock {
                id: CtBlockId(1),
                instructions: vec![
                    CtInstruction {
                        result: Some(CtValueId(1)),
                        kind: CtInstructionKind::Load(CtLocalId(0)),
                        span: s,
                    },
                    CtInstruction {
                        result: Some(CtValueId(2)),
                        kind: CtInstructionKind::Constant(CtConstantId(1)),
                        span: s,
                    },
                    CtInstruction {
                        result: Some(CtValueId(3)),
                        kind: CtInstructionKind::Binary {
                            op: zil_ir::BinaryOp::Less,
                            left: CtValueId(1),
                            right: CtValueId(2),
                        },
                        span: s,
                    },
                ],
                terminator: CtTerminator::Branch {
                    condition: CtValueId(3),
                    then_block: CtBlockId(2),
                    else_block: CtBlockId(3),
                },
            },
            CtBlock {
                id: CtBlockId(2),
                instructions: vec![
                    CtInstruction {
                        result: Some(CtValueId(4)),
                        kind: CtInstructionKind::Load(CtLocalId(0)),
                        span: s,
                    },
                    CtInstruction {
                        result: Some(CtValueId(5)),
                        kind: CtInstructionKind::Constant(CtConstantId(2)),
                        span: s,
                    },
                    CtInstruction {
                        result: Some(CtValueId(6)),
                        kind: CtInstructionKind::Binary {
                            op: zil_ir::BinaryOp::Add,
                            left: CtValueId(4),
                            right: CtValueId(5),
                        },
                        span: s,
                    },
                    CtInstruction {
                        result: None,
                        kind: CtInstructionKind::Store {
                            local: CtLocalId(0),
                            value: CtValueId(6),
                        },
                        span: s,
                    },
                ],
                terminator: CtTerminator::Jump(CtBlockId(1)),
            },
            CtBlock {
                id: CtBlockId(3),
                instructions: vec![CtInstruction {
                    result: Some(CtValueId(7)),
                    kind: CtInstructionKind::Load(CtLocalId(0)),
                    span: s,
                }],
                terminator: CtTerminator::Return(Some(CtValueId(7))),
            },
        ];
        program.constants = vec![
            CtConstant::Scalar(Constant::I32(0)),
            CtConstant::Scalar(Constant::I32(3)),
            CtConstant::Scalar(Constant::I32(1)),
        ];
        let authority = Authority {
            snapshot: program.snapshot.clone(),
        };
        let verified = verify_ct_artifact(program.clone(), &Limits::default(), &authority).unwrap();
        let (value, _) = evaluate_ct_artifact(
            &verified,
            &[],
            &Reader,
            &mut CtExecutionBudget::new(program.budget),
        )
        .unwrap();
        assert_eq!(value, CtValue::Scalar(Constant::I32(3)));
    }
    #[test]
    fn constructs_array_type_value_with_element_identity() {
        let mut program = raw();
        let s = Span::default();
        let f = &mut program.functions[0];
        f.result_type = CtTy::Type;
        f.values = vec![CtTy::Type, CtTy::Scalar(CtScalarTy::I32), CtTy::Type];
        f.blocks[0].instructions = vec![
            CtInstruction {
                result: Some(CtValueId(0)),
                kind: CtInstructionKind::TypePrimitive(CtScalarTy::I32),
                span: s,
            },
            CtInstruction {
                result: Some(CtValueId(1)),
                kind: CtInstructionKind::Constant(CtConstantId(0)),
                span: s,
            },
            CtInstruction {
                result: Some(CtValueId(2)),
                kind: CtInstructionKind::TypeConstruct {
                    constructor: CtTypeConstructor::Array,
                    arguments: vec![CtValueId(0), CtValueId(1)],
                },
                span: s,
            },
        ];
        f.blocks[0].terminator = CtTerminator::Return(Some(CtValueId(2)));
        let authority = Authority {
            snapshot: program.snapshot.clone(),
        };
        let verified = verify_ct_artifact(program.clone(), &Limits::default(), &authority).unwrap();
        let (answer, _) = evaluate_ct_artifact(
            &verified,
            &[],
            &Reader,
            &mut CtExecutionBudget::new(program.budget),
        )
        .unwrap();
        let CtValue::Type(result) = answer else {
            panic!("expected CT Type")
        };
        assert_eq!(result.root, CtTypeNodeId(1));
        assert_eq!(
            result.nodes[1],
            CtTypeNode::Array {
                element: CtTypeNodeId(0),
                length: 4
            }
        );
    }
    #[test]
    fn anonymous_type_constructor_vm_wire_and_raw_admission() {
        let mut program = raw();
        program.budget.max_temporary_bytes = 1_000_000;
        program.budget.max_analysis_work = 1_000_000;
        program.budget.max_generated_nodes = 100_000;
        let f = &mut program.functions[0];
        f.result_type = CtTy::Scalar(CtScalarTy::Bool);
        f.values = vec![
            CtTy::Type,
            CtTy::Type,
            CtTy::Type,
            CtTy::Type,
            CtTy::Scalar(CtScalarTy::Bool),
        ];
        let kinds = [
            CtInstructionKind::TypePrimitive(CtScalarTy::I32),
            CtInstructionKind::TypePrimitive(CtScalarTy::Bool),
            CtInstructionKind::TypeConstruct {
                constructor: CtTypeConstructor::AnonymousSum,
                arguments: vec![CtValueId(1), CtValueId(0)],
            },
            CtInstructionKind::TypeConstruct {
                constructor: CtTypeConstructor::AnonymousSum,
                arguments: vec![CtValueId(0), CtValueId(1)],
            },
            CtInstructionKind::Binary {
                op: zil_ir::BinaryOp::Equal,
                left: CtValueId(2),
                right: CtValueId(3),
            },
        ];
        f.blocks[0].instructions = kinds
            .into_iter()
            .enumerate()
            .map(|(id, kind)| CtInstruction {
                result: Some(CtValueId(id as u32)),
                kind,
                span: Span::default(),
            })
            .collect();
        f.blocks[0].terminator = CtTerminator::Return(Some(CtValueId(4)));
        let limits = Limits::default();
        let wire = zil_ir::ct_wire::encode_program(&program, &limits).unwrap();
        let decoded = zil_ir::ct_wire::decode_program(&wire, &limits).unwrap();
        let authority = Authority {
            snapshot: program.snapshot.clone(),
        };
        let verified = verify_ct_artifact(decoded, &limits, &authority).unwrap();
        let (result, _) = evaluate_ct_artifact(
            &verified,
            &[],
            &Reader,
            &mut CtExecutionBudget::new(program.budget),
        )
        .unwrap();
        assert_eq!(result, CtValue::Scalar(Constant::Bool(true)));
        let mut wrong = program.clone();
        let CtInstructionKind::TypeConstruct { arguments, .. } =
            &mut wrong.functions[0].blocks[0].instructions[2].kind
        else {
            unreachable!()
        };
        arguments.truncate(1);
        assert!(verify_ct_artifact(wrong, &limits, &authority).is_err());
        let mut repeated = program.clone();
        let CtInstructionKind::TypeConstruct { arguments, .. } =
            &mut repeated.functions[0].blocks[0].instructions[2].kind
        else {
            unreachable!()
        };
        arguments[1] = arguments[0];
        let verified = verify_ct_artifact(repeated, &limits, &authority).unwrap();
        assert!(
            evaluate_ct_artifact(
                &verified,
                &[],
                &Reader,
                &mut CtExecutionBudget::new(program.budget)
            )
            .is_err()
        );
        let canonical = crate::canonicalize_anonymous_type(
            &CtTypeRef {
                nodes: vec![
                    CtTypeNode::Primitive(CtScalarTy::I32),
                    CtTypeNode::Primitive(CtScalarTy::Bool),
                    CtTypeNode::AnonymousSum(vec![CtTypeNodeId(0), CtTypeNodeId(1)]),
                ],
                root: CtTypeNodeId(2),
                ambient_classes: Vec::new(),
            },
            &mut CtExecutionBudget::new(program.budget),
        )
        .unwrap();
        for attack in 0..4 {
            let mut bad = canonical.clone();
            let CtTypeNode::AnonymousSum(members) = &mut bad.nodes[2] else {
                unreachable!()
            };
            match attack {
                0 => members.swap(0, 1),
                1 => members[1] = members[0],
                2 => members.truncate(1),
                _ => members[0] = CtTypeNodeId(2),
            }
            let bytes = zil_ir::ct_wire::encode_value(&CtValue::Type(bad), &limits).unwrap();
            let CtValue::Type(decoded) = zil_ir::ct_wire::decode_value(&bytes, &limits).unwrap()
            else {
                unreachable!()
            };
            assert!(
                !type_ref_matches(
                    &decoded,
                    &program,
                    &mut CtExecutionBudget::new(program.budget)
                )
                .unwrap_or(false),
                "raw/Wire type attack {attack}"
            );
        }
    }
    fn anonymous_value_program() -> CtArtifactProgram {
        let mut p = raw();
        p.budget.max_analysis_work = 1_000_000;
        p.budget.max_temporary_bytes = 1_000_000;
        let members = vec![
            CtTy::Scalar(CtScalarTy::Bool),
            CtTy::Scalar(CtScalarTy::I32),
        ];
        let f = &mut p.functions[0];
        f.result_type = CtTy::AnonymousSum(members.clone());
        f.values = vec![CtTy::Scalar(CtScalarTy::I32), f.result_type.clone()];
        f.blocks[0].instructions = vec![
            CtInstruction {
                result: Some(CtValueId(0)),
                kind: CtInstructionKind::Constant(CtConstantId(0)),
                span: Span::default(),
            },
            CtInstruction {
                result: Some(CtValueId(1)),
                kind: CtInstructionKind::MakeAnonymousSum {
                    members,
                    active: 1,
                    payload: CtValueId(0),
                },
                span: Span::default(),
            },
        ];
        f.blocks[0].terminator = CtTerminator::Return(Some(CtValueId(1)));
        p
    }
    #[test]
    fn anonymous_value_raw_vm_wire_and_inactive_projection() {
        let p = anonymous_value_program();
        let limits = Limits::default();
        let authority = Authority {
            snapshot: p.snapshot.clone(),
        };
        let bytes = zil_ir::ct_wire::encode_program(&p, &limits).unwrap();
        let decoded = zil_ir::ct_wire::decode_program(&bytes, &limits).unwrap();
        let checked = verify_ct_artifact(decoded, &limits, &authority).unwrap();
        let (value, _) = evaluate_ct_artifact(
            &checked,
            &[],
            &Reader,
            &mut CtExecutionBudget::new(p.budget),
        )
        .unwrap();
        let expected = CtValue::AnonymousSum {
            members: vec![
                CtTy::Scalar(CtScalarTy::Bool),
                CtTy::Scalar(CtScalarTy::I32),
            ],
            active: 1,
            payload: Box::new(CtValue::Scalar(Constant::I32(4))),
        };
        assert_eq!(value, expected);
        let value_bytes = zil_ir::ct_wire::encode_value(&value, &limits).unwrap();
        assert_eq!(
            zil_ir::ct_wire::decode_value(&value_bytes, &limits).unwrap(),
            value
        );
        for attack in 0..5 {
            let mut raw = p.clone();
            let CtInstructionKind::MakeAnonymousSum {
                members,
                active,
                payload,
            } = &mut raw.functions[0].blocks[0].instructions[1].kind
            else {
                unreachable!()
            };
            match attack {
                0 => members.reverse(),
                1 => *active = 2,
                2 => *active = 0,
                3 => members[1] = members[0].clone(),
                _ => *payload = CtValueId(9),
            };
            assert!(
                verify_ct_artifact(raw, &limits, &authority).is_err(),
                "raw attack {attack}"
            );
        }
        for active in [true, false] {
            let mut raw = p.clone();
            let f = &mut raw.functions[0];
            f.result_type = CtTy::Scalar(if active {
                CtScalarTy::I32
            } else {
                CtScalarTy::Bool
            });
            f.values.push(f.result_type.clone());
            f.blocks[0].instructions.push(CtInstruction {
                result: Some(CtValueId(2)),
                kind: CtInstructionKind::VariantField {
                    value: CtValueId(1),
                    variant: if active { 1 } else { 0 },
                    field: 0,
                },
                span: Span::default(),
            });
            f.blocks[0].terminator = CtTerminator::Return(Some(CtValueId(2)));
            let checked = verify_ct_artifact(raw.clone(), &limits, &authority).unwrap();
            let output = evaluate_ct_artifact(
                &checked,
                &[],
                &Reader,
                &mut CtExecutionBudget::new(raw.budget),
            );
            if active {
                assert_eq!(output.unwrap().0, CtValue::Scalar(Constant::I32(4)));
            } else {
                assert!(output.unwrap_err()[0].message.contains("inactive"));
            }
        }
    }
    #[test]
    fn anonymous_value_complete_descriptor_and_payload_are_checked() {
        let raw = anonymous_value_program();
        let ty = raw.functions[0].result_type.clone();
        let CtTy::AnonymousSum(members) = &ty else {
            unreachable!()
        };
        let valid = CtValue::AnonymousSum {
            members: members.clone(),
            active: 1,
            payload: Box::new(CtValue::Scalar(Constant::I32(4))),
        };
        let mut budget = CtExecutionBudget::new(raw.budget);
        assert!(value_matches_type(&valid, &ty, &raw, 0, &mut budget).unwrap());
        assert_eq!(budget.live_temporary_bytes(), 0);
        for attack in 0..5 {
            let mut value = valid.clone();
            let CtValue::AnonymousSum {
                members,
                active,
                payload,
            } = &mut value
            else {
                unreachable!()
            };
            match attack {
                0 => members.reverse(),
                1 => *active = 9,
                2 => **payload = CtValue::Scalar(Constant::Bool(false)),
                3 => members[0] = CtTy::Record(CtRecordSchemaId(9)),
                _ => members[0] = CtTy::Scalar(CtScalarTy::U8),
            };
            assert!(
                !value_matches_type(
                    &value,
                    &ty,
                    &raw,
                    0,
                    &mut CtExecutionBudget::new(raw.budget)
                )
                .unwrap_or(false),
                "value attack {attack}"
            );
        }
    }
    #[test]
    fn ct4_closed_invocation_metadata_rejects_equal_sibling_owner_reuse() {
        let mut p = anonymous_value_program();
        let owner = CtSymbolId {
            declaration: NodeId(30),
            ..symbol()
        };
        let tag = CtSymbolId {
            declaration: NodeId(31),
            ..symbol()
        };
        let mut nodes = Vec::new();
        for base in [0, 3] {
            nodes.push(CtTypeNode::Nominal {
                symbol: tag.clone(),
                arguments: vec![CtTypeArgument::Lifetime(CtLifetimeArgument {
                    owner: CtTypeNodeId(base + 2),
                    binder: zil_ir::StorageBinder::Invocation(0),
                })],
            });
            nodes.push(CtTypeNode::Address {
                mutable: false,
                inner: CtTypeNodeId(base),
            });
            nodes.push(CtTypeNode::BorrowedFunctionPointer {
                parameters: vec![CtTypeNodeId(base + 1)],
                result: CtTypeNodeId(base + 1),
                life: zil_ir::StorageLifeContract::from_flat(zil_ir::FunctionLifeContract {
                    binder_count: 1,
                    parameter_lives: vec![Some(0)],
                    result_life: Some(0),
                })
                .unwrap(),
            });
        }
        nodes.push(CtTypeNode::Tuple(vec![CtTypeNodeId(2), CtTypeNodeId(5)]));
        let graph = crate::canonicalize_anonymous_type(
            &CtTypeRef {
                nodes,
                root: CtTypeNodeId(6),
                ambient_classes: Vec::new(),
            },
            &mut CtExecutionBudget::new(p.budget),
        )
        .unwrap();
        let roots = graph
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                matches!(n, CtTypeNode::BorrowedFunctionPointer { .. })
                    .then_some(CtTypeNodeId(i as u32))
            })
            .collect::<Vec<_>>();
        p.record_schemas.push(CtRecordSchema {
            name: owner,
            life_context: None,
            arguments: vec![CtSchemaArgument::Type(graph)],
            fields: vec![CtTy::Scalar(CtScalarTy::I32)],
            origin: CtSchemaOrigin::UserPure,
        });
        let limits = Limits::default();
        let authority = Authority {
            snapshot: p.snapshot.clone(),
        };
        assert!(verify_ct_artifact(p.clone(), &limits, &authority).is_ok());
        let key = crate::artifact_type_key(
            &CtTy::Record(CtRecordSchemaId(0)),
            &p.record_schemas,
            &[],
            &mut CtExecutionBudget::new(p.budget),
        )
        .unwrap();
        assert!(!key.is_empty());
        let encoded = zil_ir::ct_wire::encode_program(&p, &limits).unwrap();
        assert!(
            verify_ct_artifact(
                zil_ir::ct_wire::decode_program(&encoded, &limits).unwrap(),
                &limits,
                &authority
            )
            .is_ok()
        );
        let CtSchemaArgument::Type(graph) = &mut p.record_schemas[0].arguments[0] else {
            unreachable!()
        };
        let CtTypeNode::Nominal { arguments, .. } = &mut graph.nodes[0] else {
            unreachable!()
        };
        let CtTypeArgument::Lifetime(value) = &mut arguments[0] else {
            unreachable!()
        };
        value.owner = roots[1];
        assert!(verify_ct_artifact(p.clone(), &limits, &authority).is_err());
        assert!(
            crate::artifact_type_key(
                &CtTy::Record(CtRecordSchemaId(0)),
                &p.record_schemas,
                &[],
                &mut CtExecutionBudget::new(p.budget)
            )
            .is_err()
        );
    }
    #[test]
    fn ct4_joint_nominal_scope_keys_wire_and_exact_argument_projections() {
        use zil_ir::{
            LifeDeclarationId, LifeTypeProjection as P, NominalLifeArgument, StorageBinder as B,
            StorageLifeShape,
        };
        let mut p = anonymous_value_program();
        let owner = CtSymbolId {
            declaration: NodeId(30),
            ..symbol()
        };
        let inner = CtSymbolId {
            declaration: NodeId(31),
            ..symbol()
        };
        let lifetime = |n| CtLifetimeArgument {
            owner: CtTypeNodeId(3),
            binder: B::Fixed(n),
        };
        let row = |path, decl: &CtSymbolId, argument, binder| NominalLifeArgument {
            path,
            declaration: LifeDeclarationId {
                module: decl.module,
                declaration: decl.declaration,
                compiler: false,
            },
            argument,
            binder,
        };
        let mut context = CtTypeRef {
            root: CtTypeNodeId(3),
            nodes: vec![
                CtTypeNode::Primitive(CtScalarTy::I32),
                CtTypeNode::Nominal {
                    symbol: inner.clone(),
                    arguments: vec![
                        CtTypeArgument::Lifetime(lifetime(0)),
                        CtTypeArgument::Lifetime(lifetime(1)),
                    ],
                },
                CtTypeNode::Nominal {
                    symbol: owner.clone(),
                    arguments: vec![
                        CtTypeArgument::Type(CtTypeNodeId(0)),
                        CtTypeArgument::Type(CtTypeNodeId(1)),
                        CtTypeArgument::Lifetime(lifetime(0)),
                        CtTypeArgument::Lifetime(lifetime(1)),
                        CtTypeArgument::Lifetime(lifetime(0)),
                    ],
                },
                CtTypeNode::BorrowedStorage {
                    storage: CtTypeNodeId(2),
                    life: StorageLifeShape {
                        binder_count: 2,
                        slots: vec![],
                        retained: vec![],
                        nominal_arguments: vec![
                            row(vec![], &owner, 2, 0),
                            row(vec![], &owner, 3, 1),
                            row(vec![], &owner, 4, 0),
                            row(vec![P::TypeArgument(1)], &inner, 0, 0),
                            row(vec![P::TypeArgument(1)], &inner, 1, 1),
                        ],
                    },
                },
            ],
            ambient_classes: Vec::new(),
        };
        p.ambient_life_scopes.push(CtAmbientLifeScope {
            enclosing: None,
            declaration: symbol(),
            binder_site: Vec::new(),
            instance: CtScopeInstance::Definition,
        });
        context.ambient_classes = (0..2)
            .map(|class| CtAmbientClassBinding {
                owner: context.root,
                class,
                source: CtAmbientLifeReference {
                    scope: CtAmbientScopeId(0),
                    parameter: class,
                },
            })
            .collect();
        let normalized =
            crate::canonicalize_anonymous_type(&context, &mut CtExecutionBudget::new(p.budget))
                .unwrap();
        assert_eq!(normalized, context);
        p.record_schemas.push(CtRecordSchema {
            name: owner,
            life_context: Some(context.clone()),
            arguments: vec![
                CtSchemaArgument::TypeInContext(CtTypeNodeId(0)),
                CtSchemaArgument::TypeInContext(CtTypeNodeId(1)),
                CtSchemaArgument::Lifetime(lifetime(0)),
                CtSchemaArgument::Lifetime(lifetime(1)),
                CtSchemaArgument::Lifetime(lifetime(0)),
            ],
            fields: vec![CtTy::Scalar(CtScalarTy::I32)],
            origin: CtSchemaOrigin::UserPure,
        });
        let authority = Authority {
            snapshot: p.snapshot.clone(),
        };
        let limits = Limits::default();
        let key = crate::artifact_type_key(
            &CtTy::Record(CtRecordSchemaId(0)),
            &p.record_schemas,
            &[],
            &mut CtExecutionBudget::new(p.budget),
        )
        .unwrap();
        let encoded = zil_ir::ct_wire::encode_program(&p, &limits).unwrap();
        let decoded = zil_ir::ct_wire::decode_program(&encoded, &limits).unwrap();
        let verified = verify_ct_artifact(decoded, &limits, &authority).unwrap();
        assert!(
            evaluate_ct_artifact(
                &verified,
                &[],
                &Reader,
                &mut CtExecutionBudget::new(p.budget)
            )
            .is_ok()
        );
        for attack in 0..7 {
            let mut bad = p.clone();
            let schema = &mut bad.record_schemas[0];
            match attack {
                0 => {
                    schema.arguments[0] = CtSchemaArgument::Type(CtTypeRef {
                        root: CtTypeNodeId(0),
                        nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
                        ambient_classes: Vec::new(),
                    })
                }
                1 => schema.arguments[0] = CtSchemaArgument::TypeInContext(CtTypeNodeId(1)),
                2 => {
                    schema.arguments[2] = CtSchemaArgument::Lifetime(CtLifetimeArgument {
                        owner: CtTypeNodeId(2),
                        binder: B::Fixed(0),
                    })
                }
                3 => schema.life_context = None,
                4 => {
                    let CtTypeNode::BorrowedStorage { life, .. } =
                        &mut schema.life_context.as_mut().unwrap().nodes[3]
                    else {
                        unreachable!()
                    };
                    life.nominal_arguments.pop();
                }
                5 => {
                    let CtTypeNode::Nominal { arguments, .. } =
                        &mut schema.life_context.as_mut().unwrap().nodes[1]
                    else {
                        unreachable!()
                    };
                    arguments[0] = CtTypeArgument::Lifetime(CtLifetimeArgument {
                        owner: CtTypeNodeId(2),
                        binder: B::Fixed(0),
                    });
                }
                _ => {
                    let CtTypeNode::BorrowedStorage { life, .. } =
                        &mut schema.life_context.as_mut().unwrap().nodes[3]
                    else {
                        unreachable!()
                    };
                    life.nominal_arguments[0].declaration.compiler = true;
                }
            }
            assert!(
                verify_ct_artifact(bad, &limits, &authority).is_err(),
                "joint scope attack {attack}"
            );
        }
        // A caller-supplied lexical host edge remains an untrusted graph edge.
        // Even a broad test authority cannot turn a cycle or missing row into
        // an admissible complete scope inventory.
        for parent in [CtAmbientScopeId(0), CtAmbientScopeId(u32::MAX)] {
            let mut bad = p.clone();
            bad.ambient_life_scopes[0].enclosing = Some(parent);
            let bytes = zil_ir::ct_wire::encode_program(&bad, &limits).unwrap();
            let decoded = zil_ir::ct_wire::decode_program(&bytes, &limits).unwrap();
            assert!(verify_ct_artifact(decoded, &limits, &authority).is_err());
        }
        let mut different = p.clone();
        let schema = &mut different.record_schemas[0];
        schema.arguments[4] = CtSchemaArgument::Lifetime(lifetime(1));
        let nodes = &mut schema.life_context.as_mut().unwrap().nodes;
        let CtTypeNode::Nominal { arguments, .. } = &mut nodes[2] else {
            unreachable!()
        };
        arguments[4] = CtTypeArgument::Lifetime(lifetime(1));
        let CtTypeNode::BorrowedStorage { life, .. } = &mut nodes[3] else {
            unreachable!()
        };
        life.nominal_arguments[2].binder = 1;
        assert_ne!(
            key,
            crate::artifact_type_key(
                &CtTy::Record(CtRecordSchemaId(0)),
                &different.record_schemas,
                &[],
                &mut CtExecutionBudget::new(p.budget)
            )
            .unwrap()
        );
        assert!(verify_ct_artifact(different, &limits, &authority).is_ok());
        let run = |work| {
            let mut candidate = p.clone();
            candidate.budget.max_analysis_work = work;
            verify_ct_artifact(candidate, &limits, &authority)
        };
        let (mut low, mut high) = (1, p.budget.max_analysis_work);
        while low < high {
            let mid = (low + high) / 2;
            if run(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        assert!(run(low).is_ok());
        assert_eq!(
            run(low - 1).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert!(run(low).is_ok());
        eprintln!("344 CT4 joint verification N={low}/N-1/fresh");
    }
    #[test]
    fn anonymous_value_nominal_phantom_const_identity_cycles_and_budget() {
        let mut p = anonymous_value_program();
        let owner = CtSymbolId {
            declaration: NodeId(30),
            ..symbol()
        };
        for (phantom, n) in [
            (CtScalarTy::Bool, 7),
            (CtScalarTy::U8, 7),
            (CtScalarTy::Bool, 8),
        ] {
            p.record_schemas.push(CtRecordSchema {
                life_context: None,
                name: owner.clone(),
                arguments: vec![
                    CtSchemaArgument::Type(CtTypeRef {
                        nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
                        root: CtTypeNodeId(0),
                        ambient_classes: Vec::new(),
                    }),
                    CtSchemaArgument::Type(CtTypeRef {
                        nodes: vec![CtTypeNode::Primitive(phantom)],
                        root: CtTypeNodeId(0),
                        ambient_classes: Vec::new(),
                    }),
                    CtSchemaArgument::Scalar(Constant::I32(n)),
                ],
                fields: vec![CtTy::Scalar(CtScalarTy::I32)],
                origin: CtSchemaOrigin::UserPure,
            });
        }
        let input = vec![
            CtTy::Record(CtRecordSchemaId(2)),
            CtTy::Record(CtRecordSchemaId(0)),
            CtTy::Record(CtRecordSchemaId(1)),
        ];
        let canonical = crate::canonical_artifact_sum(
            input.clone(),
            &p.record_schemas,
            &[],
            &mut CtExecutionBudget::new(p.budget),
        )
        .unwrap();
        let mut swapped = input;
        swapped.reverse();
        assert_eq!(
            crate::canonical_artifact_sum(
                swapped,
                &p.record_schemas,
                &[],
                &mut CtExecutionBudget::new(p.budget)
            )
            .unwrap(),
            canonical
        );
        let limits = Limits::default();
        let authority = Authority {
            snapshot: p.snapshot.clone(),
        };
        assert!(verify_ct_artifact(p.clone(), &limits, &authority).is_ok());
        let mut duplicate = p.clone();
        duplicate
            .record_schemas
            .push(duplicate.record_schemas[0].clone());
        assert!(
            verify_ct_artifact(duplicate, &limits, &authority).unwrap_err()[0]
                .message
                .contains("duplicated")
        );
        let mut cycle = p.clone();
        cycle.record_schemas[0].arguments[0] = CtSchemaArgument::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Tuple(vec![CtTypeNodeId(0)])],
            root: CtTypeNodeId(0),
            ambient_classes: Vec::new(),
        });
        assert!(
            verify_ct_artifact(cycle, &limits, &authority).unwrap_err()[0]
                .message
                .contains("cycle")
        );
        let mut wide = p.clone();
        wide.target_pointer_width = 32;
        wide.record_schemas[0].arguments[2] = CtSchemaArgument::Scalar(Constant::Usize(u64::MAX));
        assert!(
            verify_ct_artifact(wide, &limits, &authority).unwrap_err()[0]
                .message
                .contains("width")
        );
        let run = |n| {
            let mut p = p.clone();
            p.budget.max_analysis_work = n;
            verify_ct_artifact(p, &limits, &authority)
        };
        let (mut low, mut high) = (1, p.budget.max_analysis_work);
        while low < high {
            let middle = (low + high) / 2;
            if run(middle).is_ok() {
                high = middle
            } else {
                low = middle + 1
            }
        }
        assert!(run(low).is_ok());
        assert_eq!(
            run(low - 1).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert!(run(low).is_ok());
        eprintln!("344 CT3 nominal verification N={low}/N-1/fresh");
        let key = crate::artifact_type_key(
            &canonical,
            &p.record_schemas,
            &[],
            &mut CtExecutionBudget::new(p.budget),
        )
        .unwrap();
        assert!(!key.is_empty());
    }
    #[test]
    fn anonymous_value_phantom_borrowed_graphs_keep_joint_relations() {
        fn graph(result_life: u32) -> CtTypeRef {
            let mut nodes = Vec::new();
            for _ in 0..3 {
                let inner = CtTypeNodeId(nodes.len() as u32);
                nodes.push(CtTypeNode::Primitive(CtScalarTy::I32));
                nodes.push(CtTypeNode::Address {
                    mutable: false,
                    inner,
                });
            }
            nodes.push(CtTypeNode::BorrowedFunctionPointer {
                parameters: vec![CtTypeNodeId(1), CtTypeNodeId(3)],
                result: CtTypeNodeId(5),
                life: zil_ir::StorageLifeContract::from_flat(zil_ir::FunctionLifeContract {
                    binder_count: 2,
                    parameter_lives: vec![Some(0), Some(1)],
                    result_life: Some(result_life),
                })
                .unwrap(),
            });
            CtTypeRef {
                nodes,
                root: CtTypeNodeId(6),
                ambient_classes: Vec::new(),
            }
        }
        let mut p = anonymous_value_program();
        for result in [0, 1] {
            p.record_schemas.push(CtRecordSchema {
                life_context: None,
                name: CtSymbolId {
                    declaration: NodeId(41),
                    ..symbol()
                },
                arguments: vec![CtSchemaArgument::Type(graph(result))],
                fields: vec![CtTy::Scalar(CtScalarTy::I32)],
                origin: CtSchemaOrigin::UserPure,
            });
        }
        let limits = Limits::default();
        let authority = Authority {
            snapshot: p.snapshot.clone(),
        };
        let verified = verify_ct_artifact(p.clone(), &limits, &authority).unwrap();
        let wire = zil_ir::ct_wire::encode_program(verified.program(), &limits).unwrap();
        assert_eq!(zil_ir::ct_wire::decode_program(&wire, &limits).unwrap(), p);
        let keys: Vec<_> = (0..2)
            .map(|id| {
                crate::artifact_type_key(
                    &CtTy::Record(CtRecordSchemaId(id)),
                    &p.record_schemas,
                    &[],
                    &mut CtExecutionBudget::new(p.budget),
                )
                .unwrap()
            })
            .collect();
        assert_ne!(keys[0], keys[1]);
        let canonical = |members| {
            crate::canonical_artifact_sum(
                members,
                &p.record_schemas,
                &[],
                &mut CtExecutionBudget::new(p.budget),
            )
            .unwrap()
        };
        assert_eq!(
            canonical(vec![
                CtTy::Record(CtRecordSchemaId(0)),
                CtTy::Record(CtRecordSchemaId(1))
            ]),
            canonical(vec![
                CtTy::Record(CtRecordSchemaId(1)),
                CtTy::Record(CtRecordSchemaId(0))
            ])
        );
        let mut invalid = p.clone();
        invalid.record_schemas[0].arguments[0] = CtSchemaArgument::Type(graph(2));
        assert!(verify_ct_artifact(invalid, &limits, &authority).is_err());
        let mut duplicate = p.clone();
        duplicate.record_schemas[1].arguments = duplicate.record_schemas[0].arguments.clone();
        assert!(
            verify_ct_artifact(duplicate, &limits, &authority).unwrap_err()[0]
                .message
                .contains("duplicated")
        );
        let run = |n| {
            let mut raw = p.clone();
            raw.budget.max_analysis_work = n;
            verify_ct_artifact(raw, &limits, &authority)
        };
        let (mut low, mut high) = (1, p.budget.max_analysis_work);
        while low < high {
            let middle = (low + high) / 2;
            if run(middle).is_ok() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        assert!(run(low).is_ok());
        assert_eq!(
            run(low - 1).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert!(run(low).is_ok());
        eprintln!("344 CT3 phantom BFP metadata verification N={low}/N-1/fresh");
    }
    #[test]
    fn anonymous_value_schema_three_rejects_preserved_v2_envelopes() {
        let limits = Limits::default();
        let program = include_bytes!(
            "../../../tests/language-readiness/terminal/344/anonymous/fixtures/artifact-v2-program.wire"
        );
        let value = include_bytes!(
            "../../../tests/language-readiness/terminal/344/anonymous/fixtures/artifact-v2-value.wire"
        );
        for bytes in [program.as_slice(), value.as_slice()] {
            assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 2);
        }
        assert_eq!(
            zil_ir::ct_wire::decode_program(program, &limits),
            Err(zil_ir::ct_wire::WireError::BadFingerprint)
        );
        assert_eq!(
            zil_ir::ct_wire::decode_value(value, &limits),
            Err(zil_ir::ct_wire::WireError::BadFingerprint)
        );
        let mut p = anonymous_value_program();
        p.schema_version = 2;
        let authority = Authority {
            snapshot: p.snapshot.clone(),
        };
        assert!(
            verify_ct_artifact(p, &limits, &authority).unwrap_err()[0]
                .message
                .contains("schema")
        );
    }
    #[test]
    fn anonymous_value_tag_and_legacy_inactive_empty_enum_project_cleanly() {
        let mut p = anonymous_value_program();
        let f = &mut p.functions[0];
        f.result_type = CtTy::Scalar(CtScalarTy::I32);
        f.values.push(f.result_type.clone());
        f.blocks[0].instructions.push(CtInstruction {
            result: Some(CtValueId(2)),
            kind: CtInstructionKind::VariantTag(CtValueId(1)),
            span: Span::default(),
        });
        f.blocks[0].terminator = CtTerminator::Return(Some(CtValueId(2)));
        let limits = Limits::default();
        let authority = Authority {
            snapshot: p.snapshot.clone(),
        };
        let checked = verify_ct_artifact(p.clone(), &limits, &authority).unwrap();
        assert_eq!(
            evaluate_ct_artifact(
                &checked,
                &[],
                &Reader,
                &mut CtExecutionBudget::new(p.budget)
            )
            .unwrap()
            .0,
            CtValue::Scalar(Constant::I32(1))
        );
        p.enum_schemas.push(CtEnumSchema {
            life_context: None,
            name: CtSymbolId {
                declaration: NodeId(50),
                ..symbol()
            },
            arguments: Vec::new(),
            variants: vec![Vec::new(), vec![CtTy::Scalar(CtScalarTy::I32)]],
            origin: CtSchemaOrigin::UserPure,
        });
        let f = &mut p.functions[0];
        f.values = vec![CtTy::Enum(CtEnumSchemaId(0)), CtTy::Scalar(CtScalarTy::I32)];
        f.blocks[0].instructions = vec![
            CtInstruction {
                result: Some(CtValueId(0)),
                kind: CtInstructionKind::MakeVariant {
                    schema: CtEnumSchemaId(0),
                    tag: 0,
                    fields: Vec::new(),
                },
                span: Span::default(),
            },
            CtInstruction {
                result: Some(CtValueId(1)),
                kind: CtInstructionKind::VariantField {
                    value: CtValueId(0),
                    variant: 1,
                    field: 0,
                },
                span: Span::default(),
            },
        ];
        f.blocks[0].terminator = CtTerminator::Return(Some(CtValueId(1)));
        let checked = verify_ct_artifact(p.clone(), &limits, &authority).unwrap();
        assert!(
            evaluate_ct_artifact(
                &checked,
                &[],
                &Reader,
                &mut CtExecutionBudget::new(p.budget)
            )
            .unwrap_err()[0]
                .message
                .contains("inactive")
        );
    }
}
