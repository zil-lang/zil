//! Resume the structural DropPlan of an actual frame-owned inline value.
//! The frame node supplies the next step; the value supplies its own tags.
use super::*;
use drop_plan::DropPlan;
use zil_ir::TypedModule;

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod tests;

#[derive(Clone, Copy)]
enum Edge<'a> {
    Field(&'a m::Ty, u32),
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
            for (index, field) in fields {
                child(Edge::Field(layout, *index), field)?;
            }
            return Ok(());
        }
        DropPlan::Array(layout, length, item) => {
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
        DropPlan::Closure(id) => Action::Environment(*id, owner_groups::closure(module, &Type::Closure(*id), meter)?.is_some()),
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
            Edge::Field(layout, index) => (layout, index, false),
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

pub(super) fn functions(layouts: &LayoutContext<'_>, module: &TypedModule, ty: &Type, name: &str, closure_helpers: &mut Vec<ClosureId>, meter: &Meter) -> CompileResult<Vec<m::Item>> {
    let plan = drop_plan::inline_plan(layouts, ty, &module.resources, meter)?;
    let mut entries = vec![];
    steps(&plan, module, &mut vec![], &mut entries, meter)?;
    emit(layouts, name, &entries, closure_helpers)
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
            let mut path = vec![Edge::Live(&environment, flag), Edge::Field(&environment, capture), Edge::Field(layout, *index)];
            steps(child, module, &mut path, &mut entries, meter)?;
            meter.spend(256)?;
            entries.push(Step { path: vec![Edge::Field(&environment, flag)], action: Action::ClearFlag });
        }
    } else {
        steps(&plan, module, &mut vec![Edge::Field(&environment, capture)], &mut entries, meter)?;
    }
    emit(layouts, name, &entries, closure_helpers)
}

fn emit(layouts: &LayoutContext<'_>, name: &str, entries: &[Step<'_>], closure_helpers: &mut Vec<ClosureId>) -> CompileResult<Vec<m::Item>> {
    let mut poll = vec![];
    let route = dispatcher(&mut poll, 0, entries.len());
    let mut entry = vec![];
    load(&mut entry, "resume", m::Ty::Int(32), loc("cursor"));
    branch(&mut entry, &route);
    poll.insert(0, block("entry", entry));

    let mut request = vec![];
    let mut previous = vec![];
    load(&mut previous, "resume", m::Ty::Int(32), loc("cursor"));
    let mut previous_name = "entry".to_owned();

    for (index, step) in entries.iter().enumerate() {
        let step_name = format!("step.{index}");
        let advance = format!("advance.{index}");
        let (mut label, mut body, payload) = project(layouts, &mut poll, &step_name, &step.path, &advance, vec![])?;
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
            let (name, mut visit, payload) = project(layouts, &mut request, &check, &step.path, &next, vec![])?;
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
    let params = || vec![m::Param::new(m::Ty::Ptr, "value"), m::Param::new(m::Ty::Ptr, "cursor")];
    Ok(vec![
        function(format!("{name}.poll"), m::Ty::Int(1), params(), poll),
        function(format!("{name}.request"), m::Ty::Void, params(), request),
    ])
}
