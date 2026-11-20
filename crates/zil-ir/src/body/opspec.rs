//! OpSpec: the single definition of what each operation *attempts*.
//!
//! Four parts, each an exhaustive static function over the operation enums:
//!
//! | part        | function                                   |
//! |-------------|--------------------------------------------|
//! | operands    | [`InstructionKind::accesses`], [`Terminator::accesses`] (role per use) |
//! | places      | the same accesses, in evaluation order, plus [`Terminator::edge_accesses`] |
//! | constraints | [`check_instruction`], [`check_terminator`], [`rvalue_ty`] |
//! | effects     | [`InstructionKind::effects`], [`Terminator::effects`] |
//!
//! OpSpec never states that an attempt is safe in the current state; the
//! semantic kernel decides that from these definitions.

use super::ids::{BlockId, FieldIdx, Local, TyId};
use super::inst::{
    AggregateKind, BinOp, BodyCx, Callee, ClosureCallMode, ConstValue, EdgeKind, FnRole, FnSig,
    InstructionKind, IntrinsicOp, Operand, Rvalue, Sigs, Terminator, UnOp,
};
use super::place::{FieldRef, Place, PlaceTyError, type_place};
use super::ty::{AdtKind, IntTy, Mutability, TyKind, ValueClass};
use std::borrow::Cow;

/// Why an operand or place participates in an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OperandRole {
    /// A dynamic index read while evaluating another place.
    Index,
    /// The value of an assignment or replacement.
    Value,
    /// The n-th aggregate component.
    Component(u32),
    Lhs,
    Rhs,
    /// The n-th call argument.
    Arg(u32),
    /// The function-pointer, closure or dyn object being called.
    Callee,
    Discriminant,
    Condition,
    ReturnValue,
    DeferClosure,
    /// The place an operation writes, borrows, drops or replaces.
    Target,
    /// The local receiving a replaced instance.
    ReplacedOld,
}

/// How an operation touches one place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaceAccess {
    /// Read a Copy value.
    Copy,
    /// Take the value; the place becomes uninitialized.
    Move,
    /// Initialize an uninitialized place (or overwrite a Copy value).
    Init,
    Borrow(Mutability),
    /// Create a descriptor of the reached resource.
    Describe,
    /// End the lifetime of the value and run its cleanup.
    Drop,
    /// Swap the instance at a live place.
    ReplaceInstance,
    /// Read only the active variant/arm tag.
    InspectTag,
    /// Read only the length.
    InspectLen,
    /// Use a closure environment in place for a call.
    CallClosure(ClosureCallMode),
}

/// One access in evaluation order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Access<'a> {
    pub place: Cow<'a, Place>,
    pub access: PlaceAccess,
    pub role: OperandRole,
}

/// Effect categories (`_zil_design.md` §4.1). An operation may produce
/// several; none substitutes for another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Effect {
    Read,
    Write,
    Init,
    Move,
    Borrow,
    Describe,
    Drop,
    ChangeVariant,
    Relocate,
    ReplaceInstance,
    EndLifetime,
    Create,
    Call,
    Protocol,
    Scope,
    Storage,
    MayTrap,
    Suspend,
}

impl Effect {
    pub const ALL: [Effect; 18] = [
        Effect::Read,
        Effect::Write,
        Effect::Init,
        Effect::Move,
        Effect::Borrow,
        Effect::Describe,
        Effect::Drop,
        Effect::ChangeVariant,
        Effect::Relocate,
        Effect::ReplaceInstance,
        Effect::EndLifetime,
        Effect::Create,
        Effect::Call,
        Effect::Protocol,
        Effect::Scope,
        Effect::Storage,
        Effect::MayTrap,
        Effect::Suspend,
    ];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EffectSet(u32);

impl EffectSet {
    pub const EMPTY: EffectSet = EffectSet(0);
    pub fn with(self, effect: Effect) -> Self {
        EffectSet(self.0 | (1 << effect as u8))
    }
    pub fn union(self, other: EffectSet) -> Self {
        EffectSet(self.0 | other.0)
    }
    pub fn contains(self, effect: Effect) -> bool {
        self.0 & (1 << effect as u8) != 0
    }
    pub fn iter(self) -> impl Iterator<Item = Effect> {
        Effect::ALL.into_iter().filter(move |e| self.contains(*e))
    }
}

impl PlaceAccess {
    /// Effects of one access. Exhaustive.
    pub fn effects(self) -> EffectSet {
        let e = EffectSet::EMPTY;
        match self {
            PlaceAccess::Copy | PlaceAccess::InspectTag | PlaceAccess::InspectLen => {
                e.with(Effect::Read)
            }
            PlaceAccess::Move => e.with(Effect::Read).with(Effect::Move),
            PlaceAccess::Init => e.with(Effect::Write).with(Effect::Init),
            PlaceAccess::Borrow(_) => e.with(Effect::Borrow),
            PlaceAccess::Describe => e.with(Effect::Describe),
            PlaceAccess::Drop => e.with(Effect::Drop).with(Effect::EndLifetime),
            PlaceAccess::ReplaceInstance => e
                .with(Effect::Write)
                .with(Effect::ReplaceInstance)
                .with(Effect::ChangeVariant),
            PlaceAccess::CallClosure(ClosureCallMode::Shared) => e.with(Effect::Borrow),
            PlaceAccess::CallClosure(ClosureCallMode::Mut) => {
                e.with(Effect::Borrow).with(Effect::Write)
            }
            PlaceAccess::CallClosure(ClosureCallMode::Once) => {
                e.with(Effect::Read).with(Effect::Move)
            }
        }
    }
}

fn push_place<'a>(
    out: &mut Vec<Access<'a>>,
    place: &'a Place,
    access: PlaceAccess,
    role: OperandRole,
) {
    for index in place.index_locals() {
        out.push(Access {
            place: Cow::Owned(Place::local(index)),
            access: PlaceAccess::Copy,
            role: OperandRole::Index,
        });
    }
    out.push(Access {
        place: Cow::Borrowed(place),
        access,
        role,
    });
}

