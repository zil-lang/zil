//! Intrusive responsibility records live in the actual stable task frame.
//! Exactly one canonical slot owns each active record; aliases have no node.
use super::*;
use zil_ir::TypedModule;
use zil_verify::{TaskOwnerAction, TaskOwnerSlot, TaskOwnership};
const MARK: &str = "zil.task.owner.marker.";
fn node_ty() -> m::Ty {
    m::Ty::Struct(vec![m::Ty::Ptr; 6])
}
fn helper(op: &str) -> String {
    format!("zil.task.owner.{op}")
}
pub(super) struct OwnerFrame {
    anchor: u32,
    nodes: Vec<u32>,
    drops: Vec<Option<String>>,
    pollable: Vec<Option<(String, String)>>,
    groups: Vec<bool>,
    // Only consuming invocations can temporarily redirect a canonical SSA
    // owner to its actual mutable environment spill. A later Link resets it.
    canonical: BTreeMap<usize, u32>,
    result: Option<usize>,
}
pub(super) fn plan(
    frame: &mut FramePlan,
    ownership: Option<&TaskOwnership>,
    module: &TypedModule,
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
    let mut pollable = vec![];
    let mut groups = vec![];
    for owner in ownership.owners() {
        nodes.push(frame.fields.len() as u32);
        frame.fields.push(node_ty());
        let registration = matches!(owner.ty(),Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..}));
        let child = matches!(owner.ty(),Type::Managed(p) if matches!(p.as_ref(),Type::Task{..}|Type::TaskI32));
        let group = owner_groups::closure(module, owner.ty(), meter)?;
        groups.push(group.is_some());
        pollable.push(if let Some(id) = group {
            Some((owner_groups::symbol(id, "poll"), owner_groups::symbol(id, "request")))
        } else if registration {
            Some((
                "zil.async_registration.drop".into(),
                "zil.async_registration.request_head".into(),
            ))
        } else if child {
            Some(("zil.task.poll_close".into(), "zil.task.request_cancel".into()))
        } else {
            None
        });
        drops.push(if registration {
            None
        } else {
            drop_plan::task_leaf(owner.ty(), &module.resources, limits, meter)?
        });
    }
    let result = if let Some(ty) = ownership.result_type() {
        let index = nodes.len();
        nodes.push(frame.fields.len() as u32);
        frame.fields.push(node_ty());
        let registration = matches!(ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::AsyncRegistration { .. }));
        let child = matches!(ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. } | Type::TaskI32));
        let group = owner_groups::closure(module, ty, meter)?;
        groups.push(group.is_some());
        pollable.push(if let Some(id) = group {
            Some((owner_groups::symbol(id, "poll"), owner_groups::symbol(id, "request")))
        } else if registration {
            Some(("zil.async_registration.drop".into(), "zil.async_registration.request_head".into()))
        } else if child {
            Some(("zil.task.poll_close".into(), "zil.task.request_cancel".into()))
        } else { None });
        drops.push(if registration { None } else {
            drop_plan::task_leaf(ty, &module.resources, limits, meter)?
        });
        Some(index)
    } else {
        None
    };
    let mut canonical = BTreeMap::new();
    for actions in ownership.actions() {
        if let Some(index) = actions.invocation_owner() {
            meter.spend(64)?;
            let Some(owner) = ownership.owners().get(index) else {
                return invalid("E-CODEGEN-TASK", "invocation cleanup owner is absent", None);
            };
            let TaskOwnerSlot::Value(value) = owner.slot() else {
                return invalid("E-CODEGEN-TASK", "invocation cleanup owner is not the actual SSA carrier", None);
            };
            let field = frame.slots.get(&format!("v.{}", value.0)).ok_or_else(|| {
                vec![Diagnostic::error(Phase::Codegen, "E-CODEGEN-TASK", None,
                    "invocation owner has no canonical persistent value slot")]
            })?;
            canonical.insert(index, field.index);
        }
    }
    frame.owners = Some(OwnerFrame {
        anchor,
        nodes,
        drops,
        pollable,
        groups,
        canonical,
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
    for (&index, &field) in &owners.canonical {
        gep(v, &format!("task.owner.canonical.{index}"), frame, loc("frame"), field);
    }
}

pub(super) fn uses_retarget(owners: Option<&OwnerFrame>) -> bool {
    owners.is_some_and(|owners| !owners.canonical.is_empty())
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
            if owners.groups[index] { m::Val::Null } else {
                owners.drops[index].as_ref().map_or(m::Val::Null, |name| m::global(name.clone()))
            },
        );
        for (field, role) in [
            (4, owners.pollable[index].as_ref().map(|p| p.0.as_str())),
            (5, owners.pollable[index].as_ref().map(|p| p.1.as_str())),
        ] {
            write(
                v,
                &format!("owner.async.{index}.{field}"),
                loc(format!("task.owner.{index}")),
                field,
                role.map_or(m::Val::Null, |name| m::global(name.into())),
            );
        }
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
            if owners.groups[index] { m::Val::Null } else {
                owners.drops[index].as_ref().map_or(m::Val::Null, |name| m::global(name.clone()))
            },
        );
        for (field, role) in [(4, owners.pollable[index].as_ref().map(|p|p.0.as_str())), (5, owners.pollable[index].as_ref().map(|p|p.1.as_str()))] {
            write(v, &format!("owner.result.async.{field}"), loc(format!("task.owner.{index}")), field, role.map_or(m::Val::Null, |name|m::global(name.into())));
        }
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
pub(in crate::emit) fn retarget_marker(index: usize, payload: m::Val) -> m::Event {
    m::Event::Instruction(inst(None, m::Op::Call {
        result: m::Ty::Void, result_attrs: vec![],
        callee: m::global(format!("{MARK}retarget")),
        args: vec![m::Arg::new(m::Ty::Int(32), index), m::Arg::new(m::Ty::Ptr, payload)],
        vararg: None,
    }))
}

