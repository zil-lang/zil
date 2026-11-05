//! Category-directed, transactional project materialization. The caller owns
//! the cloned AST until every generated list has passed its preflight.
use super::{
    compiler::{self, Entry},
    execute::Environment,
    input,
    lock::provider_entry,
    render::ParsedSyntax,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use zil_base::{
    CompileResult, Diagnostic, Fingerprint, Limits, ModuleId, Phase, SourceUnit, Span,
    TargetProfile,
};
use zil_ir::{CoreImport, ct_artifact::*};
use zil_syntax::{GeneratedSyntax, ParseResult, ast as s};
#[path = "associated_constants.rs"]
mod associated_constants;
#[path = "materialize_generated_lineage.rs"]
mod generated_lineage;
#[path = "materialize_member_lineage.rs"]
mod member_lineage;
#[path = "self_constant_coverage.rs"]
mod self_constant_coverage;
#[path = "self_constants.rs"]
mod self_constants;
#[path = "materialize_source_sites.rs"]
mod source_sites;

fn category(message: impl Into<String>, span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-CATEGORY",
        Some(span),
        message,
    )]
}
fn runtime(message: impl Into<String>, span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-RUNTIME",
        Some(span),
        message,
    )]
}
fn late_import(span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Expand,
        "E-CT-LATE-IMPORT",
        Some(span),
        "generated import would change the sealed module/provider graph",
    )]
}

pub(crate) struct Materialized {
    pub modules: Vec<s::Module>,
    pub previews: Vec<Preview>,
    pub required_imports: Vec<(ModuleId, CoreImport)>,
    pub usage: CtUsageReceipt,
    /// Rows move with actual original items. No generated/member lineage is
    /// inferred from the output's names, spans or new vector positions.
    pub declaration_origins: Option<Vec<zil_expand::source_context::ModuleDeclarationOrigins>>,
    pub lineage_remaining: Option<Limits>,
}
impl Materialized {
    pub(crate) fn retain_items(
        &mut self,
        index: usize,
        mut keep: impl FnMut(&s::Item) -> bool,
    ) -> CompileResult<()> {
        let module = &mut self.modules[index];
        let Some(origins) = &mut self.declaration_origins else {
            module.items.retain(keep);
            return Ok(());
        };
        let rows = origins
            .get_mut(index)
            .ok_or_else(|| category("declaration-origin module absent", module.span))?;
        if rows.module != module.id || rows.items.len() != module.items.len() {
            return Err(category(
                "declaration-origin placement differs before import removal",
                module.span,
            ));
        }
        let count = rows.items.len();
        let remaining = self
            .lineage_remaining
            .as_mut()
            .ok_or_else(|| category("declaration lineage ledger absent", module.span))?;
        // The original rows are moved, not cloned; reserve the complete visit
        // and destination vector before filtering either aligned collection.
        remaining.max_analysis_steps =
            remaining
                .max_analysis_steps
                .checked_sub(count)
                .ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Ct,
                        "declaration lineage move work exhausted",
                        Some(module.span),
                    )]
                })?;
        remaining.max_nodes = remaining.max_nodes.checked_sub(count).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Ct,
                "declaration lineage move nodes exhausted",
                Some(module.span),
            )]
        })?;
        let header_bytes = count
            .checked_mul(std::mem::size_of::<
                zil_expand::source_context::DeclarationOrigin,
            >())
            .ok_or_else(|| category("declaration lineage move size overflow", module.span))?;
        remaining.max_source_bytes = remaining
            .max_source_bytes
            .checked_sub(header_bytes)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Ct,
                    "declaration lineage destination bytes exhausted",
                    Some(module.span),
                )]
            })?;
        self.usage.analysis_work = self
            .usage
            .analysis_work
            .checked_add(count as u64)
            .ok_or_else(|| category("declaration lineage work count overflow", module.span))?;
        self.usage.generated_nodes = self
            .usage
            .generated_nodes
            .checked_add(count as u64)
            .ok_or_else(|| category("declaration lineage node count overflow", module.span))?;
        let mut input = std::mem::take(&mut rows.items).into_iter();
        let mut moved = Vec::with_capacity(count);
        module.items.retain(|item| {
            let row = input.next().expect("checked aligned declaration count");
            if keep(item) {
                moved.push(row);
                true
            } else {
                false
            }
        });
        rows.items = moved;
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Preview {
    invocation: Span,
    category: CtSyntaxCategory,
    text: String,
    tokens: Vec<super::render::RenderedToken>,
    origins: Vec<CtOrigin>,
}

