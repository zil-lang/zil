//! Persistent scalar continuation frames. Source TaskYield becomes a saved pc;
//! all actual machine SSA and local storage survive return from poll.
use super::*;
mod owners;
pub(super) use owners::marker as owner_marker;
const STATE: u32 = 4;
const PC: u32 = 5;
const CANCEL: u32 = 6;
const RESULT: u32 = 7;
const MARKER: &str = "zil.task.yield.marker";
fn header() -> Vec<m::Ty> {
    vec![
        m::Ty::Ptr,
        m::Ty::Ptr,
        m::Ty::Ptr,
        m::Ty::Ptr,
        m::Ty::Int(32),
        m::Ty::Int(32),
        m::Ty::Int(1),
        m::Ty::Int(32),
    ]
}
// Unit has no Direct ABI value. The private poll/frame uses an ordinary
// zero byte, while the source Ready variant retains exactly Unit.
fn internal_result_type(ty: &m::Ty) -> m::Ty {
    if *ty == m::Ty::Void {
        m::Ty::Int(8)
    } else {
        ty.clone()
    }
}
fn result_ty() -> m::Ty {
    m::Ty::Struct(vec![m::Ty::Int(32), m::Ty::Int(32)])
}
fn typed_poll_name(payload: &Type) -> String {
    format!("zil.task.poll.typed.{}", payload.fingerprint())
}
pub(super) fn name(id: FunctionId, operation: &str) -> String {
    format!("zil.task.{operation}.{}", id.0)
}
/// One shared request meter, including parallel function batches. It never
/// renews per worker; task-free modules perform no Task inventory or debit.
pub(super) struct Meter(std::sync::atomic::AtomicUsize);
impl Meter {
    pub(super) fn new(left: usize) -> Self {
        Self(std::sync::atomic::AtomicUsize::new(left))
    }
    pub(super) fn remaining(&self) -> usize {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub(super) fn spend(&self, amount: usize) -> CompileResult<()> {
        self.0
            .try_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |left| left.checked_sub(amount),
            )
            .map(|_| ())
            .map_err(|_| {
                vec![Diagnostic::limit(
                    Phase::Codegen,
                    "persistent task lowering budget exceeded",
                    None,
                )]
            })
    }
}
// Bridge expansion is constant-sized in the independently admitted i32
// profile. This inventory shares the actual remaining task planning ledger.
pub(super) fn reserve_owner_actions(
    function: &TypedFunction,
    ownership: Option<&zil_verify::TaskOwnership>,
    meter: &Meter,
) -> CompileResult<()> {
    let Some(ownership) = ownership else {
        return Ok(());
    };
    meter.spend(function.blocks.len().saturating_add(1))?;
    let lookups = ownership
        .action_count()
        .checked_ilog2()
        .map_or(1, |n| n as usize + 2)
        .saturating_mul(12);
    for b in &function.blocks {
        meter.spend(b.instructions.len().saturating_mul(lookups))?;
    }
    // One instruction may consume an arbitrarily wide prepared argument list.
    // Pay actual markers, including their operand vectors and symbol strings,
    // before FunctionEmitter constructs any of them.
    meter.spend(ownership.action_count())?;
    for actions in ownership.actions() {
        meter.spend(
            actions
                .before()
                .len()
                .saturating_add(actions.after().len())
                .saturating_mul(256),
        )?;
    }
    meter.spend(
        function
            .blocks
            .len()
            .saturating_mul(ownership.owners().len())
            .saturating_mul(4)
            .saturating_add(
                function
                    .parameters
                    .len()
                    .saturating_mul(ownership.owners().len())
                    .saturating_mul(16),
            ),
    )
}
pub(super) fn reserve_copy_frame(
    function: &TypedFunction,
    meter: &Meter,
    limits: &zil_base::Limits,
) -> CompileResult<()> {
    fn structured(t: &Type) -> bool {
        matches!(
            t,
            Type::Managed(_)
                | Type::Package { .. }
                | Type::Record { .. }
                | Type::Tuple(_)
                | Type::Enum { .. }
                | Type::ErrorUnion { .. }
        )
    }
    meter.spend(
        function
            .locals
            .len()
            .saturating_add(function.values.len())
            .saturating_add(1),
    )?;
    if !structured(&function.return_type)
        && !function.locals.iter().any(|l| structured(&l.ty))
        && !function.values.iter().any(structured)
    {
        return Ok(());
    }
    meter.spend(function.blocks.len())?;
    for block in &function.blocks {
        meter.spend(block.instructions.len())?;
    }
    let before = meter.remaining();
    let mut left = before;
    let reserved: CompileResult<()> = (|| {
        for ty in std::iter::once(&function.return_type)
            .chain(function.locals.iter().map(|l| &l.ty))
            .chain(&function.values)
        {
            if structured(ty) {
                for _ in 0..16 {
                    copy_aggregates::reserve_enum_metadata(ty, &mut left, 0, limits)?;
                }
            }
        }
        for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
            if let Some(output) = instruction.result {
                let ty = &function.values[output.0 as usize];
                if structured(ty) {
                    let copies = match &instruction.kind {
                        InstructionKind::MakeAggregate { fields }
                        | InstructionKind::MakeManaged { fields }
                        | InstructionKind::MakePackage { fields }
                        | InstructionKind::MakeVariant { fields, .. } => {
                            fields.len().saturating_add(16)
                        }
                        _ => 16,
                    };
                    for _ in 0..copies {
                        copy_aggregates::reserve_enum_metadata(ty, &mut left, 0, limits)?;
                    }
                }
            }
        }
        Ok(())
    })();
    meter.spend(before - left)?;
    reserved
}
pub(super) fn reserve_bridges(function: &TypedFunction, meter: &Meter) -> CompileResult<()> {
    meter.spend(function.blocks.len())?;
    for block in &function.blocks {
        for i in &block.instructions {
            meter.spend(1)?;
            match &i.kind {
                InstructionKind::TaskCreate { arguments, .. } => {
                    meter.spend(256usize.saturating_add(arguments.len().saturating_mul(16)))?
                }
                InstructionKind::TaskPoll { task } | InstructionKind::TaskPollClose { task }
                    if matches!(&function.values[task.0 as usize],Type::Address{inner,..} if matches!(inner.as_ref(),Type::Managed(p) if matches!(p.as_ref(),Type::Task{..}))) =>
                {
                    meter.spend(1024)?
                }
                _ => {}
            }
        }
    }
    Ok(())
}
fn loc(s: impl Into<String>) -> m::Val {
    m::local(s.into())
}
fn inst(s: Option<&str>, op: m::Op) -> m::Inst {
    m::Inst::new(s.map(str::to_owned), op)
}
fn push(v: &mut Vec<m::Inst>, s: Option<&str>, op: m::Op) {
    v.push(inst(s, op));
}
fn block(name: &str, instructions: Vec<m::Inst>) -> m::Block {
    m::Block {
        name: name.into(),
        instructions,
    }
}
fn function(name: String, result: m::Ty, params: Vec<m::Param>, blocks: Vec<m::Block>) -> m::Item {
    // Preserve the explicit fallible allocation boundary: source-visible
    // task_valid failure must survive optimization of a private start/drop.
    let attrs = if name.starts_with("zil.task.init.") {
        vec![m::Attr::NoInline]
    } else {
        vec![]
    };
    m::Item::Function(m::Function {
        name,
        linkage: m::Linkage::Internal,
        signature: m::Signature::new(result, params),
        attrs,
        blocks: Some(blocks),
    })
}
fn gep(v: &mut Vec<m::Inst>, name: &str, frame: &m::Ty, base: m::Val, index: u32) {
    push(
        v,
        Some(name),
        m::Op::Gep {
            ty: frame.clone(),
            ptr: base,
            indices: vec![
                m::Arg::new(m::Ty::Int(32), 0),
                m::Arg::new(m::Ty::Int(32), index),
            ],
        },
    );
}
fn load(v: &mut Vec<m::Inst>, name: &str, ty: m::Ty, ptr: m::Val) {
    push(v, Some(name), m::Op::Load { ty, ptr, align: 1 });
}
fn store(v: &mut Vec<m::Inst>, ty: m::Ty, value: m::Val, ptr: m::Val) {
    push(
        v,
        None,
        m::Op::Store {
            arg: m::Arg::new(ty, value),
            ptr,
            align: 1,
        },
    );
}
fn call(
    v: &mut Vec<m::Inst>,
    output: Option<&str>,
    result: m::Ty,
    callee: m::Val,
    args: Vec<m::Arg>,
) {
    push(
        v,
        output,
        m::Op::Call {
            result,
            result_attrs: vec![],
            callee,
            args,
            vararg: None,
        },
    );
}
fn ret(v: &mut Vec<m::Inst>, ty: m::Ty, value: m::Val) {
    push(v, None, m::Op::Return(Some(m::Arg::new(ty, value))));
}
fn branch(v: &mut Vec<m::Inst>, target: &str) {
    push(v, None, m::Op::Branch(target.into()));
}
fn cond(v: &mut Vec<m::Inst>, condition: &str, yes: &str, no: &str) {
    push(
        v,
        None,
        m::Op::CondBranch {
            condition: loc(condition),
            yes: yes.into(),
            no: no.into(),
        },
    );
}
fn compare(
    v: &mut Vec<m::Inst>,
    output: &str,
    ty: m::Ty,
    left: m::Val,
    right: m::Val,
    predicate: &str,
) {
    push(
        v,
        Some(output),
        m::Op::Compare {
            predicate: predicate.into(),
            ty,
            left,
            right,
        },
    );
}
fn poll_result(v: &mut Vec<m::Inst>, status: i32, value: m::Val, prefix: &str) {
    poll_result_typed(v, status, value, prefix, &m::Ty::Int(32));
}
fn poll_result_typed(
    v: &mut Vec<m::Inst>,
    status: i32,
    value: m::Val,
    prefix: &str,
    payload: &m::Ty,
) {
    let result = m::Ty::Struct(vec![m::Ty::Int(32), payload.clone()]);
    push(
        v,
        Some(&format!("{prefix}.tag")),
        m::Op::Insert {
            arg: m::Arg::new(result.clone(), m::Val::Zero),
            field: m::Arg::new(m::Ty::Int(32), status),
            index: 0,
        },
    );
    push(
        v,
        Some(&format!("{prefix}.value")),
        m::Op::Insert {
            arg: m::Arg::new(result.clone(), loc(format!("{prefix}.tag"))),
            field: m::Arg::new(payload.clone(), value),
            index: 1,
        },
    );
    ret(v, result.clone(), loc(format!("{prefix}.value")));
}
fn trap_block() -> m::Block {
    let mut v = vec![];
    call(
        &mut v,
        None,
        m::Ty::Void,
        m::global("llvm.trap".into()),
        vec![],
    );
    push(&mut v, None, m::Op::Unreachable);
    block("task.trap", v)
}
fn op_type(op: &m::Op) -> Option<m::Ty> {
    match op {
        m::Op::Load { ty, .. } | m::Op::Binary { ty, .. } => Some(ty.clone()),
        m::Op::Compare { .. } => Some(m::Ty::Int(1)),
        m::Op::Cast { to, .. } => Some(to.clone()),
        m::Op::Call { result, .. } if *result != m::Ty::Void => Some(result.clone()),
        m::Op::Freeze(arg) | m::Op::Insert { arg, .. } => Some(arg.ty.clone()),
        m::Op::Extract { arg, index } => {
            if let m::Ty::Struct(fields) = &arg.ty {
                fields.get(*index as usize).cloned()
            } else {
                None
            }
        }
        m::Op::Gep { .. } => Some(m::Ty::Ptr),
        _ => None,
    }
}
#[derive(Clone)]
struct Slot {
    index: u32,
    ty: m::Ty,
    storage: bool,
}
// Rebuild each operand from its actual persistent slot. Original alloca names
// denote the persistent field itself; they are never pointers into a dead poll.
fn rewrite_value(
    value: &mut m::Val,
    slots: &BTreeMap<String, Slot>,
    pre: &mut Vec<m::Inst>,
    serial: &mut u64,
) {
    if let m::Val::Local(name) = value
        && let Some(slot) = slots.get(name)
    {
        let pointer = format!("task.slot.{}", slot.index);
        if slot.storage {
            *value = loc(pointer);
        } else {
            let next = format!("task.reload.{serial}");
            *serial += 1;
            load(pre, &next, slot.ty.clone(), loc(pointer));
            *value = loc(next);
        }
    }
}
fn rewrite(
    op: &mut m::Op,
    slots: &BTreeMap<String, Slot>,
    pre: &mut Vec<m::Inst>,
    serial: &mut u64,
) -> CompileResult<()> {
    let mut rv = |v: &mut m::Val| rewrite_value(v, slots, pre, serial);
    match op {
        m::Op::Alloca { .. } | m::Op::Branch(_) | m::Op::Unreachable | m::Op::Return(None) => {}
        m::Op::Load { ptr, .. } => rv(ptr),
        m::Op::Store { arg, ptr, .. } => {
            rv(&mut arg.value);
            rv(ptr);
        }
        m::Op::Binary { left, right, .. } | m::Op::Compare { left, right, .. } => {
            rv(left);
            rv(right);
        }
        m::Op::Cast { arg, .. }
        | m::Op::Extract { arg, .. }
        | m::Op::Freeze(arg)
        | m::Op::Return(Some(arg)) => rv(&mut arg.value),
        m::Op::Insert { arg, field, .. } => {
            rv(&mut arg.value);
            rv(&mut field.value);
        }
        m::Op::Gep { ptr, indices, .. } => {
            rv(ptr);
            for a in indices {
                rv(&mut a.value);
            }
        }
        m::Op::Call { callee, args, .. } => {
            rv(callee);
            for a in args {
                rv(&mut a.value);
            }
        }
        m::Op::CondBranch { condition, .. } => rv(condition),
        _ => {
            return invalid(
                "E-CODEGEN-TASK",
                "scalar task machine instruction has no persistent lowering",
                None,
            );
        }
    }
    Ok(())
}
const AWAIT_MARKER: &str = "zil.task.await.marker.";
fn awaited(op: &m::Op) -> Option<FunctionId> {
    let m::Op::Call {
        callee: m::Val::Global(name),
        ..
    } = op
    else {
        return None;
    };
    name.strip_prefix(AWAIT_MARKER)?
        .parse::<u32>()
        .ok()
        .map(FunctionId)
}
struct ChildFrame {
    worker: FunctionId,
    result: m::Ty,
    frame: u32,
    active: u32,
}
struct FramePlan {
    fields: Vec<m::Ty>,
    slots: BTreeMap<String, Slot>,
    children: Vec<ChildFrame>,
    owners: Option<owners::OwnerFrame>,
}
struct AwaitHandoff<'a> {
    child: &'a ChildFrame,
    result_marker: Option<m::Inst>,
}
fn split_await(
    mut current: m::Block,
    instruction: &m::Inst,
    handoff: AwaitHandoff<'_>,
    site: usize,
    pieces: &mut Vec<m::Block>,
    resumes: &mut BTreeSet<usize>,
    parent_result: &m::Ty,
) -> m::Block {
    let AwaitHandoff {
        child,
        result_marker,
    } = handoff;
    let m::Op::Call { args, .. } = &instruction.op else {
        unreachable!("actual await marker is a call")
    };
    let prefix = format!("task.await.{site}");
    let pointer = format!("task.child.{site}.frame");
    let active = format!("task.child.{site}.active");
    let polled = format!("{prefix}.poll");
    let mut arguments = vec![m::Arg::new(m::Ty::Ptr, loc(&pointer))];
    arguments.extend(args.iter().cloned());
    call(
        &mut current.instructions,
        None,
        m::Ty::Void,
        m::global(name(child.worker, "init_into")),
        arguments,
    );
    store(
        &mut current.instructions,
        m::Ty::Int(1),
        m::val(true),
        loc(&active),
    );
    branch(&mut current.instructions, &polled);
    pieces.push(current);
    // Only this PC is saved on Pending; the argument/initialization block is
    // never entered again when a later poll resumes the same child instance.
    let poll_pc = pieces.len();
    resumes.insert(poll_pc);
    let mut poll = vec![];
    call(
        &mut poll,
        Some(&format!("{prefix}.answer")),
        m::Ty::Struct(vec![m::Ty::Int(32), child.result.clone()]),
        m::global(name(child.worker, "poll")),
        vec![m::Arg::new(m::Ty::Ptr, loc(&pointer))],
    );
    push(
        &mut poll,
        Some(&format!("{prefix}.tag")),
        m::Op::Extract {
            arg: m::Arg::new(
                m::Ty::Struct(vec![m::Ty::Int(32), child.result.clone()]),
                loc(format!("{prefix}.answer")),
            ),
            index: 0,
        },
    );
    compare(
        &mut poll,
        &format!("{prefix}.is.ready"),
        m::Ty::Int(32),
        loc(format!("{prefix}.tag")),
        m::val(1),
        "eq",
    );
    cond(
        &mut poll,
        &format!("{prefix}.is.ready"),
        &format!("{prefix}.ready"),
        &format!("{prefix}.not.ready"),
    );
    pieces.push(block(&polled, poll));
    let mut pending_check = vec![];
    compare(
        &mut pending_check,
        &format!("{prefix}.is.pending"),
        m::Ty::Int(32),
        loc(format!("{prefix}.tag")),
        m::val(0),
        "eq",
    );
    cond(
        &mut pending_check,
        &format!("{prefix}.is.pending"),
        &format!("{prefix}.pending"),
        "task.trap",
    );
    pieces.push(block(&format!("{prefix}.not.ready"), pending_check));
    let mut pending = vec![];
    store(
        &mut pending,
        m::Ty::Int(32),
        m::val(poll_pc),
        loc("task.pc"),
    );
    store(&mut pending, m::Ty::Int(32), m::val(0), loc("task.state"));
    poll_result_typed(
        &mut pending,
        0,
        m::Val::Zero,
        &format!("{prefix}.pending"),
        parent_result,
    );
    pieces.push(block(&format!("{prefix}.pending"), pending));
    let mut ready = vec![];
    if instruction.result.is_some() {
        push(
            &mut ready,
            instruction.result.as_deref(),
            m::Op::Extract {
                arg: m::Arg::new(
                    m::Ty::Struct(vec![m::Ty::Int(32), child.result.clone()]),
                    loc(format!("{prefix}.answer")),
                ),
                index: 1,
            },
        );
    }
    // Extract is spilled to the canonical parent slot by the ordinary frame
    // rewriter before this exact Verify-derived result record becomes live.
    // Child teardown therefore runs with the transferred result owned by the
    // parent, not in an unrecorded transient interval.
    if let Some(marker) = result_marker {
        ready.push(marker);
    }
    branch(&mut ready, &format!("{prefix}.close"));
    pieces.push(block(&format!("{prefix}.ready"), ready));
    let close_pc = pieces.len();
    resumes.insert(close_pc);
    let mut close = vec![];
    call(
        &mut close,
        Some(&format!("{prefix}.closed")),
        m::Ty::Int(1),
        m::global(name(child.worker, "poll_close")),
        vec![m::Arg::new(m::Ty::Ptr, loc(&pointer))],
    );
    cond(
        &mut close,
        &format!("{prefix}.closed"),
        &format!("{prefix}.destroy"),
        &format!("{prefix}.close.pending"),
    );
    pieces.push(block(&format!("{prefix}.close"), close));
    let mut pending = vec![];
    store(
        &mut pending,
        m::Ty::Int(32),
        m::val(close_pc),
        loc("task.pc"),
    );
    store(&mut pending, m::Ty::Int(32), m::val(0), loc("task.state"));
    poll_result_typed(
        &mut pending,
        0,
        m::Val::Zero,
        &format!("{prefix}.close.pending"),
        parent_result,
    );
    pieces.push(block(&format!("{prefix}.close.pending"), pending));
    let mut destroy = vec![];
    call(
        &mut destroy,
        None,
        m::Ty::Void,
        m::global(name(child.worker, "finish_into")),
        vec![m::Arg::new(m::Ty::Ptr, loc(&pointer))],
    );
    store(&mut destroy, m::Ty::Int(1), m::val(false), loc(active));
    branch(&mut destroy, &format!("{prefix}.after"));
    pieces.push(block(&format!("{prefix}.destroy"), destroy));
    block(&format!("{prefix}.after"), vec![])
}
fn own_frame(machine: &m::Function, meter: &Meter) -> CompileResult<FramePlan> {
    meter.spend(2048usize.saturating_add(machine.signature.params.len().saturating_mul(16)))?;
    let mut fields = header();
    fields[RESULT as usize] = internal_result_type(&machine.signature.result);
    let mut slots = BTreeMap::new();
    for p in &machine.signature.params {
        let n = p.name.clone().expect("scalar worker parameters are named");
        slots.insert(
            n,
            Slot {
                index: fields.len() as u32,
                ty: p.ty.clone(),
                storage: false,
            },
        );
        fields.push(p.ty.clone());
    }
    let original = machine.blocks.as_ref().expect("scalar worker definition");
    meter.spend(original.len().saturating_mul(32))?;
    for b in original {
        for i in &b.instructions {
            meter.spend(128usize.saturating_add(i.op.values().count().saturating_mul(16)))?;
            if let Some(n) = &i.result {
                meter.spend(
                    n.len()
                        .saturating_add(slots.len().saturating_add(1).ilog2() as usize)
                        .saturating_add(16),
                )?;
                let (ty, storage) = if let m::Op::Alloca { ty, .. } = &i.op {
                    (ty.clone(), true)
                } else {
                    (
                        op_type(&i.op).ok_or_else(|| {
                            vec![Diagnostic::error(
                                Phase::Codegen,
                                "E-CODEGEN-TASK",
                                None,
                                "task SSA result has no finite spill type",
                            )]
                        })?,
                        false,
                    )
                };
                slots.insert(
                    n.clone(),
                    Slot {
                        index: fields.len() as u32,
                        ty: ty.clone(),
                        storage,
                    },
                );
                fields.push(ty);
            }
        }
    }
    meter.spend(slots.len().saturating_mul(16))?;
    Ok(FramePlan {
        fields,
        slots,
        children: vec![],
        owners: None,
    })
}
fn reserve_frame_type(ty: &m::Ty, meter: &Meter, depth: usize) -> CompileResult<()> {
    meter.spend(8)?;
    if depth > 64 {
        return invalid(
            "E-CODEGEN-TASK",
            "embedded task frame type depth exceeds profile",
            None,
        );
    }
    match ty {
        m::Ty::Struct(fields) => {
            meter.spend(fields.len())?;
            for field in fields {
                reserve_frame_type(field, meter, depth + 1)?;
            }
        }
        // Ordinary Enum payloads use one bounded array type, including a
        // zero-length area. Reserving a type does not enumerate array cells.
        m::Ty::Array(_, element) => reserve_frame_type(element, meter, depth + 1)?,
        // The child stores the actual generated named type, not an expanded
        // copy of its fields. Pay its cloned name here; physical layout and
        // identity resolve the unique actual definition in task_abi.
        m::Ty::Named(name) => meter.spend(name.len())?,
        m::Ty::Int(_) | m::Ty::Ptr => {}
        _ => {
            return invalid(
                "E-CODEGEN-TASK",
                "embedded task frame has an unsupported physical type",
                None,
            );
        }
    }
    Ok(())
}
fn plan_frame(
    id: FunctionId,
    machines: &BTreeMap<FunctionId, m::Function>,
    plans: &mut BTreeMap<FunctionId, FramePlan>,
    path: &mut BTreeSet<FunctionId>,
    meter: &Meter,
    verified: Option<&VerifiedModule>,
) -> CompileResult<()> {
    meter.spend(plans.len().saturating_add(path.len()).saturating_add(1))?;
    if plans.contains_key(&id) {
        return Ok(());
    }
    if path.len() >= 64 || !path.insert(id) {
        return invalid(
            "E-CODEGEN-TASK",
            "static Suspend child graph is recursive or exceeds depth profile",
            None,
        );
    }
    let machine = machines.get(&id).ok_or_else(|| {
        vec![Diagnostic::error(
            Phase::Codegen,
            "E-CODEGEN-TASK",
            None,
            "actual Suspend child machine is absent",
        )]
    })?;
    if verified.is_some() {
        let before = meter.remaining();
        let mut left = before;
        let reservation: CompileResult<()> = (|| {
            for _ in 0..16 {
                crate::task_abi::prepay_owned_frame(machine, &mut left)?;
            }
            Ok(())
        })();
        meter.spend(before - left)?;
        reservation?;
    }
    let mut plan = own_frame(machine, meter)?;
    if let Some(verified) = verified {
        owners::plan(
            &mut plan,
            verified.task_ownership(id),
            &verified.module().resources,
            verified.limits(),
            meter,
        )?;
    }
    for block in machine.blocks.as_ref().expect("task machine definition") {
        for instruction in &block.instructions {
            meter.spend(1)?;
            if let Some(worker) = awaited(&instruction.op) {
                plan_frame(worker, machines, plans, path, meter, verified)?;
                let child = &plans[&worker];
                // Prepay each later child-result clone in the poll call and
                // aggregate extraction before constructing its machine type.
                let result_type = internal_result_type(&machines[&worker].signature.result);
                let result = &result_type;
                for _ in 0..8 {
                    reserve_frame_type(result, meter, 0)?;
                }
                let result = result.clone();
                for field in &child.fields {
                    reserve_frame_type(field, meter, 0)?;
                }
                let frame = plan.fields.len() as u32;
                plan.fields.push(m::Ty::Struct(child.fields.clone()));
                let active = plan.fields.len() as u32;
                plan.fields.push(m::Ty::Int(1));
                plan.children.push(ChildFrame {
                    worker,
                    result,
                    frame,
                    active,
                });
            }
        }
    }
    path.remove(&id);
    plans.insert(id, plan);
    Ok(())
}
pub(super) fn lower_workers(
    machines: Vec<(FunctionId, m::Function)>,
    functions: &BTreeMap<FunctionId, &TypedFunction>,
    verified: &VerifiedModule,
    meter: &Meter,
) -> CompileResult<Vec<m::Item>> {
    if machines.is_empty() {
        return Ok(vec![]);
    }
    meter.spend(machines.len().saturating_mul(4))?;
    let mut actual = BTreeMap::new();
    for (id, machine) in machines {
        meter.spend(actual.len().checked_ilog2().map_or(1, |n| n as usize + 2))?;
        if actual.insert(id, machine).is_some() {
            return invalid(
                "E-CODEGEN-TASK",
                "duplicate actual task worker machine",
                None,
            );
        }
    }
    let machines = actual;
    let mut plans = BTreeMap::new();
    let mut path = BTreeSet::new();
    for &id in machines.keys() {
        plan_frame(id, &machines, &mut plans, &mut path, meter, Some(verified))?;
    }
    let mut items = vec![];
    if machines
        .keys()
        .any(|id| verified.task_ownership(*id).is_some())
    {
        items.extend(owners::helpers(meter)?);
    }
    for (id, mut machine) in machines {
        let plan = plans.remove(&id).expect("actual frame was planned");
        items.extend(lower_worker(id, &machine, plan, meter, verified)?);
        if verified.is_task_export(id) {
            items.extend(exports(id, meter)?);
        }
        if !verified.is_suspend(id) {
            for block in machine.blocks.as_mut().expect("actual Direct body") {
                block.instructions.retain(|i| !owners::is_marker(&i.op));
            }
            scalar_locals::promote(functions[&id], &mut machine);
            items.push(m::Item::Function(machine));
        }
    }
    Ok(items)
}
fn lower_worker(
    id: FunctionId,
    machine: &m::Function,
    plan: FramePlan,
    meter: &Meter,
    verified: &VerifiedModule,
) -> CompileResult<Vec<m::Item>> {
    let payload_storage = internal_result_type(&machine.signature.result);
    let payload_ty = &payload_storage;
    let poll_ty = m::Ty::Struct(vec![m::Ty::Int(32), payload_ty.clone()]);
    let frame_name = format!("zil_task_frame_{}", id.0);
    let FramePlan {
        fields: frame_fields,
        slots,
        children,
        owners,
    } = plan;
    let frame = m::Ty::Named(frame_name.clone());
    let original = machine.blocks.as_ref().expect("scalar worker definition");
    meter.spend(children.len().saturating_mul(4096))?;
    let mut child_site = 0;
    let mut pieces: Vec<m::Block> = vec![];
    let mut resumes = BTreeSet::from([0]);
    for b in original {
        let mut current = block(&b.name, vec![]);
        let mut original_instructions = b.instructions.iter().peekable();
        while let Some(i) = original_instructions.next() {
            if matches!(&i.op, m::Op::Call { callee: m::Val::Global(n), .. } if n == MARKER) {
                let next = format!("task.resume.{}", pieces.len());
                // The resume pc is its exact position in the final block table.
                let pc = pieces.len() + 1;
                resumes.insert(pc);
                store(
                    &mut current.instructions,
                    m::Ty::Int(32),
                    m::val(pc),
                    loc("task.pc"),
                );
                store(
                    &mut current.instructions,
                    m::Ty::Int(32),
                    m::val(0),
                    loc("task.state"),
                );
                poll_result_typed(
                    &mut current.instructions,
                    0,
                    m::Val::Zero,
                    &format!("pending.{pc}"),
                    payload_ty,
                );
                pieces.push(current);
                current = block(&next, vec![]);
            } else if matches!(&i.op,m::Op::Call{callee:m::Val::Global(n),..} if n=="zil.async_registration.drop.marker")
            {
                let m::Op::Call { args, .. } = &i.op else {
                    unreachable!()
                };
                let prefix = format!("async.drop.{}", pieces.len());
                let loop_label = format!("{prefix}.poll");
                branch(&mut current.instructions, &loop_label);
                pieces.push(current);
                let pc = pieces.len();
                resumes.insert(pc);
                let mut drain = vec![];
                call(
                    &mut drain,
                    Some(&format!("{prefix}.ready")),
                    m::Ty::Int(1),
                    m::global("zil.async_registration.drop".into()),
                    args.clone(),
                );
                cond(
                    &mut drain,
                    &format!("{prefix}.ready"),
                    &format!("{prefix}.after"),
                    &format!("{prefix}.pending"),
                );
                pieces.push(block(&loop_label, drain));
                let mut pending = vec![];
                store(&mut pending, m::Ty::Int(32), m::val(pc), loc("task.pc"));
                store(&mut pending, m::Ty::Int(32), m::val(0), loc("task.state"));
                poll_result_typed(&mut pending, 0, m::Val::Zero, &prefix, payload_ty);
                pieces.push(block(&format!("{prefix}.pending"), pending));
                current = block(&format!("{prefix}.after"), vec![]);
            } else if awaited(&i.op).is_some() {
                let mut result_marker = None;
                if let Some(plan) = verified.task_ownership(id) {
                    meter.spend(plan.owners().len().saturating_add(512))?;
                    let output = i
                        .result
                        .as_deref()
                        .and_then(|name| name.strip_prefix("v."))
                        .and_then(|id| id.parse::<u32>().ok())
                        .map(ValueId);
                    if let Some(owner) = output.and_then(|out| plan.owner_for_value(out)) {
                        let m::Event::Instruction(expected) =
                            owners::marker(zil_verify::TaskOwnerAction::Link(owner))
                        else {
                            unreachable!("owner marker instruction")
                        };
                        if !original_instructions
                            .peek()
                            .is_some_and(|actual| actual.op == expected.op)
                        {
                            return invalid(
                                "E-CODEGEN-TASK",
                                "owned Await result lacks its exact verified handoff marker",
                                None,
                            );
                        }
                        original_instructions.next();
                        result_marker = Some(expected);
                    }
                }
                current = split_await(
                    current,
                    i,
                    AwaitHandoff {
                        child: &children[child_site],
                        result_marker,
                    },
                    child_site,
                    &mut pieces,
                    &mut resumes,
                    payload_ty,
                );
                child_site += 1;
            } else {
                current.instructions.push(i.clone());
            }
        }
        pieces.push(current);
    }
    // pc=0 starts the original entry. Every yield stores the next piece index.
    let mut setup = vec![];
    owners::setup(&mut setup, &frame, owners.as_ref());
    for (index, label) in [
        (STATE, "task.state"),
        (PC, "task.pc"),
        (CANCEL, "task.cancel"),
        (RESULT, "task.result"),
    ] {
        gep(&mut setup, label, &frame, loc("frame"), index);
    }
    for slot in slots.values() {
        gep(
            &mut setup,
            &format!("task.slot.{}", slot.index),
            &frame,
            loc("frame"),
            slot.index,
        );
    }
    for (site, child) in children.iter().enumerate() {
        gep(
            &mut setup,
            &format!("task.child.{site}.frame"),
            &frame,
            loc("frame"),
            child.frame,
        );
        gep(
            &mut setup,
            &format!("task.child.{site}.active"),
            &frame,
            loc("frame"),
            child.active,
        );
    }
    load(
        &mut setup,
        "task.old.state",
        m::Ty::Int(32),
        loc("task.state"),
    );
    compare(
        &mut setup,
        "task.open",
        m::Ty::Int(32),
        loc("task.old.state"),
        m::val(0),
        "eq",
    );
    cond(
        &mut setup,
        "task.open",
        "task.check.cancel",
        "task.finished",
    );
    let mut poll = vec![block("task.entry", setup)];
    let mut v = vec![];
    load(&mut v, "task.cancelled", m::Ty::Int(1), loc("task.cancel"));
    cond(
        &mut v,
        "task.cancelled",
        "task.cancelled.result",
        "task.dispatch",
    );
    poll.push(block("task.check.cancel", v));
    let mut v = vec![];
    store(&mut v, m::Ty::Int(32), m::val(4), loc("task.state"));
    load(&mut v, "task.saved.pc", m::Ty::Int(32), loc("task.pc"));
    branch(&mut v, "task.select.0");
    poll.push(block("task.dispatch", v));
    for (choice, &index) in resumes.iter().enumerate() {
        let piece = &pieces[index];
        let mut v = vec![];
        compare(
            &mut v,
            &format!("task.pc.is.{index}"),
            m::Ty::Int(32),
            loc("task.saved.pc"),
            m::val(index),
            "eq",
        );
        let no = if choice + 1 == resumes.len() {
            "task.trap".into()
        } else {
            format!("task.select.{}", choice + 1)
        };
        cond(&mut v, &format!("task.pc.is.{index}"), &piece.name, &no);
        poll.push(block(&format!("task.select.{choice}"), v));
    }
    let mut serial = 0;
    for mut piece in pieces {
        let mut instructions = vec![];
        for mut i in piece.instructions {
            if matches!(i.op, m::Op::Alloca { .. }) {
                continue;
            }
            owners::lower(&mut i.op)?;
            rewrite(&mut i.op, &slots, &mut instructions, &mut serial)?;
            if matches!(i.op, m::Op::Return(None)) && machine.signature.result == m::Ty::Void {
                store(
                    &mut instructions,
                    payload_ty.clone(),
                    m::Val::Zero,
                    loc("task.result"),
                );
                store(
                    &mut instructions,
                    m::Ty::Int(32),
                    m::val(1),
                    loc("task.state"),
                );
                poll_result_typed(
                    &mut instructions,
                    1,
                    m::Val::Zero,
                    &format!("unit.ready.{serial}"),
                    payload_ty,
                );
                serial += 1;
                continue;
            }
            if let m::Op::Return(Some(arg)) = &i.op {
                // Generated Pending returns already have the SABI result type.
                if arg.ty == *payload_ty {
                    store(
                        &mut instructions,
                        payload_ty.clone(),
                        arg.value.clone(),
                        loc("task.result"),
                    );
                    owners::result_debt(&mut instructions, owners.as_ref(), true);
                    store(
                        &mut instructions,
                        m::Ty::Int(32),
                        m::val(1),
                        loc("task.state"),
                    );
                    load(
                        &mut instructions,
                        &format!("task.ready.{serial}"),
                        payload_ty.clone(),
                        loc("task.result"),
                    );
                    let ready = loc(format!("task.ready.{serial}"));
                    serial += 1;
                    owners::result_debt(&mut instructions, owners.as_ref(), false);
                    poll_result_typed(
                        &mut instructions,
                        1,
                        ready,
                        &format!("ready.{serial}"),
                        payload_ty,
                    );
                    continue;
                }
            }
            let output = i.result.clone();
            instructions.push(i);
            if let Some(slot) = output.as_ref().and_then(|n| slots.get(n)) {
                store(
                    &mut instructions,
                    slot.ty.clone(),
                    loc(output.unwrap()),
                    loc(format!("task.slot.{}", slot.index)),
                );
            }
        }
        piece.instructions = instructions;
        poll.push(piece);
    }
    let mut v = vec![];
    compare(
        &mut v,
        "task.taken",
        m::Ty::Int(32),
        loc("task.old.state"),
        m::val(1),
        "eq",
    );
    cond(
        &mut v,
        "task.taken",
        "task.complete.result",
        "task.was.cancelled",
    );
    poll.push(block("task.finished", v));
    let mut v = vec![];
    compare(
        &mut v,
        "task.was.cancelled.flag",
        m::Ty::Int(32),
        loc("task.old.state"),
        m::val(2),
        "eq",
    );
    cond(
        &mut v,
        "task.was.cancelled.flag",
        "task.cancelled.result",
        "task.invalid.result",
    );
    poll.push(block("task.was.cancelled", v));
    for (label, status) in [
        ("task.complete.result", 2),
        ("task.cancelled.result", 3),
        ("task.invalid.result", 4),
    ] {
        let mut v = vec![];
        if status == 3 {
            store(&mut v, m::Ty::Int(32), m::val(2), loc("task.state"));
        }
        poll_result_typed(&mut v, status, m::Val::Zero, label, payload_ty);
        poll.push(block(label, v));
    }
    poll.push(trap_block());
    let mut items = vec![
        m::Item::NamedType(frame_name, frame_fields),
        function(
            name(id, "poll"),
            poll_ty,
            vec![m::Param::new(m::Ty::Ptr, "frame")],
            poll,
        ),
    ];
    let mut init = vec![];
    push(
        &mut init,
        Some("size.ptr"),
        m::Op::Gep {
            ty: frame.clone(),
            ptr: m::Val::Null,
            indices: vec![m::Arg::new(m::Ty::Int(64), 1)],
        },
    );
    push(
        &mut init,
        Some("size"),
        m::Op::Cast {
            opcode: "ptrtoint".into(),
            arg: m::Arg::new(m::Ty::Ptr, loc("size.ptr")),
            to: m::Ty::Int(64),
        },
    );
    call(
        &mut init,
        Some("frame"),
        m::Ty::Ptr,
        m::global("calloc".into()),
        vec![
            m::Arg::new(m::Ty::Int(64), 1),
            m::Arg::new(m::Ty::Int(64), loc("size")),
        ],
    );
    compare(
        &mut init,
        "allocated",
        m::Ty::Ptr,
        loc("frame"),
        m::Val::Null,
        "ne",
    );
    cond(
        &mut init,
        "allocated",
        "init",
        if owners.is_some() {
            "failure"
        } else {
            "return"
        },
    );
    let mut initialize = vec![];
    store(&mut initialize, frame.clone(), m::Val::Zero, loc("frame"));
    for (index, operation) in ["poll", "request_cancel", "poll_close", "finish_into"]
        .iter()
        .enumerate()
    {
        let slot = format!("header.{index}");
        gep(&mut initialize, &slot, &frame, loc("frame"), index as u32);
        store(
            &mut initialize,
            m::Ty::Ptr,
            m::global(name(id, operation)),
            loc(slot),
        );
    }
    for p in &machine.signature.params {
        let s = &slots[p.name.as_ref().unwrap()];
        let pointer = format!("argument.{}", s.index);
        gep(&mut initialize, &pointer, &frame, loc("frame"), s.index);
        store(
            &mut initialize,
            p.ty.clone(),
            loc(p.name.clone().unwrap()),
            loc(pointer),
        );
    }
    owners::initialize(
        &mut initialize,
        &frame,
        &slots,
        owners.as_ref(),
        verified.task_ownership(id),
        &verified.module().functions[id.0 as usize],
    )?;
    push(&mut initialize, None, m::Op::Return(None));
    let mut into_parameters = vec![m::Param::new(m::Ty::Ptr, "frame")];
    into_parameters.extend(machine.signature.params.clone());
    items.push(function(
        name(id, "init_into"),
        m::Ty::Void,
        into_parameters,
        vec![block("entry", initialize)],
    ));
    let mut commit = vec![];
    let mut arguments = vec![m::Arg::new(m::Ty::Ptr, loc("frame"))];
    arguments.extend(
        machine
            .signature
            .params
            .iter()
            .map(|p| m::Arg::new(p.ty.clone(), loc(p.name.clone().unwrap()))),
    );
    call(
        &mut commit,
        None,
        m::Ty::Void,
        m::global(name(id, "init_into")),
        arguments,
    );
    gep(&mut commit, "owner.destroy", &frame, loc("frame"), 3);
    store(
        &mut commit,
        m::Ty::Ptr,
        m::global(name(id, "finish_destroy")),
        loc("owner.destroy"),
    );
    branch(&mut commit, "return");
    let mut returned = vec![];
    ret(&mut returned, m::Ty::Ptr, loc("frame"));
    let mut init_blocks = vec![block("entry", init), block("init", commit)];
    if owners.is_some() {
        let mut failed = vec![];
        owners::allocation_failure(
            &mut failed,
            owners.as_ref(),
            verified.task_ownership(id),
            &verified.module().functions[id.0 as usize],
        )?;
        branch(&mut failed, "return");
        init_blocks.push(block("failure", failed));
    }
    init_blocks.push(block("return", returned));
    items.push(function(
        name(id, "init"),
        m::Ty::Ptr,
        machine.signature.params.clone(),
        init_blocks,
    ));
    let mut cancel = vec![];
    gep(&mut cancel, "cancel.state", &frame, loc("frame"), STATE);
    load(
        &mut cancel,
        "cancel.previous",
        m::Ty::Int(32),
        loc("cancel.state"),
    );
    store(&mut cancel, m::Ty::Int(32), m::val(4), loc("cancel.state"));
    gep(&mut cancel, "cancel", &frame, loc("frame"), CANCEL);
    store(&mut cancel, m::Ty::Int(1), m::val(true), loc("cancel"));
    let mut cancel_blocks = vec![];
    let mut label = "entry".to_owned();
    for (site, child) in children.iter().enumerate() {
        let pointer = format!("child.{site}.frame");
        let active = format!("child.{site}.active");
        let is_active = format!("child.{site}.is.active");
        let notify = format!("child.{site}.notify");
        let next = format!("child.{site}.next");
        gep(&mut cancel, &pointer, &frame, loc("frame"), child.frame);
        gep(&mut cancel, &active, &frame, loc("frame"), child.active);
        load(&mut cancel, &is_active, m::Ty::Int(1), loc(active));
        cond(&mut cancel, &is_active, &notify, &next);
        cancel_blocks.push(block(&label, cancel));
        let mut notified = vec![];
        call(
            &mut notified,
            None,
            m::Ty::Void,
            m::global(name(child.worker, "request_cancel")),
            vec![m::Arg::new(m::Ty::Ptr, loc(pointer))],
        );
        branch(&mut notified, &next);
        cancel_blocks.push(block(&notify, notified));
        label = next;
        cancel = vec![];
    }
    owners::request(&mut cancel, &frame, owners.as_ref());
    store(
        &mut cancel,
        m::Ty::Int(32),
        loc("cancel.previous"),
        loc("cancel.state"),
    );
    push(&mut cancel, None, m::Op::Return(None));
    cancel_blocks.push(block(&label, cancel));
    items.push(function(
        name(id, "request_cancel"),
        m::Ty::Void,
        vec![m::Param::new(m::Ty::Ptr, "frame")],
        cancel_blocks,
    ));
    let mut close = vec![];
    gep(&mut close, "state", &frame, loc("frame"), STATE);
    load(&mut close, "current", m::Ty::Int(32), loc("state"));
    compare(
        &mut close,
        "active",
        m::Ty::Int(32),
        loc("current"),
        m::val(4),
        "eq",
    );
    cond(&mut close, "active", "busy.pending", "close");
    let mut close_blocks = vec![block("entry", close)];
    let mut closed = vec![];
    store(&mut closed, m::Ty::Int(32), m::val(4), loc("state"));
    // The independently admitted scalar lane has sequential Await only:
    // at most one direct child is active in each parent frame. Static site
    // order therefore agrees with the active creation order here. Owning or
    // concurrent sibling children must use their actual dynamic creation order.
    // Notify every active child before starting the drain.
    call(
        &mut closed,
        None,
        m::Ty::Void,
        m::global(name(id, "request_cancel")),
        vec![m::Arg::new(m::Ty::Ptr, loc("frame"))],
    );
    let mut label = "close".to_owned();
    for (site, child) in children.iter().enumerate() {
        let pointer = format!("child.{site}.frame");
        let active = format!("child.{site}.active");
        let is_active = format!("child.{site}.is.active");
        let drain = format!("child.{site}.drain");
        let destroy = format!("child.{site}.destroy");
        let next = format!("child.{site}.next");
        gep(&mut closed, &pointer, &frame, loc("frame"), child.frame);
        gep(&mut closed, &active, &frame, loc("frame"), child.active);
        load(&mut closed, &is_active, m::Ty::Int(1), loc(&active));
        cond(&mut closed, &is_active, &drain, &next);
        close_blocks.push(block(&label, closed));
        let mut drained = vec![];
        let answer = format!("child.{site}.closed");
        call(
            &mut drained,
            Some(&answer),
            m::Ty::Int(1),
            m::global(name(child.worker, "poll_close")),
            vec![m::Arg::new(m::Ty::Ptr, loc(&pointer))],
        );
        cond(&mut drained, &answer, &destroy, "pending");
        close_blocks.push(block(&drain, drained));
        let mut destroyed = vec![];
        call(
            &mut destroyed,
            None,
            m::Ty::Void,
            m::global(name(child.worker, "finish_into")),
            vec![m::Arg::new(m::Ty::Ptr, loc(pointer))],
        );
        store(&mut destroyed, m::Ty::Int(1), m::val(false), loc(active));
        branch(&mut destroyed, &next);
        close_blocks.push(block(&destroy, destroyed));
        label = next;
        closed = vec![];
    }
    owners::drain(&mut closed, &frame, owners.as_ref());
    if owners.is_some() {
        close_blocks.push(block(&label, closed));
        label = "owners.closed".into();
        closed = vec![];
    }
    store(&mut closed, m::Ty::Int(32), m::val(3), loc("state"));
    ret(&mut closed, m::Ty::Int(1), m::val(true));
    close_blocks.push(block(&label, closed));
    let mut pending = vec![];
    store(&mut pending, m::Ty::Int(32), m::val(5), loc("state"));
    ret(&mut pending, m::Ty::Int(1), m::val(false));
    close_blocks.push(block("pending", pending));
    close_blocks.push(block(
        "busy.pending",
        vec![inst(
            None,
            m::Op::Return(Some(m::Arg::new(m::Ty::Int(1), false))),
        )],
    ));
    items.push(function(
        name(id, "poll_close"),
        m::Ty::Int(1),
        vec![m::Param::new(m::Ty::Ptr, "frame")],
        close_blocks,
    ));
    let mut into_destroy = vec![];
    gep(&mut into_destroy, "state", &frame, loc("frame"), STATE);
    load(&mut into_destroy, "current", m::Ty::Int(32), loc("state"));
    compare(
        &mut into_destroy,
        "closed",
        m::Ty::Int(32),
        loc("current"),
        m::val(3),
        "eq",
    );
    cond(&mut into_destroy, "closed", "done", "task.trap");
    items.push(function(
        name(id, "finish_into"),
        m::Ty::Void,
        vec![m::Param::new(m::Ty::Ptr, "frame")],
        vec![
            block("entry", into_destroy),
            block("done", vec![inst(None, m::Op::Return(None))]),
            trap_block(),
        ],
    ));
    let mut destroy = vec![];
    gep(&mut destroy, "state", &frame, loc("frame"), STATE);
    load(&mut destroy, "current", m::Ty::Int(32), loc("state"));
    compare(
        &mut destroy,
        "closed",
        m::Ty::Int(32),
        loc("current"),
        m::val(3),
        "eq",
    );
    cond(&mut destroy, "closed", "destroy", "task.trap");
    let mut freed = vec![];
    call(
        &mut freed,
        None,
        m::Ty::Void,
        m::global("free".into()),
        vec![m::Arg::new(m::Ty::Ptr, loc("frame"))],
    );
    push(&mut freed, None, m::Op::Return(None));
    items.push(function(
        name(id, "finish_destroy"),
        m::Ty::Void,
        vec![m::Param::new(m::Ty::Ptr, "frame")],
        vec![
            block("entry", destroy),
            block("destroy", freed),
            trap_block(),
        ],
    ));
    Ok(items)
}

