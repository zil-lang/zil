//! Independent 096 boundary probes. Types/Analyze produce ordinary candidates;
//! only the sealed result of Verify is then used to exercise Lower and emit.
//! These tests do not review or certify the imported 094/095 implementations.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{Constant, FunctionId, InstructionKind, LirModule};
use zil_verify::VerifiedModule;

const BODY: &str = "fn value()->i64{let b=bytes_new();bytes_reserve(b,5);bytes_push(b,42u8);let p=bytes_view(b);let n=i64(p[0]);return n+bytes_len(b)}";

fn checked(text: &str, target: &TargetProfile) -> (VerifiedModule, LirModule) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "review096.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &syntax).unwrap();
    let typed = zil_types::check(&core, target, &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&typed, &Limits::default()).unwrap();
    let verified = zil_verify::verify_module(typed, plan, target, &Limits::default()).unwrap();
    let lir = zil_lower::lower(&verified).unwrap();
    (verified, lir)
}

fn retain(label: &str, text: &str) {
    if let Some(out) = std::env::var_os("ZIL_REVIEW096_OUT") {
        let out = PathBuf::from(out);
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join(label), text).unwrap();
    }
}

#[test]
fn copied_fingerprint_cannot_authorize_changed_byte_storage_lir() {
    let (verified, original) = checked(BODY, &TargetProfile::host_llvm());
    retain("binding-source.zil", BODY);
    let llvm = zil_backend_llvm::emit(&verified, &original).unwrap();
    assert!(llvm.contains("define internal i1 @zil.bytes.reserve"));
    retain("binding-valid.ll", &llvm);
    let mut mutations = Vec::new();
    for label in ["reserve-operand", "new-kind", "borrow-mutability"] {
        let mut lir = original.clone();
        let mut changed = false;
        for instruction in lir
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
        {
            match (&mut instruction.kind, label) {
                (InstructionKind::ByteBufferReserve { buffer, capacity }, "reserve-operand") => {
                    *capacity = *buffer;
                    changed = true;
                }
                (kind @ InstructionKind::ByteBufferNew, "new-kind") => {
                    *kind = InstructionKind::Constant(Constant::I64(0));
                    changed = true;
                }
                (InstructionKind::ByteBufferBorrow { mutable, .. }, "borrow-mutability") => {
                    *mutable = !*mutable;
                    changed = true;
                }
                _ => {}
            }
        }
        assert!(changed, "fixture did not exercise {label}");
        mutations.push((label, lir));
    }
    let mut lir = original.clone();
    lir.resources[0].teardown = Some(FunctionId(0));
    mutations.push(("teardown", lir));
    let mut lir = original.clone();
    lir.resources[0].name.push_str("-forged");
    mutations.push(("resource-identity", lir));
    let mut lir = original.clone();
    lir.target = TargetProfile::windows_x64_llvm();
    mutations.push(("target", lir));
    let mut lir = original.clone();
    lir.layouts[3].alignment = 4;
    mutations.push(("layout", lir));
    let mut lir = original.clone();
    lir.module_id = ModuleId(1);
    mutations.push(("module-identity", lir));
    assert_eq!(mutations.len(), 8);
    for (label, lir) in mutations {
        assert_eq!(lir.verified_fingerprint, verified.fingerprint());
        let errors = zil_backend_llvm::emit(&verified, &lir).unwrap_err();
        assert_eq!(
            errors[0].code, "E-CODEGEN-UNBOUND-LIR",
            "{label}: {errors:?}"
        );
        retain(&format!("binding-{label}.txt"), &format!("{errors:#?}\n"));
        println!("binding {label}: rejected E-CODEGEN-UNBOUND-LIR");
    }
}

#[test]
fn allocator_declarations_must_match_native_abi_without_duplicate_symbols() {
    let linux =
        "extern \"C\" fn realloc(p:c_handle,n:i64)->c_handle;extern \"C\" fn free(p:c_handle);";
    let windows = "extern \"C\" fn GetProcessHeap()->c_handle;extern \"C\" fn HeapAlloc(h:c_handle,f:i32,n:i64)->c_handle;extern \"C\" fn HeapReAlloc(h:c_handle,f:i32,p:c_handle,n:i64)->c_handle;extern \"C\" fn HeapFree(h:c_handle,f:i32,p:c_handle)->i32;";
    for (name, declarations, target, symbols) in [
        (
            "linux",
            linux,
            TargetProfile::host_llvm(),
            vec!["realloc", "free"],
        ),
        (
            "win64",
            windows,
            TargetProfile::windows_x64_llvm(),
            vec!["GetProcessHeap", "HeapAlloc", "HeapReAlloc", "HeapFree"],
        ),
    ] {
        let text = format!("{declarations}{BODY}");
        let (verified, lir) = checked(&text, &target);
        let llvm = zil_backend_llvm::emit(&verified, &lir).unwrap();
        for symbol in symbols {
            assert_eq!(
                llvm.lines()
                    .filter(|line| line.starts_with("declare ")
                        && line.contains(&format!("@{symbol}(")))
                    .count(),
                1,
                "duplicate declaration for {symbol}"
            );
        }
        retain(&format!("allocator-{name}-valid.zil"), &text);
        retain(&format!("allocator-{name}-valid.ll"), &llvm);
        println!("allocator {name}: matching declarations accepted exactly once");
    }
    let bad = [
        (
            "realloc-size",
            "extern \"C\" fn realloc(p:c_handle,n:i32)->c_handle;",
            false,
        ),
        (
            "free-result",
            "extern \"C\" fn free(p:c_handle)->i32;",
            false,
        ),
        (
            "process-heap-result",
            "extern \"C\" fn GetProcessHeap()->i64;",
            true,
        ),
        (
            "heap-alloc-result",
            "extern \"C\" fn HeapAlloc(h:c_handle,f:i32,n:i64)->i64;",
            true,
        ),
        (
            "heap-realloc-flags",
            "extern \"C\" fn HeapReAlloc(h:c_handle,f:i64,p:c_handle,n:i64)->c_handle;",
            true,
        ),
        (
            "heap-free-pointer",
            "extern \"C\" fn HeapFree(h:c_handle,f:i32,p:i64)->i32;",
            true,
        ),
    ];
    for (name, declaration, windows) in bad {
        let target = if windows {
            TargetProfile::windows_x64_llvm()
        } else {
            TargetProfile::host_llvm()
        };
        let text = format!("{declaration}{BODY}");
        let (verified, lir) = checked(&text, &target);
        let errors = zil_backend_llvm::emit(&verified, &lir).unwrap_err();
        assert_eq!(errors[0].code, "E-CODEGEN-FOREIGN", "{name}: {errors:?}");
        retain(&format!("allocator-{name}-invalid.zil"), &text);
        retain(
            &format!("allocator-{name}-invalid.txt"),
            &format!("{errors:#?}\n"),
        );
        println!("allocator {name}: rejected E-CODEGEN-FOREIGN");
    }
}
