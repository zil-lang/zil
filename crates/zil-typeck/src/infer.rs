//! infer: name resolution inside bodies and type inference, producing THIR.
//!
//! Inference is equality-based: every expression gets a type, possibly an
//! inference variable, and constraints are solved by unification as they
//! arise. Unsuffixed literals carry kinded variables that default at the
//! end. `finalize` then zonks every type and applies the typing rules that
//! need closed types (operator domains, literal ranges, bounds).

use std::collections::BTreeSet;

use crate::diag;
use crate::items::{
    ConstRef, FnDecl, ForeignDeclInfo, ItemRef, Items, ModIdx, VariantShape,
    resolve_type_with_self, scalar_type,
};
use crate::literal;
use crate::thir::*;
use crate::ty::{AdtRef, ClosureRef, OpaqueKind, Ty, TyData, Types, VarKind};
use zil_base::{Diagnostic, Span};
use zil_ir::body::{
    AdtKind, BinOp, ClosureCallMode, ClosureKind, IntTy, IntrinsicOp, Mutability, UnOp,
};
use zil_syntax::ast;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CaptureSource {
    Local(LocalId),
    OuterCapture(u32),
}

struct CaptureEntry {
    source: CaptureSource,
    orig_ty: Ty,
    by_value: bool,
    reborrow_mut: bool,
    mutable: bool,
    mutated: bool,
    consumed: bool,
    field_ty: Ty,
}

#[derive(Default)]
struct ClosureUsage {
    mutated: std::collections::HashSet<String>,
    moved: std::collections::HashSet<String>,
    given: std::collections::HashSet<String>,
    called: std::collections::HashSet<String>,
    direct_methods: Vec<(String, String)>,
}

struct CaptureEnv {
    /// Visible locals in the immediately enclosing function or closure: `(name, local, ty, mutable)`.
    outer: Vec<(String, LocalId, Ty, bool)>,
    move_capture: bool,
    usage: ClosureUsage,
    captures: Vec<CaptureEntry>,
}

pub struct FnCx<'c, 'a, 'e> {
    items: &'c Items<'a>,
    types: &'c mut Types,
    diags: &'c mut Vec<Diagnostic>,
    completed_thir: &'c [Option<ThirFn>],
    ct: Option<&'e mut dyn crate::ConstEvaluator>,
    limits: crate::build::Limits,
    fn_ref: FnRef,
    decl: &'c FnDecl<'a>,
    current_module: ModIdx,
    const_eval_stack: Vec<ConstRef>,
    opaque_return: Option<ClosureRef>,
    generic_names: Vec<String>,
    ret_ty: Ty,
    locals: Vec<LocalInfo>,
    scopes: Vec<Vec<(String, LocalId)>>,
    closures: Vec<ThirClosure>,
    capture_stack: Vec<CaptureEnv>,
    loop_depth: u32,
    defer_depth: u32,
    in_unsafe: u32,
    meta_yield_stack: Vec<MetaYieldState>,
}

struct MetaYieldState {
    yield_ty: Option<Ty>,
    has_yield: bool,
}