pub(super) fn runtime(
    lir: &LirView<'_>,
    enabled: bool,
    limits: &zil_base::Limits,
    meter: &Meter,
) -> CompileResult<m::Code> {
    if !enabled {
        return Ok(m::Code::new());
    }
    meter.spend(
        2048usize
            .saturating_add(lir.resources.len())
            .saturating_add(lir.functions.len()),
    )?;
    if lir.target.triple != "x86_64-unknown-linux-gnu" {
        return unsupported(
            "scalar task allocation currently requires x86_64-unknown-linux-gnu",
            None,
        );
    }
    let mut items = vec![];
    for f in lir.functions {
        if f.foreign.as_ref().is_some_and(|foreign| {
            foreign.symbol.starts_with("zil.task.")
                || foreign.symbol.starts_with("zil.sabi.task.")
                || foreign.symbol.starts_with("zil_task_frame_")
        }) {
            return invalid(
                "E-CODEGEN-FOREIGN",
                "foreign symbol conflicts with sealed task ABI",
                Some(f.span),
            );
        }
    }
    let byte_allocator = lir
        .resources
        .iter()
        .any(|r| r.payload == Type::ByteBuffer || matches!(r.payload, Type::OwnedStore { .. }));
    for (n, result, params) in [
        ("calloc", m::Ty::Ptr, vec![m::Ty::Int(64), m::Ty::Int(64)]),
        ("free", m::Ty::Void, vec![m::Ty::Ptr]),
    ] {
        if n == "free" && byte_allocator {
            continue;
        }
        if let Some(f) = lir.functions.iter().find(|f| {
            f.foreign
                .as_ref()
                .is_some_and(|foreign| foreign.symbol == n)
        }) {
            meter.spend(f.parameters.len().saturating_add(1))?;
            let before = meter.remaining();
            let mut left = before;
            for local in &f.parameters {
                copy_aggregates::reserve_enum_metadata(
                    &f.locals[local.0 as usize].ty,
                    &mut left,
                    0,
                    limits,
                )?;
            }
            copy_aggregates::reserve_enum_metadata(&f.return_type, &mut left, 0, limits)?;
            meter.spend(before - left)?;
            let actual = f
                .parameters
                .iter()
                .map(|id| llvm_type(&f.locals[id.0 as usize].ty))
                .collect::<CompileResult<Vec<_>>>()?;
            if actual != params || llvm_type(&f.return_type)? != result {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    format!("{n} conflicts with task allocation ABI"),
                    Some(f.span),
                );
            }
        } else {
            items.push(m::Item::Function(m::Function {
                name: n.into(),
                linkage: m::Linkage::External,
                signature: m::Signature::new(
                    result,
                    params.into_iter().map(m::Param::unnamed).collect(),
                ),
                attrs: vec![],
                blocks: None,
            }));
        }
    }
    let frame = m::Ty::Struct(header());
    for (operation, index, result, empty) in [
        ("poll", 0, result_ty(), Some(4)),
        ("request_cancel", 1, m::Ty::Void, None),
        ("poll_close", 2, m::Ty::Int(1), Some(1)),
        ("finish_destroy", 3, m::Ty::Void, None),
    ] {
        let mut entry = vec![];
        if index == 3 {
            compare(
                &mut entry,
                "valid",
                m::Ty::Ptr,
                loc("task"),
                m::Val::Null,
                "ne",
            );
        } else {
            load(&mut entry, "frame", m::Ty::Ptr, loc("task"));
            compare(
                &mut entry,
                "valid",
                m::Ty::Ptr,
                loc("frame"),
                m::Val::Null,
                "ne",
            );
        }
        cond(&mut entry, "valid", "invoke", "empty");
        let actual_frame = if index == 3 { "task" } else { "frame" };
        let mut invoke = vec![];
        gep(&mut invoke, "entry.slot", &frame, loc(actual_frame), index);
        load(&mut invoke, "entry.fn", m::Ty::Ptr, loc("entry.slot"));
        call(
            &mut invoke,
            if result == m::Ty::Void {
                None
            } else {
                Some("answer")
            },
            result.clone(),
            loc("entry.fn"),
            vec![m::Arg::new(m::Ty::Ptr, loc(actual_frame))],
        );
        if result == m::Ty::Void {
            push(&mut invoke, None, m::Op::Return(None));
        } else {
            ret(&mut invoke, result.clone(), loc("answer"));
        }
        let mut invalid = vec![];
        if index == 0 {
            poll_result(&mut invalid, 4, m::val(0), "invalid");
        } else if let Some(value) = empty {
            ret(&mut invalid, result.clone(), m::val(value != 0));
        } else {
            push(&mut invalid, None, m::Op::Return(None));
        }
        items.push(function(
            format!("zil.task.{operation}"),
            result,
            vec![m::Param::new(m::Ty::Ptr, "task")],
            vec![
                block("entry", entry),
                block("invoke", invoke),
                block("empty", invalid),
            ],
        ));
    }
    let mut drop = vec![];
    call(
        &mut drop,
        None,
        m::Ty::Void,
        m::global("zil.task.request_cancel".into()),
        vec![m::Arg::new(m::Ty::Ptr, loc("head"))],
    );
    call(
        &mut drop,
        Some("closed"),
        m::Ty::Int(1),
        m::global("zil.task.poll_close".into()),
        vec![m::Arg::new(m::Ty::Ptr, loc("head"))],
    );
    cond(&mut drop, "closed", "destroy", "task.trap");
    let mut destroy = vec![];
    load(&mut destroy, "frame", m::Ty::Ptr, loc("head"));
    call(
        &mut destroy,
        None,
        m::Ty::Void,
        m::global("zil.task.finish_destroy".into()),
        vec![m::Arg::new(m::Ty::Ptr, loc("frame"))],
    );
    store(&mut destroy, m::Ty::Ptr, m::Val::Null, loc("head"));
    push(&mut destroy, None, m::Op::Return(None));
    items.push(function(
        "zil.task.drop".into(),
        m::Ty::Void,
        vec![m::Param::new(m::Ty::Ptr, "head")],
        vec![
            block("entry", drop),
            block("destroy", destroy),
            trap_block(),
        ],
    ));
    for resource in lir.resources {
        let Type::Task { result, .. } = &resource.payload else {
            continue;
        };
        if **result == Type::I32 {
            continue;
        }
        let before = meter.remaining();
        let mut left = before;
        let reservation: CompileResult<()> = (|| {
            for _ in 0..16 {
                copy_aggregates::reserve_enum_metadata(&resource.payload, &mut left, 0, limits)?;
            }
            Ok(())
        })();
        meter.spend(before - left)?;
        reservation?;
        meter.spend(2048)?;
        let payload = aggregate_field_type(result)?;
        let response = m::Ty::Struct(vec![m::Ty::Int(32), payload.clone()]);
        let mut entry = vec![];
        load(&mut entry, "frame", m::Ty::Ptr, loc("owner"));
        compare(
            &mut entry,
            "valid",
            m::Ty::Ptr,
            loc("frame"),
            m::Val::Null,
            "ne",
        );
        cond(&mut entry, "valid", "poll", "invalid");
        let mut poll = vec![];
        load(&mut poll, "poll.fn", m::Ty::Ptr, loc("frame"));
        call(
            &mut poll,
            Some("answer"),
            response.clone(),
            loc("poll.fn"),
            vec![m::Arg::new(m::Ty::Ptr, loc("frame"))],
        );
        ret(&mut poll, response.clone(), loc("answer"));
        let mut invalid = vec![];
        poll_result_typed(&mut invalid, 4, m::Val::Zero, "invalid", &payload);
        items.push(function(
            typed_poll_name(&resource.payload),
            response,
            vec![m::Param::new(m::Ty::Ptr, "owner")],
            vec![
                block("entry", entry),
                block("poll", poll),
                block("invalid", invalid),
            ],
        ));
    }
    let mut code = m::Code::new();
    for item in items {
        match item {
            m::Item::Function(f) if f.blocks.is_none() => code.push(m::Event::Declare(f)),
            m::Item::Function(f) => {
                let blocks = f.blocks.clone().unwrap();
                let mut head = f;
                head.blocks = None;
                code.push(m::Event::Begin(head));
                for b in blocks {
                    code.push(m::Event::Label(b.name));
                    for i in b.instructions {
                        code.push(m::Event::Instruction(i));
                    }
                }
                code.push(m::Event::End);
            }
            _ => unreachable!(),
        }
    }
    Ok(code)
}
impl FunctionEmitter<'_> {
    fn general_task_poll(&mut self, i: &Instruction, task: ValueId) -> CompileResult<()> {
        let output = result(i)?;
        let actual = &self.function.values[output.0 as usize];
        let llvm = llvm_type(actual)?;
        let slot = self.temp();
        let answer = self.temp();
        let tag = self.temp();
        let serial = self.continuation;
        self.continuation += 1;
        let label = |s: &str| format!("task.general.{serial}.{s}");
        let mut body = vec![];
        push(
            &mut body,
            Some(&m::name(&slot)),
            m::Op::Alloca {
                ty: llvm.clone(),
                align: 0,
            },
        );
        store(&mut body, llvm.clone(), m::Val::Zero, m::val(slot.clone()));
        if matches!(i.kind, InstructionKind::TaskPollClose { .. }) {
            call(
                &mut body,
                Some(&m::name(&answer)),
                m::Ty::Int(1),
                m::global("zil.task.poll_close".into()),
                vec![m::Arg::new(m::Ty::Ptr, m::val(value(task)))],
            );
            push(
                &mut body,
                Some(&m::name(&tag)),
                m::Op::Cast {
                    opcode: "zext".into(),
                    arg: m::Arg::new(m::Ty::Int(1), m::val(answer)),
                    to: m::Ty::Int(32),
                },
            );
            gep(&mut body, &label("tag.ptr"), &llvm, m::val(slot.clone()), 0);
            store(
                &mut body,
                m::Ty::Int(32),
                m::val(tag),
                loc(label("tag.ptr")),
            );
            for instruction in body {
                self.event(m::Event::Instruction(instruction));
            }
        } else {
            let Type::ErrorUnion {
                error,
                result: poll,
            } = actual
            else {
                unreachable!()
            };
            let poll_llvm = llvm_type(poll)?;
            let error_llvm = llvm_type(error)?;
            let Type::Address { inner, .. } = &self.function.values[task.0 as usize] else {
                unreachable!()
            };
            let Type::Managed(carrier) = inner.as_ref() else {
                unreachable!()
            };
            let Type::Task {
                result: payload, ..
            } = carrier.as_ref()
            else {
                unreachable!()
            };
            let payload_llvm = aggregate_field_type(payload)?;
            let raw_llvm = m::Ty::Struct(vec![m::Ty::Int(32), payload_llvm.clone()]);
            let poll_symbol = if **payload == Type::I32 {
                "zil.task.poll".into()
            } else {
                typed_poll_name(carrier)
            };
            call(
                &mut body,
                Some(&m::name(&answer)),
                raw_llvm.clone(),
                m::global(poll_symbol),
                vec![m::Arg::new(m::Ty::Ptr, m::val(value(task)))],
            );
            push(
                &mut body,
                Some(&m::name(&tag)),
                m::Op::Extract {
                    arg: m::Arg::new(raw_llvm.clone(), m::val(answer.clone())),
                    index: 0,
                },
            );
            compare(
                &mut body,
                &label("ready.test"),
                m::Ty::Int(32),
                m::val(tag.clone()),
                m::val(1),
                "eq",
            );
            cond(
                &mut body,
                &label("ready.test"),
                &label("ready"),
                &label("not.ready"),
            );
            for instruction in body {
                self.event(m::Event::Instruction(instruction));
            }
            self.event(m::Event::Label(label("ready")));
            let mut body = vec![];
            gep(
                &mut body,
                &label("poll.ptr"),
                &llvm,
                m::val(slot.clone()),
                2,
            );
            gep(
                &mut body,
                &label("poll.tag.ptr"),
                &poll_llvm,
                loc(label("poll.ptr")),
                0,
            );
            store(
                &mut body,
                m::Ty::Int(32),
                m::val(1),
                loc(label("poll.tag.ptr")),
            );
            if **payload != Type::Unit {
                gep(
                    &mut body,
                    &label("payload.ptr"),
                    &poll_llvm,
                    loc(label("poll.ptr")),
                    1,
                );
                // Only Ready may move the actual complete owning payload. Other
                // poll outcomes never materialize a source T.
                push(
                    &mut body,
                    Some(&label("payload")),
                    m::Op::Extract {
                        arg: m::Arg::new(raw_llvm, m::val(answer)),
                        index: 1,
                    },
                );
                store(
                    &mut body,
                    payload_llvm,
                    loc(label("payload")),
                    loc(label("payload.ptr")),
                );
            }
            branch(&mut body, &label("done"));
            for instruction in body {
                self.event(m::Event::Instruction(instruction));
            }
            self.event(m::Event::Label(label("not.ready")));
            let mut body = vec![];
            compare(
                &mut body,
                &label("pending.test"),
                m::Ty::Int(32),
                m::val(tag.clone()),
                m::val(0),
                "eq",
            );
            cond(
                &mut body,
                &label("pending.test"),
                &label("done"),
                &label("error"),
            );
            for instruction in body {
                self.event(m::Event::Instruction(instruction));
            }
            self.event(m::Event::Label(label("error")));
            let mut body = vec![];
            gep(
                &mut body,
                &label("error.flag"),
                &llvm,
                m::val(slot.clone()),
                0,
            );
            store(
                &mut body,
                m::Ty::Int(1),
                m::val(true),
                loc(label("error.flag")),
            );
            gep(
                &mut body,
                &label("error.ptr"),
                &llvm,
                m::val(slot.clone()),
                1,
            );
            gep(
                &mut body,
                &label("error.tag.ptr"),
                &error_llvm,
                loc(label("error.ptr")),
                0,
            );
            push(
                &mut body,
                Some(&label("error.tag")),
                m::Op::Binary {
                    opcode: "sub".into(),
                    ty: m::Ty::Int(32),
                    left: m::val(tag),
                    right: m::val(2),
                },
            );
            store(
                &mut body,
                m::Ty::Int(32),
                loc(label("error.tag")),
                loc(label("error.tag.ptr")),
            );
            branch(&mut body, &label("done"));
            for instruction in body {
                self.event(m::Event::Instruction(instruction));
            }
            self.event(m::Event::Label(label("done")));
        }
        if self.copy_values.contains_key(&output) {
            self.copy_read(output, m::val(slot))?;
        } else {
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name(value(output))),
                m::Op::Load {
                    ty: llvm,
                    ptr: m::val(slot),
                    align: 0,
                },
            )));
        }
        Ok(())
    }
    pub(super) fn task_instruction(&mut self, i: &Instruction) -> CompileResult<()> {
        if let InstructionKind::SuspendCall {
            function,
            arguments,
        } = &i.kind
        {
            let output = result(i)?;
            let args = arguments
                .iter()
                .filter(|id| self.function.values[id.0 as usize] != Type::Unit)
                .map(|id| {
                    Ok(m::Arg::new(
                        llvm_type(&self.function.values[id.0 as usize])?,
                        m::val(value(*id)),
                    ))
                })
                .collect::<CompileResult<Vec<_>>>()?;
            self.event(m::Event::Instruction(inst(
                (self.function.values[output.0 as usize] != Type::Unit)
                    .then(|| m::name(value(output)))
                    .as_deref(),
                m::Op::Call {
                    result: llvm_type(&self.function.values[output.0 as usize])?,
                    result_attrs: vec![],
                    callee: m::global(format!("{AWAIT_MARKER}{}", function.0)),
                    args,
                    vararg: None,
                },
            )));
            return Ok(());
        }
        if matches!(i.kind, InstructionKind::TaskYield) {
            self.event(m::Event::Instruction(inst(
                None,
                m::Op::Call {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    callee: m::global(MARKER.into()),
                    args: vec![],
                    vararg: None,
                },
            )));
            return Ok(());
        }
        if let InstructionKind::TaskValid { task } = i.kind {
            let frame = self.temp();
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name(&frame)),
                m::Op::Load {
                    ty: m::Ty::Ptr,
                    ptr: m::val(value(task)),
                    align: 8,
                },
            )));
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name(value(result(i)?))),
                m::Op::Compare {
                    predicate: "ne".into(),
                    ty: m::Ty::Ptr,
                    left: m::val(frame),
                    right: m::Val::Null,
                },
            )));
            return Ok(());
        }
        if let InstructionKind::TaskCreate { worker, arguments } = &i.kind {
            let output = result(i)?;
            let ty = llvm_type(&self.function.values[output.0 as usize])?;
            let frame = self.temp();
            let failed = self.temp();
            let tagged = self.temp();
            let mut body = vec![];
            let args = arguments
                .iter()
                .filter(|v| self.function.values[v.0 as usize] != Type::Unit)
                .map(|v| {
                    Ok(m::Arg::new(
                        llvm_type(&self.function.values[v.0 as usize])?,
                        m::val(value(*v)),
                    ))
                })
                .collect::<CompileResult<Vec<_>>>()?;
            call(
                &mut body,
                Some(&m::name(&frame)),
                m::Ty::Ptr,
                m::global(name(*worker, "init")),
                args,
            );
            compare(
                &mut body,
                &m::name(&failed),
                m::Ty::Ptr,
                m::val(frame.clone()),
                m::Val::Null,
                "eq",
            );
            push(
                &mut body,
                Some(&m::name(&tagged)),
                m::Op::Insert {
                    arg: m::Arg::new(ty.clone(), m::Val::Zero),
                    index: 0,
                    field: m::Arg::new(m::Ty::Int(1), m::val(failed)),
                },
            );
            // The inactive success field is never exposed on Allocation.
            push(
                &mut body,
                Some(&m::name(value(output))),
                m::Op::Insert {
                    arg: m::Arg::new(ty, m::val(tagged)),
                    index: 2,
                    field: m::Arg::new(m::Ty::Ptr, m::val(frame)),
                },
            );
            for instruction in body {
                self.event(m::Event::Instruction(instruction));
            }
            return Ok(());
        }
        if let InstructionKind::TaskPoll { task } | InstructionKind::TaskPollClose { task } = i.kind
            && matches!(self.function.values[task.0 as usize],Type::Address{ref inner,..} if matches!(inner.as_ref(),Type::Managed(p) if matches!(p.as_ref(),Type::Task{..})))
        {
            return self.general_task_poll(i, task);
        }
        let (callee, argument, ty) = match i.kind {
            InstructionKind::TaskStart { worker, argument } => (
                name(worker, "init"),
                m::Arg::new(m::Ty::Int(32), m::val(value(argument))),
                m::Ty::Ptr,
            ),
            InstructionKind::TaskPoll { task } => (
                "zil.task.poll".into(),
                m::Arg::new(m::Ty::Ptr, m::val(value(task))),
                result_ty(),
            ),
            InstructionKind::TaskRequestCancel { task } => (
                "zil.task.request_cancel".into(),
                m::Arg::new(m::Ty::Ptr, m::val(value(task))),
                m::Ty::Void,
            ),
            InstructionKind::TaskPollClose { task } => (
                "zil.task.poll_close".into(),
                m::Arg::new(m::Ty::Ptr, m::val(value(task))),
                m::Ty::Int(1),
            ),
            InstructionKind::TaskFinishDestroy { task } => (
                "zil.task.finish_destroy".into(),
                m::Arg::new(m::Ty::Ptr, m::val(value(task))),
                m::Ty::Void,
            ),
            _ => unreachable!(),
        };
        let output = if ty == m::Ty::Void {
            None
        } else {
            Some(m::name(value(result(i)?)))
        };
        self.event(m::Event::Instruction(m::Inst::new(
            output,
            m::Op::Call {
                result: ty,
                result_attrs: vec![],
                callee: m::global(callee),
                args: vec![argument],
                vararg: None,
            },
        )));
        Ok(())
    }
}

