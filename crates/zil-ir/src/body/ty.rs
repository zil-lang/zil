//! Interned types and nominal declarations.
//!
//! The table is an immutable snapshot for every consumer except its producer
//! (typeck). Consumers that need a substituted type call
//! [`TyTable::subst_lookup`]; a type the producer did not intern is a
//! structural defect of the candidate and must be rejected, never invented.

use super::ids::{AdtId, ArgsId, ClosureId, DynId, FieldIdx, RegionVid, TyId, VariantIdx};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntTy {
    I8,
    I16,
    I32,
    I64,
    Isize,
    U8,
    U16,
    U32,
    U64,
    Usize,
}

impl IntTy {
    pub const fn signed(self) -> bool {
        match self {
            IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::Isize => true,
            IntTy::U8 | IntTy::U16 | IntTy::U32 | IntTy::U64 | IntTy::Usize => false,
        }
    }
    /// Width in bits for a target with `pointer_bits`.
    pub const fn bits(self, pointer_bits: u32) -> u32 {
        match self {
            IntTy::I8 | IntTy::U8 => 8,
            IntTy::I16 | IntTy::U16 => 16,
            IntTy::I32 | IntTy::U32 => 32,
            IntTy::I64 | IntTy::U64 => 64,
            IntTy::Isize | IntTy::Usize => pointer_bits,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FloatTy {
    F32,
    F64,
}

/// Access mode of an address window (`&T` / `&mut T`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mutability {
    Shared,
    Exclusive,
}

/// A region is a first-class parameter. `Var` is a solver variable whose
/// solution is part of the certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Region {
    Static,
    /// The n-th region parameter of the enclosing signature or declaration.
    Param(u32),
    Var(RegionVid),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyncCellKind {
    I32,
    I64,
    Usize,
}

/// Sealed runtime-provided payloads. They are only reachable through the
/// carrier/descriptor/address forms their protocol permits.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpaqueTy {
    ByteBuffer,
    OwnedStore { element: TyId },
    Mutex { payload: TyId },
    MutexGuard { payload: TyId, region: Region },
    Registration { environment: TyId },
    AsyncRegistration { environment: TyId },
    Task { result: TyId, context: Option<TyId> },
    ForeignHandle,
    CWideString,
    ForeignCallback { params: Vec<TyId>, result: TyId },
    SyncCell(SyncCellKind),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TyKind {
    Unit,
    Bool,
    Never,
    Int(IntTy),
    Float(FloatTy),
    /// A nominal Record/Package/Resource/Enum instance.
    Adt {
        adt: AdtId,
        args: ArgsId,
    },
    Tuple(Vec<TyId>),
    Array {
        element: TyId,
        length: u32,
    },
    /// Unsized; only legal as the pointee of an address.
    Slice(TyId),
    /// `Address<m, T>`: a continuous address window.
    Ref {
        region: Region,
        mutability: Mutability,
        pointee: TyId,
    },
    /// The unique responsibility carrier of a resource payload.
    Managed(TyId),
    /// A descriptor of a resource payload; copies share identity.
    Desc(TyId),
    ErrorUnion {
        ok: TyId,
        err: TyId,
    },
    FnPtr {
        params: Vec<TyId>,
        result: TyId,
    },
    Closure {
        def: ClosureId,
        args: ArgsId,
    },
    /// Unsized dynamic pointee.
    Dyn(DynId),
    /// Affine owner of an erased concrete value.
    OwnedDyn {
        def: DynId,
        context: Option<TyId>,
    },
    /// The n-th type parameter of the enclosing generic scope.
    Param(u32),
    Opaque(OpaqueTy),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AdtKind {
    /// Finite Copy record.
    Record,
    /// Owning aggregate with at least one responsibility field.
    Package,
    /// Resource payload; values exist only behind `Managed`/`Desc`/addresses.
    Resource,
    /// Tagged union; variants in declaration order.
    Enum,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldDef {
    pub name: String,
    /// Declared type; may mention the declaration's `Param`s.
    pub ty: TyId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdtDef {
    /// Canonical module-qualified name; used for diagnostics only.
    pub name: String,
    pub kind: AdtKind,
    pub type_params: u32,
    pub region_params: u32,
    /// Records, packages and resources have exactly one variant.
    pub variants: Vec<VariantDef>,
    /// A checked user `SyncDrop`. Partial moves out of such a value are
    /// rejected unless decomposed through a checked protocol.
    pub user_drop: bool,
}

impl AdtDef {
    pub fn variant(&self, variant: VariantIdx) -> Option<&VariantDef> {
        self.variants.get(variant.index())
    }
    pub fn field(&self, variant: VariantIdx, field: FieldIdx) -> Option<&FieldDef> {
        self.variant(variant)?.fields.get(field.index())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClosureKind {
    Fn,
    FnMut,
    FnOnce,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureDef {
    pub kind: ClosureKind,
    pub type_params: u32,
    /// Environment fields in capture order; may mention `Param`s.
    pub captures: Vec<TyId>,
    pub params: Vec<TyId>,
    pub result: TyId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynSlotSig {
    pub name: String,
    pub receiver: Option<Mutability>,
    pub params: Vec<TyId>,
    pub result: TyId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynDef {
    pub name: String,
    pub slots: Vec<DynSlotSig>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenericArgs {
    pub tys: Vec<TyId>,
    pub regions: Vec<Region>,
}

/// Ownership class of a value type under a parameter environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueClass {
    /// Structurally copyable; reads leave the source initialized.
    Copy,
    /// Moved by use; the source becomes uninitialized.
    Affine,
    /// Not a value type (unsized pointee or resource payload).
    NotValue,
}

/// Bounds of the type parameters in scope. Only `Copy` matters to the
/// structural class; other bounds are typeck facts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParamEnv {
    pub copy_params: Vec<bool>,
}

#[derive(Clone, Debug, Default)]
pub struct TyTable {
    kinds: Vec<TyKind>,
    by_kind: HashMap<TyKind, TyId>,
    args: Vec<GenericArgs>,
    by_args: HashMap<GenericArgs, ArgsId>,
    adts: Vec<AdtDef>,
    closures: Vec<ClosureDef>,
    dyns: Vec<DynDef>,
}

pub struct TyTableParts<'a> {
    pub kinds: &'a [TyKind],
    pub args: &'a [GenericArgs],
    pub adts: &'a [AdtDef],
    pub closures: &'a [ClosureDef],
    pub dyns: &'a [DynDef],
}

impl TyTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn raw_parts(&self) -> TyTableParts<'_> {
        TyTableParts {
            kinds: &self.kinds,
            args: &self.args,
            adts: &self.adts,
            closures: &self.closures,
            dyns: &self.dyns,
        }
    }

    pub fn from_raw_parts(
        kinds: Vec<TyKind>,
        args: Vec<GenericArgs>,
        adts: Vec<AdtDef>,
        closures: Vec<ClosureDef>,
        dyns: Vec<DynDef>,
    ) -> Self {
        let mut by_kind = HashMap::with_capacity(kinds.len());
        for (i, k) in kinds.iter().enumerate() {
            by_kind.insert(k.clone(), TyId::from_index(i));
        }
        let mut by_args = HashMap::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            by_args.insert(a.clone(), ArgsId::from_index(i));
        }
        Self {
            kinds,
            by_kind,
            args,
            by_args,
            adts,
            closures,
            dyns,
        }
    }

    // ---- producer interface -------------------------------------------------

    pub fn intern(&mut self, kind: TyKind) -> TyId {
        if let Some(id) = self.by_kind.get(&kind) {
            return *id;
        }
        let id = TyId::from_index(self.kinds.len());
        self.kinds.push(kind.clone());
        self.by_kind.insert(kind, id);
        id
    }
    pub fn intern_args(&mut self, args: GenericArgs) -> ArgsId {
        if let Some(id) = self.by_args.get(&args) {
            return *id;
        }
        let id = ArgsId::from_index(self.args.len());
        self.args.push(args.clone());
        self.by_args.insert(args, id);
        id
    }
    pub fn add_adt(&mut self, def: AdtDef) -> AdtId {
        let id = AdtId::from_index(self.adts.len());
        self.adts.push(def);
        id
    }
    /// Declarations may be recursive; the producer reserves the id first.
    pub fn set_adt(&mut self, id: AdtId, def: AdtDef) {
        self.adts[id.index()] = def;
    }
    pub fn add_closure(&mut self, def: ClosureDef) -> ClosureId {
        let id = ClosureId::from_index(self.closures.len());
        self.closures.push(def);
        id
    }
    pub fn add_dyn(&mut self, def: DynDef) -> DynId {
        let id = DynId::from_index(self.dyns.len());
        self.dyns.push(def);
        id
    }
    pub fn dyn_def(&self, id: DynId) -> &DynDef {
        &self.dyns[id.index()]
    }
    pub fn get_dyn(&self, id: DynId) -> Option<&DynDef> {
        self.dyns.get(id.index())
    }
    /// Interns `ty` with `args` substituted for declaration parameters.
    pub fn subst(&mut self, ty: TyId, args: ArgsId) -> TyId {
        let kind = self.kind(ty).clone();
        let a = self.args(args).clone();
        let kind = match kind {
            TyKind::Param(i) => return a.tys.get(i as usize).copied().unwrap_or(ty),
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Dyn(_) => return ty,
            TyKind::Adt { adt, args: inner } => {
                let inner = self.subst_args(inner, args);
                return self.intern(TyKind::Adt { adt, args: inner });
            }
            TyKind::Closure { def, args: inner } => {
                let inner = self.subst_args(inner, args);
                return self.intern(TyKind::Closure { def, args: inner });
            }
            other @ (TyKind::Tuple(_)
            | TyKind::Array { .. }
            | TyKind::Slice(_)
            | TyKind::Ref { .. }
            | TyKind::Managed(_)
            | TyKind::Desc(_)
            | TyKind::ErrorUnion { .. }
            | TyKind::FnPtr { .. }
            | TyKind::OwnedDyn { .. }
            | TyKind::Opaque(_)) => other,
        };
        let mut map = |t: TyId| self.subst(t, args);
        let kind = map_kind(kind, &mut map, &mut |r| subst_region(r, &a));
        self.intern(kind)
    }

    /// Interns `inner` with every component substituted by `outer`.
    pub fn subst_args(&mut self, inner: ArgsId, outer: ArgsId) -> ArgsId {
        let GenericArgs { tys, regions } = self.args(inner).clone();
        let o = self.args(outer).clone();
        let tys = tys.into_iter().map(|t| self.subst(t, outer)).collect();
        let regions = regions.into_iter().map(|r| subst_region(r, &o)).collect();
        self.intern_args(GenericArgs { tys, regions })
    }

    // ---- consumer interface -------------------------------------------------

    pub fn kind(&self, ty: TyId) -> &TyKind {
        &self.kinds[ty.index()]
    }
    pub fn get(&self, ty: TyId) -> Option<&TyKind> {
        self.kinds.get(ty.index())
    }
    pub fn lookup(&self, kind: &TyKind) -> Option<TyId> {
        self.by_kind.get(kind).copied()
    }
    pub fn args(&self, args: ArgsId) -> &GenericArgs {
        &self.args[args.index()]
    }
    pub fn get_args(&self, args: ArgsId) -> Option<&GenericArgs> {
        self.args.get(args.index())
    }
    pub fn adt(&self, adt: AdtId) -> &AdtDef {
        &self.adts[adt.index()]
    }
    pub fn get_adt(&self, adt: AdtId) -> Option<&AdtDef> {
        self.adts.get(adt.index())
    }
    pub fn adt_count(&self) -> usize {
        self.adts.len()
    }
    /// The declaration kind of a nominal type; `None` for every structural type.
    pub fn adt_kind(&self, ty: TyId) -> Option<AdtKind> {
        let TyKind::Adt { adt, .. } = self.get(ty)? else {
            return None;
        };
        self.get_adt(*adt).map(|d| d.kind)
    }
    pub fn closure(&self, def: ClosureId) -> Option<&ClosureDef> {
        self.closures.get(def.index())
    }
    pub fn len(&self) -> usize {
        self.kinds.len()
    }
    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// Substitution without interning. `None` means the candidate did not
    /// intern a type it uses, which consumers reject.
    pub fn subst_lookup(&self, ty: TyId, args: ArgsId) -> Option<TyId> {
        let a = self.get_args(args)?;
        let kind = match self.get(ty)?.clone() {
            TyKind::Param(i) => return a.tys.get(i as usize).copied(),
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Dyn(_) => return Some(ty),
            TyKind::Adt { adt, args: inner } => {
                let inner = self.subst_args_lookup(inner, args)?;
                return self.lookup(&TyKind::Adt { adt, args: inner });
            }
            TyKind::Closure { def, args: inner } => {
                let inner = self.subst_args_lookup(inner, args)?;
                return self.lookup(&TyKind::Closure { def, args: inner });
            }
            other @ (TyKind::Tuple(_)
            | TyKind::Array { .. }
            | TyKind::Slice(_)
            | TyKind::Ref { .. }
            | TyKind::Managed(_)
            | TyKind::Desc(_)
            | TyKind::OwnedDyn { .. }
            | TyKind::ErrorUnion { .. }
            | TyKind::FnPtr { .. }
            | TyKind::Opaque(_)) => other,
        };
        let mut failed = false;
        let mut map = |t: TyId| match self.subst_lookup(t, args) {
            Some(t) => t,
            None => {
                failed = true;
                t
            }
        };
        let kind = map_kind(kind, &mut map, &mut |r| subst_region(r, a));
        if failed { None } else { self.lookup(&kind) }
    }

    pub fn subst_args_lookup(&self, inner: ArgsId, outer: ArgsId) -> Option<ArgsId> {
        let i = self.get_args(inner)?;
        let o = self.get_args(outer)?;
        let mut tys = Vec::with_capacity(i.tys.len());
        for t in &i.tys {
            tys.push(self.subst_lookup(*t, outer)?);
        }
        let regions = i.regions.iter().map(|r| subst_region(*r, o)).collect();
        self.by_args.get(&GenericArgs { tys, regions }).copied()
    }

    /// Declared type of `field` of `variant` instantiated at `args`.
    pub fn field_ty(
        &self,
        adt: AdtId,
        variant: VariantIdx,
        field: FieldIdx,
        args: ArgsId,
    ) -> Option<TyId> {
        let declared = self.get_adt(adt)?.field(variant, field)?.ty;
        self.subst_lookup(declared, args)
    }

    /// Structural ownership class. Unknown or malformed types are `NotValue`
    /// so that every consumer treats them as unusable.
    pub fn class(&self, ty: TyId, env: &ParamEnv) -> ValueClass {
        self.class_depth(ty, env, 0)
    }

    fn class_depth(&self, ty: TyId, env: &ParamEnv, depth: u32) -> ValueClass {
        const LIMIT: u32 = 128;
        if depth > LIMIT {
            return ValueClass::NotValue;
        }
        let Some(kind) = self.get(ty) else {
            return ValueClass::NotValue;
        };
        let all = |tys: &mut dyn Iterator<Item = TyId>| -> ValueClass {
            let mut class = ValueClass::Copy;
            for t in tys {
                match self.class_depth(t, env, depth + 1) {
                    ValueClass::Copy => {}
                    ValueClass::Affine => class = ValueClass::Affine,
                    ValueClass::NotValue => return ValueClass::NotValue,
                }
            }
            class
        };
        match kind {
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Desc(_)
            | TyKind::FnPtr { .. } => ValueClass::Copy,
            TyKind::Ref {
                mutability: Mutability::Shared,
                ..
            } => ValueClass::Copy,
            TyKind::Ref {
                mutability: Mutability::Exclusive,
                ..
            } => ValueClass::Affine,
            TyKind::Managed(_) | TyKind::OwnedDyn { .. } => ValueClass::Affine,
            TyKind::Slice(_) | TyKind::Dyn(_) => ValueClass::NotValue,
            TyKind::Param(i) => {
                if env.copy_params.get(*i as usize).copied().unwrap_or(false) {
                    ValueClass::Copy
                } else {
                    ValueClass::Affine
                }
            }
            TyKind::Tuple(items) => all(&mut items.iter().copied()),
            TyKind::Array { element, .. } => all(&mut std::iter::once(*element)),
            TyKind::ErrorUnion { ok, err } => all(&mut [*ok, *err].into_iter()),
            TyKind::Adt { adt, args } => {
                let Some(def) = self.get_adt(*adt) else {
                    return ValueClass::NotValue;
                };
                match def.kind {
                    AdtKind::Resource => ValueClass::NotValue,
                    AdtKind::Package => ValueClass::Affine,
                    AdtKind::Record | AdtKind::Enum => {
                        if def.user_drop {
                            return ValueClass::Affine;
                        }
                        let mut fields = Vec::new();
                        for variant in &def.variants {
                            for field in &variant.fields {
                                match self.subst_lookup(field.ty, *args) {
                                    Some(t) => fields.push(t),
                                    None => return ValueClass::NotValue,
                                }
                            }
                        }
                        all(&mut fields.into_iter())
                    }
                }
            }
            TyKind::Closure { def, args } => {
                let Some(closure) = self.closure(*def) else {
                    return ValueClass::NotValue;
                };
                let mut captures = Vec::new();
                for c in &closure.captures {
                    match self.subst_lookup(*c, *args) {
                        Some(t) => captures.push(t),
                        None => return ValueClass::NotValue,
                    }
                }
                match (closure.kind, all(&mut captures.into_iter())) {
                    (_, ValueClass::NotValue) => ValueClass::NotValue,
                    (ClosureKind::FnOnce, _) => ValueClass::Affine,
                    (ClosureKind::Fn | ClosureKind::FnMut, class) => class,
                }
            }
            TyKind::Opaque(opaque) => match opaque {
                OpaqueTy::ForeignHandle
                | OpaqueTy::CWideString
                | OpaqueTy::ForeignCallback { .. } => ValueClass::Copy,
                OpaqueTy::MutexGuard { .. } => ValueClass::Affine,
                OpaqueTy::ByteBuffer
                | OpaqueTy::OwnedStore { .. }
                | OpaqueTy::Mutex { .. }
                | OpaqueTy::Registration { .. }
                | OpaqueTy::AsyncRegistration { .. }
                | OpaqueTy::Task { .. }
                | OpaqueTy::SyncCell(_) => ValueClass::NotValue,
            },
        }
    }

    /// Whether ending the lifetime of an initialized value of `ty` carries a
    /// cleanup obligation (as opposed to merely forgetting bits).
    ///
    /// Compositional over structural types and closures: an exclusive address
    /// `&mut T` is affine (cannot be duplicated) but carries no cleanup
    /// obligation, so a tuple, enum, error union or closure whose only affine
    /// fields are `&mut` borrows likewise carries no cleanup obligation.
    pub fn needs_cleanup(&self, ty: TyId, env: &ParamEnv) -> bool {
        let mut visiting = Vec::new();
        self.needs_cleanup_in(ty, env, &mut visiting)
    }

    fn needs_cleanup_in(&self, ty: TyId, env: &ParamEnv, visiting: &mut Vec<TyId>) -> bool {
        if visiting.contains(&ty) {
            return false;
        }
        let Some(kind) = self.get(ty) else {
            return true;
        };
        visiting.push(ty);
        let result = match kind {
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::FnPtr { .. }
            | TyKind::Desc(_)
            | TyKind::Ref { .. } => false,
            TyKind::Managed(_) | TyKind::OwnedDyn { .. } | TyKind::Slice(_) | TyKind::Dyn(_) => {
                true
            }
            TyKind::Tuple(items) => items
                .iter()
                .any(|t| self.needs_cleanup_in(*t, env, visiting)),
            TyKind::Array { element, .. } => self.needs_cleanup_in(*element, env, visiting),
            TyKind::ErrorUnion { ok, err } => {
                self.needs_cleanup_in(*ok, env, visiting)
                    || self.needs_cleanup_in(*err, env, visiting)
            }
            TyKind::Closure { def, args } => match self.closure(*def) {
                None => true,
                Some(c) => c
                    .captures
                    .iter()
                    .any(|cap| match self.subst_lookup(*cap, *args) {
                        Some(t) => self.needs_cleanup_in(t, env, visiting),
                        None => true,
                    }),
            },
            TyKind::Adt { adt, args } => match self.get_adt(*adt) {
                None => true,
                Some(def) => match def.kind {
                    AdtKind::Package | AdtKind::Resource => true,
                    AdtKind::Record | AdtKind::Enum => {
                        def.user_drop
                            || def.variants.iter().enumerate().any(|(v, variant)| {
                                (0..variant.fields.len()).any(|f| {
                                    match self.field_ty(
                                        *adt,
                                        VariantIdx::from_index(v),
                                        FieldIdx::from_index(f),
                                        *args,
                                    ) {
                                        Some(t) => self.needs_cleanup_in(t, env, visiting),
                                        None => true,
                                    }
                                })
                            })
                    }
                },
            },
            TyKind::Opaque(OpaqueTy::SyncCell(_)) => false,
            TyKind::Param(_) | TyKind::Opaque(_) => self.class(ty, env) != ValueClass::Copy,
        };
        visiting.pop();
        result
    }

    /// Whether a value of `ty` may contain an address or descriptor, i.e.
    /// may keep a loan alive. Unknown structure (parameters, opaque and
    /// erased types, uninterned substitutions) answers `true`.
    pub fn holds_address(&self, ty: TyId) -> bool {
        let mut visiting = Vec::new();
        self.holds_address_in(ty, &mut visiting)
    }

    fn holds_address_in(&self, ty: TyId, visiting: &mut Vec<TyId>) -> bool {
        if visiting.contains(&ty) {
            // A recursive occurrence adds nothing beyond its other fields.
            return false;
        }
        let Some(kind) = self.get(ty) else {
            return true;
        };
        visiting.push(ty);
        let result = match kind {
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::FnPtr { .. } => false,
            TyKind::OwnedDyn { context, .. } => {
                context.is_some_and(|c| self.holds_address_in(c, visiting))
            }
            TyKind::Ref { .. } | TyKind::Desc(_) => true,
            TyKind::Opaque(o) => match opaque_contents(o) {
                Some(parts) => parts.iter().any(|t| self.holds_address_in(*t, visiting)),
                None => true,
            },
            TyKind::Closure { def, args } => match self.closure(*def) {
                None => true,
                Some(c) => c
                    .captures
                    .iter()
                    .any(|cap| match self.subst_lookup(*cap, *args) {
                        Some(t) => self.holds_address_in(t, visiting),
                        None => true,
                    }),
            },
            TyKind::Dyn(_) | TyKind::Param(_) => true,
            TyKind::Tuple(items) => items.iter().any(|t| self.holds_address_in(*t, visiting)),
            TyKind::Array { element, .. } | TyKind::Slice(element) | TyKind::Managed(element) => {
                self.holds_address_in(*element, visiting)
            }
            TyKind::ErrorUnion { ok, err } => {
                self.holds_address_in(*ok, visiting) || self.holds_address_in(*err, visiting)
            }
            TyKind::Adt { adt, args } => match self.get_adt(*adt) {
                None => true,
                Some(def) => def.variants.iter().enumerate().any(|(v, variant)| {
                    (0..variant.fields.len()).any(|f| {
                        match self.field_ty(
                            *adt,
                            VariantIdx::from_index(v),
                            FieldIdx::from_index(f),
                            *args,
                        ) {
                            Some(t) => self.holds_address_in(t, visiting),
                            None => true,
                        }
                    })
                }),
            },
        };
        visiting.pop();
        result
    }

    /// Whether a value of `ty` grants write access to storage that may hold
    /// addresses (so a callee receiving it can store a loan there).
    /// Unknown structure answers `true`.
    pub fn may_store_address(&self, ty: TyId) -> bool {
        let mut visiting = Vec::new();
        self.may_store_address_in(ty, &mut visiting)
    }

    fn may_store_address_in(&self, ty: TyId, visiting: &mut Vec<TyId>) -> bool {
        if visiting.contains(&ty) {
            return false;
        }
        let Some(kind) = self.get(ty) else {
            return true;
        };
        visiting.push(ty);
        let result = match kind {
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::FnPtr { .. } => false,
            TyKind::OwnedDyn { context, .. } => {
                context.is_some_and(|c| self.may_store_address_in(c, visiting))
            }
            TyKind::Opaque(o) => match opaque_contents(o) {
                Some(parts) => parts
                    .iter()
                    .any(|t| self.may_store_address_in(*t, visiting)),
                None => true,
            },
            TyKind::Closure { def, args } => match self.closure(*def) {
                None => true,
                Some(c) => c
                    .captures
                    .iter()
                    .any(|cap| match self.subst_lookup(*cap, *args) {
                        Some(t) => self.may_store_address_in(t, visiting),
                        None => true,
                    }),
            },
            TyKind::Dyn(_) | TyKind::Param(_) => true,
            TyKind::Ref {
                mutability: Mutability::Exclusive,
                pointee,
                ..
            } => self.holds_address(*pointee) || self.may_store_address_in(*pointee, visiting),
            TyKind::Ref {
                mutability: Mutability::Shared,
                pointee,
                ..
            } => self.may_store_address_in(*pointee, visiting),
            TyKind::Desc(payload) => self.holds_address(*payload),
            TyKind::Tuple(items) => items
                .iter()
                .any(|t| self.may_store_address_in(*t, visiting)),
            TyKind::Array { element, .. } | TyKind::Slice(element) | TyKind::Managed(element) => {
                self.may_store_address_in(*element, visiting)
            }
            TyKind::ErrorUnion { ok, err } => {
                self.may_store_address_in(*ok, visiting)
                    || self.may_store_address_in(*err, visiting)
            }
            TyKind::Adt { adt, args } => match self.get_adt(*adt) {
                None => true,
                Some(def) => def.variants.iter().enumerate().any(|(v, variant)| {
                    (0..variant.fields.len()).any(|f| {
                        match self.field_ty(
                            *adt,
                            VariantIdx::from_index(v),
                            FieldIdx::from_index(f),
                            *args,
                        ) {
                            Some(t) => self.may_store_address_in(t, visiting),
                            None => true,
                        }
                    })
                }),
            },
        };
        visiting.pop();
        result
    }

    /// Every distinct `Region::Param(r)` occurring in `ty`.
    pub fn regions_in(&self, ty: TyId) -> Vec<u32> {
        let mut out = Vec::new();
        let mut visiting = Vec::new();
        self.collect_regions_in(ty, &mut out, &mut visiting);
        out
    }

    fn collect_regions_in(&self, ty: TyId, out: &mut Vec<u32>, visiting: &mut Vec<TyId>) {
        if visiting.contains(&ty) {
            return;
        }
        let Some(kind) = self.get(ty) else {
            return;
        };
        visiting.push(ty);
        match kind {
            TyKind::Ref {
                region: Region::Param(r),
                pointee,
                ..
            } => {
                if !out.contains(r) {
                    out.push(*r);
                }
                self.collect_regions_in(*pointee, out, visiting);
            }
            TyKind::Ref { pointee, .. }
            | TyKind::Array {
                element: pointee, ..
            }
            | TyKind::Slice(pointee)
            | TyKind::Managed(pointee)
            | TyKind::Desc(pointee) => self.collect_regions_in(*pointee, out, visiting),
            TyKind::Tuple(items) => {
                for t in items {
                    self.collect_regions_in(*t, out, visiting);
                }
            }
            TyKind::ErrorUnion { ok, err } => {
                self.collect_regions_in(*ok, out, visiting);
                self.collect_regions_in(*err, out, visiting);
            }
            TyKind::Adt { args, .. } => {
                if let Some(a) = self.get_args(*args) {
                    for r in &a.regions {
                        if let Region::Param(idx) = r
                            && !out.contains(idx)
                        {
                            out.push(*idx);
                        }
                    }
                    for t in &a.tys {
                        self.collect_regions_in(*t, out, visiting);
                    }
                }
            }
            TyKind::FnPtr { params, result } => {
                for p in params {
                    self.collect_regions_in(*p, out, visiting);
                }
                self.collect_regions_in(*result, out, visiting);
            }
            TyKind::Closure { def, args } => {
                if let Some(c) = self.closure(*def) {
                    for cap in &c.captures {
                        if let Some(t) = self.subst_lookup(*cap, *args) {
                            self.collect_regions_in(t, out, visiting);
                        }
                    }
                }
            }
            TyKind::Opaque(o) => {
                if let OpaqueTy::MutexGuard {
                    region: Region::Param(r),
                    ..
                } = o
                    && !out.contains(r)
                {
                    out.push(*r);
                }
                if let Some(parts) = opaque_contents(o) {
                    for p in parts {
                        self.collect_regions_in(p, out, visiting);
                    }
                }
            }
            TyKind::OwnedDyn { context, .. } => {
                if let Some(c) = context {
                    self.collect_regions_in(*c, out, visiting);
                }
            }
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Dyn(_)
            | TyKind::Param(_) => {}
        }
        visiting.pop();
    }

    /// Structural type equality modulo `Region` annotations on references and guards.
    pub fn same_erased_region(&self, a: TyId, b: TyId) -> bool {
        if a == b {
            return true;
        }
        let (Some(ka), Some(kb)) = (self.get(a), self.get(b)) else {
            return false;
        };
        match (ka, kb) {
            (
                TyKind::Ref {
                    mutability: ma,
                    pointee: pa,
                    ..
                },
                TyKind::Ref {
                    mutability: mb,
                    pointee: pb,
                    ..
                },
            ) => ma == mb && self.same_erased_region(*pa, *pb),
            (
                TyKind::Adt {
                    adt: aa,
                    args: args_a,
                },
                TyKind::Adt {
                    adt: ab,
                    args: args_b,
                },
            ) => {
                if aa != ab {
                    return false;
                }
                let (Some(ga), Some(gb)) = (self.get_args(*args_a), self.get_args(*args_b)) else {
                    return false;
                };
                ga.tys.len() == gb.tys.len()
                    && ga
                        .tys
                        .iter()
                        .zip(&gb.tys)
                        .all(|(x, y)| self.same_erased_region(*x, *y))
            }
            (TyKind::Tuple(xs), TyKind::Tuple(ys)) => {
                xs.len() == ys.len()
                    && xs
                        .iter()
                        .zip(ys)
                        .all(|(x, y)| self.same_erased_region(*x, *y))
            }
            (
                TyKind::Array {
                    element: ea,
                    length: la,
                },
                TyKind::Array {
                    element: eb,
                    length: lb,
                },
            ) => la == lb && self.same_erased_region(*ea, *eb),
            (TyKind::Slice(ea), TyKind::Slice(eb))
            | (TyKind::Managed(ea), TyKind::Managed(eb))
            | (TyKind::Desc(ea), TyKind::Desc(eb)) => self.same_erased_region(*ea, *eb),
            (TyKind::ErrorUnion { ok: oa, err: ea }, TyKind::ErrorUnion { ok: ob, err: eb }) => {
                self.same_erased_region(*oa, *ob) && self.same_erased_region(*ea, *eb)
            }
            (
                TyKind::FnPtr {
                    params: pa,
                    result: ra,
                },
                TyKind::FnPtr {
                    params: pb,
                    result: rb,
                },
            ) => {
                pa.len() == pb.len()
                    && pa
                        .iter()
                        .zip(pb)
                        .all(|(x, y)| self.same_erased_region(*x, *y))
                    && self.same_erased_region(*ra, *rb)
            }
            (
                TyKind::Opaque(OpaqueTy::MutexGuard { payload: pa, .. }),
                TyKind::Opaque(OpaqueTy::MutexGuard { payload: pb, .. }),
            ) => self.same_erased_region(*pa, *pb),
            (
                TyKind::Opaque(OpaqueTy::Task {
                    result: ra,
                    context: ca,
                }),
                TyKind::Opaque(OpaqueTy::Task {
                    result: rb,
                    context: cb,
                }),
            ) => {
                self.same_erased_region(*ra, *rb)
                    && match (ca, cb) {
                        (None, None) => true,
                        (Some(a), Some(b)) => self.same_erased_region(*a, *b),
                        (None, Some(_)) | (Some(_), None) => false,
                    }
            }
            (
                TyKind::OwnedDyn {
                    def: da,
                    context: ca,
                },
                TyKind::OwnedDyn {
                    def: db,
                    context: cb,
                },
            ) => {
                da == db
                    && match (ca, cb) {
                        (None, None) => true,
                        (Some(a), Some(b)) => self.same_erased_region(*a, *b),
                        (None, Some(_)) | (Some(_), None) => false,
                    }
            }
            _ => false,
        }
    }

    /// Looks up `kind`, falling back to any already-interned type that differs
    /// only in region annotations.
    pub fn lookup_erased(&self, kind: &TyKind) -> Option<TyId> {
        if let Some(id) = self.lookup(kind) {
            return Some(id);
        }
        match kind {
            TyKind::Ref {
                mutability,
                pointee,
                ..
            } => {
                for (i, k) in self.kinds.iter().enumerate() {
                    if let TyKind::Ref {
                        mutability: m,
                        pointee: p,
                        ..
                    } = k
                        && *m == *mutability
                        && self.same_erased_region(*p, *pointee)
                    {
                        return Some(TyId::from_index(i));
                    }
                }
                None
            }
            TyKind::Tuple(items) => {
                for (i, k) in self.kinds.iter().enumerate() {
                    if let TyKind::Tuple(other) = k
                        && other.len() == items.len()
                        && other
                            .iter()
                            .zip(items)
                            .all(|(a, b)| self.same_erased_region(*a, *b))
                    {
                        return Some(TyId::from_index(i));
                    }
                }
                None
            }
            TyKind::Adt { adt, args } => {
                let want = self.get_args(*args)?;
                for (i, k) in self.kinds.iter().enumerate() {
                    if let TyKind::Adt {
                        adt: a,
                        args: other_args,
                    } = k
                        && *a == *adt
                        && let Some(got) = self.get_args(*other_args)
                        && got.tys.len() == want.tys.len()
                        && got
                            .tys
                            .iter()
                            .zip(&want.tys)
                            .all(|(x, y)| self.same_erased_region(*x, *y))
                    {
                        return Some(TyId::from_index(i));
                    }
                }
                None
            }
            TyKind::Unit
            | TyKind::Bool
            | TyKind::Never
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::Array { .. }
            | TyKind::Slice(_)
            | TyKind::Managed(_)
            | TyKind::Desc(_)
            | TyKind::ErrorUnion { .. }
            | TyKind::FnPtr { .. }
            | TyKind::Closure { .. }
            | TyKind::Dyn(_)
            | TyKind::OwnedDyn { .. }
            | TyKind::Param(_)
            | TyKind::Opaque(_) => None,
        }
    }
}

fn subst_region(region: Region, args: &GenericArgs) -> Region {
    match region {
        Region::Param(i) => args.regions.get(i as usize).copied().unwrap_or(region),
        Region::Static | Region::Var(_) => region,
    }
}

/// Rebuilds one level of `kind` with `ty` applied to every child type and
/// `region` applied to every region. Exhaustive: a new constructor must
/// decide how substitution traverses it.
pub fn map_kind(
    kind: TyKind,
    ty: &mut dyn FnMut(TyId) -> TyId,
    region: &mut dyn FnMut(Region) -> Region,
) -> TyKind {
    match kind {
        TyKind::Unit => TyKind::Unit,
        TyKind::Bool => TyKind::Bool,
        TyKind::Never => TyKind::Never,
        TyKind::Int(i) => TyKind::Int(i),
        TyKind::Float(f) => TyKind::Float(f),
        TyKind::Dyn(d) => TyKind::Dyn(d),
        TyKind::OwnedDyn { def, context } => TyKind::OwnedDyn {
            def,
            context: context.map(&mut *ty),
        },
        TyKind::Param(i) => TyKind::Param(i),
        // Nominal argument lists are interned separately; substitution
        // handles them before calling this one-level map.
        TyKind::Adt { adt, args } => TyKind::Adt { adt, args },
        TyKind::Closure { def, args } => TyKind::Closure { def, args },
        TyKind::Tuple(items) => TyKind::Tuple(items.into_iter().map(&mut *ty).collect()),
        TyKind::Array { element, length } => TyKind::Array {
            element: ty(element),
            length,
        },
        TyKind::Slice(element) => TyKind::Slice(ty(element)),
        TyKind::Ref {
            region: r,
            mutability,
            pointee,
        } => TyKind::Ref {
            region: region(r),
            mutability,
            pointee: ty(pointee),
        },
        TyKind::Managed(p) => TyKind::Managed(ty(p)),
        TyKind::Desc(p) => TyKind::Desc(ty(p)),
        TyKind::ErrorUnion { ok, err } => TyKind::ErrorUnion {
            ok: ty(ok),
            err: ty(err),
        },
        TyKind::FnPtr { params, result } => TyKind::FnPtr {
            params: params.into_iter().map(&mut *ty).collect(),
            result: ty(result),
        },
        TyKind::Opaque(opaque) => TyKind::Opaque(match opaque {
            OpaqueTy::ByteBuffer => OpaqueTy::ByteBuffer,
            OpaqueTy::ForeignHandle => OpaqueTy::ForeignHandle,
            OpaqueTy::CWideString => OpaqueTy::CWideString,
            OpaqueTy::SyncCell(k) => OpaqueTy::SyncCell(k),
            OpaqueTy::OwnedStore { element } => OpaqueTy::OwnedStore {
                element: ty(element),
            },
            OpaqueTy::Mutex { payload } => OpaqueTy::Mutex {
                payload: ty(payload),
            },
            OpaqueTy::MutexGuard { payload, region: r } => OpaqueTy::MutexGuard {
                payload: ty(payload),
                region: region(r),
            },
            OpaqueTy::Registration { environment } => OpaqueTy::Registration {
                environment: ty(environment),
            },
            OpaqueTy::AsyncRegistration { environment } => OpaqueTy::AsyncRegistration {
                environment: ty(environment),
            },
            OpaqueTy::Task { result, context } => OpaqueTy::Task {
                result: ty(result),
                context: context.map(ty),
            },
            OpaqueTy::ForeignCallback { params, result } => OpaqueTy::ForeignCallback {
                params: params.into_iter().map(&mut *ty).collect(),
                result: ty(result),
            },
        }),
    }
}

/// The values an opaque runtime object owns, when its structure is fixed by
/// the language: a byte buffer owns bytes, a store owns its elements, a
/// foreign handle is a scalar. `None` when unknown, which every address
/// predicate reads as "may hold an address".
fn opaque_contents(o: &OpaqueTy) -> Option<Vec<TyId>> {
    match o {
        OpaqueTy::ByteBuffer
        | OpaqueTy::ForeignHandle
        | OpaqueTy::ForeignCallback { .. }
        | OpaqueTy::SyncCell(_) => Some(Vec::new()),
        OpaqueTy::OwnedStore { element } => Some(vec![*element]),
        OpaqueTy::Mutex { payload } | OpaqueTy::MutexGuard { payload, .. } => Some(vec![*payload]),
        OpaqueTy::Registration { environment } | OpaqueTy::AsyncRegistration { environment } => {
            Some(vec![*environment])
        }
        OpaqueTy::Task { result, context } => {
            let mut v = vec![*result];
            if let Some(c) = context {
                v.push(*c);
            }
            Some(v)
        }
        OpaqueTy::CWideString => None,
    }
}
