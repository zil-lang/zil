//! Intrusive responsibility records live in the actual stable task frame.
//! Exactly one canonical slot owns each active record; aliases have no node.
use super::*;
use zil_ir::TypedModule;
use zil_verify::{TaskOwnerAction, TaskOwnerSlot, TaskOwnership};
const MARK: &str = "zil.task.owner.marker.";
fn node_ty() -> m::Ty {
    let mut fields = vec![m::Ty::Ptr; 6];
    fields.extend([m::Ty::Int(32), m::Ty::Int(1)]);
    m::Ty::Struct(fields)
}
fn helper(op: &str) -> String {
    format!("zil.task.owner.{op}")
}
struct PartialFlags {
    descriptor: u32,
    table: u32,
    leaves: Vec<u32>,
}

pub(super) struct OwnerFrame {
    anchor: u32,
    nodes: Vec<u32>,
    drops: Vec<Option<String>>,
    pollable: Vec<Option<(String, String, bool)>>,
    groups: Vec<bool>,
    // The table stores pointers to the ordinary partial-place flags already
    // spilled in this same frame, ordered by the inline cleanup steps.
    partial_flags: BTreeMap<usize, PartialFlags>,
    // Only consuming invocations can temporarily redirect a canonical SSA
    // owner to its actual mutable environment spill. A later Link resets it.
    canonical: BTreeMap<usize, u32>,
    result: Option<usize>,
}
pub(super) fn plan(
    worker:FunctionId,
    frame: &mut FramePlan,
    ownership: Option<&TaskOwnership>,
    module: &TypedModule,
    layouts: &LayoutContext<'_>,
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
    let mut partial_flags = BTreeMap::new();
    let retirement_roles=owner_groups::RetirementRoles::of(ownership,meter)?;
    for (owner_index,owner) in ownership.owners().iter().enumerate() {
        nodes.push(frame.fields.len() as u32);
        frame.fields.push(node_ty());
        let registration = matches!(enums::storage_shape(owner.ty()),Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..}));
        let child = matches!(enums::storage_shape(owner.ty()),Type::Managed(p) if matches!(p.as_ref(),Type::Task{..}|Type::TaskI32));
        let mut group=match retirement_roles.group(owner_index,module,owner.ty(),meter) {
            Ok(group)=>group,Err(diagnostics)=>return describe_failure(diagnostics,worker,Some(owner_index),owner.ty(),ownership,meter)
        };
        if let Some(group @ owner_groups::Group::Inline { .. }) = &mut group
            && let TaskOwnerSlot::Local(local) = owner.slot()
            && frame.slots.contains_key(&super::super::partial_places::flag_name(local, 0))
        {
            let steps = super::inline_drop::partial_flags(layouts, module, owner.ty(), meter)?;
            meter.spend(steps.len().saturating_mul(256).saturating_add(128))?;
            let mut flags = Vec::with_capacity(steps.len());
            for step in steps {
                let key = super::super::partial_places::flag_name(local, step);
                let flag = frame.slots.get(&key).filter(|slot| slot.ty == m::Ty::Int(1))
                    .ok_or_else(|| vec![Diagnostic::error(Phase::Codegen, "E-CODEGEN-TASK", None,
                        "partial inline cleanup has no stable frame flag for its actual leaf")])?;
                flags.push(flag.index);
            }
            let table = frame.fields.len() as u32;
            frame.fields.push(m::Ty::Array(flags.len() as u64, Box::new(m::Ty::Ptr)));
            let descriptor = frame.fields.len() as u32;
            frame.fields.push(super::inline_drop::descriptor_type());
            partial_flags.insert(owner_index, PartialFlags { descriptor, table, leaves: flags });
            group.mark_partial();
        }
        groups.push(group.is_some());
        pollable.push(if let Some(group) = group {
            Some(group.helpers())
        } else if registration {
            Some((
                "zil.async_registration.drop".into(),
                "zil.async_registration.request_head".into(),
                false,
            ))
        } else if child {
            Some(("zil.task.poll_close".into(), "zil.task.request_cancel".into(), false))
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
        let registration = matches!(enums::storage_shape(ty), Type::Managed(payload) if matches!(payload.as_ref(), Type::AsyncRegistration { .. }));
        let child = matches!(enums::storage_shape(ty), Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. } | Type::TaskI32));
        let group=match owner_groups::route(module,ty,meter) {
            Ok(group)=>group,Err(diagnostics)=>return describe_failure(diagnostics,worker,None,ty,ownership,meter)
        };
        groups.push(group.is_some());
        pollable.push(if let Some(group) = group {
            Some(group.helpers())
        } else if registration {
            Some(("zil.async_registration.drop".into(), "zil.async_registration.request_head".into(), false))
        } else if child {
            Some(("zil.task.poll_close".into(), "zil.task.request_cancel".into(), false))
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
        partial_flags,
        canonical,
        result,
    });
    Ok(())
}
/// Diagnostic only: this exports the already-produced immutable public
/// responsibility artifact. Neither a raw boundary id nor a live responsibility
/// is interpreted as Closed, synchronous cleanup, or a source permission.
fn describe_failure<T>(mut diagnostics:Vec<Diagnostic>,worker:FunctionId,owner:Option<usize>,ty:&Type,ownership:&TaskOwnership,meter:&Meter)->CompileResult<T> {
    let copies=diagnostics.len().saturating_add(1);
    let limits=meter.limits()?;let before=meter.remaining();let mut work=before;
    let counted=copy_aggregates::reserve_enum_metadata(ty,&mut work,0,limits);
    meter.spend(before.checked_sub(work).unwrap_or(before).saturating_mul(64).saturating_mul(copies))?;counted?;
    let records=ownership.boundaries().iter().fold(ownership.owners().len(),|n,boundary|n.saturating_add(boundary.may_live().len()).saturating_add(1));
    meter.spend(records.saturating_mul(128).saturating_add(512).saturating_mul(copies))?;
    let roles=ownership.owners().iter().enumerate().map(|(index,owner)|(index,owner.slot(),owner.cleanup_anchor())).collect::<Vec<_>>();
    let boundaries=ownership.boundaries().iter().map(|boundary|(boundary.block(),boundary.instruction(),boundary.may_live())).collect::<Vec<_>>();
    for diagnostic in &mut diagnostics {
        diagnostic.notes.push(format!("actual worker {worker:?}; canonical owner index {owner:?}; complete actual owner/result type {ty:?}"));
        if let Some(index)=owner {diagnostic.notes.push(format!("actual owner record {:?}",ownership.owners()[index]));}
        diagnostic.notes.push(format!("actual canonical roles (index,slot,cleanup_anchor) {roles:?}; actual entry owners {:?}",ownership.entry()));
        diagnostic.notes.push(format!("actual retirement boundary records (block,instruction,raw_may_live_ids) {boundaries:?}"));
    }
    Err(diagnostics)
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
fn cursor(v: &mut Vec<m::Inst>, node: m::Val) -> String {
    field(v, "owner.cursor", node, 6)
}
pub(super) fn poll_inline(v: &mut Vec<m::Inst>, owners: Option<&OwnerFrame>, index: usize, prefix: &str, meter: &Meter) -> CompileResult<()> {
    meter.spend(prefix.len().saturating_add(256))?;
    let Some((poll, _, true)) = owners.and_then(|owners| owners.pollable.get(index)).and_then(Option::as_ref) else {
        return invalid("E-CODEGEN-TASK", "inline drop selects no verified resumable frame owner", None);
    };
    let node = loc(format!("task.owner.{index}"));
    let payload = read(v, &format!("{prefix}.payload"), node.clone(), 3);
    let progress = field(v, &format!("{prefix}.cursor"), node, 6);
    call(v, Some(&format!("{prefix}.ready")), m::Ty::Int(1), m::global(poll.clone()), vec![
        m::Arg::new(m::Ty::Ptr, loc(payload)), m::Arg::new(m::Ty::Ptr, loc(progress)),
    ]);
    Ok(())
}
fn cursor_abi(v: &mut Vec<m::Inst>, node: m::Val) -> String {
    let flag = field(v, "owner.inline", node, 7);
    load(v, "inline", m::Ty::Int(1), loc(flag));
    "inline".into()
}
fn setup_cursor_abi(v: &mut Vec<m::Inst>, prefix: &str, node: m::Val, inline: bool) {
    let flag = field(v, prefix, node, 7);
    store(v, m::Ty::Int(1), m::val(inline), loc(flag));
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
        setup_cursor_abi(v, &format!("owner.inline.{index}"), loc(format!("task.owner.{index}")), owners.pollable[index].as_ref().is_some_and(|route| route.2));
        let payload = if let Some(flags) = owners.partial_flags.get(&index) {
            let directory = format!("owner.flag.table.{index}");
            gep(v, &directory, frame, loc("frame"), flags.table);
            let table_type = m::Ty::Array(flags.leaves.len() as u64, Box::new(m::Ty::Ptr));
            for (ordinal, field) in flags.leaves.iter().enumerate() {
                let flag = format!("owner.flag.{index}.{ordinal}");
                let slot = format!("owner.flag.slot.{index}.{ordinal}");
                gep(v, &flag, frame, loc("frame"), *field);
                gep(v, &slot, &table_type, loc(&directory), ordinal as u32);
                store(v, m::Ty::Ptr, loc(flag), loc(slot));
            }
            let descriptor = format!("owner.partial.{index}");
            gep(v, &descriptor, frame, loc("frame"), flags.descriptor);
            let descriptor_type = super::inline_drop::descriptor_type();
            for (field, pointer) in [(0, payload), (1, directory)] {
                let slot = format!("owner.partial.{index}.{field}");
                gep(v, &slot, &descriptor_type, loc(&descriptor), field);
                store(v, m::Ty::Ptr, loc(pointer), loc(slot));
            }
            descriptor
        } else {
            payload
        };
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
        setup_cursor_abi(v, "owner.result.inline", loc(format!("task.owner.{index}")), owners.pollable[index].as_ref().is_some_and(|route| route.2));
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
            TaskOwnerSlot::LocalField { index, .. } => match enums::storage_shape(&function.locals[local.0 as usize].ty) {
                Type::Array { length, .. } if index < *length => arrays::element_flag(local, index),
                Type::Package { fields, .. } if (index as usize) < fields.len() => {
                    format!("package.live.{}.{index}", local.0)
                }
                _ => return invalid("E-CODEGEN-TASK", "input owner field has no corresponding aggregate element", None),
            },
            _ => format!("resource.live.{}", local.0),
        };
        if let Some(flag) = slots.get(&flag_name) {
            let pointer = format!("input.live.{index}");
            gep(v, &pointer, frame, loc("frame"), flag.index);
            store(v, m::Ty::Int(1), m::val(true), loc(pointer));
        }
        if let Some(flags) = owners.partial_flags.get(&index) {
            for (ordinal, field) in flags.leaves.iter().enumerate() {
                let flag = format!("input.partial.live.{index}.{ordinal}");
                gep(v, &flag, frame, loc("frame"), *field);
                store(v, m::Ty::Int(1), m::val(true), loc(flag));
            }
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
pub(super) fn allocation_failure(layouts: &LayoutContext<'_>,
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
        let ty = layouts.value_type(&function.locals[local.0 as usize].ty)?;
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
    let progress = cursor(&mut v, loc("node"));
    store(&mut v, m::Ty::Int(32), m::val(0), loc(progress));
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
    let progress = cursor(&mut v, loc("node"));
    store(&mut v, m::Ty::Int(32), m::val(0), loc(progress));
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
    let mut poll_route = vec![];
    let inline = cursor_abi(&mut poll_route, loc(&tail));
    cond(&mut poll_route, &inline, "poll.inline", "poll.leaf");
    let mut polled = vec![];
    call(
        &mut polled,
        Some("ready"),
        m::Ty::Int(1),
        loc(&poll),
        vec![m::Arg::new(m::Ty::Ptr, loc(&payload))],
    );
    cond(&mut polled, "ready", "drop", "pending");
    let mut polled_inline = vec![];
    let progress = cursor(&mut polled_inline, loc(&tail));
    call(&mut polled_inline, Some("inline.ready"), m::Ty::Int(1), loc(poll), vec![
        m::Arg::new(m::Ty::Ptr, loc(&payload)),
        m::Arg::new(m::Ty::Ptr, loc(progress)),
    ]);
    cond(&mut polled_inline, "inline.ready", "drop", "pending");
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
            block("poll", poll_route),
            block("poll.leaf", polled),
            block("poll.inline", polled_inline),
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
    let mut request_route = vec![];
    let inline = cursor_abi(&mut request_route, loc("node"));
    cond(&mut request_route, &inline, "invoke.inline", "invoke.leaf");
    let mut invoke = vec![];
    call(
        &mut invoke,
        None,
        m::Ty::Void,
        loc(&req),
        vec![m::Arg::new(m::Ty::Ptr, loc(&payload))],
    );
    branch(&mut invoke, "advance");
    let mut inline_request = vec![];
    let progress = cursor(&mut inline_request, loc("node"));
    call(&mut inline_request, None, m::Ty::Void, loc(req), vec![
        m::Arg::new(m::Ty::Ptr, loc(payload)),
        m::Arg::new(m::Ty::Ptr, loc(progress)),
    ]);
    branch(&mut inline_request, "advance");
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
            block("invoke", request_route),
            block("invoke.leaf", invoke),
            block("invoke.inline", inline_request),
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
