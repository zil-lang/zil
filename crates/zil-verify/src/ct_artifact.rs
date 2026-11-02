//! Independent admission for the CT-only artifact. Raw IDs grant no authority.
use std::collections::{BTreeSet, VecDeque};

use zil_base::{CompileResult, Diagnostic, Fingerprint, Limits, Phase, Span};
use zil_ir::ct_artifact::*;
use zil_ir::{BinaryOp, Constant, UnaryOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtSymbolUsage {
    FunctionDefinition,
    Call,
    TypeName,
    TypeApply,
    Schema,
    QuoteDefinition,
    DefinitionName,
}

/// The driver implements this using one immutable project/source snapshot.
/// Implementations do not run inside a provider or supply verified code.
pub trait CtArtifactAuthority {
    fn source_fingerprint(&self) -> Fingerprint;
    fn snapshot(&self) -> &CtSnapshotRequest;
    fn authenticate_input_table(&self, input: &CtInputTable) -> bool;
    /// Bind all referenced constants/templates to the trusted source compiler
    /// output. Function-body equality alone does not protect indexed tables.
    fn authenticate_artifact(&self, artifact: &CtArtifactProgram) -> bool;
    /// Compare with a trusted compilation of the frozen source definition;
    /// a raw body cannot inherit another definition's private query rights.
    fn authenticate_function(&self, function: &CtFunction) -> bool;
    fn authorize_symbol(
        &self,
        requester: &CtSymbolId,
        symbol: &CtSymbolId,
        usage: CtSymbolUsage,
    ) -> bool;
    fn authorize_query(&self, requester: &CtSymbolId, kind: CtQueryKind) -> bool;
}

/// Private fields prevent construction from raw CT data or ordinary Verify.
#[derive(Clone, Debug)]
pub struct VerifiedCTArtifactProgram {
    raw: CtArtifactProgram,
}
impl VerifiedCTArtifactProgram {
    pub fn program(&self) -> &CtArtifactProgram {
        &self.raw
    }
}

fn reject(span: Option<Span>, reason: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-ARTIFACT", span, reason)]
}
fn limit(span: Option<Span>, reason: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::limit(Phase::Ct, reason, span)]
}
type Check<T> = Result<T, Vec<Diagnostic>>;

pub fn verify_ct_artifact(
    raw: CtArtifactProgram,
    limits: &Limits,
    authority: &impl CtArtifactAuthority,
) -> CompileResult<VerifiedCTArtifactProgram> {
    preflight(&raw, limits)?;
    if !authority.authenticate_artifact(&raw)
        || raw.source_fingerprint != authority.source_fingerprint()
        || raw.snapshot != *authority.snapshot()
        || !authority.authenticate_input_table(&raw.input_tokens)
    {
        return Err(reject(
            None,
            "CT artifact source or frozen snapshot is not authenticated",
        ));
    }
    check_constants(&raw)?;
    check_templates(&raw, authority)?;
    for schema in &raw.record_schemas {
        if schema.origin != CtSchemaOrigin::UserPure
            || !authority.authorize_symbol(&schema.name, &schema.name, CtSymbolUsage::Schema)
        {
            return Err(reject(
                None,
                "CT record schema is not an authenticated pure declaration",
            ));
        }
        for ty in &schema.fields {
            check_ty(ty, &raw, limits)?;
        }
    }
    for schema in &raw.enum_schemas {
        if schema.origin != CtSchemaOrigin::UserPure
            || !authority.authorize_symbol(&schema.name, &schema.name, CtSymbolUsage::Schema)
        {
            return Err(reject(
                None,
                "CT enum schema is not an authenticated pure declaration",
            ));
        }
        for variant in &schema.variants {
            for ty in variant {
                check_ty(ty, &raw, limits)?;
            }
        }
    }
    for (index, function) in raw.functions.iter().enumerate() {
        if function.id.0 as usize != index {
            return Err(reject(
                Some(function.span),
                "CT function IDs must match their table indices",
            ));
        }
        if !authority.authenticate_function(function)
            || !authority.authorize_symbol(
                &function.definition,
                &function.definition,
                CtSymbolUsage::FunctionDefinition,
            )
        {
            return Err(reject(
                Some(function.span),
                "CT function definition is not in the frozen project",
            ));
        }
        check_function(&raw, function, limits, authority)?;
    }
    if raw.functions.get(raw.entry.0 as usize).is_none() {
        return Err(reject(None, "CT entry function is missing"));
    }
    Ok(VerifiedCTArtifactProgram { raw })
}

fn preflight(raw: &CtArtifactProgram, limits: &Limits) -> Check<()> {
    if raw.schema_version != CT_ARTIFACT_SCHEMA_VERSION {
        return Err(reject(None, "unknown CT artifact schema"));
    }
    let b = raw.budget;
    if b.max_steps == 0
        || b.max_steps > limits.max_ct_steps
        || b.max_call_depth == 0
        || b.max_call_depth as usize > limits.max_call_depth
        || b.max_live_cells == 0
        || b.max_live_cells > limits.max_values.saturating_add(limits.max_locals) as u64
        || b.max_temporary_bytes > limits.max_source_bytes as u64
        || b.max_generated_nodes > limits.max_nodes as u64
        || b.max_instances > limits.max_functions as u64
        || b.max_analysis_work == 0
        || b.max_analysis_work > limits.max_analysis_steps as u64
    {
        return Err(limit(
            None,
            "CT artifact declared budget exceeds selected limits",
        ));
    }
    let mut blocks = 0usize;
    let mut instructions = 0usize;
    let mut locals = 0usize;
    let mut values = 0usize;
    for function in &raw.functions {
        blocks = blocks.saturating_add(function.blocks.len());
        locals = locals.saturating_add(function.locals.len());
        values = values.saturating_add(function.values.len());
        for block in &function.blocks {
            instructions = instructions.saturating_add(block.instructions.len());
        }
    }
    let nodes = raw
        .templates
        .iter()
        .fold(0usize, |n, t| {
            n.saturating_add(t.syntax.nodes.len())
                .saturating_add(t.syntax.origins.len())
        })
        .saturating_add(raw.input_tokens.tokens.len());
    let bytes = raw
        .constants
        .iter()
        .fold(0usize, |n, c| match c {
            CtConstant::Text(s) => n.saturating_add(s.len()),
            CtConstant::Scalar(_) => n,
        })
        .saturating_add(
            raw.input_tokens
                .tokens
                .iter()
                .fold(0usize, |n, t| n.saturating_add(t.text.len())),
        )
        .saturating_add(raw.templates.iter().flat_map(|t| &t.syntax.nodes).fold(
            0usize,
            |n, node| match node {
                CtSyntaxNode::Token(t) => n.saturating_add(t.text.len()),
                _ => n,
            },
        ));
    if raw.functions.is_empty()
        || raw.functions.len() > limits.max_functions
        || blocks > limits.max_blocks
        || instructions > limits.max_instructions
        || locals > limits.max_locals
        || values > limits.max_values
        || nodes > limits.max_nodes
        || bytes > limits.max_source_bytes
        || raw.constants.len() > limits.max_values
        || raw
            .record_schemas
            .len()
            .saturating_add(raw.enum_schemas.len())
            > limits.max_nodes
    {
        return Err(limit(None, "CT artifact structural budget exceeded"));
    }
    Ok(())
}

