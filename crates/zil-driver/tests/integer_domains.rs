//! Complete scalar domains through Source, independent raw/Wire Verify and native code.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{Constant, InstructionKind, Type};

const CONSUMER: &str = include_str!("../../../tests/integers/complete-domains.zil");
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "integer-domains.zil", text)
}
fn check(text: &str) {
    let r = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
    assert_eq!(r.verification_runs, 1);
}
fn raw(text: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let s = source(text);
    let ast = zil_syntax::parse(&s).into_result().unwrap();
    let core = zil_expand::expand(&s, &ast).unwrap();
    let module = zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    (module, plan)
}
fn backends() -> Vec<NativeBackend> {
    #[allow(unused_mut)]
    let mut out = vec![NativeBackend::Clang];
    #[cfg(feature = "llvm-api")]
    out.push(NativeBackend::LlvmApi);
    out
}
fn native(text: &str, entry: Option<&str>, stdout: &[u8], success: bool) {
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join(format!(
                    "integer-domains-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(&root).unwrap();
            fs::write(root.join("input.zil"), text).unwrap();
            let report = compile_source(
                &source(text),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: entry.map(str::to_owned),
                    output: Some(root.join("program")),
                    emit_dir: Some(root.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(root.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let result = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(root.join("stdout"), &result.stdout).unwrap();
            fs::write(root.join("stderr"), &result.stderr).unwrap();
            if success {
                assert!(result.status.success(), "{root:?}: {result:?}");
                assert_eq!(result.stdout, stdout);
            } else {
                assert!(!result.status.success(), "trap was omitted at {root:?}");
            }
        }
    }
}
#[test]
fn complete_domains_source_and_ct() {
    check(CONSUMER);
}
#[test]
fn complete_domains_native_o0_o2() {
    native(CONSUMER, Some("result"), b"0\n", true);
}
#[test]
fn all_integer_conversion_pairs_preserve_math_value() {
    let domains = [
        "u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize",
    ];
    let mut text = String::from("fn result()->i32{");
    for (i, from) in domains.iter().enumerate() {
        text.push_str(&format!("let a{i}:{from}=42;"));
        for to in domains {
            text.push_str(&format!("if {to}(a{i})!=42{to}{{return 1}}"));
        }
    }
    text.push_str("return 0}");
    check(&text);
    native(&text, Some("result"), b"0\n", true);
}
#[test]
fn literal_range_mixed_type_and_reserved_conversion_rejections() {
    for text in [
        "fn main(){let n:i8=128}",
        "fn main(){let n:i8=-129}",
        "fn main(){let n:i16=32768}",
        "fn main(){let n:i16=-32769}",
        "fn main(){let n:u32=4294967296}",
        "fn main(){let n:u32=-1}",
        "fn main(){let n:isize=9223372036854775808}",
        "fn main(){let n:isize=-9223372036854775809}",
        "fn main(){let n=1i8+1i16}",
        "fn main(){let n=1u32+1i32}",
        "fn main(){let n=u32(true)}",
        "fn main(){let n=i8(1,2)}",
        "fn u32()->i32{return 0}",
        "fn main(){let isize=0}",
    ] {
        let r = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            r.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            r.to_json()
        );
        assert_eq!(r.verification_runs, 0);
        assert!(!r.artifacts.contains_key("executable"));
    }
}
#[test]
fn checked_ct_rejects_overflow_bad_shift_and_bad_conversions() {
    for expression in [
        "127i8+1i8",
        "-128i8-1i8",
        "32767i16+1i16",
        "4294967295u32+1u32",
        "0u32-1u32",
        "9223372036854775807isize+1isize",
        "1i8<<8i8",
        "1i16>>-1i16",
        "1u32<<32u32",
        "1isize>>64isize",
        "u32(-1i8)",
        "i8(128u32)",
        "i16(-32769isize)",
        "i8(42.5)",
        "u32(4294967296.0)",
    ] {
        let text = format!("fn main(){{let n=#({expression})}}");
        let r = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            r.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            r.to_json()
        );
        assert!(!r.artifacts.contains_key("executable"));
    }
}
#[test]
fn raw_wire_reconstructs_exact_integer_identity() {
    for (ty, constant) in [
        (Type::U32, Constant::U32(u32::MAX)),
        (Type::I8, Constant::I8(i8::MIN)),
        (Type::I16, Constant::I16(i16::MIN)),
        (Type::Isize, Constant::Isize(i64::MIN)),
    ] {
        let (module, plan) = raw(&format!("fn value()->{}{{return 0}}", ty.name()));
        let mut module = module;
        let instruction = module.functions[0]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::Constant(_)))
            .unwrap();
        instruction.kind = InstructionKind::Constant(constant.clone());
        let mut plan = plan;
        plan.module_fingerprint = module.fingerprint();
        let verified = zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(verified.module(), &module);
        let bytes =
            zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        assert_eq!(
            zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default())
                .unwrap()
                .module(),
            &module
        );
        let wrong = if ty == Type::I8 {
            Constant::U8(0)
        } else {
            Constant::I32(0)
        };
        module.functions[0]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::Constant(_)))
            .unwrap()
            .kind = InstructionKind::Constant(wrong);
        plan.module_fingerprint = module.fingerprint();
        assert!(
            zil_verify::verify_module(
                module.clone(),
                plan.clone(),
                &module.target,
                &Limits::default()
            )
            .is_err()
        );
        let bytes =
            zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).is_err()
        );
    }
}
#[test]
fn return_value_entries_preserve_signedness_and_width() {
    for (ty, value) in [
        ("u32", "4294967295"),
        ("i8", "-128"),
        ("i16", "-32768"),
        ("isize", "-9223372036854775808"),
    ] {
        native(
            &format!("fn result()->{ty}{{return {value}}}"),
            Some("result"),
            format!("{value}\n").as_bytes(),
            true,
        );
    }
}
#[test]
fn checked_runtime_failures_trap_in_both_backends() {
    for (ty, expression) in [
        ("i8", "127i8+1i8"),
        ("i16", "-32768i16-1i16"),
        ("u32", "0u32-1u32"),
        ("isize", "9223372036854775807isize+1isize"),
        ("i8", "i8(-129i16)"),
        ("i16", "i16(32768u32)"),
        ("u32", "u32(-1isize)"),
        ("isize", "isize(18446744073709551615u64)"),
        ("i8", "1i8<<8i8"),
        ("u32", "1u32/0u32"),
    ] {
        native(
            &format!("fn result()->{ty}{{return {expression}}}"),
            Some("result"),
            b"",
            false,
        );
    }
}

