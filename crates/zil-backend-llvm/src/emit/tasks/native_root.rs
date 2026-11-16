//! A native execution root for a checked, parameterless SABI worker.
//! The root owns the actual worker frame until its complete poll/close
//! protocol finishes. Pending preserves that frame and resumes the same
//! operation; only Ready transfers the result to the native caller.
use super::*;

pub(in super::super) fn adapter(
    source: &TypedFunction,
    abi: Option<&large_copy_abi::Signature>,
    verified: &VerifiedModule,
    meter: &Meter,
) -> CompileResult<(String, m::Code)> {
    let id = source.id;
    if !verified.is_suspend(id)
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
    branch(&mut entry, "poll");
    let mut polling = vec![];
    call(
        &mut polling,
        Some("polled"),
        poll.clone(),
        m::global(name(id, "poll")),
        vec![m::Arg::new(m::Ty::Ptr, loc("frame"))],
    );
    push(
        &mut polling,
        Some("tag"),
        m::Op::Extract {
            arg: m::Arg::new(poll.clone(), loc("polled")),
            index: 0,
        },
    );
    compare(
        &mut polling,
        "ready",
        m::Ty::Int(32),
        loc("tag"),
        m::val(1),
        "eq",
    );
    cond(&mut polling, "ready", "receive.result", "not.ready");
    let mut pending = vec![];
    compare(
        &mut pending,
        "pending",
        m::Ty::Int(32),
        loc("tag"),
        m::val(0),
        "eq",
    );
    // Completed, Cancelled, and Invalid are protocol errors for this root:
    // it owns a freshly initialized frame and takes Ready exactly once.
    cond(&mut pending, "pending", "poll", "task.trap");
    let mut ready = vec![];
    if returned != m::Ty::Void {
        push(
            &mut ready,
            Some("result"),
            m::Op::Extract {
                arg: m::Arg::new(poll, loc("polled")),
                index: 1,
            },
        );
    }
    branch(&mut ready, "close");
    let mut close = vec![];
    call(
        &mut close,
        Some("closed"),
        m::Ty::Int(1),
        m::global(name(id, "poll_close")),
        vec![m::Arg::new(m::Ty::Ptr, loc("frame"))],
    );
    // The result remains owned by this invocation while children and any
    // remaining resource owners drain. Never repeat poll after taking Ready.
    cond(&mut close, "closed", "done", "close");
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
        block("poll", polling),
        block("not.ready", pending),
        block("receive.result", ready),
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
