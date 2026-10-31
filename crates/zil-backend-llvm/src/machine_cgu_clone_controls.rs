//! Dynamic machine-model controls; these do not claim frontend acceptance.
use super::*;
use std::{fs, process::Command};
use zil_llvm_api::{Config, FunctionMerging, Optimization, model::*};

fn source(count: u32) -> LirModule {
    LirModule {
        schema_version: zil_base::SCHEMA_VERSION,
        module_id: zil_base::ModuleId(0),
        verified_fingerprint: Fingerprint::ZERO,
        target: zil_base::TargetProfile::host_llvm(),
        resources: vec![],
        closures: vec![],
        layouts: vec![],
        functions: (0..count)
            .map(|id| zil_ir::TypedFunction {
                callable_return_contract: None,
                callable_contract: None,
                id: zil_ir::FunctionId(id),
                name: format!("f{id}"),
                foreign: None,
                environment: None,
                parameters: vec![],
                return_type: zil_ir::Type::I32,
                result_mode: zil_ir::PassingMode::Infer,
                locals: vec![],
                values: vec![],
                blocks: vec![],
                entry: zil_ir::BlockId(0),
                span: zil_base::Span::default(),
            })
            .collect(),
    }
}
fn instruction(name: &str, op: Op) -> Inst {
    Inst::new(
        if name.is_empty() {
            None
        } else {
            Some(name.into())
        },
        op,
    )
}
fn argument(ty: Ty, name: &str) -> Arg {
    Arg::new(ty, Val::Local(name.into()))
}
fn function(name: &str, params: Vec<Param>, instructions: Vec<Inst>) -> Item {
    Item::Function(Function {
        name: name.into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(32), params),
        attrs: vec![],
        blocks: Some(vec![Block {
            name: "entry".into(),
            instructions,
        }]),
    })
}
fn call(name: &str, args: Vec<Arg>) -> Op {
    Op::Call {
        result: Ty::Int(32),
        result_attrs: vec![],
        callee: Val::Global(name.into()),
        args,
        vararg: None,
    }
}
fn constants(count: u32) -> Vec<Item> {
    (0..count)
        .map(|id| {
            function(
                &format!("zil.fn.{id}"),
                vec![],
                vec![instruction(
                    "",
                    Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(7)))),
                )],
            )
        })
        .collect()
}
fn execute(name: &str, items: Vec<Item>, count: u32, c_source: &str, expected: &[u8]) {
    let directory =
        std::env::temp_dir().join(format!("zil-complete-clone-{}-{name}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let module = Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "clone-control".into(),
        items,
    };
    let source = source(count);
    let plan = partition_view(module, &source.as_view(), EntryPoint::None, 8, true, true);
    assert!(plan.fallback.is_none(), "{:?}", plan.fallback);
    assert!(plan.units.len() > 1);
    assert!(
        plan.units
            .iter()
            .flat_map(|unit| &unit.items)
            .any(|item| matches!(item, Item::Function(f) if f.name.starts_with("zil.import.")))
    );
    let config = Config {
        triple: "x86_64-unknown-linux-gnu",
        optimization: Optimization::O2,
    };
    let mut objects = vec![];
    for (index, module) in plan.units.iter().enumerate() {
        zil_llvm_api::validate_module(module).unwrap();
        let object = zil_llvm_api::compile_module_with_function_merging(
            module,
            &config,
            FunctionMerging::AfterOptimization,
            &|| false,
        )
        .unwrap();
        assert!(object.receipt.verify_before_after);
        let path = directory.join(format!("unit-{index}.o"));
        fs::write(&path, object.bytes).unwrap();
        objects.push(path);
    }
    fs::write(directory.join("oracle.c"), c_source).unwrap();
    let executable = directory.join("program");
    let linked =
        Command::new(std::env::var_os("ZIL_CLANG").expect("explicit clang for dynamic control"))
            .arg("-O2")
            .arg(directory.join("oracle.c"))
            .args(objects)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let native = Command::new(&executable).output().unwrap();
    assert!(native.status.success(), "{native:?}");
    assert_eq!(native.stdout, expected);
    assert!(native.stderr.is_empty());
}

#[test]
fn direct_clones_preserve_self_addresses_callbacks_globals_and_distinct_equal_functions() {
    let mut items = constants(600);
    for id in [256, 257] {
        let name = format!("zil.fn.{id}");
        items[id] = function(
            &name,
            vec![Param::new(Ty::Ptr, "candidate")],
            vec![
                instruction(
                    "old",
                    Op::Load {
                        ty: Ty::Int(32),
                        ptr: Val::Global("shared_counter".into()),
                        align: 4,
                    },
                ),
                instruction(
                    "next",
                    Op::Binary {
                        opcode: "add".into(),
                        ty: Ty::Int(32),
                        left: Val::Local("old".into()),
                        right: Val::Int(1),
                    },
                ),
                instruction(
                    "",
                    Op::Store {
                        arg: argument(Ty::Int(32), "next"),
                        ptr: Val::Global("shared_counter".into()),
                        align: 4,
                    },
                ),
                instruction(
                    "same",
                    Op::Compare {
                        predicate: "eq".into(),
                        ty: Ty::Ptr,
                        left: Val::Local("candidate".into()),
                        right: Val::Global(name.clone()),
                    },
                ),
                instruction(
                    "result",
                    Op::Cast {
                        opcode: "zext".into(),
                        arg: argument(Ty::Int(1), "same"),
                        to: Ty::Int(32),
                    },
                ),
                instruction("", Op::Return(Some(argument(Ty::Int(32), "result")))),
            ],
        );
    }
    items[0] = function(
        "zil.fn.0",
        vec![],
        vec![
            instruction(
                "direct",
                call(
                    "zil.fn.256",
                    vec![Arg::new(Ty::Ptr, Val::Global("zil.fn.256".into()))],
                ),
            ),
            instruction(
                "callback",
                call(
                    "check_callback",
                    vec![
                        Arg::new(Ty::Ptr, Val::Global("zil.fn.256".into())),
                        Arg::new(Ty::Ptr, Val::Global("zil.fn.257".into())),
                        Arg::new(Ty::Ptr, Val::Global("zil.fn.256".into())),
                    ],
                ),
            ),
            instruction(
                "result",
                Op::Binary {
                    opcode: "add".into(),
                    ty: Ty::Int(32),
                    left: Val::Local("direct".into()),
                    right: Val::Local("callback".into()),
                },
            ),
            instruction("", Op::Return(Some(argument(Ty::Int(32), "result")))),
        ],
    );
    items.push(Item::Function(Function {
        name: "check_callback".into(),
        linkage: Linkage::External,
        signature: Signature::new(
            Ty::Int(32),
            (0..3)
                .map(|index| Param::new(Ty::Ptr, format!("p{index}")))
                .collect(),
        ),
        attrs: vec![],
        blocks: None,
    }));
    items.push(Item::Global(Global {
        name: "shared_counter".into(),
        linkage: Linkage::External,
        ty: Ty::Int(32),
        initializer: Some(Val::Int(0)),
        constant: false,
        unnamed_addr: false,
        alignment: 4,
    }));
    items.push(Item::Global(Global {
        name: "callbacks".into(),
        linkage: Linkage::External,
        ty: Ty::array(2, Ty::Ptr),
        initializer: Some(Val::Array(vec![
            Arg::new(Ty::Ptr, Val::Global("zil.fn.256".into())),
            Arg::new(Ty::Ptr, Val::Global("zil.fn.257".into())),
        ])),
        constant: false,
        unnamed_addr: false,
        alignment: 8,
    }));
    execute(
        "addresses",
        items,
        600,
        r#"
#include <stdio.h>
typedef int(*Self)(void*);
extern int invoke(void) __asm__("zil.fn.0");
extern int equal_a(void) __asm__("zil.fn.1"),equal_b(void) __asm__("zil.fn.2");
extern int self_a(void*) __asm__("zil.fn.256"),self_b(void*) __asm__("zil.fn.257");
extern Self callbacks[2]; extern int shared_counter;
int check_callback(void *a,void *b,Self f){return f==(Self)a && f(a)==1 && f(b)==0;}
int main(void){int result=invoke();Self volatile a=callbacks[0],b=callbacks[1];int(*volatile x)(void)=equal_a;int(*volatile y)(void)=equal_b;
if(result!=2||shared_counter!=3||a!=self_a||b!=self_b||a==b||x==y)return 71;
putchar(result);putchar(shared_counter);putchar(a==self_a);putchar(x!=y);return 0;}
"#,
        &[2, 3, 1, 1],
    );
}

#[test]
fn mutual_recursion_preserves_each_direct_call_and_canonical_entry() {
    let mut items = constants(600);
    for (id, target) in [(256, 512), (512, 256)] {
        items[id] = Item::Function(Function {
            name: format!("zil.fn.{id}"),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![Param::new(Ty::Int(32), "x")]),
            attrs: vec![],
            blocks: Some(vec![
                Block {
                    name: "entry".into(),
                    instructions: vec![
                        instruction(
                            "done",
                            Op::Compare {
                                predicate: "eq".into(),
                                ty: Ty::Int(32),
                                left: Val::Local("x".into()),
                                right: Val::Int(0),
                            },
                        ),
                        instruction(
                            "",
                            Op::CondBranch {
                                condition: Val::Local("done".into()),
                                yes: "base".into(),
                                no: "step".into(),
                            },
                        ),
                    ],
                },
                Block {
                    name: "base".into(),
                    instructions: vec![instruction(
                        "",
                        Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(7)))),
                    )],
                },
                Block {
                    name: "step".into(),
                    instructions: vec![
                        instruction(
                            "next",
                            Op::Binary {
                                opcode: "sub".into(),
                                ty: Ty::Int(32),
                                left: Val::Local("x".into()),
                                right: Val::Int(1),
                            },
                        ),
                        instruction(
                            "value",
                            call(
                                &format!("zil.fn.{target}"),
                                vec![argument(Ty::Int(32), "next")],
                            ),
                        ),
                        instruction(
                            "result",
                            Op::Binary {
                                opcode: "add".into(),
                                ty: Ty::Int(32),
                                left: Val::Local("value".into()),
                                right: Val::Int(1),
                            },
                        ),
                        instruction("", Op::Return(Some(argument(Ty::Int(32), "result")))),
                    ],
                },
            ]),
        });
    }
    items[0] = function(
        "zil.fn.0",
        vec![],
        vec![
            instruction(
                "value",
                call("zil.fn.256", vec![Arg::new(Ty::Int(32), Val::Int(5))]),
            ),
            instruction("", Op::Return(Some(argument(Ty::Int(32), "value")))),
        ],
    );
    execute(
        "recursion",
        items,
        600,
        r#"
#include <stdio.h>
extern int invoke(void) __asm__("zil.fn.0");
extern int canonical(int) __asm__("zil.fn.256");
int main(void){if(invoke()!=12)return 72;for(int x=0;x<64;x++){int y=canonical(x);if(y!=x+7)return 73;putchar(y);}return 0;}
"#,
        &(7..71).collect::<Vec<u8>>(),
    );
}

