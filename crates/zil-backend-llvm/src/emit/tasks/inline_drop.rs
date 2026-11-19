//! Resume the structural DropPlan of an actual frame-owned inline value.
//! The frame node supplies the next step; the value supplies its own tags.
use super::*;
use drop_plan::DropPlan;
use zil_ir::{PlaceProjection, TypedModule, VariantProjection};

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod tests;

#[derive(Clone, Copy)]
enum Edge<'a> {
    Field { layout: &'a m::Ty, machine: u32, declared: u32 },
    Element(&'a m::Ty, u32),
    Live(&'a m::Ty, u32),
    Alternative { layout: &'a m::Ty, width: u32, tag: u32, payload: u32 },
}

#[derive(Clone, Copy)]
enum Action<'a> {
    Call(&'a str),
    Task,
    Registration,
    Environment(ClosureId, bool),
    ClearFlag,
}

struct Step<'a> {
    path: Vec<Edge<'a>>,
    action: Action<'a>,
}

fn steps<'a>(plan: &'a DropPlan, module: &TypedModule, path: &mut Vec<Edge<'a>>, result: &mut Vec<Step<'a>>, meter: &Meter) -> CompileResult<()> {
    meter.spend(1)?;
    let mut child = |edge, child| {
        meter.spend(2)?;
        path.push(edge);
        let result = steps(child, module, path, result, meter);
        path.pop();
        result
    };
    let action = match plan {
        DropPlan::Empty => return Ok(()),
        DropPlan::Product(layout, fields) => {
            for (declared, (machine, field)) in fields.iter().enumerate() {
                child(Edge::Field { layout, machine: *machine, declared: declared as u32 }, field)?;
            }
            return Ok(());
        }
        DropPlan::Array(layout, length, item) => {
            if matches!(item.as_ref(), DropPlan::Empty) { return Ok(()); }
            for index in 0..*length {
                child(Edge::Element(layout, index), item)?;
            }
            return Ok(());
        }
        DropPlan::Enum(layout, variants) => {
            for (tag, alternative) in variants.iter().enumerate() {
                child(Edge::Alternative { layout, width: 32, tag: tag as u32, payload: 1 }, alternative)?;
            }
            return Ok(());
        }
        DropPlan::Error(layout, error, value) => {
            child(Edge::Alternative { layout, width: 1, tag: 1, payload: 1 }, error)?;
            child(Edge::Alternative { layout, width: 1, tag: 0, payload: 2 }, value)?;
            return Ok(());
        }
        DropPlan::Mutex(..) => return unsupported("resumable inline cleanup cannot enter a heap-owned mutex payload", None),
        DropPlan::Leaf(name) => Action::Call(name),
        DropPlan::Task => Action::Task,
        DropPlan::AsyncRegistration => Action::Registration,
        DropPlan::Closure(id) => Action::Environment(*id, owner_groups::closure(module, *id, meter)?.is_some()),
    };
    // Account for the copied occurrence path and the two emitted consumers.
    meter.spend(256usize.saturating_add(path.len().saturating_mul(128)))?;
    result.push(Step { path: path.clone(), action });
    Ok(())
}

fn project(layouts: &LayoutContext<'_>, blocks: &mut Vec<m::Block>, prefix: &str, path: &[Edge<'_>], skip: &str, mut body: Vec<m::Inst>) -> CompileResult<(String, Vec<m::Inst>, String)> {
    let mut label = prefix.to_owned();
    let mut base = "value".to_owned();
    for (ordinal, edge) in path.iter().enumerate() {
        let pointer = format!("{prefix}.path.{ordinal}");
        let (layout, index, element) = match *edge {
            Edge::Field { layout, machine, .. } => (layout, machine, false),
            Edge::Element(layout, index) => (layout, index, true),
            Edge::Live(layout, index) => {
                gep(&mut body, &pointer, &layouts.clone_machine(layout)?, loc(&base), index);
                let flag = format!("{prefix}.live.{ordinal}");
                load(&mut body, &flag, m::Ty::Int(1), loc(pointer));
                let next = format!("{prefix}.live.field.{ordinal}");
                cond(&mut body, &flag, &next, skip);
                blocks.push(block(&label, body));
                label = next;
                body = vec![];
                continue;
            }
            Edge::Alternative { layout, width, tag, payload } => {
                let tag_pointer = format!("{prefix}.tag.ptr.{ordinal}");
                gep(&mut body, &tag_pointer, &layouts.clone_machine(layout)?, loc(&base), 0);
                let code = format!("{prefix}.tag.{ordinal}");
                load(&mut body, &code, m::Ty::Int(width), loc(tag_pointer));
                let selected = format!("{prefix}.tag.selected.{ordinal}");
                compare(&mut body, &selected, m::Ty::Int(width), loc(code), m::val(tag), "eq");
                let next = format!("{prefix}.selected.{ordinal}");
                cond(&mut body, &selected, &next, skip);
                blocks.push(block(&label, body));
                label = next;
                body = vec![];
                (layout, payload, false)
            }
        };
        let mut indices = vec![m::Arg::new(m::Ty::Int(32), 0)];
        indices.push(m::Arg::new(m::Ty::Int(if element { 64 } else { 32 }), index));
        push(&mut body, Some(&pointer), m::Op::Gep {
            ty: layouts.clone_machine(layout)?, ptr: loc(base), indices,
        });
        base = pointer;
    }
    Ok((label, body, base))
}

