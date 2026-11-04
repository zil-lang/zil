//! Closed CT services over the pre-generation source snapshot. Host callbacks
//! return pure data and never execute a provider or grant ordinary Verify.
use super::{
    authority::stage_allows,
    ordinary_type, render,
    snapshot::{ImplSnapshot, ShapeSnapshot},
    source_index::SourceIndex,
    type_ref::from_checked_type,
};
use zil_base::{CompileResult, Diagnostic, FileId, Fingerprint, ModuleId, Phase, SourceUnit, Span};
use zil_ct::{CtExecutionBudget, CtReadView};
use zil_ir::ct_wire::{self, WireBudget, WireError};
use zil_ir::{
    AggregateKind, Constant, CoreAggregateFieldRecipe, CoreAggregateTemplate,
    CoreAggregateTypeRecipe, CoreProgram, ProgramModule, Type, ct_artifact::*,
};
use zil_syntax::{
    GeneratedSyntax,
    ast::{self as s, ItemKind},
};

fn error(code: &str, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, code, None, message)]
}
fn phase(message: &str) -> Vec<Diagnostic> {
    error("E-CT-PHASE-DEPENDENCY", message)
}

pub(super) struct FrozenReadView<'a> {
    pub index: &'a SourceIndex,
    pub sources: &'a [SourceUnit],
    pub syntax: &'a [&'a s::Module],
    pub shape: Option<&'a ShapeSnapshot>,
    pub impl_index: Option<&'a ImplSnapshot>,
    pub shape_failure: Option<&'a [Diagnostic]>,
    pub input: Option<&'a CtTokenTree>,
    pub limits: &'a zil_base::Limits,
    pub target: &'a zil_base::TargetProfile,
    pub snapshot: &'a CtSnapshotRequest,
}

