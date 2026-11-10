use super::*;
use zil_base::{FileId, Span, TargetProfile};
use zil_ir::{
    CallKind, CallableKind, ClosureDefinition, ClosureId, FunctionId, LirModule, PassingMode, Type,
    TypedFunction,
};
use zil_llvm_api::model::{Arg, Block, Function, Global, Inst, Param, Signature, Ty};

fn source(count: u32) -> LirModule {
    LirModule {
        schema_version: zil_base::SCHEMA_VERSION,
        module_id: zil_base::ModuleId(0),
        verified_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        resources: vec![],
        layouts: vec![],
        closures: vec![ClosureDefinition {
            storage_life: None,
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![Type::I64],
            parameters: vec![],
            result: Type::I64,
            kind: CallableKind::Fn,
            lending_captures: vec![],
        }],
        functions: (0..count)
            .map(|id| TypedFunction {
                storage_life: None,
                call_kind: CallKind::Direct,
                reference_return_bound: None,
                callable_return_contract: None,
                callable_contract: None,
                protocol_contract: None,
                id: FunctionId(id),
                name: format!("source_{id}"),
                foreign: None,
                environment: None,
                parameters: vec![],
                return_type: Type::I64,
                result_mode: PassingMode::Infer,
                locals: vec![],
                values: vec![],
                blocks: vec![],
                entry: zil_ir::BlockId(0),
                span: Span {
                    file: FileId(id),
                    ..Span::default()
                },
            })
            .collect(),
    }
}
fn local(name: &str) -> Val {
    Val::Local(name.into())
}
fn global(name: &str) -> Val {
    Val::Global(name.into())
}
fn inst(name: Option<&str>, op: Op) -> Inst {
    Inst::new(name.map(str::to_owned), op)
}
fn call(name: &str) -> Op {
    Op::Call {
        result: Ty::Int(64),
        result_attrs: vec![],
        callee: global(name),
        args: vec![Arg::new(Ty::Int(64), local("x"))],
        vararg: None,
    }
}
fn function(name: &str, linkage: Linkage, body: Vec<Inst>) -> Item {
    Item::Function(Function {
        name: name.into(),
        linkage,
        signature: Signature::new(Ty::Int(64), vec![Param::new(Ty::Int(64), "x")]),
        attrs: vec![],
        blocks: Some(vec![Block {
            name: "entry".into(),
            instructions: body,
        }]),
    })
}
fn caller(id: u32, callee: &str) -> Item {
    function(
        &format!("zil.fn.{id}"),
        Linkage::External,
        vec![
            inst(Some("value"), call(callee)),
            inst(
                None,
                Op::Return(Some(Arg::new(Ty::Int(64), local("value")))),
            ),
        ],
    )
}
fn helper(name: &str, by: i128) -> Item {
    function(
        name,
        Linkage::Internal,
        vec![
            inst(
                Some("value"),
                Op::Binary {
                    opcode: "add".into(),
                    ty: Ty::Int(64),
                    left: local("x"),
                    right: Val::Int(by),
                },
            ),
            inst(
                None,
                Op::Return(Some(Arg::new(Ty::Int(64), local("value")))),
            ),
        ],
    )
}
fn module(items: Vec<Item>) -> Module {
    Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: String::new(),
        items,
    }
}
fn input() -> Module {
    module(vec![
        caller(0, "helper"),
        caller(1, "helper"),
        helper("helper", 7),
    ])
}
fn planned(input: Module, lir: &LirModule, count: usize) -> Plan {
    prepare(&input, &lir.as_view(), EntryPoint::None, count)
        .expect("copyable helpers split")
        .replay(input)
}
fn symbol_count(unit: &Module, name: &str) -> usize {
    unit.items
        .iter()
        .filter(|item| matches!(item,Item::Function(f) if f.name==name && f.blocks.is_some()))
        .count()
}

#[test]
fn shared_call_only_helpers_do_not_join_independent_source_modules() {
    let lir = source(2);
    let plan = planned(input(), &lir, 8);
    assert_eq!(plan.units.len(), 2);
    for unit in &plan.units {
        zil_llvm_api::validate_module(unit).unwrap();
        assert_eq!(symbol_count(unit, "helper"), 1);
        assert!(unit.items.iter().any(
            |i| matches!(i,Item::Function(f) if f.name=="helper" && f.linkage==Linkage::Internal)
        ));
    }
    let mut changed = input();
    let Item::Function(f) = &mut changed.items[0] else {
        unreachable!()
    };
    f.blocks.as_mut().unwrap()[0].instructions[0].op = Op::Binary {
        opcode: "add".into(),
        ty: Ty::Int(64),
        left: local("x"),
        right: Val::Int(19),
    };
    let after = planned(changed, &lir, 8);
    assert_ne!(plan.units[0], after.units[0]);
    assert_eq!(plan.units[1], after.units[1]);
    let mut changed = input();
    changed.items[2] = helper("helper", 9);
    let after = planned(changed, &lir, 8);
    assert!(
        plan.units.iter().zip(after.units).all(|(a, b)| *a != b),
        "helper change reaches all actual copies"
    );
}

