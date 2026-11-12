//! Paid source-lifetime alpha replacement before physical erasure.
use super::*;
pub(super) fn function(
    f: &mut Function,
    names: &BTreeMap<String, String>,
    budget: &mut Budget<'_>,
) -> R<()> {
    let mut w = Alpha {
        names,
        bound: Vec::new(),
        budget,
    };
    for p in &f.generics {
        w.budget.node(p.span, 0)?;
        if matches!(p.kind, GenericParamKind::Lifetime) {
            w.budget.text(&p.name, p.span)?;
            w.bound.push(p.name.clone());
        }
    }
    for p in &mut f.generics {
        match &mut p.kind {
            GenericParamKind::Type(bounds) => {
                for t in bounds {
                    w.ty(t, 1)?;
                }
            }
            GenericParamKind::Const(t) => w.ty(t, 1)?,
            GenericParamKind::Lifetime => {}
        }
    }
    for p in &mut f.params {
        w.ty(&mut p.ty, 1)?;
    }
    if let Some(t) = &mut f.return_type {
        w.ty(t, 1)?;
    }
    for c in &mut f.where_clause {
        w.budget.node(c.span, 0)?;
        match &mut c.subject {
            BoundSubject::Type(t) => w.ty(t, 1)?,
            BoundSubject::Lifetime(n) => w.life(n, c.span)?,
        }
        for t in &mut c.bounds {
            w.ty(t, 1)?;
        }
        if let Some(n) = &mut c.lifetime_bound {
            w.life(n, c.span)?;
        }
    }
    for c in &mut f.contract {
        w.budget.node(c.span, 0)?;
        for t in &mut c.terms {
            w.term(t, 1)?;
        }
    }
    match &mut f.body {
        FunctionBody::Block(b) => w.block(b, 1),
        FunctionBody::Expression(e) => w.expr(e, 1),
        FunctionBody::Declaration => Ok(()),
    }
}
struct Alpha<'a, 'b, 'l> {
    names: &'a BTreeMap<String, String>,
    bound: Vec<String>,
    budget: &'b mut Budget<'l>,
}
impl Alpha<'_, '_, '_> {
    fn life(&mut self, n: &mut String, s: Span) -> R<()> {
        for b in &self.bound {
            self.budget.text(b, s)?;
            self.budget.text(n, s)?;
            if b == n {
                return Ok(());
            }
        }
        self.budget.identity_work(
            self.names
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2),
            s,
        )?;
        self.budget.text(n, s)?;
        if let Some(a) = self.names.get(n) {
            self.budget.text(a, s)?;
            *n = a.clone();
        }
        Ok(())
    }
    fn arg(&mut self, a: &mut GenericArg, s: Span, d: usize) -> R<()> {
        self.budget.node(s, d)?;
        match a {
            GenericArg::AssociatedBinding(binding) => {
                return Err(error(
                    binding.span,
                    "associated equations require the original trait application obligation consumer",
                ));
            }
            GenericArg::Lifetime(n) => self.life(n, s),
            GenericArg::Type(t) => self.ty(t, d + 1),
            GenericArg::Const(e) => self.expr(e, d + 1),
        }
    }
    fn term(&mut self, t: &mut ContractTerm, d: usize) -> R<()> {
        self.budget.node(t.span, d)?;
        for a in &mut t.type_args {
            self.arg(a, t.span, d + 1)?;
        }
        for a in &mut t.arguments {
            self.term(a, d + 1)?;
        }
        if let Some(i) = &mut t.index {
            self.term(i, d + 1)?;
        }
        Ok(())
    }
    fn ty(&mut self, t: &mut TypeRef, d: usize) -> R<()> {
        self.budget.node(t.span, d)?;
        match &mut t.kind {
            TypeKind::Named { args, .. } => {
                for a in args {
                    self.arg(a, t.span, d + 1)?;
                }
            }
            TypeKind::Dyn { args, bindings, .. } => {
                for a in args {
                    self.arg(a, t.span, d + 1)?;
                }
                for b in bindings {
                    self.budget.node(b.span, d)?;
                    match &mut b.kind {
                        DynBindingKind::Type { value, .. } => self.ty(value, d + 1)?,
                        DynBindingKind::Constant { ty, value, .. } => {
                            self.ty(ty, d + 1)?;
                            self.expr(value, d + 1)?;
                        }
                        DynBindingKind::Method { arguments, .. } => {
                            for a in arguments {
                                self.arg(a, b.span, d + 1)?;
                            }
                        }
                    }
                }
            }
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                if let Some(n) = lifetime {
                    self.life(n, t.span)?;
                }
                self.ty(inner, d + 1)?;
            }
            TypeKind::ForLifetime { lifetime, inner } => {
                self.budget.text(lifetime, t.span)?;
                self.bound.push(lifetime.clone());
                self.ty(inner, d + 1)?;
                self.bound.pop();
            }
            TypeKind::Tuple(ts) | TypeKind::Union(ts) | TypeKind::Impl(ts) => {
                for t in ts {
                    self.ty(t, d + 1)?;
                }
            }
            TypeKind::Group(t) | TypeKind::Optional(t) => self.ty(t, d + 1)?,
            TypeKind::Array { element, length } => {
                self.ty(element, d + 1)?;
                if let Some(e) = length {
                    self.expr(e, d + 1)?;
                }
            }
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for t in params {
                    self.ty(t, d + 1)?;
                }
                self.ty(result, d + 1)?;
            }
            TypeKind::Error { error, result } => {
                self.ty(error, d + 1)?;
                self.ty(result, d + 1)?;
            }
            TypeKind::MetaEval(e) | TypeKind::MetaSplice(e) => self.expr(e, d + 1)?,
            TypeKind::MetaBlock(b) => self.block(b, d + 1)?,
        }
        Ok(())
    }
    fn condition(&mut self, c: &mut Condition, d: usize) -> R<()> {
        match c {
            Condition::Expression(e) | Condition::Let { value: e, .. } => self.expr(e, d),
        }
    }
    fn block(&mut self, b: &mut Block, d: usize) -> R<()> {
        self.budget.node(b.span, d)?;
        for s in &mut b.statements {
            self.statement(s, d + 1)?;
        }
        Ok(())
    }
    fn statement(&mut self, s: &mut Stmt, d: usize) -> R<()> {
        self.budget.node(s.span, d)?;
        match &mut s.kind {
            StmtKind::Binding { ty, value, .. } => {
                if let Some(t) = ty {
                    self.ty(t, d + 1)?;
                }
                self.expr(value, d + 1)?;
            }
            StmtKind::Return(Some(e)) | StmtKind::Yield(e) | StmtKind::Expression(e) => {
                self.expr(e, d + 1)?
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Assign { target, value, .. } => {
                self.expr(target, d + 1)?;
                self.expr(value, d + 1)?;
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.condition(condition, d + 1)?;
                self.block(then_body, d + 1)?;
                match else_body {
                    Some(ElseBranch::Block(b)) => self.block(b, d + 1)?,
                    Some(ElseBranch::If(s)) => self.statement(s, d + 1)?,
                    None => {}
                }
            }
            StmtKind::While { condition, body } => {
                self.condition(condition, d + 1)?;
                self.block(body, d + 1)?;
            }
            StmtKind::For { iterable, body, .. }
            | StmtKind::With {
                value: iterable,
                body,
                ..
            } => {
                self.expr(iterable, d + 1)?;
                self.block(body, d + 1)?;
            }
            StmtKind::Loop(b) | StmtKind::Unsafe(b) | StmtKind::Defer(b) => self.block(b, d + 1)?,
            StmtKind::Match { value, arms } => {
                self.expr(value, d + 1)?;
                for a in arms {
                    self.arrow(&mut a.body, d + 1)?;
                }
            }
            StmtKind::Meta(_) => {
                return Err(unsupported(
                    s.span,
                    "provider lifetime alpha requires evaluated statement syntax",
                ));
            }
        }
        Ok(())
    }
    fn arrow(&mut self, b: &mut ArrowBody, d: usize) -> R<()> {
        match b {
            ArrowBody::Block(b) => self.block(b, d),
            ArrowBody::Expression(e) => self.expr(e, d),
        }
    }
    fn expr(&mut self, e: &mut Expr, d: usize) -> R<()> {
        self.budget.node(e.span, d)?;
        match &mut e.kind {
            ExprKind::Literal(_) | ExprKind::Name(_) | ExprKind::ReceiverMember(_) => {}
            ExprKind::Ascribed { ty, value } => {
                self.ty(ty, d + 1)?;
                self.expr(value, d + 1)?;
            }
            ExprKind::Group(e)
            | ExprKind::Try(e)
            | ExprKind::MetaEval(e)
            | ExprKind::MetaSplice(e)
            | ExprKind::Unary { operand: e, .. } => self.expr(e, d + 1)?,
            ExprKind::Tuple(es) | ExprKind::Array(es) => {
                for e in es {
                    self.expr(e, d + 1)?;
                }
            }
            ExprKind::Binary { left, right, .. }
            | ExprKind::Pipeline {
                input: left,
                target: right,
            }
            | ExprKind::Index {
                object: left,
                index: right,
            } => {
                self.expr(left, d + 1)?;
                self.expr(right, d + 1)?;
            }
            ExprKind::Member { object, member } => {
                self.expr(object, d + 1)?;
                if let MemberName::Constrained { owner, .. } = member {
                    self.ty(owner, d + 1)?;
                }
            }
            ExprKind::TypeApply { callee, arguments } => {
                self.expr(callee, d + 1)?;
                for a in arguments {
                    self.arg(a, e.span, d + 1)?;
                }
            }
            ExprKind::Call { callee, arguments } => {
                self.expr(callee, d + 1)?;
                for a in arguments {
                    self.expr(&mut a.value, d + 1)?;
                }
            }
            ExprKind::Slice { object, start, end } => {
                self.expr(object, d + 1)?;
                for e in [start, end].into_iter().flatten() {
                    self.expr(e, d + 1)?;
                }
            }
            ExprKind::Record { ty, base, fields } => {
                for e in [ty, base].into_iter().flatten() {
                    self.expr(e, d + 1)?;
                }
                for f in fields {
                    if let Some(e) = &mut f.value {
                        self.expr(e, d + 1)?;
                    }
                }
            }
            ExprKind::Closure {
                params,
                return_type,
                body,
                ..
            } => {
                for p in params {
                    if let Some(t) = &mut p.ty {
                        self.ty(t, d + 1)?;
                    }
                }
                if let Some(t) = return_type {
                    self.ty(t, d + 1)?;
                }
                self.arrow(body, d + 1)?;
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, d + 1)?;
                self.expr(then_value, d + 1)?;
                self.expr(else_value, d + 1)?;
            }
            ExprKind::MetaBlock(b) => self.block(b, d + 1)?,
            ExprKind::Quote { .. } | ExprKind::SyntaxIsland { .. } => {
                return Err(unsupported(
                    e.span,
                    "provider lifetime alpha requires evaluated expression syntax",
                ));
            }
        }
        Ok(())
    }
}