fn scalar(ty: &CtTy) -> Option<CtScalarTy> {
    if let CtTy::Scalar(s) = ty {
        Some(*s)
    } else {
        None
    }
}
fn constant_type(value: &Constant) -> Option<CtScalarTy> {
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
fn integer(ty: &CtTy) -> bool {
    matches!(
        scalar(ty),
        Some(CtScalarTy::U8 | CtScalarTy::U16 | CtScalarTy::I32 | CtScalarTy::I64)
    )
}
fn index_type(ty: &CtTy) -> bool {
    integer(ty)
}
fn check_constants(raw: &CtArtifactProgram) -> Check<()> {
    for c in &raw.constants {
        match c {
            CtConstant::Scalar(s) if constant_type(s).is_some() => {}
            CtConstant::Text(t) if t.len() <= raw.budget.max_temporary_bytes as usize => {}
            _ => {
                return Err(reject(
                    None,
                    "CT constant is not a bounded pure scalar or text",
                ));
            }
        }
    }
    Ok(())
}

fn check_ident(
    raw: &CtArtifactProgram,
    requester: &CtSymbolId,
    ident: &CtIdent,
    authority: &impl CtArtifactAuthority,
    allow_export: bool,
) -> Check<()> {
    if ident.text.is_empty() || ident.text.len() > raw.budget.max_temporary_bytes as usize {
        return Err(reject(None, "CT identifier text is empty or over budget"));
    }
    match &ident.source {
        CtNameSource::Input(id) => {
            if ident.namespace == CtNameNamespace::Unbound {
                return Err(reject(None, "CT input name cannot have unbound namespace"));
            }
            let Some(token) = raw.input_tokens.tokens.get(id.0 as usize) else {
                return Err(reject(None, "CT input name token is out of range"));
            };
            if token.class != CtTokenClass::Identifier || token.text != ident.text {
                return Err(reject(
                    None,
                    "CT input name differs from authenticated token",
                ));
            }
        }
        CtNameSource::Definition(symbol) => {
            if ident.namespace == CtNameNamespace::Unbound {
                return Err(reject(
                    None,
                    "CT definition name cannot have unbound namespace",
                ));
            }
            if !authority.authorize_symbol(requester, symbol, CtSymbolUsage::DefinitionName) {
                return Err(reject(None, "CT definition-site name is not visible"));
            }
        }
        CtNameSource::Fresh { invocation, .. } => {
            if ident.namespace != CtNameNamespace::Unbound
                || *invocation != raw.input_tokens.invocation_fingerprint
            {
                return Err(reject(None, "CT fresh name belongs to another invocation"));
            }
        }
        CtNameSource::Export if allow_export && ident.namespace == CtNameNamespace::Unbound => {}
        CtNameSource::Export => {
            return Err(reject(
                None,
                "CT export intent is outside a declaration list",
            ));
        }
    }
    if ident.member_owner.is_some() != ident.member_owner_type.is_some() {
        return Err(reject(
            None,
            "CT member name needs both nominal and instantiated owner",
        ));
    }
    if let Some(owner) = &ident.member_owner {
        if ident.namespace != CtNameNamespace::Member
            || !authority.authorize_symbol(requester, owner, CtSymbolUsage::DefinitionName)
        {
            return Err(reject(None, "CT member name owner is not authenticated"));
        }
        let actual = ident
            .member_owner_type
            .as_ref()
            .expect("paired member owner type");
        if actual.root.0 as usize >= actual.nodes.len()
            || !matches!(&actual.nodes[actual.root.0 as usize], CtTypeNode::Nominal { symbol, .. } if symbol == owner)
        {
            return Err(reject(
                None,
                "CT instantiated member owner differs from declaration",
            ));
        }
    }
    Ok(())
}

fn check_templates(raw: &CtArtifactProgram, authority: &impl CtArtifactAuthority) -> Check<()> {
    for template in &raw.templates {
        if !authority.authorize_symbol(
            &template.definition,
            &template.definition,
            CtSymbolUsage::QuoteDefinition,
        ) {
            return Err(reject(None, "CT quote definition is not authenticated"));
        }
        let tree = &template.syntax;
        if !matches!(
            tree.category,
            CtSyntaxCategory::StmtList | CtSyntaxCategory::ItemList
        ) && tree.roots.len() != 1
        {
            return Err(reject(
                None,
                "CT expression/type template needs exactly one root",
            ));
        }
        for (index, origin) in tree.origins.iter().enumerate() {
            if let Some(parent) = origin.parent
                && parent.0 as usize >= index
            {
                return Err(reject(None, "CT origin parent is not an earlier node"));
            }
        }
        for root in &tree.roots {
            if root.0 as usize >= tree.nodes.len() {
                return Err(reject(None, "CT template root is out of range"));
            }
        }
        for (index, node) in tree.nodes.iter().enumerate() {
            match node {
                CtSyntaxNode::Token(token) => {
                    if token.origin.0 as usize >= tree.origins.len() {
                        return Err(reject(None, "CT token origin is out of range"));
                    }
                    if let Some(name) = &token.name {
                        check_ident(
                            raw,
                            &template.definition,
                            name,
                            authority,
                            matches!(
                                tree.category,
                                CtSyntaxCategory::StmtList | CtSyntaxCategory::ItemList
                            ),
                        )?;
                    }
                }
                CtSyntaxNode::Group {
                    children, origin, ..
                } => {
                    if origin.0 as usize >= tree.origins.len()
                        || children.iter().any(|c| c.0 as usize >= index)
                    {
                        return Err(reject(None, "CT template group is not a bounded DAG"));
                    }
                }
                CtSyntaxNode::Hole { index, origin } => {
                    if *index as usize >= template.holes.len()
                        || origin.0 as usize >= tree.origins.len()
                    {
                        return Err(reject(None, "CT quote hole is out of range"));
                    }
                }
            }
        }
    }
    Ok(())
}

fn check_ty(ty: &CtTy, raw: &CtArtifactProgram, limits: &Limits) -> Check<()> {
    let mut stack = vec![(ty, 0usize)];
    let mut visited = 0usize;
    while let Some((item, depth)) = stack.pop() {
        visited = visited.saturating_add(1);
        if depth > limits.max_call_depth || visited > limits.max_nodes {
            return Err(limit(None, "CT type shape budget exceeded"));
        }
        match item {
            CtTy::Tuple(items) => stack.extend(items.iter().map(|t| (t, depth + 1))),
            CtTy::Array { element, length } => {
                if *length as usize > limits.max_nodes {
                    return Err(limit(None, "CT array length exceeds node quota"));
                }
                stack.push((element, depth + 1));
            }
            CtTy::List(element) => stack.push((element, depth + 1)),
            CtTy::Record(id) if id.0 as usize >= raw.record_schemas.len() => {
                return Err(reject(None, "CT record type ID is out of range"));
            }
            CtTy::Enum(id) if id.0 as usize >= raw.enum_schemas.len() => {
                return Err(reject(None, "CT enum type ID is out of range"));
            }
            _ => {}
        }
    }
    Ok(())
}

fn predecessors(function: &CtFunction) -> Check<Vec<Vec<usize>>> {
    if function.blocks.is_empty() || function.entry.0 as usize >= function.blocks.len() {
        return Err(reject(
            Some(function.span),
            "CT function has no valid entry block",
        ));
    }
    let mut result = vec![Vec::new(); function.blocks.len()];
    for (index, block) in function.blocks.iter().enumerate() {
        if block.id.0 as usize != index {
            return Err(reject(
                Some(function.span),
                "CT block ID differs from table index",
            ));
        }
        let targets: Vec<CtBlockId> = match block.terminator {
            CtTerminator::Return(_) | CtTerminator::Trap => Vec::new(),
            CtTerminator::Jump(to) => vec![to],
            CtTerminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![then_block, else_block],
        };
        for target in targets {
            let Some(preds) = result.get_mut(target.0 as usize) else {
                return Err(reject(
                    Some(function.span),
                    "CT branch target is out of range",
                ));
            };
            preds.push(index);
        }
    }
    let mut seen = vec![false; result.len()];
    let mut pending = VecDeque::from([function.entry.0 as usize]);
    while let Some(index) = pending.pop_front() {
        if seen[index] {
            continue;
        }
        seen[index] = true;
        match function.blocks[index].terminator {
            CtTerminator::Jump(to) => pending.push_back(to.0 as usize),
            CtTerminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                pending.push_back(then_block.0 as usize);
                pending.push_back(else_block.0 as usize);
            }
            _ => {}
        }
    }
    if seen.iter().any(|v| !*v) {
        return Err(reject(
            Some(function.span),
            "CT CFG contains unreachable blocks",
        ));
    }
    Ok(result)
}