#[test]
fn address_observation_and_local_mutable_state_prevent_code_duplication() {
    let lir = source(2);
    let mut address = input();
    address.items.push(Item::Global(Global {
        name: "callback".into(),
        linkage: Linkage::External,
        ty: Ty::Ptr,
        constant: true,
        initializer: Some(global("helper")),
        unnamed_addr: false,
        alignment: 8,
    }));
    assert!(prepare(&address, &lir.as_view(), EntryPoint::None, 8).is_none());
    let mut state = input();
    state.items.push(Item::Global(Global {
        name: "state".into(),
        linkage: Linkage::Internal,
        ty: Ty::Int(64),
        constant: false,
        initializer: Some(Val::Int(0)),
        unnamed_addr: false,
        alignment: 8,
    }));
    let Item::Function(f) = &mut state.items[2] else {
        unreachable!()
    };
    f.blocks.as_mut().unwrap()[0].instructions.insert(
        0,
        inst(
            Some("state"),
            Op::Load {
                ty: Ty::Int(64),
                ptr: global("state"),
                align: 8,
            },
        ),
    );
    assert!(prepare(&state, &lir.as_view(), EntryPoint::None, 8).is_none());
    // An address passed as an argument is not a direct-call-only use.
    let mut indirect = input();
    let mut observer = Function {
        name: "observe".into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Void, vec![Param::unnamed(Ty::Ptr)]),
        attrs: vec![],
        blocks: None,
    };
    observer.signature.variadic = false;
    indirect.items.push(Item::Function(observer));
    let Item::Function(f) = &mut indirect.items[0] else {
        unreachable!()
    };
    f.blocks.as_mut().unwrap()[0].instructions.insert(
        0,
        inst(
            None,
            Op::Call {
                result: Ty::Void,
                result_attrs: vec![],
                callee: global("observe"),
                args: vec![Arg::new(Ty::Ptr, global("helper"))],
                vararg: None,
            },
        ),
    );
    assert!(prepare(&indirect, &lir.as_view(), EntryPoint::None, 8).is_none());
}

#[test]
fn helper_rejection_propagates_along_real_reverse_dependencies_and_cycles_are_bounded() {
    let lir = source(2);
    let mut m = input();
    m.items[2] = function(
        "helper",
        Linkage::Internal,
        vec![
            inst(Some("v"), call("inner")),
            inst(None, Op::Return(Some(Arg::new(Ty::Int(64), local("v"))))),
        ],
    );
    m.items.push(helper("inner", 3));
    let good = planned(m.clone(), &lir, 8);
    assert!(good.units.iter().all(|u| symbol_count(u, "inner") == 1));
    m.items.push(Item::Global(Global {
        name: "ptr".into(),
        linkage: Linkage::External,
        ty: Ty::Ptr,
        constant: true,
        initializer: Some(global("inner")),
        unnamed_addr: false,
        alignment: 8,
    }));
    assert!(prepare(&m, &lir.as_view(), EntryPoint::None, 8).is_none());
    let mut recursive = input();
    recursive.items[2] = function(
        "helper",
        Linkage::Internal,
        vec![
            inst(Some("v"), call("helper")),
            inst(None, Op::Return(Some(Arg::new(Ty::Int(64), local("v"))))),
        ],
    );
    let p = planned(recursive, &lir, 8);
    assert_eq!(p.units.len(), 2);
    for u in &p.units {
        zil_llvm_api::validate_module(u).unwrap();
    }
}

#[test]
fn complete_source_imports_preserve_canonical_addresses_and_respect_copy_budget() {
    let lir = source(2);
    let m = module(vec![
        caller(0, "zil.fn.1"),
        caller(1, "helper"),
        helper("helper", 7),
    ]);
    let p = planned(m, &lir, 8);
    assert!(p.units[0].items.iter().any(|i|matches!(i,Item::Function(f) if f.name=="zil.resource.import.0.zil.fn.1" && f.linkage==Linkage::Internal)));
    assert_eq!(symbol_count(&p.units[1], "zil.fn.1"), 1);
    for u in &p.units {
        zil_llvm_api::validate_module(u).unwrap();
    }
    let mut m = module(vec![
        caller(0, "zil.fn.1"),
        caller(1, "zil.fn.2"),
        caller(2, "zil.fn.0"),
        helper("helper", 7),
    ]);
    let Item::Function(f) = &mut m.items[0] else {
        unreachable!()
    };
    f.blocks.as_mut().unwrap()[0]
        .instructions
        .insert(0, inst(Some("h"), call("helper")));
    assert!(
        prepare(&m, &source(3).as_view(), EntryPoint::None, 8).is_none(),
        "complete copies exceed one original graph"
    );
}

