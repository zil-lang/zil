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
pub struct Ty(pub(crate) u32);

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
    Task { result: Ty, context: Option<Ty> },
    Registration(Ty),
    AsyncRegistration(Ty),
    ForeignHandle,
    ForeignCallback { params: Vec<Ty>, result: Ty },
    SyncCell(zil_ir::body::SyncCellKind),
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
        regions: Vec<Option<u32>>,
    },
    Tuple(Vec<Ty>),
    /// Anonymous tagged sum `A | B`: flattened, duplicate-free, canonically sorted.
    Union(Vec<Ty>),
    Array {
        element: Ty,
        length: u32,
    },
    Slice(Ty),
    /// An address window. `region` carries a declared function region parameter
    /// index (`life a` -> `Some(0)`) for region-contract checking; unannotated
    /// references have `region: None` (erased to `Region::Static` in body IR).
    Ref {
        region: Option<u32>,
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
        owner_args: Vec<Ty>,
        kind: ClosureKind,
        params: Vec<Ty>,
        result: Ty,
    },
    /// Return-position `impl Fn/FnMut/FnOnce` of `owner`.
    ImplCallable {
        owner: FnRef,
        owner_args: Vec<Ty>,
        kind: ClosureKind,
        params: Vec<Ty>,
        result: Ty,
    },
    /// `dyn Trait` unsized pointee.
    Dyn(TraitRef),
    /// `OwnedDyn<dyn Trait>` or `OwnedDyn<dyn Trait, Context<'a>>` affine owner.
    OwnedDyn {
        trait_ref: TraitRef,
        context: Option<Ty>,
    },
    /// The n-th type parameter of the enclosing generic declaration.
    Param(u32),
    /// `Self` inside a trait method signature.
    SelfParam,
    /// Associated type projection (`I.Item`, `M.Output`, `Self.Item`).
    Assoc {
        base: Ty,
        trait_ref: Option<TraitRef>,
        name: String,
    },
    /// Fixed-size array whose length is a const generic parameter or inference variable.
    ArrayConst {
        element: Ty,
        length: Ty,
    },
    /// The n-th generic parameter when it is a `const` integer parameter.
    ConstParam(u32),
    /// Concrete instantiated integer const generic argument.
    ConstU32(u32),
    /// Compile-time first-class `Type` value (carries real interned `Ty` identity).
    Type,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnionError {
    TooFew,
    Duplicate(Ty),
}

fn int_rank(i: IntTy) -> u8 {
    match i {
        IntTy::I8 => 0,
        IntTy::I16 => 1,
        IntTy::I32 => 2,
        IntTy::I64 => 3,
        IntTy::Isize => 4,
        IntTy::U8 => 5,
        IntTy::U16 => 6,
        IntTy::U32 => 7,
        IntTy::U64 => 8,
        IntTy::Usize => 9,
    }
}

fn float_rank(f: FloatTy) -> u8 {
    match f {
        FloatTy::F32 => 0,
        FloatTy::F64 => 1,
    }
}

fn closure_kind_rank(k: ClosureKind) -> u8 {
    match k {
        ClosureKind::Fn => 0,
        ClosureKind::FnMut => 1,
        ClosureKind::FnOnce => 2,
    }
}

fn sync_cell_rank(k: zil_ir::body::SyncCellKind) -> u8 {
    match k {
        zil_ir::body::SyncCellKind::I32 => 0,
        zil_ir::body::SyncCellKind::I64 => 1,
        zil_ir::body::SyncCellKind::Usize => 2,
    }
}

fn ty_data_rank(d: &TyData) -> u8 {
    match d {
        TyData::Unit => 0,
        TyData::Bool => 1,
        TyData::Never => 2,
        TyData::Int(_) => 3,
        TyData::Float(_) => 4,
        TyData::Adt { .. } => 5,
        TyData::Tuple(_) => 6,
        TyData::Union(_) => 7,
        TyData::Array { .. } => 8,
        TyData::Slice(_) => 9,
        TyData::Ref { .. } => 10,
        TyData::Managed(_) => 11,
        TyData::Desc(_) => 12,
        TyData::Opaque(_) => 13,
        TyData::ErrorUnion { .. } => 14,
        TyData::FnPtr { .. } => 15,
        TyData::Closure { .. } => 16,
        TyData::ImplCallable { .. } => 17,
        TyData::Dyn(_) => 18,
        TyData::OwnedDyn { .. } => 19,
        TyData::Param(_) => 20,
        TyData::SelfParam => 21,
        TyData::Assoc { .. } => 22,
        TyData::ArrayConst { .. } => 23,
        TyData::ConstParam(_) => 24,
        TyData::ConstU32(_) => 25,
        TyData::Type => 26,
        TyData::Var(_) => 27,
        TyData::Error => 28,
    }
}

