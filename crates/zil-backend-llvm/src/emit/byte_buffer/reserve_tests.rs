use super::*;

fn functions() -> Vec<m::Function> {
    storage_operations()
        .into_items()
        .unwrap()
        .into_iter()
        .filter_map(|i| {
            if let m::Item::Function(f) = i {
                Some(f)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn reserve_fast_path_keeps_checks_without_inline_allocation_or_public_symbols() {
    let fs = functions();
    let front = fs.iter().find(|f| f.name == "zil.bytes.reserve").unwrap();
    let grow = fs.iter().find(|f| f.name == "zil.bytes.grow").unwrap();
    assert_eq!(front.signature, grow.signature);
    assert!(front.attrs.is_empty());
    assert_eq!(grow.attrs, vec![m::Attr::NoInline]);
    assert!(fs.iter().all(|f| f.linkage == m::Linkage::Internal));
    assert_eq!(
        fs.iter()
            .filter(|f| f.attrs.contains(&m::Attr::NoInline))
            .count(),
        1
    );
    let fb = front.blocks.as_ref().unwrap();
    let gb = grow.blocks.as_ref().unwrap();
    // The public-facing helper rejects negative input before dereferencing the
    // header, and reaches the private grow only from the failed capacity test.
    assert_eq!(fb[0].name, "entry");
    assert!(matches!(&fb[0].instructions[0].op,
        m::Op::Compare { predicate, ty:m::Ty::Int(64), left:m::Val::Local(x), right:m::Val::Int(0) }
        if predicate == "slt" && x == "requested"));
    assert!(matches!(&fb[0].instructions[1].op,
        m::Op::CondBranch { condition:m::Val::Local(c), yes, no }
        if c == "negative" && yes == "failed" && no == "capacity"));
    let capacity = fb.iter().find(|b| b.name == "capacity").unwrap();
    assert!(matches!(&capacity.instructions.last().unwrap().op,
        m::Op::CondBranch { condition:m::Val::Local(c), yes, no }
        if c == "fits" && yes == "done" && no == "growth"));
    assert!(capacity.instructions.iter().any(|i| matches!(&i.op,
        m::Op::Compare { predicate, ty:m::Ty::Int(64), left:m::Val::Local(x), right:m::Val::Local(cap) }
        if predicate == "ule" && x == "requested" && cap == "cap")));
    assert_eq!(gb[0].name, "entry");
    assert!(
        !gb.iter()
            .any(|b| b.name == "capacity" || b.name == "prepare")
    );
    assert!(!gb.iter().flat_map(|b| &b.instructions).any(|i| matches!(
        &i.op,
        m::Op::Compare {
            ty: m::Ty::Int(64),
            ..
        }
    )));
    let callers = fs
        .iter()
        .filter(|f| {
            f.blocks
                .as_ref()
                .unwrap()
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| {
                    matches!(&i.op,
            m::Op::Call { callee:m::Val::Global(name), .. } if name == "zil.bytes.grow")
                })
        })
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(callers, vec!["zil.bytes.reserve"]);
    for label in ["done", "failed"] {
        assert_eq!(
            fb.iter().find(|b| b.name == label),
            gb.iter().find(|b| b.name == label)
        );
    }
    let calls = fb
        .iter()
        .flat_map(|b| &b.instructions)
        .filter_map(|i| {
            if let m::Op::Call { callee, .. } = &i.op {
                Some(callee)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(calls, vec![&m::Val::Global("zil.bytes.grow".into())]);
    assert!(
        !fb.iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.op, m::Op::Store { .. }))
    );
    let stores = gb
        .iter()
        .flat_map(|b| {
            b.instructions
                .iter()
                .filter_map(move |i| matches!(i.op, m::Op::Store { .. }).then_some(b.name.as_str()))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        stores,
        vec!["publish", "publish"],
        "pointer/capacity are published only after success"
    );
}

#[test]
fn outlined_storage_helpers_pass_complete_machine_preflight() {
    let mut code = m::Code::new();
    for (name, result, params) in [
        ("realloc", m::Ty::Ptr, vec![m::Ty::Ptr, m::Ty::Int(64)]),
        ("free", m::Ty::Void, vec![m::Ty::Ptr]),
    ] {
        code.push(m::Event::Declare(m::Function {
            name: name.into(),
            linkage: m::Linkage::External,
            signature: m::Signature {
                result,
                result_attrs: vec![],
                params: params.into_iter().map(m::Param::unnamed).collect(),
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }));
    }
    code.append(linux_allocation());
    code.append(storage_operations());
    let module = m::Module::from_code(
        "x86_64-unknown-linux-gnu".into(),
        "reserve-control".into(),
        code,
    )
    .unwrap();
    zil_llvm_api::validate_module(&module).unwrap();
}
