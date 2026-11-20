//! The interpreter proper: storage, places and the explicit frame stack.
//!
//! Frames live on an explicit stack (no host recursion), so the call-depth
//! budget, not the host stack, bounds CT recursion. Each local is one
//! allocation; references are `(allocation, generation, path)` triples and
//! a dangling or stale reference is an internal error, never silently
//! re-targeted.

use crate::arith::{self, ArithError, ConvTarget};
use crate::{EvalBudget, EvalError, Resource, Value};
use zil_ir::body::{
    AdtKind, AggregateKind, BlockId, Body, Callee, ConstValue, DerefBase, FnId, InstructionKind,
    IntTy, IntrinsicOp, Module, Operand, Place, PlaceRoot, Projection, Rvalue, Terminator,
    TrapKind, TyId, TyKind, type_place,
};

/// One storage step below an allocation root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Field of a record/package/tuple/closure environment.
    Field(u32),
    /// Field of the given enum variant; the variant must be active.
    VariantField { variant: u32, field: u32 },
    /// Arm of an error union; it must be the active arm.
    ErrorArm(bool),
    /// Array element.
    Elem(u32),
}

/// An interpreter place reached through a reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ptr {
    alloc: usize,
    generation: u64,
    path: Vec<Step>,
}

/// Interpreter storage. `Uninit` marks a hole left by a move, a drop or a
/// not-yet-initialized local; reading it is an internal error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Cell {
    Uninit,
    Scalar(ConstValue),
    Agg {
        ty: TyId,
        variant: Option<u32>,
        fields: Vec<Cell>,
    },
    ErrorUnion {
        ty: TyId,
        ok: bool,
        payload: Box<Cell>,
    },
    FnRef(FnId),
    Ref(Ptr),
}

impl Cell {
    pub(crate) fn from_value(value: Value) -> Cell {
        match value {
            Value::Unit => Cell::Scalar(ConstValue::Unit),
            Value::Bool(b) => Cell::Scalar(ConstValue::Bool(b)),
            Value::Int { ty, bits } => Cell::Scalar(ConstValue::Int { ty, bits }),
            Value::Float { ty, bits } => Cell::Scalar(ConstValue::Float { ty, bits }),
            Value::Aggregate {
                ty,
                variant,
                fields,
            } => Cell::Agg {
                ty,
                variant,
                fields: fields.into_iter().map(Cell::from_value).collect(),
            },
            Value::ErrorUnion { ty, ok, payload } => Cell::ErrorUnion {
                ty,
                ok,
                payload: Box::new(Cell::from_value(*payload)),
            },
            Value::FnRef(f) => Cell::FnRef(f),
        }
    }

    /// The public value of a result. A reference anywhere inside is an
    /// escaping VM address.
    pub(crate) fn into_value(self, function: FnId) -> Result<Value, EvalError> {
        Ok(match self {
            Cell::Uninit => {
                return Err(EvalError::Internal {
                    what: "result contains uninitialized storage",
                    function,
                });
            }
            Cell::Ref(_) => return Err(EvalError::EscapingRef),
            Cell::Scalar(c) => Value::from_const(c),
            Cell::Agg {
                ty,
                variant,
                fields,
            } => Value::Aggregate {
                ty,
                variant,
                fields: fields
                    .into_iter()
                    .map(|f| f.into_value(function))
                    .collect::<Result<_, _>>()?,
            },
            Cell::ErrorUnion { ty, ok, payload } => Value::ErrorUnion {
                ty,
                ok,
                payload: Box::new(payload.into_value(function)?),
            },
            Cell::FnRef(f) => Value::FnRef(f),
        })
    }

    fn fully_initialized(&self) -> bool {
        match self {
            Cell::Uninit => false,
            Cell::Scalar(_) | Cell::FnRef(_) | Cell::Ref(_) => true,
            Cell::Agg { fields, .. } => fields.iter().all(Cell::fully_initialized),
            Cell::ErrorUnion { payload, .. } => payload.fully_initialized(),
        }
    }

    fn child(&self, step: Step) -> Option<&Cell> {
        match (step, self) {
            (Step::Field(i) | Step::Elem(i), Cell::Agg { fields, .. }) => fields.get(i as usize),
            (
                Step::VariantField { variant, field },
                Cell::Agg {
                    variant: Some(active),
                    fields,
                    ..
                },
            ) if variant == *active => fields.get(field as usize),
            (Step::ErrorArm(arm), Cell::ErrorUnion { ok, payload, .. }) if arm == *ok => {
                Some(payload)
            }
            (
                Step::Field(_) | Step::Elem(_) | Step::VariantField { .. } | Step::ErrorArm(_),
                Cell::Uninit
                | Cell::Scalar(_)
                | Cell::Agg { .. }
                | Cell::ErrorUnion { .. }
                | Cell::FnRef(_)
                | Cell::Ref(_),
            ) => None,
        }
    }

