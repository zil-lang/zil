use super::*;

pub(super) struct Materializer<'a, 'b> {
    pub module: ModuleId,
    pub context: &'a Context<'b>,
    pub budget: &'a mut Budget,
    pub target: &'a TargetProfile,
    pub trace: &'a mut Vec<Trace>,
    pub locals: Vec<Vec<String>>,
}
impl Materializer<'_, '_> {
    pub fn items(
        &mut self,
        items: &mut Vec<s::Item>,
        depth: usize,
        generated: bool,
    ) -> CompileResult<()> {
        let mut output = Vec::new();
        for mut item in std::mem::take(items) {
            self.budget.node(item.span, depth)?;
            match &mut item.kind {
                s::ItemKind::Meta(s::MetaItem::If {
                    condition,
                    then_items,
                    else_items,
                }) => {
                    if !item.attributes.is_empty() || item.public {
                        return Err(vec![Diagnostic::unsupported(
                            Phase::Expand,
                            "attributes or public modifier on a selection directive",
                            Some(item.span),
                        )]);
                    }
                    let chosen = self.choose(condition)?;
                    let selected = if chosen { then_items } else { else_items };
                    self.items(selected, depth + 1, true)?;
                    for item in std::mem::take(selected) {
                        self.budget.node(item.span, depth + 1)?;
                        output.push(item);
                    }
                }
                s::ItemKind::Meta(_) => {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Expand,
                        "#for/#emit source generation is not implemented",
                        Some(item.span),
                    )]);
                }
                s::ItemKind::Import(_) if generated => {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Expand,
                        "conditional imports are outside the closed ConfigReady import graph",
                        Some(item.span),
                    )]);
                }
                _ => {
                    self.item(&mut item, depth + 1)?;
                    self.budget.node(item.span, depth + 1)?;
                    output.push(item);
                }
            }
        }
        *items = output;
        Ok(())
    }
    fn item(&mut self, item: &mut s::Item, depth: usize) -> CompileResult<()> {
        self.budget.node(item.span, depth)?;
        match &mut item.kind {
            s::ItemKind::Function(f) | s::ItemKind::Extern { function: f, .. } => {
                self.function(f, depth + 1)?
            }
            s::ItemKind::Aggregate(a) => {
                for field in &mut a.fields {
                    if let Some(value) = &mut field.default {
                        self.expression(value, depth + 1)?;
                    }
                }
                self.items(&mut a.methods, depth + 1, false)?;
            }
            s::ItemKind::Enum(e) => {
                for variant in &mut e.variants {
                    self.budget.node(variant.span, depth + 1)?;
                    if let s::VariantFields::Record(fields) = &mut variant.fields {
                        for field in fields {
                            if let Some(e) = &mut field.default {
                                self.expression(e, depth + 2)?;
                            }
                        }
                    }
                }
            }
            s::ItemKind::Trait(t) => self.items(&mut t.members, depth + 1, false)?,
            s::ItemKind::Impl(i) => self.items(&mut i.members, depth + 1, false)?,
            s::ItemKind::Const(c) => {
                if let Some(e) = &mut c.value {
                    self.expression(e, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn bind_name(&mut self, name: &str, span: Span) -> CompileResult<()> {
        self.budget.node(span, 0)?;
        self.budget.text(name, span)?;
        self.locals
            .last_mut()
            .expect("receiving runtime scope")
            .push(name.into());
        Ok(())
    }
    fn bind(&mut self, pattern: &s::Pattern, depth: usize) -> CompileResult<()> {
        self.budget.node(pattern.span, depth)?;
        match &pattern.kind {
            s::PatternKind::Name(name) => self.bind_name(name, pattern.span)?,
            s::PatternKind::Reference { pattern, .. } => self.bind(pattern, depth + 1)?,
            s::PatternKind::Tuple(ps) | s::PatternKind::Constructor { arguments: ps, .. } => {
                for p in ps {
                    self.bind(p, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn function(&mut self, f: &mut s::Function, depth: usize) -> CompileResult<()> {
        self.budget.node(f.span, depth)?;
        self.locals.push(Vec::new());
        for p in &f.params {
            self.bind_name(&p.name, p.span)?;
        }
        let result = match &mut f.body {
            s::FunctionBody::Block(b) => self.block(b, depth + 1),
            s::FunctionBody::Expression(e) => self.expression(e, depth + 1),
            s::FunctionBody::Declaration => Ok(()),
        };
        self.locals.pop();
        result
    }
    fn block(&mut self, b: &mut s::Block, depth: usize) -> CompileResult<()> {
        self.budget.node(b.span, depth)?;
        self.locals.push(Vec::new());
        let result = self.statements(&mut b.statements, depth);
        self.locals.pop();
        result
    }
    fn statements(&mut self, statements: &mut Vec<s::Stmt>, depth: usize) -> CompileResult<()> {
        let mut output = Vec::new();
        for mut statement in std::mem::take(statements) {
            self.budget.node(statement.span, depth + 1)?;
            match &mut statement.kind {
                s::StmtKind::Meta(s::MetaStatement::If {
                    condition,
                    then_body,
                    else_body,
                }) => {
                    let chosen = self.choose(condition)?;
                    let selected = if chosen {
                        Some(then_body)
                    } else {
                        else_body.as_mut()
                    };
                    if let Some(selected) = selected {
                        self.budget.node(selected.span, depth + 1)?;
                        // No runtime scope is pushed for the directive's braces,
                        // including during later conditions' lexical lookup.
                        self.statements(&mut selected.statements, depth + 1)?;
                        for statement in std::mem::take(&mut selected.statements) {
                            self.budget.node(statement.span, depth + 1)?;
                            output.push(statement);
                        }
                    }
                }
                s::StmtKind::Meta(_) => {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Expand,
                        "#for/#emit source generation is not implemented",
                        Some(statement.span),
                    )]);
                }
                _ => {
                    self.statement(&mut statement, depth + 1)?;
                    self.budget.node(statement.span, depth + 1)?;
                    output.push(statement);
                }
            }
        }
        *statements = output;
        Ok(())
    }
    fn pattern_body(
        &mut self,
        pattern: &s::Pattern,
        body: &mut s::Block,
        depth: usize,
    ) -> CompileResult<()> {
        self.budget.node(body.span, depth)?;
        self.locals.push(Vec::new());
        self.bind(pattern, depth + 1)?;
        let result = self.block(body, depth + 1);
        self.locals.pop();
        result
    }
    fn condition(&mut self, c: &mut s::Condition, depth: usize) -> CompileResult<()> {
        match c {
            s::Condition::Expression(e) | s::Condition::Let { value: e, .. } => {
                self.expression(e, depth)
            }
        }
    }
    fn statement(&mut self, s: &mut s::Stmt, depth: usize) -> CompileResult<()> {
        self.budget.node(s.span, depth)?;
        match &mut s.kind {
            s::StmtKind::Binding { pattern, value, .. } => {
                self.expression(value, depth + 1)?;
                self.bind(pattern, depth + 1)?;
            }
            s::StmtKind::Yield(value) | s::StmtKind::Expression(value) => {
                self.expression(value, depth + 1)?
            }
            s::StmtKind::Return(value) => {
                if let Some(e) = value {
                    self.expression(e, depth + 1)?;
                }
            }
            s::StmtKind::Defer(b) | s::StmtKind::Loop(b) | s::StmtKind::Unsafe(b) => {
                self.block(b, depth + 1)?
            }
            s::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.condition(condition, depth + 1)?;
                if let s::Condition::Let { pattern, .. } = condition {
                    self.pattern_body(pattern, then_body, depth + 1)?;
                } else {
                    self.block(then_body, depth + 1)?;
                }
                match else_body {
                    Some(s::ElseBranch::Block(b)) => self.block(b, depth + 1)?,
                    Some(s::ElseBranch::If(s)) => self.statement(s, depth + 1)?,
                    None => {}
                }
            }
            s::StmtKind::While { condition, body } => {
                self.condition(condition, depth + 1)?;
                if let s::Condition::Let { pattern, .. } = condition {
                    self.pattern_body(pattern, body, depth + 1)?;
                } else {
                    self.block(body, depth + 1)?;
                }
            }
            s::StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                self.expression(iterable, depth + 1)?;
                self.pattern_body(pattern, body, depth + 1)?;
            }
            s::StmtKind::With { name, value, body } => {
                self.expression(value, depth + 1)?;
                self.budget.node(body.span, depth + 1)?;
                self.locals.push(Vec::new());
                self.bind_name(name, s.span)?;
                self.block(body, depth + 1)?;
                self.locals.pop();
            }
            s::StmtKind::Match { value, arms } => {
                self.expression(value, depth + 1)?;
                for arm in arms {
                    self.budget.node(arm.span, depth + 1)?;
                    self.locals.push(Vec::new());
                    self.bind(&arm.pattern, depth + 2)?;
                    self.arrow(&mut arm.body, depth + 2)?;
                    self.locals.pop();
                }
            }
            s::StmtKind::Assign { target, value, .. } => {
                self.expression(target, depth + 1)?;
                self.expression(value, depth + 1)?;
            }
            s::StmtKind::Break | s::StmtKind::Continue => {}
            s::StmtKind::Meta(_) => unreachable!("selection statements are materialized as lists"),
        }
        Ok(())
    }
    fn arrow(&mut self, body: &mut s::ArrowBody, depth: usize) -> CompileResult<()> {
        match body {
            s::ArrowBody::Expression(e) => self.expression(e, depth),
            s::ArrowBody::Block(b) => self.block(b, depth),
        }
    }
    fn expression(&mut self, e: &mut s::Expr, depth: usize) -> CompileResult<()> {
        self.budget.node(e.span, depth)?;
        match &mut e.kind {
            s::ExprKind::Group(e)
            | s::ExprKind::Try(e)
            | s::ExprKind::MetaEval(e)
            | s::ExprKind::Unary { operand: e, .. } => self.expression(e, depth + 1)?,
            s::ExprKind::Tuple(es) | s::ExprKind::Array(es) => {
                for e in es {
                    self.expression(e, depth + 1)?;
                }
            }
            s::ExprKind::Binary { left, right, .. }
            | s::ExprKind::Pipeline {
                input: left,
                target: right,
            } => {
                self.expression(left, depth + 1)?;
                self.expression(right, depth + 1)?;
            }
            s::ExprKind::Member { object, .. } => self.expression(object, depth + 1)?,
            s::ExprKind::TypeApply { callee, arguments } => {
                self.expression(callee, depth + 1)?;
                for a in arguments {
                    if let s::GenericArg::Const(e) = a {
                        self.expression(e, depth + 1)?;
                    }
                }
            }
            s::ExprKind::Call { callee, arguments } => {
                self.expression(callee, depth + 1)?;
                for a in arguments {
                    self.budget.node(a.span, depth + 1)?;
                    self.expression(&mut a.value, depth + 2)?;
                }
            }
            s::ExprKind::Index { object, index } => {
                self.expression(object, depth + 1)?;
                self.expression(index, depth + 1)?;
            }
            s::ExprKind::Slice { object, start, end } => {
                self.expression(object, depth + 1)?;
                for e in [start, end].into_iter().flatten() {
                    self.expression(e, depth + 1)?;
                }
            }
            s::ExprKind::Record { ty, base, fields } => {
                for e in [ty, base].into_iter().flatten() {
                    self.expression(e, depth + 1)?;
                }
                for f in fields {
                    self.budget.node(f.span, depth + 1)?;
                    if let Some(e) = &mut f.value {
                        self.expression(e, depth + 2)?;
                    }
                }
            }
            s::ExprKind::Closure { params, body, .. } => {
                self.locals.push(Vec::new());
                for p in params {
                    self.bind_name(&p.name, p.span)?;
                }
                self.arrow(body, depth + 1)?;
                self.locals.pop();
            }
            s::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expression(condition, depth + 1)?;
                self.expression(then_value, depth + 1)?;
                self.expression(else_value, depth + 1)?;
            }
            // Type/Syntax-producing CT blocks keep their existing unsupported
            // boundary. Selection does not execute generators inside them.
            s::ExprKind::MetaBlock(_)
            | s::ExprKind::SyntaxIsland { .. }
            | s::ExprKind::Literal(_)
            | s::ExprKind::Name(_)
            | s::ExprKind::ReceiverMember(_) => {}
        }
        Ok(())
    }
}
