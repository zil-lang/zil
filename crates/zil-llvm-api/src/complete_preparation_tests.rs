//! Complete preparation uses the existing machine validator and one request
//! ledger. Pointer recursion/function recursion must differ from inline cycles.
use super::*;

fn complete_module() -> Module {
    let function = |name: &str, other: &str| {
        Item::Function(Function {
            name: name.into(),
            linkage: Linkage::Internal,
            signature: Signature::new(Ty::Void, vec![]),
            attrs: vec![],
            blocks: Some(vec![Block {
                name: "entry".into(),
                instructions: vec![
                    Inst::new(
                        None,
                        Op::Call {
                            result: Ty::Void,
                            result_attrs: vec![],
                            callee: Val::Global(other.into()),
                            args: vec![],
                            vararg: None,
                        },
                    ),
                    Inst::new(None, Op::Return(None)),
                ],
            }]),
        })
    };
    Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "complete-preparation".into(),
        items: vec![
            Item::NamedType("Node".into(), vec![Ty::Ptr, Ty::Int(8)]),
            Item::NamedType("Pair".into(), vec![Ty::array(2, Ty::Named("Node".into()))]),
            Item::Global(Global {
                name: "callbacks".into(),
                linkage: Linkage::Internal,
                ty: Ty::array(2, Ty::Ptr),
                initializer: Some(Val::Array(vec![
                    Arg::new(Ty::Ptr, Val::Global("first".into())),
                    Arg::new(Ty::Ptr, Val::Global("second".into())),
                ])),
                constant: true,
                unnamed_addr: false,
                alignment: 8,
            }),
            function("first", "second"),
            function("second", "first"),
        ],
    }
}

fn measured(module: &Module) -> usize {
    let mut work = 0usize;
    module
        .validate_complete(|count| {
            work = work.checked_add(count).expect("bounded fixture");
            Ok(())
        })
        .expect("valid complete machine module");
    assert!(work > 0);
    work
}

#[test]
fn complete_module_uses_same_validator_and_exact_shared_request_remainder() {
    let module = complete_module();
    validate_only(&module).unwrap();
    let work = measured(&module);
    for initial in [0, work / 2, work - 1, work] {
        let mut remaining = initial;
        let result = module.validate_complete(|count| {
            remaining = remaining.checked_sub(count).ok_or("request exhausted")?;
            Ok(())
        });
        assert_eq!(result.is_ok(), initial == work);
        if initial == work {
            assert_eq!(remaining, 0);
        } else {
            assert!(result.unwrap_err().contains("request exhausted"));
        }
    }
    let mut remaining = work * 2 - 1;
    let mut pay = |count| {
        remaining = remaining.checked_sub(count).ok_or("request exhausted")?;
        Ok(())
    };
    module.validate_complete(&mut pay).unwrap();
    assert!(module.validate_complete(&mut pay).is_err());
    assert_eq!(
        measured(&module),
        work,
        "fresh request has no cached authority"
    );
}

#[test]
fn complete_module_rejects_missing_definitions_inline_cycles_and_call_abi() {
    let original = complete_module();
    let mut missing = original.clone();
    missing.items.remove(0);
    let mut cycle = original.clone();
    cycle.items[0] = Item::NamedType("Node".into(), vec![Ty::Named("Pair".into())]);
    let mut missing_symbol = original.clone();
    missing_symbol.items.pop();
    let mut wrong_abi = original.clone();
    let Item::Function(function) = &mut wrong_abi.items[4] else {
        unreachable!()
    };
    function
        .signature
        .params
        .push(Param::new(Ty::Int(32), "argument"));
    let mut duplicate = original.clone();
    duplicate.items.push(original.items[3].clone());
    for (module, expected) in [
        (missing, "undefined type"),
        (cycle, "recursive inline"),
        (missing_symbol, "undefined symbol"),
        (wrong_abi, "call signature mismatch"),
        (duplicate, "duplicate/invalid global"),
    ] {
        let old = validate_only(&module).unwrap_err();
        let charged = module.validate_complete(|_| Ok(())).unwrap_err();
        assert_eq!(charged, old);
        assert!(charged.contains(expected), "{charged}");
    }
}

#[test]
fn preparation_charges_cyclic_cfg_and_unreachable_ssa_schedule() {
    let mut module = complete_module();
    let Item::Function(function) = &mut module.items[3] else {
        unreachable!()
    };
    function.blocks = Some(vec![
        Block {
            name: "entry".into(),
            instructions: vec![Inst::new(None, Op::Branch("loop".into()))],
        },
        Block {
            name: "loop".into(),
            instructions: vec![Inst::new(
                None,
                Op::CondBranch {
                    condition: Val::Bool(false),
                    yes: "loop".into(),
                    no: "exit".into(),
                },
            )],
        },
        Block {
            name: "exit".into(),
            instructions: vec![Inst::new(None, Op::Return(None))],
        },
        Block {
            name: "unreachable".into(),
            instructions: vec![Inst::new(None, Op::Return(None))],
        },
    ]);
    validate_only(&module).unwrap();
    let work = measured(&module);
    let mut remaining = work - 1;
    let failure = module
        .validate_complete(|count| {
            remaining = remaining.checked_sub(count).ok_or("CFG budget exhausted")?;
            Ok(())
        })
        .unwrap_err();
    assert!(failure.contains("CFG budget exhausted"));
}
