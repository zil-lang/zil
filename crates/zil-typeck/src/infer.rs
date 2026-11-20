//! infer: name resolution inside bodies and type inference, producing THIR.
//!
//! Inference is equality-based: every expression gets a type, possibly an
//! inference variable, and constraints are solved by unification as they
//! arise. Unsuffixed literals carry kinded variables that default at the
//! end. `finalize` then zonks every type and applies the typing rules that
//! need closed types (operator domains, literal ranges, bounds).

use crate::diag;
use crate::items::{FnDecl, ItemRef, Items, VariantShape, resolve_type, scalar_type};
use crate::literal;
use crate::thir::*;
use crate::ty::{AdtRef, Ty, TyData, Types, VarKind};
use zil_base::{Diagnostic, Span};
use zil_ir::body::{AdtKind, BinOp, IntTy, UnOp};
use zil_syntax::ast;

pub struct FnCx<'c, 'a> {
    items: &'c Items<'a>,
    types: &'c mut Types,
    diags: &'c mut Vec<Diagnostic>,
    decl: &'c FnDecl<'a>,
    generic_names: Vec<String>,
    locals: Vec<LocalInfo>,
    scopes: Vec<Vec<(String, LocalId)>>,
    loop_depth: u32,
}

/// Infers one function body against its declared signature.
pub fn infer_fn<'a>(
    items: &Items<'a>,
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
    f: FnRef,
) -> Option<ThirFn> {
    let decl = items.func(f);
    let errors_before = diags.len();
    let mut cx = FnCx {
        items,
        types,
        diags,
        decl,
        generic_names: decl.generics.iter().map(|g| g.name.clone()).collect(),
        locals: Vec::new(),
        scopes: vec![Vec::new()],
        loop_depth: 0,
    };
    let mut params = Vec::new();
    for p in &decl.params {
        params.push(cx.declare(&p.name, p.ty, false, p.span));
    }
    let body = match &decl.ast.body {
        ast::FunctionBody::Block(b) => cx.block(b),
        ast::FunctionBody::Expression(e) => {
            let value = cx.infer(e);
            cx.coerce(&value, decl.result, e.span);
            let span = e.span;
            let ty = cx.types.never();
            Expr {
                kind: ExprKind::Return(Some(Box::new(value))),
                ty,
                span,
            }
        }
        ast::FunctionBody::Declaration => {
            cx.diags
                .push(diag::unsupported("body-less declarations", decl.ast.span));
            return None;
        }
    };
    let unit = cx.types.unit();
    if !cx.types.is_never(body.ty) && decl.result != unit {
        cx.diags.push(diag::error(
            diag::MISSING_RETURN,
            decl.ast.span,
            format!("`{}` can finish without returning a value", decl.name),
        ));
    }
    let mut thir = ThirFn {
        locals: cx.locals,
        params,
        body,
    };
    if diags.len() > errors_before {
        return None;
    }
    Finalizer {
        items,
        types,
        diags,
        generics: &decl.generics,
        reported_inference: false,
    }
    .function(&mut thir);
    (diags.len() == errors_before).then_some(thir)
}

impl<'c, 'a> FnCx<'c, 'a> {
    fn declare(&mut self, name: &str, ty: Ty, mutable: bool, span: Span) -> LocalId {
        let id = LocalId(self.locals.len() as u32);
        self.locals.push(LocalInfo {
            name: name.to_string(),
            ty,
            mutable,
            span,
        });
        if let Some(scope) = self.scopes.last_mut() {
            scope.push((name.to_string(), id));
        }
        id
    }

