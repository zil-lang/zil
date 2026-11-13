use super::*;
use zil_base::Span;
use zil_ir::{
    BasicBlock, BlockId, FunctionId, Instruction, InstructionKind as I, LocalDecl, LocalId,
    PassingMode, Terminator, Type, ValueId,
};

fn local(n: &str) -> m::Val {
    m::local(n.into())
}
fn inst(n: &str, op: m::Op) -> m::Inst {
    m::Inst::new((!n.is_empty()).then(|| n.into()), op)
}
fn block(n: &str, instructions: Vec<m::Inst>) -> m::Block {
    m::Block {
        name: n.into(),
        instructions,
    }
}
fn ret(v: m::Val) -> m::Inst {
    inst("", m::Op::Return(Some(m::Arg::new(m::Ty::Int(32), v))))
}
fn alloca(s: usize) -> m::Inst {
    inst(
        &format!("local.{s}"),
        m::Op::Alloca {
            ty: m::Ty::Int(32),
            align: 4,
        },
    )
}
fn store(s: usize, v: m::Val) -> m::Inst {
    inst(
        "",
        m::Op::Store {
            arg: m::Arg::new(m::Ty::Int(32), v),
            ptr: local(&format!("local.{s}")),
            align: 4,
        },
    )
}
fn load(s: usize, n: &str) -> m::Inst {
    inst(
        n,
        m::Op::Load {
            ty: m::Ty::Int(32),
            ptr: local(&format!("local.{s}")),
            align: 4,
        },
    )
}
fn jump(n: &str) -> m::Inst {
    inst("", m::Op::Branch(n.into()))
}
fn binary(n: &str, op: &str, a: m::Val, b: m::Val) -> m::Inst {
    inst(
        n,
        m::Op::Binary {
            opcode: op.into(),
            ty: m::Ty::Int(32),
            left: a,
            right: b,
        },
    )
}
fn function(blocks: Vec<m::Block>) -> m::Function {
    m::Function {
        name: "zil.fn.0".into(),
        linkage: m::Linkage::External,
        signature: m::Signature::new(m::Ty::Int(32), vec![m::Param::new(m::Ty::Int(32), "arg.0")]),
        attrs: vec![],
        blocks: Some(blocks.into()),
    }
}
// Internal transformation guards, not Source acceptance evidence. The exact
// scalar Source inventory is given independently of machine CFG/phi placement.
fn source(locals: usize, loads: &[usize], stores: &[usize], parameter: bool) -> TypedFunction {
    let mut instructions = vec![];
    let mut result = 0u32;
    for &s in loads {
        instructions.push(Instruction {
            result: Some(ValueId(result)),
            kind: I::Load(LocalId(s as u32)),
            span: Span::default(),
        });
        result += 1;
    }
    for &s in stores {
        instructions.push(Instruction {
            result: None,
            kind: I::Store {
                local: LocalId(s as u32),
                value: ValueId(0),
            },
            span: Span::default(),
        });
    }
    TypedFunction {
        storage_life: None,
        call_kind: zil_ir::CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contract: None,
        id: FunctionId(0),
        name: "guard".into(),
        foreign: None,
        environment: None,
        parameters: if parameter { vec![LocalId(0)] } else { vec![] },
        return_type: Type::I32,
        result_mode: PassingMode::Infer,
        locals: (0..locals)
            .map(|i| LocalDecl {
                id: LocalId(i as u32),
                name: format!("l{i}"),
                ty: Type::I32,
                mutable: true,
                span: Span::default(),
            })
            .collect::<Vec<_>>()
            .into(),
        values: vec![Type::I32; usize::max(1, result as usize)].into(),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions,
            terminator: Terminator::Return(None),
        }]
        .into(),
        entry: BlockId(0),
        span: Span::default(),
    }
}
fn diamond(duplicate: bool) -> (TypedFunction, m::Function) {
    let mut entry = vec![alloca(0), store(0, m::Val::Int(1)), inst("flag",m::Op::Compare { predicate:"sgt".into(),ty:m::Ty::Int(32),left:local("arg.0"),right:m::Val::Int(0) })];
    entry.push(inst(
        "",
        m::Op::CondBranch {
            condition: local("flag"),
            yes: "left".into(),
            no: if duplicate { "left" } else { "right" }.into(),
        },
    ));
    let mut blocks = vec![
        block("entry", entry),
        block("left", vec![store(0, m::Val::Int(11)), jump("join")]),
    ];
    if !duplicate {
        blocks.push(block(
            "right",
            vec![store(0, m::Val::Int(19)), jump("join")],
        ));
    }
    blocks.push(block("join", vec![load(0, "answer"), ret(local("answer"))]));
    (
        source(1, &[0], if duplicate { &[0, 0] } else { &[0, 0, 0] }, false),
        function(blocks),
    )
}
fn memory_ops(f: &m::Function) -> usize {
    f.blocks
        .iter()
        .flatten()
        .flat_map(|b| &b.instructions)
        .filter(|i| {
            matches!(
                i.op,
                m::Op::Alloca { .. } | m::Op::Load { .. } | m::Op::Store { .. }
            )
        })
        .count()
}