impl Preview {
    pub(crate) fn json(&self) -> String {
        use zil_base::json_string;
        let span = |span: Span| {
            format!(
                "{{\"file\":{},\"start\":{},\"end\":{}}}",
                span.file.0, span.start, span.end
            )
        };
        let tokens = self
            .tokens
            .iter()
            .map(|token| {
                format!(
                    "{{\"span\":{},\"origin\":{},\"name\":{}}}",
                    span(token.span),
                    token.origin.0,
                    token.name.as_ref().map_or_else(
                        || "null".into(),
                        |name| format!(
                            "{{\"text\":{},\"namespace\":{},\"source\":{}}}",
                            json_string(&name.text),
                            json_string(&format!("{:?}", name.namespace)),
                            json_string(&format!("{:?}", name.source))
                        )
                    )
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let origins = self
            .origins
            .iter()
            .map(|origin| {
                format!(
                    "{{\"span\":{},\"parent\":{},\"role\":{}}}",
                    span(origin.span),
                    origin
                        .parent
                        .map_or_else(|| "null".into(), |parent| parent.0.to_string()),
                    json_string(&format!("{:?}", origin.role))
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"invocation\":{},\"category\":{},\"text\":{},\"tokens\":[{}],\"origins\":[{}]}}",
            span(self.invocation),
            json_string(&format!("{:?}", self.category)),
            json_string(&self.text),
            tokens,
            origins
        )
    }
}

struct Materializer<'a, 'b> {
    environment: &'a mut Environment<'b>,
    sources: &'b [SourceUnit],
    parsed: &'b [Arc<ParseResult>],
    registry: Option<&'a CtLockedProviderRegistry>,
    paths: &'b BTreeMap<Vec<String>, ModuleId>,
    previews: Vec<Preview>,
    required_imports: Vec<(ModuleId, CoreImport)>,
    runtime_scopes: Vec<BTreeSet<String>>,
    data_source_scopes: Vec<super::execute::value_context::ValueSourceScope>,
    max_depth: usize,
    track_declarations: bool,
    declaration_lineage_preserved: bool,
    member_retentions: Vec<member_lineage::MemberRetention>,
    generated_trees: Vec<Arc<zil_expand::source_context::GeneratedSourceTree>>,
    current_generated_tree: Option<Arc<zil_expand::source_context::GeneratedSourceTree>>,
    pending_generated_tree: Option<Arc<zil_expand::source_context::GeneratedSourceTree>>,
    current_emitter: Option<zil_expand::source_context::SourceDeclarationSite>,
    item_path: Vec<u32>,
    next_generated_declaration: BTreeMap<ModuleId, u32>,
    source_namespaces: BTreeMap<ModuleId, zil_ir::LifeDeclarationIdentity>,
    canonical_source_sites: BTreeMap<(bool, usize), compiler::source_sites::SourceSite>,
    indexed_source_trees: BTreeSet<(ModuleId, Option<u32>)>,
    bound_source_sites: source_sites::BoundSites,
    generated_context: bool,
    // The revision is sampled at the actual original top-level item visit.
    // Changed rows move with retained original ordinals, never with spans.
    closure_site_revision: u64,
    changed_closure_sites: BTreeSet<(ModuleId, usize)>,
    self_constant_sources: Vec<(ModuleId, usize, usize)>,
    self_constant_impl_sources: Vec<(ModuleId, usize)>,
    self_constant_impl_outputs: Vec<(ModuleId, usize)>,
}

impl super::hygiene::Context for Materializer<'_, '_> {
    fn owner_type(&mut self, ident: &CtIdent, span: Span) -> CompileResult<s::TypeRef> {
        self.member_owner_type(ident, span)
    }
    fn prepay_record_shorthand(&mut self, work: u64, bytes: u64) -> CompileResult<()> {
        self.environment.budget.reserve_analysis_work(work)?;
        self.environment.budget.reserve_generated_nodes(1)?;
        self.environment.budget.reserve_temporary_bytes(bytes)
    }
}

fn take_statement(statement: &mut s::Stmt) -> s::Stmt {
    let span = statement.span;
    std::mem::replace(
        statement,
        s::Stmt {
            span,
            kind: s::StmtKind::Expr(s::Expr {
                span,
                kind: s::ExprKind::Literal(s::Literal::Bool(false)),
            }),
        },
    )
}

fn float_expression(
    value: &zil_ir::Constant,
    budget: &mut zil_ct::CtExecutionBudget,
    span: Span,
) -> CompileResult<s::Expr> {
    use zil_base::numeric::FloatValue;
    let value = match value {
        zil_ir::Constant::F32(bits) => FloatValue::F32(*bits),
        zil_ir::Constant::F64(bits) => FloatValue::F64(*bits),
        _ => unreachable!("floating reification caller"),
    };
    // Preserve every raw bit, including noncanonical/signaling NaNs. This
    // generated-only fixed spelling goes directly to Core Constant.
    budget.reserve_analysis_work(26)?;
    budget.reserve_temporary_bytes(26)?;
    budget.reserve_generated_nodes(1)?;
    Ok(s::Expr {
        span,
        kind: s::ExprKind::Literal(s::Literal::Float(value.generated_spelling())),
    })
}

impl Materializer<'_, '_> {
    fn depth(&self, depth: usize, span: Span) -> CompileResult<()> {
        if depth > self.max_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "generated syntax recursion budget exceeded",
                Some(span),
            )]);
        }
        Ok(())
    }
    fn parsed_source(&self, module: ModuleId) -> CompileResult<(&SourceUnit, &ParseResult)> {
        self.sources
            .iter()
            .zip(self.parsed)
            .find(|(source, _)| source.module_id == module)
            .map(|(source, parsed)| (source, parsed.as_ref()))
            .ok_or_else(|| category("CT source module is absent", Span::default()))
    }
    fn owner(
        &self,
        module: ModuleId,
        span: Span,
        inherited: Option<&CtSymbolId>,
    ) -> CompileResult<CtSymbolId> {
        self.environment
            .index
            .symbol_at(module, span)
            .or_else(|| inherited.cloned())
            .or_else(|| {
                // The old finite #if/#for selector may lift a parsed child
                // declaration into the selected module. It is still anchored
                // in its original, frozen meta item; do not mint a new symbol
                // for a declaration generated in the current batch.
                let original = self
                    .parsed
                    .iter()
                    .find(|parsed| parsed.module.id == module)?;
                original.module.items.iter().find_map(|item| {
                    (matches!(
                        item.kind,
                        s::ItemKind::Meta(s::MetaItem::If { .. } | s::MetaItem::For { .. })
                    ) && item.span.file == span.file
                        && item.span.start <= span.start
                        && span.end <= item.span.end)
                        .then(|| self.environment.index.symbol_at(module, item.span))
                        .flatten()
                })
            })
            .ok_or_else(|| category("CT expression has no frozen declaration owner", span))
    }
    fn reject_alias_cycle(&self, owner: &CtSymbolId, expression: &s::Expr) -> CompileResult<()> {
        let Some(index) = self
            .sources
            .iter()
            .position(|source| source.module_id == owner.module)
        else {
            return Ok(());
        };
        let Some(s::Item {
            kind: s::ItemKind::TypeAlias(alias),
            ..
        }) = self.parsed[index]
            .module
            .items
            .get(owner.declaration.0 as usize)
        else {
            return Ok(());
        };
        let source = &self.sources[index];
        if self.parsed[index].tokens.iter().any(|token| {
            token.span.start >= expression.span.start
                && token.span.end <= expression.span.end
                && token.kind == zil_syntax::token::TokenKind::Identifier
                && token.text(&source.text) == alias.name
        }) {
            return Err(vec![Diagnostic::error(
                Phase::Ct,
                "E-CT-PHASE-CYCLE",
                Some(expression.span),
                "a generated type alias reads its own unfinished type or layout",
            )]);
        }
        Ok(())
    }
    fn reject_runtime_capture(&self, expression: &s::Expr) -> CompileResult<()> {
        let mut pending = vec![expression];
        while let Some(current) = pending.pop() {
            match &current.kind {
                s::ExprKind::Name(name)
                    if self
                        .runtime_scopes
                        .iter()
                        .rev()
                        .any(|scope| scope.contains(name)) =>
                {
                    return Err(vec![Diagnostic::error(
                        Phase::Ct,
                        "E-CT-SOURCE",
                        Some(current.span),
                        format!(
                            "runtime local `{name}` cannot be read by a compile-time expression"
                        ),
                    )]);
                }
                s::ExprKind::Group(value)
                | s::ExprKind::Try(value)
                | s::ExprKind::MetaEval(value)
                | s::ExprKind::MetaSplice(value)
                | s::ExprKind::Unary { operand: value, .. } => pending.push(value),
                s::ExprKind::Tuple(values) | s::ExprKind::Array(values) => pending.extend(values),
                s::ExprKind::Binary { left, right, .. }
                | s::ExprKind::Pipeline {
                    input: left,
                    target: right,
                } => {
                    pending.push(left);
                    pending.push(right);
                }
                s::ExprKind::Member { object, .. } => pending.push(object),
                s::ExprKind::TypeApply { callee, arguments } => {
                    pending.push(callee);
                    for arg in arguments {
                        if let s::GenericArg::Const(value) = arg {
                            pending.push(value);
                        }
                    }
                }
                s::ExprKind::Call { callee, arguments } => {
                    pending.push(callee);
                    pending.extend(arguments.iter().map(|arg| &arg.value));
                }
                s::ExprKind::Index { object, index } => {
                    pending.push(object);
                    pending.push(index);
                }
                s::ExprKind::Slice { object, start, end } => {
                    pending.push(object);
                    pending.extend(start.iter().map(Box::as_ref));
                    pending.extend(end.iter().map(Box::as_ref));
                }
                s::ExprKind::Record { ty, base, fields } => {
                    pending.extend(ty.iter().map(Box::as_ref));
                    pending.extend(base.iter().map(Box::as_ref));
                    pending.extend(fields.iter().filter_map(|field| field.value.as_ref()));
                }
                s::ExprKind::If {
                    condition,
                    then_value,
                    else_value,
                } => {
                    pending.push(condition);
                    pending.push(then_value);
                    pending.push(else_value);
                }
                s::ExprKind::Quote { template, .. } => {
                    pending.extend(template.holes.iter().map(|hole| hole.value.as_ref()))
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn bind_runtime_pattern(&mut self, pattern: &s::Pattern) {
        let Some(scope) = self.runtime_scopes.last_mut() else {
            return;
        };
        let mut pending = vec![pattern];
        while let Some(pattern) = pending.pop() {
            match &pattern.kind {
                s::PatternKind::Name(name) => {
                    scope.insert(name.clone());
                }
                s::PatternKind::Reference { pattern, .. }
                | s::PatternKind::ErrorArm { pattern, .. } => pending.push(pattern),
                s::PatternKind::RecordConstructor { fields, .. } => {
                    pending.extend(fields.iter().map(|field| &field.pattern));
                }
                s::PatternKind::Tuple(parts) => pending.extend(parts),
                s::PatternKind::Constructor { arguments, .. } => pending.extend(arguments),
                _ => {}
            }
        }
    }
    fn owner_alias(
        &mut self,
        caller: ModuleId,
        target: ModuleId,
        span: Span,
    ) -> CompileResult<String> {
        if let Some(alias) = self.environment.index.aliases(caller).get(&target) {
            return Ok(alias.clone());
        }
        if let Some((_, import)) = self
            .required_imports
            .iter()
            .find(|(module, import)| *module == caller && import.module == target)
        {
            return Ok(import.alias.clone());
        }
        if !self.sources.iter().any(|source| source.module_id == target) {
            return Err(vec![Diagnostic::error(
                Phase::Ct,
                "E-CT-MEMBER-OWNER",
                Some(span),
                "reflected owner module is outside the sealed project",
            )]);
        }
        let stem = format!("__zil_ct_owner{}_{}_{}", caller.0, target.0, span.start);
        let mut suffix = 0u32;
        let alias = loop {
            self.environment.budget.reserve_analysis_work(1)?;
            let candidate = if suffix == 0 {
                stem.clone()
            } else {
                format!("{stem}_{suffix}")
            };
            if !self.environment.index.alias_taken(caller, &candidate)
                && !self
                    .required_imports
                    .iter()
                    .any(|(module, import)| *module == caller && import.alias == candidate)
            {
                break candidate;
            }
            suffix = suffix.checked_add(1).ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Ct,
                    "reflected owner alias count exceeds identity range",
                    Some(span),
                )]
            })?;
        };
        self.environment.budget.reserve_generated_nodes(1)?;
        self.environment
            .budget
            .reserve_temporary_bytes((alias.len() as u64).saturating_mul(2))?;
        self.required_imports.push((
            caller,
            CoreImport {
                alias: alias.clone(),
                module: target,
                span,
            },
        ));
        Ok(alias)
    }
    fn owner_node(
        &mut self,
        ty: &CtTypeRef,
        id: CtTypeNodeId,
        caller: ModuleId,
        span: Span,
        depth: usize,
    ) -> CompileResult<s::TypeRef> {
        if depth > self.max_depth {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "reflected owner Type nesting exceeds budget",
                Some(span),
            )]);
        }
        self.environment.budget.reserve_analysis_work(1)?;
        self.environment.budget.reserve_generated_nodes(1)?;
        let node = ty
            .nodes
            .get(id.0 as usize)
            .ok_or_else(|| category("reflected owner Type DAG node is absent", span))?;
        let named = |name: &str, args: Vec<s::GenericArg>| s::TypeKind::Named {
            path: vec![name.into()],
            args,
        };
        let type_arg = |ty: s::TypeRef| s::GenericArg::Type(ty);
        let kind = match node {
            CtTypeNode::BorrowedStorage { storage, .. } => {
                self.environment
                    .budget
                    .reserve_analysis_work(self.data_source_scopes.len() as u64)?;
                if !self.data_source_scopes.iter().any(|s| s.owner == id) {
                    return Err(category(
                        "stored-borrow Type has no genuine scoped source handle",
                        span,
                    ));
                }
                return self.owner_node(ty, *storage, caller, span, depth + 1);
            }
            CtTypeNode::Primitive(CtScalarTy::Unit) => s::TypeKind::Tuple(Vec::new()),
            CtTypeNode::Primitive(primitive) => named(
                match primitive {
                    CtScalarTy::Unit => unreachable!("Unit uses the ordinary empty-tuple type"),
                    CtScalarTy::Bool => "bool",
                    CtScalarTy::U8 => "u8",
                    CtScalarTy::U16 => "u16",
                    CtScalarTy::I32 => "i32",
                    CtScalarTy::I64 => "i64",
                    CtScalarTy::F32 => "f32",
                    CtScalarTy::F64 => "f64",
                    CtScalarTy::U64 => "u64",
                    CtScalarTy::Usize => "usize",
                },
                Vec::new(),
            ),
            CtTypeNode::Builtin(builtin) => named(
                match builtin {
                    CtBuiltinType::ByteBuffer => "ByteBuffer",
                    CtBuiltinType::ForeignHandle => "ForeignHandle",
                    CtBuiltinType::CWideString => "CWideString",
                },
                Vec::new(),
            ),
            CtTypeNode::Nominal { symbol, arguments } => {
                self.prepay_source_lookup()?;
                let item = self.source_declaration(symbol, span)?;
                let name = match &item.kind {
                    s::ItemKind::Aggregate(value) => value.name.clone(),
                    s::ItemKind::Enum(value) => value.name.clone(),
                    s::ItemKind::TypeAlias(value) => value.name.clone(),
                    _ => return Err(category("reflected owner is not a nominal type", span)),
                };
                let path = if symbol.module == caller {
                    vec![name]
                } else {
                    vec![self.owner_alias(caller, symbol.module, span)?, name]
                };
                let mut args = Vec::new();
                for argument in arguments {
                    match argument {
                        CtTypeArgument::Type(id) => args.push(type_arg(self.owner_node(
                            ty,
                            *id,
                            caller,
                            span,
                            depth + 1,
                        )?)),
                        CtTypeArgument::Lifetime(value) => {
                            if let zil_ir::StorageBinder::Fixed(binder) = value.binder {
                                self.environment
                                    .budget
                                    .reserve_analysis_work(self.data_source_scopes.len() as u64)?;
                                let scope = self
                                    .data_source_scopes
                                    .iter()
                                    .find(|s| s.owner == value.owner)
                                    .ok_or_else(|| {
                                        runtime(
                                            "fixed lifetime has no authentic source handle",
                                            span,
                                        )
                                    })?;
                                let ordinal =
                                    *scope.parameters.get(binder as usize).ok_or_else(|| {
                                        runtime("fixed lifetime source ordinal is absent", span)
                                    })?;
                                let definition = scope.definition.clone();
                                let name = self.source_lifetime_name(&definition, ordinal, span)?;
                                args.push(s::GenericArg::Lifetime(name));
                                continue;
                            }
                            let zil_ir::StorageBinder::Invocation(binder) = value.binder else {
                                unreachable!()
                            };
                            let Some(CtTypeNode::BorrowedFunctionPointer { life, .. }) =
                                ty.nodes.get(value.owner.0 as usize)
                            else {
                                return Err(runtime(
                                    "lifetime metadata has no invocation owner",
                                    span,
                                ));
                            };
                            if binder >= life.invocation_binder_count {
                                return Err(runtime(
                                    "lifetime metadata ordinal exceeds its owner",
                                    span,
                                ));
                            }
                            self.environment.budget.reserve_analysis_work(32)?;
                            self.environment.budget.reserve_temporary_bytes(32)?;
                            args.push(s::GenericArg::Lifetime(format!(
                                "zil_life_{}_{}",
                                value.owner.0, binder
                            )));
                        }
                        CtTypeArgument::Scalar(value) => {
                            if matches!(value, zil_ir::Constant::F32(_) | zil_ir::Constant::F64(_))
                            {
                                args.push(s::GenericArg::Const(float_expression(
                                    value,
                                    &mut self.environment.budget,
                                    span,
                                )?));
                                continue;
                            }
                            let literal = match value {
                                zil_ir::Constant::Bool(value) => s::Literal::Bool(*value),
                                zil_ir::Constant::U8(value) => {
                                    s::Literal::Integer(value.to_string())
                                }
                                zil_ir::Constant::U16(value) => {
                                    s::Literal::Integer(value.to_string())
                                }
                                zil_ir::Constant::I32(value) => {
                                    s::Literal::Integer(value.to_string())
                                }
                                zil_ir::Constant::U64(value) => {
                                    s::Literal::Integer(format!("{value}u64"))
                                }
                                zil_ir::Constant::Usize(value) => {
                                    s::Literal::Integer(format!("{value}usize"))
                                }
                                zil_ir::Constant::I64(value) => {
                                    s::Literal::Integer(value.to_string())
                                }
                                _ => {
                                    return Err(category(
                                        "reflected owner const argument has no ordinary spelling",
                                        span,
                                    ));
                                }
                            };
                            args.push(s::GenericArg::Const(s::Expr {
                                span,
                                kind: s::ExprKind::Literal(literal),
                            }));
                        }
                    }
                }
                s::TypeKind::Named { path, args }
            }
            CtTypeNode::AnonymousSum(parts) => s::TypeKind::Union(
                parts
                    .iter()
                    .map(|id| self.owner_node(ty, *id, caller, span, depth + 1))
                    .collect::<CompileResult<Vec<_>>>()?,
            ),
            CtTypeNode::Tuple(parts) => s::TypeKind::Tuple(
                parts
                    .iter()
                    .map(|id| self.owner_node(ty, *id, caller, span, depth + 1))
                    .collect::<CompileResult<Vec<_>>>()?,
            ),
            CtTypeNode::Array { element, length } => s::TypeKind::Array {
                element: Box::new(self.owner_node(ty, *element, caller, span, depth + 1)?),
                length: Some(Box::new(s::Expr {
                    span,
                    kind: s::ExprKind::Literal(s::Literal::Integer(length.to_string())),
                })),
            },
            CtTypeNode::Slice(element) => s::TypeKind::Array {
                element: Box::new(self.owner_node(ty, *element, caller, span, depth + 1)?),
                length: None,
            },
            CtTypeNode::Address { mutable, inner } => s::TypeKind::Reference {
                lifetime: None,
                mutable: *mutable,
                inner: Box::new(self.owner_node(ty, *inner, caller, span, depth + 1)?),
            },
            CtTypeNode::Managed(inner) => named(
                "Managed",
                vec![type_arg(self.owner_node(
                    ty,
                    *inner,
                    caller,
                    span,
                    depth + 1,
                )?)],
            ),
            CtTypeNode::Desc(inner) => named(
                "Desc",
                vec![type_arg(self.owner_node(
                    ty,
                    *inner,
                    caller,
                    span,
                    depth + 1,
                )?)],
            ),
            CtTypeNode::OwnedStore(inner) => named(
                "Store",
                vec![type_arg(self.owner_node(
                    ty,
                    *inner,
                    caller,
                    span,
                    depth + 1,
                )?)],
            ),
            CtTypeNode::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                let life = super::scope_metadata::closed_signature(
                    life,
                    parameters.len(),
                    &mut self.environment.budget,
                )?;
                self.environment.budget.reserve_analysis_work(
                    (life.binder_count as u64).saturating_add(life.parameter_lives.len() as u64),
                )?;
                self.environment
                    .budget
                    .reserve_generated_nodes(life.binder_count as u64)?;
                self.environment
                    .budget
                    .reserve_temporary_bytes((life.binder_count as u64).saturating_mul(64))?;
                let label = |mut ty: s::TypeRef, tag: Option<u32>| -> CompileResult<s::TypeRef> {
                    if let Some(tag) = tag {
                        let s::TypeKind::Reference { lifetime, .. } = &mut ty.kind else {
                            return Err(category(
                                "borrowed pointer label names a non-address",
                                span,
                            ));
                        };
                        *lifetime = Some(format!("zil_life_{}_{tag}", id.0));
                    }
                    Ok(ty)
                };
                let params = parameters
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        label(
                            self.owner_node(ty, *id, caller, span, depth + 1)?,
                            life.parameter_lives.get(i).copied().flatten(),
                        )
                    })
                    .collect::<CompileResult<Vec<_>>>()?;
                let result = Box::new(label(
                    self.owner_node(ty, *result, caller, span, depth + 1)?,
                    life.result_life,
                )?);
                let mut kind = s::TypeKind::Function { params, result };
                for i in (0..life.binder_count).rev() {
                    kind = s::TypeKind::ForLifetime {
                        lifetime: format!("zil_life_{}_{i}", id.0),
                        inner: Box::new(s::TypeRef { kind, span }),
                    };
                }
                kind
            }
            CtTypeNode::FunctionPointer { parameters, result } => s::TypeKind::Function {
                params: parameters
                    .iter()
                    .map(|id| self.owner_node(ty, *id, caller, span, depth + 1))
                    .collect::<CompileResult<Vec<_>>>()?,
                result: Box::new(self.owner_node(ty, *result, caller, span, depth + 1)?),
            },
            CtTypeNode::ForeignCallback { parameters, result } => s::TypeKind::ForeignFunction {
                abi: "C".into(),
                params: parameters
                    .iter()
                    .map(|id| self.owner_node(ty, *id, caller, span, depth + 1))
                    .collect::<CompileResult<Vec<_>>>()?,
                result: Box::new(self.owner_node(ty, *result, caller, span, depth + 1)?),
            },
            CtTypeNode::ErrorUnion { error, result } => s::TypeKind::Error {
                error: Box::new(self.owner_node(ty, *error, caller, span, depth + 1)?),
                result: Box::new(self.owner_node(ty, *result, caller, span, depth + 1)?),
            },
            CtTypeNode::Opaque { .. } => {
                return Err(vec![Diagnostic::error(
                    Phase::Ct,
                    "E-CT-MEMBER-OWNER",
                    Some(span),
                    "opaque reflected owner cannot authorize member access",
                )]);
            }
        };
        Ok(s::TypeRef { kind, span })
    }
    fn member_owner_type(&mut self, ident: &CtIdent, span: Span) -> CompileResult<s::TypeRef> {
        let Some(ty) = &ident.member_owner_type else {
            return Err(category("reflected member lacks a full owner Type", span));
        };
        let caller = self
            .sources
            .iter()
            .find(|source| source.file_id == span.file)
            .map(|source| source.module_id)
            .ok_or_else(|| category("reflected member caller is absent", span))?;
        self.owner_node(ty, ty.root, caller, span, 0)
    }
    fn expression_value(
        &mut self,
        module: ModuleId,
        owner: &CtSymbolId,
        expression: &s::Expr,
        expected: Option<CtTy>,
    ) -> CompileResult<CtValue> {
        self.expression_data(module, owner, expression, expected)
            .map(|output| output.value)
    }
    fn expression_data(
        &mut self,
        module: ModuleId,
        owner: &CtSymbolId,
        expression: &s::Expr,
        expected: Option<CtTy>,
    ) -> CompileResult<super::execute::EvaluatedData> {
        if let s::ExprKind::SyntaxIsland { path, tree } = &expression.kind {
            return self
                .provider(
                    module,
                    path,
                    tree,
                    expression.span,
                    expected.as_ref().ok_or_else(|| {
                        category(
                            "provider output needs a Syntax receiving category",
                            expression.span,
                        )
                    })?,
                )
                .map(|value| super::execute::EvaluatedData {
                    value,
                    context: None,
                });
        }
        let input = self.environment.empty_input(expression.span)?;
        self.reject_runtime_capture(expression)?;
        let has_receiving_category = expected.is_some();
        let site = self.expression_source_site(expression)?;
        let entry = Entry::Expression {
            module,
            expression,
            definition: owner.clone(),
            lexical_owner: Some(owner.clone()),
            expected,
            parameters: &[],
        };
        let result = if let Some(site) = site {
            self.environment.evaluate_data_at_source(
                entry,
                &input,
                None,
                &[],
                expression.span,
                site,
            )
        } else {
            self.environment
                .evaluate_data(entry, &input, None, &[], expression.span)
        };
        result.map_err(|mut errors| {
            if has_receiving_category {
                for error in &mut errors {
                    if error.code == "E-CT-SOURCE"
                        && error.message
                            == "CT expression type does not match its actual expected type"
                    {
                        error.code = "E-CT-CATEGORY".into();
                        error.message =
                            "compile-time value category differs from its receiving syntax slot"
                                .into();
                    }
                }
            }
            errors
        })
    }
    fn uses_ct_domain(&mut self, module: ModuleId, expression: &s::Expr) -> CompileResult<bool> {
        let syntax = self
            .parsed
            .iter()
            .map(|parsed| &parsed.module)
            .collect::<Vec<_>>();
        let modules = self.environment.index.modules(self.sources, &syntax);
        let parameters = self
            .runtime_scopes
            .iter()
            .flat_map(|scope| scope.iter())
            .map(|name| (name.clone(), CtTy::Scalar(CtScalarTy::Unit)))
            .collect::<Vec<_>>();
        let mut limits = self.environment.remaining_limits_for_input();
        let before = limits.clone();
        let result =
            compiler::uses_ct_domain(&modules, module, expression, &parameters, &mut limits);
        self.environment.budget.reserve_analysis_work(
            before
                .max_analysis_steps
                .saturating_sub(limits.max_analysis_steps) as u64,
        )?;
        self.environment
            .budget
            .reserve_generated_nodes(before.max_nodes.saturating_sub(limits.max_nodes) as u64)?;
        self.environment.budget.reserve_temporary_bytes(
            before
                .max_source_bytes
                .saturating_sub(limits.max_source_bytes) as u64,
        )?;
        result
    }
    fn provider(
        &mut self,
        module: ModuleId,
        path: &[String],
        tree: &s::TokenTree,
        span: Span,
        expected: &CtTy,
    ) -> CompileResult<CtValue> {
        let ([alias] | [alias, _]) = path else {
            return Err(category("provider path needs alias or alias.entry", span));
        };
        let export = path.get(1).map_or("default", String::as_str);
        let (source, parsed) = self.parsed_source(module)?;
        let import_path = parsed
            .module
            .items
            .iter()
            .find_map(|item| match &item.kind {
                s::ItemKind::Import(import)
                    if import.syntax
                        && import
                            .alias
                            .as_deref()
                            .or_else(|| import.path.last().map(String::as_str))
                            == Some(alias) =>
                {
                    Some(import.path.clone())
                }
                _ => None,
            })
            .ok_or_else(|| category("provider alias has no syntax use binding", span))?;
        let registry = self
            .registry
            .ok_or_else(|| category("syntax provider request has no lock", span))?;
        let delimiter = match tree.delimiter {
            s::Delimiter::Parenthesis => CtDelimiter::Parenthesis,
            s::Delimiter::Bracket => CtDelimiter::Bracket,
            s::Delimiter::Brace => CtDelimiter::Brace,
        };
        let entry = provider_entry(registry, &import_path.join("."), export, delimiter, span)?;
        if *expected != CtTy::Syntax(entry.output) {
            return Err(category(
                "provider output category differs from its receiving slot",
                span,
            ));
        }
        let (input_tree, tokens) = input::from_source(
            source,
            &parsed.tokens,
            tree,
            path,
            &self.environment.remaining_limits_for_input(),
        )?;
        self.environment.evaluate(
            Entry::Function(entry.symbol.clone()),
            &tokens,
            Some(&input_tree),
            &[CtValue::TokenTree(input_tree.clone())],
            span,
        )
    }
    fn parsed(
        &mut self,
        value: CtValue,
        expected: CtSyntaxCategory,
        span: Span,
    ) -> CompileResult<GeneratedSyntax> {
        let CtValue::Syntax(mut tree) = value else {
            return Err(category("compile-time result is not Syntax", span));
        };
        if tree.category != expected {
            return Err(category(
                "Syntax category differs from the receiving slot",
                span,
            ));
        }
        let retained_bytes = 16u64
            .saturating_add((tree.nodes.len() as u64).saturating_mul(80))
            .saturating_add((tree.origins.len() as u64).saturating_mul(32))
            .saturating_add(tree.nodes.iter().fold(0u64, |total, node| match node {
                CtSyntaxNode::Token(token) => total.saturating_add(token.text.len() as u64),
                _ => total,
            }));
        let mut renamed_bytes = 0u64;
        let caller = self
            .sources
            .iter()
            .find(|source| source.file_id == span.file)
            .map(|source| source.module_id)
            .ok_or_else(|| category("generated caller module is absent", span))?;
        let sequence_nodes = tree.roots.len().saturating_add(
            tree.nodes
                .iter()
                .map(|node| match node {
                    CtSyntaxNode::Group { children, .. } => children.len(),
                    _ => 0,
                })
                .sum::<usize>(),
        );
        let sequence_bytes = (sequence_nodes as u64).saturating_mul(16);
        self.environment
            .budget
            .reserve_analysis_work(tree.nodes.len() as u64)?;
        self.environment
            .budget
            .reserve_temporary_bytes(sequence_bytes)?;
        let mut sequences = vec![tree.roots.clone()];
        for node in &tree.nodes {
            if let CtSyntaxNode::Group { children, .. } = node {
                sequences.push(children.clone());
            }
        }
        let mut alias_sites = Vec::new();
        for sequence in &sequences {
            for pair in sequence.windows(2) {
                if matches!(tree.nodes.get(pair[1].0 as usize), Some(CtSyntaxNode::Token(token)) if token.text == ".")
                {
                    alias_sites.push(pair[0]);
                }
            }
        }
        for site in alias_sites {
            let Some(CtSyntaxNode::Token(token)) = tree.nodes.get(site.0 as usize) else {
                continue;
            };
            if token.name.is_some() {
                continue;
            }
            let Some(origin) = tree.origins.get(token.origin.0 as usize) else {
                return Err(category("generated origin is absent", span));
            };
            if origin.role != CtOriginRole::QuoteDefinition {
                continue;
            }
            let Some(provider_index) = self
                .sources
                .iter()
                .position(|source| source.file_id == origin.span.file)
            else {
                continue;
            };
            let source_import = self.parsed[provider_index]
                .module
                .items
                .iter()
                .find_map(|item| match &item.kind {
                    s::ItemKind::Import(import)
                        if !import.syntax
                            && import
                                .alias
                                .as_deref()
                                .or_else(|| import.path.last().map(String::as_str))
                                == Some(token.text.as_str()) =>
                    {
                        Some((item.span, import))
                    }
                    _ => None,
                });
            let Some((import_span, source_import)) = source_import else {
                continue;
            };
            let Some(target) = self.paths.get(&source_import.path) else {
                continue;
            };
            let alias = format!("__zil_ct_mod{}_{}_{}", provider_index, target.0, span.start);
            let symbol = self
                .environment
                .index
                .symbol_at(self.sources[provider_index].module_id, import_span)
                .ok_or_else(|| {
                    category("definition-site module import has no frozen symbol", span)
                })?;
            self.environment
                .budget
                .reserve_temporary_bytes(alias.len() as u64)?;
            renamed_bytes = renamed_bytes.saturating_add(alias.len() as u64);
            if let Some(CtSyntaxNode::Token(token)) = tree.nodes.get_mut(site.0 as usize) {
                token.text = alias.clone();
                token.name = Some(CtIdent {
                    text: alias.clone(),
                    namespace: CtNameNamespace::Module,
                    source: CtNameSource::Definition(symbol),
                    member_owner: None,
                    member_owner_type: None,
                });
            }
            if !self.required_imports.iter().any(|(module, import)| {
                *module == caller && import.alias == alias && import.module == *target
            }) {
                self.environment.budget.reserve_generated_nodes(1)?;
                self.required_imports.push((
                    caller,
                    CoreImport {
                        alias,
                        module: *target,
                        span,
                    },
                ));
            }
        }
        drop(sequences);
        self.environment
            .budget
            .release_temporary_bytes(sequence_bytes);
        for node in &tree.nodes {
            let CtSyntaxNode::Token(token) = node else {
                continue;
            };
            let Some(CtIdent {
                source: CtNameSource::Definition(symbol),
                namespace: CtNameNamespace::Type | CtNameNamespace::Value,
                ..
            }) = &token.name
            else {
                continue;
            };
            if symbol.module == caller
                || self
                    .environment
                    .index
                    .aliases(caller)
                    .contains_key(&symbol.module)
                || self
                    .required_imports
                    .iter()
                    .any(|(module, import)| *module == caller && import.module == symbol.module)
            {
                continue;
            }
            if !self
                .sources
                .iter()
                .any(|source| source.module_id == symbol.module)
            {
                return Err(category(
                    "definition-site name refers outside the sealed project",
                    span,
                ));
            }
            let alias = format!(
                "__zil_ct_def{}_{}_{}",
                caller.0, symbol.module.0, span.start
            );
            self.environment.budget.reserve_generated_nodes(1)?;
            self.environment
                .budget
                .reserve_temporary_bytes(alias.len() as u64)?;
            self.required_imports.push((
                caller,
                CoreImport {
                    alias,
                    module: symbol.module,
                    span,
                },
            ));
        }
        let extra_aliases = self
            .required_imports
            .iter()
            .filter(|(module, _)| *module == caller)
            .map(|(_, import)| (import.module, import.alias.clone()))
            .collect::<Vec<_>>();
        let ParsedSyntax { rendered, parsed } =
            self.environment.parse_tree(&tree, span, &extra_aliases)?;
        let mut syntax = parsed
            .syntax
            .ok_or_else(|| category("generated parser returned no category", span))?;
        let longest = rendered
            .tokens
            .iter()
            .map(|token| token.span.end.saturating_sub(token.span.start))
            .max()
            .unwrap_or(0);
        let hygiene_bytes =
            (rendered.tokens.len() as u64).saturating_mul((longest as u64).saturating_add(64));
        self.environment
            .budget
            .reserve_analysis_work((rendered.tokens.len() as u64).saturating_mul(8))?;
        self.environment
            .budget
            .reserve_temporary_bytes(hygiene_bytes)?;
        super::hygiene::rename(&mut syntax, &rendered, &tree.origins, span, self)?;
        self.freeze_generated_items(&syntax, caller, span)?;
        let limits = self.environment.remaining_limits_for_input();
        let work = zil_expand::remap_generated_spans(&mut syntax, span, &limits)?;
        self.environment.budget.reserve_analysis_work(work as u64)?;
        self.environment
            .budget
            .release_temporary_bytes(retained_bytes.saturating_add(renamed_bytes));
        self.previews.push(Preview {
            invocation: span,
            category: expected,
            text: rendered.source.text,
            tokens: rendered.tokens,
            origins: tree.origins,
        });
        Ok(syntax)
    }
    fn closure_sites_changed(&mut self, span: Span) -> CompileResult<()> {
        if self.track_declarations {
            self.environment.budget.reserve_analysis_work(1)?;
            self.closure_site_revision = self
                .closure_site_revision
                .checked_add(1)
                .ok_or_else(|| category("closure-site lineage revision overflow", span))?;
        }
        Ok(())
    }
    fn record_changed_closure_sites(
        &mut self,
        module: ModuleId,
        ordinal: usize,
    ) -> CompileResult<()> {
        self.environment.budget.reserve_analysis_work(
            self.changed_closure_sites
                .len()
                .saturating_mul(2)
                .saturating_add(1) as u64,
        )?;
        self.environment.budget.reserve_temporary_bytes(
            (std::mem::size_of::<(ModuleId, usize)>() as u64).saturating_add(64),
        )?;
        self.changed_closure_sites.insert((module, ordinal));
        Ok(())
    }
    fn items(
        &mut self,
        module: ModuleId,
        items: &mut Vec<s::Item>,
        inherited: Option<&CtSymbolId>,
        depth: usize,
        generated: bool,
    ) -> CompileResult<Option<Vec<generated_lineage::OutputOrigin>>> {
        self.depth(
            depth,
            items.first().map_or(Span::default(), |item| item.span),
        )?;
        let mut output = Vec::new();
        let mut retained = self.track_declarations.then(Vec::new);
        // CT scalar constants need authentic coordinates even when this source
        // has no life metadata and therefore no declaration-lineage inventory.
        let constant_coordinates = if self.self_constant_sources.is_empty() {
            false
        } else {
            self.environment
                .budget
                .reserve_analysis_work(self.self_constant_sources.len() as u64 + 1)?;
            self.self_constant_sources
                .iter()
                .any(|(owner, _, _)| *owner == module)
        };
        let original_coordinates = self.track_declarations || constant_coordinates;
        for (ordinal, mut item) in std::mem::take(items).into_iter().enumerate() {
            let closure_site_revision = self.closure_site_revision;
            if generated && matches!(item.kind, s::ItemKind::Import(_)) {
                return Err(late_import(item.span));
            }
            let path_len = self.item_path.len();
            self.item_path.push(
                u32::try_from(ordinal)
                    .map_err(|_| category("item structural ordinal overflow", item.span))?,
            );
            let owner = if let Some(tree) = self.current_generated_tree.clone() {
                let local = if generated {
                    zil_base::NodeId(
                        u32::try_from(ordinal)
                            .map_err(|_| category("generated root ordinal overflow", item.span))?,
                    )
                } else {
                    let parent = inherited
                        .ok_or_else(|| category("generated member parent absent", item.span))?;
                    let local_parent = tree
                        .declarations
                        .iter()
                        .position(|id| *id == parent.declaration)
                        .ok_or_else(|| {
                            category("generated member parent declaration absent", item.span)
                        })?;
                    compiler::source_scope::original_member(
                        &tree.module,
                        zil_base::NodeId(local_parent as u32),
                        ordinal,
                        self.max_depth,
                        |work| self.environment.budget.reserve_analysis_work(work as u64),
                    )?
                    .ok_or_else(|| {
                        category("generated member structural declaration absent", item.span)
                    })?
                };
                self.generated_symbol(&tree, local, item.span)?
            } else if original_coordinates && inherited.is_none() && !generated {
                self.environment
                    .budget
                    .reserve_analysis_work(self.environment.index.symbol_lookup_work())?;
                let symbol =
                    CtSymbolId {
                        module,
                        declaration: zil_base::NodeId(u32::try_from(ordinal).map_err(|_| {
                            category("original declaration index overflow", item.span)
                        })?),
                        generation_path: None,
                    };
                if !self.environment.index.contains(&symbol) {
                    return Err(category(
                        "original declaration absent from frozen inventory",
                        item.span,
                    ));
                }
                symbol
            } else if original_coordinates
                && !generated
                && !self.generated_context
                && inherited.is_some()
            {
                self.environment
                    .budget
                    .reserve_analysis_work(self.parsed.len() as u64)?;
                let parsed = self
                    .parsed
                    .iter()
                    .find(|p| p.module.id == module)
                    .ok_or_else(|| category("original member module is absent", item.span))?;
                let parent = inherited.expect("checked inherited declaration");
                let max_depth = self.environment.budget.remaining().max_call_depth as usize;
                if let Some(declaration) = compiler::source_scope::original_member(
                    &parsed.module,
                    parent.declaration,
                    ordinal,
                    max_depth,
                    |work| self.environment.budget.reserve_analysis_work(work as u64),
                )? {
                    CtSymbolId {
                        module,
                        declaration,
                        generation_path: None,
                    }
                } else {
                    self.owner(module, item.span, inherited)?
                }
            } else {
                self.owner(module, item.span, inherited)?
            };
            if self.environment.ct_only.contains(&owner)
                && matches!(
                    item.kind,
                    s::ItemKind::Function(_) | s::ItemKind::Extern { .. }
                )
            {
                self.item_path.truncate(path_len);
                continue;
            }
            let previous_bound_sites = std::mem::take(&mut self.bound_source_sites);
            self.bind_item_source_sites(&owner, &item)?;
            let previous_generated = self.generated_context;
            self.generated_context |= generated;
            match &mut item.kind {
                s::ItemKind::Meta(s::MetaItem::Emit(expression)) => {
                    let previous_emitter = self.current_emitter.replace(
                        zil_expand::source_context::SourceDeclarationSite {
                            tree: self.current_generated_tree.as_ref().map_or(
                                zil_expand::source_context::SourceTreeIdentity::Original(module),
                                |tree| tree.identity.clone(),
                            ),
                            declaration: owner.declaration,
                            event_path: self.item_path.clone(),
                        },
                    );
                    let value = self.expression_value(
                        module,
                        &owner,
                        expression,
                        Some(CtTy::Syntax(CtSyntaxCategory::ItemList)),
                    )?;
                    let GeneratedSyntax::ItemList(mut generated_items) =
                        self.parsed(value, CtSyntaxCategory::ItemList, item.span)?
                    else {
                        return Err(category(
                            "#emit must yield ItemList in item position",
                            item.span,
                        ));
                    };
                    let tree = self.pending_generated_tree.take().ok_or_else(|| {
                        category("generated ItemList frozen tree absent", item.span)
                    })?;
                    let previous_tree = self.current_generated_tree.replace(tree);
                    let previous_path = std::mem::take(&mut self.item_path);
                    let generated_rows =
                        self.items(module, &mut generated_items, Some(&owner), depth + 1, true)?;
                    self.item_path = previous_path;
                    self.current_generated_tree = previous_tree;
                    self.current_emitter = previous_emitter;
                    if let Some(retained) = &mut retained {
                        retained.extend(generated_rows.ok_or_else(|| {
                            category("generated ItemList origin rows absent", item.span)
                        })?);
                    }
                    output.extend(generated_items);
                }
                s::ItemKind::Meta(s::MetaItem::If {
                    then_items,
                    else_items,
                    ..
                }) => {
                    self.declaration_lineage_preserved = false;
                    self.nested_items(module, then_items, Some(&owner), depth + 1, generated)?;
                    self.nested_items(module, else_items, Some(&owner), depth + 1, generated)?;
                    output.push(item);
                }
                s::ItemKind::Meta(s::MetaItem::For { items, .. }) => {
                    self.declaration_lineage_preserved = false;
                    self.nested_items(module, items, Some(&owner), depth + 1, generated)?;
                    output.push(item);
                }
                _ => {
                    self.item(module, &mut item, &owner, depth + 1)?;
                    if inherited.is_none()
                        && !generated
                        && self.closure_site_revision != closure_site_revision
                    {
                        self.record_changed_closure_sites(module, ordinal)?;
                    }
                    if let Some(retained) = &mut retained {
                        self.environment.budget.reserve_analysis_work(1)?;
                        self.environment
                            .budget
                            .reserve_temporary_bytes(std::mem::size_of::<usize>() as u64)?;
                        if generated {
                            let tree = self.current_generated_tree.clone().ok_or_else(|| {
                                category("generated output source tree absent", item.span)
                            })?;
                            retained.push(generated_lineage::OutputOrigin::Generated(
                                self.generated_origin(&tree, ordinal)?,
                            ));
                        } else {
                            retained.push(generated_lineage::OutputOrigin::Original(ordinal));
                        }
                    }
                    if inherited.is_none()
                        && !generated
                        && !self.generated_context
                        && matches!(item.kind, s::ItemKind::Impl(_))
                    {
                        self.environment.budget.reserve_analysis_work(
                            self.self_constant_impl_sources.len() as u64 + 1,
                        )?;
                        if self.self_constant_impl_sources.contains(&(module, ordinal)) {
                            self.environment.budget.reserve_analysis_work(
                                self.self_constant_impl_outputs.len() as u64 + 1,
                            )?;
                            self.environment.budget.reserve_temporary_bytes(128)?;
                            self.self_constant_impl_outputs.push((module, output.len()));
                        }
                    }
                    output.push(item);
                }
            }
            self.release_bound_source_sites();
            self.bound_source_sites = previous_bound_sites;
            self.generated_context = previous_generated;
            self.item_path.truncate(path_len);
        }
        *items = output;
        Ok(retained)
    }
    fn nested_items(
        &mut self,
        module: ModuleId,
        items: &mut Vec<s::Item>,
        inherited: Option<&CtSymbolId>,
        depth: usize,
        generated: bool,
    ) -> CompileResult<()> {
        let original_count = items.len();
        let retained = self.items(module, items, inherited, depth, generated)?;
        if let Some(retained) = retained {
            self.environment
                .budget
                .reserve_analysis_work(original_count as u64)?;
            let unchanged = retained.len() == original_count && retained.iter().enumerate().all(|(i, row)|
                matches!(row, generated_lineage::OutputOrigin::Original(n) if *n == i));
            if !unchanged {
                let retained = retained
                    .iter()
                    .filter_map(|row| match row {
                        generated_lineage::OutputOrigin::Original(n) => Some(*n),
                        generated_lineage::OutputOrigin::Generated(_) => None,
                    })
                    .collect::<Vec<_>>();
                // Preserve supported original deletion events; unknown or
                // generated member changes keep the old unavailable state.
                let recorded = if self.declaration_lineage_preserved
                    && !generated
                    && let Some(host) = inherited
                {
                    self.record_member_retention(module, host, original_count, &retained)?
                } else {
                    false
                };
                if !recorded {
                    self.declaration_lineage_preserved = false;
                }
            }
        }
        Ok(())
    }
    fn item(
        &mut self,
        module: ModuleId,
        item: &mut s::Item,
        owner: &CtSymbolId,
        depth: usize,
    ) -> CompileResult<()> {
        self.depth(depth, item.span)?;
        match &mut item.kind {
            s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } => {
                for param in &mut f.params {
                    self.ty(module, &mut param.ty, owner, depth + 1)?;
                }
                if let Some(ty) = &mut f.return_type {
                    self.ty(module, ty, owner, depth + 1)?;
                }
                self.runtime_scopes
                    .push(f.params.iter().map(|param| param.name.clone()).collect());
                let result = match &mut f.body {
                    s::FunctionBody::Block(block) => self.block(module, block, owner, depth + 1),
                    s::FunctionBody::Expression(e) => self.expr(module, e, owner, depth + 1),
                    s::FunctionBody::Declaration => Ok(()),
                };
                self.runtime_scopes.pop();
                result?;
            }
            s::ItemKind::Aggregate(a) => {
                for field in &mut a.fields {
                    self.ty(module, &mut field.ty, owner, depth + 1)?;
                    if let Some(e) = &mut field.default {
                        self.expr(module, e, owner, depth + 1)?;
                    }
                }
                self.nested_items(module, &mut a.methods, Some(owner), depth + 1, false)?;
            }
            s::ItemKind::Enum(e) => {
                for variant in &mut e.variants {
                    match &mut variant.fields {
                        s::VariantFields::Unit => {}
                        s::VariantFields::Tuple(fields) => {
                            for field in fields {
                                self.ty(module, &mut field.ty, owner, depth + 1)?;
                            }
                        }
                        s::VariantFields::Record(fields) => {
                            for field in fields {
                                self.ty(module, &mut field.ty, owner, depth + 1)?;
                                if let Some(e) = &mut field.default {
                                    self.expr(module, e, owner, depth + 1)?;
                                }
                            }
                        }
                    }
                }
            }
            s::ItemKind::TypeAlias(alias) => {
                if let Some(ty) = &mut alias.value {
                    self.ty(module, ty, owner, depth + 1)?;
                }
            }
            s::ItemKind::Const(c) => {
                if let Some(e) = &mut c.value {
                    self.expr(module, e, owner, depth + 1)?;
                }
            }
            s::ItemKind::Trait(t) => {
                self.associated_constants(module, &mut t.members, &t.generics, owner, false)?;
                self.nested_items(module, &mut t.members, Some(owner), depth + 1, false)?
            }
            s::ItemKind::Impl(i) => {
                self.associated_constants(module, &mut i.members, &i.generics, owner, false)?;
                self.nested_items(module, &mut i.members, Some(owner), depth + 1, false)?
            }
            s::ItemKind::Import(_) | s::ItemKind::Meta(_) => {}
        }
        Ok(())
    }
    fn block(
        &mut self,
        module: ModuleId,
        block: &mut s::Block,
        owner: &CtSymbolId,
        depth: usize,
    ) -> CompileResult<()> {
        self.runtime_scopes.push(BTreeSet::new());
        let result = self.statements(module, &mut block.statements, owner, depth + 1);
        self.runtime_scopes.pop();
        result
    }
    fn statements(
        &mut self,
        module: ModuleId,
        statements: &mut Vec<s::Stmt>,
        owner: &CtSymbolId,
        depth: usize,
    ) -> CompileResult<()> {
        self.depth(
            depth,
            statements.first().map_or(Span::default(), |stmt| stmt.span),
        )?;
        let mut output = Vec::new();
        let mut input_statements = std::mem::take(statements);
        for stmt in &mut input_statements {
            // Even MetaIf/For left for Engine can later select/unroll sites.
            if matches!(stmt.kind, s::StmtKind::Meta(_)) {
                self.closure_sites_changed(stmt.span)?;
            }
            match &mut stmt.kind {
                s::StmtKind::Meta(s::MetaStatement::Emit(expression)) => {
                    let value = self.expression_value(
                        module,
                        owner,
                        expression,
                        Some(CtTy::Syntax(CtSyntaxCategory::StmtList)),
                    )?;
                    let GeneratedSyntax::StmtList(mut generated) =
                        self.parsed(value, CtSyntaxCategory::StmtList, stmt.span)?
                    else {
                        return Err(category(
                            "#emit must yield StmtList in statement position",
                            stmt.span,
                        ));
                    };
                    self.clear_statement_source_sites(stmt)?;
                    self.statements(module, &mut generated, owner, depth + 1)?;
                    output.extend(generated);
                }
                s::StmtKind::Meta(s::MetaStatement::If {
                    then_body,
                    else_body,
                    ..
                }) => {
                    self.block(module, then_body, owner, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(module, body, owner, depth + 1)?;
                    }
                    self.clear_statement_source_sites(stmt)?;
                    output.push(take_statement(stmt));
                }
                s::StmtKind::Meta(s::MetaStatement::For { body, .. }) => {
                    self.block(module, body, owner, depth + 1)?;
                    self.clear_statement_source_sites(stmt)?;
                    output.push(take_statement(stmt));
                }
                _ => {
                    self.statement(module, stmt, owner, depth + 1)?;
                    self.clear_statement_source_sites(stmt)?;
                    output.push(take_statement(stmt));
                }
            }
        }
        *statements = output;
        Ok(())
    }
    fn statement(
        &mut self,
        module: ModuleId,
        stmt: &mut s::Stmt,
        owner: &CtSymbolId,
        depth: usize,
    ) -> CompileResult<()> {
        self.depth(depth, stmt.span)?;
        match &mut stmt.kind {
            s::StmtKind::Binding {
                pattern, ty, value, ..
            } => {
                if let Some(ty) = ty {
                    self.ty(module, ty, owner, depth + 1)?;
                }
                self.expr(module, value, owner, depth + 1)?;
                self.bind_runtime_pattern(pattern);
            }
            s::StmtKind::Return(Some(e)) | s::StmtKind::Yield(e) | s::StmtKind::Expression(e) => {
                self.expr(module, e, owner, depth + 1)?
            }
            s::StmtKind::Return(None) | s::StmtKind::Break | s::StmtKind::Continue => {}
            s::StmtKind::Defer(block) | s::StmtKind::Loop(block) | s::StmtKind::Unsafe(block) => {
                self.block(module, block, owner, depth + 1)?
            }
            s::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.condition(module, condition, owner, depth + 1)?;
                self.block(module, then_body, owner, depth + 1)?;
                if let Some(branch) = else_body {
                    match branch {
                        s::ElseBranch::Block(block) => {
                            self.block(module, block, owner, depth + 1)?
                        }
                        s::ElseBranch::If(stmt) => {
                            self.statement(module, stmt, owner, depth + 1)?
                        }
                    }
                }
            }
            s::StmtKind::While { condition, body } => {
                self.condition(module, condition, owner, depth + 1)?;
                self.block(module, body, owner, depth + 1)?;
            }
            s::StmtKind::For { iterable, body, .. } => {
                self.expr(module, iterable, owner, depth + 1)?;
                self.block(module, body, owner, depth + 1)?;
            }
            s::StmtKind::Match { value, arms } => {
                self.expr(module, value, owner, depth + 1)?;
                for arm in arms {
                    self.arrow(module, &mut arm.body, owner, depth + 1)?;
                }
            }
            s::StmtKind::With { value, body, .. } => {
                self.expr(module, value, owner, depth + 1)?;
                self.block(module, body, owner, depth + 1)?;
            }
            s::StmtKind::Assign { target, value, .. } => {
                self.expr(module, target, owner, depth + 1)?;
                self.expr(module, value, owner, depth + 1)?;
            }
            s::StmtKind::Meta(_) => {}
        }
        Ok(())
    }
    fn condition(
        &mut self,
        module: ModuleId,
        condition: &mut s::Condition,
        owner: &CtSymbolId,
        depth: usize,
    ) -> CompileResult<()> {
        match condition {
            s::Condition::Expression(e) | s::Condition::Let { value: e, .. } => {
                self.expr(module, e, owner, depth)
            }
        }
    }
    fn arrow(
        &mut self,
        module: ModuleId,
        arrow: &mut s::ArrowBody,
        owner: &CtSymbolId,
        depth: usize,
    ) -> CompileResult<()> {
        match arrow {
            s::ArrowBody::Expression(e) => self.expr(module, e, owner, depth),
            s::ArrowBody::Block(block) => self.block(module, block, owner, depth),
        }
    }
    fn ty(
        &mut self,
        module: ModuleId,
        ty: &mut s::TypeRef,
        owner: &CtSymbolId,
        depth: usize,
    ) -> CompileResult<()> {
        self.depth(depth, ty.span)?;
        if matches!(
            ty.kind,
            s::TypeKind::MetaSplice(_) | s::TypeKind::MetaEval(_)
        ) {
            self.closure_sites_changed(ty.span)?;
        }
        if let s::TypeKind::Dyn { bindings, .. } = &mut ty.kind {
            self.environment
                .budget
                .reserve_analysis_work(bindings.len() as u64)?;
            for binding in bindings {
                match &mut binding.kind {
                    s::DynBindingKind::Type { value, .. } => {
                        self.ty(module, value, owner, depth + 2)?
                    }
                    s::DynBindingKind::Constant { ty, value, .. } => {
                        self.ty(module, ty, owner, depth + 2)?;
                        self.expr(module, value, owner, depth + 2)?;
                    }
                    s::DynBindingKind::Method { arguments, .. } => {
                        for argument in arguments {
                            match argument {
                                s::GenericArg::Type(ty) => self.ty(module, ty, owner, depth + 2)?,
                                s::GenericArg::Const(value) => {
                                    self.expr(module, value, owner, depth + 2)?
                                }
                                s::GenericArg::Lifetime(_) => {}
                            }
                        }
                    }
                }
            }
        }
        match &mut ty.kind {
            s::TypeKind::MetaSplice(expression) => {
                self.reject_alias_cycle(owner, expression)?;
                let value = self.expression_value(
                    module,
                    owner,
                    expression,
                    Some(CtTy::Syntax(CtSyntaxCategory::Type)),
                )?;
                let GeneratedSyntax::Type(replacement) =
                    self.parsed(value, CtSyntaxCategory::Type, ty.span)?
                else {
                    return Err(category("type #splice needs Syntax<Type>", ty.span));
                };
                *ty = replacement;
            }
            s::TypeKind::MetaEval(expression) => {
                self.reject_alias_cycle(owner, expression)?;
                let output = self.expression_data(module, owner, expression, Some(CtTy::Type))?;
                let CtValue::Type(value) = output.value else {
                    return Err(category("type #(...) needs Type value", ty.span));
                };
                if !value.ambient_classes.is_empty() {
                    let context = output.context.as_ref().ok_or_else(|| {
                        runtime(
                            "contextual Type result lost its actual source context",
                            ty.span,
                        )
                    })?;
                    let scopes =
                        context.source_scopes_for_graph(&value, &mut self.environment.budget)?;
                    let mut name_bytes = 0u64;
                    for node in &value.nodes {
                        self.environment.budget.reserve_analysis_work(1)?;
                        if let CtTypeNode::Nominal { symbol, .. } = node {
                            let (_, parsed) = self.parsed_source(symbol.module)?;
                            let item = parsed
                                .module
                                .items
                                .get(symbol.declaration.0 as usize)
                                .ok_or_else(|| {
                                    runtime("Type result nominal declaration is absent", ty.span)
                                })?;
                            let name = match &item.kind {
                                s::ItemKind::Aggregate(a) => &a.name,
                                s::ItemKind::Enum(e) => &e.name,
                                _ => {
                                    return Err(runtime(
                                        "Type result has no nominal declaration",
                                        ty.span,
                                    ));
                                }
                            };
                            name_bytes = name_bytes.saturating_add(name.len() as u64);
                        }
                    }
                    self.environment.budget.reserve_analysis_work(name_bytes)?;
                    self.environment.budget.reserve_temporary_bytes(
                        name_bytes.saturating_add((value.nodes.len() as u64).saturating_mul(256)),
                    )?;
                    let previous = std::mem::replace(&mut self.data_source_scopes, scopes);
                    let replacement =
                        self.owner_node(&value, value.root, module, ty.span, depth + 1);
                    self.data_source_scopes = previous;
                    self.environment.budget.release_value(&CtValue::Type(value));
                    self.environment
                        .budget
                        .release_temporary_bytes(context.bytes);
                    *ty = replacement?;
                    return Ok(());
                }
                let syntax = self.environment.type_syntax(&value)?;
                self.environment.budget.release_value(&CtValue::Type(value));
                let GeneratedSyntax::Type(replacement) =
                    self.parsed(CtValue::Syntax(syntax), CtSyntaxCategory::Type, ty.span)?
                else {
                    return Err(category("Type value did not reify to Type syntax", ty.span));
                };
                *ty = replacement;
            }
            s::TypeKind::Named { args, .. } | s::TypeKind::Dyn { args, .. } => {
                for arg in args {
                    match arg {
                        s::GenericArg::Lifetime(_) => {}
                        s::GenericArg::Type(ty) => self.ty(module, ty, owner, depth + 1)?,
                        s::GenericArg::Const(e) => self.expr(module, e, owner, depth + 1)?,
                    }
                }
            }
            s::TypeKind::Reference { inner, .. }
            | s::TypeKind::Group(inner)
            | s::TypeKind::Optional(inner)
            | s::TypeKind::ForLifetime { inner, .. } => self.ty(module, inner, owner, depth + 1)?,
            s::TypeKind::Tuple(types) | s::TypeKind::Union(types) | s::TypeKind::Impl(types) => {
                for ty in types {
                    self.ty(module, ty, owner, depth + 1)?;
                }
            }
            s::TypeKind::Array { element, length } => {
                self.ty(module, element, owner, depth + 1)?;
                if let Some(e) = length {
                    self.expr(module, e, owner, depth + 1)?;
                }
            }
            s::TypeKind::Function { params, result }
            | s::TypeKind::ForeignFunction { params, result, .. } => {
                for ty in params {
                    self.ty(module, ty, owner, depth + 1)?;
                }
                self.ty(module, result, owner, depth + 1)?;
            }
            s::TypeKind::Error { error, result } => {
                self.ty(module, error, owner, depth + 1)?;
                self.ty(module, result, owner, depth + 1)?;
            }
            s::TypeKind::MetaBlock(_) => {
                return Err(runtime(
                    "compile-time Type block must be materialized explicitly",
                    ty.span,
                ));
            }
        }
        Ok(())
    }
    fn expr(
        &mut self,
        module: ModuleId,
        expression: &mut s::Expr,
        owner: &CtSymbolId,
        depth: usize,
    ) -> CompileResult<()> {
        self.depth(depth, expression.span)?;
        if matches!(
            expression.kind,
            s::ExprKind::MetaSplice(_)
                | s::ExprKind::SyntaxIsland { .. }
                | s::ExprKind::MetaEval(_)
                | s::ExprKind::MetaBlock(_)
        ) {
            self.closure_sites_changed(expression.span)?;
        }
        match &mut expression.kind {
            s::ExprKind::Ascribed { ty, value } => {
                self.ty(module, ty, owner, depth + 1)?;
                self.expr(module, value, owner, depth + 1)?;
            }
            s::ExprKind::MetaSplice(inner) => {
                let value = self.expression_value(
                    module,
                    owner,
                    inner,
                    Some(CtTy::Syntax(CtSyntaxCategory::Expr)),
                )?;
                let GeneratedSyntax::Expr(replacement) =
                    self.parsed(value, CtSyntaxCategory::Expr, expression.span)?
                else {
                    return Err(category(
                        "expression #splice needs Syntax<Expr>",
                        expression.span,
                    ));
                };
                *expression = replacement;
            }
            s::ExprKind::SyntaxIsland { path, tree } => {
                let value = self.provider(
                    module,
                    path,
                    tree,
                    expression.span,
                    &CtTy::Syntax(CtSyntaxCategory::Expr),
                )?;
                let GeneratedSyntax::Expr(replacement) =
                    self.parsed(value, CtSyntaxCategory::Expr, expression.span)?
                else {
                    return Err(category(
                        "expression provider needs Syntax<Expr>",
                        expression.span,
                    ));
                };
                *expression = replacement;
            }
            s::ExprKind::Quote { .. } => {
                return Err(runtime(
                    "Syntax value cannot enter a runtime expression",
                    expression.span,
                ));
            }
            s::ExprKind::MetaEval(inner) => {
                if !self.uses_ct_domain(module, inner)? {
                    // Ordinary scalar, Copy record, enum and opaque CT values
                    // retain the established producer and receiving type
                    // context. Selection happens before either VM runs.
                    return Ok(());
                }
                let evaluated = self.expression_data(module, owner, inner, None)?;
                if let Some(context) = evaluated.context {
                    let replacement = self.data_expression(
                        module,
                        &evaluated.value,
                        &context.ty,
                        &context,
                        expression.span,
                        depth + 1,
                    );
                    self.environment.budget.release_value(&evaluated.value);
                    self.environment
                        .budget
                        .release_temporary_bytes(context.bytes);
                    *expression = replacement?;
                    return Ok(());
                }
                let CtValue::Scalar(scalar) = evaluated.value else {
                    return Err(runtime(
                        "Type, Syntax, Ident and CT views cannot enter runtime expressions",
                        expression.span,
                    ));
                };
                expression.kind = match scalar {
                    value @ (zil_ir::Constant::F32(_) | zil_ir::Constant::F64(_)) => {
                        float_expression(&value, &mut self.environment.budget, expression.span)?
                            .kind
                    }
                    zil_ir::Constant::Bool(value) => s::ExprKind::Literal(s::Literal::Bool(value)),
                    zil_ir::Constant::U8(value) => {
                        s::ExprKind::Literal(s::Literal::Integer(value.to_string()))
                    }
                    zil_ir::Constant::U16(value) => {
                        s::ExprKind::Literal(s::Literal::Integer(value.to_string()))
                    }
                    zil_ir::Constant::I32(value) => {
                        s::ExprKind::Literal(s::Literal::Integer(value.to_string()))
                    }
                    zil_ir::Constant::U64(value) => {
                        s::ExprKind::Literal(s::Literal::Integer(format!("{value}u64")))
                    }
                    zil_ir::Constant::Usize(value) => {
                        s::ExprKind::Literal(s::Literal::Integer(format!("{value}usize")))
                    }
                    zil_ir::Constant::I64(value) => {
                        s::ExprKind::Literal(s::Literal::Integer(value.to_string()))
                    }
                    zil_ir::Constant::Unit => s::ExprKind::Tuple(Vec::new()),
                    _ => {
                        return Err(runtime(
                            "CT result has no ordinary scalar literal",
                            expression.span,
                        ));
                    }
                };
                self.environment.budget.release_temporary_bytes(16);
            }
            s::ExprKind::MetaBlock(_) => {}
            s::ExprKind::Group(e) | s::ExprKind::Try(e) | s::ExprKind::Unary { operand: e, .. } => {
                self.expr(module, e, owner, depth + 1)?
            }
            s::ExprKind::Tuple(values) | s::ExprKind::Array(values) => {
                for e in values {
                    self.expr(module, e, owner, depth + 1)?;
                }
            }
            s::ExprKind::Binary { left, right, .. }
            | s::ExprKind::Pipeline {
                input: left,
                target: right,
            } => {
                self.expr(module, left, owner, depth + 1)?;
                self.expr(module, right, owner, depth + 1)?;
            }
            s::ExprKind::Member { object, .. } => self.expr(module, object, owner, depth + 1)?,
            s::ExprKind::TypeApply { callee, arguments } => {
                self.expr(module, callee, owner, depth + 1)?;
                for arg in arguments {
                    match arg {
                        s::GenericArg::Lifetime(_) => {}
                        s::GenericArg::Type(ty) => self.ty(module, ty, owner, depth + 1)?,
                        s::GenericArg::Const(e) => self.expr(module, e, owner, depth + 1)?,
                    }
                }
            }
            s::ExprKind::Call { callee, arguments } => {
                self.expr(module, callee, owner, depth + 1)?;
                for arg in arguments {
                    self.expr(module, &mut arg.value, owner, depth + 1)?;
                }
            }
            s::ExprKind::Index { object, index } => {
                self.expr(module, object, owner, depth + 1)?;
                self.expr(module, index, owner, depth + 1)?;
            }
            s::ExprKind::Slice { object, start, end } => {
                self.expr(module, object, owner, depth + 1)?;
                for e in [start, end].into_iter().flatten() {
                    self.expr(module, e, owner, depth + 1)?;
                }
            }
            s::ExprKind::Record { ty, base, fields } => {
                if let Some(ty) = ty {
                    self.expr(module, ty, owner, depth + 1)?;
                }
                if let Some(base) = base {
                    self.expr(module, base, owner, depth + 1)?;
                }
                for field in fields {
                    if let Some(e) = &mut field.value {
                        self.expr(module, e, owner, depth + 1)?;
                    }
                }
            }
            s::ExprKind::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                for param in params {
                    if let Some(ty) = &mut param.ty {
                        self.ty(module, ty, owner, depth + 1)?;
                    }
                }
                if let Some(ty) = return_type {
                    self.ty(module, ty, owner, depth + 1)?;
                }
                self.arrow(module, body, owner, depth + 1)?;
            }
            s::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(module, condition, owner, depth + 1)?;
                self.expr(module, then_value, owner, depth + 1)?;
                self.expr(module, else_value, owner, depth + 1)?;
            }
            s::ExprKind::Literal(_) | s::ExprKind::Name(_) | s::ExprKind::ReceiverMember(_) => {}
        }
        Ok(())
    }
}

