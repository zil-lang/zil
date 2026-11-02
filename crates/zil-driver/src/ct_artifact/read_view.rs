//! Closed CT services over the pre-generation source snapshot. Host callbacks
//! return pure data and never execute a provider or grant ordinary Verify.
use super::{authority::stage_allows, ordinary_type, render, snapshot::ShapeSnapshot, source_index::SourceIndex, type_ref::from_checked_type};
use zil_base::{CompileResult, Diagnostic, FileId, Fingerprint, ModuleId, Phase, SourceUnit, Span};
use zil_ct::{CtExecutionBudget, CtReadView};
use zil_ir::{Constant, ct_artifact::*};
use zil_syntax::ast::{self as s, ItemKind};

fn error(code: &str, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, code, None, message)]
}
fn phase(message: &str) -> Vec<Diagnostic> { error("E-CT-PHASE-DEPENDENCY", message) }

pub(super) struct FrozenReadView<'a> {
    pub index: &'a SourceIndex,
    pub sources: &'a [SourceUnit],
    pub syntax: &'a [&'a s::Module],
    pub shape: Option<&'a ShapeSnapshot>,
    pub shape_failure: Option<&'a [Diagnostic]>,
    pub input: Option<&'a CtTokenTree>,
    pub limits: &'a zil_base::Limits,
    pub target: &'a zil_base::TargetProfile,
    pub snapshot: &'a CtSnapshotRequest,
}