    fn lookup_local(&self, name: &str) -> Option<LocalId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| s.iter().rev().find(|(n, _)| n == name).map(|(_, l)| *l))
    }

    fn error(&mut self, code: &str, span: Span, message: impl Into<String>) {
        self.diags.push(diag::error(code, span, message));
    }

    fn unsupported(&mut self, feature: &str, span: Span) {
        self.diags.push(diag::unsupported(feature, span));
    }

    fn err_expr(&mut self, span: Span) -> Expr {
        let ty = self.types.error();
        Expr {
            kind: ExprKind::Lit(Lit::Unit),
            ty,
            span,
        }
    }

    fn coerce(&mut self, value: &Expr, expected: Ty, span: Span) {
        if self.types.coerce(value.ty, expected).is_err() {
            self.error(
                diag::TYPE_MISMATCH,
                span,
                "the value does not have the expected type",
            );
        }
    }

    /// Dereferences an address-typed base of a member or index access.
    fn auto_deref(&mut self, mut base: Expr) -> Expr {
        loop {
            let bt = self.types.shallow(base.ty);
            let TyData::Ref { pointee, .. } = *self.types.data(bt) else {
                return base;
            };
            let span = base.span;
            base = Expr {
                kind: ExprKind::Deref(Box::new(base)),
                ty: pointee,
                span,
            };
        }
    }

    /// Coerces an argument at a call site. An exclusive address held in a
    /// place is reborrowed (`&mut *p`) instead of moved, and an exclusive
    /// address converts to a shared one (`&*p`); both are new loans the
    /// kernel checks like any other.
    fn coerce_arg(&mut self, value: Expr, expected: Ty, span: Span) -> Expr {
        let et = self.types.shallow(expected);
        let vt = self.types.shallow(value.ty);
        let (
            TyData::Ref { mutable: want, .. },
            TyData::Ref {
                mutable: true,
                pointee,
            },
        ) = (self.types.data(et).clone(), self.types.data(vt).clone())
        else {
            self.coerce(&value, expected, span);
            return value;
        };
        if want && !is_place(&value.kind) {
            self.coerce(&value, expected, span);
            return value;
        }
        let vspan = value.span;
        let target = Expr {
            kind: ExprKind::Deref(Box::new(value)),
            ty: pointee,
            span: vspan,
        };
        let ty = self.types.intern(TyData::Ref {
            mutable: want,
            pointee,
        });
        let out = Expr {
            kind: ExprKind::Borrow {
                mutable: want,
                place: Box::new(target),
            },
            ty,
            span: vspan,
        };
        self.coerce(&out, expected, span);
        out
    }

    fn unify(&mut self, a: Ty, b: Ty, span: Span) {
        if self.types.unify(a, b).is_err() {
            self.error(diag::TYPE_MISMATCH, span, "mismatched types");
        }
    }

    /// Least type of two branches: `Never` yields to the other side.
    fn join(&mut self, a: Ty, b: Ty, span: Span) -> Ty {
        if self.types.is_never(a) {
            return b;
        }
        if self.types.is_never(b) {
            return a;
        }
        self.unify(a, b, span);
        a
    }

    fn resolve_written(&mut self, t: &ast::TypeRef) -> Ty {
        match resolve_type(t, &self.generic_names, &self.items.names, self.types) {
            Ok(ty) => ty,
            Err(d) => {
                self.diags.push(d);
                self.types.error()
            }
        }
    }

    fn fresh_args(&mut self, adt: AdtRef) -> Vec<Ty> {
        let n = self.items.adt(adt).generics.len();
        (0..n).map(|_| self.types.fresh(VarKind::General)).collect()
    }

    fn adt_ty(&mut self, adt: AdtRef, args: &[Ty]) -> Ty {
        self.types.intern(TyData::Adt {
            adt,
            args: args.to_vec(),
        })
    }

    fn field_ty(&mut self, adt: AdtRef, variant: u32, field: u32, args: &[Ty]) -> Ty {
        let declared = self.items.adt(adt).variants[variant as usize].fields[field as usize].ty;
        self.types.subst(declared, args)
    }

    /// `Enum.Variant` or `Record` written as a path.
    fn resolve_variant_path(&mut self, path: &[String], span: Span) -> Option<(AdtRef, u32)> {
        match path {
            [name] => match self.items.names.get(name) {
                Some(ItemRef::Adt(adt)) if self.items.adt(*adt).kind != AdtKind::Enum => {
                    Some((*adt, 0))
                }
                Some(ItemRef::Adt(_)) => {
                    self.error(diag::ENUM_VARIANT, span, "an enum needs a variant name");
                    None
                }
                Some(ItemRef::Fn(_)) | None => {
                    self.error(
                        diag::NAME_UNRESOLVED,
                        span,
                        format!("unknown type `{name}`"),
                    );
                    None
                }
            },
            [enum_name, variant] => match self.items.names.get(enum_name) {
                Some(ItemRef::Adt(adt)) if self.items.adt(*adt).kind == AdtKind::Enum => {
                    match self.items.adt(*adt).variant_named(variant) {
                        Some(v) => Some((*adt, v)),
                        None => {
                            self.error(
                                diag::ENUM_VARIANT,
                                span,
                                format!("`{enum_name}` has no variant `{variant}`"),
                            );
                            None
                        }
                    }
                }
                Some(ItemRef::Adt(_) | ItemRef::Fn(_)) | None => {
                    self.error(
                        diag::NAME_UNRESOLVED,
                        span,
                        format!("unknown enum `{enum_name}`"),
                    );
                    None
                }
            },
            _ => {
                self.unsupported("qualified paths", span);
                None
            }
        }
    }

    /// The written path of an expression made only of names and members.
    fn expr_path(e: &ast::Expr) -> Option<Vec<String>> {
        if let ast::ExprKind::Name(n) = &e.kind {
            return Some(vec![n.clone()]);
        }
        if let ast::ExprKind::Group(inner) = &e.kind {
            return Self::expr_path(inner);
        }
        let ast::ExprKind::Member {
            object,
            member: ast::MemberName::Name(m),
        } = &e.kind
        else {
            return None;
        };
        let mut p = Self::expr_path(object)?;
        p.push(m.clone());
        Some(p)
    }

    /// Whether `path` starts with a local binding (and so is a value).
    fn path_is_value(&self, path: &[String]) -> bool {
        path.first().is_some_and(|n| self.lookup_local(n).is_some())
    }

    // ---- blocks and statements ---------------------------------------------

    fn block(&mut self, b: &ast::Block) -> Expr {
        self.scopes.push(Vec::new());
        let mut stmts = Vec::new();
        let mut diverges = false;
        for s in &b.statements {
            let stmt = self.stmt(s);
            let ty = match &stmt {
                Some(Stmt::Expr(e)) => Some(e.ty),
                Some(Stmt::Let { init, .. }) => Some(init.ty),
                None => None,
            };
            if ty.is_some_and(|t| self.types.is_never(t)) {
                diverges = true;
            }
            if let Some(stmt) = stmt {
                stmts.push(stmt);
            }
        }
        self.scopes.pop();
        let ty = if diverges {
            self.types.never()
        } else {
            self.types.unit()
        };
        Expr {
            kind: ExprKind::Block(stmts),
            ty,
            span: b.span,
        }
    }

    fn stmt(&mut self, s: &ast::Stmt) -> Option<Stmt> {
        let span = s.span;
        let never = self.types.never();
        let unit = self.types.unit();
        let e = |kind, ty| Some(Stmt::Expr(Expr { kind, ty, span }));
        match &s.kind {
            ast::StmtKind::Binding {
                mutable,
                pattern,
                ty,
                value,
            } => {
                let init = self.infer(value);
                let declared = match ty {
                    Some(t) => {
                        let t = self.resolve_written(t);
                        self.coerce(&init, t, value.span);
                        t
                    }
                    None => init.ty,
                };
                let pattern = self.pattern(pattern, declared, *mutable);
                if !self.irrefutable(&pattern) {
                    self.error(
                        diag::PATTERN_REFUTABLE,
                        pattern.span,
                        "a binding pattern must match every value",
                    );
                }
                Some(Stmt::Let { pattern, init })
            }
            ast::StmtKind::Return(value) => {
                let result = self.decl.result;
                let value = match value {
                    Some(v) => {
                        let v = self.infer(v);
                        self.coerce(&v, result, span);
                        Some(Box::new(v))
                    }
                    None => {
                        if result != unit {
                            self.error(diag::TYPE_MISMATCH, span, "a value must be returned");
                        }
                        None
                    }
                };
                e(ExprKind::Return(value), never)
            }
            ast::StmtKind::Break | ast::StmtKind::Continue => {
                if self.loop_depth == 0 {
                    self.error(
                        diag::LOOP_CONTROL,
                        span,
                        "`break`/`continue` outside a loop",
                    );
                }
                let kind = if matches!(s.kind, ast::StmtKind::Break) {
                    ExprKind::Break
                } else {
                    ExprKind::Continue
                };
                e(kind, never)
            }
            ast::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => Some(Stmt::Expr(self.if_stmt(
                condition,
                then_body,
                else_body.as_ref(),
                span,
            ))),
            ast::StmtKind::While { condition, body } => {
                self.loop_depth += 1;
                let out = match condition {
                    ast::Condition::Expression(c) => {
                        let cond = self.infer(c);
                        let b = self.types.bool();
                        self.coerce(&cond, b, c.span);
                        let body = self.block(body);
                        ExprKind::While {
                            cond: Box::new(cond),
                            body: Box::new(body),
                        }
                    }
                    ast::Condition::Let { pattern, value } => {
                        // loop { match value { pattern => body, _ => break } }
                        let scrutinee = self.infer(value);
                        self.scopes.push(Vec::new());
                        let pat = self.pattern(pattern, scrutinee.ty, false);
                        let body = self.block(body);
                        self.scopes.pop();
                        let wild = Pat {
                            kind: PatKind::Wild,
                            ty: scrutinee.ty,
                            span: pattern.span,
                        };
                        let brk = Expr {
                            kind: ExprKind::Break,
                            ty: never,
                            span,
                        };
                        let m = Expr {
                            kind: ExprKind::Match {
                                scrutinee: Box::new(scrutinee),
                                arms: vec![
                                    Arm {
                                        pattern: pat,
                                        body,
                                        span,
                                    },
                                    Arm {
                                        pattern: wild,
                                        body: brk,
                                        span,
                                    },
                                ],
                            },
                            ty: unit,
                            span,
                        };
                        ExprKind::Loop(Box::new(m))
                    }
                };
                self.loop_depth -= 1;
                e(out, unit)
            }
            ast::StmtKind::Loop(body) => {
                self.loop_depth += 1;
                let body = self.block(body);
                self.loop_depth -= 1;
                // A loop without `break` never finishes.
                let ty = if contains_break(&body) { unit } else { never };
                e(ExprKind::Loop(Box::new(body)), ty)
            }
            ast::StmtKind::Match { value, arms } => {
                let scrutinee = self.infer(value);
                let mut out = Vec::new();
                let mut all_never = !arms.is_empty();
                for arm in arms {
                    self.scopes.push(Vec::new());
                    let pattern = self.pattern(&arm.pattern, scrutinee.ty, false);
                    let body = match &arm.body {
                        ast::ArrowBody::Block(b) => self.block(b),
                        ast::ArrowBody::Expression(x) => self.infer(x),
                    };
                    self.scopes.pop();
                    all_never &= self.types.is_never(body.ty);
                    out.push(Arm {
                        pattern,
                        body,
                        span: arm.span,
                    });
                }
                let ty = if all_never { never } else { unit };
                e(
                    ExprKind::Match {
                        scrutinee: Box::new(scrutinee),
                        arms: out,
                    },
                    ty,
                )
            }
            ast::StmtKind::Assign { target, op, value } => {
                let place = self.infer(target);
                self.check_assignable(&place);
                let rhs = self.infer(value);
                let bin = match op {
                    ast::AssignOp::Set => None,
                    ast::AssignOp::Add => Some(BinOp::Add),
                    ast::AssignOp::Subtract => Some(BinOp::Sub),
                    ast::AssignOp::Multiply => Some(BinOp::Mul),
                    ast::AssignOp::Divide => Some(BinOp::Div),
                    ast::AssignOp::Remainder => Some(BinOp::Rem),
                    ast::AssignOp::BitAnd => Some(BinOp::BitAnd),
                    ast::AssignOp::BitOr => Some(BinOp::BitOr),
                    ast::AssignOp::BitXor => Some(BinOp::BitXor),
                };
                self.coerce(&rhs, place.ty, value.span);
                e(
                    ExprKind::Assign {
                        place: Box::new(place),
                        op: bin,
                        value: Box::new(rhs),
                    },
                    unit,
                )
            }
            ast::StmtKind::Expression(x) => Some(Stmt::Expr(self.infer(x))),
            ast::StmtKind::Yield(_) => {
                self.unsupported("yield", span);
                None
            }
            ast::StmtKind::Defer(_) => {
                self.unsupported("defer", span);
                None
            }
            ast::StmtKind::For { .. } => {
                self.unsupported("for loops", span);
                None
            }
            ast::StmtKind::With { .. } => {
                self.unsupported("with statements", span);
                None
            }
            ast::StmtKind::Unsafe(_) => {
                self.unsupported("unsafe blocks", span);
                None
            }
            ast::StmtKind::Meta(_) => {
                self.unsupported("meta statements", span);
                None
            }
        }
    }

    fn if_stmt(
        &mut self,
        condition: &ast::Condition,
        then_body: &ast::Block,
        else_body: Option<&ast::ElseBranch>,
        span: Span,
    ) -> Expr {
        let unit = self.types.unit();
        let otherwise = else_body.map(|b| match b {
            ast::ElseBranch::Block(b) => self.block(b),
            ast::ElseBranch::If(s) => match self.stmt(s) {
                Some(Stmt::Expr(e)) => e,
                Some(Stmt::Let { .. }) | None => self.err_expr(s.span),
            },
        });
        match condition {
            ast::Condition::Expression(c) => {
                let cond = self.infer(c);
                let b = self.types.bool();
                self.coerce(&cond, b, c.span);
                let then = self.block(then_body);
                let ty = match &otherwise {
                    Some(o) if self.types.is_never(then.ty) && self.types.is_never(o.ty) => {
                        self.types.never()
                    }
                    Some(_) | None => unit,
                };
                Expr {
                    kind: ExprKind::If {
                        cond: Box::new(cond),
                        then: Box::new(then),
                        otherwise: otherwise.map(Box::new),
                    },
                    ty,
                    span,
                }
            }
            ast::Condition::Let { pattern, value } => {
                let scrutinee = self.infer(value);
                self.scopes.push(Vec::new());
                let pat = self.pattern(pattern, scrutinee.ty, false);
                let then = self.block(then_body);
                self.scopes.pop();
                let else_expr = match otherwise {
                    Some(o) => o,
                    None => Expr {
                        kind: ExprKind::Block(Vec::new()),
                        ty: unit,
                        span,
                    },
                };
                let ty = if self.types.is_never(then.ty) && self.types.is_never(else_expr.ty) {
                    self.types.never()
                } else {
                    unit
                };
                let wild = Pat {
                    kind: PatKind::Wild,
                    ty: scrutinee.ty,
                    span: pattern.span,
                };
                Expr {
                    kind: ExprKind::Match {
                        scrutinee: Box::new(scrutinee),
                        arms: vec![
                            Arm {
                                pattern: pat,
                                body: then,
                                span,
                            },
                            Arm {
                                pattern: wild,
                                body: else_expr,
                                span,
                            },
                        ],
                    },
                    ty,
                    span,
                }
            }
        }
    }

    fn check_assignable(&mut self, place: &Expr) {
        self.check_mutable_place(place, "assigned");
    }

    /// A place written or borrowed exclusively must be reached through a
    /// `var` binding or through an exclusive address. Binding mutability is
    /// a name fact the IR does not carry, so typeck owns this rule; the
    /// address case repeats the type rule for a source-level diagnostic.
    fn check_mutable_place(&mut self, place: &Expr, what: &str) {
        let mut cur = place;
        loop {
            match &cur.kind {
                ExprKind::Local(l) => {
                    if !self.locals[l.0 as usize].mutable {
                        let name = self.locals[l.0 as usize].name.clone();
                        self.error(
                            diag::ASSIGN_IMMUTABLE,
                            place.span,
                            format!("`{name}` is not declared with `var` and cannot be {what}"),
                        );
                    }
                    return;
                }
                ExprKind::Field { base, .. }
                | ExprKind::TupleField { base, .. }
                | ExprKind::Index { base, .. } => cur = base,
                ExprKind::Deref(base) => {
                    let bt = self.types.shallow(base.ty);
                    if let TyData::Ref { mutable: false, .. } = self.types.data(bt) {
                        self.error(
                            diag::ASSIGN_IMMUTABLE,
                            place.span,
                            format!("a place behind a shared address cannot be {what}"),
                        );
                    }
                    return;
                }
                ExprKind::Borrow { .. }
                | ExprKind::Lit(_)
                | ExprKind::Unary { .. }
                | ExprKind::Binary { .. }
                | ExprKind::Logical { .. }
                | ExprKind::Convert { .. }
                | ExprKind::Call { .. }
                | ExprKind::Adt { .. }
                | ExprKind::Tuple(_)
                | ExprKind::Array(_)
                | ExprKind::If { .. }
                | ExprKind::Match { .. }
                | ExprKind::Block(_)
                | ExprKind::Loop(_)
                | ExprKind::While { .. }
                | ExprKind::Break
                | ExprKind::Continue
                | ExprKind::Return(_)
                | ExprKind::Assign { .. } => {
                    self.error(
                        diag::ASSIGN_TARGET,
                        place.span,
                        "only a place can be assigned",
                    );
                    return;
                }
            }
        }
    }

    // ---- patterns ----------------------------------------------------------

    fn pattern(&mut self, p: &ast::Pattern, expected: Ty, mutable: bool) -> Pat {
        let span = p.span;
        let kind = match &p.kind {
            ast::PatternKind::Name(n) => PatKind::Binding(self.declare(n, expected, mutable, span)),
            ast::PatternKind::Wildcard => PatKind::Wild,
            ast::PatternKind::Literal(l) => match self.literal(l, span) {
                Some((lit, ty)) => {
                    self.unify(ty, expected, span);
                    PatKind::Lit(lit)
                }
                None => PatKind::Wild,
            },
            ast::PatternKind::Tuple(items) => {
                let tys: Vec<Ty> = items
                    .iter()
                    .map(|_| self.types.fresh(VarKind::General))
                    .collect();
                let t = self.types.intern(TyData::Tuple(tys.clone()));
                self.unify(t, expected, span);
                PatKind::Tuple(
                    items
                        .iter()
                        .zip(tys)
                        .map(|(i, t)| self.pattern(i, t, mutable))
                        .collect(),
                )
            }
            ast::PatternKind::Constructor { path, arguments } => {
                let Some((adt, variant)) = self.resolve_variant_path(path, span) else {
                    return self.wild(expected, span);
                };
                let decl = &self.items.adt(adt).variants[variant as usize];
                let shape = decl.shape;
                let n = decl.fields.len();
                if shape == VariantShape::Record || n != arguments.len() {
                    self.error(
                        diag::MATCH_ARITY,
                        span,
                        "the pattern does not match the variant's fields",
                    );
                    return self.wild(expected, span);
                }
                let args = self.fresh_args(adt);
                let t = self.adt_ty(adt, &args);
                self.unify(t, expected, span);
                let fields = arguments
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        let ft = self.field_ty(adt, variant, i as u32, &args);
                        self.pattern(a, ft, mutable)
                    })
                    .collect();
                PatKind::Adt {
                    adt,
                    variant,
                    args,
                    fields,
                }
            }
            ast::PatternKind::RecordConstructor { path, fields } => {
                let Some((adt, variant)) = self.resolve_variant_path(path, span) else {
                    return self.wild(expected, span);
                };
                let args = self.fresh_args(adt);
                let t = self.adt_ty(adt, &args);
                self.unify(t, expected, span);
                let decl_fields: Vec<String> = self.items.adt(adt).variants[variant as usize]
                    .fields
                    .iter()
                    .map(|f| f.name.clone())
                    .collect();
                let mut slots: Vec<Option<Pat>> = vec![None; decl_fields.len()];
                for f in fields {
                    let Some(i) = decl_fields.iter().position(|n| *n == f.name) else {
                        self.error(
                            diag::RECORD_UNKNOWN_FIELD,
                            f.span,
                            format!("no field `{}`", f.name),
                        );
                        continue;
                    };
                    if slots[i].is_some() {
                        self.error(
                            diag::RECORD_DUPLICATE_FIELD,
                            f.span,
                            format!("field `{}` appears more than once", f.name),
                        );
                        continue;
                    }
                    let ft = self.field_ty(adt, variant, i as u32, &args);
                    slots[i] = Some(self.pattern(&f.pattern, ft, mutable));
                }
                let mut out = Vec::new();
                for (i, s) in slots.into_iter().enumerate() {
                    match s {
                        Some(p) => out.push(p),
                        None => {
                            self.error(
                                diag::RECORD_MISSING_FIELD,
                                span,
                                format!("field `{}` is missing from the pattern", decl_fields[i]),
                            );
                            let ft = self.field_ty(adt, variant, i as u32, &args);
                            out.push(self.wild(ft, span));
                        }
                    }
                }
                PatKind::Adt {
                    adt,
                    variant,
                    args,
                    fields: out,
                }
            }
            ast::PatternKind::Reference { .. } => {
                self.unsupported("reference patterns", span);
                PatKind::Wild
            }
            ast::PatternKind::ErrorArm { .. } => {
                self.unsupported("error arm patterns", span);
                PatKind::Wild
            }
        };
        Pat {
            kind,
            ty: expected,
            span,
        }
    }

    fn wild(&mut self, ty: Ty, span: Span) -> Pat {
        Pat {
            kind: PatKind::Wild,
            ty,
            span,
        }
    }

    fn irrefutable(&self, p: &Pat) -> bool {
        match &p.kind {
            PatKind::Wild | PatKind::Binding(_) => true,
            PatKind::Lit(_) => false,
            PatKind::Tuple(items) => items.iter().all(|i| self.irrefutable(i)),
            PatKind::Adt { adt, fields, .. } => {
                self.items.adt(*adt).variants.len() == 1
                    && fields.iter().all(|f| self.irrefutable(f))
            }
        }
    }

    // ---- expressions -------------------------------------------------------

    fn literal(&mut self, l: &ast::Literal, span: Span) -> Option<(Lit, Ty)> {
        match l {
            ast::Literal::Integer(text) => {
                let Some((v, suffix)) = literal::parse_int(text) else {
                    self.error(diag::LITERAL_RANGE, span, "malformed integer literal");
                    return None;
                };
                let Ok(v) = i128::try_from(v) else {
                    self.error(diag::LITERAL_RANGE, span, "integer literal out of range");
                    return None;
                };
                let ty = match suffix {
                    Some(t) => self.types.int(t),
                    None => self.types.fresh(VarKind::Int),
                };
                Some((Lit::Int(v), ty))
            }
            ast::Literal::Float(text) => {
                let Some((v, suffix)) = literal::parse_float(text) else {
                    self.error(diag::FLOAT_LITERAL, span, "malformed float literal");
                    return None;
                };
                let ty = match suffix {
                    Some(t) => self.types.intern(TyData::Float(t)),
                    None => self.types.fresh(VarKind::Float),
                };
                Some((Lit::Float(v), ty))
            }
            ast::Literal::Bool(b) => {
                let ty = self.types.bool();
                Some((Lit::Bool(*b), ty))
            }
            ast::Literal::String(_) => {
                self.unsupported("string literals", span);
                None
            }
            ast::Literal::Char(_) => {
                self.unsupported("character literals", span);
                None
            }
        }
    }

    pub fn infer(&mut self, e: &ast::Expr) -> Expr {
        let span = e.span;
        let mk = |kind, ty| Expr { kind, ty, span };
        match &e.kind {
            ast::ExprKind::Literal(l) => match self.literal(l, span) {
                Some((lit, ty)) => mk(ExprKind::Lit(lit), ty),
                None => self.err_expr(span),
            },
            ast::ExprKind::Name(n) => {
                if let Some(l) = self.lookup_local(n) {
                    let ty = self.locals[l.0 as usize].ty;
                    return mk(ExprKind::Local(l), ty);
                }
                match self.items.names.get(n) {
                    Some(ItemRef::Fn(_)) => {
                        self.unsupported("function values", span);
                        self.err_expr(span)
                    }
                    Some(ItemRef::Adt(_)) => {
                        self.error(diag::NAME_UNRESOLVED, span, format!("`{n}` is a type"));
                        self.err_expr(span)
                    }
                    None => {
                        self.error(diag::NAME_UNRESOLVED, span, format!("unknown name `{n}`"));
                        self.err_expr(span)
                    }
                }
            }
            ast::ExprKind::Group(inner) => self.infer(inner),
            ast::ExprKind::Tuple(items) => {
                if items.is_empty() {
                    let ty = self.types.unit();
                    return mk(ExprKind::Lit(Lit::Unit), ty);
                }
                let items: Vec<Expr> = items.iter().map(|i| self.infer(i)).collect();
                let ty = self
                    .types
                    .intern(TyData::Tuple(items.iter().map(|i| i.ty).collect()));
                mk(ExprKind::Tuple(items), ty)
            }
            ast::ExprKind::Array(items) => {
                let element = self.types.fresh(VarKind::General);
                let items: Vec<Expr> = items
                    .iter()
                    .map(|i| {
                        let x = self.infer(i);
                        self.coerce(&x, element, i.span);
                        x
                    })
                    .collect();
                let Ok(length) = u32::try_from(items.len()) else {
                    self.diags.push(diag::limit("array literal too long", span));
                    return self.err_expr(span);
                };
                let ty = self.types.intern(TyData::Array { element, length });
                mk(ExprKind::Array(items), ty)
            }
            ast::ExprKind::Unary { op, operand } => self.unary(*op, operand, span),
            ast::ExprKind::Binary { op, left, right } => self.binary(*op, left, right, span),
            ast::ExprKind::Member { object, member } => self.member(e, object, member, span),
            ast::ExprKind::Call { callee, arguments } => self.call(callee, arguments, span),
            ast::ExprKind::Index { object, index } => {
                let base = self.infer(object);
                let base = self.auto_deref(base);
                let idx = self.infer(index);
                // An unconstrained literal index is a usize.
                if let TyData::Var(_) = self.types.data(self.types.shallow(idx.ty)) {
                    let usize_ty = self.types.int(IntTy::Usize);
                    let _ = self.types.unify(idx.ty, usize_ty);
                }
                let bt = self.types.shallow(base.ty);
                let element = match self.types.data(bt) {
                    TyData::Array { element, .. } => *element,
                    TyData::Error => self.types.error(),
                    TyData::Unit
                    | TyData::Bool
                    | TyData::Never
                    | TyData::Int(_)
                    | TyData::Float(_)
                    | TyData::Adt { .. }
                    | TyData::Tuple(_)
                    | TyData::Ref { .. }
                    | TyData::Param(_)
                    | TyData::Var(_) => {
                        self.error(diag::OPERATOR_TYPE, span, "only arrays can be indexed");
                        self.types.error()
                    }
                };
                mk(
                    ExprKind::Index {
                        base: Box::new(base),
                        index: Box::new(idx),
                    },
                    element,
                )
            }
            ast::ExprKind::Record { ty, base, fields } => {
                if let Some(b) = base {
                    self.unsupported("record update syntax", b.span);
                    return self.err_expr(span);
                }
                let Some(ty) = ty else {
                    self.error(
                        "E-RECORD-NO-CONTEXT",
                        span,
                        "a record literal needs its type name",
                    );
                    return self.err_expr(span);
                };
                let Some(path) = Self::expr_path(ty) else {
                    self.unsupported("computed record types", span);
                    return self.err_expr(span);
                };
                let Some((adt, variant)) = self.resolve_variant_path(&path, span) else {
                    return self.err_expr(span);
                };
                self.record(adt, variant, fields, span)
            }
            ast::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let cond = self.infer(condition);
                let b = self.types.bool();
                self.coerce(&cond, b, condition.span);
                let then = self.infer(then_value);
                let otherwise = self.infer(else_value);
                let ty = self.join(then.ty, otherwise.ty, span);
                mk(
                    ExprKind::If {
                        cond: Box::new(cond),
                        then: Box::new(then),
                        otherwise: Some(Box::new(otherwise)),
                    },
                    ty,
                )
            }
            ast::ExprKind::ReceiverMember(_) => {
                self.unsupported("receiver members", span);
                self.err_expr(span)
            }
            ast::ExprKind::Ascribed { .. } => {
                self.unsupported("generated type ascriptions", span);
                self.err_expr(span)
            }
            ast::ExprKind::Pipeline { .. } => {
                self.unsupported("pipelines", span);
                self.err_expr(span)
            }
            ast::ExprKind::TypeApply { .. } => {
                self.unsupported("explicit type arguments outside a call", span);
                self.err_expr(span)
            }
            ast::ExprKind::Slice { .. } => {
                self.unsupported("slices", span);
                self.err_expr(span)
            }
            ast::ExprKind::Try(_) => {
                self.unsupported("`?`", span);
                self.err_expr(span)
            }
            ast::ExprKind::Closure { .. } => {
                self.unsupported("closures", span);
                self.err_expr(span)
            }
            ast::ExprKind::MetaEval(_)
            | ast::ExprKind::MetaSplice(_)
            | ast::ExprKind::Quote { .. }
            | ast::ExprKind::MetaBlock(_)
            | ast::ExprKind::SyntaxIsland { .. } => {
                self.unsupported("compile-time evaluation", span);
                self.err_expr(span)
            }
        }
    }

    fn unary(&mut self, op: ast::UnaryOp, operand: &ast::Expr, span: Span) -> Expr {
        let mk = |kind, ty| Expr { kind, ty, span };
        match op {
            ast::UnaryOp::Negate => {
                // A negated numeric literal is one literal, so that the
                // minimum of a signed type is writable.
                if let ast::ExprKind::Literal(
                    l @ (ast::Literal::Integer(_) | ast::Literal::Float(_)),
                ) = &operand.kind
                {
                    return match self.literal(l, span) {
                        Some((Lit::Int(v), ty)) => mk(ExprKind::Lit(Lit::Int(-v)), ty),
                        Some((Lit::Float(v), ty)) => mk(ExprKind::Lit(Lit::Float(-v)), ty),
                        Some((Lit::Unit | Lit::Bool(_), _)) | None => self.err_expr(span),
                    };
                }
                let x = self.infer(operand);
                let ty = x.ty;
                mk(
                    ExprKind::Unary {
                        op: UnOp::Neg,
                        operand: Box::new(x),
                    },
                    ty,
                )
            }
            ast::UnaryOp::Not => {
                let x = self.infer(operand);
                let b = self.types.bool();
                self.coerce(&x, b, operand.span);
                mk(
                    ExprKind::Unary {
                        op: UnOp::Not,
                        operand: Box::new(x),
                    },
                    b,
                )
            }
            ast::UnaryOp::BitNot => {
                let x = self.infer(operand);
                let ty = x.ty;
                mk(
                    ExprKind::Unary {
                        op: UnOp::Not,
                        operand: Box::new(x),
                    },
                    ty,
                )
            }
            ast::UnaryOp::Dereference => {
                let x = self.infer(operand);
                let xt = self.types.shallow(x.ty);
                match self.types.data(xt).clone() {
                    TyData::Ref { pointee, .. } => mk(ExprKind::Deref(Box::new(x)), pointee),
                    TyData::Error => self.err_expr(span),
                    TyData::Var(_) => {
                        self.error(
                            diag::TYPE_INFERENCE,
                            span,
                            "the type must be known before dereferencing",
                        );
                        self.err_expr(span)
                    }
                    TyData::Unit
                    | TyData::Bool
                    | TyData::Never
                    | TyData::Int(_)
                    | TyData::Float(_)
                    | TyData::Adt { .. }
                    | TyData::Tuple(_)
                    | TyData::Array { .. }
                    | TyData::Param(_) => {
                        self.error(
                            diag::OPERATOR_TYPE,
                            span,
                            "only an address can be dereferenced",
                        );
                        self.err_expr(span)
                    }
                }
            }
            ast::UnaryOp::Borrow | ast::UnaryOp::BorrowMut => {
                let mutable = op == ast::UnaryOp::BorrowMut;
                let x = self.infer(operand);
                if mutable && is_place(&x.kind) {
                    self.check_mutable_place(&x, "borrowed exclusively");
                }
                let ty = self.types.intern(TyData::Ref {
                    mutable,
                    pointee: x.ty,
                });
                mk(
                    ExprKind::Borrow {
                        mutable,
                        place: Box::new(x),
                    },
                    ty,
                )
            }
            ast::UnaryOp::Give => {
                self.unsupported("`give`", span);
                self.err_expr(span)
            }
        }
    }

    fn binary(
        &mut self,
        op: ast::BinaryOp,
        left: &ast::Expr,
        right: &ast::Expr,
        span: Span,
    ) -> Expr {
        let lhs = self.infer(left);
        let rhs = self.infer(right);
        let bin = match op {
            ast::BinaryOp::And | ast::BinaryOp::Or => {
                let b = self.types.bool();
                self.coerce(&lhs, b, left.span);
                self.coerce(&rhs, b, right.span);
                return Expr {
                    kind: ExprKind::Logical {
                        and: op == ast::BinaryOp::And,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                    ty: b,
                    span,
                };
            }
            ast::BinaryOp::Range => {
                self.unsupported("ranges", span);
                return self.err_expr(span);
            }
            ast::BinaryOp::Multiply => BinOp::Mul,
            ast::BinaryOp::Divide => BinOp::Div,
            ast::BinaryOp::Remainder => BinOp::Rem,
            ast::BinaryOp::MultiplyWrapping => BinOp::WrappingMul,
            ast::BinaryOp::Add => BinOp::Add,
            ast::BinaryOp::Subtract => BinOp::Sub,
            ast::BinaryOp::AddWrapping => BinOp::WrappingAdd,
            ast::BinaryOp::SubtractWrapping => BinOp::WrappingSub,
            ast::BinaryOp::ShiftLeft => BinOp::Shl,
            ast::BinaryOp::ShiftRight => BinOp::Shr,
            ast::BinaryOp::BitAnd => BinOp::BitAnd,
            ast::BinaryOp::BitXor => BinOp::BitXor,
            ast::BinaryOp::BitOr => BinOp::BitOr,
            ast::BinaryOp::Equal => BinOp::Eq,
            ast::BinaryOp::NotEqual => BinOp::Ne,
            ast::BinaryOp::Less => BinOp::Lt,
            ast::BinaryOp::LessEqual => BinOp::Le,
            ast::BinaryOp::Greater => BinOp::Gt,
            ast::BinaryOp::GreaterEqual => BinOp::Ge,
        };
        self.unify(lhs.ty, rhs.ty, span);
        let ty = if bin.is_comparison() {
            self.types.bool()
        } else {
            lhs.ty
        };
        Expr {
            kind: ExprKind::Binary {
                op: bin,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            ty,
            span,
        }
    }

    fn member(
        &mut self,
        whole: &ast::Expr,
        object: &ast::Expr,
        member: &ast::MemberName,
        span: Span,
    ) -> Expr {
        // `Enum.Variant` as a value: a unit variant.
        if let Some(path) = Self::expr_path(whole)
            && !self.path_is_value(&path)
            && path.len() == 2
        {
            let Some((adt, variant)) = self.resolve_variant_path(&path, span) else {
                return self.err_expr(span);
            };
            if self.items.adt(adt).variants[variant as usize].shape != VariantShape::Unit {
                self.error(
                    diag::ENUM_ARITY,
                    span,
                    "this variant has fields and must be constructed with them",
                );
                return self.err_expr(span);
            }
            let args = self.fresh_args(adt);
            let ty = self.adt_ty(adt, &args);
            return Expr {
                kind: ExprKind::Adt {
                    adt,
                    variant,
                    args,
                    fields: Vec::new(),
                },
                ty,
                span,
            };
        }
        let base = self.infer(object);
        let base = self.auto_deref(base);
        let bt = self.types.shallow(base.ty);
        match (member, self.types.data(bt).clone()) {
            (ast::MemberName::TupleIndex(i), TyData::Tuple(items)) => {
                let Some(ty) = items.get(*i as usize).copied() else {
                    self.error(diag::UNKNOWN_FIELD, span, format!("no tuple slot {i}"));
                    return self.err_expr(span);
                };
                Expr {
                    kind: ExprKind::TupleField {
                        base: Box::new(base),
                        index: *i,
                    },
                    ty,
                    span,
                }
            }
            (ast::MemberName::Name(name), TyData::Adt { adt, args }) => {
                let decl = self.items.adt(adt);
                if decl.variants.len() != 1 || decl.kind == AdtKind::Enum {
                    self.error(
                        diag::UNKNOWN_FIELD,
                        span,
                        "fields of an enum are reached through a pattern",
                    );
                    return self.err_expr(span);
                }
                let Some(field) = decl.variants[0].fields.iter().position(|f| f.name == *name)
                else {
                    self.error(diag::UNKNOWN_FIELD, span, format!("no field `{name}`"));
                    return self.err_expr(span);
                };
                let ty = self.field_ty(adt, 0, field as u32, &args);
                Expr {
                    kind: ExprKind::Field {
                        base: Box::new(base),
                        adt,
                        variant: 0,
                        field: field as u32,
                        args,
                    },
                    ty,
                    span,
                }
            }
            (_, TyData::Error) => self.err_expr(span),
            (_, TyData::Var(_)) => {
                self.error(
                    diag::TYPE_INFERENCE,
                    span,
                    "the type must be known before selecting a member",
                );
                self.err_expr(span)
            }
            (ast::MemberName::Constrained { .. }, _) => {
                self.unsupported("reflected members", span);
                self.err_expr(span)
            }
            (ast::MemberName::Name(_) | ast::MemberName::TupleIndex(_), _) => {
                self.error(diag::UNKNOWN_FIELD, span, "this type has no such member");
                self.err_expr(span)
            }
        }
    }

    fn record(
        &mut self,
        adt: AdtRef,
        variant: u32,
        fields: &[ast::RecordField],
        span: Span,
    ) -> Expr {
        let decl_fields: Vec<String> = self.items.adt(adt).variants[variant as usize]
            .fields
            .iter()
            .map(|f| f.name.clone())
            .collect();
        if self.items.adt(adt).variants[variant as usize].shape != VariantShape::Record {
            self.error(
                diag::ENUM_ARITY,
                span,
                "this variant is not a record variant",
            );
            return self.err_expr(span);
        }
        let args = self.fresh_args(adt);
        let mut slots: Vec<Option<Expr>> = vec![None; decl_fields.len()];
        for f in fields {
            let Some(i) = decl_fields.iter().position(|n| *n == f.name) else {
                self.error(
                    diag::RECORD_UNKNOWN_FIELD,
                    f.span,
                    format!("no field `{}`", f.name),
                );
                continue;
            };
            if slots[i].is_some() {
                self.error(
                    diag::RECORD_DUPLICATE_FIELD,
                    f.span,
                    format!("field `{}` appears more than once", f.name),
                );
                continue;
            }
            let value = match &f.value {
                Some(v) => self.infer(v),
                None => self.infer(&ast::Expr {
                    kind: ast::ExprKind::Name(f.name.clone()),
                    span: f.span,
                    semantic: ast::SemanticContextMarker::default(),
                }),
            };
            let ft = self.field_ty(adt, variant, i as u32, &args);
            self.coerce(&value, ft, f.span);
            slots[i] = Some(value);
        }
        let mut out = Vec::new();
        for (i, s) in slots.into_iter().enumerate() {
            match s {
                Some(v) => out.push(v),
                None => {
                    self.error(
                        diag::RECORD_MISSING_FIELD,
                        span,
                        format!("field `{}` is missing", decl_fields[i]),
                    );
                    out.push(self.err_expr(span));
                }
            }
        }
        let ty = self.adt_ty(adt, &args);
        Expr {
            kind: ExprKind::Adt {
                adt,
                variant,
                args,
                fields: out,
            },
            ty,
            span,
        }
    }

    fn call(&mut self, callee: &ast::Expr, arguments: &[ast::Argument], span: Span) -> Expr {
        if let Some(a) = arguments.iter().find(|a| a.spread) {
            self.unsupported("spread arguments", a.span);
            return self.err_expr(span);
        }
        let (callee, explicit) =
            if let ast::ExprKind::TypeApply { callee, arguments } = &callee.kind {
                (callee.as_ref(), Some(arguments))
            } else {
                (callee, None)
            };
        let path = Self::expr_path(callee).filter(|p| !self.path_is_value(p));
        match path.as_deref() {
            Some([name]) if scalar_type(name).is_some() && explicit.is_none() => {
                let target = self
                    .types
                    .intern(scalar_type(name).unwrap_or(TyData::Error));
                let [arg] = arguments else {
                    self.error(diag::CALL_ARITY, span, "a conversion takes one argument");
                    return self.err_expr(span);
                };
                let operand = self.infer(&arg.value);
                Expr {
                    kind: ExprKind::Convert {
                        operand: Box::new(operand),
                    },
                    ty: target,
                    span,
                }
            }
            Some([name]) if matches!(self.items.names.get(name), Some(ItemRef::Fn(_))) => {
                let Some(ItemRef::Fn(f)) = self.items.names.get(name).copied() else {
                    return self.err_expr(span);
                };
                self.direct_call(f, explicit.map(Vec::as_slice), arguments, span)
            }
            Some(path @ [_, _]) if explicit.is_none() => {
                let Some((adt, variant)) = self.resolve_variant_path(path, span) else {
                    return self.err_expr(span);
                };
                let v = &self.items.adt(adt).variants[variant as usize];
                if v.shape != VariantShape::Tuple || v.fields.len() != arguments.len() {
                    self.error(
                        diag::ENUM_ARITY,
                        span,
                        "the arguments do not match the variant's fields",
                    );
                    return self.err_expr(span);
                }
                let args = self.fresh_args(adt);
                let mut fields = Vec::new();
                for (i, a) in arguments.iter().enumerate() {
                    let value = self.infer(&a.value);
                    let ft = self.field_ty(adt, variant, i as u32, &args);
                    self.coerce(&value, ft, a.span);
                    fields.push(value);
                }
                let ty = self.adt_ty(adt, &args);
                Expr {
                    kind: ExprKind::Adt {
                        adt,
                        variant,
                        args,
                        fields,
                    },
                    ty,
                    span,
                }
            }
            Some([name]) if !self.items.names.contains_key(name) => {
                self.error(
                    diag::NAME_UNRESOLVED,
                    span,
                    format!("unknown function `{name}`"),
                );
                self.err_expr(span)
            }
            Some(_) | None => {
                self.unsupported("calls of values and methods", span);
                self.err_expr(span)
            }
        }
    }

    fn direct_call(
        &mut self,
        f: FnRef,
        explicit: Option<&[ast::GenericArg]>,
        arguments: &[ast::Argument],
        span: Span,
    ) -> Expr {
        let decl = self.items.func(f);
        let n = decl.generics.len();
        let type_args: Vec<Ty> = match explicit {
            None => (0..n).map(|_| self.types.fresh(VarKind::General)).collect(),
            Some(given) => {
                if given.len() != n {
                    self.error(diag::TYPE_ARGUMENT, span, "wrong number of type arguments");
                    return self.err_expr(span);
                }
                let mut out = Vec::new();
                for g in given {
                    match g {
                        ast::GenericArg::Type(t) => out.push(self.resolve_written(t)),
                        ast::GenericArg::Lifetime(_)
                        | ast::GenericArg::Const(_)
                        | ast::GenericArg::AssociatedBinding(_) => {
                            self.unsupported("non-type generic arguments", span);
                            return self.err_expr(span);
                        }
                    }
                }
                out
            }
        };
        let param_tys: Vec<Ty> = decl.params.iter().map(|p| p.ty).collect();
        let result = decl.result;
        if param_tys.len() != arguments.len() {
            self.error(
                diag::CALL_ARITY,
                span,
                format!(
                    "`{}` takes {} arguments, {} given",
                    decl.name,
                    param_tys.len(),
                    arguments.len()
                ),
            );
            return self.err_expr(span);
        }
        let mut args = Vec::new();
        for (p, a) in param_tys.iter().zip(arguments) {
            let value = self.infer(&a.value);
            let expected = self.types.subst(*p, &type_args);
            let value = self.coerce_arg(value, expected, a.span);
            args.push(value);
        }
        let ty = self.types.subst(result, &type_args);
        Expr {
            kind: ExprKind::Call {
                callee: f,
                type_args,
                args,
            },
            ty,
            span,
        }
    }
}

fn contains_break(e: &Expr) -> bool {
    // `break` inside a nested loop belongs to that loop.
    fn walk(e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Break => true,
            ExprKind::Loop(_) | ExprKind::While { .. } => false,
            ExprKind::Block(stmts) => stmts.iter().any(|s| match s {
                Stmt::Let { init, .. } => walk(init),
                Stmt::Expr(e) => walk(e),
            }),
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => walk(cond) || walk(then) || otherwise.as_deref().is_some_and(walk),
            ExprKind::Match { scrutinee, arms } => {
                walk(scrutinee) || arms.iter().any(|a| walk(&a.body))
            }
            ExprKind::Deref(inner) | ExprKind::Borrow { place: inner, .. } => walk(inner),
            ExprKind::Lit(_)
            | ExprKind::Local(_)
            | ExprKind::Continue
            | ExprKind::Field { .. }
            | ExprKind::TupleField { .. }
            | ExprKind::Index { .. }
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Logical { .. }
            | ExprKind::Convert { .. }
            | ExprKind::Call { .. }
            | ExprKind::Adt { .. }
            | ExprKind::Tuple(_)
            | ExprKind::Array(_)
            | ExprKind::Return(_)
            | ExprKind::Assign { .. } => false,
        }
    }
    walk(e)
}

