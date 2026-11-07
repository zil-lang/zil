//! Actual Source tests for the qualified pure-value machine path. These await
//! the matching Enum/Array producer and independent consumers in static member.6.3.
use super::*;

fn qualified_array_program(with_abi: bool) -> String {
    let mut ty = "[Leaf<'s>;1]".to_owned();
    let mut initializers =
        String::from("let leaf:Leaf<'s>=#(make::<i32,7,'s>(i32));let a0:[Leaf<'s>;1]=[leaf];");
    for level in 1..=12 {
        ty = format!("[{ty};2]");
        initializers.push_str(&format!(
            "let a{level}:{ty}=[a{},a{}];",
            level - 1,
            level - 1
        ));
    }
    let first = "[0]".repeat(13);
    let (helper, body) = if with_abi {
        let helper_ty = ty.replace("'s", "'a");
        (
            format!(
                "fn keep<life a>(token:&'a i32,value:{helper_ty})->{helper_ty}{{let saved=value;return saved}}"
            ),
            format!("let returned=keep(token,a12);return returned{first}.bits"),
        )
    } else {
        (
            String::new(),
            format!(
                "let saved=a12;let pair=[saved,a12];let index=1;return pair[index]{first}.bits"
            ),
        )
    };
    format!(
        "struct Leaf<life a>{{bits:u64}}fn make<T:Copy,#N:i32,life a>(marker:Type)->Leaf<'a>{{return Leaf::<'a>{{bits:18446744073709551557u64}}}}{helper}fn read<life s>(token:&'s i32)->u64{{{initializers}{body}}}fn main()->u64{{let n=0;return read(&n)}}"
    )
}

fn qualified_native(text: &str, with_abi: bool) {
    use zil_driver::{NativeBackend, Optimization};
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = Temp::new();
        let report = compile_source(
            &source(text),
            &CompileOptions {
                backend: NativeBackend::Clang,
                optimization,
                entry: Some("main".into()),
                stop_after: Stage::Build,
                output: Some(out.0.join("program")),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
        assert!(llvm.contains("copy.value."), "missing private snapshot");
        assert!(
            llvm.contains("llvm.memcpy.p0.p0.i64"),
            "missing bounded copy"
        );
        assert!(
            !llvm.contains("@malloc"),
            "pure snapshots must not become heaps"
        );
        if with_abi {
            assert!(
                llvm.lines().any(
                    |line| line.starts_with("define ") && line.contains(".lc1(ptr %lc1.result")
                ),
                "missing versioned large result ABI"
            );
        }
        let observed = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert!(observed.status.success(), "{optimization:?}: {observed:?}");
        assert_eq!(observed.stdout, b"18446744073709551557\n");
        assert!(observed.stderr.is_empty());
    }
}

#[test]
fn qualified_array_snapshot_preserves_every_u64_bit() {
    qualified_native(&qualified_array_program(false), false);
}

#[test]
fn qualified_array_large_parameter_and_result_copy_in() {
    qualified_native(&qualified_array_program(true), true);
}

#[test]
fn old_ordinary_large_array_keeps_snapshot_and_abi() {
    let llvm = native_entry(
        include_str!("../../../codegen-pending/large-array-a18-by-value.zil"),
        "main",
        Some(0),
    );
    assert!(llvm.contains("copy.value."));
    assert!(llvm.contains("llvm.memcpy.p0.p0.i64"));
    assert!(llvm.contains(".lc1("));
}
