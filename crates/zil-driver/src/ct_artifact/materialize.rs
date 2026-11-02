//! Category-directed, transactional project materialization. The caller owns
//! the cloned AST until every generated list has passed its preflight.
use super::{compiler::Entry, execute::Environment, input, lock::provider_entry, render::ParsedSyntax};
use std::collections::BTreeMap;
use std::sync::Arc;
use zil_base::{CompileResult, Diagnostic, Fingerprint, Limits, ModuleId, Phase, SourceUnit, Span, TargetProfile};
use zil_ir::{CoreImport, ct_artifact::*};
use zil_syntax::{GeneratedSyntax, ParseResult, ast as s};

fn category(message: impl Into<String>, span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-CATEGORY", Some(span), message)]
}
fn runtime(message: impl Into<String>, span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Ct, "E-CT-RUNTIME", Some(span), message)]
}
fn late_import(span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Expand, "E-CT-LATE-IMPORT", Some(span), "generated import would change the sealed module/provider graph")]
}

pub(crate) struct Materialized {
    pub modules: Vec<s::Module>,
    pub previews: Vec<Preview>,
    pub required_imports: Vec<(ModuleId, CoreImport)>,
    pub usage: CtUsageReceipt,
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
        let span = |span: Span| format!("{{\"file\":{},\"start\":{},\"end\":{}}}", span.file.0, span.start, span.end);
        let tokens = self.tokens.iter().map(|token| format!(
            "{{\"span\":{},\"origin\":{},\"name\":{}}}", span(token.span), token.origin.0,
            token.name.as_ref().map_or_else(|| "null".into(), |name| format!(
                "{{\"text\":{},\"namespace\":{},\"source\":{}}}",
                json_string(&name.text), json_string(&format!("{:?}", name.namespace)), json_string(&format!("{:?}", name.source))
            ))
        )).collect::<Vec<_>>().join(",");
        let origins = self.origins.iter().map(|origin| format!(
            "{{\"span\":{},\"parent\":{},\"role\":{}}}", span(origin.span),
            origin.parent.map_or_else(|| "null".into(), |parent| parent.0.to_string()),
            json_string(&format!("{:?}", origin.role))
        )).collect::<Vec<_>>().join(",");
        format!("{{\"invocation\":{},\"category\":{},\"text\":{},\"tokens\":[{}],\"origins\":[{}]}}",
            span(self.invocation), json_string(&format!("{:?}", self.category)), json_string(&self.text), tokens, origins)
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
    max_depth: usize,
}