fn check_function(
    raw: &CtArtifactProgram,
    function: &CtFunction,
    limits: &Limits,
    authority: &impl CtArtifactAuthority,
) -> Check<()> {
    if function.parameters.len() != function.parameter_types.len() {
        return Err(reject(
            Some(function.span),
            "CT function parameter type count differs from locals",
        ));
    }
    let mut parameter_ids = BTreeSet::new();
    for (id, ty) in function.parameters.iter().zip(&function.parameter_types) {
        if !parameter_ids.insert(id.0) || function.locals.get(id.0 as usize) != Some(ty) {
            return Err(reject(
                Some(function.span),
                "CT parameter local is missing or has wrong type",
            ));
        }
    }
    check_ty(&function.result_type, raw, limits)?;
    for ty in function
        .parameter_types
        .iter()
        .chain(&function.locals)
        .chain(&function.values)
    {
        check_ty(ty, raw, limits)?;
    }
    let preds = predecessors(function)?;
    let blocks = function.blocks.len();
    let static_work = blocks
        .saturating_mul(blocks)
        .saturating_add(blocks.saturating_mul(function.locals.len()))
        .saturating_add(function.values.len())
        .saturating_add(
            function
                .blocks
                .iter()
                .map(|b| b.instructions.len())
                .sum::<usize>(),
        );
    if static_work
        > limits
            .max_analysis_steps
            .min(raw.budget.max_analysis_work as usize)
    {
        return Err(limit(
            Some(function.span),
            "CT CFG analysis matrix exceeds work budget",
        ));
    }
    let all: BTreeSet<usize> = (0..blocks).collect();
    let mut dominators = vec![all; blocks];
    dominators[function.entry.0 as usize] = BTreeSet::from([function.entry.0 as usize]);
    let mut changed = true;
    let mut iterations = 0usize;
    while changed {
        changed = false;
        iterations = iterations.saturating_add(1);
        if iterations.saturating_mul(static_work)
            > limits
                .max_analysis_steps
                .min(raw.budget.max_analysis_work as usize)
        {
            return Err(limit(
                Some(function.span),
                "CT dominance analysis budget exceeded",
            ));
        }
        for block in 0..blocks {
            if block == function.entry.0 as usize {
                continue;
            }
            let mut next = dominators[preds[block][0]].clone();
            for pred in &preds[block][1..] {
                next = next.intersection(&dominators[*pred]).copied().collect();
            }
            next.insert(block);
            if next != dominators[block] {
                dominators[block] = next;
                changed = true;
            }
        }
    }

    let mut definitions = vec![None; function.values.len()];
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            let actual = check_instruction(raw, function, instruction, authority)?;
            match (instruction.result, actual) {
                (Some(id), Some(ty)) => {
                    let Some(declared) = function.values.get(id.0 as usize) else {
                        return Err(reject(
                            Some(instruction.span),
                            "CT result ID is out of range",
                        ));
                    };
                    if declared != &ty
                        || definitions[id.0 as usize]
                            .replace((block_index, instruction_index))
                            .is_some()
                    {
                        return Err(reject(
                            Some(instruction.span),
                            "CT result type or single definition is invalid",
                        ));
                    }
                }
                (None, None) => {}
                _ => {
                    return Err(reject(
                        Some(instruction.span),
                        "CT instruction result presence is invalid",
                    ));
                }
            }
        }
    }
    if definitions.iter().any(Option::is_none) {
        return Err(reject(Some(function.span), "CT value has no definition"));
    }
    for (block_index, block) in function.blocks.iter().enumerate() {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            for value in operands(&instruction.kind) {
                check_use(
                    value,
                    block_index,
                    instruction_index,
                    &definitions,
                    &dominators,
                    instruction.span,
                )?;
            }
        }
        let at_end = block.instructions.len();
        match block.terminator {
            CtTerminator::Return(Some(value)) => {
                check_use(
                    value,
                    block_index,
                    at_end,
                    &definitions,
                    &dominators,
                    function.span,
                )?;
                if function.values[value.0 as usize] != function.result_type {
                    return Err(reject(
                        Some(function.span),
                        "CT return type does not match function result",
                    ));
                }
            }
            CtTerminator::Return(None)
                if function.result_type != CtTy::Scalar(CtScalarTy::Unit) =>
            {
                return Err(reject(
                    Some(function.span),
                    "CT non-Unit function returns without a value",
                ));
            }
            CtTerminator::Branch { condition, .. } => {
                check_use(
                    condition,
                    block_index,
                    at_end,
                    &definitions,
                    &dominators,
                    function.span,
                )?;
                if function.values[condition.0 as usize] != CtTy::Scalar(CtScalarTy::Bool) {
                    return Err(reject(
                        Some(function.span),
                        "CT branch condition is not bool",
                    ));
                }
            }
            _ => {}
        }
    }
    check_locals(
        function,
        &preds,
        limits
            .max_analysis_steps
            .min(raw.budget.max_analysis_work as usize),
    )
}