#[test]
fn pruned_ssa_diamond_loops_and_duplicate_edges_keep_actual_values() {
    let (s, mut f) = diamond(false);
    let w = run(&s, &mut f, DEFAULT_FUEL).unwrap();
    assert_eq!(w.slots, 1);
    assert_eq!(memory_ops(&f), 0);
    assert_eq!(f.blocks.as_ref().unwrap().len(),1);
    assert!(f.blocks.as_ref().unwrap()[0].instructions.iter().any(|i| matches!(&i.op,m::Op::Select {ty:m::Ty::Int(32),condition:m::Val::Local(flag),yes:m::Val::Int(11),no:m::Val::Int(19)} if flag=="flag")));
    let (s, mut f) = diamond(true);
    run(&s, &mut f, DEFAULT_FUEL).unwrap();
    assert_eq!(memory_ops(&f), 0);
    // Both duplicate entry edges carry the same state; no useless phi required.
    assert!(
        !f.blocks
            .iter()
            .flatten()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.op, m::Op::Phi { .. }))
    );
    let (s, mut f) = diamond(false);
    f.blocks.as_mut().unwrap()[1].instructions[1] = inst(
        "",
        m::Op::CondBranch {
            condition: local("flag"),
            yes: "join".into(),
            no: "join".into(),
        },
    );
    run(&s, &mut f, DEFAULT_FUEL).unwrap();
    assert!(
        matches!(&f.blocks.as_ref().unwrap()[3].instructions[0].op,m::Op::Phi {incoming,..} if incoming==&vec![(m::Val::Int(11),"left".into()),(m::Val::Int(11),"left".into()),(m::Val::Int(19),"right".into())])
    );
    let (s, mut f) = loop_model();
    run(&s, &mut f, DEFAULT_FUEL).unwrap();
    assert_eq!(memory_ops(&f), 0);
    let header = &f.blocks.as_ref().unwrap()[1];
    assert_eq!(
        header
            .instructions
            .iter()
            .filter(|i| matches!(i.op, m::Op::Phi { .. }))
            .count(),
        2
    );
    // A loop-carried unknown range must leave this actual checked operation.
    assert_eq!(overflow_calls(&f), 1);
}
#[test]
fn fallback_preserves_uninitialized_unreachable_escaping_atomic_and_fuel_inputs() {
    let (s, f) = diamond(false);
    let mut completed = false;
    // Cross the transactional boundary at every fuel limit, including failures
    // after speculative SSA mutation but before range/alias processing finishes.
    for fuel in 0..2048 {
        let mut x = f.clone();
        if let Some(work) = run(&s, &mut x, fuel) {
            assert_eq!(work.slots, 1);
            completed = true;
            break;
        }
        assert_eq!(x, f, "fuel {fuel}");
    }
    assert!(completed);
    let mut uninitialized = f.clone();
    uninitialized.blocks.as_mut().unwrap()[2]
        .instructions
        .remove(0);
    // Remove entry initialization too: the right edge has no definition.
    uninitialized.blocks.as_mut().unwrap()[0]
        .instructions
        .remove(1);
    let s2 = source(1, &[0], &[0], false);
    let before = uninitialized.clone();
    run(&s2, &mut uninitialized, DEFAULT_FUEL).unwrap();
    assert_eq!(uninitialized, before);
    for atomic in [false, true] {
        let mut x = f.clone();
        let extra = if atomic {
            inst(
                "atomic",
                m::Op::AtomicLoad {
                    ty: m::Ty::Int(32),
                    ptr: local("local.0"),
                    align: 4,
                    ordering: m::AtomicOrdering::Relaxed,
                },
            )
        } else {
            inst(
                "",
                m::Op::Call {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    callee: m::global("escape".into()),
                    args: vec![m::Arg::new(m::Ty::Ptr, local("local.0"))],
                    vararg: None,
                },
            )
        };
        x.blocks.as_mut().unwrap()[0].instructions.insert(2, extra);
        let before = x.clone();
        run(&s, &mut x, DEFAULT_FUEL).unwrap();
        assert_eq!(x, before);
    }
    let mut x = f.clone();
    x.blocks.as_mut().unwrap().push(block(
        "dead",
        vec![load(0, "deadvalue"), ret(local("deadvalue"))],
    ));
    let s = source(1, &[0, 0], &[0, 0, 0], false);
    let mut reachable_only = x.clone();
    reachable_only.blocks.as_mut().unwrap().pop();
    run(&s, &mut x, DEFAULT_FUEL).unwrap();
    assert_eq!(x, reachable_only);
}
fn checked(n: &str, kind: &str, width: u32, a: m::Val, b: m::Val) -> m::Inst {
    inst(
        n,
        m::Op::Call {
            result: m::Ty::Struct(vec![m::Ty::Int(width), m::Ty::Int(1)]),
            result_attrs: vec![],
            callee: m::global(format!("llvm.{kind}.with.overflow.i{width}")),
            args: vec![
                m::Arg::new(m::Ty::Int(width), a),
                m::Arg::new(m::Ty::Int(width), b),
            ],
            vararg: None,
        },
    )
}
fn extract(n: &str, p: &str, index: u32, width: u32) -> m::Inst {
    inst(
        n,
        m::Op::Extract {
            arg: m::Arg::new(
                m::Ty::Struct(vec![m::Ty::Int(width), m::Ty::Int(1)]),
                local(p),
            ),
            index,
        },
    )
}
fn overflow_calls(f: &m::Function) -> usize {
    f.blocks
        .iter()
        .flatten()
        .flat_map(|b| &b.instructions)
        .filter(|i| ranges::checked_for_test(&i.op))
        .count()
}
fn checked_model() -> m::Function {
    function(vec![
        block(
            "entry",
            vec![
                checked("first", "sadd", 32, local("arg.0"), m::Val::Int(3)),
                extract("x", "first", 0, 32),
                extract("bad", "first", 1, 32),
                inst(
                    "",
                    m::Op::CondBranch {
                        condition: local("bad"),
                        yes: "trap".into(),
                        no: "body".into(),
                    },
                ),
            ],
        ),
        block(
            "body",
            vec![
                binary("r", "srem", local("x"), m::Val::Int(251)),
                checked("bounded", "smul", 32, local("r"), m::Val::Int(2)),
                extract("answer", "bounded", 0, 32),
                extract("bad2", "bounded", 1, 32),
                inst(
                    "",
                    m::Op::CondBranch {
                        condition: local("bad2"),
                        yes: "trap".into(),
                        no: "exit".into(),
                    },
                ),
            ],
        ),
        block("exit", vec![ret(local("answer"))]),
        block(
            "trap",
            vec![
                inst(
                    "",
                    m::Op::Call {
                        result: m::Ty::Void,
                        result_attrs: vec![],
                        callee: m::global("llvm.trap".into()),
                        args: vec![],
                        vararg: None,
                    },
                ),
                inst("", m::Op::Unreachable),
            ],
        ),
    ])
}
fn loop_model() -> (TypedFunction, m::Function) {
    let f = function(vec![
        block(
            "entry",
            vec![
                alloca(0),
                alloca(1),
                store(0, local("arg.0")),
                store(1, m::Val::Int(0)),
                jump("header"),
            ],
        ),
        block(
            "header",
            vec![
                load(0, "n"),
                inst(
                    "more",
                    m::Op::Compare {
                        predicate: "sgt".into(),
                        ty: m::Ty::Int(32),
                        left: local("n"),
                        right: m::Val::Int(0),
                    },
                ),
                inst(
                    "",
                    m::Op::CondBranch {
                        condition: local("more"),
                        yes: "body".into(),
                        no: "exit".into(),
                    },
                ),
            ],
        ),
        block(
            "body",
            vec![
                load(1, "a"),
                checked("pair", "sadd", 32, local("a"), m::Val::Int(7)),
                extract("sum", "pair", 0, 32),
                extract("overflow", "pair", 1, 32),
                inst(
                    "",
                    m::Op::CondBranch {
                        condition: local("overflow"),
                        yes: "trap".into(),
                        no: "tail".into(),
                    },
                ),
            ],
        ),
        block(
            "tail",
            vec![
                binary("r", "srem", local("sum"), m::Val::Int(251)),
                store(1, local("r")),
                binary("next", "sub", local("n"), m::Val::Int(1)),
                store(0, local("next")),
                jump("header"),
            ],
        ),
        block("exit", vec![load(1, "out"), ret(local("out"))]),
        block(
            "trap",
            vec![
                inst(
                    "",
                    m::Op::Call {
                        result: m::Ty::Void,
                        result_attrs: vec![],
                        callee: m::global("llvm.trap".into()),
                        args: vec![],
                        vararg: None,
                    },
                ),
                inst("", m::Op::Unreachable),
            ],
        ),
    ]);
    (source(2, &[0, 1, 1], &[1, 1, 0], true), f)
}
#[test]
fn range_removes_only_proven_overflow_and_preserves_unknown_tuple_and_poison_uses() {
    let s = source(0, &[], &[], false);
    let mut f = checked_model();
    let w = run(&s, &mut f, DEFAULT_FUEL).unwrap();
    assert_eq!(w.checks, 1);
    assert_eq!(overflow_calls(&f), 1);
    assert!(!f.blocks.iter().flatten().flat_map(|b|&b.instructions).any(|i| matches!(&i.op,m::Op::CondBranch {condition:m::Val::Bool(false),..})));
    assert!(
        f.blocks.as_ref().unwrap()[1]
            .instructions
            .iter()
            .any(|i| matches!(&i.op,m::Op::Binary {opcode,..} if opcode=="mul"))
    );
    for poison in [m::Val::Poison, m::Val::Undef] {
        let mut x = checked_model();
        if let m::Op::Call { args, .. } = &mut x.blocks.as_mut().unwrap()[0].instructions[0].op {
            args[0].value = poison;
        }
        run(&s, &mut x, DEFAULT_FUEL).unwrap();
        assert_eq!(overflow_calls(&x), 2);
    }
    let mut x = checked_model();
    x.blocks.as_mut().unwrap()[1].instructions.insert(
        1,
        inst(
            "frozen",
            m::Op::Freeze(m::Arg::new(m::Ty::Int(32), local("r"))),
        ),
    );
    if let m::Op::Call { args, .. } = &mut x.blocks.as_mut().unwrap()[1].instructions[2].op {
        args[0].value = local("frozen");
    }
    run(&s, &mut x, DEFAULT_FUEL).unwrap();
    assert_eq!(overflow_calls(&x), 2);
    let mut x = checked_model();
    x.blocks.as_mut().unwrap()[1].instructions.insert(
        2,
        inst(
            "",
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global("observe_tuple".into()),
                args: vec![m::Arg::new(
                    m::Ty::Struct(vec![m::Ty::Int(32), m::Ty::Int(1)]),
                    local("bounded"),
                )],
                vararg: None,
            },
        ),
    );
    run(&s, &mut x, DEFAULT_FUEL).unwrap();
    assert_eq!(overflow_calls(&x), 2);
}
#[test]
fn integer_extremes_never_use_host_overflow_or_assume_nowrap_flags() {
    let s = source(0, &[], &[], false);
    for (kind, width, a, b, safe) in [
        ("sadd", 8, 127, 1, false),
        ("sadd", 8, -128, -1, false),
        ("smul", 64, i64::MIN as i128, -1, false),
        ("smul", 64, i64::MIN as i128, 1, true),
        ("uadd", 64, u64::MAX as i128, 1, false),
        ("umul", 64, u64::MAX as i128, u64::MAX as i128, false),
        ("umul", 64, u64::MAX as i128, 1, true),
        ("usub", 32, 0, 1, false),
        ("ssub", 32, i32::MIN as i128, 1, false),
        ("sadd", 1, -1, 0, true),
    ] {
        let mut f = function(vec![block(
            "entry",
            vec![
                checked("pair", kind, width, m::Val::Int(a), m::Val::Int(b)),
                extract("answer", "pair", 0, width),
                extract("overflow", "pair", 1, width),
                ret(m::Val::Int(0)),
            ],
        )]);
        let w = run(&s, &mut f, DEFAULT_FUEL).unwrap();
        assert_eq!(w.checks, usize::from(safe), "{kind} i{width} {a} {b}");
        assert!(f.blocks.iter().flatten().flat_map(|b|&b.instructions).all(|i|!matches!(&i.op,m::Op::Binary{opcode,..} if opcode.contains("nsw")||opcode.contains("nuw"))));
    }
}

