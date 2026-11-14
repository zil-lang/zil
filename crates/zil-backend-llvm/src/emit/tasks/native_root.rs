//! A native execution root for a fixed SABI body proved never to suspend.
//! Use the actual worker frame and its existing protocol. No public calling
//! convention is narrowed, no extra heap allocation is inserted, and no
//! Pending result is silently driven or discarded.
use super::*;

pub(in super::super) fn adapter(
    source: &TypedFunction,
    abi: Option<&large_copy_abi::Signature>,
    verified: &VerifiedModule,
    meter: &Meter,
) -> CompileResult<(String, m::Code)> {
    let id = source.id;
    if !verified.has_non_suspending_sabi_body(id)
        || !verified.task_worker_ids().contains(&id)
        || source.foreign.is_some()
        || source.environment.is_some()
        || !source.parameters.is_empty()
    {
        return invalid(
            "E-TASK-NATIVE-ROOT",
            "native SABI root lacks its complete checked worker body",
            Some(source.span),
        );
    }
    meter.spend(2048)?;
    let indirect_result = abi.is_some_and(|abi| abi.result.is_some());
    let returned = if indirect_result {
        m::Ty::Void
    } else {
        llvm_type(&source.return_type)?
    };
    let payload = internal_result_type(&returned);
    let poll = m::Ty::Struct(vec![m::Ty::Int(32), payload]);
    let root_name = format!("zil.sabi.native.root.{}", id.0);
    let parameters = if indirect_result {
        vec![m::Param::new(m::Ty::Ptr, "result.out")]
    } else {
        vec![]
    };
    let mut entry = vec![];
    push(
        &mut entry,
        Some("frame"),
        m::Op::Alloca {
            ty: m::Ty::Named(format!("zil_task_frame_{}", id.0)),
            align: 8,
        },
    );
    let mut inputs = vec![m::Arg::new(m::Ty::Ptr, loc("frame"))];
    if indirect_result {
        inputs.push(m::Arg::new(m::Ty::Ptr, loc("result.out")));
    }
    call(
        &mut entry,
        None,
        m::Ty::Void,
        m::global(name(id, "init_into")),
        inputs,
    );
    call(
        &mut entry,
        Some("polled"),
        poll.clone(),
        m::global(name(id, "poll")),
        vec![m::Arg::new(m::Ty::Ptr, loc("frame"))],
    );
    push(
        &mut entry,
        Some("tag"),
        m::Op::Extract {
            arg: m::Arg::new(poll.clone(), loc("polled")),
            index: 0,
        },
    );
    compare(
        &mut entry,
        "ready",
        m::Ty::Int(32),
        loc("tag"),
        m::val(1),
        "eq",
    );
    cond(&mut entry, "ready", "close", "task.trap");
    let mut close = vec![];
    if returned != m::Ty::Void {
        push(
            &mut close,
            Some("result"),
            m::Op::Extract {
                arg: m::Arg::new(poll, loc("polled")),
                index: 1,
            },
        );
    }
    call(
        &mut close,
        Some("closed"),
        m::Ty::Int(1),
        m::global(name(id, "poll_close")),
        vec![m::Arg::new(m::Ty::Ptr, loc("frame"))],
    );
    cond(&mut close, "closed", "done", "task.trap");
    let mut done = vec![];
    call(
        &mut done,
        None,
        m::Ty::Void,
        m::global(name(id, "finish_into")),
        vec![m::Arg::new(m::Ty::Ptr, loc("frame"))],
    );
    if returned == m::Ty::Void {
        push(&mut done, None, m::Op::Return(None));
    } else {
        ret(&mut done, returned.clone(), loc("result"));
    }
    let mut code = m::Code::from(m::Event::Begin(m::Function {
        name: root_name.clone(),
        linkage: m::Linkage::Internal,
        signature: m::Signature::new(returned, parameters),
        attrs: vec![],
        blocks: None,
    }));
    for block in [
        block("entry", entry),
        block("close", close),
        block("done", done),
        trap_block(),
    ] {
        code.push(m::Event::Label(block.name));
        for instruction in block.instructions {
            code.push(m::Event::Instruction(instruction));
        }
    }
    code.push(m::Event::End);
    Ok((root_name, code))
}
