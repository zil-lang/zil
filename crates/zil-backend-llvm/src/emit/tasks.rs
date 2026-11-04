//! Persistent scalar continuation frames. Source TaskYield becomes a saved pc;
//! all actual machine SSA and local storage survive return from poll.
use super::*;
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
fn result_ty() -> m::Ty {
    m::Ty::Struct(vec![m::Ty::Int(32), m::Ty::Int(32)])
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
    fn spend(&self, amount: usize) -> CompileResult<()> {
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
    m::Item::Function(m::Function {
        name,
        linkage: m::Linkage::Internal,
        signature: m::Signature::new(result, params),
        attrs: vec![],
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
    push(
        v,
        Some(&format!("{prefix}.tag")),
        m::Op::Insert {
            arg: m::Arg::new(result_ty(), m::Val::Zero),
            field: m::Arg::new(m::Ty::Int(32), status),
            index: 0,
        },
    );
    push(
        v,
        Some(&format!("{prefix}.value")),
        m::Op::Insert {
            arg: m::Arg::new(result_ty(), loc(format!("{prefix}.tag"))),
            field: m::Arg::new(m::Ty::Int(32), value),
            index: 1,
        },
    );
    ret(v, result_ty(), loc(format!("{prefix}.value")));
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
pub(super) fn lower_worker(
    id: FunctionId,
    machine: &m::Function,
    meter: &Meter,
) -> CompileResult<Vec<m::Item>> {
    meter.spend(2048usize.saturating_add(machine.signature.params.len().saturating_mul(16)))?;
    let mut fields = header();
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
    let frame_name = format!("zil_task_frame_{}", id.0);
    let frame_fields = fields;
    let frame = m::Ty::Named(frame_name.clone());
    let mut pieces: Vec<m::Block> = vec![];
    for b in original {
        let mut current = block(&b.name, vec![]);
        for i in &b.instructions {
            if matches!(&i.op, m::Op::Call { callee: m::Val::Global(n), .. } if n == MARKER) {
                let next = format!("task.resume.{}", pieces.len());
                // The resume pc is its exact position in the final block table.
                let pc = pieces.len() + 1;
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
                poll_result(
                    &mut current.instructions,
                    0,
                    m::val(0),
                    &format!("pending.{pc}"),
                );
                pieces.push(current);
                current = block(&next, vec![]);
            } else {
                current.instructions.push(i.clone());
            }
        }
        pieces.push(current);
    }
    // pc=0 starts the original entry. Every yield stores the next piece index.
    let mut setup = vec![];
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
    for (index, piece) in pieces.iter().enumerate() {
        let mut v = vec![];
        compare(
            &mut v,
            &format!("task.pc.is.{index}"),
            m::Ty::Int(32),
            loc("task.saved.pc"),
            m::val(index),
            "eq",
        );
        let no = if index + 1 == pieces.len() {
            "task.trap".into()
        } else {
            format!("task.select.{}", index + 1)
        };
        cond(&mut v, &format!("task.pc.is.{index}"), &piece.name, &no);
        poll.push(block(&format!("task.select.{index}"), v));
    }
    let mut serial = 0;
    for mut piece in pieces {
        let mut instructions = vec![];
        for mut i in piece.instructions {
            if matches!(i.op, m::Op::Alloca { .. }) {
                continue;
            }
            rewrite(&mut i.op, &slots, &mut instructions, &mut serial)?;
            if let m::Op::Return(Some(arg)) = &i.op {
                // Generated Pending returns already have the SABI result type.
                if arg.ty == m::Ty::Int(32) {
                    store(
                        &mut instructions,
                        m::Ty::Int(32),
                        arg.value.clone(),
                        loc("task.result"),
                    );
                    store(
                        &mut instructions,
                        m::Ty::Int(32),
                        m::val(1),
                        loc("task.state"),
                    );
                    load(
                        &mut instructions,
                        &format!("task.ready.{serial}"),
                        m::Ty::Int(32),
                        loc("task.result"),
                    );
                    let ready = loc(format!("task.ready.{serial}"));
                    serial += 1;
                    poll_result(&mut instructions, 1, ready, &format!("ready.{serial}"));
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
        poll_result(&mut v, status, m::val(0), label);
        poll.push(block(label, v));
    }
    poll.push(trap_block());
    let mut items = vec![
        m::Item::NamedType(frame_name, frame_fields),
        function(
            name(id, "poll"),
            result_ty(),
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
    cond(&mut init, "allocated", "init", "return");
    let mut initialize = vec![];
    for (index, operation) in ["poll", "request_cancel", "poll_close", "finish_destroy"]
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
    branch(&mut initialize, "return");
    let mut returned = vec![];
    ret(&mut returned, m::Ty::Ptr, loc("frame"));
    items.push(function(
        name(id, "init"),
        m::Ty::Ptr,
        machine.signature.params.clone(),
        vec![
            block("entry", init),
            block("init", initialize),
            block("return", returned),
        ],
    ));
    let mut cancel = vec![];
    gep(&mut cancel, "cancel", &frame, loc("frame"), CANCEL);
    store(&mut cancel, m::Ty::Int(1), m::val(true), loc("cancel"));
    push(&mut cancel, None, m::Op::Return(None));
    items.push(function(
        name(id, "request_cancel"),
        m::Ty::Void,
        vec![m::Param::new(m::Ty::Ptr, "frame")],
        vec![block("entry", cancel)],
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
    cond(&mut close, "active", "pending", "close");
    let mut closed = vec![];
    store(&mut closed, m::Ty::Int(32), m::val(3), loc("state"));
    ret(&mut closed, m::Ty::Int(1), m::val(true));
    let mut pending = vec![];
    ret(&mut pending, m::Ty::Int(1), m::val(false));
    items.push(function(
        name(id, "poll_close"),
        m::Ty::Int(1),
        vec![m::Param::new(m::Ty::Ptr, "frame")],
        vec![
            block("entry", close),
            block("close", closed),
            block("pending", pending),
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

pub(super) fn runtime(lir: &LirView<'_>, enabled: bool, meter: &Meter) -> CompileResult<m::Code> {
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
            foreign.symbol.starts_with("zil.task.") || foreign.symbol.starts_with("zil_task_frame_")
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
    pub(super) fn task_instruction(&mut self, i: &Instruction) -> CompileResult<()> {
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