impl FrozenReadView<'_> {
    fn unready(&self, message: &str) -> Vec<Diagnostic> {
        let cycle = self.shape_failure.is_some_and(|errors| {
            errors.iter().any(|error| {
                let code = error.code.to_ascii_lowercase();
                let text = error.message.to_ascii_lowercase();
                code.contains("cycle")
                    || code.contains("recursive")
                    || text.contains("cycle")
                    || text.contains("recursive")
            })
        });
        error(
            if cycle {
                "E-CT-PHASE-CYCLE"
            } else {
                "E-CT-PHASE-DEPENDENCY"
            },
            message,
        )
    }
    fn pointer_component_spelling(
        &self,
        ty: &CtTypeRef,
        id: CtTypeNodeId,
        label: Option<u32>,
        out: &mut Vec<(String, CtTokenClass, Option<CtIdent>)>,
        depth: usize,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        if let Some(label) = label {
            let Some(CtTypeNode::Address { mutable, inner }) = ty.nodes.get(id.0 as usize) else {
                return Err(phase(
                    "pointer life label needs an actual address component",
                ));
            };
            budget.reserve_analysis_work(3)?;
            budget.reserve_temporary_bytes(96)?;
            out.push(("&".into(), CtTokenClass::Punctuation, None));
            out.push((
                format!("'zil_life_{label}"),
                CtTokenClass::Punctuation,
                None,
            ));
            if *mutable {
                out.push(("mut".into(), CtTokenClass::Identifier, None));
            }
            self.type_spelling(ty, *inner, out, depth + 1, budget)
        } else {
            self.type_spelling(ty, id, out, depth + 1, budget)
        }
    }
    fn type_spelling(
        &self,
        ty: &CtTypeRef,
        node: CtTypeNodeId,
        out: &mut Vec<(String, CtTokenClass, Option<CtIdent>)>,
        depth: usize,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        budget.reserve_analysis_work(1)?;
        budget.reserve_temporary_bytes(64)?;
        if depth > 128 {
            return Err(error(
                "E-CT-TYPE-PHASE",
                "type syntax nesting exceeds budget",
            ));
        }
        let actual = ty
            .nodes
            .get(node.0 as usize)
            .ok_or_else(|| error("E-CT-TYPE-PHASE", "type node is out of range"))?;
        let punctuation = |out: &mut Vec<_>, text: &str| {
            out.push((text.to_owned(), CtTokenClass::Punctuation, None))
        };
        let word = |out: &mut Vec<_>, text: &str| {
            out.push((text.to_owned(), CtTokenClass::Identifier, None))
        };
        match actual {
            CtTypeNode::Primitive(scalar) => out.push((
                match scalar {
                    CtScalarTy::Unit => "Unit",
                    CtScalarTy::Bool => "bool",
                    CtScalarTy::U8 => "u8",
                    CtScalarTy::U16 => "u16",
                    CtScalarTy::I32 => "i32",
                    CtScalarTy::I64 => "i64",
                    CtScalarTy::U64 => "u64",
                    CtScalarTy::Usize => "usize",
                }
                .into(),
                CtTokenClass::Identifier,
                None,
            )),
            CtTypeNode::Builtin(builtin) => word(
                out,
                match builtin {
                    CtBuiltinType::ByteBuffer => "ByteBuffer",
                    CtBuiltinType::ForeignHandle => "ForeignHandle",
                    CtBuiltinType::CWideString => "CWideString",
                },
            ),
            CtTypeNode::Nominal { symbol, arguments } => {
                let (text, namespace) = self.text_name(symbol)?;
                out.push((
                    text.clone(),
                    CtTokenClass::Identifier,
                    Some(CtIdent {
                        text,
                        namespace,
                        source: CtNameSource::Definition(symbol.clone()),
                        member_owner: None,
                        member_owner_type: None,
                    }),
                ));
                if !arguments.is_empty() {
                    punctuation(out, "<");
                    for (index, argument) in arguments.iter().enumerate() {
                        if index != 0 {
                            punctuation(out, ",");
                        }
                        match argument {
                            CtTypeArgument::Type(id) => {
                                self.type_spelling(ty, *id, out, depth + 1, budget)?
                            }
                            CtTypeArgument::Scalar(value) => {
                                let text = match value {
                                    Constant::Bool(value) => value.to_string(),
                                    Constant::U8(value) => value.to_string(),
                                    Constant::U16(value) => value.to_string(),
                                    Constant::I32(value) => value.to_string(),
                                    Constant::I64(value) => value.to_string(),
                                    Constant::U64(value) => format!("{value}u64"),
                                    Constant::Usize(value) => format!("{value}usize"),
                                    _ => {
                                        return Err(phase(
                                            "generic Type constant has no ordinary syntax spelling",
                                        ));
                                    }
                                };
                                if let Some(digits) = text.strip_prefix('-') {
                                    punctuation(out, "-");
                                    out.push((digits.into(), CtTokenClass::Integer, None));
                                } else {
                                    out.push((
                                        text,
                                        if matches!(value, Constant::Bool(_)) {
                                            CtTokenClass::Keyword
                                        } else {
                                            CtTokenClass::Integer
                                        },
                                        None,
                                    ));
                                }
                            }
                        }
                    }
                    punctuation(out, ">");
                }
            }
            CtTypeNode::Array { element, length } => {
                punctuation(out, "[");
                self.type_spelling(ty, *element, out, depth + 1, budget)?;
                punctuation(out, ";");
                out.push((length.to_string(), CtTokenClass::Integer, None));
                punctuation(out, "]");
            }
            CtTypeNode::Tuple(items) => {
                punctuation(out, "(");
                for (index, item) in items.iter().enumerate() {
                    if index != 0 {
                        punctuation(out, ",");
                    }
                    self.type_spelling(ty, *item, out, depth + 1, budget)?;
                }
                if items.len() == 1 {
                    punctuation(out, ",");
                }
                punctuation(out, ")");
            }
            CtTypeNode::Slice(inner) => {
                punctuation(out, "[");
                self.type_spelling(ty, *inner, out, depth + 1, budget)?;
                punctuation(out, "]");
            }
            CtTypeNode::Address { mutable, inner } => {
                punctuation(out, "&");
                if *mutable {
                    word(out, "mut");
                }
                self.type_spelling(ty, *inner, out, depth + 1, budget)?;
            }
            CtTypeNode::Managed(inner)
            | CtTypeNode::Desc(inner)
            | CtTypeNode::OwnedStore(inner) => {
                word(
                    out,
                    match actual {
                        CtTypeNode::Managed(_) => "Managed",
                        CtTypeNode::Desc(_) => "Desc",
                        _ => "Store",
                    },
                );
                punctuation(out, "<");
                self.type_spelling(ty, *inner, out, depth + 1, budget)?;
                punctuation(out, ">");
            }
            CtTypeNode::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                budget.reserve_analysis_work(
                    (life.binder_count as u64)
                        .saturating_mul(8)
                        .saturating_add(life.parameter_lives.len() as u64),
                )?;
                budget.reserve_temporary_bytes((life.binder_count as u64).saturating_mul(192))?;
                for i in 0..life.binder_count {
                    word(out, "for");
                    punctuation(out, "<");
                    word(out, "life");
                    word(out, &format!("zil_life_{i}"));
                    punctuation(out, ">");
                }
                word(out, "fn");
                punctuation(out, "(");
                for (i, id) in parameters.iter().enumerate() {
                    if i > 0 {
                        punctuation(out, ",");
                    }
                    self.pointer_component_spelling(
                        ty,
                        *id,
                        life.parameter_lives.get(i).copied().flatten(),
                        out,
                        depth + 1,
                        budget,
                    )?;
                }
                punctuation(out, ")");
                punctuation(out, "->");
                self.pointer_component_spelling(
                    ty,
                    *result,
                    life.result_life,
                    out,
                    depth + 1,
                    budget,
                )?;
            }
            CtTypeNode::FunctionPointer { parameters, result }
            | CtTypeNode::ForeignCallback { parameters, result } => {
                if matches!(actual, CtTypeNode::ForeignCallback { .. }) {
                    word(out, "extern");
                    out.push(("\"C\"".into(), CtTokenClass::String, None));
                }
                word(out, "fn");
                punctuation(out, "(");
                for (index, parameter) in parameters.iter().enumerate() {
                    if index != 0 {
                        punctuation(out, ",");
                    }
                    self.type_spelling(ty, *parameter, out, depth + 1, budget)?;
                }
                punctuation(out, ")");
                punctuation(out, "->");
                self.type_spelling(ty, *result, out, depth + 1, budget)?;
            }
            CtTypeNode::ErrorUnion { error, result } => {
                self.type_spelling(ty, *error, out, depth + 1, budget)?;
                punctuation(out, "!");
                self.type_spelling(ty, *result, out, depth + 1, budget)?;
            }
            CtTypeNode::Opaque { .. } => {
                return Err(phase("opaque Type cannot reveal a source spelling"));
            }
        }
        Ok(())
    }
    fn syntax_tokens(
        category: CtSyntaxCategory,
        parts: Vec<(String, CtTokenClass, Option<CtIdent>)>,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtSyntaxTree> {
        let count = u64::try_from(parts.len())
            .map_err(|_| error("E-CT-SYNTAX-LIMIT", "syntax node count overflow"))?;
        let bytes = parts
            .iter()
            .map(|(text, _, _)| text.len() as u64)
            .sum::<u64>();
        budget.reserve_analysis_work(count.saturating_add(1))?;
        budget.reserve_generated_nodes(count.saturating_add(1))?;
        budget.reserve_temporary_bytes(bytes)?;
        let origin = CtOriginId(0);
        let mut tree = CtSyntaxTree {
            category,
            nodes: Vec::with_capacity(parts.len()),
            roots: Vec::with_capacity(parts.len()),
            origins: vec![CtOrigin {
                span: Span::default(),
                parent: None,
                role: CtOriginRole::Generated,
            }],
        };
        for (text, class, name) in parts {
            let id = CtSyntaxNodeId(
                u32::try_from(tree.nodes.len())
                    .map_err(|_| error("E-CT-SYNTAX-LIMIT", "syntax node identity overflow"))?,
            );
            tree.nodes.push(CtSyntaxNode::Token(CtToken {
                class,
                text,
                origin,
                name,
            }));
            tree.roots.push(id);
        }
        if tree.roots.len() != 1
            && matches!(category, CtSyntaxCategory::Expr | CtSyntaxCategory::Type)
        {
            let id = CtSyntaxNodeId(
                u32::try_from(tree.nodes.len())
                    .map_err(|_| error("E-CT-SYNTAX-LIMIT", "syntax node identity overflow"))?,
            );
            let children = std::mem::take(&mut tree.roots);
            tree.nodes.push(CtSyntaxNode::Group {
                delimiter: CtDelimiter::Parenthesis,
                children,
                origin,
            });
            tree.roots.push(id);
        }
        Ok(tree)
    }
    fn instantiate(
        template: &CtTemplate,
        holes: &[CtValue],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtSyntaxTree> {
        if holes.len() != template.holes.len() {
            return Err(error(
                "E-CT-QUOTE-HOLE",
                "quote hole count differs from verified template",
            ));
        }
        let mut needed_nodes = template.syntax.nodes.len() as u64;
        let mut needed_origins = template.syntax.origins.len() as u64;
        let mut needed_bytes = 0u64;
        for node in &template.syntax.nodes {
            match node {
                CtSyntaxNode::Token(token) => {
                    needed_bytes = needed_bytes.saturating_add(token.text.len() as u64)
                }
                CtSyntaxNode::Hole { index, .. } => match holes.get(*index as usize) {
                    Some(CtValue::Syntax(tree)) => {
                        needed_nodes = needed_nodes.saturating_add(tree.nodes.len() as u64);
                        needed_origins = needed_origins.saturating_add(tree.origins.len() as u64);
                        for node in &tree.nodes {
                            if let CtSyntaxNode::Token(token) = node {
                                needed_bytes = needed_bytes.saturating_add(token.text.len() as u64);
                            }
                        }
                    }
                    Some(CtValue::Ident(ident)) => {
                        needed_nodes = needed_nodes.saturating_add(1);
                        needed_origins = needed_origins.saturating_add(1);
                        needed_bytes = needed_bytes.saturating_add(ident.text.len() as u64);
                    }
                    _ => {
                        return Err(error(
                            "E-CT-QUOTE-HOLE",
                            "quote hole value is not Syntax or Ident",
                        ));
                    }
                },
                _ => {}
            }
        }
        budget.reserve_analysis_work(needed_nodes.saturating_add(needed_origins))?;
        budget.reserve_generated_nodes(needed_nodes.saturating_add(needed_origins))?;
        budget.reserve_temporary_bytes(needed_bytes)?;
        let mut out = CtSyntaxTree {
            category: template.syntax.category,
            nodes: Vec::new(),
            roots: Vec::new(),
            origins: template.syntax.origins.clone(),
        };
        let mut map = Vec::<Vec<CtSyntaxNodeId>>::with_capacity(template.syntax.nodes.len());
        for node in &template.syntax.nodes {
            let mapped =
                match node {
                    CtSyntaxNode::Token(token) => {
                        let id = CtSyntaxNodeId(u32::try_from(out.nodes.len()).map_err(|_| {
                            error("E-CT-QUOTE-LIMIT", "quote node identity overflow")
                        })?);
                        out.nodes.push(CtSyntaxNode::Token(token.clone()));
                        vec![id]
                    }
                    CtSyntaxNode::Group {
                        delimiter,
                        children,
                        origin,
                    } => {
                        let mut mapped_children = Vec::new();
                        for child in children {
                            mapped_children.extend(
                                map.get(child.0 as usize)
                                    .ok_or_else(|| {
                                        error(
                                            "E-CT-QUOTE-HOLE",
                                            "quote child is not earlier than its group",
                                        )
                                    })?
                                    .iter()
                                    .copied(),
                            );
                        }
                        let id = CtSyntaxNodeId(u32::try_from(out.nodes.len()).map_err(|_| {
                            error("E-CT-QUOTE-LIMIT", "quote node identity overflow")
                        })?);
                        out.nodes.push(CtSyntaxNode::Group {
                            delimiter: *delimiter,
                            children: mapped_children,
                            origin: *origin,
                        });
                        vec![id]
                    }
                    CtSyntaxNode::Hole { index, origin } => {
                        let value = holes.get(*index as usize).ok_or_else(|| {
                            error("E-CT-QUOTE-HOLE", "quote hole index is out of range")
                        })?;
                        let expected = template.holes.get(*index as usize).ok_or_else(|| {
                            error("E-CT-QUOTE-HOLE", "quote hole kind is missing")
                        })?;
                        match (expected, value) {
                            (CtHoleKind::Ident(namespace), CtValue::Ident(ident))
                                if namespace == &ident.namespace
                                    || ident.namespace == CtNameNamespace::Unbound =>
                            {
                                let mut ident = ident.clone();
                                ident.namespace = *namespace;
                                let new_origin =
                                    CtOriginId(u32::try_from(out.origins.len()).map_err(|_| {
                                        error("E-CT-QUOTE-LIMIT", "quote origin identity overflow")
                                    })?);
                                let span = out
                                    .origins
                                    .get(origin.0 as usize)
                                    .ok_or_else(|| {
                                        error("E-CT-QUOTE-HOLE", "quote hole origin is missing")
                                    })?
                                    .span;
                                out.origins.push(CtOrigin {
                                    span,
                                    parent: Some(*origin),
                                    role: CtOriginRole::Splice,
                                });
                                let id = CtSyntaxNodeId(u32::try_from(out.nodes.len()).map_err(
                                    |_| error("E-CT-QUOTE-LIMIT", "quote node identity overflow"),
                                )?);
                                out.nodes.push(CtSyntaxNode::Token(CtToken {
                                    class: CtTokenClass::Identifier,
                                    text: ident.text.clone(),
                                    origin: new_origin,
                                    name: Some(ident),
                                }));
                                vec![id]
                            }
                            (CtHoleKind::Expr, CtValue::Syntax(tree))
                                if tree.category == CtSyntaxCategory::Expr =>
                            {
                                Self::graft(&mut out, tree, *origin)?
                            }
                            (CtHoleKind::Type, CtValue::Syntax(tree))
                                if tree.category == CtSyntaxCategory::Type =>
                            {
                                Self::graft(&mut out, tree, *origin)?
                            }
                            (CtHoleKind::StmtList, CtValue::Syntax(tree))
                                if tree.category == CtSyntaxCategory::StmtList =>
                            {
                                Self::graft(&mut out, tree, *origin)?
                            }
                            (CtHoleKind::ItemList, CtValue::Syntax(tree))
                                if tree.category == CtSyntaxCategory::ItemList =>
                            {
                                Self::graft(&mut out, tree, *origin)?
                            }
                            _ => {
                                return Err(error(
                                    "E-CT-QUOTE-HOLE",
                                    "quote hole category or name namespace differs",
                                ));
                            }
                        }
                    }
                };
            map.push(mapped);
        }
        for root in &template.syntax.roots {
            out.roots.extend(
                map.get(root.0 as usize)
                    .ok_or_else(|| error("E-CT-QUOTE-HOLE", "quote root is out of range"))?
                    .iter()
                    .copied(),
            );
        }
        Ok(out)
    }
    fn graft(
        out: &mut CtSyntaxTree,
        tree: &CtSyntaxTree,
        parent: CtOriginId,
    ) -> CompileResult<Vec<CtSyntaxNodeId>> {
        let origin_base = u32::try_from(out.origins.len())
            .map_err(|_| error("E-CT-QUOTE-LIMIT", "quote origin identity overflow"))?;
        for origin in &tree.origins {
            let parent = match origin.parent {
                Some(id) => Some(CtOriginId(origin_base.checked_add(id.0).ok_or_else(
                    || error("E-CT-QUOTE-LIMIT", "quote origin identity overflow"),
                )?)),
                None => Some(parent),
            };
            out.origins.push(CtOrigin {
                span: origin.span,
                parent,
                role: origin.role,
            });
        }
        let mut map = Vec::with_capacity(tree.nodes.len());
        for node in &tree.nodes {
            let mapped = match node {
                CtSyntaxNode::Token(token) => {
                    let mut token = token.clone();
                    token.origin =
                        CtOriginId(origin_base.checked_add(token.origin.0).ok_or_else(|| {
                            error("E-CT-QUOTE-LIMIT", "quote origin identity overflow")
                        })?);
                    CtSyntaxNode::Token(token)
                }
                CtSyntaxNode::Group {
                    delimiter,
                    children,
                    origin,
                } => CtSyntaxNode::Group {
                    delimiter: *delimiter,
                    children: children
                        .iter()
                        .map(|id| {
                            map.get(id.0 as usize).copied().ok_or_else(|| {
                                error("E-CT-QUOTE-HOLE", "spliced syntax has invalid child")
                            })
                        })
                        .collect::<CompileResult<Vec<_>>>()?,
                    origin: CtOriginId(origin_base.checked_add(origin.0).ok_or_else(|| {
                        error("E-CT-QUOTE-LIMIT", "quote origin identity overflow")
                    })?),
                },
                CtSyntaxNode::Hole { .. } => {
                    return Err(error(
                        "E-CT-QUOTE-HOLE",
                        "evaluated Syntax still contains a hole",
                    ));
                }
            };
            let id = CtSyntaxNodeId(
                u32::try_from(out.nodes.len())
                    .map_err(|_| error("E-CT-QUOTE-LIMIT", "quote node identity overflow"))?,
            );
            out.nodes.push(mapped);
            map.push(id);
        }
        tree.roots
            .iter()
            .map(|id| {
                map.get(id.0 as usize)
                    .copied()
                    .ok_or_else(|| error("E-CT-QUOTE-HOLE", "spliced syntax has invalid root"))
            })
            .collect()
    }
    fn token_tree(&self) -> CompileResult<&CtTokenTree> {
        self.input
            .ok_or_else(|| error("E-CT-INPUT", "no authenticated provider token input exists"))
    }
    fn wire_budget(&self, budget: &CtExecutionBudget) -> WireBudget {
        let remaining = budget.remaining();
        let cap = |value: u64| usize::try_from(value).unwrap_or(usize::MAX);
        WireBudget {
            bytes: self
                .limits
                .max_source_bytes
                .min(cap(remaining.max_temporary_bytes)),
            nodes: self
                .limits
                .max_nodes
                .min(cap(remaining.max_generated_nodes)),
            work: self
                .limits
                .max_analysis_steps
                .min(cap(remaining.max_analysis_work)),
            allocation: self
                .limits
                .max_source_bytes
                .min(cap(remaining.max_temporary_bytes)),
            max_depth: self
                .limits
                .max_call_depth
                .min(cap(u64::from(remaining.max_call_depth))),
        }
    }
    fn wire_error(error: WireError) -> Vec<Diagnostic> {
        if error == WireError::Limit {
            vec![Diagnostic::limit(
                Phase::Ct,
                "canonical CT fingerprint exceeds the shared request budget",
                None,
            )]
        } else {
            vec![Diagnostic::error(
                Phase::Ct,
                "E-CT-WIRE",
                None,
                format!("canonical CT fingerprint rejected data: {error:?}"),
            )]
        }
    }
    fn settle_wire(
        budget: &mut CtExecutionBudget,
        before: WireBudget,
        after: WireBudget,
    ) -> CompileResult<()> {
        budget.reserve_analysis_work(before.work.saturating_sub(after.work) as u64)?;
        budget.reserve_generated_nodes(before.nodes.saturating_sub(after.nodes) as u64)?;
        let bytes = before.allocation.saturating_sub(after.allocation) as u64;
        budget.reserve_temporary_bytes(bytes)?;
        budget.release_temporary_bytes(bytes);
        Ok(())
    }
    fn value_fingerprint(
        &self,
        value: &CtValue,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Fingerprint> {
        let mut wire = self.wire_budget(budget);
        let before = wire;
        let result = ct_wire::value_fingerprint_with_usage(value, self.limits, &mut wire);
        Self::settle_wire(budget, before, wire)?;
        result
            .map(|(fingerprint, _)| fingerprint)
            .map_err(Self::wire_error)
    }
    fn token_fingerprint(
        &self,
        tree: &CtTokenTree,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Fingerprint> {
        let mut wire = self.wire_budget(budget);
        let before = wire;
        let result = ct_wire::token_tree_fingerprint_with_usage(tree, self.limits, &mut wire);
        Self::settle_wire(budget, before, wire)?;
        result
            .map(|(fingerprint, _)| fingerprint)
            .map_err(Self::wire_error)
    }
    fn declaration(&self, symbol: &CtSymbolId) -> Option<&s::Item> {
        self.syntax
            .iter()
            .find(|module| module.id == symbol.module)?
            .items
            .get(symbol.declaration.0 as usize)
    }
    fn nominal(&self, requester: &CtSymbolId, symbol: &CtSymbolId) -> CompileResult<&s::Item> {
        if !self.index.visible(requester, symbol) {
            return Err(error(
                "E-CT-VISIBILITY",
                "CT type declaration is absent or hidden at this definition site",
            ));
        }
        let item = self
            .declaration(symbol)
            .ok_or_else(|| phase("type declaration is not in the frozen source batch"))?;
        if !matches!(
            item.kind,
            ItemKind::Aggregate(_) | ItemKind::Enum(_) | ItemKind::TypeAlias(_)
        ) {
            return Err(phase("symbol is not a ready nominal type"));
        }
        let ready = self
            .shape
            .ok_or_else(|| self.unready("ShapeReady has not completed"))?;
        let found = ready.core.modules.iter().any(|module| {
            module.core.module_id == symbol.module
                && (module
                    .core
                    .aggregates
                    .iter()
                    .any(|aggregate| aggregate.span == item.span)
                    || module
                        .core
                        .enums
                        .iter()
                        .any(|enumeration| enumeration.span == item.span)
                    || module
                        .core
                        .aggregate_templates
                        .iter()
                        .any(|template| template.span == item.span))
        });
        let selected_generic = ready
            .selected
            .iter()
            .find(|module| module.id == symbol.module)
            .is_some_and(|module| {
                module.items.iter().any(|candidate| {
                    candidate.span == item.span
                        && match &candidate.kind {
                            ItemKind::Aggregate(value) => !value.generics.is_empty(),
                            ItemKind::Enum(value) => !value.generics.is_empty(),
                            ItemKind::TypeAlias(value) => value.value.is_some(),
                            _ => false,
                        }
                })
            });
        if !found && !selected_generic {
            return Err(phase(
                "type shape depends on a generated or unfinished declaration",
            ));
        }
        Ok(item)
    }
    fn derived_shape(
        &self,
        owner: &CtSymbolId,
        owner_type: &CtTypeRef,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<(ShapeSnapshot, u64)> {
        let base = self
            .shape
            .ok_or_else(|| self.unready("ShapeReady has not completed"))?;
        let source = self
            .sources
            .iter()
            .find(|source| source.module_id == owner.module)
            .ok_or_else(|| phase("generic type source module is absent"))?;
        let insertion = self
            .index
            .span(owner)
            .ok_or_else(|| phase("generic type declaration is absent"))?;
        let clone_charge = (owner_type.nodes.len() as u64).saturating_mul(96);
        budget.reserve_temporary_bytes(clone_charge)?;
        let syntax_result = self.build_syntax(
            CtSyntaxBuilder::TypeRef,
            &[CtValue::Type(owner_type.clone())],
            budget,
        );
        budget.release_temporary_bytes(clone_charge);
        let syntax = syntax_result?;
        let parsed = render::parse(
            &syntax,
            source.file_id,
            owner.module,
            &self.index.aliases(owner.module),
            self.limits,
            budget,
        )?;
        let Some(GeneratedSyntax::Type(ty)) = parsed.parsed.syntax else {
            return Err(error(
                "E-CT-QUERY-TYPE",
                "generic type application did not parse as a type",
            ));
        };
        let mut generated = GeneratedSyntax::Type(ty);
        let mut remap_limits = self.limits.clone();
        remap_limits.max_analysis_steps = remap_limits
            .max_analysis_steps
            .min(budget.remaining().max_analysis_work as usize);
        let remap_work =
            zil_expand::remap_generated_spans(&mut generated, insertion, &remap_limits)?;
        budget.reserve_analysis_work(remap_work as u64)?;
        let GeneratedSyntax::Type(ty) = generated else {
            unreachable!("only the type spans were remapped")
        };
        // Give each ordinary pass a finite sublimit, then prepay every pass
        // against the same request ledger before cloning the frozen AST.
        let source_bytes = self.sources.iter().fold(0u64, |sum, source| {
            sum.saturating_add(source.text.len() as u64)
        });
        let scale = source_bytes
            .saturating_add(owner_type.nodes.len() as u64)
            .saturating_add(128);
        let remaining = budget.remaining();
        let nodes = scale
            .saturating_mul(8)
            .min(remaining.max_generated_nodes / 3);
        let work = scale
            .saturating_mul(32)
            .min(remaining.max_analysis_work / 3);
        let bytes = scale.saturating_mul(32).min(remaining.max_temporary_bytes);
        if nodes == 0 || work == 0 || bytes == 0 {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "generic shape derivation exceeds remaining CT budget",
                Some(insertion),
            )]);
        }
        budget.reserve_generated_nodes(nodes.saturating_mul(3))?;
        budget.reserve_analysis_work(work.saturating_mul(3))?;
        budget.reserve_temporary_bytes(bytes)?;
        budget.reserve_instance()?;
        let mut limits = self.limits.clone();
        limits.max_nodes = limits.max_nodes.min(nodes as usize);
        limits.max_analysis_steps = limits.max_analysis_steps.min(work as usize);
        limits.max_source_bytes = limits.max_source_bytes.min(bytes as usize);
        limits.max_functions = limits.max_functions.min(remaining.max_instances as usize);
        limits.max_values = limits.max_values.min(remaining.max_live_cells as usize);
        limits.max_call_depth = limits.max_call_depth.min(remaining.max_call_depth as usize);
        let mut selected = base.selected.clone();
        let index = selected
            .iter()
            .position(|module| module.id == owner.module)
            .ok_or_else(|| phase("generic type module is absent from frozen shape batch"))?;
        let probe_name = format!(
            "__zil_ct_shape_probe_{}_{}",
            owner.module.0, owner.declaration.0
        );
        if selected[index].items.iter().any(|item| {
            matches!(&item.kind,
            ItemKind::Function(function) if function.name == probe_name)
        }) {
            return Err(phase(
                "generic shape probe name conflicts with frozen source",
            ));
        }
        let probe = s::Function {
            name: probe_name,
            generics: Vec::new(),
            params: vec![s::Param {
                name: "value".into(),
                ty,
                give: false,
                attributes: Vec::new(),
                span: insertion,
            }],
            return_type: None,
            where_clause: Vec::new(),
            contract: Vec::new(),
            body: s::FunctionBody::Block(s::Block {
                statements: Vec::new(),
                span: insertion,
            }),
            span: insertion,
        };
        selected[index].items.push(s::Item {
            kind: ItemKind::Function(probe),
            attributes: Vec::new(),
            public: false,
            span: insertion,
        });
        let refs = selected.iter().collect::<Vec<_>>();
        let imports = base
            .core
            .modules
            .iter()
            .map(|module| module.imports.clone())
            .collect::<Vec<_>>();
        let expanded =
            zil_expand::expand_program_with_limits(self.sources, &refs, &imports, &limits)?;
        let modules = expanded
            .into_iter()
            .map(|(module, _)| ProgramModule {
                core: module.core,
                imports: module.imports,
                exports: module.exports,
            })
            .collect();
        let core = CoreProgram {
            root: base.core.root,
            source_fingerprint: base.core.source_fingerprint,
            modules,
        };
        let core = zil_types::resolve_program_shapes(&core, &limits)?;
        zil_types::check_program(&core, self.target, &limits)?;
        Ok((ShapeSnapshot { core, selected }, bytes))
    }
    fn nominal_root<'a>(
        &self,
        value: &'a CtValue,
    ) -> CompileResult<(&'a CtTypeRef, &'a CtSymbolId)> {
        let CtValue::Type(ty) = value else {
            return Err(error("E-CT-QUERY-TYPE", "reflection requires a Type value"));
        };
        let Some(CtTypeNode::Nominal { symbol, .. }) = ty.nodes.get(ty.root.0 as usize) else {
            return Err(error(
                "E-CT-QUERY-TYPE",
                "reflection requires a nominal type",
            ));
        };
        Ok((ty, symbol))
    }
    fn aggregate_in<'a>(
        &self,
        ready: &'a ShapeSnapshot,
        symbol: &CtSymbolId,
    ) -> CompileResult<&'a zil_ir::CoreAggregate> {
        let span = self
            .index
            .span(symbol)
            .ok_or_else(|| phase("shape symbol is absent"))?;
        ready
            .core
            .modules
            .iter()
            .find(|module| module.core.module_id == symbol.module)
            .and_then(|module| {
                module
                    .core
                    .aggregates
                    .iter()
                    .find(|aggregate| aggregate.span == span)
            })
            .ok_or_else(|| phase("nominal has no completed aggregate shape"))
    }
    fn template(&self, symbol: &CtSymbolId) -> Option<&CoreAggregateTemplate> {
        let ready = self.shape?;
        let span = self.index.span(symbol)?;
        ready
            .core
            .modules
            .iter()
            .find(|module| module.core.module_id == symbol.module)?
            .core
            .aggregate_templates
            .iter()
            .find(|template| template.span == span)
    }
    fn checked_type_nodes(
        ty: &Type,
        budget: &mut CtExecutionBudget,
        depth: usize,
        max_depth: usize,
    ) -> CompileResult<u64> {
        if depth > max_depth {
            return Err(phase("checked generic field type exceeds CT depth"));
        }
        budget.reserve_analysis_work(1)?;
        let mut child = |ty| Self::checked_type_nodes(ty, budget, depth + 1, max_depth);
        let count = match ty {
            Type::Tuple(items) => items.iter().try_fold(1u64, |sum, item| {
                Ok::<_, Vec<Diagnostic>>(sum.saturating_add(Self::checked_type_nodes(
                    item,
                    budget,
                    depth + 1,
                    max_depth,
                )?))
            })?,
            Type::Array { element, .. }
            | Type::Slice { element }
            | Type::OwnedStore { element } => 1u64.saturating_add(child(element)?),
            Type::Address { inner, .. } | Type::Managed(inner) | Type::Desc(inner) => {
                1u64.saturating_add(child(inner)?)
            }
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                let params = parameters.iter().try_fold(1u64, |sum, item| {
                    Ok::<_, Vec<Diagnostic>>(sum.saturating_add(Self::checked_type_nodes(
                        item,
                        budget,
                        depth + 1,
                        max_depth,
                    )?))
                })?;
                params.saturating_add(Self::checked_type_nodes(
                    result,
                    budget,
                    depth + 1,
                    max_depth,
                )?)
            }
            Type::ErrorUnion { error, result } => 1u64
                .saturating_add(child(error)?)
                .saturating_add(child(result)?),
            _ => 1,
        };
        Ok(count)
    }
    fn template_recipe(
        &self,
        owner_type: &CtTypeRef,
        recipe: &CoreAggregateTypeRecipe,
        budget: &mut CtExecutionBudget,
        depth: usize,
    ) -> CompileResult<CtTypeRef> {
        if depth > self.limits.max_call_depth {
            return Err(self.unready("generic shape recipe exceeds CT depth"));
        }
        budget.reserve_analysis_work(1)?;
        match recipe {
            CoreAggregateTypeRecipe::Parameter { index: 0 } => {
                let Some(CtTypeNode::Nominal { arguments, .. }) =
                    owner_type.nodes.get(owner_type.root.0 as usize)
                else {
                    return Err(error(
                        "E-CT-QUERY-TYPE",
                        "generic shape requires a nominal Type",
                    ));
                };
                let [CtTypeArgument::Type(root)] = arguments.as_slice() else {
                    return Err(error(
                        "E-CT-TYPE-ARITY",
                        "generic shape requires one concrete Type argument",
                    ));
                };
                budget.reserve_generated_nodes(owner_type.nodes.len() as u64)?;
                budget
                    .reserve_temporary_bytes((owner_type.nodes.len() as u64).saturating_mul(96))?;
                Ok(CtTypeRef {
                    nodes: owner_type.nodes.clone(),
                    root: *root,
                })
            }
            CoreAggregateTypeRecipe::Concrete(ty) => {
                let count = Self::checked_type_nodes(ty, budget, 0, self.limits.max_call_depth)?;
                budget.reserve_generated_nodes(count)?;
                budget.reserve_temporary_bytes(count.saturating_mul(96))?;
                from_checked_type(
                    ty,
                    self.shape
                        .ok_or_else(|| self.unready("ShapeReady has not completed"))?,
                    self.index,
                    usize::try_from(count).unwrap_or(usize::MAX),
                )
            }
            CoreAggregateTypeRecipe::Store { element } => {
                let mut ty = self.template_recipe(owner_type, element, budget, depth + 1)?;
                budget.reserve_generated_nodes(2)?;
                budget.reserve_temporary_bytes(192)?;
                let store = CtTypeNodeId(u32::try_from(ty.nodes.len()).map_err(|_| {
                    error(
                        "E-CT-TYPE-LIMIT",
                        "generic shape DAG exceeds identity range",
                    )
                })?);
                ty.nodes.push(CtTypeNode::OwnedStore(ty.root));
                let desc = CtTypeNodeId(store.0.checked_add(1).ok_or_else(|| {
                    error(
                        "E-CT-TYPE-LIMIT",
                        "generic shape DAG exceeds identity range",
                    )
                })?);
                ty.nodes.push(CtTypeNode::Desc(store));
                ty.root = desc;
                Ok(ty)
            }
            _ => {
                Err(self
                    .unready("nested generic shape recipe is not complete in this frozen batch"))
            }
        }
    }
    fn template_field_info(
        &self,
        requester: &CtSymbolId,
        owner: &CtSymbolId,
        owner_type: &CtTypeRef,
        field: &CoreAggregateFieldRecipe,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtFieldInfo> {
        if requester.module != owner.module && !field.public {
            return Err(error(
                "E-CT-VISIBILITY",
                "field is private at this CT definition site",
            ));
        }
        let field_symbol = self
            .index
            .symbol_at(owner.module, field.span)
            .ok_or_else(|| phase("field identity is absent from frozen source"))?;
        budget.reserve_generated_nodes(
            2u64.saturating_add(owner_type.nodes.len() as u64)
                .saturating_mul(2),
        )?;
        budget.reserve_temporary_bytes(field.name.len() as u64)?;
        let ty = Self::compact_type_ref(
            self.template_recipe(owner_type, &field.ty, budget, 0)?,
            budget,
        )?;
        Ok(CtFieldInfo {
            owner: owner.clone(),
            owner_type: owner_type.clone(),
            field: field_symbol.clone(),
            name: CtIdent {
                text: field.name.clone(),
                namespace: CtNameNamespace::Member,
                source: CtNameSource::Definition(field_symbol),
                member_owner: Some(owner.clone()),
                member_owner_type: Some(owner_type.clone()),
            },
            ty,
            managed: field.manages,
        })
    }
    fn field_info(
        &self,
        requester: &CtSymbolId,
        owner: &CtSymbolId,
        owner_type: &CtTypeRef,
        field: &zil_ir::CoreField,
        ready: &ShapeSnapshot,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtFieldInfo> {
        if requester.module != owner.module && !field.public {
            return Err(error(
                "E-CT-VISIBILITY",
                "field is private at this CT definition site",
            ));
        }
        let field_symbol = self
            .index
            .symbol_at(owner.module, field.span)
            .ok_or_else(|| phase("field identity is absent from frozen source"))?;
        budget.reserve_analysis_work(1)?;
        budget.reserve_generated_nodes(
            2u64.saturating_add(owner_type.nodes.len() as u64)
                .saturating_mul(2),
        )?;
        budget.reserve_temporary_bytes(field.name.len() as u64)?;
        let ty = if let Type::Record { name, .. }
        | Type::Package { name, .. }
        | Type::Enum { name, .. } = &field.ty
        {
            let generated = ready.core.modules.iter().find_map(|module| {
                module
                    .core
                    .aggregates
                    .iter()
                    .find(|candidate| {
                        name == &candidate.name
                            || name
                                == &format!("@module{}:{}", module.core.module_id.0, candidate.name)
                    })
                    .filter(|candidate| candidate.name.starts_with("$generic_"))
                    .map(|candidate| (module.core.module_id, candidate.span))
            });
            if let Some((module, span)) = generated {
                self.generic_field_type(
                    requester, owner, owner_type, field.span, module, span, budget,
                )?
            } else {
                from_checked_type(
                    &field.ty,
                    ready,
                    self.index,
                    budget.remaining().max_generated_nodes as usize,
                )?
            }
        } else {
            from_checked_type(
                &field.ty,
                ready,
                self.index,
                budget.remaining().max_generated_nodes as usize,
            )?
        };
        Ok(CtFieldInfo {
            owner: owner.clone(),
            owner_type: owner_type.clone(),
            field: field_symbol.clone(),
            name: CtIdent {
                text: field.name.clone(),
                namespace: CtNameNamespace::Member,
                source: CtNameSource::Definition(field_symbol),
                member_owner: Some(owner.clone()),
                member_owner_type: Some(owner_type.clone()),
            },
            ty,
            managed: field.manages,
        })
    }
    // Recursive source substitution carries its definition site, substitutions,
    // output DAG and shared CT budget through one bounded walk.
    #[allow(clippy::too_many_arguments)]
    fn append_source_type(
        &self,
        requester: &CtSymbolId,
        module: ModuleId,
        ty: &s::TypeRef,
        substitutions: &[(&str, CtTypeNodeId)],
        nodes: &mut Vec<CtTypeNode>,
        budget: &mut CtExecutionBudget,
        depth: usize,
    ) -> CompileResult<CtTypeNodeId> {
        if depth > self.limits.max_call_depth {
            return Err(self.unready("generic field type nesting exceeds CT depth"));
        }
        budget.reserve_analysis_work(1)?;
        let node = match &ty.kind {
            s::TypeKind::Group(inner) => {
                return self.append_source_type(
                    requester,
                    module,
                    inner,
                    substitutions,
                    nodes,
                    budget,
                    depth + 1,
                );
            }
            s::TypeKind::Named { path, args }
                if path.len() == 1
                    && args.is_empty()
                    && substitutions.iter().any(|(formal, _)| path[0] == *formal) =>
            {
                return Ok(substitutions
                    .iter()
                    .find(|(formal, _)| path[0] == *formal)
                    .expect("guarded substitution")
                    .1);
            }
            s::TypeKind::Named { path, args } if path.len() == 1 && args.is_empty() => {
                match path[0].as_str() {
                    "Unit" | "void" => CtTypeNode::Primitive(CtScalarTy::Unit),
                    "bool" => CtTypeNode::Primitive(CtScalarTy::Bool),
                    "u8" => CtTypeNode::Primitive(CtScalarTy::U8),
                    "u16" => CtTypeNode::Primitive(CtScalarTy::U16),
                    "i32" => CtTypeNode::Primitive(CtScalarTy::I32),
                    "i64" => CtTypeNode::Primitive(CtScalarTy::I64),
                    "u64" => CtTypeNode::Primitive(CtScalarTy::U64),
                    "usize" => CtTypeNode::Primitive(CtScalarTy::Usize),
                    "ByteBuffer" => CtTypeNode::Builtin(CtBuiltinType::ByteBuffer),
                    "ForeignHandle" => CtTypeNode::Builtin(CtBuiltinType::ForeignHandle),
                    "CWideString" => CtTypeNode::Builtin(CtBuiltinType::CWideString),
                    _ => {
                        let symbol = self.source_nominal(module, ty, false).ok_or_else(|| {
                            self.unready(
                                "nested generic field type is not a frozen nominal declaration",
                            )
                        })?;
                        if !self.index.visible(requester, &symbol) {
                            return Err(error(
                                "E-CT-VISIBILITY",
                                "nested field type is private at this CT definition site",
                            ));
                        }
                        CtTypeNode::Nominal {
                            symbol,
                            arguments: Vec::new(),
                        }
                    }
                }
            }
            s::TypeKind::Named { args, .. } => {
                let symbol = self.source_nominal(module, ty, false).ok_or_else(|| {
                    self.unready("nested generic field type is not a frozen nominal declaration")
                })?;
                if !self.index.visible(requester, &symbol) {
                    return Err(error(
                        "E-CT-VISIBILITY",
                        "nested field type is private at this CT definition site",
                    ));
                }
                budget.reserve_temporary_bytes((args.len() as u64).saturating_mul(32))?;
                let mut arguments = Vec::new();
                for arg in args {
                    match arg {
                        s::GenericArg::Type(inner) => {
                            arguments.push(CtTypeArgument::Type(self.append_source_type(
                                requester,
                                module,
                                inner,
                                substitutions,
                                nodes,
                                budget,
                                depth + 1,
                            )?))
                        }
                        s::GenericArg::Const(_) => {
                            return Err(self.unready(
                                "nested const generic field argument has no completed CT lineage",
                            ));
                        }
                    }
                }
                CtTypeNode::Nominal { symbol, arguments }
            }
            s::TypeKind::Tuple(parts) if parts.is_empty() => {
                CtTypeNode::Primitive(CtScalarTy::Unit)
            }
            s::TypeKind::Tuple(parts) => {
                budget.reserve_temporary_bytes((parts.len() as u64).saturating_mul(8))?;
                CtTypeNode::Tuple(
                    parts
                        .iter()
                        .map(|inner| {
                            self.append_source_type(
                                requester,
                                module,
                                inner,
                                substitutions,
                                nodes,
                                budget,
                                depth + 1,
                            )
                        })
                        .collect::<CompileResult<Vec<_>>>()?,
                )
            }
            s::TypeKind::Array { element, length } => {
                let inner = self.append_source_type(
                    requester,
                    module,
                    element,
                    substitutions,
                    nodes,
                    budget,
                    depth + 1,
                )?;
                if let Some(length) = length {
                    let s::ExprKind::Literal(s::Literal::Integer(text)) = &length.kind else {
                        return Err(self
                            .unready("nonliteral array length has no completed CT Type lineage"));
                    };
                    budget.reserve_analysis_work(text.len() as u64)?;
                    budget.reserve_temporary_bytes(text.len() as u64)?;
                    let length = text.replace('_', "").parse::<u32>().map_err(|_| {
                        self.unready("array length is not a bounded integer constant")
                    })?;
                    budget.release_temporary_bytes(text.len() as u64);
                    CtTypeNode::Array {
                        element: inner,
                        length,
                    }
                } else {
                    CtTypeNode::Slice(inner)
                }
            }
            s::TypeKind::ForLifetime { .. } => {
                let mut inner = ty;
                let mut binders = Vec::new();
                while let s::TypeKind::ForLifetime {
                    lifetime,
                    inner: next,
                } = &inner.kind
                {
                    budget.reserve_analysis_work((lifetime.len() as u64).saturating_add(1))?;
                    budget.reserve_temporary_bytes((lifetime.len() as u64).saturating_add(8))?;
                    binders.push(lifetime.as_str());
                    inner = next;
                }
                let s::TypeKind::Function { params, result } = &inner.kind else {
                    return Err(
                        self.unready("higher-rank Type requires an ordinary pointer signature")
                    );
                };
                budget.reserve_analysis_work(
                    (binders.len() as u64).saturating_mul((params.len() as u64).saturating_add(1)),
                )?;
                budget.reserve_temporary_bytes((params.len() as u64).saturating_mul(16))?;
                let life = super::compiler::source_relation(&binders, params, result);
                let parameters = params
                    .iter()
                    .map(|p| {
                        self.append_source_type(
                            requester,
                            module,
                            p,
                            substitutions,
                            nodes,
                            budget,
                            depth + 1,
                        )
                    })
                    .collect::<CompileResult<Vec<_>>>()?;
                let result = self.append_source_type(
                    requester,
                    module,
                    result,
                    substitutions,
                    nodes,
                    budget,
                    depth + 1,
                )?;
                if life.binder_count == 0 {
                    CtTypeNode::FunctionPointer { parameters, result }
                } else {
                    CtTypeNode::BorrowedFunctionPointer {
                        parameters,
                        result,
                        life,
                    }
                }
            }
            s::TypeKind::Function { params, result } => {
                budget.reserve_temporary_bytes((params.len() as u64).saturating_mul(8))?;
                let parameters = params
                    .iter()
                    .map(|param| {
                        self.append_source_type(
                            requester,
                            module,
                            param,
                            substitutions,
                            nodes,
                            budget,
                            depth + 1,
                        )
                    })
                    .collect::<CompileResult<Vec<_>>>()?;
                let result = self.append_source_type(
                    requester,
                    module,
                    result,
                    substitutions,
                    nodes,
                    budget,
                    depth + 1,
                )?;
                let address = |id: &CtTypeNodeId| {
                    matches!(nodes.get(id.0 as usize), Some(CtTypeNode::Address { .. }))
                };
                if parameters.iter().any(address) || address(&result) {
                    let life = zil_ir::FunctionLifeContract {
                        binder_count: 1,
                        parameter_lives: parameters
                            .iter()
                            .map(|id| address(id).then_some(0))
                            .collect(),
                        result_life: address(&result).then_some(0),
                    };
                    CtTypeNode::BorrowedFunctionPointer {
                        parameters,
                        result,
                        life,
                    }
                } else {
                    CtTypeNode::FunctionPointer { parameters, result }
                }
            }
            s::TypeKind::ForeignFunction {
                abi,
                params,
                result,
            } if abi == "\"C\"" => {
                budget.reserve_temporary_bytes((params.len() as u64).saturating_mul(8))?;
                let parameters = params
                    .iter()
                    .map(|param| {
                        self.append_source_type(
                            requester,
                            module,
                            param,
                            substitutions,
                            nodes,
                            budget,
                            depth + 1,
                        )
                    })
                    .collect::<CompileResult<Vec<_>>>()?;
                let result = self.append_source_type(
                    requester,
                    module,
                    result,
                    substitutions,
                    nodes,
                    budget,
                    depth + 1,
                )?;
                CtTypeNode::ForeignCallback { parameters, result }
            }
            s::TypeKind::Error { error, result } => CtTypeNode::ErrorUnion {
                error: self.append_source_type(
                    requester,
                    module,
                    error,
                    substitutions,
                    nodes,
                    budget,
                    depth + 1,
                )?,
                result: self.append_source_type(
                    requester,
                    module,
                    result,
                    substitutions,
                    nodes,
                    budget,
                    depth + 1,
                )?,
            },
            s::TypeKind::Reference { mutable, inner, .. } => CtTypeNode::Address {
                mutable: *mutable,
                inner: self.append_source_type(
                    requester,
                    module,
                    inner,
                    substitutions,
                    nodes,
                    budget,
                    depth + 1,
                )?,
            },
            _ => return Err(self.unready("nested generic field type has no completed CT lineage")),
        };
        if let CtTypeNode::Nominal { symbol, arguments } = &node
            && let ItemKind::TypeAlias(alias) = &self.nominal(requester, symbol)?.kind
        {
            let rhs = alias
                .value
                .as_ref()
                .ok_or_else(|| self.unready("nested type alias has no checked right hand side"))?;
            if arguments.len() != alias.generics.len() {
                return Err(error(
                    "E-CT-TYPE-ARITY",
                    "nested alias argument count differs from frozen declaration",
                ));
            }
            let charge = (arguments.len() as u64).saturating_mul(32);
            budget.reserve_temporary_bytes(charge)?;
            let substitutions = alias
                .generics
                .iter()
                .zip(arguments)
                .map(|(parameter, argument)| match (&parameter.kind, argument) {
                    (s::GenericParamKind::Type(_), CtTypeArgument::Type(actual)) => {
                        Ok((parameter.name.as_str(), *actual))
                    }
                    _ => Err(self.unready(
                        "nested alias has a non-Type formal without completed CT lineage",
                    )),
                })
                .collect::<CompileResult<Vec<_>>>()?;
            let result = self.append_source_type(
                requester,
                symbol.module,
                rhs,
                &substitutions,
                nodes,
                budget,
                depth + 1,
            );
            budget.release_temporary_bytes(charge);
            return result;
        }
        budget.reserve_generated_nodes(1)?;
        budget.reserve_temporary_bytes(96)?;
        let id = CtTypeNodeId(u32::try_from(nodes.len()).map_err(|_| {
            error(
                "E-CT-TYPE-LIMIT",
                "generic field Type DAG exceeds identity range",
            )
        })?);
        nodes.push(node);
        Ok(id)
    }
    // Both field and instantiated owner identities must survive this checked
    // lineage comparison; packaging them would not reduce the underlying inputs.
    #[allow(clippy::too_many_arguments)]
    fn generic_field_type(
        &self,
        requester: &CtSymbolId,
        owner: &CtSymbolId,
        owner_type: &CtTypeRef,
        field_span: Span,
        expected_module: ModuleId,
        expected_span: Span,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtTypeRef> {
        let ItemKind::Aggregate(declaration) = &self
            .declaration(owner)
            .ok_or_else(|| phase("generic owner declaration is absent"))?
            .kind
        else {
            return Err(phase("generic field owner is not an aggregate"));
        };
        let source_field = declaration
            .fields
            .iter()
            .find(|field| field.span == field_span)
            .ok_or_else(|| phase("generic field source identity differs from checked shape"))?;
        let Some(CtTypeNode::Nominal { arguments, .. }) =
            owner_type.nodes.get(owner_type.root.0 as usize)
        else {
            return Err(error(
                "E-CT-QUERY-TYPE",
                "generic field owner requires nominal Type",
            ));
        };
        if arguments.len() != declaration.generics.len() {
            return Err(error(
                "E-CT-TYPE-ARITY",
                "generic field owner argument count differs from frozen declaration",
            ));
        }
        let substitutions = declaration
            .generics
            .iter()
            .zip(arguments)
            .map(|(parameter, argument)| match (&parameter.kind, argument) {
                (s::GenericParamKind::Type(_), CtTypeArgument::Type(actual)) => {
                    Ok((parameter.name.as_str(), *actual))
                }
                _ => Err(self
                    .unready("generic field has a non-Type formal without completed CT lineage")),
            })
            .collect::<CompileResult<Vec<_>>>()?;
        budget.reserve_generated_nodes(owner_type.nodes.len() as u64)?;
        budget.reserve_temporary_bytes((owner_type.nodes.len() as u64).saturating_mul(96))?;
        let mut nodes = owner_type.nodes.clone();
        let root = self.append_source_type(
            requester,
            owner.module,
            &source_field.ty,
            &substitutions,
            &mut nodes,
            budget,
            0,
        )?;
        let Some(CtTypeNode::Nominal { symbol, .. }) = nodes.get(root.0 as usize) else {
            return Err(phase(
                "checked generic field is nominal but source lineage is not",
            ));
        };
        let expected = self
            .index
            .symbol_at(expected_module, expected_span)
            .ok_or_else(|| phase("checked generic field instance has no frozen declaration"))?;
        if symbol != &expected {
            return Err(phase(
                "checked generic field instance differs from source lineage",
            ));
        }
        Self::compact_type_ref(CtTypeRef { nodes, root }, budget)
    }
    fn matches_checked_type(
        &self,
        checked: &Type,
        value: &CtTypeRef,
        id: CtTypeNodeId,
        derived: &ShapeSnapshot,
        budget: &mut CtExecutionBudget,
        depth: usize,
    ) -> CompileResult<bool> {
        if depth >= self.limits.max_call_depth.min(128) {
            return Err(self.unready("checked alias type exceeds CT depth"));
        }
        budget.reserve_analysis_work(1)?;
        let node = value
            .nodes
            .get(id.0 as usize)
            .ok_or_else(|| error("E-CT-TYPE-LIMIT", "alias Type DAG node is absent"))?;
        let matches = match (checked, node) {
            (Type::Unit, CtTypeNode::Primitive(CtScalarTy::Unit))
            | (Type::Bool, CtTypeNode::Primitive(CtScalarTy::Bool))
            | (Type::U8, CtTypeNode::Primitive(CtScalarTy::U8))
            | (Type::U16, CtTypeNode::Primitive(CtScalarTy::U16))
            | (Type::I32, CtTypeNode::Primitive(CtScalarTy::I32))
            | (Type::I64, CtTypeNode::Primitive(CtScalarTy::I64))
            | (Type::U64, CtTypeNode::Primitive(CtScalarTy::U64))
            | (Type::Usize, CtTypeNode::Primitive(CtScalarTy::Usize))
            | (Type::ByteBuffer, CtTypeNode::Builtin(CtBuiltinType::ByteBuffer))
            | (Type::ForeignHandle, CtTypeNode::Builtin(CtBuiltinType::ForeignHandle))
            | (Type::CWideString, CtTypeNode::Builtin(CtBuiltinType::CWideString)) => true,
            (
                Type::Record { name, .. } | Type::Package { name, .. } | Type::Enum { name, .. },
                CtTypeNode::Nominal { symbol, .. },
            ) => derived.core.modules.iter().any(|module| {
                module.core.aggregates.iter().any(|candidate| {
                    (name == &candidate.name
                        || name
                            == &format!("@module{}:{}", module.core.module_id.0, candidate.name))
                        && self
                            .index
                            .symbol_at(module.core.module_id, candidate.span)
                            .as_ref()
                            == Some(symbol)
                }) || module.core.enums.iter().any(|candidate| {
                    (name == &candidate.name
                        || name
                            == &format!("@module{}:{}", module.core.module_id.0, candidate.name))
                        && self
                            .index
                            .symbol_at(module.core.module_id, candidate.span)
                            .as_ref()
                            == Some(symbol)
                })
            }),
            (Type::Tuple(types), CtTypeNode::Tuple(ids)) if types.len() == ids.len() => {
                let mut valid = true;
                for (ty, id) in types.iter().zip(ids) {
                    valid &=
                        self.matches_checked_type(ty, value, *id, derived, budget, depth + 1)?;
                }
                valid
            }
            (
                Type::Array { element, length },
                CtTypeNode::Array {
                    element: actual,
                    length: count,
                },
            ) if length == count => {
                self.matches_checked_type(element, value, *actual, derived, budget, depth + 1)?
            }
            (Type::Slice { element }, CtTypeNode::Slice(actual)) => {
                self.matches_checked_type(element, value, *actual, derived, budget, depth + 1)?
            }
            (
                Type::Address { mutable, inner },
                CtTypeNode::Address {
                    mutable: actual,
                    inner: id,
                },
            ) if mutable == actual => {
                self.matches_checked_type(inner, value, *id, derived, budget, depth + 1)?
            }
            (
                Type::BorrowedFunctionPointer {
                    parameters,
                    result,
                    life,
                },
                CtTypeNode::BorrowedFunctionPointer {
                    parameters: ids,
                    result: id,
                    life: actual,
                },
            ) if parameters.len() == ids.len() => {
                budget.reserve_analysis_work(
                    (life.parameter_lives.len() as u64)
                        .saturating_mul(2)
                        .saturating_add(life.binder_count as u64),
                )?;
                if life != actual {
                    return Ok(false);
                }
                let mut valid =
                    self.matches_checked_type(result, value, *id, derived, budget, depth + 1)?;
                for (ty, id) in parameters.iter().zip(ids) {
                    valid &=
                        self.matches_checked_type(ty, value, *id, derived, budget, depth + 1)?;
                }
                valid
            }
            (
                Type::FunctionPointer { parameters, result },
                CtTypeNode::FunctionPointer {
                    parameters: ids,
                    result: id,
                },
            )
            | (
                Type::ForeignCallback { parameters, result },
                CtTypeNode::ForeignCallback {
                    parameters: ids,
                    result: id,
                },
            ) if parameters.len() == ids.len() => {
                let mut valid =
                    self.matches_checked_type(result, value, *id, derived, budget, depth + 1)?;
                for (ty, id) in parameters.iter().zip(ids) {
                    valid &=
                        self.matches_checked_type(ty, value, *id, derived, budget, depth + 1)?;
                }
                valid
            }
            (
                Type::ErrorUnion { error, result },
                CtTypeNode::ErrorUnion {
                    error: left,
                    result: right,
                },
            ) => {
                self.matches_checked_type(error, value, *left, derived, budget, depth + 1)?
                    && self.matches_checked_type(
                        result,
                        value,
                        *right,
                        derived,
                        budget,
                        depth + 1,
                    )?
            }
            _ => false,
        };
        Ok(matches)
    }
    fn alias_value(
        &self,
        requester: &CtSymbolId,
        alias: &CtSymbolId,
        handle: &CtTypeRef,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtTypeRef> {
        self.alias_value_inner(requester, alias, handle, budget, 0)
    }
    fn alias_value_inner(
        &self,
        requester: &CtSymbolId,
        alias: &CtSymbolId,
        handle: &CtTypeRef,
        budget: &mut CtExecutionBudget,
        depth: usize,
    ) -> CompileResult<CtTypeRef> {
        if depth >= self.limits.max_call_depth.min(128) {
            return Err(self.unready("type alias chain exceeds CT depth"));
        }
        budget.reserve_analysis_work(1)?;
        self.nominal(requester, alias)?;
        let ItemKind::TypeAlias(declaration) = &self
            .declaration(alias)
            .ok_or_else(|| phase("type alias declaration is absent"))?
            .kind
        else {
            return Err(phase("type alias identity differs from frozen source"));
        };
        let rhs = declaration
            .value
            .as_ref()
            .ok_or_else(|| self.unready("type alias has no completed right hand side"))?;
        let Some(CtTypeNode::Nominal { arguments, .. }) = handle.nodes.get(handle.root.0 as usize)
        else {
            return Err(error(
                "E-CT-QUERY-TYPE",
                "alias requires a nominal Type application",
            ));
        };
        if arguments.len() != declaration.generics.len() {
            return Err(error(
                "E-CT-TYPE-ARITY",
                "alias argument count differs from frozen declaration",
            ));
        }
        let substitutions = declaration
            .generics
            .iter()
            .zip(arguments)
            .map(|(parameter, argument)| match (&parameter.kind, argument) {
                (s::GenericParamKind::Type(_), CtTypeArgument::Type(actual)) => {
                    Ok((parameter.name.as_str(), *actual))
                }
                _ => Err(self.unready("alias has a non-Type formal without completed CT lineage")),
            })
            .collect::<CompileResult<Vec<_>>>()?;
        let (derived, bytes) = self.derived_shape(alias, handle, budget)?;
        let probe_name = format!(
            "__zil_ct_shape_probe_{}_{}",
            alias.module.0, alias.declaration.0
        );
        let checked = derived
            .core
            .modules
            .iter()
            .find(|module| module.core.module_id == alias.module)
            .and_then(|module| {
                module
                    .core
                    .functions
                    .iter()
                    .find(|function| function.name == probe_name)
            })
            .and_then(|function| function.parameters.first())
            .map(|parameter| &parameter.ty)
            .ok_or_else(|| phase("ordinary type alias probe has no checked type"))?;
        budget.reserve_generated_nodes(handle.nodes.len() as u64)?;
        budget.reserve_temporary_bytes((handle.nodes.len() as u64).saturating_mul(96))?;
        let mut nodes = handle.nodes.clone();
        let root = self.append_source_type(
            requester,
            alias.module,
            rhs,
            &substitutions,
            &mut nodes,
            budget,
            0,
        )?;
        let value = Self::compact_type_ref(CtTypeRef { nodes, root }, budget)?;
        let next_alias = match value.nodes.get(value.root.0 as usize) {
            Some(CtTypeNode::Nominal { symbol, .. })
                if matches!(
                    self.declaration(symbol).map(|item| &item.kind),
                    Some(ItemKind::TypeAlias(_))
                ) =>
            {
                Some(symbol.clone())
            }
            _ => None,
        };
        let value = if let Some(next) = next_alias {
            self.alias_value_inner(requester, &next, &value, budget, depth + 1)?
        } else {
            value
        };
        let matching =
            self.matches_checked_type(checked, &value, value.root, &derived, budget, 0)?;
        if !matching {
            return Err(self.unready("alias Type lineage differs from checked ordinary type"));
        }
        budget.release_temporary_bytes(bytes);
        Ok(value)
    }
    fn compact_type_ref(ty: CtTypeRef, budget: &mut CtExecutionBudget) -> CompileResult<CtTypeRef> {
        fn comparison_work(node: &CtTypeNode) -> u64 {
            match node {
                CtTypeNode::Nominal { arguments, .. } => {
                    1u64.saturating_add(arguments.iter().fold(0u64, |sum, argument| {
                        sum.saturating_add(match argument {
                            CtTypeArgument::Type(_) => 1,
                            CtTypeArgument::Scalar(zil_ir::Constant::WideString(value)) => {
                                1u64.saturating_add(value.len() as u64)
                            }
                            CtTypeArgument::Scalar(_) => 1,
                        })
                    }))
                }
                CtTypeNode::Tuple(parts) => 1u64.saturating_add(parts.len() as u64),
                CtTypeNode::FunctionPointer { parameters, .. }
                | CtTypeNode::BorrowedFunctionPointer { parameters, .. }
                | CtTypeNode::ForeignCallback { parameters, .. } => {
                    2u64.saturating_add(parameters.len() as u64)
                }
                _ => 1,
            }
        }
        let count = ty.nodes.len();
        budget.reserve_analysis_work(count as u64)?;
        let edges = ty.nodes.iter().fold(0u64, |sum, node| {
            sum.saturating_add(match node {
                CtTypeNode::Nominal { arguments, .. } => arguments.len() as u64,
                CtTypeNode::Tuple(parts) => parts.len() as u64,
                CtTypeNode::FunctionPointer { parameters, .. }
                | CtTypeNode::BorrowedFunctionPointer { parameters, .. }
                | CtTypeNode::ForeignCallback { parameters, .. } => {
                    (parameters.len() as u64).saturating_add(1)
                }
                CtTypeNode::ErrorUnion { .. } => 2,
                CtTypeNode::Array { .. }
                | CtTypeNode::Slice(_)
                | CtTypeNode::Managed(_)
                | CtTypeNode::Desc(_)
                | CtTypeNode::OwnedStore(_)
                | CtTypeNode::Address { .. } => 1,
                _ => 0,
            })
        });
        budget.reserve_analysis_work((count as u64).saturating_mul(3).saturating_add(edges))?;
        let scratch = (count as u64)
            .saturating_mul(16)
            .saturating_add(edges.saturating_mul(8));
        budget.reserve_temporary_bytes(scratch)?;
        let mut reached = vec![0u8; count];
        let mut pending = vec![(ty.root, false)];
        let mut order = Vec::with_capacity(count);
        while let Some((id, complete)) = pending.pop() {
            let index = id.0 as usize;
            let node = ty.nodes.get(index).ok_or_else(|| {
                error(
                    "E-CT-TYPE-LIMIT",
                    "type DAG node is outside the frozen field value",
                )
            })?;
            if complete {
                reached[index] = 2;
                order.push(index);
                continue;
            }
            if reached[index] == 2 {
                continue;
            }
            if reached[index] == 1 {
                return Err(error("E-CT-TYPE-LIMIT", "type DAG has a cycle"));
            }
            reached[index] = 1;
            pending.push((id, true));
            let children = match node {
                CtTypeNode::Nominal { arguments, .. } => arguments
                    .iter()
                    .filter_map(|arg| match arg {
                        CtTypeArgument::Type(id) => Some(*id),
                        CtTypeArgument::Scalar(_) => None,
                    })
                    .collect::<Vec<_>>(),
                CtTypeNode::Tuple(parts) => parts.clone(),
                CtTypeNode::Array { element, .. }
                | CtTypeNode::Slice(element)
                | CtTypeNode::Managed(element)
                | CtTypeNode::Desc(element)
                | CtTypeNode::OwnedStore(element)
                | CtTypeNode::Address { inner: element, .. } => vec![*element],
                CtTypeNode::FunctionPointer { parameters, result }
                | CtTypeNode::BorrowedFunctionPointer {
                    parameters, result, ..
                }
                | CtTypeNode::ForeignCallback { parameters, result } => {
                    let mut children = parameters.clone();
                    children.push(*result);
                    children
                }
                CtTypeNode::ErrorUnion { error, result } => vec![*error, *result],
                _ => Vec::new(),
            };
            for child in children.into_iter().rev() {
                pending.push((child, false));
            }
        }
        let live = order.len();
        budget.reserve_generated_nodes(live as u64)?;
        budget.reserve_temporary_bytes(
            (live as u64)
                .saturating_mul(96)
                .saturating_add(edges.saturating_mul(32)),
        )?;
        let mut remap = vec![CtTypeNodeId(0); count];
        let mut nodes = Vec::with_capacity(live);
        let mut originals = ty.nodes.into_iter().map(Some).collect::<Vec<_>>();
        for index in order {
            let node = originals[index]
                .take()
                .expect("each reachable DAG node has one postorder entry");
            let mapped = |id: CtTypeNodeId| remap[id.0 as usize];
            let node = match node {
                CtTypeNode::Nominal { symbol, arguments } => CtTypeNode::Nominal {
                    symbol,
                    arguments: arguments
                        .into_iter()
                        .map(|argument| match argument {
                            CtTypeArgument::Type(id) => CtTypeArgument::Type(mapped(id)),
                            CtTypeArgument::Scalar(value) => CtTypeArgument::Scalar(value),
                        })
                        .collect(),
                },
                CtTypeNode::Tuple(parts) => {
                    CtTypeNode::Tuple(parts.into_iter().map(mapped).collect())
                }
                CtTypeNode::Array { element, length } => CtTypeNode::Array {
                    element: mapped(element),
                    length,
                },
                CtTypeNode::BorrowedFunctionPointer {
                    parameters,
                    result,
                    life,
                } => CtTypeNode::BorrowedFunctionPointer {
                    parameters: parameters.into_iter().map(mapped).collect(),
                    result: mapped(result),
                    life,
                },
                CtTypeNode::FunctionPointer { parameters, result } => CtTypeNode::FunctionPointer {
                    parameters: parameters.into_iter().map(mapped).collect(),
                    result: mapped(result),
                },
                CtTypeNode::Address { mutable, inner } => CtTypeNode::Address {
                    mutable,
                    inner: mapped(inner),
                },
                CtTypeNode::Slice(inner) => CtTypeNode::Slice(mapped(inner)),
                CtTypeNode::Managed(inner) => CtTypeNode::Managed(mapped(inner)),
                CtTypeNode::Desc(inner) => CtTypeNode::Desc(mapped(inner)),
                CtTypeNode::OwnedStore(inner) => CtTypeNode::OwnedStore(mapped(inner)),
                CtTypeNode::ForeignCallback { parameters, result } => CtTypeNode::ForeignCallback {
                    parameters: parameters.into_iter().map(mapped).collect(),
                    result: mapped(result),
                },
                CtTypeNode::ErrorUnion { error, result } => CtTypeNode::ErrorUnion {
                    error: mapped(error),
                    result: mapped(result),
                },
                other => other,
            };
            let mut existing = None;
            for (candidate_id, candidate) in nodes.iter().enumerate() {
                budget.reserve_analysis_work(
                    comparison_work(&node).saturating_add(comparison_work(candidate)),
                )?;
                if candidate == &node {
                    existing = Some(candidate_id);
                    break;
                }
            }
            if let Some(existing) = existing {
                remap[index] =
                    CtTypeNodeId(u32::try_from(existing).map_err(|_| {
                        error("E-CT-TYPE-LIMIT", "type DAG exceeds identity range")
                    })?);
            } else {
                remap[index] =
                    CtTypeNodeId(u32::try_from(nodes.len()).map_err(|_| {
                        error("E-CT-TYPE-LIMIT", "type DAG exceeds identity range")
                    })?);
                nodes.push(node);
            }
        }
        budget.release_temporary_bytes(scratch);
        Ok(CtTypeRef {
            root: remap[ty.root.0 as usize],
            nodes,
        })
    }
    fn shape_info(
        &self,
        requester: &CtSymbolId,
        owner: &CtSymbolId,
        owner_type: &CtTypeRef,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtShapeInfo> {
        self.nominal(requester, owner)?;
        if let Some(template) = self.template(owner) {
            let mut fields = Vec::new();
            let mut complete = true;
            for field in &template.fields {
                budget.reserve_analysis_work(1)?;
                if requester.module == owner.module || field.public {
                    fields.push(
                        self.template_field_info(requester, owner, owner_type, field, budget)?,
                    );
                } else {
                    complete = false;
                }
            }
            budget.reserve_generated_nodes(1)?;
            return Ok(CtShapeInfo {
                nominal: owner.clone(),
                visible_fields: fields,
                complete,
            });
        }
        let derived = if matches!(self.declaration(owner).map(|item| &item.kind),
            Some(ItemKind::Aggregate(value)) if !value.generics.is_empty())
        {
            Some(self.derived_shape(owner, owner_type, budget)?)
        } else {
            None
        };
        let ready = derived
            .as_ref()
            .map(|(shape, _)| shape)
            .or(self.shape)
            .ok_or_else(|| self.unready("ShapeReady has not completed"))?;
        let aggregate = self.aggregate_in(ready, owner)?;
        let mut fields = Vec::new();
        let mut complete = true;
        for field in &aggregate.fields {
            budget.reserve_analysis_work(1)?;
            if requester.module == owner.module || field.public {
                fields.push(self.field_info(requester, owner, owner_type, field, ready, budget)?);
            } else {
                complete = false;
            }
        }
        budget.reserve_generated_nodes(1)?;
        if let Some((_, bytes)) = derived {
            budget.release_temporary_bytes(bytes);
        }
        Ok(CtShapeInfo {
            nominal: owner.clone(),
            visible_fields: fields,
            complete,
        })
    }
    fn dependency(
        &self,
        kind: CtQueryKind,
        input: &[CtValue],
        output: &CtValue,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtQueryDependency> {
        let bytes = 25u64.saturating_add((input.len() as u64).saturating_mul(32));
        budget.reserve_temporary_bytes(bytes)?;
        let mut subject = Vec::with_capacity(bytes as usize);
        subject.extend_from_slice(b"zil/ct-query-subject/v1\0");
        subject.push(match kind {
            CtQueryKind::Shape => 1,
            CtQueryKind::Field => 2,
            CtQueryKind::ImplIndex => 3,
            CtQueryKind::Layout => 4,
        });
        for value in input {
            subject.extend_from_slice(&self.value_fingerprint(value, budget)?.0);
        }
        budget.reserve_analysis_work(subject.len() as u64)?;
        let subject_fingerprint = Fingerprint::of(&subject);
        budget.release_temporary_bytes(bytes);
        let result_fingerprint = self.value_fingerprint(output, budget)?;
        Ok(CtQueryDependency {
            kind,
            subject_fingerprint,
            result_fingerprint,
        })
    }
    fn text_name(&self, symbol: &CtSymbolId) -> CompileResult<(String, CtNameNamespace)> {
        let item = self
            .declaration(symbol)
            .ok_or_else(|| phase("definition is absent from source index"))?;
        Ok(match &item.kind {
            ItemKind::Function(f) => (f.name.clone(), CtNameNamespace::Value),
            ItemKind::Aggregate(a) => (a.name.clone(), CtNameNamespace::Type),
            ItemKind::Enum(e) => (e.name.clone(), CtNameNamespace::Type),
            ItemKind::Trait(t) => (t.name.clone(), CtNameNamespace::Type),
            ItemKind::TypeAlias(t) => (t.name.clone(), CtNameNamespace::Type),
            ItemKind::Const(c) => (c.name.clone(), CtNameNamespace::Value),
            _ => return Err(phase("definition has no CT name")),
        })
    }
    fn source_nominal(
        &self,
        module: ModuleId,
        ty: &s::TypeRef,
        trait_name: bool,
    ) -> Option<CtSymbolId> {
        let s::TypeKind::Named { path, .. } = &ty.kind else {
            return None;
        };
        let (owner, name) = match path.as_slice() {
            [name] => (module, name.as_str()),
            [alias, name] => (self.index.imported_module(module, alias)?, name.as_str()),
            _ => return None,
        };
        let item = self
            .syntax
            .iter()
            .find(|candidate| candidate.id == owner)?
            .items
            .iter()
            .find(|item| match &item.kind {
                ItemKind::Trait(value) if trait_name => value.name == name,
                ItemKind::Aggregate(value) if !trait_name => value.name == name,
                ItemKind::Enum(value) if !trait_name => value.name == name,
                ItemKind::TypeAlias(value) if !trait_name => value.name == name,
                _ => false,
            })?;
        self.index.symbol_at(owner, item.span)
    }
}

impl CtReadView for FrozenReadView<'_> {
    fn authenticate_argument(&self, index: u32, ty: &CtTy, value: &CtValue) -> CompileResult<()> {
        match (ty, value) {
            (CtTy::TokenTree, CtValue::TokenTree(tree))
                if index == 0 && self.input == Some(tree) =>
            {
                Ok(())
            }
            (CtTy::TokenTree, _) => Err(error(
                "E-CT-INPUT",
                "TokenTree argument is not the authenticated provider input",
            )),
            (CtTy::Type, CtValue::Type(_))
            | (CtTy::Syntax(_), CtValue::Syntax(_))
            | (CtTy::Ident, CtValue::Ident(_)) => Ok(()),
            (CtTy::Scalar(_), CtValue::Scalar(_)) | (CtTy::Text, CtValue::Text(_)) => Ok(()),
            _ => Err(error(
                "E-CT-INPUT",
                "CT argument category differs from the declared parameter",
            )),
        }
    }
    fn type_name(
        &self,
        requester: &CtSymbolId,
        symbol: &CtSymbolId,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtTypeRef> {
        let item = self.nominal(requester, symbol)?;
        budget.reserve_analysis_work(1)?;
        budget.reserve_generated_nodes(1)?;
        let handle = CtTypeRef {
            nodes: vec![CtTypeNode::Nominal {
                symbol: symbol.clone(),
                arguments: Vec::new(),
            }],
            root: CtTypeNodeId(0),
        };
        if matches!(&item.kind, ItemKind::TypeAlias(alias) if alias.generics.is_empty()) {
            return self.alias_value(requester, symbol, &handle, budget);
        }
        Ok(handle)
    }
    fn type_apply(
        &self,
        requester: &CtSymbolId,
        nominal: &CtSymbolId,
        args: &[CtValue],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtTypeRef> {
        let item = self.nominal(requester, nominal)?;
        let params = match &item.kind {
            ItemKind::Aggregate(value) => &value.generics,
            ItemKind::Enum(value) => &value.generics,
            ItemKind::TypeAlias(value) => &value.generics,
            _ => {
                return Err(phase(
                    "type application target has no nominal generic declaration",
                ));
            }
        };
        if params.len() != args.len() {
            return Err(error(
                "E-CT-TYPE-ARITY",
                "generic Type application argument count differs from the frozen declaration",
            ));
        }
        let total = args.iter().fold(1u64, |count, arg| {
            count.saturating_add(match arg {
                CtValue::Type(ty) => ty.nodes.len() as u64,
                _ => 0,
            })
        });
        if total > u32::MAX as u64 {
            return Err(error(
                "E-CT-TYPE-LIMIT",
                "generic Type DAG exceeds identity range",
            ));
        }
        budget.reserve_analysis_work(total.saturating_add(params.len() as u64))?;
        budget.reserve_generated_nodes(total)?;
        budget.reserve_temporary_bytes(total.saturating_mul(96))?;
        let mut nodes = Vec::new();
        let mut arguments = Vec::new();
        for (param, arg) in params.iter().zip(args) {
            match (&param.kind, arg) {
                (s::GenericParamKind::Type(_), CtValue::Type(value)) => {
                    let base = u32::try_from(nodes.len()).map_err(|_| {
                        error("E-CT-TYPE-LIMIT", "generic Type node identity overflow")
                    })?;
                    let shift = |id: CtTypeNodeId| CtTypeNodeId(id.0.saturating_add(base));
                    for node in &value.nodes {
                        nodes.push(match node {
                            CtTypeNode::Primitive(value) => CtTypeNode::Primitive(*value),
                            CtTypeNode::Builtin(value) => CtTypeNode::Builtin(*value),
                            CtTypeNode::Nominal { symbol, arguments } => CtTypeNode::Nominal {
                                symbol: symbol.clone(),
                                arguments: arguments
                                    .iter()
                                    .map(|arg| match arg {
                                        CtTypeArgument::Type(id) => {
                                            CtTypeArgument::Type(shift(*id))
                                        }
                                        CtTypeArgument::Scalar(value) => {
                                            CtTypeArgument::Scalar(value.clone())
                                        }
                                    })
                                    .collect(),
                            },
                            CtTypeNode::Tuple(parts) => {
                                CtTypeNode::Tuple(parts.iter().copied().map(shift).collect())
                            }
                            CtTypeNode::Array { element, length } => CtTypeNode::Array {
                                element: shift(*element),
                                length: *length,
                            },
                            CtTypeNode::BorrowedFunctionPointer {
                                parameters,
                                result,
                                life,
                            } => CtTypeNode::BorrowedFunctionPointer {
                                parameters: parameters.iter().copied().map(shift).collect(),
                                result: shift(*result),
                                life: life.clone(),
                            },
                            CtTypeNode::FunctionPointer { parameters, result } => {
                                CtTypeNode::FunctionPointer {
                                    parameters: parameters.iter().copied().map(shift).collect(),
                                    result: shift(*result),
                                }
                            }
                            CtTypeNode::Address { mutable, inner } => CtTypeNode::Address {
                                mutable: *mutable,
                                inner: shift(*inner),
                            },
                            CtTypeNode::Slice(inner) => CtTypeNode::Slice(shift(*inner)),
                            CtTypeNode::Managed(inner) => CtTypeNode::Managed(shift(*inner)),
                            CtTypeNode::Desc(inner) => CtTypeNode::Desc(shift(*inner)),
                            CtTypeNode::OwnedStore(inner) => CtTypeNode::OwnedStore(shift(*inner)),
                            CtTypeNode::ForeignCallback { parameters, result } => {
                                CtTypeNode::ForeignCallback {
                                    parameters: parameters.iter().copied().map(shift).collect(),
                                    result: shift(*result),
                                }
                            }
                            CtTypeNode::Opaque { identity } => CtTypeNode::Opaque {
                                identity: identity.clone(),
                            },
                            CtTypeNode::ErrorUnion { error, result } => CtTypeNode::ErrorUnion {
                                error: shift(*error),
                                result: shift(*result),
                            },
                        });
                    }
                    arguments.push(CtTypeArgument::Type(shift(value.root)));
                }
                (s::GenericParamKind::Const(_), CtValue::Scalar(value))
                    if !matches!(value, zil_ir::Constant::Unit) =>
                {
                    arguments.push(CtTypeArgument::Scalar(value.clone()))
                }
                (s::GenericParamKind::Lifetime, _) => {
                    return Err(phase(
                        "lifetime arguments cannot be reified as CT Type values",
                    ));
                }
                _ => {
                    return Err(error(
                        "E-CT-TYPE-ARG",
                        "generic Type argument category differs from the frozen parameter",
                    ));
                }
            }
        }
        let root = CtTypeNodeId(
            u32::try_from(nodes.len())
                .map_err(|_| error("E-CT-TYPE-LIMIT", "generic Type node identity overflow"))?,
        );
        nodes.push(CtTypeNode::Nominal {
            symbol: nominal.clone(),
            arguments,
        });
        let applied = Self::compact_type_ref(CtTypeRef { nodes, root }, budget)?;
        if matches!(&item.kind, ItemKind::TypeAlias(_)) {
            return self.alias_value(requester, nominal, &applied, budget);
        }
        if (matches!(&item.kind, ItemKind::Aggregate(value) if !value.generics.is_empty())
            || matches!(&item.kind, ItemKind::Enum(value) if !value.generics.is_empty()))
            && self.template(nominal).is_none()
        {
            let (_, bytes) = self.derived_shape(nominal, &applied, budget)?;
            budget.release_temporary_bytes(bytes);
        }
        Ok(applied)
    }
    fn definition_ident(
        &self,
        requester: &CtSymbolId,
        symbol: &CtSymbolId,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtIdent> {
        if !self.index.visible(requester, symbol) {
            return Err(error("E-CT-VISIBILITY", "definition name is private"));
        }
        let (text, namespace) = self.text_name(symbol)?;
        budget.reserve_analysis_work(1)?;
        budget.reserve_generated_nodes(1)?;
        budget.reserve_temporary_bytes(text.len() as u64)?;
        Ok(CtIdent {
            text,
            namespace,
            source: CtNameSource::Definition(symbol.clone()),
            member_owner: None,
            member_owner_type: None,
        })
    }
    fn parse_ident(
        &self,
        input: &CtTokenTree,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtIdent> {
        if self.input != Some(input) {
            return Err(error(
                "E-CT-INPUT",
                "identifier parser needs the authenticated provider input",
            ));
        }
        let [root] = input.roots.as_slice() else {
            return Err(error(
                "E-CT-CATEGORY",
                "identifier input needs one outer delimiter",
            ));
        };
        let Some(CtTokenNode::Group { children, .. }) = input.nodes.get(root.0 as usize) else {
            return Err(error(
                "E-CT-CATEGORY",
                "identifier input needs one outer delimiter",
            ));
        };
        let [child] = children.as_slice() else {
            return Err(error(
                "E-CT-CATEGORY",
                "identifier input must contain exactly one name",
            ));
        };
        let Some(CtTokenNode::Token(token)) = input.nodes.get(child.0 as usize) else {
            return Err(error(
                "E-CT-CATEGORY",
                "identifier input must contain one token",
            ));
        };
        if token.class != CtTokenClass::Identifier {
            return Err(error(
                "E-CT-CATEGORY",
                "identifier input token is not a name",
            ));
        }
        let ident = token
            .name
            .as_ref()
            .ok_or_else(|| error("E-CT-INPUT", "identifier has no authenticated input name"))?;
        budget.reserve_analysis_work(2)?;
        budget.reserve_generated_nodes(1)?;
        budget.reserve_temporary_bytes(ident.text.len() as u64)?;
        Ok(ident.clone())
    }
    fn parse(
        &self,
        category: CtSyntaxCategory,
        input: &CtTokenTree,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtSyntaxTree> {
        if self.input != Some(input) {
            return Err(error(
                "E-CT-INPUT",
                "syntax parser needs the authenticated provider input",
            ));
        }
        let [root] = input.roots.as_slice() else {
            return Err(error(
                "E-CT-CATEGORY",
                "provider parser needs one outer delimiter",
            ));
        };
        let Some(CtTokenNode::Group {
            children, origin, ..
        }) = input.nodes.get(root.0 as usize)
        else {
            return Err(error(
                "E-CT-CATEGORY",
                "provider parser needs one outer delimiter",
            ));
        };
        let byte_count = input
            .nodes
            .iter()
            .filter_map(|node| match node {
                CtTokenNode::Token(token) => Some(token.text.len() as u64),
                _ => None,
            })
            .sum::<u64>();
        budget
            .reserve_analysis_work(input.nodes.len().saturating_add(input.origins.len()) as u64)?;
        budget
            .reserve_generated_nodes(input.nodes.len().saturating_add(input.origins.len()) as u64)?;
        budget.reserve_temporary_bytes(byte_count)?;
        let nodes = input
            .nodes
            .iter()
            .map(|node| match node {
                CtTokenNode::Token(token) => CtSyntaxNode::Token(token.clone()),
                CtTokenNode::Group {
                    delimiter,
                    children,
                    origin,
                } => CtSyntaxNode::Group {
                    delimiter: *delimiter,
                    children: children.iter().map(|id| CtSyntaxNodeId(id.0)).collect(),
                    origin: *origin,
                },
            })
            .collect::<Vec<_>>();
        let mut tree = CtSyntaxTree {
            category,
            nodes,
            roots: children.iter().map(|id| CtSyntaxNodeId(id.0)).collect(),
            origins: input.origins.clone(),
        };
        if tree.roots.len() != 1
            && matches!(category, CtSyntaxCategory::Expr | CtSyntaxCategory::Type)
        {
            let id = CtSyntaxNodeId(
                u32::try_from(tree.nodes.len())
                    .map_err(|_| error("E-CT-CATEGORY", "provider syntax node overflow"))?,
            );
            let children = std::mem::take(&mut tree.roots);
            tree.nodes.push(CtSyntaxNode::Group {
                delimiter: CtDelimiter::Parenthesis,
                children,
                origin: *origin,
            });
            tree.roots.push(id);
        }
        let file = input
            .origins
            .first()
            .map_or(FileId(0), |origin| origin.span.file);
        let module = self
            .sources
            .iter()
            .find(|source| source.file_id == file)
            .map_or(ModuleId(0), |source| source.module_id);
        let mut limits = self.limits.clone();
        limits.max_nodes = limits
            .max_nodes
            .min(budget.remaining().max_analysis_work as usize);
        limits.max_call_depth = limits
            .max_call_depth
            .min(budget.remaining().max_call_depth as usize);
        render::parse(
            &tree,
            file,
            module,
            &self.index.aliases(module),
            &limits,
            budget,
        )?;
        Ok(tree)
    }
    fn query(
        &self,
        requester: &CtSymbolId,
        kind: CtQueryKind,
        args: &[CtValue],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<(CtValue, CtQueryDependency)> {
        if !self.index.contains(requester)
            || !self.snapshot.requested_queries.contains(&kind)
            || !stage_allows(self.snapshot.ready, kind)
        {
            return Err(self.unready("query is not ready or granted in this immutable CT snapshot"));
        }
        let value = match kind {
            CtQueryKind::Shape => {
                let (ty, nominal) = self.nominal_root(
                    args.first()
                        .ok_or_else(|| error("E-CT-QUERY-ARG", "shape query needs a type"))?,
                )?;
                CtValue::ShapeInfo(self.shape_info(requester, nominal, ty, budget)?)
            }
            CtQueryKind::Field => {
                let (ty, nominal) = self.nominal_root(
                    args.first()
                        .ok_or_else(|| error("E-CT-QUERY-ARG", "field query needs a type"))?,
                )?;
                self.nominal(requester, nominal)?;
                let name = match args.get(1) {
                    Some(CtValue::Text(name)) => name.as_str(),
                    _ => return Err(error("E-CT-QUERY-ARG", "field query needs a text name")),
                };
                if let Some(template) = self.template(nominal) {
                    let field = template
                        .fields
                        .iter()
                        .find(|field| field.name == name)
                        .ok_or_else(|| {
                            error(
                                "E-CT-FIELD-NOT-FOUND",
                                "field is not in the completed generic shape",
                            )
                        })?;
                    CtValue::FieldInfo(
                        self.template_field_info(requester, nominal, ty, field, budget)?,
                    )
                } else {
                    let derived = if matches!(self.declaration(nominal).map(|item| &item.kind),
                    Some(ItemKind::Aggregate(value)) if !value.generics.is_empty())
                    {
                        Some(self.derived_shape(nominal, ty, budget)?)
                    } else {
                        None
                    };
                    let ready = derived
                        .as_ref()
                        .map(|(shape, _)| shape)
                        .or(self.shape)
                        .ok_or_else(|| self.unready("ShapeReady has not completed"))?;
                    let field = self
                        .aggregate_in(ready, nominal)?
                        .fields
                        .iter()
                        .find(|field| field.name == name)
                        .ok_or_else(|| {
                            error(
                                "E-CT-FIELD-NOT-FOUND",
                                "field is not in the completed shape",
                            )
                        })?;
                    let result = self.field_info(requester, nominal, ty, field, ready, budget)?;
                    if let Some((_, bytes)) = derived {
                        budget.release_temporary_bytes(bytes);
                    }
                    CtValue::FieldInfo(result)
                }
            }
            CtQueryKind::ImplIndex => {
                let index = self.impl_index.ok_or_else(|| {
                    self.unready("ImplIndexReady has not completed for this generation batch")
                })?;
                let mut results = Vec::new();
                for argument in args {
                    budget.reserve_analysis_work(1)?;
                    let (ty, subject) = self.nominal_root(argument)?;
                    self.nominal(requester, subject)?;
                    let Some(CtTypeNode::Nominal { arguments, .. }) =
                        ty.nodes.get(ty.root.0 as usize)
                    else {
                        unreachable!("nominal_root checked")
                    };
                    for (module, span, trait_type, target_type) in &index.entries {
                        budget.reserve_analysis_work(1)?;
                        if self.source_nominal(*module, target_type, false).as_ref()
                            != Some(subject)
                        {
                            continue;
                        }
                        let s::TypeKind::Named {
                            args: target_args, ..
                        } = &target_type.kind
                        else {
                            continue;
                        };
                        let s::TypeKind::Named {
                            args: trait_args, ..
                        } = &trait_type.kind
                        else {
                            continue;
                        };
                        if !target_args.is_empty()
                            || !arguments.is_empty()
                            || !trait_args.is_empty()
                        {
                            return Err(self.unready("generic trait implementation specialization is not a completed immutable index entry"));
                        }
                        let Some(trait_symbol) = self.source_nominal(*module, trait_type, true)
                        else {
                            return Err(self.unready(
                                "trait declaration is not in the completed implementation index",
                            ));
                        };
                        if !self.index.visible(requester, &trait_symbol) {
                            continue;
                        }
                        let implementation =
                            self.index.symbol_at(*module, *span).ok_or_else(|| {
                                self.unready("implementation has no frozen declaration identity")
                            })?;
                        if results.iter().any(|candidate| matches!(candidate,
                            CtValue::ImplInfo(info) if info.implementation == implementation && info.for_type == *ty)) {
                            continue;
                        }
                        budget
                            .reserve_generated_nodes(1u64.saturating_add(ty.nodes.len() as u64))?;
                        budget.reserve_temporary_bytes(
                            (ty.nodes.len() as u64)
                                .saturating_mul(96)
                                .saturating_add(256),
                        )?;
                        results.push(CtValue::ImplInfo(CtImplInfo {
                            trait_symbol,
                            implementation,
                            for_type: ty.clone(),
                        }));
                    }
                }
                CtValue::List(results)
            }
            CtQueryKind::Layout => {
                if self.snapshot.target_fingerprint != Some(self.target.fingerprint()) {
                    return Err(self
                        .unready("LayoutReady target does not match this immutable CT snapshot"));
                }
                let (ty, nominal) = self.nominal_root(
                    args.first()
                        .ok_or_else(|| error("E-CT-QUERY-ARG", "layout query needs a type"))?,
                )?;
                self.nominal(requester, nominal)?;
                let ready = self.shape.ok_or_else(|| {
                    self.unready("LayoutReady has not completed for this generation batch")
                })?;
                let ordinary = if matches!(self.declaration(nominal).map(|item| &item.kind),
                    Some(ItemKind::Aggregate(value)) if !value.generics.is_empty())
                {
                    let (derived, bytes) = self.derived_shape(nominal, ty, budget)?;
                    let aggregate = self.aggregate_in(&derived, nominal)?;
                    if requester.module != nominal.module
                        && aggregate.fields.iter().any(|field| !field.public)
                    {
                        return Err(error(
                            "E-CT-VISIBILITY",
                            "layout offsets require a complete visible shape",
                        ));
                    }
                    budget.reserve_analysis_work(aggregate.fields.len() as u64)?;
                    budget.reserve_temporary_bytes(
                        (aggregate.fields.len() as u64).saturating_mul(128),
                    )?;
                    let name = format!("@module{}:{}", nominal.module.0, aggregate.name);
                    let fields = aggregate
                        .fields
                        .iter()
                        .map(|field| (field.name.clone(), field.ty.clone()))
                        .collect();
                    let ordinary = match aggregate.kind {
                        AggregateKind::Struct => Type::Record { name, fields },
                        AggregateKind::Package => Type::Package { name, fields },
                        AggregateKind::Resource => {
                            return Err(
                                self.unready("bare resource layout is not an ordinary value type")
                            );
                        }
                    };
                    budget.release_temporary_bytes(bytes);
                    ordinary
                } else if matches!(self.declaration(nominal).map(|item| &item.kind),
                    Some(ItemKind::Enum(value)) if !value.generics.is_empty())
                {
                    let (derived, bytes) = self.derived_shape(nominal, ty, budget)?;
                    let span = self
                        .index
                        .span(nominal)
                        .ok_or_else(|| phase("generic enum declaration is absent"))?;
                    let enumeration = derived
                        .core
                        .modules
                        .iter()
                        .find(|module| module.core.module_id == nominal.module)
                        .and_then(|module| {
                            module
                                .core
                                .enums
                                .iter()
                                .find(|enumeration| enumeration.span == span)
                        })
                        .ok_or_else(|| {
                            self.unready(
                                "generic enum instance has not completed ordinary checking",
                            )
                        })?;
                    budget.reserve_analysis_work(enumeration.variants.len() as u64)?;
                    budget.reserve_temporary_bytes(
                        (enumeration.variants.len() as u64).saturating_mul(128),
                    )?;
                    let ordinary = Type::Enum {
                        name: format!("@module{}:{}", nominal.module.0, enumeration.name),
                        variants: enumeration.variants.clone(),
                    };
                    budget.release_temporary_bytes(bytes);
                    ordinary
                } else {
                    if let Ok(aggregate) = self.aggregate_in(ready, nominal)
                        && requester.module != nominal.module
                        && aggregate.fields.iter().any(|field| !field.public)
                    {
                        return Err(error(
                            "E-CT-VISIBILITY",
                            "layout offsets require a complete visible shape",
                        ));
                    }
                    ordinary_type::checked_type(
                        ty,
                        ready,
                        self.index,
                        self.limits.max_call_depth,
                        budget,
                    )?
                };
                let (size, alignment, field_offsets) = zil_backend_llvm::ct_layout_of_checked_type(
                    &ordinary,
                    self.target,
                    self.limits,
                    &mut |charge| {
                        budget.reserve_analysis_work(charge as u64)?;
                        budget.reserve_temporary_bytes(charge as u64)
                    },
                )?;
                budget.reserve_generated_nodes(field_offsets.len().saturating_add(1) as u64)?;
                CtValue::LayoutInfo(CtLayoutInfo {
                    target_fingerprint: self.target.fingerprint(),
                    size,
                    alignment,
                    field_offsets,
                })
            }
        };
        let dependency = self.dependency(kind, args, &value, budget)?;
        Ok((value, dependency))
    }
    fn build_syntax(
        &self,
        kind: CtSyntaxBuilder,
        args: &[CtValue],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtSyntaxTree> {
        match (kind, args) {
            (CtSyntaxBuilder::Literal, [CtValue::Scalar(value)]) => {
                let (text, class) = match value {
                    Constant::Unit => {
                        return Self::syntax_tokens(
                            CtSyntaxCategory::Expr,
                            vec![
                                ("(".into(), CtTokenClass::Punctuation, None),
                                (")".into(), CtTokenClass::Punctuation, None),
                            ],
                            budget,
                        );
                    }
                    Constant::Bool(value) => (value.to_string(), CtTokenClass::Keyword),
                    Constant::U8(value) => (value.to_string(), CtTokenClass::Integer),
                    Constant::U16(value) => (value.to_string(), CtTokenClass::Integer),
                    Constant::I32(value) => (value.to_string(), CtTokenClass::Integer),
                    Constant::I64(value) => (value.to_string(), CtTokenClass::Integer),
                    Constant::U64(value) => (format!("{value}u64"), CtTokenClass::Integer),
                    Constant::Usize(value) => (format!("{value}usize"), CtTokenClass::Integer),
                    _ => {
                        return Err(error(
                            "E-CT-SYNTAX-LITERAL",
                            "runtime value is not a CT literal",
                        ));
                    }
                };
                if let Some(stripped) = text.strip_prefix('-') {
                    return Self::syntax_tokens(
                        CtSyntaxCategory::Expr,
                        vec![
                            ("-".into(), CtTokenClass::Punctuation, None),
                            (stripped.into(), class, None),
                        ],
                        budget,
                    );
                }
                Self::syntax_tokens(CtSyntaxCategory::Expr, vec![(text, class, None)], budget)
            }
            (CtSyntaxBuilder::Literal, [CtValue::Text(value)]) => Self::syntax_tokens(
                CtSyntaxCategory::Expr,
                vec![(format!("{value:?}"), CtTokenClass::String, None)],
                budget,
            ),
            (CtSyntaxBuilder::TypeRef, [CtValue::Type(ty)]) => {
                let mut parts = Vec::new();
                self.type_spelling(ty, ty.root, &mut parts, 0, budget)?;
                Self::syntax_tokens(CtSyntaxCategory::Type, parts, budget)
            }
            (CtSyntaxBuilder::NameExpr, [CtValue::Ident(ident)])
                if ident.namespace == CtNameNamespace::Value =>
            {
                Self::syntax_tokens(
                    CtSyntaxCategory::Expr,
                    vec![(
                        ident.text.clone(),
                        CtTokenClass::Identifier,
                        Some(ident.clone()),
                    )],
                    budget,
                )
            }
            _ => Err(error(
                "E-CT-SYNTAX-BUILD",
                "syntax builder arguments or name namespace are invalid",
            )),
        }
    }
    fn instantiate_quote(
        &self,
        template: &CtTemplate,
        holes: &[CtValue],
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtSyntaxTree> {
        Self::instantiate(template, holes, budget)
    }
    fn token_inspect(
        &self,
        source: &CtValue,
        projection: CtTokenProjection,
        index: Option<i64>,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<CtValue> {
        use CtTokenProjection as P;
        let tree = self.token_tree()?;
        budget.reserve_analysis_work(1)?;
        let indexed = || -> CompileResult<usize> {
            usize::try_from(
                index
                    .ok_or_else(|| error("E-CT-TOKEN-INDEX", "token projection needs an index"))?,
            )
            .map_err(|_| error("E-CT-TOKEN-INDEX", "token projection index is negative"))
        };
        let cursor =
            |node: CtTokenNodeId, budget: &mut CtExecutionBudget| -> CompileResult<CtValue> {
                budget.reserve_generated_nodes(1)?;
                Ok(CtValue::TokenCursor(CtTokenCursor {
                    tree_fingerprint: self.token_fingerprint(tree, budget)?,
                    node,
                }))
            };
        match (source, projection) {
            (CtValue::TokenTree(actual), P::RootCount) if actual == tree => Ok(CtValue::Scalar(
                Constant::I64(i64::try_from(tree.roots.len()).map_err(|_| {
                    error(
                        "E-CT-TOKEN-LIMIT",
                        "token root count exceeds target integer",
                    )
                })?),
            )),
            (CtValue::TokenTree(actual), P::RootAt) if actual == tree => {
                let node = *tree
                    .roots
                    .get(indexed()?)
                    .ok_or_else(|| error("E-CT-TOKEN-INDEX", "token root index is out of range"))?;
                cursor(node, budget)
            }
            (CtValue::TokenCursor(actual), projection)
                if actual.tree_fingerprint == self.token_fingerprint(tree, budget)? =>
            {
                let node = tree.nodes.get(actual.node.0 as usize).ok_or_else(|| {
                    error("E-CT-TOKEN-INDEX", "token cursor node is out of range")
                })?;
                match (node, projection) {
                    (CtTokenNode::Token(token), P::Class) => {
                        Ok(CtValue::Scalar(Constant::I32(token.class as i32)))
                    }
                    (CtTokenNode::Group { .. }, P::Class) => Ok(CtValue::Scalar(Constant::I32(5))),
                    (CtTokenNode::Group { delimiter, .. }, P::Delimiter) => {
                        Ok(CtValue::Scalar(Constant::I32(*delimiter as i32)))
                    }
                    (CtTokenNode::Token(_), P::Delimiter) => {
                        Err(error("E-CT-TOKEN-KIND", "token has no delimiter"))
                    }
                    (CtTokenNode::Token(token), P::Text) => {
                        budget.reserve_temporary_bytes(token.text.len() as u64)?;
                        Ok(CtValue::Text(token.text.clone()))
                    }
                    (CtTokenNode::Group { .. }, P::Text) => {
                        Err(error("E-CT-TOKEN-KIND", "group has no token text"))
                    }
                    (CtTokenNode::Group { children, .. }, P::ChildCount) => Ok(CtValue::Scalar(
                        Constant::I64(i64::try_from(children.len()).map_err(|_| {
                            error(
                                "E-CT-TOKEN-LIMIT",
                                "token child count exceeds target integer",
                            )
                        })?),
                    )),
                    (CtTokenNode::Token(_), P::ChildCount) => Ok(CtValue::Scalar(Constant::I64(0))),
                    (CtTokenNode::Group { children, .. }, P::ChildAt) => {
                        let node = *children.get(indexed()?).ok_or_else(|| {
                            error("E-CT-TOKEN-INDEX", "token child index is out of range")
                        })?;
                        cursor(node, budget)
                    }
                    (CtTokenNode::Token(_), P::ChildAt) => {
                        Err(error("E-CT-TOKEN-KIND", "token has no child"))
                    }
                    (CtTokenNode::Token(token), P::InputName) => {
                        let ident = token.name.as_ref().ok_or_else(|| {
                            error("E-CT-TOKEN-NAME", "token is not an input identifier")
                        })?;
                        budget.reserve_generated_nodes(1)?;
                        budget.reserve_temporary_bytes(ident.text.len() as u64)?;
                        Ok(CtValue::Ident(ident.clone()))
                    }
                    (CtTokenNode::Group { .. }, P::InputName) => {
                        Err(error("E-CT-TOKEN-NAME", "group is not an input identifier"))
                    }
                    _ => Err(error(
                        "E-CT-TOKEN-KIND",
                        "token projection does not match this node",
                    )),
                }
            }
            _ => Err(error(
                "E-CT-INPUT",
                "token cursor is not bound to the authenticated provider input",
            )),
        }
    }
}