fn push_operand<'a>(out: &mut Vec<Access<'a>>, operand: &'a Operand, role: OperandRole) {
    match operand {
        Operand::Copy(p) => push_place(out, p, PlaceAccess::Copy, role),
        Operand::Move(p) => push_place(out, p, PlaceAccess::Move, role),
        Operand::Const(_) | Operand::Pooled(_) => {}
    }
}

fn push_rvalue<'a>(out: &mut Vec<Access<'a>>, rvalue: &'a Rvalue) {
    match rvalue {
        Rvalue::Use(o) => push_operand(out, o, OperandRole::Value),
        Rvalue::Ref {
            mutability, place, ..
        } => push_place(
            out,
            place,
            PlaceAccess::Borrow(*mutability),
            OperandRole::Target,
        ),
        Rvalue::Describe { place } => {
            push_place(out, place, PlaceAccess::Describe, OperandRole::Target)
        }
        Rvalue::Aggregate { operands, .. } => {
            for (i, o) in operands.iter().enumerate() {
                push_operand(out, o, OperandRole::Component(i as u32));
            }
        }
        Rvalue::ErrorUnion { value, .. } => push_operand(out, value, OperandRole::Value),
        Rvalue::Unary { operand, .. } => push_operand(out, operand, OperandRole::Value),
        Rvalue::Binary { lhs, rhs, .. } => {
            push_operand(out, lhs, OperandRole::Lhs);
            push_operand(out, rhs, OperandRole::Rhs);
        }
        Rvalue::Convert { operand, .. } => push_operand(out, operand, OperandRole::Value),
        Rvalue::Discriminant(p) | Rvalue::ErrorArm(p) => {
            push_place(out, p, PlaceAccess::InspectTag, OperandRole::Target)
        }
        Rvalue::Len(p) => push_place(out, p, PlaceAccess::InspectLen, OperandRole::Target),
        Rvalue::FnRef(_) | Rvalue::ForeignFnRef(_) | Rvalue::StaticUtf(_) => {}
    }
}

impl InstructionKind {
    /// Operands and places in evaluation order: sources left to right, then
    /// the target. This order is normative for the kernel.
    pub fn accesses(&self) -> Vec<Access<'_>> {
        let mut out = Vec::new();
        match self {
            InstructionKind::Assign { dest, value } => {
                push_rvalue(&mut out, value);
                push_place(&mut out, dest, PlaceAccess::Init, OperandRole::Target);
            }
            InstructionKind::Drop { place } => {
                push_place(&mut out, place, PlaceAccess::Drop, OperandRole::Target)
            }
            InstructionKind::Replace { place, value, old } => {
                push_operand(&mut out, value, OperandRole::Value);
                push_place(
                    &mut out,
                    place,
                    PlaceAccess::ReplaceInstance,
                    OperandRole::Target,
                );
                out.push(Access {
                    place: Cow::Owned(Place::local(*old)),
                    access: PlaceAccess::Init,
                    role: OperandRole::ReplacedOld,
                });
            }
            InstructionKind::RegisterDefer { closure, .. } => {
                push_operand(&mut out, closure, OperandRole::DeferClosure)
            }
            InstructionKind::StorageLive(_)
            | InstructionKind::StorageDead(_)
            | InstructionKind::ScopeEnter(_)
            | InstructionKind::ScopeExit { .. } => {}
        }
        out
    }

    pub fn effects(&self) -> EffectSet {
        let base = match self {
            InstructionKind::Assign { value, .. } => rvalue_effects(value),
            InstructionKind::Drop { .. } | InstructionKind::Replace { .. } => EffectSet::EMPTY,
            InstructionKind::StorageLive(_) | InstructionKind::StorageDead(_) => {
                EffectSet::EMPTY.with(Effect::Storage)
            }
            InstructionKind::ScopeEnter(_) | InstructionKind::ScopeExit { .. } => {
                EffectSet::EMPTY.with(Effect::Scope)
            }
            InstructionKind::RegisterDefer { .. } => EffectSet::EMPTY.with(Effect::Scope),
        };
        self.accesses()
            .iter()
            .fold(base, |set, a| set.union(a.access.effects()))
    }
}

fn rvalue_effects(rvalue: &Rvalue) -> EffectSet {
    let e = EffectSet::EMPTY;
    match rvalue {
        Rvalue::Binary { op, .. } if op.may_trap() => e.with(Effect::MayTrap),
        Rvalue::Unary { op: UnOp::Neg, .. } | Rvalue::Convert { .. } => e.with(Effect::MayTrap),
        Rvalue::Use(_)
        | Rvalue::Ref { .. }
        | Rvalue::Describe { .. }
        | Rvalue::Aggregate { .. }
        | Rvalue::ErrorUnion { .. }
        | Rvalue::Unary { .. }
        | Rvalue::Binary { .. }
        | Rvalue::Discriminant(_)
        | Rvalue::ErrorArm(_)
        | Rvalue::Len(_)
        | Rvalue::FnRef(_)
        | Rvalue::ForeignFnRef(_)
        | Rvalue::StaticUtf(_) => e,
    }
}