impl Materializer<'_, '_> {
    fn depth(&self, depth: usize, span: Span) -> CompileResult<()> {
        if depth > self.max_depth { return Err(vec![Diagnostic::limit(Phase::Ct, "generated syntax recursion budget exceeded", Some(span))]); }
        Ok(())
    }
    fn parsed_source(&self, module: ModuleId) -> CompileResult<(&SourceUnit, &ParseResult)> {
        self.sources.iter().zip(self.parsed).find(|(source, _)| source.module_id == module)
            .map(|(source, parsed)| (source, parsed.as_ref()))
            .ok_or_else(|| category("CT source module is absent", Span::default()))
    }
    fn owner(&self, module: ModuleId, span: Span, inherited: Option<&CtSymbolId>) -> CompileResult<CtSymbolId> {
        self.environment.index.symbol_at(module, span).or_else(|| inherited.cloned())
            .ok_or_else(|| category("CT expression has no frozen declaration owner", span))
    }
    fn expression_value(&mut self, module: ModuleId, owner: &CtSymbolId, expression: &s::Expr, expected: Option<CtTy>) -> CompileResult<CtValue> {
        if let s::ExprKind::SyntaxIsland { path, tree } = &expression.kind {
            return self.provider(module, path, tree, expression.span, expected.as_ref().ok_or_else(|| category("provider output needs a Syntax receiving category", expression.span))?);
        }
        let input = self.environment.empty_input(expression.span)?;
        self.environment.evaluate(Entry::Expression { module, expression, definition: owner.clone(), expected, parameters: &[] },
            &input, None, &[], expression.span)
    }
    fn provider(&mut self, module: ModuleId, path: &[String], tree: &s::TokenTree, span: Span, expected: &CtTy) -> CompileResult<CtValue> {
        let ([alias] | [alias, _]) = path else { return Err(category("provider path needs alias or alias.entry", span)); };
        let export = path.get(1).map_or("default", String::as_str);
        let (source, parsed) = self.parsed_source(module)?;
        let import_path = parsed.module.items.iter().find_map(|item| match &item.kind {
            s::ItemKind::Import(import) if import.syntax && import.alias.as_deref().or_else(|| import.path.last().map(String::as_str)) == Some(alias) => Some(import.path.clone()),
            _ => None,
        }).ok_or_else(|| category("provider alias has no syntax use binding", span))?;
        let registry = self.registry.ok_or_else(|| category("syntax provider request has no lock", span))?;
        let delimiter = match tree.delimiter { s::Delimiter::Parenthesis => CtDelimiter::Parenthesis, s::Delimiter::Bracket => CtDelimiter::Bracket, s::Delimiter::Brace => CtDelimiter::Brace };
        let entry = provider_entry(registry, &import_path.join("."), export, delimiter, span)?;
        if *expected != CtTy::Syntax(entry.output) { return Err(category("provider output category differs from its receiving slot", span)); }
        let (input_tree, tokens) = input::from_source(source, &parsed.tokens, tree, path, &self.environment.remaining_limits_for_input())?;
        self.environment.evaluate(Entry::Function(entry.symbol.clone()), &tokens, Some(&input_tree), &[CtValue::TokenTree(input_tree.clone())], span)
    }
    fn parsed(&mut self, value: CtValue, expected: CtSyntaxCategory, span: Span) -> CompileResult<GeneratedSyntax> {
        let CtValue::Syntax(tree) = value else { return Err(category("compile-time result is not Syntax", span)); };
        if tree.category != expected { return Err(category("Syntax category differs from the receiving slot", span)); }
        let ParsedSyntax { rendered, parsed, names: _ } = self.environment.parse_tree(&tree, span)?;
        let caller = self.sources.iter().find(|source| source.file_id == span.file)
            .map(|source| source.module_id).ok_or_else(|| category("generated caller module is absent", span))?;
        self.environment.budget.reserve_analysis_work(rendered.tokens.len() as u64)?;
        for pair in rendered.tokens.windows(2) {
            let first = &pair[0];
            if rendered.source.text.get(pair[1].span.start..pair[1].span.end) != Some(".") { continue; }
            let Some(origin) = tree.origins.get(first.origin.0 as usize) else { return Err(category("generated origin is absent", span)); };
            if origin.role != CtOriginRole::QuoteDefinition { continue; }
            let Some(provider_index) = self.sources.iter().position(|source| source.file_id == origin.span.file) else { continue; };
            let alias = &rendered.source.text[first.span.start..first.span.end];
            let source_import = self.parsed[provider_index].module.items.iter().find_map(|item| match &item.kind {
                s::ItemKind::Import(import) if !import.syntax && import.alias.as_deref().or_else(|| import.path.last().map(String::as_str)) == Some(alias) => Some(import),
                _ => None,
            });
            let Some(source_import) = source_import else { continue; };
            let Some(target) = self.paths.get(&source_import.path) else { continue; };
            let conflicting = self.parsed_source(caller)?.1.module.items.iter().any(|item| match &item.kind {
                s::ItemKind::Import(import) => import.alias.as_deref().or_else(|| import.path.last().map(String::as_str)) == Some(alias)
                    && self.paths.get(&import.path) != Some(target),
                _ => false,
            });
            if conflicting { return Err(category("definition-site module alias collides with a caller binding", span)); }
            if !self.required_imports.iter().any(|(module, import)| *module == caller && import.alias == alias && import.module == *target) {
                self.environment.budget.reserve_generated_nodes(1)?;
                self.required_imports.push((caller, CoreImport { alias: alias.to_owned(), module: *target, span }));
            }
        }
        let mut syntax = parsed.syntax.ok_or_else(|| category("generated parser returned no category", span))?;
        super::hygiene::rename(&mut syntax, &rendered, &tree.origins, span)?;
        let limits = self.environment.remaining_limits_for_input();
        let work = zil_expand::remap_generated_spans(&mut syntax, span, &limits)?;
        self.environment.budget.reserve_analysis_work(work as u64)?;
        self.previews.push(Preview { invocation: span, category: expected, text: rendered.source.text,
            tokens: rendered.tokens, origins: tree.origins });
        Ok(syntax)
    }
    fn items(&mut self, module: ModuleId, items: &mut Vec<s::Item>, inherited: Option<&CtSymbolId>, depth: usize, generated: bool) -> CompileResult<()> {
        self.depth(depth, items.first().map_or(Span::default(), |item| item.span))?;
        let mut output = Vec::new();
        for mut item in std::mem::take(items) {
            if generated && matches!(item.kind, s::ItemKind::Import(_)) { return Err(late_import(item.span)); }
            let owner = self.owner(module, item.span, inherited)?;
            if self.environment.ct_only.contains(&owner)
                && matches!(item.kind, s::ItemKind::Function(_) | s::ItemKind::Extern { .. }) {
                continue;
            }
            match &mut item.kind {
                s::ItemKind::Meta(s::MetaItem::Emit(expression)) => {
                    let value = self.expression_value(module, &owner, expression, Some(CtTy::Syntax(CtSyntaxCategory::ItemList)))?;
                    let GeneratedSyntax::ItemList(mut generated_items) = self.parsed(value, CtSyntaxCategory::ItemList, item.span)? else {
                        return Err(category("#emit must yield ItemList in item position", item.span));
                    };
                    self.items(module, &mut generated_items, Some(&owner), depth + 1, true)?;
                    output.extend(generated_items);
                }
                s::ItemKind::Meta(s::MetaItem::If { then_items, else_items, .. }) => {
                    self.items(module, then_items, Some(&owner), depth + 1, generated)?;
                    self.items(module, else_items, Some(&owner), depth + 1, generated)?;
                    output.push(item);
                }
                s::ItemKind::Meta(s::MetaItem::For { items, .. }) => {
                    self.items(module, items, Some(&owner), depth + 1, generated)?;
                    output.push(item);
                }
                _ => { self.item(module, &mut item, &owner, depth + 1)?; output.push(item); }
            }
        }
        *items = output;
        Ok(())
    }
    fn item(&mut self, module: ModuleId, item: &mut s::Item, owner: &CtSymbolId, depth: usize) -> CompileResult<()> {
        self.depth(depth, item.span)?;
        match &mut item.kind {
            s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } => {
                for param in &mut f.params { self.ty(module, &mut param.ty, owner, depth + 1)?; }
                if let Some(ty) = &mut f.return_type { self.ty(module, ty, owner, depth + 1)?; }
                match &mut f.body { s::FunctionBody::Block(block) => self.block(module, block, owner, depth + 1)?,
                    s::FunctionBody::Expression(e) => self.expr(module, e, owner, depth + 1)?, s::FunctionBody::Declaration => {} }
            }
            s::ItemKind::Aggregate(a) => {
                for field in &mut a.fields { self.ty(module, &mut field.ty, owner, depth + 1)?;
                    if let Some(e) = &mut field.default { self.expr(module, e, owner, depth + 1)?; } }
                self.items(module, &mut a.methods, Some(owner), depth + 1, false)?;
            }
            s::ItemKind::Enum(e) => for variant in &mut e.variants { match &mut variant.fields {
                s::VariantFields::Unit => {}, s::VariantFields::Tuple(fields) => for field in fields { self.ty(module, &mut field.ty, owner, depth + 1)?; },
                s::VariantFields::Record(fields) => for field in fields { self.ty(module, &mut field.ty, owner, depth + 1)?;
                    if let Some(e) = &mut field.default { self.expr(module, e, owner, depth + 1)?; } },
            } },
            s::ItemKind::TypeAlias(alias) => if let Some(ty) = &mut alias.value { self.ty(module, ty, owner, depth + 1)?; },
            s::ItemKind::Const(c) => if let Some(e) = &mut c.value { self.expr(module, e, owner, depth + 1)?; },
            s::ItemKind::Trait(t) => self.items(module, &mut t.members, Some(owner), depth + 1, false)?,
            s::ItemKind::Impl(i) => self.items(module, &mut i.members, Some(owner), depth + 1, false)?,
            s::ItemKind::Import(_) | s::ItemKind::Meta(_) => {},
        }
        Ok(())
    }
    fn block(&mut self, module: ModuleId, block: &mut s::Block, owner: &CtSymbolId, depth: usize) -> CompileResult<()> {
        self.statements(module, &mut block.statements, owner, depth + 1)
    }
    fn statements(&mut self, module: ModuleId, statements: &mut Vec<s::Stmt>, owner: &CtSymbolId, depth: usize) -> CompileResult<()> {
        self.depth(depth, statements.first().map_or(Span::default(), |stmt| stmt.span))?;
        let mut output = Vec::new();
        for mut stmt in std::mem::take(statements) {
            match &mut stmt.kind {
                s::StmtKind::Meta(s::MetaStatement::Emit(expression)) => {
                    let value = self.expression_value(module, owner, expression, Some(CtTy::Syntax(CtSyntaxCategory::StmtList)))?;
                    let GeneratedSyntax::StmtList(mut generated) = self.parsed(value, CtSyntaxCategory::StmtList, stmt.span)? else {
                        return Err(category("#emit must yield StmtList in statement position", stmt.span));
                    };
                    self.statements(module, &mut generated, owner, depth + 1)?;
                    output.extend(generated);
                }
                s::StmtKind::Meta(s::MetaStatement::If { then_body, else_body, .. }) => {
                    self.block(module, then_body, owner, depth + 1)?;
                    if let Some(body) = else_body { self.block(module, body, owner, depth + 1)?; }
                    output.push(stmt);
                }
                s::StmtKind::Meta(s::MetaStatement::For { body, .. }) => { self.block(module, body, owner, depth + 1)?; output.push(stmt); }
                _ => { self.statement(module, &mut stmt, owner, depth + 1)?; output.push(stmt); }
            }
        }
        *statements = output;
        Ok(())
    }
    fn statement(&mut self, module: ModuleId, stmt: &mut s::Stmt, owner: &CtSymbolId, depth: usize) -> CompileResult<()> {
        self.depth(depth, stmt.span)?;
        match &mut stmt.kind {
            s::StmtKind::Binding { ty, value, .. } => { if let Some(ty) = ty { self.ty(module, ty, owner, depth + 1)?; } self.expr(module, value, owner, depth + 1)?; },
            s::StmtKind::Return(Some(e)) | s::StmtKind::Yield(e) | s::StmtKind::Expression(e) => self.expr(module, e, owner, depth + 1)?,
            s::StmtKind::Return(None) | s::StmtKind::Break | s::StmtKind::Continue => {},
            s::StmtKind::Defer(block) | s::StmtKind::Loop(block) | s::StmtKind::Unsafe(block) => self.block(module, block, owner, depth + 1)?,
            s::StmtKind::If { condition, then_body, else_body } => {
                self.condition(module, condition, owner, depth + 1)?; self.block(module, then_body, owner, depth + 1)?;
                if let Some(branch) = else_body { match branch { s::ElseBranch::Block(block) => self.block(module, block, owner, depth + 1)?,
                    s::ElseBranch::If(stmt) => self.statement(module, stmt, owner, depth + 1)? } }
            }
            s::StmtKind::While { condition, body } => { self.condition(module, condition, owner, depth + 1)?; self.block(module, body, owner, depth + 1)?; }
            s::StmtKind::For { iterable, body, .. } => { self.expr(module, iterable, owner, depth + 1)?; self.block(module, body, owner, depth + 1)?; }
            s::StmtKind::Match { value, arms } => { self.expr(module, value, owner, depth + 1)?;
                for arm in arms { self.arrow(module, &mut arm.body, owner, depth + 1)?; } }
            s::StmtKind::With { value, body, .. } => { self.expr(module, value, owner, depth + 1)?; self.block(module, body, owner, depth + 1)?; }
            s::StmtKind::Assign { target, value, .. } => { self.expr(module, target, owner, depth + 1)?; self.expr(module, value, owner, depth + 1)?; }
            s::StmtKind::Meta(_) => {},
        }
        Ok(())
    }
    fn condition(&mut self, module: ModuleId, condition: &mut s::Condition, owner: &CtSymbolId, depth: usize) -> CompileResult<()> {
        match condition { s::Condition::Expression(e) | s::Condition::Let { value: e, .. } => self.expr(module, e, owner, depth) }
    }
    fn arrow(&mut self, module: ModuleId, arrow: &mut s::ArrowBody, owner: &CtSymbolId, depth: usize) -> CompileResult<()> {
        match arrow { s::ArrowBody::Expression(e) => self.expr(module, e, owner, depth), s::ArrowBody::Block(block) => self.block(module, block, owner, depth) }
    }
    fn ty(&mut self, module: ModuleId, ty: &mut s::TypeRef, owner: &CtSymbolId, depth: usize) -> CompileResult<()> {
        self.depth(depth, ty.span)?;
        match &mut ty.kind {
            s::TypeKind::MetaSplice(expression) => {
                let value = self.expression_value(module, owner, expression, Some(CtTy::Syntax(CtSyntaxCategory::Type)))?;
                let GeneratedSyntax::Type(replacement) = self.parsed(value, CtSyntaxCategory::Type, ty.span)? else { return Err(category("type #splice needs Syntax<Type>", ty.span)); };
                *ty = replacement;
            }
            s::TypeKind::MetaEval(expression) => {
                let value = self.expression_value(module, owner, expression, Some(CtTy::Type))?;
                let CtValue::Type(value) = value else { return Err(category("type #(...) needs Type value", ty.span)); };
                let syntax = self.environment.type_syntax(value)?;
                let GeneratedSyntax::Type(replacement) = self.parsed(CtValue::Syntax(syntax), CtSyntaxCategory::Type, ty.span)? else { return Err(category("Type value did not reify to Type syntax", ty.span)); };
                *ty = replacement;
            }
            s::TypeKind::Named { args, .. } | s::TypeKind::Dyn { args, .. } => for arg in args { match arg { s::GenericArg::Type(ty) => self.ty(module, ty, owner, depth + 1)?, s::GenericArg::Const(e) => self.expr(module, e, owner, depth + 1)? } },
            s::TypeKind::Reference { inner, .. } | s::TypeKind::Group(inner) | s::TypeKind::Optional(inner) | s::TypeKind::ForLifetime { inner, .. } => self.ty(module, inner, owner, depth + 1)?,
            s::TypeKind::Tuple(types) | s::TypeKind::Union(types) | s::TypeKind::Impl(types) => for ty in types { self.ty(module, ty, owner, depth + 1)?; },
            s::TypeKind::Array { element, length } => { self.ty(module, element, owner, depth + 1)?; if let Some(e) = length { self.expr(module, e, owner, depth + 1)?; } },
            s::TypeKind::Function { params, result } | s::TypeKind::ForeignFunction { params, result, .. } => { for ty in params { self.ty(module, ty, owner, depth + 1)?; } self.ty(module, result, owner, depth + 1)?; },
            s::TypeKind::Error { error, result } => { self.ty(module, error, owner, depth + 1)?; self.ty(module, result, owner, depth + 1)?; },
            s::TypeKind::MetaBlock(_) => return Err(runtime("compile-time Type block must be materialized explicitly", ty.span)),
        }
        Ok(())
    }
    fn expr(&mut self, module: ModuleId, expression: &mut s::Expr, owner: &CtSymbolId, depth: usize) -> CompileResult<()> {
        self.depth(depth, expression.span)?;
        match &mut expression.kind {
            s::ExprKind::MetaSplice(inner) => {
                let value = self.expression_value(module, owner, inner, Some(CtTy::Syntax(CtSyntaxCategory::Expr)))?;
                let GeneratedSyntax::Expr(replacement) = self.parsed(value, CtSyntaxCategory::Expr, expression.span)? else { return Err(category("expression #splice needs Syntax<Expr>", expression.span)); };
                *expression = replacement;
            }
            s::ExprKind::SyntaxIsland { path, tree } => {
                let value = self.provider(module, path, tree, expression.span, &CtTy::Syntax(CtSyntaxCategory::Expr))?;
                let GeneratedSyntax::Expr(replacement) = self.parsed(value, CtSyntaxCategory::Expr, expression.span)? else { return Err(category("expression provider needs Syntax<Expr>", expression.span)); };
                *expression = replacement;
            }
            s::ExprKind::Quote { .. } => return Err(runtime("Syntax value cannot enter a runtime expression", expression.span)),
            s::ExprKind::MetaEval(inner) => {
                let value = self.expression_value(module, owner, inner, None)?;
                let CtValue::Scalar(scalar) = value else { return Err(runtime("Type, Syntax, Ident and CT views cannot enter runtime expressions", expression.span)); };
                expression.kind = match scalar {
                    zil_ir::Constant::Bool(value) => s::ExprKind::Literal(s::Literal::Bool(value)),
                    zil_ir::Constant::U8(value) => s::ExprKind::Literal(s::Literal::Integer(value.to_string())),
                    zil_ir::Constant::U16(value) => s::ExprKind::Literal(s::Literal::Integer(value.to_string())),
                    zil_ir::Constant::I32(value) => s::ExprKind::Literal(s::Literal::Integer(value.to_string())),
                    zil_ir::Constant::I64(value) => s::ExprKind::Literal(s::Literal::Integer(value.to_string())),
                    zil_ir::Constant::Unit => s::ExprKind::Tuple(Vec::new()),
                    _ => return Err(runtime("CT result has no ordinary scalar literal", expression.span)),
                };
            }
            s::ExprKind::MetaBlock(_) => {},
            s::ExprKind::Group(e) | s::ExprKind::Try(e) | s::ExprKind::Unary { operand: e, .. } => self.expr(module, e, owner, depth + 1)?,
            s::ExprKind::Tuple(values) | s::ExprKind::Array(values) => for e in values { self.expr(module, e, owner, depth + 1)?; },
            s::ExprKind::Binary { left, right, .. } | s::ExprKind::Pipeline { input: left, target: right } => { self.expr(module, left, owner, depth + 1)?; self.expr(module, right, owner, depth + 1)?; },
            s::ExprKind::Member { object, .. } => self.expr(module, object, owner, depth + 1)?,
            s::ExprKind::TypeApply { callee, arguments } => { self.expr(module, callee, owner, depth + 1)?; for arg in arguments { match arg { s::GenericArg::Type(ty) => self.ty(module, ty, owner, depth + 1)?, s::GenericArg::Const(e) => self.expr(module, e, owner, depth + 1)?, } } },
            s::ExprKind::Call { callee, arguments } => { self.expr(module, callee, owner, depth + 1)?; for arg in arguments { self.expr(module, &mut arg.value, owner, depth + 1)?; } },
            s::ExprKind::Index { object, index } => { self.expr(module, object, owner, depth + 1)?; self.expr(module, index, owner, depth + 1)?; },
            s::ExprKind::Slice { object, start, end } => { self.expr(module, object, owner, depth + 1)?; for e in [start, end].into_iter().flatten() { self.expr(module, e, owner, depth + 1)?; } },
            s::ExprKind::Record { ty, base, fields } => { if let Some(ty) = ty { self.expr(module, ty, owner, depth + 1)?; } if let Some(base) = base { self.expr(module, base, owner, depth + 1)?; }
                for field in fields { if let Some(e) = &mut field.value { self.expr(module, e, owner, depth + 1)?; } } },
            s::ExprKind::Closure { params, return_type, body, .. } => { for param in params { if let Some(ty) = &mut param.ty { self.ty(module, ty, owner, depth + 1)?; } }
                if let Some(ty) = return_type { self.ty(module, ty, owner, depth + 1)?; } self.arrow(module, body, owner, depth + 1)?; },
            s::ExprKind::If { condition, then_value, else_value } => { self.expr(module, condition, owner, depth + 1)?; self.expr(module, then_value, owner, depth + 1)?; self.expr(module, else_value, owner, depth + 1)?; },
            s::ExprKind::Literal(_) | s::ExprKind::Name(_) | s::ExprKind::ReceiverMember(_) => {},
        }
        Ok(())
    }
}

pub(crate) fn materialize(
    sources: &[SourceUnit], parsed: &[Arc<ParseResult>], modules: &[&s::Module], paths: &BTreeMap<Vec<String>, ModuleId>,
    imports: &[Vec<CoreImport>], root: ModuleId, fingerprint: Fingerprint,
    target: &TargetProfile, limits: &Limits, registry: Option<&CtLockedProviderRegistry>,
) -> CompileResult<Materialized> {
    let mut environment = Environment::new(sources, parsed, paths, imports, root, fingerprint, target, limits)?;
    environment.classify_definitions()?;
    let mut output = modules.iter().map(|module| (*module).clone()).collect::<Vec<_>>();
    let mut materializer = Materializer { environment: &mut environment, sources, parsed, registry, paths,
        previews: Vec::new(), required_imports: Vec::new(), max_depth: limits.max_call_depth };
    for module in &mut output { materializer.items(module.id, &mut module.items, None, 0, false)?; }
    Ok(Materialized { modules: output, previews: materializer.previews, required_imports: materializer.required_imports,
        usage: environment.budget.usage().clone() })
}
