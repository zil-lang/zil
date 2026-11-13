use super::*;
use crate::model::{
    Arg, Attr, Block, Function, Inst, Item, Linkage, Op, Param, Signature, Ty, Val,
};
use std::collections::BTreeMap;

fn module(noinline: bool) -> crate::model::Module {
    let function = |name: &str, params: Vec<Param>, instructions, attrs| {
        Item::Function(Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), params),
            attrs,
            blocks: Some(
                vec![Block {
                    name: "entry".into(),
                    instructions,
                }]
                .into(),
            ),
        })
    };
    crate::model::Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "same diagnostic".into(),
        items: vec![
            function(
                "leaf",
                vec![Param::new(Ty::Int(32), "x")],
                vec![
                    Inst::new(
                        Some("v".into()),
                        Op::Binary {
                            opcode: "add".into(),
                            ty: Ty::Int(32),
                            left: Val::Local("x".into()),
                            right: Val::Int(7),
                        },
                    ),
                    Inst::new(
                        None,
                        Op::Return(Some(Arg::new(Ty::Int(32), Val::Local("v".into())))),
                    ),
                ],
                if noinline {
                    vec![Attr::NoInline]
                } else {
                    vec![]
                },
            ),
            function(
                "root",
                vec![Param::new(Ty::Int(32), "x")],
                vec![
                    Inst::new(
                        Some("v".into()),
                        Op::Call {
                            result: Ty::Int(32),
                            result_attrs: vec![],
                            callee: Val::Global("leaf".into()),
                            args: vec![Arg::new(Ty::Int(32), Val::Local("x".into()))],
                            vararg: None,
                        },
                    ),
                    Inst::new(
                        None,
                        Op::Return(Some(Arg::new(Ty::Int(32), Val::Local("v".into())))),
                    ),
                ],
                vec![],
            ),
            function(
                "dead",
                vec![],
                vec![Inst::new(None, Op::Return(Some(Arg::new(Ty::Int(32), 42))))],
                vec![],
            ),
            function(
                "abi",
                vec![],
                vec![Inst::new(None, Op::Return(Some(Arg::new(Ty::Int(32), 3))))],
                vec![],
            ),
        ]
        .into(),
    }
}
fn config() -> Config<'static> {
    Config {
        triple: "x86_64-unknown-linux-gnu",
        optimization: Optimization::O2,
    }
}
// Exact ELF64 ET_REL STT_FUNC bytes. The fixture's retained bodies have no
// symbolic operands after inline; the larger project quality gate separately
// compares normalized relocations/instructions/CFG/ABI for all real outputs.
fn functions(b: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let u16at = |o| u16::from_le_bytes(b[o..o + 2].try_into().unwrap()) as usize;
    let u32at = |o| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
    let u64at = |o| u64::from_le_bytes(b[o..o + 8].try_into().unwrap()) as usize;
    assert_eq!(&b[..6], b"\x7fELF\x02\x01");
    assert_eq!(u16at(16), 1);
    let (base, width, count) = (u64at(40), u16at(58), u16at(60));
    let section = |i: usize| base + i * width;
    let mut out = BTreeMap::new();
    for i in 0..count {
        let sh = section(i);
        if u32at(sh + 4) != 2 {
            continue;
        }
        let strings = section(u32at(sh + 40));
        let names = &b[u64at(strings + 24)..u64at(strings + 24) + u64at(strings + 32)];
        let (offset, length, entry) = (u64at(sh + 24), u64at(sh + 32), u64at(sh + 56));
        for sym in (offset..offset + length).step_by(entry) {
            let (index, size) = (u16at(sym + 6), u64at(sym + 16));
            if b[sym + 4] & 15 != 2 || index == 0 || index >= count || size == 0 {
                continue;
            }
            let name = &names[u32at(sym)..];
            let name =
                std::str::from_utf8(&name[..name.iter().position(|v| *v == 0).unwrap()]).unwrap();
            let at = u64at(section(index) + 24) + u64at(sym + 8);
            assert!(out.insert(name.into(), b[at..at + size].to_vec()).is_none());
        }
    }
    out
}

#[test]
fn late_closed_removes_only_after_original_inline_and_keeps_retained_machine_bytes() {
    let m = module(false);
    let ordinary = crate::compile_module(&m, &config(), &|| false).unwrap();
    let plan = crate::LateClosedPlan::new(&m, ["leaf".into(), "dead".into()]);
    let (late, _, products) = crate::compile_module_with_late_closed(
        &m,
        &config(),
        FunctionMerging::Disabled,
        &|| false,
        &plan,
        false,
        None,
    )
    .unwrap();
    assert!(products.is_none());
    assert!(late.receipt.verify_before_after);
    assert!(late.receipt.preflight_ns > 0);
    let report = late.receipt.late_closed.as_ref().unwrap();
    assert!(report.ran_globaldce);
    assert!(report.cost_ns > 0);
    assert_eq!(report.eligible, ["dead", "leaf"]);
    assert_eq!(report.removed, ["dead", "leaf"]);
    assert_eq!(report.after_definitions, ["abi", "root"]);
    let (before, after) = (functions(&ordinary.bytes), functions(&late.bytes));
    assert_eq!(before.len(), 4);
    assert_eq!(after.len(), 2);
    for (name, bytes) in &after {
        assert_eq!(Some(bytes), before.get(name));
    }
    assert!(late.bytes.len() < ordinary.bytes.len());
    let again = crate::compile_module_with_late_closed(
        &m,
        &config(),
        FunctionMerging::Disabled,
        &|| false,
        &plan,
        false,
        None,
    )
    .unwrap()
    .0;
    assert_eq!(again.bytes, late.bytes);
}

#[test]
fn late_closed_preserves_current_remaining_calls_and_rejects_an_unbound_plan() {
    let m = module(true);
    let ordinary = crate::compile_module(&m, &config(), &|| false).unwrap();
    let plan = crate::LateClosedPlan::new(&m, ["leaf".into(), "dead".into()]);
    let late = crate::compile_module_with_late_closed(
        &m,
        &config(),
        FunctionMerging::Disabled,
        &|| false,
        &plan,
        false,
        None,
    )
    .unwrap()
    .0;
    let report = late.receipt.late_closed.as_ref().unwrap();
    assert_eq!(report.removed, ["dead"]);
    assert_eq!(report.refused.get("remaining-users"), Some(&1));
    for (name, bytes) in functions(&late.bytes) {
        assert_eq!(Some(&bytes), functions(&ordinary.bytes).get(&name));
    }
    assert_eq!(
        crate::compile_module_with_late_closed(
            &module(false),
            &config(),
            FunctionMerging::Disabled,
            &|| false,
            &plan,
            false,
            None
        )
        .unwrap_err()
        .kind,
        ErrorKind::Verify
    );
}