impl Terminator {
    /// Accesses performed before control leaves the block, in order.
    pub fn accesses(&self) -> Vec<Access<'_>> {
        let mut out = Vec::new();
        match self {
            Terminator::Switch { discr, .. } => {
                push_operand(&mut out, discr, OperandRole::Discriminant)
            }
            Terminator::Return { value } => push_operand(&mut out, value, OperandRole::ReturnValue),
            Terminator::Assert { cond, .. } => push_operand(&mut out, cond, OperandRole::Condition),
            Terminator::Call { callee, args, .. } => {
                match callee {
                    Callee::Indirect(o) | Callee::DynSlot { object: o, .. } => {
                        push_operand(&mut out, o, OperandRole::Callee)
                    }
                    Callee::Closure { closure, mode } => push_place(
                        &mut out,
                        closure,
                        PlaceAccess::CallClosure(*mode),
                        OperandRole::Callee,
                    ),
                    Callee::Direct(_) | Callee::Foreign(_) | Callee::Intrinsic(_) => {}
                }
                for (i, a) in args.iter().enumerate() {
                    push_operand(&mut out, a, OperandRole::Arg(i as u32));
                }
            }
            Terminator::Goto { .. } | Terminator::Unreachable | Terminator::Trap(_) => {}
        }
        out
    }

    /// Accesses that happen on one outgoing edge only.
    pub fn edge_accesses(&self, edge: EdgeKind) -> Vec<Access<'_>> {
        match (self, edge) {
            (Terminator::Call { dest, .. }, EdgeKind::CallReturn) => {
                let mut out = Vec::new();
                push_place(&mut out, dest, PlaceAccess::Init, OperandRole::Target);
                out
            }
            (
                Terminator::Goto { .. }
                | Terminator::Switch { .. }
                | Terminator::Return { .. }
                | Terminator::Call { .. }
                | Terminator::Assert { .. }
                | Terminator::Unreachable
                | Terminator::Trap(_),
                _,
            ) => Vec::new(),
        }
    }

    pub fn effects(&self) -> EffectSet {
        let base = match self {
            Terminator::Call { callee, cancel, .. } => {
                let mut e = EffectSet::EMPTY.with(Effect::Call);
                if let Callee::Intrinsic(_) = callee {
                    e = e.with(Effect::Protocol);
                }
                if cancel.is_some() {
                    e = e.with(Effect::Suspend);
                }
                e
            }
            Terminator::Assert { .. } | Terminator::Trap(_) => {
                EffectSet::EMPTY.with(Effect::MayTrap)
            }
            Terminator::Goto { .. }
            | Terminator::Switch { .. }
            | Terminator::Return { .. }
            | Terminator::Unreachable => EffectSet::EMPTY,
        };
        let mut set = self
            .accesses()
            .iter()
            .fold(base, |s, a| s.union(a.access.effects()));
        for (kind, _) in self.edges() {
            for a in self.edge_accesses(kind) {
                set = set.union(a.access.effects());
            }
        }
        set
    }
}

// ---- constraints ---------------------------------------------------------

/// Structural defects. Every one is a SafeReject of the candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstraintError {
    Place(PlaceTyError),
    UnknownLocal(Local),
    UnknownBlock(BlockId),
    UnknownFunction(u32),
    TypeMismatch {
        expected: TyId,
        found: TyId,
    },
    /// A type the producer did not intern.
    MissingType,
    /// `Copy` of a non-Copy value, or `Move` of a non-value.
    OperandClass {
        place: Place,
        class: ValueClass,
    },
    NotDescribable(TyId),
    Aggregate {
        reason: &'static str,
    },
    Arity {
        expected: usize,
        found: usize,
    },
    Operator {
        reason: &'static str,
    },
    /// Moving or dropping storage that is not inline storage of an owned
    /// root (behind an indirection, inside an array element, or a capture
    /// the closure kind does not own).
    MoveFromBorrowed {
        place: Place,
    },
    /// Writing through a shared address or into a capture of an `Fn`.
    WriteThroughShared {
        place: Place,
    },
    /// Initializing storage whose previous value cannot be shown dead and
    /// carries a cleanup obligation; such storage must use `Replace`.
    OverwriteThroughIndirection {
        place: Place,
    },
    /// Moving a part out of a value with a user `SyncDrop`.
    PartialMoveOfDropValue {
        place: Place,
    },
    /// The operation is declared but its contract is not specified yet;
    /// rejected explicitly rather than ignored.
    Unspecified(&'static str),
}

impl From<PlaceTyError> for ConstraintError {
    fn from(e: PlaceTyError) -> Self {
        ConstraintError::Place(e)
    }
}

/// Context for constraint checks: one body and the module signatures.
#[derive(Clone, Copy)]
pub struct ConstraintCx<'a> {
    pub body: BodyCx<'a>,
    pub sigs: Sigs<'a>,
}

impl ConstraintCx<'_> {
    fn place_ty(&self, place: &Place) -> Result<TyId, ConstraintError> {
        Ok(type_place(place, &self.body)?.ty())
    }
    fn lookup(&self, kind: TyKind) -> Result<TyId, ConstraintError> {
        self.body
            .tys
            .lookup(&kind)
            .ok_or(ConstraintError::MissingType)
    }
    fn expect(&self, expected: TyId, found: TyId) -> Result<(), ConstraintError> {
        if expected == found {
            Ok(())
        } else {
            Err(ConstraintError::TypeMismatch { expected, found })
        }
    }
}