// ---- finalize -------------------------------------------------------------

/// Zonks every type and applies the rules that need closed types.
struct Finalizer<'c, 'a> {
    items: &'c Items<'a>,
    types: &'c mut Types,
    diags: &'c mut Vec<Diagnostic>,
    generics: &'c [crate::items::GenericInfo],
    reported_inference: bool,
}

/// Target pointer width used for literal ranges. The body IR carries
/// `isize`/`usize` symbolically; only literal range checks need a width.
pub const POINTER_BITS: u32 = 64;

impl Finalizer<'_, '_> {
    fn zonk(&mut self, ty: Ty, span: Span) -> Ty {
        match self.types.zonk(ty) {
            Some(t) => t,
            None => {
                if !self.reported_inference {
                    self.reported_inference = true;
                    self.diags.push(diag::error(
                        diag::TYPE_INFERENCE,
                        span,
                        "the type of this expression cannot be inferred",
                    ));
                }
                self.types.error()
            }
        }
    }

    fn function(&mut self, f: &mut ThirFn) {
        for l in &mut f.locals {
            let span = l.span;
            l.ty = self.zonk(l.ty, span);
        }
        self.expr(&mut f.body);
    }

    fn data(&self, ty: Ty) -> TyData {
        self.types.data(ty).clone()
    }

