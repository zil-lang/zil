//! Merged operation set and body structure.
//!
//! The old per-container and Local/Address-paired variants collapse into
//! place access through [`Operand`] and [`Rvalue`]; protocol operations are
//! [`IntrinsicOp`]s called through [`Callee::Intrinsic`] so that their
//! normal, error and cancellation edges use the same terminator.

use super::ids::{
    AdtId, ArgsId, BlockId, ClosureId, ConstId, DynId, FieldIdx, FnId, ForeignId, Local, ScopeId,
    StaticId, TyId, VariantIdx,
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
    /// Borrow a slice (`&place[start..end]` / `&mut place[start..end]`) of an
    /// array or slice place.
    SliceRef {
        region: Region,
        mutability: Mutability,
        place: Place,
        start: Option<Operand>,
        end: Option<Operand>,
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
    /// Allocate an `OwnedDyn<dyn Trait>` from a concrete value (`owned_dyn`).
    OwnedDynCreate,
    /// Run a scoped child thread alongside a parent callback (`thread_scope`).
    ThreadScope,
    /// Initialize an in-place `SyncCell` slot (`atomic_scope`).
    SyncInit,
    /// Run a scoped foreign callback registration (`c_with_callback`).
    ScopedCallback,
    /// Ordered atomic operations with statically verified memory orderings.
    SyncLoadOrdered(MemoryOrder),
    SyncStoreOrdered(MemoryOrder),
    SyncFetchAddOrdered(MemoryOrder),
    SyncExchangeOrdered(MemoryOrder),
    SyncCompareExchangeOrdered {
        success: MemoryOrder,
        failure: MemoryOrder,
    },
    /// Produce a null `c_handle` value (`c_null()`).
    CNull,
    /// Convert a non-capturing C-ABI scalar function pointer into `extern "C" fn(...)` (`c_callback(f)`).
    CCallback,
}

/// Explicit atomic memory ordering (`MemoryOrder`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemoryOrder {
    Relaxed,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}

impl MemoryOrder {
    pub const fn is_valid_load(self) -> bool {
        matches!(self, Self::Relaxed | Self::Acquire | Self::SeqCst)
    }

    pub const fn is_valid_store(self) -> bool {
        matches!(self, Self::Relaxed | Self::Release | Self::SeqCst)
    }

    pub const fn is_valid_cas(success: Self, failure: Self) -> bool {
        match success {
            Self::Relaxed | Self::Release => matches!(failure, Self::Relaxed),
            Self::Acquire | Self::AcqRel => matches!(failure, Self::Relaxed | Self::Acquire),
            Self::SeqCst => matches!(failure, Self::Relaxed | Self::Acquire | Self::SeqCst),
        }
    }

    pub const fn llvm_name(self) -> &'static str {
        match self {
            Self::Relaxed => "monotonic",
            Self::Acquire => "acquire",
            Self::Release => "release",
            Self::AcqRel => "acq_rel",
            Self::SeqCst => "seq_cst",
        }
    }
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
    Cancel,
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

/// A declared drop flag: the `Bool` local `flag` may be proved to equal
/// "`place` is fully initialized" (`spec/semantic-kernel.md` §5). The
/// declaration grants nothing; the kernel establishes and checks the
/// correlation. `place` is inline (no `Deref`/`Index`) local storage.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DropFlag {
    pub flag: Local,
    pub place: Place,
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
    /// Each flag local appears at most once.
    pub drop_flags: Vec<DropFlag>,
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

/// What a function may be used for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FnRole {
    /// Callable and nameable as a value.
    Normal,
    /// The bound `impl Drop` entry of a resource (`_zil_design.md` §7): its
    /// only parameter is the descriptor of the closing resource, and it runs
    /// only from cleanup of a `Managed` carrier. No call or value names it.
    Drop(AdtId),
    /// A concrete `impl Trait for ConcreteType` vtable slot entry bound to
    /// `(dyn_id, concrete_ty, slot)`. Runs only through `Callee::DynSlot`
    /// dispatch; an unrelated `FnRole::Normal` function of the same signature
    /// cannot substitute for it.
    DynSlotImpl {
        dyn_id: DynId,
        concrete_ty: TyId,
        slot: u32,
    },
}

/// Bitmask of finite protocol states (up to 64 variants).
/// Built-in Task/Registration handles use bits 0..3 (`Open` | `Closing` | `Closed`);
/// user-defined state protocols use bits `0..variant_count` matching their state enum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ProtocolStateSet(pub u64);