#[cfg(all(feature = "llvm-api", target_os = "linux", target_arch = "x86_64"))]
#[test]
fn actual_native_o0_o2_matches_unoptimized_models_for_loops_extremes_and_traps() {
    use std::{fs, process::Command};
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "zil-early-ssa-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    let clang = std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into());
    let c = dir.join("oracle.c");
    fs::write(&c,"#include <stdio.h>\n#include <stdlib.h>\n#include <stdint.h>\nextern int32_t a(int32_t) __asm__(\"zil.fn.0\");\nint main(int c,char**v){if(c!=2)return 90;printf(\"%d\\n\",a((int32_t)strtol(v[1],0,10)));return 0;}\n").unwrap();
    for (case, (s, original), inputs) in [
        ("loop", loop_model(), vec![-1, 0, 1, 7, 50]),
        (
            "range",
            (source(0, &[], &[], false), checked_model()),
            vec![i32::MIN, -7, -1, 0, 100, i32::MAX - 3, i32::MAX],
        ),
    ] {
        let mut optimized = original.clone();
        run(&s, &mut optimized, DEFAULT_FUEL).unwrap();
        assert_ne!(optimized, original);
        for opt in [
            zil_llvm_api::Optimization::O0,
            zil_llvm_api::Optimization::O2,
        ] {
            let mut executables = vec![];
            for (version, f) in [("before", original.clone()), ("after", optimized.clone())] {
                let declaration = |name: &str, width: u32| {
                    m::Item::Function(m::Function {
                        name: name.into(),
                        linkage: m::Linkage::External,
                        signature: m::Signature::new(
                            m::Ty::Struct(vec![m::Ty::Int(width), m::Ty::Int(1)]),
                            vec![
                                m::Param::unnamed(m::Ty::Int(width)),
                                m::Param::unnamed(m::Ty::Int(width)),
                            ],
                        ),
                        attrs: vec![],
                        blocks: None,
                    })
                };
                let module = m::Module {
                    triple: "x86_64-unknown-linux-gnu".into(),
                    fingerprint: "early-scalar-equivalence".into(),
                    items: vec![
                        m::Item::Function(m::Function {
                            name: "llvm.trap".into(),
                            linkage: m::Linkage::External,
                            signature: m::Signature::new(m::Ty::Void, vec![]),
                            attrs: vec![],
                            blocks: None,
                        }),
                        declaration("llvm.sadd.with.overflow.i32", 32),
                        declaration("llvm.smul.with.overflow.i32", 32),
                        m::Item::Function(f),
                    ]
                    .into(),
                };
                zil_llvm_api::validate_module(&module).unwrap();
                let object = zil_llvm_api::compile_module(
                    &module,
                    &zil_llvm_api::Config {
                        triple: "x86_64-unknown-linux-gnu",
                        optimization: opt,
                    },
                    &|| false,
                )
                .unwrap();
                assert!(object.receipt.verify_before_after);
                let file = dir.join(format!("{case}-{opt:?}-{version}.o"));
                fs::write(&file, object.bytes).unwrap();
                let exe = file.with_extension("program");
                let linked = Command::new(&clang)
                    .arg(&c)
                    .arg(file)
                    .arg("-o")
                    .arg(&exe)
                    .output()
                    .unwrap();
                assert!(
                    linked.status.success(),
                    "{}",
                    String::from_utf8_lossy(&linked.stderr)
                );
                executables.push(exe);
            }
            for x in &inputs {
                let before = Command::new(&executables[0])
                    .arg(x.to_string())
                    .output()
                    .unwrap();
                let after = Command::new(&executables[1])
                    .arg(x.to_string())
                    .output()
                    .unwrap();
                assert_eq!(before.status, after.status, "{case} {opt:?} {x}");
                assert_eq!(before.stdout, after.stdout);
                assert_eq!(before.stderr, after.stderr);
                if case == "range" && *x == i32::MAX {
                    assert!(!after.status.success());
                } else {
                    let expected = if case == "loop" {
                        i64::from((*x).max(0)) * 7 % 251
                    } else {
                        (i64::from(*x) + 3) % 251 * 2
                    };
                    assert!(after.status.success());
                    assert_eq!(after.stdout, format!("{expected}\n").as_bytes());
                }
            }
        }
    }
}