// This is the single project materialization boundary: the parsed graph,
// target, limits and frozen provider registry are all independent inputs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn materialize(
    sources: &[SourceUnit],
    parsed: &[Arc<ParseResult>],
    modules: &[&s::Module],
    paths: &BTreeMap<Vec<String>, ModuleId>,
    imports: &[Vec<CoreImport>],
    root: ModuleId,
    fingerprint: Fingerprint,
    target: &TargetProfile,
    limits: &Limits,
    registry: Option<&CtLockedProviderRegistry>,
    original_declarations: bool,
) -> CompileResult<Materialized> {
    let (mut declaration_origins, capture_remaining) = if original_declarations {
        zil_expand::source_context::capture_original(
            sources,
            modules,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root,
                aliases: paths,
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
            limits,
        )?
    } else {
        (None, limits.clone())
    };
    let mut environment = Environment::new(
        sources,
        parsed,
        paths,
        imports,
        root,
        fingerprint,
        target,
        &capture_remaining,
    )?;
    environment.bind_source_origins(declaration_origins.as_deref())?;
    environment.classify_definitions()?;
    let mut output = modules
        .iter()
        .map(|module| (*module).clone())
        .collect::<Vec<_>>();
    let mut source_namespaces = BTreeMap::new();
    if let Some(origins) = &declaration_origins {
        for module in origins {
            if let Some(row) = module.items.first() {
                let bytes = super::invocation_context::identity_cost(
                    &row.identity,
                    &mut environment.budget,
                )?;
                environment.budget.reserve_temporary_bytes(
                    bytes + std::mem::size_of::<zil_ir::LifeDeclarationIdentity>() as u64,
                )?;
                source_namespaces.insert(module.module, row.identity.clone());
            }
        }
    }
    let mut materializer = Materializer {
        environment: &mut environment,
        sources,
        parsed,
        registry,
        paths,
        previews: Vec::new(),
        required_imports: Vec::new(),
        runtime_scopes: Vec::new(),
        data_source_scopes: Vec::new(),
        max_depth: limits.max_call_depth,
        track_declarations: declaration_origins.is_some(),
        declaration_lineage_preserved: true,
        member_retentions: Vec::new(),
        generated_trees: Vec::new(),
        current_generated_tree: None,
        pending_generated_tree: None,
        current_emitter: None,
        item_path: Vec::new(),
        next_generated_declaration: BTreeMap::new(),
        source_namespaces,
        canonical_source_sites: BTreeMap::new(),
        indexed_source_trees: BTreeSet::new(),
        bound_source_sites: Default::default(),
        generated_context: false,
        closure_site_revision: 0,
        changed_closure_sites: BTreeSet::new(),
        self_constant_sources: Vec::new(),
        self_constant_impl_sources: Vec::new(),
        self_constant_impl_outputs: Vec::new(),
    };
    for (index, module) in output.iter_mut().enumerate() {
        materializer.self_constants(module)?;
        let retained = materializer.items(module.id, &mut module.items, None, 0, false)?;
        if !materializer.declaration_lineage_preserved {
            declaration_origins = None;
        }
        if let Some(origins) = &mut declaration_origins {
            let rows = origins
                .get_mut(index)
                .ok_or_else(|| category("declaration-origin module absent", module.span))?;
            if rows.module != module.id {
                return Err(category(
                    "declaration-origin module owner differs",
                    module.span,
                ));
            }
            materializer.attach_member_retentions(module.id, &mut rows.items)?;
            let original_count = rows.items.len();
            materializer
                .environment
                .budget
                .reserve_analysis_work(original_count as u64)?;
            materializer.environment.budget.reserve_temporary_bytes(
                original_count as u64
                    * std::mem::size_of::<zil_expand::source_context::DeclarationOrigin>() as u64,
            )?;
            let retained = retained
                .ok_or_else(|| category("retained declaration positions absent", module.span))?;
            let mut original_rows = std::mem::take(&mut rows.items)
                .into_iter()
                .map(Some)
                .collect::<Vec<_>>();
            let mut moved = Vec::with_capacity(retained.len());
            for entry in retained {
                let mut row = match entry {
                    generated_lineage::OutputOrigin::Generated(row) => row,
                    generated_lineage::OutputOrigin::Original(ordinal) => {
                        let mut row = original_rows
                            .get_mut(ordinal)
                            .and_then(Option::take)
                            .ok_or_else(|| {
                                category("retained declaration repeats or is absent", module.span)
                            })?;
                        if materializer
                            .changed_closure_sites
                            .contains(&(module.id, ordinal))
                        {
                            row.closure_sites =
                                zil_expand::source_context::ClosureSiteLineage::Unavailable;
                        }
                        row
                    }
                };
                moved.push(row);
            }
            if moved.len() != module.items.len() {
                return Err(category(
                    "retained declaration positions differ from output",
                    module.span,
                ));
            }
            rows.items = moved;
        }
    }
    materializer.audit_self_constant_coverage(&output, imports)?;
    if !materializer.declaration_lineage_preserved {
        declaration_origins = None;
    }
    let lineage_remaining =
        original_declarations.then(|| materializer.environment.remaining_limits_for_input());
    let mut usage = materializer.environment.budget.usage().clone();
    usage.analysis_work = usage
        .analysis_work
        .checked_add((limits.max_analysis_steps - capture_remaining.max_analysis_steps) as u64)
        .ok_or_else(|| category("declaration capture work count overflow", Span::default()))?;
    usage.generated_nodes = usage
        .generated_nodes
        .checked_add((limits.max_nodes - capture_remaining.max_nodes) as u64)
        .ok_or_else(|| category("declaration capture node count overflow", Span::default()))?;
    Ok(Materialized {
        modules: output,
        previews: materializer.previews,
        required_imports: materializer.required_imports,
        usage,
        declaration_origins,
        lineage_remaining,
    })
}

