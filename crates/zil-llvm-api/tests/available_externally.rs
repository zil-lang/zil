mod support210;
use support210::*;
use zil_llvm_api::{model::*, validate_module};

fn imported() -> Function {
    let mut function = f(
        "answer",
        vec![],
        Ty::Int(32),
        vec![b(
            "entry",
            vec![i("", Op::Return(Some(a(Ty::Int(32), 42))))],
        )],
    );
    function.linkage = Linkage::AvailableExternally;
    function
}

#[test]
fn available_body_requires_current_valid_body_and_is_not_local() {
    let function = imported();
    assert!(!function.linkage.is_local());
    let module = m(vec![Item::Function(function.clone())]);
    validate_module(&module).unwrap();
    assert!(module.to_text().contains("available_externally"));
    let mut missing = function.clone();
    missing.blocks = None;
    assert!(validate_module(&m(vec![Item::Function(missing)])).is_err());
    let mut malformed = function;
    malformed.blocks.as_mut().unwrap()[0].instructions[0] =
        i("", Op::Return(Some(a(Ty::Int(32), "%missing"))));
    assert!(validate_module(&m(vec![Item::Function(malformed)])).is_err());
    assert!(
        validate_module(&m(vec![Item::Global(Global {
            name: "unsupported".into(),
            linkage: Linkage::AvailableExternally,
            ty: Ty::Int(32),
            initializer: Some(Val::Int(42)),
            constant: true,
            unnamed_addr: false,
            alignment: 4,
        })]))
        .is_err()
    );
}

#[cfg(feature = "native")]
#[test]
fn imported_body_never_emits_a_second_symbol_and_retains_canonical_address() {
    use std::{fs, process::Command};
    use zil_llvm_api::{Config, Optimization, compile_module};
    let mut canonical = imported();
    canonical.linkage = Linkage::External;
    let get_address = f(
        "get_address",
        vec![],
        Ty::Ptr,
        vec![b(
            "entry",
            vec![i("", Op::Return(Some(a(Ty::Ptr, global("answer".into())))))],
        )],
    );
    let owner = m(vec![
        Item::Function(canonical),
        Item::Function(get_address.clone()),
    ]);
    let caller = m(vec![
        Item::Function(imported()),
        Item::Function(get_address.declaration()),
        Item::Function(f(
            "main",
            vec![],
            Ty::Int(32),
            vec![b(
                "entry",
                vec![
                    i(
                        "result",
                        Op::Call {
                            result: Ty::Int(32),
                            result_attrs: vec![],
                            callee: global("answer".into()),
                            args: vec![],
                            vararg: None,
                        },
                    ),
                    i(
                        "address",
                        Op::Call {
                            result: Ty::Ptr,
                            result_attrs: vec![],
                            callee: global("get_address".into()),
                            args: vec![],
                            vararg: None,
                        },
                    ),
                    i(
                        "equal",
                        Op::Compare {
                            predicate: "eq".into(),
                            ty: Ty::Ptr,
                            left: Val::Local("address".into()),
                            right: global("answer".into()),
                        },
                    ),
                    i(
                        "equal32",
                        Op::Cast {
                            opcode: "zext".into(),
                            arg: a(Ty::Int(1), "%equal"),
                            to: Ty::Int(32),
                        },
                    ),
                    i(
                        "both",
                        Op::Binary {
                            opcode: "add".into(),
                            ty: Ty::Int(32),
                            left: Val::Local("result".into()),
                            right: Val::Local("equal32".into()),
                        },
                    ),
                    i("", Op::Return(Some(a(Ty::Int(32), "%both")))),
                ],
            )],
        )),
    ]);
    let dir = std::env::temp_dir().join(format!("zil-import-address-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());
    for optimization in [Optimization::O0, Optimization::O2] {
        let config = Config {
            triple: "x86_64-unknown-linux-gnu",
            optimization,
        };
        for (name, module) in [("owner", &owner), ("caller", &caller)] {
            let object = compile_module(module, &config, &|| false).unwrap();
            assert!(object.receipt.verify_before_after);
            fs::write(dir.join(format!("{name}.o")), object.bytes).unwrap();
        }
        let symbols = Command::new("nm")
            .arg("--defined-only")
            .arg(dir.join("caller.o"))
            .output()
            .unwrap();
        assert!(symbols.status.success());
        assert!(
            !String::from_utf8_lossy(&symbols.stdout)
                .lines()
                .any(|line| line.split_whitespace().last() == Some("answer"))
        );
        let clang = std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into());
        let link = Command::new(clang)
            .arg(dir.join("caller.o"))
            .arg(dir.join("owner.o"))
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
        #[cfg(feature = "text-reference")]
        {
            let reference = zil_llvm_api::compile(&caller.to_text(), &config, &|| false).unwrap();
            assert!(reference.receipt.verify_before_after);
            fs::write(dir.join("caller.o"), reference.bytes).unwrap();
            let link =
                Command::new(std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into()))
                    .arg(dir.join("caller.o"))
                    .arg(dir.join("owner.o"))
                    .arg("-o")
                    .arg(dir.join("reference"))
                    .output()
                    .unwrap();
            assert!(
                link.status.success(),
                "{}",
                String::from_utf8_lossy(&link.stderr)
            );
            assert_eq!(
                Command::new(dir.join("reference")).status().unwrap().code(),
                Some(43)
            );
        }
    }
}
