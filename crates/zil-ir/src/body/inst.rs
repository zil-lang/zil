//! Merged operation set and body structure.
//!
//! The old per-container and Local/Address-paired variants collapse into
//! place access through [`Operand`] and [`Rvalue`]; protocol operations are
//! [`IntrinsicOp`]s called through [`Callee::Intrinsic`] so that their
//! normal, error and cancellation edges use the same terminator.

use super::ids::{
    AdtId, ArgsId, BlockId, ClosureId, ConstId, FnId, ForeignId, Local, ScopeId, StaticId, TyId,
    VariantIdx,
};
use super::place::{Place, PlaceTypes};
use super::ty::{FloatTy, IntTy, Mutability, ParamEnv, Region, TyTable};
use zil_base::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConstValue {
    Unit,
    Bool(bool),
    /// Mathematical value in two's complement, truncated to the type width.
    Int {
        ty: IntTy,
        bits: u128,
    },
    /// Raw IEEE bits.
    Float {
        ty: FloatTy,
        bits: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Operand {
    /// Read a Copy value; the place stays initialized.
    Copy(Place),
    /// Take the value; the place becomes uninitialized.
    Move(Place),
    Const(ConstValue),
    /// A pooled constant of a non-scalar type.
    Pooled(ConstId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnOp {
    Not,
    /// Checked negation for integers; IEEE negation for floats.
    Neg,
    WrappingNeg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    WrappingAdd,
    WrappingSub,
    WrappingMul,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl BinOp {
    /// Whether the operation can fail its checked contract and trap.
    pub const fn may_trap(self) -> bool {
        match self {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => true,
            BinOp::Shl | BinOp::Shr => true,
            BinOp::WrappingAdd
            | BinOp::WrappingSub
            | BinOp::WrappingMul
            | BinOp::BitAnd
            | BinOp::BitOr
            | BinOp::BitXor
            | BinOp::Eq
            | BinOp::Ne
            | BinOp::Lt
            | BinOp::Le
            | BinOp::Gt
            | BinOp::Ge => false,
        }
    }
    pub const fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AggregateKind {
    Tuple,
    /// A Record/Package value or one Enum variant.
    Adt {
        adt: AdtId,
        variant: VariantIdx,
        args: ArgsId,
    },
    Array {
        element: TyId,
    },
    /// A closure environment; operands are captures in order.
    Closure {
        def: ClosureId,
        args: ArgsId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Rvalue {
    Use(Operand),
    /// Open an address window on `place`.
    Ref {
        region: Region,
        mutability: Mutability,
        place: Place,
    },
    /// Describe the resource reached by `place` (a `Managed` carrier, a
    /// descriptor, or its payload). Copies no responsibility.
    Describe {
        place: Place,
    },
    Aggregate {
        kind: AggregateKind,
        operands: Vec<Operand>,
    },
    /// Build an error union in its success (`ok`) or error arm.
    ErrorUnion {
        ok: bool,
        value: Operand,
        ty: TyId,
    },
    Unary {
        op: UnOp,
        operand: Operand,
    },
    Binary {
        op: BinOp,
        lhs: Operand,
        rhs: Operand,
    },
    /// Checked numeric conversion preserving the mathematical value.
    Convert {
        operand: Operand,
        to: TyId,
    },
    /// Read the active variant tag of an Enum place.
    Discriminant(Place),
    /// Read the active arm of an error-union place (`true` = success).
    ErrorArm(Place),
    /// Length of an array or slice place.
    Len(Place),
    FnRef(FnId),
    ForeignFnRef(ForeignId),
    StaticUtf(StaticId),
}

/// Reason for leaving a lexical cleanup scope. Grants no authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScopeExit {
    Fallthrough,
    Return,
    Error,
    Break,
    Continue,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InstructionKind {
    /// Initialize `dest` with `value`. `dest` must be uninitialized unless its
    /// type is Copy; replacing a live owning value uses `Replace`.
    Assign {
        dest: Place,
        value: Rvalue,
    },
    /// End the lifetime of the initialized value at `place`, running its
    /// cleanup obligation.
    Drop {
        place: Place,
    },
    /// Replace the live value at `place`: the old instance moves to `old`
    /// (an uninitialized local) and `value` initializes `place`.
    Replace {
        place: Place,
        value: Operand,
        old: Local,
    },
    StorageLive(Local),
    StorageDead(Local),
    ScopeEnter(ScopeId),
    ScopeExit {
        scope: ScopeId,
        exit: ScopeExit,
    },
    /// Register a deferred closure on `scope`; the operand is moved.
    RegisterDefer {
        scope: ScopeId,
        closure: Operand,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClosureCallMode {
    Shared,
    Mut,
    Once,
}

/// Runtime protocol operations. Each has a fixed signature, an OpSpec entry
/// and a protocol transition in the kernel (or an explicit rejection).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntrinsicOp {
    StoreNew,
    StoreReserve,
    StorePush,
    StorePop,
    StoreLen,
    StoreCapacity,
    StoreBorrow {
        mutability: Mutability,
    },
    ByteBufferNew,
    ByteBufferReserve,
    ByteBufferPush,
    ByteBufferTruncate,
    ByteBufferLen,
    ByteBufferCapacity,
    ByteBufferBorrow,
    MutexCreate,
    MutexLock,
    MutexValue,
    MutexUnlock,
    SyncLoad,
    SyncStore,
    SyncFetchAdd,
    SyncCompareExchange,
    TaskStart,
    TaskCreate,
    TaskYield,
    TaskValid,
    TaskPoll,
    TaskRequestCancel,
    TaskPollClose,
    TaskFinishDestroy,
    RegistrationStart,
    RegistrationValid,
    RegistrationClose,
    AsyncRegistrationStart,
    AsyncRegistrationValid,
    AsyncRegistrationRequestClose,
    AsyncRegistrationPollClose,
    AsyncRegistrationFinishDestroy,
    /// Allocate a resource head from a payload value (`create`).
    ManagedCreate,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Callee {
    Direct(FnId),
    /// A function pointer value.
    Indirect(Operand),
    /// Call the closure stored at `closure` in the given mode. `Once`
    /// consumes the environment; the others borrow it.
    Closure {
        closure: Place,
        mode: ClosureCallMode,
    },
    /// A dynamic interface slot of `object` (an address or owned dyn).
    DynSlot {
        object: Operand,
        slot: u32,
    },
    Foreign(ForeignId),
    Intrinsic(IntrinsicOp),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrapKind {
    Bounds,
    Overflow,
    DivideByZero,
    Conversion,
    Explicit,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Terminator {
    Goto {
        target: BlockId,
    },
    /// Branch on an integer/bool/tag operand.
    Switch {
        discr: Operand,
        arms: Vec<(u128, BlockId)>,
        otherwise: BlockId,
    },
    /// Leave the function with the value of `value`.
    Return {
        value: Operand,
    },
    /// `dest` is initialized on `target` only. A suspending call has a
    /// `cancel` edge on which `dest` stays uninitialized.
    Call {
        callee: Callee,
        args: Vec<Operand>,
        dest: Place,
        target: BlockId,
        cancel: Option<BlockId>,
    },
    /// Continue to `target` iff `cond == expected`, else trap.
    Assert {
        cond: Operand,
        expected: bool,
        target: BlockId,
        trap: TrapKind,
    },
    /// Must be semantically unreachable; the checker proves it.
    Unreachable,
    /// Abort the process. No cleanup obligation survives an abort.
    Trap(TrapKind),
}

/// The kind of a control edge. Every edge has explicit effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EdgeKind {
    Goto,
    SwitchArm(u128),
    SwitchOtherwise,
    CallReturn,
    CallCancel,
    AssertOk,
}

impl Terminator {
    /// All outgoing edges. Exhaustive: a new terminator must list its edges.
    pub fn edges(&self) -> Vec<(EdgeKind, BlockId)> {
        match self {
            Terminator::Goto { target } => vec![(EdgeKind::Goto, *target)],
            Terminator::Switch {
                arms, otherwise, ..
            } => arms
                .iter()
                .map(|(v, b)| (EdgeKind::SwitchArm(*v), *b))
                .chain(std::iter::once((EdgeKind::SwitchOtherwise, *otherwise)))
                .collect(),
            Terminator::Call { target, cancel, .. } => {
                std::iter::once((EdgeKind::CallReturn, *target))
                    .chain(cancel.map(|c| (EdgeKind::CallCancel, c)))
                    .collect()
            }
            Terminator::Assert { target, .. } => vec![(EdgeKind::AssertOk, *target)],
            Terminator::Return { .. } | Terminator::Unreachable | Terminator::Trap(_) => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub kind: InstructionKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
    pub terminator_span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDecl {
    pub ty: TyId,
    /// Source binding name, diagnostics only.
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Body {
    /// Parameters are locals `0..param_count`, initialized on entry.
    pub param_count: u32,
    pub locals: Vec<LocalDecl>,
    /// Environment field types when this body is a closure.
    pub captures: Vec<TyId>,
    pub closure: Option<ClosureId>,
    pub result: TyId,
    pub param_env: ParamEnv,
    /// `blocks[0]` is the entry.
    pub blocks: Vec<Block>,
}

impl Body {
    pub fn entry(&self) -> BlockId {
        BlockId(0)
    }
    pub fn block(&self, id: BlockId) -> Option<&Block> {
        self.blocks.get(id.index())
    }
    pub fn params(&self) -> impl Iterator<Item = Local> {
        (0..self.param_count).map(Local)
    }
    pub fn is_param(&self, local: Local) -> bool {
        local.0 < self.param_count
    }
    /// Predecessor lists, one per block.
    pub fn predecessors(&self) -> Vec<Vec<(BlockId, EdgeKind)>> {
        let mut preds = vec![Vec::new(); self.blocks.len()];
        for (i, block) in self.blocks.iter().enumerate() {
            for (kind, target) in block.terminator.edges() {
                if let Some(p) = preds.get_mut(target.index()) {
                    p.push((BlockId::from_index(i), kind));
                }
            }
        }
        preds
    }
    pub fn cx<'a>(&'a self, tys: &'a TyTable) -> BodyCx<'a> {
        BodyCx { body: self, tys }
    }
}

/// A body together with the type table it was produced against.
#[derive(Clone, Copy)]
pub struct BodyCx<'a> {
    pub body: &'a Body,
    pub tys: &'a TyTable,
}

impl PlaceTypes for BodyCx<'_> {
    fn local_ty(&self, local: Local) -> Option<TyId> {
        self.body.locals.get(local.index()).map(|l| l.ty)
    }
    fn capture_ty(&self, index: u32) -> Option<TyId> {
        self.body.captures.get(index as usize).copied()
    }
    fn tys(&self) -> &TyTable {
        self.tys
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FnSig {
    pub params: Vec<TyId>,
    pub result: TyId,
    pub type_params: u32,
    pub region_params: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    /// Canonical qualified name; diagnostics and symbols only.
    pub name: String,
    pub sig: FnSig,
    pub body: Body,
}

#[derive(Clone, Debug, Default)]
pub struct Module {
    pub tys: TyTable,
    pub functions: Vec<Function>,
}