#[cfg(test)]
mod float_tests {
    use super::*;
    use zil_ir::{Constant, InstructionKind};

    fn allowance() -> CtDeclaredBudget {
        CtDeclaredBudget {
            max_steps: 100,
            max_call_depth: 10,
            max_live_cells: 100,
            max_temporary_bytes: 26,
            max_generated_nodes: 1,
            max_instances: 1,
            max_analysis_work: 26,
        }
    }

    fn generated_module(value: s::Expr, name: &str) -> (SourceUnit, s::Module) {
        let source = SourceUnit::new(
            zil_base::FileId(0),
            ModuleId(0),
            "float-bits.zil",
            format!("fn result()->{name}{{return 0.0}}"),
        );
        let mut syntax = zil_syntax::parse(&source).into_result().unwrap();
        let s::ItemKind::Function(function) = &mut syntax.items[0].kind else {
            unreachable!()
        };
        let s::FunctionBody::Block(body) = &mut function.body else {
            unreachable!()
        };
        body.statements[0].kind = s::StmtKind::Return(Some(value));
        (source, syntax)
    }

    #[test]
    fn ieee_float_ct_reification_preserves_all_raw_bits_through_wire() {
        let target = TargetProfile::host_llvm();
        let limits = Limits::default();
        let mut fingerprints = BTreeSet::new();
        for constant in [
            Constant::F32(0),
            Constant::F32(0x80000000),
            Constant::F32(1),
            Constant::F32(0x7fc00001),
            Constant::F32(0x7f800001),
            Constant::F64(0),
            Constant::F64(0x8000000000000000),
            Constant::F64(1),
            Constant::F64(0x7ff8000000000001),
            Constant::F64(0xfff0000000000001),
        ] {
            let mut budget = zil_ct::CtExecutionBudget::new(allowance());
            let expression = float_expression(&constant, &mut budget, Span::default()).unwrap();
            assert_eq!(budget.usage().analysis_work, 26);
            assert_eq!(budget.usage().generated_nodes, 1);
            assert_eq!(budget.live_temporary_bytes(), 26);
            let name = if matches!(constant, Constant::F32(_)) {
                "f32"
            } else {
                "f64"
            };
            let (source, syntax) = generated_module(expression, name);
            let core = zil_expand::expand(&source, &syntax).unwrap();
            let typed = zil_types::check(&core, &target, &limits).unwrap();
            assert!(typed.functions[0].blocks.iter().flat_map(|b| &b.instructions)
                .any(|i| matches!(&i.kind, InstructionKind::Constant(value) if value == &constant)));
            assert!(fingerprints.insert(typed.fingerprint()));
            let plan = zil_analyze::analyze(&typed, &limits).unwrap();
            zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
            let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
            assert_eq!(
                zil_verify::verify_module_bytes(&wire, &target, &limits)
                    .unwrap()
                    .module(),
                &typed
            );
        }
        for dimension in 0..3 {
            let mut limits = allowance();
            match dimension {
                0 => limits.max_analysis_work -= 1,
                1 => limits.max_temporary_bytes -= 1,
                _ => limits.max_generated_nodes -= 1,
            }
            let mut budget = zil_ct::CtExecutionBudget::new(limits);
            let errors =
                float_expression(&Constant::F64(1), &mut budget, Span::default()).unwrap_err();
            assert!(
                errors
                    .iter()
                    .all(|error| error.category == zil_base::ResultCategory::ResourceLimit)
            );
            assert_eq!(budget.usage().generated_nodes, 0);
        }
    }

