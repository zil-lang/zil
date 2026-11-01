#![cfg(feature = "native")]
mod support210;
use std::{fs, process::Command};
use support210::*;
use zil_llvm_api::{Config, ErrorKind, FunctionMerging, Optimization, model::*};

fn body(name: &str) -> Function {
    f(
        name,
        vec![Param::new(Ty::Int(32), "x")],
        Ty::Int(32),
        vec![b(
            "entry",
            vec![
                i(
                    "q",
                    Op::Binary {
                        opcode: "sdiv".into(),
                        ty: Ty::Int(32),
                        left: Val::Local("x".into()),
                        right: Val::Int(3),
                    },
                ),
                i(
                    "m",
                    Op::Binary {
                        opcode: "mul".into(),
                        ty: Ty::Int(32),
                        left: Val::Local("q".into()),
                        right: Val::Int(5),
                    },
                ),
                i(
                    "r",
                    Op::Binary {
                        opcode: "srem".into(),
                        ty: Ty::Int(32),
                        left: Val::Local("m".into()),
                        right: Val::Int(97),
                    },
                ),
                i(
                    "n",
                    Op::Binary {
                        opcode: "add".into(),
                        ty: Ty::Int(32),
                        left: Val::Local("r".into()),
                        right: Val::Int(11),
                    },
                ),
                i("", Op::Return(Some(a(Ty::Int(32), "%n")))),
            ],
        )],
    )
}

#[test]
fn merging_preserves_distinct_addresses_exports_and_current_validation() {
    let call = |name: &str| Op::Call {
        result: Ty::Int(32),
        result_attrs: vec![],
        callee: global(name.into()),
        args: vec![a(Ty::Int(32), 7)],
        vararg: None,
    };
    let module = m(vec![
        Item::Function(body("alpha")),
        Item::Function(body("beta")),
        Item::Function(
            f(
                "different",
                vec![Param::new(Ty::Ptr, "a"), Param::new(Ty::Ptr, "b")],
                Ty::Int(32),
                vec![],
            )
            .declaration(),
        ),
        Item::Function(f(
            "main",
            vec![],
            Ty::Int(32),
            vec![b(
                "entry",
                vec![
                    i("a", call("alpha")),
                    i("b", call("beta")),
                    i(
                        "different",
                        Op::Call {
                            result: Ty::Int(32),
                            result_attrs: vec![],
                            callee: global("different".into()),
                            args: vec![
                                a(Ty::Ptr, global("alpha".into())),
                                a(Ty::Ptr, global("beta".into())),
                            ],
                            vararg: None,
                        },
                    ),
                    i(
                        "sum",
                        Op::Binary {
                            opcode: "add".into(),
                            ty: Ty::Int(32),
                            left: Val::Local("a".into()),
                            right: Val::Local("b".into()),
                        },
                    ),
                    i(
                        "result",
                        Op::Binary {
                            opcode: "add".into(),
                            ty: Ty::Int(32),
                            left: Val::Local("sum".into()),
                            right: Val::Local("different".into()),
                        },
                    ),
                    i("", Op::Return(Some(a(Ty::Int(32), "%result")))),
                ],
            )],
        )),
    ]);
    let dir = std::env::temp_dir().join(format!("zil-merge-functions-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    // Separate native C object observes escaped addresses. LLVM optimizing the
    // tested module cannot replace this comparison with a constant assumption.
    fs::write(
        dir.join("foreign.c"),
        "int different(int (*a)(int), int (*b)(int)) { return a != b; }\n",
    )
    .unwrap();
    let foreign = Command::new(std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into()))
        .args(["-O2", "-c"])
        .arg(dir.join("foreign.c"))
        .arg("-o")
        .arg(dir.join("foreign.o"))
        .output()
        .unwrap();
    assert!(
        foreign.status.success(),
        "{}",
        String::from_utf8_lossy(&foreign.stderr)
    );
    let config = Config {
        triple: "x86_64-unknown-linux-gnu",
        optimization: Optimization::O2,
    };
    for policy in [
        FunctionMerging::Disabled,
        FunctionMerging::AfterOptimization,
        FunctionMerging::BeforeAndAfter,
    ] {
        let object =
            zil_llvm_api::compile_module_with_function_merging(&module, &config, policy, &|| false)
                .unwrap();
        assert!(object.receipt.verify_before_after);
        assert_eq!(
            object.receipt.pipeline,
            zil_llvm_api::pipeline_identity(config.optimization, policy)
        );
        let objects = vec![object];
        #[cfg(feature = "text-reference")]
        let objects = {
            let mut objects = objects;
            objects.push(
                zil_llvm_api::compile_with_function_merging(
                    &module.to_text(),
                    &config,
                    policy,
                    &|| false,
                )
                .unwrap(),
            );
            objects
        };
        for object in objects {
            fs::write(dir.join("module.o"), object.bytes).unwrap();
            let link =
                Command::new(std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into()))
                    .arg(dir.join("module.o"))
                    .arg(dir.join("foreign.o"))
                    .arg("-o")
                    .arg(dir.join("program"))
                    .output()
                    .unwrap();
            assert!(
                link.status.success(),
                "{}",
                String::from_utf8_lossy(&link.stderr)
            );
            assert_eq!(
                Command::new(dir.join("program")).status().unwrap().code(),
                Some(43)
            );
            let nm = Command::new("nm")
                .args(["-g", "--defined-only"])
                .arg(dir.join("program"))
                .output()
                .unwrap();
            assert!(nm.status.success());
            let symbols = String::from_utf8(nm.stdout).unwrap();
            let address = |name: &str| {
                symbols
                    .lines()
                    .find_map(|line| {
                        let fields = line.split_whitespace().collect::<Vec<_>>();
                        (fields.len() == 3 && fields[2] == name).then(|| fields[0])
                    })
                    .unwrap()
            };
            assert_ne!(
                address("alpha"),
                address("beta"),
                "distinct exported function addresses must remain unique"
            );
            let disassembly = Command::new("objdump")
                .arg("-d")
                .arg(dir.join("program"))
                .output()
                .unwrap();
            assert!(disassembly.status.success());
            let disassembly = String::from_utf8(disassembly.stdout).unwrap();
            let thunk = ["alpha", "beta"].into_iter().any(|name| {
                disassembly
                    .split(&format!("<{name}>:"))
                    .nth(1)
                    .unwrap()
                    .split("\n\n")
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .any(|word| word == "jmp")
            });
            assert_eq!(
                thunk,
                policy != FunctionMerging::Disabled,
                "actual machine code must demonstrate the requested pass: {disassembly}"
            );
        }
        let mut malformed = module.clone();
        if let Item::Function(function) = &mut malformed.items[0] {
            function.blocks.as_mut().unwrap()[0].instructions[0] =
                i("", Op::Return(Some(a(Ty::Int(64), 0))));
        }
        assert_eq!(
            zil_llvm_api::compile_module_with_function_merging(
                &malformed,
                &config,
                policy,
                &|| false
            )
            .unwrap_err()
            .kind,
            ErrorKind::Verify
        );
        assert_eq!(
            zil_llvm_api::compile_module_with_function_merging(&module, &config, policy, &|| true)
                .unwrap_err()
                .kind,
            ErrorKind::Cancelled
        );
    }
}