pub(super) fn is_marker(op: &m::Op) -> bool {
    matches!(op, m::Op::Call { callee: m::Val::Global(name), .. } if name.starts_with(MARK))
}
pub(super) fn lower(
    op: &mut m::Op,
    owners: Option<&OwnerFrame>,
    pre: &mut Vec<m::Inst>,
    meter: &Meter,
) -> CompileResult<()> {
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
    if operation == "retarget" {
        let [owner, _] = args.as_mut_slice() else {
            return invalid("E-CODEGEN-TASK", "invocation retarget has no exact owner and actual spill", None);
        };
        let m::Val::Int(index) = owner.value else {
            return invalid("E-CODEGEN-TASK", "invocation retarget lacks its checked owner slot", None);
        };
        if !owners.is_some_and(|owners| owners.canonical.contains_key(&(index as usize))) {
            return invalid("E-CODEGEN-TASK", "invocation retarget is not bound to a verified cleanup seam", None);
        }
        *owner = m::Arg::new(m::Ty::Ptr, loc(format!("task.owner.{index}")));
        *name = helper("retarget");
        return Ok(());
    }
    if operation == "link"
        && let Some(m::Arg { value: m::Val::Int(index), .. }) = args.first()
        && owners.is_some_and(|owners| owners.canonical.contains_key(&(*index as usize))) {
        // A constructor site can execute again. Never let a fresh owner use
        // the previous generation's mutated once-environment spill.
        meter.spend(128)?;
        call(pre, None, m::Ty::Void, m::global(helper("retarget")), vec![
            m::Arg::new(m::Ty::Ptr, loc(format!("task.owner.{index}"))),
            m::Arg::new(m::Ty::Ptr, loc(format!("task.owner.canonical.{index}"))),
        ]);
    }
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
            Some("close.owners.ready"),
            m::Ty::Int(1),
            m::global(helper("drain")),
            vec![m::Arg::new(m::Ty::Ptr, loc("close.owners.anchor"))],
        );
        cond(v, "close.owners.ready", "owners.closed", "pending");
    }
}
pub(super) fn request(v: &mut Vec<m::Inst>, frame: &m::Ty, owners: Option<&OwnerFrame>) {
    if let Some(owners) = owners {
        gep(
            v,
            "cancel.owners.anchor",
            frame,
            loc("frame"),
            owners.anchor,
        );
        call(
            v,
            None,
            m::Ty::Void,
            m::global(helper("request")),
            vec![m::Arg::new(m::Ty::Ptr, loc("cancel.owners.anchor"))],
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
pub(super) fn helpers(meter: &Meter, retarget: bool) -> CompileResult<Vec<m::Item>> {
    meter.spend(8192)?;
    let mut items = vec![];
    if retarget {
        meter.spend(256)?;
        let mut code = vec![];
        write(&mut code, "owner.storage", loc("node"), 3, loc("payload"));
        code.push(inst(None, m::Op::Return(None)));
        items.push(function(helper("retarget"), m::Ty::Void,
            vec![m::Param::new(m::Ty::Ptr, "node"), m::Param::new(m::Ty::Ptr, "payload")],
            vec![block("entry", code)]));
    }
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
    cond(&mut v, "empty", "done", "check");
    let mut check = vec![];
    let poll = read(&mut check, "poll", loc(&tail), 4);
    let payload = read(&mut check, "payload", loc(&tail), 3);
    compare(
        &mut check,
        "async",
        m::Ty::Ptr,
        loc(&poll),
        m::Val::Null,
        "ne",
    );
    cond(&mut check, "async", "poll", "drop");
    let mut polled = vec![];
    call(
        &mut polled,
        Some("ready"),
        m::Ty::Int(1),
        loc(poll),
        vec![m::Arg::new(m::Ty::Ptr, loc(&payload))],
    );
    cond(&mut polled, "ready", "drop", "pending");
    let mut d = vec![];
    // Pending retains the actual intrusive obligation. Only Ready unlinks.
    call(
        &mut d,
        None,
        m::Ty::Void,
        m::global(helper("unlink")),
        vec![m::Arg::new(m::Ty::Ptr, loc(&tail))],
    );
    let drop = read(&mut d, "drop", loc(&tail), 2);
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
        m::Ty::Int(1),
        vec![m::Param::new(m::Ty::Ptr, "anchor")],
        vec![
            block("entry", vec![inst(None, m::Op::Branch("loop".into()))]),
            block("loop", v),
            block("check", check),
            block("poll", polled),
            block("drop", d),
            block("invoke", invoke),
            block(
                "pending",
                vec![inst(
                    None,
                    m::Op::Return(Some(m::Arg::new(m::Ty::Int(1), false))),
                )],
            ),
            block(
                "done",
                vec![inst(
                    None,
                    m::Op::Return(Some(m::Arg::new(m::Ty::Int(1), true))),
                )],
            ),
        ],
    ));
    // Notify all live owners before any potentially Pending drain. The list
    // itself remains stable across request reentry into Task close.
    let mut start = vec![];
    let first = read(&mut start, "first", loc("anchor"), 1);
    branch(&mut start, "loop");
    let mut test = vec![inst(
        Some("node"),
        m::Op::Phi {
            ty: m::Ty::Ptr,
            incoming: vec![
                (loc(first), "entry".into()),
                (loc("next.value"), "advance".into()),
            ],
        },
    )];
    compare(
        &mut test,
        "empty",
        m::Ty::Ptr,
        loc("node"),
        loc("anchor"),
        "eq",
    );
    cond(&mut test, "empty", "done", "check");
    let mut check = vec![];
    let req = read(&mut check, "request", loc("node"), 5);
    let payload = read(&mut check, "payload", loc("node"), 3);
    compare(
        &mut check,
        "has.request",
        m::Ty::Ptr,
        loc(&req),
        m::Val::Null,
        "ne",
    );
    cond(&mut check, "has.request", "invoke", "advance");
    let mut invoke = vec![];
    call(
        &mut invoke,
        None,
        m::Ty::Void,
        loc(req),
        vec![m::Arg::new(m::Ty::Ptr, loc(payload))],
    );
    branch(&mut invoke, "advance");
    let mut advance = vec![];
    read(&mut advance, "next", loc("node"), 1);
    branch(&mut advance, "loop");
    items.push(function(
        helper("request"),
        m::Ty::Void,
        vec![m::Param::new(m::Ty::Ptr, "anchor")],
        vec![
            block("entry", start),
            block("loop", test),
            block("check", check),
            block("invoke", invoke),
            block("advance", advance),
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
