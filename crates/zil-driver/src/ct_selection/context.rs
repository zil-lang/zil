use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Availability {
    Ready,
    Generated,
    BodyGeneration,
    Profile,
    Foreign,
    Shape,
    Constant,
}
struct Declaration<'a> {
    module: ModuleId,
    name: &'a str,
    item: &'a s::Item,
    availability: Availability,
    conditional: bool,
}
pub(super) struct Context<'a> {
    declarations: Vec<Declaration<'a>>,
    pub program: CoreProgram,
    pub cost: SyntaxCost,
}
fn scalar(t: &s::TypeRef) -> bool {
    match &t.kind {
        s::TypeKind::Named { path, args } => {
            args.is_empty()
                && path.len() == 1
                && matches!(
                    path[0].as_str(),
                    "void" | "bool" | "u8" | "u16" | "i32" | "i64"
                )
        }
        s::TypeKind::Group(t) => scalar(t),
        s::TypeKind::Tuple(ts) => ts.is_empty(),
        _ => false,
    }
}
impl<'a> Context<'a> {
    pub fn freeze(
        sources: &[SourceUnit],
        modules: &[&'a s::Module],
        imports: &[Vec<CoreImport>],
        root: ModuleId,
        fingerprint: Fingerprint,
        budget: &mut Budget,
    ) -> CompileResult<Self> {
        let mut declarations = Vec::new();
        for module in modules {
            index(
                module.id,
                &module.items,
                false,
                0,
                &mut declarations,
                budget,
            )?;
        }
        let mut expanded = Vec::new();
        for ((source, module), imports) in sources.iter().zip(modules).zip(imports) {
            budget.node(module.span, 0)?;
            let mut items = Vec::new();
            for d in &declarations {
                budget.node(d.item.span, 0)?;
                if d.module != module.id || d.availability != Availability::Ready {
                    continue;
                }
                let cost = syntax::item_cost(d.item, &budget.remaining)?;
                budget.copy(cost, d.item.span)?;
                items.push(d.item.clone());
            }
            for import in imports {
                budget.node(module.span, 0)?;
                budget.text(&import.alias, module.span)?;
            }
            let syntax = s::Module {
                id: module.id,
                items,
                span: module.span,
            };
            let (core, map) = zil_expand::expand_program_module_with_limits(
                source,
                &syntax,
                imports.clone(),
                &budget.remaining,
            )?;
            budget.spend(map.entries.len(), map.entries.len(), module.span)?;
            expanded.push(core);
        }
        let program = CoreProgram {
            root,
            source_fingerprint: fingerprint,
            modules: expanded,
        };
        let (nodes, work) = super::super::ct_program::selection_cost(&program, &budget.remaining)?;
        let cost = SyntaxCost {
            nodes,
            work,
            generation: false,
        };
        if let Some(module) = modules.first() {
            budget.copy(cost, module.span)?;
        }
        Ok(Self {
            declarations,
            program,
            cost,
        })
    }
    fn lookup(
        &self,
        module: ModuleId,
        name: &str,
        span: Span,
        budget: &mut Budget,
    ) -> CompileResult<Option<usize>> {
        let mut generated = None;
        let mut ordinary = None;
        for (index, d) in self.declarations.iter().enumerate() {
            budget.spend(0, 1, span)?;
            if d.module != module {
                continue;
            }
            budget.text(name, span)?;
            if d.name != name {
                continue;
            }
            if d.conditional {
                generated = Some(index);
            } else if ordinary.replace(index).is_some() {
                // Ordinary name/type checking reports an ambiguous declaration.
                return Ok(None);
            }
        }
        Ok(ordinary.or(generated))
    }
    pub fn dependencies(
        &self,
        module: ModuleId,
        expression: &s::Expr,
        runtime: &[Vec<String>],
        budget: &mut Budget,
    ) -> CompileResult<Vec<Dependency>> {
        let mut visitor = Dependencies {
            context: self,
            budget,
            module,
            locals: Vec::new(),
            queue: Vec::new(),
            seen: BTreeSet::new(),
            runtime,
            runtime_active: true,
        };
        visitor.expression(expression, 0)?;
        let mut next = 0;
        while next < visitor.queue.len() {
            let index = visitor.queue[next];
            next += 1;
            let d = &self.declarations[index];
            let s::ItemKind::Function(f) = &d.item.kind else {
                unreachable!()
            };
            visitor.module = d.module;
            visitor.runtime_active = false;
            visitor.budget.node(f.span, 0)?;
            visitor.locals.push(Vec::new());
            for p in &f.params {
                visitor.bind_name(&p.name, p.span)?;
            }
            match &f.body {
                s::FunctionBody::Block(b) => visitor.block(b, 0)?,
                s::FunctionBody::Expression(e) => visitor.expression(e, 0)?,
                s::FunctionBody::Declaration => {}
            }
            visitor.locals.pop();
        }
        let mut result = Vec::new();
        for index in visitor.queue {
            let d = &self.declarations[index];
            visitor.budget.node(d.item.span, 0)?;
            visitor.budget.text(d.name, d.item.span)?;
            let source = self
                .program
                .modules
                .iter()
                .find(|m| m.core.module_id == d.module)
                .expect("snapshot module")
                .core
                .source_fingerprint;
            result.push(Dependency {
                module: d.module,
                function: d.name.into(),
                declaration: d.item.span,
                source,
            });
        }
        Ok(result)
    }
}
fn index<'a>(
    module: ModuleId,
    items: &'a [s::Item],
    conditional: bool,
    depth: usize,
    output: &mut Vec<Declaration<'a>>,
    budget: &mut Budget,
) -> CompileResult<()> {
    for item in items {
        budget.node(item.span, depth)?;
        let (name, availability) = match &item.kind {
            s::ItemKind::Meta(s::MetaItem::If {
                then_items,
                else_items,
                ..
            }) => {
                index(module, then_items, true, depth + 1, output, budget)?;
                index(module, else_items, true, depth + 1, output, budget)?;
                continue;
            }
            s::ItemKind::Meta(s::MetaItem::For { items, .. }) => {
                index(module, items, true, depth + 1, output, budget)?;
                continue;
            }
            s::ItemKind::Function(f) if conditional => (f.name.as_str(), Availability::Generated),
            s::ItemKind::Function(f) => {
                let cost = syntax::item_cost(item, &budget.remaining)?;
                budget.copy(cost, item.span)?;
                let state = if cost.generation {
                    Availability::BodyGeneration
                } else if !f.generics.is_empty()
                    || !f.where_clause.is_empty()
                    || !f.contract.is_empty()
                    || !f.params.iter().all(|p| !p.give && scalar(&p.ty))
                    || f.return_type.as_ref().is_some_and(|t| !scalar(t))
                    || matches!(f.body, s::FunctionBody::Declaration)
                {
                    Availability::Profile
                } else {
                    Availability::Ready
                };
                (f.name.as_str(), state)
            }
            s::ItemKind::Extern { function, .. } => (function.name.as_str(), Availability::Foreign),
            s::ItemKind::Aggregate(a) => (a.name.as_str(), Availability::Shape),
            s::ItemKind::Enum(e) => (e.name.as_str(), Availability::Shape),
            s::ItemKind::TypeAlias(a) => (a.name.as_str(), Availability::Shape),
            s::ItemKind::Trait(t) => (t.name.as_str(), Availability::Shape),
            s::ItemKind::Const(c) => (c.name.as_str(), Availability::Constant),
            _ => continue,
        };
        budget.node(item.span, depth)?;
        budget.text(name, item.span)?;
        output.push(Declaration {
            module,
            name,
            item,
            availability: if conditional {
                Availability::Generated
            } else {
                availability
            },
            conditional,
        });
    }
    Ok(())
}