fn invoke(body: &mut Vec<m::Inst>, answer: Option<&str>, result: m::Ty, function: &str, value: &str) {
    call(body, answer, result, m::global(function.to_owned()), vec![m::Arg::new(m::Ty::Ptr, loc(value))]);
}

fn dispatcher(blocks: &mut Vec<m::Block>, start: usize, end: usize) -> String {
    if start == end {
        return format!("step.{start}");
    }
    let middle = start + (end - start) / 2;
    let left = dispatcher(blocks, start, middle);
    let right = dispatcher(blocks, middle + 1, end);
    let name = format!("dispatch.{start}.{end}");
    let test = format!("{name}.lower");
    let mut body = vec![];
    compare(&mut body, &test, m::Ty::Int(32), loc("resume"), m::val(middle), "ule");
    cond(&mut body, &test, &left, &right);
    blocks.push(block(&name, body));
    name
}

pub(super) fn functions(layouts: &LayoutContext<'_>, module: &TypedModule, ty: &Type, name: &str, partial: bool, closure_helpers: &mut Vec<ClosureId>, meter: &Meter) -> CompileResult<Vec<m::Item>> {
    let plan = drop_plan::inline_plan(layouts, ty, &module.resources, meter)?;
    let mut entries = vec![];
    steps(&plan, module, &mut vec![], &mut entries, meter)?;
    emit(layouts, name, &entries, partial, closure_helpers)
}

// Use the ordinary local's existing leaf flags in exact drop-step order.
// An inactive variant still needs its own path: a different variant may
// reuse the same physical payload address and field ordinal.
pub(super) fn partial_flags(
    layouts: &LayoutContext<'_>,
    module: &TypedModule,
    ty: &Type,
    local: LocalId,
    slots: &BTreeMap<String, Slot>,
    meter: &Meter,
) -> CompileResult<Option<Vec<String>>> {
    use super::super::{arrays, packages, partial_places};
    // The most precise existing flag belongs to this actual local. A
    // transferred whole field or array element guards its entire subtree.
    // These guards are read-only during retirement. The cursor advances past
    // each completed leaf and remains on a child that returned Pending.
    enum Guards { Leaves, Fields, Elements }
    meter.spend(slots.len().checked_ilog2().map_or(1, |n| n as usize + 2).saturating_mul(3))?;
    let package_prefix = format!("package.live.{}.", local.0);
    let guards = if slots.contains_key(&partial_places::flag_name(local, 0)) {
        Guards::Leaves
    } else if matches!(enums::storage_shape(ty), Type::Package { .. })
        && slots.range(package_prefix.clone()..).next().is_some_and(|(key, _)| key.starts_with(&package_prefix)) {
        Guards::Fields
    } else if slots.contains_key(&arrays::element_flag(local, 0)) {
        Guards::Elements
    } else { return Ok(None); };
    let limits = meter.limits()?;
    let mut budget = drop_plan::Budget::new(limits);
    budget.tree(ty, 0)?;
    for resource in &module.resources {
        budget.tree(&resource.payload, 0)?;
    }
    meter.spend(budget.spent())?;
    let leaves = match guards {
        Guards::Leaves => Some(partial_places::leaves(ty, &module.resources)?),
        Guards::Fields | Guards::Elements => None,
    };
    let plan = drop_plan::inline_plan(layouts, ty, &module.resources, meter)?;
    let mut entries = Vec::new();
    steps(&plan, module, &mut Vec::new(), &mut entries, meter)?;
    let mut flags = Vec::with_capacity(entries.len());
    for step in entries {
        meter.spend(step.path.len().saturating_add(1))?;
        match (&guards, step.path.first()) {
            (Guards::Elements, Some(Edge::Element(_, index))) => {
                flags.push(arrays::element_flag(local, *index));
                continue;
            }
            (Guards::Fields, Some(Edge::Field { declared, .. })) => {
                flags.push(packages::field_flag(local, *declared));
                continue;
            }
            (Guards::Leaves, _) => {}
            _ => return invalid("E-CODEGEN-TASK", "partial owner step has no actual guarded component", None),
        }
        let mut path = Vec::new();
        let mut variant = None;
        for edge in step.path {
            match edge {
                Edge::Field { declared: index, .. } => path.push(match variant.take() {
                    Some(variant) => PlaceProjection::Variant(VariantProjection { variant, index }),
                    None => PlaceProjection::Field(index),
                }),
                Edge::Alternative { width: 32, tag, .. } if variant.is_none() => variant = Some(tag),
                Edge::Alternative { width: 1, tag, .. } if variant.is_none() => path.push(PlaceProjection::ErrorArm(tag == 1)),
                _ => return invalid("E-CODEGEN-TASK", "inline cleanup step has no canonical partial local field path", None),
            }
        }
        if variant.is_some() {
            return invalid("E-CODEGEN-TASK", "inline cleanup step ends at an alternative without its field", None);
        }
        let mut matched = None;
        for (index, leaf) in leaves.iter().flatten().enumerate() {
            meter.spend(leaf.path.len().saturating_add(path.len()).saturating_add(1))?;
            if leaf.path == path {
                matched = Some(index);
                break;
            }
        }
        flags.push(partial_places::flag_name(local, matched.ok_or_else(|| vec![Diagnostic::error(
            Phase::Codegen, "E-CODEGEN-TASK", None,
            "inline cleanup step has no corresponding live leaf flag",
        )])?));
    }
    Ok(Some(flags))
}

