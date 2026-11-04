//! Source class constraints before each qualified type gets its own local class0.
//! These private facts describe lexical type meaning, never loans or authority.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Binder(usize, usize); // actual declaration in this walk, original full ordinal
#[derive(Clone)]
enum Fact {
    Unknown,
    Class(Binder),
    Parameter(Binder),
    Named(Vec<String>, Vec<Fact>),
    Address(Option<Box<Fact>>, Box<Fact>),
    Tuple(Vec<Fact>),
    List(Box<Fact>),
    Function(Vec<Fact>, Box<Fact>),
}
type Facts = BTreeMap<String, Fact>;
type Substitutions = BTreeMap<Binder, Fact>;
struct Scope<'a> {
    owner: usize,
    parameters: &'a [GenericParam],
}
struct Check<'a, 'b, 'l> {
    functions: Vec<&'a Function>,
    records: Vec<&'a Aggregate>,
    budget: &'b mut Budget<'l>,
}
impl Fact {
    fn has_class(&self) -> bool {
        match self {
            Self::Class(_) => true,
            Self::Named(_, fs) | Self::Tuple(fs) => fs.iter().any(Self::has_class),
            Self::Address(_, inner) | Self::List(inner) => inner.has_class(),
            Self::Function(_, _) | Self::Unknown | Self::Parameter(_) => false,
        }
    }
}
impl Check<'_, '_, '_> {
    fn pay(&mut self, span: Span, n: usize) -> R<()> {
        self.budget.identity_work(n, span)
    }
    fn work(&mut self, f: &Fact, span: Span) -> R<()> {
        self.pay(span, 1)?;
        match f {
            Fact::Named(path, fs) => {
                for part in path {
                    self.budget.text(part, span)?;
                }
                for f in fs {
                    self.work(f, span)?;
                }
            }
            Fact::Tuple(fs) => {
                for f in fs {
                    self.work(f, span)?;
                }
            }
            Fact::Address(life, inner) => {
                if let Some(life) = life {
                    self.work(life, span)?;
                }
                self.work(inner, span)?;
            }
            Fact::List(inner) => self.work(inner, span)?,
            Fact::Function(params, result) => {
                for f in params {
                    self.work(f, span)?;
                }
                self.work(result, span)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn copy(&mut self, f: &Fact, span: Span) -> R<Fact> {
        self.work(f, span)?;
        Ok(f.clone())
    }
    fn environment(&mut self, env: &Facts, span: Span) -> R<Facts> {
        for (name, f) in env {
            self.budget.text(name, span)?;
            self.work(f, span)?;
        }
        Ok(env.clone())
    }
    fn binder(&mut self, scope: &Scope<'_>, name: &str, span: Span) -> R<Option<(Binder, bool)>> {
        for (ordinal, p) in scope.parameters.iter().enumerate() {
            self.budget.node(p.span, 0)?;
            self.budget.text(name, span)?;
            self.budget.text(&p.name, p.span)?;
            if p.name == name {
                return Ok(Some((
                    Binder(scope.owner, ordinal),
                    matches!(p.kind, GenericParamKind::Lifetime),
                )));
            }
        }
        Ok(None)
    }
    fn ty(&mut self, ty: &TypeRef, scope: &Scope<'_>, depth: usize) -> R<Fact> {
        self.budget.node(ty.span, depth)?;
        Ok(match &ty.kind {
            TypeKind::Group(inner) => self.ty(inner, scope, depth + 1)?,
            TypeKind::Named { path, args } => {
                if let [name] = path.as_slice()
                    && args.is_empty()
                    && let Some((binder, false)) = self.binder(scope, name, ty.span)?
                {
                    return Ok(Fact::Parameter(binder));
                }
                for part in path {
                    self.budget.text(part, ty.span)?;
                }
                let mut actuals = Vec::new();
                for arg in args {
                    self.budget.node(ty.span, depth)?;
                    actuals.push(match arg {
                        GenericArg::Type(t) => self.ty(t, scope, depth + 1)?,
                        GenericArg::Lifetime(name) => match self.binder(scope, name, ty.span)? {
                            Some((binder, true)) => Fact::Class(binder),
                            _ => Fact::Unknown,
                        },
                        _ => Fact::Unknown,
                    });
                }
                Fact::Named(path.clone(), actuals)
            }
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                let life = lifetime
                    .as_ref()
                    .map(|name| self.binder(scope, name, ty.span))
                    .transpose()?
                    .flatten()
                    .map(|(b, _)| Box::new(Fact::Class(b)));
                Fact::Address(life, Box::new(self.ty(inner, scope, depth + 1)?))
            }
            TypeKind::Tuple(fs) => {
                let mut out = Vec::new();
                for f in fs {
                    out.push(self.ty(f, scope, depth + 1)?);
                }
                Fact::Tuple(out)
            }
            TypeKind::Array { element, .. } | TypeKind::Optional(element) => {
                Fact::List(Box::new(self.ty(element, scope, depth + 1)?))
            }
            TypeKind::Function { params, result } => {
                let mut out = Vec::new();
                for p in params {
                    out.push(self.ty(p, scope, depth + 1)?);
                }
                Fact::Function(out, Box::new(self.ty(result, scope, depth + 1)?))
            }
            // Independently quantified Code has its own binder scope and consumer.
            _ => Fact::Unknown,
        })
    }
    fn compatible(&mut self, expected: &Fact, actual: &Fact, span: Span) -> R<()> {
        self.work(expected, span)?;
        self.work(actual, span)?;
        match (expected, actual) {
            (Fact::Class(a), Fact::Class(b)) if a != b => {
                return Err(error(
                    span,
                    "source annotation cannot equate distinct declared lifetime classes",
                ));
            }
            (Fact::Named(a, xs), Fact::Named(b, ys)) if a == b && xs.len() == ys.len() => {
                for (x, y) in xs.iter().zip(ys) {
                    self.compatible(x, y, span)?;
                }
            }
            (Fact::Tuple(xs), Fact::Tuple(ys)) if xs.len() == ys.len() => {
                for (x, y) in xs.iter().zip(ys) {
                    self.compatible(x, y, span)?;
                }
            }
            // Outer address lifetimes permit ordinary borrow/reborrow relations;
            // invariant nominal classes inside the pointee are checked separately.
            (Fact::Address(_, x), Fact::Address(_, y)) | (Fact::List(x), Fact::List(y)) => {
                self.compatible(x, y, span)?
            }
            (x, Fact::Unknown) if x.has_class() => {
                return Err(unsupported(
                    span,
                    "ambient source annotation requires its actual class substitution",
                ));
            }
            _ => {}
        }
        Ok(())
    }
    fn substitute(&mut self, f: &Fact, map: &Substitutions, span: Span) -> R<Fact> {
        self.work(f, span)?;
        Ok(match f {
            Fact::Class(b) | Fact::Parameter(b) => match map.get(b) {
                Some(f) => self.copy(f, span)?,
                None => Fact::Unknown,
            },
            Fact::Named(path, fs) => {
                let mut out = Vec::new();
                for f in fs {
                    out.push(self.substitute(f, map, span)?);
                }
                Fact::Named(path.clone(), out)
            }
            Fact::Tuple(fs) => {
                let mut out = Vec::new();
                for f in fs {
                    out.push(self.substitute(f, map, span)?);
                }
                Fact::Tuple(out)
            }
            Fact::Address(life, inner) => Fact::Address(
                life.as_ref()
                    .map(|l| self.substitute(l, map, span).map(Box::new))
                    .transpose()?,
                Box::new(self.substitute(inner, map, span)?),
            ),
            Fact::List(f) => Fact::List(Box::new(self.substitute(f, map, span)?)),
            _ => self.copy(f, span)?,
        })
    }
    fn infer(
        &mut self,
        formal: &Fact,
        actual: &Fact,
        map: &mut Substitutions,
        span: Span,
    ) -> R<()> {
        self.work(formal, span)?;
        self.work(actual, span)?;
        match (formal, actual) {
            (Fact::Class(b) | Fact::Parameter(b), actual) => {
                self.pay(span, map.len().saturating_add(1))?;
                if let Some(prior) = map.get(b) {
                    self.compatible(prior, actual, span)?;
                } else {
                    map.insert(*b, self.copy(actual, span)?);
                }
            }
            (Fact::Named(a, fs), Fact::Named(b, xs)) if a == b => {
                for (f, x) in fs.iter().zip(xs) {
                    self.infer(f, x, map, span)?;
                }
            }
            (Fact::Tuple(fs), Fact::Tuple(xs)) => {
                for (f, x) in fs.iter().zip(xs) {
                    self.infer(f, x, map, span)?;
                }
            }
            (Fact::Address(f, fi), Fact::Address(x, xi)) => {
                if let (Some(f), Some(x)) = (f, x) {
                    self.infer(f, x, map, span)?;
                }
                self.infer(fi, xi, map, span)?;
            }
            (Fact::List(f), Fact::List(x)) => self.infer(f, x, map, span)?,
            _ => {}
        }
        Ok(())
    }
    fn bind(&mut self, p: &Pattern, fact: &Fact, env: &mut Facts) -> R<()> {
        self.budget.node(p.span, 0)?;
        match &p.kind {
            PatternKind::Name(name) => {
                self.budget.text(name, p.span)?;
                env.insert(name.clone(), self.copy(fact, p.span)?);
            }
            PatternKind::Tuple(ps) => {
                for (i, p) in ps.iter().enumerate() {
                    let f = if let Fact::Tuple(fs) = fact {
                        fs.get(i).unwrap_or(&Fact::Unknown)
                    } else {
                        &Fact::Unknown
                    };
                    self.bind(p, f, env)?;
                }
            }
            PatternKind::Reference { pattern, .. } => {
                self.bind(
                    pattern,
                    if let Fact::Address(_, f) = fact {
                        f
                    } else {
                        &Fact::Unknown
                    },
                    env,
                )?;
            }
            _ => {}
        }
        Ok(())
    }
    fn field(&mut self, fact: &Fact, member: &MemberName, span: Span, depth: usize) -> R<Fact> {
        let fact = if let Fact::Address(_, f) = fact {
            f.as_ref()
        } else {
            fact
        };
        if let (Fact::Tuple(fs), MemberName::TupleIndex(index)) = (fact, member) {
            return match fs.get(*index as usize) {
                Some(f) => self.copy(f, span),
                None => Ok(Fact::Unknown),
            };
        }
        let (Fact::Named(path, actuals), MemberName::Name(name)) = (fact, member) else {
            return Ok(Fact::Unknown);
        };
        let Some(root) = path.first().filter(|_| path.len() == 1) else {
            return Ok(Fact::Unknown);
        };
        for index in 0..self.records.len() {
            let declaration = self.records[index];
            self.budget.text(root, span)?;
            self.budget.text(&declaration.name, span)?;
            if &declaration.name != root {
                continue;
            }
            let scope = Scope {
                owner: self.functions.len() + index,
                parameters: &declaration.generics,
            };
            let mut map = Substitutions::new();
            for (ordinal, actual) in actuals.iter().enumerate() {
                self.pay(span, 1)?;
                map.insert(Binder(scope.owner, ordinal), self.copy(actual, span)?);
            }
            for field in &declaration.fields {
                self.budget.text(name, span)?;
                self.budget.text(&field.name, span)?;
                if &field.name == name {
                    let formal = self.ty(&field.ty, &scope, depth + 1)?;
                    return self.substitute(&formal, &map, span);
                }
            }
        }
        Ok(Fact::Unknown)
    }
    fn expr(
        &mut self,
        e: &Expr,
        scope: &Scope<'_>,
        env: &mut Facts,
        wanted: Option<&Fact>,
        depth: usize,
    ) -> R<Fact> {
        self.budget.node(e.span, depth)?;
        let actual = match &e.kind {
            ExprKind::Name(name) => {
                self.budget.text(name, e.span)?;
                match env.get(name) {
                    Some(f) => self.copy(f, e.span)?,
                    None => Fact::Unknown,
                }
            }
            ExprKind::Group(inner) | ExprKind::MetaEval(inner) | ExprKind::MetaSplice(inner) => {
                self.expr(inner, scope, env, wanted, depth + 1)?
            }
            ExprKind::Ascribed { ty, value } => {
                let annotation = self.ty(ty, scope, depth + 1)?;
                self.expr(value, scope, env, Some(&annotation), depth + 1)?;
                annotation
            }
            ExprKind::Record { ty, base, fields } => {
                let explicit = match ty.as_deref().map(|t| &t.kind) {
                    Some(ExprKind::TypeApply { callee, arguments }) => match &callee.kind {
                        ExprKind::Name(name) => {
                            self.budget.text(name, e.span)?;
                            let mut args = Vec::new();
                            for a in arguments {
                                args.push(match a {
                                    GenericArg::Lifetime(name) => {
                                        match self.binder(scope, name, e.span)? {
                                            Some((b, true)) => Fact::Class(b),
                                            _ => Fact::Unknown,
                                        }
                                    }
                                    GenericArg::Type(t) => self.ty(t, scope, depth + 1)?,
                                    _ => Fact::Unknown,
                                });
                            }
                            Fact::Named(vec![name.clone()], args)
                        }
                        _ => Fact::Unknown,
                    },
                    Some(ExprKind::Name(name)) => {
                        self.budget.text(name, e.span)?;
                        Fact::Named(vec![name.clone()], vec![])
                    }
                    _ => Fact::Unknown,
                };
                let record = if let Some(base) = base {
                    self.expr(base, scope, env, Some(&explicit), depth + 1)?
                } else {
                    explicit
                };
                for field in fields {
                    if let Some(value) = &field.value {
                        let hint = self.field(
                            &record,
                            &MemberName::Name(field.name.clone()),
                            e.span,
                            depth + 1,
                        )?;
                        self.expr(value, scope, env, Some(&hint), depth + 1)?;
                    }
                }
                record
            }
            ExprKind::Tuple(values) | ExprKind::Array(values) => {
                let mut fs = Vec::new();
                for (i, value) in values.iter().enumerate() {
                    let hint = if let Some(Fact::Tuple(fs)) = wanted {
                        fs.get(i)
                    } else {
                        None
                    };
                    fs.push(self.expr(value, scope, env, hint, depth + 1)?);
                }
                Fact::Tuple(fs)
            }
            ExprKind::Member { object, member } => {
                let object = self.expr(object, scope, env, None, depth + 1)?;
                self.field(&object, member, e.span, depth + 1)?
            }
            ExprKind::Unary { op, operand } => {
                let source = self.expr(operand, scope, env, None, depth + 1)?;
                match op {
                    UnaryOp::Borrow | UnaryOp::BorrowMut => Fact::Address(None, Box::new(source)),
                    UnaryOp::Dereference => {
                        if let Fact::Address(_, f) = source {
                            *f
                        } else {
                            Fact::Unknown
                        }
                    }
                    _ => Fact::Unknown,
                }
            }
            ExprKind::Call { callee, arguments } => {
                let mut actuals = Vec::new();
                for arg in arguments {
                    actuals.push(self.expr(&arg.value, scope, env, None, depth + 1)?);
                }
                let (root, explicit) = match &callee.kind {
                    ExprKind::Name(name) => (Some(name.as_str()), None),
                    ExprKind::TypeApply { callee, arguments } => (
                        if let ExprKind::Name(name) = &callee.kind {
                            Some(name.as_str())
                        } else {
                            None
                        },
                        Some(arguments),
                    ),
                    _ => (None, None),
                };
                let mut output = Fact::Unknown;
                if let Some(name) = root.filter(|name| !env.contains_key(*name)) {
                    for owner in 0..self.functions.len() {
                        let f = self.functions[owner];
                        self.budget.text(name, e.span)?;
                        self.budget.text(&f.name, e.span)?;
                        if f.name != name {
                            continue;
                        }
                        let definition = Scope {
                            owner,
                            parameters: &f.generics,
                        };
                        let mut map = Substitutions::new();
                        if let Some(args) = explicit {
                            for ((ordinal, _), arg) in f
                                .generics
                                .iter()
                                .enumerate()
                                .filter(|(_, p)| !matches!(p.kind, GenericParamKind::Lifetime))
                                .zip(args)
                            {
                                if let GenericArg::Type(t) = arg {
                                    let fact = self.ty(t, scope, depth + 1)?;
                                    self.pay(e.span, 1)?;
                                    map.insert(Binder(owner, ordinal), fact);
                                }
                            }
                        }
                        for (formal, actual) in f.params.iter().zip(&actuals) {
                            let formal = self.ty(&formal.ty, &definition, depth + 1)?;
                            self.infer(&formal, actual, &mut map, e.span)?;
                        }
                        if let Some(result) = &f.return_type {
                            let f = self.ty(result, &definition, depth + 1)?;
                            output = self.substitute(&f, &map, e.span)?;
                        }
                        break;
                    }
                }
                output
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, scope, env, None, depth + 1)?;
                let a = self.expr(then_value, scope, env, wanted, depth + 1)?;
                self.expr(else_value, scope, env, wanted.or(Some(&a)), depth + 1)?;
                a
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr(left, scope, env, None, depth + 1)?;
                self.expr(right, scope, env, None, depth + 1)?;
                Fact::Unknown
            }
            ExprKind::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                let mut child = self.environment(env, e.span)?;
                for p in params {
                    let f =
                        p.ty.as_ref()
                            .map(|t| self.ty(t, scope, depth + 1))
                            .transpose()?
                            .unwrap_or(Fact::Unknown);
                    self.budget.text(&p.name, p.span)?;
                    child.insert(p.name.clone(), f);
                }
                let output = return_type
                    .as_ref()
                    .map(|t| self.ty(t, scope, depth + 1))
                    .transpose()?;
                self.arrow(body, scope, &mut child, output.as_ref(), depth + 1)?;
                Fact::Unknown
            }
            _ => Fact::Unknown,
        };
        if let Some(wanted) = wanted {
            self.compatible(wanted, &actual, e.span)?;
        }
        Ok(actual)
    }
    fn arrow(
        &mut self,
        body: &ArrowBody,
        scope: &Scope<'_>,
        env: &mut Facts,
        result: Option<&Fact>,
        depth: usize,
    ) -> R<()> {
        match body {
            ArrowBody::Block(b) => self.block(b, scope, env, result, depth),
            ArrowBody::Expression(e) => self.expr(e, scope, env, result, depth).map(|_| ()),
        }
    }
    fn block(
        &mut self,
        block: &Block,
        scope: &Scope<'_>,
        env: &mut Facts,
        result: Option<&Fact>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(block.span, depth)?;
        let mut local = self.environment(env, block.span)?;
        for statement in &block.statements {
            self.statement(statement, scope, &mut local, result, depth + 1)?;
        }
        Ok(())
    }
    fn condition(
        &mut self,
        condition: &Condition,
        scope: &Scope<'_>,
        env: &mut Facts,
        depth: usize,
    ) -> R<()> {
        match condition {
            Condition::Expression(e) => {
                self.expr(e, scope, env, None, depth)?;
            }
            Condition::Let { pattern, value } => {
                let f = self.expr(value, scope, env, None, depth)?;
                self.bind(pattern, &f, env)?;
            }
        }
        Ok(())
    }
    fn statement(
        &mut self,
        s: &Stmt,
        scope: &Scope<'_>,
        env: &mut Facts,
        result: Option<&Fact>,
        depth: usize,
    ) -> R<()> {
        self.budget.node(s.span, depth)?;
        match &s.kind {
            StmtKind::Binding {
                pattern, ty, value, ..
            } => {
                let annotation = ty
                    .as_ref()
                    .map(|t| self.ty(t, scope, depth + 1))
                    .transpose()?;
                let actual = self.expr(value, scope, env, annotation.as_ref(), depth + 1)?;
                self.bind(pattern, annotation.as_ref().unwrap_or(&actual), env)?;
            }
            StmtKind::Assign { target, value, .. } => {
                let target = self.expr(target, scope, env, None, depth + 1)?;
                self.expr(value, scope, env, Some(&target), depth + 1)?;
            }
            StmtKind::Return(Some(e)) => {
                self.expr(e, scope, env, result, depth + 1)?;
            }
            StmtKind::Yield(e) | StmtKind::Expression(e) => {
                self.expr(e, scope, env, None, depth + 1)?;
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let mut child = self.environment(env, s.span)?;
                self.condition(condition, scope, &mut child, depth + 1)?;
                self.block(then_body, scope, &mut child, result, depth + 1)?;
                match else_body {
                    Some(ElseBranch::Block(b)) => self.block(b, scope, env, result, depth + 1)?,
                    Some(ElseBranch::If(s)) => self.statement(s, scope, env, result, depth + 1)?,
                    None => {}
                }
            }
            StmtKind::While { condition, body } => {
                let mut child = self.environment(env, s.span)?;
                self.condition(condition, scope, &mut child, depth + 1)?;
                self.block(body, scope, &mut child, result, depth + 1)?;
            }
            StmtKind::Loop(b) | StmtKind::Unsafe(b) | StmtKind::Defer(b) => {
                self.block(b, scope, env, result, depth + 1)?
            }
            StmtKind::For {
                iterable,
                body,
                pattern,
            } => {
                let f = self.expr(iterable, scope, env, None, depth + 1)?;
                let mut child = self.environment(env, s.span)?;
                self.bind(pattern, &f, &mut child)?;
                self.block(body, scope, &mut child, result, depth + 1)?;
            }
            StmtKind::Match { value, arms } => {
                let f = self.expr(value, scope, env, None, depth + 1)?;
                for arm in arms {
                    let mut child = self.environment(env, s.span)?;
                    self.bind(&arm.pattern, &f, &mut child)?;
                    self.arrow(&arm.body, scope, &mut child, result, depth + 1)?;
                }
            }
            StmtKind::With { value, body, .. } => {
                self.expr(value, scope, env, None, depth + 1)?;
                self.block(body, scope, env, result, depth + 1)?;
            }
            _ => {}
        }
        Ok(())
    }
}