pub fn operand_ty(operand: &Operand, cx: &ConstraintCx<'_>) -> Result<TyId, ConstraintError> {
    let env = &cx.body.body.param_env;
    match operand {
        Operand::Copy(p) => {
            let ty = cx.place_ty(p)?;
            match cx.body.tys.class(ty, env) {
                ValueClass::Copy => Ok(ty),
                class @ (ValueClass::Affine | ValueClass::NotValue) => {
                    Err(ConstraintError::OperandClass {
                        place: p.clone(),
                        class,
                    })
                }
            }
        }
        Operand::Move(p) => {
            let ty = cx.place_ty(p)?;
            match cx.body.tys.class(ty, env) {
                ValueClass::Copy | ValueClass::Affine => Ok(ty),
                class @ ValueClass::NotValue => Err(ConstraintError::OperandClass {
                    place: p.clone(),
                    class,
                }),
            }
        }
        Operand::Const(c) => cx.lookup(match c {
            ConstValue::Unit => TyKind::Unit,
            ConstValue::Bool(_) => TyKind::Bool,
            ConstValue::Int { ty, .. } => TyKind::Int(*ty),
            ConstValue::Float { ty, .. } => TyKind::Float(*ty),
        }),
        Operand::Pooled(_) => Err(ConstraintError::Unspecified("pooled constants")),
    }
}

/// The single typing rule of every rvalue.
pub fn rvalue_ty(rvalue: &Rvalue, cx: &ConstraintCx<'_>) -> Result<TyId, ConstraintError> {
    let tys = cx.body.tys;
    match rvalue {
        Rvalue::Use(o) => operand_ty(o, cx),
        Rvalue::Ref {
            region,
            mutability,
            place,
        } => {
            let pointee = cx.place_ty(place)?;
            cx.lookup(TyKind::Ref {
                region: *region,
                mutability: *mutability,
                pointee,
            })
        }
        Rvalue::Describe { place } => {
            let ty = cx.place_ty(place)?;
            let payload = if let TyKind::Managed(p) | TyKind::Desc(p) = tys.kind(ty) {
                *p
            } else if tys.adt_kind(ty) == Some(AdtKind::Resource) {
                ty
            } else {
                return Err(ConstraintError::NotDescribable(ty));
            };
            cx.lookup(TyKind::Desc(payload))
        }
        Rvalue::Aggregate { kind, operands } => aggregate_ty(kind, operands, cx),
        Rvalue::ErrorUnion { ok, value, ty } => {
            let found = operand_ty(value, cx)?;
            let Some(TyKind::ErrorUnion { ok: o, err }) = tys.get(*ty) else {
                return Err(ConstraintError::Aggregate {
                    reason: "error union type expected",
                });
            };
            cx.expect(if *ok { *o } else { *err }, found)?;
            Ok(*ty)
        }
        Rvalue::Unary { op, operand } => {
            let ty = operand_ty(operand, cx)?;
            let ok = match (op, tys.kind(ty)) {
                (UnOp::Not, TyKind::Bool | TyKind::Int(_)) => true,
                (UnOp::Neg, TyKind::Int(i)) => i.signed(),
                (UnOp::Neg, TyKind::Float(_)) => true,
                (UnOp::WrappingNeg, TyKind::Int(i)) => i.signed(),
                _ => false,
            };
            if ok {
                Ok(ty)
            } else {
                Err(ConstraintError::Operator {
                    reason: "unary operand type",
                })
            }
        }
        Rvalue::Binary { op, lhs, rhs } => {
            let l = operand_ty(lhs, cx)?;
            let r = operand_ty(rhs, cx)?;
            cx.expect(l, r)?;
            let kind = tys.kind(l);
            let ok = match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                    matches!(kind, TyKind::Int(_) | TyKind::Float(_))
                }
                BinOp::WrappingAdd
                | BinOp::WrappingSub
                | BinOp::WrappingMul
                | BinOp::Shl
                | BinOp::Shr => matches!(kind, TyKind::Int(_)),
                BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                    matches!(kind, TyKind::Int(_) | TyKind::Bool)
                }
                BinOp::Eq | BinOp::Ne => {
                    matches!(
                        kind,
                        TyKind::Int(_) | TyKind::Float(_) | TyKind::Bool | TyKind::Unit
                    )
                }
                BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                    matches!(kind, TyKind::Int(_) | TyKind::Float(_))
                }
            };
            if !ok {
                return Err(ConstraintError::Operator {
                    reason: "binary operand type",
                });
            }
            if op.is_comparison() {
                cx.lookup(TyKind::Bool)
            } else {
                Ok(l)
            }
        }
        Rvalue::Convert { operand, to } => {
            let from = operand_ty(operand, cx)?;
            let numeric = |t: TyId| matches!(tys.get(t), Some(TyKind::Int(_) | TyKind::Float(_)));
            if numeric(from) && numeric(*to) {
                Ok(*to)
            } else {
                Err(ConstraintError::Operator {
                    reason: "conversion between non-numeric types",
                })
            }
        }
        Rvalue::Discriminant(p) => {
            let ty = cx.place_ty(p)?;
            if tys.adt_kind(ty) == Some(AdtKind::Enum) {
                cx.lookup(TyKind::Int(IntTy::I32))
            } else {
                Err(ConstraintError::Operator {
                    reason: "discriminant of a non-enum",
                })
            }
        }
        Rvalue::ErrorArm(p) => {
            if let TyKind::ErrorUnion { .. } = tys.kind(cx.place_ty(p)?) {
                cx.lookup(TyKind::Bool)
            } else {
                Err(ConstraintError::Operator {
                    reason: "error arm of a non-error-union",
                })
            }
        }
        Rvalue::Len(p) => {
            if let TyKind::Array { .. } | TyKind::Slice(_) = tys.kind(cx.place_ty(p)?) {
                cx.lookup(TyKind::Int(IntTy::Usize))
            } else {
                Err(ConstraintError::Operator {
                    reason: "length of a non-sequence",
                })
            }
        }
        Rvalue::FnRef(f) => {
            let sig = callable(cx, f.0)?;
            if sig.type_params != 0 || sig.region_params != 0 {
                return Err(ConstraintError::Unspecified("generic function values"));
            }
            cx.lookup(TyKind::FnPtr {
                params: sig.params.clone(),
                result: sig.result,
            })
        }
        Rvalue::ForeignFnRef(_) => Err(ConstraintError::Unspecified("foreign function values")),
        Rvalue::StaticUtf(_) => Err(ConstraintError::Unspecified("static UTF-8 values")),
    }
}

