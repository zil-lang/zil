#![cfg(feature = "native")]
mod support210;
use std::{
    fs,
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
use support210::*;
use zil_llvm_api::{Config, Optimization, compile_module, model::*};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn run(module: &Module, optimization: Optimization) -> Output {
    let path = std::env::temp_dir().join(format!(
        "zil-native211-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    let object = compile_module(
        module,
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
        .arg("-o")
        .arg(path.join("program"))
        .output()
        .unwrap();
    assert!(
        link.status.success(),
        "{}",
        String::from_utf8_lossy(&link.stderr)
    );
    Command::new(path.join("program")).output().unwrap()
}
#[test]
fn reused_direct_call_type_keeps_narrow_attributes_and_function_local_scopes() {
    let mut narrow = f(
        "narrow",
        vec![Param::new(Ty::Int(8), "arg")],
        Ty::Int(8),
        vec![b(
            "entry",
            vec![
                i("result", Op::Freeze(a(Ty::Int(8), "%arg"))),
                i("", Op::Return(Some(a(Ty::Int(8), "%result")))),
            ],
        )],
    );
    narrow.signature.result_attrs = vec![Attr::ZeroExt];
    narrow.signature.params[0].attrs = vec![Attr::ZeroExt];
    let wide = f(
        "wide",
        vec![Param::new(Ty::Int(32), "arg")],
        Ty::Int(32),
        vec![b(
            "entry",
            vec![
                i("result", Op::Freeze(a(Ty::Int(32), "%arg"))),
                i("", Op::Return(Some(a(Ty::Int(32), "%result")))),
            ],
        )],
    );
    let module = m(vec![
        Item::Function(narrow),
        Item::Function(wide),
        Item::Function(f(
            "main",
            vec![],
            Ty::Int(32),
            vec![b(
                "entry",
                vec![
                    i(
                        "one",
                        Op::Call {
                            result: Ty::Int(8),
                            result_attrs: vec![Attr::ZeroExt],
                            callee: global("narrow".into()),
                            args: vec![a(Ty::Int(8), 42).attributed(vec![Attr::ZeroExt])],
                            vararg: None,
                        },
                    ),
                    i(
                        "two",
                        Op::Call {
                            result: Ty::Int(8),
                            result_attrs: vec![Attr::ZeroExt],
                            callee: global("narrow".into()),
                            args: vec![a(Ty::Int(8), "%one").attributed(vec![Attr::ZeroExt])],
                            vararg: None,
                        },
                    ),
                    i(
                        "widevalue",
                        Op::Cast {
                            opcode: "zext".into(),
                            arg: a(Ty::Int(8), "%two"),
                            to: Ty::Int(32),
                        },
                    ),
                    i("pointer", Op::Freeze(a(Ty::Ptr, global("wide".into())))),
                    i(
                        "answer",
                        Op::Call {
                            result: Ty::Int(32),
                            result_attrs: vec![],
                            callee: val("%pointer"),
                            args: vec![a(Ty::Int(32), "%widevalue")],
                            vararg: None,
                        },
                    ),
                    i("", Op::Return(Some(a(Ty::Int(32), "%answer")))),
                ],
            )],
        )),
    ]);
    for opt in [Optimization::O0, Optimization::O2] {
        let out = run(&module, opt);
        assert_eq!(out.status.code(), Some(42));
        assert!(out.stdout.is_empty() && out.stderr.is_empty());
    }
}
#[test]
fn cached_variadic_function_type_preserves_fixed_and_extra_arguments() {
    let printf = Function {
        name: "printf".into(),
        linkage: Linkage::External,
        signature: Signature {
            result: Ty::Int(32),
            result_attrs: vec![],
            params: vec![Param::unnamed(Ty::Ptr)],
            variadic: true,
        },
        attrs: vec![],
        blocks: None,
    };
    let module = m(vec![
        Item::Global(Global {
            name: "format".into(),
            linkage: Linkage::Private,
            ty: Ty::array(4, Ty::Int(8)),
            initializer: Some(Val::Bytes(b"%d\n\0".to_vec())),
            constant: true,
            unnamed_addr: true,
            alignment: 1,
        }),
        Item::Function(printf),
        Item::Function(f(
            "main",
            vec![],
            Ty::Int(32),
            vec![b(
                "entry",
                vec![
                    i(
                        "printed",
                        Op::Call {
                            result: Ty::Int(32),
                            result_attrs: vec![],
                            callee: global("printf".into()),
                            args: vec![a(Ty::Ptr, global("format".into())), a(Ty::Int(32), 42)],
                            vararg: Some(vec![Ty::Ptr]),
                        },
                    ),
                    i("", Op::Return(Some(a(Ty::Int(32), 0)))),
                ],
            )],
        )),
    ]);
    for opt in [Optimization::O0, Optimization::O2] {
        let out = run(&module, opt);
        assert!(out.status.success());
        assert_eq!(out.stdout, b"42\n");
        assert!(out.stderr.is_empty());
    }
}