impl FrozenReadView<'_> {
    fn unready(&self, message: &str) -> Vec<Diagnostic> {
        let cycle = self.shape_failure.is_some_and(|errors| errors.iter().any(|error| {
            let code = error.code.to_ascii_lowercase();
            let text = error.message.to_ascii_lowercase();
            code.contains("cycle") || code.contains("recursive") || text.contains("cycle") || text.contains("recursive")
        }));
        error(if cycle { "E-CT-PHASE-CYCLE" } else { "E-CT-PHASE-DEPENDENCY" }, message)
    }
    fn type_spelling(&self, ty: &CtTypeRef, node: CtTypeNodeId, out: &mut Vec<(String, CtTokenClass, Option<CtIdent>)>, depth: usize) -> CompileResult<()> {
        if depth > 128 { return Err(error("E-CT-TYPE-PHASE", "type syntax nesting exceeds budget")); }
        let actual = ty.nodes.get(node.0 as usize).ok_or_else(|| error("E-CT-TYPE-PHASE", "type node is out of range"))?;
        let punctuation = |out: &mut Vec<_>, text: &str| out.push((text.to_owned(), CtTokenClass::Punctuation, None));
        match actual {
            CtTypeNode::Primitive(scalar) => out.push((match scalar {
                CtScalarTy::Unit => "Unit", CtScalarTy::Bool => "bool", CtScalarTy::U8 => "u8", CtScalarTy::U16 => "u16", CtScalarTy::I32 => "i32", CtScalarTy::I64 => "i64",
            }.into(), CtTokenClass::Identifier, None)),
            CtTypeNode::Nominal { symbol, arguments } => {
                let (text, namespace) = self.text_name(symbol)?;
                out.push((text.clone(), CtTokenClass::Identifier, Some(CtIdent { text, namespace, source: CtNameSource::Definition(symbol.clone()), member_owner: None, member_owner_type: None })));
                if !arguments.is_empty() {
                    punctuation(out, "<");
                    for (index, argument) in arguments.iter().enumerate() {
                        if index != 0 { punctuation(out, ","); }
                        match argument {
                            CtTypeArgument::Type(id) => self.type_spelling(ty, *id, out, depth + 1)?,
                            CtTypeArgument::Scalar(value) => out.push((format!("{value:?}"), CtTokenClass::Integer, None)),
                        }
                    }
                    punctuation(out, ">");
                }
            }
            CtTypeNode::Array { element, length } => {
                punctuation(out, "["); self.type_spelling(ty, *element, out, depth + 1)?;
                punctuation(out, ";"); out.push((length.to_string(), CtTokenClass::Integer, None)); punctuation(out, "]");
            }
            CtTypeNode::Tuple(items) => {
                punctuation(out, "(");
                for (index, item) in items.iter().enumerate() { if index != 0 { punctuation(out, ","); } self.type_spelling(ty, *item, out, depth + 1)?; }
                punctuation(out, ")");
            }
            _ => return Err(phase("this checked Type constructor has no materializable source spelling")),
        }
        Ok(())
    }
    fn syntax_tokens(category: CtSyntaxCategory, parts: Vec<(String, CtTokenClass, Option<CtIdent>)>, budget: &mut CtExecutionBudget) -> CompileResult<CtSyntaxTree> {
        let count = u64::try_from(parts.len()).map_err(|_| error("E-CT-SYNTAX-LIMIT", "syntax node count overflow"))?;
        let bytes = parts.iter().map(|(text, _, _)| text.len() as u64).sum::<u64>();
        budget.reserve_analysis_work(count.saturating_add(1))?;
        budget.reserve_generated_nodes(count.saturating_add(1))?;
        budget.reserve_temporary_bytes(bytes)?;
        let origin = CtOriginId(0);
        let mut tree = CtSyntaxTree { category, nodes: Vec::with_capacity(parts.len()), roots: Vec::with_capacity(parts.len()),
            origins: vec![CtOrigin { span: Span::default(), parent: None, role: CtOriginRole::Generated }] };
        for (text, class, name) in parts {
            let id = CtSyntaxNodeId(u32::try_from(tree.nodes.len()).map_err(|_| error("E-CT-SYNTAX-LIMIT", "syntax node identity overflow"))?);
            tree.nodes.push(CtSyntaxNode::Token(CtToken { class, text, origin, name }));
            tree.roots.push(id);
        }
        if tree.roots.len() != 1 && matches!(category, CtSyntaxCategory::Expr | CtSyntaxCategory::Type) {
            let id = CtSyntaxNodeId(u32::try_from(tree.nodes.len()).map_err(|_| error("E-CT-SYNTAX-LIMIT", "syntax node identity overflow"))?);
            let children = std::mem::take(&mut tree.roots);
            tree.nodes.push(CtSyntaxNode::Group { delimiter: CtDelimiter::Parenthesis, children, origin });
            tree.roots.push(id);
        }
        Ok(tree)
    }
    fn instantiate(template: &CtTemplate, holes: &[CtValue], budget: &mut CtExecutionBudget) -> CompileResult<CtSyntaxTree> {
        if holes.len() != template.holes.len() { return Err(error("E-CT-QUOTE-HOLE", "quote hole count differs from verified template")); }
        let mut needed_nodes = template.syntax.nodes.len() as u64;
        let mut needed_origins = template.syntax.origins.len() as u64;
        let mut needed_bytes = 0u64;
        for node in &template.syntax.nodes {
            match node {
                CtSyntaxNode::Token(token) => needed_bytes = needed_bytes.saturating_add(token.text.len() as u64),
                CtSyntaxNode::Hole { index, .. } => match holes.get(*index as usize) {
                    Some(CtValue::Syntax(tree)) => {
                        needed_nodes = needed_nodes.saturating_add(tree.nodes.len() as u64);
                        needed_origins = needed_origins.saturating_add(tree.origins.len() as u64);
                        for node in &tree.nodes { if let CtSyntaxNode::Token(token) = node { needed_bytes = needed_bytes.saturating_add(token.text.len() as u64); } }
                    }
                    Some(CtValue::Ident(ident)) => {
                        needed_nodes = needed_nodes.saturating_add(1);
                        needed_origins = needed_origins.saturating_add(1);
                        needed_bytes = needed_bytes.saturating_add(ident.text.len() as u64);
                    }
                    _ => return Err(error("E-CT-QUOTE-HOLE", "quote hole value is not Syntax or Ident")),
                },
                _ => {}
            }
        }
        budget.reserve_analysis_work(needed_nodes.saturating_add(needed_origins))?;
        budget.reserve_generated_nodes(needed_nodes.saturating_add(needed_origins))?;
        budget.reserve_temporary_bytes(needed_bytes)?;
        let mut out = CtSyntaxTree { category: template.syntax.category, nodes: Vec::new(), roots: Vec::new(), origins: template.syntax.origins.clone() };
        let mut map = Vec::<Vec<CtSyntaxNodeId>>::with_capacity(template.syntax.nodes.len());
        for node in &template.syntax.nodes {
            let mapped = match node {
                CtSyntaxNode::Token(token) => {
                    let id = CtSyntaxNodeId(u32::try_from(out.nodes.len()).map_err(|_| error("E-CT-QUOTE-LIMIT", "quote node identity overflow"))?);
                    out.nodes.push(CtSyntaxNode::Token(token.clone()));
                    vec![id]
                }
                CtSyntaxNode::Group { delimiter, children, origin } => {
                    let mut mapped_children = Vec::new();
                    for child in children {
                        mapped_children.extend(map.get(child.0 as usize).ok_or_else(|| error("E-CT-QUOTE-HOLE", "quote child is not earlier than its group"))?.iter().copied());
                    }
                    let id = CtSyntaxNodeId(u32::try_from(out.nodes.len()).map_err(|_| error("E-CT-QUOTE-LIMIT", "quote node identity overflow"))?);
                    out.nodes.push(CtSyntaxNode::Group { delimiter: *delimiter, children: mapped_children, origin: *origin });
                    vec![id]
                }
                CtSyntaxNode::Hole { index, origin } => {
                    let value = holes.get(*index as usize).ok_or_else(|| error("E-CT-QUOTE-HOLE", "quote hole index is out of range"))?;
                    let expected = template.holes.get(*index as usize).ok_or_else(|| error("E-CT-QUOTE-HOLE", "quote hole kind is missing"))?;
                    match (expected, value) {
                        (CtHoleKind::Ident(namespace), CtValue::Ident(ident)) if namespace == &ident.namespace => {
                            let new_origin = CtOriginId(u32::try_from(out.origins.len()).map_err(|_| error("E-CT-QUOTE-LIMIT", "quote origin identity overflow"))?);
                            let span = out.origins.get(origin.0 as usize).ok_or_else(|| error("E-CT-QUOTE-HOLE", "quote hole origin is missing"))?.span;
                            out.origins.push(CtOrigin { span, parent: Some(*origin), role: CtOriginRole::Splice });
                            let id = CtSyntaxNodeId(u32::try_from(out.nodes.len()).map_err(|_| error("E-CT-QUOTE-LIMIT", "quote node identity overflow"))?);
                            out.nodes.push(CtSyntaxNode::Token(CtToken { class: CtTokenClass::Identifier, text: ident.text.clone(), origin: new_origin, name: Some(ident.clone()) }));
                            vec![id]
                        }
                        (CtHoleKind::Expr, CtValue::Syntax(tree)) if tree.category == CtSyntaxCategory::Expr => Self::graft(&mut out, tree, *origin)?,
                        (CtHoleKind::Type, CtValue::Syntax(tree)) if tree.category == CtSyntaxCategory::Type => Self::graft(&mut out, tree, *origin)?,
                        (CtHoleKind::StmtList, CtValue::Syntax(tree)) if tree.category == CtSyntaxCategory::StmtList => Self::graft(&mut out, tree, *origin)?,
                        (CtHoleKind::ItemList, CtValue::Syntax(tree)) if tree.category == CtSyntaxCategory::ItemList => Self::graft(&mut out, tree, *origin)?,
                        _ => return Err(error("E-CT-QUOTE-HOLE", "quote hole category or name namespace differs")),
                    }
                }
            };
            map.push(mapped);
        }
        for root in &template.syntax.roots {
            out.roots.extend(map.get(root.0 as usize).ok_or_else(|| error("E-CT-QUOTE-HOLE", "quote root is out of range"))?.iter().copied());
        }
        Ok(out)
    }
    fn graft(out: &mut CtSyntaxTree, tree: &CtSyntaxTree, parent: CtOriginId) -> CompileResult<Vec<CtSyntaxNodeId>> {
        let origin_base = u32::try_from(out.origins.len()).map_err(|_| error("E-CT-QUOTE-LIMIT", "quote origin identity overflow"))?;
        for origin in &tree.origins {
            let parent = match origin.parent { Some(id) => Some(CtOriginId(origin_base.checked_add(id.0).ok_or_else(|| error("E-CT-QUOTE-LIMIT", "quote origin identity overflow"))?)), None => Some(parent) };
            out.origins.push(CtOrigin { span: origin.span, parent, role: origin.role });
        }
        let mut map = Vec::with_capacity(tree.nodes.len());
        for node in &tree.nodes {
            let mapped = match node {
                CtSyntaxNode::Token(token) => {
                    let mut token = token.clone();
                    token.origin = CtOriginId(origin_base.checked_add(token.origin.0).ok_or_else(|| error("E-CT-QUOTE-LIMIT", "quote origin identity overflow"))?);
                    CtSyntaxNode::Token(token)
                }
                CtSyntaxNode::Group { delimiter, children, origin } => CtSyntaxNode::Group {
                    delimiter: *delimiter,
                    children: children.iter().map(|id| map.get(id.0 as usize).copied().ok_or_else(|| error("E-CT-QUOTE-HOLE", "spliced syntax has invalid child"))).collect::<CompileResult<Vec<_>>>()?,
                    origin: CtOriginId(origin_base.checked_add(origin.0).ok_or_else(|| error("E-CT-QUOTE-LIMIT", "quote origin identity overflow"))?),
                },
                CtSyntaxNode::Hole { .. } => return Err(error("E-CT-QUOTE-HOLE", "evaluated Syntax still contains a hole")),
            };
            let id = CtSyntaxNodeId(u32::try_from(out.nodes.len()).map_err(|_| error("E-CT-QUOTE-LIMIT", "quote node identity overflow"))?);
            out.nodes.push(mapped);
            map.push(id);
        }
        tree.roots.iter().map(|id| map.get(id.0 as usize).copied().ok_or_else(|| error("E-CT-QUOTE-HOLE", "spliced syntax has invalid root"))).collect()
    }
    fn token_tree(&self) -> CompileResult<&CtTokenTree> {
        self.input.ok_or_else(|| error("E-CT-INPUT", "no authenticated provider token input exists"))
    }
    fn token_fingerprint(tree: &CtTokenTree) -> Fingerprint {
        Fingerprint::of(format!("zil/ct-token-cursor/v1\0{tree:?}").as_bytes())
    }
    fn declaration(&self, symbol: &CtSymbolId) -> Option<&s::Item> {
        self.syntax.iter().find(|module| module.id == symbol.module)?.items.get(symbol.declaration.0 as usize)
    }
    fn nominal(&self, requester: &CtSymbolId, symbol: &CtSymbolId) -> CompileResult<&s::Item> {
        if !self.index.visible(requester, symbol) { return Err(error("E-CT-VISIBILITY", "CT type declaration is absent or hidden at this definition site")); }
        let item = self.declaration(symbol).ok_or_else(|| phase("type declaration is not in the frozen source batch"))?;
        if !matches!(item.kind, ItemKind::Aggregate(_) | ItemKind::Enum(_) | ItemKind::TypeAlias(_)) { return Err(phase("symbol is not a ready nominal type")); }
        let ready = self.shape.ok_or_else(|| self.unready("ShapeReady has not completed"))?;
        let found = ready.core.modules.iter().any(|module| module.core.module_id == symbol.module && (
            module.core.aggregates.iter().any(|aggregate| aggregate.span == item.span)
            || module.core.enums.iter().any(|enumeration| enumeration.span == item.span)
        ));
        if !found { return Err(phase("type shape depends on a generated or unfinished declaration")); }
        Ok(item)
    }
    fn nominal_root<'a>(&self, value: &'a CtValue) -> CompileResult<(&'a CtTypeRef, &'a CtSymbolId)> {
        let CtValue::Type(ty) = value else { return Err(error("E-CT-QUERY-TYPE", "reflection requires a Type value")); };
        let Some(CtTypeNode::Nominal { symbol, .. }) = ty.nodes.get(ty.root.0 as usize) else { return Err(error("E-CT-QUERY-TYPE", "reflection requires a nominal type")); };
        Ok((ty, symbol))
    }
    fn aggregate(&self, symbol: &CtSymbolId) -> CompileResult<&zil_ir::CoreAggregate> {
        let ready = self.shape.ok_or_else(|| self.unready("ShapeReady has not completed"))?;
        let span = self.index.span(symbol).ok_or_else(|| phase("shape symbol is absent"))?;
        ready.core.modules.iter().find(|module| module.core.module_id == symbol.module)
            .and_then(|module| module.core.aggregates.iter().find(|aggregate| aggregate.span == span))
            .ok_or_else(|| phase("nominal has no completed aggregate shape"))
    }
    fn field_info(&self, requester: &CtSymbolId, owner: &CtSymbolId, owner_type: &CtTypeRef, field: &zil_ir::CoreField, budget: &mut CtExecutionBudget) -> CompileResult<CtFieldInfo> {
        if requester.module != owner.module && !field.public { return Err(error("E-CT-VISIBILITY", "field is private at this CT definition site")); }
        let field_symbol = self.index.symbol_at(owner.module, field.span).ok_or_else(|| phase("field identity is absent from frozen source"))?;
        let ready = self.shape.ok_or_else(|| self.unready("ShapeReady has not completed"))?;
        budget.reserve_analysis_work(1)?;
        budget.reserve_generated_nodes(2u64.saturating_add(owner_type.nodes.len() as u64).saturating_mul(2))?;
        budget.reserve_temporary_bytes(field.name.len() as u64)?;
        let ty = from_checked_type(&field.ty, ready, self.index, budget.remaining().max_generated_nodes as usize)?;
        Ok(CtFieldInfo {
            owner: owner.clone(), owner_type: owner_type.clone(), field: field_symbol.clone(),
            name: CtIdent { text: field.name.clone(), namespace: CtNameNamespace::Member,
                source: CtNameSource::Definition(field_symbol), member_owner: Some(owner.clone()), member_owner_type: Some(owner_type.clone()) },
            ty, managed: field.manages,
        })
    }
    fn shape_info(&self, requester: &CtSymbolId, owner: &CtSymbolId, owner_type: &CtTypeRef, budget: &mut CtExecutionBudget) -> CompileResult<CtShapeInfo> {
        self.nominal(requester, owner)?;
        let aggregate = self.aggregate(owner)?;
        let mut fields = Vec::new();
        let mut complete = true;
        for field in &aggregate.fields {
            budget.reserve_analysis_work(1)?;
            if requester.module == owner.module || field.public {
                fields.push(self.field_info(requester, owner, owner_type, field, budget)?);
            } else { complete = false; }
        }
        budget.reserve_generated_nodes(1)?;
        Ok(CtShapeInfo { nominal: owner.clone(), visible_fields: fields, complete })
    }
    fn dependency(kind: CtQueryKind, input: &[CtValue], output: &CtValue) -> CtQueryDependency {
        CtQueryDependency { kind, subject_fingerprint: Fingerprint::of(format!("{input:?}").as_bytes()),
            result_fingerprint: Fingerprint::of(format!("{output:?}").as_bytes()) }
    }
    fn text_name(&self, symbol: &CtSymbolId) -> CompileResult<(String, CtNameNamespace)> {
        let item = self.declaration(symbol).ok_or_else(|| phase("definition is absent from source index"))?;
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
}

