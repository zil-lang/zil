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
fn storage_cost(
    life: &zil_ir::StorageLifeShape,
    budget: &mut CtExecutionBudget,
) -> CompileResult<u64> {
    budget.reserve_analysis_work(
        (life.slots.len() as u64)
            .saturating_add(life.retained.len() as u64)
            .saturating_add(life.nominal_arguments.len() as u64),
    )?;
    let mut bytes = 0u64;
    for row in &life.slots {
        budget.reserve_analysis_work(row.path.len() as u64)?;
        bytes = bytes
            .saturating_add(std::mem::size_of::<zil_ir::StorageLifeSlot>() as u64)
            .saturating_add(
                (row.path.len() as u64)
                    .saturating_mul(std::mem::size_of::<zil_ir::PlaceProjection>() as u64),
            );
    }
    for row in &life.retained {
        budget.reserve_analysis_work(row.path.len() as u64)?;
        bytes = bytes
            .saturating_add(std::mem::size_of::<zil_ir::StorageRetainedBinding>() as u64)
            .saturating_add(
                (row.path.len() as u64)
                    .saturating_mul(std::mem::size_of::<zil_ir::LifeTypeProjection>() as u64),
            );
    }
    for row in &life.nominal_arguments {
        budget.reserve_analysis_work(row.path.len() as u64)?;
        bytes = bytes
            .saturating_add(std::mem::size_of::<zil_ir::NominalLifeArgument>() as u64)
            .saturating_add(
                (row.path.len() as u64)
                    .saturating_mul(std::mem::size_of::<zil_ir::LifeTypeProjection>() as u64),
            );
    }
    Ok(bytes)
}
pub(in crate::ct_artifact) fn graph_cost(
    graph: &CtTypeRef,
    budget: &mut CtExecutionBudget,
) -> CompileResult<u64> {
    budget.reserve_analysis_work(graph.ambient_classes.len() as u64)?;
    let mut bytes = (graph.ambient_classes.len() as u64)
        .saturating_mul(std::mem::size_of::<CtAmbientClassBinding>() as u64);
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
        if let CtTypeNode::BorrowedStorage { life, .. } = node {
            bytes = bytes.saturating_add(storage_cost(life, budget)?);
        }
        if let CtTypeNode::BorrowedFunctionPointer { life, .. } = node {
            bytes = bytes.saturating_add(crate::ct_artifact::scope_metadata::signature_cost(
                life, budget,
            )?);
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
        if let Some(context) = &record.life_context {
            bytes = bytes.saturating_add(graph_cost(context, budget)?);
        }
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
        if let Some(context) = &en.life_context {
            bytes = bytes.saturating_add(graph_cost(context, budget)?);
        }
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
pub(in crate::ct_artifact) fn ambient_cost(
    scopes: &[CtAmbientLifeScope],
    budget: &mut CtExecutionBudget,
) -> CompileResult<u64> {
    budget.reserve_analysis_work(scopes.len() as u64)?;
    let mut bytes =
        (scopes.len() as u64).saturating_mul(std::mem::size_of::<CtAmbientLifeScope>() as u64);
    for scope in scopes {
        budget.reserve_analysis_work(scope.binder_site.len() as u64)?;
        bytes = bytes.saturating_add(scope.binder_site.len() as u64 * 4);
        if let CtScopeInstance::Instantiated(actuals) = &scope.instance {
            budget.reserve_analysis_work(actuals.len() as u64)?;
            bytes = bytes.saturating_add(
                (actuals.len() as u64).saturating_mul(std::mem::size_of::<CtScopeActual>() as u64),
            );
            for actual in actuals {
                match actual {
                    CtScopeActual::Type(graph) => {
                        bytes = bytes.saturating_add(graph_cost(graph, budget)?)
                    }
                    CtScopeActual::Scalar(zil_ir::Constant::WideString(text)) => {
                        budget.reserve_analysis_work(text.len() as u64)?;
                        bytes = bytes.saturating_add(text.len() as u64 * 2);
                    }
                    CtScopeActual::Scalar(_) | CtScopeActual::Lifetime(_) => {}
                }
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
    let mut bytes = (std::mem::size_of::<CtArtifactProgram>() as u64)
        .saturating_add(ambient_cost(&program.ambient_life_scopes, budget)?);
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
                        if let CtTypeConstructor::BorrowedStorage { life } = constructor {
                            bytes = bytes.saturating_add(storage_cost(life, budget)?);
                        }
                        if let CtTypeConstructor::BorrowedFunctionPointer { life } = constructor {
                            bytes = bytes.saturating_add(
                                crate::ct_artifact::scope_metadata::signature_cost(life, budget)?,
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
    pub source_bindings: Vec<compiler::SourceLifeBinding>,
    pub ambient_life_scopes: Vec<CtAmbientLifeScope>,
}
#[derive(Clone)]
pub(in crate::ct_artifact) struct ValueSourceScope {
    pub owner: CtTypeNodeId,
    pub definition: CtSymbolId,
    pub parameters: Vec<u32>,
}
impl ValueContext {
    pub fn capture(
        program: &CtArtifactProgram,
        source_bindings: &[compiler::SourceLifeBinding],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Self> {
        let ty = &program.functions[program.entry.0 as usize].result_type;
        let mut bytes = schema_copy_cost(&program.record_schemas, &program.enum_schemas, budget)?
            .saturating_add(ty_cost(ty, budget)?)
            .saturating_add(ambient_cost(&program.ambient_life_scopes, budget)?);
        budget.reserve_analysis_work(source_bindings.len() as u64)?;
        for binding in source_bindings {
            budget.reserve_analysis_work(binding.parameters.len() as u64)?;
            bytes = bytes
                .saturating_add(std::mem::size_of::<compiler::SourceLifeBinding>() as u64)
                .saturating_add(binding.parameters.len() as u64 * 4);
        }
        budget.reserve_temporary_bytes(bytes)?;
        Ok(Self {
            records: program.record_schemas.clone(),
            enums: program.enum_schemas.clone(),
            ty: ty.clone(),
            bytes,
            source_bindings: source_bindings.to_vec(),
            ambient_life_scopes: program.ambient_life_scopes.clone(),
        })
    }
    pub(in crate::ct_artifact) fn type_with_scopes(
        &self,
        ty: &CtTy,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<(CtTypeRef, Vec<ValueSourceScope>)> {
        let graph = self.type_ref(ty, budget)?;
        let mut scopes = Vec::new();
        if self.source_bindings.is_empty() {
            return Ok((graph, scopes));
        }
        budget.reserve_temporary_bytes(64)?;
        let mut pending = vec![(ty, graph.root, 0usize)];
        while let Some((ty, id, depth)) = pending.pop() {
            budget.reserve_analysis_work(1)?;
            if depth > budget.remaining().max_call_depth as usize {
                return Err(bad());
            }
            let node = graph.nodes.get(id.0 as usize).ok_or_else(bad)?;
            match (ty, node) {
                (CtTy::Record(_), CtTypeNode::BorrowedStorage { .. })
                | (CtTy::Enum(_), CtTypeNode::BorrowedStorage { .. }) => {
                    let schema = match ty {
                        CtTy::Record(id) => id.0,
                        CtTy::Enum(id) => id.0,
                        _ => unreachable!(),
                    };
                    let enumeration = matches!(ty, CtTy::Enum(_));
                    budget.reserve_analysis_work(self.source_bindings.len() as u64)?;
                    let binding = self
                        .source_bindings
                        .iter()
                        .find(|b| b.enumeration == enumeration && b.schema == schema)
                        .ok_or_else(bad)?;
                    budget.reserve_analysis_work(binding.parameters.len() as u64)?;
                    budget.reserve_temporary_bytes(
                        (std::mem::size_of::<ValueSourceScope>() as u64).saturating_mul(2)
                            + binding.parameters.len() as u64 * 4,
                    )?;
                    scopes.push(ValueSourceScope {
                        owner: id,
                        definition: binding.definition.clone(),
                        parameters: binding.parameters.clone(),
                    });
                }
                (CtTy::Tuple(parts), CtTypeNode::Tuple(ids))
                | (CtTy::AnonymousSum(parts), CtTypeNode::AnonymousSum(ids)) => {
                    if parts.len() != ids.len() {
                        return Err(bad());
                    }
                    budget.reserve_analysis_work(parts.len() as u64)?;
                    budget.reserve_temporary_bytes(parts.len() as u64 * 64)?;
                    pending.extend(
                        parts
                            .iter()
                            .zip(ids)
                            .map(|(part, id)| (part, *id, depth + 1)),
                    );
                }
                (CtTy::Array { element, .. }, CtTypeNode::Array { element: id, .. })
                | (CtTy::List(element), CtTypeNode::Slice(id)) => {
                    budget.reserve_temporary_bytes(64)?;
                    pending.push((element, *id, depth + 1));
                }
                _ => {}
            }
        }
        Ok((graph, scopes))
    }
    pub fn type_ref(&self, ty: &CtTy, budget: &mut CtExecutionBudget) -> CompileResult<CtTypeRef> {
        if !self.ambient_life_scopes.is_empty() {
            return Err(vec![Diagnostic::unsupported(
                Phase::Ct,
                "CT5 value reification requires authenticated ambient context",
                None,
            )]);
        }
        fn append(
            out: &mut Vec<CtTypeNode>,
            mut graph: CtTypeRef,
            budget: &mut CtExecutionBudget,
        ) -> CompileResult<CtTypeNodeId> {
            if !graph.ambient_classes.is_empty() {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "CT5 value type assembly requires joint ambient relocation",
                    None,
                )]);
            }
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
                        if let Some(context) = &s.life_context {
                            let bytes = graph_cost(context, budget)?;
                            budget.reserve_generated_nodes(context.nodes.len() as u64)?;
                            budget.reserve_temporary_bytes(bytes)?;
                            return Ok(context.clone());
                        }
                        (&s.name, &s.arguments)
                    }
                    CtTy::Enum(id) => {
                        let s = self.enums.get(id.0 as usize).ok_or_else(bad)?;
                        if let Some(context) = &s.life_context {
                            let bytes = graph_cost(context, budget)?;
                            budget.reserve_generated_nodes(context.nodes.len() as u64)?;
                            budget.reserve_temporary_bytes(bytes)?;
                            return Ok(context.clone());
                        }
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
                        CtSchemaArgument::TypeInContext(_) | CtSchemaArgument::Lifetime(_) => {
                            return Err(vec![Diagnostic::unsupported(
                                Phase::Ct,
                                "CT4 scoped data reification is not yet implemented",
                                None,
                            )]);
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
        Ok(CtTypeRef {
            nodes,
            root,
            ambient_classes: Vec::new(),
        })
    }
}
