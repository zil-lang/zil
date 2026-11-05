//! Static routing only: this grants no execution or source authority.
use super::*;
struct Selection<'c, 'a> {
    compiler: &'c mut Compiler<'a>,
    visited: Vec<usize>,
}
type Locals = Vec<(String, bool)>;
impl Selection<'_, '_> {
    fn local(&mut self, locals: &Locals, name: &str, span: Span) -> R<Option<bool>> {
        for (actual, domain) in locals.iter().rev() {
            self.compiler
                .budget
                .work(name.len().saturating_add(1), span)?;
            if actual == name {
                return Ok(Some(*domain));
            }
        }
        Ok(None)
    }
    fn bind(&mut self, locals: &mut Locals, name: &str, domain: bool, span: Span) -> R<()> {
        self.compiler.budget.text(name, span)?;
        self.compiler.budget.node(span)?;
        locals.push((name.to_owned(), domain));
        Ok(())
    }
    fn pattern(
        &mut self,
        locals: &mut Locals,
        pattern: &s::Pattern,
        domain: bool,
        depth: usize,
    ) -> R<()> {
        self.compiler.budget.depth(depth, pattern.span)?;
        match &pattern.kind {
            s::PatternKind::Name(name) => self.bind(locals, name, domain, pattern.span)?,
            s::PatternKind::Reference { pattern, .. }
            | s::PatternKind::ErrorArm { pattern, .. } => {
                self.pattern(locals, pattern, domain, depth + 1)?
            }
            s::PatternKind::RecordConstructor { fields, .. } => {
                for field in fields {
                    self.compiler.budget.node(field.span)?;
                    self.compiler.budget.text(&field.name, field.span)?;
                    self.pattern(locals, &field.pattern, domain, depth + 1)?;
                }
            }
            s::PatternKind::Tuple(fields)
            | s::PatternKind::Constructor {
                arguments: fields, ..
            } => {
                for field in fields {
                    self.pattern(locals, field, domain, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn path(&mut self, expr: &s::Expr, depth: usize) -> R<Option<Vec<String>>> {
        self.compiler.budget.depth(depth, expr.span)?;
        match &expr.kind {
            s::ExprKind::Name(name) => {
                self.compiler.budget.text(name, expr.span)?;
                self.compiler.budget.node(expr.span)?;
                Ok(Some(vec![name.clone()]))
            }
            s::ExprKind::Group(inner) | s::ExprKind::TypeApply { callee: inner, .. } => {
                self.path(inner, depth + 1)
            }
            s::ExprKind::Member {
                object,
                member: s::MemberName::Name(name),
            } => {
                let Some(mut path) = self.path(object, depth + 1)? else {
                    return Ok(None);
                };
                self.compiler.budget.text(name, expr.span)?;
                self.compiler.budget.node(expr.span)?;
                path.push(name.clone());
                Ok(Some(path))
            }
            _ => Ok(None),
        }
    }
    fn target(&mut self, module: usize, path: &[String], span: Span) -> R<Option<(usize, usize)>> {
        if let Some(index) = self.compiler.member_function(module, path, span)? {
            return Ok(Some((self.compiler.declarations[index].module, index)));
        }
        self.compiler.budget.work(
            self.compiler
                .member_hosts
                .len()
                .saturating_mul(self.compiler.declarations.len()),
            span,
        )?;
        let (owner, name) = if path.len() == 1 {
            (module, &path[0])
        } else if path.len() == 2 {
            let mut owner = None;
            for import in self.compiler.modules[module].imports {
                self.compiler
                    .budget
                    .work(path[0].len().saturating_add(1), span)?;
                if import.alias == path[0] {
                    if let ImportTarget::Module(id) = import.target {
                        owner = Some(self.compiler.module_index(id, span)?);
                    }
                    break;
                }
            }
            let Some(owner) = owner else {
                return Ok(None);
            };
            (owner, &path[1])
        } else {
            return Ok(None);
        };
        for (index, declaration) in self.compiler.declarations.iter().enumerate() {
            self.compiler
                .budget
                .work(name.len().saturating_add(1), span)?;
            if declaration.module == owner
                && declaration.function.name == *name
                && !self
                    .compiler
                    .member_hosts
                    .iter()
                    .any(|member| member.declaration == index)
            {
                // This is only routing; compile/ordinary Expand enforce visibility.
                return Ok(Some((owner, index)));
            }
        }
        Ok(None)
    }
    fn standard(&mut self, module: usize, path: &[String], span: Span) -> R<bool> {
        if path.len() != 2 {
            return Ok(false);
        }
        for import in self.compiler.modules[module].imports {
            self.compiler
                .budget
                .work(path[0].len().saturating_add(1), span)?;
            if import.alias == path[0] {
                return Ok(matches!(
                    import.target,
                    ImportTarget::Meta | ImportTarget::Syntax
                ));
            }
        }
        Ok(false)
    }
    fn nominal(&mut self, module: usize, path: &[String], span: Span) -> R<bool> {
        let (owner, name) = if path.len() == 1 {
            (module, &path[0])
        } else if path.len() == 2 {
            let mut owner = None;
            for import in self.compiler.modules[module].imports {
                self.compiler
                    .budget
                    .work(path[0].len().saturating_add(1), span)?;
                if import.alias == path[0] {
                    if let ImportTarget::Module(id) = import.target {
                        owner = Some(self.compiler.module_index(id, span)?);
                    }
                    break;
                }
            }
            let Some(owner) = owner else {
                return Ok(false);
            };
            (owner, &path[1])
        } else {
            return Ok(false);
        };
        for item in &self.compiler.modules[owner].syntax.items {
            self.compiler
                .budget
                .work(name.len().saturating_add(1), span)?;
            let found = match &item.kind {
                s::ItemKind::Aggregate(a) => a.name == *name,
                s::ItemKind::Enum(e) => e.name == *name,
                s::ItemKind::TypeAlias(a) => a.name == *name,
                _ => false,
            };
            if found {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn function(&mut self, index: usize, depth: usize) -> R<bool> {
        let declaration = self.compiler.declarations[index];
        self.compiler.budget.depth(depth, declaration.item.span)?;
        self.compiler
            .budget
            .work(self.visited.len(), declaration.item.span)?;
        if self.visited.contains(&index) {
            return Ok(false);
        }
        self.compiler.budget.node(declaration.item.span)?;
        self.visited.push(index);
        if self
            .compiler
            .ct_only_function(declaration.module, declaration.function)?
        {
            return Ok(true);
        }
        let mut locals = Vec::new();
        for parameter in &declaration.function.params {
            self.bind(&mut locals, &parameter.name, false, parameter.span)?;
        }
        match &declaration.function.body {
            s::FunctionBody::Block(body) => {
                self.block(declaration.module, body, &mut locals, depth + 1)
            }
            s::FunctionBody::Expression(expression) => {
                self.expr(declaration.module, expression, &mut locals, depth + 1)
            }
            s::FunctionBody::Declaration => Ok(false),
        }
    }
    fn arrow(
        &mut self,
        module: usize,
        body: &s::ArrowBody,
        locals: &mut Locals,
        depth: usize,
    ) -> R<bool> {
        match body {
            s::ArrowBody::Expression(e) => self.expr(module, e, locals, depth),
            s::ArrowBody::Block(b) => self.block(module, b, locals, depth),
        }
    }
    fn condition(
        &mut self,
        module: usize,
        condition: &s::Condition,
        locals: &mut Locals,
        depth: usize,
    ) -> R<bool> {
        match condition {
            s::Condition::Expression(e) => self.expr(module, e, locals, depth),
            s::Condition::Let { pattern, value } => {
                let domain = self.expr(module, value, locals, depth)?;
                self.pattern(locals, pattern, domain, depth + 1)?;
                Ok(domain)
            }
        }
    }
    fn block(
        &mut self,
        module: usize,
        block: &s::Block,
        locals: &mut Locals,
        depth: usize,
    ) -> R<bool> {
        self.compiler.budget.depth(depth, block.span)?;
        let start = locals.len();
        let result = (|| {
            for statement in &block.statements {
                if self.statement(module, statement, locals, depth + 1)? {
                    return Ok(true);
                }
            }
            Ok(false)
        })();
        locals.truncate(start);
        result
    }
    fn statement(
        &mut self,
        module: usize,
        statement: &s::Stmt,
        locals: &mut Locals,
        depth: usize,
    ) -> R<bool> {
        self.compiler.budget.depth(depth, statement.span)?;
        Ok(match &statement.kind {
            s::StmtKind::Binding {
                pattern, ty, value, ..
            } => {
                let domain = if let Some(ty) = ty {
                    self.compiler
                        .ct_only_type(module, ty, depth + 1, &mut Vec::new())?
                } else {
                    false
                } || self.expr(module, value, locals, depth + 1)?;
                self.pattern(locals, pattern, domain, depth + 1)?;
                domain
            }
            s::StmtKind::Expression(e) | s::StmtKind::Yield(e) | s::StmtKind::Return(Some(e)) => {
                self.expr(module, e, locals, depth + 1)?
            }
            s::StmtKind::Assign { target, value, .. } => {
                self.expr(module, target, locals, depth + 1)?
                    || self.expr(module, value, locals, depth + 1)?
            }
            s::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let start = locals.len();
                let mut found = self.condition(module, condition, locals, depth + 1)?
                    || self.block(module, then_body, locals, depth + 1)?;
                locals.truncate(start);
                if !found && let Some(branch) = else_body {
                    found = match branch {
                        s::ElseBranch::Block(b) => self.block(module, b, locals, depth + 1)?,
                        s::ElseBranch::If(s) => self.statement(module, s, locals, depth + 1)?,
                    };
                }
                found
            }
            s::StmtKind::While { condition, body } => {
                let start = locals.len();
                let result = self.condition(module, condition, locals, depth + 1)?
                    || self.block(module, body, locals, depth + 1)?;
                locals.truncate(start);
                result
            }
            s::StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                let domain = self.expr(module, iterable, locals, depth + 1)?;
                let start = locals.len();
                self.pattern(locals, pattern, domain, depth + 1)?;
                let result = domain || self.block(module, body, locals, depth + 1)?;
                locals.truncate(start);
                result
            }
            s::StmtKind::Loop(b) | s::StmtKind::Unsafe(b) | s::StmtKind::Defer(b) => {
                self.block(module, b, locals, depth + 1)?
            }
            s::StmtKind::With { name, value, body } => {
                let domain = self.expr(module, value, locals, depth + 1)?;
                let start = locals.len();
                self.bind(locals, name, domain, statement.span)?;
                let result = domain || self.block(module, body, locals, depth + 1)?;
                locals.truncate(start);
                result
            }
            s::StmtKind::Match { value, arms } => {
                let mut found = self.expr(module, value, locals, depth + 1)?;
                for arm in arms {
                    if found {
                        break;
                    }
                    let start = locals.len();
                    self.pattern(locals, &arm.pattern, false, depth + 1)?;
                    found = self.arrow(module, &arm.body, locals, depth + 1)?;
                    locals.truncate(start);
                }
                found
            }
            s::StmtKind::Meta(_) => false, // Selection executes before this routing query.
            s::StmtKind::Return(None) | s::StmtKind::Break | s::StmtKind::Continue => false,
        })
    }
    fn expr(
        &mut self,
        module: usize,
        expression: &s::Expr,
        locals: &mut Locals,
        depth: usize,
    ) -> R<bool> {
        self.compiler.budget.depth(depth, expression.span)?;
        Ok(match &expression.kind {
            s::ExprKind::Ascribed { ty, value } => {
                let domain = self
                    .compiler
                    .ct_only_type(module, ty, depth + 1, &mut Vec::new())?;
                domain | self.expr(module, value, locals, depth + 1)?
            }
            s::ExprKind::Quote { .. }
            | s::ExprKind::MetaSplice(_)
            | s::ExprKind::SyntaxIsland { .. } => true,
            s::ExprKind::Literal(s::Literal::String(_)) => true,
            s::ExprKind::Name(name) => match self.local(locals, name, expression.span)? {
                Some(domain) => domain,
                None => {
                    if types::scalar(name).is_some()
                        || matches!(
                            name.as_str(),
                            "ByteBuffer" | "ForeignHandle" | "CWideString"
                        )
                    {
                        true
                    } else {
                        let mut found = false;
                        for item in &self.compiler.modules[module].syntax.items {
                            self.compiler.budget.work(1, expression.span)?;
                            found |= match &item.kind {
                                s::ItemKind::Aggregate(a) => a.name == *name,
                                s::ItemKind::Enum(e) => e.name == *name,
                                s::ItemKind::TypeAlias(a) => a.name == *name,
                                _ => false,
                            };
                        }
                        found
                    }
                }
            },
            s::ExprKind::Group(e)
            | s::ExprKind::MetaEval(e)
            | s::ExprKind::Try(e)
            | s::ExprKind::Unary { operand: e, .. } => self.expr(module, e, locals, depth + 1)?,
            s::ExprKind::Tuple(fields) | s::ExprKind::Array(fields) => {
                let mut found = false;
                for field in fields {
                    if self.expr(module, field, locals, depth + 1)? {
                        found = true;
                        break;
                    }
                }
                found
            }
            s::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(module, condition, locals, depth + 1)?
                    || self.expr(module, then_value, locals, depth + 1)?
                    || self.expr(module, else_value, locals, depth + 1)?
            }
            s::ExprKind::Binary { left, right, .. }
            | s::ExprKind::Pipeline {
                input: left,
                target: right,
            } => {
                self.expr(module, left, locals, depth + 1)?
                    || self.expr(module, right, locals, depth + 1)?
            }
            s::ExprKind::Call { callee, arguments } => {
                let mut found = false;
                for argument in arguments {
                    if self.expr(module, &argument.value, locals, depth + 1)? {
                        found = true;
                        break;
                    }
                }
                if !found {
                    if let Some(path) = self.path(callee, depth + 1)? {
                        if let Some(domain) = self.local(locals, &path[0], callee.span)? {
                            found = domain;
                        } else {
                            found = self.standard(module, &path, callee.span)?;
                            if !found
                                && let Some((_, index)) = self.target(module, &path, callee.span)?
                            {
                                found = self.function(index, depth + 1)?;
                            }
                        }
                    } else {
                        found = self.expr(module, callee, locals, depth + 1)?;
                    }
                }
                found
            }
            s::ExprKind::Member { object, .. } => {
                if let Some(path) = self.path(expression, depth + 1)? {
                    if self.local(locals, &path[0], expression.span)?.is_some() {
                        self.expr(module, object, locals, depth + 1)?
                    } else {
                        self.standard(module, &path, expression.span)?
                            || self.nominal(module, &path, expression.span)?
                    }
                } else {
                    self.expr(module, object, locals, depth + 1)?
                }
            }
            s::ExprKind::TypeApply { callee, arguments } => {
                let mut found = self.expr(module, callee, locals, depth + 1)?;
                for argument in arguments {
                    if found {
                        break;
                    }
                    found = match argument {
                        s::GenericArg::Lifetime(_) => false,
                        s::GenericArg::Type(ty) => {
                            self.compiler
                                .ct_only_type(module, ty, depth + 1, &mut Vec::new())?
                        }
                        s::GenericArg::Const(e) => self.expr(module, e, locals, depth + 1)?,
                    };
                }
                found
            }
            s::ExprKind::Index { object, index } => {
                self.expr(module, object, locals, depth + 1)?
                    || self.expr(module, index, locals, depth + 1)?
            }
            s::ExprKind::Slice { object, start, end } => {
                let mut found = self.expr(module, object, locals, depth + 1)?;
                for endpoint in [start, end].into_iter().flatten() {
                    if !found {
                        found = self.expr(module, endpoint, locals, depth + 1)?;
                    }
                }
                found
            }
            s::ExprKind::Record { base, fields, .. } => {
                let mut found = if let Some(base) = base {
                    self.expr(module, base, locals, depth + 1)?
                } else {
                    false
                };
                for field in fields {
                    if found {
                        break;
                    }
                    self.compiler.budget.work(1, field.span)?;
                    if let Some(value) = &field.value {
                        found = self.expr(module, value, locals, depth + 1)?;
                    } else {
                        found = self
                            .local(locals, &field.name, field.span)?
                            .unwrap_or(false);
                    }
                }
                found
            }
            s::ExprKind::MetaBlock(b) => self.block(module, b, locals, depth + 1)?,
            s::ExprKind::Closure { .. }
            | s::ExprKind::Literal(_)
            | s::ExprKind::ReceiverMember(_) => false,
        })
    }
}

fn parameter_domain(budget: &mut Budget<'_>, ty: &CtTy, depth: usize, span: Span) -> R<bool> {
    budget.depth(depth, span)?;
    Ok(match ty {
        CtTy::Scalar(_) | CtTy::Record(_) | CtTy::Enum(_) => false,
        CtTy::Tuple(fields) => {
            let mut found = false;
            for field in fields {
                if parameter_domain(budget, field, depth + 1, span)? {
                    found = true;
                    break;
                }
            }
            found
        }
        CtTy::Array { element, .. } => parameter_domain(budget, element, depth + 1, span)?,
        _ => true,
    })
}
pub(super) fn uses(
    modules: &[SourceModule<'_>],
    module: ModuleId,
    expression: &s::Expr,
    parameters: &[(String, CtTy)],
    limits: &mut Limits,
) -> R<bool> {
    let mut compiler = Compiler::unindexed(modules, limits);
    let result = (|| {
        compiler.index()?;
        let owner = compiler.module_index(module, expression.span)?;
        let mut selection = Selection {
            compiler: &mut compiler,
            visited: Vec::new(),
        };
        let mut locals = Vec::new();
        for (name, ty) in parameters {
            let domain = parameter_domain(&mut selection.compiler.budget, ty, 0, expression.span)?;
            selection.bind(&mut locals, name, domain, expression.span)?;
        }
        selection.expr(owner, expression, &mut locals, 0)
    })();
    let remaining = compiler.budget.declared();
    drop(compiler);
    limits.max_analysis_steps = remaining.max_analysis_work as usize;
    limits.max_nodes = remaining.max_generated_nodes as usize;
    limits.max_source_bytes = remaining.max_temporary_bytes as usize;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, NodeId};
    fn route(texts: &[&str], entry: &str, limits: &mut Limits) -> R<bool> {
        let sources = texts
            .iter()
            .enumerate()
            .map(|(m, text)| {
                SourceUnit::new(
                    FileId(m as u32),
                    ModuleId(m as u32),
                    format!("{m}.zil"),
                    *text,
                )
            })
            .collect::<Vec<_>>();
        let syntax = sources
            .iter()
            .map(|source| zil_syntax::parse(source).into_result().unwrap())
            .collect::<Vec<_>>();
        let symbols = syntax
            .iter()
            .enumerate()
            .map(|(m, module)| {
                module
                    .items
                    .iter()
                    .enumerate()
                    .map(|(n, item)| {
                        (
                            item.span,
                            CtSymbolId {
                                module: ModuleId(m as u32),
                                declaration: NodeId(n as u32),
                                generation_path: None,
                            },
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let imports = syntax
            .iter()
            .map(|module| {
                module
                    .items
                    .iter()
                    .filter_map(|item| {
                        let s::ItemKind::Import(import) = &item.kind else {
                            return None;
                        };
                        let target = match import
                            .path
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .as_slice()
                        {
                            ["std", "meta"] => ImportTarget::Meta,
                            ["std", "syntax"] => ImportTarget::Syntax,
                            ["model"] => ImportTarget::Module(ModuleId(1)),
                            _ => panic!("fixture import identity"),
                        };
                        Some(SourceImport {
                            alias: import
                                .alias
                                .clone()
                                .unwrap_or_else(|| import.path.last().unwrap().clone()),
                            target,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let modules = (0..sources.len())
            .map(|m| SourceModule {
                source: &sources[m],
                syntax: &syntax[m],
                imports: &imports[m],
                symbols: &symbols[m],
            })
            .collect::<Vec<_>>();
        let function = syntax[0]
            .items
            .iter()
            .find_map(|item| match &item.kind {
                s::ItemKind::Function(f) if f.name == entry => Some(f),
                _ => None,
            })
            .unwrap();
        let s::FunctionBody::Expression(expression) = &function.body else {
            panic!("fixture expression entry")
        };
        let parameters = function
            .params
            .iter()
            .map(|p| (p.name.clone(), CtTy::Scalar(CtScalarTy::I32)))
            .collect::<Vec<_>>();
        super::super::uses_ct_domain(&modules, ModuleId(0), expression, &parameters, limits)
    }
    #[test]
    fn actual_domains_close_calls_without_compiling_old_only_nominals() {
        for source in [
            "fn sum(n:i64)->i64{var v:i64=0;for i in 0..n{v+=i}return v}fn entry()=>sum(10)",
            "struct Item<T>{value:T}type Alias<T>=Item<T>;fn make()->Alias<i64>{return Alias::<i64>{value:3}}fn entry()=>make()",
            "enum Token{Empty,Byte(u8)}fn make()->Token{var rows=[Token.Empty(),Token.Empty()];rows[1]=Token.Byte(3u8);return rows[1]}fn entry()=>make()",
            "resource R{n:i32}package P{manages r:R}enum E{Fail}fn make()->E!P=>P{r:R{n:1}};fn entry()=>make()",
            "import std.meta as meta;fn helper(meta:i32)->i32{return meta.layout(1)}fn entry()=>helper(1)",
            "import std.syntax as syntax;fn helper()->i32{let syntax=0;return syntax.literal(1)}fn entry()=>helper()",
            "fn unused()->Type{return i32}struct R{x:i32}fn make()->R{return R{x:1}}fn entry()=>make()",
        ] {
            assert!(
                !route(&[source], "entry", &mut Limits::default()).unwrap(),
                "{source}"
            );
        }
        let recursive = "import std.meta as meta;fn a(n:i32)->i64{if n>0{return b(n-1)}return meta.layout(i32).size}fn b(n:i32)->i64{return a(n)}fn entry()=>a(2);fn second()=>b(2)";
        for entry in ["entry", "second"] {
            assert!(route(&[recursive], entry, &mut Limits::default()).unwrap());
        }
        for expression in ["model.Point", "model.Alias", "model.kind()"] {
            assert!(route(&[&format!("import model;fn entry()=>{expression}"),"pub struct Point{x:i32}pub type Alias=Point;pub fn kind()->Type{return Point}"],"entry",&mut Limits::default()).unwrap());
        }
        assert!(
            !route(
                &[
                    "import model;fn entry(model:i32)=>model.Point",
                    "pub struct Point{x:i32}"
                ],
                "entry",
                &mut Limits::default()
            )
            .unwrap()
        );
    }
    #[test]
    fn routing_work_is_bounded_and_failed_indexing_keeps_spent_work() {
        let sources = [
            "import std.meta as meta;fn helper()->i64{return meta.layout(i32).size}fn entry()=>helper()",
        ];
        let mut low = 0;
        let mut high = Limits::default().max_analysis_steps;
        while low < high {
            let n = low + (high - low) / 2;
            let mut limits = Limits {
                max_analysis_steps: n,
                ..Limits::default()
            };
            match route(&sources, "entry", &mut limits) {
                Ok(true) => high = n,
                Ok(false) => panic!("lost domain"),
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
                    );
                    low = n + 1;
                }
            }
        }
        let mut limits = Limits {
            max_analysis_steps: low,
            ..Limits::default()
        };
        assert!(route(&sources, "entry", &mut limits).unwrap());
        let left = limits.max_analysis_steps;
        let mut fresh = Limits {
            max_analysis_steps: low,
            ..Limits::default()
        };
        assert!(route(&sources, "entry", &mut fresh).unwrap());
        assert_eq!(fresh.max_analysis_steps, left);
        let mut short = Limits {
            max_analysis_steps: low - 1,
            ..Limits::default()
        };
        let errors = route(&sources, "entry", &mut short).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
        );
        let mut index_short = Limits {
            max_analysis_steps: 2,
            ..Limits::default()
        };
        assert!(route(&sources, "entry", &mut index_short).is_err());
        assert!(index_short.max_analysis_steps < 2);
        println!("static CT routing N={low}, N-1 ResourceLimit, fresh remaining={left}");
    }
}
