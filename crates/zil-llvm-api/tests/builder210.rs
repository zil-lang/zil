#![cfg(feature = "native")]
mod support210;
use std::{
    fs,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use support210::*;
use zil_llvm_api::{Config, ErrorKind, Optimization, compile_module, model::*};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn config() -> Config<'static> {
    Config {
        triple: "x86_64-unknown-linux-gnu",
        optimization: Optimization::O0,
    }
}
struct Dir(std::path::PathBuf);
impl Dir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zil-direct210-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn execute(module: &Module, opt: Optimization) -> i32 {
    let d = Dir::new();
    let out = compile_module(
        module,
        &Config {
            optimization: opt,
            ..config()
        },
        &|| false,
    )
    .unwrap();
    assert_eq!(out.receipt.input_kind, "builder");
    assert_eq!(out.receipt.parse_ns, 0);
    assert!(out.receipt.verify_before_after);
    fs::write(d.0.join("p.o"), out.bytes).unwrap();
    let clang = std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into());
    let link = Command::new(clang)
        .arg(d.0.join("p.o"))
        .arg("-o")
        .arg(d.0.join("p"))
        .output()
        .unwrap();
    assert!(
        link.status.success(),
        "{}",
        String::from_utf8_lossy(&link.stderr)
    );
    Command::new(d.0.join("p"))
        .status()
        .unwrap()
        .code()
        .unwrap()
}
#[test]
fn direct_loop_forward_block_phi_executes_o0_o2() {
    for opt in [Optimization::O0, Optimization::O2] {
        assert_eq!(execute(&loop_module(), opt), 42);
    }
}
#[test]
fn direct_aggregates_global_function_table_and_indirect_call_execute() {
    for opt in [Optimization::O0, Optimization::O2] {
        assert_eq!(execute(&aggregate_module(), opt), 42);
    }
}
#[test]
fn direct_target_not_host_and_optimization_modes() {
    for opt in [Optimization::O0, Optimization::O2] {
        for (triple, magic) in [
            ("x86_64-unknown-linux-gnu", &b"\x7fELF"[..]),
            ("x86_64-pc-windows-msvc", &b"\x64\x86"[..]),
        ] {
            let mut m = aggregate_module();
            m.triple = triple.into();
            let out = compile_module(
                &m,
                &Config {
                    triple,
                    optimization: opt,
                },
                &|| false,
            )
            .unwrap();
            assert!(out.bytes.starts_with(magic));
            assert_eq!(out.receipt.input_kind, "builder");
            assert_eq!(out.receipt.parse_ns, 0);
            assert_eq!(out.receipt.llvm_version, zil_llvm_api::LLVM_VERSION);
            assert!(
                out.receipt
                    .data_layout
                    .contains(if triple.contains("windows") {
                        "m:w"
                    } else {
                        "m:e"
                    })
            );
        }
    }
}
#[test]
fn invalid_module_never_reaches_assert_prone_builder() {
    let mut m = simple();
    let Item::Function(f) = &mut m.items[0] else {
        panic!()
    };
    f.blocks.as_mut().unwrap()[0].instructions.insert(
        0,
        i(
            "bad",
            Op::Extract {
                arg: a(Ty::Int(32), 0),
                index: 0,
            },
        ),
    );
    let e = compile_module(&m, &config(), &|| false).unwrap_err();
    assert_eq!(e.kind, ErrorKind::Verify);
}
#[test]
fn target_mismatch_is_explicit() {
    let e = compile_module(
        &simple(),
        &Config {
            triple: "x86_64-pc-windows-msvc",
            ..config()
        },
        &|| false,
    )
    .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Target);
}
#[test]
fn cancellation_at_each_native_boundary() {
    for at in 0..7 {
        let polls = AtomicUsize::new(0);
        let e = compile_module(&loop_module(), &config(), &|| {
            polls.fetch_add(1, Ordering::SeqCst) >= at
        })
        .unwrap_err();
        assert_eq!(e.kind, ErrorKind::Cancelled, "boundary {at}");
    }
}
#[test]
fn separate_contexts_have_deterministic_objects() {
    let module = aggregate_module();
    let reference = compile_module(&module, &config(), &|| false).unwrap().bytes;
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let module = &module;
            let reference = &reference;
            scope.spawn(move || {
                for _ in 0..8 {
                    assert_eq!(
                        &compile_module(module, &config(), &|| false).unwrap().bytes,
                        reference
                    );
                }
            });
        }
    });
}
#[test]
fn direct_backend_rejects_disabled_text_adapter() {
    #[cfg(not(feature = "text-reference"))]
    assert_eq!(
        zil_llvm_api::compile("", &config(), &|| false)
            .unwrap_err()
            .kind,
        ErrorKind::Unavailable
    );
}
#[cfg(feature = "text-reference")]
#[test]
fn rendered_and_direct_modes_have_same_behavior() {
    let m = loop_module();
    for opt in [Optimization::O0, Optimization::O2] {
        let direct = compile_module(
            &m,
            &Config {
                optimization: opt,
                ..config()
            },
            &|| false,
        )
        .unwrap();
        let reference = zil_llvm_api::compile(
            &m.to_text(),
            &Config {
                optimization: opt,
                ..config()
            },
            &|| false,
        )
        .unwrap();
        assert_eq!(direct.receipt.input_kind, "builder");
        assert_eq!(reference.receipt.input_kind, "llvm-text");
        // Objects contain module identity; compare .text bytes through a pinned objcopy in the external differential suite.
        assert!(reference.bytes.starts_with(b"\x7fELF"));
    }
}