pub(crate) fn export_name(id: FunctionId, operation: &str) -> String {
    format!("zil.sabi.task.{operation}.{}", id.0)
}

/// The public v1 boundary has only scalar/pointer C-compatible results. The
/// source tuple and internal worker poll stay independent of C aggregate ABI.
pub(super) fn exports(id: FunctionId, meter: &Meter) -> CompileResult<Vec<m::Item>> {
    meter.spend(512)?;
    let mut result = Vec::new();
    for operation in [
        "init",
        "poll",
        "request_cancel",
        "poll_close",
        "finish_destroy",
    ] {
        let mut body = vec![];
        let (parameter, returned) = match operation {
            "init" => (m::Ty::Int(32), m::Ty::Ptr),
            "poll" => (m::Ty::Ptr, m::Ty::Int(64)),
            "poll_close" => (m::Ty::Ptr, m::Ty::Int(1)),
            _ => (m::Ty::Ptr, m::Ty::Void),
        };
        if operation == "init" {
            call(
                &mut body,
                Some("result"),
                returned.clone(),
                m::global(name(id, "init")),
                vec![m::Arg::new(parameter.clone(), loc("input"))],
            );
            ret(&mut body, returned.clone(), loc("result"));
        } else if operation == "finish_destroy" {
            call(
                &mut body,
                None,
                m::Ty::Void,
                m::global("zil.task.finish_destroy".into()),
                vec![m::Arg::new(m::Ty::Ptr, loc("input"))],
            );
            push(&mut body, None, m::Op::Return(None));
        } else {
            push(
                &mut body,
                Some("head"),
                m::Op::Alloca {
                    ty: m::Ty::Ptr,
                    align: 8,
                },
            );
            store(&mut body, m::Ty::Ptr, loc("input"), loc("head"));
            let call_result = if operation == "poll" {
                result_ty()
            } else {
                returned.clone()
            };
            call(
                &mut body,
                (call_result != m::Ty::Void).then_some("result"),
                call_result.clone(),
                m::global(format!("zil.task.{operation}")),
                vec![m::Arg::new(m::Ty::Ptr, loc("head"))],
            );
            if operation == "poll" {
                for (index, label) in [(0, "tag"), (1, "value")] {
                    push(
                        &mut body,
                        Some(label),
                        m::Op::Extract {
                            arg: m::Arg::new(call_result.clone(), loc("result")),
                            index,
                        },
                    );
                    push(
                        &mut body,
                        Some(&format!("{label}.wide")),
                        m::Op::Cast {
                            opcode: "zext".into(),
                            arg: m::Arg::new(m::Ty::Int(32), loc(label)),
                            to: m::Ty::Int(64),
                        },
                    );
                }
                push(
                    &mut body,
                    Some("tag.shifted"),
                    m::Op::Binary {
                        opcode: "shl".into(),
                        ty: m::Ty::Int(64),
                        left: loc("tag.wide"),
                        right: m::val(32),
                    },
                );
                push(
                    &mut body,
                    Some("packed"),
                    m::Op::Binary {
                        opcode: "or".into(),
                        ty: m::Ty::Int(64),
                        left: loc("tag.shifted"),
                        right: loc("value.wide"),
                    },
                );
                ret(&mut body, returned.clone(), loc("packed"));
            } else if returned == m::Ty::Void {
                push(&mut body, None, m::Op::Return(None));
            } else {
                ret(&mut body, returned.clone(), loc("result"));
            }
        }
        result.push(m::Item::Function(m::Function {
            name: export_name(id, operation),
            linkage: m::Linkage::External,
            signature: m::Signature::new(returned, vec![m::Param::new(parameter, "input")]),
            attrs: vec![],
            blocks: Some(vec![block("entry", body)]),
        }));
    }
    Ok(result)
}