fn aggregate_ty(
    kind: &AggregateKind,
    operands: &[Operand],
    cx: &ConstraintCx<'_>,
) -> Result<TyId, ConstraintError> {
    let tys = cx.body.tys;
    let found: Vec<TyId> = operands
        .iter()
        .map(|o| operand_ty(o, cx))
        .collect::<Result<_, _>>()?;
    match kind {
        AggregateKind::Tuple => cx.lookup(TyKind::Tuple(found)),
        AggregateKind::Array { element } => {
            for t in &found {
                cx.expect(*element, *t)?;
            }
            let length = u32::try_from(found.len()).map_err(|_| ConstraintError::Aggregate {
                reason: "array too long",
            })?;
            cx.lookup(TyKind::Array {
                element: *element,
                length,
            })
        }
        AggregateKind::Adt { adt, variant, args } => {
            let def = tys.get_adt(*adt).ok_or(ConstraintError::Aggregate {
                reason: "unknown declaration",
            })?;
            if def.kind == AdtKind::Resource {
                // Resource payloads only exist behind a created head.
                return Err(ConstraintError::Aggregate {
                    reason: "resource payload outside create",
                });
            }
            let v = def.variant(*variant).ok_or(ConstraintError::Aggregate {
                reason: "unknown variant",
            })?;
            if v.fields.len() != found.len() {
                return Err(ConstraintError::Arity {
                    expected: v.fields.len(),
                    found: found.len(),
                });
            }
            for (i, t) in found.iter().enumerate() {
                let declared = tys
                    .field_ty(*adt, *variant, FieldIdx::from_index(i), *args)
                    .ok_or(ConstraintError::MissingType)?;
                cx.expect(declared, *t)?;
            }
            cx.lookup(TyKind::Adt {
                adt: *adt,
                args: *args,
            })
        }
        AggregateKind::Closure { def, args } => {
            let closure = tys.closure(*def).ok_or(ConstraintError::Aggregate {
                reason: "unknown closure",
            })?;
            if closure.captures.len() != found.len() {
                return Err(ConstraintError::Arity {
                    expected: closure.captures.len(),
                    found: found.len(),
                });
            }
            for (c, t) in closure.captures.iter().zip(&found) {
                let declared = tys
                    .subst_lookup(*c, *args)
                    .ok_or(ConstraintError::MissingType)?;
                cx.expect(declared, *t)?;
            }
            cx.lookup(TyKind::Closure {
                def: *def,
                args: *args,
            })
        }
    }
}

/// Field reference of an Enum/Record aggregate component, for consumers
/// that need the declaration identity of an aggregate operand.
pub fn aggregate_field(kind: &AggregateKind, component: u32) -> Option<FieldRef> {
    match kind {
        AggregateKind::Adt { adt, variant, args } => Some(FieldRef {
            adt: *adt,
            variant: *variant,
            field: FieldIdx(component),
            args: *args,
        }),
        AggregateKind::Tuple | AggregateKind::Array { .. } | AggregateKind::Closure { .. } => None,
    }
}

/// Access permission determined by types alone: what a place may be used
/// for given the indirections it goes through and the closure kind that
/// owns its captures. Exhaustive over [`PlaceAccess`].
pub fn check_access(access: &Access<'_>, cx: &ConstraintCx<'_>) -> Result<(), ConstraintError> {
    use super::place::{DerefBase, PlaceRoot, Projection};
    use super::ty::ClosureKind;
    let place: &Place = &access.place;
    let typing = type_place(place, &cx.body)?;
    let tys = cx.body.tys;
    let env = &cx.body.body.param_env;
    let capture_kind = match place.root {
        PlaceRoot::Capture(_) => {
            let def = cx.body.body.closure.ok_or(ConstraintError::Unspecified(
                "capture outside a closure body",
            ))?;
            Some(
                tys.closure(def)
                    .ok_or(ConstraintError::Aggregate {
                        reason: "unknown closure",
                    })?
                    .kind,
            )
        }
        PlaceRoot::Local(_) => None,
    };
    let indirect = place.projections.iter().any(|p| match p {
        Projection::Deref | Projection::Index(_) | Projection::ConstIndex(_) => true,
        Projection::Field(_) | Projection::TupleField(_) | Projection::ErrorArm(_) => false,
    });
    let through_shared =
        typing.derefs.contains(&DerefBase::SharedRef) || capture_kind == Some(ClosureKind::Fn);
    let owned_root = matches!(capture_kind, None | Some(ClosureKind::FnOnce));
    let moving = |cx_place: &Place| -> Result<(), ConstraintError> {
        if indirect || !owned_root {
            return Err(ConstraintError::MoveFromBorrowed {
                place: cx_place.clone(),
            });
        }
        // Every strict prefix is inline storage; none may have a user drop.
        for len in 0..place.projections.len() {
            if let TyKind::Adt { adt, .. } = tys.kind(typing.ty_at(len))
                && tys.adt(*adt).user_drop
            {
                return Err(ConstraintError::PartialMoveOfDropValue {
                    place: cx_place.clone(),
                });
            }
        }
        Ok(())
    };
    let writing = |cx_place: &Place| -> Result<(), ConstraintError> {
        if through_shared {
            Err(ConstraintError::WriteThroughShared {
                place: cx_place.clone(),
            })
        } else {
            Ok(())
        }
    };
    match access.access {
        PlaceAccess::Copy
        | PlaceAccess::Describe
        | PlaceAccess::InspectTag
        | PlaceAccess::InspectLen
        | PlaceAccess::Borrow(Mutability::Shared)
        | PlaceAccess::CallClosure(ClosureCallMode::Shared) => Ok(()),
        PlaceAccess::Move | PlaceAccess::Drop | PlaceAccess::CallClosure(ClosureCallMode::Once) => {
            moving(place)
        }
        PlaceAccess::Borrow(Mutability::Exclusive)
        | PlaceAccess::CallClosure(ClosureCallMode::Mut)
        | PlaceAccess::ReplaceInstance => writing(place),
        PlaceAccess::Init => {
            writing(place)?;
            if (indirect || !owned_root) && tys.needs_cleanup(typing.ty(), env) {
                return Err(ConstraintError::OverwriteThroughIndirection {
                    place: place.clone(),
                });
            }
            Ok(())
        }
    }
}

