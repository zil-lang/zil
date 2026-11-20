//! infer: name resolution inside bodies and type inference, producing THIR.
//!
//! Inference is equality-based: every expression gets a type, possibly an
//! inference variable, and constraints are solved by unification as they
//! arise. Unsuffixed literals carry kinded variables that default at the
//! end. `finalize` then zonks every type and applies the typing rules that
//! need closed types (operator domains, literal ranges, bounds).

use crate::diag;
use crate::items::{
    FnDecl, ForeignDeclInfo, ItemRef, Items, VariantShape, resolve_type, scalar_type,
};
use crate::literal;
use crate::thir::*;
use crate::ty::{AdtRef, ClosureRef, OpaqueKind, Ty, TyData, Types, VarKind};
use zil_base::{Diagnostic, Span};
use zil_ir::body::{
    AdtKind, BinOp, ClosureCallMode, ClosureKind, IntTy, IntrinsicOp, Mutability, UnOp,
};
use zil_syntax::ast;

struct CaptureEntry {
    source: LocalId,
    orig_ty: Ty,
    by_value: bool,
    mutable: bool,
    field_ty: Ty,
}

struct CaptureEnv {
    /// Visible locals in the enclosing function: `(name, local, ty, mutable)`.
    outer: Vec<(String, LocalId, Ty, bool)>,
    move_capture: bool,
    captures: Vec<CaptureEntry>,
}

