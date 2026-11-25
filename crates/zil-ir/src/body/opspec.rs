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
        Rvalue::SliceRef {
            mutability,
            place,
            start,
            end,
            ..
        } => {
            if let Some(s) = start {
                push_operand(out, s, OperandRole::Index);
            }
            if let Some(e) = end {
                push_operand(out, e, OperandRole::Index);
            }
            push_place(
                out,
                place,
                PlaceAccess::Borrow(*mutability),
                OperandRole::Target,
            );
        }
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
        Rvalue::SliceRef { start, end, .. } if start.is_some() || end.is_some() => {
            e.with(Effect::MayTrap)
        }
        Rvalue::Use(_)
        | Rvalue::Ref { .. }
        | Rvalue::SliceRef { .. }
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
            .lookup_erased(&kind)
            .ok_or(ConstraintError::MissingType)
    }
    fn expect(&self, expected: TyId, found: TyId) -> Result<(), ConstraintError> {
        if expected == found || self.body.tys.same_erased_region(expected, found) {
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
        Rvalue::SliceRef {
            region,
            mutability,
            place,
            start,
            end,
        } => {
            for bound in [start.as_ref(), end.as_ref()].into_iter().flatten() {
                let bty = operand_ty(bound, cx)?;
                if !matches!(tys.kind(bty), TyKind::Int(_)) {
                    return Err(ConstraintError::Operator {
                        reason: "slice bound must be an integer",
                    });
                }
            }
            let base_ty = cx.place_ty(place)?;
            let element =
                if let TyKind::Array { element, .. } | TyKind::Slice(element) = tys.kind(base_ty) {
                    *element
                } else {
                    return Err(ConstraintError::Operator {
                        reason: "slice of a non-sequence",
                    });
                };
            let slice_ty = cx.lookup(TyKind::Slice(element))?;
            cx.lookup(TyKind::Ref {
                region: *region,
                mutability: *mutability,
                pointee: slice_ty,
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
                        TyKind::Int(_)
                            | TyKind::Float(_)
                            | TyKind::Bool
                            | TyKind::Unit
                            | TyKind::Opaque(super::ty::OpaqueTy::ForeignHandle)
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
            if sig.type_params != 0 {
                return Err(ConstraintError::Unspecified("generic function values"));
            }
            cx.lookup(TyKind::FnPtr {
                params: sig.params.clone(),
                result: sig.result,
            })
        }
        Rvalue::ForeignFnRef(id) => {
            let decl = cx
                .sigs
                .foreigns
                .get(id.index())
                .ok_or(ConstraintError::Operator {
                    reason: "unknown foreign function",
                })?;
            cx.lookup(TyKind::Opaque(super::ty::OpaqueTy::ForeignCallback {
                params: decl.params.clone(),
                result: decl.result,
            }))
        }
        Rvalue::StaticUtf(id) => {
            if id.index() >= cx.sigs.static_count {
                return Err(ConstraintError::Operator {
                    reason: "unknown static byte literal",
                });
            }
            let u8_ty = cx.lookup(TyKind::Int(IntTy::U8))?;
            let slice_u8 = cx.lookup(TyKind::Slice(u8_ty))?;
            cx.lookup(TyKind::Ref {
                region: super::ty::Region::Static,
                mutability: Mutability::Shared,
                pointee: slice_u8,
            })
        }
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
    let owned_root = matches!(
        capture_kind,
        None | Some(ClosureKind::FnOnce) | Some(ClosureKind::FnMut)
    );
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
        FnRole::DynSlotImpl { .. } => Err(ConstraintError::Operator {
            reason: "a dyn slot implementation entry is neither called nor named directly",
        }),
    }
}

/// C ABI value types of foreign parameters (`_zil_design.md` §12.4).
fn c_abi_value(tys: &super::ty::TyTable, ty: TyId) -> bool {
    match tys.get(ty) {
        Some(TyKind::Int(_) | TyKind::Float(_)) => true,
        Some(TyKind::Opaque(o)) => matches!(
            o,
            super::ty::OpaqueTy::ForeignHandle | super::ty::OpaqueTy::ForeignCallback { .. }
        ),
        Some(_) | None => false,
    }
}

/// Pointee types allowed behind a `noescape` C pointer parameter.
fn c_abi_pointee(tys: &super::ty::TyTable, ty: TyId) -> bool {
    let mut seen = Vec::new();
    c_abi_pointee_inner(tys, ty, &mut seen)
}

fn c_abi_pointee_inner(tys: &super::ty::TyTable, ty: TyId, seen: &mut Vec<super::AdtId>) -> bool {
    if c_abi_value(tys, ty) {
        return true;
    }
    match tys.get(ty) {
        Some(TyKind::Array { element, .. }) => c_abi_pointee_inner(tys, *element, seen),
        Some(TyKind::Adt { adt, args }) => {
            if seen.contains(adt) {
                return false;
            }
            let Some(def) = tys.get_adt(*adt) else {
                return false;
            };
            if def.kind != super::ty::AdtKind::Record
                || def.user_drop
                || def.type_params != 0
                || def.region_params != 0
                || def.variants.len() != 1
            {
                return false;
            }
            seen.push(*adt);
            let ok = def.variants[0].fields.iter().all(|f| {
                let fty = tys.subst_lookup(f.ty, *args).unwrap_or(f.ty);
                c_abi_pointee_inner(tys, fty, seen)
            });
            seen.pop();
            ok
        }
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
            if sig.type_params != 0 {
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
        Callee::DynSlot { object, slot } => {
            let obj_ty = operand_ty(object, cx)?;
            let (dyn_id, obj_mut) = match tys.get(obj_ty).ok_or(ConstraintError::MissingType)? {
                TyKind::Ref {
                    mutability,
                    pointee,
                    ..
                } => {
                    if let TyKind::Dyn(d) | TyKind::OwnedDyn { def: d, .. } =
                        *tys.get(*pointee).ok_or(ConstraintError::MissingType)?
                    {
                        (d, Some(*mutability))
                    } else {
                        return Err(ConstraintError::Operator {
                            reason: "dynamic slot receiver must be a dyn or OwnedDyn reference",
                        });
                    }
                }
                TyKind::OwnedDyn { def: d, .. } => (*d, None),
                TyKind::Unit
                | TyKind::Bool
                | TyKind::Never
                | TyKind::Int(_)
                | TyKind::Float(_)
                | TyKind::Adt { .. }
                | TyKind::Tuple(_)
                | TyKind::Array { .. }
                | TyKind::Slice(_)
                | TyKind::Managed(_)
                | TyKind::Desc(_)
                | TyKind::ErrorUnion { .. }
                | TyKind::FnPtr { .. }
                | TyKind::Closure { .. }
                | TyKind::Dyn(_)
                | TyKind::Param(_)
                | TyKind::Opaque(_) => {
                    return Err(ConstraintError::Operator {
                        reason: "dynamic slot receiver must be a dyn or OwnedDyn value",
                    });
                }
            };
            let def = tys.get_dyn(dyn_id).ok_or(ConstraintError::MissingType)?;
            let sig = def
                .slots
                .get(*slot as usize)
                .ok_or(ConstraintError::Operator {
                    reason: "dynamic interface slot out of range",
                })?;
            let mut_ok = matches!(
                (sig.receiver, obj_mut),
                (Some(Mutability::Exclusive), Some(Mutability::Exclusive))
                    | (
                        Some(Mutability::Shared),
                        Some(Mutability::Shared | Mutability::Exclusive)
                    )
                    | (None, None)
            );
            if !mut_ok {
                return Err(ConstraintError::Operator {
                    reason: "dynamic slot receiver mutability mismatch",
                });
            }
            Ok((sig.params.clone(), sig.result))
        }
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
            let params_ok = decl.params.iter().enumerate().all(|(i, p)| {
                if c_abi_value(tys, *p) {
                    true
                } else if decl.noescape_params.contains(&(i as u32)) {
                    matches!(tys.get(*p), Some(TyKind::Ref { pointee, .. }) if c_abi_pointee(tys, *pointee))
                } else {
                    false
                }
            });
            if !result_ok || !params_ok {
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
        IntrinsicOp::OwnedDynCreate => {
            let [val_ty] = found else {
                return Err(bad("owned_dyn.create takes one argument"));
            };
            let Some(TyKind::ErrorUnion { ok, err }) = tys.get(dest_ty) else {
                return Err(bad("owned_dyn.create requires an error union destination"));
            };
            let Some(TyKind::OwnedDyn { context, .. }) = tys.get(*ok) else {
                return Err(bad("owned_dyn.create ok arm must be OwnedDyn"));
            };
            match context {
                None => {
                    if tys.holds_address(*val_ty) || tys.may_store_address(*val_ty) {
                        return Err(bad(
                            "owned_dyn.create cannot erase an address-holding value without a Context bound",
                        ));
                    }
                }
                Some(ctx_ty) => {
                    let val_regions = tys.regions_in(*val_ty);
                    let ctx_regions = tys.regions_in(*ctx_ty);
                    if val_regions.iter().any(|r| !ctx_regions.contains(r)) {
                        return Err(bad(
                            "owned_dyn.create provider region escapes OwnedDyn Context bound",
                        ));
                    }
                    if (tys.holds_address(*val_ty) || tys.may_store_address(*val_ty))
                        && !tys.holds_address(*ctx_ty)
                        && !tys.may_store_address(*ctx_ty)
                    {
                        return Err(bad(
                            "owned_dyn.create borrowed provider requires address-bearing Context bound",
                        ));
                    }
                    if tys.may_store_address(*val_ty) && !tys.may_store_address(*ctx_ty) {
                        return Err(bad(
                            "owned_dyn.create writable-address provider requires writable-address Context bound",
                        ));
                    }
                    if tys.holds_exclusive_ref(*val_ty) && !tys.holds_exclusive_ref(*ctx_ty) {
                        return Err(bad(
                            "owned_dyn.create exclusive-ref provider requires exclusive-ref Context bound",
                        ));
                    }
                }
            }
            let err_is_enum = matches!(
                tys.get(*err),
                Some(TyKind::Adt { adt, .. })
                    if tys.get_adt(*adt).is_some_and(|d| d.kind == AdtKind::Enum)
            );
            if !err_is_enum {
                return Err(bad("owned_dyn.create error arm must be an enum"));
            }
            Ok((vec![*val_ty], dest_ty))
        }
        IntrinsicOp::StoreNew => match managed_dest() {
            Some((_, TyKind::Opaque(OpaqueTy::OwnedStore { element })))
                if (!tys.holds_address(element) && !tys.may_store_address(element))
                    || matches!(
                        tys.get(element),
                        Some(TyKind::OwnedDyn {
                            context: Some(_),
                            ..
                        })
                    ) =>
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
        IntrinsicOp::StoreBorrow { mutability } => {
            let (d, element) = store()?;
            Ok((
                vec![d, i64_ty()?],
                cx.lookup(TyKind::Ref {
                    region: super::ty::Region::Static,
                    mutability,
                    pointee: element,
                })?,
            ))
        }
        IntrinsicOp::ByteBufferBorrow => {
            let d = bytes()?;
            let u8_ty = cx.lookup(TyKind::Int(IntTy::U8))?;
            let slice_u8 = cx.lookup(TyKind::Slice(u8_ty))?;
            let mutability = match tys.get(dest_ty) {
                Some(TyKind::Ref { mutability, .. }) => *mutability,
                _ => Mutability::Shared,
            };
            Ok((
                vec![d],
                cx.lookup(TyKind::Ref {
                    region: super::ty::Region::Static,
                    mutability,
                    pointee: slice_u8,
                })?,
            ))
        }
        IntrinsicOp::MutexCreate => {
            let [init_ty] = found else {
                return Err(bad("mutex.create takes one initial value"));
            };
            if tys.holds_address(*init_ty) || tys.may_store_address(*init_ty) {
                return Err(bad("mutex.create payload cannot hold addresses"));
            }
            let Some(TyKind::ErrorUnion { ok, err }) = tys.get(dest_ty) else {
                return Err(bad("mutex.create requires an error union destination"));
            };
            let ok_matches = matches!(
                tys.get(*ok),
                Some(TyKind::Managed(inner))
                    if matches!(tys.get(*inner), Some(TyKind::Opaque(OpaqueTy::Mutex { payload })) if *payload == *init_ty)
            );
            let err_is_enum = matches!(
                tys.get(*err),
                Some(TyKind::Adt { adt, .. })
                    if tys.get_adt(*adt).is_some_and(|d| d.kind == AdtKind::Enum)
            );
            if !ok_matches || !err_is_enum {
                return Err(bad(
                    "mutex.create destination must be Error!Managed<Mutex<T>>",
                ));
            }
            Ok((vec![*init_ty], dest_ty))
        }
        IntrinsicOp::MutexLock => {
            let (d, OpaqueTy::Mutex { payload }) = desc_arg()? else {
                return Err(bad("mutex.lock requires a Mutex descriptor"));
            };
            let Some(TyKind::Opaque(OpaqueTy::MutexGuard { payload: gp, .. })) = tys.get(dest_ty)
            else {
                return Err(bad("mutex.lock destination must be MutexGuard<T>"));
            };
            if *gp != payload {
                return Err(bad("mutex.lock guard payload mismatch"));
            }
            Ok((vec![d], dest_ty))
        }
        IntrinsicOp::MutexValue => {
            let [guard_ref] = found else {
                return Err(bad("mutex.value takes &mut MutexGuard<T>"));
            };
            let Some(TyKind::Ref {
                mutability: Mutability::Exclusive,
                pointee,
                ..
            }) = tys.get(*guard_ref)
            else {
                return Err(bad("mutex.value requires &mut MutexGuard<T>"));
            };
            let Some(TyKind::Opaque(OpaqueTy::MutexGuard { payload, .. })) = tys.get(*pointee)
            else {
                return Err(bad("mutex.value requires &mut MutexGuard<T>"));
            };
            let out_ty = cx.lookup(TyKind::Ref {
                region: super::ty::Region::Static,
                mutability: Mutability::Exclusive,
                pointee: *payload,
            })?;
            Ok((vec![*guard_ref], out_ty))
        }
        IntrinsicOp::ThreadScope => {
            let [worker_ty, cell_ty, parent_ty] = found else {
                return Err(bad("thread.scope takes (worker, cell, parent_callback)"));
            };
            let Some(TyKind::FnPtr {
                params: wparams,
                result: r1,
            }) = tys.get(*worker_ty)
            else {
                return Err(bad("thread.scope worker must be a function pointer"));
            };
            if wparams.as_slice() != [*cell_ty] {
                return Err(bad("thread.scope worker parameter must match cell"));
            }
            let Some(TyKind::Closure { def, args }) = tys.get(*parent_ty) else {
                return Err(bad("thread.scope parent must be a closure"));
            };
            let cdef = tys.closure(*def).ok_or(ConstraintError::MissingType)?;
            if !cdef.params.is_empty() {
                return Err(bad("thread.scope parent closure takes no parameters"));
            }
            let r2 = tys
                .subst_lookup(cdef.result, *args)
                .ok_or(ConstraintError::MissingType)?;
            let is_task = match tys.get(*cell_ty) {
                Some(TyKind::Managed(inner)) => match tys.get(*inner) {
                    Some(TyKind::Opaque(OpaqueTy::Task { context: None, .. })) => true,
                    Some(TyKind::Opaque(OpaqueTy::Task {
                        context: Some(_), ..
                    })) => {
                        return Err(bad(
                            "thread.scope cannot transfer a Task with a borrowed Context",
                        ));
                    }
                    _ => false,
                },
                _ => false,
            };
            if is_task {
                let Some(TyKind::Adt {
                    adt,
                    args: dest_args,
                }) = tys.get(dest_ty)
                else {
                    return Err(bad(
                        "thread.scope task transfer destination must be TaskThreadScopeResult enum",
                    ));
                };
                let adef = tys.get_adt(*adt).ok_or(ConstraintError::MissingType)?;
                if adef.kind != AdtKind::Enum || adef.variants.len() != 2 {
                    return Err(bad(
                        "thread.scope task transfer destination must be a 2-variant enum",
                    ));
                }
                let v0 = &adef.variants[0].fields;
                let v1 = &adef.variants[1].fields;
                if v0.len() != 1 || v1.len() != 2 {
                    return Err(bad(
                        "thread.scope task transfer enum variants must be Failed(T) and Completed(R1, R2)",
                    ));
                }
                let v0_t = tys
                    .subst_lookup(v0[0].ty, *dest_args)
                    .ok_or(ConstraintError::MissingType)?;
                let v1_r1 = tys
                    .subst_lookup(v1[0].ty, *dest_args)
                    .ok_or(ConstraintError::MissingType)?;
                let v1_r2 = tys
                    .subst_lookup(v1[1].ty, *dest_args)
                    .ok_or(ConstraintError::MissingType)?;
                if !tys.same_erased_region(v0_t, *cell_ty)
                    || !tys.same_erased_region(v1_r1, *r1)
                    || !tys.same_erased_region(v1_r2, r2)
                {
                    return Err(bad(
                        "thread.scope task transfer enum payload types do not match (Task, R1, R2)",
                    ));
                }
                return Ok((vec![*worker_ty, *cell_ty, *parent_ty], dest_ty));
            }
            let out_ty = cx.lookup(TyKind::Tuple(vec![bool_ty()?, *r1, r2]))?;
            Ok((vec![*worker_ty, *cell_ty, *parent_ty], out_ty))
        }
        IntrinsicOp::TaskCreate => {
            let Some((&worker_ty, ctx_args)) = found.split_first() else {
                return Err(bad("task.create requires a worker function"));
            };
            let Some(TyKind::FnPtr { params, result }) = tys.get(worker_ty) else {
                return Err(bad("task.create worker must be a function pointer"));
            };
            if params.len() != ctx_args.len()
                || !params
                    .iter()
                    .zip(ctx_args)
                    .all(|(p, c)| tys.same_erased_region(*p, *c))
            {
                return Err(bad(
                    "task.create worker parameters must match context arguments",
                ));
            }
            let Some(TyKind::ErrorUnion { ok, err }) = tys.get(dest_ty) else {
                return Err(bad("task.create requires an error union destination"));
            };
            let ok_matches = matches!(
                tys.get(*ok),
                Some(TyKind::Managed(inner))
                    if matches!(
                        tys.get(*inner),
                        Some(TyKind::Opaque(OpaqueTy::Task { result: tr, .. }))
                            if tys.same_erased_region(*tr, *result)
                    )
            );
            let err_is_adt = matches!(
                tys.get(*err),
                Some(TyKind::Adt { adt, .. })
                    if tys.get_adt(*adt).is_some_and(|d| matches!(d.kind, AdtKind::Enum | AdtKind::Package))
            );
            if !ok_matches || !err_is_adt {
                return Err(bad(
                    "task.create destination must be Error!Managed<Task<T>>",
                ));
            }
            Ok((found.to_vec(), dest_ty))
        }
        IntrinsicOp::TaskValid => {
            let [task_arg] = found else {
                return Err(bad("task.valid takes one task handle"));
            };
            let ok_arg = matches!(
                tys.get(*task_arg),
                Some(TyKind::Desc(inner) | TyKind::Managed(inner))
                    if matches!(tys.get(*inner), Some(TyKind::Opaque(OpaqueTy::Task { .. })))
            ) || matches!(
                tys.get(*task_arg),
                Some(TyKind::Ref { pointee, .. })
                    if matches!(
                        tys.get(*pointee),
                        Some(TyKind::Managed(inner) | TyKind::Desc(inner))
                            if matches!(tys.get(*inner), Some(TyKind::Opaque(OpaqueTy::Task { .. })))
                    )
            );
            if !ok_arg {
                return Err(bad("task.valid expects a Task handle"));
            }
            Ok((vec![*task_arg], bool_ty()?))
        }
        IntrinsicOp::TaskYield => Ok((Vec::new(), cx.lookup(TyKind::Unit)?)),
        IntrinsicOp::TaskPoll => {
            let [task_ref] = found else {
                return Err(bad("task.poll takes &mut Managed<Task<T>>"));
            };
            let ok_ref = matches!(
                tys.get(*task_ref),
                Some(TyKind::Ref { mutability: Mutability::Exclusive, pointee, .. })
                    if matches!(
                        tys.get(*pointee),
                        Some(TyKind::Managed(inner))
                            if matches!(tys.get(*inner), Some(TyKind::Opaque(OpaqueTy::Task { .. })))
                    )
            );
            if !ok_ref {
                return Err(bad("task.poll requires &mut Managed<Task<T>>"));
            }
            if !matches!(tys.get(dest_ty), Some(TyKind::ErrorUnion { .. })) {
                return Err(bad("task.poll destination must be an error union"));
            }
            Ok((vec![*task_ref], dest_ty))
        }
        IntrinsicOp::TaskRequestCancel => {
            let [task_ref] = found else {
                return Err(bad("task.request_cancel takes &mut Managed<Task<T>>"));
            };
            let ok_ref = matches!(
                tys.get(*task_ref),
                Some(TyKind::Ref { mutability: Mutability::Exclusive, pointee, .. })
                    if matches!(
                        tys.get(*pointee),
                        Some(TyKind::Managed(inner))
                            if matches!(tys.get(*inner), Some(TyKind::Opaque(OpaqueTy::Task { .. })))
                    )
            );
            if !ok_ref {
                return Err(bad("task.request_cancel requires &mut Managed<Task<T>>"));
            }
            Ok((vec![*task_ref], cx.lookup(TyKind::Unit)?))
        }
        IntrinsicOp::TaskPollClose => {
            let [task_ref] = found else {
                return Err(bad("task.poll_close takes &mut Managed<Task<T>>"));
            };
            let ok_ref = matches!(
                tys.get(*task_ref),
                Some(TyKind::Ref { mutability: Mutability::Exclusive, pointee, .. })
                    if matches!(
                        tys.get(*pointee),
                        Some(TyKind::Managed(inner))
                            if matches!(tys.get(*inner), Some(TyKind::Opaque(OpaqueTy::Task { .. })))
                    )
            );
            if !ok_ref {
                return Err(bad("task.poll_close requires &mut Managed<Task<T>>"));
            }
            let dest_is_enum = matches!(
                tys.get(dest_ty),
                Some(TyKind::Adt { adt, .. })
                    if tys.get_adt(*adt).is_some_and(|d| d.kind == AdtKind::Enum)
            );
            if !dest_is_enum {
                return Err(bad("task.poll_close destination must be Poll<()>"));
            }
            Ok((vec![*task_ref], dest_ty))
        }
        IntrinsicOp::TaskFinishDestroy => {
            let [task_ty] = found else {
                return Err(bad("task.finish_destroy takes Managed<Task<T>>"));
            };
            let ok = matches!(
                tys.get(*task_ty),
                Some(TyKind::Managed(inner))
                    if matches!(tys.get(*inner), Some(TyKind::Opaque(OpaqueTy::Task { .. })))
            );
            if !ok {
                return Err(bad("task.finish_destroy requires Managed<Task<T>>"));
            }
            Ok((vec![*task_ty], cx.lookup(TyKind::Unit)?))
        }
        IntrinsicOp::AsyncRegistrationStart => {
            let [reg, req, drain, env] = found else {
                return Err(bad("async_registration.start takes 4 arguments"));
            };
            if !is_foreign_register_cb(tys, *reg) {
                return Err(bad(
                    "async_registration.start register callback must have ABI extern \"C\" fn(extern \"C\" fn(c_handle, i32) -> i64, c_handle) -> c_handle",
                ));
            }
            if !is_foreign_close_cb(tys, *req) {
                return Err(bad(
                    "async_registration.start request_close callback must have ABI extern \"C\" fn(c_handle)",
                ));
            }
            if !is_foreign_drain_cb(tys, *drain) {
                return Err(bad(
                    "async_registration.start poll_drain callback must have ABI extern \"C\" fn(c_handle) -> i32",
                ));
            }
            if tys.holds_address(*env) || tys.may_store_address(*env) {
                return Err(bad(
                    "async_registration.start environment cannot hold addresses (ClosedEnvironment)",
                ));
            }
            if !ty_is_transfer_safe(tys, *env) {
                return Err(bad(
                    "async_registration.start environment must satisfy Transfer (no Task/MutexGuard/Registration/OwnedDyn)",
                ));
            }
            let ok_dest = matches!(
                tys.get(dest_ty),
                Some(TyKind::Managed(inner))
                    if matches!(tys.get(*inner), Some(TyKind::Opaque(OpaqueTy::AsyncRegistration { environment })) if *environment == *env)
            );
            if !ok_dest {
                return Err(bad(
                    "async_registration.start destination must be Managed<AsyncRegistration<F>>",
                ));
            }
            Ok((vec![*reg, *req, *drain, *env], dest_ty))
        }
        IntrinsicOp::AsyncRegistrationValid => {
            let (d, OpaqueTy::AsyncRegistration { .. }) = desc_arg()? else {
                return Err(bad(
                    "async_registration.valid requires AsyncRegistration descriptor",
                ));
            };
            Ok((vec![d], bool_ty()?))
        }
        IntrinsicOp::AsyncRegistrationRequestClose => {
            let [reg_ref] = found else {
                return Err(bad(
                    "async_registration.request_close takes &mut Managed<AsyncRegistration<F>>",
                ));
            };
            let ok_ref = matches!(
                tys.get(*reg_ref),
                Some(TyKind::Ref { mutability: Mutability::Exclusive, pointee, .. })
                    if matches!(
                        tys.get(*pointee),
                        Some(TyKind::Managed(inner))
                            if matches!(tys.get(*inner), Some(TyKind::Opaque(OpaqueTy::AsyncRegistration { .. })))
                    )
            );
            if !ok_ref {
                return Err(bad(
                    "async_registration.request_close requires &mut Managed<AsyncRegistration<F>>",
                ));
            }
            Ok((vec![*reg_ref], cx.lookup(TyKind::Unit)?))
        }
        IntrinsicOp::AsyncRegistrationPollClose => {
            let [reg_ref] = found else {
                return Err(bad(
                    "async_registration.poll_close takes &mut Managed<AsyncRegistration<F>>",
                ));
            };
            let ok_ref = matches!(
                tys.get(*reg_ref),
                Some(TyKind::Ref { mutability: Mutability::Exclusive, pointee, .. })
                    if matches!(
                        tys.get(*pointee),
                        Some(TyKind::Managed(inner))
                            if matches!(tys.get(*inner), Some(TyKind::Opaque(OpaqueTy::AsyncRegistration { .. })))
                    )
            );
            if !ok_ref {
                return Err(bad(
                    "async_registration.poll_close requires &mut Managed<AsyncRegistration<F>>",
                ));
            }
            let dest_is_enum = matches!(
                tys.get(dest_ty),
                Some(TyKind::Adt { adt, .. })
                    if tys.get_adt(*adt).is_some_and(|d| d.kind == AdtKind::Enum)
            );
            if !dest_is_enum {
                return Err(bad(
                    "async_registration.poll_close destination must be Poll<()>",
                ));
            }
            Ok((vec![*reg_ref], dest_ty))
        }
        IntrinsicOp::AsyncRegistrationFinishDestroy => {
            let [reg_ty] = found else {
                return Err(bad(
                    "async_registration.finish_destroy takes Managed<AsyncRegistration<F>>",
                ));
            };
            let ok = matches!(
                tys.get(*reg_ty),
                Some(TyKind::Managed(inner))
                    if matches!(tys.get(*inner), Some(TyKind::Opaque(OpaqueTy::AsyncRegistration { .. })))
            );
            if !ok {
                return Err(bad(
                    "async_registration.finish_destroy requires Managed<AsyncRegistration<F>>",
                ));
            }
            Ok((vec![*reg_ty], cx.lookup(TyKind::Unit)?))
        }
        IntrinsicOp::RegistrationStart => {
            let [reg, close, env] = found else {
                return Err(bad("registration.start takes 3 arguments"));
            };
            if !is_foreign_register_cb(tys, *reg) {
                return Err(bad(
                    "registration.start register callback must have ABI extern \"C\" fn(extern \"C\" fn(c_handle, i32) -> i64, c_handle) -> c_handle",
                ));
            }
            if !is_foreign_close_cb(tys, *close) {
                return Err(bad(
                    "registration.start close callback must have ABI extern \"C\" fn(c_handle)",
                ));
            }
            if tys.holds_address(*env) || tys.may_store_address(*env) {
                return Err(bad("registration.start environment cannot hold addresses"));
            }
            let ok_dest = matches!(
                tys.get(dest_ty),
                Some(TyKind::Managed(inner))
                    if matches!(tys.get(*inner), Some(TyKind::Opaque(OpaqueTy::Registration { environment })) if *environment == *env)
            );
            if !ok_dest {
                return Err(bad(
                    "registration.start destination must be Managed<Registration<F>>",
                ));
            }
            Ok((vec![*reg, *close, *env], dest_ty))
        }
        IntrinsicOp::RegistrationValid => {
            let (d, OpaqueTy::Registration { .. }) = desc_arg()? else {
                return Err(bad("registration.valid requires Registration descriptor"));
            };
            Ok((vec![d], bool_ty()?))
        }
        IntrinsicOp::RegistrationClose => {
            let [reg_ty] = found else {
                return Err(bad("registration.close takes Managed<Registration<F>>"));
            };
            let ok = matches!(
                tys.get(*reg_ty),
                Some(TyKind::Managed(inner))
                    if matches!(tys.get(*inner), Some(TyKind::Opaque(OpaqueTy::Registration { .. })))
            );
            if !ok {
                return Err(bad("registration.close requires Managed<Registration<F>>"));
            }
            Ok((vec![*reg_ty], cx.lookup(TyKind::Unit)?))
        }
        IntrinsicOp::MutexUnlock => {
            let [guard_ty] = found else {
                return Err(bad("mutex.unlock takes one MutexGuard<T>"));
            };
            if !matches!(
                tys.get(*guard_ty),
                Some(TyKind::Opaque(OpaqueTy::MutexGuard { .. }))
            ) {
                return Err(bad("mutex.unlock requires MutexGuard<T>"));
            }
            Ok((vec![*guard_ty], cx.lookup(TyKind::Unit)?))
        }
        IntrinsicOp::SyncInit => {
            let [init_ty] = found else {
                return Err(bad("sync.init takes one initial integer value"));
            };
            let Some(TyKind::Opaque(OpaqueTy::SyncCell(kind))) = tys.get(dest_ty) else {
                return Err(bad("sync.init destination must be SyncCell"));
            };
            let elem_ty = cx.lookup(TyKind::Int(match kind {
                super::ty::SyncCellKind::I32 => IntTy::I32,
                super::ty::SyncCellKind::I64 => IntTy::I64,
                super::ty::SyncCellKind::Usize => IntTy::Usize,
            }))?;
            if *init_ty != elem_ty {
                return Err(bad("sync.init initial value does not match SyncCell width"));
            }
            Ok((vec![elem_ty], dest_ty))
        }
        IntrinsicOp::SyncLoad | IntrinsicOp::SyncLoadOrdered(_) => {
            if let IntrinsicOp::SyncLoadOrdered(order) = op
                && !order.is_valid_load()
            {
                return Err(bad(
                    "sync.load_ordered forbids Release and AcqRel memory orderings",
                ));
            }
            let [cell_ref] = found else {
                return Err(bad("sync.load takes &Atomic*"));
            };
            let Some(TyKind::Ref {
                mutability: Mutability::Shared,
                pointee,
                ..
            }) = tys.get(*cell_ref)
            else {
                return Err(bad("sync.load requires &Atomic*"));
            };
            let Some(TyKind::Opaque(OpaqueTy::SyncCell(kind))) = tys.get(*pointee) else {
                return Err(bad("sync.load requires &Atomic*"));
            };
            let elem_ty = cx.lookup(TyKind::Int(match kind {
                super::ty::SyncCellKind::I32 => IntTy::I32,
                super::ty::SyncCellKind::I64 => IntTy::I64,
                super::ty::SyncCellKind::Usize => IntTy::Usize,
            }))?;
            Ok((vec![*cell_ref], elem_ty))
        }
        IntrinsicOp::SyncStore | IntrinsicOp::SyncStoreOrdered(_) => {
            if let IntrinsicOp::SyncStoreOrdered(order) = op
                && !order.is_valid_store()
            {
                return Err(bad(
                    "sync.store_ordered forbids Acquire and AcqRel memory orderings",
                ));
            }
            let [cell_ref, val_ty] = found else {
                return Err(bad("sync.store takes (&Atomic*, value)"));
            };
            let Some(TyKind::Ref {
                mutability: Mutability::Shared,
                pointee,
                ..
            }) = tys.get(*cell_ref)
            else {
                return Err(bad("sync.store requires &Atomic*"));
            };
            let Some(TyKind::Opaque(OpaqueTy::SyncCell(kind))) = tys.get(*pointee) else {
                return Err(bad("sync.store requires &Atomic*"));
            };
            let elem_ty = cx.lookup(TyKind::Int(match kind {
                super::ty::SyncCellKind::I32 => IntTy::I32,
                super::ty::SyncCellKind::I64 => IntTy::I64,
                super::ty::SyncCellKind::Usize => IntTy::Usize,
            }))?;
            if *val_ty != elem_ty {
                return Err(bad("sync.store value does not match SyncCell width"));
            }
            Ok((vec![*cell_ref, elem_ty], cx.lookup(TyKind::Unit)?))
        }
        IntrinsicOp::SyncFetchAdd
        | IntrinsicOp::SyncFetchAddOrdered(_)
        | IntrinsicOp::SyncExchangeOrdered(_) => {
            let [cell_ref, delta_ty] = found else {
                return Err(bad("sync.fetch_add takes (&Atomic*, delta)"));
            };
            let Some(TyKind::Ref {
                mutability: Mutability::Shared,
                pointee,
                ..
            }) = tys.get(*cell_ref)
            else {
                return Err(bad("sync.fetch_add requires &Atomic*"));
            };
            let Some(TyKind::Opaque(OpaqueTy::SyncCell(kind))) = tys.get(*pointee) else {
                return Err(bad("sync.fetch_add requires &Atomic*"));
            };
            let elem_ty = cx.lookup(TyKind::Int(match kind {
                super::ty::SyncCellKind::I32 => IntTy::I32,
                super::ty::SyncCellKind::I64 => IntTy::I64,
                super::ty::SyncCellKind::Usize => IntTy::Usize,
            }))?;
            if *delta_ty != elem_ty {
                return Err(bad("sync.fetch_add delta does not match SyncCell width"));
            }
            Ok((vec![*cell_ref, elem_ty], elem_ty))
        }
        IntrinsicOp::SyncCompareExchange | IntrinsicOp::SyncCompareExchangeOrdered { .. } => {
            if let IntrinsicOp::SyncCompareExchangeOrdered { success, failure } = op
                && !super::inst::MemoryOrder::is_valid_cas(success, failure)
            {
                return Err(bad(
                    "sync.compare_exchange_ordered failure ordering cannot be Release/AcqRel or stronger than success ordering",
                ));
            }
            let [cell_ref, exp_ty, des_ty] = found else {
                return Err(bad(
                    "sync.compare_exchange takes (&Atomic*, expected, desired)",
                ));
            };
            let Some(TyKind::Ref {
                mutability: Mutability::Shared,
                pointee,
                ..
            }) = tys.get(*cell_ref)
            else {
                return Err(bad("sync.compare_exchange requires &Atomic*"));
            };
            let Some(TyKind::Opaque(OpaqueTy::SyncCell(kind))) = tys.get(*pointee) else {
                return Err(bad("sync.compare_exchange requires &Atomic*"));
            };
            let elem_ty = cx.lookup(TyKind::Int(match kind {
                super::ty::SyncCellKind::I32 => IntTy::I32,
                super::ty::SyncCellKind::I64 => IntTy::I64,
                super::ty::SyncCellKind::Usize => IntTy::Usize,
            }))?;
            if *exp_ty != elem_ty || *des_ty != elem_ty {
                return Err(bad(
                    "sync.compare_exchange operands do not match SyncCell width",
                ));
            }
            let out_ty = cx.lookup(TyKind::Tuple(vec![elem_ty, bool_ty()?]))?;
            Ok((vec![*cell_ref, elem_ty, elem_ty], out_ty))
        }
        IntrinsicOp::CNull => {
            if !found.is_empty() {
                return Err(bad("c_null takes no arguments"));
            }
            Ok((
                Vec::new(),
                cx.lookup(TyKind::Opaque(OpaqueTy::ForeignHandle))?,
            ))
        }
        IntrinsicOp::CCallback => {
            let [fn_ty] = found else {
                return Err(bad("c_callback takes one function argument"));
            };
            let Some(TyKind::FnPtr { params, result }) = tys.get(*fn_ty).cloned() else {
                return Err(bad("c_callback requires a non-capturing function pointer"));
            };
            let is_c_scalar = |t: TyId| {
                matches!(
                    tys.get(t),
                    Some(
                        TyKind::Int(_)
                            | TyKind::Float(_)
                            | TyKind::Bool
                            | TyKind::Opaque(OpaqueTy::ForeignHandle)
                    )
                )
            };
            if !params.iter().all(|&p| is_c_scalar(p))
                || !(is_c_scalar(result) || matches!(tys.get(result), Some(TyKind::Unit)))
            {
                return Err(bad(
                    "c_callback requires C-ABI scalar parameters and scalar or unit return type",
                ));
            }
            let out_ty = cx.lookup(TyKind::Opaque(OpaqueTy::ForeignCallback { params, result }))?;
            Ok((vec![*fn_ty], out_ty))
        }
        IntrinsicOp::ScopedCallback => {
            let [reg_ty, close_ty, f_ref_ty, body_ty] = found else {
                return Err(bad(
                    "scoped_callback takes (register, close, &mut callback, body)",
                ));
            };
            let i32_ty = cx.lookup(TyKind::Int(IntTy::I32))?;
            let i64_ty = cx.lookup(TyKind::Int(IntTy::I64))?;
            let handle_ty = cx.lookup(TyKind::Opaque(OpaqueTy::ForeignHandle))?;
            let unit_ty = cx.lookup(TyKind::Unit)?;
            let cb_ty = cx.lookup(TyKind::Opaque(OpaqueTy::ForeignCallback {
                params: vec![handle_ty, i32_ty],
                result: i64_ty,
            }))?;
            let expected_reg = cx.lookup(TyKind::Opaque(OpaqueTy::ForeignCallback {
                params: vec![cb_ty, handle_ty],
                result: handle_ty,
            }))?;
            let expected_close = cx.lookup(TyKind::Opaque(OpaqueTy::ForeignCallback {
                params: vec![handle_ty],
                result: unit_ty,
            }))?;
            if *reg_ty != expected_reg {
                return Err(bad(
                    "scoped_callback register must have signature fn(extern \"C\" fn(c_handle, i32)->i64, c_handle)->c_handle",
                ));
            }
            if *close_ty != expected_close {
                return Err(bad(
                    "scoped_callback close must have signature fn(c_handle)->void",
                ));
            }
            let Some(TyKind::Ref {
                mutability: Mutability::Exclusive,
                pointee: f_ty,
                ..
            }) = tys.get(*f_ref_ty)
            else {
                return Err(bad("scoped_callback callback must be passed as &mut F"));
            };
            if tys.holds_address(*f_ty) || tys.may_store_address(*f_ty) {
                return Err(bad(
                    "scoped_callback callback environment cannot capture borrowed addresses",
                ));
            }
            let f_ok = match tys.get(*f_ty) {
                Some(TyKind::Closure { def, args }) => tys.closure(*def).is_some_and(|cdef| {
                    matches!(
                        cdef.kind,
                        super::ty::ClosureKind::Fn | super::ty::ClosureKind::FnMut
                    ) && cdef.params.len() == 1
                        && tys.subst_lookup(cdef.params[0], *args) == Some(i32_ty)
                        && tys.subst_lookup(cdef.result, *args) == Some(i32_ty)
                }),
                Some(TyKind::FnPtr { params, result }) => {
                    params.as_slice() == [i32_ty] && *result == i32_ty
                }
                _ => false,
            };
            if !f_ok {
                return Err(bad(
                    "scoped_callback callback must be Fn(i32)->i32 or FnMut(i32)->i32",
                ));
            }
            let body_ok = match tys.get(*body_ty) {
                Some(TyKind::Closure { def, args }) => tys.closure(*def).is_some_and(|cdef| {
                    cdef.params.is_empty() && tys.subst_lookup(cdef.result, *args) == Some(i32_ty)
                }),
                Some(TyKind::FnPtr { params, result }) => params.is_empty() && *result == i32_ty,
                _ => false,
            };
            if !body_ok {
                return Err(bad(
                    "scoped_callback body must be a 0-argument callable returning i32",
                ));
            }
            let out_ty = cx.lookup(TyKind::Tuple(vec![bool_ty()?, i32_ty]))?;
            Ok((vec![*reg_ty, *close_ty, *f_ref_ty, *body_ty], out_ty))
        }
        IntrinsicOp::TaskStart => {
            let Some((&worker_ty, ctx_args)) = found.split_first() else {
                return Err(bad("task.start requires a worker function"));
            };
            let Some(TyKind::FnPtr { params, result }) = tys.get(worker_ty) else {
                return Err(bad("task.start worker must be a function pointer"));
            };
            if params.len() != ctx_args.len()
                || !params
                    .iter()
                    .zip(ctx_args)
                    .all(|(p, c)| tys.same_erased_region(*p, *c))
            {
                return Err(bad(
                    "task.start worker parameters must match context arguments",
                ));
            }
            let dest_matches = matches!(
                tys.get(dest_ty),
                Some(TyKind::Managed(inner))
                    if matches!(
                        tys.get(*inner),
                        Some(TyKind::Opaque(OpaqueTy::Task { result: tr, .. }))
                            if tys.same_erased_region(*tr, *result)
                    )
            );
            if !dest_matches {
                return Err(bad("task.start destination must be Managed<Task<T>>"));
            }
            Ok((found.to_vec(), dest_ty))
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
        IntrinsicOp::SyncInit => "sync.init",
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
        IntrinsicOp::OwnedDynCreate => "owned_dyn.create",
        IntrinsicOp::ThreadScope => "thread.scope",
        IntrinsicOp::ScopedCallback => "scoped_callback",
        IntrinsicOp::SyncLoadOrdered(_) => "sync.load_ordered",
        IntrinsicOp::SyncStoreOrdered(_) => "sync.store_ordered",
        IntrinsicOp::SyncFetchAddOrdered(_) => "sync.fetch_add_ordered",
        IntrinsicOp::SyncExchangeOrdered(_) => "sync.exchange_ordered",
        IntrinsicOp::SyncCompareExchangeOrdered { .. } => "sync.compare_exchange_ordered",
        IntrinsicOp::CNull => "c_null",
        IntrinsicOp::CCallback => "c_callback",
    }
}

fn is_foreign_register_cb(tys: &super::ty::TyTable, ty: TyId) -> bool {
    use super::ty::{IntTy, OpaqueTy};
    let Some(TyKind::Opaque(OpaqueTy::ForeignCallback { params, result })) = tys.get(ty) else {
        return false;
    };
    if params.len() != 2 {
        return false;
    }
    let cb_ok = matches!(
        tys.get(params[0]),
        Some(TyKind::Opaque(OpaqueTy::ForeignCallback {
            params: inner_params,
            result: inner_res,
        })) if inner_params.len() == 2
            && matches!(tys.get(inner_params[0]), Some(TyKind::Opaque(OpaqueTy::ForeignHandle)))
            && matches!(tys.get(inner_params[1]), Some(TyKind::Int(IntTy::I32)))
            && matches!(tys.get(*inner_res), Some(TyKind::Int(IntTy::I64)))
    );
    let ctx_ok = matches!(
        tys.get(params[1]),
        Some(TyKind::Opaque(OpaqueTy::ForeignHandle))
    );
    let res_ok = matches!(
        tys.get(*result),
        Some(TyKind::Opaque(OpaqueTy::ForeignHandle))
    );
    cb_ok && ctx_ok && res_ok
}

fn is_foreign_close_cb(tys: &super::ty::TyTable, ty: TyId) -> bool {
    use super::ty::OpaqueTy;
    matches!(
        tys.get(ty),
        Some(TyKind::Opaque(OpaqueTy::ForeignCallback { params, result }))
            if params.len() == 1
                && matches!(tys.get(params[0]), Some(TyKind::Opaque(OpaqueTy::ForeignHandle)))
                && matches!(tys.get(*result), Some(TyKind::Unit))
    )
}

fn is_foreign_drain_cb(tys: &super::ty::TyTable, ty: TyId) -> bool {
    use super::ty::{IntTy, OpaqueTy};
    matches!(
        tys.get(ty),
        Some(TyKind::Opaque(OpaqueTy::ForeignCallback { params, result }))
            if params.len() == 1
                && matches!(tys.get(params[0]), Some(TyKind::Opaque(OpaqueTy::ForeignHandle)))
                && matches!(tys.get(*result), Some(TyKind::Int(IntTy::I32)))
    )
}

/// Checks whether a value of type `ty` satisfies the closed `Transfer` contract
/// required for cross-thread `AsyncRegistration` environments: no references,
/// descriptors, thread-affine guards/tasks/registrations, or uncertified erased `dyn` values.
fn ty_is_transfer_safe(tys: &super::ty::TyTable, ty: TyId) -> bool {
    use super::ty::{OpaqueTy, TyTable};
    fn walk(tys: &TyTable, ty: TyId, seen: &mut Vec<TyId>) -> bool {
        if seen.contains(&ty) {
            return true;
        }
        seen.push(ty);
        let Some(kind) = tys.get(ty) else {
            return false;
        };
        match kind {
            TyKind::Unit
            | TyKind::Never
            | TyKind::Bool
            | TyKind::Int(_)
            | TyKind::Float(_)
            | TyKind::FnPtr { .. } => true,
            TyKind::Ref { .. } | TyKind::Desc(_) | TyKind::Dyn(_) | TyKind::OwnedDyn { .. } => {
                false
            }
            TyKind::Param(_) => false,
            TyKind::Opaque(op) => match op {
                OpaqueTy::ByteBuffer | OpaqueTy::ForeignCallback { .. } | OpaqueTy::SyncCell(_) => {
                    true
                }
                OpaqueTy::OwnedStore { element } | OpaqueTy::Mutex { payload: element } => {
                    walk(tys, *element, seen)
                }
                OpaqueTy::MutexGuard { .. }
                | OpaqueTy::Task { .. }
                | OpaqueTy::Registration { .. }
                | OpaqueTy::AsyncRegistration { .. }
                | OpaqueTy::ForeignHandle
                | OpaqueTy::CWideString => false,
            },
            TyKind::Managed(inner)
            | TyKind::Slice(inner)
            | TyKind::Array { element: inner, .. } => walk(tys, *inner, seen),
            TyKind::Tuple(elems) => elems.iter().all(|e| walk(tys, *e, seen)),
            TyKind::ErrorUnion { ok, err } => walk(tys, *ok, seen) && walk(tys, *err, seen),
            TyKind::Closure { def, args } => tys.closure(*def).is_some_and(|c| {
                c.captures.iter().all(|cap| {
                    tys.subst_lookup(*cap, *args)
                        .is_some_and(|t| walk(tys, t, seen))
                })
            }),
            TyKind::Adt { adt, args } => tys.get_adt(*adt).is_some_and(|def| {
                def.variants.iter().all(|v| {
                    v.fields.iter().all(|f| {
                        tys.subst_lookup(f.ty, *args)
                            .is_some_and(|t| walk(tys, t, seen))
                    })
                })
            }),
        }
    }
    walk(tys, ty, &mut Vec::new())
}