impl CtReadView for FrozenReadView<'_> {
    fn authenticate_argument(&self, index: u32, ty: &CtTy, value: &CtValue) -> CompileResult<()> {
        match (ty, value) {
            (CtTy::TokenTree, CtValue::TokenTree(tree)) if index == 0 && self.input == Some(tree) => Ok(()),
            (CtTy::TokenTree, _) => Err(error("E-CT-INPUT", "TokenTree argument is not the authenticated provider input")),
            (CtTy::Type, CtValue::Type(_)) | (CtTy::Syntax(_), CtValue::Syntax(_)) | (CtTy::Ident, CtValue::Ident(_)) => Ok(()),
            (CtTy::Scalar(_), CtValue::Scalar(_)) | (CtTy::Text, CtValue::Text(_)) => Ok(()),
            _ => Err(error("E-CT-INPUT", "CT argument category differs from the declared parameter")),
        }
    }
    fn type_name(&self, requester: &CtSymbolId, symbol: &CtSymbolId, budget: &mut CtExecutionBudget) -> CompileResult<CtTypeRef> {
        self.nominal(requester, symbol)?;
        budget.reserve_analysis_work(1)?;
        budget.reserve_generated_nodes(1)?;
        Ok(CtTypeRef { nodes: vec![CtTypeNode::Nominal { symbol: symbol.clone(), arguments: Vec::new() }], root: CtTypeNodeId(0) })
    }
    fn type_apply(&self, requester: &CtSymbolId, nominal: &CtSymbolId, args: &[CtValue], budget: &mut CtExecutionBudget) -> CompileResult<CtTypeRef> {
        self.nominal(requester, nominal)?;
        if !args.is_empty() { return Err(phase("generic type application has no completed shape in this batch")); }
        self.type_name(requester, nominal, budget)
    }
    fn definition_ident(&self, requester: &CtSymbolId, symbol: &CtSymbolId, budget: &mut CtExecutionBudget) -> CompileResult<CtIdent> {
        if !self.index.visible(requester, symbol) { return Err(error("E-CT-VISIBILITY", "definition name is private")); }
        let (text, namespace) = self.text_name(symbol)?;
        budget.reserve_analysis_work(1)?;
        budget.reserve_generated_nodes(1)?;
        budget.reserve_temporary_bytes(text.len() as u64)?;
        Ok(CtIdent { text, namespace, source: CtNameSource::Definition(symbol.clone()), member_owner: None, member_owner_type: None })
    }
    fn parse_ident(&self, input: &CtTokenTree, budget: &mut CtExecutionBudget) -> CompileResult<CtIdent> {
        if self.input != Some(input) { return Err(error("E-CT-INPUT", "identifier parser needs the authenticated provider input")); }
        let [root] = input.roots.as_slice() else { return Err(error("E-CT-CATEGORY", "identifier input needs one outer delimiter")); };
        let Some(CtTokenNode::Group { children, .. }) = input.nodes.get(root.0 as usize) else {
            return Err(error("E-CT-CATEGORY", "identifier input needs one outer delimiter"));
        };
        let [child] = children.as_slice() else { return Err(error("E-CT-CATEGORY", "identifier input must contain exactly one name")); };
        let Some(CtTokenNode::Token(token)) = input.nodes.get(child.0 as usize) else {
            return Err(error("E-CT-CATEGORY", "identifier input must contain one token"));
        };
        if token.class != CtTokenClass::Identifier { return Err(error("E-CT-CATEGORY", "identifier input token is not a name")); }
        let ident = token.name.as_ref().ok_or_else(|| error("E-CT-INPUT", "identifier has no authenticated input name"))?;
        budget.reserve_analysis_work(2)?;
        budget.reserve_generated_nodes(1)?;
        budget.reserve_temporary_bytes(ident.text.len() as u64)?;
        Ok(ident.clone())
    }
    fn parse(&self, category: CtSyntaxCategory, input: &CtTokenTree, budget: &mut CtExecutionBudget) -> CompileResult<CtSyntaxTree> {
        if self.input != Some(input) { return Err(error("E-CT-INPUT", "syntax parser needs the authenticated provider input")); }
        let [root] = input.roots.as_slice() else { return Err(error("E-CT-CATEGORY", "provider parser needs one outer delimiter")); };
        let Some(CtTokenNode::Group { children, origin, .. }) = input.nodes.get(root.0 as usize) else {
            return Err(error("E-CT-CATEGORY", "provider parser needs one outer delimiter"));
        };
        let byte_count = input.nodes.iter().filter_map(|node| match node { CtTokenNode::Token(token) => Some(token.text.len() as u64), _ => None }).sum::<u64>();
        budget.reserve_analysis_work(input.nodes.len().saturating_add(input.origins.len()) as u64)?;
        budget.reserve_generated_nodes(input.nodes.len().saturating_add(input.origins.len()) as u64)?;
        budget.reserve_temporary_bytes(byte_count)?;
        let nodes = input.nodes.iter().map(|node| match node {
            CtTokenNode::Token(token) => CtSyntaxNode::Token(token.clone()),
            CtTokenNode::Group { delimiter, children, origin } => CtSyntaxNode::Group {
                delimiter: *delimiter, children: children.iter().map(|id| CtSyntaxNodeId(id.0)).collect(), origin: *origin,
            },
        }).collect::<Vec<_>>();
        let mut tree = CtSyntaxTree { category, nodes, roots: children.iter().map(|id| CtSyntaxNodeId(id.0)).collect(), origins: input.origins.clone() };
        if tree.roots.len() != 1 && matches!(category, CtSyntaxCategory::Expr | CtSyntaxCategory::Type) {
            let id = CtSyntaxNodeId(u32::try_from(tree.nodes.len()).map_err(|_| error("E-CT-CATEGORY", "provider syntax node overflow"))?);
            let children = std::mem::take(&mut tree.roots);
            tree.nodes.push(CtSyntaxNode::Group { delimiter: CtDelimiter::Parenthesis, children, origin: *origin });
            tree.roots.push(id);
        }
        let file = input.origins.first().map_or(FileId(0), |origin| origin.span.file);
        let module = self.sources.iter().find(|source| source.file_id == file).map_or(ModuleId(0), |source| source.module_id);
        let mut limits = self.limits.clone();
        limits.max_nodes = limits.max_nodes.min(budget.remaining().max_analysis_work as usize);
        limits.max_call_depth = limits.max_call_depth.min(budget.remaining().max_call_depth as usize);
        render::parse(&tree, file, module, &limits, budget)?;
        Ok(tree)
    }
    fn query(&self, requester: &CtSymbolId, kind: CtQueryKind, args: &[CtValue], budget: &mut CtExecutionBudget) -> CompileResult<(CtValue, CtQueryDependency)> {
        if !self.index.contains(requester) || !self.snapshot.requested_queries.contains(&kind)
            || !stage_allows(self.snapshot.ready, kind) {
            return Err(self.unready("query is not ready or granted in this immutable CT snapshot"));
        }
        let value = match kind {
            CtQueryKind::Shape => {
                let (ty, nominal) = self.nominal_root(args.first().ok_or_else(|| error("E-CT-QUERY-ARG", "shape query needs a type"))?)?;
                CtValue::ShapeInfo(self.shape_info(requester, nominal, ty, budget)?)
            }
            CtQueryKind::Field => {
                let (ty, nominal) = self.nominal_root(args.first().ok_or_else(|| error("E-CT-QUERY-ARG", "field query needs a type"))?)?;
                self.nominal(requester, nominal)?;
                let name = match args.get(1) { Some(CtValue::Text(name)) => name.as_str(), _ => return Err(error("E-CT-QUERY-ARG", "field query needs a text name")) };
                let field = self.aggregate(nominal)?.fields.iter().find(|field| field.name == name)
                    .ok_or_else(|| error("E-CT-FIELD-NOT-FOUND", "field is not in the completed shape"))?;
                CtValue::FieldInfo(self.field_info(requester, nominal, ty, field, budget)?)
            }
            CtQueryKind::ImplIndex => return Err(phase("ImplIndexReady has not completed for this generation batch")),
            CtQueryKind::Layout => {
                if self.snapshot.target_fingerprint != Some(self.target.fingerprint()) {
                    return Err(self.unready("LayoutReady target does not match this immutable CT snapshot"));
                }
                let (ty, nominal) = self.nominal_root(args.first().ok_or_else(|| error("E-CT-QUERY-ARG", "layout query needs a type"))?)?;
                self.nominal(requester, nominal)?;
                let ready = self.shape.ok_or_else(|| self.unready("LayoutReady has not completed for this generation batch"))?;
                let ordinary = ordinary_type::checked_type(ty, ready, self.index, self.limits.max_call_depth)?;
                let (size, alignment, field_offsets) = zil_backend_llvm::ct_layout_of_checked_type(
                    &ordinary, self.target, self.limits, &mut |charge| {
                        budget.reserve_analysis_work(charge as u64)?;
                        budget.reserve_temporary_bytes(charge as u64)
                    },
                )?;
                budget.reserve_generated_nodes(field_offsets.len().saturating_add(1) as u64)?;
                CtValue::LayoutInfo(CtLayoutInfo { target_fingerprint: self.target.fingerprint(), size, alignment, field_offsets })
            }
        };
        let dependency = Self::dependency(kind, args, &value);
        Ok((value, dependency))
    }
    fn build_syntax(&self, kind: CtSyntaxBuilder, args: &[CtValue], budget: &mut CtExecutionBudget) -> CompileResult<CtSyntaxTree> {
        match (kind, args) {
            (CtSyntaxBuilder::Literal, [CtValue::Scalar(value)]) => {
                let (text, class) = match value {
                    Constant::Unit => return Self::syntax_tokens(CtSyntaxCategory::Expr,
                        vec![("(".into(), CtTokenClass::Punctuation, None), (")".into(), CtTokenClass::Punctuation, None)], budget),
                    Constant::Bool(value) => (value.to_string(), CtTokenClass::Keyword),
                    Constant::U8(value) => (value.to_string(), CtTokenClass::Integer),
                    Constant::U16(value) => (value.to_string(), CtTokenClass::Integer),
                    Constant::I32(value) => (value.to_string(), CtTokenClass::Integer),
                    Constant::I64(value) => (value.to_string(), CtTokenClass::Integer),
                    _ => return Err(error("E-CT-SYNTAX-LITERAL", "runtime value is not a CT literal")),
                };
                if text.starts_with('-') {
                    return Self::syntax_tokens(CtSyntaxCategory::Expr,
                        vec![("-".into(), CtTokenClass::Punctuation, None), (text[1..].into(), class, None)], budget);
                }
                Self::syntax_tokens(CtSyntaxCategory::Expr, vec![(text, class, None)], budget)
            }
            (CtSyntaxBuilder::Literal, [CtValue::Text(value)]) => Self::syntax_tokens(CtSyntaxCategory::Expr,
                vec![(format!("{value:?}"), CtTokenClass::String, None)], budget),
            (CtSyntaxBuilder::TypeRef, [CtValue::Type(ty)]) => {
                let mut parts = Vec::new();
                self.type_spelling(ty, ty.root, &mut parts, 0)?;
                Self::syntax_tokens(CtSyntaxCategory::Type, parts, budget)
            }
            (CtSyntaxBuilder::NameExpr, [CtValue::Ident(ident)]) if ident.namespace == CtNameNamespace::Value => {
                Self::syntax_tokens(CtSyntaxCategory::Expr, vec![(ident.text.clone(), CtTokenClass::Identifier, Some(ident.clone()))], budget)
            }
            _ => Err(error("E-CT-SYNTAX-BUILD", "syntax builder arguments or name namespace are invalid")),
        }
    }
    fn instantiate_quote(&self, template: &CtTemplate, holes: &[CtValue], budget: &mut CtExecutionBudget) -> CompileResult<CtSyntaxTree> {
        Self::instantiate(template, holes, budget)
    }
    fn token_inspect(&self, source: &CtValue, projection: CtTokenProjection, index: Option<i64>, budget: &mut CtExecutionBudget) -> CompileResult<CtValue> {
        use CtTokenProjection as P;
        let tree = self.token_tree()?;
        budget.reserve_analysis_work(1)?;
        let indexed = || -> CompileResult<usize> {
            usize::try_from(index.ok_or_else(|| error("E-CT-TOKEN-INDEX", "token projection needs an index"))?)
                .map_err(|_| error("E-CT-TOKEN-INDEX", "token projection index is negative"))
        };
        let cursor = |node: CtTokenNodeId, budget: &mut CtExecutionBudget| -> CompileResult<CtValue> {
            budget.reserve_generated_nodes(1)?;
            Ok(CtValue::TokenCursor(CtTokenCursor { tree_fingerprint: Self::token_fingerprint(tree), node }))
        };
        match (source, projection) {
            (CtValue::TokenTree(actual), P::RootCount) if actual == tree => Ok(CtValue::Scalar(Constant::I64(i64::try_from(tree.roots.len()).map_err(|_| error("E-CT-TOKEN-LIMIT", "token root count exceeds target integer"))?))),
            (CtValue::TokenTree(actual), P::RootAt) if actual == tree => {
                let node = *tree.roots.get(indexed()?).ok_or_else(|| error("E-CT-TOKEN-INDEX", "token root index is out of range"))?;
                cursor(node, budget)
            }
            (CtValue::TokenCursor(actual), projection) if actual.tree_fingerprint == Self::token_fingerprint(tree) => {
                let node = tree.nodes.get(actual.node.0 as usize).ok_or_else(|| error("E-CT-TOKEN-INDEX", "token cursor node is out of range"))?;
                match (node, projection) {
                    (CtTokenNode::Token(token), P::Class) => Ok(CtValue::Scalar(Constant::I32(token.class as i32))),
                    (CtTokenNode::Group { .. }, P::Class) => Ok(CtValue::Scalar(Constant::I32(5))),
                    (CtTokenNode::Group { delimiter, .. }, P::Delimiter) => Ok(CtValue::Scalar(Constant::I32(*delimiter as i32))),
                    (CtTokenNode::Token(_), P::Delimiter) => Err(error("E-CT-TOKEN-KIND", "token has no delimiter")),
                    (CtTokenNode::Token(token), P::Text) => {
                        budget.reserve_temporary_bytes(token.text.len() as u64)?;
                        Ok(CtValue::Text(token.text.clone()))
                    }
                    (CtTokenNode::Group { .. }, P::Text) => Err(error("E-CT-TOKEN-KIND", "group has no token text")),
                    (CtTokenNode::Group { children, .. }, P::ChildCount) => Ok(CtValue::Scalar(Constant::I64(i64::try_from(children.len()).map_err(|_| error("E-CT-TOKEN-LIMIT", "token child count exceeds target integer"))?))),
                    (CtTokenNode::Token(_), P::ChildCount) => Ok(CtValue::Scalar(Constant::I64(0))),
                    (CtTokenNode::Group { children, .. }, P::ChildAt) => {
                        let node = *children.get(indexed()?).ok_or_else(|| error("E-CT-TOKEN-INDEX", "token child index is out of range"))?;
                        cursor(node, budget)
                    }
                    (CtTokenNode::Token(_), P::ChildAt) => Err(error("E-CT-TOKEN-KIND", "token has no child")),
                    (CtTokenNode::Token(token), P::InputName) => {
                        let ident = token.name.as_ref().ok_or_else(|| error("E-CT-TOKEN-NAME", "token is not an input identifier"))?;
                        budget.reserve_generated_nodes(1)?;
                        budget.reserve_temporary_bytes(ident.text.len() as u64)?;
                        Ok(CtValue::Ident(ident.clone()))
                    }
                    (CtTokenNode::Group { .. }, P::InputName) => Err(error("E-CT-TOKEN-NAME", "group is not an input identifier")),
                    _ => Err(error("E-CT-TOKEN-KIND", "token projection does not match this node")),
                }
            }
            _ => Err(error("E-CT-INPUT", "token cursor is not bound to the authenticated provider input")),
        }
    }
}