#[cfg(test)]
mod child_frame_tests {
    use super::*;

    fn machine(children: usize, target: u32) -> m::Function {
        let mut instructions = vec![];
        for site in 0..children {
            call(
                &mut instructions,
                Some(&format!("child.result.{site}")),
                m::Ty::Int(32),
                m::global(format!("{AWAIT_MARKER}{target}")),
                vec![m::Arg::new(m::Ty::Int(32), loc("arg"))],
            );
        }
        ret(&mut instructions, m::Ty::Int(32), loc("arg"));
        m::Function {
            name: "fixture".into(),
            linkage: m::Linkage::Internal,
            signature: m::Signature::new(
                m::Ty::Int(32),
                vec![m::Param::new(m::Ty::Int(32), "arg")],
            ),
            attrs: vec![],
            blocks: Some(vec![block("entry", instructions)]),
        }
    }
    fn planned(children: usize, balance: usize) -> CompileResult<(usize, FramePlan)> {
        let machines = BTreeMap::from([
            (FunctionId(0), machine(0, 0)),
            (FunctionId(1), machine(children, 0)),
        ]);
        let meter = Meter::new(balance);
        let mut plans = BTreeMap::new();
        plan_frame(
            FunctionId(1),
            &machines,
            &mut plans,
            &mut BTreeSet::new(),
            &meter,
            None,
        )?;
        Ok((
            balance - meter.remaining(),
            plans.remove(&FunctionId(1)).unwrap(),
        ))
    }
    #[test]
    fn child343_actual_frame_planning_exact_budget_and_distinct_instances() {
        let (n, plan) = planned(8, 1_000_000).unwrap();
        assert_eq!(plan.children.len(), 8);
        let positions = plan
            .children
            .iter()
            .flat_map(|c| [c.frame, c.active])
            .collect::<BTreeSet<_>>();
        assert_eq!(positions.len(), 16);
        for child in &plan.children {
            assert!(matches!(
                plan.fields[child.frame as usize],
                m::Ty::Struct(_)
            ));
            assert_eq!(plan.fields[child.active as usize], m::Ty::Int(1));
        }
        assert!(n > planned(1, 1_000_000).unwrap().0 + 7 * 64);
        assert_eq!(planned(8, n).unwrap().0, n);
        assert_eq!(
            planned(8, n - 1).err().unwrap()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(planned(8, n).unwrap().0, n);
        let cycle = BTreeMap::from([(FunctionId(0), machine(1, 0))]);
        assert!(
            plan_frame(
                FunctionId(0),
                &cycle,
                &mut BTreeMap::new(),
                &mut BTreeSet::new(),
                &Meter::new(1_000_000),
                None,
            )
            .is_err()
        );
        eprintln!("child343 actual frame planning N={n}");
    }
}
