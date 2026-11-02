//! Borrowed preflight for the AST trees specialization may clone or substitute.
//! This is a private bound on work, never a type or capability certificate.
use super::*;

pub(crate) struct Budget<'a> {
    limits: &'a Limits,
    nodes: usize,
    work: usize,
}
impl<'a> Budget<'a> {
    pub(crate) fn new(limits: &'a Limits) -> Self {
        Self {
            limits,
            nodes: limits.max_nodes,
            work: limits.max_analysis_steps,
        }
    }
    pub(super) fn remaining_limits(&self) -> Limits {
        Limits {
            max_nodes: self.nodes,
            max_analysis_steps: self.work,
            ..self.limits.clone()
        }
    }
    pub(crate) fn node(&mut self, span: Span, depth: usize) -> R<()> {
        if depth > self.limits.max_call_depth || self.nodes == 0 || self.work == 0 {
            return Err(Diagnostic::limit(
                Phase::Expand,
                "generic expansion budget exceeded",
                Some(span),
            ));
        }
        self.nodes -= 1;
        self.work -= 1;
        Ok(())
    }
    pub(crate) fn text(&mut self, text: &str, span: Span) -> R<()> {
        self.work = self.work.checked_sub(text.len()).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Expand,
                "generic metadata budget exceeded",
                Some(span),
            )
        })?;
        Ok(())
    }
    pub(crate) fn wide_units(&mut self, units: &[u16], span: Span) -> R<()> {
        self.work = self.work.checked_sub(units.len()).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Expand,
                "generic static UTF-16 budget exceeded",
                Some(span),
            )
        })?;
        Ok(())
    }
    pub(crate) fn ty(&mut self, ty: &TypeRef) -> R<()> {
        self.ty_at(ty, 0, None)
    }
    pub(crate) fn ty_depth(&mut self, ty: &TypeRef, depth: usize) -> R<()> {
        self.ty_at(ty, depth, None)
    }
    pub(crate) fn substituted_type(
        &mut self,
        ty: &TypeRef,
        substitutions: &BTreeMap<String, TypeRef>,
    ) -> R<()> {
        self.ty_at(ty, 0, Some(substitutions))
    }
    pub(crate) fn substituted_type_depth(
        &mut self,
        ty: &TypeRef,
        substitutions: &BTreeMap<String, TypeRef>,
        depth: usize,
    ) -> R<()> {
        self.ty_at(ty, depth, Some(substitutions))
    }
    pub(crate) fn item(&mut self, item: &Item) -> R<()> {
        self.item_at(item, 0, None)
    }
    pub(crate) fn substituted_item(
        &mut self,
        item: &Item,
        substitutions: &BTreeMap<String, TypeRef>,
    ) -> R<()> {
        self.item_at(item, 0, Some(substitutions))
    }
    fn ty_at(
        &mut self,
        ty: &TypeRef,
        depth: usize,
        substitutions: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.ty_context(ty, depth, substitutions, false)
    }
    fn ty_context(
        &mut self,
        ty: &TypeRef,
        depth: usize,
        substitutions: Option<&BTreeMap<String, TypeRef>>,
        slice_element: bool,
    ) -> R<()> {
        self.node(ty.span, depth)?;
        if let TypeKind::Named { path, args } = &ty.kind
            && path.len() == 1
            && args.is_empty()
            && let Some(value) = substitutions.and_then(|s| s.get(&path[0]))
        {
            // A substitution is simultaneous. Do not substitute caller T again
            // merely because the callee's type parameter is also named T.
            return self.ty_context(value, depth, None, slice_element);
        }
        if let TypeKind::ForeignFunction { abi, .. } = &ty.kind {
            self.text(abi, ty.span)?;
        }
        match &ty.kind {
            TypeKind::Named { path, args } | TypeKind::Dyn { path, args } => {
                for name in path {
                    self.text(name, ty.span)?;
                }
                for arg in args {
                    self.arg(arg, depth + 1, substitutions)?;
                }
            }
            TypeKind::Reference {
                lifetime, inner, ..
            } => {
                if let Some(name) = lifetime {
                    self.text(name, ty.span)?;
                }
                self.ty_context(inner, depth + 1, substitutions, slice_element)?;
            }
            TypeKind::Group(inner) | TypeKind::Optional(inner) => {
                self.ty_context(inner, depth + 1, substitutions, slice_element)?
            }
            TypeKind::ForLifetime { lifetime, inner } => {
                self.text(lifetime, ty.span)?;
                self.ty_context(inner, depth + 1, substitutions, slice_element)?;
            }
            TypeKind::Tuple(types) | TypeKind::Union(types) | TypeKind::Impl(types) => {
                for ty in types {
                    self.ty_context(ty, depth + 1, substitutions, slice_element)?;
                }
            }
            TypeKind::Array { element, length } => {
                self.ty_context(
                    element,
                    depth + 1,
                    substitutions,
                    slice_element || length.is_none(),
                )?;
                if let Some(e) = length {
                    self.expr(e, depth + 1, substitutions)?;
                }
            }
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for ty in params {
                    self.ty_context(ty, depth + 1, substitutions, slice_element)?;
                }
                self.ty_context(result, depth + 1, substitutions, slice_element)?;
            }
            TypeKind::Error { error, result } => {
                self.ty_context(error, depth + 1, substitutions, slice_element)?;
                self.ty_context(result, depth + 1, substitutions, slice_element)?;
            }
            TypeKind::MetaEval(e) => self.expr(e, depth + 1, substitutions)?,
            TypeKind::MetaBlock(b) => self.block(b, depth + 1, substitutions)?,
        }
        // Each classified subtree has consumed the existing type budget. Preserve
        // definite non-Copy slice element shapes even in erased aliases and
        // unused templates. Named types/arguments still require ordinary
        // resolution; this does not infer a Copy capability for them.
        if slice_element
            && matches!(
                ty.kind,
                TypeKind::Reference { .. } | TypeKind::Array { length: None, .. }
            )
        {
            return Err(unsupported(
                ty.span,
                "references and unsized arrays cannot be slice elements",
            ));
        }
        // Preserve definite callback formation errors even when specialization
        // or alias elimination later removes this written type. Unresolved
        // names are checked again in their normalized lexical type scope.
        super::super::foreign::callback_type_formation(ty, None)
    }
    fn arg(
        &mut self,
        arg: &GenericArg,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match arg {
            GenericArg::Type(t) => self.ty_at(t, depth, subs),
            GenericArg::Const(e) => self.expr(e, depth, subs),
        }
    }
    fn attrs(
        &mut self,
        attrs: &[Attribute],
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        for a in attrs {
            self.node(a.span, depth)?;
            for s in &a.path {
                self.text(s, a.span)?;
            }
            if let Some(s) = &a.string_value {
                self.text(s, a.span)?;
            }
            for e in &a.arguments {
                self.expr(e, depth + 1, subs)?;
            }
        }
        Ok(())
    }
    fn generics(
        &mut self,
        params: &[GenericParam],
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        for p in params {
            self.node(p.span, depth)?;
            self.text(&p.name, p.span)?;
            match &p.kind {
                GenericParamKind::Type(ts) => {
                    for t in ts {
                        self.ty_at(t, depth + 1, subs)?;
                    }
                }
                GenericParamKind::Const(t) => self.ty_at(t, depth + 1, subs)?,
                GenericParamKind::Lifetime => {}
            }
        }
        Ok(())
    }
    fn bounds(
        &mut self,
        bounds: &[BoundClause],
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        for b in bounds {
            self.node(b.span, depth)?;
            match &b.subject {
                BoundSubject::Type(t) => self.ty_at(t, depth + 1, subs)?,
                BoundSubject::Lifetime(s) => self.text(s, b.span)?,
            }
            for t in &b.bounds {
                self.ty_at(t, depth + 1, subs)?;
            }
            if let Some(s) = &b.lifetime_bound {
                self.text(s, b.span)?;
            }
        }
        Ok(())
    }
    fn param(
        &mut self,
        p: &Param,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.node(p.span, depth)?;
        self.text(&p.name, p.span)?;
        self.attrs(&p.attributes, depth + 1, subs)?;
        self.ty_at(&p.ty, depth + 1, subs)
    }
    fn function(
        &mut self,
        f: &Function,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.node(f.span, depth)?;
        self.text(&f.name, f.span)?;
        self.generics(&f.generics, depth + 1, subs)?;
        self.bounds(&f.where_clause, depth + 1, subs)?;
        for clause in &f.contract {
            self.node(clause.span, depth + 1)?;
            self.text(&clause.name, clause.span)?;
            for term in &clause.terms {
                self.contract(term, depth + 2, subs)?;
            }
        }
        for p in &f.params {
            self.param(p, depth + 1, subs)?;
        }
        if let Some(t) = &f.return_type {
            self.ty_at(t, depth + 1, subs)?;
        }
        match &f.body {
            FunctionBody::Block(b) => self.block(b, depth + 1, subs)?,
            FunctionBody::Expression(e) => self.expr(e, depth + 1, subs)?,
            FunctionBody::Declaration => {}
        }
        Ok(())
    }
    fn contract(
        &mut self,
        term: &ContractTerm,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.node(term.span, depth)?;
        for part in &term.path {
            self.text(part, term.span)?;
        }
        for arg in &term.type_args {
            self.arg(arg, depth + 1, subs)?;
        }
        for arg in &term.arguments {
            self.contract(arg, depth + 1, subs)?;
        }
        if let Some(index) = &term.index {
            self.contract(index, depth + 1, subs)?;
        }
        Ok(())
    }
    fn item_at(
        &mut self,
        item: &Item,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.node(item.span, depth)?;
        self.attrs(&item.attributes, depth + 1, subs)?;
        match &item.kind {
            ItemKind::TypeAlias(a) => {
                self.text(&a.name, item.span)?;
                self.generics(&a.generics, depth + 1, subs)?;
                for t in &a.bounds {
                    self.ty_at(t, depth + 1, subs)?;
                }
                if let Some(t) = &a.value {
                    self.ty_at(t, depth + 1, subs)?;
                }
            }
            ItemKind::Function(f) => self.function(f, depth + 1, subs)?,
            ItemKind::Aggregate(a) => {
                self.text(&a.name, item.span)?;
                self.generics(&a.generics, depth + 1, subs)?;
                for f in &a.fields {
                    self.node(f.span, depth + 1)?;
                    self.text(&f.name, f.span)?;
                    self.attrs(&f.attributes, depth + 2, subs)?;
                    self.ty_at(&f.ty, depth + 2, subs)?;
                    if let Some(e) = &f.default {
                        self.expr(e, depth + 2, subs)?;
                    }
                }
                for m in &a.methods {
                    self.item_at(m, depth + 1, subs)?;
                }
            }
            ItemKind::Enum(e) => {
                self.text(&e.name, item.span)?;
                self.generics(&e.generics, depth + 1, subs)?;
                for v in &e.variants {
                    self.node(v.span, depth + 1)?;
                    self.text(&v.name, v.span)?;
                    self.attrs(&v.attributes, depth + 2, subs)?;
                    match &v.fields {
                        VariantFields::Unit => {}
                        VariantFields::Tuple(ts) => {
                            for t in ts {
                                self.ty_at(t, depth + 2, subs)?;
                            }
                        }
                        VariantFields::Record(fs) => {
                            for f in fs {
                                self.node(f.span, depth + 2)?;
                                self.text(&f.name, f.span)?;
                                self.attrs(&f.attributes, depth + 3, subs)?;
                                self.ty_at(&f.ty, depth + 3, subs)?;
                                if let Some(e) = &f.default {
                                    self.expr(e, depth + 3, subs)?;
                                }
                            }
                        }
                    }
                }
            }
            ItemKind::Impl(i) => {
                self.generics(&i.generics, depth + 1, subs)?;
                self.bounds(&i.where_clause, depth + 1, subs)?;
                self.ty_at(&i.target, depth + 1, subs)?;
                if let Some(t) = &i.trait_type {
                    self.ty_at(t, depth + 1, subs)?;
                }
                if let Some(p) = &i.receiver_default {
                    self.param(p, depth + 1, subs)?;
                }
                for m in &i.members {
                    self.item_at(m, depth + 1, subs)?;
                }
            }
            ItemKind::Import(i) => {
                for s in &i.path {
                    self.text(s, item.span)?;
                }
                if let Some(s) = &i.alias {
                    self.text(s, item.span)?;
                }
            }
            ItemKind::Trait(t) => {
                self.text(&t.name, item.span)?;
                self.generics(&t.generics, depth + 1, subs)?;
                for bound in &t.bounds {
                    self.ty_at(bound, depth + 1, subs)?;
                }
                for member in &t.members {
                    self.item_at(member, depth + 1, subs)?;
                }
            }
            ItemKind::Const(c) => {
                self.text(&c.name, item.span)?;
                if let Some(ty) = &c.ty {
                    self.ty_at(ty, depth + 1, subs)?;
                }
                if let Some(value) = &c.value {
                    self.expr(value, depth + 1, subs)?;
                }
            }
            ItemKind::Extern { abi, function } => {
                self.text(abi, item.span)?;
                self.function(function, depth + 1, subs)?;
            }
            ItemKind::Meta(meta) => match meta {
                MetaItem::If {
                    condition,
                    then_items,
                    else_items,
                } => {
                    self.expr(condition, depth + 1, subs)?;
                    for item in then_items.iter().chain(else_items) {
                        self.item_at(item, depth + 1, subs)?;
                    }
                }
                MetaItem::For {
                    binding,
                    iterable,
                    items,
                } => {
                    self.text(binding, item.span)?;
                    self.expr(iterable, depth + 1, subs)?;
                    for item in items {
                        self.item_at(item, depth + 1, subs)?;
                    }
                }
                MetaItem::Emit(value) => self.expr(value, depth + 1, subs)?,
            },
        }
        Ok(())
    }
    fn pattern(&mut self, p: &Pattern, depth: usize) -> R<()> {
        self.node(p.span, depth)?;
        match &p.kind {
            PatternKind::Name(s) => self.text(s, p.span)?,
            PatternKind::Tuple(ps) => {
                for p in ps {
                    self.pattern(p, depth + 1)?;
                }
            }
            PatternKind::Constructor { path, arguments } => {
                for s in path {
                    self.text(s, p.span)?;
                }
                for p in arguments {
                    self.pattern(p, depth + 1)?;
                }
            }
            PatternKind::Reference { pattern, .. } => self.pattern(pattern, depth + 1)?,
            PatternKind::Literal(l) => self.literal(l, p.span)?,
            PatternKind::Wildcard => {}
        }
        Ok(())
    }
    fn literal(&mut self, l: &Literal, span: Span) -> R<()> {
        match l {
            Literal::Bool(_) => Ok(()),
            Literal::Integer(s) | Literal::Float(s) | Literal::String(s) | Literal::Char(s) => {
                self.text(s, span)
            }
        }
    }
    fn condition(
        &mut self,
        c: &Condition,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match c {
            Condition::Expression(e) => self.expr(e, depth, subs),
            Condition::Let { pattern, value } => {
                self.pattern(pattern, depth)?;
                self.expr(value, depth, subs)
            }
        }
    }
    fn block(
        &mut self,
        b: &Block,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.node(b.span, depth)?;
        for s in &b.statements {
            self.statement(s, depth + 1, subs)?;
        }
        Ok(())
    }
    fn statement(
        &mut self,
        s: &Stmt,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        self.node(s.span, depth)?;
        match &s.kind {
            StmtKind::Binding {
                pattern, ty, value, ..
            } => {
                self.pattern(pattern, depth + 1)?;
                if let Some(t) = ty {
                    self.ty_at(t, depth + 1, subs)?;
                }
                self.expr(value, depth + 1, subs)?;
            }
            StmtKind::Return(Some(e)) | StmtKind::Yield(e) | StmtKind::Expression(e) => {
                self.expr(e, depth + 1, subs)?
            }
            StmtKind::Assign { target, value, .. } => {
                self.expr(target, depth + 1, subs)?;
                self.expr(value, depth + 1, subs)?;
            }
            StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                self.condition(condition, depth + 1, subs)?;
                self.block(then_body, depth + 1, subs)?;
                match else_body {
                    Some(ElseBranch::Block(b)) => self.block(b, depth + 1, subs)?,
                    Some(ElseBranch::If(s)) => self.statement(s, depth + 1, subs)?,
                    None => {}
                }
            }
            StmtKind::While { condition, body } => {
                self.condition(condition, depth + 1, subs)?;
                self.block(body, depth + 1, subs)?;
            }
            StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                self.pattern(pattern, depth + 1)?;
                self.expr(iterable, depth + 1, subs)?;
                self.block(body, depth + 1, subs)?;
            }
            StmtKind::Loop(b) | StmtKind::Unsafe(b) | StmtKind::Defer(b) => {
                self.block(b, depth + 1, subs)?
            }
            StmtKind::With { name, value, body } => {
                self.text(name, s.span)?;
                self.expr(value, depth + 1, subs)?;
                self.block(body, depth + 1, subs)?;
            }
            StmtKind::Match { value, arms } => {
                self.expr(value, depth + 1, subs)?;
                for a in arms {
                    self.pattern(&a.pattern, depth + 1)?;
                    self.arrow(&a.body, depth + 1, subs)?;
                }
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Meta(_) => {
                return Err(unsupported(s.span, "generic expansion of meta statement"));
            }
        }
        Ok(())
    }
    fn arrow(
        &mut self,
        b: &ArrowBody,
        depth: usize,
        subs: Option<&BTreeMap<String, TypeRef>>,
    ) -> R<()> {
        match b {
            ArrowBody::Block(b) => self.block(b, depth, subs),
            ArrowBody::Expression(e) => self.expr(e, depth, subs),
        }
    }
    fn expr(&mut self, e: &Expr, depth: usize, subs: Option<&BTreeMap<String, TypeRef>>) -> R<()> {
        self.node(e.span, depth)?;
        match &e.kind {
            ExprKind::Literal(l) => self.literal(l, e.span)?,
            ExprKind::Name(s) | ExprKind::ReceiverMember(s) => self.text(s, e.span)?,
            ExprKind::Group(x)
            | ExprKind::Try(x)
            | ExprKind::MetaEval(x)
            | ExprKind::Unary { operand: x, .. } => self.expr(x, depth + 1, subs)?,
            ExprKind::Tuple(xs) | ExprKind::Array(xs) => {
                for x in xs {
                    self.expr(x, depth + 1, subs)?;
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr(left, depth + 1, subs)?;
                self.expr(right, depth + 1, subs)?;
            }
            ExprKind::Pipeline { input, target } => {
                self.expr(input, depth + 1, subs)?;
                self.expr(target, depth + 1, subs)?;
            }
            ExprKind::Member { object, member } => {
                self.expr(object, depth + 1, subs)?;
                if let MemberName::Name(s) = member {
                    self.text(s, e.span)?;
                }
            }
            ExprKind::TypeApply { callee, arguments } => {
                self.expr(callee, depth + 1, subs)?;
                for a in arguments {
                    self.arg(a, depth + 1, subs)?;
                }
            }
            ExprKind::Call { callee, arguments } => {
                self.expr(callee, depth + 1, subs)?;
                for a in arguments {
                    self.expr(&a.value, depth + 1, subs)?;
                }
            }
            ExprKind::Index { object, index } => {
                self.expr(object, depth + 1, subs)?;
                self.expr(index, depth + 1, subs)?;
            }
            ExprKind::Slice { object, start, end } => {
                self.expr(object, depth + 1, subs)?;
                for x in [start, end].into_iter().flatten() {
                    self.expr(x, depth + 1, subs)?;
                }
            }
            ExprKind::Record { ty, base, fields } => {
                for x in [ty, base].into_iter().flatten() {
                    self.expr(x, depth + 1, subs)?;
                }
                for f in fields {
                    self.text(&f.name, f.span)?;
                    if let Some(x) = &f.value {
                        self.expr(x, depth + 1, subs)?;
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
                    self.node(p.span, depth + 1)?;
                    self.text(&p.name, p.span)?;
                    if let Some(t) = &p.ty {
                        self.ty_at(t, depth + 1, subs)?;
                    }
                }
                if let Some(t) = return_type {
                    self.ty_at(t, depth + 1, subs)?;
                }
                self.arrow(body, depth + 1, subs)?;
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expr(condition, depth + 1, subs)?;
                self.expr(then_value, depth + 1, subs)?;
                self.expr(else_value, depth + 1, subs)?;
            }
            ExprKind::MetaBlock(b) => self.block(b, depth + 1, subs)?,
            ExprKind::SyntaxIsland { .. } => {
                return Err(unsupported(
                    e.span,
                    "generic expansion of unexecuted syntax island",
                ));
            }
        }
        Ok(())
    }
}