    fn is_copy(&mut self, ty: Ty) -> bool {
        self.is_copy_depth(ty, 0)
    }

    fn is_copy_depth(&mut self, ty: Ty, depth: u32) -> bool {
        // Deeper nesting than this is an infinite by-value layout, which the
        // declaration check rejects separately.
        if depth > 64 {
            return false;
        }
        match self.data(ty) {
            TyData::Unit
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Error => true,
            TyData::Tuple(items) => items.iter().all(|t| self.is_copy_depth(*t, depth + 1)),
            TyData::Array { element, .. } => self.is_copy_depth(element, depth + 1),
            TyData::Ref { mutable, .. } => !mutable,
            TyData::Param(i) => self.generics.get(i as usize).is_some_and(|g| g.copy),
            TyData::Adt { adt, args } => {
                let decl = self.items.adt(adt);
                match decl.kind {
                    AdtKind::Record | AdtKind::Enum => {
                        let fields: Vec<Ty> = decl
                            .variants
                            .iter()
                            .flat_map(|v| v.fields.iter().map(|f| f.ty))
                            .collect();
                        fields.into_iter().all(|f| {
                            let t = self.types.subst(f, &args);
                            self.is_copy_depth(t, depth + 1)
                        })
                    }
                    AdtKind::Package | AdtKind::Resource => false,
                }
            }
            TyData::Var(_) => false,
        }
    }