pub(super) fn capture_functions(
    layouts: &LayoutContext<'_>, module: &TypedModule, ty: &Type, id: ClosureId,
    capture: u32, name: &str, closure_helpers: &mut Vec<ClosureId>, meter: &Meter,
) -> CompileResult<Vec<m::Item>> {
    let plan = drop_plan::inline_plan(layouts, ty, &module.resources, meter)?;
    let environment = environment_type(id);
    let mut entries = vec![];
    if let Some(Type::Package { fields, .. }) = packages::physical_package(ty)
        && let DropPlan::Product(layout, children) = &plan
    {
        let mut flag = layouts.flag_start(id, capture)? as u32;
        for ((index, child), (_, field)) in children.iter().zip(fields) {
            if !packages::owned_field(field) { continue; }
            flag += 1;
            let mut path = vec![Edge::Live(&environment, flag), Edge::Field { layout: &environment, machine: capture, declared: capture }, Edge::Field { layout, machine: *index, declared: *index }];
            steps(child, module, &mut path, &mut entries, meter)?;
            meter.spend(256)?;
            entries.push(Step { path: vec![Edge::Field { layout: &environment, machine: flag, declared: flag }], action: Action::ClearFlag });
        }
    } else {
        steps(&plan, module, &mut vec![Edge::Field { layout: &environment, machine: capture, declared: capture }], &mut entries, meter)?;
    }
    emit(layouts, name, &entries, false, closure_helpers)
}

pub(super) fn descriptor_type() -> m::Ty {
    m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Ptr])
}

fn load_descriptor(body: &mut Vec<m::Inst>) {
    for (field, name) in [(0, "value"), (1, "flags")] {
        let pointer = format!("descriptor.{name}");
        gep(body, &pointer, &descriptor_type(), loc("descriptor"), field);
        load(body, name, m::Ty::Ptr, loc(pointer));
    }
}

fn checked_flag(
    blocks: &mut Vec<m::Block>,
    prefix: &str,
    index: usize,
    skip: &str,
) -> String {
    let ready = format!("{prefix}.flags.ready");
    let pointer = format!("{prefix}.flag.pointer");
    let flag = format!("{prefix}.flag");
    let live = format!("{prefix}.flag.live");
    let mut instructions = Vec::new();
    push(&mut instructions, Some(&pointer), m::Op::Gep {
        ty: m::Ty::Ptr,
        ptr: loc("flags"),
        indices: vec![m::Arg::new(m::Ty::Int(64), index as u64)],
    });
    load(&mut instructions, &flag, m::Ty::Ptr, loc(pointer));
    load(&mut instructions, &live, m::Ty::Int(1), loc(flag));
    cond(&mut instructions, &live, &ready, skip);
    blocks.push(block(prefix, instructions));
    ready
}

