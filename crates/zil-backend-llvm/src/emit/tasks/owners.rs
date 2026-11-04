//! Intrusive responsibility records live in the actual stable task frame.
//! Exactly one canonical slot owns each active record; aliases have no node.
use super::*;
use zil_verify::{TaskOwnerAction, TaskOwnerSlot, TaskOwnership};
const MARK: &str = "zil.task.owner.marker.";
fn node_ty() -> m::Ty {
    m::Ty::Struct(vec![m::Ty::Ptr; 4])
}
fn helper(op: &str) -> String {
    format!("zil.task.owner.{op}")
}
pub(super) struct OwnerFrame {
    anchor: u32,
    nodes: Vec<u32>,
    drops: Vec<Option<String>>,
    result: Option<usize>,
}
pub(super) fn plan(
    frame: &mut FramePlan,
    ownership: Option<&TaskOwnership>,
    resources: &[ResourceDefinition],
    limits: &zil_base::Limits,
    meter: &Meter,
) -> CompileResult<()> {
    let Some(ownership) = ownership else {
        return Ok(());
    };
    meter.spend(4096usize.saturating_add(ownership.owners().len().saturating_mul(512)))?;
    let anchor = frame.fields.len() as u32;
    frame.fields.push(node_ty());
    let mut nodes = vec![];
    let mut drops = vec![];
    for owner in ownership.owners() {
        nodes.push(frame.fields.len() as u32);
        frame.fields.push(node_ty());
        drops.push(drop_plan::task_leaf(owner.ty(), resources, limits, meter)?);
    }
    let result = if let Some(ty) = ownership.result_type() {
        let index = nodes.len();
        nodes.push(frame.fields.len() as u32);
        frame.fields.push(node_ty());
        drops.push(drop_plan::task_leaf(ty, resources, limits, meter)?);
        Some(index)
    } else {
        None
    };
    frame.owners = Some(OwnerFrame {
        anchor,
        nodes,
        drops,
        result,
    });
    Ok(())
}
fn field(v: &mut Vec<m::Inst>, prefix: &str, ptr: m::Val, index: u32) -> String {
    let name = format!("{prefix}.{index}");
    gep(v, &name, &node_ty(), ptr, index);
    name
}
fn write(v: &mut Vec<m::Inst>, prefix: &str, ptr: m::Val, index: u32, value: m::Val) {
    let p = field(v, prefix, ptr, index);
    store(v, m::Ty::Ptr, value, loc(p));
}
fn read(v: &mut Vec<m::Inst>, prefix: &str, ptr: m::Val, index: u32) -> String {
    let p = field(v, prefix, ptr, index);
    let value = format!("{prefix}.value");
    load(v, &value, m::Ty::Ptr, loc(p));
    value
}
pub(super) fn setup(v: &mut Vec<m::Inst>, frame: &m::Ty, owners: Option<&OwnerFrame>) {
    let Some(owners) = owners else {
        return;
    };
    gep(v, "task.owners.anchor", frame, loc("frame"), owners.anchor);
    for (index, field) in owners.nodes.iter().enumerate() {
        gep(
            v,
            &format!("task.owner.{index}"),
            frame,
            loc("frame"),
            *field,
        );
    }
}
pub(super) fn initialize(
    v: &mut Vec<m::Inst>,
    frame: &m::Ty,
    slots: &BTreeMap<String, Slot>,
    owners: Option<&OwnerFrame>,
    ownership: Option<&TaskOwnership>,
    function: &TypedFunction,
) -> CompileResult<()> {
    let (Some(owners), Some(ownership)) = (owners, ownership) else {
        return Ok(());
    };
    setup(v, frame, Some(owners));
    write(
        v,
        "anchor.prev",
        loc("task.owners.anchor"),
        0,
        loc("task.owners.anchor"),
    );
    write(
        v,
        "anchor.next",
        loc("task.owners.anchor"),
        1,
        loc("task.owners.anchor"),
    );
    for (index, owner) in ownership.owners().iter().enumerate() {
        let key = match owner.slot() {
            TaskOwnerSlot::Local(local)
            | TaskOwnerSlot::LocalShell(local)
            | TaskOwnerSlot::LocalField { local, .. } => format!("local.{}", local.0),
            TaskOwnerSlot::Value(value) => format!("v.{}", value.0),
        };
        let slot = slots.get(&key).ok_or_else(|| {
            vec![Diagnostic::error(
                Phase::Codegen,
                "E-CODEGEN-TASK",
                None,
                "canonical owned obligation has no actual frame spill",
            )]
        })?;
        let payload = format!("owner.payload.{index}");
        gep(v, &payload, frame, loc("frame"), slot.index);
        let payload = if let TaskOwnerSlot::LocalField { index: field, .. } = owner.slot() {
            let child = format!("owner.field.{index}");
            gep(v, &child, &slot.ty, loc(payload), field);
            child
        } else {
            payload
        };
        write(
            v,
            &format!("owner.drop.{index}"),
            loc(format!("task.owner.{index}")),
            2,
            owners.drops[index]
                .as_ref()
                .map_or(m::Val::Null, |name| m::global(name.clone())),
        );
        write(
            v,
            &format!("owner.storage.{index}"),
            loc(format!("task.owner.{index}")),
            3,
            loc(payload),
        );
    }
    if let Some(index) = owners.result {
        gep(v, "owner.result.payload", frame, loc("frame"), RESULT);
        write(
            v,
            "owner.result.drop",
            loc(format!("task.owner.{index}")),
            2,
            owners.drops[index]
                .as_ref()
                .map_or(m::Val::Null, |name| m::global(name.clone())),
        );
        write(
            v,
            "owner.result.storage",
            loc(format!("task.owner.{index}")),
            3,
            loc("owner.result.payload"),
        );
    }
    for &index in ownership.entry() {
        let owner = ownership.owners()[index].slot();
        let local = match owner {
            TaskOwnerSlot::Local(local)
            | TaskOwnerSlot::LocalShell(local)
            | TaskOwnerSlot::LocalField { local, .. } => local,
            _ => unreachable!(),
        };
        let parameter = function
            .parameters
            .iter()
            .position(|p| *p == local)
            .expect("checked input debt");
        let slot = &slots[&format!("local.{}", local.0)];
        let payload = format!("input.owner.{index}");
        gep(v, &payload, frame, loc("frame"), slot.index);
        if !matches!(owner, TaskOwnerSlot::LocalField { .. }) {
            store(
                v,
                slot.ty.clone(),
                loc(format!("arg.{parameter}")),
                loc(payload),
            );
        }
        let flag_name = match owner {
            TaskOwnerSlot::LocalField { index, .. } => format!("package.live.{}.{index}", local.0),
            _ => format!("resource.live.{}", local.0),
        };
        if let Some(flag) = slots.get(&flag_name) {
            let pointer = format!("input.live.{index}");
            gep(v, &pointer, frame, loc("frame"), flag.index);
            store(v, m::Ty::Int(1), m::val(true), loc(pointer));
        }
        let anchor = ownership.owners()[index].cleanup_anchor();
        call(
            v,
            None,
            m::Ty::Void,
            m::global(helper(if anchor.is_some() {
                "insert_after"
            } else {
                "link"
            })),
            vec![
                m::Arg::new(m::Ty::Ptr, loc(format!("task.owner.{index}"))),
                m::Arg::new(
                    m::Ty::Ptr,
                    anchor.map_or_else(
                        || loc("task.owners.anchor"),
                        |i| loc(format!("task.owner.{i}")),
                    ),
                ),
            ],
        );
    }
    Ok(())
}
pub(in crate::emit) fn marker(action: TaskOwnerAction) -> m::Event {
    let (op, indices) = match action {
        TaskOwnerAction::Link(i) => ("link", vec![i]),
        TaskOwnerAction::LinkField { slot, after } => ("insert_after", vec![slot, after]),
        TaskOwnerAction::Unlink(i) => ("unlink", vec![i]),
    };
    m::Event::Instruction(inst(
        None,
        m::Op::Call {
            result: m::Ty::Void,
            result_attrs: vec![],
            callee: m::global(format!("{MARK}{op}")),
            args: indices
                .into_iter()
                .map(|i| m::Arg::new(m::Ty::Int(32), i))
                .collect(),
            vararg: None,
        },
    ))
}
pub(super) fn is_marker(op: &m::Op) -> bool {
    matches!(op, m::Op::Call { callee: m::Val::Global(name), .. } if name.starts_with(MARK))
}
pub(super) fn lower(op: &mut m::Op) -> CompileResult<()> {
    let m::Op::Call {
        callee: m::Val::Global(name),
        args,
        ..
    } = op
    else {
        return Ok(());
    };
    let Some(operation) = name.strip_prefix(MARK) else {
        return Ok(());
    };
    let operation = operation.to_owned();
    for argument in args.iter_mut() {
        let m::Val::Int(index) = argument.value else {
            return invalid(
                "E-CODEGEN-TASK",
                "canonical owner marker has no checked slot",
                None,
            );
        };
        *argument = m::Arg::new(m::Ty::Ptr, loc(format!("task.owner.{index}")));
    }
    if operation == "link" {
        args.push(m::Arg::new(m::Ty::Ptr, loc("task.owners.anchor")));
    }
    *name = helper(&operation);
    Ok(())
}
pub(super) fn drain(v: &mut Vec<m::Inst>, frame: &m::Ty, owners: Option<&OwnerFrame>) {
    if let Some(owners) = owners {
        gep(v, "close.owners.anchor", frame, loc("frame"), owners.anchor);
        call(
            v,
            None,
            m::Ty::Void,
            m::global(helper("drain")),
            vec![m::Arg::new(m::Ty::Ptr, loc("close.owners.anchor"))],
        );
    }
}
pub(super) fn allocation_failure(
    v: &mut Vec<m::Inst>,
    owners: Option<&OwnerFrame>,
    ownership: Option<&TaskOwnership>,
    function: &TypedFunction,
) -> CompileResult<()> {
    let (Some(owners), Some(ownership)) = (owners, ownership) else {
        return Ok(());
    };
    for &index in ownership.entry().iter().rev() {
        let Some(drop) = &owners.drops[index] else {
            continue;
        };
        let owner = ownership.owners()[index].slot();
        let local = match owner {
            TaskOwnerSlot::Local(local)
            | TaskOwnerSlot::LocalShell(local)
            | TaskOwnerSlot::LocalField { local, .. } => local,
            _ => unreachable!(),
        };
        let parameter = function
            .parameters
            .iter()
            .position(|p| *p == local)
            .expect("checked input obligation");
        let ty = llvm_type(&function.locals[local.0 as usize].ty)?;
        let pointer = format!("failed.input.{index}");
        push(
            v,
            Some(&pointer),
            m::Op::Alloca {
                ty: ty.clone(),
                align: 8,
            },
        );
        store(
            v,
            ty.clone(),
            loc(format!("arg.{parameter}")),
            loc(&pointer),
        );
        let pointer = if let TaskOwnerSlot::LocalField { index: field, .. } = owner {
            let child = format!("failed.input.field.{index}");
            gep(v, &child, &ty, loc(pointer), field);
            child
        } else {
            pointer
        };
        call(
            v,
            None,
            m::Ty::Void,
            m::global(drop.clone()),
            vec![m::Arg::new(m::Ty::Ptr, loc(pointer))],
        );
    }
    Ok(())
}
pub(super) fn helpers(meter: &Meter) -> CompileResult<Vec<m::Item>> {
    meter.spend(8192)?;
    let mut items = vec![];
    let mut v = vec![];
    let next = read(&mut v, "next", loc("node"), 1);
    compare(&mut v, "live", m::Ty::Ptr, loc(&next), m::Val::Null, "ne");
    cond(&mut v, "live", "unlink", "done");
    let mut u = vec![];
    let prev = read(&mut u, "prev", loc("node"), 0);
    write(&mut u, "prev.next", loc(&prev), 1, loc(&next));
    write(&mut u, "next.prev", loc(&next), 0, loc(&prev));
    write(&mut u, "node.prev", loc("node"), 0, m::Val::Null);
    write(&mut u, "node.next", loc("node"), 1, m::Val::Null);
    branch(&mut u, "done");
    items.push(function(
        helper("unlink"),
        m::Ty::Void,
        vec![m::Param::new(m::Ty::Ptr, "node")],
        vec![
            block("entry", v),
            block("unlink", u),
            block("done", vec![inst(None, m::Op::Return(None))]),
        ],
    ));
    let mut v = vec![];
    call(
        &mut v,
        None,
        m::Ty::Void,
        m::global(helper("unlink")),
        vec![m::Arg::new(m::Ty::Ptr, loc("node"))],
    );
    let prev = read(&mut v, "tail", loc("anchor"), 0);
    write(&mut v, "node.prev", loc("node"), 0, loc(&prev));
    write(&mut v, "node.next", loc("node"), 1, loc("anchor"));
    write(&mut v, "tail.next", loc(prev), 1, loc("node"));
    write(&mut v, "anchor.prev", loc("anchor"), 0, loc("node"));
    push(&mut v, None, m::Op::Return(None));
    items.push(function(
        helper("link"),
        m::Ty::Void,
        vec![
            m::Param::new(m::Ty::Ptr, "node"),
            m::Param::new(m::Ty::Ptr, "anchor"),
        ],
        vec![block("entry", v)],
    ));
    let mut v = vec![];
    call(
        &mut v,
        None,
        m::Ty::Void,
        m::global(helper("unlink")),
        vec![m::Arg::new(m::Ty::Ptr, loc("node"))],
    );
    let next = read(&mut v, "successor", loc("after"), 1);
    write(&mut v, "node.prev", loc("node"), 0, loc("after"));
    write(&mut v, "node.next", loc("node"), 1, loc(&next));
    write(&mut v, "after.next", loc("after"), 1, loc("node"));
    write(&mut v, "next.prev", loc(next), 0, loc("node"));
    push(&mut v, None, m::Op::Return(None));
    items.push(function(
        helper("insert_after"),
        m::Ty::Void,
        vec![
            m::Param::new(m::Ty::Ptr, "node"),
            m::Param::new(m::Ty::Ptr, "after"),
        ],
        vec![block("entry", v)],
    ));
    let mut v = vec![];
    let tail = read(&mut v, "tail", loc("anchor"), 0);
    compare(&mut v, "empty", m::Ty::Ptr, loc(&tail), loc("anchor"), "eq");
    cond(&mut v, "empty", "done", "drop");
    let mut d = vec![];
    // Unlink before entering user teardown; repeated close never owns it again.
    call(
        &mut d,
        None,
        m::Ty::Void,
        m::global(helper("unlink")),
        vec![m::Arg::new(m::Ty::Ptr, loc(&tail))],
    );
    let drop = read(&mut d, "drop", loc(&tail), 2);
    let payload = read(&mut d, "payload", loc(tail), 3);
    compare(
        &mut d,
        "has.drop",
        m::Ty::Ptr,
        loc(&drop),
        m::Val::Null,
        "ne",
    );
    cond(&mut d, "has.drop", "invoke", "loop");
    let mut invoke = vec![];
    call(
        &mut invoke,
        None,
        m::Ty::Void,
        loc(drop),
        vec![m::Arg::new(m::Ty::Ptr, loc(payload))],
    );
    branch(&mut invoke, "loop");
    items.push(function(
        helper("drain"),
        m::Ty::Void,
        vec![m::Param::new(m::Ty::Ptr, "anchor")],
        vec![
            block("entry", vec![inst(None, m::Op::Branch("loop".into()))]),
            block("loop", v),
            block("drop", d),
            block("invoke", invoke),
            block("done", vec![inst(None, m::Op::Return(None))]),
        ],
    ));
    Ok(items)
}

pub(super) fn result_debt(v: &mut Vec<m::Inst>, owners: Option<&OwnerFrame>, acquire: bool) {
    if let Some(index) = owners.and_then(|o| o.result) {
        let mut args = vec![m::Arg::new(m::Ty::Ptr, loc(format!("task.owner.{index}")))];
        if acquire {
            args.push(m::Arg::new(m::Ty::Ptr, loc("task.owners.anchor")));
        }
        call(
            v,
            None,
            m::Ty::Void,
            m::global(helper(if acquire { "link" } else { "unlink" })),
            args,
        );
    }
}