struct Dependencies<'a, 'b, 'c> {
    context: &'a Context<'b>,
    budget: &'c mut Budget,
    module: ModuleId,
    locals: Vec<Vec<String>>,
    queue: Vec<usize>,
    seen: BTreeSet<usize>,
    runtime: &'a [Vec<String>],
    runtime_active: bool,
}
impl Dependencies<'_, '_, '_> {
    fn runtime_name(&mut self, name: &str, span: Span) -> CompileResult<bool> {
        if self.runtime_active {
            for scope in self.runtime.iter().rev() {
                for local in scope.iter().rev() {
                    self.budget.text(name, span)?;
                    if local == name {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
    fn local(&mut self, name: &str, span: Span) -> CompileResult<bool> {
        for scope in self.locals.iter().rev() {
            for local in scope.iter().rev() {
                self.budget.text(name, span)?;
                if local == name {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    fn bind_name(&mut self, name: &str, span: Span) -> CompileResult<()> {
        self.budget.node(span, 0)?;
        self.budget.text(name, span)?;
        self.locals
            .last_mut()
            .expect("lexical scope")
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
    fn resolve(&mut self, e: &s::Expr) -> CompileResult<Option<(ModuleId, String)>> {
        self.budget.node(e.span, 0)?;
        if let s::ExprKind::Name(name) = &e.kind
            && self.runtime_name(name, e.span)?
        {
            return Err(forbidden(
                e.span,
                "#if condition cannot read a receiving runtime parameter or local",
            ));
        }
        match &e.kind {
            s::ExprKind::Group(e) => self.resolve(e),
            s::ExprKind::Name(name) if !self.local(name, e.span)? => {
                self.budget.text(name, e.span)?;
                Ok(Some((self.module, name.clone())))
            }
            s::ExprKind::Member {
                object,
                member: s::MemberName::Name(name),
            } => {
                let s::ExprKind::Name(alias) = &object.kind else {
                    return Ok(None);
                };
                if self.runtime_name(alias, object.span)? {
                    return Err(forbidden(
                        object.span,
                        "#if condition cannot read a receiving runtime parameter or local",
                    ));
                }
                if self.local(alias, object.span)? {
                    return Ok(None);
                }
                let mut found = None;
                for module in &self.context.program.modules {
                    self.budget.spend(0, 1, e.span)?;
                    if module.core.module_id != self.module {
                        continue;
                    }
                    for import in &module.imports {
                        self.budget.text(alias, object.span)?;
                        if import.alias == *alias {
                            if found.replace(import.module).is_some() {
                                return Ok(None);
                            }
                        }
                    }
                }
                self.budget.text(name, e.span)?;
                Ok(found.map(|module| (module, name.clone())))
            }
            _ => Ok(None),
        }
    }
    fn name(&mut self, e: &s::Expr, call: bool) -> CompileResult<()> {
        let Some((module, name)) = self.resolve(e)? else {
            return Ok(());
        };
        let Some(index) = self.context.lookup(module, &name, e.span, self.budget)? else {
            return Ok(());
        };
        let d = &self.context.declarations[index];
        // Visibility remains an ordinary module typing obligation. Never use an
        // invisible candidate as a ConfigReady substitute for the actual name.
        if module != self.module && !d.item.public {
            return Ok(());
        }
        match d.availability {
            Availability::Generated => {
                return Err(phase_error(
                    e.span,
                    "#if condition depends on a declaration emitted by source generation; ConfigReady is closed",
                ));
            }
            Availability::BodyGeneration => {
                return Err(phase_error(
                    e.span,
                    "#if condition depends on a function body with unmaterialized generation",
                ));
            }
            Availability::Shape => {
                return Err(phase_error(
                    e.span,
                    "#if condition cannot query a declaration shape/layout or mutable trait set before its phase is ready",
                ));
            }
            Availability::Constant => {
                return Err(forbidden(
                    e.span,
                    "ordinary CT global declarations are outside the closed scalar selection profile",
                ));
            }
            Availability::Profile => {
                return Err(forbidden(
                    e.span,
                    "selection calls require an unconditional non-generic ordinary scalar function",
                ));
            }
            Availability::Foreign => {
                return Err(forbidden(
                    e.span,
                    "foreign functions cannot execute in compile-time selection",
                ));
            }
            Availability::Ready => {}
        }
        if call {
            self.budget.node(e.span, 0)?;
            if self.seen.insert(index) {
                if self.queue.len() >= self.budget.remaining.max_functions {
                    return Err(vec![Diagnostic::limit(
                        Phase::Ct,
                        "selection dependency function budget exceeded",
                        Some(e.span),
                    )]);
                }
                self.budget.node(e.span, 0)?;
                self.queue.push(index);
            }
        }
        Ok(())
    }
    fn pattern_body(
        &mut self,
        pattern: &s::Pattern,
        body: &s::Block,
        depth: usize,
    ) -> CompileResult<()> {
        self.budget.node(body.span, depth)?;
        self.locals.push(Vec::new());
        self.bind(pattern, depth + 1)?;
        let result = self.block(body, depth + 1);
        self.locals.pop();
        result
    }
}
impl Visitor for Dependencies<'_, '_, '_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        self.budget.node(span, depth)
    }
    fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
        self.budget.text(text, span)
    }
    fn block(&mut self, block: &s::Block, depth: usize) -> CompileResult<()> {
        self.budget.node(block.span, depth)?;
        self.locals.push(Vec::new());
        let result = syntax::walk_block(self, block, depth);
        self.locals.pop();
        result
    }
    fn statement(&mut self, statement: &s::Stmt, depth: usize) -> CompileResult<()> {
        match &statement.kind {
            s::StmtKind::Binding { pattern, .. } => {
                syntax::walk_statement(self, statement, depth)?;
                self.bind(pattern, depth + 1)
            }
            s::StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                self.node(statement.span, depth)?;
                self.expression(iterable, depth + 1)?;
                self.pattern_body(pattern, body, depth + 1)
            }
            s::StmtKind::While {
                condition: s::Condition::Let { pattern, value },
                body,
            } => {
                self.node(statement.span, depth)?;
                self.expression(value, depth + 1)?;
                self.pattern_body(pattern, body, depth + 1)
            }
            s::StmtKind::If {
                condition: s::Condition::Let { pattern, value },
                then_body,
                else_body,
            } => {
                self.node(statement.span, depth)?;
                self.expression(value, depth + 1)?;
                self.pattern_body(pattern, then_body, depth + 1)?;
                match else_body {
                    Some(s::ElseBranch::Block(b)) => self.block(b, depth + 1),
                    Some(s::ElseBranch::If(s)) => self.statement(s, depth + 1),
                    None => Ok(()),
                }
            }
            s::StmtKind::Match { value, arms } => {
                self.node(statement.span, depth)?;
                self.expression(value, depth + 1)?;
                for arm in arms {
                    self.node(arm.span, depth + 1)?;
                    self.locals.push(Vec::new());
                    self.bind(&arm.pattern, depth + 2)?;
                    syntax::arrow(self, &arm.body, depth + 2)?;
                    self.locals.pop();
                }
                Ok(())
            }
            s::StmtKind::With { name, value, body } => {
                self.node(statement.span, depth)?;
                self.expression(value, depth + 1)?;
                self.locals.push(Vec::new());
                self.bind_name(name, statement.span)?;
                let result = self.block(body, depth + 1);
                self.locals.pop();
                result
            }
            _ => syntax::walk_statement(self, statement, depth),
        }
    }
    fn expression(&mut self, expression: &s::Expr, depth: usize) -> CompileResult<()> {
        match &expression.kind {
            s::ExprKind::Call { callee, .. } => self.name(callee, true)?,
            s::ExprKind::Name(_) | s::ExprKind::Member { .. } => self.name(expression, false)?,
            s::ExprKind::Closure { .. } => {
                return Err(forbidden(
                    expression.span,
                    "closures are runtime-only in scalar selection",
                ));
            }
            s::ExprKind::MetaBlock(_) | s::ExprKind::SyntaxIsland { .. } => {
                return Err(phase_error(
                    expression.span,
                    "selection condition requires an already materialized scalar expression",
                ));
            }
            _ => {}
        }
        syntax::walk_expression(self, expression, depth)
    }
}