#[test]
fn direct_recursion_keeps_canonical_self_address_through_every_clone_call() {
    let mut items = constants(600);
    items[256] = Item::Function(Function {
        name: "zil.fn.256".into(),
        linkage: Linkage::External,
        signature: Signature::new(
            Ty::Int(32),
            vec![Param::new(Ty::Ptr, "p"), Param::new(Ty::Int(32), "n")],
        ),
        attrs: vec![],
        blocks: Some(vec![
            Block {
                name: "entry".into(),
                instructions: vec![
                    instruction(
                        "done",
                        Op::Compare {
                            predicate: "eq".into(),
                            ty: Ty::Int(32),
                            left: Val::Local("n".into()),
                            right: Val::Int(0),
                        },
                    ),
                    instruction(
                        "",
                        Op::CondBranch {
                            condition: Val::Local("done".into()),
                            yes: "base".into(),
                            no: "step".into(),
                        },
                    ),
                ],
            },
            Block {
                name: "base".into(),
                instructions: vec![
                    instruction(
                        "same",
                        Op::Compare {
                            predicate: "eq".into(),
                            ty: Ty::Ptr,
                            left: Val::Local("p".into()),
                            right: Val::Global("zil.fn.256".into()),
                        },
                    ),
                    instruction(
                        "result",
                        Op::Cast {
                            opcode: "zext".into(),
                            arg: argument(Ty::Int(1), "same"),
                            to: Ty::Int(32),
                        },
                    ),
                    instruction("", Op::Return(Some(argument(Ty::Int(32), "result")))),
                ],
            },
            Block {
                name: "step".into(),
                instructions: vec![
                    instruction(
                        "next",
                        Op::Binary {
                            opcode: "sub".into(),
                            ty: Ty::Int(32),
                            left: Val::Local("n".into()),
                            right: Val::Int(1),
                        },
                    ),
                    instruction(
                        "value",
                        call(
                            "zil.fn.256",
                            vec![argument(Ty::Ptr, "p"), argument(Ty::Int(32), "next")],
                        ),
                    ),
                    instruction("", Op::Return(Some(argument(Ty::Int(32), "value")))),
                ],
            },
        ]),
    });
    items[0] = function(
        "zil.fn.0",
        vec![],
        vec![
            instruction(
                "value",
                call(
                    "zil.fn.256",
                    vec![
                        Arg::new(Ty::Ptr, Val::Global("zil.fn.256".into())),
                        Arg::new(Ty::Int(32), Val::Int(7)),
                    ],
                ),
            ),
            instruction("", Op::Return(Some(argument(Ty::Int(32), "value")))),
        ],
    );
    execute(
        "recursive-address",
        items,
        600,
        r#"
#include <stdio.h>
typedef int(*Self)(void*,int);
extern int invoke(void) __asm__("zil.fn.0");
extern int canonical(void*,int) __asm__("zil.fn.256");
extern int other(void) __asm__("zil.fn.1");
int main(void){Self volatile f=canonical;int(*volatile g)(void)=other;
int a=invoke(),b=f((void*)f,64),c=f((void*)g,9);if(a!=1||b!=1||c!=0)return 74;
putchar(a);putchar(b);putchar(c);return 0;}
"#,
        &[1, 1, 0],
    );
}