#[test]
fn rejected_or_ineligible_plans_leave_the_original_input_untouched() {
    let mut lir = source(2);
    let original = input();
    assert!(prepare(&original, &lir.as_view(), EntryPoint::None, 1).is_none());
    lir.functions[1].span.file = FileId(0);
    assert!(prepare(&original, &lir.as_view(), EntryPoint::None, 8).is_none());
    let mut broken = input();
    broken.items.push(helper("helper", 3));
    let before = broken.clone();
    assert!(prepare(&broken, &source(2).as_view(), EntryPoint::None, 8).is_none());
    assert_eq!(broken, before);
    let mut malformed = input();
    malformed.items.push(function(
        "unused",
        Linkage::Internal,
        vec![inst(
            None,
            Op::Return(Some(Arg::new(Ty::Int(64), local("absent")))),
        )],
    ));
    assert!(
        prepare(&malformed, &source(2).as_view(), EntryPoint::None, 8).is_none(),
        "unused malformed body must not be hidden"
    );
}

#[cfg(all(feature = "llvm-api", target_os = "linux", target_arch = "x86_64"))]
#[test]
fn duplicated_helper_code_preserves_real_external_effects_and_native_results() {
    use std::{fs, process::Command};
    use zil_llvm_api::{Config, FunctionMerging, Optimization};
    let mut m = input();
    m.items.push(Item::Function(Function {
        name: "tick".into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(64), vec![Param::unnamed(Ty::Int(64))]),
        attrs: vec![],
        blocks: None,
    }));
    let Item::Function(f) = &mut m.items[2] else {
        unreachable!()
    };
    f.blocks.as_mut().unwrap()[0]
        .instructions
        .insert(0, inst(Some("tick"), call("tick")));
    let p = planned(m, &source(2), 8);
    let dir = std::env::temp_dir().join(format!("zil-helper-partition-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let mut objects = Vec::new();
    for (index, unit) in p.units.iter().enumerate() {
        zil_llvm_api::validate_module(unit).unwrap();
        let result = zil_llvm_api::compile_module_with_function_merging(
            unit,
            &Config {
                triple: "x86_64-unknown-linux-gnu",
                optimization: Optimization::O2,
            },
            FunctionMerging::AfterOptimization,
            &|| false,
        )
        .unwrap();
        assert!(result.receipt.verify_before_after);
        let object = dir.join(format!("{index}.o"));
        fs::write(&object, result.bytes).unwrap();
        objects.push(object);
    }
    fs::write(
        dir.join("oracle.c"),
        r#"#include <stdio.h>
static long calls;
long tick(long x){++calls;return x;}
extern long a(long) __asm__("zil.fn.0");
extern long b(long) __asm__("zil.fn.1");
int main(void){long x=a(5),y=b(8);printf("%ld %ld %ld\n",x,y,calls);return 0;}
"#,
    )
    .unwrap();
    let exe = dir.join("app");
    let link = Command::new(std::env::var_os("ZIL_CLANG").expect("explicit clang"))
        .arg(dir.join("oracle.c"))
        .args(objects)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(
        link.status.success(),
        "{}",
        String::from_utf8_lossy(&link.stderr)
    );
    let run = Command::new(exe).output().unwrap();
    assert!(run.status.success());
    assert_eq!(run.stdout, b"12 15 2\n");
}

#[test]
fn large_source_bodies_keep_canonical_calls_and_independent_object_inputs() {
    let lir = source(2);
    let mut input = module(vec![
        caller(0, "zil.fn.1"),
        caller(1, "helper"),
        helper("helper", 7),
    ]);
    let Item::Function(function) = &mut input.items[1] else {
        unreachable!()
    };
    let mut index = 0;
    while function.cost() <= MAX_IMPORTED_SOURCE_COST {
        function.blocks.as_mut().unwrap()[0].instructions.insert(
            0,
            inst(
                Some(&format!("extra.{index}")),
                Op::Binary {
                    opcode: "xor".into(),
                    ty: Ty::Int(64),
                    left: local("x"),
                    right: Val::Int(index),
                },
            ),
        );
        index += 1;
    }
    let before = planned(input.clone(), &lir, 8);
    assert_eq!(before.units.len(), 2);
    assert_eq!(
        symbol_count(&before.units[0], "zil.resource.import.0.zil.fn.1"),
        0
    );
    assert!(before.units[0].items.iter().any(|item| matches!(item,
        Item::Function(f) if f.name == "zil.fn.1" && f.blocks.is_none())));
    assert_eq!(symbol_count(&before.units[1], "zil.fn.1"), 1);
    for unit in &before.units {
        zil_llvm_api::validate_module(unit).unwrap();
    }

    // This changes the actual returned result, not merely unreachable padding.
    // The caller's prototype and instruction stream must remain identical.
    let Item::Function(function) = &mut input.items[1] else {
        unreachable!()
    };
    *function.blocks.as_mut().unwrap()[0]
        .instructions
        .last_mut()
        .unwrap() = inst(None, Op::Return(Some(Arg::new(Ty::Int(64), Val::Int(99)))));
    let after = planned(input, &lir, 8);
    assert_eq!(before.units[0], after.units[0]);
    assert_ne!(before.units[1], after.units[1]);
}
