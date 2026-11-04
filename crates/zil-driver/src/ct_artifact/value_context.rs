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
        let edges = match node {
            CtTypeNode::Nominal { arguments, .. } => arguments.len(),
            CtTypeNode::Tuple(v) | CtTypeNode::AnonymousSum(v) => v.len(),
            CtTypeNode::FunctionPointer { parameters, .. }
            | CtTypeNode::BorrowedFunctionPointer { parameters, .. }
            | CtTypeNode::ForeignCallback { parameters, .. } => parameters.len().saturating_add(1),
            _ => 2,
        };
        budget.reserve_analysis_work(edges as u64)?;
        bytes = bytes
            .saturating_add(std::mem::size_of::<CtTypeNode>() as u64)
            .saturating_add((edges as u64).saturating_mul(16));
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
            bytes = bytes.saturating_add(32);
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
            bytes = bytes.saturating_add(32);
            if let CtSchemaArgument::Type(graph) = arg {
                bytes = bytes.saturating_add(graph_cost(graph, budget)?);
            }
        }
    }
    Ok(bytes)
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
                        (&s.name, &s.arguments)
                    }
                    CtTy::Enum(id) => {
                        let s = self.enums.get(id.0 as usize).ok_or_else(bad)?;
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