pub fn check_instruction(
    kind: &InstructionKind,
    cx: &ConstraintCx<'_>,
) -> Result<(), ConstraintError> {
    for access in kind.accesses() {
        check_access(&access, cx)?;
    }
    let local = |l: Local| {
        cx.body
            .body
            .locals
            .get(l.index())
            .map(|d| d.ty)
            .ok_or(ConstraintError::UnknownLocal(l))
    };
    match kind {
        InstructionKind::Assign { dest, value } => {
            let ty = rvalue_ty(value, cx)?;
            cx.expect(cx.place_ty(dest)?, ty)
        }
        InstructionKind::Drop { place } => {
            let ty = cx.place_ty(place)?;
            match cx.body.tys.class(ty, &cx.body.body.param_env) {
                ValueClass::Copy | ValueClass::Affine => Ok(()),
                class @ ValueClass::NotValue => Err(ConstraintError::OperandClass {
                    place: place.clone(),
                    class,
                }),
            }
        }
        InstructionKind::Replace { place, value, old } => {
            let ty = cx.place_ty(place)?;
            cx.expect(ty, operand_ty(value, cx)?)?;
            cx.expect(ty, local(*old)?)
        }
        InstructionKind::StorageLive(l) | InstructionKind::StorageDead(l) => local(*l).map(|_| ()),
        InstructionKind::ScopeEnter(_) | InstructionKind::ScopeExit { .. } => Ok(()),
        InstructionKind::RegisterDefer { closure, .. } => {
            let ty = operand_ty(closure, cx)?;
            if let TyKind::Closure { .. } = cx.body.tys.kind(ty) {
                Ok(())
            } else {
                Err(ConstraintError::Operator {
                    reason: "defer of a non-closure",
                })
            }
        }
    }
}

pub fn check_terminator(term: &Terminator, cx: &ConstraintCx<'_>) -> Result<(), ConstraintError> {
    let blocks = cx.body.body.blocks.len();
    for (kind, target) in term.edges() {
        if target.index() >= blocks {
            return Err(ConstraintError::UnknownBlock(target));
        }
        for access in term.edge_accesses(kind) {
            check_access(&access, cx)?;
        }
    }
    for access in term.accesses() {
        check_access(&access, cx)?;
    }
    let tys = cx.body.tys;
    match term {
        Terminator::Goto { .. } | Terminator::Unreachable | Terminator::Trap(_) => Ok(()),
        Terminator::Switch { discr, .. } => {
            if let TyKind::Bool | TyKind::Int(_) = tys.kind(operand_ty(discr, cx)?) {
                Ok(())
            } else {
                Err(ConstraintError::Operator {
                    reason: "switch on a non-integer",
                })
            }
        }
        Terminator::Return { value } => cx.expect(cx.body.body.result, operand_ty(value, cx)?),
        Terminator::Assert { cond, .. } => {
            cx.expect(cx.lookup(TyKind::Bool)?, operand_ty(cond, cx)?)
        }
        Terminator::Call {
            callee, args, dest, ..
        } => {
            let found: Vec<TyId> = args
                .iter()
                .map(|a| operand_ty(a, cx))
                .collect::<Result<_, _>>()?;
            let dest_ty = cx.place_ty(dest)?;
            let (params, result) = callee_sig(callee, &found, dest_ty, cx)?;
            if params.len() != found.len() {
                return Err(ConstraintError::Arity {
                    expected: params.len(),
                    found: found.len(),
                });
            }
            for (p, f) in params.iter().zip(&found) {
                cx.expect(*p, *f)?;
            }
            cx.expect(dest_ty, result)
        }
    }
}

/// A function that ordinary code may call or name: not a cleanup entry.
fn callable<'a>(cx: &ConstraintCx<'a>, f: u32) -> Result<&'a FnSig, ConstraintError> {
    let sig = cx
        .sigs
        .functions
        .get(f as usize)
        .ok_or(ConstraintError::UnknownFunction(f))?;
    match sig.role {
        FnRole::Normal => Ok(sig),
        FnRole::Drop(_) => Err(ConstraintError::Operator {
            reason: "a cleanup entry is neither called nor named",
        }),
    }
}

/// C ABI value types of foreign parameters (`_zil_design.md` §12.4).
fn c_abi_value(tys: &super::ty::TyTable, ty: TyId) -> bool {
    match tys.get(ty) {
        Some(TyKind::Int(_) | TyKind::Float(_)) => true,
        Some(TyKind::Opaque(o)) => matches!(o, super::ty::OpaqueTy::ForeignHandle),
        Some(_) | None => false,
    }
}

