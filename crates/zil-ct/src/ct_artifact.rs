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
    values: Vec<Option<CtValue>>,
    block: usize,
    instruction: usize,
    destination: Option<CtValueId>,
}
impl<'a> Frame<'a> {
    fn new(
        function: &'a CtFunction,
        arguments: Vec<CtValue>,
        destination: Option<CtValueId>,
    ) -> Self {
        let mut locals = vec![None; function.locals.len()];
        for (id, value) in function.parameters.iter().zip(arguments) {
            locals[id.0 as usize] = Some(value);
        }
        Self {
            function,
            locals,
            values: vec![None; function.values.len()],
            block: function.entry.0 as usize,
            instruction: 0,
            destination,
        }
    }
    fn value(&self, id: CtValueId) -> &CtValue {
        self.values[id.0 as usize]
            .as_ref()
            .expect("verified CT value dominates use")
    }
}

fn scalar_ty(value: &Constant) -> Option<CtScalarTy> {
    match value {
        Constant::Unit => Some(CtScalarTy::Unit),
        Constant::Bool(_) => Some(CtScalarTy::Bool),
        Constant::U8(_) => Some(CtScalarTy::U8),
        Constant::U16(_) => Some(CtScalarTy::U16),
        Constant::I32(_) => Some(CtScalarTy::I32),
        Constant::I64(_) => Some(CtScalarTy::I64),
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
    }
}
fn integer_index(value: &CtValue, len: usize) -> CompileResult<usize> {
    let n = match value {
        CtValue::Scalar(Constant::U8(n)) => *n as i64,
        CtValue::Scalar(Constant::U16(n)) => *n as i64,
        CtValue::Scalar(Constant::I32(n)) => *n as i64,
        CtValue::Scalar(Constant::I64(n)) => *n,
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
    let raw = program.program();
    let entry = &raw.functions[raw.entry.0 as usize];
    if arguments.len() != entry.parameter_types.len() {
        return Err(error("CT invocation argument count differs"));
    }
    for (index, (value, ty)) in arguments.iter().zip(&entry.parameter_types).enumerate() {
        if !value_matches_type(value, ty, raw, 0) {
            return Err(error("CT invocation argument type differs"));
        }
        reader.authenticate_argument(index as u32, ty, value)?;
    }
    budget.reserve_instance()?;
    budget.observe_depth(1)?;
    let mut frames = vec![Frame::new(entry, arguments.to_vec(), None)];
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
                let args = arguments.iter().map(|v| frame.value(*v).clone()).collect();
                frames.push(Frame::new(target, args, inst.result));
                continue;
            }
            let produced = execute_instruction(raw, frame, inst, arguments, reader, budget)?;
            if let Some(id) = inst.result {
                let value = produced.expect("verified CT value instruction has result");
                if !value_matches_type(&value, &frame.function.values[id.0 as usize], raw, 0) {
                    return Err(error("CT read view returned a value of the wrong type"));
                }
                frame.values[id.0 as usize] = Some(value);
            }
        } else {
            match block.terminator {
                CtTerminator::Return(result) => {
                    let output = result
                        .map(|id| frame.value(id).clone())
                        .unwrap_or(CtValue::Scalar(Constant::Unit));
                    let completed = frames.pop().expect("active CT frame");
                    if let Some(parent) = frames.last_mut() {
                        if let Some(id) = completed.destination {
                            parent.values[id.0 as usize] = Some(output);
                        }
                    } else {
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

fn value_matches_type(value: &CtValue, ty: &CtTy, raw: &CtArtifactProgram, depth: usize) -> bool {
    if depth > raw.budget.max_call_depth as usize {
        return false;
    }
    match (value, ty) {
        (CtValue::Scalar(c), CtTy::Scalar(t)) => scalar_ty(c) == Some(*t),
        (CtValue::Text(v), CtTy::Text) => v.len() as u64 <= raw.budget.max_temporary_bytes,
        (CtValue::Type(v), CtTy::Type) => type_ref_valid(v, raw),
        (CtValue::Ident(v), CtTy::Ident) => {
            !v.text.is_empty() && v.text.len() as u64 <= raw.budget.max_temporary_bytes
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
        (CtValue::Tuple(v), CtTy::Tuple(t)) => {
            v.len() == t.len()
                && v.iter()
                    .zip(t)
                    .all(|(v, t)| value_matches_type(v, t, raw, depth + 1))
        }
        (CtValue::Array(v), CtTy::Array { element, length }) => {
            v.len() == *length as usize
                && v.iter()
                    .all(|v| value_matches_type(v, element, raw, depth + 1))
        }
        (CtValue::List(v), CtTy::List(element)) => {
            v.len() as u64 <= raw.budget.max_generated_nodes
                && v.iter()
                    .all(|v| value_matches_type(v, element, raw, depth + 1))
        }
        (CtValue::Record { schema, fields }, CtTy::Record(id)) => {
            schema == id
                && raw.record_schemas.get(id.0 as usize).is_some_and(|s| {
                    fields.len() == s.fields.len()
                        && fields
                            .iter()
                            .zip(&s.fields)
                            .all(|(v, t)| value_matches_type(v, t, raw, depth + 1))
                })
        }
        (
            CtValue::Variant {
                schema,
                tag,
                fields,
            },
            CtTy::Enum(id),
        ) => {
            schema == id
                && raw
                    .enum_schemas
                    .get(id.0 as usize)
                    .and_then(|s| s.variants.get(*tag as usize))
                    .is_some_and(|ts| {
                        fields.len() == ts.len()
                            && fields
                                .iter()
                                .zip(ts)
                                .all(|(v, t)| value_matches_type(v, t, raw, depth + 1))
                    })
        }
        _ => false,
    }
}

fn type_ref_valid(reference: &CtTypeRef, raw: &CtArtifactProgram) -> bool {
    if reference.nodes.len() as u64 > raw.budget.max_generated_nodes
        || reference.root.0 as usize >= reference.nodes.len()
    {
        return false;
    }
    for (index, node) in reference.nodes.iter().enumerate() {
        let child = |id: CtTypeNodeId| (id.0 as usize) < index;
        let good = match node {
            CtTypeNode::Primitive(_) | CtTypeNode::Builtin(_) | CtTypeNode::Opaque { .. } => true,
            CtTypeNode::Nominal { arguments, .. } => arguments.iter().all(|a| match a {
                CtTypeArgument::Type(id) => child(*id),
                CtTypeArgument::Scalar(c) => scalar_ty(c).is_some(),
            }),
            CtTypeNode::Tuple(v) => v.iter().all(|id| child(*id)),
            CtTypeNode::Array { element, .. }
            | CtTypeNode::Slice(element)
            | CtTypeNode::Managed(element)
            | CtTypeNode::Desc(element)
            | CtTypeNode::OwnedStore(element) => child(*element),
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
        || (!matches!(
            tree.category,
            CtSyntaxCategory::StmtList | CtSyntaxCategory::ItemList
        ) && tree.roots.len() != 1)
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

fn int(value: &CtValue) -> i64 {
    match value {
        CtValue::Scalar(Constant::U8(v)) => *v as i64,
        CtValue::Scalar(Constant::U16(v)) => *v as i64,
        CtValue::Scalar(Constant::I32(v)) => *v as i64,
        CtValue::Scalar(Constant::I64(v)) => *v,
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
    let offset = u32::try_from(out.len()).map_err(|_| limit("CT type node count overflow"))?;
    for node in &source.nodes {
        let plus = |id: CtTypeNodeId| -> CompileResult<CtTypeNodeId> {
            Ok(CtTypeNodeId(
                id.0.checked_add(offset)
                    .ok_or_else(|| limit("CT type node ID overflow"))?,
            ))
        };
        let mapped = match node {
            CtTypeNode::Primitive(s) => CtTypeNode::Primitive(*s),
            CtTypeNode::Builtin(b) => CtTypeNode::Builtin(*b),
            CtTypeNode::Nominal { symbol, arguments } => CtTypeNode::Nominal {
                symbol: symbol.clone(),
                arguments: arguments
                    .iter()
                    .map(|a| match a {
                        CtTypeArgument::Type(id) => plus(*id).map(CtTypeArgument::Type),
                        CtTypeArgument::Scalar(c) => Ok(CtTypeArgument::Scalar(c.clone())),
                    })
                    .collect::<CompileResult<Vec<_>>>()?,
            },
            CtTypeNode::Tuple(v) => CtTypeNode::Tuple(
                v.iter()
                    .copied()
                    .map(plus)
                    .collect::<CompileResult<Vec<_>>>()?,
            ),
            CtTypeNode::Array { element, length } => CtTypeNode::Array {
                element: plus(*element)?,
                length: *length,
            },
            CtTypeNode::FunctionPointer { parameters, result } => CtTypeNode::FunctionPointer {
                parameters: parameters
                    .iter()
                    .copied()
                    .map(plus)
                    .collect::<CompileResult<Vec<_>>>()?,
                result: plus(*result)?,
            },
            CtTypeNode::Address { mutable, inner } => CtTypeNode::Address {
                mutable: *mutable,
                inner: plus(*inner)?,
            },
            CtTypeNode::Slice(inner) => CtTypeNode::Slice(plus(*inner)?),
            CtTypeNode::Managed(inner) => CtTypeNode::Managed(plus(*inner)?),
            CtTypeNode::Desc(inner) => CtTypeNode::Desc(plus(*inner)?),
            CtTypeNode::OwnedStore(inner) => CtTypeNode::OwnedStore(plus(*inner)?),
            CtTypeNode::ForeignCallback { parameters, result } => CtTypeNode::ForeignCallback {
                parameters: parameters
                    .iter()
                    .copied()
                    .map(plus)
                    .collect::<CompileResult<Vec<_>>>()?,
                result: plus(*result)?,
            },
            CtTypeNode::Opaque { identity } => CtTypeNode::Opaque {
                identity: identity.clone(),
            },
            CtTypeNode::ErrorUnion { error, result } => CtTypeNode::ErrorUnion {
                error: plus(*error)?,
                result: plus(*result)?,
            },
        };
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
    kind: CtTypeConstructor,
    args: &[CtValue],
    budget: &mut CtExecutionBudget,
) -> CompileResult<CtTypeRef> {
    budget.reserve_generated_nodes(args.len() as u64 + 1)?;
    let mut nodes = Vec::new();
    let mut children = Vec::new();
    for arg in args {
        if let CtValue::Type(value) = arg {
            budget.reserve_temporary_bytes((value.nodes.len() as u64).saturating_mul(48))?;
            children.push(append_type(&mut nodes, value)?);
        }
    }
    let last = u32::try_from(nodes.len()).map_err(|_| limit("CT type node count overflow"))?;
    let node = match kind {
        CtTypeConstructor::Array => {
            let length = u32::try_from(int(&args[1]))
                .map_err(|_| error("CT array type length is outside u32"))?;
            CtTypeNode::Array {
                element: children[0],
                length,
            }
        }
        CtTypeConstructor::Tuple => CtTypeNode::Tuple(children),
        CtTypeConstructor::FunctionPointer => {
            let result = children.pop().expect("verified function Type result");
            CtTypeNode::FunctionPointer {
                parameters: children,
                result,
            }
        }
        CtTypeConstructor::Address { mutable } => CtTypeNode::Address {
            mutable,
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
    Ok(CtTypeRef {
        nodes,
        root: CtTypeNodeId(last),
    })
}
fn project_meta(
    value: &CtValue,
    projection: CtMetaProjection,
    requester: &CtSymbolId,
    reader: &impl CtReadView,
    budget: &mut CtExecutionBudget,
) -> CompileResult<CtValue> {
    use CtMetaProjection as P;
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
        (CtValue::FieldInfo(f), P::FieldName) => Ok(CtValue::Ident(f.name.clone())),
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
    let output = match &inst.kind {
        K::Constant(id) => match &raw.constants[id.0 as usize] {
            CtConstant::Scalar(c) => Some(CtValue::Scalar(c.clone())),
            CtConstant::Text(t) => {
                budget.reserve_temporary_bytes(t.len() as u64)?;
                Some(CtValue::Text(t.clone()))
            }
        },
        K::InvocationInput { index } => Some(invocation[*index as usize].clone()),
        K::StorageLive(id) | K::StorageDead(id) => {
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
            frame.locals[local.0 as usize] = Some(get(*value).clone());
            None
        }
        K::ConvertInteger { value, target } => {
            let CtValue::Scalar(c) = get(*value) else {
                unreachable!("verified scalar conversion")
            };
            Some(CtValue::Scalar(crate::convert_integer(
                c,
                &ordinary_scalar(*target),
            )?))
        }
        K::Unary { op, operand } => {
            let CtValue::Scalar(c) = get(*operand) else {
                unreachable!("verified scalar unary")
            };
            Some(CtValue::Scalar(crate::unary(*op, c)?))
        }
        K::Binary { op, left, right } => {
            let a = get(*left);
            let b = get(*right);
            let result = match (a, b) {
                (CtValue::Scalar(a), CtValue::Scalar(b)) => crate::binary(*op, a, b)?,
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
            let CtValue::Variant { tag, .. } = get(*id) else {
                unreachable!("verified enum")
            };
            Some(CtValue::Scalar(Constant::I32(
                i32::try_from(*tag).map_err(|_| limit("CT variant tag exceeds i32"))?,
            )))
        }
        K::VariantField {
            value,
            variant,
            field,
        } => {
            let CtValue::Variant { tag, fields, .. } = get(*value) else {
                unreachable!("verified enum")
            };
            if tag != variant {
                return Err(error("CT inactive variant projection reached"));
            }
            Some(fields[*field as usize].clone())
        }
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
        })),
        K::TypeBuiltin(b) => Some(CtValue::Type(CtTypeRef {
            nodes: vec![CtTypeNode::Builtin(*b)],
            root: CtTypeNodeId(0),
        })),
        K::TypeName(symbol) => Some(CtValue::Type(reader.type_name(
            &frame.function.definition,
            symbol,
            budget,
        )?)),
        K::TypeApply { nominal, arguments } => {
            let args: Vec<_> = arguments.iter().map(|id| get(*id).clone()).collect();
            Some(CtValue::Type(reader.type_apply(
                &frame.function.definition,
                nominal,
                &args,
                budget,
            )?))
        }
        K::TypeConstruct {
            constructor,
            arguments,
        } => {
            let args: Vec<_> = arguments.iter().map(|id| get(*id).clone()).collect();
            Some(CtValue::Type(construct_type(*constructor, &args, budget)?))
        }
        K::Quote { template, holes } => {
            let args: Vec<_> = holes.iter().map(|id| get(*id).clone()).collect();
            let template = &raw.templates[template.0 as usize];
            for (hole, value) in template.holes.iter().zip(&args) {
                if let CtHoleKind::Ident(namespace) = hole {
                    let CtValue::Ident(name) = value else {
                        unreachable!("verified Ident hole")
                    };
                    if name.namespace != *namespace {
                        return Err(error("CT quote name hole has wrong namespace"));
                    }
                }
            }
            Some(CtValue::Syntax(
                reader.instantiate_quote(template, &args, budget)?,
            ))
        }
        K::Parse { category, input } => {
            let CtValue::TokenTree(tree) = get(*input) else {
                unreachable!("verified TokenTree")
            };
            Some(CtValue::Syntax(reader.parse(*category, tree, budget)?))
        }
        K::TokenInspect {
            source,
            projection,
            index,
        } => {
            let number = index.map(|id| int(get(id)));
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
                    budget.reserve_temporary_bytes(fragment.len() as u64)?;
                    CtValue::Text(fragment.to_owned())
                }
                T::Concatenate => {
                    let b = text(get(arguments[1]));
                    budget.reserve_temporary_bytes(a.len().saturating_add(b.len()) as u64)?;
                    CtValue::Text(format!("{a}{b}"))
                }
            })
        }
        K::SyntaxBuild { kind, arguments } => {
            let args: Vec<_> = arguments.iter().map(|id| get(*id).clone()).collect();
            Some(CtValue::Syntax(reader.build_syntax(*kind, &args, budget)?))
        }
        K::NameBuild { kind, text: source } => {
            let label = text(get(*source));
            budget.reserve_generated_nodes(1)?;
            budget.reserve_temporary_bytes(label.len() as u64)?;
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
                namespace: CtNameNamespace::Value,
                source,
                member_owner: None,
            }))
        }
        K::Query { kind, arguments } => {
            let args: Vec<_> = arguments.iter().map(|id| get(*id).clone()).collect();
            let (answer, dependency) =
                reader.query(&frame.function.definition, *kind, &args, budget)?;
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
        fn source_fingerprint(&self) -> Fingerprint {
            Fingerprint::ZERO
        }
        fn snapshot(&self) -> &CtSnapshotRequest {
            &self.snapshot
        }
        fn authenticate_input_table(&self, input: &CtInputTable) -> bool {
            input.tokens.is_empty()
        }
        fn authorize_symbol(&self, _: &CtSymbolId, _: &CtSymbolId, _: CtSymbolUsage) -> bool {
            true
        }
        fn authorize_query(&self, _: &CtSymbolId, _: CtQueryKind) -> bool {
            false
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
                max_temporary_bytes: 1024,
                max_generated_nodes: 100,
                max_instances: 10,
                max_analysis_work: 100,
            },
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
}