fn check_use(
    value: CtValueId,
    block: usize,
    position: usize,
    definitions: &[Option<(usize, usize)>],
    dominators: &[BTreeSet<usize>],
    span: Span,
) -> Check<()> {
    let Some(Some((defined_block, defined_at))) = definitions.get(value.0 as usize) else {
        return Err(reject(
            Some(span),
            "CT operand value is out of range or undefined",
        ));
    };
    if (*defined_block == block && *defined_at >= position)
        || (*defined_block != block && !dominators[block].contains(defined_block))
    {
        return Err(reject(
            Some(span),
            "CT value definition does not dominate use",
        ));
    }
    Ok(())
}

fn check_locals(function: &CtFunction, predecessors: &[Vec<usize>], max_work: usize) -> Check<()> {
    let n = function.locals.len();
    let entry = function.entry.0 as usize;
    let mut initial = vec![false; n];
    for p in &function.parameters {
        initial[p.0 as usize] = true;
    }
    let mut incoming = vec![vec![true; n]; function.blocks.len()];
    let mut outgoing = incoming.clone();
    incoming[entry] = initial.clone();
    let mut changed = true;
    let mut iterations = 0usize;
    while changed {
        changed = false;
        iterations = iterations.saturating_add(1);
        if iterations.saturating_mul(function.blocks.len().saturating_mul(n.max(1))) > max_work {
            return Err(limit(
                Some(function.span),
                "CT local-flow analysis budget exceeded",
            ));
        }
        for (b, block) in function.blocks.iter().enumerate() {
            let mut state = if b == entry {
                initial.clone()
            } else {
                let mut state = vec![true; n];
                for pred in &predecessors[b] {
                    for (slot, flag) in state.iter_mut().enumerate() {
                        *flag &= outgoing[*pred][slot];
                    }
                }
                state
            };
            if b != entry && incoming[b] != state {
                incoming[b] = state.clone();
                changed = true;
            }
            for instruction in &block.instructions {
                match instruction.kind {
                    CtInstructionKind::StorageLive(local)
                    | CtInstructionKind::StorageDead(local) => state[local.0 as usize] = false,
                    CtInstructionKind::Store { local, .. } => state[local.0 as usize] = true,
                    _ => {}
                }
            }
            if outgoing[b] != state {
                outgoing[b] = state;
                changed = true;
            }
        }
    }
    for (b, block) in function.blocks.iter().enumerate() {
        let mut state = incoming[b].clone();
        for instruction in &block.instructions {
            match instruction.kind {
                CtInstructionKind::Load(local) if !state[local.0 as usize] => {
                    return Err(reject(
                        Some(instruction.span),
                        "CT local may be uninitialized",
                    ));
                }
                CtInstructionKind::StorageLive(local) | CtInstructionKind::StorageDead(local) => {
                    state[local.0 as usize] = false
                }
                CtInstructionKind::Store { local, .. } => state[local.0 as usize] = true,
                _ => {}
            }
        }
    }
    Ok(())
}

fn operands(kind: &CtInstructionKind) -> Vec<CtValueId> {
    match kind {
        CtInstructionKind::Store { value, .. }
        | CtInstructionKind::ConvertInteger { value, .. }
        | CtInstructionKind::Length(value)
        | CtInstructionKind::VariantTag(value)
        | CtInstructionKind::MetaProject { value, .. } => vec![*value],
        CtInstructionKind::Unary { operand, .. } => vec![*operand],
        CtInstructionKind::Binary { left, right, .. } => vec![*left, *right],
        CtInstructionKind::MakeTuple(v)
        | CtInstructionKind::MakeArray(v)
        | CtInstructionKind::MakeList(v)
        | CtInstructionKind::Call { arguments: v, .. }
        | CtInstructionKind::TypeApply { arguments: v, .. }
        | CtInstructionKind::TypeConstruct { arguments: v, .. }
        | CtInstructionKind::Quote { holes: v, .. }
        | CtInstructionKind::Text { arguments: v, .. }
        | CtInstructionKind::SyntaxBuild { arguments: v, .. }
        | CtInstructionKind::Query { arguments: v, .. } => v.clone(),
        CtInstructionKind::MakeRecord { fields, .. }
        | CtInstructionKind::MakeVariant { fields, .. } => fields.clone(),
        CtInstructionKind::VariantField { value, .. }
        | CtInstructionKind::Parse { input: value, .. }
        | CtInstructionKind::ParseIdent { input: value }
        | CtInstructionKind::NameBuild { text: value, .. } => vec![*value],
        CtInstructionKind::ExtractField { aggregate, .. } => vec![*aggregate],
        CtInstructionKind::ReplaceField {
            aggregate, value, ..
        } => vec![*aggregate, *value],
        CtInstructionKind::Extract { aggregate, index } => vec![*aggregate, *index],
        CtInstructionKind::Replace {
            aggregate,
            index,
            value,
        } => vec![*aggregate, *index, *value],
        CtInstructionKind::TokenInspect { source, index, .. } => {
            let mut values = vec![*source];
            values.extend(*index);
            values
        }
        CtInstructionKind::Constant(_)
        | CtInstructionKind::InvocationInput { .. }
        | CtInstructionKind::StorageLive(_)
        | CtInstructionKind::StorageDead(_)
        | CtInstructionKind::Load(_)
        | CtInstructionKind::TypePrimitive(_)
        | CtInstructionKind::TypeBuiltin(_)
        | CtInstructionKind::TypeName(_) => Vec::new(),
    }
}

