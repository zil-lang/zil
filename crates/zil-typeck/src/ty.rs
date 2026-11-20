//! typeck-internal types with inference variables.
//!
//! These types never leave the crate: after inference every type is
//! resolved ("zonked") to a closed form and interned into the body
//! `TyTable` by the builder. The constructors mirror the subset of
//! `body::TyKind` that Source can currently name; a construct that has no
//! internal form yet is reported as `Unsupported`, never approximated.

use crate::thir::FnRef;
use std::collections::HashMap;
use zil_ir::body::{ClosureKind, FloatTy, IntTy};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ty(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdtRef(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClosureRef(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TraitRef(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VarId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VarKind {
    General,
    /// Unsuffixed integer literal; defaults to `i32`.
    Int,
    /// Unsuffixed float literal; defaults to `f64`.
    Float,
}

/// Built-in opaque runtime objects known to typeck.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum OpaqueKind {
    ByteBuffer,
    OwnedStore(Ty),
    Mutex(Ty),
    MutexGuard(Ty),
    Task(Ty),
    AsyncRegistration(Ty),
    ForeignHandle,
    ForeignCallback { params: Vec<Ty>, result: Ty },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TyData {
    Unit,
    Bool,
    Never,
    Int(IntTy),
    Float(FloatTy),
    Adt {
        adt: AdtRef,
        args: Vec<Ty>,
    },
    Tuple(Vec<Ty>),
    Array {
        element: Ty,
        length: u32,
    },
    Slice(Ty),
    /// An address window. Regions are not part of typeck types yet: the
    /// kernel's loan model carries safety until region contracts (P4).
    Ref {
        mutable: bool,
        pointee: Ty,
    },
    /// Owning responsibility carrier of a resource or built-in opaque object.
    Managed(Ty),
    /// Non-owning descriptor of a resource or built-in opaque object.
    Desc(Ty),
    Opaque(OpaqueKind),
    /// `E!T`: `ok` is the success arm `T`, `err` is the error arm `E`.
    ErrorUnion {
        ok: Ty,
        err: Ty,
    },
    /// `fn(P...) -> R`: non-capturing function pointer.
    FnPtr {
        params: Vec<Ty>,
        result: Ty,
    },
    /// A unique closure expression inside `owner`.
    Closure {
        owner: FnRef,
        id: ClosureRef,
        kind: ClosureKind,
        params: Vec<Ty>,
        result: Ty,
    },
    /// Return-position `impl Fn/FnMut/FnOnce` of `owner`.
    ImplCallable {
        owner: FnRef,
        kind: ClosureKind,
        params: Vec<Ty>,
        result: Ty,
    },
    /// `dyn Trait` unsized pointee.
    Dyn(TraitRef),
    /// `OwnedDyn<dyn Trait>` affine owner.
    OwnedDyn(TraitRef),
    /// The n-th type parameter of the enclosing generic declaration.
    Param(u32),
    /// `Self` inside a trait method signature.
    SelfParam,
    /// Associated type projection (`I.Item`, `M.Output`, `Self.Item`).
    Assoc {
        base: Ty,
        name: String,
    },
    Var(VarId),
    /// Already diagnosed; unifies with everything to avoid cascades.
    Error,
}

#[derive(Default)]
pub struct Types {
    data: Vec<TyData>,
    by_data: HashMap<TyData, Ty>,
    vars: Vec<(VarKind, Option<Ty>)>,
}

#[derive(Clone, Copy, Debug)]
pub struct Mismatch;

impl Types {
    pub fn intern(&mut self, data: TyData) -> Ty {
        if let Some(t) = self.by_data.get(&data) {
            return *t;
        }
        let t = Ty(self.data.len() as u32);
        self.data.push(data.clone());
        self.by_data.insert(data, t);
        t
    }
    pub fn data(&self, ty: Ty) -> &TyData {
        &self.data[ty.0 as usize]
    }
    pub fn unit(&mut self) -> Ty {
        self.intern(TyData::Unit)
    }
    pub fn bool(&mut self) -> Ty {
        self.intern(TyData::Bool)
    }
    pub fn never(&mut self) -> Ty {
        self.intern(TyData::Never)
    }
    pub fn error(&mut self) -> Ty {
        self.intern(TyData::Error)
    }
    pub fn int(&mut self, i: IntTy) -> Ty {
        self.intern(TyData::Int(i))
    }
    pub fn fresh(&mut self, kind: VarKind) -> Ty {
        let v = VarId(self.vars.len() as u32);
        self.vars.push((kind, None));
        self.intern(TyData::Var(v))
    }

    /// Follows variable bindings at the top level only.
    pub fn shallow(&self, mut ty: Ty) -> Ty {
        while let TyData::Var(v) = self.data(ty) {
            match self.vars[v.0 as usize].1 {
                Some(t) => ty = t,
                None => break,
            }
        }
        ty
    }

    pub fn is_never(&self, ty: Ty) -> bool {
        matches!(self.data(self.shallow(ty)), TyData::Never)
    }

    /// Probes whether `a` and `b` can unify without mutating inference state.
    pub fn can_unify(&mut self, a: Ty, b: Ty) -> bool {
        let saved = self.vars.clone();
        let ok = self.unify(a, b).is_ok();
        self.vars = saved;
        ok
    }

    fn occurs(&self, v: VarId, ty: Ty) -> bool {
        let ty = self.shallow(ty);
        match self.data(ty) {
            TyData::Var(w) => *w == v,
            TyData::Adt { args, .. } => args.iter().any(|t| self.occurs(v, *t)),
            TyData::Tuple(items) => items.iter().any(|t| self.occurs(v, *t)),
            TyData::Array { element, .. } | TyData::Slice(element) => self.occurs(v, *element),
            TyData::Ref { pointee, .. } => self.occurs(v, *pointee),
            TyData::Managed(inner) | TyData::Desc(inner) => self.occurs(v, *inner),
            TyData::Opaque(
                OpaqueKind::OwnedStore(elem)
                | OpaqueKind::Mutex(elem)
                | OpaqueKind::MutexGuard(elem)
                | OpaqueKind::Task(elem)
                | OpaqueKind::AsyncRegistration(elem),
            ) => self.occurs(v, *elem),
            TyData::Opaque(OpaqueKind::ForeignCallback { params, result }) => {
                params.iter().any(|t| self.occurs(v, *t)) || self.occurs(v, *result)
            }
            TyData::Opaque(OpaqueKind::ByteBuffer | OpaqueKind::ForeignHandle) => false,
            TyData::ErrorUnion { ok, err } => self.occurs(v, *ok) || self.occurs(v, *err),
            TyData::FnPtr { params, result }
            | TyData::Closure { params, result, .. }
            | TyData::ImplCallable { params, result, .. } => {
                params.iter().any(|t| self.occurs(v, *t)) || self.occurs(v, *result)
            }
            TyData::Assoc { base, .. } => self.occurs(v, *base),
            TyData::Unit
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Dyn(_)
            | TyData::OwnedDyn(_)
            | TyData::Param(_)
            | TyData::SelfParam
            | TyData::Error => false,
        }
    }

    fn bind(&mut self, v: VarId, ty: Ty) -> Result<(), Mismatch> {
        let (kind, _) = self.vars[v.0 as usize];
        let target = self.data(ty).clone();
        let ok = match (kind, target) {
            (VarKind::General, _) => true,
            (VarKind::Int, TyData::Int(_)) | (VarKind::Float, TyData::Float(_)) => true,
            (_, TyData::Error) => true,
            (VarKind::Int | VarKind::Float, TyData::Var(w)) => {
                // A literal variable unified with a general one transfers
                // its kind; two literal kinds must agree.
                let (wk, _) = self.vars[w.0 as usize];
                match wk {
                    VarKind::General => {
                        self.vars[w.0 as usize] = (kind, None);
                        true
                    }
                    VarKind::Int | VarKind::Float => wk == kind,
                }
            }
            (VarKind::Int | VarKind::Float, _) => false,
        };
        if !ok || self.occurs(v, ty) {
            return Err(Mismatch);
        }
        self.vars[v.0 as usize].1 = Some(ty);
        Ok(())
    }

    /// Equality constraint.
    pub fn unify(&mut self, a: Ty, b: Ty) -> Result<(), Mismatch> {
        let a = self.shallow(a);
        let b = self.shallow(b);
        if a == b {
            return Ok(());
        }
        match (self.data(a).clone(), self.data(b).clone()) {
            (TyData::Error, _) | (_, TyData::Error) => Ok(()),
            (TyData::Var(v), _) => self.bind(v, b),
            (_, TyData::Var(v)) => self.bind(v, a),
            (TyData::Adt { adt: x, args: xa }, TyData::Adt { adt: y, args: ya }) if x == y => {
                self.unify_all(&xa, &ya)
            }
            (TyData::Tuple(xs), TyData::Tuple(ys)) => self.unify_all(&xs, &ys),
            (
                TyData::Array {
                    element: x,
                    length: n,
                },
                TyData::Array {
                    element: y,
                    length: m,
                },
            ) if n == m => self.unify(x, y),
            (
                TyData::Ref {
                    mutable: m1,
                    pointee: x,
                },
                TyData::Ref {
                    mutable: m2,
                    pointee: y,
                },
            ) if m1 == m2 => self.unify(x, y),
            (TyData::Managed(x), TyData::Managed(y))
            | (TyData::Desc(x), TyData::Desc(y))
            | (TyData::Slice(x), TyData::Slice(y)) => self.unify(x, y),
            (
                TyData::Opaque(OpaqueKind::OwnedStore(x)),
                TyData::Opaque(OpaqueKind::OwnedStore(y)),
            )
            | (TyData::Opaque(OpaqueKind::Mutex(x)), TyData::Opaque(OpaqueKind::Mutex(y)))
            | (
                TyData::Opaque(OpaqueKind::MutexGuard(x)),
                TyData::Opaque(OpaqueKind::MutexGuard(y)),
            )
            | (TyData::Opaque(OpaqueKind::Task(x)), TyData::Opaque(OpaqueKind::Task(y)))
            | (
                TyData::Opaque(OpaqueKind::AsyncRegistration(x)),
                TyData::Opaque(OpaqueKind::AsyncRegistration(y)),
            ) => self.unify(x, y),
            (
                TyData::Opaque(OpaqueKind::ForeignCallback {
                    params: p1,
                    result: r1,
                }),
                TyData::Opaque(OpaqueKind::ForeignCallback {
                    params: p2,
                    result: r2,
                }),
            ) => {
                self.unify_all(&p1, &p2)?;
                self.unify(r1, r2)
            }
            (
                TyData::FnPtr {
                    params: p1,
                    result: r1,
                },
                TyData::FnPtr {
                    params: p2,
                    result: r2,
                },
            ) => {
                self.unify_all(&p1, &p2)?;
                self.unify(r1, r2)
            }
            (TyData::ErrorUnion { ok: o1, err: e1 }, TyData::ErrorUnion { ok: o2, err: e2 }) => {
                self.unify(o1, o2)?;
                self.unify(e1, e2)
            }
            (TyData::Assoc { base: b1, name: n1 }, TyData::Assoc { base: b2, name: n2 })
                if n1 == n2 =>
            {
                self.unify(b1, b2)
            }
            _ => Err(Mismatch),
        }
    }

    fn unify_all(&mut self, xs: &[Ty], ys: &[Ty]) -> Result<(), Mismatch> {
        if xs.len() != ys.len() {
            return Err(Mismatch);
        }
        for (x, y) in xs.iter().zip(ys) {
            self.unify(*x, *y)?;
        }
        Ok(())
    }

    /// Coercion site: `found` flows into a slot of type `expected`. The
    /// only coercion is from `Never`.
    pub fn coerce(&mut self, found: Ty, expected: Ty) -> Result<(), Mismatch> {
        if self.is_never(found) {
            return Ok(());
        }
        self.unify(found, expected)
    }

    /// Deep resolution. Literal variables take their default type; an
    /// unconstrained general variable is returned as `None`.
    pub fn zonk(&mut self, ty: Ty) -> Option<Ty> {
        let ty = self.shallow(ty);
        let data = self.data(ty).clone();
        Some(match data {
            TyData::Var(v) => match self.vars[v.0 as usize].0 {
                VarKind::General => return None,
                VarKind::Int => {
                    let t = self.int(IntTy::I32);
                    self.vars[v.0 as usize].1 = Some(t);
                    t
                }
                VarKind::Float => {
                    let t = self.intern(TyData::Float(FloatTy::F64));
                    self.vars[v.0 as usize].1 = Some(t);
                    t
                }
            },
            TyData::Adt { adt, args } => {
                let args = args
                    .iter()
                    .map(|t| self.zonk(*t))
                    .collect::<Option<Vec<_>>>()?;
                self.intern(TyData::Adt { adt, args })
            }
            TyData::Tuple(items) => {
                let items = items
                    .iter()
                    .map(|t| self.zonk(*t))
                    .collect::<Option<Vec<_>>>()?;
                self.intern(TyData::Tuple(items))
            }
            TyData::Array { element, length } => {
                let element = self.zonk(element)?;
                self.intern(TyData::Array { element, length })
            }
            TyData::Slice(element) => {
                let element = self.zonk(element)?;
                self.intern(TyData::Slice(element))
            }
            TyData::Ref { mutable, pointee } => {
                let pointee = self.zonk(pointee)?;
                self.intern(TyData::Ref { mutable, pointee })
            }
            TyData::Managed(inner) => {
                let inner = self.zonk(inner)?;
                self.intern(TyData::Managed(inner))
            }
            TyData::Desc(inner) => {
                let inner = self.zonk(inner)?;
                self.intern(TyData::Desc(inner))
            }
            TyData::Opaque(OpaqueKind::OwnedStore(elem)) => {
                let elem = self.zonk(elem)?;
                self.intern(TyData::Opaque(OpaqueKind::OwnedStore(elem)))
            }
            TyData::Opaque(OpaqueKind::Mutex(elem)) => {
                let elem = self.zonk(elem)?;
                self.intern(TyData::Opaque(OpaqueKind::Mutex(elem)))
            }
            TyData::Opaque(OpaqueKind::MutexGuard(elem)) => {
                let elem = self.zonk(elem)?;
                self.intern(TyData::Opaque(OpaqueKind::MutexGuard(elem)))
            }
            TyData::Opaque(OpaqueKind::Task(elem)) => {
                let elem = self.zonk(elem)?;
                self.intern(TyData::Opaque(OpaqueKind::Task(elem)))
            }
            TyData::Opaque(OpaqueKind::AsyncRegistration(elem)) => {
                let elem = self.zonk(elem)?;
                self.intern(TyData::Opaque(OpaqueKind::AsyncRegistration(elem)))
            }
            TyData::Opaque(OpaqueKind::ForeignCallback { params, result }) => {
                let params = params
                    .iter()
                    .map(|t| self.zonk(*t))
                    .collect::<Option<Vec<_>>>()?;
                let result = self.zonk(result)?;
                self.intern(TyData::Opaque(OpaqueKind::ForeignCallback {
                    params,
                    result,
                }))
            }
            TyData::Opaque(OpaqueKind::ByteBuffer | OpaqueKind::ForeignHandle) => ty,
            TyData::ErrorUnion { ok, err } => {
                let ok = self.zonk(ok)?;
                let err = self.zonk(err)?;
                self.intern(TyData::ErrorUnion { ok, err })
            }
            TyData::FnPtr { params, result } => {
                let params = params
                    .iter()
                    .map(|t| self.zonk(*t))
                    .collect::<Option<Vec<_>>>()?;
                let result = self.zonk(result)?;
                self.intern(TyData::FnPtr { params, result })
            }
            TyData::Closure {
                owner,
                id,
                kind,
                params,
                result,
            } => {
                let params = params
                    .iter()
                    .map(|t| self.zonk(*t))
                    .collect::<Option<Vec<_>>>()?;
                let result = self.zonk(result)?;
                self.intern(TyData::Closure {
                    owner,
                    id,
                    kind,
                    params,
                    result,
                })
            }
            TyData::ImplCallable {
                owner,
                kind,
                params,
                result,
            } => {
                let params = params
                    .iter()
                    .map(|t| self.zonk(*t))
                    .collect::<Option<Vec<_>>>()?;
                let result = self.zonk(result)?;
                self.intern(TyData::ImplCallable {
                    owner,
                    kind,
                    params,
                    result,
                })
            }
            TyData::Assoc { base, name } => {
                let base = self.zonk(base)?;
                self.intern(TyData::Assoc { base, name })
            }
            TyData::Unit
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Dyn(_)
            | TyData::OwnedDyn(_)
            | TyData::Param(_)
            | TyData::SelfParam
            | TyData::Error => ty,
        })
    }

    /// Replaces `Param(i)` with `args[i]` and `SelfParam` with `self_ty`.
    pub fn subst_with_self(&mut self, ty: Ty, args: &[Ty], self_ty: Ty) -> Ty {
        let data = self.data(ty).clone();
        match data {
            TyData::SelfParam => self_ty,
            TyData::Param(i) => args.get(i as usize).copied().unwrap_or(ty),
            TyData::Adt { adt, args: inner } => {
                let inner = inner
                    .iter()
                    .map(|t| self.subst_with_self(*t, args, self_ty))
                    .collect();
                self.intern(TyData::Adt { adt, args: inner })
            }
            TyData::Tuple(items) => {
                let items = items
                    .iter()
                    .map(|t| self.subst_with_self(*t, args, self_ty))
                    .collect();
                self.intern(TyData::Tuple(items))
            }
            TyData::Array { element, length } => {
                let element = self.subst_with_self(element, args, self_ty);
                self.intern(TyData::Array { element, length })
            }
            TyData::Slice(element) => {
                let element = self.subst_with_self(element, args, self_ty);
                self.intern(TyData::Slice(element))
            }
            TyData::Ref { mutable, pointee } => {
                let pointee = self.subst_with_self(pointee, args, self_ty);
                self.intern(TyData::Ref { mutable, pointee })
            }
            TyData::Managed(inner) => {
                let inner = self.subst_with_self(inner, args, self_ty);
                self.intern(TyData::Managed(inner))
            }
            TyData::Desc(inner) => {
                let inner = self.subst_with_self(inner, args, self_ty);
                self.intern(TyData::Desc(inner))
            }
            TyData::Opaque(OpaqueKind::OwnedStore(elem)) => {
                let elem = self.subst_with_self(elem, args, self_ty);
                self.intern(TyData::Opaque(OpaqueKind::OwnedStore(elem)))
            }
            TyData::Opaque(OpaqueKind::Mutex(elem)) => {
                let elem = self.subst_with_self(elem, args, self_ty);
                self.intern(TyData::Opaque(OpaqueKind::Mutex(elem)))
            }
            TyData::Opaque(OpaqueKind::MutexGuard(elem)) => {
                let elem = self.subst_with_self(elem, args, self_ty);
                self.intern(TyData::Opaque(OpaqueKind::MutexGuard(elem)))
            }
            TyData::Opaque(OpaqueKind::Task(elem)) => {
                let elem = self.subst_with_self(elem, args, self_ty);
                self.intern(TyData::Opaque(OpaqueKind::Task(elem)))
            }
            TyData::Opaque(OpaqueKind::AsyncRegistration(elem)) => {
                let elem = self.subst_with_self(elem, args, self_ty);
                self.intern(TyData::Opaque(OpaqueKind::AsyncRegistration(elem)))
            }
            TyData::Opaque(OpaqueKind::ForeignCallback { params, result }) => {
                let params = params
                    .iter()
                    .map(|t| self.subst_with_self(*t, args, self_ty))
                    .collect();
                let result = self.subst_with_self(result, args, self_ty);
                self.intern(TyData::Opaque(OpaqueKind::ForeignCallback {
                    params,
                    result,
                }))
            }
            TyData::Opaque(OpaqueKind::ByteBuffer | OpaqueKind::ForeignHandle) => ty,
            TyData::ErrorUnion { ok, err } => {
                let ok = self.subst_with_self(ok, args, self_ty);
                let err = self.subst_with_self(err, args, self_ty);
                self.intern(TyData::ErrorUnion { ok, err })
            }
            TyData::FnPtr { params, result } => {
                let params = params
                    .iter()
                    .map(|t| self.subst_with_self(*t, args, self_ty))
                    .collect();
                let result = self.subst_with_self(result, args, self_ty);
                self.intern(TyData::FnPtr { params, result })
            }
            TyData::Closure {
                owner,
                id,
                kind,
                params,
                result,
            } => {
                let params = params
                    .iter()
                    .map(|t| self.subst_with_self(*t, args, self_ty))
                    .collect();
                let result = self.subst_with_self(result, args, self_ty);
                self.intern(TyData::Closure {
                    owner,
                    id,
                    kind,
                    params,
                    result,
                })
            }
            TyData::ImplCallable {
                owner,
                kind,
                params,
                result,
            } => {
                let params = params
                    .iter()
                    .map(|t| self.subst_with_self(*t, args, self_ty))
                    .collect();
                let result = self.subst_with_self(result, args, self_ty);
                self.intern(TyData::ImplCallable {
                    owner,
                    kind,
                    params,
                    result,
                })
            }
            TyData::Assoc { base, name } => {
                let base = self.subst_with_self(base, args, self_ty);
                self.intern(TyData::Assoc { base, name })
            }
            TyData::Unit
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Dyn(_)
            | TyData::OwnedDyn(_)
            | TyData::Var(_)
            | TyData::Error => ty,
        }
    }

    /// Replaces `Param(i)` with `args[i]`.
    pub fn subst(&mut self, ty: Ty, args: &[Ty]) -> Ty {
        let data = self.data(ty).clone();
        match data {
            TyData::Param(i) => args.get(i as usize).copied().unwrap_or(ty),
            TyData::Adt { adt, args: inner } => {
                let inner = inner.iter().map(|t| self.subst(*t, args)).collect();
                self.intern(TyData::Adt { adt, args: inner })
            }
            TyData::Tuple(items) => {
                let items = items.iter().map(|t| self.subst(*t, args)).collect();
                self.intern(TyData::Tuple(items))
            }
            TyData::Array { element, length } => {
                let element = self.subst(element, args);
                self.intern(TyData::Array { element, length })
            }
            TyData::Slice(element) => {
                let element = self.subst(element, args);
                self.intern(TyData::Slice(element))
            }
            TyData::Ref { mutable, pointee } => {
                let pointee = self.subst(pointee, args);
                self.intern(TyData::Ref { mutable, pointee })
            }
            TyData::Managed(inner) => {
                let inner = self.subst(inner, args);
                self.intern(TyData::Managed(inner))
            }
            TyData::Desc(inner) => {
                let inner = self.subst(inner, args);
                self.intern(TyData::Desc(inner))
            }
            TyData::Opaque(OpaqueKind::OwnedStore(elem)) => {
                let elem = self.subst(elem, args);
                self.intern(TyData::Opaque(OpaqueKind::OwnedStore(elem)))
            }
            TyData::Opaque(OpaqueKind::Mutex(elem)) => {
                let elem = self.subst(elem, args);
                self.intern(TyData::Opaque(OpaqueKind::Mutex(elem)))
            }
            TyData::Opaque(OpaqueKind::MutexGuard(elem)) => {
                let elem = self.subst(elem, args);
                self.intern(TyData::Opaque(OpaqueKind::MutexGuard(elem)))
            }
            TyData::Opaque(OpaqueKind::Task(elem)) => {
                let elem = self.subst(elem, args);
                self.intern(TyData::Opaque(OpaqueKind::Task(elem)))
            }
            TyData::Opaque(OpaqueKind::AsyncRegistration(elem)) => {
                let elem = self.subst(elem, args);
                self.intern(TyData::Opaque(OpaqueKind::AsyncRegistration(elem)))
            }
            TyData::Opaque(OpaqueKind::ForeignCallback { params, result }) => {
                let params = params.iter().map(|t| self.subst(*t, args)).collect();
                let result = self.subst(result, args);
                self.intern(TyData::Opaque(OpaqueKind::ForeignCallback {
                    params,
                    result,
                }))
            }
            TyData::Opaque(OpaqueKind::ByteBuffer | OpaqueKind::ForeignHandle) => ty,
            TyData::ErrorUnion { ok, err } => {
                let ok = self.subst(ok, args);
                let err = self.subst(err, args);
                self.intern(TyData::ErrorUnion { ok, err })
            }
            TyData::FnPtr { params, result } => {
                let params = params.iter().map(|t| self.subst(*t, args)).collect();
                let result = self.subst(result, args);
                self.intern(TyData::FnPtr { params, result })
            }
            TyData::Closure {
                owner,
                id,
                kind,
                params,
                result,
            } => {
                let params = params.iter().map(|t| self.subst(*t, args)).collect();
                let result = self.subst(result, args);
                self.intern(TyData::Closure {
                    owner,
                    id,
                    kind,
                    params,
                    result,
                })
            }
            TyData::ImplCallable {
                owner,
                kind,
                params,
                result,
            } => {
                let params = params.iter().map(|t| self.subst(*t, args)).collect();
                let result = self.subst(result, args);
                self.intern(TyData::ImplCallable {
                    owner,
                    kind,
                    params,
                    result,
                })
            }
            TyData::Assoc { base, name } => {
                let base = self.subst(base, args);
                self.intern(TyData::Assoc { base, name })
            }
            TyData::Unit
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Dyn(_)
            | TyData::OwnedDyn(_)
            | TyData::SelfParam
            | TyData::Var(_)
            | TyData::Error => ty,
        }
    }
}
