#![cfg(feature = "text-reference")]
use std::sync::atomic::{AtomicUsize, Ordering};
use zil_llvm_api::{Config, ErrorKind, Optimization, compile};
const GOOD: &str =
    "target triple = \"x86_64-unknown-linux-gnu\"\ndefine i32 @answer() { ret i32 42 }\n";
fn config() -> Config<'static> {
    Config {
        triple: "x86_64-unknown-linux-gnu",
        optimization: Optimization::O0,
    }
}
#[test]
fn real_elf_object_in_both_modes() {
    for opt in [Optimization::O0, Optimization::O2] {
        let o = compile(
            GOOD,
            &Config {
                optimization: opt,
                ..config()
            },
            &|| false,
        )
        .unwrap();
        assert!(o.bytes.starts_with(b"\x7fELF"));
        assert_eq!(o.receipt.llvm_version, "17.0.6");
        assert!(o.receipt.verify_before_after);
        assert!(o.receipt.data_layout.starts_with('e'));
        assert_eq!(o.receipt.cpu, "x86-64");
    }
}
#[test]
fn win64_object_uses_target_not_host_layout() {
    let ir = GOOD.replace("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc");
    let o = compile(
        &ir,
        &Config {
            triple: "x86_64-pc-windows-msvc",
            ..config()
        },
        &|| false,
    )
    .unwrap();
    assert_eq!(&o.bytes[..2], &[0x64, 0x86]);
    assert!(o.receipt.data_layout.contains("m:w"));
}
#[test]
fn parse_error_returns_no_object() {
    assert_eq!(
        compile("not valid IR", &config(), &|| false)
            .unwrap_err()
            .kind,
        ErrorKind::Parse
    );
}
#[test]
fn verifier_error_is_not_print_and_continue() {
    let ir = "target triple = \"x86_64-unknown-linux-gnu\"\ndefine i32 @bad(i1 %c) { entry: br i1 %c, label %a, label %b\na: %v = add i32 1, 2\n br label %b\nb: ret i32 %v\n}";
    assert_eq!(
        compile(ir, &config(), &|| false).unwrap_err().kind,
        ErrorKind::Verify
    );
}
#[test]
fn target_mismatch_rejected_before_optimization() {
    assert_eq!(
        compile(
            GOOD,
            &Config {
                triple: "x86_64-pc-windows-msvc",
                ..config()
            },
            &|| false
        )
        .unwrap_err()
        .kind,
        ErrorKind::Target
    );
}
#[test]
fn unimplemented_target_does_not_fall_back_to_host() {
    assert_eq!(
        compile(
            GOOD,
            &Config {
                triple: "aarch64-unknown-linux-gnu",
                ..config()
            },
            &|| false
        )
        .unwrap_err()
        .kind,
        ErrorKind::Target
    );
}
#[test]
fn contradictory_layout_cannot_be_overwritten() {
    let ir = format!("target datalayout = \"e-p:32:32\"\n{GOOD}");
    assert_eq!(
        compile(&ir, &config(), &|| false).unwrap_err().kind,
        ErrorKind::Target
    );
}
#[test]
fn cancellation_at_each_boundary_never_returns_an_object() {
    for at in 0..4 {
        let polls = AtomicUsize::new(0);
        let cancelled = || polls.fetch_add(1, Ordering::SeqCst) >= at;
        assert_eq!(
            compile(GOOD, &config(), &cancelled).unwrap_err().kind,
            ErrorKind::Cancelled
        );
    }
}
#[test]
fn independent_contexts_parallel_objects_are_deterministic() {
    let reference = compile(GOOD, &config(), &|| false).unwrap().bytes;
    std::thread::scope(|s| {
        for _ in 0..4 {
            let reference = &reference;
            s.spawn(move || {
                for _ in 0..8 {
                    assert_eq!(
                        &compile(GOOD, &config(), &|| false).unwrap().bytes,
                        reference
                    );
                }
            });
        }
    });
}