/// Infers one function body against its declared signature.
#[allow(clippy::too_many_arguments)]
pub fn infer_fn<'a>(
    items: &Items<'a>,
    types: &mut Types,
    diags: &mut Vec<Diagnostic>,
    completed_thir: &[Option<ThirFn>],
    ct: Option<&mut dyn crate::ConstEvaluator>,
    limits: crate::build::Limits,
    f: FnRef,
) -> Option<ThirFn> {
    let decl = items.func(f);
    let errors_before = diags.len();
    let mut cx = FnCx {
        items,
        types,
        diags,
        completed_thir,
        ct,
        limits,
        fn_ref: f,
        decl,
        current_module: decl.module,
        const_eval_stack: Vec::new(),
        opaque_return: None,
        generic_names: decl.generics.iter().map(|g| g.name.clone()).collect(),
        ret_ty: decl.result,
        locals: Vec::new(),
        scopes: vec![Vec::new()],
        closures: Vec::new(),
        capture_stack: Vec::new(),
        loop_depth: 0,
        defer_depth: 0,
        in_unsafe: 0,
        meta_yield_stack: Vec::new(),
    };
    let mut params = Vec::new();
    for p in &decl.params {
        params.push(cx.declare(&p.name, p.ty, false, p.span));
    }
    let body = match &decl.ast.body {
        ast::FunctionBody::Block(b) => cx.block(b),
        ast::FunctionBody::Expression(e) => {
            let value = cx.infer_expected(e, decl.result);
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
    let explicit_direct =
        decl.ast.contract.iter().any(|c| {
            c.name == "effects" && !c.terms.iter().any(|t| t.path.as_slice() == ["suspend"])
        });
    Finalizer {
        items,
        types,
        diags,
        generics: &decl.generics,
        explicit_direct,
        reported_inference: false,
    }
    .function(&mut thir);
    (diags.len() == errors_before).then_some(thir)
}

impl<'c, 'a, 'e> FnCx<'c, 'a, 'e> {
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

    fn infer_const_ref(&mut self, cid: ConstRef, span: Span) -> Expr {
        if self.const_eval_stack.contains(&cid) {
            self.error(
                diag::NAME_UNRESOLVED,
                span,
                format!("cyclic constant `{}`", self.items.const_decl(cid).name),
            );
            return self.err_expr(span);
        }
        let info = self.items.const_decl(cid);
        let Some(value_ast) = info.value else {
            self.error(
                diag::NAME_UNRESOLVED,
                span,
                format!("constant `{}` has no value", info.name),
            );
            return self.err_expr(span);
        };
        let target_mod = info.module;
        let expected_ty = info.ty;
        self.const_eval_stack.push(cid);
        let saved_mod = self.current_module;
        let saved_scopes = std::mem::take(&mut self.scopes);
        let saved_cap = std::mem::take(&mut self.capture_stack);
        self.current_module = target_mod;
        self.scopes.push(Vec::new());
        let mut expr = if let Some(ty) = expected_ty {
            self.infer_expected(value_ast, ty)
        } else {
            self.infer(value_ast)
        };
        if let Some(ty) = expected_ty {
            expr = self.coerce_value(expr, ty, span);
        }
        expr.span = span;
        self.current_module = saved_mod;
        self.scopes = saved_scopes;
        self.capture_stack = saved_cap;
        self.const_eval_stack.pop();
        expr
    }

    fn infer_slice_borrow(
        &mut self,
        object: &ast::Expr,
        start: Option<&ast::Expr>,
        end: Option<&ast::Expr>,
        mutable: bool,
        span: Span,
    ) -> Expr {
        let base = self.infer(object);
        let base = self.auto_deref(base);
        if mutable && is_place(&base.kind) {
            self.check_mutable_place(&base, "borrowed exclusively");
        }
        let usize_ty = self.types.int(IntTy::Usize);
        let start = start.map(|e| {
            let idx = self.infer(e);
            let it = self.types.shallow(idx.ty);
            if let TyData::Var(_) = self.types.data(it) {
                let _ = self.types.unify(idx.ty, usize_ty);
            } else if !matches!(self.types.data(it), TyData::Int(_) | TyData::Error) {
                self.error(diag::OPERATOR_TYPE, e.span, "slice bounds must be integers");
            }
            Box::new(idx)
        });
        let end = end.map(|e| {
            let idx = self.infer(e);
            let it = self.types.shallow(idx.ty);
            if let TyData::Var(_) = self.types.data(it) {
                let _ = self.types.unify(idx.ty, usize_ty);
            } else if !matches!(self.types.data(it), TyData::Int(_) | TyData::Error) {
                self.error(diag::OPERATOR_TYPE, e.span, "slice bounds must be integers");
            }
            Box::new(idx)
        });
        let bt = self.types.shallow(base.ty);
        let elem =
            if let TyData::Array { element, .. } | TyData::Slice(element) = *self.types.data(bt) {
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
            region: None,
            mutable,
            pointee: slice_ty,
        });
        Expr {
            kind: ExprKind::SliceBorrow {
                mutable,
                place: Box::new(base),
                start,
                end,
            },
            ty,
            span,
        }
    }

    fn infer_collection_for(
        &mut self,
        pattern: &ast::Pattern,
        iterable: &ast::Expr,
        body: &ast::Block,
        span: Span,
    ) -> Option<Stmt> {
        let unit = self.types.unit();
        let never = self.types.never();
        let usize_ty = self.types.int(IntTy::Usize);
        let i64_ty = self.types.int(IntTy::I64);
        let iter_expr = self.infer(iterable);
        let iter_ty = self.types.shallow(iter_expr.ty);
        let data = self.types.data(iter_ty).clone();

        // Case 1: Array `[T; N]`, `&[T; N]`, `&mut [T; N]`, Slice `[T]`, `&[T]`, `&mut [T]`.
        let seq_info: Option<(Ty, Option<u32>, bool)> =
            if let TyData::Array { element, length } = data {
                Some((element, Some(length), false))
            } else if let TyData::Slice(element) = data {
                Some((element, None, false))
            } else if let TyData::Ref { pointee, .. } = data {
                let p = self.types.data(self.types.shallow(pointee));
                if let TyData::Array { element, length } = *p {
                    Some((element, Some(length), true))
                } else if let TyData::Slice(element) = *p {
                    Some((element, None, true))
                } else {
                    None
                }
            } else {
                None
            };
        if let Some((elem_ty, fixed_len, needs_deref)) = seq_info {
            self.scopes.push(Vec::new());
            let src_local = self.declare("__for_src", iter_ty, false, iterable.span);
            let idx_local = self.declare("__for_idx", usize_ty, true, iterable.span);
            let len_local = self.declare("__for_len", usize_ty, false, iterable.span);

            let src_place = Expr {
                kind: ExprKind::Local(src_local),
                ty: iter_ty,
                span: iterable.span,
            };
            let idx_place = Expr {
                kind: ExprKind::Local(idx_local),
                ty: usize_ty,
                span: iterable.span,
            };
            let len_place = Expr {
                kind: ExprKind::Local(len_local),
                ty: usize_ty,
                span: iterable.span,
            };
            let base_place = if needs_deref {
                let inner_ty = if let TyData::Ref { pointee, .. } = *self.types.data(iter_ty) {
                    pointee
                } else {
                    iter_ty
                };
                Expr {
                    kind: ExprKind::Deref(Box::new(src_place.clone())),
                    ty: inner_ty,
                    span: iterable.span,
                }
            } else {
                src_place.clone()
            };
            let len_init = if let Some(n) = fixed_len {
                Expr {
                    kind: ExprKind::Lit(Lit::Int(i128::from(n))),
                    ty: usize_ty,
                    span: iterable.span,
                }
            } else {
                Expr {
                    kind: ExprKind::Convert {
                        operand: Box::new(Expr {
                            kind: ExprKind::Len(Box::new(base_place.clone())),
                            ty: i64_ty,
                            span: iterable.span,
                        }),
                    },
                    ty: usize_ty,
                    span: iterable.span,
                }
            };
            let cond = Expr {
                kind: ExprKind::Binary {
                    op: BinOp::Ge,
                    lhs: Box::new(idx_place.clone()),
                    rhs: Box::new(len_place),
                },
                ty: self.types.bool(),
                span: iterable.span,
            };
            let break_stmt = Stmt::Expr(Expr {
                kind: ExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(Expr {
                        kind: ExprKind::Block(vec![Stmt::Expr(Expr {
                            kind: ExprKind::Break,
                            ty: never,
                            span: iterable.span,
                        })]),
                        ty: unit,
                        span: iterable.span,
                    }),
                    otherwise: None,
                },
                ty: unit,
                span: iterable.span,
            });
            self.scopes.push(Vec::new());
            let pat = self.pattern(pattern, elem_ty, false);
            let bind_stmt = Stmt::Let {
                pattern: pat,
                init: Expr {
                    kind: ExprKind::Index {
                        base: Box::new(base_place),
                        index: Box::new(idx_place.clone()),
                    },
                    ty: elem_ty,
                    span: iterable.span,
                },
            };
            let inc_stmt = Stmt::Expr(Expr {
                kind: ExprKind::Assign {
                    place: Box::new(idx_place),
                    op: Some(BinOp::Add),
                    value: Box::new(Expr {
                        kind: ExprKind::Lit(Lit::Int(1)),
                        ty: usize_ty,
                        span: iterable.span,
                    }),
                },
                ty: unit,
                span: iterable.span,
            });
            self.loop_depth += 1;
            let body_expr = self.block(body);
            self.loop_depth -= 1;
            self.scopes.pop();
            let loop_expr = Expr {
                kind: ExprKind::Loop(Box::new(Expr {
                    kind: ExprKind::Block(vec![
                        break_stmt,
                        bind_stmt,
                        inc_stmt,
                        Stmt::Expr(body_expr),
                    ]),
                    ty: unit,
                    span: body.span,
                })),
                ty: unit,
                span,
            };
            self.scopes.pop();
            let outer_block = Expr {
                kind: ExprKind::Block(vec![
                    Stmt::Let {
                        pattern: Pat {
                            kind: PatKind::Binding(src_local),
                            ty: iter_ty,
                            span: iterable.span,
                        },
                        init: iter_expr,
                    },
                    Stmt::Let {
                        pattern: Pat {
                            kind: PatKind::Binding(idx_local),
                            ty: usize_ty,
                            span: iterable.span,
                        },
                        init: Expr {
                            kind: ExprKind::Lit(Lit::Int(0)),
                            ty: usize_ty,
                            span: iterable.span,
                        },
                    },
                    Stmt::Let {
                        pattern: Pat {
                            kind: PatKind::Binding(len_local),
                            ty: usize_ty,
                            span: iterable.span,
                        },
                        init: len_init,
                    },
                    Stmt::Expr(loop_expr),
                ]),
                ty: unit,
                span,
            };
            return Some(Stmt::Expr(outer_block));
        }

        // Case 2: `Managed<OwnedStore<T>>` or `Desc<OwnedStore<T>>`.
        let store_info: Option<(Ty, Ty, bool)> = if let TyData::Managed(inner) = data {
            if let TyData::Opaque(OpaqueKind::OwnedStore(elem)) =
                *self.types.data(self.types.shallow(inner))
            {
                Some((elem, inner, true))
            } else {
                None
            }
        } else if let TyData::Desc(inner) = data {
            if let TyData::Opaque(OpaqueKind::OwnedStore(elem)) =
                *self.types.data(self.types.shallow(inner))
            {
                Some((elem, inner, false))
            } else {
                None
            }
        } else {
            None
        };
        if let Some((elem_ty, inner_ty, wrap_desc)) = store_info {
            let desc_ty = self.types.intern(TyData::Desc(inner_ty));
            let desc_init = if wrap_desc {
                Expr {
                    kind: ExprKind::Describe(Box::new(iter_expr)),
                    ty: desc_ty,
                    span: iterable.span,
                }
            } else {
                iter_expr
            };
            self.scopes.push(Vec::new());
            let src_local = self.declare("__for_store", desc_ty, false, iterable.span);
            let idx_local = self.declare("__for_idx", i64_ty, true, iterable.span);
            let len_local = self.declare("__for_len", i64_ty, false, iterable.span);

            let src_place = Expr {
                kind: ExprKind::Local(src_local),
                ty: desc_ty,
                span: iterable.span,
            };
            let idx_place = Expr {
                kind: ExprKind::Local(idx_local),
                ty: i64_ty,
                span: iterable.span,
            };
            let len_place = Expr {
                kind: ExprKind::Local(len_local),
                ty: i64_ty,
                span: iterable.span,
            };
            let len_init = Expr {
                kind: ExprKind::Intrinsic {
                    op: IntrinsicOp::StoreLen,
                    args: vec![src_place.clone()],
                },
                ty: i64_ty,
                span: iterable.span,
            };
            let cond = Expr {
                kind: ExprKind::Binary {
                    op: BinOp::Ge,
                    lhs: Box::new(idx_place.clone()),
                    rhs: Box::new(len_place),
                },
                ty: self.types.bool(),
                span: iterable.span,
            };
            let break_stmt = Stmt::Expr(Expr {
                kind: ExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(Expr {
                        kind: ExprKind::Block(vec![Stmt::Expr(Expr {
                            kind: ExprKind::Break,
                            ty: never,
                            span: iterable.span,
                        })]),
                        ty: unit,
                        span: iterable.span,
                    }),
                    otherwise: None,
                },
                ty: unit,
                span: iterable.span,
            });
            let elem_ref_ty = self.types.intern(TyData::Ref {
                region: None,
                mutable: false,
                pointee: elem_ty,
            });
            let elem_ref = Expr {
                kind: ExprKind::Intrinsic {
                    op: IntrinsicOp::StoreBorrow {
                        mutability: Mutability::Shared,
                    },
                    args: vec![src_place, idx_place.clone()],
                },
                ty: elem_ref_ty,
                span: iterable.span,
            };
            let elem_val = Expr {
                kind: ExprKind::Deref(Box::new(elem_ref)),
                ty: elem_ty,
                span: iterable.span,
            };
            self.scopes.push(Vec::new());
            let pat = self.pattern(pattern, elem_ty, false);
            let bind_stmt = Stmt::Let {
                pattern: pat,
                init: elem_val,
            };
            let inc_stmt = Stmt::Expr(Expr {
                kind: ExprKind::Assign {
                    place: Box::new(idx_place),
                    op: Some(BinOp::Add),
                    value: Box::new(Expr {
                        kind: ExprKind::Lit(Lit::Int(1)),
                        ty: i64_ty,
                        span: iterable.span,
                    }),
                },
                ty: unit,
                span: iterable.span,
            });
            self.loop_depth += 1;
            let body_expr = self.block(body);
            self.loop_depth -= 1;
            self.scopes.pop();
            let loop_expr = Expr {
                kind: ExprKind::Loop(Box::new(Expr {
                    kind: ExprKind::Block(vec![
                        break_stmt,
                        bind_stmt,
                        inc_stmt,
                        Stmt::Expr(body_expr),
                    ]),
                    ty: unit,
                    span: body.span,
                })),
                ty: unit,
                span,
            };
            self.scopes.pop();
            let outer_block = Expr {
                kind: ExprKind::Block(vec![
                    Stmt::Let {
                        pattern: Pat {
                            kind: PatKind::Binding(src_local),
                            ty: desc_ty,
                            span: iterable.span,
                        },
                        init: desc_init,
                    },
                    Stmt::Let {
                        pattern: Pat {
                            kind: PatKind::Binding(idx_local),
                            ty: i64_ty,
                            span: iterable.span,
                        },
                        init: Expr {
                            kind: ExprKind::Lit(Lit::Int(0)),
                            ty: i64_ty,
                            span: iterable.span,
                        },
                    },
                    Stmt::Let {
                        pattern: Pat {
                            kind: PatKind::Binding(len_local),
                            ty: i64_ty,
                            span: iterable.span,
                        },
                        init: len_init,
                    },
                    Stmt::Expr(loop_expr),
                ]),
                ty: unit,
                span,
            };
            return Some(Stmt::Expr(outer_block));
        }

        // Case 3: Any type with a `.next(&mut self) -> Option<T>` method (e.g. `std.iter`).
        self.scopes.push(Vec::new());
        let it_local = self.declare("__for_it", iter_ty, true, iterable.span);
        let diags_before = self.diags.len();
        let next_callee = ast::Expr {
            kind: ast::ExprKind::Member {
                object: Box::new(ast::Expr {
                    kind: ast::ExprKind::Name("__for_it".to_string()),
                    span: iterable.span,
                    semantic: ast::SemanticContextMarker::default(),
                }),
                member: ast::MemberName::Name("next".to_string()),
            },
            span: iterable.span,
            semantic: ast::SemanticContextMarker::default(),
        };
        let next_call = self.call(&next_callee, &[], iterable.span);
        let next_ty = self.types.shallow(next_call.ty);
        if self.diags.len() == diags_before
            && let TyData::Adt { adt, args, .. } = self.types.data(next_ty).clone()
            && adt == AdtRef(0)
            && args.len() == 1
        {
            let elem_ty = args[0];
            self.scopes.push(Vec::new());
            let pat = self.pattern(pattern, elem_ty, false);
            self.loop_depth += 1;
            let body_expr = self.block(body);
            self.loop_depth -= 1;
            self.scopes.pop();
            let match_expr = Expr {
                kind: ExprKind::Match {
                    scrutinee: Box::new(next_call),
                    arms: vec![
                        Arm {
                            pattern: Pat {
                                kind: PatKind::Adt {
                                    adt: AdtRef(0),
                                    variant: 1, // Option.Some(pat)
                                    args: args.clone(),
                                    fields: vec![pat],
                                },
                                ty: next_ty,
                                span: pattern.span,
                            },
                            guard: None,
                            guard_refs: Vec::new(),
                            body: body_expr,
                            span: body.span,
                        },
                        Arm {
                            pattern: Pat {
                                kind: PatKind::Adt {
                                    adt: AdtRef(0),
                                    variant: 0, // Option.None
                                    args,
                                    fields: Vec::new(),
                                },
                                ty: next_ty,
                                span: iterable.span,
                            },
                            guard: None,
                            guard_refs: Vec::new(),
                            body: Expr {
                                kind: ExprKind::Break,
                                ty: never,
                                span: iterable.span,
                            },
                            span: iterable.span,
                        },
                    ],
                },
                ty: unit,
                span,
            };
            let loop_expr = Expr {
                kind: ExprKind::Loop(Box::new(match_expr)),
                ty: unit,
                span,
            };
            self.scopes.pop();
            let outer_block = Expr {
                kind: ExprKind::Block(vec![
                    Stmt::Let {
                        pattern: Pat {
                            kind: PatKind::Binding(it_local),
                            ty: iter_ty,
                            span: iterable.span,
                        },
                        init: iter_expr,
                    },
                    Stmt::Expr(loop_expr),
                ]),
                ty: unit,
                span,
            };
            return Some(Stmt::Expr(outer_block));
        }
        self.diags.truncate(diags_before);
        self.scopes.pop();
        self.error(
            diag::OPERATOR_TYPE,
            span,
            "for-in requires a range, array, slice, store, or iterator with `next() -> Option<T>`",
        );
        None
    }

    fn err_expr(&mut self, span: Span) -> Expr {
        let ty = self.types.error();
        Expr {
            kind: ExprKind::Lit(Lit::Unit),
            ty,
            span,
        }
    }

    fn normalize(&mut self, ty: Ty) -> Ty {
        self.items
            .normalize_ty_in_env(ty, &self.decl.generics, self.types)
    }

    fn coerce(&mut self, value: &Expr, expected: Ty, span: Span) {
        let got = self.normalize(value.ty);
        let exp = self.normalize(expected);
        if self.types.coerce(got, exp).is_err() {
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
        let et = self.normalize(expected);
        let vt = self.normalize(value.ty);
        let ed = self.types.data(et).clone();
        let vd = self.types.data(vt).clone();

        // 0. Return-position `impl Fn/FnMut/FnOnce` coercion.
        if let (
            TyData::ImplCallable {
                owner,
                kind: want_k,
                params: wp,
                result: wr,
                ..
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

        // 0b. Non-capturing closure -> function pointer coercion.
        if let (
            TyData::FnPtr {
                params: wp,
                result: wr,
            },
            TyData::Closure {
                id,
                params: gp,
                result: gr,
                ..
            },
        ) = (&ed, &vd)
            && matches!(&value.kind, ExprKind::Closure { captures, .. } if captures.is_empty())
            && gp.len() == wp.len()
        {
            for (g, w) in gp.iter().zip(wp) {
                self.unify(*g, *w, span);
            }
            self.unify(*gr, *wr, span);
            let vspan = value.span;
            return Expr {
                kind: ExprKind::ClosureFnRef(*id),
                ty: expected,
                span: vspan,
            };
        }

        // 1. Reborrow `&mut T` -> `&mut T` / `&T`.
        if let (
            TyData::Ref { mutable: want, .. },
            TyData::Ref {
                mutable: true,
                pointee,
                ..
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
                region: None,
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

        // 2. Auto-describe: `Managed<R>` or `&Managed<R>` -> `Desc<R>`, and `&Managed<R>` -> `&Desc<R>`.
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
        if let (
            TyData::Ref {
                mutable: false,
                pointee: want_pt,
                ..
            },
            TyData::Ref {
                pointee: got_pt, ..
            },
        ) = (&ed, &vd)
        {
            let want_pt = self.types.shallow(*want_pt);
            let got_pt = self.types.shallow(*got_pt);
            if let (TyData::Desc(want_inner), TyData::Managed(got_inner)) = (
                self.types.data(want_pt).clone(),
                self.types.data(got_pt).clone(),
            ) {
                self.unify(got_inner, want_inner, span);
                let vspan = value.span;
                let managed_place = if let ExprKind::Borrow { place, .. } = value.kind {
                    *place
                } else {
                    Expr {
                        kind: ExprKind::Deref(Box::new(value)),
                        ty: got_pt,
                        span: vspan,
                    }
                };
                let desc = Expr {
                    kind: ExprKind::Describe(Box::new(managed_place)),
                    ty: want_pt,
                    span: vspan,
                };
                return Expr {
                    kind: ExprKind::Borrow {
                        mutable: false,
                        place: Box::new(desc),
                    },
                    ty: expected,
                    span: vspan,
                };
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

        // 4. Implicit anonymous-union injection: `A | B`.
        if let TyData::Union(members) = ed
            && !matches!(vd, TyData::Union(_) | TyData::Error | TyData::Never)
        {
            let mut matched = Vec::new();
            for &m in &members {
                if self.types.can_unify(value.ty, m) {
                    matched.push(m);
                }
            }
            match matched.as_slice() {
                [target] => {
                    let _ = self.types.unify(value.ty, *target);
                    let vspan = value.span;
                    return Expr {
                        kind: ExprKind::UnionInject {
                            value: Box::new(value),
                        },
                        ty: expected,
                        span: vspan,
                    };
                }
                [] => {
                    self.error(
                        diag::TYPE_MISMATCH,
                        span,
                        "value does not match any member of anonymous union",
                    );
                    return self.err_expr(span);
                }
                _ => {
                    self.error(
                        diag::TYPE_MISMATCH,
                        span,
                        "value matches multiple members of anonymous union",
                    );
                    return self.err_expr(span);
                }
            }
        }

        self.coerce(&value, expected, span);
        value
    }

    fn unify(&mut self, a: Ty, b: Ty, span: Span) {
        let a = self.normalize(a);
        let b = self.normalize(b);
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
        if let ast::TypeKind::MetaEval(inner_ast) = &t.kind {
            let meta_ty = self.types.meta_type();
            let expr = self.infer_expected(inner_ast, meta_ty);
            let expr = self.coerce_value(expr, meta_ty, inner_ast.span);
            let Some(ct) = self.ct.as_deref_mut() else {
                self.unsupported("compile-time types", t.span);
                return self.types.error();
            };
            let mut expr = expr;
            expr.ty = self.types.zonk(expr.ty).unwrap_or(meta_ty);
            return crate::build::eval_ct_type_expr(
                self.items,
                self.types,
                self.completed_thir,
                self.fn_ref,
                &self.locals,
                &expr,
                self.diags,
                self.limits,
                ct,
                t.span,
            )
            .unwrap_or_else(|| self.types.error());
        }
        match resolve_type_with_self(
            t,
            self.decl.module,
            &self.generic_names,
            &self.decl.region_params,
            self.decl.self_ty,
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

    fn resolve_const_generic_arg(&mut self, expr: &ast::Expr) -> Ty {
        if let Some(n) = self.items.eval_const_u32(expr, self.current_module) {
            return self.types.intern(TyData::ConstU32(n));
        }
        if let ast::ExprKind::Name(cname) = &expr.kind
            && let Some(idx) = self.generic_names.iter().rposition(|g| g == cname)
        {
            return self.types.intern(TyData::ConstParam(idx as u32));
        }
        self.error(
            diag::ARRAY_LENGTH,
            expr.span,
            "const generic argument must be a compile-time usize constant",
        );
        self.types.error()
    }

    fn fresh_args(&mut self, adt: AdtRef) -> Vec<Ty> {
        let n = self.items.adt(adt).generics.len();
        (0..n).map(|_| self.types.fresh(VarKind::General)).collect()
    }

    fn adt_ty(&mut self, adt: AdtRef, args: &[Ty]) -> Ty {
        let region_params = self.items.adt(adt).region_params as usize;
        self.types.intern(TyData::Adt {
            adt,
            args: args.to_vec(),
            regions: vec![None; region_params],
        })
    }

    fn field_ty(&mut self, adt: AdtRef, variant: u32, field: u32, args: &[Ty]) -> Ty {
        let declared = self.items.adt(adt).variants[variant as usize].fields[field as usize].ty;
        let erased = self.types.erase_regions(declared);
        self.types.subst(erased, args)
    }

    /// `Enum.Variant`, `Record`, `mod.Record`, or `mod.Enum.Variant`.
    fn resolve_variant_path(&mut self, path: &[String], span: Span) -> Option<(AdtRef, u32)> {
        if path.len() == 1
            && path[0] == "Self"
            && let Some(self_ty) = self.decl.self_ty
            && let TyData::Adt { adt, .. } = *self.types.data(self.types.shallow(self_ty))
            && self.items.adt(adt).kind != AdtKind::Enum
        {
            return Some((adt, 0));
        }
        if let Some(adt) = self
            .items
            .resolve_adt_of(self.decl.module, path, self.types)
        {
            if self.items.adt(adt).kind != AdtKind::Enum {
                return Some((adt, 0));
            }
            self.error(diag::ENUM_VARIANT, span, "an enum needs a variant name");
            return None;
        }
        if path.len() >= 2 {
            let (enum_path, variant) = (&path[..path.len() - 1], &path[path.len() - 1]);
            let resolved_enum = if enum_path == ["Self"]
                && let Some(self_ty) = self.decl.self_ty
                && let TyData::Adt { adt, .. } = *self.types.data(self.types.shallow(self_ty))
                && self.items.adt(adt).kind == AdtKind::Enum
            {
                Some(adt)
            } else {
                self.items
                    .resolve_adt_of(self.decl.module, enum_path, self.types)
                    .filter(|&adt| self.items.adt(adt).kind == AdtKind::Enum)
            };
            if let Some(adt) = resolved_enum {
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
        let type_args: Vec<Ty> = (0..decl.generics.len())
            .map(|_| self.types.fresh(VarKind::General))
            .collect();
        let params: Vec<Ty> = decl
            .params
            .iter()
            .map(|p| self.types.subst(p.ty, &type_args))
            .collect();
        let result = self.types.subst(decl.result, &type_args);
        let ty = self.types.intern(TyData::FnPtr { params, result });
        Expr {
            kind: ExprKind::FnRef { func: f, type_args },
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
                    .capture_stack
                    .iter()
                    .rev()
                    .any(|env| env.outer.iter().any(|(name, ..)| name == n))
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
                Some(Stmt::Defer(_)) | None => None,
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
                let (init, declared) = match ty {
                    Some(t) => {
                        let t = self.resolve_written(t);
                        let mut init = self.infer_expected(value, t);
                        let tt = self.types.shallow(t);
                        let it = self.types.shallow(init.ty);
                        if let (TyData::Desc(want), TyData::Managed(got)) =
                            (self.types.data(tt).clone(), self.types.data(it).clone())
                        {
                            self.unify(got, want, value.span);
                            if is_place(&init.kind) {
                                let vspan = init.span;
                                init = Expr {
                                    kind: ExprKind::Describe(Box::new(init)),
                                    ty: t,
                                    span: vspan,
                                };
                                (init, t)
                            } else {
                                let ty = init.ty;
                                (init, ty)
                            }
                        } else {
                            init = self.coerce_value(init, t, value.span);
                            (init, t)
                        }
                    }
                    None => {
                        let mut init = self.infer(value);
                        let it = self.types.shallow(init.ty);
                        if is_place(&init.kind)
                            && let TyData::Managed(inner) = self.types.data(it).clone()
                        {
                            let desc_ty = self.types.intern(TyData::Desc(inner));
                            let vspan = init.span;
                            init = Expr {
                                kind: ExprKind::Describe(Box::new(init)),
                                ty: desc_ty,
                                span: vspan,
                            };
                            (init, desc_ty)
                        } else {
                            let ty = init.ty;
                            (init, ty)
                        }
                    }
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
                if !self.meta_yield_stack.is_empty() {
                    self.error(
                        diag::LOOP_CONTROL,
                        span,
                        "cannot `return` inside a `#{ ... }` compile-time block; use `yield` instead",
                    );
                    return None;
                }
                if self.defer_depth > 0 {
                    self.error(
                        diag::LOOP_CONTROL,
                        span,
                        "cannot `return` out of a `defer` block",
                    );
                    return None;
                }
                let result = self.ret_ty;
                let value = match value {
                    Some(v) => {
                        let inferred = self.infer_expected(v, result);
                        let v = self.coerce_value(inferred, result, span);
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
                                        guard: None,
                                        guard_refs: Vec::new(),
                                        body,
                                        span,
                                    },
                                    Arm {
                                        pattern: wild,
                                        guard: None,
                                        guard_refs: Vec::new(),
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
                    let mut guard_refs = Vec::new();
                    let guard = arm.guard.as_ref().map(|g| {
                        let mut ref_mut_locals = std::collections::HashSet::new();
                        collect_ref_mut_bindings(&pattern, &mut ref_mut_locals);
                        let arm_entries = self.scopes.last().cloned().unwrap_or_default();
                        self.scopes.push(Vec::new());
                        for (name, arm_local) in arm_entries {
                            let info = self.locals[arm_local.0 as usize].clone();
                            let ty = self.types.shallow(info.ty);
                            if ref_mut_locals.contains(&arm_local) {
                                if let TyData::Ref { pointee, .. } = *self.types.data(ty) {
                                    let guard_ref_ty = self.types.intern(TyData::Ref {
                                        region: None,
                                        mutable: false,
                                        pointee,
                                    });
                                    let guard_local =
                                        self.declare(&name, guard_ref_ty, false, info.span);
                                    guard_refs.push((arm_local, guard_local));
                                }
                            } else if !self.items.is_copy_in_env(
                                ty,
                                &self.decl.generics,
                                self.types,
                            ) {
                                let guard_ref_ty = self.types.intern(TyData::Ref {
                                    region: None,
                                    mutable: false,
                                    pointee: ty,
                                });
                                let guard_local =
                                    self.declare(&name, guard_ref_ty, false, info.span);
                                guard_refs.push((arm_local, guard_local));
                            }
                        }
                        let bool_ty = self.types.bool();
                        let ge = self.infer_expected(g, bool_ty);
                        let ge = self.coerce_value(ge, bool_ty, g.span);
                        self.scopes.pop();
                        Box::new(ge)
                    });
                    let body = match &arm.body {
                        ast::ArrowBody::Block(b) => self.block(b),
                        ast::ArrowBody::Expression(x) => self.infer(x),
                    };
                    self.scopes.pop();
                    all_never &= self.types.is_never(body.ty);
                    out.push(Arm {
                        pattern,
                        guard,
                        guard_refs,
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
                let rhs = if bin.is_none() {
                    let v = self.infer_expected(value, place.ty);
                    self.coerce_value(v, place.ty, value.span)
                } else {
                    let v = self.infer(value);
                    self.coerce(&v, place.ty, value.span);
                    v
                };
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
            ast::StmtKind::Yield(val_expr) => {
                if self.meta_yield_stack.is_empty() {
                    self.error(
                        diag::LOOP_CONTROL,
                        span,
                        "`yield` is only allowed inside a `#{ ... }` compile-time block",
                    );
                    return None;
                }
                let expected = self.meta_yield_stack.last().and_then(|s| s.yield_ty);
                let val = if let Some(exp) = expected {
                    let inferred = self.infer_expected(val_expr, exp);
                    self.coerce_value(inferred, exp, val_expr.span)
                } else {
                    self.infer(val_expr)
                };
                if let Some(state) = self.meta_yield_stack.last_mut() {
                    if state.yield_ty.is_none() {
                        state.yield_ty = Some(val.ty);
                    }
                    state.has_yield = true;
                }
                e(ExprKind::Return(Some(Box::new(val))), never)
            }
            ast::StmtKind::Defer(block) => {
                if self.defer_depth > 0 {
                    self.error(
                        diag::LOOP_CONTROL,
                        span,
                        "nested `defer` inside a `defer` block is not allowed",
                    );
                    return None;
                }
                let saved_loop = self.loop_depth;
                self.loop_depth = 0;
                self.defer_depth += 1;
                let expr = if block.statements.len() == 1
                    && let ast::StmtKind::Expression(act) = &block.statements[0].kind
                    && let ast::ExprKind::Closure {
                        params,
                        body: ast::ArrowBody::Block(inner),
                        ..
                    } = &act.kind
                    && params.is_empty()
                {
                    self.block(inner)
                } else {
                    self.block(block)
                };
                self.defer_depth -= 1;
                self.loop_depth = saved_loop;
                Some(Stmt::Defer(expr))
            }
            ast::StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                if let ast::ExprKind::Binary {
                    op: op @ (ast::BinaryOp::Range | ast::BinaryOp::RangeInclusive),
                    left,
                    right,
                } = &iterable.kind
                {
                    let inclusive = *op == ast::BinaryOp::RangeInclusive;
                    let start_expr = self.infer(left);
                    let end_expr = self.infer(right);
                    let iter_ty = start_expr.ty;
                    let end_expr = self.coerce_value(end_expr, iter_ty, right.span);

                    self.scopes.push(Vec::new());
                    let cur_local = self.declare("__for_cur", iter_ty, true, left.span);
                    let end_local = self.declare("__for_end", iter_ty, false, right.span);
                    let bool_ty = self.types.bool();
                    let done_local = if inclusive {
                        Some(self.declare("__for_done", bool_ty, true, iterable.span))
                    } else {
                        None
                    };

                    let cur_place = Expr {
                        kind: ExprKind::Local(cur_local),
                        ty: iter_ty,
                        span: left.span,
                    };
                    let end_place = Expr {
                        kind: ExprKind::Local(end_local),
                        ty: iter_ty,
                        span: right.span,
                    };
                    let cond = if let Some(dl) = done_local {
                        Expr {
                            kind: ExprKind::Local(dl),
                            ty: bool_ty,
                            span: iterable.span,
                        }
                    } else {
                        Expr {
                            kind: ExprKind::Binary {
                                op: BinOp::Ge,
                                lhs: Box::new(cur_place.clone()),
                                rhs: Box::new(end_place.clone()),
                            },
                            ty: bool_ty,
                            span: iterable.span,
                        }
                    };
                    let break_stmt = Stmt::Expr(Expr {
                        kind: ExprKind::If {
                            cond: Box::new(cond),
                            then: Box::new(Expr {
                                kind: ExprKind::Block(vec![Stmt::Expr(Expr {
                                    kind: ExprKind::Break,
                                    ty: never,
                                    span: iterable.span,
                                })]),
                                ty: unit,
                                span: iterable.span,
                            }),
                            otherwise: None,
                        },
                        ty: unit,
                        span: iterable.span,
                    });

                    self.scopes.push(Vec::new());
                    let pat = self.pattern(pattern, iter_ty, false);
                    let bind_stmt = Stmt::Let {
                        pattern: pat,
                        init: cur_place.clone(),
                    };
                    let one_ty = self.types.fresh(VarKind::Int);
                    let _ = self.types.unify(one_ty, iter_ty);
                    let inc_only = Expr {
                        kind: ExprKind::Assign {
                            place: Box::new(cur_place.clone()),
                            op: Some(BinOp::Add),
                            value: Box::new(Expr {
                                kind: ExprKind::Lit(Lit::Int(1)),
                                ty: iter_ty,
                                span: iterable.span,
                            }),
                        },
                        ty: unit,
                        span: iterable.span,
                    };
                    let inc_stmt = if let Some(dl) = done_local {
                        Stmt::Expr(Expr {
                            kind: ExprKind::If {
                                cond: Box::new(Expr {
                                    kind: ExprKind::Binary {
                                        op: BinOp::Eq,
                                        lhs: Box::new(cur_place.clone()),
                                        rhs: Box::new(end_place.clone()),
                                    },
                                    ty: bool_ty,
                                    span: iterable.span,
                                }),
                                then: Box::new(Expr {
                                    kind: ExprKind::Block(vec![Stmt::Expr(Expr {
                                        kind: ExprKind::Assign {
                                            place: Box::new(Expr {
                                                kind: ExprKind::Local(dl),
                                                ty: bool_ty,
                                                span: iterable.span,
                                            }),
                                            op: None,
                                            value: Box::new(Expr {
                                                kind: ExprKind::Lit(Lit::Bool(true)),
                                                ty: bool_ty,
                                                span: iterable.span,
                                            }),
                                        },
                                        ty: unit,
                                        span: iterable.span,
                                    })]),
                                    ty: unit,
                                    span: iterable.span,
                                }),
                                otherwise: Some(Box::new(Expr {
                                    kind: ExprKind::Block(vec![Stmt::Expr(inc_only)]),
                                    ty: unit,
                                    span: iterable.span,
                                })),
                            },
                            ty: unit,
                            span: iterable.span,
                        })
                    } else {
                        Stmt::Expr(inc_only)
                    };

                    self.loop_depth += 1;
                    let body_expr = self.block(body);
                    self.loop_depth -= 1;
                    self.scopes.pop();

                    let loop_expr = Expr {
                        kind: ExprKind::Loop(Box::new(Expr {
                            kind: ExprKind::Block(vec![
                                break_stmt,
                                bind_stmt,
                                inc_stmt,
                                Stmt::Expr(body_expr),
                            ]),
                            ty: unit,
                            span: body.span,
                        })),
                        ty: unit,
                        span,
                    };
                    self.scopes.pop();

                    let mut outer_stmts = vec![
                        Stmt::Let {
                            pattern: Pat {
                                kind: PatKind::Binding(cur_local),
                                ty: iter_ty,
                                span: left.span,
                            },
                            init: start_expr,
                        },
                        Stmt::Let {
                            pattern: Pat {
                                kind: PatKind::Binding(end_local),
                                ty: iter_ty,
                                span: right.span,
                            },
                            init: end_expr,
                        },
                    ];
                    if let Some(dl) = done_local {
                        outer_stmts.push(Stmt::Let {
                            pattern: Pat {
                                kind: PatKind::Binding(dl),
                                ty: bool_ty,
                                span: iterable.span,
                            },
                            init: Expr {
                                kind: ExprKind::Binary {
                                    op: BinOp::Gt,
                                    lhs: Box::new(cur_place),
                                    rhs: Box::new(end_place),
                                },
                                ty: bool_ty,
                                span: iterable.span,
                            },
                        });
                    }
                    outer_stmts.push(Stmt::Expr(loop_expr));
                    let outer_block = Expr {
                        kind: ExprKind::Block(outer_stmts),
                        ty: unit,
                        span,
                    };
                    Some(Stmt::Expr(outer_block))
                } else {
                    self.infer_collection_for(pattern, iterable, body, span)
                }
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
                Some(Stmt::Let { .. } | Stmt::Defer(_)) | None => self.err_expr(s.span),
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
                                guard: None,
                                guard_refs: Vec::new(),
                                body: then,
                                span,
                            },
                            Arm {
                                pattern: wild,
                                guard: None,
                                guard_refs: Vec::new(),
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
                    if let Some(env) = self.capture_stack.last_mut()
                        && let Some(cap) = env.captures.get_mut(*idx as usize)
                    {
                        if !cap.mutable {
                            self.error(
                                diag::ASSIGN_IMMUTABLE,
                                place.span,
                                format!("captured variable cannot be {what}"),
                            );
                        } else {
                            cap.mutated = true;
                        }
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
                    } else if let ExprKind::Capture(idx) = base.kind
                        && let Some(env) = self.capture_stack.last_mut()
                        && let Some(cap) = env.captures.get_mut(idx as usize)
                    {
                        cap.mutated = true;
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
                | ExprKind::UnionInject { .. }
                | ExprKind::Try(_)
                | ExprKind::CtEval(_)
                | ExprKind::Lit(_)
                | ExprKind::ConstGeneric(_)
                | ExprKind::StaticUtf(_)
                | ExprKind::Unary { .. }
                | ExprKind::Binary { .. }
                | ExprKind::Logical { .. }
                | ExprKind::Convert { .. }
                | ExprKind::FnRef { .. }
                | ExprKind::ForeignRef(_)
                | ExprKind::Call { .. }
                | ExprKind::IndirectCall { .. }
                | ExprKind::Closure { .. }
                | ExprKind::ClosureFnRef(_)
                | ExprKind::ClosureCall { .. }
                | ExprKind::TraitCall { .. }
                | ExprKind::DynCall { .. }
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
                | ExprKind::Trap
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
        self.pattern_with_ref(p, expected, mutable, None)
    }

    fn pattern_with_ref(
        &mut self,
        p: &ast::Pattern,
        expected: Ty,
        mutable: bool,
        ref_mode: Option<bool>,
    ) -> Pat {
        let span = p.span;
        let kind = match &p.kind {
            ast::PatternKind::Name(n) => match ref_mode {
                Some(is_mut) => {
                    let ref_ty = self.types.intern(TyData::Ref {
                        region: None,
                        mutable: is_mut,
                        pointee: expected,
                    });
                    PatKind::RefBinding {
                        local: self.declare(n, ref_ty, mutable, span),
                        mutable: is_mut,
                    }
                }
                None => PatKind::Binding(self.declare(n, expected, mutable, span)),
            },
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
                            .map(|(i, t)| self.pattern_with_ref(i, t, mutable, ref_mode))
                            .collect(),
                    )
                }
            }
            ast::PatternKind::Constructor { path, arguments } => {
                let et = self.types.shallow(expected);
                let et_norm = self.items.normalize_ty(et, self.types);
                if let TyData::Union(members) = self.types.data(et_norm).clone() {
                    if arguments.len() != 1 {
                        self.error(
                            diag::MATCH_ARITY,
                            span,
                            "anonymous union member pattern requires exactly one payload pattern",
                        );
                        return self.wild(expected, span);
                    }
                    let matched = self.match_union_member_path(path, &members);
                    match matched.as_slice() {
                        [member_ty] => {
                            let inner =
                                self.pattern_with_ref(&arguments[0], *member_ty, mutable, ref_mode);
                            PatKind::UnionArm {
                                member_ty: *member_ty,
                                inner: Box::new(inner),
                            }
                        }
                        [] => {
                            self.error(
                                diag::TYPE_MISMATCH,
                                span,
                                format!(
                                    "`{}` is not a member of the anonymous union",
                                    path.join(".")
                                ),
                            );
                            return self.wild(expected, span);
                        }
                        _ => {
                            self.error(
                                diag::TYPE_MISMATCH,
                                span,
                                format!(
                                    "member pattern `{}` is ambiguous in anonymous union",
                                    path.join(".")
                                ),
                            );
                            return self.wild(expected, span);
                        }
                    }
                } else {
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
                            self.pattern_with_ref(a, ft, mutable, ref_mode)
                        })
                        .collect();
                    PatKind::Adt {
                        adt,
                        variant,
                        args,
                        fields,
                    }
                }
            }
            ast::PatternKind::RecordConstructor { path, fields, rest } => {
                let Some((adt, variant)) = self.resolve_variant_path(path, span) else {
                    return self.wild(expected, span);
                };
                let args = self.fresh_args(adt);
                let t = self.adt_ty(adt, &args);
                self.unify(t, expected, span);
                let decl = self.items.adt(adt);
                let is_cross_mod = decl.kind != AdtKind::Enum && decl.module != self.decl.module;
                let adt_name = decl.name.clone();
                let decl_fields: Vec<(String, bool)> = decl.variants[variant as usize]
                    .fields
                    .iter()
                    .map(|f| (f.name.clone(), f.public))
                    .collect();
                let mut slots: Vec<Option<Pat>> = vec![None; decl_fields.len()];
                for f in fields {
                    let Some(i) = decl_fields.iter().position(|(n, _)| *n == f.name) else {
                        self.error(
                            diag::RECORD_UNKNOWN_FIELD,
                            f.span,
                            format!("no field `{}`", f.name),
                        );
                        continue;
                    };
                    if is_cross_mod && !decl_fields[i].1 {
                        self.error(
                            "E-VISIBILITY",
                            f.span,
                            format!("field `{}` on `{adt_name}` is private", f.name),
                        );
                        let ft = self.field_ty(adt, variant, i as u32, &args);
                        slots[i] = Some(self.wild(ft, f.span));
                        continue;
                    }
                    if slots[i].is_some() {
                        self.error(
                            diag::RECORD_DUPLICATE_FIELD,
                            f.span,
                            format!("field `{}` appears more than once", f.name),
                        );
                        continue;
                    }
                    let ft = self.field_ty(adt, variant, i as u32, &args);
                    slots[i] = Some(self.pattern_with_ref(&f.pattern, ft, mutable, ref_mode));
                }
                let mut out = Vec::new();
                for (i, s) in slots.into_iter().enumerate() {
                    match s {
                        Some(p) => out.push(p),
                        None => {
                            if !*rest {
                                if is_cross_mod && !decl_fields[i].1 {
                                    self.error(
                                        "E-VISIBILITY",
                                        span,
                                        format!(
                                            "field `{}` on `{adt_name}` is private",
                                            decl_fields[i].0
                                        ),
                                    );
                                } else {
                                    self.error(
                                        diag::RECORD_MISSING_FIELD,
                                        span,
                                        format!(
                                            "field `{}` is missing from the pattern",
                                            decl_fields[i].0
                                        ),
                                    );
                                }
                            }
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
            ast::PatternKind::Reference {
                mutable: is_mut,
                pattern: inner,
            } => {
                if ref_mode == Some(false) && *is_mut {
                    self.error(
                        diag::TYPE_MISMATCH,
                        span,
                        "cannot bind `ref mut` inside a shared `ref` pattern",
                    );
                }
                return self.pattern_with_ref(inner, expected, mutable, Some(*is_mut));
            }
            ast::PatternKind::Deref {
                mutable: pat_mut,
                pattern: inner,
            } => {
                let et = self.types.shallow(expected);
                if let TyData::Ref {
                    mutable: ty_mut,
                    pointee,
                    ..
                } = self.types.data(et).clone()
                {
                    if *pat_mut && !ty_mut {
                        self.error(
                            diag::TYPE_MISMATCH,
                            span,
                            "cannot match `&mut` pattern against a shared reference",
                        );
                        PatKind::Wild
                    } else if !*pat_mut && ty_mut {
                        self.error(
                            diag::TYPE_MISMATCH,
                            span,
                            "cannot match `&` pattern against a `&mut` reference",
                        );
                        PatKind::Wild
                    } else {
                        let inner_pat = self.pattern_with_ref(inner, pointee, mutable, ref_mode);
                        PatKind::Deref(Box::new(inner_pat))
                    }
                } else if matches!(self.types.data(et), TyData::Error) {
                    PatKind::Wild
                } else {
                    self.error(
                        diag::TYPE_MISMATCH,
                        span,
                        "reference pattern `&` requires a reference type",
                    );
                    PatKind::Wild
                }
            }
            ast::PatternKind::ErrorArm { is_error, pattern } => {
                let et = self.types.shallow(expected);
                if let TyData::ErrorUnion { ok, err } = self.types.data(et).clone() {
                    let arm_ty = if *is_error { err } else { ok };
                    let inner = self.pattern_with_ref(pattern, arm_ty, mutable, ref_mode);
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

    fn match_union_member_path(&self, path: &[String], members: &[Ty]) -> Vec<Ty> {
        let target_adt = match self.items.lookup_item(self.current_module, path) {
            Some(ItemRef::Adt(adt)) => Some(adt),
            _ => None,
        };
        let single = if path.len() == 1 {
            Some(path[0].as_str())
        } else {
            None
        };
        let mut matched = Vec::new();
        for &m in members {
            let m_shallow = self.types.shallow(m);
            let core =
                if let TyData::Managed(inner) | TyData::Desc(inner) = self.types.data(m_shallow) {
                    self.types.shallow(*inner)
                } else {
                    m_shallow
                };
            let is_match = match self.types.data(core) {
                TyData::Unit => single == Some("void"),
                TyData::Bool => single == Some("bool"),
                TyData::Int(i) => {
                    let name = match i {
                        IntTy::I8 => "i8",
                        IntTy::I16 => "i16",
                        IntTy::I32 => "i32",
                        IntTy::I64 => "i64",
                        IntTy::Isize => "isize",
                        IntTy::U8 => "u8",
                        IntTy::U16 => "u16",
                        IntTy::U32 => "u32",
                        IntTy::U64 => "u64",
                        IntTy::Usize => "usize",
                    };
                    single == Some(name)
                }
                TyData::Float(f) => {
                    let name = match f {
                        zil_ir::body::FloatTy::F32 => "f32",
                        zil_ir::body::FloatTy::F64 => "f64",
                    };
                    single == Some(name)
                }
                TyData::Opaque(o) => match o {
                    OpaqueKind::ByteBuffer => single == Some("ByteBuffer"),
                    OpaqueKind::OwnedStore(_) => single == Some("Store"),
                    OpaqueKind::Mutex(_) => single == Some("Mutex"),
                    OpaqueKind::MutexGuard(_) => single == Some("MutexGuard"),
                    OpaqueKind::Task { .. } => single == Some("Task"),
                    OpaqueKind::Registration(_) => single == Some("Registration"),
                    OpaqueKind::AsyncRegistration(_) => single == Some("AsyncRegistration"),
                    OpaqueKind::ForeignHandle => single == Some("ForeignHandle"),
                    OpaqueKind::ForeignCallback { .. } | OpaqueKind::SyncCell(_) => false,
                },
                TyData::Adt { adt, .. } => target_adt == Some(*adt),
                TyData::Never
                | TyData::Tuple(_)
                | TyData::Union(_)
                | TyData::Array { .. }
                | TyData::Slice(_)
                | TyData::Ref { .. }
                | TyData::Managed(_)
                | TyData::Desc(_)
                | TyData::ErrorUnion { .. }
                | TyData::FnPtr { .. }
                | TyData::Closure { .. }
                | TyData::ImplCallable { .. }
                | TyData::Dyn { .. }
                | TyData::OwnedDyn { .. }
                | TyData::Param(_)
                | TyData::SelfParam
                | TyData::Assoc { .. }
                | TyData::ArrayConst { .. }
                | TyData::ConstParam(_)
                | TyData::ConstU32(_)
                | TyData::Type
                | TyData::Var(_)
                | TyData::Error => false,
            };
            if is_match {
                matched.push(m);
            }
        }
        matched
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
            PatKind::Wild | PatKind::Binding(_) | PatKind::RefBinding { .. } => true,
            PatKind::Deref(inner) => self.irrefutable(inner),
            PatKind::Lit(_) | PatKind::ErrorArm { .. } | PatKind::UnionArm { .. } => false,
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
            ast::Literal::Char(raw) => {
                let Some(v) = literal::parse_char(raw) else {
                    self.error("E-CHAR-LITERAL", span, "malformed character literal");
                    return None;
                };
                let ty = self.types.int(IntTy::U32);
                Some((Lit::Int(v as i128), ty))
            }
        }
    }

    fn parse_string_bytes(raw: &str) -> Vec<u8> {
        let inner = raw
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(raw);
        let mut out = Vec::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push(b'\n'),
                    Some('r') => out.push(b'\r'),
                    Some('t') => out.push(b'\t'),
                    Some('\\') => out.push(b'\\'),
                    Some('"') => out.push(b'"'),
                    Some('0') => out.push(0),
                    Some(other) => {
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
                    }
                    None => out.push(b'\\'),
                }
            } else {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
        out
    }

    pub fn infer(&mut self, e: &ast::Expr) -> Expr {
        let span = e.span;
        let mk = |kind, ty| Expr { kind, ty, span };
        match &e.kind {
            ast::ExprKind::Literal(ast::Literal::String(raw)) => {
                let bytes = Self::parse_string_bytes(raw);
                let u8_ty = self.types.intern(TyData::Int(IntTy::U8));
                let slice_ty = self.types.intern(TyData::Slice(u8_ty));
                let ref_ty = self.types.intern(TyData::Ref {
                    region: None,
                    mutable: false,
                    pointee: slice_ty,
                });
                mk(ExprKind::StaticUtf(bytes), ref_ty)
            }
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
                if let Some((idx, g)) = self
                    .decl
                    .generics
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, g)| g.name == *n && g.const_ty.is_some())
                {
                    let ty = g.const_ty.unwrap();
                    return mk(ExprKind::ConstGeneric(idx as u32), ty);
                }
                match self
                    .items
                    .lookup_item(self.current_module, std::slice::from_ref(n))
                {
                    Some(ItemRef::Fn(f)) => self.fn_value(f, span),
                    Some(ItemRef::Const(c)) => self.infer_const_ref(c, span),
                    Some(ItemRef::Foreign(f)) => {
                        let decl = self.items.foreign(f);
                        let ty = self
                            .types
                            .intern(TyData::Opaque(OpaqueKind::ForeignCallback {
                                params: decl.params.clone(),
                                result: decl.result,
                            }));
                        mk(ExprKind::ForeignRef(f), ty)
                    }
                    Some(ItemRef::Adt(adt))
                        if matches!(self.types.data(self.ret_ty), TyData::Type)
                            && self.items.adt(adt).generics.is_empty()
                            && self.items.adt(adt).region_params == 0 =>
                    {
                        let val_ty = self.adt_ty(adt, &[]);
                        let meta_ty = self.types.meta_type();
                        mk(ExprKind::Lit(Lit::Int(val_ty.0 as i128)), meta_ty)
                    }
                    Some(ItemRef::Adt(_) | ItemRef::Trait(_) | ItemRef::TypeAlias(_, _)) => {
                        self.error(diag::NAME_UNRESOLVED, span, format!("`{n}` is a type"));
                        self.err_expr(span)
                    }
                    None => {
                        if n != "Type"
                            && let Some(scalar) = scalar_type(n)
                        {
                            let val_ty = self.types.intern(scalar);
                            let meta_ty = self.types.meta_type();
                            return mk(ExprKind::Lit(Lit::Int(val_ty.0 as i128)), meta_ty);
                        }
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
                    TyData::Array { element, .. }
                    | TyData::ArrayConst { element, .. }
                    | TyData::Slice(element) => *element,
                    TyData::Error => self.types.error(),
                    TyData::Unit
                    | TyData::Type
                    | TyData::ConstParam(_)
                    | TyData::ConstU32(_)
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
                    | TyData::Union(_)
                    | TyData::FnPtr { .. }
                    | TyData::Closure { .. }
                    | TyData::ImplCallable { .. }
                    | TyData::Dyn { .. }
                    | TyData::OwnedDyn { .. }
                    | TyData::Param(_)
                    | TyData::SelfParam
                    | TyData::Assoc { .. }
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
                let Some(ty) = ty else {
                    self.error(
                        "E-RECORD-NO-CONTEXT",
                        span,
                        "a record literal needs its type name",
                    );
                    return self.err_expr(span);
                };
                let (ty_base, explicit_args) =
                    if let ast::ExprKind::TypeApply { callee, arguments } = &ty.kind {
                        (callee.as_ref(), Some(arguments.as_slice()))
                    } else {
                        (ty.as_ref(), None)
                    };
                let Some(path) = Self::expr_path(ty_base) else {
                    self.unsupported("computed record types", span);
                    return self.err_expr(span);
                };
                let Some((adt, variant)) = self.resolve_variant_path(&path, span) else {
                    return self.err_expr(span);
                };
                self.record(adt, variant, explicit_args, base.as_deref(), fields, span)
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
            ast::ExprKind::Pipeline { input, target } => {
                let first_arg = ast::Argument {
                    spread: false,
                    value: (**input).clone(),
                    span: input.span,
                };
                if let ast::ExprKind::Call { callee, arguments } = &target.kind {
                    let mut new_args = Vec::with_capacity(arguments.len() + 1);
                    new_args.push(first_arg);
                    new_args.extend(arguments.iter().cloned());
                    self.call(callee, &new_args, span)
                } else {
                    self.call(target, &[first_arg], span)
                }
            }
            ast::ExprKind::TypeApply { callee, arguments } => {
                if let Some(path) = Self::expr_path(callee).filter(|p| !self.path_is_value(p))
                    && let Some(ItemRef::Fn(f)) = self.items.lookup_item(self.decl.module, &path)
                {
                    let n = self.items.func(f).generics.len();
                    let mut explicit_tys = Vec::new();
                    for g in arguments {
                        match g {
                            ast::GenericArg::Type(t) => explicit_tys.push(self.resolve_written(t)),
                            ast::GenericArg::Lifetime(_) => {}
                            ast::GenericArg::Const(expr) => {
                                explicit_tys.push(self.resolve_const_generic_arg(expr));
                            }
                            ast::GenericArg::AssociatedBinding(_) => {
                                self.unsupported("non-type generic arguments", span);
                                return self.err_expr(span);
                            }
                        }
                    }
                    let type_args = if explicit_tys.is_empty() {
                        (0..n).map(|_| self.types.fresh(VarKind::General)).collect()
                    } else if explicit_tys.len() == n {
                        explicit_tys
                    } else {
                        self.error(diag::TYPE_ARGUMENT, span, "wrong number of type arguments");
                        return self.err_expr(span);
                    };
                    let decl = self.items.func(f);
                    let params: Vec<Ty> = decl
                        .params
                        .iter()
                        .map(|p| self.types.subst(p.ty, &type_args))
                        .collect();
                    let result = self.types.subst(decl.result, &type_args);
                    let ty = self.types.intern(TyData::FnPtr { params, result });
                    return Expr {
                        kind: ExprKind::FnRef { func: f, type_args },
                        ty,
                        span,
                    };
                }
                self.unsupported("explicit type arguments outside a call", span);
                self.err_expr(span)
            }
            ast::ExprKind::Slice { object, start, end } => {
                self.infer_slice_borrow(object, start.as_deref(), end.as_deref(), false, span)
            }
            ast::ExprKind::Try(inner) => {
                if self.defer_depth > 0 {
                    self.error(
                        diag::LOOP_CONTROL,
                        span,
                        "cannot propagate errors with `?` out of a `defer` block",
                    );
                    return self.err_expr(span);
                }
                let x = self.infer(inner);
                let xt = self.types.shallow(x.ty);
                let xt_data = self.types.data(xt).clone();
                if let TyData::ErrorUnion {
                    ok: ok_ty,
                    err: err_ty,
                } = xt_data
                {
                    let ret_t = self.types.shallow(self.ret_ty);
                    if let TyData::ErrorUnion { err: ret_err, .. } = self.types.data(ret_t).clone()
                    {
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
                } else if let TyData::Adt {
                    adt: AdtRef(0),
                    ref args,
                    ..
                } = xt_data
                    && args.len() == 1
                {
                    let ok_ty = args[0];
                    let ret_t = self.types.shallow(self.ret_ty);
                    if !matches!(
                        self.types.data(ret_t),
                        TyData::Adt { adt: AdtRef(0), .. } | TyData::Error
                    ) {
                        self.error(
                            diag::TRY_RETURN_TYPE,
                            span,
                            "`?` on `Option` requires the enclosing function to return `Option`",
                        );
                    }
                    mk(ExprKind::Try(Box::new(x)), ok_ty)
                } else if matches!(xt_data, TyData::Error) {
                    self.err_expr(span)
                } else {
                    self.error(
                        diag::TRY_NOT_ERROR,
                        span,
                        "`?` requires an error union or `Option` operand",
                    );
                    self.err_expr(span)
                }
            }
            ast::ExprKind::Closure {
                move_capture,
                params,
                return_type,
                body,
                ..
            } => self.infer_closure(
                *move_capture,
                params,
                return_type.as_ref(),
                body,
                span,
                None,
            ),
            ast::ExprKind::MetaEval(inner) => {
                // Compile-time expressions cannot observe enclosing runtime locals.
                let saved_scopes = std::mem::take(&mut self.scopes);
                self.scopes.push(Vec::new());
                let expr = self.infer(inner);
                self.scopes = saved_scopes;
                let ty = expr.ty;
                mk(ExprKind::CtEval(Box::new(expr)), ty)
            }
            ast::ExprKind::MetaBlock(block) => self.infer_meta_block(block, span, None),
            ast::ExprKind::MetaSplice(_)
            | ast::ExprKind::Quote { .. }
            | ast::ExprKind::SyntaxIsland { .. } => {
                self.unsupported("compile-time evaluation", span);
                self.err_expr(span)
            }
        }
    }

    fn infer_meta_block(&mut self, block: &ast::Block, span: Span, expected: Option<Ty>) -> Expr {
        // Compile-time blocks cannot observe enclosing runtime locals, loops, or defers.
        let saved_scopes = std::mem::take(&mut self.scopes);
        self.scopes.push(Vec::new());
        let saved_loop = std::mem::replace(&mut self.loop_depth, 0);
        let saved_defer = std::mem::replace(&mut self.defer_depth, 0);
        let seeded = expected.filter(|t| {
            !matches!(
                *self.types.data(self.types.shallow(*t)),
                TyData::Var(_) | TyData::Error
            )
        });
        self.meta_yield_stack.push(MetaYieldState {
            yield_ty: seeded,
            has_yield: false,
        });
        let body = self.block(block);
        let state = self
            .meta_yield_stack
            .pop()
            .expect("balanced meta yield stack");
        self.scopes = saved_scopes;
        self.loop_depth = saved_loop;
        self.defer_depth = saved_defer;

        let unit = self.types.unit();
        let result_ty = match state.yield_ty {
            Some(ty) => {
                if !state.has_yield && ty != unit && !self.types.is_error(ty) {
                    self.error(
                        diag::MISSING_RETURN,
                        span,
                        "compile-time block does not yield a value",
                    );
                } else if state.has_yield
                    && !self.types.is_never(body.ty)
                    && ty != unit
                    && !self.types.is_error(ty)
                {
                    self.error(
                        diag::MISSING_RETURN,
                        span,
                        "compile-time block can finish without yielding a value",
                    );
                }
                ty
            }
            None => unit,
        };
        Expr {
            kind: ExprKind::CtEval(Box::new(body)),
            ty: result_ty,
            span,
        }
    }

    fn resolve_capture_at(
        &mut self,
        depth: usize,
        name: &str,
    ) -> Option<(u32, Ty, Ty, bool, bool)> {
        let found = self.capture_stack[depth]
            .outer
            .iter()
            .rev()
            .find(|(n, ..)| n == name)
            .map(|(_, l, ty, mutable)| (CaptureSource::Local(*l), *ty, *mutable));
        let (source, orig_ty, outer_mut) = if let Some(entry) = found {
            entry
        } else if depth > 0 {
            let (outer_idx, _, orig_ty, _, outer_mut) = self.resolve_capture_at(depth - 1, name)?;
            (CaptureSource::OuterCapture(outer_idx), orig_ty, outer_mut)
        } else {
            return None;
        };

        let existing = self.capture_stack[depth]
            .captures
            .iter()
            .position(|c| c.source == source);
        let idx = if let Some(pos) = existing {
            pos
        } else {
            let move_capture = self.capture_stack[depth].move_capture;
            let moved_direct = self.capture_stack[depth].usage.moved.contains(name);
            let given_direct = self.capture_stack[depth].usage.given.contains(name);
            let mutated_direct = self.capture_stack[depth].usage.mutated.contains(name);
            let called_direct = self.capture_stack[depth].usage.called.contains(name);
            let direct_methods: Vec<String> = self.capture_stack[depth]
                .usage
                .direct_methods
                .iter()
                .filter(|(v, _)| v == name)
                .map(|(_, m)| m.clone())
                .collect();

            let orig_shallow = self.types.shallow(orig_ty);
            let orig_data = self.types.data(orig_shallow).clone();
            let reborrow_mut =
                !move_capture && matches!(orig_data, TyData::Ref { mutable: true, .. });

            let mut method_mutates = false;
            let mut method_moves = false;
            if !move_capture && !reborrow_mut {
                for mname in &direct_methods {
                    match &orig_data {
                        TyData::Adt { adt, .. } => {
                            if let Some(method) = self.items.adt(*adt).method_named(mname)
                                && method.has_self
                            {
                                let self_ty =
                                    self.items.func(method.fn_ref).params.first().map(|p| p.ty);
                                match self_ty.map(|t| self.types.data(self.types.shallow(t))) {
                                    Some(TyData::Ref { mutable: false, .. }) => {}
                                    Some(TyData::Ref { mutable: true, .. }) => {
                                        method_mutates = true;
                                    }
                                    Some(_) => {
                                        method_moves = true;
                                    }
                                    None => {
                                        method_mutates = true;
                                    }
                                }
                            } else {
                                method_mutates = true;
                            }
                        }
                        TyData::Opaque(OpaqueKind::ByteBuffer | OpaqueKind::OwnedStore(_)) => {
                            match mname.as_str() {
                                "len" | "is_empty" | "capacity" | "get" | "as_slice" | "at" => {}
                                _ => method_mutates = true,
                            }
                        }
                        TyData::Unit
                        | TyData::Bool
                        | TyData::Never
                        | TyData::Int(_)
                        | TyData::Float(_)
                        | TyData::Ref { .. }
                        | TyData::Managed(_)
                        | TyData::Desc(_)
                        | TyData::Tuple(_)
                        | TyData::Array { .. }
                        | TyData::Slice(_)
                        | TyData::ErrorUnion { .. }
                        | TyData::Union(_)
                        | TyData::FnPtr { .. }
                        | TyData::Closure { .. }
                        | TyData::ImplCallable { .. }
                        | TyData::Dyn { .. }
                        | TyData::OwnedDyn { .. }
                        | TyData::Opaque(_)
                        | TyData::Param(_)
                        | TyData::SelfParam
                        | TyData::Assoc { .. }
                        | TyData::ArrayConst { .. }
                        | TyData::ConstParam(_)
                        | TyData::ConstU32(_)
                        | TyData::Type
                        | TyData::Var(_)
                        | TyData::Error => {
                            method_mutates = true;
                        }
                    }
                }
            }

            let is_non_copy = ty_is_non_copy(self.items, self.types, orig_ty);
            let is_managed = matches!(orig_data, TyData::Managed(_));
            let is_fn_once = matches!(
                orig_data,
                TyData::Closure {
                    kind: ClosureKind::FnOnce,
                    ..
                } | TyData::ImplCallable {
                    kind: ClosureKind::FnOnce,
                    ..
                }
            );
            let is_fn_mut = matches!(
                orig_data,
                TyData::Closure {
                    kind: ClosureKind::FnMut,
                    ..
                } | TyData::ImplCallable {
                    kind: ClosureKind::FnMut,
                    ..
                }
            );
            let consumed_by_value = if is_managed {
                given_direct
            } else {
                is_non_copy && (moved_direct || given_direct || method_moves)
            };
            let move_by_value = !move_capture
                && !reborrow_mut
                && (consumed_by_value || (is_fn_once && called_direct));
            let by_value = move_capture || reborrow_mut || move_by_value;
            let mutable = if by_value {
                outer_mut
            } else {
                outer_mut && (mutated_direct || method_mutates || (is_fn_mut && called_direct))
            };
            let field_ty = if by_value {
                orig_ty
            } else {
                self.types.intern(TyData::Ref {
                    region: None,
                    mutable,
                    pointee: orig_ty,
                })
            };
            let env = &mut self.capture_stack[depth];
            env.captures.push(CaptureEntry {
                source,
                orig_ty,
                by_value,
                reborrow_mut,
                mutable,
                mutated: false,
                consumed: false,
                field_ty,
            });
            env.captures.len() - 1
        };
        let cap = &self.capture_stack[depth].captures[idx];
        Some((
            idx as u32,
            cap.field_ty,
            cap.orig_ty,
            cap.by_value,
            cap.mutable,
        ))
    }

    fn capture_outer(&mut self, name: &str, span: Span) -> Option<Expr> {
        let depth = self.capture_stack.len().checked_sub(1)?;
        let (idx, field_ty, orig_ty, by_value, _) = self.resolve_capture_at(depth, name)?;
        if by_value {
            let ot = self.types.shallow(orig_ty);
            if matches!(self.types.data(ot), TyData::Param(_))
                && let Some(cap) = self.capture_stack[depth].captures.get_mut(idx as usize)
            {
                cap.consumed = true;
            }
        }
        let cap_expr = Expr {
            kind: ExprKind::Capture(idx),
            ty: field_ty,
            span,
        };
        if by_value {
            Some(cap_expr)
        } else {
            Some(Expr {
                kind: ExprKind::Deref(Box::new(cap_expr)),
                ty: orig_ty,
                span,
            })
        }
    }

    fn infer_expected(&mut self, expr: &ast::Expr, expected: Ty) -> Expr {
        let et = self.normalize(expected);
        let ed = self.types.data(et).clone();
        match &expr.kind {
            ast::ExprKind::Closure {
                move_capture,
                params,
                return_type,
                body,
                ..
            } => {
                let exp = if let TyData::ImplCallable {
                    params: ep,
                    result: er,
                    ..
                }
                | TyData::FnPtr {
                    params: ep,
                    result: er,
                } = &ed
                    && ep.len() == params.len()
                {
                    Some((ep.clone(), *er))
                } else {
                    None
                };
                self.infer_closure(
                    *move_capture,
                    params,
                    return_type.as_ref(),
                    body,
                    expr.span,
                    exp.as_ref().map(|(p, r)| (p.as_slice(), *r)),
                )
            }
            ast::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let cond = self.infer(condition);
                let b = self.types.bool();
                self.coerce(&cond, b, condition.span);
                let then_v = self.infer_expected(then_value, expected);
                let then_v = self.coerce_value(then_v, expected, then_value.span);
                let else_v = self.infer_expected(else_value, expected);
                let else_v = self.coerce_value(else_v, expected, else_value.span);
                let ty = self.join(then_v.ty, else_v.ty, expr.span);
                Expr {
                    kind: ExprKind::If {
                        cond: Box::new(cond),
                        then: Box::new(then_v),
                        otherwise: Some(Box::new(else_v)),
                    },
                    ty,
                    span: expr.span,
                }
            }
            ast::ExprKind::Tuple(items) => {
                if let TyData::Tuple(elem_tys) = &ed
                    && elem_tys.len() == items.len()
                {
                    let mut out = Vec::with_capacity(items.len());
                    let mut tys = Vec::with_capacity(items.len());
                    for (it, &ety) in items.iter().zip(elem_tys) {
                        let v = self.infer_expected(it, ety);
                        let v = self.coerce_value(v, ety, it.span);
                        tys.push(v.ty);
                        out.push(v);
                    }
                    let ty = self.types.intern(TyData::Tuple(tys));
                    return Expr {
                        kind: ExprKind::Tuple(out),
                        ty,
                        span: expr.span,
                    };
                }
                self.infer(expr)
            }
            ast::ExprKind::Array(items) => {
                if let TyData::Array { element, .. } = ed
                    && !items.is_empty()
                {
                    let mut out = Vec::with_capacity(items.len());
                    for it in items {
                        let v = self.infer_expected(it, element);
                        let v = self.coerce_value(v, element, it.span);
                        out.push(v);
                    }
                    let ty = self.types.intern(TyData::Array {
                        element,
                        length: items.len() as u32,
                    });
                    return Expr {
                        kind: ExprKind::Array(out),
                        ty,
                        span: expr.span,
                    };
                }
                self.infer(expr)
            }
            ast::ExprKind::MetaBlock(block) => {
                self.infer_meta_block(block, expr.span, Some(expected))
            }
            ast::ExprKind::Literal(_)
            | ast::ExprKind::Name(_)
            | ast::ExprKind::ReceiverMember(_)
            | ast::ExprKind::Group(_)
            | ast::ExprKind::Ascribed { .. }
            | ast::ExprKind::Unary { .. }
            | ast::ExprKind::Binary { .. }
            | ast::ExprKind::Pipeline { .. }
            | ast::ExprKind::Member { .. }
            | ast::ExprKind::TypeApply { .. }
            | ast::ExprKind::Call { .. }
            | ast::ExprKind::Index { .. }
            | ast::ExprKind::Slice { .. }
            | ast::ExprKind::Try(_)
            | ast::ExprKind::Record { .. }
            | ast::ExprKind::MetaEval(_)
            | ast::ExprKind::MetaSplice(_)
            | ast::ExprKind::Quote { .. }
            | ast::ExprKind::SyntaxIsland { .. } => self.infer(expr),
        }
    }

    fn infer_closure(
        &mut self,
        move_capture: bool,
        params: &[ast::ClosureParam],
        return_type: Option<&ast::TypeRef>,
        body: &ast::ArrowBody,
        span: Span,
        expected: Option<(&[Ty], Ty)>,
    ) -> Expr {
        // Snapshot currently visible locals of the immediately enclosing function or closure.
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
        let saved_defer = self.defer_depth;
        let saved_unsafe = self.in_unsafe;

        self.scopes.push(Vec::new());
        self.loop_depth = 0;
        self.defer_depth = 0;
        self.in_unsafe = 0;
        let usage = if move_capture {
            ClosureUsage::default()
        } else {
            scan_closure_usage(params, body)
        };
        self.capture_stack.push(CaptureEnv {
            outer,
            move_capture,
            usage,
            captures: Vec::new(),
        });

        let mut param_ids = Vec::with_capacity(params.len());
        let mut param_tys = Vec::with_capacity(params.len());
        for (idx, p) in params.iter().enumerate() {
            let ty = match &p.ty {
                Some(t) => self.resolve_written(t),
                None => expected
                    .and_then(|(ep, _)| ep.get(idx).copied())
                    .unwrap_or_else(|| self.types.fresh(VarKind::General)),
            };
            param_tys.push(ty);
            param_ids.push(self.declare(&p.name, ty, false, p.span));
        }
        let result_ty = match return_type {
            Some(t) => self.resolve_written(t),
            None => expected
                .map(|(_, er)| er)
                .unwrap_or_else(|| self.types.fresh(VarKind::General)),
        };
        self.ret_ty = result_ty;

        let body_expr = match body {
            ast::ArrowBody::Block(b) => self.block(b),
            ast::ArrowBody::Expression(e) => {
                let val = self.infer_expected(e, result_ty);
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
        self.defer_depth = saved_defer;
        self.in_unsafe = saved_unsafe;
        let mut env = self
            .capture_stack
            .pop()
            .expect("capture_stack pushed above");
        let (end_state, move_res) = analyze_capture_moves(self.items, self.types, &body_expr);
        let still_uninit: BTreeSet<(u32, Option<u32>)> = end_state
            .uninit
            .into_iter()
            .chain(move_res.escaped_uninit)
            .collect();
        for idx in move_res.whole_moved {
            if let Some(cap) = env.captures.get_mut(idx as usize)
                && cap.by_value
                && ty_is_non_copy(self.items, self.types, cap.orig_ty)
            {
                cap.consumed = true;
            }
        }
        for (idx, opt_field) in move_res.ever_given {
            if let Some(cap) = env.captures.get_mut(idx as usize)
                && cap.by_value
                && ty_is_non_copy(self.items, self.types, cap.orig_ty)
            {
                let unrefilled =
                    still_uninit.contains(&(idx, opt_field)) || still_uninit.contains(&(idx, None));
                if !unrefilled && cap.mutable {
                    cap.mutated = true;
                } else {
                    cap.consumed = true;
                }
            }
        }

        let kind = if env.captures.iter().any(|c| c.consumed) {
            ClosureKind::FnOnce
        } else if env.captures.iter().any(|c| c.mutated) {
            ClosureKind::FnMut
        } else {
            ClosureKind::Fn
        };
        let id = ClosureRef(self.closures.len() as u32);
        let capture_tys: Vec<Ty> = env.captures.iter().map(|c| c.field_ty).collect();
        let mut capture_exprs = Vec::with_capacity(env.captures.len());
        for c in &env.captures {
            let place = match c.source {
                CaptureSource::Local(l) => Expr {
                    kind: ExprKind::Local(l),
                    ty: c.orig_ty,
                    span,
                },
                CaptureSource::OuterCapture(outer_idx) => {
                    let is_non_copy = ty_is_non_copy(self.items, self.types, c.orig_ty);
                    let outer_env = self
                        .capture_stack
                        .last_mut()
                        .expect("OuterCapture requires enclosing closure");
                    let outer_cap = &mut outer_env.captures[outer_idx as usize];
                    if c.consumed || (c.by_value && is_non_copy) {
                        outer_cap.consumed = true;
                    }
                    if c.mutated || (!c.by_value && c.mutable) {
                        outer_cap.mutated = true;
                    }
                    let cap_expr = Expr {
                        kind: ExprKind::Capture(outer_idx),
                        ty: outer_cap.field_ty,
                        span,
                    };
                    if outer_cap.by_value {
                        cap_expr
                    } else {
                        Expr {
                            kind: ExprKind::Deref(Box::new(cap_expr)),
                            ty: outer_cap.orig_ty,
                            span,
                        }
                    }
                }
            };
            capture_exprs.push(if c.reborrow_mut {
                let pointee = if let TyData::Ref { pointee, .. } =
                    *self.types.data(self.types.shallow(c.orig_ty))
                {
                    pointee
                } else {
                    self.types.error()
                };
                Expr {
                    kind: ExprKind::Borrow {
                        mutable: true,
                        place: Box::new(Expr {
                            kind: ExprKind::Deref(Box::new(place)),
                            ty: pointee,
                            span,
                        }),
                    },
                    ty: c.field_ty,
                    span,
                }
            } else if c.by_value {
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
            });
        }
        self.closures.push(ThirClosure {
            kind,
            capture_tys,
            locals: closure_locals,
            params: param_ids,
            param_tys: param_tys.clone(),
            result: result_ty,
            body: body_expr,
        });
        let owner_args = (0..self.generic_names.len() as u32)
            .map(|i| self.types.intern(TyData::Param(i)))
            .collect();
        let ty = self.types.intern(TyData::Closure {
            owner: self.fn_ref,
            id,
            owner_args,
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
                    | TyData::Type
                    | TyData::ConstParam(_)
                    | TyData::ConstU32(_)
                    | TyData::Bool
                    | TyData::Never
                    | TyData::Int(_)
                    | TyData::Float(_)
                    | TyData::Adt { .. }
                    | TyData::Tuple(_)
                    | TyData::Array { .. }
                    | TyData::ArrayConst { .. }
                    | TyData::Slice(_)
                    | TyData::Managed(_)
                    | TyData::Desc(_)
                    | TyData::Opaque(_)
                    | TyData::ErrorUnion { .. }
                    | TyData::Union(_)
                    | TyData::FnPtr { .. }
                    | TyData::Closure { .. }
                    | TyData::ImplCallable { .. }
                    | TyData::Dyn { .. }
                    | TyData::OwnedDyn { .. }
                    | TyData::Param(_)
                    | TyData::SelfParam
                    | TyData::Assoc { .. } => {
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
                    let elem = if let TyData::Array { element, .. }
                    | TyData::ArrayConst { element, .. }
                    | TyData::Slice(element) = *self.types.data(bt)
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
                        region: None,
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
                    region: None,
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
                    if let ExprKind::Capture(idx) = x.kind
                        && let Some(env) = self.capture_stack.last_mut()
                        && let Some(entry) = env.captures.get_mut(idx as usize)
                    {
                        entry.consumed = true;
                    }
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
            ast::BinaryOp::Range | ast::BinaryOp::RangeInclusive => {
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
            if let Some(item) = self.items.lookup_item(self.current_module, &path) {
                match item {
                    ItemRef::Fn(f) => return self.fn_value(f, span),
                    ItemRef::Const(c) => return self.infer_const_ref(c, span),
                    ItemRef::Adt(_)
                    | ItemRef::Foreign(_)
                    | ItemRef::Trait(_)
                    | ItemRef::TypeAlias(_, _) => {}
                }
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
            (ast::MemberName::Name(name), TyData::Adt { adt, args, .. }) => {
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
                if decl.module != self.decl.module && !decl.variants[0].fields[field].public {
                    let adt_name = decl.name.clone();
                    self.error(
                        "E-VISIBILITY",
                        span,
                        format!("field `{name}` on `{adt_name}` is private"),
                    );
                    return self.err_expr(span);
                }
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
        explicit: Option<&[ast::GenericArg]>,
        base: Option<&ast::Expr>,
        fields: &[ast::RecordField],
        span: Span,
    ) -> Expr {
        let decl = self.items.adt(adt);
        let is_cross_mod = decl.kind != AdtKind::Enum && decl.module != self.decl.module;
        let adt_kind = decl.kind;
        let user_drop = decl.user_drop;
        let adt_name = decl.name.clone();
        let decl_fields: Vec<(String, bool)> = decl.variants[variant as usize]
            .fields
            .iter()
            .map(|f| (f.name.clone(), f.public))
            .collect();
        if decl.variants[variant as usize].shape != VariantShape::Record {
            self.error(
                diag::ENUM_ARITY,
                span,
                "this variant is not a record variant",
            );
            return self.err_expr(span);
        }
        if let Some(b) = base
            && (adt_kind != AdtKind::Record || user_drop)
        {
            self.error(
                "E-RECORD-UPDATE",
                b.span,
                format!(
                    "record update syntax is only supported on plain `struct` types without custom teardown (`{adt_name}`)"
                ),
            );
            return self.err_expr(span);
        }
        let args = match explicit {
            None => self.fresh_args(adt),
            Some(given) => {
                let expected_n = self.items.adt(adt).generics.len();
                let mut explicit_tys = Vec::new();
                for g in given {
                    match g {
                        ast::GenericArg::Type(t) => explicit_tys.push(self.resolve_written(t)),
                        ast::GenericArg::Lifetime(_) => {}
                        ast::GenericArg::Const(expr) => {
                            explicit_tys.push(self.resolve_const_generic_arg(expr));
                        }
                        ast::GenericArg::AssociatedBinding(_) => {
                            self.unsupported("non-type generic arguments", span);
                            explicit_tys.push(self.types.error());
                        }
                    }
                }
                if explicit_tys.is_empty() {
                    self.fresh_args(adt)
                } else if explicit_tys.len() != expected_n {
                    self.error(diag::TYPE_ARGUMENT, span, "wrong number of type arguments");
                    self.fresh_args(adt)
                } else {
                    explicit_tys
                }
            }
        };
        let adt_ty = self.adt_ty(adt, &args);
        let base_binding = base.map(|b| {
            let base_expr = self.infer_expected(b, adt_ty);
            let base_expr = self.coerce_value(base_expr, adt_ty, b.span);
            let base_local = LocalId(self.locals.len() as u32);
            self.locals.push(LocalInfo {
                name: String::new(),
                ty: adt_ty,
                mutable: false,
                span: b.span,
            });
            (base_local, base_expr, b.span)
        });
        let mut slots: Vec<Option<Expr>> = vec![None; decl_fields.len()];
        for f in fields {
            let Some(i) = decl_fields.iter().position(|(n, _)| *n == f.name) else {
                self.error(
                    diag::RECORD_UNKNOWN_FIELD,
                    f.span,
                    format!("no field `{}`", f.name),
                );
                continue;
            };
            if is_cross_mod && !decl_fields[i].1 {
                self.error(
                    "E-VISIBILITY",
                    f.span,
                    format!("field `{}` on `{adt_name}` is private", f.name),
                );
                slots[i] = Some(self.err_expr(f.span));
                continue;
            }
            if slots[i].is_some() {
                self.error(
                    diag::RECORD_DUPLICATE_FIELD,
                    f.span,
                    format!("field `{}` appears more than once", f.name),
                );
                continue;
            }
            let ft = self.field_ty(adt, variant, i as u32, &args);
            let value = match &f.value {
                Some(v) => self.infer_expected(v, ft),
                None => self.infer(&ast::Expr {
                    kind: ast::ExprKind::Name(f.name.clone()),
                    span: f.span,
                    semantic: ast::SemanticContextMarker::default(),
                }),
            };
            let value = self.coerce_value(value, ft, f.span);
            slots[i] = Some(value);
        }
        let mut out = Vec::new();
        for (i, s) in slots.into_iter().enumerate() {
            match s {
                Some(v) => out.push(v),
                None => {
                    if is_cross_mod && !decl_fields[i].1 {
                        self.error(
                            "E-VISIBILITY",
                            span,
                            format!("field `{}` on `{adt_name}` is private", decl_fields[i].0),
                        );
                        out.push(self.err_expr(span));
                        continue;
                    }
                    if let Some((base_local, _, b_span)) = &base_binding {
                        let ft = self.field_ty(adt, variant, i as u32, &args);
                        out.push(Expr {
                            kind: ExprKind::Field {
                                base: Box::new(Expr {
                                    kind: ExprKind::Local(*base_local),
                                    ty: adt_ty,
                                    span: *b_span,
                                }),
                                adt,
                                variant,
                                args: args.clone(),
                                field: i as u32,
                            },
                            ty: ft,
                            span: *b_span,
                        });
                    } else {
                        let default_expr = self.items.adt(adt).variants[variant as usize].fields[i]
                            .default
                            .clone();
                        if let Some(def_expr) = default_expr {
                            let v = self.infer(&def_expr);
                            let ft = self.field_ty(adt, variant, i as u32, &args);
                            out.push(self.coerce_value(v, ft, span));
                        } else {
                            self.error(
                                diag::RECORD_MISSING_FIELD,
                                span,
                                format!("field `{}` is missing", decl_fields[i].0),
                            );
                            out.push(self.err_expr(span));
                        }
                    }
                }
            }
        }
        let result = if self.items.adt(adt).kind == AdtKind::Resource {
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
        };
        if let Some((base_local, base_expr, b_span)) = base_binding {
            Expr {
                kind: ExprKind::Block(vec![
                    Stmt::Let {
                        pattern: Pat {
                            kind: PatKind::Binding(base_local),
                            ty: adt_ty,
                            span: b_span,
                        },
                        init: base_expr,
                    },
                    Stmt::Expr(result),
                ]),
                ty: adt_ty,
                span,
            }
        } else {
            result
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
        // Explicit generic enum variant call: `Option::<u8>.None()`, `Option::<u8>.Some(x)`.
        if explicit.is_none()
            && let ast::ExprKind::Member {
                object,
                member: ast::MemberName::Name(variant),
            } = &callee.kind
            && let ast::ExprKind::TypeApply {
                callee: enum_callee,
                arguments: enum_type_args,
            } = &object.kind
            && let Some(enum_path) = Self::expr_path(enum_callee).filter(|p| !self.path_is_value(p))
            && let Some(adt) = self
                .items
                .resolve_adt_of(self.decl.module, &enum_path, self.types)
            && self.items.adt(adt).kind == AdtKind::Enum
            && let Some(v_idx) = self.items.adt(adt).variant_named(variant)
        {
            let v = &self.items.adt(adt).variants[v_idx as usize];
            let shape_ok = v.shape == VariantShape::Tuple
                || (v.shape == VariantShape::Unit && arguments.is_empty());
            if !shape_ok || v.fields.len() != arguments.len() {
                self.error(
                    diag::ENUM_ARITY,
                    span,
                    "the arguments do not match the variant's fields",
                );
                return self.err_expr(span);
            }
            let expected_n = self.items.adt(adt).generics.len();
            let mut explicit_tys = Vec::new();
            for g in enum_type_args {
                match g {
                    ast::GenericArg::Type(t) => explicit_tys.push(self.resolve_written(t)),
                    ast::GenericArg::Lifetime(_) => {}
                    ast::GenericArg::Const(expr) => {
                        explicit_tys.push(self.resolve_const_generic_arg(expr));
                    }
                    ast::GenericArg::AssociatedBinding(_) => {
                        self.unsupported("non-type generic arguments", span);
                        explicit_tys.push(self.types.error());
                    }
                }
            }
            let args = if explicit_tys.is_empty() {
                self.fresh_args(adt)
            } else if explicit_tys.len() != expected_n {
                self.error(diag::TYPE_ARGUMENT, span, "wrong number of type arguments");
                self.fresh_args(adt)
            } else {
                explicit_tys
            };
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
        // Static method call with explicit host generics: `Host::<T>.method::<U>(args...)`.
        if let ast::ExprKind::Member {
            object,
            member: ast::MemberName::Name(mname),
        } = &callee.kind
            && let ast::ExprKind::TypeApply {
                callee: host_callee,
                arguments: host_type_args,
            } = &object.kind
            && let Some(host_path) = Self::expr_path(host_callee).filter(|p| !self.path_is_value(p))
            && let Some(adt) = self
                .items
                .resolve_adt_of(self.decl.module, &host_path, self.types)
        {
            if let Some(method) = self.items.adt(adt).method_named(mname).cloned() {
                if !method.public && self.decl.module != self.items.adt(adt).module {
                    self.error(
                        "E-VISIBILITY",
                        span,
                        format!(
                            "method `{mname}` on `{}` is private",
                            self.items.adt(adt).name
                        ),
                    );
                    return self.err_expr(span);
                }
                let Some(mut full_args) = self.resolve_explicit_type_args(
                    Some(host_type_args.as_slice()),
                    method.host_generic_count,
                    object.span,
                ) else {
                    return self.err_expr(span);
                };
                let method_n = self
                    .items
                    .func(method.fn_ref)
                    .generics
                    .len()
                    .saturating_sub(method.host_generic_count);
                let Some(method_args) =
                    self.resolve_explicit_type_args(explicit.map(Vec::as_slice), method_n, span)
                else {
                    return self.err_expr(span);
                };
                full_args.extend(method_args);
                return self.direct_call_with_args(method.fn_ref, full_args, None, arguments, span);
            }
            let expected_host_n = self.items.adt(adt).generics.len();
            if let Some(host_args) = self.resolve_explicit_type_args(
                Some(host_type_args.as_slice()),
                expected_host_n,
                object.span,
            ) {
                let host_adt_ty = self.adt_ty(adt, &host_args);
                let host_ty = if self.items.adt(adt).kind == AdtKind::Resource {
                    self.types.intern(TyData::Desc(host_adt_ty))
                } else {
                    host_adt_ty
                };
                if let Some((method, impl_args)) =
                    self.items.inherent_impls.iter().find_map(|imp| {
                        let impl_args = self.items.match_inherent_impl(imp, host_ty, self.types)?;
                        imp.methods
                            .iter()
                            .find(|m| m.name == *mname)
                            .cloned()
                            .map(|m| (m, impl_args))
                    })
                {
                    if !method.public && self.decl.module != method.module {
                        self.error(
                            "E-VISIBILITY",
                            span,
                            format!(
                                "method `{mname}` on `{}` is private",
                                self.items.adt(adt).name
                            ),
                        );
                        return self.err_expr(span);
                    }
                    let method_n = self
                        .items
                        .func(method.fn_ref)
                        .generics
                        .len()
                        .saturating_sub(method.impl_generic_count);
                    let Some(method_args) = self.resolve_explicit_type_args(
                        explicit.map(Vec::as_slice),
                        method_n,
                        span,
                    ) else {
                        return self.err_expr(span);
                    };
                    let mut full_args = impl_args;
                    full_args.extend(method_args);
                    return self.direct_call_with_args(
                        method.fn_ref,
                        full_args,
                        None,
                        arguments,
                        span,
                    );
                }
                if explicit.is_none()
                    && let Some((tr_ref, tr_args, m_params, m_result)) =
                        self.items.trait_impls.iter().find_map(|imp| {
                            let impl_args = self.items.match_impl(imp, host_ty, self.types)?;
                            let tr = self.items.trait_decl(imp.trait_ref);
                            tr.methods.iter().find(|m| m.name == *mname).map(|m| {
                                let resolved_targs: Vec<Ty> = imp
                                    .trait_args
                                    .iter()
                                    .map(|t| self.types.subst(*t, &impl_args))
                                    .collect();
                                (imp.trait_ref, resolved_targs, m.params.clone(), m.result)
                            })
                        })
                {
                    if arguments.len() != m_params.len() {
                        self.error(
                            diag::CALL_ARITY,
                            span,
                            format!(
                                "`{}.{mname}` expects {} argument(s), found {}",
                                host_path.join("."),
                                m_params.len(),
                                arguments.len()
                            ),
                        );
                        return self.err_expr(span);
                    }
                    let mut args = Vec::with_capacity(arguments.len());
                    for (a, pt) in arguments.iter().zip(&m_params) {
                        let expected = self.types.subst_with_self(*pt, &tr_args, host_ty);
                        let expected = self.items.normalize_ty(expected, self.types);
                        let v = self.infer(&a.value);
                        args.push(self.coerce_arg(v, expected, a.span));
                    }
                    let ret = self.types.subst_with_self(m_result, &tr_args, host_ty);
                    let ret = self.items.normalize_ty(ret, self.types);
                    return Expr {
                        kind: ExprKind::TraitCall {
                            trait_ref: tr_ref,
                            trait_args: tr_args,
                            method_args: Vec::new(),
                            self_ty: host_ty,
                            method: mname.clone(),
                            args,
                        },
                        ty: ret,
                        span,
                    };
                }
            }
        }
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
                    ItemRef::Const(_) => {
                        self.error(
                            diag::ENUM_VARIANT,
                            span,
                            "a constant cannot be called directly",
                        );
                        return self.err_expr(span);
                    }
                    ItemRef::Adt(_) | ItemRef::Trait(_) | ItemRef::TypeAlias(_, _) => {
                        self.error(diag::ENUM_VARIANT, span, "a type cannot be called directly");
                        return self.err_expr(span);
                    }
                }
            }
            if path.len() >= 2 && explicit.is_none() {
                let (enum_path, variant) = (&path[..path.len() - 1], &path[path.len() - 1]);
                let resolved_enum = if enum_path == ["Self"]
                    && let Some(self_ty) = self.decl.self_ty
                    && let TyData::Adt { adt, .. } = *self.types.data(self.types.shallow(self_ty))
                    && self.items.adt(adt).kind == AdtKind::Enum
                {
                    Some(adt)
                } else {
                    self.items
                        .resolve_adt_of(self.decl.module, enum_path, self.types)
                        .filter(|&adt| self.items.adt(adt).kind == AdtKind::Enum)
                };
                if let Some(adt) = resolved_enum
                    && let Some(v_idx) = self.items.adt(adt).variant_named(variant)
                {
                    let v = &self.items.adt(adt).variants[v_idx as usize];
                    let shape_ok = v.shape == VariantShape::Tuple
                        || (v.shape == VariantShape::Unit && arguments.is_empty());
                    if !shape_ok || v.fields.len() != arguments.len() {
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
            if path.len() >= 2 {
                let (host_path, mname) = (&path[..path.len() - 1], &path[path.len() - 1]);
                let resolved_host = if host_path == ["Self"]
                    && let Some(self_ty) = self.decl.self_ty
                    && let TyData::Adt { adt, args, .. } =
                        self.types.data(self.types.shallow(self_ty)).clone()
                {
                    Some((adt, Some(args)))
                } else {
                    self.items
                        .resolve_adt_of(self.decl.module, host_path, self.types)
                        .map(|adt| (adt, None))
                };
                if let Some((adt, self_args)) = resolved_host
                    && let Some(method) = self.items.adt(adt).method_named(mname).cloned()
                {
                    if !method.public && self.decl.module != self.items.adt(adt).module {
                        self.error(
                            "E-VISIBILITY",
                            span,
                            format!(
                                "method `{mname}` on `{}` is private",
                                self.items.adt(adt).name
                            ),
                        );
                        return self.err_expr(span);
                    }
                    let mut full_args = match self_args {
                        Some(args) => args,
                        None => (0..method.host_generic_count)
                            .map(|_| self.types.fresh(VarKind::General))
                            .collect(),
                    };
                    let method_n = self
                        .items
                        .func(method.fn_ref)
                        .generics
                        .len()
                        .saturating_sub(method.host_generic_count);
                    let Some(method_args) = self.resolve_explicit_type_args(
                        explicit.map(Vec::as_slice),
                        method_n,
                        span,
                    ) else {
                        return self.err_expr(span);
                    };
                    full_args.extend(method_args);
                    return self.direct_call_with_args(
                        method.fn_ref,
                        full_args,
                        None,
                        arguments,
                        span,
                    );
                }
                // Static inherent impl method call: `Type.fn(...)`, `Self.fn(...)`, `i64.fn(...)`.
                let exact_host_ty: Option<Ty> = if host_path == ["Self"] {
                    self.decl.self_ty.map(|t| self.types.shallow(t))
                } else if let [scalar_name] = host_path
                    && let Some(scalar) = scalar_type(scalar_name)
                {
                    Some(self.types.intern(scalar))
                } else if let Some(ItemRef::TypeAlias(ta_mod, ta_idx)) =
                    self.items.lookup_item(self.decl.module, host_path)
                {
                    Some(self.items.modules[ta_mod.0 as usize].type_aliases[ta_idx as usize].1)
                } else {
                    None
                };
                let bare_host_adt = if exact_host_ty.is_none() {
                    self.items
                        .resolve_adt_of(self.decl.module, host_path, self.types)
                } else {
                    None
                };
                let found_inherent = if let Some(host_ty) = exact_host_ty {
                    self.items.inherent_impls.iter().find_map(|imp| {
                        let impl_args = self.items.match_inherent_impl(imp, host_ty, self.types)?;
                        imp.methods
                            .iter()
                            .find(|m| m.name == *mname)
                            .cloned()
                            .map(|m| (m, impl_args))
                    })
                } else if let Some(adt) = bare_host_adt {
                    self.items.inherent_impls.iter().find_map(|imp| {
                        let target_matches = match *self.types.data(imp.target) {
                            TyData::Adt { adt: imp_adt, .. } => imp_adt == adt,
                            TyData::Desc(inner) => matches!(
                                *self.types.data(inner),
                                TyData::Adt { adt: imp_adt, .. } if imp_adt == adt
                            ),
                            TyData::Unit
                            | TyData::Type
                            | TyData::ConstParam(_)
                            | TyData::ConstU32(_)
                            | TyData::Bool
                            | TyData::Never
                            | TyData::Int(_)
                            | TyData::Float(_)
                            | TyData::Tuple(_)
                            | TyData::Array { .. }
                            | TyData::ArrayConst { .. }
                            | TyData::Slice(_)
                            | TyData::Ref { .. }
                            | TyData::Managed(_)
                            | TyData::Opaque(_)
                            | TyData::ErrorUnion { .. }
                            | TyData::FnPtr { .. }
                            | TyData::Closure { .. }
                            | TyData::ImplCallable { .. }
                            | TyData::Dyn { .. }
                            | TyData::OwnedDyn { .. }
                            | TyData::Param(_)
                            | TyData::SelfParam
                            | TyData::Assoc { .. }
                            | TyData::Var(_)
                            | TyData::Union(_)
                            | TyData::Error => false,
                        };
                        if !target_matches {
                            return None;
                        }
                        let m = imp.methods.iter().find(|m| m.name == *mname)?.clone();
                        let impl_args = (0..imp.generics.len())
                            .map(|_| self.types.fresh(VarKind::General))
                            .collect();
                        Some((m, impl_args))
                    })
                } else {
                    None
                };
                if let Some((method, impl_args)) = found_inherent {
                    if !method.public && self.decl.module != method.module {
                        self.error(
                            "E-VISIBILITY",
                            span,
                            format!("method `{mname}` on `{}` is private", host_path.join(".")),
                        );
                        return self.err_expr(span);
                    }
                    let method_n = self
                        .items
                        .func(method.fn_ref)
                        .generics
                        .len()
                        .saturating_sub(method.impl_generic_count);
                    let Some(method_args) = self.resolve_explicit_type_args(
                        explicit.map(Vec::as_slice),
                        method_n,
                        span,
                    ) else {
                        return self.err_expr(span);
                    };
                    let mut full_args = impl_args;
                    full_args.extend(method_args);
                    return self.direct_call_with_args(
                        method.fn_ref,
                        full_args,
                        None,
                        arguments,
                        span,
                    );
                }
                // Static trait impl method call on concrete types, Self, primitives, or bare ADTs.
                if explicit.is_none() {
                    let found_static_trait = if let Some(host_ty) = exact_host_ty {
                        self.items.trait_impls.iter().find_map(|imp| {
                            let impl_args = self.items.match_impl(imp, host_ty, self.types)?;
                            let tr = self.items.trait_decl(imp.trait_ref);
                            tr.methods.iter().find(|m| m.name == *mname).map(|m| {
                                let resolved_targs: Vec<Ty> = imp
                                    .trait_args
                                    .iter()
                                    .map(|t| self.types.subst(*t, &impl_args))
                                    .collect();
                                (
                                    imp.trait_ref,
                                    resolved_targs,
                                    host_ty,
                                    m.params.clone(),
                                    m.result,
                                )
                            })
                        })
                    } else if let Some(adt) = bare_host_adt {
                        self.items.trait_impls.iter().find_map(|imp| {
                            let target_matches = match *self.types.data(imp.target) {
                                TyData::Adt { adt: imp_adt, .. } => imp_adt == adt,
                                TyData::Desc(inner) => matches!(
                                    *self.types.data(inner),
                                    TyData::Adt { adt: imp_adt, .. } if imp_adt == adt
                                ),
                                TyData::Unit
                                | TyData::Type
                                | TyData::ConstParam(_)
                                | TyData::ConstU32(_)
                                | TyData::Bool
                                | TyData::Never
                                | TyData::Int(_)
                                | TyData::Float(_)
                                | TyData::Tuple(_)
                                | TyData::Array { .. }
                                | TyData::ArrayConst { .. }
                                | TyData::Slice(_)
                                | TyData::Ref { .. }
                                | TyData::Managed(_)
                                | TyData::Opaque(_)
                                | TyData::ErrorUnion { .. }
                                | TyData::FnPtr { .. }
                                | TyData::Closure { .. }
                                | TyData::ImplCallable { .. }
                                | TyData::Dyn { .. }
                                | TyData::OwnedDyn { .. }
                                | TyData::Param(_)
                                | TyData::SelfParam
                                | TyData::Assoc { .. }
                                | TyData::Var(_)
                                | TyData::Union(_)
                                | TyData::Error => false,
                            };
                            if !target_matches {
                                return None;
                            }
                            let tr = self.items.trait_decl(imp.trait_ref);
                            tr.methods.iter().find(|m| m.name == *mname).map(|m| {
                                let impl_args: Vec<Ty> = (0..imp.generics.len())
                                    .map(|_| self.types.fresh(VarKind::General))
                                    .collect();
                                let self_ty = self.types.subst(imp.target, &impl_args);
                                let resolved_targs: Vec<Ty> = imp
                                    .trait_args
                                    .iter()
                                    .map(|t| self.types.subst(*t, &impl_args))
                                    .collect();
                                (
                                    imp.trait_ref,
                                    resolved_targs,
                                    self_ty,
                                    m.params.clone(),
                                    m.result,
                                )
                            })
                        })
                    } else {
                        None
                    };
                    if let Some((tr_ref, tr_args, self_ty, m_params, m_result)) = found_static_trait
                    {
                        if arguments.len() != m_params.len() {
                            self.error(
                                diag::CALL_ARITY,
                                span,
                                format!(
                                    "`{}.{mname}` expects {} argument(s), found {}",
                                    host_path.join("."),
                                    m_params.len(),
                                    arguments.len()
                                ),
                            );
                            return self.err_expr(span);
                        }
                        let mut args = Vec::with_capacity(arguments.len());
                        for (a, pt) in arguments.iter().zip(&m_params) {
                            let expected = self.types.subst_with_self(*pt, &tr_args, self_ty);
                            let expected = self.items.normalize_ty(expected, self.types);
                            let v = self.infer(&a.value);
                            args.push(self.coerce_arg(v, expected, a.span));
                        }
                        let ret = self.types.subst_with_self(m_result, &tr_args, self_ty);
                        let ret = self.items.normalize_ty(ret, self.types);
                        return Expr {
                            kind: ExprKind::TraitCall {
                                trait_ref: tr_ref,
                                trait_args: tr_args,
                                method_args: Vec::new(),
                                self_ty,
                                method: mname.clone(),
                                args,
                            },
                            ty: ret,
                            span,
                        };
                    }
                }
            }
            if let [param_name, method_name] = path.as_slice()
                && let Some(param_idx) = self.generic_names.iter().rposition(|g| g == param_name)
            {
                let bounds = self.decl.generics[param_idx].traits.clone();
                let self_ty = self.types.intern(TyData::Param(param_idx as u32));
                for bound in &bounds {
                    let closure = self
                        .items
                        .supertrait_closure(bound, Some(self_ty), self.types);
                    for entailed in closure {
                        let tr = self.items.trait_decl(entailed.trait_ref);
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
                            let m_generics_len = m.generics.len();
                            let m_params = m.params.clone();
                            let m_result = m.result;
                            let Some(method_args) = self.resolve_explicit_type_args(
                                explicit.map(Vec::as_slice),
                                m_generics_len,
                                span,
                            ) else {
                                return self.err_expr(span);
                            };
                            let mut full_subst = entailed.trait_args.clone();
                            full_subst.extend(method_args.iter().copied());
                            let mut args = Vec::with_capacity(arguments.len());
                            for (a, pt) in arguments.iter().zip(&m_params) {
                                let expected =
                                    self.types.subst_with_self(*pt, &full_subst, self_ty);
                                let expected = self.normalize(expected);
                                let v = self.infer(&a.value);
                                args.push(self.coerce_arg(v, expected, a.span));
                            }
                            let ret = self.types.subst_with_self(m_result, &full_subst, self_ty);
                            let ret = self.normalize(ret);
                            return Expr {
                                kind: ExprKind::TraitCall {
                                    trait_ref: entailed.trait_ref,
                                    trait_args: entailed.trait_args,
                                    method_args,
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

        // Built-in `.len()`, inherent inline methods, and trait/dyn calls (`receiver.method(args...)`).
        let mut fallback_callee: Option<Expr> = None;
        if let ast::ExprKind::Member {
            object,
            member: ast::MemberName::Name(mname),
        } = &callee.kind
        {
            let receiver = self.infer(object);
            let rec_ty = self.types.shallow(receiver.ty);
            if explicit.is_none() && mname == "len" && arguments.is_empty() {
                let derefed = self.auto_deref(receiver.clone());
                let bt = self.types.shallow(derefed.ty);
                if matches!(self.types.data(bt), TyData::Array { .. } | TyData::Slice(_)) {
                    let i64_ty = self.types.int(IntTy::I64);
                    return Expr {
                        kind: ExprKind::Len(Box::new(derefed)),
                        ty: i64_ty,
                        span,
                    };
                }
            }
            if explicit.is_none()
                && matches!(
                    mname.as_str(),
                    "is_some" | "is_none" | "is_ok" | "is_err" | "unwrap" | "unwrap_or"
                )
            {
                let derefed = self.auto_deref(receiver.clone());
                let bt = self.types.shallow(derefed.ty);
                let bt_data = self.types.data(bt).clone();
                if let TyData::Adt {
                    adt: AdtRef(0),
                    ref args,
                    ..
                } = bt_data
                    && args.len() == 1
                    && matches!(
                        mname.as_str(),
                        "is_some" | "is_none" | "unwrap" | "unwrap_or"
                    )
                {
                    let args = args.clone();
                    let ok_ty = args[0];
                    let bool_ty = self.types.bool();
                    let never_ty = self.types.never();
                    match mname.as_str() {
                        "is_some" | "is_none" => {
                            if !arguments.is_empty() {
                                self.error(
                                    diag::CALL_ARITY,
                                    span,
                                    format!("`Option.{mname}` takes 0 arguments"),
                                );
                                return self.err_expr(span);
                            }
                            let is_some = mname == "is_some";
                            return Expr {
                                kind: ExprKind::Match {
                                    scrutinee: Box::new(derefed),
                                    arms: vec![
                                        Arm {
                                            pattern: Pat {
                                                kind: PatKind::Adt {
                                                    adt: AdtRef(0),
                                                    variant: 1,
                                                    args: args.clone(),
                                                    fields: vec![self.wild(ok_ty, span)],
                                                },
                                                ty: bt,
                                                span,
                                            },
                                            guard: None,
                                            guard_refs: Vec::new(),
                                            body: Expr {
                                                kind: ExprKind::Lit(Lit::Bool(is_some)),
                                                ty: bool_ty,
                                                span,
                                            },
                                            span,
                                        },
                                        Arm {
                                            pattern: Pat {
                                                kind: PatKind::Adt {
                                                    adt: AdtRef(0),
                                                    variant: 0,
                                                    args,
                                                    fields: Vec::new(),
                                                },
                                                ty: bt,
                                                span,
                                            },
                                            guard: None,
                                            guard_refs: Vec::new(),
                                            body: Expr {
                                                kind: ExprKind::Lit(Lit::Bool(!is_some)),
                                                ty: bool_ty,
                                                span,
                                            },
                                            span,
                                        },
                                    ],
                                },
                                ty: bool_ty,
                                span,
                            };
                        }
                        "unwrap" => {
                            if !arguments.is_empty() {
                                self.error(
                                    diag::CALL_ARITY,
                                    span,
                                    "`Option.unwrap` takes 0 arguments",
                                );
                                return self.err_expr(span);
                            }
                            self.scopes.push(Vec::new());
                            let val_local = self.declare("__opt_val", ok_ty, false, span);
                            self.scopes.pop();
                            return Expr {
                                kind: ExprKind::Match {
                                    scrutinee: Box::new(derefed),
                                    arms: vec![
                                        Arm {
                                            pattern: Pat {
                                                kind: PatKind::Adt {
                                                    adt: AdtRef(0),
                                                    variant: 1,
                                                    args: args.clone(),
                                                    fields: vec![Pat {
                                                        kind: PatKind::Binding(val_local),
                                                        ty: ok_ty,
                                                        span,
                                                    }],
                                                },
                                                ty: bt,
                                                span,
                                            },
                                            guard: None,
                                            guard_refs: Vec::new(),
                                            body: Expr {
                                                kind: ExprKind::Local(val_local),
                                                ty: ok_ty,
                                                span,
                                            },
                                            span,
                                        },
                                        Arm {
                                            pattern: Pat {
                                                kind: PatKind::Adt {
                                                    adt: AdtRef(0),
                                                    variant: 0,
                                                    args,
                                                    fields: Vec::new(),
                                                },
                                                ty: bt,
                                                span,
                                            },
                                            guard: None,
                                            guard_refs: Vec::new(),
                                            body: Expr {
                                                kind: ExprKind::Trap,
                                                ty: never_ty,
                                                span,
                                            },
                                            span,
                                        },
                                    ],
                                },
                                ty: ok_ty,
                                span,
                            };
                        }
                        "unwrap_or" => {
                            if arguments.len() != 1 {
                                self.error(
                                    diag::CALL_ARITY,
                                    span,
                                    "`Option.unwrap_or` takes 1 argument",
                                );
                                return self.err_expr(span);
                            }
                            let def_arg = &arguments[0].value;
                            let def_expr = self.infer_expected(def_arg, ok_ty);
                            let def_expr = self.coerce_value(def_expr, ok_ty, def_arg.span);
                            self.scopes.push(Vec::new());
                            let scrut_local = self.declare("__opt_scrut", bt, false, span);
                            let def_local =
                                self.declare("__opt_default", ok_ty, false, def_arg.span);
                            let val_local = self.declare("__opt_val", ok_ty, false, span);
                            self.scopes.pop();
                            return Expr {
                                kind: ExprKind::Block(vec![
                                    Stmt::Let {
                                        pattern: Pat {
                                            kind: PatKind::Binding(scrut_local),
                                            ty: bt,
                                            span,
                                        },
                                        init: derefed,
                                    },
                                    Stmt::Let {
                                        pattern: Pat {
                                            kind: PatKind::Binding(def_local),
                                            ty: ok_ty,
                                            span: def_arg.span,
                                        },
                                        init: def_expr,
                                    },
                                    Stmt::Expr(Expr {
                                        kind: ExprKind::Match {
                                            scrutinee: Box::new(Expr {
                                                kind: ExprKind::Local(scrut_local),
                                                ty: bt,
                                                span,
                                            }),
                                            arms: vec![
                                                Arm {
                                                    pattern: Pat {
                                                        kind: PatKind::Adt {
                                                            adt: AdtRef(0),
                                                            variant: 1,
                                                            args: args.clone(),
                                                            fields: vec![Pat {
                                                                kind: PatKind::Binding(val_local),
                                                                ty: ok_ty,
                                                                span,
                                                            }],
                                                        },
                                                        ty: bt,
                                                        span,
                                                    },
                                                    guard: None,
                                                    guard_refs: Vec::new(),
                                                    body: Expr {
                                                        kind: ExprKind::Local(val_local),
                                                        ty: ok_ty,
                                                        span,
                                                    },
                                                    span,
                                                },
                                                Arm {
                                                    pattern: Pat {
                                                        kind: PatKind::Adt {
                                                            adt: AdtRef(0),
                                                            variant: 0,
                                                            args,
                                                            fields: Vec::new(),
                                                        },
                                                        ty: bt,
                                                        span,
                                                    },
                                                    guard: None,
                                                    guard_refs: Vec::new(),
                                                    body: Expr {
                                                        kind: ExprKind::Local(def_local),
                                                        ty: ok_ty,
                                                        span: def_arg.span,
                                                    },
                                                    span,
                                                },
                                            ],
                                        },
                                        ty: ok_ty,
                                        span,
                                    }),
                                ]),
                                ty: ok_ty,
                                span,
                            };
                        }
                        _ => {}
                    }
                } else if let TyData::ErrorUnion {
                    ok: ok_ty,
                    err: err_ty,
                } = bt_data
                    && matches!(mname.as_str(), "is_ok" | "is_err" | "unwrap" | "unwrap_or")
                {
                    let bool_ty = self.types.bool();
                    let never_ty = self.types.never();
                    match mname.as_str() {
                        "is_ok" | "is_err" => {
                            if !arguments.is_empty() {
                                self.error(
                                    diag::CALL_ARITY,
                                    span,
                                    format!("error union `.{mname}` takes 0 arguments"),
                                );
                                return self.err_expr(span);
                            }
                            let is_ok = mname == "is_ok";
                            return Expr {
                                kind: ExprKind::Match {
                                    scrutinee: Box::new(derefed),
                                    arms: vec![
                                        Arm {
                                            pattern: Pat {
                                                kind: PatKind::ErrorArm {
                                                    ok: true,
                                                    inner: Box::new(self.wild(ok_ty, span)),
                                                },
                                                ty: bt,
                                                span,
                                            },
                                            guard: None,
                                            guard_refs: Vec::new(),
                                            body: Expr {
                                                kind: ExprKind::Lit(Lit::Bool(is_ok)),
                                                ty: bool_ty,
                                                span,
                                            },
                                            span,
                                        },
                                        Arm {
                                            pattern: Pat {
                                                kind: PatKind::ErrorArm {
                                                    ok: false,
                                                    inner: Box::new(self.wild(err_ty, span)),
                                                },
                                                ty: bt,
                                                span,
                                            },
                                            guard: None,
                                            guard_refs: Vec::new(),
                                            body: Expr {
                                                kind: ExprKind::Lit(Lit::Bool(!is_ok)),
                                                ty: bool_ty,
                                                span,
                                            },
                                            span,
                                        },
                                    ],
                                },
                                ty: bool_ty,
                                span,
                            };
                        }
                        "unwrap" => {
                            if !arguments.is_empty() {
                                self.error(
                                    diag::CALL_ARITY,
                                    span,
                                    "error union `.unwrap` takes 0 arguments",
                                );
                                return self.err_expr(span);
                            }
                            self.scopes.push(Vec::new());
                            let val_local = self.declare("__ok_val", ok_ty, false, span);
                            self.scopes.pop();
                            return Expr {
                                kind: ExprKind::Match {
                                    scrutinee: Box::new(derefed),
                                    arms: vec![
                                        Arm {
                                            pattern: Pat {
                                                kind: PatKind::ErrorArm {
                                                    ok: true,
                                                    inner: Box::new(Pat {
                                                        kind: PatKind::Binding(val_local),
                                                        ty: ok_ty,
                                                        span,
                                                    }),
                                                },
                                                ty: bt,
                                                span,
                                            },
                                            guard: None,
                                            guard_refs: Vec::new(),
                                            body: Expr {
                                                kind: ExprKind::Local(val_local),
                                                ty: ok_ty,
                                                span,
                                            },
                                            span,
                                        },
                                        Arm {
                                            pattern: Pat {
                                                kind: PatKind::ErrorArm {
                                                    ok: false,
                                                    inner: Box::new(self.wild(err_ty, span)),
                                                },
                                                ty: bt,
                                                span,
                                            },
                                            guard: None,
                                            guard_refs: Vec::new(),
                                            body: Expr {
                                                kind: ExprKind::Trap,
                                                ty: never_ty,
                                                span,
                                            },
                                            span,
                                        },
                                    ],
                                },
                                ty: ok_ty,
                                span,
                            };
                        }
                        "unwrap_or" => {
                            if arguments.len() != 1 {
                                self.error(
                                    diag::CALL_ARITY,
                                    span,
                                    "error union `.unwrap_or` takes 1 argument",
                                );
                                return self.err_expr(span);
                            }
                            let def_arg = &arguments[0].value;
                            let def_expr = self.infer_expected(def_arg, ok_ty);
                            let def_expr = self.coerce_value(def_expr, ok_ty, def_arg.span);
                            self.scopes.push(Vec::new());
                            let scrut_local = self.declare("__err_scrut", bt, false, span);
                            let def_local =
                                self.declare("__err_default", ok_ty, false, def_arg.span);
                            let val_local = self.declare("__ok_val", ok_ty, false, span);
                            self.scopes.pop();
                            return Expr {
                                kind: ExprKind::Block(vec![
                                    Stmt::Let {
                                        pattern: Pat {
                                            kind: PatKind::Binding(scrut_local),
                                            ty: bt,
                                            span,
                                        },
                                        init: derefed,
                                    },
                                    Stmt::Let {
                                        pattern: Pat {
                                            kind: PatKind::Binding(def_local),
                                            ty: ok_ty,
                                            span: def_arg.span,
                                        },
                                        init: def_expr,
                                    },
                                    Stmt::Expr(Expr {
                                        kind: ExprKind::Match {
                                            scrutinee: Box::new(Expr {
                                                kind: ExprKind::Local(scrut_local),
                                                ty: bt,
                                                span,
                                            }),
                                            arms: vec![
                                                Arm {
                                                    pattern: Pat {
                                                        kind: PatKind::ErrorArm {
                                                            ok: true,
                                                            inner: Box::new(Pat {
                                                                kind: PatKind::Binding(val_local),
                                                                ty: ok_ty,
                                                                span,
                                                            }),
                                                        },
                                                        ty: bt,
                                                        span,
                                                    },
                                                    guard: None,
                                                    guard_refs: Vec::new(),
                                                    body: Expr {
                                                        kind: ExprKind::Local(val_local),
                                                        ty: ok_ty,
                                                        span,
                                                    },
                                                    span,
                                                },
                                                Arm {
                                                    pattern: Pat {
                                                        kind: PatKind::ErrorArm {
                                                            ok: false,
                                                            inner: Box::new(
                                                                self.wild(err_ty, span),
                                                            ),
                                                        },
                                                        ty: bt,
                                                        span,
                                                    },
                                                    guard: None,
                                                    guard_refs: Vec::new(),
                                                    body: Expr {
                                                        kind: ExprKind::Local(def_local),
                                                        ty: ok_ty,
                                                        span: def_arg.span,
                                                    },
                                                    span,
                                                },
                                            ],
                                        },
                                        ty: ok_ty,
                                        span,
                                    }),
                                ]),
                                ty: ok_ty,
                                span,
                            };
                        }
                        _ => {}
                    }
                }
            }
            let self_ty = if let TyData::Ref { pointee, .. } = *self.types.data(rec_ty) {
                self.types.shallow(pointee)
            } else {
                rec_ty
            };
            let inherent_adt =
                if let TyData::Adt { adt, args, .. } = self.types.data(self_ty).clone() {
                    Some((adt, args, false))
                } else if let TyData::Managed(inner) = *self.types.data(self_ty)
                    && let TyData::Adt { adt, args, .. } =
                        self.types.data(self.types.shallow(inner)).clone()
                {
                    Some((adt, args, true))
                } else if let TyData::Desc(inner) = *self.types.data(self_ty)
                    && let TyData::Adt { adt, args, .. } =
                        self.types.data(self.types.shallow(inner)).clone()
                {
                    Some((adt, args, false))
                } else {
                    None
                };
            if let Some((adt, adt_args, is_managed)) = inherent_adt
                && let Some(method) = self.items.adt(adt).method_named(mname).cloned()
            {
                if !method.public && self.decl.module != self.items.adt(adt).module {
                    self.error(
                        "E-VISIBILITY",
                        span,
                        format!(
                            "method `{mname}` on `{}` is private",
                            self.items.adt(adt).name
                        ),
                    );
                    return self.err_expr(span);
                }
                if !method.has_self {
                    self.error(
                        diag::CALL_ARITY,
                        span,
                        format!("associated function `{mname}` takes no `self` receiver"),
                    );
                    return self.err_expr(span);
                }
                let method_n = self
                    .items
                    .func(method.fn_ref)
                    .generics
                    .len()
                    .saturating_sub(method.host_generic_count);
                let Some(method_args) =
                    self.resolve_explicit_type_args(explicit.map(Vec::as_slice), method_n, span)
                else {
                    return self.err_expr(span);
                };
                let mut full_args = adt_args;
                full_args.extend(method_args);
                let expected_self = self
                    .types
                    .subst(self.items.func(method.fn_ref).params[0].ty, &full_args);
                let expected_self = self.types.shallow(expected_self);
                let rec_expr = if let TyData::Ref { mutable, .. } = *self.types.data(expected_self)
                {
                    if matches!(self.types.data(rec_ty), TyData::Ref { .. }) {
                        receiver
                    } else {
                        if mutable {
                            self.check_mutable_place(&receiver, "borrowed mutably");
                        }
                        let borrow_ty = self.types.intern(TyData::Ref {
                            region: None,
                            mutable,
                            pointee: rec_ty,
                        });
                        Expr {
                            kind: ExprKind::Borrow {
                                mutable,
                                place: Box::new(receiver),
                            },
                            ty: borrow_ty,
                            span: object.span,
                        }
                    }
                } else if is_managed && let TyData::Desc(inner) = *self.types.data(expected_self) {
                    let desc_ty = self.types.intern(TyData::Desc(inner));
                    Expr {
                        kind: ExprKind::Describe(Box::new(receiver)),
                        ty: desc_ty,
                        span: object.span,
                    }
                } else {
                    receiver
                };
                return self.direct_call_with_args(
                    method.fn_ref,
                    full_args,
                    Some(rec_expr),
                    arguments,
                    span,
                );
            }
            let found_inherent_method = self
                .items
                .inherent_impls
                .iter()
                .find_map(|imp| {
                    let impl_args = self.items.match_inherent_impl(imp, self_ty, self.types)?;
                    imp.methods
                        .iter()
                        .find(|m| m.name == *mname)
                        .cloned()
                        .map(|m| (m, impl_args, false))
                })
                .or_else(|| {
                    if let TyData::Managed(inner) = *self.types.data(self_ty) {
                        let desc_ty = self.types.intern(TyData::Desc(inner));
                        self.items.inherent_impls.iter().find_map(|imp| {
                            let impl_args =
                                self.items.match_inherent_impl(imp, desc_ty, self.types)?;
                            imp.methods
                                .iter()
                                .find(|m| m.name == *mname)
                                .cloned()
                                .map(|m| (m, impl_args, true))
                        })
                    } else {
                        None
                    }
                });
            if let Some((method, impl_args, is_managed)) = found_inherent_method {
                if !method.public && self.decl.module != method.module {
                    self.error("E-VISIBILITY", span, format!("method `{mname}` is private"));
                    return self.err_expr(span);
                }
                if !method.has_self {
                    self.error(
                        diag::CALL_ARITY,
                        span,
                        format!("associated function `{mname}` takes no `self` receiver"),
                    );
                    return self.err_expr(span);
                }
                let method_n = self
                    .items
                    .func(method.fn_ref)
                    .generics
                    .len()
                    .saturating_sub(method.impl_generic_count);
                let Some(method_args) =
                    self.resolve_explicit_type_args(explicit.map(Vec::as_slice), method_n, span)
                else {
                    return self.err_expr(span);
                };
                let mut full_args = impl_args;
                full_args.extend(method_args);
                let expected_self = self
                    .types
                    .subst(self.items.func(method.fn_ref).params[0].ty, &full_args);
                let expected_self = self.types.shallow(expected_self);
                let rec_expr = if let TyData::Ref { mutable, .. } = *self.types.data(expected_self)
                {
                    if matches!(self.types.data(rec_ty), TyData::Ref { .. }) {
                        receiver
                    } else {
                        if mutable {
                            self.check_mutable_place(&receiver, "borrowed mutably");
                        }
                        let borrow_ty = self.types.intern(TyData::Ref {
                            region: None,
                            mutable,
                            pointee: rec_ty,
                        });
                        Expr {
                            kind: ExprKind::Borrow {
                                mutable,
                                place: Box::new(receiver),
                            },
                            ty: borrow_ty,
                            span: object.span,
                        }
                    }
                } else if is_managed && let TyData::Desc(inner) = *self.types.data(expected_self) {
                    let desc_ty = self.types.intern(TyData::Desc(inner));
                    Expr {
                        kind: ExprKind::Describe(Box::new(receiver)),
                        ty: desc_ty,
                        span: object.span,
                    }
                } else {
                    receiver
                };
                return self.direct_call_with_args(
                    method.fn_ref,
                    full_args,
                    Some(rec_expr),
                    arguments,
                    span,
                );
            }
            if let TyData::Dyn {
                trait_ref: tr,
                trait_args,
                assoc_bindings,
            }
            | TyData::OwnedDyn {
                trait_ref: tr,
                trait_args,
                assoc_bindings,
                ..
            } = self.types.data(self_ty).clone()
            {
                if explicit.is_some() {
                    self.error(
                        diag::TYPE_ARGUMENT,
                        span,
                        format!("dyn method `{mname}` does not accept explicit type arguments"),
                    );
                    return self.err_expr(span);
                }
                let tr_decl = self.items.trait_decl(tr);
                if let Some((slot, m)) = tr_decl
                    .methods
                    .iter()
                    .enumerate()
                    .find(|(_, m)| m.name == *mname && !m.params.is_empty())
                {
                    let m_params = m.params.clone();
                    let m_result = m.result;
                    if arguments.len() + 1 != m_params.len() {
                        self.error(
                            diag::CALL_ARITY,
                            span,
                            format!(
                                "method `{mname}` expects {} argument(s), found {}",
                                m_params.len().saturating_sub(1),
                                arguments.len()
                            ),
                        );
                        return self.err_expr(span);
                    }
                    let rec_expr = if let TyData::Ref { mutable, .. } =
                        *self.types.data(self.types.shallow(m_params[0]))
                    {
                        if let TyData::Ref { .. } = *self.types.data(rec_ty) {
                            let want_ref = self.types.intern(TyData::Ref {
                                region: None,
                                mutable,
                                pointee: self_ty,
                            });
                            self.coerce_arg(receiver, want_ref, object.span)
                        } else {
                            if mutable {
                                self.check_mutable_place(&receiver, "exclusively borrowed");
                            }
                            let borrow_ty = self.types.intern(TyData::Ref {
                                region: None,
                                mutable,
                                pointee: rec_ty,
                            });
                            Expr {
                                kind: ExprKind::Borrow {
                                    mutable,
                                    place: Box::new(receiver),
                                },
                                ty: borrow_ty,
                                span: object.span,
                            }
                        }
                    } else {
                        receiver
                    };
                    let mut args = Vec::with_capacity(arguments.len());
                    for (a, pt) in arguments.iter().zip(&m_params[1..]) {
                        let expected = self.types.subst_with_self(*pt, &trait_args, self_ty);
                        let expected =
                            self.types
                                .subst_self_assoc(expected, self_ty, tr, &assoc_bindings);
                        let expected = self.normalize(expected);
                        let v = self.infer_expected(&a.value, expected);
                        args.push(self.coerce_arg(v, expected, a.span));
                    }
                    let ret = self.types.subst_with_self(m_result, &trait_args, self_ty);
                    let ret = self
                        .types
                        .subst_self_assoc(ret, self_ty, tr, &assoc_bindings);
                    let ret = self.normalize(ret);
                    return Expr {
                        kind: ExprKind::DynCall {
                            object: Box::new(rec_expr),
                            slot: slot as u32,
                            args,
                        },
                        ty: ret,
                        span,
                    };
                }
            }
            let mut resolved_self_ty = self_ty;
            let mut receiver = receiver;
            let found_trait = if let TyData::Param(param_idx) = *self.types.data(self_ty) {
                let bounds = self
                    .decl
                    .generics
                    .get(param_idx as usize)
                    .map(|g| g.traits.clone())
                    .unwrap_or_default();
                let mut found = None;
                'search_bounds: for bound in &bounds {
                    let closure = self
                        .items
                        .supertrait_closure(bound, Some(self_ty), self.types);
                    for entailed in closure {
                        let tr = self.items.trait_decl(entailed.trait_ref);
                        if let Some(m) = tr
                            .methods
                            .iter()
                            .find(|m| m.name == *mname && !m.params.is_empty())
                        {
                            found = Some((
                                entailed.trait_ref,
                                entailed.trait_args,
                                m.generics.len(),
                                m.params.clone(),
                                m.result,
                            ));
                            break 'search_bounds;
                        }
                    }
                }
                if found.is_none() {
                    // Also allow blanket impls whose bounds are satisfied by `Param(param_idx)`.
                    let env_generics = &self.decl.generics;
                    found = self.items.trait_impls.iter().find_map(|imp| {
                        let impl_args =
                            self.items
                                .match_impl_in_env(imp, self_ty, env_generics, self.types)?;
                        let tr = self.items.trait_decl(imp.trait_ref);
                        tr.methods
                            .iter()
                            .find(|m| m.name == *mname && !m.params.is_empty())
                            .map(|m| {
                                let resolved_targs: Vec<Ty> = imp
                                    .trait_args
                                    .iter()
                                    .map(|t| self.types.subst(*t, &impl_args))
                                    .collect();
                                (
                                    imp.trait_ref,
                                    resolved_targs,
                                    m.generics.len(),
                                    m.params.clone(),
                                    m.result,
                                )
                            })
                    });
                }
                found
            } else {
                let env_generics = &self.decl.generics;
                let try_lookup = |target_ty: Ty, types: &mut Types| {
                    self.items.trait_impls.iter().find_map(|imp| {
                        let impl_args =
                            self.items
                                .match_impl_in_env(imp, target_ty, env_generics, types)?;
                        let tr = self.items.trait_decl(imp.trait_ref);
                        tr.methods
                            .iter()
                            .find(|m| m.name == *mname && !m.params.is_empty())
                            .map(|m| {
                                let resolved_targs: Vec<Ty> = imp
                                    .trait_args
                                    .iter()
                                    .map(|t| types.subst(*t, &impl_args))
                                    .collect();
                                (
                                    imp.trait_ref,
                                    resolved_targs,
                                    m.generics.len(),
                                    m.params.clone(),
                                    m.result,
                                )
                            })
                    })
                };
                if let Some(found) = try_lookup(self_ty, self.types) {
                    Some(found)
                } else if let TyData::Managed(inner) = *self.types.data(self_ty) {
                    let desc_ty = self.types.intern(TyData::Desc(inner));
                    if let Some(found) = try_lookup(desc_ty, self.types) {
                        resolved_self_ty = desc_ty;
                        receiver = Expr {
                            kind: ExprKind::Describe(Box::new(receiver)),
                            ty: desc_ty,
                            span: object.span,
                        };
                        Some(found)
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            let self_ty = resolved_self_ty;
            if let Some((tr_ref, tr_args, m_generics_len, m_params, m_result)) = found_trait {
                if arguments.len() + 1 != m_params.len() {
                    self.error(
                        diag::CALL_ARITY,
                        span,
                        format!(
                            "method `{mname}` expects {} argument(s), found {}",
                            m_params.len().saturating_sub(1),
                            arguments.len()
                        ),
                    );
                    return self.err_expr(span);
                }
                let Some(method_args) = self.resolve_explicit_type_args(
                    explicit.map(Vec::as_slice),
                    m_generics_len,
                    span,
                ) else {
                    return self.err_expr(span);
                };
                let mut full_subst = tr_args.clone();
                full_subst.extend(method_args.iter().copied());
                let expected_self = self
                    .types
                    .subst_with_self(m_params[0], &full_subst, self_ty);
                let expected_self = self.normalize(expected_self);
                let expected_self_shallow = self.types.shallow(expected_self);
                let rec_expr = if let TyData::Ref { mutable, .. } =
                    *self.types.data(expected_self_shallow)
                    && !matches!(self.types.data(rec_ty), TyData::Ref { .. })
                {
                    let borrow_ty = self.types.intern(TyData::Ref {
                        region: None,
                        mutable,
                        pointee: rec_ty,
                    });
                    Expr {
                        kind: ExprKind::Borrow {
                            mutable,
                            place: Box::new(receiver),
                        },
                        ty: borrow_ty,
                        span: object.span,
                    }
                } else {
                    receiver
                };
                let mut args = Vec::with_capacity(m_params.len());
                args.push(self.coerce_arg(rec_expr, expected_self, object.span));
                for (a, pt) in arguments.iter().zip(&m_params[1..]) {
                    let expected = self.types.subst_with_self(*pt, &full_subst, self_ty);
                    let expected = self.normalize(expected);
                    let v = self.infer(&a.value);
                    args.push(self.coerce_arg(v, expected, a.span));
                }
                let ret = self.types.subst_with_self(m_result, &full_subst, self_ty);
                let ret = self.normalize(ret);
                return Expr {
                    kind: ExprKind::TraitCall {
                        trait_ref: tr_ref,
                        trait_args: tr_args,
                        method_args,
                        self_ty,
                        method: mname.clone(),
                        args,
                    },
                    ty: ret,
                    span,
                };
            }
            if explicit.is_some() {
                self.error(
                    diag::TYPE_ARGUMENT,
                    span,
                    format!("method `{mname}` does not accept explicit type arguments here"),
                );
                return self.err_expr(span);
            }
            fallback_callee = Some(self.member_on_kind(
                receiver,
                &ast::MemberName::Name(mname.clone()),
                callee.span,
            ));
        }

        // Value call: function pointer, closure, `impl` callable or generic `F: Fn*`.
        if explicit.is_some() {
            self.error(
                diag::TYPE_ARGUMENT,
                span,
                "indirect calls take no explicit type arguments",
            );
        }
        let callee_expr = fallback_callee.unwrap_or_else(|| self.infer(callee));
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
                let v = self.infer_expected(&a.value, *p);
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
        } else if matches!(self.types.data(ct), TyData::Var(_)) {
            let mut args = Vec::with_capacity(arguments.len());
            let mut param_tys = Vec::with_capacity(arguments.len());
            for a in arguments {
                let v = self.infer(&a.value);
                param_tys.push(v.ty);
                args.push(v);
            }
            let result = self.types.fresh(VarKind::General);
            let fn_ptr = self.types.intern(TyData::FnPtr {
                params: param_tys,
                result,
            });
            self.unify(ct, fn_ptr, span);
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
        if matches!(
            decl.callback_role,
            Some(
                zil_ir::body::ForeignCallbackRole::ScopedRegister
                    | zil_ir::body::ForeignCallbackRole::ScopedClose
            )
        ) {
            self.error(
                diag::FFI_DECLARATION,
                span,
                format!(
                    "`{}` declares a scoped callback provider role and may only be passed to `c_with_callback`",
                    decl.name
                ),
            );
            return self.err_expr(span);
        }
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
        if name == "owned_dyn" {
            let (t, ctx_ast) = match explicit {
                Some([ast::GenericArg::Type(t)]) => (t, None),
                Some([ast::GenericArg::Type(t), ast::GenericArg::Type(c)]) => (t, Some(c)),
                _ => {
                    self.error(
                        diag::TYPE_ARGUMENT,
                        span,
                        "`owned_dyn` requires one or two type arguments (`dyn Trait[, Context]`)",
                    );
                    return Some(self.err_expr(span));
                }
            };
            let target = self.resolve_written(t);
            let TyData::Dyn {
                trait_ref: tr,
                trait_args,
                assoc_bindings,
            } = self.types.data(target).clone()
            else {
                self.error(
                    diag::TYPE_ARGUMENT,
                    span,
                    "`owned_dyn` first type argument must be `dyn Trait`",
                );
                return Some(self.err_expr(span));
            };
            if let Err(msg) =
                self.items
                    .check_dyn_object_safe(tr, &trait_args, &assoc_bindings, self.types)
            {
                self.error(diag::GENERIC_BOUND, span, msg);
                return Some(self.err_expr(span));
            }
            let context = ctx_ast.map(|c| self.resolve_written(c));
            let [arg] = arguments else {
                self.error(diag::CALL_ARITY, span, "`owned_dyn` takes one argument");
                return Some(self.err_expr(span));
            };
            let val = self.infer(&arg.value);
            let val_ty = self.types.shallow(val.ty);
            if !self.types.is_error(val_ty) {
                let bound = crate::items::TraitBound {
                    trait_ref: tr,
                    trait_args: trait_args.clone(),
                    assoc_bindings: assoc_bindings.clone(),
                };
                if !self.items.satisfies_trait_bound(
                    val_ty,
                    &bound,
                    &self.decl.generics,
                    self.types,
                ) {
                    let trait_name = self.items.trait_decl(tr).name.clone();
                    self.error(
                        diag::GENERIC_BOUND,
                        arg.span,
                        format!(
                            "value does not satisfy trait `{trait_name}` required by `owned_dyn`"
                        ),
                    );
                    return Some(self.err_expr(span));
                }
            }
            let ok = self.types.intern(TyData::OwnedDyn {
                trait_ref: tr,
                trait_args,
                assoc_bindings,
                context,
            });
            let err = self.types.intern(TyData::Adt {
                adt: AdtRef(1),
                args: Vec::new(),
                regions: Vec::new(),
            });
            let ret = self.types.intern(TyData::ErrorUnion { ok, err });
            return Some(Expr {
                kind: ExprKind::Intrinsic {
                    op: IntrinsicOp::OwnedDynCreate,
                    args: vec![val],
                },
                ty: ret,
                span,
            });
        }
        if name == "task_await" {
            if self.defer_depth > 0 {
                self.error(
                    diag::LOOP_CONTROL,
                    span,
                    "`task_await` is not allowed inside `defer` cleanup",
                );
                return Some(self.err_expr(span));
            }
            if explicit.is_some() {
                self.error(
                    diag::TYPE_ARGUMENT,
                    span,
                    "`task_await` takes no explicit type arguments",
                );
            }
            let Some((callee_arg, rest_args)) = arguments.split_first() else {
                self.error(
                    diag::CALL_ARITY,
                    span,
                    "`task_await` requires a child function argument",
                );
                return Some(self.err_expr(span));
            };
            return Some(self.call(&callee_arg.value, rest_args, span));
        }
        if name == "atomic_scope" {
            if explicit.is_some() {
                self.error(
                    diag::TYPE_ARGUMENT,
                    span,
                    "`atomic_scope` takes no explicit type arguments",
                );
            }
            let [init_arg, body_arg] = arguments else {
                self.error(diag::CALL_ARITY, span, "`atomic_scope` expects 2 arguments");
                return Some(self.err_expr(span));
            };
            let init_expr = self.infer(&init_arg.value);
            let body_expr = self.infer(&body_arg.value);
            let body_ty = self.types.shallow(body_expr.ty);
            let callable = if let TyData::Closure {
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
            } = self.types.data(body_ty).clone()
            {
                Some((
                    Some(match kind {
                        ClosureKind::Fn => ClosureCallMode::Shared,
                        ClosureKind::FnMut => ClosureCallMode::Mut,
                        ClosureKind::FnOnce => ClosureCallMode::Once,
                    }),
                    params,
                    result,
                ))
            } else if let TyData::Param(idx) = self.types.data(body_ty) {
                self.decl
                    .generics
                    .get(*idx as usize)
                    .and_then(|g| g.callable.clone())
                    .map(|(kind, params, result)| {
                        (
                            Some(match kind {
                                ClosureKind::Fn => ClosureCallMode::Shared,
                                ClosureKind::FnMut => ClosureCallMode::Mut,
                                ClosureKind::FnOnce => ClosureCallMode::Once,
                            }),
                            params,
                            result,
                        )
                    })
            } else if let TyData::FnPtr { params, result } = self.types.data(body_ty).clone() {
                Some((None, params, result))
            } else {
                None
            };
            let Some((call_mode, params, ret_ty)) = callable else {
                self.error(
                    diag::TYPE_MISMATCH,
                    body_arg.span,
                    "`atomic_scope` second argument must be a 1-argument callable",
                );
                return Some(self.err_expr(span));
            };
            if params.len() != 1 {
                self.error(
                    diag::CALL_ARITY,
                    body_arg.span,
                    "`atomic_scope` body must take 1 argument (`&Atomic*`)",
                );
                return Some(self.err_expr(span));
            }
            let param_ty = self.types.shallow(params[0]);
            let cell_kind = if let TyData::Ref {
                mutable: false,
                pointee,
                ..
            } = self.types.data(param_ty).clone()
                && let TyData::Opaque(OpaqueKind::SyncCell(k)) =
                    self.types.data(self.types.shallow(pointee))
            {
                Some(*k)
            } else if let TyData::Int(IntTy::I32) =
                self.types.data(self.types.shallow(init_expr.ty))
            {
                Some(zil_ir::body::SyncCellKind::I32)
            } else if let TyData::Int(IntTy::I64) =
                self.types.data(self.types.shallow(init_expr.ty))
            {
                Some(zil_ir::body::SyncCellKind::I64)
            } else if let TyData::Int(IntTy::Usize) =
                self.types.data(self.types.shallow(init_expr.ty))
            {
                Some(zil_ir::body::SyncCellKind::Usize)
            } else if let TyData::Var(_) = self.types.data(self.types.shallow(init_expr.ty)) {
                Some(zil_ir::body::SyncCellKind::I32)
            } else {
                None
            };
            let Some(cell_kind) = cell_kind else {
                self.error(
                    diag::TYPE_MISMATCH,
                    body_arg.span,
                    "`atomic_scope` requires an `&AtomicI32`, `&AtomicI64`, or `&AtomicUsize` body parameter",
                );
                return Some(self.err_expr(span));
            };
            let elem_int = match cell_kind {
                zil_ir::body::SyncCellKind::I32 => IntTy::I32,
                zil_ir::body::SyncCellKind::I64 => IntTy::I64,
                zil_ir::body::SyncCellKind::Usize => IntTy::Usize,
            };
            let elem_ty = self.types.intern(TyData::Int(elem_int));
            let cell_ty = self
                .types
                .intern(TyData::Opaque(OpaqueKind::SyncCell(cell_kind)));
            let cell_ref_ty = self.types.intern(TyData::Ref {
                region: None,
                mutable: false,
                pointee: cell_ty,
            });
            let bool_ty = self.types.intern(TyData::Bool);
            let init_expr = self.coerce_arg(init_expr, elem_ty, init_arg.span);
            self.unify(param_ty, cell_ref_ty, body_arg.span);

            self.scopes.push(Vec::new());
            let cell_local = self.declare("__atomic_cell", cell_ty, false, span);
            let res_local = self.declare("__atomic_res", ret_ty, false, span);
            let cell_ref_expr = Expr {
                kind: ExprKind::Borrow {
                    mutable: false,
                    place: Box::new(Expr {
                        kind: ExprKind::Local(cell_local),
                        ty: cell_ty,
                        span,
                    }),
                },
                ty: cell_ref_ty,
                span,
            };
            let call_expr = match call_mode {
                Some(mode) => {
                    if mode == ClosureCallMode::Mut && is_place(&body_expr.kind) {
                        self.check_mutable_place(&body_expr, "called as a mutable closure");
                    }
                    Expr {
                        kind: ExprKind::ClosureCall {
                            closure: Box::new(body_expr),
                            mode,
                            args: vec![cell_ref_expr],
                        },
                        ty: ret_ty,
                        span,
                    }
                }
                None => Expr {
                    kind: ExprKind::IndirectCall {
                        callee: Box::new(body_expr),
                        args: vec![cell_ref_expr],
                    },
                    ty: ret_ty,
                    span,
                },
            };
            let tuple_ty = self.types.intern(TyData::Tuple(vec![bool_ty, ret_ty]));
            let block = Expr {
                kind: ExprKind::Block(vec![
                    Stmt::Let {
                        pattern: Pat {
                            kind: PatKind::Binding(cell_local),
                            ty: cell_ty,
                            span,
                        },
                        init: Expr {
                            kind: ExprKind::Intrinsic {
                                op: IntrinsicOp::SyncInit,
                                args: vec![init_expr],
                            },
                            ty: cell_ty,
                            span,
                        },
                    },
                    Stmt::Let {
                        pattern: Pat {
                            kind: PatKind::Binding(res_local),
                            ty: ret_ty,
                            span,
                        },
                        init: call_expr,
                    },
                    Stmt::Expr(Expr {
                        kind: ExprKind::Tuple(vec![
                            Expr {
                                kind: ExprKind::Lit(Lit::Bool(true)),
                                ty: bool_ty,
                                span,
                            },
                            Expr {
                                kind: ExprKind::Local(res_local),
                                ty: ret_ty,
                                span,
                            },
                        ]),
                        ty: tuple_ty,
                        span,
                    }),
                ]),
                ty: tuple_ty,
                span,
            };
            self.scopes.pop();
            return Some(block);
        }
        if name == "mutex_with" {
            if explicit.is_some() {
                self.error(
                    diag::TYPE_ARGUMENT,
                    span,
                    "`mutex_with` takes no explicit type arguments",
                );
            }
            let [cell_arg, body_arg] = arguments else {
                self.error(diag::CALL_ARITY, span, "`mutex_with` expects 2 arguments");
                return Some(self.err_expr(span));
            };
            let cell_expr = self.infer(&cell_arg.value);
            let body_expr = self.infer(&body_arg.value);
            let elem = self.types.fresh(VarKind::General);
            let mutex_inner = self.types.intern(TyData::Opaque(OpaqueKind::Mutex(elem)));
            let mutex_desc = self.types.intern(TyData::Desc(mutex_inner));
            let cell_expr = self.coerce_value(cell_expr, mutex_desc, cell_arg.span);
            let guard_ty = self
                .types
                .intern(TyData::Opaque(OpaqueKind::MutexGuard(elem)));
            let guard_mut_ref = self.types.intern(TyData::Ref {
                region: None,
                mutable: true,
                pointee: guard_ty,
            });
            let val_mut_ref = self.types.intern(TyData::Ref {
                region: None,
                mutable: true,
                pointee: elem,
            });
            let body_ty = self.types.shallow(body_expr.ty);
            let callable = if let TyData::Closure {
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
            } = self.types.data(body_ty).clone()
            {
                Some((
                    Some(match kind {
                        ClosureKind::Fn => ClosureCallMode::Shared,
                        ClosureKind::FnMut => ClosureCallMode::Mut,
                        ClosureKind::FnOnce => ClosureCallMode::Once,
                    }),
                    params,
                    result,
                ))
            } else if let TyData::Param(idx) = self.types.data(body_ty) {
                self.decl
                    .generics
                    .get(*idx as usize)
                    .and_then(|g| g.callable.clone())
                    .map(|(kind, params, result)| {
                        (
                            Some(match kind {
                                ClosureKind::Fn => ClosureCallMode::Shared,
                                ClosureKind::FnMut => ClosureCallMode::Mut,
                                ClosureKind::FnOnce => ClosureCallMode::Once,
                            }),
                            params,
                            result,
                        )
                    })
            } else if let TyData::FnPtr { params, result } = self.types.data(body_ty).clone() {
                Some((None, params, result))
            } else {
                None
            };
            let Some((call_mode, params, ret_ty)) = callable else {
                self.error(
                    diag::TYPE_MISMATCH,
                    body_arg.span,
                    "`mutex_with` second argument must be a 1-argument callable",
                );
                return Some(self.err_expr(span));
            };
            if params.len() != 1 {
                self.error(
                    diag::CALL_ARITY,
                    body_arg.span,
                    "`mutex_with` body must take 1 argument (`&mut T`)",
                );
                return Some(self.err_expr(span));
            }
            self.unify(params[0], val_mut_ref, body_arg.span);

            self.scopes.push(Vec::new());
            let guard_local = self.declare("__mutex_guard", guard_ty, true, span);
            let val_local = self.declare("__mutex_val", val_mut_ref, false, span);
            let res_local = self.declare("__mutex_res", ret_ty, false, span);
            let val_arg_expr = Expr {
                kind: ExprKind::Local(val_local),
                ty: val_mut_ref,
                span,
            };
            let call_expr = match call_mode {
                Some(mode) => {
                    if mode == ClosureCallMode::Mut && is_place(&body_expr.kind) {
                        self.check_mutable_place(&body_expr, "called as a mutable closure");
                    }
                    Expr {
                        kind: ExprKind::ClosureCall {
                            closure: Box::new(body_expr),
                            mode,
                            args: vec![val_arg_expr],
                        },
                        ty: ret_ty,
                        span,
                    }
                }
                None => Expr {
                    kind: ExprKind::IndirectCall {
                        callee: Box::new(body_expr),
                        args: vec![val_arg_expr],
                    },
                    ty: ret_ty,
                    span,
                },
            };
            let block = Expr {
                kind: ExprKind::Block(vec![
                    Stmt::Let {
                        pattern: Pat {
                            kind: PatKind::Binding(guard_local),
                            ty: guard_ty,
                            span,
                        },
                        init: Expr {
                            kind: ExprKind::Intrinsic {
                                op: IntrinsicOp::MutexLock,
                                args: vec![cell_expr],
                            },
                            ty: guard_ty,
                            span,
                        },
                    },
                    Stmt::Let {
                        pattern: Pat {
                            kind: PatKind::Binding(val_local),
                            ty: val_mut_ref,
                            span,
                        },
                        init: Expr {
                            kind: ExprKind::Intrinsic {
                                op: IntrinsicOp::MutexValue,
                                args: vec![Expr {
                                    kind: ExprKind::Borrow {
                                        mutable: true,
                                        place: Box::new(Expr {
                                            kind: ExprKind::Local(guard_local),
                                            ty: guard_ty,
                                            span,
                                        }),
                                    },
                                    ty: guard_mut_ref,
                                    span,
                                }],
                            },
                            ty: val_mut_ref,
                            span,
                        },
                    },
                    Stmt::Let {
                        pattern: Pat {
                            kind: PatKind::Binding(res_local),
                            ty: ret_ty,
                            span,
                        },
                        init: call_expr,
                    },
                    Stmt::Expr(Expr {
                        kind: ExprKind::Local(res_local),
                        ty: ret_ty,
                        span,
                    }),
                ]),
                ty: ret_ty,
                span,
            };
            self.scopes.pop();
            return Some(block);
        }
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
            "bytes_view" | "bytes_view_mut" => IntrinsicOp::ByteBufferBorrow,
            "mutex_create" => IntrinsicOp::MutexCreate,
            "mutex_lock" => IntrinsicOp::MutexLock,
            "mutex_value" => IntrinsicOp::MutexValue,
            "mutex_unlock" => IntrinsicOp::MutexUnlock,
            "atomic_load" => IntrinsicOp::SyncLoad,
            "atomic_store" => IntrinsicOp::SyncStore,
            "atomic_fetch_add" => IntrinsicOp::SyncFetchAdd,
            "atomic_compare_exchange" => IntrinsicOp::SyncCompareExchange,
            "atomic_load_ordered" => {
                IntrinsicOp::SyncLoadOrdered(zil_ir::body::MemoryOrder::SeqCst)
            }
            "atomic_store_ordered" => {
                IntrinsicOp::SyncStoreOrdered(zil_ir::body::MemoryOrder::SeqCst)
            }
            "atomic_fetch_add_ordered" => {
                IntrinsicOp::SyncFetchAddOrdered(zil_ir::body::MemoryOrder::SeqCst)
            }
            "atomic_exchange_ordered" => {
                IntrinsicOp::SyncExchangeOrdered(zil_ir::body::MemoryOrder::SeqCst)
            }
            "atomic_compare_exchange_ordered" => IntrinsicOp::SyncCompareExchangeOrdered {
                success: zil_ir::body::MemoryOrder::SeqCst,
                failure: zil_ir::body::MemoryOrder::SeqCst,
            },
            "c_null" => IntrinsicOp::CNull,
            "c_callback" => IntrinsicOp::CCallback,
            "thread_scope" => IntrinsicOp::ThreadScope,
            "task_create" | "task_create_owned" => IntrinsicOp::TaskCreate,
            "task_valid" => IntrinsicOp::TaskValid,
            "task_yield" => IntrinsicOp::TaskYield,
            "task_poll" => IntrinsicOp::TaskPoll,
            "task_request_cancel" => IntrinsicOp::TaskRequestCancel,
            "task_poll_close" => IntrinsicOp::TaskPollClose,
            "task_finish_destroy" => IntrinsicOp::TaskFinishDestroy,
            "c_with_callback" => IntrinsicOp::ScopedCallback,
            "c_register" => IntrinsicOp::RegistrationStart,
            "registration_valid" => IntrinsicOp::RegistrationValid,
            "registration_close" => IntrinsicOp::RegistrationClose,
            "c_register_async" => IntrinsicOp::AsyncRegistrationStart,
            "async_registration_valid" => IntrinsicOp::AsyncRegistrationValid,
            "async_registration_request_close" => IntrinsicOp::AsyncRegistrationRequestClose,
            "async_registration_poll_close" => IntrinsicOp::AsyncRegistrationPollClose,
            "async_registration_finish_destroy" | "async_registration_finish_destroy_checked" => {
                IntrinsicOp::AsyncRegistrationFinishDestroy
            }
            _ => return None,
        };
        if matches!(
            op,
            IntrinsicOp::RegistrationStart
                | IntrinsicOp::AsyncRegistrationStart
                | IntrinsicOp::ScopedCallback
        ) && self.in_unsafe == 0
        {
            self.error(
                diag::FFI_UNSAFE,
                span,
                format!("calling `{name}` requires an `unsafe` block"),
            );
        }
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

        if op == IntrinsicOp::TaskCreate {
            if arguments.is_empty() {
                self.error(
                    diag::CALL_ARITY,
                    span,
                    format!("`{name}` expects at least 1 argument"),
                );
                return Some(self.err_expr(span));
            }
            let res = self.types.fresh(VarKind::General);
            let param_tys: Vec<Ty> = (1..arguments.len())
                .map(|_| self.types.fresh(VarKind::General))
                .collect();
            let worker_expected = self.types.intern(TyData::FnPtr {
                params: param_tys.clone(),
                result: res,
            });
            let worker_raw = self.infer(&arguments[0].value);
            let worker = self.coerce_arg(worker_raw, worker_expected, arguments[0].span);
            let mut args = Vec::with_capacity(arguments.len());
            args.push(worker);
            for (a, pt) in arguments[1..].iter().zip(&param_tys) {
                let v = self.infer(&a.value);
                args.push(self.coerce_arg(v, *pt, a.span));
            }
            let full_context = match param_tys.as_slice() {
                [] => None,
                [single] => Some(*single),
                many => Some(self.types.intern(TyData::Tuple(many.to_vec()))),
            };
            let borrowed: Vec<Ty> = param_tys
                .iter()
                .copied()
                .filter(|t| self.ty_contains_ref(*t))
                .collect();
            let compact_context = match borrowed.as_slice() {
                [] => None,
                [single] => Some(*single),
                many => Some(self.types.intern(TyData::Tuple(many.to_vec()))),
            };
            let ret_shallow = self.types.shallow(self.ret_ty);
            let exp_task_ctx = if let TyData::ErrorUnion { ok, .. } =
                self.types.data(ret_shallow).clone()
                && let ok_s = self.types.shallow(ok)
                && let TyData::Managed(inner) = self.types.data(ok_s).clone()
                && let inner_s = self.types.shallow(inner)
                && let TyData::Opaque(OpaqueKind::Task { context, .. }) =
                    self.types.data(inner_s).clone()
            {
                context
            } else {
                None
            };
            let context = if let (Some(full), Some(exp)) = (full_context, exp_task_ctx)
                && self.types.can_unify(full, exp)
            {
                Some(full)
            } else {
                compact_context
            };
            let inner = self.types.intern(TyData::Opaque(OpaqueKind::Task {
                result: res,
                context,
            }));
            let ok = self.types.intern(TyData::Managed(inner));
            let err = if name == "task_create_owned" {
                let inputs_ty = self.types.intern(TyData::Tuple(param_tys));
                self.types.intern(TyData::Adt {
                    adt: AdtRef(6),
                    args: vec![inputs_ty],
                    regions: Vec::new(),
                })
            } else {
                self.types.intern(TyData::Adt {
                    adt: AdtRef(3),
                    args: Vec::new(),
                    regions: Vec::new(),
                })
            };
            let ret_ty = self.types.intern(TyData::ErrorUnion { ok, err });
            return Some(Expr {
                kind: ExprKind::Intrinsic { op, args },
                ty: ret_ty,
                span,
            });
        }

        if op == IntrinsicOp::ThreadScope {
            let [w_arg, c_arg, p_arg] = arguments else {
                self.error(diag::CALL_ARITY, span, "`thread_scope` expects 3 arguments");
                return Some(self.err_expr(span));
            };
            let w = self.infer(&w_arg.value);
            let c = self.infer(&c_arg.value);
            let p = self.infer(&p_arg.value);
            let cell_ty = self.types.fresh(VarKind::General);
            let r1 = self.types.fresh(VarKind::General);
            let w_expected = self.types.intern(TyData::FnPtr {
                params: vec![cell_ty],
                result: r1,
            });
            let w = self.coerce_arg(w, w_expected, w_arg.span);
            let c = self.coerce_arg(c, cell_ty, c_arg.span);
            let r2 = if let TyData::Closure { params, result, .. } =
                self.types.data(self.types.shallow(p.ty)).clone()
                && params.is_empty()
            {
                result
            } else {
                self.error(
                    diag::TYPE_MISMATCH,
                    p_arg.span,
                    "`thread_scope` third argument must be a 0-argument closure",
                );
                self.types.error()
            };
            let is_task_transfer =
                if let TyData::Managed(inner) = self.types.data(self.types.shallow(c.ty)) {
                    matches!(
                        self.types.data(self.types.shallow(*inner)),
                        TyData::Opaque(OpaqueKind::Task { .. })
                    )
                } else {
                    false
                };
            let ret_ty = if is_task_transfer {
                self.types.intern(TyData::Adt {
                    adt: AdtRef(7),
                    args: vec![cell_ty, r1, r2],
                    regions: Vec::new(),
                })
            } else {
                self.types.intern(TyData::Tuple(vec![bool_ty, r1, r2]))
            };
            return Some(Expr {
                kind: ExprKind::Intrinsic {
                    op,
                    args: vec![w, c, p],
                },
                ty: ret_ty,
                span,
            });
        }

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
                    region: None,
                    mutable: mutability == Mutability::Exclusive,
                    pointee: elem,
                });
                (vec![desc, i64_ty], res)
            }
            IntrinsicOp::ByteBufferBorrow => {
                let slice_u8 = self.types.intern(TyData::Slice(u8_ty));
                let res = self.types.intern(TyData::Ref {
                    region: None,
                    mutable: name == "bytes_view_mut",
                    pointee: slice_u8,
                });
                (vec![bytes_desc], res)
            }
            IntrinsicOp::MutexCreate => {
                let elem = self.types.fresh(VarKind::General);
                let inner = self.types.intern(TyData::Opaque(OpaqueKind::Mutex(elem)));
                let ok = self.types.intern(TyData::Managed(inner));
                let err = self.types.intern(TyData::Adt {
                    adt: AdtRef(5),
                    args: Vec::new(),
                    regions: Vec::new(),
                });
                (
                    vec![elem],
                    self.types.intern(TyData::ErrorUnion { ok, err }),
                )
            }
            IntrinsicOp::MutexLock => {
                let elem = self.types.fresh(VarKind::General);
                let inner = self.types.intern(TyData::Opaque(OpaqueKind::Mutex(elem)));
                let desc = self.types.intern(TyData::Desc(inner));
                let guard = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::MutexGuard(elem)));
                (vec![desc], guard)
            }
            IntrinsicOp::MutexValue => {
                let elem = self.types.fresh(VarKind::General);
                let guard = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::MutexGuard(elem)));
                let guard_ref = self.types.intern(TyData::Ref {
                    region: None,
                    mutable: true,
                    pointee: guard,
                });
                let val_ref = self.types.intern(TyData::Ref {
                    region: None,
                    mutable: true,
                    pointee: elem,
                });
                (vec![guard_ref], val_ref)
            }
            IntrinsicOp::TaskCreate => unreachable!("handled above"),
            IntrinsicOp::TaskValid => {
                let res = self.types.fresh(VarKind::General);
                let inner = self.types.intern(TyData::Opaque(OpaqueKind::Task {
                    result: res,
                    context: None,
                }));
                let desc = self.types.intern(TyData::Desc(inner));
                (vec![desc], bool_ty)
            }
            IntrinsicOp::TaskYield => {
                if self.defer_depth > 0 {
                    self.error(
                        diag::LOOP_CONTROL,
                        span,
                        "cannot suspend with `task_yield` inside a `defer` block",
                    );
                }
                (Vec::new(), unit_ty)
            }
            IntrinsicOp::TaskPoll => {
                let res = self.types.fresh(VarKind::General);
                let inner = self.types.intern(TyData::Opaque(OpaqueKind::Task {
                    result: res,
                    context: None,
                }));
                let managed = self.types.intern(TyData::Managed(inner));
                let task_ref = self.types.intern(TyData::Ref {
                    region: None,
                    mutable: true,
                    pointee: managed,
                });
                let ok = self.types.intern(TyData::Adt {
                    adt: AdtRef(2),
                    args: vec![res],
                    regions: Vec::new(),
                });
                let err = self.types.intern(TyData::Adt {
                    adt: AdtRef(4),
                    args: Vec::new(),
                    regions: Vec::new(),
                });
                (
                    vec![task_ref],
                    self.types.intern(TyData::ErrorUnion { ok, err }),
                )
            }
            IntrinsicOp::TaskRequestCancel => {
                let res = self.types.fresh(VarKind::General);
                let inner = self.types.intern(TyData::Opaque(OpaqueKind::Task {
                    result: res,
                    context: None,
                }));
                let managed = self.types.intern(TyData::Managed(inner));
                let task_ref = self.types.intern(TyData::Ref {
                    region: None,
                    mutable: true,
                    pointee: managed,
                });
                (vec![task_ref], unit_ty)
            }
            IntrinsicOp::TaskPollClose => {
                let res = self.types.fresh(VarKind::General);
                let inner = self.types.intern(TyData::Opaque(OpaqueKind::Task {
                    result: res,
                    context: None,
                }));
                let managed = self.types.intern(TyData::Managed(inner));
                let task_ref = self.types.intern(TyData::Ref {
                    region: None,
                    mutable: true,
                    pointee: managed,
                });
                let poll_unit = self.types.intern(TyData::Adt {
                    adt: AdtRef(2),
                    args: vec![unit_ty],
                    regions: Vec::new(),
                });
                (vec![task_ref], poll_unit)
            }
            IntrinsicOp::TaskFinishDestroy => {
                let res = self.types.fresh(VarKind::General);
                let inner = self.types.intern(TyData::Opaque(OpaqueKind::Task {
                    result: res,
                    context: None,
                }));
                let managed = self.types.intern(TyData::Managed(inner));
                (vec![managed], unit_ty)
            }
            IntrinsicOp::AsyncRegistrationStart => {
                let env = self.types.fresh(VarKind::General);
                let reg = self.types.fresh(VarKind::General);
                let req = self.types.fresh(VarKind::General);
                let drain = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::AsyncRegistration(env)));
                let managed = self.types.intern(TyData::Managed(inner));
                (vec![reg, req, drain, env], managed)
            }
            IntrinsicOp::AsyncRegistrationValid => {
                let env = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::AsyncRegistration(env)));
                let desc = self.types.intern(TyData::Desc(inner));
                (vec![desc], bool_ty)
            }
            IntrinsicOp::AsyncRegistrationRequestClose => {
                let env = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::AsyncRegistration(env)));
                let managed = self.types.intern(TyData::Managed(inner));
                let reg_ref = self.types.intern(TyData::Ref {
                    region: None,
                    mutable: true,
                    pointee: managed,
                });
                (vec![reg_ref], unit_ty)
            }
            IntrinsicOp::AsyncRegistrationPollClose => {
                let env = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::AsyncRegistration(env)));
                let managed = self.types.intern(TyData::Managed(inner));
                let reg_ref = self.types.intern(TyData::Ref {
                    region: None,
                    mutable: true,
                    pointee: managed,
                });
                let poll_unit = self.types.intern(TyData::Adt {
                    adt: AdtRef(2),
                    args: vec![unit_ty],
                    regions: Vec::new(),
                });
                (vec![reg_ref], poll_unit)
            }
            IntrinsicOp::AsyncRegistrationFinishDestroy => {
                let env = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::AsyncRegistration(env)));
                let managed = self.types.intern(TyData::Managed(inner));
                (vec![managed], unit_ty)
            }
            IntrinsicOp::RegistrationStart => {
                let env = self.types.fresh(VarKind::General);
                let reg = self.types.fresh(VarKind::General);
                let close = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::Registration(env)));
                let managed = self.types.intern(TyData::Managed(inner));
                (vec![reg, close, env], managed)
            }
            IntrinsicOp::RegistrationValid => {
                let env = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::Registration(env)));
                let desc = self.types.intern(TyData::Desc(inner));
                (vec![desc], bool_ty)
            }
            IntrinsicOp::RegistrationClose => {
                let env = self.types.fresh(VarKind::General);
                let inner = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::Registration(env)));
                let managed = self.types.intern(TyData::Managed(inner));
                (vec![managed], unit_ty)
            }
            IntrinsicOp::MutexUnlock => {
                let elem = self.types.fresh(VarKind::General);
                let guard = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::MutexGuard(elem)));
                (vec![guard], unit_ty)
            }
            IntrinsicOp::SyncLoad
            | IntrinsicOp::SyncStore
            | IntrinsicOp::SyncFetchAdd
            | IntrinsicOp::SyncCompareExchange
            | IntrinsicOp::SyncLoadOrdered(_)
            | IntrinsicOp::SyncStoreOrdered(_)
            | IntrinsicOp::SyncFetchAddOrdered(_)
            | IntrinsicOp::SyncExchangeOrdered(_)
            | IntrinsicOp::SyncCompareExchangeOrdered { .. } => {
                let is_ordered = matches!(
                    op,
                    IntrinsicOp::SyncLoadOrdered(_)
                        | IntrinsicOp::SyncStoreOrdered(_)
                        | IntrinsicOp::SyncFetchAddOrdered(_)
                        | IntrinsicOp::SyncExchangeOrdered(_)
                        | IntrinsicOp::SyncCompareExchangeOrdered { .. }
                );
                let is_cas = matches!(
                    op,
                    IntrinsicOp::SyncCompareExchange
                        | IntrinsicOp::SyncCompareExchangeOrdered { .. }
                );
                let is_load = matches!(op, IntrinsicOp::SyncLoad | IntrinsicOp::SyncLoadOrdered(_));
                let is_store = matches!(
                    op,
                    IntrinsicOp::SyncStore | IntrinsicOp::SyncStoreOrdered(_)
                );
                let value_arg_count = if is_load {
                    1
                } else if is_cas {
                    3
                } else {
                    2
                };
                let order_arg_count = if !is_ordered {
                    0
                } else if is_cas {
                    2
                } else {
                    1
                };
                let expected_arity = value_arg_count + order_arg_count;
                if arguments.len() != expected_arity {
                    self.error(
                        diag::CALL_ARITY,
                        span,
                        format!(
                            "`{name}` expects {expected_arity} argument(s), found {}",
                            arguments.len()
                        ),
                    );
                    return Some(self.err_expr(span));
                }
                let parse_order = |expr: &ast::Expr| -> Option<zil_ir::body::MemoryOrder> {
                    use zil_ir::body::MemoryOrder;
                    let ast::ExprKind::Member {
                        object,
                        member: ast::MemberName::Name(s),
                    } = &expr.kind
                    else {
                        return None;
                    };
                    if !matches!(&object.kind, ast::ExprKind::Name(n) if n == "MemoryOrder") {
                        return None;
                    }
                    match s.as_str() {
                        "Relaxed" => Some(MemoryOrder::Relaxed),
                        "Acquire" => Some(MemoryOrder::Acquire),
                        "Release" => Some(MemoryOrder::Release),
                        "AcqRel" => Some(MemoryOrder::AcqRel),
                        "SeqCst" => Some(MemoryOrder::SeqCst),
                        _ => None,
                    }
                };
                let final_op = if is_ordered {
                    let Some(ord1) = parse_order(&arguments[value_arg_count].value) else {
                        self.error(
                            diag::OPERATOR_TYPE,
                            arguments[value_arg_count].span,
                            format!(
                                "`{name}` requires a compile-time `MemoryOrder` variant (Relaxed, Acquire, Release, AcqRel, or SeqCst)"
                            ),
                        );
                        return Some(self.err_expr(span));
                    };
                    if matches!(op, IntrinsicOp::SyncLoadOrdered(_)) {
                        if !ord1.is_valid_load() {
                            self.error(
                                diag::OPERATOR_TYPE,
                                arguments[value_arg_count].span,
                                "`atomic_load_ordered` does not allow `Release` or `AcqRel`",
                            );
                            return Some(self.err_expr(span));
                        }
                        IntrinsicOp::SyncLoadOrdered(ord1)
                    } else if matches!(op, IntrinsicOp::SyncStoreOrdered(_)) {
                        if !ord1.is_valid_store() {
                            self.error(
                                diag::OPERATOR_TYPE,
                                arguments[value_arg_count].span,
                                "`atomic_store_ordered` does not allow `Acquire` or `AcqRel`",
                            );
                            return Some(self.err_expr(span));
                        }
                        IntrinsicOp::SyncStoreOrdered(ord1)
                    } else if matches!(op, IntrinsicOp::SyncFetchAddOrdered(_)) {
                        IntrinsicOp::SyncFetchAddOrdered(ord1)
                    } else if matches!(op, IntrinsicOp::SyncExchangeOrdered(_)) {
                        IntrinsicOp::SyncExchangeOrdered(ord1)
                    } else if matches!(op, IntrinsicOp::SyncCompareExchangeOrdered { .. }) {
                        let Some(ord2) = parse_order(&arguments[value_arg_count + 1].value) else {
                            self.error(
                                diag::OPERATOR_TYPE,
                                arguments[value_arg_count + 1].span,
                                "`atomic_compare_exchange_ordered` requires a compile-time failure `MemoryOrder` variant",
                            );
                            return Some(self.err_expr(span));
                        };
                        if !zil_ir::body::MemoryOrder::is_valid_cas(ord1, ord2) {
                            self.error(
                                diag::OPERATOR_TYPE,
                                arguments[value_arg_count + 1].span,
                                "`atomic_compare_exchange_ordered` failure ordering cannot be `Release`/`AcqRel` or stronger than success ordering",
                            );
                            return Some(self.err_expr(span));
                        }
                        IntrinsicOp::SyncCompareExchangeOrdered {
                            success: ord1,
                            failure: ord2,
                        }
                    } else {
                        op
                    }
                } else {
                    op
                };
                let cell_expr = self.infer(&arguments[0].value);
                let cell_ty = self.types.shallow(cell_expr.ty);
                let cell_kind = if let TyData::Ref {
                    mutable: false,
                    pointee,
                    ..
                } = self.types.data(cell_ty).clone()
                    && let TyData::Opaque(OpaqueKind::SyncCell(k)) =
                        self.types.data(self.types.shallow(pointee))
                {
                    Some(*k)
                } else {
                    None
                };
                let Some(cell_kind) = cell_kind else {
                    self.error(
                        diag::TYPE_MISMATCH,
                        arguments[0].span,
                        format!(
                            "`{name}` first argument must be `&AtomicI32`, `&AtomicI64`, or `&AtomicUsize`"
                        ),
                    );
                    return Some(self.err_expr(span));
                };
                let elem_int = match cell_kind {
                    zil_ir::body::SyncCellKind::I32 => IntTy::I32,
                    zil_ir::body::SyncCellKind::I64 => IntTy::I64,
                    zil_ir::body::SyncCellKind::Usize => IntTy::Usize,
                };
                let elem_ty = self.types.intern(TyData::Int(elem_int));
                let mut args = vec![cell_expr];
                for a in &arguments[1..value_arg_count] {
                    let v = self.infer(&a.value);
                    args.push(self.coerce_arg(v, elem_ty, a.span));
                }
                let ret_ty = if is_store {
                    unit_ty
                } else if is_cas {
                    self.types.intern(TyData::Tuple(vec![elem_ty, bool_ty]))
                } else {
                    elem_ty
                };
                return Some(Expr {
                    kind: ExprKind::Intrinsic { op: final_op, args },
                    ty: ret_ty,
                    span,
                });
            }
            IntrinsicOp::CNull => {
                if !arguments.is_empty() {
                    self.error(
                        diag::CALL_ARITY,
                        span,
                        format!("`c_null` expects 0 arguments, found {}", arguments.len()),
                    );
                    return Some(self.err_expr(span));
                }
                let handle_ty = self.types.intern(TyData::Opaque(OpaqueKind::ForeignHandle));
                return Some(Expr {
                    kind: ExprKind::Intrinsic {
                        op: IntrinsicOp::CNull,
                        args: Vec::new(),
                    },
                    ty: handle_ty,
                    span,
                });
            }
            IntrinsicOp::CCallback => {
                let [fn_arg] = arguments else {
                    self.error(
                        diag::CALL_ARITY,
                        span,
                        format!("`c_callback` expects 1 argument, found {}", arguments.len()),
                    );
                    return Some(self.err_expr(span));
                };
                let fn_expr = self.infer(&fn_arg.value);
                let fn_ty = self.normalize(fn_expr.ty);
                let TyData::FnPtr { params, result } = self.types.data(fn_ty).clone() else {
                    self.error(
                        diag::FFI_DECLARATION,
                        fn_arg.span,
                        "`c_callback` requires a non-capturing function name, not a closure or value",
                    );
                    return Some(self.err_expr(span));
                };
                let is_c_scalar = |this: &mut Self, t: Ty| {
                    let nt = this.normalize(t);
                    matches!(
                        this.types.data(nt),
                        TyData::Int(_)
                            | TyData::Float(_)
                            | TyData::Bool
                            | TyData::Opaque(OpaqueKind::ForeignHandle)
                    )
                };
                let mut ok = true;
                for &p in &params {
                    if !is_c_scalar(self, p) {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    let res_nt = self.normalize(result);
                    ok = is_c_scalar(self, result)
                        || matches!(self.types.data(res_nt), TyData::Unit);
                }
                if !ok {
                    self.error(
                        diag::FFI_DECLARATION,
                        fn_arg.span,
                        "`c_callback` target function must use C-ABI scalar parameters and scalar or unit return type",
                    );
                    return Some(self.err_expr(span));
                }
                let cb_ty = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::ForeignCallback {
                        params,
                        result,
                    }));
                return Some(Expr {
                    kind: ExprKind::Intrinsic {
                        op: IntrinsicOp::CCallback,
                        args: vec![fn_expr],
                    },
                    ty: cb_ty,
                    span,
                });
            }
            IntrinsicOp::ScopedCallback => {
                let [reg_arg, close_arg, f_arg, body_arg] = arguments else {
                    self.error(
                        diag::CALL_ARITY,
                        span,
                        "`c_with_callback` expects 4 arguments (register, close, &mut callback, body)",
                    );
                    return Some(self.err_expr(span));
                };
                let reg_expr = self.infer(&reg_arg.value);
                let close_expr = self.infer(&close_arg.value);
                let f_expr = self.infer(&f_arg.value);
                let body_expr = self.infer(&body_arg.value);

                let check_role = |this: &mut Self,
                                  e: &Expr,
                                  arg_span: Span,
                                  want: zil_ir::body::ForeignCallbackRole,
                                  label: &str| {
                    if let ExprKind::ForeignRef(fref) = e.kind {
                        if this.items.foreign(fref).callback_role != Some(want) {
                            this.error(
                                diag::FFI_DECLARATION,
                                arg_span,
                                format!(
                                    "`c_with_callback` {label} must declare `provides(scoped_callback_{label}())`"
                                ),
                            );
                        }
                    } else {
                        this.error(
                            diag::FFI_DECLARATION,
                            arg_span,
                            format!(
                                "`c_with_callback` {label} must be a direct `extern \"C\"` function with `scoped_callback_{label}()` role"
                            ),
                        );
                    }
                };
                check_role(
                    self,
                    &reg_expr,
                    reg_arg.span,
                    zil_ir::body::ForeignCallbackRole::ScopedRegister,
                    "register",
                );
                check_role(
                    self,
                    &close_expr,
                    close_arg.span,
                    zil_ir::body::ForeignCallbackRole::ScopedClose,
                    "close",
                );

                let i32_ty = self.types.intern(TyData::Int(IntTy::I32));
                let handle_ty = self.types.intern(TyData::Opaque(OpaqueKind::ForeignHandle));
                let cb_ty = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::ForeignCallback {
                        params: vec![handle_ty, i32_ty],
                        result: i64_ty,
                    }));
                let reg_expected = self
                    .types
                    .intern(TyData::Opaque(OpaqueKind::ForeignCallback {
                        params: vec![cb_ty, handle_ty],
                        result: handle_ty,
                    }));
                let close_expected =
                    self.types
                        .intern(TyData::Opaque(OpaqueKind::ForeignCallback {
                            params: vec![handle_ty],
                            result: unit_ty,
                        }));
                let reg_expr = self.coerce_arg(reg_expr, reg_expected, reg_arg.span);
                let close_expr = self.coerce_arg(close_expr, close_expected, close_arg.span);

                let f_ref_ty = self.types.shallow(f_expr.ty);
                if let TyData::Ref {
                    mutable: true,
                    pointee,
                    ..
                } = self.types.data(f_ref_ty).clone()
                {
                    let f_ty = self.types.shallow(pointee);
                    let mut f_ok = false;
                    let mut f_is_once = false;
                    if let TyData::Closure {
                        kind,
                        params,
                        result,
                        ..
                    } = self.types.data(f_ty).clone()
                    {
                        if kind == ClosureKind::FnOnce {
                            f_is_once = true;
                        } else if params.len() == 1 {
                            self.unify(params[0], i32_ty, f_arg.span);
                            self.unify(result, i32_ty, f_arg.span);
                            f_ok = true;
                        }
                    } else if let TyData::ImplCallable {
                        kind,
                        params,
                        result,
                        ..
                    } = self.types.data(f_ty).clone()
                    {
                        if kind == ClosureKind::FnOnce {
                            f_is_once = true;
                        } else if params.len() == 1 {
                            self.unify(params[0], i32_ty, f_arg.span);
                            self.unify(result, i32_ty, f_arg.span);
                            f_ok = true;
                        }
                    } else if let TyData::Param(idx) = self.types.data(f_ty) {
                        if let Some((kind, params, result)) = self
                            .decl
                            .generics
                            .get(*idx as usize)
                            .and_then(|g| g.callable.clone())
                        {
                            if kind == ClosureKind::FnOnce {
                                f_is_once = true;
                            } else if params.len() == 1 {
                                self.unify(params[0], i32_ty, f_arg.span);
                                self.unify(result, i32_ty, f_arg.span);
                                f_ok = true;
                            }
                        }
                    } else if let TyData::FnPtr { params, result } = self.types.data(f_ty).clone()
                        && params.len() == 1
                    {
                        self.unify(params[0], i32_ty, f_arg.span);
                        self.unify(result, i32_ty, f_arg.span);
                        f_ok = true;
                    }
                    if f_is_once {
                        self.error(
                            diag::TYPE_MISMATCH,
                            f_arg.span,
                            "`c_with_callback` callback must be `Fn` or `FnMut`, not `FnOnce`",
                        );
                    } else if !f_ok {
                        self.error(
                            diag::TYPE_MISMATCH,
                            f_arg.span,
                            "`c_with_callback` callback must have signature `(i32) -> i32`",
                        );
                    }
                } else {
                    self.error(
                        diag::TYPE_MISMATCH,
                        f_arg.span,
                        "`c_with_callback` third argument must be `&mut F`",
                    );
                }

                let body_ty = self.types.shallow(body_expr.ty);
                let mut body_ok = false;
                if let TyData::Closure { params, result, .. }
                | TyData::ImplCallable { params, result, .. }
                | TyData::FnPtr { params, result } = self.types.data(body_ty).clone()
                {
                    if params.is_empty() {
                        self.unify(result, i32_ty, body_arg.span);
                        body_ok = true;
                    }
                } else if let TyData::Param(idx) = self.types.data(body_ty)
                    && let Some((_, params, result)) = self
                        .decl
                        .generics
                        .get(*idx as usize)
                        .and_then(|g| g.callable.clone())
                    && params.is_empty()
                {
                    self.unify(result, i32_ty, body_arg.span);
                    body_ok = true;
                }
                if !body_ok {
                    self.error(
                        diag::TYPE_MISMATCH,
                        body_arg.span,
                        "`c_with_callback` body must be a 0-argument callable returning `i32`",
                    );
                }

                let ret_ty = self.types.intern(TyData::Tuple(vec![bool_ty, i32_ty]));
                return Some(Expr {
                    kind: ExprKind::Intrinsic {
                        op: IntrinsicOp::ScopedCallback,
                        args: vec![reg_expr, close_expr, f_expr, body_expr],
                    },
                    ty: ret_ty,
                    span,
                });
            }
            IntrinsicOp::TaskStart
            | IntrinsicOp::ManagedCreate
            | IntrinsicOp::OwnedDynCreate
            | IntrinsicOp::ThreadScope
            | IntrinsicOp::SyncInit => return None,
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

    fn resolve_explicit_type_args(
        &mut self,
        explicit: Option<&[ast::GenericArg]>,
        expected_n: usize,
        span: Span,
    ) -> Option<Vec<Ty>> {
        match explicit {
            None => Some(
                (0..expected_n)
                    .map(|_| self.types.fresh(VarKind::General))
                    .collect(),
            ),
            Some(given) => {
                let mut out = Vec::new();
                for g in given {
                    match g {
                        ast::GenericArg::Type(t) => out.push(self.resolve_written(t)),
                        ast::GenericArg::Lifetime(_) => {}
                        ast::GenericArg::Const(expr) => {
                            out.push(self.resolve_const_generic_arg(expr));
                        }
                        ast::GenericArg::AssociatedBinding(_) => {
                            self.unsupported("non-type generic arguments", span);
                            return None;
                        }
                    }
                }
                if out.is_empty() {
                    Some(
                        (0..expected_n)
                            .map(|_| self.types.fresh(VarKind::General))
                            .collect(),
                    )
                } else if out.len() != expected_n {
                    self.error(diag::TYPE_ARGUMENT, span, "wrong number of type arguments");
                    None
                } else {
                    Some(out)
                }
            }
        }
    }

    fn direct_call_with_args(
        &mut self,
        f: FnRef,
        type_args: Vec<Ty>,
        receiver: Option<Expr>,
        arguments: &[ast::Argument],
        span: Span,
    ) -> Expr {
        let decl = self.items.func(f);
        let param_tys: Vec<Ty> = decl.params.iter().map(|p| p.ty).collect();
        let result = decl.result;
        let rec_n = usize::from(receiver.is_some());
        if param_tys.len() != arguments.len() + rec_n {
            self.error(
                diag::CALL_ARITY,
                span,
                format!(
                    "`{}` takes {} arguments, {} given",
                    decl.name,
                    param_tys.len().saturating_sub(rec_n),
                    arguments.len()
                ),
            );
            return self.err_expr(span);
        }
        let mut args = Vec::with_capacity(param_tys.len());
        let mut param_iter = param_tys.iter();
        if let Some(rec) = receiver {
            let p0 = *param_iter.next().expect("receiver param");
            let expected = self.types.subst(p0, &type_args);
            let expected = self.types.erase_regions(expected);
            let rec_span = rec.span;
            args.push(self.coerce_arg(rec, expected, rec_span));
        }
        for (p, a) in param_iter.zip(arguments) {
            let expected = self.types.subst(*p, &type_args);
            let expected = self.types.erase_regions(expected);
            let value = self.infer_expected(&a.value, expected);
            let value = self.coerce_arg(value, expected, a.span);
            args.push(value);
        }
        let generics = self.items.func(f).generics.clone();
        for (g, a) in generics.iter().zip(type_args.iter()) {
            if let Some((want_k, wp, wr)) = &g.callable {
                let resolved = self.normalize(*a);
                let td = self.types.data(resolved).clone();
                if let TyData::Closure {
                    kind: got_k,
                    params: gp,
                    result: gr,
                    ..
                } = td
                {
                    let kind_ok = matches!(
                        (got_k, want_k),
                        (ClosureKind::Fn, _)
                            | (ClosureKind::FnMut, ClosureKind::FnMut | ClosureKind::FnOnce)
                            | (ClosureKind::FnOnce, ClosureKind::FnOnce)
                    );
                    if !kind_ok || gp.len() != wp.len() {
                        self.error(
                            diag::GENERIC_BOUND,
                            span,
                            format!(
                                "type argument for `{}` does not satisfy callable bound",
                                g.name
                            ),
                        );
                    } else {
                        for (gp_ty, wp_ty) in gp.iter().zip(wp.iter()) {
                            let wp_sub = self.types.subst(*wp_ty, &type_args);
                            let wp_sub = self.types.erase_regions(wp_sub);
                            let wp_sub = self.normalize(wp_sub);
                            self.unify(*gp_ty, wp_sub, span);
                        }
                        let wr_sub = self.types.subst(*wr, &type_args);
                        let wr_sub = self.types.erase_regions(wr_sub);
                        let wr_sub = self.normalize(wr_sub);
                        self.unify(gr, wr_sub, span);
                    }
                }
            }
        }
        let ty = self.types.subst(result, &type_args);
        let ty = self.types.erase_regions(ty);
        let ty = self.normalize(ty);
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

    fn direct_call(
        &mut self,
        f: FnRef,
        explicit: Option<&[ast::GenericArg]>,
        arguments: &[ast::Argument],
        span: Span,
    ) -> Expr {
        let n = self.items.func(f).generics.len();
        let Some(type_args) = self.resolve_explicit_type_args(explicit, n, span) else {
            return self.err_expr(span);
        };
        self.direct_call_with_args(f, type_args, None, arguments, span)
    }

    fn ty_contains_ref(&mut self, ty: Ty) -> bool {
        fn walk(types: &mut Types, ty: Ty, depth: u32) -> bool {
            if depth > 32 {
                return false;
            }
            let t = types.shallow(ty);
            match types.data(t).clone() {
                TyData::Ref { .. } => true,
                TyData::Tuple(xs) | TyData::Union(xs) => {
                    xs.into_iter().any(|x| walk(types, x, depth + 1))
                }
                TyData::Array { element: elem, .. }
                | TyData::ArrayConst { element: elem, .. }
                | TyData::Slice(elem)
                | TyData::Managed(elem)
                | TyData::Desc(elem) => walk(types, elem, depth + 1),
                TyData::ErrorUnion { ok, err } => {
                    walk(types, ok, depth + 1) || walk(types, err, depth + 1)
                }
                TyData::Adt { args, .. } => args.into_iter().any(|a| walk(types, a, depth + 1)),
                TyData::Opaque(OpaqueKind::Task {
                    result,
                    context: Some(ctx),
                }) => walk(types, result, depth + 1) || walk(types, ctx, depth + 1),
                TyData::Opaque(OpaqueKind::Task {
                    result,
                    context: None,
                }) => walk(types, result, depth + 1),
                TyData::Opaque(
                    OpaqueKind::OwnedStore(e)
                    | OpaqueKind::Mutex(e)
                    | OpaqueKind::MutexGuard(e)
                    | OpaqueKind::Registration(e)
                    | OpaqueKind::AsyncRegistration(e),
                ) => walk(types, e, depth + 1),
                TyData::Unit
                | TyData::Type
                | TyData::ConstParam(_)
                | TyData::ConstU32(_)
                | TyData::Bool
                | TyData::Int(_)
                | TyData::Float(_)
                | TyData::Never
                | TyData::Param(_)
                | TyData::Var(_)
                | TyData::FnPtr { .. }
                | TyData::Closure { .. }
                | TyData::Dyn { .. } => false,
                TyData::OwnedDyn { context, .. } => {
                    context.is_some_and(|c| walk(types, c, depth + 1))
                }
                TyData::Opaque(
                    OpaqueKind::ByteBuffer
                    | OpaqueKind::ForeignHandle
                    | OpaqueKind::ForeignCallback { .. }
                    | OpaqueKind::SyncCell(_),
                )
                | TyData::ImplCallable { .. }
                | TyData::SelfParam
                | TyData::Assoc { .. }
                | TyData::Error => false,
            }
        }
        walk(self.types, ty, 0)
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
                Stmt::Expr(e) | Stmt::Defer(e) => walk(e),
            }),
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => walk(cond) || walk(then) || otherwise.as_deref().is_some_and(walk),
            ExprKind::Match { scrutinee, arms } => {
                walk(scrutinee)
                    || arms
                        .iter()
                        .any(|a| a.guard.as_deref().is_some_and(walk) || walk(&a.body))
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
            | ExprKind::UnionInject { value: inner }
            | ExprKind::Try(inner)
            | ExprKind::CtEval(inner) => walk(inner),
            ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::Local(_)
            | ExprKind::Capture(_)
            | ExprKind::Continue
            | ExprKind::Trap
            | ExprKind::Field { .. }
            | ExprKind::TupleField { .. }
            | ExprKind::Index { .. }
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Logical { .. }
            | ExprKind::Convert { .. }
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::Call { .. }
            | ExprKind::IndirectCall { .. }
            | ExprKind::Closure { .. }
            | ExprKind::ClosureFnRef(_)
            | ExprKind::ClosureCall { .. }
            | ExprKind::TraitCall { .. }
            | ExprKind::DynCall { .. }
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
    explicit_direct: bool,
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
        if !expr_contains_suspend(&f.body) {
            for &p in &f.params {
                let l = &f.locals[p.0 as usize];
                if let TyData::Param(i) = self.data(l.ty)
                    && let Some(g) = self.generics.get(i as usize)
                    && !g.copy
                    && !g.sync_drop
                    && !g.store_element
                    && !local_is_consumed(&f.body, p)
                {
                    let mut d = diag::error(
                        "E-ORIGINAL-CLEANUP",
                        l.span,
                        "live original value has no independent SyncDrop proof",
                    );
                    d.notes
                        .push(format!("original scope cleanup binding \"{}\"", l.name));
                    self.diags.push(d);
                }
            }
            for (li, l) in f.locals.iter().enumerate() {
                let lid = LocalId(li as u32);
                if f.params.contains(&lid) {
                    continue;
                }
                if let TyData::Param(i) = self.data(l.ty)
                    && let Some(g) = self.generics.get(i as usize)
                    && !g.copy
                    && !g.sync_drop
                    && !g.store_element
                    && !local_is_consumed(&f.body, lid)
                {
                    let mut d = diag::error(
                        "E-ORIGINAL-CLEANUP",
                        l.span,
                        "live original value has no independent SyncDrop proof",
                    );
                    d.notes
                        .push(format!("original scope cleanup binding \"{}\"", l.name));
                    self.diags.push(d);
                }
            }
        }
        for c in &f.closures {
            if expr_contains_suspend(&c.body) {
                continue;
            }
            for (ci, &cap_ty) in c.capture_tys.iter().enumerate() {
                if let TyData::Param(i) = self.data(cap_ty)
                    && let Some(g) = self.generics.get(i as usize)
                    && !g.copy
                    && !g.sync_drop
                    && !g.store_element
                    && !capture_is_consumed(&c.body, ci as u32)
                {
                    self.diags.push(diag::error(
                        diag::GENERIC_BOUND,
                        c.body.span,
                        format!(
                            "dropping unconsumed capture of generic type `{}` requires `Copy` or `SyncDrop`",
                            g.name
                        ),
                    ));
                }
            }
        }
        if !expr_contains_suspend(&f.body) {
            let mut reported: std::collections::HashSet<LocalId> = std::collections::HashSet::new();
            if self.explicit_direct {
                for (li, l) in f.locals.iter().enumerate() {
                    let lid = LocalId(li as u32);
                    if self.may_suspend_on_drop(l.ty) && !local_is_consumed(&f.body, lid) {
                        reported.insert(lid);
                        self.diags.push(diag::error(
                            diag::GENERIC_BOUND,
                            l.span,
                            "default Drop cleanup may suspend and lacks a Closed fact or a Suspend execution root",
                        ));
                    }
                }
            }
            let mut tainted: std::collections::HashSet<LocalId> = std::collections::HashSet::new();
            for &p in &f.params {
                if self.ty_owns_task_or_async_reg(f.locals[p.0 as usize].ty, 0) {
                    tainted.insert(p);
                }
            }
            if !tainted.is_empty() {
                loop {
                    let before = tainted.len();
                    propagate_tainted_locals(&f.body, &mut tainted, |lid| {
                        self.may_suspend_on_drop(f.locals[lid.0 as usize].ty)
                    });
                    if tainted.len() == before {
                        break;
                    }
                }
                for (li, l) in f.locals.iter().enumerate() {
                    let lid = LocalId(li as u32);
                    if tainted.contains(&lid)
                        && !reported.contains(&lid)
                        && !local_is_consumed(&f.body, lid)
                        && !local_is_poll_closed(&f.body, lid)
                        && !local_is_called(&f.body, lid)
                    {
                        self.diags.push(diag::error(
                            diag::GENERIC_BOUND,
                            l.span,
                            "default Drop cleanup may suspend and lacks a Closed fact or a Suspend execution root",
                        ));
                    }
                }
            }
        }
    }

    fn ty_owns_task_or_async_reg(&self, ty: Ty, depth: u32) -> bool {
        if depth > 16 {
            return false;
        }
        match self.data(ty) {
            TyData::Managed(inner) => {
                if matches!(
                    self.data(inner),
                    TyData::Opaque(OpaqueKind::Task { .. } | OpaqueKind::AsyncRegistration(_))
                ) {
                    true
                } else {
                    self.ty_owns_task_or_async_reg(inner, depth + 1)
                }
            }
            TyData::Tuple(xs) | TyData::Union(xs) => xs
                .iter()
                .any(|&x| self.ty_owns_task_or_async_reg(x, depth + 1)),
            TyData::Array { element, .. } | TyData::ArrayConst { element, .. } => {
                self.ty_owns_task_or_async_reg(element, depth + 1)
            }
            TyData::ErrorUnion { ok, err } => {
                self.ty_owns_task_or_async_reg(ok, depth + 1)
                    || self.ty_owns_task_or_async_reg(err, depth + 1)
            }
            TyData::Adt { adt, args, .. } => {
                let decl = self.items.adt(adt);
                decl.variants.iter().any(|v| {
                    v.fields.iter().any(|fld| {
                        let fty = if let TyData::Param(idx) = self.data(fld.ty) {
                            args.get(idx as usize).copied().unwrap_or(fld.ty)
                        } else {
                            fld.ty
                        };
                        self.ty_owns_task_or_async_reg(fty, depth + 1)
                    })
                })
            }
            TyData::Unit
            | TyData::Type
            | TyData::ConstParam(_)
            | TyData::ConstU32(_)
            | TyData::Bool
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Never
            | TyData::Param(_)
            | TyData::Var(_)
            | TyData::Slice(_)
            | TyData::Ref { .. }
            | TyData::FnPtr { .. }
            | TyData::Closure { .. }
            | TyData::ImplCallable { .. }
            | TyData::Dyn { .. }
            | TyData::OwnedDyn { .. }
            | TyData::Desc(_)
            | TyData::Opaque(_)
            | TyData::SelfParam
            | TyData::Assoc { .. }
            | TyData::Error => false,
        }
    }

    fn may_suspend_on_drop(&self, ty: Ty) -> bool {
        match self.data(ty) {
            TyData::Closure { .. } | TyData::ImplCallable { .. } => true,
            TyData::Managed(_)
            | TyData::Tuple(_)
            | TyData::Array { .. }
            | TyData::ArrayConst { .. }
            | TyData::ErrorUnion { .. }
            | TyData::Union(_)
            | TyData::Adt { .. } => self.ty_owns_task_or_async_reg(ty, 0),
            TyData::Unit
            | TyData::Type
            | TyData::ConstParam(_)
            | TyData::ConstU32(_)
            | TyData::Bool
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Never
            | TyData::Param(_)
            | TyData::Var(_)
            | TyData::Slice(_)
            | TyData::Ref { .. }
            | TyData::FnPtr { .. }
            | TyData::Dyn { .. }
            | TyData::OwnedDyn { .. }
            | TyData::Desc(_)
            | TyData::Opaque(_)
            | TyData::SelfParam
            | TyData::Assoc { .. }
            | TyData::Error => false,
        }
    }

    fn data(&self, ty: Ty) -> TyData {
        self.types.data(ty).clone()
    }

    fn is_copy(&mut self, ty: Ty) -> bool {
        self.items.is_copy_in_env(ty, self.generics, self.types)
    }

    fn check_generic_bounds(
        &mut self,
        generics: &[crate::items::GenericInfo],
        type_args: &[Ty],
        subst_args: &[Ty],
        span: Span,
    ) {
        for (g, a) in generics.iter().zip(type_args.iter()) {
            if g.store_element && matches!(self.data(*a), TyData::Ref { .. } | TyData::Desc(_)) {
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
            for bound in &g.traits {
                let inst_bound = crate::items::TraitBound {
                    trait_ref: bound.trait_ref,
                    trait_args: bound
                        .trait_args
                        .iter()
                        .map(|t| {
                            let s = self.types.subst(*t, subst_args);
                            self.items.normalize_ty_in_env(s, self.generics, self.types)
                        })
                        .collect(),
                    assoc_bindings: bound
                        .assoc_bindings
                        .iter()
                        .map(|(n, t)| {
                            let s = self.types.subst(*t, subst_args);
                            (
                                n.clone(),
                                self.items.normalize_ty_in_env(s, self.generics, self.types),
                            )
                        })
                        .collect(),
                };
                if !self
                    .items
                    .satisfies_trait_bound(*a, &inst_bound, self.generics, self.types)
                {
                    let tname = &self.items.trait_decl(bound.trait_ref).name;
                    self.diags.push(diag::error(
                        diag::GENERIC_BOUND,
                        span,
                        format!("type argument for `{}` does not satisfy `{tname}`", g.name),
                    ));
                }
            }
        }
    }

    fn pat(&mut self, p: &mut Pat) {
        p.ty = self.zonk(p.ty, p.span);
        match &mut p.kind {
            PatKind::Wild | PatKind::Binding(_) | PatKind::RefBinding { .. } => {}
            PatKind::Deref(inner) => self.pat(inner),
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
            PatKind::UnionArm { member_ty, inner } => {
                *member_ty = self.zonk(*member_ty, p.span);
                self.pat(inner);
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
            | (Lit::Int(_), TyData::Type)
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
                TyData::Int(_)
                    | TyData::Float(_)
                    | TyData::Bool
                    | TyData::Unit
                    | TyData::Type
                    | TyData::Opaque(OpaqueKind::ForeignHandle)
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
            ExprKind::StaticUtf(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::Local(_)
            | ExprKind::Capture(_)
            | ExprKind::ForeignRef(_)
            | ExprKind::ClosureFnRef(_)
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Trap => {}
            ExprKind::FnRef { func, type_args } => {
                for a in type_args.iter_mut() {
                    *a = self.zonk(*a, span);
                }
                let generics = self.items.func(*func).generics.clone();
                self.check_generic_bounds(&generics, type_args, type_args, span);
            }
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
            | ExprKind::UnionInject { value: inner }
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
                self.check_generic_bounds(&generics, type_args, type_args, span);
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
                trait_ref,
                trait_args,
                method_args,
                self_ty,
                method,
                args,
            } => {
                for a in trait_args.iter_mut() {
                    *a = self.zonk(*a, span);
                }
                for a in method_args.iter_mut() {
                    *a = self.zonk(*a, span);
                }
                *self_ty = self.zonk(*self_ty, span);
                if !method_args.is_empty()
                    && let Some(tm_generics) = self
                        .items
                        .trait_decl(*trait_ref)
                        .methods
                        .iter()
                        .find(|m| m.name == *method)
                        .map(|m| m.generics.clone())
                {
                    let mut full_subst = trait_args.clone();
                    full_subst.extend(method_args.iter().copied());
                    self.check_generic_bounds(&tm_generics, method_args, &full_subst, span);
                }
                args.iter_mut().for_each(|a| self.expr(a));
            }
            ExprKind::DynCall { object, args, .. } => {
                self.expr(object);
                args.iter_mut().for_each(|a| self.expr(a));
            }
            ExprKind::ForeignCall { args, .. } => {
                args.iter_mut().for_each(|a| self.expr(a));
            }
            ExprKind::Intrinsic { op, args } => {
                args.iter_mut().for_each(|a| self.expr(a));
                if *op == IntrinsicOp::AsyncRegistrationStart
                    && let Some(env_arg) = args.get(3)
                    && let TyData::Param(i) = self.data(env_arg.ty)
                    && let Some(g) = self.generics.get(i as usize)
                    && !g.copy
                    && !g.sync_drop
                {
                    self.diags.push(diag::error(
                        diag::GENERIC_BOUND,
                        span,
                        format!(
                            "`c_register_async` requires `SyncDrop` on generic callback type `{}`",
                            g.name
                        ),
                    ));
                }
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
                    if let Some(g) = &mut a.guard {
                        self.expr(g);
                    }
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
                        Stmt::Expr(e) | Stmt::Defer(e) => self.expr(e),
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
                } else if let TyData::Param(i) = self.data(place.ty)
                    && let Some(g) = self.generics.get(i as usize)
                    && !g.copy
                    && !g.sync_drop
                    && !matches!(g.callable, Some((ClosureKind::FnOnce, _, _)))
                {
                    self.diags.push(diag::error(
                        diag::GENERIC_BOUND,
                        span,
                        format!(
                            "overwriting a value of generic type `{}` requires `Copy` or `SyncDrop`",
                            g.name
                        ),
                    ));
                }
            }
        }
    }
}

fn expr_contains_suspend(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Intrinsic {
            op: IntrinsicOp::TaskYield,
            ..
        } => true,
        ExprKind::Lit(_)
        | ExprKind::ConstGeneric(_)
        | ExprKind::StaticUtf(_)
        | ExprKind::Local(_)
        | ExprKind::Capture(_)
        | ExprKind::FnRef { .. }
        | ExprKind::ForeignRef(_)
        | ExprKind::ClosureFnRef(_)
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Trap => false,
        ExprKind::Field { base, .. }
        | ExprKind::TupleField { base, .. }
        | ExprKind::Deref(base)
        | ExprKind::Give(base)
        | ExprKind::Borrow { place: base, .. }
        | ExprKind::Describe(base)
        | ExprKind::Unary { operand: base, .. }
        | ExprKind::Convert { operand: base }
        | ExprKind::Len(base)
        | ExprKind::Try(base)
        | ExprKind::ErrorInject { value: base, .. }
        | ExprKind::UnionInject { value: base }
        | ExprKind::CtEval(base)
        | ExprKind::Loop(base) => expr_contains_suspend(base),
        ExprKind::SliceBorrow {
            place, start, end, ..
        } => {
            expr_contains_suspend(place)
                || start.as_deref().is_some_and(expr_contains_suspend)
                || end.as_deref().is_some_and(expr_contains_suspend)
        }
        ExprKind::Index { base, index }
        | ExprKind::Binary {
            lhs: base,
            rhs: index,
            ..
        }
        | ExprKind::Logical {
            lhs: base,
            rhs: index,
            ..
        }
        | ExprKind::Assign {
            place: base,
            value: index,
            ..
        } => expr_contains_suspend(base) || expr_contains_suspend(index),
        ExprKind::Tuple(xs)
        | ExprKind::Array(xs)
        | ExprKind::Call { args: xs, .. }
        | ExprKind::TraitCall { args: xs, .. }
        | ExprKind::ForeignCall { args: xs, .. }
        | ExprKind::Intrinsic { args: xs, .. }
        | ExprKind::ManagedCreate { fields: xs, .. }
        | ExprKind::Adt { fields: xs, .. }
        | ExprKind::Closure { captures: xs, .. } => xs.iter().any(expr_contains_suspend),
        ExprKind::IndirectCall { callee, args }
        | ExprKind::ClosureCall {
            closure: callee,
            args,
            ..
        }
        | ExprKind::DynCall {
            object: callee,
            args,
            ..
        } => expr_contains_suspend(callee) || args.iter().any(expr_contains_suspend),
        ExprKind::Return(v) => v.as_deref().is_some_and(expr_contains_suspend),
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => {
            expr_contains_suspend(cond)
                || expr_contains_suspend(then)
                || otherwise.as_deref().is_some_and(expr_contains_suspend)
        }
        ExprKind::While { cond, body } => {
            expr_contains_suspend(cond) || expr_contains_suspend(body)
        }
        ExprKind::Match { scrutinee, arms } => {
            expr_contains_suspend(scrutinee)
                || arms.iter().any(|a| {
                    a.guard.as_deref().is_some_and(expr_contains_suspend)
                        || expr_contains_suspend(&a.body)
                })
        }
        ExprKind::Block(stmts) => stmts.iter().any(|s| match s {
            Stmt::Let { init, .. } => expr_contains_suspend(init),
            Stmt::Expr(x) | Stmt::Defer(x) => expr_contains_suspend(x),
        }),
    }
}

fn unwrap_local_ref(e: &Expr) -> Option<LocalId> {
    match &e.kind {
        ExprKind::Local(l) => Some(*l),
        ExprKind::Give(inner) => unwrap_local_ref(inner),
        ExprKind::Lit(_)
        | ExprKind::ConstGeneric(_)
        | ExprKind::StaticUtf(_)
        | ExprKind::Capture(_)
        | ExprKind::Field { .. }
        | ExprKind::TupleField { .. }
        | ExprKind::Index { .. }
        | ExprKind::Deref(_)
        | ExprKind::Borrow { .. }
        | ExprKind::SliceBorrow { .. }
        | ExprKind::Len(_)
        | ExprKind::Describe(_)
        | ExprKind::ManagedCreate { .. }
        | ExprKind::Unary { .. }
        | ExprKind::Binary { .. }
        | ExprKind::Logical { .. }
        | ExprKind::Convert { .. }
        | ExprKind::FnRef { .. }
        | ExprKind::ForeignRef(_)
        | ExprKind::ClosureFnRef(_)
        | ExprKind::Call { .. }
        | ExprKind::IndirectCall { .. }
        | ExprKind::Closure { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::TraitCall { .. }
        | ExprKind::DynCall { .. }
        | ExprKind::ForeignCall { .. }
        | ExprKind::Intrinsic { .. }
        | ExprKind::Adt { .. }
        | ExprKind::ErrorInject { .. }
        | ExprKind::UnionInject { .. }
        | ExprKind::Try(_)
        | ExprKind::CtEval(_)
        | ExprKind::Tuple(_)
        | ExprKind::Array(_)
        | ExprKind::If { .. }
        | ExprKind::Match { .. }
        | ExprKind::Block(_)
        | ExprKind::Loop(_)
        | ExprKind::While { .. }
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Trap
        | ExprKind::Return(_)
        | ExprKind::Assign { .. } => None,
    }
}

fn local_is_consumed(e: &Expr, root: LocalId) -> bool {
    value_is_consumed(e, Some(root), None)
}

fn capture_is_consumed(e: &Expr, cap: u32) -> bool {
    value_is_consumed(e, None, Some(cap))
}

fn unwrap_local_or_borrow_ref(e: &Expr) -> Option<LocalId> {
    match &e.kind {
        ExprKind::Local(l) => Some(*l),
        ExprKind::Give(inner) | ExprKind::Borrow { place: inner, .. } => {
            unwrap_local_or_borrow_ref(inner)
        }
        ExprKind::Lit(_)
        | ExprKind::ConstGeneric(_)
        | ExprKind::StaticUtf(_)
        | ExprKind::Capture(_)
        | ExprKind::Field { .. }
        | ExprKind::TupleField { .. }
        | ExprKind::Index { .. }
        | ExprKind::Deref(_)
        | ExprKind::SliceBorrow { .. }
        | ExprKind::Len(_)
        | ExprKind::Describe(_)
        | ExprKind::ManagedCreate { .. }
        | ExprKind::Unary { .. }
        | ExprKind::Binary { .. }
        | ExprKind::Logical { .. }
        | ExprKind::Convert { .. }
        | ExprKind::FnRef { .. }
        | ExprKind::ForeignRef(_)
        | ExprKind::ClosureFnRef(_)
        | ExprKind::Call { .. }
        | ExprKind::IndirectCall { .. }
        | ExprKind::Closure { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::TraitCall { .. }
        | ExprKind::DynCall { .. }
        | ExprKind::ForeignCall { .. }
        | ExprKind::Intrinsic { .. }
        | ExprKind::Adt { .. }
        | ExprKind::ErrorInject { .. }
        | ExprKind::UnionInject { .. }
        | ExprKind::Try(_)
        | ExprKind::CtEval(_)
        | ExprKind::Tuple(_)
        | ExprKind::Array(_)
        | ExprKind::If { .. }
        | ExprKind::Match { .. }
        | ExprKind::Block(_)
        | ExprKind::Loop(_)
        | ExprKind::While { .. }
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Trap
        | ExprKind::Return(_)
        | ExprKind::Assign { .. } => None,
    }
}

fn collect_pat_bindings(pat: &Pat, out: &mut Vec<LocalId>) {
    match &pat.kind {
        PatKind::Binding(l) => out.push(*l),
        PatKind::Tuple(ps) => {
            for p in ps {
                collect_pat_bindings(p, out);
            }
        }
        PatKind::Adt { fields, .. } => {
            for p in fields {
                collect_pat_bindings(p, out);
            }
        }
        PatKind::ErrorArm { inner, .. }
        | PatKind::UnionArm { inner, .. }
        | PatKind::Deref(inner) => collect_pat_bindings(inner, out),
        PatKind::Wild | PatKind::RefBinding { .. } | PatKind::Lit(_) => {}
    }
}

fn expr_any(e: &Expr, pred: &mut impl FnMut(&Expr) -> bool) -> bool {
    if pred(e) {
        return true;
    }
    match &e.kind {
        ExprKind::Lit(_)
        | ExprKind::ConstGeneric(_)
        | ExprKind::StaticUtf(_)
        | ExprKind::Local(_)
        | ExprKind::Capture(_)
        | ExprKind::FnRef { .. }
        | ExprKind::ForeignRef(_)
        | ExprKind::ClosureFnRef(_)
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Trap => false,
        ExprKind::Field { base, .. }
        | ExprKind::TupleField { base, .. }
        | ExprKind::Deref(base)
        | ExprKind::Give(base)
        | ExprKind::Borrow { place: base, .. }
        | ExprKind::Describe(base)
        | ExprKind::Unary { operand: base, .. }
        | ExprKind::Convert { operand: base }
        | ExprKind::Len(base)
        | ExprKind::Try(base)
        | ExprKind::ErrorInject { value: base, .. }
        | ExprKind::UnionInject { value: base }
        | ExprKind::CtEval(base)
        | ExprKind::Loop(base) => expr_any(base, pred),
        ExprKind::SliceBorrow {
            place, start, end, ..
        } => {
            expr_any(place, pred)
                || start.as_deref().is_some_and(|x| expr_any(x, pred))
                || end.as_deref().is_some_and(|x| expr_any(x, pred))
        }
        ExprKind::Index { base, index }
        | ExprKind::Binary {
            lhs: base,
            rhs: index,
            ..
        }
        | ExprKind::Logical {
            lhs: base,
            rhs: index,
            ..
        }
        | ExprKind::While {
            cond: base,
            body: index,
        }
        | ExprKind::Assign {
            place: base,
            value: index,
            ..
        } => expr_any(base, pred) || expr_any(index, pred),
        ExprKind::Return(v) => v.as_deref().is_some_and(|x| expr_any(x, pred)),
        ExprKind::ManagedCreate { fields, .. }
        | ExprKind::Adt { fields, .. }
        | ExprKind::Tuple(fields)
        | ExprKind::Array(fields)
        | ExprKind::Closure {
            captures: fields, ..
        }
        | ExprKind::Call { args: fields, .. }
        | ExprKind::ForeignCall { args: fields, .. }
        | ExprKind::Intrinsic { args: fields, .. }
        | ExprKind::TraitCall { args: fields, .. } => fields.iter().any(|x| expr_any(x, pred)),
        ExprKind::IndirectCall { callee, args }
        | ExprKind::ClosureCall {
            closure: callee,
            args,
            ..
        }
        | ExprKind::DynCall {
            object: callee,
            args,
            ..
        } => expr_any(callee, pred) || args.iter().any(|x| expr_any(x, pred)),
        ExprKind::If {
            cond,
            then,
            otherwise,
        } => {
            expr_any(cond, pred)
                || expr_any(then, pred)
                || otherwise.as_deref().is_some_and(|x| expr_any(x, pred))
        }
        ExprKind::Match { scrutinee, arms } => {
            expr_any(scrutinee, pred)
                || arms.iter().any(|a| {
                    a.guard.as_deref().is_some_and(|g| expr_any(g, pred)) || expr_any(&a.body, pred)
                })
        }
        ExprKind::Block(stmts) => stmts.iter().any(|s| match s {
            Stmt::Let { init, .. } => expr_any(init, pred),
            Stmt::Expr(x) | Stmt::Defer(x) => expr_any(x, pred),
        }),
    }
}

fn expr_mentions_any(e: &Expr, set: &std::collections::HashSet<LocalId>) -> bool {
    expr_any(
        e,
        &mut |sub| matches!(sub.kind, ExprKind::Local(l) if set.contains(&l)),
    )
}

fn propagate_tainted_locals(
    e: &Expr,
    tainted: &mut std::collections::HashSet<LocalId>,
    mut may_suspend: impl FnMut(LocalId) -> bool,
) {
    fn walk(
        e: &Expr,
        tainted: &mut std::collections::HashSet<LocalId>,
        may_suspend: &mut impl FnMut(LocalId) -> bool,
    ) {
        match &e.kind {
            ExprKind::Block(stmts) => {
                for s in stmts {
                    match s {
                        Stmt::Let { pattern, init } => {
                            walk(init, tainted, may_suspend);
                            if expr_mentions_any(init, tainted) {
                                let mut bound = Vec::new();
                                collect_pat_bindings(pattern, &mut bound);
                                for b in bound {
                                    if may_suspend(b) {
                                        tainted.insert(b);
                                    }
                                }
                            }
                        }
                        Stmt::Expr(x) | Stmt::Defer(x) => walk(x, tainted, may_suspend),
                    }
                }
            }
            ExprKind::Assign { place, value, .. } => {
                walk(value, tainted, may_suspend);
                if let Some(dst) = unwrap_local_ref(place)
                    && expr_mentions_any(value, tainted)
                    && may_suspend(dst)
                {
                    tainted.insert(dst);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                walk(scrutinee, tainted, may_suspend);
                let scrut_tainted = expr_mentions_any(scrutinee, tainted);
                for arm in arms {
                    if scrut_tainted {
                        let mut bound = Vec::new();
                        collect_pat_bindings(&arm.pattern, &mut bound);
                        for b in bound {
                            if may_suspend(b) {
                                tainted.insert(b);
                            }
                        }
                    }
                    walk(&arm.body, tainted, may_suspend);
                }
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                walk(cond, tainted, may_suspend);
                walk(then, tainted, may_suspend);
                if let Some(eb) = otherwise {
                    walk(eb, tainted, may_suspend);
                }
            }
            ExprKind::Loop(body) => walk(body, tainted, may_suspend),
            ExprKind::While { cond, body } => {
                walk(cond, tainted, may_suspend);
                walk(body, tainted, may_suspend);
            }
            ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::Local(_)
            | ExprKind::Capture(_)
            | ExprKind::Field { .. }
            | ExprKind::TupleField { .. }
            | ExprKind::Index { .. }
            | ExprKind::Deref(_)
            | ExprKind::Give(_)
            | ExprKind::Borrow { .. }
            | ExprKind::SliceBorrow { .. }
            | ExprKind::Len(_)
            | ExprKind::Describe(_)
            | ExprKind::ManagedCreate { .. }
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Logical { .. }
            | ExprKind::Convert { .. }
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::ClosureFnRef(_)
            | ExprKind::Call { .. }
            | ExprKind::IndirectCall { .. }
            | ExprKind::Closure { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::TraitCall { .. }
            | ExprKind::DynCall { .. }
            | ExprKind::ForeignCall { .. }
            | ExprKind::Intrinsic { .. }
            | ExprKind::Adt { .. }
            | ExprKind::ErrorInject { .. }
            | ExprKind::UnionInject { .. }
            | ExprKind::Try(_)
            | ExprKind::CtEval(_)
            | ExprKind::Tuple(_)
            | ExprKind::Array(_)
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Trap
            | ExprKind::Return(_) => {}
        }
    }
    walk(e, tainted, &mut may_suspend);
}

fn local_is_poll_closed(e: &Expr, target: LocalId) -> bool {
    expr_any(e, &mut |sub| {
        matches!(
            &sub.kind,
            ExprKind::Intrinsic {
                op: IntrinsicOp::TaskPollClose | IntrinsicOp::AsyncRegistrationPollClose,
                args,
            } if args
                .first()
                .and_then(unwrap_local_or_borrow_ref)
                == Some(target)
        )
    })
}

fn local_is_called(e: &Expr, target: LocalId) -> bool {
    expr_any(e, &mut |sub| match &sub.kind {
        ExprKind::ClosureCall {
            closure: callee, ..
        }
        | ExprKind::IndirectCall { callee, .. } => {
            unwrap_local_or_borrow_ref(callee) == Some(target)
        }
        ExprKind::TraitCall { args, .. } => {
            args.first().and_then(unwrap_local_or_borrow_ref) == Some(target)
        }
        ExprKind::DynCall { object, .. } => unwrap_local_or_borrow_ref(object) == Some(target),
        ExprKind::Lit(_)
        | ExprKind::ConstGeneric(_)
        | ExprKind::StaticUtf(_)
        | ExprKind::Local(_)
        | ExprKind::Capture(_)
        | ExprKind::Field { .. }
        | ExprKind::TupleField { .. }
        | ExprKind::Index { .. }
        | ExprKind::Deref(_)
        | ExprKind::Give(_)
        | ExprKind::Borrow { .. }
        | ExprKind::SliceBorrow { .. }
        | ExprKind::Len(_)
        | ExprKind::Describe(_)
        | ExprKind::ManagedCreate { .. }
        | ExprKind::Unary { .. }
        | ExprKind::Binary { .. }
        | ExprKind::Logical { .. }
        | ExprKind::Convert { .. }
        | ExprKind::FnRef { .. }
        | ExprKind::ForeignRef(_)
        | ExprKind::ClosureFnRef(_)
        | ExprKind::Call { .. }
        | ExprKind::Closure { .. }
        | ExprKind::ForeignCall { .. }
        | ExprKind::Intrinsic { .. }
        | ExprKind::Adt { .. }
        | ExprKind::ErrorInject { .. }
        | ExprKind::UnionInject { .. }
        | ExprKind::Try(_)
        | ExprKind::CtEval(_)
        | ExprKind::Tuple(_)
        | ExprKind::Array(_)
        | ExprKind::If { .. }
        | ExprKind::Match { .. }
        | ExprKind::Block(_)
        | ExprKind::Loop(_)
        | ExprKind::While { .. }
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Trap
        | ExprKind::Return(_)
        | ExprKind::Assign { .. } => false,
    })
}

fn unwrap_capture_ref(e: &Expr) -> Option<u32> {
    match &e.kind {
        ExprKind::Capture(c) => Some(*c),
        ExprKind::Give(inner) => unwrap_capture_ref(inner),
        ExprKind::Lit(_)
        | ExprKind::ConstGeneric(_)
        | ExprKind::StaticUtf(_)
        | ExprKind::Local(_)
        | ExprKind::Field { .. }
        | ExprKind::TupleField { .. }
        | ExprKind::Index { .. }
        | ExprKind::Deref(_)
        | ExprKind::Borrow { .. }
        | ExprKind::SliceBorrow { .. }
        | ExprKind::Len(_)
        | ExprKind::Describe(_)
        | ExprKind::ManagedCreate { .. }
        | ExprKind::Unary { .. }
        | ExprKind::Binary { .. }
        | ExprKind::Logical { .. }
        | ExprKind::Convert { .. }
        | ExprKind::FnRef { .. }
        | ExprKind::ForeignRef(_)
        | ExprKind::ClosureFnRef(_)
        | ExprKind::Call { .. }
        | ExprKind::IndirectCall { .. }
        | ExprKind::Closure { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::TraitCall { .. }
        | ExprKind::DynCall { .. }
        | ExprKind::ForeignCall { .. }
        | ExprKind::Intrinsic { .. }
        | ExprKind::Adt { .. }
        | ExprKind::ErrorInject { .. }
        | ExprKind::UnionInject { .. }
        | ExprKind::Try(_)
        | ExprKind::CtEval(_)
        | ExprKind::Tuple(_)
        | ExprKind::Array(_)
        | ExprKind::If { .. }
        | ExprKind::Match { .. }
        | ExprKind::Block(_)
        | ExprKind::Loop(_)
        | ExprKind::While { .. }
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Trap
        | ExprKind::Return(_)
        | ExprKind::Assign { .. } => None,
    }
}

fn value_is_consumed(e: &Expr, root_local: Option<LocalId>, root_cap: Option<u32>) -> bool {
    fn walk(e: &Expr, aliases: &mut Vec<LocalId>, root_cap: Option<u32>) -> bool {
        let is_alias = |x: &Expr, aliases: &[LocalId]| {
            unwrap_local_ref(x).is_some_and(|l| aliases.contains(&l))
                || (root_cap.is_some() && unwrap_capture_ref(x) == root_cap)
        };
        match &e.kind {
            ExprKind::Block(stmts) => {
                for s in stmts {
                    match s {
                        Stmt::Let { pattern, init } => {
                            if is_alias(init, aliases) {
                                if let PatKind::Binding(local) = pattern.kind {
                                    aliases.push(local);
                                }
                            } else if walk(init, aliases, root_cap) {
                                return true;
                            }
                        }
                        Stmt::Expr(x) | Stmt::Defer(x) => {
                            if walk(x, aliases, root_cap) {
                                return true;
                            }
                            if matches!(x.kind, ExprKind::Return(_)) {
                                return false;
                            }
                        }
                    }
                }
                false
            }
            ExprKind::ClosureCall {
                closure,
                mode,
                args,
            } => {
                if *mode == ClosureCallMode::Once && is_alias(closure, aliases) {
                    return true;
                }
                if args.iter().any(|a| is_alias(a, aliases)) {
                    return true;
                }
                walk(closure, aliases, root_cap) || args.iter().any(|a| walk(a, aliases, root_cap))
            }
            ExprKind::Call { args, .. }
            | ExprKind::TraitCall { args, .. }
            | ExprKind::ForeignCall { args, .. }
            | ExprKind::Intrinsic { args, .. }
            | ExprKind::Tuple(args)
            | ExprKind::Array(args)
            | ExprKind::ManagedCreate { fields: args, .. }
            | ExprKind::Adt { fields: args, .. }
            | ExprKind::Closure { captures: args, .. } => {
                args.iter().any(|a| is_alias(a, aliases))
                    || args.iter().any(|a| walk(a, aliases, root_cap))
            }
            ExprKind::IndirectCall { callee, args }
            | ExprKind::DynCall {
                object: callee,
                args,
                ..
            } => {
                args.iter().any(|a| is_alias(a, aliases))
                    || walk(callee, aliases, root_cap)
                    || args.iter().any(|a| walk(a, aliases, root_cap))
            }
            ExprKind::Return(Some(v)) => is_alias(v, aliases) || walk(v, aliases, root_cap),
            ExprKind::Return(None) => false,
            ExprKind::ErrorInject { value, .. } | ExprKind::UnionInject { value } => {
                is_alias(value, aliases) || walk(value, aliases, root_cap)
            }
            ExprKind::Borrow { .. } | ExprKind::SliceBorrow { .. } | ExprKind::Describe(_) => false,
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                walk(cond, aliases, root_cap)
                    || (walk(then, &mut aliases.clone(), root_cap)
                        && otherwise
                            .as_deref()
                            .is_some_and(|x| walk(x, &mut aliases.clone(), root_cap)))
            }
            ExprKind::While { cond, .. } => walk(cond, aliases, root_cap),
            ExprKind::Match { scrutinee, arms } => {
                walk(scrutinee, aliases, root_cap)
                    || (!arms.is_empty()
                        && arms
                            .iter()
                            .all(|a| walk(&a.body, &mut aliases.clone(), root_cap)))
            }
            ExprKind::Loop(inner)
            | ExprKind::CtEval(inner)
            | ExprKind::Try(inner)
            | ExprKind::Give(inner)
            | ExprKind::Deref(inner)
            | ExprKind::Len(inner)
            | ExprKind::Unary { operand: inner, .. }
            | ExprKind::Convert { operand: inner }
            | ExprKind::Field { base: inner, .. }
            | ExprKind::TupleField { base: inner, .. } => walk(inner, aliases, root_cap),
            ExprKind::Index { base, index }
            | ExprKind::Binary {
                lhs: base,
                rhs: index,
                ..
            }
            | ExprKind::Logical {
                lhs: base,
                rhs: index,
                ..
            } => walk(base, aliases, root_cap) || walk(index, aliases, root_cap),
            ExprKind::Assign { place, value, .. } => {
                is_alias(value, aliases)
                    || walk(place, aliases, root_cap)
                    || walk(value, aliases, root_cap)
            }
            ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::Local(_)
            | ExprKind::Capture(_)
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::ClosureFnRef(_)
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Trap => false,
        }
    }
    let mut aliases: Vec<LocalId> = root_local.into_iter().collect();
    walk(e, &mut aliases, root_cap)
}

/// Computes a transitive fixed-point `can_suspend` summary over all THIR
/// functions and closures, then rejects any `defer` block that directly or
/// transitively invokes a suspending function/closure or an unknown indirect
/// callable.
pub fn check_defer_suspend_effects(
    items: &Items<'_>,
    types: &Types,
    thir: &[Option<ThirFn>],
    diags: &mut Vec<Diagnostic>,
) {
    fn resolve_closure(types: &Types, mut ty: Ty) -> Option<(usize, usize)> {
        for _ in 0..4 {
            match types.data(ty) {
                TyData::Ref { pointee, .. } => ty = *pointee,
                TyData::Closure { owner, id, .. } => {
                    return Some((owner.0 as usize, id.0 as usize));
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
                | TyData::Union(_)
                | TyData::FnPtr { .. }
                | TyData::ImplCallable { .. }
                | TyData::Dyn { .. }
                | TyData::OwnedDyn { .. }
                | TyData::Param(_)
                | TyData::SelfParam
                | TyData::Assoc { .. }
                | TyData::ArrayConst { .. }
                | TyData::ConstParam(_)
                | TyData::ConstU32(_)
                | TyData::Type
                | TyData::Var(_)
                | TyData::Error => return None,
            }
        }
        None
    }

    fn expr_suspends(
        e: &Expr,
        items: &Items<'_>,
        types: &Types,
        fn_susp: &[bool],
        cl_susp: &[Vec<bool>],
    ) -> bool {
        match &e.kind {
            ExprKind::Intrinsic { op, args } => {
                *op == IntrinsicOp::TaskYield
                    || args
                        .iter()
                        .any(|a| expr_suspends(a, items, types, fn_susp, cl_susp))
            }
            ExprKind::Call { callee, args, .. } => {
                fn_susp.get(callee.0 as usize).copied().unwrap_or(false)
                    || args
                        .iter()
                        .any(|a| expr_suspends(a, items, types, fn_susp, cl_susp))
            }
            ExprKind::TraitCall {
                trait_ref,
                method,
                args,
                ..
            } => {
                let trait_may_suspend = items.trait_impls.iter().any(|imp| {
                    imp.trait_ref == *trait_ref
                        && imp.methods.iter().any(|(name, fn_ref)| {
                            name == method
                                && fn_susp.get(fn_ref.0 as usize).copied().unwrap_or(false)
                        })
                });
                trait_may_suspend
                    || args
                        .iter()
                        .any(|a| expr_suspends(a, items, types, fn_susp, cl_susp))
            }
            ExprKind::ClosureCall { closure, args, .. } => {
                let target_susp = match resolve_closure(types, closure.ty) {
                    Some((o, i)) => cl_susp
                        .get(o)
                        .and_then(|v| v.get(i))
                        .copied()
                        .unwrap_or(true),
                    None => false,
                };
                target_susp
                    || expr_suspends(closure, items, types, fn_susp, cl_susp)
                    || args
                        .iter()
                        .any(|a| expr_suspends(a, items, types, fn_susp, cl_susp))
            }
            ExprKind::IndirectCall { callee, args } => {
                expr_suspends(callee, items, types, fn_susp, cl_susp)
                    || args
                        .iter()
                        .any(|a| expr_suspends(a, items, types, fn_susp, cl_susp))
            }
            ExprKind::DynCall { object, args, .. } => {
                expr_suspends(object, items, types, fn_susp, cl_susp)
                    || args
                        .iter()
                        .any(|a| expr_suspends(a, items, types, fn_susp, cl_susp))
            }
            ExprKind::ForeignCall { args, .. }
            | ExprKind::ManagedCreate { fields: args, .. }
            | ExprKind::Adt { fields: args, .. }
            | ExprKind::Tuple(args)
            | ExprKind::Array(args)
            | ExprKind::Closure { captures: args, .. } => args
                .iter()
                .any(|a| expr_suspends(a, items, types, fn_susp, cl_susp)),
            ExprKind::Field { base: x, .. }
            | ExprKind::TupleField { base: x, .. }
            | ExprKind::Deref(x)
            | ExprKind::Borrow { place: x, .. }
            | ExprKind::Len(x)
            | ExprKind::Give(x)
            | ExprKind::Describe(x)
            | ExprKind::Unary { operand: x, .. }
            | ExprKind::Convert { operand: x }
            | ExprKind::ErrorInject { value: x, .. }
            | ExprKind::UnionInject { value: x }
            | ExprKind::Try(x)
            | ExprKind::CtEval(x)
            | ExprKind::Loop(x) => expr_suspends(x, items, types, fn_susp, cl_susp),
            ExprKind::SliceBorrow {
                place, start, end, ..
            } => {
                expr_suspends(place, items, types, fn_susp, cl_susp)
                    || start
                        .as_deref()
                        .is_some_and(|s| expr_suspends(s, items, types, fn_susp, cl_susp))
                    || end
                        .as_deref()
                        .is_some_and(|s| expr_suspends(s, items, types, fn_susp, cl_susp))
            }
            ExprKind::Index { base: a, index: b }
            | ExprKind::Binary { lhs: a, rhs: b, .. }
            | ExprKind::Logical { lhs: a, rhs: b, .. }
            | ExprKind::While { cond: a, body: b }
            | ExprKind::Assign {
                place: a, value: b, ..
            } => {
                expr_suspends(a, items, types, fn_susp, cl_susp)
                    || expr_suspends(b, items, types, fn_susp, cl_susp)
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                expr_suspends(cond, items, types, fn_susp, cl_susp)
                    || expr_suspends(then, items, types, fn_susp, cl_susp)
                    || otherwise
                        .as_deref()
                        .is_some_and(|o| expr_suspends(o, items, types, fn_susp, cl_susp))
            }
            ExprKind::Match { scrutinee, arms } => {
                expr_suspends(scrutinee, items, types, fn_susp, cl_susp)
                    || arms.iter().any(|arm| {
                        arm.guard
                            .as_deref()
                            .is_some_and(|g| expr_suspends(g, items, types, fn_susp, cl_susp))
                            || expr_suspends(&arm.body, items, types, fn_susp, cl_susp)
                    })
            }
            ExprKind::Block(stmts) => stmts.iter().any(|s| match s {
                Stmt::Let { init, .. } => expr_suspends(init, items, types, fn_susp, cl_susp),
                Stmt::Expr(x) | Stmt::Defer(x) => expr_suspends(x, items, types, fn_susp, cl_susp),
            }),
            ExprKind::Return(opt) => opt
                .as_deref()
                .is_some_and(|x| expr_suspends(x, items, types, fn_susp, cl_susp)),
            ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::Local(_)
            | ExprKind::Capture(_)
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::ClosureFnRef(_)
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Trap => false,
        }
    }

    fn check_defer_expr(
        e: &Expr,
        items: &Items<'_>,
        types: &Types,
        fn_susp: &[bool],
        cl_susp: &[Vec<bool>],
        diags: &mut Vec<Diagnostic>,
    ) {
        match &e.kind {
            ExprKind::Call { callee, args, .. } => {
                if fn_susp.get(callee.0 as usize).copied().unwrap_or(false) {
                    diags.push(Diagnostic {
                        phase: zil_base::Phase::Types,
                        category: zil_base::ResultCategory::SafeReject,
                        code: diag::LOOP_CONTROL.to_string(),
                        message: "cannot call suspending function inside a `defer` block".into(),
                        span: Some(e.span),
                        notes: Vec::new(),
                        related: Vec::new(),
                    });
                }
                for a in args {
                    check_defer_expr(a, items, types, fn_susp, cl_susp, diags);
                }
            }
            ExprKind::TraitCall {
                trait_ref,
                method,
                args,
                ..
            } => {
                let trait_may_suspend = items.trait_impls.iter().any(|imp| {
                    imp.trait_ref == *trait_ref
                        && imp.methods.iter().any(|(name, fn_ref)| {
                            name == method
                                && fn_susp.get(fn_ref.0 as usize).copied().unwrap_or(false)
                        })
                });
                if trait_may_suspend {
                    diags.push(Diagnostic {
                        phase: zil_base::Phase::Types,
                        category: zil_base::ResultCategory::SafeReject,
                        code: diag::LOOP_CONTROL.to_string(),
                        message: "cannot call suspending trait method inside a `defer` block"
                            .into(),
                        span: Some(e.span),
                        notes: Vec::new(),
                        related: Vec::new(),
                    });
                }
                for a in args {
                    check_defer_expr(a, items, types, fn_susp, cl_susp, diags);
                }
            }
            ExprKind::ClosureCall { closure, args, .. } => {
                let unsafe_target = match resolve_closure(types, closure.ty) {
                    Some((o, i)) => cl_susp
                        .get(o)
                        .and_then(|v| v.get(i))
                        .copied()
                        .unwrap_or(true),
                    None => true,
                };
                if unsafe_target {
                    diags.push(Diagnostic {
                        phase: zil_base::Phase::Types,
                        category: zil_base::ResultCategory::SafeReject,
                        code: diag::LOOP_CONTROL.to_string(),
                        message: "cannot call suspending or unknown closure inside a `defer` block"
                            .into(),
                        span: Some(e.span),
                        notes: Vec::new(),
                        related: Vec::new(),
                    });
                }
                check_defer_expr(closure, items, types, fn_susp, cl_susp, diags);
                for a in args {
                    check_defer_expr(a, items, types, fn_susp, cl_susp, diags);
                }
            }
            ExprKind::IndirectCall { callee, args } => {
                diags.push(Diagnostic {
                    phase: zil_base::Phase::Types,
                    category: zil_base::ResultCategory::SafeReject,
                    code: diag::LOOP_CONTROL.to_string(),
                    message: "cannot perform unknown indirect call inside a `defer` block".into(),
                    span: Some(e.span),
                    notes: Vec::new(),
                    related: Vec::new(),
                });
                check_defer_expr(callee, items, types, fn_susp, cl_susp, diags);
                for a in args {
                    check_defer_expr(a, items, types, fn_susp, cl_susp, diags);
                }
            }
            ExprKind::DynCall { object, args, .. } => {
                check_defer_expr(object, items, types, fn_susp, cl_susp, diags);
                for a in args {
                    check_defer_expr(a, items, types, fn_susp, cl_susp, diags);
                }
            }
            ExprKind::Intrinsic { args, .. }
            | ExprKind::ForeignCall { args, .. }
            | ExprKind::ManagedCreate { fields: args, .. }
            | ExprKind::Adt { fields: args, .. }
            | ExprKind::Tuple(args)
            | ExprKind::Array(args)
            | ExprKind::Closure { captures: args, .. } => {
                for a in args {
                    check_defer_expr(a, items, types, fn_susp, cl_susp, diags);
                }
            }
            ExprKind::Field { base: x, .. }
            | ExprKind::TupleField { base: x, .. }
            | ExprKind::Deref(x)
            | ExprKind::Borrow { place: x, .. }
            | ExprKind::Len(x)
            | ExprKind::Give(x)
            | ExprKind::Describe(x)
            | ExprKind::Unary { operand: x, .. }
            | ExprKind::Convert { operand: x }
            | ExprKind::ErrorInject { value: x, .. }
            | ExprKind::UnionInject { value: x }
            | ExprKind::Try(x)
            | ExprKind::CtEval(x)
            | ExprKind::Loop(x) => {
                check_defer_expr(x, items, types, fn_susp, cl_susp, diags);
            }
            ExprKind::SliceBorrow {
                place, start, end, ..
            } => {
                check_defer_expr(place, items, types, fn_susp, cl_susp, diags);
                if let Some(s) = start {
                    check_defer_expr(s, items, types, fn_susp, cl_susp, diags);
                }
                if let Some(s) = end {
                    check_defer_expr(s, items, types, fn_susp, cl_susp, diags);
                }
            }
            ExprKind::Index { base: a, index: b }
            | ExprKind::Binary { lhs: a, rhs: b, .. }
            | ExprKind::Logical { lhs: a, rhs: b, .. }
            | ExprKind::While { cond: a, body: b }
            | ExprKind::Assign {
                place: a, value: b, ..
            } => {
                check_defer_expr(a, items, types, fn_susp, cl_susp, diags);
                check_defer_expr(b, items, types, fn_susp, cl_susp, diags);
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                check_defer_expr(cond, items, types, fn_susp, cl_susp, diags);
                check_defer_expr(then, items, types, fn_susp, cl_susp, diags);
                if let Some(o) = otherwise {
                    check_defer_expr(o, items, types, fn_susp, cl_susp, diags);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                check_defer_expr(scrutinee, items, types, fn_susp, cl_susp, diags);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        check_defer_expr(g, items, types, fn_susp, cl_susp, diags);
                    }
                    check_defer_expr(&arm.body, items, types, fn_susp, cl_susp, diags);
                }
            }
            ExprKind::Block(stmts) => {
                for s in stmts {
                    match s {
                        Stmt::Let { init, .. } => {
                            check_defer_expr(init, items, types, fn_susp, cl_susp, diags);
                        }
                        Stmt::Expr(x) | Stmt::Defer(x) => {
                            check_defer_expr(x, items, types, fn_susp, cl_susp, diags);
                        }
                    }
                }
            }
            ExprKind::Return(opt) => {
                if let Some(x) = opt {
                    check_defer_expr(x, items, types, fn_susp, cl_susp, diags);
                }
            }
            ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::Local(_)
            | ExprKind::Capture(_)
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::ClosureFnRef(_)
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Trap => {}
        }
    }

    fn walk_for_defers(
        e: &Expr,
        items: &Items<'_>,
        types: &Types,
        fn_susp: &[bool],
        cl_susp: &[Vec<bool>],
        diags: &mut Vec<Diagnostic>,
    ) {
        match &e.kind {
            ExprKind::Block(stmts) => {
                for s in stmts {
                    match s {
                        Stmt::Let { init, .. } => {
                            walk_for_defers(init, items, types, fn_susp, cl_susp, diags);
                        }
                        Stmt::Expr(x) => {
                            walk_for_defers(x, items, types, fn_susp, cl_susp, diags);
                        }
                        Stmt::Defer(x) => {
                            check_defer_expr(x, items, types, fn_susp, cl_susp, diags);
                        }
                    }
                }
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                walk_for_defers(cond, items, types, fn_susp, cl_susp, diags);
                walk_for_defers(then, items, types, fn_susp, cl_susp, diags);
                if let Some(o) = otherwise {
                    walk_for_defers(o, items, types, fn_susp, cl_susp, diags);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                walk_for_defers(scrutinee, items, types, fn_susp, cl_susp, diags);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        walk_for_defers(g, items, types, fn_susp, cl_susp, diags);
                    }
                    walk_for_defers(&arm.body, items, types, fn_susp, cl_susp, diags);
                }
            }
            ExprKind::Loop(x) => walk_for_defers(x, items, types, fn_susp, cl_susp, diags),
            ExprKind::While { cond, body } => {
                walk_for_defers(cond, items, types, fn_susp, cl_susp, diags);
                walk_for_defers(body, items, types, fn_susp, cl_susp, diags);
            }
            ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::Local(_)
            | ExprKind::Capture(_)
            | ExprKind::Field { .. }
            | ExprKind::TupleField { .. }
            | ExprKind::Index { .. }
            | ExprKind::Deref(_)
            | ExprKind::Borrow { .. }
            | ExprKind::SliceBorrow { .. }
            | ExprKind::Len(_)
            | ExprKind::Give(_)
            | ExprKind::Describe(_)
            | ExprKind::ManagedCreate { .. }
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Logical { .. }
            | ExprKind::Convert { .. }
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::Call { .. }
            | ExprKind::IndirectCall { .. }
            | ExprKind::Closure { .. }
            | ExprKind::ClosureFnRef(_)
            | ExprKind::ClosureCall { .. }
            | ExprKind::TraitCall { .. }
            | ExprKind::DynCall { .. }
            | ExprKind::ForeignCall { .. }
            | ExprKind::Intrinsic { .. }
            | ExprKind::Adt { .. }
            | ExprKind::ErrorInject { .. }
            | ExprKind::UnionInject { .. }
            | ExprKind::Try(_)
            | ExprKind::CtEval(_)
            | ExprKind::Tuple(_)
            | ExprKind::Array(_)
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Trap
            | ExprKind::Return(_)
            | ExprKind::Assign { .. } => {}
        }
    }

    let mut fn_susp = vec![false; thir.len()];
    let mut cl_susp: Vec<Vec<bool>> = thir
        .iter()
        .map(|f| vec![false; f.as_ref().map_or(0, |t| t.closures.len())])
        .collect();

    let mut changed = true;
    while changed {
        changed = false;
        for (i, tf_opt) in thir.iter().enumerate() {
            let Some(tf) = tf_opt else { continue };
            for (ci, cl) in tf.closures.iter().enumerate() {
                if !cl_susp[i][ci] && expr_suspends(&cl.body, items, types, &fn_susp, &cl_susp) {
                    cl_susp[i][ci] = true;
                    changed = true;
                }
            }
            if !fn_susp[i] && expr_suspends(&tf.body, items, types, &fn_susp, &cl_susp) {
                fn_susp[i] = true;
                changed = true;
            }
        }
    }

    for tf in thir.iter().flatten() {
        walk_for_defers(&tf.body, items, types, &fn_susp, &cl_susp, diags);
        for cl in &tf.closures {
            walk_for_defers(&cl.body, items, types, &fn_susp, &cl_susp, diags);
        }
    }
}

fn ty_is_non_copy(items: &Items, types: &mut crate::ty::Types, ty: Ty) -> bool {
    fn walk(items: &Items, types: &mut crate::ty::Types, ty: Ty, depth: u32) -> bool {
        if depth > 32 {
            return true;
        }
        let t = types.shallow(ty);
        match types.data(t).clone() {
            TyData::Managed(_)
            | TyData::OwnedDyn { .. }
            | TyData::Dyn { .. }
            | TyData::Slice(_) => true,
            TyData::Opaque(
                OpaqueKind::ByteBuffer
                | OpaqueKind::OwnedStore(_)
                | OpaqueKind::Mutex(_)
                | OpaqueKind::MutexGuard(_)
                | OpaqueKind::Task { .. }
                | OpaqueKind::Registration(_)
                | OpaqueKind::AsyncRegistration(_)
                | OpaqueKind::SyncCell(_),
            ) => true,
            TyData::Adt { adt, args, .. } => {
                let decl = items.adt(adt);
                match decl.kind {
                    AdtKind::Package | AdtKind::Resource => true,
                    AdtKind::Record | AdtKind::Enum => {
                        decl.user_drop
                            || decl.variants.iter().any(|v| {
                                v.fields.iter().any(|f| {
                                    let fty = types.subst(f.ty, &args);
                                    walk(items, types, fty, depth + 1)
                                })
                            })
                    }
                }
            }
            TyData::Tuple(xs) | TyData::Union(xs) => {
                xs.iter().any(|x| walk(items, types, *x, depth + 1))
            }
            TyData::Array { element, .. } => walk(items, types, element, depth + 1),
            TyData::ErrorUnion { ok, err } => {
                walk(items, types, ok, depth + 1) || walk(items, types, err, depth + 1)
            }
            TyData::Closure { kind, .. } | TyData::ImplCallable { kind, .. } => {
                kind != ClosureKind::Fn
            }
            TyData::Unit
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Ref { .. }
            | TyData::Desc(_)
            | TyData::FnPtr { .. }
            | TyData::Opaque(OpaqueKind::ForeignHandle | OpaqueKind::ForeignCallback { .. })
            | TyData::Param(_)
            | TyData::SelfParam
            | TyData::Assoc { .. }
            | TyData::ArrayConst { .. }
            | TyData::ConstParam(_)
            | TyData::ConstU32(_)
            | TyData::Type
            | TyData::Var(_)
            | TyData::Error => false,
        }
    }
    walk(items, types, ty, 0)
}

fn capture_place_field(expr: &Expr) -> Option<(u32, Option<u32>)> {
    let mut top_field: Option<u32> = None;
    let mut cur = expr;
    loop {
        match &cur.kind {
            ExprKind::Capture(idx) => return Some((*idx, top_field)),
            ExprKind::Field { base, field, .. } => {
                top_field = Some(*field);
                cur = base;
            }
            ExprKind::TupleField { base, index } => {
                top_field = Some(*index);
                cur = base;
            }
            ExprKind::Index { base, .. } => {
                top_field = None;
                cur = base;
            }
            ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::Local(_)
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::Adt { .. }
            | ExprKind::ErrorInject { .. }
            | ExprKind::UnionInject { .. }
            | ExprKind::ManagedCreate { .. }
            | ExprKind::Tuple(_)
            | ExprKind::Array(_)
            | ExprKind::Closure { .. }
            | ExprKind::ClosureFnRef(_)
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Logical { .. }
            | ExprKind::Convert { .. }
            | ExprKind::Borrow { .. }
            | ExprKind::SliceBorrow { .. }
            | ExprKind::Deref(_)
            | ExprKind::Describe(_)
            | ExprKind::Give(_)
            | ExprKind::Len(_)
            | ExprKind::Call { .. }
            | ExprKind::ForeignCall { .. }
            | ExprKind::IndirectCall { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::TraitCall { .. }
            | ExprKind::DynCall { .. }
            | ExprKind::Intrinsic { .. }
            | ExprKind::Try(_)
            | ExprKind::CtEval(_)
            | ExprKind::Block(_)
            | ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::Loop(_)
            | ExprKind::While { .. }
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Trap
            | ExprKind::Return(_)
            | ExprKind::Assign { .. } => return None,
        }
    }
}

/// Returns `Some((cap_idx, opt_field))` only when `expr` directly names a
/// capture (`cap`) or a direct single field of a capture (`cap.f` / `cap.0`).
/// Deeper projections (such as `cap.a.b` or `cap.arr[i]`) return `None`
/// because assigning to a sub-projection does not refill the enclosing field
/// or capture.
fn direct_capture_assign_target(expr: &Expr) -> Option<(u32, Option<u32>)> {
    match &expr.kind {
        ExprKind::Capture(idx) => Some((*idx, None)),
        ExprKind::Field { base, field, .. } => match base.kind {
            ExprKind::Capture(idx) => Some((idx, Some(*field))),
            ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::Local(_)
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::Adt { .. }
            | ExprKind::ErrorInject { .. }
            | ExprKind::UnionInject { .. }
            | ExprKind::ManagedCreate { .. }
            | ExprKind::Tuple(_)
            | ExprKind::Array(_)
            | ExprKind::Closure { .. }
            | ExprKind::ClosureFnRef(_)
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Logical { .. }
            | ExprKind::Convert { .. }
            | ExprKind::Field { .. }
            | ExprKind::TupleField { .. }
            | ExprKind::Index { .. }
            | ExprKind::Borrow { .. }
            | ExprKind::SliceBorrow { .. }
            | ExprKind::Deref(_)
            | ExprKind::Describe(_)
            | ExprKind::Give(_)
            | ExprKind::Len(_)
            | ExprKind::Call { .. }
            | ExprKind::ForeignCall { .. }
            | ExprKind::IndirectCall { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::TraitCall { .. }
            | ExprKind::DynCall { .. }
            | ExprKind::Intrinsic { .. }
            | ExprKind::Try(_)
            | ExprKind::CtEval(_)
            | ExprKind::Block(_)
            | ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::Loop(_)
            | ExprKind::While { .. }
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Trap
            | ExprKind::Return(_)
            | ExprKind::Assign { .. } => None,
        },
        ExprKind::TupleField { base, index } => match base.kind {
            ExprKind::Capture(idx) => Some((idx, Some(*index))),
            ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::Local(_)
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::Adt { .. }
            | ExprKind::ErrorInject { .. }
            | ExprKind::UnionInject { .. }
            | ExprKind::ManagedCreate { .. }
            | ExprKind::Tuple(_)
            | ExprKind::Array(_)
            | ExprKind::Closure { .. }
            | ExprKind::ClosureFnRef(_)
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Logical { .. }
            | ExprKind::Convert { .. }
            | ExprKind::Field { .. }
            | ExprKind::TupleField { .. }
            | ExprKind::Index { .. }
            | ExprKind::Borrow { .. }
            | ExprKind::SliceBorrow { .. }
            | ExprKind::Deref(_)
            | ExprKind::Describe(_)
            | ExprKind::Give(_)
            | ExprKind::Len(_)
            | ExprKind::Call { .. }
            | ExprKind::ForeignCall { .. }
            | ExprKind::IndirectCall { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::TraitCall { .. }
            | ExprKind::DynCall { .. }
            | ExprKind::Intrinsic { .. }
            | ExprKind::Try(_)
            | ExprKind::CtEval(_)
            | ExprKind::Block(_)
            | ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::Loop(_)
            | ExprKind::While { .. }
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Trap
            | ExprKind::Return(_)
            | ExprKind::Assign { .. } => None,
        },
        ExprKind::Lit(_)
        | ExprKind::ConstGeneric(_)
        | ExprKind::StaticUtf(_)
        | ExprKind::Local(_)
        | ExprKind::FnRef { .. }
        | ExprKind::ForeignRef(_)
        | ExprKind::Adt { .. }
        | ExprKind::ErrorInject { .. }
        | ExprKind::UnionInject { .. }
        | ExprKind::ManagedCreate { .. }
        | ExprKind::Tuple(_)
        | ExprKind::Array(_)
        | ExprKind::Closure { .. }
        | ExprKind::ClosureFnRef(_)
        | ExprKind::Unary { .. }
        | ExprKind::Binary { .. }
        | ExprKind::Logical { .. }
        | ExprKind::Convert { .. }
        | ExprKind::Index { .. }
        | ExprKind::Borrow { .. }
        | ExprKind::SliceBorrow { .. }
        | ExprKind::Deref(_)
        | ExprKind::Describe(_)
        | ExprKind::Give(_)
        | ExprKind::Len(_)
        | ExprKind::Call { .. }
        | ExprKind::ForeignCall { .. }
        | ExprKind::IndirectCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::TraitCall { .. }
        | ExprKind::DynCall { .. }
        | ExprKind::Intrinsic { .. }
        | ExprKind::Try(_)
        | ExprKind::CtEval(_)
        | ExprKind::Block(_)
        | ExprKind::If { .. }
        | ExprKind::Match { .. }
        | ExprKind::Loop(_)
        | ExprKind::While { .. }
        | ExprKind::Break
        | ExprKind::Continue
        | ExprKind::Trap
        | ExprKind::Return(_)
        | ExprKind::Assign { .. } => None,
    }
}

#[derive(Clone, Default)]
struct CaptureMoveState {
    /// Captures or top-level capture fields that may currently be uninitialized
    /// along at least one path reaching the current program point.
    uninit: BTreeSet<(u32, Option<u32>)>,
}

#[derive(Default)]
struct CaptureMoveResult {
    /// Whole captures moved directly in value position or via `give cap`.
    whole_moved: BTreeSet<u32>,
    /// Capture fields (or indexed capture places) ever moved via `give`.
    ever_given: BTreeSet<(u32, Option<u32>)>,
    /// Capture fields still uninitialized at an early exit (`return`, `?`,
    /// `break`, `continue`, or `defer`).
    escaped_uninit: BTreeSet<(u32, Option<u32>)>,
}

/// Returns `true` if matching `pat` against a place performs at least one
/// by-value binding of a non-`Copy` type (which `build.rs` lowers to
/// `Operand::Move`). `ref`/`ref mut`, `&pat`, literals, and wildcards do not
/// move ownership out of the scrutinee.
fn pat_moves_non_copy(items: &Items<'_>, types: &mut Types, pat: &Pat) -> bool {
    match &pat.kind {
        PatKind::Wild | PatKind::Lit(_) | PatKind::RefBinding { .. } | PatKind::Deref(_) => false,
        PatKind::Binding(_) => ty_is_non_copy(items, types, pat.ty),
        PatKind::Tuple(fields) | PatKind::Adt { fields, .. } => {
            fields.iter().any(|f| pat_moves_non_copy(items, types, f))
        }
        PatKind::ErrorArm { inner, .. } | PatKind::UnionArm { inner, .. } => {
            pat_moves_non_copy(items, types, inner)
        }
    }
}

/// Applies the implicit non-`Copy` ownership moves performed when matching
/// `pat` against `scrutinee` onto `state` and `res`.
fn apply_pattern_moves(
    items: &Items<'_>,
    types: &mut Types,
    pat: &Pat,
    scrutinee: &Expr,
    state: &mut CaptureMoveState,
    res: &mut CaptureMoveResult,
) {
    if let ExprKind::Capture(ci) = scrutinee.kind {
        match &pat.kind {
            PatKind::Wild | PatKind::Lit(_) | PatKind::RefBinding { .. } | PatKind::Deref(_) => {}
            PatKind::Binding(_) => {
                if ty_is_non_copy(items, types, pat.ty) {
                    res.whole_moved.insert(ci);
                    state.uninit.insert((ci, None));
                }
            }
            PatKind::Tuple(fields) | PatKind::Adt { fields, .. } => {
                for (fi, sub_pat) in fields.iter().enumerate() {
                    if pat_moves_non_copy(items, types, sub_pat) {
                        let pair = (ci, Some(fi as u32));
                        res.ever_given.insert(pair);
                        state.uninit.insert(pair);
                    }
                }
            }
            PatKind::ErrorArm { inner, .. } | PatKind::UnionArm { inner, .. } => {
                if pat_moves_non_copy(items, types, inner) {
                    let pair = (ci, None);
                    res.ever_given.insert(pair);
                    state.uninit.insert(pair);
                }
            }
        }
    } else if let Some(pair) = capture_place_field(scrutinee)
        && pat_moves_non_copy(items, types, pat)
    {
        res.ever_given.insert(pair);
        state.uninit.insert(pair);
    }
}

/// Path-sensitive forward analysis over a closure body to prove whether every
/// capture field move (`give cap.field`, non-Copy value read, or `match`/`let`
/// pattern binding) is refilled along all normal and early-exit paths before
/// the closure can be classified as `FnMut`.
fn analyze_capture_moves(
    items: &Items<'_>,
    types: &mut Types,
    expr: &Expr,
) -> (CaptureMoveState, CaptureMoveResult) {
    struct Ctx<'a, 'b> {
        items: &'a Items<'b>,
        types: &'a mut Types,
    }

    fn walk_place(
        ctx: &mut Ctx<'_, '_>,
        e: &Expr,
        state: &mut CaptureMoveState,
        res: &mut CaptureMoveResult,
        mut break_uninit: Option<&mut BTreeSet<(u32, Option<u32>)>>,
    ) {
        match &e.kind {
            ExprKind::Field { base, .. }
            | ExprKind::TupleField { base, .. }
            | ExprKind::Deref(base) => walk_place(ctx, base, state, res, break_uninit),
            ExprKind::Index { base, index } => {
                walk_place(ctx, base, state, res, break_uninit.as_deref_mut());
                let _ = walk_expr(ctx, index, state, res, break_uninit);
            }
            ExprKind::Capture(_) | ExprKind::Local(_) => {}
            ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::Adt { .. }
            | ExprKind::ErrorInject { .. }
            | ExprKind::UnionInject { .. }
            | ExprKind::ManagedCreate { .. }
            | ExprKind::Tuple(_)
            | ExprKind::Array(_)
            | ExprKind::Closure { .. }
            | ExprKind::ClosureFnRef(_)
            | ExprKind::Unary { .. }
            | ExprKind::Binary { .. }
            | ExprKind::Logical { .. }
            | ExprKind::Convert { .. }
            | ExprKind::Borrow { .. }
            | ExprKind::SliceBorrow { .. }
            | ExprKind::Describe(_)
            | ExprKind::Give(_)
            | ExprKind::Len(_)
            | ExprKind::Call { .. }
            | ExprKind::ForeignCall { .. }
            | ExprKind::IndirectCall { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::TraitCall { .. }
            | ExprKind::DynCall { .. }
            | ExprKind::Intrinsic { .. }
            | ExprKind::Try(_)
            | ExprKind::CtEval(_)
            | ExprKind::Block(_)
            | ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::Loop(_)
            | ExprKind::While { .. }
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Trap
            | ExprKind::Return(_)
            | ExprKind::Assign { .. } => {
                let _ = walk_expr(ctx, e, state, res, break_uninit);
            }
        }
    }

    /// Returns `true` if `expr` unconditionally diverges (e.g., `return`,
    /// `break`, `continue`, or all branches of an `if`/`match` diverge).
    fn walk_expr(
        ctx: &mut Ctx<'_, '_>,
        expr: &Expr,
        state: &mut CaptureMoveState,
        res: &mut CaptureMoveResult,
        mut break_uninit: Option<&mut BTreeSet<(u32, Option<u32>)>>,
    ) -> bool {
        match &expr.kind {
            ExprKind::Capture(idx) => {
                res.whole_moved.insert(*idx);
                state.uninit.insert((*idx, None));
                false
            }
            ExprKind::Field { base, .. } | ExprKind::TupleField { base, .. } => {
                walk_place(ctx, base, state, res, break_uninit);
                if ty_is_non_copy(ctx.items, ctx.types, expr.ty)
                    && let Some(pair) = capture_place_field(expr)
                {
                    res.ever_given.insert(pair);
                    state.uninit.insert(pair);
                }
                false
            }
            ExprKind::Deref(base)
            | ExprKind::Borrow { place: base, .. }
            | ExprKind::Describe(base)
            | ExprKind::Len(base) => {
                walk_place(ctx, base, state, res, break_uninit);
                false
            }
            ExprKind::SliceBorrow {
                place, start, end, ..
            } => {
                walk_place(ctx, place, state, res, break_uninit.as_deref_mut());
                if let Some(s) = start {
                    let _ = walk_expr(ctx, s, state, res, break_uninit.as_deref_mut());
                }
                if let Some(e) = end {
                    let _ = walk_expr(ctx, e, state, res, break_uninit);
                }
                false
            }
            ExprKind::Index { base, index } => {
                walk_place(ctx, base, state, res, break_uninit.as_deref_mut());
                let _ = walk_expr(ctx, index, state, res, break_uninit);
                false
            }
            ExprKind::Give(inner) => {
                if let ExprKind::Capture(idx) = inner.kind {
                    res.whole_moved.insert(idx);
                    state.uninit.insert((idx, None));
                } else if let Some(pair) = capture_place_field(inner) {
                    walk_place(ctx, inner, state, res, break_uninit);
                    res.ever_given.insert(pair);
                    state.uninit.insert(pair);
                } else {
                    return walk_expr(ctx, inner, state, res, break_uninit);
                }
                false
            }
            ExprKind::Try(inner) => {
                let div = walk_expr(ctx, inner, state, res, break_uninit);
                // `?` may exit the closure early on the error path.
                res.escaped_uninit.extend(state.uninit.iter().copied());
                div
            }
            ExprKind::ErrorInject { value: inner, .. }
            | ExprKind::UnionInject { value: inner }
            | ExprKind::Unary { operand: inner, .. }
            | ExprKind::Convert { operand: inner, .. } => {
                walk_expr(ctx, inner, state, res, break_uninit)
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                let l_div = walk_expr(ctx, lhs, state, res, break_uninit.as_deref_mut());
                if l_div {
                    return true;
                }
                walk_expr(ctx, rhs, state, res, break_uninit)
            }
            ExprKind::Logical { lhs, rhs, .. } => {
                if walk_expr(ctx, lhs, state, res, break_uninit.as_deref_mut()) {
                    return true;
                }
                // Short-circuit: `rhs` may or may not execute, so it cannot
                // unconditionally refill a field, but any move in `rhs` may happen.
                let mut rhs_state = state.clone();
                let _ = walk_expr(ctx, rhs, &mut rhs_state, res, break_uninit);
                state.uninit.extend(rhs_state.uninit);
                false
            }
            ExprKind::Assign { place, value, .. } => {
                walk_place(ctx, place, state, res, break_uninit.as_deref_mut());
                let div = walk_expr(ctx, value, state, res, break_uninit);
                if !div && let Some((ci, opt_field)) = direct_capture_assign_target(place) {
                    if let Some(fi) = opt_field {
                        state.uninit.remove(&(ci, Some(fi)));
                    } else {
                        state.uninit.retain(|(c, _)| *c != ci);
                    }
                }
                div
            }
            ExprKind::Adt { fields, .. } | ExprKind::ManagedCreate { fields, .. } => {
                for f in fields {
                    if walk_expr(ctx, f, state, res, break_uninit.as_deref_mut()) {
                        return true;
                    }
                }
                false
            }
            ExprKind::Tuple(xs) | ExprKind::Array(xs) => {
                for x in xs {
                    if walk_expr(ctx, x, state, res, break_uninit.as_deref_mut()) {
                        return true;
                    }
                }
                false
            }
            ExprKind::Closure { captures, .. } => {
                for c in captures {
                    if walk_expr(ctx, c, state, res, break_uninit.as_deref_mut()) {
                        return true;
                    }
                }
                false
            }
            ExprKind::Call { args, .. }
            | ExprKind::ForeignCall { args, .. }
            | ExprKind::TraitCall { args, .. }
            | ExprKind::Intrinsic { args, .. } => {
                for a in args {
                    if walk_expr(ctx, a, state, res, break_uninit.as_deref_mut()) {
                        return true;
                    }
                }
                false
            }
            ExprKind::IndirectCall { callee, args } => {
                if walk_expr(ctx, callee, state, res, break_uninit.as_deref_mut()) {
                    return true;
                }
                for a in args {
                    if walk_expr(ctx, a, state, res, break_uninit.as_deref_mut()) {
                        return true;
                    }
                }
                false
            }
            ExprKind::ClosureCall {
                closure,
                mode,
                args,
            } => {
                if *mode == ClosureCallMode::Once {
                    if walk_expr(ctx, closure, state, res, break_uninit.as_deref_mut()) {
                        return true;
                    }
                } else {
                    walk_place(ctx, closure, state, res, break_uninit.as_deref_mut());
                }
                for a in args {
                    if walk_expr(ctx, a, state, res, break_uninit.as_deref_mut()) {
                        return true;
                    }
                }
                false
            }
            ExprKind::DynCall { object, args, .. } => {
                // `object` is a `Borrow` for `&self`/`&mut self` slots (which
                // delegates to `walk_place`) and a bare value for `self: Self`
                // slots (which moves an `OwnedDyn` capture).
                if walk_expr(ctx, object, state, res, break_uninit.as_deref_mut()) {
                    return true;
                }
                for a in args {
                    if walk_expr(ctx, a, state, res, break_uninit.as_deref_mut()) {
                        return true;
                    }
                }
                false
            }
            ExprKind::Block(stmts) => {
                for s in stmts {
                    match s {
                        Stmt::Let { pattern, init } => {
                            if matches!(pattern.kind, PatKind::Binding(_)) {
                                if walk_expr(ctx, init, state, res, break_uninit.as_deref_mut()) {
                                    return true;
                                }
                            } else {
                                walk_place(ctx, init, state, res, break_uninit.as_deref_mut());
                                apply_pattern_moves(
                                    ctx.items, ctx.types, pattern, init, state, res,
                                );
                            }
                        }
                        Stmt::Expr(e) => {
                            if walk_expr(ctx, e, state, res, break_uninit.as_deref_mut()) {
                                return true;
                            }
                        }
                        Stmt::Defer(e) => {
                            // `defer` executes on scope exit; analyze conservatively
                            // so moves inside `defer` are recorded without letting
                            // a `defer` prematurely clear `state.uninit`.
                            let mut defer_state = state.clone();
                            let _ = walk_expr(ctx, e, &mut defer_state, res, None);
                            for item in defer_state.uninit {
                                res.escaped_uninit.insert(item);
                                state.uninit.insert(item);
                            }
                        }
                    }
                }
                false
            }
            ExprKind::If {
                cond,
                then,
                otherwise,
            } => {
                if walk_expr(ctx, cond, state, res, break_uninit.as_deref_mut()) {
                    return true;
                }
                let mut then_state = state.clone();
                let then_div =
                    walk_expr(ctx, then, &mut then_state, res, break_uninit.as_deref_mut());
                let mut else_state = state.clone();
                let else_div = if let Some(o) = otherwise {
                    walk_expr(ctx, o, &mut else_state, res, break_uninit)
                } else {
                    false
                };
                match (then_div, else_div) {
                    (true, true) => true,
                    (true, false) => {
                        *state = else_state;
                        false
                    }
                    (false, true) => {
                        *state = then_state;
                        false
                    }
                    (false, false) => {
                        state.uninit = then_state
                            .uninit
                            .into_iter()
                            .chain(else_state.uninit)
                            .collect();
                        false
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                walk_place(ctx, scrutinee, state, res, break_uninit.as_deref_mut());
                if arms.is_empty() {
                    return false;
                }
                let mut merged = BTreeSet::new();
                let mut any_normal = false;
                for arm in arms {
                    let mut arm_state = state.clone();
                    apply_pattern_moves(
                        ctx.items,
                        ctx.types,
                        &arm.pattern,
                        scrutinee,
                        &mut arm_state,
                        res,
                    );
                    let guard_diverges = arm.guard.as_ref().is_some_and(|g| {
                        walk_expr(ctx, g, &mut arm_state, res, break_uninit.as_deref_mut())
                    });
                    if !guard_diverges
                        && !walk_expr(
                            ctx,
                            &arm.body,
                            &mut arm_state,
                            res,
                            break_uninit.as_deref_mut(),
                        )
                    {
                        any_normal = true;
                        merged.extend(arm_state.uninit);
                    }
                }
                if any_normal {
                    state.uninit = merged;
                    false
                } else {
                    true
                }
            }
            ExprKind::While { cond, body } => {
                if walk_expr(ctx, cond, state, res, break_uninit) {
                    return true;
                }
                let mut body_state = state.clone();
                let mut loop_breaks = BTreeSet::new();
                let _ = walk_expr(ctx, body, &mut body_state, res, Some(&mut loop_breaks));
                // Loop body may execute 0 or more times: it cannot unconditionally
                // refill pre-loop uninit fields, and any uninit left by the body
                // or a `break` persists after the loop (where it may still be refilled).
                state.uninit.extend(body_state.uninit);
                state.uninit.extend(loop_breaks);
                false
            }
            ExprKind::Loop(body) => {
                let mut body_state = state.clone();
                let mut loop_breaks = BTreeSet::new();
                let _ = walk_expr(ctx, body, &mut body_state, res, Some(&mut loop_breaks));
                state.uninit.extend(body_state.uninit);
                state.uninit.extend(loop_breaks);
                false
            }
            ExprKind::Return(opt_v) => {
                if let Some(v) = opt_v {
                    let _ = walk_expr(ctx, v, state, res, None);
                }
                res.escaped_uninit.extend(state.uninit.iter().copied());
                true
            }
            ExprKind::Break => {
                if let Some(bu) = break_uninit {
                    bu.extend(state.uninit.iter().copied());
                } else {
                    res.escaped_uninit.extend(state.uninit.iter().copied());
                }
                true
            }
            ExprKind::Trap => true,
            ExprKind::Continue => {
                res.escaped_uninit.extend(state.uninit.iter().copied());
                true
            }
            ExprKind::Lit(_)
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
            | ExprKind::Local(_)
            | ExprKind::FnRef { .. }
            | ExprKind::ForeignRef(_)
            | ExprKind::ClosureFnRef(_)
            | ExprKind::CtEval(_) => false,
        }
    }

    let mut ctx = Ctx { items, types };
    let mut state = CaptureMoveState::default();
    let mut res = CaptureMoveResult::default();
    let _ = walk_expr(&mut ctx, expr, &mut state, &mut res, None);
    (state, res)
}

/// Pre-scans a non-`move` closure AST body to classify free outer variable
/// references as read-only, mutated, moved/consumed, or called.
fn scan_closure_usage(params: &[ast::ClosureParam], body: &ast::ArrowBody) -> ClosureUsage {
    fn is_bound(bound: &[Vec<String>], name: &str) -> bool {
        bound.iter().any(|s| s.iter().any(|n| n == name))
    }

    fn bind_pattern(pat: &ast::Pattern, scope: &mut Vec<String>) {
        match &pat.kind {
            ast::PatternKind::Name(n) => scope.push(n.clone()),
            ast::PatternKind::Reference { pattern, .. }
            | ast::PatternKind::Deref { pattern, .. }
            | ast::PatternKind::ErrorArm { pattern, .. } => bind_pattern(pattern, scope),
            ast::PatternKind::Tuple(ps) | ast::PatternKind::Constructor { arguments: ps, .. } => {
                for p in ps {
                    bind_pattern(p, scope);
                }
            }
            ast::PatternKind::RecordConstructor { fields, .. } => {
                for f in fields {
                    bind_pattern(&f.pattern, scope);
                }
            }
            ast::PatternKind::Wildcard | ast::PatternKind::Literal(_) => {}
        }
    }

    /// Extracts `(root_name, has_projection)` for a direct place path without
    /// crossing a `Dereference`.
    fn root_place_name(expr: &ast::Expr) -> Option<(&str, bool)> {
        match &expr.kind {
            ast::ExprKind::Name(n) => Some((n.as_str(), false)),
            ast::ExprKind::Group(inner) | ast::ExprKind::Ascribed { value: inner, .. } => {
                root_place_name(inner)
            }
            ast::ExprKind::Member { object, .. }
            | ast::ExprKind::Index { object, .. }
            | ast::ExprKind::Slice { object, .. } => {
                root_place_name(object).map(|(n, _)| (n, true))
            }
            ast::ExprKind::Literal(_)
            | ast::ExprKind::ReceiverMember(_)
            | ast::ExprKind::Tuple(_)
            | ast::ExprKind::Array(_)
            | ast::ExprKind::Unary { .. }
            | ast::ExprKind::Binary { .. }
            | ast::ExprKind::Pipeline { .. }
            | ast::ExprKind::TypeApply { .. }
            | ast::ExprKind::Call { .. }
            | ast::ExprKind::Try(_)
            | ast::ExprKind::Record { .. }
            | ast::ExprKind::Closure { .. }
            | ast::ExprKind::If { .. }
            | ast::ExprKind::MetaEval(_)
            | ast::ExprKind::MetaSplice(_)
            | ast::ExprKind::Quote { .. }
            | ast::ExprKind::MetaBlock(_)
            | ast::ExprKind::SyntaxIsland { .. } => None,
        }
    }

    fn scan_place(
        expr: &ast::Expr,
        force_move: bool,
        bound: &mut Vec<Vec<String>>,
        out: &mut ClosureUsage,
    ) {
        match &expr.kind {
            ast::ExprKind::Name(n) => {
                if force_move && !is_bound(bound, n) {
                    out.moved.insert(n.clone());
                    out.given.insert(n.clone());
                }
            }
            ast::ExprKind::Group(inner) | ast::ExprKind::Ascribed { value: inner, .. } => {
                scan_place(inner, force_move, bound, out);
            }
            ast::ExprKind::Member { object, .. } => {
                scan_place(object, force_move, bound, out);
            }
            ast::ExprKind::Index { object, index } => {
                scan_place(object, force_move, bound, out);
                scan_expr(index, true, force_move, bound, out);
            }
            ast::ExprKind::Slice { object, start, end } => {
                scan_place(object, force_move, bound, out);
                if let Some(s) = start {
                    scan_expr(s, true, force_move, bound, out);
                }
                if let Some(e) = end {
                    scan_expr(e, true, force_move, bound, out);
                }
            }
            ast::ExprKind::Literal(_)
            | ast::ExprKind::ReceiverMember(_)
            | ast::ExprKind::Tuple(_)
            | ast::ExprKind::Array(_)
            | ast::ExprKind::Unary { .. }
            | ast::ExprKind::Binary { .. }
            | ast::ExprKind::Pipeline { .. }
            | ast::ExprKind::TypeApply { .. }
            | ast::ExprKind::Call { .. }
            | ast::ExprKind::Try(_)
            | ast::ExprKind::Record { .. }
            | ast::ExprKind::Closure { .. }
            | ast::ExprKind::If { .. }
            | ast::ExprKind::MetaEval(_)
            | ast::ExprKind::MetaSplice(_)
            | ast::ExprKind::Quote { .. }
            | ast::ExprKind::MetaBlock(_)
            | ast::ExprKind::SyntaxIsland { .. } => {
                scan_expr(expr, false, force_move, bound, out);
            }
        }
    }

    fn unwrap_callee(mut expr: &ast::Expr) -> &ast::Expr {
        loop {
            match &expr.kind {
                ast::ExprKind::Group(inner)
                | ast::ExprKind::Ascribed { value: inner, .. }
                | ast::ExprKind::TypeApply { callee: inner, .. } => expr = inner,
                ast::ExprKind::Literal(_)
                | ast::ExprKind::Name(_)
                | ast::ExprKind::ReceiverMember(_)
                | ast::ExprKind::Tuple(_)
                | ast::ExprKind::Array(_)
                | ast::ExprKind::Unary { .. }
                | ast::ExprKind::Binary { .. }
                | ast::ExprKind::Pipeline { .. }
                | ast::ExprKind::Member { .. }
                | ast::ExprKind::Call { .. }
                | ast::ExprKind::Index { .. }
                | ast::ExprKind::Slice { .. }
                | ast::ExprKind::Try(_)
                | ast::ExprKind::Record { .. }
                | ast::ExprKind::Closure { .. }
                | ast::ExprKind::If { .. }
                | ast::ExprKind::MetaEval(_)
                | ast::ExprKind::MetaSplice(_)
                | ast::ExprKind::Quote { .. }
                | ast::ExprKind::MetaBlock(_)
                | ast::ExprKind::SyntaxIsland { .. } => return expr,
            }
        }
    }

    fn scan_expr(
        expr: &ast::Expr,
        in_value_pos: bool,
        force_move: bool,
        bound: &mut Vec<Vec<String>>,
        out: &mut ClosureUsage,
    ) {
        match &expr.kind {
            ast::ExprKind::Name(n) => {
                if !is_bound(bound, n) {
                    if in_value_pos || force_move {
                        out.moved.insert(n.clone());
                    }
                    if force_move {
                        out.given.insert(n.clone());
                    }
                }
            }
            ast::ExprKind::Group(inner) | ast::ExprKind::Ascribed { value: inner, .. } => {
                scan_expr(inner, in_value_pos, force_move, bound, out);
            }
            ast::ExprKind::Unary { op, operand } => match op {
                ast::UnaryOp::Borrow => scan_place(operand, force_move, bound, out),
                ast::UnaryOp::BorrowMut => {
                    if let Some((n, _)) = root_place_name(operand)
                        && !is_bound(bound, n)
                    {
                        out.mutated.insert(n.to_string());
                    }
                    scan_place(operand, force_move, bound, out);
                }
                ast::UnaryOp::Give => {
                    if let Some((n, _)) = root_place_name(operand)
                        && !is_bound(bound, n)
                    {
                        out.moved.insert(n.to_string());
                        out.given.insert(n.to_string());
                    }
                    scan_place(operand, force_move, bound, out);
                }
                ast::UnaryOp::Not
                | ast::UnaryOp::Negate
                | ast::UnaryOp::BitNot
                | ast::UnaryOp::Dereference => {
                    scan_expr(operand, false, force_move, bound, out);
                }
            },
            ast::ExprKind::Binary { left, right, .. } => {
                scan_expr(left, false, force_move, bound, out);
                scan_expr(right, false, force_move, bound, out);
            }
            ast::ExprKind::Pipeline { input, target } => {
                scan_expr(input, true, force_move, bound, out);
                scan_expr(target, false, force_move, bound, out);
            }
            ast::ExprKind::Member { object, .. } => {
                scan_place(object, force_move, bound, out);
            }
            ast::ExprKind::Index { object, index } => {
                scan_place(object, force_move, bound, out);
                scan_expr(index, true, force_move, bound, out);
            }
            ast::ExprKind::Slice { object, start, end } => {
                scan_place(object, force_move, bound, out);
                if let Some(s) = start {
                    scan_expr(s, true, force_move, bound, out);
                }
                if let Some(e) = end {
                    scan_expr(e, true, force_move, bound, out);
                }
            }
            ast::ExprKind::Call { callee, arguments } => {
                let mut skip_first_arg = false;
                match &unwrap_callee(callee).kind {
                    ast::ExprKind::Member { object, member } => {
                        if let Some((n, has_proj)) = root_place_name(object)
                            && !is_bound(bound, n)
                        {
                            if force_move {
                                out.moved.insert(n.to_string());
                            } else if !has_proj {
                                match member {
                                    ast::MemberName::Name(mname)
                                    | ast::MemberName::Constrained { name: mname, .. } => {
                                        out.direct_methods.push((n.to_string(), mname.clone()));
                                    }
                                    ast::MemberName::TupleIndex(_) => {
                                        out.mutated.insert(n.to_string());
                                    }
                                }
                            } else {
                                out.mutated.insert(n.to_string());
                            }
                        }
                        scan_place(object, force_move, bound, out);
                    }
                    ast::ExprKind::Name(n) => {
                        let is_shared_intrinsic = matches!(
                            n.as_str(),
                            "bytes_len"
                                | "bytes_capacity"
                                | "bytes_view"
                                | "store_len"
                                | "store_capacity"
                                | "store_at"
                                | "mutex_lock"
                                | "atomic_load"
                                | "atomic_load_ordered"
                                | "task_valid"
                                | "registration_valid"
                                | "async_registration_valid"
                        );
                        let is_mut_intrinsic = matches!(
                            n.as_str(),
                            "bytes_reserve"
                                | "bytes_push"
                                | "bytes_truncate"
                                | "bytes_view_mut"
                                | "store_reserve"
                                | "store_push"
                                | "store_pop"
                                | "store_at_mut"
                                | "mutex_value"
                                | "atomic_store"
                                | "atomic_fetch_add"
                                | "atomic_compare_exchange"
                                | "atomic_store_ordered"
                                | "atomic_fetch_add_ordered"
                                | "atomic_exchange_ordered"
                                | "atomic_compare_exchange_ordered"
                                | "task_poll"
                                | "task_request_cancel"
                                | "task_poll_close"
                                | "task_finish_destroy"
                                | "registration_close"
                                | "async_registration_request_close"
                                | "async_registration_poll_close"
                                | "async_registration_finish_destroy"
                                | "async_registration_finish_destroy_checked"
                        );
                        if (is_shared_intrinsic || is_mut_intrinsic) && !arguments.is_empty() {
                            skip_first_arg = true;
                            let first = &arguments[0].value;
                            if is_mut_intrinsic
                                && let Some((root, _)) = root_place_name(first)
                                && !is_bound(bound, root)
                            {
                                if force_move {
                                    out.moved.insert(root.to_string());
                                } else {
                                    out.mutated.insert(root.to_string());
                                }
                            }
                            scan_place(first, force_move, bound, out);
                        } else if !is_bound(bound, n) {
                            if force_move {
                                out.moved.insert(n.clone());
                            } else {
                                out.called.insert(n.clone());
                            }
                        }
                    }
                    ast::ExprKind::Literal(_)
                    | ast::ExprKind::ReceiverMember(_)
                    | ast::ExprKind::Group(_)
                    | ast::ExprKind::Ascribed { .. }
                    | ast::ExprKind::Tuple(_)
                    | ast::ExprKind::Array(_)
                    | ast::ExprKind::Unary { .. }
                    | ast::ExprKind::Binary { .. }
                    | ast::ExprKind::Pipeline { .. }
                    | ast::ExprKind::TypeApply { .. }
                    | ast::ExprKind::Call { .. }
                    | ast::ExprKind::Index { .. }
                    | ast::ExprKind::Slice { .. }
                    | ast::ExprKind::Try(_)
                    | ast::ExprKind::Record { .. }
                    | ast::ExprKind::Closure { .. }
                    | ast::ExprKind::If { .. }
                    | ast::ExprKind::MetaEval(_)
                    | ast::ExprKind::MetaSplice(_)
                    | ast::ExprKind::Quote { .. }
                    | ast::ExprKind::MetaBlock(_)
                    | ast::ExprKind::SyntaxIsland { .. } => {
                        scan_expr(callee, false, force_move, bound, out);
                    }
                }
                for (idx, arg) in arguments.iter().enumerate() {
                    if skip_first_arg && idx == 0 {
                        continue;
                    }
                    scan_expr(&arg.value, true, force_move, bound, out);
                }
            }
            ast::ExprKind::TypeApply { callee, .. } => {
                scan_expr(callee, false, force_move, bound, out);
            }
            ast::ExprKind::Try(inner) => {
                scan_expr(inner, true, force_move, bound, out);
            }
            ast::ExprKind::Tuple(items) | ast::ExprKind::Array(items) => {
                for it in items {
                    scan_expr(it, true, force_move, bound, out);
                }
            }
            ast::ExprKind::Record { ty, base, fields } => {
                if let Some(t) = ty {
                    scan_expr(t, false, force_move, bound, out);
                }
                if let Some(b) = base {
                    scan_expr(b, true, force_move, bound, out);
                }
                for f in fields {
                    match &f.value {
                        Some(v) => scan_expr(v, true, force_move, bound, out),
                        None => {
                            if !is_bound(bound, &f.name) {
                                out.moved.insert(f.name.clone());
                            }
                        }
                    }
                }
            }
            ast::ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                scan_expr(condition, false, force_move, bound, out);
                scan_expr(then_value, in_value_pos, force_move, bound, out);
                scan_expr(else_value, in_value_pos, force_move, bound, out);
            }
            ast::ExprKind::Closure {
                move_capture,
                params,
                body,
                ..
            } => {
                bound.push(params.iter().map(|p| p.name.clone()).collect());
                scan_arrow_body(body, force_move || *move_capture, bound, out);
                bound.pop();
            }
            ast::ExprKind::Literal(_)
            | ast::ExprKind::ReceiverMember(_)
            | ast::ExprKind::MetaEval(_)
            | ast::ExprKind::MetaSplice(_)
            | ast::ExprKind::Quote { .. }
            | ast::ExprKind::MetaBlock(_)
            | ast::ExprKind::SyntaxIsland { .. } => {}
        }
    }

    fn scan_condition(
        cond: &ast::Condition,
        force_move: bool,
        bound: &mut Vec<Vec<String>>,
        out: &mut ClosureUsage,
    ) {
        match cond {
            ast::Condition::Expression(e) => scan_expr(e, false, force_move, bound, out),
            ast::Condition::Let { pattern, value } => {
                scan_expr(value, false, force_move, bound, out);
                if let Some(scope) = bound.last_mut() {
                    bind_pattern(pattern, scope);
                }
            }
        }
    }

    fn scan_stmt(
        stmt: &ast::Stmt,
        force_move: bool,
        bound: &mut Vec<Vec<String>>,
        out: &mut ClosureUsage,
    ) {
        match &stmt.kind {
            ast::StmtKind::Binding { pattern, value, .. } => {
                scan_expr(value, true, force_move, bound, out);
                if let Some(scope) = bound.last_mut() {
                    bind_pattern(pattern, scope);
                }
            }
            ast::StmtKind::Assign { target, value, .. } => {
                if let Some((n, _)) = root_place_name(target)
                    && !is_bound(bound, n)
                {
                    if force_move {
                        out.moved.insert(n.to_string());
                    } else {
                        out.mutated.insert(n.to_string());
                    }
                }
                scan_place(target, force_move, bound, out);
                scan_expr(value, true, force_move, bound, out);
            }
            ast::StmtKind::Return(Some(e)) | ast::StmtKind::Yield(e) => {
                scan_expr(e, true, force_move, bound, out);
            }
            ast::StmtKind::Return(None)
            | ast::StmtKind::Break
            | ast::StmtKind::Continue
            | ast::StmtKind::Meta(_) => {}
            ast::StmtKind::Defer(b) | ast::StmtKind::Loop(b) | ast::StmtKind::Unsafe(b) => {
                scan_block(b, force_move, bound, out);
            }
            ast::StmtKind::If {
                condition,
                then_body,
                else_body,
            } => {
                bound.push(Vec::new());
                scan_condition(condition, force_move, bound, out);
                scan_block(then_body, force_move, bound, out);
                bound.pop();
                if let Some(eb) = else_body {
                    match eb {
                        ast::ElseBranch::Block(b) => scan_block(b, force_move, bound, out),
                        ast::ElseBranch::If(s) => scan_stmt(s, force_move, bound, out),
                    }
                }
            }
            ast::StmtKind::While { condition, body } => {
                bound.push(Vec::new());
                scan_condition(condition, force_move, bound, out);
                scan_block(body, force_move, bound, out);
                bound.pop();
            }
            ast::StmtKind::For {
                pattern,
                iterable,
                body,
            } => {
                scan_expr(iterable, true, force_move, bound, out);
                bound.push(Vec::new());
                if let Some(scope) = bound.last_mut() {
                    bind_pattern(pattern, scope);
                }
                scan_block(body, force_move, bound, out);
                bound.pop();
            }
            ast::StmtKind::Match { value, arms } => {
                scan_place(value, force_move, bound, out);
                for arm in arms {
                    bound.push(Vec::new());
                    if let Some(scope) = bound.last_mut() {
                        bind_pattern(&arm.pattern, scope);
                    }
                    if let Some(g) = &arm.guard {
                        scan_expr(g, false, force_move, bound, out);
                    }
                    scan_arrow_body(&arm.body, force_move, bound, out);
                    bound.pop();
                }
            }
            ast::StmtKind::With { name, value, body } => {
                scan_expr(value, true, force_move, bound, out);
                bound.push(vec![name.clone()]);
                scan_block(body, force_move, bound, out);
                bound.pop();
            }
            ast::StmtKind::Expression(e) => {
                scan_expr(e, false, force_move, bound, out);
            }
        }
    }

    fn scan_block(
        block: &ast::Block,
        force_move: bool,
        bound: &mut Vec<Vec<String>>,
        out: &mut ClosureUsage,
    ) {
        bound.push(Vec::new());
        for s in &block.statements {
            scan_stmt(s, force_move, bound, out);
        }
        bound.pop();
    }

    fn scan_arrow_body(
        body: &ast::ArrowBody,
        force_move: bool,
        bound: &mut Vec<Vec<String>>,
        out: &mut ClosureUsage,
    ) {
        match body {
            ast::ArrowBody::Block(b) => scan_block(b, force_move, bound, out),
            ast::ArrowBody::Expression(e) => scan_expr(e, true, force_move, bound, out),
        }
    }

    let mut out = ClosureUsage::default();
    let mut bound = vec![params.iter().map(|p| p.name.clone()).collect()];
    scan_arrow_body(body, false, &mut bound, &mut out);
    out
}

fn collect_ref_mut_bindings(pat: &Pat, out: &mut std::collections::HashSet<LocalId>) {
    match &pat.kind {
        PatKind::RefBinding {
            local,
            mutable: true,
        } => {
            out.insert(*local);
        }
        PatKind::Deref(inner)
        | PatKind::ErrorArm { inner, .. }
        | PatKind::UnionArm { inner, .. } => collect_ref_mut_bindings(inner, out),
        PatKind::Tuple(items) => {
            for item in items {
                collect_ref_mut_bindings(item, out);
            }
        }
        PatKind::Adt { fields, .. } => {
            for field in fields {
                collect_ref_mut_bindings(field, out);
            }
        }
        PatKind::Wild
        | PatKind::Binding(_)
        | PatKind::RefBinding { mutable: false, .. }
        | PatKind::Lit(_) => {}
    }
}