fn callee_sig(
    callee: &Callee,
    found: &[TyId],
    dest_ty: TyId,
    cx: &ConstraintCx<'_>,
) -> Result<(Vec<TyId>, TyId), ConstraintError> {
    let tys = cx.body.tys;
    match callee {
        Callee::Direct(f) => {
            let sig = callable(cx, f.0)?;
            if sig.type_params != 0 || sig.region_params != 0 {
                return Err(ConstraintError::Unspecified("generic call instantiation"));
            }
            Ok((sig.params.clone(), sig.result))
        }
        Callee::Indirect(o) => {
            let TyKind::FnPtr { params, result } = tys.kind(operand_ty(o, cx)?) else {
                return Err(ConstraintError::Operator {
                    reason: "indirect call of a non-function",
                });
            };
            Ok((params.clone(), *result))
        }
        Callee::Closure { closure, mode } => {
            let ty = cx.place_ty(closure)?;
            let TyKind::Closure { def, args } = tys.kind(ty) else {
                return Err(ConstraintError::Operator {
                    reason: "closure call of a non-closure",
                });
            };
            let c = tys.closure(*def).ok_or(ConstraintError::Aggregate {
                reason: "unknown closure",
            })?;
            let allowed = match (c.kind, mode) {
                (super::ty::ClosureKind::Fn, _) => true,
                (super::ty::ClosureKind::FnMut, ClosureCallMode::Mut | ClosureCallMode::Once) => {
                    true
                }
                (super::ty::ClosureKind::FnOnce, ClosureCallMode::Once) => true,
                (super::ty::ClosureKind::FnMut, ClosureCallMode::Shared)
                | (
                    super::ty::ClosureKind::FnOnce,
                    ClosureCallMode::Shared | ClosureCallMode::Mut,
                ) => false,
            };
            if !allowed {
                return Err(ConstraintError::Operator {
                    reason: "closure call mode exceeds closure kind",
                });
            }
            let params = c
                .params
                .iter()
                .map(|p| {
                    tys.subst_lookup(*p, *args)
                        .ok_or(ConstraintError::MissingType)
                })
                .collect::<Result<_, _>>()?;
            let result = tys
                .subst_lookup(c.result, *args)
                .ok_or(ConstraintError::MissingType)?;
            Ok((params, result))
        }
        Callee::DynSlot { .. } => Err(ConstraintError::Unspecified("dynamic interface slots")),
        Callee::Foreign(id) => {
            let decl = cx
                .sigs
                .foreigns
                .get(id.index())
                .ok_or(ConstraintError::Operator {
                    reason: "unknown foreign function",
                })?;
            let result_ok =
                matches!(tys.get(decl.result), Some(TyKind::Unit)) || c_abi_value(tys, decl.result);
            if !result_ok || !decl.params.iter().all(|p| c_abi_value(tys, *p)) {
                return Err(ConstraintError::Operator {
                    reason: "foreign signature outside the C value ABI",
                });
            }
            Ok((decl.params.clone(), decl.result))
        }
        Callee::Intrinsic(op) => intrinsic_sig(*op, found, dest_ty, cx),
    }
}

/// Signature of an intrinsic, instantiated from its descriptor argument or
/// its destination. Intrinsics whose protocol is not specified yet are
/// rejected (`spec/semantic-kernel.md` §5: P3 protocols).
fn intrinsic_sig(
    op: IntrinsicOp,
    found: &[TyId],
    dest_ty: TyId,
    cx: &ConstraintCx<'_>,
) -> Result<(Vec<TyId>, TyId), ConstraintError> {
    use super::ty::OpaqueTy;
    let tys = cx.body.tys;
    let bad = |reason| ConstraintError::Operator { reason };
    let i64_ty = || cx.lookup(TyKind::Int(IntTy::I64));
    let bool_ty = || cx.lookup(TyKind::Bool);
    // The opaque payload of a `Managed` destination.
    let managed_dest = || match tys.get(dest_ty) {
        Some(TyKind::Managed(p)) => tys.get(*p).cloned().map(|k| (*p, k)),
        Some(_) | None => None,
    };
    // The opaque payload behind the descriptor in argument 0.
    let desc_arg = || -> Result<(TyId, OpaqueTy), ConstraintError> {
        let first = *found.first().ok_or(bad("intrinsic without a descriptor"))?;
        match tys.get(first) {
            Some(TyKind::Desc(p)) => match tys.get(*p) {
                Some(TyKind::Opaque(o)) => Ok((first, o.clone())),
                Some(_) | None => Err(bad("descriptor of the wrong resource")),
            },
            Some(_) | None => Err(bad("intrinsic without a descriptor")),
        }
    };
    let store = || match desc_arg()? {
        (d, OpaqueTy::OwnedStore { element }) => Ok((d, element)),
        (_, _) => Err(bad("store intrinsic on a non-store")),
    };
    let bytes = || match desc_arg()? {
        (d, OpaqueTy::ByteBuffer) => Ok(d),
        (_, _) => Err(bad("byte buffer intrinsic on a non-buffer")),
    };
    match op {
        IntrinsicOp::ManagedCreate => {
            let Some((_, TyKind::Adt { adt, args })) = managed_dest() else {
                return Err(bad("create without a managed resource destination"));
            };
            let def = tys.get_adt(adt).ok_or(ConstraintError::MissingType)?;
            if def.kind != AdtKind::Resource {
                return Err(bad("create of a non-resource"));
            }
            let fields = def.variants.first().map(|v| v.fields.len()).unwrap_or(0);
            let mut params = Vec::with_capacity(fields);
            for f in 0..fields {
                let ty = tys
                    .field_ty(
                        adt,
                        super::ids::VariantIdx(0),
                        FieldIdx::from_index(f),
                        args,
                    )
                    .ok_or(ConstraintError::MissingType)?;
                if tys.class(ty, &cx.body.body.param_env) != ValueClass::Copy {
                    return Err(bad("resource payload field is not Copy"));
                }
                params.push(ty);
            }
            Ok((params, dest_ty))
        }
        IntrinsicOp::StoreNew => match managed_dest() {
            Some((_, TyKind::Opaque(OpaqueTy::OwnedStore { element })))
                if !tys.holds_address(element) && !tys.may_store_address(element) =>
            {
                Ok((Vec::new(), dest_ty))
            }
            Some(_) | None => Err(bad("store.new without a store destination")),
        },
        IntrinsicOp::StoreReserve => {
            let (d, _) = store()?;
            Ok((vec![d, i64_ty()?], bool_ty()?))
        }
        IntrinsicOp::StorePush => {
            let (d, element) = store()?;
            let unit = cx.lookup(TyKind::Unit)?;
            Ok((
                vec![d, element],
                cx.lookup(TyKind::ErrorUnion {
                    ok: unit,
                    err: element,
                })?,
            ))
        }
        IntrinsicOp::StorePop => {
            let (d, element) = store()?;
            let unit = cx.lookup(TyKind::Unit)?;
            Ok((
                vec![d],
                cx.lookup(TyKind::ErrorUnion {
                    ok: element,
                    err: unit,
                })?,
            ))
        }
        IntrinsicOp::StoreLen | IntrinsicOp::StoreCapacity => {
            let (d, _) = store()?;
            Ok((vec![d], i64_ty()?))
        }
        IntrinsicOp::ByteBufferNew => match managed_dest() {
            Some((_, TyKind::Opaque(OpaqueTy::ByteBuffer))) => Ok((Vec::new(), dest_ty)),
            Some(_) | None => Err(bad("bytes.new without a buffer destination")),
        },
        IntrinsicOp::ByteBufferReserve | IntrinsicOp::ByteBufferTruncate => {
            Ok((vec![bytes()?, i64_ty()?], bool_ty()?))
        }
        IntrinsicOp::ByteBufferPush => Ok((
            vec![bytes()?, cx.lookup(TyKind::Int(IntTy::U8))?],
            bool_ty()?,
        )),
        IntrinsicOp::ByteBufferLen | IntrinsicOp::ByteBufferCapacity => {
            Ok((vec![bytes()?], i64_ty()?))
        }
        // Element and view borrows need the store borrow protocol: a live
        // element address must block growth through any descriptor.
        IntrinsicOp::StoreBorrow { .. }
        | IntrinsicOp::ByteBufferBorrow
        | IntrinsicOp::MutexCreate
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
        | IntrinsicOp::AsyncRegistrationFinishDestroy => {
            Err(ConstraintError::Unspecified(intrinsic_name(op)))
        }
    }
}