fn opaque_rank(o: &OpaqueKind) -> u8 {
    match o {
        OpaqueKind::ByteBuffer => 0,
        OpaqueKind::OwnedStore(_) => 1,
        OpaqueKind::Mutex(_) => 2,
        OpaqueKind::MutexGuard(_) => 3,
        OpaqueKind::Task { .. } => 4,
        OpaqueKind::Registration(_) => 5,
        OpaqueKind::AsyncRegistration(_) => 6,
        OpaqueKind::ForeignHandle => 7,
        OpaqueKind::ForeignCallback { .. } => 8,
        OpaqueKind::SyncCell(_) => 9,
    }
}

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
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn unit(&mut self) -> Ty {
        self.intern(TyData::Unit)
    }
    pub fn meta_type(&mut self) -> Ty {
        self.intern(TyData::Type)
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

    /// Deterministic total order over complete type identity (independent of
    /// interning order or source spelling).
    pub fn cmp_ty(&self, a: Ty, b: Ty) -> std::cmp::Ordering {
        self.cmp_ty_depth(a, b, 0)
    }

    fn cmp_ty_slice(&self, xs: &[Ty], ys: &[Ty], depth: u32) -> std::cmp::Ordering {
        xs.len().cmp(&ys.len()).then_with(|| {
            for (&x, &y) in xs.iter().zip(ys.iter()) {
                let ord = self.cmp_ty_depth(x, y, depth + 1);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            std::cmp::Ordering::Equal
        })
    }

    fn cmp_opt_ty(&self, a: Option<Ty>, b: Option<Ty>, depth: u32) -> std::cmp::Ordering {
        match (a, b) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(x), Some(y)) => self.cmp_ty_depth(x, y, depth + 1),
        }
    }

    fn cmp_ty_depth(&self, a: Ty, b: Ty, depth: u32) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        let a = self.shallow(a);
        let b = self.shallow(b);
        if a == b || depth > 64 {
            return a.cmp(&b);
        }
        let da = self.data(a);
        let db = self.data(b);
        let ra = ty_data_rank(da);
        let rb = ty_data_rank(db);
        if ra != rb {
            return ra.cmp(&rb);
        }
        match (da, db) {
            (TyData::Unit, TyData::Unit)
            | (TyData::Bool, TyData::Bool)
            | (TyData::Never, TyData::Never)
            | (TyData::SelfParam, TyData::SelfParam)
            | (TyData::Type, TyData::Type)
            | (TyData::Error, TyData::Error) => Ordering::Equal,
            (TyData::Int(x), TyData::Int(y)) => int_rank(*x).cmp(&int_rank(*y)),
            (TyData::Float(x), TyData::Float(y)) => float_rank(*x).cmp(&float_rank(*y)),
            (
                TyData::Adt {
                    adt: a1,
                    args: args1,
                    regions: r1,
                },
                TyData::Adt {
                    adt: a2,
                    args: args2,
                    regions: r2,
                },
            ) => a1
                .cmp(a2)
                .then_with(|| self.cmp_ty_slice(args1, args2, depth))
                .then_with(|| r1.cmp(r2)),
            (TyData::Tuple(xs), TyData::Tuple(ys)) | (TyData::Union(xs), TyData::Union(ys)) => {
                self.cmp_ty_slice(xs, ys, depth)
            }
            (
                TyData::Array {
                    element: e1,
                    length: l1,
                },
                TyData::Array {
                    element: e2,
                    length: l2,
                },
            ) => self
                .cmp_ty_depth(*e1, *e2, depth + 1)
                .then_with(|| l1.cmp(l2)),
            (TyData::Slice(e1), TyData::Slice(e2))
            | (TyData::Managed(e1), TyData::Managed(e2))
            | (TyData::Desc(e1), TyData::Desc(e2)) => self.cmp_ty_depth(*e1, *e2, depth + 1),
            (
                TyData::Ref {
                    region: r1,
                    mutable: m1,
                    pointee: p1,
                },
                TyData::Ref {
                    region: r2,
                    mutable: m2,
                    pointee: p2,
                },
            ) => m1
                .cmp(m2)
                .then_with(|| self.cmp_ty_depth(*p1, *p2, depth + 1))
                .then_with(|| r1.cmp(r2)),
            (TyData::Opaque(o1), TyData::Opaque(o2)) => {
                let ro1 = opaque_rank(o1);
                let ro2 = opaque_rank(o2);
                if ro1 != ro2 {
                    return ro1.cmp(&ro2);
                }
                match (o1, o2) {
                    (OpaqueKind::ByteBuffer, OpaqueKind::ByteBuffer)
                    | (OpaqueKind::ForeignHandle, OpaqueKind::ForeignHandle) => Ordering::Equal,
                    (OpaqueKind::OwnedStore(e1), OpaqueKind::OwnedStore(e2))
                    | (OpaqueKind::Mutex(e1), OpaqueKind::Mutex(e2))
                    | (OpaqueKind::MutexGuard(e1), OpaqueKind::MutexGuard(e2))
                    | (OpaqueKind::Registration(e1), OpaqueKind::Registration(e2))
                    | (OpaqueKind::AsyncRegistration(e1), OpaqueKind::AsyncRegistration(e2)) => {
                        self.cmp_ty_depth(*e1, *e2, depth + 1)
                    }
                    (
                        OpaqueKind::Task {
                            result: r1,
                            context: c1,
                        },
                        OpaqueKind::Task {
                            result: r2,
                            context: c2,
                        },
                    ) => self
                        .cmp_ty_depth(*r1, *r2, depth + 1)
                        .then_with(|| self.cmp_opt_ty(*c1, *c2, depth)),
                    (
                        OpaqueKind::ForeignCallback {
                            params: p1,
                            result: r1,
                        },
                        OpaqueKind::ForeignCallback {
                            params: p2,
                            result: r2,
                        },
                    ) => self
                        .cmp_ty_slice(p1, p2, depth)
                        .then_with(|| self.cmp_ty_depth(*r1, *r2, depth + 1)),
                    (OpaqueKind::SyncCell(k1), OpaqueKind::SyncCell(k2)) => {
                        sync_cell_rank(*k1).cmp(&sync_cell_rank(*k2))
                    }
                    _ => unreachable!("same opaque_rank"),
                }
            }
            (TyData::ErrorUnion { ok: o1, err: e1 }, TyData::ErrorUnion { ok: o2, err: e2 }) => {
                self.cmp_ty_depth(*o1, *o2, depth + 1)
                    .then_with(|| self.cmp_ty_depth(*e1, *e2, depth + 1))
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
            ) => self
                .cmp_ty_slice(p1, p2, depth)
                .then_with(|| self.cmp_ty_depth(*r1, *r2, depth + 1)),
            (
                TyData::Closure {
                    owner: o1,
                    id: id1,
                    owner_args: oa1,
                    kind: k1,
                    params: p1,
                    result: r1,
                },
                TyData::Closure {
                    owner: o2,
                    id: id2,
                    owner_args: oa2,
                    kind: k2,
                    params: p2,
                    result: r2,
                },
            ) => {
                o1.0.cmp(&o2.0)
                    .then_with(|| id1.cmp(id2))
                    .then_with(|| closure_kind_rank(*k1).cmp(&closure_kind_rank(*k2)))
                    .then_with(|| self.cmp_ty_slice(oa1, oa2, depth))
                    .then_with(|| self.cmp_ty_slice(p1, p2, depth))
                    .then_with(|| self.cmp_ty_depth(*r1, *r2, depth + 1))
            }
            (
                TyData::ImplCallable {
                    owner: o1,
                    owner_args: oa1,
                    kind: k1,
                    params: p1,
                    result: r1,
                },
                TyData::ImplCallable {
                    owner: o2,
                    owner_args: oa2,
                    kind: k2,
                    params: p2,
                    result: r2,
                },
            ) => {
                o1.0.cmp(&o2.0)
                    .then_with(|| closure_kind_rank(*k1).cmp(&closure_kind_rank(*k2)))
                    .then_with(|| self.cmp_ty_slice(oa1, oa2, depth))
                    .then_with(|| self.cmp_ty_slice(p1, p2, depth))
                    .then_with(|| self.cmp_ty_depth(*r1, *r2, depth + 1))
            }
            (TyData::Dyn(t1), TyData::Dyn(t2)) => t1.cmp(t2),
            (
                TyData::OwnedDyn {
                    trait_ref: t1,
                    context: c1,
                },
                TyData::OwnedDyn {
                    trait_ref: t2,
                    context: c2,
                },
            ) => t1.cmp(t2).then_with(|| self.cmp_opt_ty(*c1, *c2, depth)),
            (TyData::Param(i), TyData::Param(j))
            | (TyData::ConstParam(i), TyData::ConstParam(j))
            | (TyData::ConstU32(i), TyData::ConstU32(j)) => i.cmp(j),
            (
                TyData::Assoc {
                    base: b1,
                    trait_ref: t1,
                    name: n1,
                },
                TyData::Assoc {
                    base: b2,
                    trait_ref: t2,
                    name: n2,
                },
            ) => self
                .cmp_ty_depth(*b1, *b2, depth + 1)
                .then_with(|| t1.cmp(t2))
                .then_with(|| n1.cmp(n2)),
            (
                TyData::ArrayConst {
                    element: e1,
                    length: l1,
                },
                TyData::ArrayConst {
                    element: e2,
                    length: l2,
                },
            ) => self
                .cmp_ty_depth(*e1, *e2, depth + 1)
                .then_with(|| self.cmp_ty_depth(*l1, *l2, depth + 1)),
            (TyData::Var(v1), TyData::Var(v2)) => v1.cmp(v2),
            _ => unreachable!("same ty_data_rank"),
        }
    }

    fn flatten_union_members(&self, members: Vec<Ty>) -> Vec<Ty> {
        let mut flat = Vec::with_capacity(members.len());
        let mut stack: Vec<Ty> = members.into_iter().rev().collect();
        while let Some(m) = stack.pop() {
            let m = self.shallow(m);
            if let TyData::Union(inner) = self.data(m) {
                for &sub in inner.iter().rev() {
                    stack.push(sub);
                }
            } else {
                flat.push(m);
            }
        }
        flat
    }

    /// Canonicalizes an anonymous union `A | B`: flattens nested unions, sorts
    /// members by complete type identity, and rejects duplicate members or `< 2`
    /// members.
    pub fn make_union(&mut self, members: Vec<Ty>) -> Result<Ty, UnionError> {
        let mut flat = self.flatten_union_members(members);
        flat.sort_by(|&a, &b| self.cmp_ty(a, b));
        for w in flat.windows(2) {
            if self.cmp_ty(w[0], w[1]) == std::cmp::Ordering::Equal {
                return Err(UnionError::Duplicate(w[0]));
            }
        }
        if flat.len() < 2 {
            return Err(UnionError::TooFew);
        }
        Ok(self.intern(TyData::Union(flat)))
    }

    /// Flattens and canonically sorts a union after substitution/zonking without
    /// silently dropping duplicates (so downstream checks can reject collapsed
    /// generic instantiations like `T | i32` with `T = i32`).
    pub fn canonicalize_union_raw(&mut self, members: Vec<Ty>) -> Ty {
        let mut flat = self.flatten_union_members(members);
        flat.sort_by(|&a, &b| self.cmp_ty(a, b));
        self.intern(TyData::Union(flat))
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
            TyData::Tuple(items) | TyData::Union(items) => items.iter().any(|t| self.occurs(v, *t)),
            TyData::Array { element, .. } | TyData::Slice(element) => self.occurs(v, *element),
            TyData::ArrayConst { element, length } => {
                self.occurs(v, *element) || self.occurs(v, *length)
            }
            TyData::Ref { pointee, .. } => self.occurs(v, *pointee),
            TyData::Managed(inner) | TyData::Desc(inner) => self.occurs(v, *inner),
            TyData::Opaque(
                OpaqueKind::OwnedStore(elem)
                | OpaqueKind::Mutex(elem)
                | OpaqueKind::MutexGuard(elem)
                | OpaqueKind::Registration(elem)
                | OpaqueKind::AsyncRegistration(elem),
            ) => self.occurs(v, *elem),
            TyData::Opaque(OpaqueKind::Task { result, context }) => {
                self.occurs(v, *result) || context.is_some_and(|c| self.occurs(v, c))
            }
            TyData::Opaque(OpaqueKind::ForeignCallback { params, result }) => {
                params.iter().any(|t| self.occurs(v, *t)) || self.occurs(v, *result)
            }
            TyData::Opaque(
                OpaqueKind::ByteBuffer | OpaqueKind::ForeignHandle | OpaqueKind::SyncCell(_),
            ) => false,
            TyData::ErrorUnion { ok, err } => self.occurs(v, *ok) || self.occurs(v, *err),
            TyData::FnPtr { params, result } => {
                params.iter().any(|t| self.occurs(v, *t)) || self.occurs(v, *result)
            }
            TyData::Closure {
                owner_args,
                params,
                result,
                ..
            }
            | TyData::ImplCallable {
                owner_args,
                params,
                result,
                ..
            } => {
                owner_args.iter().any(|t| self.occurs(v, *t))
                    || params.iter().any(|t| self.occurs(v, *t))
                    || self.occurs(v, *result)
            }
            TyData::Assoc { base, .. } => self.occurs(v, *base),
            TyData::Unit
            | TyData::Type
            | TyData::ConstU32(_)
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Dyn(_)
            | TyData::Param(_)
            | TyData::ConstParam(_)
            | TyData::SelfParam
            | TyData::Error => false,
            TyData::OwnedDyn { context, .. } => context.is_some_and(|c| self.occurs(v, c)),
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
            (
                TyData::Adt {
                    adt: x,
                    args: xa,
                    regions: rx,
                },
                TyData::Adt {
                    adt: y,
                    args: ya,
                    regions: ry,
                },
            ) if x == y
                && rx.iter().zip(&ry).all(|(a, b)| match (a, b) {
                    (Some(a), Some(b)) => a == b,
                    _ => true,
                }) =>
            {
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
                TyData::ArrayConst {
                    element: x,
                    length: l,
                },
                TyData::Array {
                    element: y,
                    length: m,
                },
            )
            | (
                TyData::Array {
                    element: y,
                    length: m,
                },
                TyData::ArrayConst {
                    element: x,
                    length: l,
                },
            ) => {
                self.unify(x, y)?;
                let cm = self.intern(TyData::ConstU32(m));
                self.unify(l, cm)
            }
            (
                TyData::ArrayConst {
                    element: x,
                    length: l1,
                },
                TyData::ArrayConst {
                    element: y,
                    length: l2,
                },
            ) => {
                self.unify(x, y)?;
                self.unify(l1, l2)
            }
            (
                TyData::Ref {
                    region: r1,
                    mutable: m1,
                    pointee: x,
                },
                TyData::Ref {
                    region: r2,
                    mutable: m2,
                    pointee: y,
                },
            ) if m1 == m2
                && match (r1, r2) {
                    (Some(a), Some(b)) => a == b,
                    _ => true,
                } =>
            {
                self.unify(x, y)
            }
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
            | (
                TyData::Opaque(OpaqueKind::Registration(x)),
                TyData::Opaque(OpaqueKind::Registration(y)),
            )
            | (
                TyData::Opaque(OpaqueKind::AsyncRegistration(x)),
                TyData::Opaque(OpaqueKind::AsyncRegistration(y)),
            ) => self.unify(x, y),
            (
                TyData::Opaque(OpaqueKind::Task {
                    result: r1,
                    context: c1,
                }),
                TyData::Opaque(OpaqueKind::Task {
                    result: r2,
                    context: c2,
                }),
            ) => {
                self.unify(r1, r2)?;
                match (c1, c2) {
                    (Some(a), Some(b)) => self.unify(a, b),
                    (None, None) | (None, Some(_)) | (Some(_), None) => Ok(()),
                }
            }
            (
                TyData::OwnedDyn {
                    trait_ref: t1,
                    context: c1,
                },
                TyData::OwnedDyn {
                    trait_ref: t2,
                    context: c2,
                },
            ) => {
                if t1 != t2 {
                    return Err(Mismatch);
                }
                match (c1, c2) {
                    (None, None) => Ok(()),
                    (Some(a), Some(b)) => self.unify(a, b),
                    (None, Some(_)) | (Some(_), None) => Err(Mismatch),
                }
            }
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
            (TyData::Union(xs), TyData::Union(ys)) => self.unify_all(&xs, &ys),
            (TyData::ErrorUnion { ok: o1, err: e1 }, TyData::ErrorUnion { ok: o2, err: e2 }) => {
                self.unify(o1, o2)?;
                self.unify(e1, e2)
            }
            (
                TyData::Assoc {
                    base: b1,
                    trait_ref: t1,
                    name: n1,
                },
                TyData::Assoc {
                    base: b2,
                    trait_ref: t2,
                    name: n2,
                },
            ) if n1 == n2 && (t1 == t2 || t1.is_none() || t2.is_none()) => self.unify(b1, b2),
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
            TyData::Adt { adt, args, regions } => {
                let args = args
                    .iter()
                    .map(|t| self.zonk(*t))
                    .collect::<Option<Vec<_>>>()?;
                self.intern(TyData::Adt { adt, args, regions })
            }
            TyData::Tuple(items) => {
                let items = items
                    .iter()
                    .map(|t| self.zonk(*t))
                    .collect::<Option<Vec<_>>>()?;
                self.intern(TyData::Tuple(items))
            }
            TyData::Union(items) => {
                let items = items
                    .iter()
                    .map(|t| self.zonk(*t))
                    .collect::<Option<Vec<_>>>()?;
                self.canonicalize_union_raw(items)
            }
            TyData::Array { element, length } => {
                let element = self.zonk(element)?;
                self.intern(TyData::Array { element, length })
            }
            TyData::ArrayConst { element, length } => {
                let element = self.zonk(element)?;
                let length = self.zonk(length)?;
                if let TyData::ConstU32(n) = *self.data(length) {
                    self.intern(TyData::Array { element, length: n })
                } else {
                    self.intern(TyData::ArrayConst { element, length })
                }
            }
            TyData::Slice(element) => {
                let element = self.zonk(element)?;
                self.intern(TyData::Slice(element))
            }
            TyData::Ref {
                region,
                mutable,
                pointee,
            } => {
                let pointee = self.zonk(pointee)?;
                self.intern(TyData::Ref {
                    region,
                    mutable,
                    pointee,
                })
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
            TyData::Opaque(OpaqueKind::Task { result, context }) => {
                let result = self.zonk(result)?;
                let context = match context {
                    Some(c) => Some(self.zonk(c)?),
                    None => None,
                };
                self.intern(TyData::Opaque(OpaqueKind::Task { result, context }))
            }
            TyData::Opaque(OpaqueKind::Registration(elem)) => {
                let elem = self.zonk(elem)?;
                self.intern(TyData::Opaque(OpaqueKind::Registration(elem)))
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
            TyData::Opaque(
                OpaqueKind::ByteBuffer | OpaqueKind::ForeignHandle | OpaqueKind::SyncCell(_),
            ) => ty,
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
                owner_args,
                kind,
                params,
                result,
            } => {
                let owner_args = owner_args
                    .iter()
                    .map(|t| self.zonk(*t))
                    .collect::<Option<Vec<_>>>()?;
                let params = params
                    .iter()
                    .map(|t| self.zonk(*t))
                    .collect::<Option<Vec<_>>>()?;
                let result = self.zonk(result)?;
                self.intern(TyData::Closure {
                    owner,
                    id,
                    owner_args,
                    kind,
                    params,
                    result,
                })
            }
            TyData::ImplCallable {
                owner,
                owner_args,
                kind,
                params,
                result,
            } => {
                let owner_args = owner_args
                    .iter()
                    .map(|t| self.zonk(*t))
                    .collect::<Option<Vec<_>>>()?;
                let params = params
                    .iter()
                    .map(|t| self.zonk(*t))
                    .collect::<Option<Vec<_>>>()?;
                let result = self.zonk(result)?;
                self.intern(TyData::ImplCallable {
                    owner,
                    owner_args,
                    kind,
                    params,
                    result,
                })
            }
            TyData::Assoc {
                base,
                trait_ref,
                name,
            } => {
                let base = self.zonk(base)?;
                self.intern(TyData::Assoc {
                    base,
                    trait_ref,
                    name,
                })
            }
            TyData::Unit
            | TyData::Type
            | TyData::ConstU32(_)
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Dyn(_)
            | TyData::Param(_)
            | TyData::ConstParam(_)
            | TyData::SelfParam
            | TyData::Error => ty,
            TyData::OwnedDyn { trait_ref, context } => {
                let context = match context {
                    Some(c) => Some(self.zonk(c)?),
                    None => None,
                };
                self.intern(TyData::OwnedDyn { trait_ref, context })
            }
        })
    }

    /// Replaces `Param(i)` with `args[i]` and `SelfParam` with `self_ty`.
    pub fn subst_with_self(&mut self, ty: Ty, args: &[Ty], self_ty: Ty) -> Ty {
        let data = self.data(ty).clone();
        match data {
            TyData::SelfParam => self_ty,
            TyData::Param(i) | TyData::ConstParam(i) => args.get(i as usize).copied().unwrap_or(ty),
            TyData::Adt {
                adt,
                args: inner,
                regions,
            } => {
                let inner = inner
                    .iter()
                    .map(|t| self.subst_with_self(*t, args, self_ty))
                    .collect();
                self.intern(TyData::Adt {
                    adt,
                    args: inner,
                    regions,
                })
            }
            TyData::Tuple(items) => {
                let items = items
                    .iter()
                    .map(|t| self.subst_with_self(*t, args, self_ty))
                    .collect();
                self.intern(TyData::Tuple(items))
            }
            TyData::Union(items) => {
                let items = items
                    .iter()
                    .map(|t| self.subst_with_self(*t, args, self_ty))
                    .collect();
                self.canonicalize_union_raw(items)
            }
            TyData::Array { element, length } => {
                let element = self.subst_with_self(element, args, self_ty);
                self.intern(TyData::Array { element, length })
            }
            TyData::ArrayConst { element, length } => {
                let element = self.subst_with_self(element, args, self_ty);
                let length = self.subst_with_self(length, args, self_ty);
                if let TyData::ConstU32(n) = *self.data(length) {
                    self.intern(TyData::Array { element, length: n })
                } else {
                    self.intern(TyData::ArrayConst { element, length })
                }
            }
            TyData::Slice(element) => {
                let element = self.subst_with_self(element, args, self_ty);
                self.intern(TyData::Slice(element))
            }
            TyData::Ref {
                region,
                mutable,
                pointee,
            } => {
                let pointee = self.subst_with_self(pointee, args, self_ty);
                self.intern(TyData::Ref {
                    region,
                    mutable,
                    pointee,
                })
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
            TyData::Opaque(OpaqueKind::Task { result, context }) => {
                let result = self.subst_with_self(result, args, self_ty);
                let context = context.map(|c| self.subst_with_self(c, args, self_ty));
                self.intern(TyData::Opaque(OpaqueKind::Task { result, context }))
            }
            TyData::Opaque(OpaqueKind::Registration(elem)) => {
                let elem = self.subst_with_self(elem, args, self_ty);
                self.intern(TyData::Opaque(OpaqueKind::Registration(elem)))
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
            TyData::Opaque(
                OpaqueKind::ByteBuffer | OpaqueKind::ForeignHandle | OpaqueKind::SyncCell(_),
            ) => ty,
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
                owner_args,
                kind,
                params,
                result,
            } => {
                let owner_args = owner_args
                    .iter()
                    .map(|t| self.subst_with_self(*t, args, self_ty))
                    .collect();
                let params = params
                    .iter()
                    .map(|t| self.subst_with_self(*t, args, self_ty))
                    .collect();
                let result = self.subst_with_self(result, args, self_ty);
                self.intern(TyData::Closure {
                    owner,
                    id,
                    owner_args,
                    kind,
                    params,
                    result,
                })
            }
            TyData::ImplCallable {
                owner,
                owner_args,
                kind,
                params,
                result,
            } => {
                let owner_args = owner_args
                    .iter()
                    .map(|t| self.subst_with_self(*t, args, self_ty))
                    .collect();
                let params = params
                    .iter()
                    .map(|t| self.subst_with_self(*t, args, self_ty))
                    .collect();
                let result = self.subst_with_self(result, args, self_ty);
                self.intern(TyData::ImplCallable {
                    owner,
                    owner_args,
                    kind,
                    params,
                    result,
                })
            }
            TyData::Assoc {
                base,
                trait_ref,
                name,
            } => {
                let base = self.subst_with_self(base, args, self_ty);
                self.intern(TyData::Assoc {
                    base,
                    trait_ref,
                    name,
                })
            }
            TyData::Unit
            | TyData::Type
            | TyData::ConstU32(_)
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Dyn(_)
            | TyData::Var(_)
            | TyData::Error => ty,
            TyData::OwnedDyn { trait_ref, context } => {
                let context = context.map(|c| self.subst_with_self(c, args, self_ty));
                self.intern(TyData::OwnedDyn { trait_ref, context })
            }
        }
    }

    /// Replaces `Param(i)` with `args[i]`.
    pub fn subst(&mut self, ty: Ty, args: &[Ty]) -> Ty {
        let data = self.data(ty).clone();
        match data {
            TyData::Param(i) | TyData::ConstParam(i) => args.get(i as usize).copied().unwrap_or(ty),
            TyData::Adt {
                adt,
                args: inner,
                regions,
            } => {
                let inner = inner.iter().map(|t| self.subst(*t, args)).collect();
                self.intern(TyData::Adt {
                    adt,
                    args: inner,
                    regions,
                })
            }
            TyData::Tuple(items) => {
                let items = items.iter().map(|t| self.subst(*t, args)).collect();
                self.intern(TyData::Tuple(items))
            }
            TyData::Union(items) => {
                let items = items.iter().map(|t| self.subst(*t, args)).collect();
                self.canonicalize_union_raw(items)
            }
            TyData::Array { element, length } => {
                let element = self.subst(element, args);
                self.intern(TyData::Array { element, length })
            }
            TyData::ArrayConst { element, length } => {
                let element = self.subst(element, args);
                let length = self.subst(length, args);
                if let TyData::ConstU32(n) = *self.data(length) {
                    self.intern(TyData::Array { element, length: n })
                } else {
                    self.intern(TyData::ArrayConst { element, length })
                }
            }
            TyData::Slice(element) => {
                let element = self.subst(element, args);
                self.intern(TyData::Slice(element))
            }
            TyData::Ref {
                region,
                mutable,
                pointee,
            } => {
                let pointee = self.subst(pointee, args);
                self.intern(TyData::Ref {
                    region,
                    mutable,
                    pointee,
                })
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
            TyData::Opaque(OpaqueKind::Task { result, context }) => {
                let result = self.subst(result, args);
                let context = context.map(|c| self.subst(c, args));
                self.intern(TyData::Opaque(OpaqueKind::Task { result, context }))
            }
            TyData::Opaque(OpaqueKind::Registration(elem)) => {
                let elem = self.subst(elem, args);
                self.intern(TyData::Opaque(OpaqueKind::Registration(elem)))
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
            TyData::Opaque(
                OpaqueKind::ByteBuffer | OpaqueKind::ForeignHandle | OpaqueKind::SyncCell(_),
            ) => ty,
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
                owner_args,
                kind,
                params,
                result,
            } => {
                let owner_args = owner_args.iter().map(|t| self.subst(*t, args)).collect();
                let params = params.iter().map(|t| self.subst(*t, args)).collect();
                let result = self.subst(result, args);
                self.intern(TyData::Closure {
                    owner,
                    id,
                    owner_args,
                    kind,
                    params,
                    result,
                })
            }
            TyData::ImplCallable {
                owner,
                owner_args,
                kind,
                params,
                result,
            } => {
                let owner_args = owner_args.iter().map(|t| self.subst(*t, args)).collect();
                let params = params.iter().map(|t| self.subst(*t, args)).collect();
                let result = self.subst(result, args);
                self.intern(TyData::ImplCallable {
                    owner,
                    owner_args,
                    kind,
                    params,
                    result,
                })
            }
            TyData::Assoc {
                base,
                trait_ref,
                name,
            } => {
                let base = self.subst(base, args);
                self.intern(TyData::Assoc {
                    base,
                    trait_ref,
                    name,
                })
            }
            TyData::Unit
            | TyData::Type
            | TyData::ConstU32(_)
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Dyn(_)
            | TyData::SelfParam
            | TyData::Var(_)
            | TyData::Error => ty,
            TyData::OwnedDyn { trait_ref, context } => {
                let context = context.map(|c| self.subst(c, args));
                self.intern(TyData::OwnedDyn { trait_ref, context })
            }
        }
    }

    /// Erases callee-bound region parameter indices (`Some(_)` -> `None`) when
    /// instantiating a callee's signature at a cross-function call site so the
    /// callee's region indices do not collide with the caller's rigid region
    /// parameters. (Local closures in the same function keep their rigid regions.)
    pub fn erase_regions(&mut self, ty: Ty) -> Ty {
        let ty = self.shallow(ty);
        let data = self.data(ty).clone();
        match data {
            TyData::Adt { adt, args, regions } => {
                let args = args.iter().map(|t| self.erase_regions(*t)).collect();
                let regions = vec![None; regions.len()];
                self.intern(TyData::Adt { adt, args, regions })
            }
            TyData::Tuple(items) => {
                let items = items.iter().map(|t| self.erase_regions(*t)).collect();
                self.intern(TyData::Tuple(items))
            }
            TyData::Union(items) => {
                let items = items.iter().map(|t| self.erase_regions(*t)).collect();
                self.canonicalize_union_raw(items)
            }
            TyData::Array { element, length } => {
                let element = self.erase_regions(element);
                self.intern(TyData::Array { element, length })
            }
            TyData::ArrayConst { element, length } => {
                let element = self.erase_regions(element);
                let length = self.erase_regions(length);
                self.intern(TyData::ArrayConst { element, length })
            }
            TyData::Slice(element) => {
                let element = self.erase_regions(element);
                self.intern(TyData::Slice(element))
            }
            TyData::Ref {
                mutable, pointee, ..
            } => {
                let pointee = self.erase_regions(pointee);
                self.intern(TyData::Ref {
                    region: None,
                    mutable,
                    pointee,
                })
            }
            TyData::Managed(inner) => {
                let inner = self.erase_regions(inner);
                self.intern(TyData::Managed(inner))
            }
            TyData::Desc(inner) => {
                let inner = self.erase_regions(inner);
                self.intern(TyData::Desc(inner))
            }
            TyData::Opaque(o) => {
                let o = match o {
                    OpaqueKind::OwnedStore(e) => OpaqueKind::OwnedStore(self.erase_regions(e)),
                    OpaqueKind::Mutex(e) => OpaqueKind::Mutex(self.erase_regions(e)),
                    OpaqueKind::MutexGuard(e) => OpaqueKind::MutexGuard(self.erase_regions(e)),
                    OpaqueKind::Task { result, context } => OpaqueKind::Task {
                        result: self.erase_regions(result),
                        context: context.map(|c| self.erase_regions(c)),
                    },
                    OpaqueKind::Registration(e) => OpaqueKind::Registration(self.erase_regions(e)),
                    OpaqueKind::AsyncRegistration(e) => {
                        OpaqueKind::AsyncRegistration(self.erase_regions(e))
                    }
                    OpaqueKind::ForeignCallback { params, result } => OpaqueKind::ForeignCallback {
                        params: params.iter().map(|t| self.erase_regions(*t)).collect(),
                        result: self.erase_regions(result),
                    },
                    OpaqueKind::ByteBuffer
                    | OpaqueKind::ForeignHandle
                    | OpaqueKind::SyncCell(_) => o,
                };
                self.intern(TyData::Opaque(o))
            }
            TyData::ErrorUnion { ok, err } => {
                let ok = self.erase_regions(ok);
                let err = self.erase_regions(err);
                self.intern(TyData::ErrorUnion { ok, err })
            }
            TyData::FnPtr { params, result } => {
                let params = params.iter().map(|t| self.erase_regions(*t)).collect();
                let result = self.erase_regions(result);
                self.intern(TyData::FnPtr { params, result })
            }
            TyData::Closure {
                owner,
                id,
                owner_args,
                kind,
                params,
                result,
            } => {
                let owner_args = owner_args.iter().map(|t| self.erase_regions(*t)).collect();
                let params = params.iter().map(|t| self.erase_regions(*t)).collect();
                let result = self.erase_regions(result);
                self.intern(TyData::Closure {
                    owner,
                    id,
                    owner_args,
                    kind,
                    params,
                    result,
                })
            }
            TyData::ImplCallable {
                owner,
                owner_args,
                kind,
                params,
                result,
            } => {
                let owner_args = owner_args.iter().map(|t| self.erase_regions(*t)).collect();
                let params = params.iter().map(|t| self.erase_regions(*t)).collect();
                let result = self.erase_regions(result);
                self.intern(TyData::ImplCallable {
                    owner,
                    owner_args,
                    kind,
                    params,
                    result,
                })
            }
            TyData::Assoc {
                base,
                trait_ref,
                name,
            } => {
                let base = self.erase_regions(base);
                self.intern(TyData::Assoc {
                    base,
                    trait_ref,
                    name,
                })
            }
            TyData::Unit
            | TyData::Type
            | TyData::ConstU32(_)
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Dyn(_)
            | TyData::Param(_)
            | TyData::ConstParam(_)
            | TyData::SelfParam
            | TyData::Var(_)
            | TyData::Error => ty,
            TyData::OwnedDyn { trait_ref, context } => {
                let context = context.map(|c| self.erase_regions(c));
                self.intern(TyData::OwnedDyn { trait_ref, context })
            }
        }
    }
}