fn emit(layouts: &LayoutContext<'_>, name: &str, entries: &[Step<'_>], partial: bool, closure_helpers: &mut Vec<ClosureId>) -> CompileResult<Vec<m::Item>> {
    let mut poll = vec![];
    let route = dispatcher(&mut poll, 0, entries.len());
    let mut entry = vec![];
    if partial { load_descriptor(&mut entry); }
    load(&mut entry, "resume", m::Ty::Int(32), loc("cursor"));
    branch(&mut entry, &route);
    poll.insert(0, block("entry", entry));

    let mut request = vec![];
    let mut previous = vec![];
    if partial { load_descriptor(&mut previous); }
    load(&mut previous, "resume", m::Ty::Int(32), loc("cursor"));
    let mut previous_name = "entry".to_owned();

    for (index, step) in entries.iter().enumerate() {
        let step_name = format!("step.{index}");
        let advance = format!("advance.{index}");
        let ready = if partial { checked_flag(&mut poll, &step_name, index, &advance) } else { step_name };
        let (mut label, mut body, payload) = project(layouts, &mut poll, &ready, &step.path, &advance, vec![])?;
        let (request_call, poll_call) = match step.action {
            Action::ClearFlag => {
                store(&mut body, m::Ty::Int(1), m::Val::Bool(false), loc(&payload));
                (None, None)
            }
            Action::Call(name) => {
                invoke(&mut body, None, m::Ty::Void, name, &payload);
                (None, None)
            }
            Action::Environment(id, false) => {
                invoke(&mut body, None, m::Ty::Void, &format!("zil.closure.drop.{}", id.0), &payload);
                (None, None)
            }
            Action::Environment(id, true) => {
                closure_helpers.push(id);
                (Some(owner_groups::symbol(id, "request")), Some(owner_groups::symbol(id, "poll")))
            }
            Action::Registration => (Some("zil.async_registration.request_head".into()), Some("zil.async_registration.drop".into())),
            Action::Task => {
                let frame = format!("task.frame.{index}");
                load(&mut body, &frame, m::Ty::Ptr, loc(&payload));
                let empty = format!("task.empty.{index}");
                let active = format!("task.active.{index}");
                compare(&mut body, &empty, m::Ty::Ptr, loc(frame), m::Val::Null, "eq");
                cond(&mut body, &empty, &advance, &active);
                poll.push(block(&label, body));
                label = active;
                body = vec![];
                (Some("zil.task.request_cancel".into()), Some("zil.task.poll_close".into()))
            }
        };
        if let Some(poll_call) = poll_call {
            if let Some(request_call) = &request_call {
                invoke(&mut body, None, m::Ty::Void, request_call, &payload);
            }
            let ready = format!("ready.{index}");
            invoke(&mut body, Some(&ready), m::Ty::Int(1), &poll_call, &payload);
            let ready_label = if matches!(step.action, Action::Task) { format!("task.finish.{index}") } else { advance.clone() };
            cond(&mut body, &ready, &ready_label, "pending");
            poll.push(block(&label, body));
            if matches!(step.action, Action::Task) {
                let mut finish = vec![];
                invoke(&mut finish, None, m::Ty::Void, "zil.task.finish_destroy", &format!("task.frame.{index}"));
                store(&mut finish, m::Ty::Ptr, m::Val::Null, loc(&payload));
                branch(&mut finish, &advance);
                poll.push(block(&ready_label, finish));
            }
        } else {
            branch(&mut body, &advance);
            poll.push(block(&label, body));
        }
        let mut body = vec![];
        store(&mut body, m::Ty::Int(32), m::val(index + 1), loc("cursor"));
        branch(&mut body, &format!("step.{}", index + 1));
        poll.push(block(&advance, body));

        if let Some(request_call) = request_call {
            let next = format!("request.next.{index}");
            let check = format!("request.leaf.{index}");
            let live = format!("request.live.{index}");
            compare(&mut previous, &live, m::Ty::Int(32), loc("resume"), m::val(index), "ule");
            cond(&mut previous, &live, &check, &next);
            request.push(block(&previous_name, previous));
            let ready = if partial { checked_flag(&mut request, &check, index, &next) } else { check };
            let (name, mut visit, payload) = project(layouts, &mut request, &ready, &step.path, &next, vec![])?;
            invoke(&mut visit, None, m::Ty::Void, &request_call, &payload);
            branch(&mut visit, &next);
            request.push(block(&name, visit));
            previous = vec![];
            previous_name = next;
        }
    }
    poll.push(block(&format!("step.{}", entries.len()), vec![inst(None, m::Op::Return(Some(m::Arg::new(m::Ty::Int(1), true))))]));
    poll.push(block("pending", vec![inst(None, m::Op::Return(Some(m::Arg::new(m::Ty::Int(1), false))))]));
    push(&mut previous, None, m::Op::Return(None));
    request.push(block(&previous_name, previous));
    let params = || vec![m::Param::new(m::Ty::Ptr, if partial { "descriptor" } else { "value" }), m::Param::new(m::Ty::Ptr, "cursor")];
    Ok(vec![
        function(format!("{name}.poll"), m::Ty::Int(1), params(), poll),
        function(format!("{name}.request"), m::Ty::Void, params(), request),
    ])
}