    fn child_mut(&mut self, step: Step) -> Option<&mut Cell> {
        match self {
            Cell::Agg {
                variant: active,
                fields,
                ..
            } => match step {
                Step::Field(i) | Step::Elem(i) => fields.get_mut(i as usize),
                Step::VariantField { variant, field } if *active == Some(variant) => {
                    fields.get_mut(field as usize)
                }
                Step::VariantField { .. } | Step::ErrorArm(_) => None,
            },
            Cell::ErrorUnion { ok, payload, .. } => match step {
                Step::ErrorArm(arm) if arm == *ok => Some(payload),
                Step::ErrorArm(_) | Step::Field(_) | Step::Elem(_) | Step::VariantField { .. } => {
                    None
                }
            },
            Cell::Uninit | Cell::Scalar(_) | Cell::FnRef(_) | Cell::Ref(_) => None,
        }
    }
}

/// A resolved place and its static type. The kernel rejects writes into
/// an uninitialized parent (`WriteIntoUninitParent`), so a write never
/// needs to invent the shape of a missing aggregate; reaching one is an
/// internal error.
struct Resolved {
    ptr: Ptr,
    ty: TyId,
}

struct Alloc {
    generation: u64,
    live: bool,
    cell: Cell,
}

struct Frame<'m> {
    function: FnId,
    body: &'m Body,
    block: BlockId,
    ip: usize,
    locals: Vec<Ptr>,
    /// The closure environment of a closure body.
    env: Option<Ptr>,
    /// Allocations owned by the frame beyond its locals (a consumed `Once`
    /// environment).
    owned: Vec<usize>,
}

pub(crate) struct Machine<'m> {
    module: &'m Module,
    allocs: Vec<Alloc>,
    free: Vec<usize>,
    steps: u64,
    depth_limit: u32,
    stack: Vec<Frame<'m>>,
}

fn intrinsic_name(op: IntrinsicOp) -> &'static str {
    match op {
        IntrinsicOp::StoreNew => "intrinsic StoreNew",
        IntrinsicOp::StoreReserve => "intrinsic StoreReserve",
        IntrinsicOp::StorePush => "intrinsic StorePush",
        IntrinsicOp::StorePop => "intrinsic StorePop",
        IntrinsicOp::StoreLen => "intrinsic StoreLen",
        IntrinsicOp::StoreCapacity => "intrinsic StoreCapacity",
        IntrinsicOp::StoreBorrow { .. } => "intrinsic StoreBorrow",
        IntrinsicOp::ByteBufferNew => "intrinsic ByteBufferNew",
        IntrinsicOp::ByteBufferReserve => "intrinsic ByteBufferReserve",
        IntrinsicOp::ByteBufferPush => "intrinsic ByteBufferPush",
        IntrinsicOp::ByteBufferTruncate => "intrinsic ByteBufferTruncate",
        IntrinsicOp::ByteBufferLen => "intrinsic ByteBufferLen",
        IntrinsicOp::ByteBufferCapacity => "intrinsic ByteBufferCapacity",
        IntrinsicOp::ByteBufferBorrow => "intrinsic ByteBufferBorrow",
        IntrinsicOp::MutexCreate => "intrinsic MutexCreate",
        IntrinsicOp::MutexLock => "intrinsic MutexLock",
        IntrinsicOp::MutexValue => "intrinsic MutexValue",
        IntrinsicOp::MutexUnlock => "intrinsic MutexUnlock",
        IntrinsicOp::SyncLoad => "intrinsic SyncLoad",
        IntrinsicOp::SyncStore => "intrinsic SyncStore",
        IntrinsicOp::SyncFetchAdd => "intrinsic SyncFetchAdd",
        IntrinsicOp::SyncCompareExchange => "intrinsic SyncCompareExchange",
        IntrinsicOp::TaskStart => "intrinsic TaskStart",
        IntrinsicOp::TaskCreate => "intrinsic TaskCreate",
        IntrinsicOp::TaskYield => "intrinsic TaskYield",
        IntrinsicOp::TaskValid => "intrinsic TaskValid",
        IntrinsicOp::TaskPoll => "intrinsic TaskPoll",
        IntrinsicOp::TaskRequestCancel => "intrinsic TaskRequestCancel",
        IntrinsicOp::TaskPollClose => "intrinsic TaskPollClose",
        IntrinsicOp::TaskFinishDestroy => "intrinsic TaskFinishDestroy",
        IntrinsicOp::RegistrationStart => "intrinsic RegistrationStart",
        IntrinsicOp::RegistrationValid => "intrinsic RegistrationValid",
        IntrinsicOp::RegistrationClose => "intrinsic RegistrationClose",
        IntrinsicOp::AsyncRegistrationStart => "intrinsic AsyncRegistrationStart",
        IntrinsicOp::AsyncRegistrationValid => "intrinsic AsyncRegistrationValid",
        IntrinsicOp::AsyncRegistrationRequestClose => "intrinsic AsyncRegistrationRequestClose",
        IntrinsicOp::AsyncRegistrationPollClose => "intrinsic AsyncRegistrationPollClose",
        IntrinsicOp::AsyncRegistrationFinishDestroy => "intrinsic AsyncRegistrationFinishDestroy",
        IntrinsicOp::ManagedCreate => "intrinsic ManagedCreate",
        IntrinsicOp::OwnedDynCreate => "intrinsic OwnedDynCreate",
        IntrinsicOp::ThreadScope => "intrinsic ThreadScope",
    }
}