    fn pat(&mut self, p: &mut Pat) {
        p.ty = self.zonk(p.ty, p.span);
        match &mut p.kind {
            PatKind::Wild | PatKind::Binding(_) => {}
            PatKind::Lit(l) => {
                let (l, ty, span) = (*l, p.ty, p.span);
                self.literal(l, ty, span);
            }
            PatKind::Tuple(items) => items.iter_mut().for_each(|i| self.pat(i)),
            PatKind::Adt { args, fields, .. } => {
                for a in args.iter_mut() {
                    *a = self.zonk(*a, p.span);
                }
                fields.iter_mut().for_each(|f| self.pat(f));
            }
        }
    }

    fn literal(&mut self, l: Lit, ty: Ty, span: Span) {
        match (l, self.data(ty)) {
            (Lit::Int(v), TyData::Int(t)) => {
                if !literal::int_fits(v, t, POINTER_BITS) {
                    self.diags.push(diag::error(
                        diag::LITERAL_RANGE,
                        span,
                        format!("literal {v} does not fit its type"),
                    ));
                }
            }
            (Lit::Float(v), TyData::Float(zil_ir::body::FloatTy::F32)) => {
                if v.is_finite() && !(v as f32).is_finite() {
                    self.diags.push(diag::error(
                        diag::LITERAL_RANGE,
                        span,
                        format!("literal {v} does not fit f32"),
                    ));
                }
            }
            (Lit::Float(_), TyData::Float(zil_ir::body::FloatTy::F64))
            | (Lit::Bool(_), TyData::Bool)
            | (Lit::Unit, TyData::Unit)
            | (_, TyData::Error) => {}
            _ => self.diags.push(diag::error(
                diag::TYPE_MISMATCH,
                span,
                "the literal does not have the expected type",
            )),
        }
    }