/// Stable diagnostic name of each intrinsic. Exhaustive.
pub const fn intrinsic_name(op: IntrinsicOp) -> &'static str {
    match op {
        IntrinsicOp::StoreNew => "store.new",
        IntrinsicOp::StoreReserve => "store.reserve",
        IntrinsicOp::StorePush => "store.push",
        IntrinsicOp::StorePop => "store.pop",
        IntrinsicOp::StoreLen => "store.len",
        IntrinsicOp::StoreCapacity => "store.capacity",
        IntrinsicOp::StoreBorrow {
            mutability: Mutability::Shared,
        } => "store.at",
        IntrinsicOp::StoreBorrow {
            mutability: Mutability::Exclusive,
        } => "store.at_mut",
        IntrinsicOp::ByteBufferNew => "bytes.new",
        IntrinsicOp::ByteBufferReserve => "bytes.reserve",
        IntrinsicOp::ByteBufferPush => "bytes.push",
        IntrinsicOp::ByteBufferTruncate => "bytes.truncate",
        IntrinsicOp::ByteBufferLen => "bytes.len",
        IntrinsicOp::ByteBufferCapacity => "bytes.capacity",
        IntrinsicOp::ByteBufferBorrow => "bytes.borrow",
        IntrinsicOp::MutexCreate => "mutex.create",
        IntrinsicOp::MutexLock => "mutex.lock",
        IntrinsicOp::MutexValue => "mutex.value",
        IntrinsicOp::MutexUnlock => "mutex.unlock",
        IntrinsicOp::SyncLoad => "sync.load",
        IntrinsicOp::SyncStore => "sync.store",
        IntrinsicOp::SyncFetchAdd => "sync.fetch_add",
        IntrinsicOp::SyncCompareExchange => "sync.compare_exchange",
        IntrinsicOp::TaskStart => "task.start",
        IntrinsicOp::TaskCreate => "task.create",
        IntrinsicOp::TaskYield => "task.yield",
        IntrinsicOp::TaskValid => "task.valid",
        IntrinsicOp::TaskPoll => "task.poll",
        IntrinsicOp::TaskRequestCancel => "task.request_cancel",
        IntrinsicOp::TaskPollClose => "task.poll_close",
        IntrinsicOp::TaskFinishDestroy => "task.finish_destroy",
        IntrinsicOp::RegistrationStart => "registration.start",
        IntrinsicOp::RegistrationValid => "registration.valid",
        IntrinsicOp::RegistrationClose => "registration.close",
        IntrinsicOp::AsyncRegistrationStart => "async_registration.start",
        IntrinsicOp::AsyncRegistrationValid => "async_registration.valid",
        IntrinsicOp::AsyncRegistrationRequestClose => "async_registration.request_close",
        IntrinsicOp::AsyncRegistrationPollClose => "async_registration.poll_close",
        IntrinsicOp::AsyncRegistrationFinishDestroy => "async_registration.finish_destroy",
        IntrinsicOp::ManagedCreate => "managed.create",
    }
}