impl<'m> Machine<'m> {
    pub(crate) fn new(module: &'m Module, budget: EvalBudget) -> Self {
        Machine {
            module,
            allocs: Vec::new(),
            free: Vec::new(),
            steps: budget.steps,
            depth_limit: budget.call_depth,
            stack: Vec::new(),
        }
    }

    // ---- errors -------------------------------------------------------------

    fn current(&self) -> FnId {
        self.stack.last().map_or(FnId(0), |f| f.function)
    }
    fn internal(&self, what: &'static str) -> EvalError {
        EvalError::Internal {
            what,
            function: self.current(),
        }
    }
    fn unsupported(&self, what: &'static str) -> EvalError {
        EvalError::Unsupported {
            what,
            function: self.current(),
        }
    }
    fn trap(&self, kind: TrapKind) -> EvalError {
        let (function, block) = self
            .stack
            .last()
            .map_or((FnId(0), BlockId(0)), |f| (f.function, f.block));
        EvalError::Trap {
            kind,
            function,
            block,
        }
    }
    fn arith(&self, e: ArithError) -> EvalError {
        match e {
            ArithError::Trap(kind) => self.trap(kind),
            ArithError::IllTyped(what) => self.internal(what),
        }
    }

    fn step(&mut self) -> Result<(), EvalError> {
        if self.steps == 0 {
            return Err(EvalError::ResourceLimit {
                resource: Resource::Steps,
            });
        }
        self.steps -= 1;
        Ok(())
    }

