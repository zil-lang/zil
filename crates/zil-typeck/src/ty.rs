//! typeck-internal types with inference variables.
//!
//! These types never leave the crate: after inference every type is
//! resolved ("zonked") to a closed form and interned into the body
//! `TyTable` by the builder. The constructors mirror the subset of
//! `body::TyKind` that Source can currently name; a construct that has no
//! internal form yet is reported as `Unsupported`, never approximated.

use std::collections::HashMap;
use zil_ir::body::{FloatTy, IntTy};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ty(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdtRef(pub u32);

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
    /// An address window. Regions are not part of typeck types yet: the
    /// kernel's loan model carries safety until region contracts (P4).
    Ref {
        mutable: bool,
        pointee: Ty,
    },
    /// The n-th type parameter of the enclosing generic declaration.
    Param(u32),
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

    fn occurs(&self, v: VarId, ty: Ty) -> bool {
        let ty = self.shallow(ty);
        match self.data(ty) {
            TyData::Var(w) => *w == v,
            TyData::Adt { args, .. } => args.iter().any(|t| self.occurs(v, *t)),
            TyData::Tuple(items) => items.iter().any(|t| self.occurs(v, *t)),
            TyData::Array { element, .. } => self.occurs(v, *element),
            TyData::Ref { pointee, .. } => self.occurs(v, *pointee),
            TyData::Unit
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Param(_)
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
            TyData::Ref { mutable, pointee } => {
                let pointee = self.zonk(pointee)?;
                self.intern(TyData::Ref { mutable, pointee })
            }
            TyData::Unit
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Param(_)
            | TyData::Error => ty,
        })
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
            TyData::Ref { mutable, pointee } => {
                let pointee = self.subst(pointee, args);
                self.intern(TyData::Ref { mutable, pointee })
            }
            TyData::Unit
            | TyData::Bool
            | TyData::Never
            | TyData::Int(_)
            | TyData::Float(_)
            | TyData::Var(_)
            | TyData::Error => ty,
        }
    }
}