    #[test]
    fn ieee_float_generated_spellings_are_untrusted_and_unlexable() {
        for spelling in [
            "@bits:f32:0000000",
            "@bits:f32:000000000",
            "@bits:f32:7FC00001",
            "@bits:f64:7ff800000000000g",
            "@bits:f16:00000000",
        ] {
            let expression = s::Expr {
                span: Span::default(),
                kind: s::ExprKind::Literal(s::Literal::Float(spelling.into())),
            };
            let (source, syntax) = generated_module(expression, "f32");
            let errors = zil_expand::expand(&source, &syntax).unwrap_err();
            assert!(
                errors.iter().any(|error| error.code == "E-FLOAT-LITERAL"),
                "{errors:?}"
            );
        }
        let mut budget = zil_ct::CtExecutionBudget::new(allowance());
        let expression =
            float_expression(&Constant::F32(0x7fc00001), &mut budget, Span::default()).unwrap();
        let (source, syntax) = generated_module(expression, "f64");
        let core = zil_expand::expand(&source, &syntax).unwrap();
        assert!(zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).is_err());
        let source = SourceUnit::new(
            zil_base::FileId(0),
            ModuleId(0),
            "not-source.zil",
            "fn result()->f32{return @bits:f32:7fc00001}",
        );
        assert!(zil_syntax::parse(&source).into_result().is_err());
    }
}