impl ProtocolStateSet {
    pub const OPEN: u64 = 1 << 0;
    pub const CLOSING: u64 = 1 << 1;
    pub const CLOSED: u64 = 1 << 2;
    pub const ALL: Self = Self(Self::OPEN | Self::CLOSING | Self::CLOSED);

    pub const fn single(variant: u32) -> Self {
        if variant < 64 {
            Self(1u64 << variant)
        } else {
            Self(0)
        }
    }

    pub const fn all_variants(count: usize) -> Self {
        if count == 0 {
            Self(0)
        } else if count >= 64 {
            Self(u64::MAX)
        } else {
            Self((1u64 << count) - 1)
        }
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn is_subset_of(self, other: Self) -> bool {
        (self.0 & !other.0) == 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub const fn subtract(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub const fn is_valid(self) -> bool {
        self.0 != 0 && (self.0 & !Self::ALL.0) == 0
    }

    pub const fn is_valid_for(self, variant_count: usize) -> bool {
        let all = Self::all_variants(variant_count);
        self.0 != 0 && (self.0 & !all.0) == 0
    }
}

/// Verified function-level protocol state contract clause (`contract { requires/provides(...) }`).
/// Fixed-depth field projection path (`p`, `p.a`, `p.a.b`, up to depth 4)
/// used in projection-invalidation upper-bound contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContractFieldPath {
    pub len: u8,
    pub fields: [FieldIdx; 4],
}

impl ContractFieldPath {
    pub const MAX_DEPTH: usize = 4;
    pub const ROOT: Self = Self {
        len: 0,
        fields: [FieldIdx(0); 4],
    };

    pub fn from_slice(slice: &[FieldIdx]) -> Option<Self> {
        if slice.len() > Self::MAX_DEPTH {
            return None;
        }
        let mut fields = [FieldIdx(0); 4];
        for (i, &f) in slice.iter().enumerate() {
            fields[i] = f;
        }
        Some(Self {
            len: slice.len() as u8,
            fields,
        })
    }

    pub fn as_slice(&self) -> &[FieldIdx] {
        &self.fields[..(self.len as usize).min(Self::MAX_DEPTH)]
    }

    /// True if this declared upper-bound prefix covers `actual` (prefix subsumption).
    pub fn subsumes(&self, actual: &[FieldIdx]) -> bool {
        let prefix = self.as_slice();
        actual.len() >= prefix.len() && actual[..prefix.len()] == *prefix
    }

    /// True if `self` and `other` overlap (one is a prefix of the other).
    pub fn overlaps(&self, other: &[FieldIdx]) -> bool {
        let a = self.as_slice();
        let n = a.len().min(other.len());
        a[..n] == other[..n]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StateContractClause {
    /// `requires(state(param, states))`
    RequiresState {
        param: u32,
        states: ProtocolStateSet,
    },
    /// `provides(retired(param))`
    ProvidesRetired { param: u32 },
    /// `provides(on_pending(state(param, states)))`
    ProvidesOnPending {
        param: u32,
        states: ProtocolStateSet,
    },
    /// `provides(on_ready(state(param, states)))`
    ProvidesOnReady {
        param: u32,
        states: ProtocolStateSet,
    },
    /// `provides(state(param, states))` — unconditional state postcondition on normal return
    ProvidesState {
        param: u32,
        states: ProtocolStateSet,
    },
    /// `provides(on_ok(state(param, states)))` — ErrorUnion `.ok` exit postcondition
    ProvidesOnOk {
        param: u32,
        states: ProtocolStateSet,
    },
    /// `provides(on_err(state(param, states)))` — ErrorUnion `.err` exit postcondition
    ProvidesOnErr {
        param: u32,
        states: ProtocolStateSet,
    },
    /// `requires(apart(param_a, param_b))` — multi-role non-aliasing precondition
    RequiresApart { param_a: u32, param_b: u32 },
    /// Marker indicating `effects(invalidates(...))` was explicitly declared as a closed upper bound.
    InvalidationUpperBound,
    /// `effects(invalidates(param.f1.f2))` — declared projection invalidation upper bound.
    Invalidates { param: u32, path: ContractFieldPath },
    /// `effects(rollback(param))` — explicitly permits transient history rollback `H` on `param`.
    AllowsRollback { param: u32 },
}

/// Declared task resume locality contract (`contract { resume(local | transfer) }`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ResumeMode {
    #[default]
    Local,
    Transfer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FnSig {
    pub params: Vec<TyId>,
    pub result: TyId,
    pub type_params: u32,
    pub region_params: u32,
    pub role: FnRole,
    pub resume_mode: ResumeMode,
    pub state_contracts: Vec<StateContractClause>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ForeignAbi {
    C,
}

/// Declared callback provider role from `extern "C" fn ... contract { provides(...) }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ForeignCallbackRole {
    AsyncRegister,
    AsyncRequestClose,
    AsyncPollDrain,
    ThreadAsyncRegister,
    ThreadAsyncRequestClose,
    ThreadAsyncPollDrain,
    ScopedRegister,
    ScopedClose,
}

impl ForeignCallbackRole {
    pub fn is_threaded(self) -> bool {
        matches!(
            self,
            Self::ThreadAsyncRegister | Self::ThreadAsyncRequestClose | Self::ThreadAsyncPollDrain
        )
    }
}

/// A foreign function declaration (`extern "C"`). Parameters and result
/// are C ABI scalars (`_zil_design.md` §12.4); they carry no address and no
/// obligation, so a call has the ordinary call effects only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignDecl {
    /// Link symbol.
    pub symbol: String,
    pub abi: ForeignAbi,
    pub params: Vec<TyId>,
    pub result: TyId,
    /// Parameter indices declared `noescape` (may accept C-ABI addresses).
    pub noescape_params: Vec<u32>,
    /// Optional async callback provider role declared in `provides(...)`.
    pub callback_role: Option<ForeignCallbackRole>,
    /// Whether `provides(thread_async_driver_transfer())` was declared.
    pub thread_async_driver_transfer: bool,
}

/// Per-function sound over-approximation of parameter and closure-capture flows
/// into the return value, independently derived from the module's bodies by
/// both solver and checker.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReturnSummary {
    /// If this function is the body of a closure, its `ClosureId`.
    pub closure: Option<ClosureId>,
    /// Parameter indices (`0..64`) that may flow into the return value.
    pub param_mask: u64,
    /// Closure capture indices (`0..64`) that may flow into the return value.
    pub capture_mask: u64,
    /// When the return value is a closure whose capture fields have known
    /// parameter origins, per-field parameter masks (`field_idx -> param_mask`).
    pub ret_field_param_masks: Vec<u64>,
    /// Parameter indices (`0..64`) whose described `OwnedStore`/`ByteBuffer`
    /// may be structurally mutated (reallocated/pushed/popped/truncated) directly
    /// or transitively by this function.
    pub mutating_params: u64,
    /// Closure capture indices (`0..64`) whose described `OwnedStore`/`ByteBuffer`
    /// may be structurally mutated directly or transitively by this closure.
    pub mutating_captures: u64,
    /// Parameter indices (`0..64`) through which this function (directly or
    /// transitively) may store an address-holding value behind a deref or descriptor.
    pub addr_store_params: u64,
    /// Closure capture indices (`0..64`) through which this closure (directly or
    /// transitively) may store an address-holding value.
    pub addr_store_captures: u64,
}

/// The signatures a body may refer to.
#[derive(Clone, Copy, Debug)]
pub struct Sigs<'a> {
    pub functions: &'a [FnSig],
    pub foreigns: &'a [ForeignDecl],
    pub static_count: usize,
    /// Per-function return flow summaries (independently derived from the
    /// module's bodies by both solver and checker).
    pub return_summaries: Option<&'a [ReturnSummary]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    /// Canonical qualified name; diagnostics and symbols only.
    pub name: String,
    pub sig: FnSig,
    pub body: Body,
}

/// A concrete vtable implementation mapping `(dyn_id, concrete_ty)` to
/// the ordered list of method `FnId`s matching `DynDef.slots`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynImpl {
    pub dyn_id: DynId,
    pub concrete_ty: TyId,
    pub methods: Vec<FnId>,
}

#[derive(Clone, Debug, Default)]
pub struct Module {
    pub tys: TyTable,
    pub functions: Vec<Function>,
    /// Indexed by [`ForeignId`].
    pub foreigns: Vec<ForeignDecl>,
    /// Static UTF-8 byte literals, indexed by [`StaticId`].
    pub statics: Vec<Vec<u8>>,
    /// Concrete `dyn Trait` vtable implementations verified by `zil-check`.
    pub dyn_impls: Vec<DynImpl>,
}

impl Module {
    /// Function signatures in [`FnId`] order.
    pub fn fn_sigs(&self) -> Vec<FnSig> {
        self.functions.iter().map(|f| f.sig.clone()).collect()
    }
    /// The bound `impl Drop` entry of `adt`, if any.
    pub fn drop_fn(&self, adt: AdtId) -> Option<FnId> {
        self.functions
            .iter()
            .position(|f| f.sig.role == FnRole::Drop(adt))
            .map(FnId::from_index)
    }
}
