#![cfg(feature = "native")]
mod support210;

use std::{fs, process::Command};
use support210::*;
use zil_llvm_api::{Config, ErrorKind, Optimization, compile_module, model::*};

fn module(prefix: &str) -> Module {
    let name = |suffix: &str| format!("{prefix}.{suffix}");
    m(vec![
        Item::Global(Global {
            name: "cp06_seed".into(),
            linkage: Linkage::External,
            ty: Ty::Int(32),
            initializer: Some(Val::Int(41)),
            constant: false,
            unnamed_addr: false,
            alignment: 4,
        }),
        Item::Function(Function {
            name: "cp06_increment".into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![Param::unnamed(Ty::Int(32))]),
            attrs: vec![],
            blocks: None,
        }),
        Item::Function(f(
            "cp06_exported",
            vec![Param::new(Ty::Int(32), name("argument"))],
            Ty::Int(32),
            vec![b(
                &name("entry"),
                vec![
                    i(
                        &name("seed"),
                        Op::Load {
                            ty: Ty::Int(32),
                            ptr: global("cp06_seed".into()),
                            align: 4,
                        },
                    ),
                    i(
                        &name("incremented"),
                        Op::Call {
                            result: Ty::Int(32),
                            result_attrs: vec![],
                            callee: global("cp06_increment".into()),
                            args: vec![a(Ty::Int(32), local(name("argument")))],
                            vararg: None,
                        },
                    ),
                    i(
                        &name("answer"),
                        Op::Binary {
                            opcode: "add".into(),
                            ty: Ty::Int(32),
                            left: local(name("seed")),
                            right: local(name("incremented")),
                        },
                    ),
                    i("", Op::Return(Some(a(Ty::Int(32), local(name("answer")))))),
                ],
            )],
        )),
    ])
}

#[test]
fn local_renaming_keeps_objects_and_diagnostic_model_independent() {
    let mut left = module("readable");
    let mut right = module("different.$-prefix");
    for triple in ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"] {
        left.triple = triple.into();
        right.triple = triple.into();
        let before = left.to_text();
        assert_ne!(before, right.to_text());
        for optimization in [Optimization::O0, Optimization::O2] {
            let config = Config {
                triple,
                optimization,
            };
            let first = compile_module(&left, &config, &|| false).unwrap();
            let second = compile_module(&right, &config, &|| false).unwrap();
            assert_eq!(first.bytes, second.bytes);
            assert!(first.receipt.verify_before_after && second.receipt.verify_before_after);
            assert_eq!(before, left.to_text());
        }
    }
}

#[test]
fn global_exports_and_external_calls_link_with_the_c_abi() {
    let path = std::env::temp_dir().join(format!("zil-cp06-names-{}", std::process::id()));
    fs::create_dir(&path).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    fs::write(
        path.join("harness.c"),
        "extern int cp06_seed;\nextern int cp06_exported(int);\n\
         int cp06_increment(int x) { return x + 1; }\n\
         int main(void) { return cp06_seed == 41 && cp06_exported(0) == 42 ? 0 : 1; }\n",
    )
    .unwrap();
    for optimization in [Optimization::O0, Optimization::O2] {
        let object = compile_module(
            &module("named.locals"),
            &Config {
                triple: "x86_64-unknown-linux-gnu",
                optimization,
            },
            &|| false,
        )
        .unwrap();
        fs::write(path.join("unit.o"), object.bytes).unwrap();
        let link = Command::new(std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into()))
            .arg(path.join("unit.o"))
            .arg(path.join("harness.c"))
            .arg("-o")
            .arg(path.join("program"))
            .output()
            .unwrap();
        assert!(
            link.status.success(),
            "{}",
            String::from_utf8_lossy(&link.stderr)
        );
        let result = Command::new(path.join("program")).output().unwrap();
        assert!(result.status.success());
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
}

#[test]
fn discarded_native_names_do_not_weaken_model_name_validation() {
    for (name, message) in [
        ("named.argument", "duplicate SSA"),
        ("bad\0name", "invalid/duplicate SSA"),
    ] {
        let mut invalid = module("named");
        let Item::Function(function) = &mut invalid.items[2] else {
            unreachable!()
        };
        function.blocks.as_mut().unwrap()[0].instructions[0].result = Some(name.into());
        let error = compile_module(
            &invalid,
            &Config {
                triple: "x86_64-unknown-linux-gnu",
                optimization: Optimization::O0,
            },
            &|| false,
        )
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Verify);
        assert!(error.message.contains(message), "{error}");
    }
}