#[path = "materialize_value.rs"]
mod data_value;

#[cfg(test)]
mod scoped_tests {
    use super::*;
    fn run(text: &str) -> CompileResult<Materialized> {
        run_limited(text, &Limits::default())
    }
    fn run_limited(text: &str, limits: &Limits) -> CompileResult<Materialized> {
        let source = SourceUnit::new(zil_base::FileId(0), ModuleId(0), "ct4-scoped.zil", text);
        let parsed = Arc::new(zil_syntax::parse(&source));
        let syntax = &parsed.module;
        materialize(
            std::slice::from_ref(&source),
            std::slice::from_ref(&parsed),
            &[syntax],
            &BTreeMap::from([(vec!["main".into()], ModuleId(0))]),
            &[vec![]],
            ModuleId(0),
            source.fingerprint(),
            &TargetProfile::host_llvm(),
            limits,
            None,
            true,
        )
    }
    #[test]
    fn closure_site_lineage_tracks_actual_materialization_and_retained_rows() {
        use zil_expand::source_context::ClosureSiteLineage::{Original, Unavailable};
        let decl = "struct Tag<life s>{p:&'s i64}";
        let closure = "(p:&Tag<'s>)->i32=>i32(*p.p+*left.p)";
        for (prefix, actual, expected) in [
            ("", closure.to_owned(), Original),
            (
                "",
                format!("#splice(quote<Expr>{{{closure}}})"),
                Unavailable,
            ),
            (
                "#emit quote<StmtList>{let generated=1;};",
                closure.to_owned(),
                Unavailable,
            ),
            (
                "#if(true){let generated=1;}",
                closure.to_owned(),
                Unavailable,
            ),
            (
                "#for(n in [1,2]){let generated=n;}",
                closure.to_owned(),
                Unavailable,
            ),
            ("let generated=#{yield 1};", closure.to_owned(), Unavailable),
        ] {
            // A removed CT-only item precedes both original creators. The
            // retained row must keep its original ordinal, not output index.
            let text = format!(
                "{decl}fn removed()->Syntax<Expr>{{return quote<Expr>{{1}}}}fn stable<life s>(left:&Tag<'s>)->i32{{let f={closure};return f(left)}}fn changed<life s>(left:&Tag<'s>)->i32{{{prefix}let f={actual};return f(left)}}fn after<life s>(left:&Tag<'s>)->i32{{let f={closure};return f(left)}}"
            );
            let syntax = zil_syntax::parse(&SourceUnit::new(
                zil_base::FileId(0),
                ModuleId(0),
                "ct4-scoped.zil",
                &text,
            ));
            assert!(
                syntax.diagnostics.is_empty(),
                "{prefix}: {:?}",
                syntax.diagnostics
            );
            let mut output = run(&text).unwrap_or_else(|e| panic!("{prefix}: {e:?}"));
            if actual.starts_with("#splice") {
                assert!(
                    !output.previews.is_empty(),
                    "actual syntax materialization must occur"
                );
                let s::ItemKind::Function(changed) = &output.modules[0].items[2].kind else {
                    unreachable!()
                };
                let s::FunctionBody::Block(body) = &changed.body else {
                    unreachable!()
                };
                assert!(matches!(&body.statements[0].kind,
                    s::StmtKind::Binding { value, .. } if matches!(value.kind, s::ExprKind::Closure { .. })));
            }
            let rows = &output.declaration_origins.as_ref().unwrap()[0].items;
            assert_eq!(rows.len(), 4);
            assert_eq!(
                rows.iter().map(|r| r.declaration.0).collect::<Vec<_>>(),
                [0, 2, 3, 4]
            );
            assert_eq!(
                rows.iter().map(|r| r.closure_sites).collect::<Vec<_>>(),
                [Original, Original, expected, Original],
                "prefix={prefix}; actual={actual}"
            );
            let before_identity = rows[2].identity.clone();
            output
                .retain_items(
                    0,
                    |item| !matches!(&item.kind, s::ItemKind::Function(f) if f.name == "stable"),
                )
                .unwrap();
            let rows = &output.declaration_origins.as_ref().unwrap()[0].items;
            assert_eq!(
                rows.iter().map(|r| r.declaration.0).collect::<Vec<_>>(),
                [0, 3, 4]
            );
            assert_eq!(rows[1].closure_sites, expected);
            assert_eq!(rows[1].identity, before_identity);
        }
    }
    #[test]
    fn closure_site_lineage_shared_spans_and_event_budget_are_independent() {
        use zil_expand::source_context::ClosureSiteLineage::{Original, Unavailable};
        let text = "struct Tag<life s>{p:&'s i64}fn changed<life s>(left:&Tag<'s>)->i32{let n=#splice(quote<Expr>{1});return n}fn stable<life s>(left:&Tag<'s>)->i32{return 1}";
        let source = SourceUnit::new(
            zil_base::FileId(0),
            ModuleId(0),
            "closure-lineage.zil",
            text,
        );
        let mut parsed = zil_syntax::parse(&source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        for item in &mut parsed.module.items {
            item.span = parsed.module.span;
        }
        let parsed = Arc::new(parsed);
        let run = |work| {
            materialize(
                std::slice::from_ref(&source),
                std::slice::from_ref(&parsed),
                &[&parsed.module],
                &BTreeMap::from([(vec!["main".into()], ModuleId(0))]),
                &[vec![]],
                ModuleId(0),
                source.fingerprint(),
                &TargetProfile::host_llvm(),
                &Limits {
                    max_analysis_steps: work,
                    ..Limits::default()
                },
                None,
                true,
            )
        };
        let expected = run(Limits::default().max_analysis_steps).unwrap();
        let origins = expected.declaration_origins.unwrap();
        assert_eq!(
            origins[0]
                .items
                .iter()
                .map(|r| r.closure_sites)
                .collect::<Vec<_>>(),
            [Original, Unavailable, Original]
        );
        let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            match run(mid) {
                Ok(_) => high = mid,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = mid + 1;
                }
            }
        }
        for (work, success) in [(low, true), (low - 1, false), (low, true)] {
            let result = run(work);
            if success {
                assert_eq!(result.unwrap().declaration_origins.unwrap(), origins);
            } else {
                assert!(
                    result
                        .err()
                        .unwrap()
                        .iter()
                        .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
                );
            }
        }
        eprintln!("G3.3.2.4 actual closure lineage N={low}/N-1/fresh/retry");
    }
    #[test]
    fn g231_actual_output_coverage_rejects_unprepared_impls_and_tracks_moves() {
        fn audit(text: &str, extra: &str, move_original: bool) -> CompileResult<()> {
            let source =
                SourceUnit::new(zil_base::FileId(0), ModuleId(0), "self-coverage.zil", text);
            let parsed = Arc::new(zil_syntax::parse(&source));
            assert!(parsed.diagnostics.is_empty());
            let sources = std::slice::from_ref(&source);
            let parsed_slice = std::slice::from_ref(&parsed);
            let paths = BTreeMap::from([(vec!["main".into()], ModuleId(0))]);
            let imports = vec![vec![]];
            let limits = Limits::default();
            let target = TargetProfile::host_llvm();
            let mut environment = Environment::new(
                sources,
                parsed_slice,
                &paths,
                &imports,
                ModuleId(0),
                source.fingerprint(),
                &target,
                &limits,
            )?;
            environment.classify_definitions()?;
            let mut materializer = Materializer {
                environment: &mut environment,
                sources,
                parsed: parsed_slice,
                registry: None,
                paths: &paths,
                previews: vec![],
                required_imports: vec![],
                runtime_scopes: vec![],
                data_source_scopes: vec![],
                max_depth: limits.max_call_depth,
                track_declarations: false,
                declaration_lineage_preserved: true,
                member_retentions: Vec::new(),
                generated_trees: Vec::new(),
                current_generated_tree: None,
                pending_generated_tree: None,
                current_emitter: None,
                item_path: Vec::new(),
                next_generated_declaration: BTreeMap::new(),
                source_namespaces: BTreeMap::new(),
                canonical_source_sites: BTreeMap::new(),
                indexed_source_trees: BTreeSet::new(),
                bound_source_sites: Default::default(),
                generated_context: false,
                closure_site_revision: 0,
                changed_closure_sites: BTreeSet::new(),
                self_constant_sources: vec![],
                self_constant_impl_sources: vec![],
                self_constant_impl_outputs: vec![],
            };
            let mut module = parsed.module.clone();
            materializer.self_constants(&mut module)?;
            if move_original {
                materializer.items(module.id, &mut module.items, None, 0, false)?;
                assert_eq!(materializer.self_constant_impl_sources.len(), 1);
                assert_eq!(materializer.self_constant_impl_outputs.len(), 1);
                assert_ne!(
                    materializer.self_constant_impl_sources[0].1,
                    materializer.self_constant_impl_outputs[0].1,
                    "the CT-only item must actually be removed before its impl"
                );
            } else {
                assert_eq!(
                    materializer.environment.budget.usage().steps,
                    0,
                    "symbolic templates execute no fabricated Self values"
                );
                let generated = SourceUnit::new(
                    zil_base::FileId(0),
                    ModuleId(0),
                    "untrusted-output.zil",
                    extra,
                );
                let generated = zil_syntax::parse(&generated).into_result().unwrap();
                module.items.extend(generated.items);
            }
            materializer.audit_self_constant_coverage(&[module], &imports)
        }
        let base = "trait C{const A:i32;const B:i32=Self.A+1;}struct P{}fn main(){}";
        for extra in [
            "impl C for P{const A:i32=2147483647;const B:i32=0;}",
            "type Alias=C;impl Alias for P{const A:i32=2147483647;const B:i32=0;}",
            "type Alias<T>=T;type Relay<T>=Alias<T>;impl Relay<Alias<C>> for P{const A:i32=2147483647;const B:i32=0;}",
            "#if(true){impl C for P{const A:i32=2147483647;const B:i32=0;}}",
            "#if(true){type Alias=C;}else{type Alias=C;}impl Alias for P{const A:i32=2147483647;const B:i32=0;}",
        ] {
            let errors = audit(base, extra, false).unwrap_err();
            assert_eq!(
                errors[0].category,
                zil_base::ResultCategory::Unsupported,
                "{errors:?}"
            );
            assert!(
                errors[0].message.contains("no completed Source CT jobs"),
                "{errors:?}"
            );
        }
        let errors = audit(
            base,
            "type A=B;type B=A;impl A for P{const A:i32=0;const B:i32=0;}",
            false,
        )
        .unwrap_err();
        assert!(
            errors[0].message.contains("recursive trait alias"),
            "{errors:?}"
        );
        audit("trait C{const A:i32;const B:i32=42;}trait Other{const A:i32;const B:i32=Self.A+1;}struct P{}fn main(){}","type Alias=C;impl Alias for P{const A:i32=41;const B:i32=0;}",false).unwrap();
        audit("trait C{const A:i32;const B:i32=Self.A+1;}fn discarded()->Type{return i32}struct P{}impl C for P{const A:i32=41;}fn main(){}","",true).unwrap();
    }

    #[test]
    fn g231_shared_constant_spans_keep_actual_source_owners_raw_and_wire() {
        let text = include_str!(
            "../../../../tests/language-readiness/terminal/341/associated-constant-self.zil"
        );
        let source = SourceUnit::new(zil_base::FileId(0), ModuleId(0), "self-shared.zil", text);
        let mut parsed = zil_syntax::parse(&source);
        assert!(parsed.diagnostics.is_empty());
        let shared = parsed.module.items[0].span;
        for item in &mut parsed.module.items {
            item.span = shared;
            match &mut item.kind {
                s::ItemKind::Function(f) => f.span = shared,
                s::ItemKind::Trait(t) => {
                    for member in &mut t.members {
                        member.span = shared;
                        if let s::ItemKind::Const(c) = &mut member.kind {
                            if let Some(ty) = &mut c.ty {
                                ty.span = shared;
                            }
                            if let Some(value) = &mut c.value {
                                value.span = shared;
                            }
                        }
                    }
                }
                s::ItemKind::Impl(i) => {
                    for member in &mut i.members {
                        member.span = shared;
                        if let s::ItemKind::Const(c) = &mut member.kind {
                            if let Some(ty) = &mut c.ty {
                                ty.span = shared;
                            }
                            if let Some(value) = &mut c.value {
                                value.span = shared;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let parsed = Arc::new(parsed);
        let limits = Limits::default();
        let target = TargetProfile::host_llvm();
        let materialized = materialize(
            std::slice::from_ref(&source),
            std::slice::from_ref(&parsed),
            &[&parsed.module],
            &BTreeMap::from([(vec!["main".into()], ModuleId(0))]),
            &[vec![]],
            ModuleId(0),
            source.fingerprint(),
            &target,
            &limits,
            None,
            true,
        )
        .unwrap();
        let core = zil_expand::expand(&source, &materialized.modules[0]).unwrap();
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        let verified =
            zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
        let lir = zil_lower::lower(&verified).unwrap();
        for optimization in [
            zil_backend_llvm::Optimization::O0,
            zil_backend_llvm::Optimization::O2,
        ] {
            let dir = std::env::temp_dir().join(format!(
                "g231-shared-{}-{:?}",
                std::process::id(),
                optimization
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let mut options = zil_backend_llvm::BuildOptions::new(dir.join("program"));
            options.backend = zil_backend_llvm::NativeBackend::Clang;
            options.optimization = optimization;
            let artifact = zil_backend_llvm::build(&verified, &lir, &options).unwrap();
            let output = std::process::Command::new(&artifact.executable)
                .output()
                .unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn ct5_shared_function_spans_keep_actual_lifetime_owners_through_verify() {
        check_shared_scope_spans(
            "struct Phantom<life s>{n:i32} fn scalar(t:Type,n:i32)->i32{return n} fn first<life left>(p:&'left i32)->i32{let value:Phantom<'left>=#(Phantom::<'left>{n:scalar(i32,42)});return value.n} fn second<life right>(p:&'right i32)->i32{let value:Phantom<'right>=#(Phantom::<'right>{n:scalar(i32,42)});return value.n} fn main()->i32{let n=0;return first(&n)+second(&n)-42}",
        );
        check_shared_scope_spans(
            "struct Phantom<life s>{n:i32} fn scalar(t:Type,n:i32)->i32{return n} struct Reader{fn first<life left>(self:Self,p:&'left i32)->i32{let value:Phantom<'left>=#(Phantom::<'left>{n:scalar(i32,42)});return value.n} fn second<life right>(self:Self,p:&'right i32)->i32{let value:Phantom<'right>=#(Phantom::<'right>{n:scalar(i32,42)});return value.n}} fn main()->i32{let n=0;let r=Reader{};return r.first(&n)+r.second(&n)-42}",
        );
    }
    fn check_shared_scope_spans(text: &str) {
        let source = SourceUnit::new(
            zil_base::FileId(0),
            ModuleId(0),
            "shared-scope-spans.zil",
            text,
        );
        let mut parsed = zil_syntax::parse(&source);
        assert!(parsed.diagnostics.is_empty());
        let shared = parsed.module.span;
        for item in &mut parsed.module.items {
            match &mut item.kind {
                s::ItemKind::Function(f) if f.name == "first" || f.name == "second" => {
                    item.span = shared
                }
                s::ItemKind::Aggregate(a) => {
                    for member in &mut a.methods {
                        member.span = shared;
                    }
                }
                _ => {}
            }
        }
        let parsed = Arc::new(parsed);
        let sources = std::slice::from_ref(&source);
        let syntax = [&parsed.module];
        let paths = BTreeMap::from([(vec!["main".into()], ModuleId(0))]);
        let limits = Limits::default();
        let target = TargetProfile::host_llvm();
        let output = materialize(
            sources,
            std::slice::from_ref(&parsed),
            &syntax,
            &paths,
            &[vec![]],
            ModuleId(0),
            source.fingerprint(),
            &target,
            &limits,
            None,
            true,
        )
        .unwrap();
        for (name, lifetime) in [("first", "left"), ("second", "right")] {
            let function = output.modules[0]
                .items
                .iter()
                .flat_map(|item| match &item.kind {
                    s::ItemKind::Aggregate(a) if !a.methods.is_empty() => a.methods.as_slice(),
                    _ => std::slice::from_ref(item),
                })
                .find_map(|item| match &item.kind {
                    s::ItemKind::Function(function) if function.name == name => Some(function),
                    _ => None,
                })
                .unwrap();
            let s::FunctionBody::Block(block) = &function.body else {
                unreachable!()
            };
            let s::StmtKind::Binding { value, .. } = &block.statements[0].kind else {
                unreachable!()
            };
            let s::ExprKind::Ascribed { ty, .. } = &value.kind else {
                panic!("scoped value lost ascription")
            };
            let s::TypeKind::Named { args, .. } = &ty.kind else {
                unreachable!()
            };
            assert!(matches!(&args[0],s::GenericArg::Lifetime(actual)if actual==lifetime));
        }
        let origins = output.declaration_origins.as_ref().unwrap();
        let refs = output.modules.iter().collect::<Vec<_>>();
        let expanded = zil_expand::expand_program_with_limits_target_context(
            sources,
            &refs,
            &[vec![]],
            &limits,
            &target,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &paths,
                declarations: zil_expand::source_context::DeclarationPlacement::Aligned(origins),
            },
        )
        .unwrap();
        let core = zil_ir::CoreProgram {
            root: ModuleId(0),
            source_fingerprint: source.fingerprint(),
            modules: expanded
                .into_iter()
                .map(|(module, _)| zil_ir::ProgramModule {
                    core: module.core,
                    imports: module.imports,
                    exports: module.exports,
                })
                .collect(),
        };
        let typed = zil_types::check_program(&core, &target, &limits).unwrap();
        let candidate = zil_analyze::analyze_program(&typed, &limits).unwrap();
        zil_verify::verify_shared_program_module(
            Arc::new(typed),
            Arc::new(candidate),
            &target,
            &limits,
        )
        .unwrap();
    }
    #[test]
    fn ct5_generated_member_keeps_naming_owner_without_original_binder_authority() {
        let text = include_str!(
            "../../../../tests/language-readiness/terminal/344/anonymous/ct5-generated-member.zil"
        );
        let probe = zil_syntax::parse(&SourceUnit::new(
            zil_base::FileId(0),
            ModuleId(0),
            "generated.zil",
            text,
        ));
        assert!(probe.diagnostics.is_empty(), "{:?}", probe.diagnostics);
        let output = run(text).unwrap();
        assert!(output.declaration_origins.is_none());
        let generated = text
            .replace("#(scalar(i32,42))", "#(Phantom::<'s>{n:scalar(i32,42)})")
            .replace("return value\n", "return value.n\n");
        let errors = run(&generated)
            .err()
            .expect("generated own binder needs authentic lineage");
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("actual lexical source owner")),
            "{errors:?}"
        );
    }
    #[test]
    fn ct4_materialization_binds_original_full_lifetime_ordinal() {
        let text = include_str!(
            "../../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-fixed-context.zil"
        );
        for (text, name) in [
            (text.to_owned(), "s"),
            (
                text.replace("result<life s>", "result<T:Copy,life renamed>")
                    .replace("'s", "'renamed")
                    .replace("return result(&n)", "return result::<i32>(&n)"),
                "renamed",
            ),
        ] {
            let output = run(&text).unwrap();
            let function = output.modules[0]
                .items
                .iter()
                .find_map(|item| match &item.kind {
                    s::ItemKind::Function(function) if function.name == "result" => Some(function),
                    _ => None,
                })
                .unwrap();
            let s::FunctionBody::Block(body) = &function.body else {
                unreachable!()
            };
            let s::StmtKind::Binding { value, .. } = &body.statements[0].kind else {
                unreachable!()
            };
            let s::ExprKind::Ascribed { ty, value } = &value.kind else {
                panic!("missing full type ascription {value:?}")
            };
            let s::TypeKind::Named { args, .. } = &ty.kind else {
                unreachable!()
            };
            let s::ExprKind::Record { fields, .. } = &value.kind else {
                unreachable!()
            };
            let s::ExprKind::Ascribed { value, .. } = &fields[0].value.as_ref().unwrap().kind
            else {
                unreachable!()
            };
            assert!(matches!(&value.kind,s::ExprKind::Literal(s::Literal::Integer(n)) if n=="42"));
            assert_eq!(args.len(), 1);
            assert!(matches!(&args[0],s::GenericArg::Lifetime(actual) if actual==name));
            assert!(output.usage.analysis_work > 0);
        }
        let collision = "struct Phantom<life a>{n:i32}fn scalar(t:Type,n:i32)->i32{return n}fn value<life a,life b>(pa:&'a i32,pb:&'b i32){let pair=#((Phantom::<'a>{n:scalar(i32,1)},Phantom::<'b>{n:scalar(i32,2)}))}";
        let output = run(collision).expect("CT5 preserves both full original lifetime ordinals");
        assert!(output.declaration_origins.is_some());
    }
    #[test]
    fn ct4_materialization_scope_work_is_exact_and_request_local() {
        let original = include_str!(
            "../../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-fixed-context.zil"
        );
        let long = format!("scope_{}", "x".repeat(512));
        let renamed = original
            .replace("life s", &format!("life {long}"))
            .replace("'s", &format!("'{long}"));
        let generic = include_str!(
            "../../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-generic-context.zil"
        );
        for text in [original, renamed.as_str(), generic] {
            let defaults = Limits::default();
            let run = |work| {
                run_limited(
                    text,
                    &Limits {
                        max_analysis_steps: work,
                        ..defaults.clone()
                    },
                )
            };
            assert!(run(defaults.max_analysis_steps).is_ok());
            let (mut low, mut high) = (1, defaults.max_analysis_steps);
            while low < high {
                let middle = (low + high) / 2;
                match run(middle) {
                    Ok(_) => high = middle,
                    Err(errors)
                        if errors
                            .iter()
                            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit) =>
                    {
                        low = middle + 1
                    }
                    Err(errors) => panic!("unexpected budget result {errors:?}"),
                }
            }
            for (work, good) in [(low, true), (low - 1, false), (low, true)] {
                let result = run(work);
                assert_eq!(result.is_ok(), good);
                if !good {
                    assert!(
                        result
                            .err()
                            .unwrap()
                            .iter()
                            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
                    );
                }
            }
            eprintln!("344 authentic CT4 source materialization N={low}/N-1/fresh");
        }
    }
    #[test]
    fn ct4_materialization_preserves_joint_and_mixed_nominal_actuals() {
        for text in [
            include_str!(
                "../../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-mixed-context.zil"
            ),
            include_str!(
                "../../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-joint-context.zil"
            ),
            include_str!(
                "../../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-generic-context.zil"
            ),
        ] {
            let output = run(text).unwrap();
            let function = output.modules[0]
                .items
                .iter()
                .find_map(|item| match &item.kind {
                    s::ItemKind::Function(f) if f.name == "result" => Some(f),
                    _ => None,
                })
                .unwrap();
            let s::FunctionBody::Block(body) = &function.body else {
                unreachable!()
            };
            let s::StmtKind::Binding { value, .. } = &body.statements[0].kind else {
                unreachable!()
            };
            let s::ExprKind::Ascribed { ty, .. } = &value.kind else {
                unreachable!()
            };
            let s::TypeKind::Named { args, .. } = &ty.kind else {
                unreachable!()
            };
            if text.contains("struct Pair") {
                assert!(matches!(&args[0],s::GenericArg::Lifetime(name) if name=="a"));
                assert!(matches!(&args[1],s::GenericArg::Lifetime(name) if name=="b"));
            } else if text.contains("#N:i32") {
                assert!(matches!(&args[0],s::GenericArg::Lifetime(name) if name=="s"));
                assert!(
                    matches!(&args[1],s::GenericArg::Type(ty) if matches!(&ty.kind,s::TypeKind::Named {path,args} if path==&["i32"] && args.is_empty()))
                );
                assert!(
                    matches!(&args[2],s::GenericArg::Const(value) if matches!(&value.kind,s::ExprKind::Literal(s::Literal::Integer(n)) if n=="7"))
                );
            } else {
                assert_eq!(args.len(), 1);
                assert!(matches!(&args[0],s::GenericArg::Lifetime(name) if name=="s"));
            }
        }
    }
}