fn check_instruction(
    raw: &CtArtifactProgram,
    function: &CtFunction,
    instruction: &CtInstruction,
    authority: &impl CtArtifactAuthority,
) -> Check<Option<CtTy>> {
    use CtInstructionKind as K;
    let span = Some(instruction.span);
    let value = |id: CtValueId| -> Check<&CtTy> {
        function
            .values
            .get(id.0 as usize)
            .ok_or_else(|| reject(span, "CT operand value ID is out of range"))
    };
    let local = |id: CtLocalId| -> Check<&CtTy> {
        function
            .locals
            .get(id.0 as usize)
            .ok_or_else(|| reject(span, "CT local ID is out of range"))
    };
    let expect = |condition: bool, message: &'static str| -> Check<()> {
        if condition {
            Ok(())
        } else {
            Err(reject(span, message))
        }
    };
    let result = match &instruction.kind {
        K::Constant(id) => match raw.constants.get(id.0 as usize) {
            Some(CtConstant::Scalar(c)) => CtTy::Scalar(
                constant_type(c).ok_or_else(|| reject(span, "CT scalar constant is invalid"))?,
            ),
            Some(CtConstant::Text(_)) => CtTy::Text,
            None => return Err(reject(span, "CT constant index is out of range")),
        },
        K::InvocationInput { index } => {
            expect(
                function.id == raw.entry,
                "CT invocation inputs are only accessible in the entry function",
            )?;
            function
                .parameter_types
                .get(*index as usize)
                .cloned()
                .ok_or_else(|| reject(span, "CT invocation input index is out of range"))?
        }
        K::StorageLive(id) | K::StorageDead(id) => {
            local(*id)?;
            return Ok(None);
        }
        K::Load(id) => local(*id)?.clone(),
        K::Store {
            local: id,
            value: input,
        } => {
            expect(
                local(*id)? == value(*input)?,
                "CT store type differs from local",
            )?;
            return Ok(None);
        }
        K::ConvertInteger {
            value: input,
            target,
        } => {
            expect(
                integer(value(*input)?) && !matches!(target, CtScalarTy::Unit | CtScalarTy::Bool),
                "CT integer conversion has non-integer type",
            )?;
            CtTy::Scalar(*target)
        }
        K::Unary { op, operand } => {
            let ty = value(*operand)?;
            match op {
                UnaryOp::Not => expect(*ty == CtTy::Scalar(CtScalarTy::Bool), "CT not needs bool")?,
                UnaryOp::Negate | UnaryOp::BitNot => {
                    expect(integer(ty), "CT integer unary needs integer")?
                }
                _ => return Err(reject(span, "runtime-only unary operation in CT artifact")),
            }
            ty.clone()
        }
        K::Binary { op, left, right } => {
            let a = value(*left)?;
            let b = value(*right)?;
            expect(a == b, "CT binary operands have different types")?;
            use BinaryOp as B;
            match op {
                B::And | B::Or => {
                    expect(
                        *a == CtTy::Scalar(CtScalarTy::Bool),
                        "CT logical operation needs bool",
                    )?;
                    a.clone()
                }
                B::Equal | B::NotEqual => {
                    expect(
                        scalar(a).is_some() || matches!(a, CtTy::Type | CtTy::Ident | CtTy::Text),
                        "CT equality needs scalar, Type, Ident or Text",
                    )?;
                    CtTy::Scalar(CtScalarTy::Bool)
                }
                B::Less | B::LessEqual | B::Greater | B::GreaterEqual => {
                    expect(integer(a), "CT ordering needs integers")?;
                    CtTy::Scalar(CtScalarTy::Bool)
                }
                _ => {
                    expect(integer(a), "CT arithmetic needs integers")?;
                    a.clone()
                }
            }
        }
        K::MakeTuple(ids) => CtTy::Tuple(
            ids.iter()
                .map(|id| value(*id).cloned())
                .collect::<Check<Vec<_>>>()?,
        ),
        K::MakeArray(ids) => {
            if ids.is_empty() {
                let Some(CtTy::Array { element, length: 0 }) = instruction
                    .result
                    .and_then(|id| function.values.get(id.0 as usize))
                else {
                    return Err(reject(
                        span,
                        "empty CT array needs declared element type and zero length",
                    ));
                };
                return Ok(Some(CtTy::Array {
                    element: element.clone(),
                    length: 0,
                }));
            }
            let item = value(ids[0])?.clone();
            for id in ids {
                expect(*value(*id)? == item, "CT array has heterogeneous elements")?;
            }
            CtTy::Array {
                element: Box::new(item),
                length: ids.len() as u32,
            }
        }
        K::MakeList(ids) => {
            if ids.is_empty() {
                let Some(CtTy::List(element)) = instruction
                    .result
                    .and_then(|id| function.values.get(id.0 as usize))
                else {
                    return Err(reject(span, "empty CT list needs declared element type"));
                };
                return Ok(Some(CtTy::List(element.clone())));
            }
            let item = value(ids[0])?.clone();
            for id in ids {
                expect(*value(*id)? == item, "CT list has heterogeneous elements")?;
            }
            CtTy::List(Box::new(item))
        }
        K::MakeRecord { schema, fields } => {
            let shape = raw
                .record_schemas
                .get(schema.0 as usize)
                .ok_or_else(|| reject(span, "CT record schema is missing"))?;
            expect(
                shape.origin == CtSchemaOrigin::UserPure && shape.fields.len() == fields.len(),
                "CT query descriptor cannot be constructed as a record",
            )?;
            for (id, ty) in fields.iter().zip(&shape.fields) {
                expect(value(*id)? == ty, "CT record field type differs")?;
            }
            CtTy::Record(*schema)
        }
        K::MakeVariant {
            schema,
            tag,
            fields,
        } => {
            let shape = raw
                .enum_schemas
                .get(schema.0 as usize)
                .ok_or_else(|| reject(span, "CT enum schema is missing"))?;
            let members = shape
                .variants
                .get(*tag as usize)
                .ok_or_else(|| reject(span, "CT enum variant is missing"))?;
            expect(
                shape.origin == CtSchemaOrigin::UserPure && members.len() == fields.len(),
                "CT query descriptor cannot be constructed as a variant",
            )?;
            for (id, ty) in fields.iter().zip(members) {
                expect(value(*id)? == ty, "CT variant field type differs")?;
            }
            CtTy::Enum(*schema)
        }
        K::Length(id) => {
            expect(
                matches!(
                    value(*id)?,
                    CtTy::Array { .. } | CtTy::List(_) | CtTy::Text | CtTy::TokenTree
                ),
                "CT length needs a finite container",
            )?;
            CtTy::Scalar(CtScalarTy::I64)
        }
        K::VariantTag(id) => {
            expect(
                matches!(value(*id)?, CtTy::Enum(_)),
                "CT variant tag needs enum",
            )?;
            CtTy::Scalar(CtScalarTy::I32)
        }
        K::VariantField {
            value: input,
            variant,
            field,
        } => {
            let CtTy::Enum(schema) = value(*input)? else {
                return Err(reject(span, "CT variant projection needs enum"));
            };
            raw.enum_schemas
                .get(schema.0 as usize)
                .and_then(|s| s.variants.get(*variant as usize))
                .and_then(|v| v.get(*field as usize))
                .cloned()
                .ok_or_else(|| reject(span, "CT variant field is out of range"))?
        }
        K::Extract { aggregate, index } => {
            expect(
                index_type(value(*index)?),
                "CT dynamic index must be integer",
            )?;
            match value(*aggregate)? {
                CtTy::Array { element, .. } | CtTy::List(element) => *element.clone(),
                _ => {
                    return Err(reject(
                        span,
                        "CT dynamic extract needs homogeneous container",
                    ));
                }
            }
        }
        K::ExtractField { aggregate, index } => match value(*aggregate)? {
            CtTy::Tuple(fields) => fields.get(*index as usize).cloned(),
            CtTy::Record(schema) => raw
                .record_schemas
                .get(schema.0 as usize)
                .and_then(|s| s.fields.get(*index as usize))
                .cloned(),
            _ => None,
        }
        .ok_or_else(|| {
            reject(
                span,
                "CT static field projection is out of range or not a tuple/record",
            )
        })?,
        K::Replace {
            aggregate,
            index,
            value: replacement,
        } => {
            expect(
                index_type(value(*index)?),
                "CT dynamic index must be integer",
            )?;
            let aggregate_ty = value(*aggregate)?;
            let element = match aggregate_ty {
                CtTy::Array { element, .. } | CtTy::List(element) => element,
                _ => {
                    return Err(reject(
                        span,
                        "CT dynamic replace needs homogeneous container",
                    ));
                }
            };
            expect(
                element.as_ref() == value(*replacement)?,
                "CT replacement element type differs",
            )?;
            aggregate_ty.clone()
        }
        K::ReplaceField {
            aggregate,
            index,
            value: replacement,
        } => {
            let aggregate_ty = value(*aggregate)?;
            let field_ty = match aggregate_ty {
                CtTy::Tuple(fields) => fields.get(*index as usize),
                CtTy::Record(schema) => raw
                    .record_schemas
                    .get(schema.0 as usize)
                    .and_then(|s| s.fields.get(*index as usize)),
                _ => None,
            }
            .ok_or_else(|| {
                reject(
                    span,
                    "CT static replacement is out of range or not a tuple/record",
                )
            })?;
            expect(
                field_ty == value(*replacement)?,
                "CT static replacement field type differs",
            )?;
            aggregate_ty.clone()
        }
        K::Call {
            function: target,
            arguments,
        } => {
            let callee = raw
                .functions
                .get(target.0 as usize)
                .ok_or_else(|| reject(span, "CT call target is missing"))?;
            expect(
                authority.authorize_symbol(
                    &function.definition,
                    &callee.definition,
                    CtSymbolUsage::Call,
                ) && arguments.len() == callee.parameter_types.len(),
                "CT call is not visible or has wrong arity",
            )?;
            for (arg, ty) in arguments.iter().zip(&callee.parameter_types) {
                expect(
                    value(*arg)? == ty,
                    "CT argument type differs from callee parameter",
                )?;
            }
            callee.result_type.clone()
        }
        K::TypePrimitive(_) | K::TypeBuiltin(_) => CtTy::Type,
        K::TypeName(symbol)
        | K::TypeApply {
            nominal: symbol, ..
        } => {
            let usage = if matches!(instruction.kind, K::TypeName(_)) {
                CtSymbolUsage::TypeName
            } else {
                CtSymbolUsage::TypeApply
            };
            expect(
                authority.authorize_symbol(&function.definition, symbol, usage),
                "CT nominal type is not visible",
            )?;
            if let K::TypeApply { arguments, .. } = &instruction.kind {
                for arg in arguments {
                    expect(
                        *value(*arg)? == CtTy::Type || scalar(value(*arg)?).is_some(),
                        "CT type argument is invalid",
                    )?;
                }
            }
            CtTy::Type
        }
        K::TypeConstruct {
            constructor,
            arguments,
        } => {
            use CtTypeConstructor as T;
            let correct = match constructor {
                T::Array => {
                    arguments.len() == 2
                        && *value(arguments[0])? == CtTy::Type
                        && integer(value(arguments[1])?)
                }
                T::Tuple => arguments
                    .iter()
                    .all(|id| value(*id).ok() == Some(&CtTy::Type)),
                T::FunctionPointer => {
                    !arguments.is_empty()
                        && arguments
                            .iter()
                            .all(|id| value(*id).ok() == Some(&CtTy::Type))
                }
                T::ErrorUnion => {
                    arguments.len() == 2
                        && arguments
                            .iter()
                            .all(|id| value(*id).ok() == Some(&CtTy::Type))
                }
                _ => arguments.len() == 1 && *value(arguments[0])? == CtTy::Type,
            };
            expect(correct, "CT type constructor operands are invalid")?;
            CtTy::Type
        }
        K::Quote { template, holes } => {
            let t = raw
                .templates
                .get(template.0 as usize)
                .ok_or_else(|| reject(span, "CT quote template is missing"))?;
            expect(
                t.definition == function.definition && holes.len() == t.holes.len(),
                "CT quote template is from wrong definition or has wrong hole count",
            )?;
            for (id, kind) in holes.iter().zip(&t.holes) {
                let ty = match kind {
                    CtHoleKind::Expr => CtTy::Syntax(CtSyntaxCategory::Expr),
                    CtHoleKind::Type => CtTy::Syntax(CtSyntaxCategory::Type),
                    CtHoleKind::StmtList => CtTy::Syntax(CtSyntaxCategory::StmtList),
                    CtHoleKind::ItemList => CtTy::Syntax(CtSyntaxCategory::ItemList),
                    CtHoleKind::Ident(_) => CtTy::Ident,
                };
                expect(*value(*id)? == ty, "CT quote hole has wrong category")?;
            }
            CtTy::Syntax(t.syntax.category)
        }
        K::Parse { category, input } => {
            expect(
                *value(*input)? == CtTy::TokenTree,
                "CT parser needs raw TokenTree input",
            )?;
            CtTy::Syntax(*category)
        }
        K::ParseIdent { input } => {
            expect(
                *value(*input)? == CtTy::TokenTree,
                "CT identifier parser needs raw TokenTree input",
            )?;
            CtTy::Ident
        }
        K::TokenInspect {
            source,
            projection,
            index,
        } => {
            use CtTokenProjection as P;
            let source_ty = value(*source)?;
            let (ty, needs_index) = match projection {
                P::RootCount if *source_ty == CtTy::TokenTree => {
                    (CtTy::Scalar(CtScalarTy::I64), false)
                }
                P::RootAt if *source_ty == CtTy::TokenTree => (CtTy::TokenCursor, true),
                P::Class | P::Delimiter if *source_ty == CtTy::TokenCursor => {
                    (CtTy::Scalar(CtScalarTy::I32), false)
                }
                P::Text if *source_ty == CtTy::TokenCursor => (CtTy::Text, false),
                P::ChildCount if *source_ty == CtTy::TokenCursor => {
                    (CtTy::Scalar(CtScalarTy::I64), false)
                }
                P::ChildAt if *source_ty == CtTy::TokenCursor => (CtTy::TokenCursor, true),
                P::InputName if *source_ty == CtTy::TokenCursor => (CtTy::Ident, false),
                _ => return Err(reject(span, "CT token projection has wrong source")),
            };
            expect(
                index.is_some() == needs_index,
                "CT token projection index presence is wrong",
            )?;
            if let Some(id) = index {
                expect(
                    index_type(value(*id)?),
                    "CT token projection index is not integer",
                )?;
            }
            ty
        }
        K::Text { op, arguments } => {
            use CtTextOp as T;
            match op {
                T::Equal
                    if arguments.len() == 2
                        && arguments
                            .iter()
                            .all(|id| value(*id).ok() == Some(&CtTy::Text)) =>
                {
                    CtTy::Scalar(CtScalarTy::Bool)
                }
                T::Length if arguments.len() == 1 && *value(arguments[0])? == CtTy::Text => {
                    CtTy::Scalar(CtScalarTy::I64)
                }
                T::Slice
                    if arguments.len() == 3
                        && *value(arguments[0])? == CtTy::Text
                        && arguments[1..]
                            .iter()
                            .all(|id| value(*id).map(index_type).unwrap_or(false)) =>
                {
                    CtTy::Text
                }
                T::Concatenate
                    if arguments.len() == 2
                        && arguments
                            .iter()
                            .all(|id| value(*id).ok() == Some(&CtTy::Text)) =>
                {
                    CtTy::Text
                }
                _ => return Err(reject(span, "CT text operation has wrong operands")),
            }
        }
        K::SyntaxBuild { kind, arguments } => {
            let required = match kind {
                CtSyntaxBuilder::Literal => None,
                CtSyntaxBuilder::TypeRef => Some(CtTy::Type),
                CtSyntaxBuilder::NameExpr => Some(CtTy::Ident),
            };
            expect(
                arguments.len() == 1,
                "CT syntax builder requires one argument",
            )?;
            let arg = value(arguments[0])?;
            if let Some(required) = required {
                expect(*arg == required, "CT syntax builder argument type differs")?;
            } else {
                expect(
                    scalar(arg).is_some() || *arg == CtTy::Text,
                    "CT literal syntax needs scalar or text",
                )?;
            }
            CtTy::Syntax(if *kind == CtSyntaxBuilder::TypeRef {
                CtSyntaxCategory::Type
            } else {
                CtSyntaxCategory::Expr
            })
        }
        K::NameBuild { text, .. } => {
            expect(*value(*text)? == CtTy::Text, "CT name builder needs text")?;
            CtTy::Ident
        }
        K::Query { kind, arguments } => {
            let required_stage = match kind {
                CtQueryKind::Shape | CtQueryKind::Field => CtReadyStage::Shape,
                CtQueryKind::ImplIndex => CtReadyStage::ImplIndex,
                CtQueryKind::Layout => CtReadyStage::Layout,
            };
            expect(
                raw.snapshot.ready as u8 >= required_stage as u8
                    && raw.snapshot.requested_queries.contains(kind)
                    && authority.authorize_query(&function.definition, *kind),
                "CT query is not in the frozen stage grant",
            )?;
            expect(
                !arguments.is_empty() && *value(arguments[0])? == CtTy::Type,
                "CT query needs Type",
            )?;
            match kind {
                CtQueryKind::Shape => {
                    expect(arguments.len() == 1, "shape query arity")?;
                    CtTy::ShapeInfo
                }
                CtQueryKind::Field => {
                    expect(
                        arguments.len() == 2 && *value(arguments[1])? == CtTy::Text,
                        "field query needs name text",
                    )?;
                    CtTy::FieldInfo
                }
                CtQueryKind::ImplIndex => CtTy::List(Box::new(CtTy::ImplInfo)),
                CtQueryKind::Layout => {
                    expect(
                        arguments.len() == 1 && raw.snapshot.target_fingerprint.is_some(),
                        "layout query needs target",
                    )?;
                    CtTy::LayoutInfo
                }
            }
        }
        K::MetaProject {
            value: input,
            projection,
        } => {
            use CtMetaProjection as P;
            let source = value(*input)?;
            match projection {
                P::ShapeFields if *source == CtTy::ShapeInfo => {
                    CtTy::List(Box::new(CtTy::FieldInfo))
                }
                P::ShapeComplete if *source == CtTy::ShapeInfo => CtTy::Scalar(CtScalarTy::Bool),
                P::FieldName if *source == CtTy::FieldInfo => CtTy::Ident,
                P::FieldType if *source == CtTy::FieldInfo => CtTy::Type,
                P::FieldManaged if *source == CtTy::FieldInfo => CtTy::Scalar(CtScalarTy::Bool),
                P::ImplTrait if *source == CtTy::ImplInfo => CtTy::Ident,
                P::ImplType if *source == CtTy::ImplInfo => CtTy::Type,
                P::LayoutSize | P::LayoutAlignment if *source == CtTy::LayoutInfo => {
                    CtTy::Scalar(CtScalarTy::I64)
                }
                P::LayoutFieldOffsets if *source == CtTy::LayoutInfo => {
                    CtTy::List(Box::new(CtTy::Scalar(CtScalarTy::I64)))
                }
                _ => {
                    return Err(reject(
                        span,
                        "CT metadata projection is not valid for descriptor",
                    ));
                }
            }
        }
    };
    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{ModuleId, NodeId};

    struct Authority {
        snapshot: CtSnapshotRequest,
        expected: Option<CtFunction>,
        expected_artifact: Option<CtArtifactProgram>,
    }
    impl CtArtifactAuthority for Authority {
        fn source_fingerprint(&self) -> Fingerprint {
            Fingerprint::ZERO
        }
        fn snapshot(&self) -> &CtSnapshotRequest {
            &self.snapshot
        }
        fn authenticate_input_table(&self, input: &CtInputTable) -> bool {
            input.invocation_fingerprint == Fingerprint::ZERO && input.tokens.is_empty()
        }
        fn authenticate_artifact(&self, artifact: &CtArtifactProgram) -> bool {
            self.expected_artifact
                .as_ref()
                .is_none_or(|expected| expected == artifact)
        }
        fn authenticate_function(&self, function: &CtFunction) -> bool {
            self.expected
                .as_ref()
                .is_none_or(|actual| actual == function)
        }
        fn authorize_symbol(&self, _: &CtSymbolId, symbol: &CtSymbolId, _: CtSymbolUsage) -> bool {
            symbol.module == ModuleId(0) && symbol.declaration == NodeId(1)
        }
        fn authorize_query(&self, _: &CtSymbolId, _: CtQueryKind) -> bool {
            true
        }
    }
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
                                op: BinaryOp::Add,
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
    fn check(raw: CtArtifactProgram) -> CompileResult<VerifiedCTArtifactProgram> {
        let authority = Authority {
            snapshot: raw.snapshot.clone(),
            expected: None,
            expected_artifact: None,
        };
        verify_ct_artifact(raw, &Limits::default(), &authority)
    }
    #[test]
    fn accepts_bounded_pure_scalar_cfg() {
        assert!(check(raw()).is_ok());
    }
    #[test]
    fn rejects_use_before_definition() {
        let mut p = raw();
        if let CtInstructionKind::Binary { left, .. } =
            &mut p.functions[0].blocks[0].instructions[2].kind
        {
            *left = CtValueId(2);
        }
        assert_eq!(check(p).unwrap_err()[0].code, "E-CT-ARTIFACT");
    }
    #[test]
    fn rejects_uninitialized_local() {
        let mut p = raw();
        p.functions[0].locals.push(CtTy::Scalar(CtScalarTy::I32));
        p.functions[0].blocks[0].instructions[0].kind = CtInstructionKind::Load(CtLocalId(0));
        assert!(check(p).unwrap_err()[0].message.contains("uninitialized"));
    }
    #[test]
    fn rejects_early_shape_query_even_if_raw_requests_it() {
        let mut p = raw();
        p.functions[0].values[2] = CtTy::ShapeInfo;
        p.functions[0].result_type = CtTy::ShapeInfo;
        p.functions[0].values[0] = CtTy::Type;
        p.functions[0].blocks[0].instructions[0].kind =
            CtInstructionKind::TypePrimitive(CtScalarTy::I32);
        p.functions[0].blocks[0].instructions[2].kind = CtInstructionKind::Query {
            kind: CtQueryKind::Shape,
            arguments: vec![CtValueId(0)],
        };
        p.snapshot.requested_queries.push(CtQueryKind::Shape);
        assert!(check(p).unwrap_err()[0].message.contains("stage grant"));
    }
    #[test]
    fn rejects_query_descriptor_forgery_through_record() {
        let mut p = raw();
        p.record_schemas.push(CtRecordSchema {
            name: symbol(),
            fields: vec![CtTy::Type],
            origin: CtSchemaOrigin::BuiltinQuery,
        });
        assert!(check(p).unwrap_err()[0].message.contains("schema"));
    }
    #[test]
    fn rejects_budget_below_actual_dominance_work() {
        let mut p = raw();
        p.budget.max_analysis_work = 0;
        assert_eq!(check(p).unwrap_err()[0].code, "E-RESOURCE-LIMIT");
    }
    #[test]
    fn rejects_body_impersonating_real_definition() {
        let mut p = raw();
        let expected = p.functions[0].clone();
        p.functions[0].blocks[0].instructions[0].kind =
            CtInstructionKind::Constant(CtConstantId(1));
        let authority = Authority {
            snapshot: p.snapshot.clone(),
            expected: Some(expected),
            expected_artifact: None,
        };
        assert!(
            verify_ct_artifact(p, &Limits::default(), &authority).unwrap_err()[0]
                .message
                .contains("definition")
        );
    }
    #[test]
    fn rejects_replaced_constant_table_under_unchanged_function() {
        let expected = raw();
        let mut forged = expected.clone();
        forged.constants[0] = CtConstant::Scalar(Constant::I32(99));
        let authority = Authority {
            snapshot: forged.snapshot.clone(),
            expected: None,
            expected_artifact: Some(expected),
        };
        assert!(
            verify_ct_artifact(forged, &Limits::default(), &authority).unwrap_err()[0]
                .message
                .contains("source or frozen snapshot")
        );
    }
    #[test]
    fn rejects_input_name_with_unbound_namespace() {
        let mut p = raw();
        p.templates.push(CtTemplate {
            definition: symbol(),
            syntax: CtSyntaxTree {
                category: CtSyntaxCategory::Expr,
                nodes: vec![CtSyntaxNode::Token(CtToken {
                    class: CtTokenClass::Identifier,
                    text: "x".into(),
                    origin: CtOriginId(0),
                    name: Some(CtIdent {
                        text: "x".into(),
                        namespace: CtNameNamespace::Unbound,
                        source: CtNameSource::Input(CtInputTokenId(0)),
                        member_owner: None,
                        member_owner_type: None,
                    }),
                })],
                roots: vec![CtSyntaxNodeId(0)],
                origins: vec![CtOrigin {
                    span: Span::default(),
                    parent: None,
                    role: CtOriginRole::Input,
                }],
            },
            holes: Vec::new(),
        });
        assert!(
            check(p).unwrap_err()[0]
                .message
                .contains("unbound namespace")
        );
    }
    #[test]
    fn rejects_bare_type_in_syntax_type_hole() {
        let mut p = raw();
        p.functions[0].values = vec![CtTy::Type, CtTy::Syntax(CtSyntaxCategory::Type)];
        p.functions[0].result_type = CtTy::Syntax(CtSyntaxCategory::Type);
        p.functions[0].blocks[0].instructions = vec![
            CtInstruction {
                result: Some(CtValueId(0)),
                kind: CtInstructionKind::TypePrimitive(CtScalarTy::I32),
                span: Span::default(),
            },
            CtInstruction {
                result: Some(CtValueId(1)),
                kind: CtInstructionKind::Quote {
                    template: CtTemplateId(0),
                    holes: vec![CtValueId(0)],
                },
                span: Span::default(),
            },
        ];
        p.functions[0].blocks[0].terminator = CtTerminator::Return(Some(CtValueId(1)));
        p.templates.push(CtTemplate {
            definition: symbol(),
            syntax: CtSyntaxTree {
                category: CtSyntaxCategory::Type,
                nodes: vec![CtSyntaxNode::Hole {
                    index: 0,
                    origin: CtOriginId(0),
                }],
                roots: vec![CtSyntaxNodeId(0)],
                origins: vec![CtOrigin {
                    span: Span::default(),
                    parent: None,
                    role: CtOriginRole::QuoteDefinition,
                }],
            },
            holes: vec![CtHoleKind::Type],
        });
        assert!(check(p).unwrap_err()[0].message.contains("hole"));
    }
}
