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
    assert_eq!(
        fb[0], gb[0],
        "negative requests still reject before accessing the header"
    );
    let mut old_capacity = gb[1].clone();
    if let m::Op::CondBranch { no, .. } = &mut old_capacity.instructions.last_mut().unwrap().op {
        assert_eq!(no, "prepare");
        *no = "growth".into();
    } else {
        panic!("capacity branch missing")
    }
    assert_eq!(fb[1], old_capacity);
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