    fn top(&self) -> Result<&Frame<'m>, EvalError> {
        self.stack
            .last()
            .ok_or_else(|| self.internal("no active frame"))
    }

    // ---- storage ------------------------------------------------------------

    fn alloc(&mut self, cell: Cell) -> Ptr {
        let alloc = match self.free.pop() {
            Some(i) => {
                let a = &mut self.allocs[i];
                a.live = true;
                a.cell = cell;
                i
            }
            None => {
                self.allocs.push(Alloc {
                    generation: 0,
                    live: true,
                    cell,
                });
                self.allocs.len() - 1
            }
        };
        Ptr {
            alloc,
            generation: self.allocs[alloc].generation,
            path: Vec::new(),
        }
    }

    fn release(&mut self, alloc: usize) {
        if let Some(a) = self.allocs.get_mut(alloc) {
            a.live = false;
            a.generation += 1;
            a.cell = Cell::Uninit;
            self.free.push(alloc);
        }
    }

    fn get(&self, ptr: &Ptr) -> Result<&Cell, EvalError> {
        let a = self
            .allocs
            .get(ptr.alloc)
            .filter(|a| a.live && a.generation == ptr.generation)
            .ok_or_else(|| self.internal("access through a dangling reference"))?;
        let mut cell = &a.cell;
        for step in &ptr.path {
            cell = cell
                .child(*step)
                .ok_or_else(|| self.internal("access to uninitialized or inactive storage"))?;
        }
        Ok(cell)
    }

    /// The storage reached by `r`.
    fn slot(&mut self, r: &Resolved) -> Result<&mut Cell, EvalError> {
        let function = self.current();
        let internal = |what| EvalError::Internal { what, function };
        let a = self
            .allocs
            .get_mut(r.ptr.alloc)
            .filter(|a| a.live && a.generation == r.ptr.generation)
            .ok_or(internal("access through a dangling reference"))?;
        let mut cell = &mut a.cell;
        for step in &r.ptr.path {
            cell = cell
                .child_mut(*step)
                .ok_or(internal("access to uninitialized or inactive storage"))?;
        }
        Ok(cell)
    }

    fn write(&mut self, r: &Resolved, cell: Cell) -> Result<(), EvalError> {
        *self.slot(r)? = cell;
        Ok(())
    }

    fn take(&mut self, r: &Resolved) -> Result<Cell, EvalError> {
        let cell = std::mem::replace(self.slot(r)?, Cell::Uninit);
        if cell == Cell::Uninit {
            return Err(self.internal("move out of uninitialized storage"));
        }
        Ok(cell)
    }

    fn read(&self, r: &Resolved) -> Result<Cell, EvalError> {
        let cell = self.get(&r.ptr)?;
        if !cell.fully_initialized() {
            return Err(self.internal("read of uninitialized storage"));
        }
        Ok(cell.clone())
    }

    // ---- places -------------------------------------------------------------

    fn place_ty(&self, place: &Place) -> Result<zil_ir::body::PlaceTyping, EvalError> {
        let frame = self.top()?;
        type_place(place, &frame.body.cx(&self.module.tys))
            .map_err(|_| self.internal("ill-typed place"))
    }

    fn resolve(&self, place: &Place) -> Result<Resolved, EvalError> {
        let typing = self.place_ty(place)?;
        let frame = self.top()?;
        let module = self.module;
        let tys = &module.tys;
        let mut ptr = match place.root {
            PlaceRoot::Local(l) => frame
                .locals
                .get(l.index())
                .cloned()
                .ok_or_else(|| self.internal("unknown local"))?,
            PlaceRoot::Capture(i) => {
                let mut env = frame
                    .env
                    .clone()
                    .ok_or_else(|| self.internal("capture outside a closure body"))?;
                env.path.push(Step::Field(i));
                env
            }
        };
        let mut derefs = typing.derefs.iter();
        for (k, projection) in place.projections.iter().enumerate() {
            let base = typing.ty_at(k);
            let step = match projection {
                Projection::Field(f) => {
                    let def = tys
                        .get_adt(f.adt)
                        .ok_or_else(|| self.internal("unknown declaration"))?;
                    match def.kind {
                        AdtKind::Enum => Step::VariantField {
                            variant: f.variant.0,
                            field: f.field.0,
                        },
                        AdtKind::Record | AdtKind::Package => Step::Field(f.field.0),
                        AdtKind::Resource => {
                            return Err(self.unsupported("resource payload field"));
                        }
                    }
                }
                Projection::TupleField(i) => Step::Field(*i),
                Projection::ErrorArm(ok) => Step::ErrorArm(*ok),
                Projection::ConstIndex(i) => Step::Elem(*i),
                Projection::Index(l) => {
                    let index = match frame.locals.get(l.index()).map(|p| self.get(p)) {
                        Some(Ok(Cell::Scalar(ConstValue::Int {
                            ty: IntTy::Usize,
                            bits,
                        }))) => *bits,
                        Some(Err(e)) => return Err(e),
                        Some(Ok(
                            Cell::Uninit
                            | Cell::Scalar(_)
                            | Cell::Agg { .. }
                            | Cell::ErrorUnion { .. }
                            | Cell::FnRef(_)
                            | Cell::Ref(_),
                        ))
                        | None => return Err(self.internal("index local is not a usize")),
                    };
                    let length = if let Some(TyKind::Array { length, .. }) = tys.get(base) {
                        u128::from(*length)
                    } else {
                        match self.get(&ptr)? {
                            Cell::Agg { fields, .. } => fields.len() as u128,
                            Cell::Uninit
                            | Cell::Scalar(_)
                            | Cell::ErrorUnion { .. }
                            | Cell::FnRef(_)
                            | Cell::Ref(_) => {
                                return Err(self.internal("index into a non-sequence"));
                            }
                        }
                    };
                    if index >= length {
                        return Err(self.trap(TrapKind::Bounds));
                    }
                    Step::Elem(index as u32)
                }
                Projection::Deref => {
                    match derefs.next() {
                        Some(DerefBase::SharedRef | DerefBase::ExclusiveRef) => {}
                        Some(DerefBase::Managed) => {
                            return Err(self.unsupported("dereference of a resource carrier"));
                        }
                        Some(DerefBase::Desc) => {
                            return Err(self.unsupported("dereference of a resource descriptor"));
                        }
                        Some(DerefBase::OwnedDyn) => {
                            return Err(self.unsupported("dereference of an owned dyn"));
                        }
                        None => return Err(self.internal("deref without typing")),
                    }
                    ptr = match self.get(&ptr)? {
                        Cell::Ref(target) => target.clone(),
                        Cell::Uninit
                        | Cell::Scalar(_)
                        | Cell::Agg { .. }
                        | Cell::ErrorUnion { .. }
                        | Cell::FnRef(_) => {
                            return Err(self.internal("dereference of a non-reference"));
                        }
                    };
                    continue;
                }
            };
            ptr.path.push(step);
        }
        Ok(Resolved {
            ptr,
            ty: typing.ty(),
        })
    }

    // ---- operands and rvalues -----------------------------------------------

    fn operand(&mut self, operand: &Operand) -> Result<Cell, EvalError> {
        match operand {
            Operand::Copy(p) => {
                let r = self.resolve(p)?;
                self.read(&r)
            }
            Operand::Move(p) => {
                let r = self.resolve(p)?;
                let cell = self.take(&r)?;
                if !cell.fully_initialized() {
                    return Err(self.internal("move of partially initialized storage"));
                }
                Ok(cell)
            }
            Operand::Const(c) => Ok(Cell::Scalar(*c)),
            Operand::Pooled(_) => {
                Err(self.unsupported("pooled constant (the module carries no constant pool)"))
            }
        }
    }

    fn scalar(&mut self, operand: &Operand) -> Result<ConstValue, EvalError> {
        match self.operand(operand)? {
            Cell::Scalar(c) => Ok(c),
            Cell::Uninit
            | Cell::Agg { .. }
            | Cell::ErrorUnion { .. }
            | Cell::FnRef(_)
            | Cell::Ref(_) => Err(self.internal("scalar operand expected")),
        }
    }

    fn rvalue(&mut self, rvalue: &Rvalue, dest_ty: TyId) -> Result<Cell, EvalError> {
        let module = self.module;
        let tys = &module.tys;
        Ok(match rvalue {
            Rvalue::Use(o) => self.operand(o)?,
            Rvalue::Ref { place, .. } => Cell::Ref(self.resolve(place)?.ptr),
            Rvalue::SliceRef { .. } => return Err(self.unsupported("slice reference")),
            Rvalue::Describe { .. } => return Err(self.unsupported("resource descriptor")),
            Rvalue::Aggregate { kind, operands } => {
                let variant = match kind {
                    AggregateKind::Tuple
                    | AggregateKind::Array { .. }
                    | AggregateKind::Closure { .. } => None,
                    AggregateKind::Adt { adt, variant, .. } => {
                        let def = tys
                            .get_adt(*adt)
                            .ok_or_else(|| self.internal("unknown declaration"))?;
                        match def.kind {
                            AdtKind::Enum => Some(variant.0),
                            AdtKind::Record | AdtKind::Package => None,
                            AdtKind::Resource => {
                                return Err(self.unsupported("resource payload construction"));
                            }
                        }
                    }
                };
                let mut fields = Vec::with_capacity(operands.len());
                for o in operands {
                    fields.push(self.operand(o)?);
                }
                Cell::Agg {
                    ty: dest_ty,
                    variant,
                    fields,
                }
            }
            Rvalue::ErrorUnion { ok, value, ty } => Cell::ErrorUnion {
                ty: *ty,
                ok: *ok,
                payload: Box::new(self.operand(value)?),
            },
            Rvalue::Unary { op, operand } => {
                let v = self.scalar(operand)?;
                Cell::Scalar(arith::unary(*op, v).map_err(|e| self.arith(e))?)
            }
            Rvalue::Binary { op, lhs, rhs } => {
                let l = self.scalar(lhs)?;
                let r = self.scalar(rhs)?;
                Cell::Scalar(arith::binary(*op, l, r).map_err(|e| self.arith(e))?)
            }
            Rvalue::Convert { operand, to } => {
                let target = match tys.get(*to) {
                    Some(TyKind::Int(i)) => ConvTarget::Int(*i),
                    Some(TyKind::Float(f)) => ConvTarget::Float(*f),
                    Some(
                        TyKind::Unit
                        | TyKind::Bool
                        | TyKind::Never
                        | TyKind::Adt { .. }
                        | TyKind::Tuple(_)
                        | TyKind::Array { .. }
                        | TyKind::Slice(_)
                        | TyKind::Ref { .. }
                        | TyKind::Managed(_)
                        | TyKind::Desc(_)
                        | TyKind::ErrorUnion { .. }
                        | TyKind::FnPtr { .. }
                        | TyKind::Closure { .. }
                        | TyKind::Dyn(_)
                        | TyKind::OwnedDyn(_)
                        | TyKind::Param(_)
                        | TyKind::Opaque(_),
                    )
                    | None => return Err(self.internal("conversion to a non-numeric type")),
                };
                let v = self.scalar(operand)?;
                Cell::Scalar(arith::convert(v, target).map_err(|e| self.arith(e))?)
            }
            Rvalue::Discriminant(p) => {
                let r = self.resolve(p)?;
                match self.get(&r.ptr)? {
                    Cell::Agg {
                        variant: Some(v), ..
                    } => Cell::Scalar(ConstValue::Int {
                        ty: IntTy::I32,
                        bits: u128::from(*v),
                    }),
                    Cell::Uninit
                    | Cell::Scalar(_)
                    | Cell::Agg { variant: None, .. }
                    | Cell::ErrorUnion { .. }
                    | Cell::FnRef(_)
                    | Cell::Ref(_) => return Err(self.internal("discriminant of a non-enum")),
                }
            }
            Rvalue::ErrorArm(p) => {
                let r = self.resolve(p)?;
                match self.get(&r.ptr)? {
                    Cell::ErrorUnion { ok, .. } => Cell::Scalar(ConstValue::Bool(*ok)),
                    Cell::Uninit
                    | Cell::Scalar(_)
                    | Cell::Agg { .. }
                    | Cell::FnRef(_)
                    | Cell::Ref(_) => {
                        return Err(self.internal("error arm of a non-error-union"));
                    }
                }
            }
            Rvalue::Len(p) => {
                let r = self.resolve(p)?;
                let length = if let Some(TyKind::Array { length, .. }) = tys.get(r.ty) {
                    u128::from(*length)
                } else {
                    match self.get(&r.ptr)? {
                        Cell::Agg { fields, .. } => fields.len() as u128,
                        Cell::Uninit
                        | Cell::Scalar(_)
                        | Cell::ErrorUnion { .. }
                        | Cell::FnRef(_)
                        | Cell::Ref(_) => return Err(self.internal("length of a non-sequence")),
                    }
                };
                Cell::Scalar(ConstValue::Int {
                    ty: IntTy::Usize,
                    bits: length,
                })
            }
            Rvalue::FnRef(f) => Cell::FnRef(*f),
            Rvalue::ForeignFnRef(_) => return Err(self.unsupported("foreign function value")),
            Rvalue::StaticUtf(_) => {
                return Err(
                    self.unsupported("static UTF-8 data (the module carries no static table)")
                );
            }
        })
    }

    fn holds_user_drop(&self, cell: &Cell) -> bool {
        match cell {
            Cell::Uninit | Cell::Scalar(_) | Cell::FnRef(_) | Cell::Ref(_) => false,
            Cell::Agg { ty, fields, .. } => {
                let own = if let Some(TyKind::Adt { adt, .. }) = self.module.tys.get(*ty) {
                    self.module.tys.get_adt(*adt).is_some_and(|d| d.user_drop)
                } else {
                    false
                };
                own || fields.iter().any(|f| self.holds_user_drop(f))
            }
            Cell::ErrorUnion { payload, .. } => self.holds_user_drop(payload),
        }
    }

    // ---- instructions -------------------------------------------------------

    fn set_local(&mut self, local: zil_ir::body::Local, cell: Cell) -> Result<(), EvalError> {
        let ptr = self
            .top()?
            .locals
            .get(local.index())
            .cloned()
            .ok_or_else(|| self.internal("unknown local"))?;
        let a = &mut self.allocs[ptr.alloc];
        a.cell = cell;
        Ok(())
    }

    fn instruction(&mut self, kind: &InstructionKind) -> Result<(), EvalError> {
        match kind {
            InstructionKind::Assign { dest, value } => {
                let r = self.resolve(dest)?;
                let cell = self.rvalue(value, r.ty)?;
                self.write(&r, cell)
            }
            InstructionKind::Drop { place } => {
                let r = self.resolve(place)?;
                let cell = self.take(&r)?;
                if self.holds_user_drop(&cell) {
                    return Err(self.unsupported("user SyncDrop during compile-time evaluation"));
                }
                Ok(())
            }
            InstructionKind::Replace { place, value, old } => {
                let v = self.operand(value)?;
                let r = self.resolve(place)?;
                let previous = self.take(&r)?;
                self.set_local(*old, previous)?;
                self.write(&r, v)
            }
            InstructionKind::StorageLive(l) | InstructionKind::StorageDead(l) => {
                self.set_local(*l, Cell::Uninit)
            }
            // No deferred closure can be registered (see `RegisterDefer`),
            // so leaving a scope runs nothing.
            InstructionKind::ScopeEnter(_) | InstructionKind::ScopeExit { .. } => Ok(()),
            InstructionKind::RegisterDefer { .. } => {
                Err(self.unsupported("deferred closure registration"))
            }
        }
    }

    // ---- frames -------------------------------------------------------------

    fn push_frame(
        &mut self,
        function: FnId,
        args: Vec<Cell>,
        env: Option<Ptr>,
        owned: Vec<usize>,
    ) -> Result<(), EvalError> {
        if self.stack.len() >= self.depth_limit as usize {
            return Err(EvalError::ResourceLimit {
                resource: Resource::CallDepth,
            });
        }
        let module = self.module;
        let f = module
            .functions
            .get(function.index())
            .ok_or_else(|| self.internal("call of an unknown function"))?;
        if f.sig.type_params != 0 {
            return Err(self.unsupported("generic function body"));
        }
        let body = &f.body;
        if args.len() != body.param_count as usize {
            return Err(self.internal("argument count differs from the callee"));
        }
        if body.closure.is_some() != env.is_some() {
            return Err(self.internal("closure body called without its environment"));
        }
        let mut args = args.into_iter();
        let mut locals = Vec::with_capacity(body.locals.len());
        for _ in 0..body.locals.len() {
            let cell = args.next().unwrap_or(Cell::Uninit);
            locals.push(self.alloc(cell));
        }
        self.stack.push(Frame {
            function,
            body,
            block: body.entry(),
            ip: 0,
            locals,
            env,
            owned,
        });
        Ok(())
    }

    fn pop_frame(&mut self) {
        if let Some(frame) = self.stack.pop() {
            for p in frame.locals {
                self.release(p.alloc);
            }
            for a in frame.owned {
                self.release(a);
            }
        }
    }

    fn goto(&mut self, target: BlockId) -> Result<(), EvalError> {
        if self.top()?.body.block(target).is_none() {
            return Err(self.internal("jump to an unknown block"));
        }
        if let Some(frame) = self.stack.last_mut() {
            frame.block = target;
            frame.ip = 0;
        }
        Ok(())
    }

    fn closure_body(&self, ty: TyId) -> Result<FnId, EvalError> {
        let Some(TyKind::Closure { def, .. }) = self.module.tys.get(ty) else {
            return Err(self.internal("closure call on a non-closure"));
        };
        let def = *def;
        self.module
            .functions
            .iter()
            .position(|f| f.body.closure == Some(def))
            .map(FnId::from_index)
            .ok_or_else(|| self.unsupported("closure definition without a body"))
    }

    fn call(&mut self, callee: &Callee, args: &[Operand]) -> Result<(), EvalError> {
        enum Target {
            Plain(FnId),
            Closure(FnId, Ptr, Vec<usize>),
        }
        let target = match callee {
            Callee::Direct(f) => Target::Plain(*f),
            Callee::Indirect(o) => match self.operand(o)? {
                Cell::FnRef(f) => Target::Plain(f),
                Cell::Uninit
                | Cell::Scalar(_)
                | Cell::Agg { .. }
                | Cell::ErrorUnion { .. }
                | Cell::Ref(_) => return Err(self.internal("indirect call of a non-function")),
            },
            Callee::Closure { closure, mode } => {
                let r = self.resolve(closure)?;
                let function = self.closure_body(r.ty)?;
                match mode {
                    zil_ir::body::ClosureCallMode::Shared | zil_ir::body::ClosureCallMode::Mut => {
                        Target::Closure(function, r.ptr, Vec::new())
                    }
                    zil_ir::body::ClosureCallMode::Once => {
                        let env = self.take(&r)?;
                        let ptr = self.alloc(env);
                        let owned = vec![ptr.alloc];
                        Target::Closure(function, ptr, owned)
                    }
                }
            }
            Callee::DynSlot { .. } => return Err(self.unsupported("dynamic interface call")),
            Callee::Foreign(_) => return Err(self.unsupported("foreign call")),
            Callee::Intrinsic(op) => return Err(self.unsupported(intrinsic_name(*op))),
        };
        let mut values = Vec::with_capacity(args.len());
        for a in args {
            values.push(self.operand(a)?);
        }
        match target {
            Target::Plain(f) => self.push_frame(f, values, None, Vec::new()),
            Target::Closure(f, env, owned) => {
                let result = self.push_frame(f, values, Some(env), owned.clone());
                if result.is_err() {
                    for a in owned {
                        self.release(a);
                    }
                }
                result
            }
        }
    }

    /// Runs `function` to completion.
    pub(crate) fn run(&mut self, function: FnId, args: Vec<Cell>) -> Result<Cell, EvalError> {
        self.push_frame(function, args, None, Vec::new())?;
        loop {
            let (body, block_id, ip) = {
                let f = self.top()?;
                (f.body, f.block, f.ip)
            };
            let block = body
                .block(block_id)
                .ok_or_else(|| self.internal("unknown block"))?;
            self.step()?;
            if let Some(inst) = block.instructions.get(ip) {
                self.instruction(&inst.kind)?;
                if let Some(f) = self.stack.last_mut() {
                    f.ip += 1;
                }
                continue;
            }
            match &block.terminator {
                Terminator::Goto { target } => self.goto(*target)?,
                Terminator::Switch {
                    discr,
                    arms,
                    otherwise,
                } => {
                    let key = match self.scalar(discr)? {
                        ConstValue::Int { bits, .. } => bits,
                        ConstValue::Bool(b) => u128::from(b),
                        ConstValue::Unit | ConstValue::Float { .. } => {
                            return Err(self.internal("switch on a non-integer"));
                        }
                    };
                    let target = arms
                        .iter()
                        .find(|(value, _)| *value == key)
                        .map_or(*otherwise, |(_, b)| *b);
                    self.goto(target)?;
                }
                Terminator::Return { value } => {
                    let result = self.operand(value)?;
                    self.pop_frame();
                    let Some(caller) = self.stack.last() else {
                        return Ok(result);
                    };
                    let call = caller
                        .body
                        .block(caller.block)
                        .map(|b| &b.terminator)
                        .ok_or_else(|| self.internal("unknown caller block"))?;
                    match call {
                        Terminator::Call { dest, target, .. } => {
                            let r = self.resolve(dest)?;
                            self.write(&r, result)?;
                            self.goto(*target)?;
                        }
                        Terminator::Goto { .. }
                        | Terminator::Switch { .. }
                        | Terminator::Return { .. }
                        | Terminator::Assert { .. }
                        | Terminator::Unreachable
                        | Terminator::Trap(_) => {
                            return Err(self.internal("return to a non-call terminator"));
                        }
                    }
                }
                // CT calls never suspend, so the cancel edge is never taken.
                Terminator::Call { callee, args, .. } => self.call(callee, args)?,
                Terminator::Assert {
                    cond,
                    expected,
                    target,
                    trap,
                } => match self.scalar(cond)? {
                    ConstValue::Bool(b) if b == *expected => self.goto(*target)?,
                    ConstValue::Bool(_) => return Err(self.trap(*trap)),
                    ConstValue::Unit | ConstValue::Int { .. } | ConstValue::Float { .. } => {
                        return Err(self.internal("assert on a non-bool"));
                    }
                },
                Terminator::Unreachable => {
                    return Err(self.internal("reached a terminator proven unreachable"));
                }
                Terminator::Trap(kind) => return Err(self.trap(*kind)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_in_a_result_escapes() {
        let leak = Cell::Agg {
            ty: TyId(0),
            variant: None,
            fields: vec![Cell::Ref(Ptr {
                alloc: 0,
                generation: 0,
                path: vec![],
            })],
        };
        assert_eq!(leak.into_value(FnId(0)), Err(EvalError::EscapingRef));
    }

    #[test]
    fn a_hole_in_a_result_is_internal() {
        let hole = Cell::ErrorUnion {
            ty: TyId(0),
            ok: true,
            payload: Box::new(Cell::Uninit),
        };
        assert!(matches!(
            hole.into_value(FnId(3)),
            Err(EvalError::Internal {
                function: FnId(3),
                ..
            })
        ));
    }
}
