//! infer: name resolution inside bodies and type inference, producing THIR.
//!
//! Inference is equality-based: every expression gets a type, possibly an
//! inference variable, and constraints are solved by unification as they
//! arise. Unsuffixed literals carry kinded variables that default at the
//! end. `finalize` then zonks every type and applies the typing rules that
//! need closed types (operator domains, literal ranges, bounds).

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
    mutable: bool,
    mutated: bool,
    consumed: bool,
    field_ty: Ty,
}

struct CaptureEnv {
    /// Visible locals in the immediately enclosing function or closure: `(name, local, ty, mutable)`.
    outer: Vec<(String, LocalId, Ty, bool)>,
    move_capture: bool,
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
        let start = start.map(|e| Box::new(self.infer(e)));
        let end = end.map(|e| Box::new(self.infer(e)));
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
        let et = self.items.normalize_ty(expected, self.types);
        let vt = self.items.normalize_ty(value.ty, self.types);
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

        self.coerce(&value, expected, span);
        value
    }

    fn unify(&mut self, a: Ty, b: Ty, span: Span) {
        let a = self.items.normalize_ty(a, self.types);
        let b = self.items.normalize_ty(b, self.types);
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
            if let Some(adt) = self
                .items
                .resolve_adt_of(self.decl.module, enum_path, self.types)
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
            ast::StmtKind::Yield(_) => {
                self.unsupported("yield", span);
                None
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
                    op: ast::BinaryOp::Range,
                    left,
                    right,
                } = &iterable.kind
                {
                    let start_expr = self.infer(left);
                    let end_expr = self.infer(right);
                    let iter_ty = start_expr.ty;
                    let end_expr = self.coerce_value(end_expr, iter_ty, right.span);

                    self.scopes.push(Vec::new());
                    let cur_local = self.declare("__for_cur", iter_ty, true, left.span);
                    let end_local = self.declare("__for_end", iter_ty, false, right.span);

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
                    let cond = Expr {
                        kind: ExprKind::Binary {
                            op: BinOp::Ge,
                            lhs: Box::new(cur_place.clone()),
                            rhs: Box::new(end_place),
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
                    let pat = self.pattern(pattern, iter_ty, false);
                    let bind_stmt = Stmt::Let {
                        pattern: pat,
                        init: cur_place.clone(),
                    };
                    let one_ty = self.types.fresh(VarKind::Int);
                    let _ = self.types.unify(one_ty, iter_ty);
                    let inc_stmt = Stmt::Expr(Expr {
                        kind: ExprKind::Assign {
                            place: Box::new(cur_place),
                            op: Some(BinOp::Add),
                            value: Box::new(Expr {
                                kind: ExprKind::Lit(Lit::Int(1)),
                                ty: iter_ty,
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
                            Stmt::Expr(loop_expr),
                        ]),
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
            ast::PatternKind::Reference {
                mutable: is_mut,
                pattern: inner,
            } => {
                if let ast::PatternKind::Name(n) = &inner.kind {
                    let ref_ty = self.types.intern(TyData::Ref {
                        region: None,
                        mutable: *is_mut,
                        pointee: expected,
                    });
                    PatKind::RefBinding {
                        local: self.declare(n, ref_ty, mutable, inner.span),
                        mutable: *is_mut,
                    }
                } else {
                    self.unsupported("nested reference patterns", span);
                    PatKind::Wild
                }
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
            PatKind::Wild | PatKind::Binding(_) | PatKind::RefBinding { .. } => true,
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
                    | TyData::FnPtr { .. }
                    | TyData::Closure { .. }
                    | TyData::ImplCallable { .. }
                    | TyData::Dyn(_)
                    | TyData::OwnedDyn(_)
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
                self.record(adt, variant, explicit_args, fields, span)
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
            ast::ExprKind::MetaSplice(_)
            | ast::ExprKind::Quote { .. }
            | ast::ExprKind::MetaBlock(_)
            | ast::ExprKind::SyntaxIsland { .. } => {
                self.unsupported("compile-time evaluation", span);
                self.err_expr(span)
            }
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

        let env = &mut self.capture_stack[depth];
        let idx = if let Some(pos) = env.captures.iter().position(|c| c.source == source) {
            pos
        } else {
            let by_value = env.move_capture;
            let mutable = outer_mut;
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
        let et = self.items.normalize_ty(expected, self.types);
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
            | ast::ExprKind::MetaBlock(_)
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
        self.capture_stack.push(CaptureEnv {
            outer,
            move_capture,
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
        let env = self
            .capture_stack
            .pop()
            .expect("capture_stack pushed above");

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
                    let is_managed = matches!(
                        self.types.data(self.types.shallow(c.orig_ty)),
                        TyData::Managed(_)
                    );
                    let outer_env = self
                        .capture_stack
                        .last_mut()
                        .expect("OuterCapture requires enclosing closure");
                    let outer_cap = &mut outer_env.captures[outer_idx as usize];
                    if c.consumed || (c.by_value && is_managed) {
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
            capture_exprs.push(if c.by_value {
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
                    | TyData::FnPtr { .. }
                    | TyData::Closure { .. }
                    | TyData::ImplCallable { .. }
                    | TyData::Dyn(_)
                    | TyData::OwnedDyn(_)
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
                            format!("field `{}` is missing", decl_fields[i]),
                        );
                        out.push(self.err_expr(span));
                    }
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
        // Static inline method call with explicit host generics: `Host::<T>.method::<U>(args...)`.
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
                if let Some(adt) =
                    self.items
                        .resolve_adt_of(self.decl.module, enum_path, self.types)
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
            }
            if let [param_name, method_name] = path.as_slice()
                && explicit.is_none()
                && let Some(param_idx) = self.generic_names.iter().rposition(|g| g == param_name)
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
            if explicit.is_some() {
                self.error(
                    diag::TYPE_ARGUMENT,
                    span,
                    format!("method `{mname}` does not accept explicit type arguments here"),
                );
                return self.err_expr(span);
            }
            if let TyData::Dyn(tr) | TyData::OwnedDyn(tr) = *self.types.data(self_ty) {
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
                        let expected = self.types.subst_with_self(*pt, &[], self_ty);
                        let expected = self.items.normalize_ty(expected, self.types);
                        let v = self.infer(&a.value);
                        args.push(self.coerce_arg(v, expected, a.span));
                    }
                    let ret = self.types.subst_with_self(m_result, &[], self_ty);
                    let ret = self.items.normalize_ty(ret, self.types);
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
                self.decl
                    .generics
                    .get(param_idx as usize)
                    .into_iter()
                    .flat_map(|g| g.traits.iter())
                    .find_map(|(tr_ref, tr_args)| {
                        let tr = self.items.trait_decl(*tr_ref);
                        tr.methods
                            .iter()
                            .find(|m| m.name == *mname && !m.params.is_empty())
                            .map(|m| (*tr_ref, tr_args.clone(), m.params.clone(), m.result))
                    })
            } else {
                let try_lookup = |target_ty: Ty, types: &mut Types| {
                    self.items.trait_impls.iter().find_map(|imp| {
                        let impl_args = self.items.match_impl(imp, target_ty, types)?;
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
                                (imp.trait_ref, resolved_targs, m.params.clone(), m.result)
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
            if let Some((tr_ref, tr_args, m_params, m_result)) = found_trait {
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
                let expected_self = self.types.subst_with_self(m_params[0], &tr_args, self_ty);
                let expected_self = self.items.normalize_ty(expected_self, self.types);
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
                        self_ty,
                        method: mname.clone(),
                        args,
                    },
                    ty: ret,
                    span,
                };
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
            let Some([ast::GenericArg::Type(t)]) = explicit else {
                self.error(
                    diag::TYPE_ARGUMENT,
                    span,
                    "`owned_dyn` requires one `dyn Trait` type argument",
                );
                return Some(self.err_expr(span));
            };
            let target = self.resolve_written(t);
            let TyData::Dyn(tr) = *self.types.data(target) else {
                self.error(
                    diag::TYPE_ARGUMENT,
                    span,
                    "`owned_dyn` type argument must be `dyn Trait`",
                );
                return Some(self.err_expr(span));
            };
            let [arg] = arguments else {
                self.error(diag::CALL_ARITY, span, "`owned_dyn` takes one argument");
                return Some(self.err_expr(span));
            };
            let val = self.infer(&arg.value);
            let ok = self.types.intern(TyData::OwnedDyn(tr));
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
            "thread_scope" => IntrinsicOp::ThreadScope,
            "task_create" | "task_create_owned" => IntrinsicOp::TaskCreate,
            "task_valid" => IntrinsicOp::TaskValid,
            "task_yield" => IntrinsicOp::TaskYield,
            "task_poll" => IntrinsicOp::TaskPoll,
            "task_request_cancel" => IntrinsicOp::TaskRequestCancel,
            "task_poll_close" => IntrinsicOp::TaskPollClose,
            "task_finish_destroy" => IntrinsicOp::TaskFinishDestroy,
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
            IntrinsicOp::RegistrationStart | IntrinsicOp::AsyncRegistrationStart
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
            let ret_ty = self.types.intern(TyData::Tuple(vec![bool_ty, r1, r2]));
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
            IntrinsicOp::MutexUnlock
            | IntrinsicOp::SyncLoad
            | IntrinsicOp::SyncStore
            | IntrinsicOp::SyncFetchAdd
            | IntrinsicOp::SyncCompareExchange
            | IntrinsicOp::TaskStart
            | IntrinsicOp::ManagedCreate
            | IntrinsicOp::OwnedDynCreate
            | IntrinsicOp::ThreadScope => return None,
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
        let ty = self.types.subst(result, &type_args);
        let ty = self.types.erase_regions(ty);
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
                TyData::Tuple(xs) => xs.into_iter().any(|x| walk(types, x, depth + 1)),
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
                | TyData::Dyn(_)
                | TyData::OwnedDyn(_)
                | TyData::Opaque(
                    OpaqueKind::ByteBuffer
                    | OpaqueKind::ForeignHandle
                    | OpaqueKind::ForeignCallback { .. },
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
            | ExprKind::ConstGeneric(_)
            | ExprKind::StaticUtf(_)
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
        if self.explicit_direct && !expr_contains_suspend(&f.body) {
            for (li, l) in f.locals.iter().enumerate() {
                let lid = LocalId(li as u32);
                if self.may_suspend_on_drop(l.ty) && !local_is_consumed(&f.body, lid) {
                    self.diags.push(diag::error(
                        diag::GENERIC_BOUND,
                        l.span,
                        "default Drop cleanup may suspend and lacks a Closed fact or a Suspend execution root",
                    ));
                }
            }
        }
    }

    fn may_suspend_on_drop(&self, ty: Ty) -> bool {
        match self.data(ty) {
            TyData::Managed(inner) => matches!(
                self.data(inner),
                TyData::Opaque(OpaqueKind::Task { .. } | OpaqueKind::AsyncRegistration(_))
            ),
            TyData::Closure { .. } | TyData::ImplCallable { .. } => true,
            TyData::Tuple(xs) => xs.iter().any(|&x| self.may_suspend_on_drop(x)),
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
            | TyData::Array { .. }
            | TyData::ArrayConst { .. }
            | TyData::Slice(_)
            | TyData::Ref { .. }
            | TyData::FnPtr { .. }
            | TyData::Dyn(_)
            | TyData::OwnedDyn(_)
            | TyData::Desc(_)
            | TyData::Adt { .. }
            | TyData::ErrorUnion { .. }
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
            | TyData::Type
            | TyData::ConstParam(_)
            | TyData::ConstU32(_)
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Error => true,
            TyData::Tuple(items) => items.iter().all(|t| self.is_copy_depth(*t, depth + 1)),
            TyData::Array { element, .. } | TyData::ArrayConst { element, .. } => {
                self.is_copy_depth(element, depth + 1)
            }
            TyData::Slice(_) | TyData::SelfParam => false,
            TyData::Ref { mutable, .. } => !mutable,
            TyData::Desc(_) | TyData::FnPtr { .. } => true,
            TyData::Closure { .. }
            | TyData::ImplCallable { .. }
            | TyData::Dyn(_)
            | TyData::OwnedDyn(_) => false,
            TyData::Managed(_) => false,
            TyData::Opaque(OpaqueKind::ForeignHandle | OpaqueKind::ForeignCallback { .. }) => true,
            TyData::Opaque(
                OpaqueKind::ByteBuffer
                | OpaqueKind::OwnedStore(_)
                | OpaqueKind::Mutex(_)
                | OpaqueKind::MutexGuard(_)
                | OpaqueKind::Task { .. }
                | OpaqueKind::Registration(_)
                | OpaqueKind::AsyncRegistration(_),
            ) => false,
            TyData::ErrorUnion { ok, err } => {
                self.is_copy_depth(ok, depth + 1) && self.is_copy_depth(err, depth + 1)
            }
            TyData::Param(i) => self.generics.get(i as usize).is_some_and(|g| g.copy),
            TyData::Adt { adt, args, .. } => {
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
            TyData::Assoc { .. } | TyData::Var(_) => false,
        }
    }

    fn pat(&mut self, p: &mut Pat) {
        p.ty = self.zonk(p.ty, p.span);
        match &mut p.kind {
            PatKind::Wild | PatKind::Binding(_) | PatKind::RefBinding { .. } => {}
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
                TyData::Int(_) | TyData::Float(_) | TyData::Bool | TyData::Unit | TyData::Type
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
            | ExprKind::Continue => {}
            ExprKind::FnRef { func, type_args } => {
                for a in type_args.iter_mut() {
                    *a = self.zonk(*a, span);
                }
                let generics = self.items.func(*func).generics.clone();
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
                                .map(|t| {
                                    let s = self.types.subst(*t, type_args);
                                    self.items.normalize_ty(s, self.types)
                                })
                                .collect();
                            let found = self.items.trait_impls.iter().any(|imp| {
                                if imp.trait_ref != *tr_ref {
                                    return false;
                                }
                                let Some(impl_args) = self.items.match_impl(imp, *a, self.types)
                                else {
                                    return false;
                                };
                                let imp_targs: Vec<Ty> = imp
                                    .trait_args
                                    .iter()
                                    .map(|t| {
                                        let s = self.types.subst(*t, &impl_args);
                                        self.items.normalize_ty(s, self.types)
                                    })
                                    .collect();
                                imp_targs == resolved_args
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
                                .map(|t| {
                                    let s = self.types.subst(*t, type_args);
                                    self.items.normalize_ty(s, self.types)
                                })
                                .collect();
                            let found = self.items.trait_impls.iter().any(|imp| {
                                if imp.trait_ref != *tr_ref {
                                    return false;
                                }
                                let Some(impl_args) = self.items.match_impl(imp, *a, self.types)
                                else {
                                    return false;
                                };
                                let imp_targs: Vec<Ty> = imp
                                    .trait_args
                                    .iter()
                                    .map(|t| {
                                        let s = self.types.subst(*t, &impl_args);
                                        self.items.normalize_ty(s, self.types)
                                    })
                                    .collect();
                                imp_targs == resolved_args
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
        | ExprKind::Continue => false,
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
            expr_contains_suspend(scrutinee) || arms.iter().any(|a| expr_contains_suspend(&a.body))
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
            ExprKind::ErrorInject { value, .. } => {
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
            | ExprKind::Continue => false,
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
                | TyData::FnPtr { .. }
                | TyData::ImplCallable { .. }
                | TyData::Dyn(_)
                | TyData::OwnedDyn(_)
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
                    || arms
                        .iter()
                        .any(|arm| expr_suspends(&arm.body, items, types, fn_susp, cl_susp))
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
            | ExprKind::Continue => false,
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
            | ExprKind::Continue => {}
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
            | ExprKind::Try(_)
            | ExprKind::CtEval(_)
            | ExprKind::Tuple(_)
            | ExprKind::Array(_)
            | ExprKind::Break
            | ExprKind::Continue
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
