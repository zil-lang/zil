//! A returned data value stays bound to the independently verified schema tables.
use super::*;
use zil_base::{CompileResult, Diagnostic, Phase};
use zil_ct::CtExecutionBudget;
fn bad() -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-ARTIFACT",
        None,
        "returned CT data schema is absent",
    )]
}
pub(in crate::ct_artifact) fn ty_cost(
    ty: &CtTy,
    budget: &mut CtExecutionBudget,
) -> CompileResult<u64> {
    budget.reserve_analysis_work(1)?;
    let mut bytes = std::mem::size_of::<CtTy>() as u64;
    match ty {
        CtTy::Tuple(v) | CtTy::AnonymousSum(v) => {
            for t in v {
                bytes = bytes.saturating_add(ty_cost(t, budget)?);
            }
        }
        CtTy::Array { element, .. } | CtTy::List(element) => {
            bytes = bytes.saturating_add(ty_cost(element, budget)?)
        }
        _ => {}
    }
    Ok(bytes)
}
fn graph_cost(graph: &CtTypeRef, budget: &mut CtExecutionBudget) -> CompileResult<u64> {
    let mut bytes = 0u64;
    for node in &graph.nodes {
        budget.reserve_analysis_work(1)?;
        let (edges, bytes_per_edge) = match node {
            CtTypeNode::Nominal { arguments, .. } => {
                (arguments.len(), std::mem::size_of::<CtTypeArgument>())
            }
            CtTypeNode::Tuple(v) | CtTypeNode::AnonymousSum(v) => {
                (v.len(), std::mem::size_of::<CtTypeNodeId>())
            }
            CtTypeNode::FunctionPointer { parameters, .. }
            | CtTypeNode::BorrowedFunctionPointer { parameters, .. }
            | CtTypeNode::ForeignCallback { parameters, .. } => {
                (parameters.len(), std::mem::size_of::<CtTypeNodeId>())
            }
            _ => (0, 0),
        };
        budget.reserve_analysis_work(edges as u64)?;
        bytes = bytes
            .saturating_add(std::mem::size_of::<CtTypeNode>() as u64)
            .saturating_add((edges as u64).saturating_mul(bytes_per_edge as u64));
        if let CtTypeNode::BorrowedFunctionPointer { life, .. } = node {
            budget.reserve_analysis_work(life.parameter_lives.len() as u64)?;
            bytes = bytes.saturating_add((life.parameter_lives.len() as u64).saturating_mul(8));
        }
    }
    Ok(bytes)
}
pub(super) fn schema_copy_cost(
    records: &[CtRecordSchema],
    enums: &[CtEnumSchema],
    budget: &mut CtExecutionBudget,
) -> CompileResult<u64> {
    let mut bytes = 0u64;
    for record in records {
        budget.reserve_analysis_work(1)?;
        bytes = bytes.saturating_add(std::mem::size_of::<CtRecordSchema>() as u64);
        for field in &record.fields {
            bytes = bytes.saturating_add(ty_cost(field, budget)?);
        }
        for arg in &record.arguments {
            budget.reserve_analysis_work(1)?;
            bytes = bytes.saturating_add(std::mem::size_of::<CtSchemaArgument>() as u64);
            if let CtSchemaArgument::Type(graph) = arg {
                bytes = bytes.saturating_add(graph_cost(graph, budget)?);
            }
        }
    }
    for en in enums {
        budget.reserve_analysis_work(1)?;
        bytes = bytes.saturating_add(std::mem::size_of::<CtEnumSchema>() as u64);
        for fields in &en.variants {
            bytes = bytes.saturating_add(24);
            for field in fields {
                bytes = bytes.saturating_add(ty_cost(field, budget)?);
            }
        }
        for arg in &en.arguments {
            budget.reserve_analysis_work(1)?;
            bytes = bytes.saturating_add(std::mem::size_of::<CtSchemaArgument>() as u64);
            if let CtSchemaArgument::Type(graph) = arg {
                bytes = bytes.saturating_add(graph_cost(graph, budget)?);
            }
        }
    }
    Ok(bytes)
}
pub(super) fn program_copy_cost(
    program: &CtArtifactProgram,
    budget: &mut CtExecutionBudget,
) -> CompileResult<u64> {
    fn ids(count: usize, budget: &mut CtExecutionBudget) -> CompileResult<u64> {
        budget.reserve_analysis_work(count as u64)?;
        Ok((count as u64).saturating_mul(4))
    }
    fn text(value: &str, budget: &mut CtExecutionBudget) -> CompileResult<u64> {
        budget.reserve_analysis_work(value.len() as u64)?;
        Ok(value.len() as u64)
    }
    let mut bytes = std::mem::size_of::<CtArtifactProgram>() as u64;
    bytes = bytes.saturating_add(schema_copy_cost(
        &program.record_schemas,
        &program.enum_schemas,
        budget,
    )?);
    for function in &program.functions {
        budget.reserve_analysis_work(1)?;
        bytes = bytes
            .saturating_add(std::mem::size_of::<CtFunction>() as u64)
            .saturating_add(ids(function.parameters.len(), budget)?);
        for ty in function
            .parameter_types
            .iter()
            .chain(&function.locals)
            .chain(&function.values)
            .chain(std::iter::once(&function.result_type))
        {
            bytes = bytes.saturating_add(ty_cost(ty, budget)?);
        }
        for block in &function.blocks {
            budget.reserve_analysis_work(1)?;
            bytes = bytes.saturating_add(std::mem::size_of::<CtBlock>() as u64);
            for instruction in &block.instructions {
                budget.reserve_analysis_work(1)?;
                bytes = bytes.saturating_add(std::mem::size_of::<CtInstruction>() as u64);
                let operands = match &instruction.kind {
                    CtInstructionKind::MakeTuple(v)
                    | CtInstructionKind::MakeArray(v)
                    | CtInstructionKind::MakeList(v)
                    | CtInstructionKind::MakeRecord { fields: v, .. }
                    | CtInstructionKind::MakeVariant { fields: v, .. }
                    | CtInstructionKind::Call { arguments: v, .. }
                    | CtInstructionKind::TypeApply { arguments: v, .. }
                    | CtInstructionKind::Quote { holes: v, .. }
                    | CtInstructionKind::Text { arguments: v, .. }
                    | CtInstructionKind::SyntaxBuild { arguments: v, .. }
                    | CtInstructionKind::Query { arguments: v, .. } => v.len(),
                    CtInstructionKind::TypeConstruct {
                        constructor,
                        arguments,
                    } => {
                        if let CtTypeConstructor::BorrowedFunctionPointer { life } = constructor {
                            budget.reserve_analysis_work(life.parameter_lives.len() as u64)?;
                            bytes = bytes.saturating_add(
                                (life.parameter_lives.len() as u64).saturating_mul(8),
                            );
                        }
                        arguments.len()
                    }
                    CtInstructionKind::MakeAnonymousSum { members, .. } => {
                        for ty in members {
                            bytes = bytes.saturating_add(ty_cost(ty, budget)?);
                        }
                        0
                    }
                    _ => 0,
                };
                bytes = bytes.saturating_add(ids(operands, budget)?);
            }
        }
    }
    for constant in &program.constants {
        budget.reserve_analysis_work(1)?;
        bytes = bytes.saturating_add(std::mem::size_of::<CtConstant>() as u64);
        if let CtConstant::Text(value) = constant {
            bytes = bytes.saturating_add(text(value, budget)?);
        }
    }
    for template in &program.templates {
        budget.reserve_analysis_work(1)?;
        bytes = bytes
            .saturating_add(std::mem::size_of::<CtTemplate>() as u64)
            .saturating_add(ids(template.syntax.roots.len(), budget)?)
            .saturating_add(
                (template.syntax.origins.len() as u64)
                    .saturating_mul(std::mem::size_of::<CtOrigin>() as u64),
            )
            .saturating_add(
                (template.holes.len() as u64)
                    .saturating_mul(std::mem::size_of::<CtHoleKind>() as u64),
            );
        budget.reserve_analysis_work(
            (template.syntax.origins.len() as u64).saturating_add(template.holes.len() as u64),
        )?;
        for node in &template.syntax.nodes {
            budget.reserve_analysis_work(1)?;
            bytes = bytes.saturating_add(std::mem::size_of::<CtSyntaxNode>() as u64);
            match node {
                CtSyntaxNode::Group { children, .. } => {
                    bytes = bytes.saturating_add(ids(children.len(), budget)?)
                }
                CtSyntaxNode::Token(token) => {
                    bytes = bytes.saturating_add(text(&token.text, budget)?);
                    if let Some(name) = &token.name {
                        bytes = bytes.saturating_add(text(&name.text, budget)?);
                        if let Some(graph) = &name.member_owner_type {
                            bytes = bytes.saturating_add(graph_cost(graph, budget)?);
                        }
                    }
                }
                CtSyntaxNode::Hole { .. } => {}
            }
        }
    }
    for token in &program.input_tokens.tokens {
        budget.reserve_analysis_work(1)?;
        bytes = bytes
            .saturating_add(std::mem::size_of::<CtInputToken>() as u64)
            .saturating_add(text(&token.text, budget)?);
    }
    budget.reserve_analysis_work(program.snapshot.requested_queries.len() as u64)?;
    Ok(bytes.saturating_add(program.snapshot.requested_queries.len() as u64))
}
pub(in crate::ct_artifact) struct ValueContext {
    pub records: Vec<CtRecordSchema>,
    pub enums: Vec<CtEnumSchema>,
    pub ty: CtTy,
    pub bytes: u64,
}
impl ValueContext {
    pub fn capture(
        program: &CtArtifactProgram,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Self> {
        let ty = &program.functions[program.entry.0 as usize].result_type;
        let bytes = schema_copy_cost(&program.record_schemas, &program.enum_schemas, budget)?
            .saturating_add(ty_cost(ty, budget)?);
        budget.reserve_temporary_bytes(bytes)?;
        Ok(Self {
            records: program.record_schemas.clone(),
            enums: program.enum_schemas.clone(),
            ty: ty.clone(),
            bytes,
        })
    }
    pub fn type_ref(&self, ty: &CtTy, budget: &mut CtExecutionBudget) -> CompileResult<CtTypeRef> {
        fn append(
            out: &mut Vec<CtTypeNode>,
            mut graph: CtTypeRef,
            budget: &mut CtExecutionBudget,
        ) -> CompileResult<CtTypeNodeId> {
            budget.reserve_analysis_work(graph.nodes.len() as u64)?;
            budget.reserve_generated_nodes(graph.nodes.len() as u64)?;
            budget.reserve_temporary_bytes((graph.nodes.len() as u64).saturating_mul(128))?;
            let offset = u32::try_from(out.len()).map_err(|_| bad())?;
            for node in &mut graph.nodes {
                let mut invalid = false;
                compiler::metadata::ids(node, |id| {
                    if let Some(n) = id.0.checked_add(offset) {
                        id.0 = n
                    } else {
                        invalid = true
                    }
                });
                if invalid {
                    return Err(bad());
                }
            }
            let root = CtTypeNodeId(graph.root.0.checked_add(offset).ok_or_else(bad)?);
            out.extend(graph.nodes);
            Ok(root)
        }
        budget.reserve_analysis_work(1)?;
        let mut nodes = Vec::new();
        let node = match ty {
            CtTy::Scalar(s) => CtTypeNode::Primitive(*s),
            CtTy::Tuple(parts) | CtTy::AnonymousSum(parts) => {
                let mut ids = Vec::new();
                for part in parts {
                    ids.push(append(&mut nodes, self.type_ref(part, budget)?, budget)?);
                }
                if matches!(ty, CtTy::Tuple(_)) {
                    CtTypeNode::Tuple(ids)
                } else {
                    CtTypeNode::AnonymousSum(ids)
                }
            }
            CtTy::Array { element, length } => CtTypeNode::Array {
                element: append(&mut nodes, self.type_ref(element, budget)?, budget)?,
                length: *length,
            },
            CtTy::List(element) => {
                CtTypeNode::Slice(append(&mut nodes, self.type_ref(element, budget)?, budget)?)
            }
            CtTy::Record(_) | CtTy::Enum(_) => {
                let (symbol, args) = match ty {
                    CtTy::Record(id) => {
                        let s = self.records.get(id.0 as usize).ok_or_else(bad)?;
                        if s.life_context.is_some() { return Err(vec![Diagnostic::unsupported(Phase::Ct, "CT4 joint context extraction is not yet implemented", None)]); }
                        (&s.name, &s.arguments)
                    }
                    CtTy::Enum(id) => {
                        let s = self.enums.get(id.0 as usize).ok_or_else(bad)?;
                        if s.life_context.is_some() { return Err(vec![Diagnostic::unsupported(Phase::Ct, "CT4 joint context extraction is not yet implemented", None)]); }
                        (&s.name, &s.arguments)
                    }
                    _ => unreachable!(),
                };
                let mut arguments = Vec::new();
                for arg in args {
                    budget.reserve_analysis_work(1)?;
                    match arg {
                        CtSchemaArgument::Type(graph) => {
                            let bytes = graph_cost(graph, budget)?;
                            budget.reserve_temporary_bytes(bytes)?;
                            arguments.push(CtTypeArgument::Type(append(
                                &mut nodes,
                                graph.clone(),
                                budget,
                            )?));
                            budget.release_temporary_bytes(bytes);
                        }
                        CtSchemaArgument::TypeInContext(_) | CtSchemaArgument::Lifetime(_) => return Err(vec![Diagnostic::unsupported(Phase::Ct, "CT4 scoped data reification is not yet implemented", None)]),
                        CtSchemaArgument::Scalar(value) => {
                            arguments.push(CtTypeArgument::Scalar(value.clone()))
                        }
                    }
                }
                CtTypeNode::Nominal {
                    symbol: symbol.clone(),
                    arguments,
                }
            }
            _ => return Err(bad()),
        };
        budget.reserve_generated_nodes(1)?;
        budget.reserve_temporary_bytes(128)?;
        let root = CtTypeNodeId(u32::try_from(nodes.len()).map_err(|_| bad())?);
        nodes.push(node);
        Ok(CtTypeRef { nodes, root })
    }
}