pub struct FnCx<'c, 'a> {
    items: &'c Items<'a>,
    types: &'c mut Types,
    diags: &'c mut Vec<Diagnostic>,
    fn_ref: FnRef,
    decl: &'c FnDecl<'a>,
    opaque_return: Option<ClosureRef>,
    generic_names: Vec<String>,
    ret_ty: Ty,
    locals: Vec<LocalInfo>,
    scopes: Vec<Vec<(String, LocalId)>>,
    closures: Vec<ThirClosure>,
    capture_env: Option<CaptureEnv>,
    loop_depth: u32,
    in_unsafe: u32,
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
        fn_ref: f,
        decl,
        opaque_return: None,
        generic_names: decl.generics.iter().map(|g| g.name.clone()).collect(),
        ret_ty: decl.result,
        locals: Vec::new(),
        scopes: vec![Vec::new()],
        closures: Vec::new(),
        capture_env: None,
        loop_depth: 0,
        in_unsafe: 0,
    };
    let mut params = Vec::new();
    for p in &decl.params {
        params.push(cx.declare(&p.name, p.ty, false, p.span));
    }
    let body = match &decl.ast.body {
        ast::FunctionBody::Block(b) => cx.block(b),
        ast::FunctionBody::Expression(e) => {
            let value = cx.infer(e);
            let value = cx.coerce_value(value, decl.result, e.span);
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
        closures: cx.closures,
        opaque_return: cx.opaque_return,
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

    /// Dereferences an address, `Managed` carrier or `Desc` base of a member
    /// or index access.
    fn auto_deref(&mut self, mut base: Expr) -> Expr {
        loop {
            let bt = self.types.shallow(base.ty);
            let inner = if let TyData::Ref { pointee, .. } = *self.types.data(bt) {
                pointee
            } else if let TyData::Managed(inner) | TyData::Desc(inner) = *self.types.data(bt) {
                inner
            } else {
                return base;
            };
            let span = base.span;
            base = Expr {
                kind: ExprKind::Deref(Box::new(base)),
                ty: inner,
                span,
            };
        }
    }

    /// Coerces an argument at a call site. An exclusive address held in a
    /// place is reborrowed (`&mut *p`) instead of moved, and an exclusive
    /// address converts to a shared one (`&*p`); both are new loans the
    /// kernel checks like any other.
    fn coerce_arg(&mut self, value: Expr, expected: Ty, span: Span) -> Expr {
        self.coerce_value(value, expected, span)
    }

    /// Unified coercion site: handles reborrow (`&mut -> &mut/&`), auto-describe
    /// (`Managed<R>` or `&Managed<R>` -> `Desc<R>`), and implicit error-union
    /// injection (`T` or `E` -> `E!T`).
    fn coerce_value(&mut self, value: Expr, expected: Ty, span: Span) -> Expr {
        if self.types.is_never(value.ty) {
            return value;
        }
        let et = self.types.shallow(expected);
        let vt = self.types.shallow(value.ty);
        let ed = self.types.data(et).clone();
        let vd = self.types.data(vt).clone();

        // 0. Return-position `impl Fn/FnMut/FnOnce` coercion.
        if let (
            TyData::ImplCallable {
                owner,
                kind: want_k,
                params: wp,
                result: wr,
            },
            TyData::Closure {
                id,
                kind: got_k,
                params: gp,
                result: gr,
                ..
            },
        ) = (&ed, &vd)
            && *owner == self.fn_ref
        {
            let kind_ok = matches!(
                (got_k, want_k),
                (ClosureKind::Fn, _)
                    | (ClosureKind::FnMut, ClosureKind::FnMut | ClosureKind::FnOnce)
                    | (ClosureKind::FnOnce, ClosureKind::FnOnce)
            );
            if !kind_ok || gp.len() != wp.len() {
                self.error(
                    diag::TYPE_MISMATCH,
                    span,
                    "closure does not satisfy the declared `impl` callable return type",
                );
                return value;
            }
            for (g, w) in gp.iter().zip(wp) {
                self.unify(*g, *w, span);
            }
            self.unify(*gr, *wr, span);
            self.opaque_return = Some(*id);
            return value;
        }

        // 1. Reborrow `&mut T` -> `&mut T` / `&T`.
        if let (
            TyData::Ref { mutable: want, .. },
            TyData::Ref {
                mutable: true,
                pointee,
            },
        ) = (&ed, &vd)
            && (!want || is_place(&value.kind))
        {
            let vspan = value.span;
            let target = Expr {
                kind: ExprKind::Deref(Box::new(value)),
                ty: *pointee,
                span: vspan,
            };
            let ty = self.types.intern(TyData::Ref {
                mutable: *want,
                pointee: *pointee,
            });
            let out = Expr {
                kind: ExprKind::Borrow {
                    mutable: *want,
                    place: Box::new(target),
                },
                ty,
                span: vspan,
            };
            self.coerce(&out, expected, span);
            return out;
        }

        // 2. Auto-describe: `Managed<R>` or `&Managed<R>` -> `Desc<R>`.
        if let TyData::Desc(want_inner) = ed {
            if let TyData::Managed(got_inner) = vd {
                self.unify(got_inner, want_inner, span);
                let vspan = value.span;
                return Expr {
                    kind: ExprKind::Describe(Box::new(value)),
                    ty: expected,
                    span: vspan,
                };
            } else if let TyData::Ref { pointee, .. } = vd {
                let pt = self.types.shallow(pointee);
                if let TyData::Managed(got_inner) = *self.types.data(pt) {
                    self.unify(got_inner, want_inner, span);
                    let vspan = value.span;
                    let deref = Expr {
                        kind: ExprKind::Deref(Box::new(value)),
                        ty: pt,
                        span: vspan,
                    };
                    return Expr {
                        kind: ExprKind::Describe(Box::new(deref)),
                        ty: expected,
                        span: vspan,
                    };
                }
            }
        }

        // 3. Implicit error-union injection: `E!T`.
        if let TyData::ErrorUnion { ok, err } = ed
            && !matches!(vd, TyData::ErrorUnion { .. } | TyData::Error)
        {
            let ok_match = self.types.can_unify(value.ty, ok);
            let err_match = self.types.can_unify(value.ty, err);
            match (ok_match, err_match) {
                (true, false) => {
                    let _ = self.types.unify(value.ty, ok);
                    let vspan = value.span;
                    return Expr {
                        kind: ExprKind::ErrorInject {
                            ok: true,
                            value: Box::new(value),
                        },
                        ty: expected,
                        span: vspan,
                    };
                }
                (false, true) => {
                    let _ = self.types.unify(value.ty, err);
                    let vspan = value.span;
                    return Expr {
                        kind: ExprKind::ErrorInject {
                            ok: false,
                            value: Box::new(value),
                        },
                        ty: expected,
                        span: vspan,
                    };
                }
                (true, true) => {
                    self.error(
                        diag::ERROR_INJECTION_AMBIGUOUS,
                        span,
                        "value matches both arms of the error union",
                    );
                    return self.err_expr(span);
                }
                (false, false) => {
                    self.error(
                        diag::ERROR_INJECTION,
                        span,
                        "value matches neither arm of the error union",
                    );
                    return self.err_expr(span);
                }
            }
        }

        self.coerce(&value, expected, span);
        value
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
        match resolve_type(
            t,
            self.decl.module,
            &self.generic_names,
            self.items,
            self.types,
        ) {
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

    /// `Enum.Variant`, `Record`, `mod.Record`, or `mod.Enum.Variant`.
    fn resolve_variant_path(&mut self, path: &[String], span: Span) -> Option<(AdtRef, u32)> {
        if let Some(ItemRef::Adt(adt)) = self.items.lookup_item(self.decl.module, path) {
            if self.items.adt(adt).kind != AdtKind::Enum {
                return Some((adt, 0));
            }
            self.error(diag::ENUM_VARIANT, span, "an enum needs a variant name");
            return None;
        }
        if path.len() >= 2 {
            let (enum_path, variant) = (&path[..path.len() - 1], &path[path.len() - 1]);
            if let Some(ItemRef::Adt(adt)) = self.items.lookup_item(self.decl.module, enum_path)
                && self.items.adt(adt).kind == AdtKind::Enum
            {
                return match self.items.adt(adt).variant_named(variant) {
                    Some(v) => Some((adt, v)),
                    None => {
                        self.error(
                            diag::ENUM_VARIANT,
                            span,
                            format!("`{}` has no variant `{variant}`", enum_path.join(".")),
                        );
                        None
                    }
                };
            }
        }
        self.error(
            diag::NAME_UNRESOLVED,
            span,
            format!("unknown type or variant `{}`", path.join(".")),
        );
        None
    }

    fn fn_value(&mut self, f: FnRef, span: Span) -> Expr {
        let decl = self.items.func(f);
        if !decl.generics.is_empty() {
            self.unsupported("generic function values", span);
            return self.err_expr(span);
        }
        let params: Vec<Ty> = decl.params.iter().map(|p| p.ty).collect();
        let result = decl.result;
        let ty = self.types.intern(TyData::FnPtr { params, result });
        Expr {
            kind: ExprKind::FnRef(f),
            ty,
            span,
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
        path.first().is_some_and(|n| {
            self.lookup_local(n).is_some()
                || self
                    .capture_env
                    .as_ref()
                    .is_some_and(|env| env.outer.iter().any(|(name, ..)| name == n))
        })
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
                let mut init = self.infer(value);
                let declared = match ty {
                    Some(t) => {
                        let t = self.resolve_written(t);
                        let tt = self.types.shallow(t);
                        let it = self.types.shallow(init.ty);
                        if let (TyData::Desc(want), TyData::Managed(got)) =
                            (self.types.data(tt).clone(), self.types.data(it).clone())
                        {
                            self.unify(got, want, value.span);
                            init.ty
                        } else {
                            init = self.coerce_value(init, t, value.span);
                            t
                        }
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
                let result = self.ret_ty;
                let value = match value {
                    Some(v) => {
                        let v = self.infer(v);
                        let v = self.coerce_value(v, result, span);
                        Some(Box::new(v))
                    }
                    None => {
                        if self.types.unify(result, unit).is_err() {
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
                let (out, ty) = match condition {
                    ast::Condition::Expression(c) => {
                        let cond = self.infer(c);
                        let b = self.types.bool();
                        self.coerce(&cond, b, c.span);
                        let body = self.block(body);
                        if matches!(cond.kind, ExprKind::Lit(Lit::Bool(true))) {
                            let ty = if contains_break(&body) { unit } else { never };
                            (ExprKind::Loop(Box::new(body)), ty)
                        } else {
                            (
                                ExprKind::While {
                                    cond: Box::new(cond),
                                    body: Box::new(body),
                                },
                                unit,
                            )
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
                        (ExprKind::Loop(Box::new(m)), unit)
                    }
                };
                self.loop_depth -= 1;
                e(out, ty)
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
            ast::StmtKind::Unsafe(b) => {
                self.in_unsafe += 1;
                let expr = self.block(b);
                self.in_unsafe -= 1;
                Some(Stmt::Expr(expr))
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
                ExprKind::Capture(idx) => {
                    if let Some(env) = &self.capture_env
                        && let Some(cap) = env.captures.get(*idx as usize)
                        && !cap.mutable
                    {
                        self.error(
                            diag::ASSIGN_IMMUTABLE,
                            place.span,
                            format!("captured variable cannot be {what}"),
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
                | ExprKind::SliceBorrow { .. }
                | ExprKind::Len(_)
                | ExprKind::Give(_)
                | ExprKind::Describe(_)
                | ExprKind::ManagedCreate { .. }
                | ExprKind::ForeignCall { .. }
                | ExprKind::Intrinsic { .. }
                | ExprKind::ErrorInject { .. }
                | ExprKind::Try(_)
                | ExprKind::CtEval(_)
                | ExprKind::Lit(_)
                | ExprKind::Unary { .. }
                | ExprKind::Binary { .. }
                | ExprKind::Logical { .. }
                | ExprKind::Convert { .. }
                | ExprKind::FnRef(_)
                | ExprKind::Call { .. }
                | ExprKind::IndirectCall { .. }
                | ExprKind::Closure { .. }
                | ExprKind::ClosureCall { .. }
                | ExprKind::TraitCall { .. }
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
                if items.is_empty() {
                    let unit = self.types.unit();
                    self.unify(unit, expected, span);
                    PatKind::Wild
                } else {
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
            ast::PatternKind::ErrorArm { is_error, pattern } => {
                let et = self.types.shallow(expected);
                if let TyData::ErrorUnion { ok, err } = self.types.data(et).clone() {
                    let arm_ty = if *is_error { err } else { ok };
                    let inner = self.pattern(pattern, arm_ty, mutable);
                    PatKind::ErrorArm {
                        ok: !*is_error,
                        inner: Box::new(inner),
                    }
                } else if matches!(self.types.data(et), TyData::Error) {
                    PatKind::Wild
                } else {
                    self.error(
                        diag::TYPE_MISMATCH,
                        span,
                        "`.ok`/`.err` pattern requires an error union type",
                    );
                    PatKind::Wild
                }
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
            PatKind::Lit(_) | PatKind::ErrorArm { .. } => false,
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
                if let Some(captured) = self.capture_outer(n, span) {
                    return captured;
                }
                match self
                    .items
                    .lookup_item(self.decl.module, std::slice::from_ref(n))
                {
                    Some(ItemRef::Fn(f)) => self.fn_value(f, span),
                    Some(ItemRef::Foreign(_)) => {
                        self.unsupported("foreign function values", span);
                        self.err_expr(span)
                    }
                    Some(ItemRef::Adt(_) | ItemRef::Trait(_)) => {
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
                    TyData::Array { element, .. } | TyData::Slice(element) => *element,
                    TyData::Error => self.types.error(),
                    TyData::Unit
                    | TyData::Bool
                    | TyData::Never
                    | TyData::Int(_)
                    | TyData::Float(_)
                    | TyData::Adt { .. }
                    | TyData::Tuple(_)
                    | TyData::Ref { .. }
                    | TyData::Managed(_)
                    | TyData::Desc(_)
                    | TyData::Opaque(_)
                    | TyData::ErrorUnion { .. }
                    | TyData::FnPtr { .. }
                    | TyData::Closure { .. }
                    | TyData::ImplCallable { .. }
                    | TyData::Param(_)
                    | TyData::SelfParam
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
            ast::ExprKind::ReceiverMember(name) => {
                let Some(local) = self.lookup_local("self") else {
                    self.error(
                        diag::NAME_UNRESOLVED,
                        span,
                        format!("receiver member `.{name}` requires `self` in scope"),
                    );
                    return self.err_expr(span);
                };
                let self_expr = Expr {
                    kind: ExprKind::Local(local),
                    ty: self.locals[local.0 as usize].ty,
                    span,
                };
                self.member_on(self_expr, name, span)
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
            ast::ExprKind::Try(inner) => {
                let x = self.infer(inner);
                let xt = self.types.shallow(x.ty);
                let (ok_ty, err_ty) =
                    if let TyData::ErrorUnion { ok, err } = self.types.data(xt).clone() {
                        (ok, err)
                    } else if matches!(self.types.data(xt), TyData::Error) {
                        return self.err_expr(span);
                    } else {
                        self.error(
                            diag::TRY_NOT_ERROR,
                            span,
                            "`?` requires an error union operand",
                        );
                        return self.err_expr(span);
                    };
                let ret_t = self.types.shallow(self.ret_ty);
                if let TyData::ErrorUnion { err: ret_err, .. } = self.types.data(ret_t).clone() {
                    if self.types.unify(err_ty, ret_err).is_err() {
                        self.error(
                            diag::TRY_ERROR_TYPE,
                            span,
                            "`?` error type does not match the enclosing function's error type",
                        );
                    }
                } else if !matches!(self.types.data(ret_t), TyData::Error) {
                    self.error(
                        diag::TRY_RETURN_TYPE,
                        span,
                        "`?` requires the enclosing function to return an error union",
                    );
                }
                mk(ExprKind::Try(Box::new(x)), ok_ty)
            }
            ast::ExprKind::Closure {
                move_capture,
                params,
                return_type,
                body,
                ..
            } => self.infer_closure(*move_capture, params, return_type.as_ref(), body, span),
            ast::ExprKind::MetaEval(inner) => {
                // Compile-time expressions cannot observe enclosing runtime locals.
                let saved_scopes = std::mem::take(&mut self.scopes);
                self.scopes.push(Vec::new());
                let expr = self.infer(inner);
                self.scopes = saved_scopes;
                let ty = expr.ty;
                mk(ExprKind::CtEval(Box::new(expr)), ty)
            }
            ast::ExprKind::MetaSplice(_)
            | ast::ExprKind::Quote { .. }
            | ast::ExprKind::MetaBlock(_)
            | ast::ExprKind::SyntaxIsland { .. } => {
                self.unsupported("compile-time evaluation", span);
                self.err_expr(span)
            }
        }
    }

    fn capture_outer(&mut self, name: &str, span: Span) -> Option<Expr> {
        let env = self.capture_env.as_mut()?;
        let (_, source, orig_ty, outer_mut) =
            env.outer.iter().rev().find(|(n, ..)| n == name).cloned()?;
        let idx = if let Some(pos) = env.captures.iter().position(|c| c.source == source) {
            pos
        } else {
            let by_value = env.move_capture;
            let mutable = outer_mut;
            let field_ty = if by_value {
                orig_ty
            } else {
                self.types.intern(TyData::Ref {
                    mutable,
                    pointee: orig_ty,
                })
            };
            env.captures.push(CaptureEntry {
                source,
                orig_ty,
                by_value,
                mutable,
                field_ty,
            });
            env.captures.len() - 1
        };
        let cap = &env.captures[idx];
        let cap_expr = Expr {
            kind: ExprKind::Capture(idx as u32),
            ty: cap.field_ty,
            span,
        };
        if cap.by_value {
            Some(cap_expr)
        } else {
            Some(Expr {
                kind: ExprKind::Deref(Box::new(cap_expr)),
                ty: cap.orig_ty,
                span,
            })
        }
    }

    fn infer_closure(
        &mut self,
        move_capture: bool,
        params: &[ast::ClosureParam],
        return_type: Option<&ast::TypeRef>,
        body: &ast::ArrowBody,
        span: Span,
    ) -> Expr {
        if self.capture_env.is_some() {
            self.unsupported("nested closures", span);
            return self.err_expr(span);
        }
        // Snapshot currently visible locals of the enclosing function.
        let mut outer = Vec::new();
        for scope in &self.scopes {
            for (name, l) in scope {
                let info = &self.locals[l.0 as usize];
                outer.push((name.clone(), *l, info.ty, info.mutable));
            }
        }
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_scopes = std::mem::take(&mut self.scopes);
        let saved_ret = self.ret_ty;
        let saved_loop = self.loop_depth;
        let saved_unsafe = self.in_unsafe;

        self.scopes.push(Vec::new());
        self.loop_depth = 0;
        self.in_unsafe = 0;
        self.capture_env = Some(CaptureEnv {
            outer,
            move_capture,
            captures: Vec::new(),
        });

        let mut param_ids = Vec::with_capacity(params.len());
        let mut param_tys = Vec::with_capacity(params.len());
        for p in params {
            let ty = match &p.ty {
                Some(t) => self.resolve_written(t),
                None => self.types.fresh(VarKind::General),
            };
            param_tys.push(ty);
            param_ids.push(self.declare(&p.name, ty, false, p.span));
        }
        let result_ty = match return_type {
            Some(t) => self.resolve_written(t),
            None => self.types.fresh(VarKind::General),
        };
        self.ret_ty = result_ty;

        let body_expr = match body {
            ast::ArrowBody::Block(b) => self.block(b),
            ast::ArrowBody::Expression(e) => {
                let val = self.infer(e);
                let val = self.coerce_value(val, result_ty, e.span);
                let ty = self.types.never();
                Expr {
                    kind: ExprKind::Return(Some(Box::new(val))),
                    ty,
                    span: e.span,
                }
            }
        };
        let unit = self.types.unit();
        if !self.types.is_never(body_expr.ty) {
            // A block closure that can fall through without returning defaults to Unit.
            let rt = self.types.shallow(result_ty);
            if matches!(self.types.data(rt), TyData::Var(_)) {
                let _ = self.types.unify(result_ty, unit);
            }
        }
        let resolved_ret = self.types.shallow(result_ty);
        if !self.types.is_never(body_expr.ty) && resolved_ret != unit {
            self.error(
                diag::MISSING_RETURN,
                span,
                "closure can finish without returning a value",
            );
        }

        let closure_locals = std::mem::replace(&mut self.locals, saved_locals);
        self.scopes = saved_scopes;
        self.ret_ty = saved_ret;
        self.loop_depth = saved_loop;
        self.in_unsafe = saved_unsafe;
        let env = self.capture_env.take().expect("capture_env set above");

        let kind = if env.captures.iter().any(|c| !c.by_value && c.mutable) {
            ClosureKind::FnMut
        } else {
            ClosureKind::Fn
        };
        let id = ClosureRef(self.closures.len() as u32);
        let capture_tys: Vec<Ty> = env.captures.iter().map(|c| c.field_ty).collect();
        let capture_exprs: Vec<Expr> = env
            .captures
            .iter()
            .map(|c| {
                let place = Expr {
                    kind: ExprKind::Local(c.source),
                    ty: c.orig_ty,
                    span,
                };
                if c.by_value {
                    place
                } else {
                    Expr {
                        kind: ExprKind::Borrow {
                            mutable: c.mutable,
                            place: Box::new(place),
                        },
                        ty: c.field_ty,
                        span,
                    }
                }
            })
            .collect();
        self.closures.push(ThirClosure {
            kind,
            capture_tys,
            locals: closure_locals,
            params: param_ids,
            param_tys: param_tys.clone(),
            result: result_ty,
            body: body_expr,
        });
        let ty = self.types.intern(TyData::Closure {
            owner: self.fn_ref,
            id,
            kind,
            params: param_tys,
            result: result_ty,
        });
        Expr {
            kind: ExprKind::Closure {
                id,
                captures: capture_exprs,
            },
            ty,
            span,
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
                    | TyData::Slice(_)
                    | TyData::Managed(_)
                    | TyData::Desc(_)
                    | TyData::Opaque(_)
                    | TyData::ErrorUnion { .. }
                    | TyData::FnPtr { .. }
                    | TyData::Closure { .. }
                    | TyData::ImplCallable { .. }
                    | TyData::Param(_)
                    | TyData::SelfParam => {
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
                if let ast::ExprKind::Slice { object, start, end } = &operand.kind {
                    let base = self.infer(object);
                    let base = self.auto_deref(base);
                    if mutable && is_place(&base.kind) {
                        self.check_mutable_place(&base, "borrowed exclusively");
                    }
                    let start = start.as_ref().map(|e| Box::new(self.infer(e)));
                    let end = end.as_ref().map(|e| Box::new(self.infer(e)));
                    let bt = self.types.shallow(base.ty);
                    let elem = if let TyData::Array { element, .. } | TyData::Slice(element) =
                        *self.types.data(bt)
                    {
                        element
                    } else if matches!(self.types.data(bt), TyData::Error) {
                        self.types.error()
                    } else {
                        self.error(
                            diag::OPERATOR_TYPE,
                            span,
                            "only arrays and slices can be sliced",
                        );
                        self.types.error()
                    };
                    let slice_ty = self.types.intern(TyData::Slice(elem));
                    let ty = self.types.intern(TyData::Ref {
                        mutable,
                        pointee: slice_ty,
                    });
                    return mk(
                        ExprKind::SliceBorrow {
                            mutable,
                            place: Box::new(base),
                            start,
                            end,
                        },
                        ty,
                    );
                }
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
                let x = self.infer(operand);
                if !is_place(&x.kind) {
                    self.error(
                        diag::GIVE_TYPE,
                        span,
                        "`give` requires a place holding a managed resource",
                    );
                    return self.err_expr(span);
                }
                let xt = self.types.shallow(x.ty);
                if matches!(self.types.data(xt), TyData::Managed(_)) {
                    let ty = x.ty;
                    mk(ExprKind::Give(Box::new(x)), ty)
                } else if matches!(self.types.data(xt), TyData::Desc(_)) {
                    self.error(
                        diag::GIVE_DESCRIPTOR,
                        span,
                        "cannot `give` a resource descriptor",
                    );
                    self.err_expr(span)
                } else if matches!(self.types.data(xt), TyData::Error) {
                    self.err_expr(span)
                } else {
                    self.error(
                        diag::GIVE_TYPE,
                        span,
                        "`give` requires a `Managed` resource place",
                    );
                    self.err_expr(span)
                }
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
        // Module-qualified function value (`transform.double`) or unit variant
        // (`Enum.Variant` / `model.Enum.Variant`).
        if let Some(path) = Self::expr_path(whole)
            && !self.path_is_value(&path)
            && path.len() >= 2
        {
            if let Some(ItemRef::Fn(f)) = self.items.lookup_item(self.decl.module, &path) {
                return self.fn_value(f, span);
            }
            if let Some((adt, variant)) = self.resolve_variant_path(&path, span) {
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
            return self.err_expr(span);
        }
        let base = self.infer(object);
        self.member_on_kind(base, member, span)
    }

    fn member_on(&mut self, base: Expr, name: &str, span: Span) -> Expr {
        self.member_on_kind(base, &ast::MemberName::Name(name.to_string()), span)
    }

    fn member_on_kind(&mut self, base: Expr, member: &ast::MemberName, span: Span) -> Expr {
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
            let value = self.coerce_value(value, ft, f.span);
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
        let adt_ty = self.adt_ty(adt, &args);
        if self.items.adt(adt).kind == AdtKind::Resource {
            let ty = self.types.intern(TyData::Managed(adt_ty));
            Expr {
                kind: ExprKind::ManagedCreate { args, fields: out },
                ty,
                span,
            }
        } else {
            Expr {
                kind: ExprKind::Adt {
                    adt,
                    variant,
                    args,
                    fields: out,
                },
                ty: adt_ty,
                span,
            }
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
        if let Some(path) = Self::expr_path(callee).filter(|p| !self.path_is_value(p)) {
            if let [name] = path.as_slice()
                && scalar_type(name).is_some()
                && explicit.is_none()
            {
                let target = self
                    .types
                    .intern(scalar_type(name).unwrap_or(TyData::Error));
                let [arg] = arguments else {
                    self.error(diag::CALL_ARITY, span, "a conversion takes one argument");
                    return self.err_expr(span);
                };
                let operand = self.infer(&arg.value);
                return Expr {
                    kind: ExprKind::Convert {
                        operand: Box::new(operand),
                    },
                    ty: target,
                    span,
                };
            }
            if let Some(item) = self.items.lookup_item(self.decl.module, &path) {
                match item {
                    ItemRef::Fn(f) => {
                        return self.direct_call(f, explicit.map(Vec::as_slice), arguments, span);
                    }
                    ItemRef::Foreign(f) => {
                        if explicit.is_some() {
                            self.error(
                                diag::TYPE_ARGUMENT,
                                span,
                                "extern \"C\" functions take no type arguments",
                            );
                        }
                        return self.foreign_call(f, arguments, span);
                    }
                    ItemRef::Adt(_) | ItemRef::Trait(_) => {
                        self.error(diag::ENUM_VARIANT, span, "a type cannot be called directly");
                        return self.err_expr(span);
                    }
                }
            }
            if path.len() >= 2 && explicit.is_none() {
                let (enum_path, variant) = (&path[..path.len() - 1], &path[path.len() - 1]);
                if let Some(ItemRef::Adt(adt)) = self.items.lookup_item(self.decl.module, enum_path)
                    && self.items.adt(adt).kind == AdtKind::Enum
                {
                    let Some(v_idx) = self.items.adt(adt).variant_named(variant) else {
                        self.error(
                            diag::ENUM_VARIANT,
                            span,
                            format!("`{}` has no variant `{variant}`", enum_path.join(".")),
                        );
                        return self.err_expr(span);
                    };
                    let v = &self.items.adt(adt).variants[v_idx as usize];
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
                        let ft = self.field_ty(adt, v_idx, i as u32, &args);
                        let value = self.coerce_value(value, ft, a.span);
                        fields.push(value);
                    }
                    let ty = self.adt_ty(adt, &args);
                    return Expr {
                        kind: ExprKind::Adt {
                            adt,
                            variant: v_idx,
                            args,
                            fields,
                        },
                        ty,
                        span,
                    };
                }
            }
            if let [param_name, method_name] = path.as_slice()
                && explicit.is_none()
                && let Some(param_idx) = self.generic_names.iter().position(|g| g == param_name)
            {
                let bounds = self.decl.generics[param_idx].traits.clone();
                let self_ty = self.types.intern(TyData::Param(param_idx as u32));
                for (tr_ref, tr_args) in bounds {
                    let tr = self.items.trait_decl(tr_ref);
                    if let Some(m) = tr.methods.iter().find(|m| m.name == *method_name) {
                        if arguments.len() != m.params.len() {
                            self.error(
                                diag::CALL_ARITY,
                                span,
                                format!(
                                    "`{param_name}.{method_name}` expects {} argument(s), found {}",
                                    m.params.len(),
                                    arguments.len()
                                ),
                            );
                            return self.err_expr(span);
                        }
                        let m_params = m.params.clone();
                        let m_result = m.result;
                        let mut args = Vec::with_capacity(arguments.len());
                        for (a, pt) in arguments.iter().zip(&m_params) {
                            let expected = self.types.subst_with_self(*pt, &tr_args, self_ty);
                            let v = self.infer(&a.value);
                            args.push(self.coerce_arg(v, expected, a.span));
                        }
                        let ret = self.types.subst_with_self(m_result, &tr_args, self_ty);
                        return Expr {
                            kind: ExprKind::TraitCall {
                                trait_ref: tr_ref,
                                trait_args: tr_args,
                                self_ty,
                                method: method_name.clone(),
                                args,
                            },
                            ty: ret,
                            span,
                        };
                    }
                }
            }
            if let [name] = path.as_slice()
                && let Some(expr) =
                    self.builtin_call(name, explicit.map(Vec::as_slice), arguments, span)
            {
                return expr;
            }
            self.error(
                diag::NAME_UNRESOLVED,
                span,
                format!("unknown function `{}`", path.join(".")),
            );
            return self.err_expr(span);
        }

        // Built-in `.len()` method on arrays and slices (`-> i64`).
        if explicit.is_none()
            && arguments.is_empty()
            && let ast::ExprKind::Member {
                object,
                member: ast::MemberName::Name(mname),
            } = &callee.kind
            && mname == "len"
        {
            let base = self.infer(object);
            let base = self.auto_deref(base);
            let bt = self.types.shallow(base.ty);
            if matches!(self.types.data(bt), TyData::Array { .. } | TyData::Slice(_)) {
                let i64_ty = self.types.int(IntTy::I64);
                return Expr {
                    kind: ExprKind::Len(Box::new(base)),
                    ty: i64_ty,
                    span,
                };
            }
        }

        // Value call: function pointer, closure, `impl` callable or generic `F: Fn*`.
        if explicit.is_some() {
            self.error(
                diag::TYPE_ARGUMENT,
                span,
                "indirect calls take no explicit type arguments",
            );
        }
        let callee_expr = self.infer(callee);
        let callee_expr = self.auto_deref(callee_expr);
        let ct = self.types.shallow(callee_expr.ty);
        if let TyData::FnPtr { params, result } = self.types.data(ct).clone() {
            if arguments.len() != params.len() {
                self.error(
                    diag::CALL_ARITY,
                    span,
                    format!(
                        "function expects {} argument(s), found {}",
                        params.len(),
                        arguments.len()
                    ),
                );
                return self.err_expr(span);
            }
            let mut args = Vec::new();
            for (a, p) in arguments.iter().zip(&params) {
                let v = self.infer(&a.value);
                args.push(self.coerce_arg(v, *p, a.span));
            }
            Expr {
                kind: ExprKind::IndirectCall {
                    callee: Box::new(callee_expr),
                    args,
                },
                ty: result,
                span,
            }
        } else {
            let callable_sig: Option<(ClosureKind, Vec<Ty>, Ty)> = if let TyData::Closure {
                kind,
                params,
                result,
                ..
            }
            | TyData::ImplCallable {
                kind,
                params,
                result,
                ..
            } =
                self.types.data(ct).clone()
            {
                Some((kind, params, result))
            } else if let TyData::Param(idx) = self.types.data(ct) {
                self.decl
                    .generics
                    .get(*idx as usize)
                    .and_then(|g| g.callable.clone())
            } else {
                None
            };
            if let Some((kind, params, result)) = callable_sig {
                let mode = match kind {
                    ClosureKind::Fn => ClosureCallMode::Shared,
                    ClosureKind::FnMut => ClosureCallMode::Mut,
                    ClosureKind::FnOnce => ClosureCallMode::Once,
                };
                if mode == ClosureCallMode::Mut && is_place(&callee_expr.kind) {
                    self.check_mutable_place(&callee_expr, "called as a mutable closure");
                }
                if arguments.len() != params.len() {
                    self.error(
                        diag::CALL_ARITY,
                        span,
                        format!(
                            "closure expects {} argument(s), found {}",
                            params.len(),
                            arguments.len()
                        ),
                    );
                    return self.err_expr(span);
                }
                let mut args = Vec::new();
                for (a, p) in arguments.iter().zip(&params) {
                    let v = self.infer(&a.value);
                    args.push(self.coerce_arg(v, *p, a.span));
                }
                Expr {
                    kind: ExprKind::ClosureCall {
                        closure: Box::new(callee_expr),
                        mode,
                        args,
                    },
                    ty: result,
                    span,
                }
            } else if matches!(self.types.data(ct), TyData::Error) {
                self.err_expr(span)
            } else {
                self.unsupported("calls of values and methods", span);
                self.err_expr(span)
            }
        }
    }

    fn foreign_call(&mut self, f: ForeignRef, arguments: &[ast::Argument], span: Span) -> Expr {
        if self.in_unsafe == 0 {
            self.error(
                diag::FFI_UNSAFE,
                span,
                "calling an `extern \"C\"` function requires an `unsafe` block",
            );
        }
        let decl: ForeignDeclInfo = self.items.foreign(f).clone();
        if arguments.len() != decl.params.len() {
            self.error(
                diag::CALL_ARITY,
                span,
                format!(
                    "`{}` expects {} argument(s), found {}",
                    decl.name,
                    decl.params.len(),
                    arguments.len()
                ),
            );
            return self.err_expr(span);
        }
        let mut args = Vec::new();
        for (a, p) in arguments.iter().zip(&decl.params) {
            let v = self.infer(&a.value);
            args.push(self.coerce_value(v, *p, a.span));
        }
        Expr {
            kind: ExprKind::ForeignCall { callee: f, args },
            ty: decl.result,
            span,
        }
    }

    fn builtin_call(
        &mut self,
        name: &str,
        explicit: Option<&[ast::GenericArg]>,
        arguments: &[ast::Argument],
        span: Span,
    ) -> Option<Expr> {
        let op = match name {
            "bytes_new" => IntrinsicOp::ByteBufferNew,
            "bytes_reserve" => IntrinsicOp::ByteBufferReserve,
            "bytes_push" => IntrinsicOp::ByteBufferPush,
            "bytes_truncate" => IntrinsicOp::ByteBufferTruncate,
            "bytes_len" => IntrinsicOp::ByteBufferLen,
            "bytes_capacity" => IntrinsicOp::ByteBufferCapacity,
            "store_new" => IntrinsicOp::StoreNew,
            "store_reserve" => IntrinsicOp::StoreReserve,
            "store_push" => IntrinsicOp::StorePush,
            "store_pop" => IntrinsicOp::StorePop,
            "store_len" => IntrinsicOp::StoreLen,
            "store_capacity" => IntrinsicOp::StoreCapacity,
            "store_at" => IntrinsicOp::StoreBorrow {
                mutability: Mutability::Shared,
            },
            "store_at_mut" => IntrinsicOp::StoreBorrow {
                mutability: Mutability::Exclusive,
            },
            "bytes_view" => IntrinsicOp::ByteBufferBorrow,
            _ => return None,
        };
        if explicit.is_some() {
            self.error(
                diag::TYPE_ARGUMENT,
                span,
                format!("`{name}` takes no explicit type arguments"),
            );
        }
        let i64_ty = self.types.int(IntTy::I64);
        let u8_ty = self.types.int(IntTy::U8);
        let bool_ty = self.types.bool();
        let unit_ty = self.types.unit();
        let bytes_inner = self.types.intern(TyData::Opaque(OpaqueKind::ByteBuffer));
        let bytes_desc = self.types.intern(TyData::Desc(bytes_inner));
        let bytes_managed = self.types.intern(TyData::Managed(bytes_inner));

        let (param_tys, ret_ty): (Vec<Ty>, Ty) = match op {
            IntrinsicOp::ByteBufferNew => (Vec::new(), bytes_managed),
            IntrinsicOp::ByteBufferReserve | IntrinsicOp::ByteBufferTruncate => {
                (vec![bytes_desc, i64_ty], bool_ty)
            }
            IntrinsicOp::ByteBufferPush => (vec![bytes_desc, u8_ty], bool_ty),
            IntrinsicOp::ByteBufferLen | IntrinsicOp::ByteBufferCapacity => {
                (vec![bytes_desc], i64_ty)
            }
            IntrinsicOp::StoreNew => {
                let elem = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::OwnedStore(elem)));
                let managed = self.types.intern(TyData::Managed(inner));
                (Vec::new(), managed)
            }
            IntrinsicOp::StoreReserve => {
                let elem = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::OwnedStore(elem)));
                let desc = self.types.intern(TyData::Desc(inner));
                (vec![desc, i64_ty], bool_ty)
            }
            IntrinsicOp::StorePush => {
                let elem = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::OwnedStore(elem)));
                let desc = self.types.intern(TyData::Desc(inner));
                let res = self.types.intern(TyData::ErrorUnion {
                    ok: unit_ty,
                    err: elem,
                });
                (vec![desc, elem], res)
            }
            IntrinsicOp::StorePop => {
                let elem = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::OwnedStore(elem)));
                let desc = self.types.intern(TyData::Desc(inner));
                let res = self.types.intern(TyData::ErrorUnion {
                    ok: elem,
                    err: unit_ty,
                });
                (vec![desc], res)
            }
            IntrinsicOp::StoreLen | IntrinsicOp::StoreCapacity => {
                let elem = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::OwnedStore(elem)));
                let desc = self.types.intern(TyData::Desc(inner));
                (vec![desc], i64_ty)
            }
            IntrinsicOp::StoreBorrow { mutability } => {
                let elem = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::OwnedStore(elem)));
                let desc = self.types.intern(TyData::Desc(inner));
                let res = self.types.intern(TyData::Ref {
                    mutable: mutability == Mutability::Exclusive,
                    pointee: elem,
                });
                (vec![desc, i64_ty], res)
            }
            IntrinsicOp::ByteBufferBorrow => {
                let slice_u8 = self.types.intern(TyData::Slice(u8_ty));
                let res = self.types.intern(TyData::Ref {
                    mutable: false,
                    pointee: slice_u8,
                });
                (vec![bytes_desc], res)
            }
            IntrinsicOp::MutexCreate
            | IntrinsicOp::MutexLock
            | IntrinsicOp::MutexValue
            | IntrinsicOp::MutexUnlock
            | IntrinsicOp::SyncLoad
            | IntrinsicOp::SyncStore
            | IntrinsicOp::SyncFetchAdd
            | IntrinsicOp::SyncCompareExchange
            | IntrinsicOp::TaskStart
            | IntrinsicOp::TaskCreate
            | IntrinsicOp::TaskYield
            | IntrinsicOp::TaskValid
            | IntrinsicOp::TaskPoll
            | IntrinsicOp::TaskRequestCancel
            | IntrinsicOp::TaskPollClose
            | IntrinsicOp::TaskFinishDestroy
            | IntrinsicOp::RegistrationStart
            | IntrinsicOp::RegistrationValid
            | IntrinsicOp::RegistrationClose
            | IntrinsicOp::AsyncRegistrationStart
            | IntrinsicOp::AsyncRegistrationValid
            | IntrinsicOp::AsyncRegistrationRequestClose
            | IntrinsicOp::AsyncRegistrationPollClose
            | IntrinsicOp::AsyncRegistrationFinishDestroy
            | IntrinsicOp::ManagedCreate => return None,
        };
        if arguments.len() != param_tys.len() {
            self.error(
                diag::CALL_ARITY,
                span,
                format!(
                    "`{name}` expects {} argument(s), found {}",
                    param_tys.len(),
                    arguments.len()
                ),
            );
            return Some(self.err_expr(span));
        }
        let mut args = Vec::new();
        for (a, p) in arguments.iter().zip(&param_tys) {
            let v = self.infer(&a.value);
            args.push(self.coerce_value(v, *p, a.span));
        }
        Some(Expr {
            kind: ExprKind::Intrinsic { op, args },
            ty: ret_ty,
            span,
        })
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
            ExprKind::SliceBorrow {
                place, start, end, ..
            } => {
                walk(place)
                    || start.as_deref().is_some_and(walk)
                    || end.as_deref().is_some_and(walk)
            }
            ExprKind::Deref(inner)
            | ExprKind::Borrow { place: inner, .. }
            | ExprKind::Len(inner)
            | ExprKind::Give(inner)
            | ExprKind::Describe(inner)
            | ExprKind::ErrorInject { value: inner, .. }
            | ExprKind::Try(inner)
            | ExprKind::CtEval(inner) => walk(inner),
            ExprKind::Lit(_)
            | ExprKind::Local(_)
            | ExprKind::Capture(_)
            | ExprKind::Continue
            | ExprKind::Field { .. }
            | ExprKind::TupleField { .. }
            | ExprKind::Index { .. }
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Logical { .. }
            | ExprKind::Convert { .. }
            | ExprKind::FnRef(_)
            | ExprKind::Call { .. }
            | ExprKind::IndirectCall { .. }
            | ExprKind::Closure { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::TraitCall { .. }
            | ExprKind::ForeignCall { .. }
            | ExprKind::Intrinsic { .. }
            | ExprKind::ManagedCreate { .. }
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
        for c in &mut f.closures {
            let span = c.body.span;
            for t in &mut c.capture_tys {
                *t = self.zonk(*t, span);
            }
            for t in &mut c.param_tys {
                *t = self.zonk(*t, span);
            }
            c.result = self.zonk(c.result, span);
            for l in &mut c.locals {
                let lspan = l.span;
                l.ty = self.zonk(l.ty, lspan);
            }
            self.expr(&mut c.body);
        }
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
            TyData::Slice(_) | TyData::SelfParam => false,
            TyData::Ref { mutable, .. } => !mutable,
            TyData::Desc(_) | TyData::FnPtr { .. } => true,
            TyData::Closure { .. } | TyData::ImplCallable { .. } => false,
            TyData::Managed(_) => false,
            TyData::Opaque(OpaqueKind::ForeignHandle) => true,
            TyData::Opaque(OpaqueKind::ByteBuffer | OpaqueKind::OwnedStore(_)) => false,
            TyData::ErrorUnion { ok, err } => {
                self.is_copy_depth(ok, depth + 1) && self.is_copy_depth(err, depth + 1)
            }
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
            PatKind::ErrorArm { inner, .. } => self.pat(inner),
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
            ExprKind::Local(_)
            | ExprKind::Capture(_)
            | ExprKind::FnRef(_)
            | ExprKind::Break
            | ExprKind::Continue => {}
            ExprKind::Field { base, args, .. } => {
                self.expr(base);
                for a in args.iter_mut() {
                    *a = self.zonk(*a, span);
                }
            }
            ExprKind::TupleField { base, .. } => self.expr(base),
            ExprKind::SliceBorrow {
                place, start, end, ..
            } => {
                self.expr(place);
                if let Some(st) = start {
                    self.expr(st);
                }
                if let Some(en) = end {
                    self.expr(en);
                }
            }
            ExprKind::Deref(inner)
            | ExprKind::Borrow { place: inner, .. }
            | ExprKind::Len(inner)
            | ExprKind::Give(inner)
            | ExprKind::Describe(inner)
            | ExprKind::ErrorInject { value: inner, .. }
            | ExprKind::Try(inner)
            | ExprKind::CtEval(inner) => self.expr(inner),
            ExprKind::Index { base, index } => {
                self.expr(base);
                self.expr(index);
                let is_int = matches!(self.data(index.ty), TyData::Int(_) | TyData::Error);
                if !is_int {
                    self.operator_error(index.span, "an index must be an integer");
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
                    if g.store_element
                        && matches!(self.data(*a), TyData::Ref { .. } | TyData::Desc(_))
                    {
                        self.diags.push(diag::error(
                            diag::GENERIC_BOUND,
                            span,
                            format!("type argument for `{}` must satisfy StoreElement", g.name),
                        ));
                    }
                    if g.copy && !self.is_copy(*a) {
                        self.diags.push(diag::error(
                            diag::GENERIC_BOUND,
                            span,
                            format!("type argument for `{}` must be Copy", g.name),
                        ));
                    }
                    for (tr_ref, tr_args) in &g.traits {
                        if !matches!(self.data(*a), TyData::Param(_) | TyData::Error) {
                            let resolved_args: Vec<Ty> = tr_args
                                .iter()
                                .map(|t| self.types.subst(*t, type_args))
                                .collect();
                            let found = self.items.trait_impls.iter().any(|imp| {
                                imp.trait_ref == *tr_ref
                                    && imp.target == *a
                                    && imp.trait_args == resolved_args
                            });
                            if !found {
                                let tname = &self.items.trait_decl(*tr_ref).name;
                                self.diags.push(diag::error(
                                    diag::GENERIC_BOUND,
                                    span,
                                    format!(
                                        "type argument for `{}` does not implement `{tname}`",
                                        g.name
                                    ),
                                ));
                            }
                        }
                    }
                }
                args.iter_mut().for_each(|a| self.expr(a));
            }
            ExprKind::IndirectCall { callee, args } => {
                self.expr(callee);
                args.iter_mut().for_each(|a| self.expr(a));
            }
            ExprKind::Closure { captures, .. } => {
                captures.iter_mut().for_each(|c| self.expr(c));
            }
            ExprKind::ClosureCall { closure, args, .. } => {
                self.expr(closure);
                args.iter_mut().for_each(|a| self.expr(a));
            }
            ExprKind::TraitCall {
                trait_args,
                self_ty,
                args,
                ..
            } => {
                for a in trait_args.iter_mut() {
                    *a = self.zonk(*a, span);
                }
                *self_ty = self.zonk(*self_ty, span);
                args.iter_mut().for_each(|a| self.expr(a));
            }
            ExprKind::ForeignCall { args, .. } | ExprKind::Intrinsic { args, .. } => {
                args.iter_mut().for_each(|a| self.expr(a));
            }
            ExprKind::ManagedCreate { args, fields, .. } | ExprKind::Adt { args, fields, .. } => {
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
