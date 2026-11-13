//! Lexical alpha-renaming for generated local binders. Provenance identifies
//! each splice instance; input and definition-site names keep their own scope.
use super::render::RenderedSyntax;
use std::collections::BTreeMap;
use zil_base::{CompileResult, Diagnostic, Phase, Span};
use zil_ir::ct_artifact::{CtIdent, CtNameNamespace, CtNameSource, CtOrigin, CtOriginId};
use zil_syntax::{GeneratedSyntax, ast as s};

fn error(span: Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Ct,
        "E-CT-HYGIENE",
        Some(span),
        message,
    )]
}

pub(super) trait Context {
    fn owner_type(&mut self, ident: &CtIdent, span: Span) -> CompileResult<s::TypeRef>;
    fn prepay_record_shorthand(&mut self, work: u64, bytes: u64) -> CompileResult<()>;
}

struct Renamer<'a> {
    rendered: &'a RenderedSyntax,
    origins: &'a [CtOrigin],
    invocation: Span,
    next: u64,
    scopes: Vec<BTreeMap<(u32, String), String>>,
    context: &'a mut dyn Context,
}
impl Renamer<'_> {
    fn region(&self, mut id: CtOriginId) -> u32 {
        let mut region = 0;
        while let Some(origin) = self.origins.get(id.0 as usize) {
            let Some(parent) = origin.parent else {
                break;
            };
            region = parent.0.saturating_add(1);
            id = parent;
        }
        region
    }
    fn token(&self, span: Span, text: &str) -> Option<&super::render::RenderedToken> {
        self.rendered.tokens.iter().find(|token| {
            token.span.start >= span.start
                && token.span.end <= span.end
                && self
                    .rendered
                    .source
                    .text
                    .get(token.span.start..token.span.end)
                    == Some(text)
        })
    }
    fn bind(&mut self, text: &mut String, span: Span) -> CompileResult<()> {
        if text == "self" {
            return Ok(());
        }
        let Some(token) = self.token(span, text) else {
            return Err(error(span, "generated binding has no exact token origin"));
        };
        if token.name.is_some() {
            return Ok(());
        }
        let region = self.region(token.origin);
        let original = text.clone();
        let replacement = format!(
            "__zil_ct_{}_{}_{}_{}",
            self.invocation.file.0, self.invocation.start, self.next, original
        );
        self.next = self
            .next
            .checked_add(1)
            .ok_or_else(|| error(span, "generated local identity overflow"))?;
        self.scopes
            .last_mut()
            .expect("generated lexical scope")
            .insert((region, original), replacement.clone());
        *text = replacement;
        Ok(())
    }
    fn reference(&mut self, text: &mut String, span: Span) -> CompileResult<()> {
        let Some(token) = self.token(span, text) else {
            return Ok(());
        };
        if let Some(name) = &token.name {
            if matches!(name.source, CtNameSource::Export) {
                return Err(vec![Diagnostic::error(
                    Phase::Ct,
                    "E-CT-EXPORT-CAPTURE",
                    Some(span),
                    "export name intent cannot reference a caller local",
                )]);
            }
            return Ok(());
        }
        let region = self.region(token.origin);
        for scope in self.scopes.iter().rev() {
            if let Some(replacement) = scope.get(&(region, text.clone())) {
                *text = replacement.clone();
                break;
            }
        }
        Ok(())
    }
    fn pattern(&mut self, pattern: &mut s::Pattern) -> CompileResult<()> {
        match &mut pattern.kind {
            s::PatternKind::Name(name) => self.bind(name, pattern.span),
            s::PatternKind::Reference { pattern, .. }
            | s::PatternKind::ErrorArm { pattern, .. } => self.pattern(pattern),
            s::PatternKind::RecordConstructor { fields, .. } => {
                // Only value binders are renamed. Member labels retain their
                // source spelling for the later nominal field check.
                for field in fields {
                    self.pattern(&mut field.pattern)?;
                }
                Ok(())
            }
            s::PatternKind::Tuple(parts) => {
                for part in parts {
                    self.pattern(part)?;
                }
                Ok(())
            }
            s::PatternKind::Constructor { arguments, .. } => {
                for part in arguments {
                    self.pattern(part)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    fn block(&mut self, block: &mut s::Block) -> CompileResult<()> {
        self.scopes.push(BTreeMap::new());
        let result = self.statements(&mut block.statements);
        self.scopes.pop();
        result
    }
    fn branch(&mut self, condition: &mut s::Condition, body: &mut s::Block) -> CompileResult<()> {
        match condition {
            s::Condition::Expression(expr) => {
                self.expr(expr)?;
                self.block(body)
            }
            s::Condition::Let { pattern, value } => {
                self.expr(value)?;
                self.scopes.push(BTreeMap::new());
                self.pattern(pattern)?;
                let result = self.block(body);
                self.scopes.pop();
                result
            }
        }
    }
    fn statements(&mut self, statements: &mut [s::Stmt]) -> CompileResult<()> {
        for statement in statements {
            self.statement(statement)?;
        }
        Ok(())
    }
    fn statement(&mut self, statement: &mut s::Stmt) -> CompileResult<()> {
        match &mut statement.kind {
            s::StmtKind::Binding {
                pattern, ty, value, ..
            } => {
                if let Some(ty) = ty {
                    self.ty(ty)?;
                }
                self.expr(value)?;
                self.pattern(pattern)
            }
            s::StmtKind::Return(Some(value))
            | s::StmtKind::Yield(value)
            | s::StmtKind::Expression(value) => self.expr(value),
            s::StmtKind::Return(None) | s::StmtKind::Break | s::StmtKind::Continue => Ok(()),
            s::StmtKind::Defer(body) | s::StmtKind::Loop(body) | s::StmtKind::Unsafe(body) => {
                self.block(body)
            }
            s::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.branch(condition, then_body)?;
                if let Some(other) = else_body {
                    match other {
                        s::ElseBranch::Block(body) => self.block(body)?,
                        s::ElseBranch::If(statement) => self.statement(statement)?,
                    }
                }
                Ok(())
            }
            s::StmtKind::While { condition, body } => self.branch(condition, body),
            s::StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                self.expr(iterable)?;
                self.scopes.push(BTreeMap::new());
                self.pattern(pattern)?;
                let result = self.block(body);
                self.scopes.pop();
                result
            }
            s::StmtKind::Match { value, arms } => {
                self.expr(value)?;
                for arm in arms {
                    self.scopes.push(BTreeMap::new());
                    self.pattern(&mut arm.pattern)?;
                    self.arrow(&mut arm.body)?;
                    self.scopes.pop();
                }
                Ok(())
            }
            s::StmtKind::With { name, value, body } => {
                self.expr(value)?;
                self.scopes.push(BTreeMap::new());
                self.bind(name, statement.span)?;
                let result = self.block(body);
                self.scopes.pop();
                result
            }
            s::StmtKind::Assign { target, value, .. } => {
                self.expr(target)?;
                self.expr(value)
            }
            s::StmtKind::Meta(_) => Err(error(
                statement.span,
                "generated meta statement escaped materialization",
            )),
        }
    }
    fn arrow(&mut self, body: &mut s::ArrowBody) -> CompileResult<()> {
        match body {
            s::ArrowBody::Expression(expr) => self.expr(expr),
            s::ArrowBody::Block(block) => self.block(block),
        }
    }
    fn ty(&mut self, ty: &mut s::TypeRef) -> CompileResult<()> {
        match &mut ty.kind {
            s::TypeKind::Named { args, .. } | s::TypeKind::Dyn { args, .. } => {
                for arg in args {
                    match arg {
                        s::GenericArg::AssociatedBinding(binding) => {
                            return Err(vec![zil_base::Diagnostic::error(
                                zil_base::Phase::Ct,
                                "E-SOURCE-ASSOCIATED-BINDING",
                                Some(binding.span),
                                "associated equations require the original trait application obligation consumer",
                            )]);
                        }
                        s::GenericArg::Lifetime(_) => {}
                        s::GenericArg::Type(ty) => self.ty(ty)?,
                        s::GenericArg::Const(expr) => self.expr(expr)?,
                    }
                }
            }
            s::TypeKind::Reference { inner, .. }
            | s::TypeKind::Group(inner)
            | s::TypeKind::Optional(inner)
            | s::TypeKind::ForLifetime { inner, .. } => self.ty(inner)?,
            s::TypeKind::Tuple(types) | s::TypeKind::Union(types) | s::TypeKind::Impl(types) => {
                for ty in types {
                    self.ty(ty)?;
                }
            }
            s::TypeKind::Array { element, length } => {
                self.ty(element)?;
                if let Some(length) = length {
                    self.expr(length)?;
                }
            }
            s::TypeKind::Function { params, result }
            | s::TypeKind::ForeignFunction { params, result, .. } => {
                for ty in params {
                    self.ty(ty)?;
                }
                self.ty(result)?;
            }
            s::TypeKind::Error { error, result } => {
                self.ty(error)?;
                self.ty(result)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn expr(&mut self, expr: &mut s::Expr) -> CompileResult<()> {
        match &mut expr.kind {
            s::ExprKind::Name(name) => self.reference(name, expr.span),
            s::ExprKind::Ascribed { ty, value } => {
                self.ty(ty)?;
                self.expr(value)
            }
            s::ExprKind::Group(value)
            | s::ExprKind::Try(value)
            | s::ExprKind::Unary { operand: value, .. } => self.expr(value),
            s::ExprKind::Tuple(values) | s::ExprKind::Array(values) => {
                for value in values {
                    self.expr(value)?;
                }
                Ok(())
            }
            s::ExprKind::Binary { left, right, .. }
            | s::ExprKind::Pipeline {
                input: left,
                target: right,
            } => {
                self.expr(left)?;
                self.expr(right)
            }
            s::ExprKind::Member { object, member } => {
                self.expr(object)?;
                if let s::MemberName::Name(name) = member {
                    let ident = self
                        .rendered
                        .tokens
                        .iter()
                        .rev()
                        .find_map(|token| {
                            (token.span.start >= expr.span.start
                                && token.span.end <= expr.span.end
                                && self
                                    .rendered
                                    .source
                                    .text
                                    .get(token.span.start..token.span.end)
                                    == Some(name.as_str()))
                            .then_some(token.name.as_ref())
                            .flatten()
                            .filter(|ident| {
                                ident.namespace == CtNameNamespace::Member
                                    && ident.member_owner_type.is_some()
                            })
                        })
                        .cloned();
                    if let Some(ident) = ident {
                        let owner = self.context.owner_type(&ident, expr.span)?;
                        *member = s::MemberName::Constrained {
                            name: name.clone(),
                            owner: Box::new(owner),
                        };
                    }
                }
                Ok(())
            }
            s::ExprKind::TypeApply { callee, arguments } => {
                self.expr(callee)?;
                for arg in arguments {
                    match arg {
                        s::GenericArg::AssociatedBinding(binding) => {
                            return Err(vec![zil_base::Diagnostic::error(
                                zil_base::Phase::Ct,
                                "E-SOURCE-ASSOCIATED-BINDING",
                                Some(binding.span),
                                "associated equations require the original trait application obligation consumer",
                            )]);
                        }
                        s::GenericArg::Lifetime(_) => {}
                        s::GenericArg::Type(ty) => self.ty(ty)?,
                        s::GenericArg::Const(expr) => self.expr(expr)?,
                    }
                }
                Ok(())
            }
            s::ExprKind::Call { callee, arguments } => {
                self.expr(callee)?;
                for argument in arguments {
                    self.expr(&mut argument.value)?;
                }
                Ok(())
            }
            s::ExprKind::Index { object, index } => {
                self.expr(object)?;
                self.expr(index)
            }
            s::ExprKind::Slice { object, start, end } => {
                self.expr(object)?;
                for value in [start, end].into_iter().flatten() {
                    self.expr(value)?;
                }
                Ok(())
            }
            s::ExprKind::Record { ty, base, fields } => {
                if let Some(ty) = ty {
                    self.expr(ty)?;
                }
                if let Some(base) = base {
                    self.expr(base)?;
                }
                for field in fields {
                    if let Some(value) = &mut field.value {
                        self.expr(value)?;
                    } else {
                        // A pun contains a member label and a separate value
                        // reference. Rename only the latter, before later
                        // passes can resolve an accidental caller binding.
                        let width = (field.name.len() as u64).saturating_add(1);
                        let visits = (self.rendered.tokens.len() as u64)
                            .saturating_add(self.origins.len() as u64)
                            .saturating_add(1);
                        let work = visits
                            .saturating_mul(width)
                            .saturating_mul(self.scopes.len() as u64 + 1)
                            .saturating_mul(16);
                        let bytes = width.saturating_add(80).saturating_mul(3);
                        self.context.prepay_record_shorthand(work, bytes)?;
                        let mut name = field.name.clone();
                        self.reference(&mut name, field.span)?;
                        field.value = Some(s::Expr {
                            semantic: Default::default(),
                            kind: s::ExprKind::Name(name),
                            span: field.span,
                        });
                    }
                }
                Ok(())
            }
            s::ExprKind::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                self.scopes.push(BTreeMap::new());
                for param in params {
                    if let Some(ty) = &mut param.ty {
                        self.ty(ty)?;
                    }
                    self.bind(&mut param.name, param.span)?;
                }
                if let Some(ty) = return_type {
                    self.ty(ty)?;
                }
                let result = self.arrow(body);
                self.scopes.pop();
                result
            }
            s::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition)?;
                self.expr(then_value)?;
                self.expr(else_value)
            }
            _ => Ok(()),
        }
    }
    fn item(&mut self, item: &mut s::Item) -> CompileResult<()> {
        match &mut item.kind {
            s::ItemKind::Function(function) | s::ItemKind::Extern { function, .. } => {
                self.scopes.push(BTreeMap::new());
                for param in &mut function.params {
                    self.ty(&mut param.ty)?;
                    self.bind(&mut param.name, param.span)?;
                }
                if let Some(ty) = &mut function.return_type {
                    self.ty(ty)?;
                }
                let result = match &mut function.body {
                    s::FunctionBody::Block(block) => self.block(block),
                    s::FunctionBody::Expression(expr) => self.expr(expr),
                    s::FunctionBody::Declaration => Ok(()),
                };
                self.scopes.pop();
                result
            }
            s::ItemKind::Aggregate(aggregate) => {
                for field in &mut aggregate.fields {
                    self.ty(&mut field.ty)?;
                }
                for method in &mut aggregate.methods {
                    self.item(method)?;
                }
                Ok(())
            }
            s::ItemKind::Enum(enumeration) => {
                for variant in &mut enumeration.variants {
                    match &mut variant.fields {
                        s::VariantFields::Unit => {}
                        s::VariantFields::Tuple(fields) => {
                            for field in fields {
                                self.ty(&mut field.ty)?;
                            }
                        }
                        s::VariantFields::Record(fields) => {
                            for field in fields {
                                self.ty(&mut field.ty)?;
                            }
                        }
                    }
                }
                Ok(())
            }
            s::ItemKind::TypeAlias(alias) => {
                if let Some(ty) = &mut alias.value {
                    self.ty(ty)?;
                }
                Ok(())
            }
            s::ItemKind::Const(constant) => {
                if let Some(ty) = &mut constant.ty {
                    self.ty(ty)?;
                }
                if let Some(value) = &mut constant.value {
                    self.expr(value)?;
                }
                Ok(())
            }
            s::ItemKind::Trait(declaration) => {
                for member in &mut declaration.members {
                    self.item(member)?;
                }
                Ok(())
            }
            s::ItemKind::Impl(declaration) => {
                for member in &mut declaration.members {
                    self.item(member)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

pub(super) fn rename(
    syntax: &mut GeneratedSyntax,
    rendered: &RenderedSyntax,
    origins: &[CtOrigin],
    invocation: Span,
    context: &mut dyn Context,
) -> CompileResult<()> {
    let mut renamer = Renamer {
        rendered,
        origins,
        invocation,
        next: 0,
        scopes: vec![BTreeMap::new()],
        context,
    };
    match syntax {
        GeneratedSyntax::Expr(expr) => renamer.expr(expr),
        GeneratedSyntax::Type(ty) => renamer.ty(ty),
        GeneratedSyntax::StmtList(statements) => renamer.statements(statements),
        GeneratedSyntax::ItemList(items) => {
            for item in items {
                renamer.item(item)?;
            }
            Ok(())
        }
    }
}