pub(super) fn validate(module: &Module, budget: &mut Budget<'_>) -> R<()> {
    if !budget.has_stored_lifetimes() {
        return Ok(());
    }
    fn inventory<'a>(
        items: &'a [Item],
        functions: &mut Vec<&'a Function>,
        records: &mut Vec<&'a Aggregate>,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<()> {
        for item in items {
            budget.node(item.span, depth)?;
            match &item.kind {
                ItemKind::Function(f) => functions.push(f),
                ItemKind::Aggregate(a) => records.push(a),
                ItemKind::Impl(i) => inventory(&i.members, functions, records, budget, depth + 1)?,
                ItemKind::Trait(t) => inventory(&t.members, functions, records, budget, depth + 1)?,
                _ => {}
            }
        }
        Ok(())
    }
    let mut functions = Vec::new();
    let mut records = Vec::new();
    inventory(&module.items, &mut functions, &mut records, budget, 0)?;
    let mut check = Check {
        functions,
        records,
        budget,
    };
    for owner in 0..check.functions.len() {
        let f = check.functions[owner];
        check.pay(f.span, f.generics.len())?;
        if !f
            .generics
            .iter()
            .any(|p| matches!(p.kind, GenericParamKind::Lifetime))
        {
            continue;
        }
        let scope = Scope {
            owner,
            parameters: &f.generics,
        };
        let mut env = Facts::new();
        for p in &f.params {
            let fact = check.ty(&p.ty, &scope, 0)?;
            check.budget.text(&p.name, p.span)?;
            env.insert(p.name.clone(), fact);
        }
        let result = f
            .return_type
            .as_ref()
            .map(|t| check.ty(t, &scope, 0))
            .transpose()?;
        match &f.body {
            FunctionBody::Block(body) => check.block(body, &scope, &mut env, result.as_ref(), 0)?,
            FunctionBody::Expression(e) => {
                check.expr(e, &scope, &mut env, result.as_ref(), 0)?;
            }
            FunctionBody::Declaration => {}
        }
    }
    Ok(())
}