#[cfg(unix)]
#[test]
fn real_c_scalar_calls_and_record_layout_both_backends() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "integer-ffi-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("oracle.c"),
        include_str!("../../../tests/integers/ffi.c"),
    )
    .unwrap();
    let object = root.join("oracle.o");
    let output = Command::new("clang")
        .args(["-std=c11", "-Werror", "-O2", "-c"])
        .arg(root.join("oracle.c"))
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let wrapper = root.join("clang");
    let quoted = format!("'{}'", object.to_string_lossy().replace('\'', "'\\''"));
    fs::write(&wrapper,format!("#!/bin/sh\nfor arg; do case \"$arg\" in -c|-S|--version) exec clang \"$@\";; esac; done\nexec clang \"$@\" -x none {quoted}\n")).unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let text = include_str!("../../../tests/integers/ffi.zil");
    check(text);
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let path = root.join(format!("{:?}-{:?}", backend, optimization));
            fs::create_dir_all(&path).unwrap();
            let report = compile_source(
                &source(text),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some("result".into()),
                    clang: Some(wrapper.clone()),
                    output: Some(path.join("program")),
                    emit_dir: Some(path.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(path.join("compile.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
            assert!(llvm.contains("declare signext i8 @echo_signed_byte(i8 signext)"));
            assert!(llvm.contains("declare signext i16 @echo_signed_word(i16 signext)"));
            assert!(llvm.contains("declare i32 @echo_unsigned_word(i32)"));
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(path.join("stdout"), &output.stdout).unwrap();
            fs::write(path.join("stderr"), &output.stderr).unwrap();
            assert!(output.status.success(), "{output:?}");
            assert_eq!(output.stdout, b"0\n");
        }
    }
}

#[test]
fn const_generic_domains_and_context_free_ct_result_identity() {
    for (ty, value) in [
        ("u32", "4294967295"),
        ("i8", "-128"),
        ("i16", "-32768"),
        ("isize", "-9223372036854775808"),
    ] {
        let text = format!(
            "fn identity<T:Copy>(x:T)->T{{return x}} fn choose<#N:{ty}>()->{ty}{{return N}} fn result()->{ty}{{let scalar=#(choose::<{value}{ty}>());return identity(scalar)}}"
        );
        check(&text);
        native(&text, Some("result"), format!("{value}\n").as_bytes(), true);
    }
}

fn ct_module(
    width: u16,
    values: &[Constant],
    operation: zil_ir::CtInstructionKind,
    ty: Type,
) -> zil_ir::CtModule {
    let mut target = TargetProfile::default();
    target.pointer_width = width;
    let mut instructions = values
        .iter()
        .enumerate()
        .map(|(i, c)| zil_ir::CtInstruction {
            result: zil_ir::ValueId(i as u32),
            ty: c.ty(),
            kind: zil_ir::CtInstructionKind::Constant(c.clone()),
        })
        .collect::<Vec<_>>();
    let result = zil_ir::ValueId(instructions.len() as u32);
    instructions.push(zil_ir::CtInstruction {
        result,
        ty,
        kind: operation,
    });
    zil_ir::CtModule {
        schema_version: zil_base::SCHEMA_VERSION,
        input_fingerprint: zil_base::Fingerprint::ZERO,
        target,
        instructions,
        result,
        max_steps: 100,
        dependencies: vec![],
    }
}
#[test]
fn ct_target32_and_target64_checked_wrapping_shift_and_conversion() {
    use zil_ir::{BinaryOp as B, CtInstructionKind as K, ValueId as V};
    for width in [32, 64] {
        let maximum = if width == 32 {
            i64::from(i32::MAX)
        } else {
            i64::MAX
        };
        let minimum = if width == 32 {
            i64::from(i32::MIN)
        } else {
            i64::MIN
        };
        for (values, op, expected) in [
            (
                vec![Constant::Isize(maximum), Constant::Isize(1)],
                B::AddWrapping,
                Some(Constant::Isize(minimum)),
            ),
            (
                vec![Constant::Isize(minimum), Constant::Isize(1)],
                B::SubtractWrapping,
                Some(Constant::Isize(maximum)),
            ),
            (
                vec![Constant::Isize(maximum), Constant::Isize(1)],
                B::Add,
                None,
            ),
            (
                vec![
                    Constant::Isize(minimum),
                    Constant::Isize(i64::from(width - 1)),
                ],
                B::ShiftRight,
                Some(Constant::Isize(-1)),
            ),
            (
                vec![Constant::Isize(1), Constant::Isize(i64::from(width))],
                B::ShiftLeft,
                None,
            ),
            (
                vec![Constant::U32(u32::MAX), Constant::U32(1)],
                B::AddWrapping,
                Some(Constant::U32(0)),
            ),
            (
                vec![Constant::I8(i8::MIN), Constant::I8(-1)],
                B::Divide,
                None,
            ),
            (
                vec![Constant::I16(i16::MAX), Constant::I16(1)],
                B::AddWrapping,
                Some(Constant::I16(i16::MIN)),
            ),
        ] {
            let module = ct_module(
                width,
                &values,
                K::Binary {
                    op,
                    left: V(0),
                    right: V(1),
                },
                values[0].ty(),
            );
            let bytes = zil_ir::wire::encode_ct(&module, &Limits::default()).unwrap();
            let verified =
                zil_verify::verify_ct_bytes(&bytes, &module.target, &Limits::default()).unwrap();
            let result = zil_ct::evaluate(&verified);
            if let Some(expected) = expected {
                assert_eq!(result.unwrap(), expected);
            } else {
                assert!(result.is_err());
            }
        }
        let module = ct_module(
            width,
            &[Constant::I64(i64::from(i32::MAX) + 1)],
            K::ConvertInteger {
                value: V(0),
                target: Type::Isize,
            },
            Type::Isize,
        );
        let verified =
            zil_verify::verify_ct(module.clone(), &module.target, &Limits::default()).unwrap();
        if width == 32 {
            assert!(zil_ct::evaluate(&verified).is_err());
        } else {
            assert_eq!(
                zil_ct::evaluate(&verified).unwrap(),
                Constant::Isize(i64::from(i32::MAX) + 1)
            );
        }
        let module = ct_module(
            width,
            &[Constant::F64(2147483648.0f64.to_bits())],
            K::ConvertNumber {
                value: V(0),
                target: Type::Isize,
            },
            Type::Isize,
        );
        let verified =
            zil_verify::verify_ct(module.clone(), &module.target, &Limits::default()).unwrap();
        assert_eq!(zil_ct::evaluate(&verified).is_ok(), width == 64);
    }
}
#[test]
fn raw32_isize_out_of_range_is_rejected_by_each_kernel_and_wire() {
    let (mut module, mut plan) = raw("fn value()->isize{return 2147483648isize}");
    module.target.pointer_width = 32;
    assert!(zil_analyze::analyze(&module, &Limits::default()).is_err());
    plan.module_fingerprint = module.fingerprint();
    plan.target_fingerprint = module.target.fingerprint();
    assert!(
        zil_verify::verify_module(
            module.clone(),
            plan.clone(),
            &module.target,
            &Limits::default()
        )
        .is_err()
    );
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).is_err());
    let ct = ct_module(
        32,
        &[Constant::Isize(i64::from(i32::MAX) + 1)],
        zil_ir::CtInstructionKind::Unary {
            op: zil_ir::UnaryOp::BitNot,
            operand: zil_ir::ValueId(0),
        },
        Type::Isize,
    );
    assert!(zil_verify::verify_ct(ct.clone(), &ct.target, &Limits::default()).is_err());
    let bytes = zil_ir::wire::encode_ct(&ct, &Limits::default()).unwrap();
    assert!(zil_verify::verify_ct_bytes(&bytes, &ct.target, &Limits::default()).is_err());
}
