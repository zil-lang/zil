//! A bounded optimizer boundary, never a new ownership or aliasing fact.
//! Keep the empty/surrendered cases visible to the caller. Only an allocated,
//! single byte-buffer capture enters the unchanged out-of-line destructor.
use super::*;

pub(super) fn capture(definition: &ClosureDefinition) -> Option<u32> {
    let mut found = None;
    for (index, ty) in definition.captures.iter().enumerate() {
        if matches!(ty, Type::Managed(payload) if **payload == Type::ByteBuffer) {
            if found.replace(u32::try_from(index).ok()?).is_some() {
                return None;
            }
        } else if flag_count(ty) != 0 {
            // No shortcut for nested ownership, user teardown or partial fields.
            return None;
        }
    }
    found
}

pub(super) fn worker_name(id: ClosureId) -> String {
    format!("zil.closure.drop.allocated.{}", id.0)
}

pub(super) fn entry(definition: &ClosureDefinition, capture: u32) -> m::Code {
    let local = |name: &str| m::local(name.into());
    let arg = |ty, name: &str| m::Arg::new(ty, local(name));
    let inst =
        |name: Option<&str>, op| m::Event::Instruction(m::Inst::new(name.map(str::to_owned), op));
    let field = |index| m::Op::Gep {
        ty: environment_type(definition.id),
        ptr: local("env"),
        indices: vec![
            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
            m::Arg::new(m::Ty::Int(32), m::val(index)),
        ],
    };
    m::Code::from(vec![
        m::Event::Begin(m::Function {
            name: format!("zil.closure.drop.{}", definition.id.0),
            linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: m::Ty::Void,
                result_attrs: vec![],
                params: vec![m::Param {
                    ty: m::Ty::Ptr,
                    name: Some("env".into()),
                    attrs: vec![],
                }],
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }),
        m::Event::Label("entry".into()),
        inst(Some("flag"), field(flag_index(definition, capture))),
        inst(
            Some("live"),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: local("flag"),
                align: 1,
            },
        ),
        inst(
            None,
            m::Op::CondBranch {
                condition: local("live"),
                yes: "inspect".into(),
                no: "done".into(),
            },
        ),
        // A moved-out capture is not inspected, including its payload pointer.
        m::Event::Label("inspect".into()),
        inst(Some("payload"), field(capture as usize)),
        inst(
            Some("data.field"),
            m::Op::Gep {
                ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                ptr: local("payload"),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                ],
            },
        ),
        inst(
            Some("data"),
            m::Op::Load {
                ty: m::Ty::Ptr,
                ptr: local("data.field"),
                align: 8,
            },
        ),
        inst(
            Some("is.empty"),
            m::Op::Compare {
                predicate: "eq".into(),
                ty: m::Ty::Ptr,
                left: local("data"),
                right: m::Val::Null,
            },
        ),
        inst(
            None,
            m::Op::CondBranch {
                condition: local("is.empty"),
                yes: "empty".into(),
                no: "allocated".into(),
            },
        ),
        m::Event::Label("empty".into()),
        inst(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                ptr: local("flag"),
                align: 1,
            },
        ),
        // Preserve the original allocator call and ordering, even for null.
        // LLVM may use its ordinary libcall rules; this emitter assumes none.
        inst(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global("zil.bytes.drop".into()),
                args: vec![arg(m::Ty::Ptr, "payload")],
                vararg: None,
            },
        ),
        inst(None, m::Op::Branch("done".into())),
        m::Event::Label("allocated".into()),
        inst(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global(worker_name(definition.id)),
                args: vec![arg(m::Ty::Ptr, "env")],
                vararg: None,
            },
        ),
        inst(None, m::Op::Branch("done".into())),
        m::Event::Label("done".into()),
        inst(None, m::Op::Return(None)),
        m::Event::End,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn definition(id: u32, count: usize) -> ClosureDefinition {
        ClosureDefinition {
            storage_life: None,
            id: ClosureId(id),
            function: FunctionId(id),
            captures: vec![Type::Managed(Box::new(Type::ByteBuffer)); count],
            parameters: vec![],
            result: Type::Unit,
            kind: CallableKind::FnMut,
            lending_captures: vec![],
        }
    }
    fn functions(definitions: &[ClosureDefinition]) -> Vec<m::Function> {
        drop_functions(
            definitions,
            &[ResourceDefinition {
                name: "bytes".into(),
                payload: Type::ByteBuffer,
                teardown: None,
            }],
        )
        .unwrap()
        .into_items()
        .unwrap()
        .into_iter()
        .map(|item| {
            let m::Item::Function(function) = item else {
                panic!("expected function")
            };
            function
        })
        .collect()
    }
    #[test]
    fn allocated_boundary_preserves_the_entire_original_worker() {
        for count in [1, 8, 15, 16, 32, 64] {
            let ds = (0..count).map(|id| definition(id, 1)).collect::<Vec<_>>();
            let fs = functions(&ds);
            assert_eq!(fs.len(), count as usize * if count >= 16 { 2 } else { 1 });
            for d in &ds {
                let original = functions(std::slice::from_ref(d)).pop().unwrap();
                if count < 16 {
                    assert_eq!(
                        *fs.iter().find(|f| f.name == original.name).unwrap(),
                        original
                    );
                } else {
                    let mut worker = fs
                        .iter()
                        .find(|f| f.name == worker_name(d.id))
                        .unwrap()
                        .clone();
                    assert_eq!(worker.attrs, vec![m::Attr::NoInline]);
                    worker.name = original.name.clone();
                    worker.attrs.clear();
                    assert_eq!(worker, original);
                    let front = fs.iter().find(|f| f.name == original.name).unwrap();
                    assert!(front.attrs.is_empty());
                    assert_eq!(front.signature, original.signature);
                    let blocks = front.blocks.as_ref().unwrap();
                    assert!(matches!(&blocks[0].instructions.last().unwrap().op,
                        m::Op::CondBranch { yes, no, .. } if yes == "inspect" && no == "done"));
                    assert!(
                        !blocks[0]
                            .instructions
                            .iter()
                            .any(|i| matches!(&i.op, m::Op::Load { ty: m::Ty::Ptr, .. }))
                    );
                    let empty = blocks.iter().find(|b| b.name == "empty").unwrap();
                    assert!(matches!(&empty.instructions[0].op, m::Op::Store { .. }));
                    assert!(
                        matches!(&empty.instructions[1].op, m::Op::Call { callee: m::Val::Global(name), .. } if name == "zil.bytes.drop")
                    );
                }
            }
        }
    }
    #[test]
    fn multiple_and_nested_owners_do_not_enter_the_fast_path() {
        assert_eq!(capture(&definition(0, 1)), Some(0));
        assert_eq!(capture(&definition(0, 2)), None);
        assert_eq!(capture(&definition(0, 0)), None);
        let mut d = definition(0, 1);
        d.captures.insert(0, Type::I64);
        assert_eq!(capture(&d), Some(1));
        d.captures.push(Type::Closure(ClosureId(1)));
        assert_eq!(capture(&d), None);
        let definitions = (0..32).map(|id| definition(id, 2)).collect::<Vec<_>>();
        let fs = functions(&definitions);
        assert_eq!(fs.len(), 32);
        assert!(fs.iter().all(|f| f.attrs.is_empty()));
    }
}