    /// The operand domain of a binary operator (`body::rvalue_ty` is the
    /// structural counterpart; this one reports source diagnostics).
    fn binary_domain(&mut self, op: BinOp, operand: Ty, span: Span) {
        let kind = self.data(operand);
        let ok = match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                matches!(kind, TyData::Int(_) | TyData::Float(_))
            }
            BinOp::WrappingAdd
            | BinOp::WrappingSub
            | BinOp::WrappingMul
            | BinOp::Shl
            | BinOp::Shr => {
                matches!(kind, TyData::Int(_))
            }
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                matches!(kind, TyData::Int(_) | TyData::Bool)
            }
            BinOp::Eq | BinOp::Ne => matches!(
                kind,
                TyData::Int(_) | TyData::Float(_) | TyData::Bool | TyData::Unit
            ),
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                matches!(kind, TyData::Int(_) | TyData::Float(_))
            }
        } || matches!(kind, TyData::Error);
        if !ok {
            self.operator_error(span, "the operator does not apply to this type");
        }
    }

    fn operator_error(&mut self, span: Span, message: &str) {
        self.diags
            .push(diag::error(diag::OPERATOR_TYPE, span, message.to_string()));
    }

    fn expr(&mut self, e: &mut Expr) {
        e.ty = self.zonk(e.ty, e.span);
        let span = e.span;
        let ty = e.ty;
        match &mut e.kind {
            ExprKind::Lit(l) => {
                let l = *l;
                self.literal(l, ty, span);
            }
            ExprKind::Local(_) | ExprKind::Break | ExprKind::Continue => {}
            ExprKind::Field { base, args, .. } => {
                self.expr(base);
                for a in args.iter_mut() {
                    *a = self.zonk(*a, span);
                }
            }
            ExprKind::TupleField { base, .. } => self.expr(base),
            ExprKind::Deref(inner) | ExprKind::Borrow { place: inner, .. } => self.expr(inner),
            ExprKind::Index { base, index } => {
                self.expr(base);
                self.expr(index);
                let unsigned = match self.data(index.ty) {
                    TyData::Int(t) => !t.signed(),
                    TyData::Error => true,
                    TyData::Unit
                    | TyData::Bool
                    | TyData::Never
                    | TyData::Float(_)
                    | TyData::Adt { .. }
                    | TyData::Tuple(_)
                    | TyData::Array { .. }
                    | TyData::Ref { .. }
                    | TyData::Param(_)
                    | TyData::Var(_) => false,
                };
                if !unsigned {
                    self.operator_error(index.span, "an index must be an unsigned integer");
                }
            }
            ExprKind::Unary { op, operand } => {
                self.expr(operand);
                let ok = match (*op, self.data(operand.ty)) {
                    (UnOp::Not, TyData::Bool | TyData::Int(_)) => true,
                    (UnOp::Neg | UnOp::WrappingNeg, TyData::Int(i)) => i.signed(),
                    (UnOp::Neg, TyData::Float(_)) => true,
                    (_, TyData::Error) => true,
                    _ => false,
                };
                if !ok {
                    self.operator_error(span, "the operator does not apply to this type");
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                self.expr(lhs);
                self.expr(rhs);
                let (op, lty) = (*op, lhs.ty);
                self.binary_domain(op, lty, span);
            }
            ExprKind::Logical { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Convert { operand } => {
                self.expr(operand);
                let numeric =
                    |d: &TyData| matches!(d, TyData::Int(_) | TyData::Float(_) | TyData::Error);
                if !numeric(&self.data(operand.ty)) || !numeric(&self.data(ty)) {
                    self.diags.push(diag::error(
                        diag::NUMERIC_CONVERSION,
                        span,
                        "conversions apply to numeric values only",
                    ));
                }
            }
            ExprKind::Call {
                callee,
                type_args,
                args,
            } => {
                for a in type_args.iter_mut() {
                    *a = self.zonk(*a, span);
                }
                let generics = self.items.func(*callee).generics.clone();
                for (g, a) in generics.iter().zip(type_args.iter()) {
                    if g.copy && !self.is_copy(*a) {
                        self.diags.push(diag::error(
                            diag::GENERIC_BOUND,
                            span,
                            format!("type argument for `{}` must be Copy", g.name),
                        ));
                    }
                }
                args.iter_mut().for_each(|a| self.expr(a));
            }
            ExprKind::Adt { args, fields, .. } => {
                for a in args.iter_mut() {
                    *a = self.zonk(*a, span);
                }
                fields.iter_mut().for_each(|f| self.expr(f));
            }
            ExprKind::Tuple(items) | ExprKind::Array(items) => {
                items.iter_mut().for_each(|i| self.expr(i))
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                self.expr(cond);
                self.expr(then);
                if let Some(o) = otherwise {
                    self.expr(o);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.expr(scrutinee);
                for a in arms {
                    self.pat(&mut a.pattern);
                    self.expr(&mut a.body);
                }
            }
            ExprKind::Block(stmts) => {
                for s in stmts {
                    match s {
                        Stmt::Let { pattern, init } => {
                            self.expr(init);
                            self.pat(pattern);
                        }
                        Stmt::Expr(e) => self.expr(e),
                    }
                }
            }
            ExprKind::Loop(body) => self.expr(body),
            ExprKind::While { cond, body } => {
                self.expr(cond);
                self.expr(body);
            }
            ExprKind::Return(v) => {
                if let Some(v) = v {
                    self.expr(v);
                }
            }
            ExprKind::Assign { place, op, value } => {
                self.expr(place);
                self.expr(value);
                if let Some(op) = *op {
                    let ty = place.ty;
                    self.binary_domain(op, ty, span);
                }
            }
        }
    }
}
