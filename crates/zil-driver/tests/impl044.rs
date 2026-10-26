//! Scalar FFI source/native acceptance and independent raw-boundary attacks.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, Span, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::*;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-044-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "ffi.zil", text)
}
fn raw(text: &str) -> (TypedModule, CandidatePlan) {
    let source = source(text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let module = zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    (module, plan)
}
const READ: &str = "extern \"C\" fn getchar()->i32; fn answer()->i32{unsafe{return getchar()}}";
const ABS: &str = "extern \"C\" fn abs(value:i32)->i32; fn answer()->i32{unsafe{return abs(-7)}}";
fn rejected(module: TypedModule, mut plan: CandidatePlan, code: &str) {
    // Rebinding the identity removes hash failure as an explanation: the
    // independent structural checker must reject these forged candidates.
    plan.module_fingerprint = module.fingerprint();
    let target = module.target.clone();
    let errors =
        zil_verify::verify_module(module.clone(), plan.clone(), &target, &Limits::default())
            .unwrap_err();
    assert_eq!(errors[0].code, code, "{errors:?}");
    let bytes = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    let errors = zil_verify::verify_module_bytes(&bytes, &target, &Limits::default()).unwrap_err();
    assert_eq!(errors[0].code, code, "{errors:?}");
}
fn foreign_call(function: &mut TypedFunction) -> &mut Instruction {
    function
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::ForeignCall { .. }))
        .unwrap()
}
fn checked(text: &str) {
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
fn native(text: &str, entry: Option<&str>, expected_code: i32, expected_stdout: &[u8]) -> String {
    let temp = Temp::new();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: entry.map(str::to_owned),
            output: Some(temp.0.join("o0")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    let optimized = temp.0.join("o2");
    let built = Command::new("clang")
        .args(["-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(&optimized)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    for path in [&report.artifacts["executable"], &optimized] {
        let result = Command::new(path).output().unwrap();
        assert_eq!(result.status.code(), Some(expected_code), "{result:?}");
        assert_eq!(result.stdout, expected_stdout);
        assert!(result.stderr.is_empty());
    }
    llvm
}

#[test]
fn c_i32_i64_and_unit_use_real_native_symbols() {
    let llvm = native(
        "extern \"C\" fn abs(x:i32)->i32; extern \"C\" fn labs(x:i64)->i64; fn answer()->i64 {unsafe{if abs(-7)==7{return labs(-5000000042)} return 0}}",
        Some("answer"),
        0,
        b"5000000042\n",
    );
    assert!(llvm.contains("declare i32 @abs(i32)"));
    assert!(llvm.contains("declare i64 @labs(i64)"));
    let llvm = native(
        "extern \"C\" fn exit(code:i32); fn main()->i32{unsafe{exit(9)} return 1}",
        None,
        9,
        b"",
    );
    assert!(llvm.contains("declare void @exit(i32)"));
}

#[test]
fn nested_unsafe_loop_break_continue_and_return_are_balanced() {
    native(
        "extern \"C\" fn abs(x:i32)->i32; fn answer()->i32{var n=0;var total=0;unsafe{loop{n+=1;if n==1{continue}unsafe{if n>4{break}total+=abs(-n)}}}unsafe{if total==9{return abs(-total)}}return 0}",
        Some("answer"),
        0,
        b"9\n",
    );
    native(
        "extern \"C\" fn abs(x:i32)->i32;fn answer()->i32{var total=0;for n in 0..5{unsafe{if n==1{continue}if n==4{break}total+=abs(-n)}}return total}",
        Some("answer"),
        0,
        b"5\n",
    );
}

#[test]
fn closure_requires_its_own_lexical_unsafe_region() {
    for body in [
        "fn answer()->i32{unsafe{let f=()=>abs(-7);return f()}}",
        "fn answer()->i32{unsafe{let f:fn()->i32=()=>abs(-7);return f()}}",
    ] {
        let text = format!("extern \"C\" fn abs(x:i32)->i32;{body}");
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.diagnostics[0].code,
            "E-FFI-UNSAFE",
            "{}",
            report.to_json()
        );
    }
    native(
        "extern \"C\" fn abs(x:i32)->i32;fn answer()->i32{let f=()=>{unsafe{return abs(-7)}};return f()}",
        Some("answer"),
        0,
        b"7\n",
    );
    checked(
        "extern \"C\" fn abs(x:i32)->i32;fn answer()->i32{let f:fn()->i32=()=>{unsafe{return abs(-7)}};return f()}",
    );
}

#[test]
fn unsupported_signatures_and_implicit_unsafe_are_rejected_at_source() {
    for text in [
        "extern \"C\" fn getchar()->i32;fn answer()->i32{return getchar()}",
        "extern \"C\" fn getchar()->i32;fn answer()->i32{unsafe{}return getchar()}",
        "extern \"C\" fn getchar()->i32;fn answer()->i32{unsafe{let f:fn()->i32=getchar;return f()}}",
        "extern \"Rust\" fn f()->i32;fn answer()->i32{unsafe{return f()}}",
        "extern \"C\" fn f()->bool;fn answer()->i32{return 0}",
        "extern \"C\" fn f(x:bool);fn answer()->i32{return 0}",
        "extern \"C\" fn f(x:());fn answer()->i32{return 0}",
        "extern \"C\" fn f(x:&i32);fn answer()->i32{return 0}",
        "extern \"C\" fn f(x:&mut i32);fn answer()->i32{return 0}",
        "extern \"C\" fn f(x:fn()->i32);fn answer()->i32{return 0}",
        "extern \"C\" fn f()->i32{return 1}fn answer()->i32{return 0}",
        "extern \"C\" fn f<T>(x:T)->i32;fn answer()->i32{return 0}",
        "extern \"C\" fn f(give x:i32);fn answer()->i32{return 0}",
        "pub extern \"C\" fn f()->i32;fn answer()->i32{return 0}",
        "extern \"C\" fn f(...)->i32;fn answer()->i32{return 0}",
        "extern \"C\" fn main()->i32;",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{text}\n{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
    let report = compile_source(
        &source(
            "extern \"C\" fn abs(x:i32)->i32;fn answer()->i32{let x:i64=1;unsafe{return abs(x)}}",
        ),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.diagnostics[0].phase,
        Phase::Types,
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.diagnostics[0].code,
        "E-TYPE-MISMATCH",
        "{}",
        report.to_json()
    );
}

#[test]
fn source_ct_rejects_direct_foreign_and_unsafe_wrappers() {
    for text in [
        "extern \"C\" fn getchar()->i32;fn answer()->i32{return #(getchar())}",
        "extern \"C\" fn getchar()->i32;fn read()->i32{unsafe{return getchar()}}fn answer()->i32{return #(read())}",
        "extern \"C\" fn getchar()->i32;fn answer()->i32{unsafe{return #(getchar())}}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.diagnostics[0].phase,
            Phase::Ct,
            "{}",
            report.to_json()
        );
        assert_eq!(
            report.diagnostics[0].code,
            "E-CT-FORBIDDEN",
            "{}",
            report.to_json()
        );
    }
    checked("extern \"C\" fn getchar()->i32;fn pure()->i32=>42;fn answer()->i32{return #(pure())}");
}

#[test]
fn raw_roundtrip_preserves_foreign_metadata_and_explicit_regions() {
    let (module, plan) = raw(READ);
    let limits = Limits::default();
    let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        wire::decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan.clone())
    );
    let verified = zil_verify::verify_module_bytes(&bytes, &module.target, &limits).unwrap();
    assert!(verified.module().functions[0].blocks.is_empty());
    assert_eq!(
        verified.module().functions[0]
            .foreign
            .as_ref()
            .unwrap()
            .symbol,
        "getchar"
    );
    for length in 0..bytes.len() {
        assert!(wire::decode_module_candidate(&bytes[..length], &limits).is_err());
    }
    let mut changed = module.clone();
    changed.functions[0].foreign.as_mut().unwrap().symbol = "other_symbol".into();
    assert_ne!(module.fingerprint(), changed.fingerprint());
    let mut lir = zil_lower::lower(&verified).unwrap();
    lir.functions[0].foreign.as_mut().unwrap().symbol = "other_symbol".into();
    assert_eq!(
        zil_backend_llvm::emit(&verified, &lir).unwrap_err()[0].code,
        "E-CODEGEN-UNBOUND-LIR"
    );
}

#[test]
fn raw_ordinary_call_and_function_pointer_cannot_hide_foreign_requirements() {
    let (module, plan) = raw(READ);
    let mut ordinary = module.clone();
    foreign_call(&mut ordinary.functions[1]).kind = InstructionKind::Call {
        function: FunctionId(0),
        arguments: vec![],
    };
    rejected(ordinary, plan.clone(), "E-VERIFY-FOREIGN");
    let mut pointer = module;
    foreign_call(&mut pointer.functions[1]).kind = InstructionKind::FunctionRef {
        function: FunctionId(0),
    };
    rejected(pointer, plan, "E-VERIFY-FOREIGN");
}

#[test]
fn raw_call_requires_active_matching_lexical_region() {
    let (module, plan) = raw(READ);
    for mutation in 0..5 {
        let mut bad = module.clone();
        let instructions = &mut bad.functions[1].blocks[0].instructions;
        match mutation {
            0 => instructions.retain(|i| !matches!(i.kind, InstructionKind::UnsafeEnter { .. })),
            1 => instructions.retain(|i| !matches!(i.kind, InstructionKind::UnsafeExit { .. })),
            2 => {
                let call = instructions
                    .iter_mut()
                    .find(|i| matches!(i.kind, InstructionKind::ForeignCall { .. }))
                    .unwrap();
                if let InstructionKind::ForeignCall { region, .. } = &mut call.kind {
                    *region = 999;
                }
            }
            3 => {
                let enter = instructions
                    .iter()
                    .position(|i| matches!(i.kind, InstructionKind::UnsafeEnter { .. }))
                    .unwrap();
                instructions[enter].kind = InstructionKind::UnsafeExit { region: 0 };
            }
            _ => {
                let call = instructions
                    .iter_mut()
                    .find(|i| matches!(i.kind, InstructionKind::ForeignCall { .. }))
                    .unwrap();
                call.span = Span::new(FileId(44), 0, 1);
            }
        }
        rejected(bad, plan.clone(), "E-VERIFY-UNSAFE");
    }
}

#[test]
fn raw_foreign_signature_body_symbol_and_environment_are_independent_checks() {
    let (module, plan) = raw(ABS);
    for mutation in 0..9 {
        let mut bad = module.clone();
        let declaration = &mut bad.functions[0];
        match mutation {
            0 => declaration.foreign.as_mut().unwrap().abi = "Rust".into(),
            1 => declaration.foreign.as_mut().unwrap().symbol = "getchar()\n".into(),
            2 => declaration.foreign.as_mut().unwrap().symbol = "main".into(),
            3 => declaration.return_type = Type::Bool,
            4 => declaration.locals[0].ty = Type::Bool,
            5 => {
                declaration.locals[0].ty = Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                }
            }
            6 => declaration.locals[0].mutable = true,
            7 => declaration.parameters.push(LocalId(0)),
            _ => declaration.blocks.push(BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Trap,
            }),
        }
        rejected(bad, plan.clone(), "E-VERIFY-FOREIGN");
    }
}

#[test]
fn raw_argument_arity_and_type_are_checked() {
    let (module, plan) = raw(ABS);
    let mut arity = module.clone();
    if let InstructionKind::ForeignCall { arguments, .. } =
        &mut foreign_call(&mut arity.functions[1]).kind
    {
        arguments.clear();
    }
    rejected(arity, plan.clone(), "E-VERIFY-FOREIGN");
    let mut signature = module;
    signature.functions[0].locals[0].ty = Type::I64;
    rejected(signature, plan, "E-VERIFY-FOREIGN");
}

#[test]
fn raw_cfg_joins_duplicate_regions_and_dead_calls_cannot_gain_authority() {
    let (mut module, plan) = raw(
        "extern \"C\" fn abs(x:i32)->i32;fn answer(flag:bool)->i32{var value=0;if flag{unsafe{value=abs(-7)}}else{value=8}return value}",
    );
    for block in &mut module.functions[1].blocks {
        block
            .instructions
            .retain(|i| !matches!(i.kind, InstructionKind::UnsafeExit { .. }));
    }
    rejected(module, plan, "E-VERIFY-UNSAFE");

    let (mut module, plan) = raw(READ);
    let instructions = &mut module.functions[1].blocks[0].instructions;
    let at = instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::UnsafeEnter { .. }))
        .unwrap();
    let enter = instructions[at].clone();
    instructions.insert(at + 1, enter);
    rejected(module, plan, "E-VERIFY-UNSAFE");

    let (mut module, mut plan) =
        raw("extern \"C\" fn quit();fn answer()->i32{unsafe{quit()}return 0}");
    let mut call = foreign_call(&mut module.functions[1]).clone();
    call.result = None;
    let id = BlockId(module.functions[1].blocks.len() as u32);
    module.functions[1].blocks.push(BasicBlock {
        id,
        instructions: vec![call],
        terminator: Terminator::Trap,
    });
    plan.functions[1].blocks.push(BlockPlan {
        block: id,
        initialized_in: vec![],
        initialized_out: vec![],
    });
    rejected(module, plan, "E-VERIFY-UNSAFE");
}

#[test]
fn raw_foreign_target_and_duplicate_symbol_checks_are_not_candidate_claims() {
    let (mut module, plan) = raw(READ);
    if let InstructionKind::ForeignCall { function, .. } =
        &mut foreign_call(&mut module.functions[1]).kind
    {
        *function = FunctionId(1);
    }
    rejected(module, plan, "E-VERIFY-FOREIGN");
    let (mut module, plan) = raw(
        "extern \"C\" fn getchar()->i32;extern \"C\" fn other()->i64;fn answer()->i32{unsafe{return getchar()}}",
    );
    module.functions[1].foreign.as_mut().unwrap().symbol = "getchar".into();
    rejected(module, plan, "E-VERIFY-FOREIGN");
}

#[test]
fn raw_ct_cannot_execute_a_well_formed_foreign_program() {
    let (module, plan) = raw(READ);
    let target = module.target.clone();
    let ct = CtProgram {
        module,
        plan,
        entry: FunctionId(1),
        max_steps: 100,
        max_call_depth: 8,
        max_live_slots: 100,
    };
    let errors = zil_verify::verify_ct_program(ct, &target, &Limits::default()).unwrap_err();
    assert_eq!(errors[0].code, "E-CT-FORBIDDEN");
}

#[test]
fn untrusted_foreign_metadata_obeys_work_budgets() {
    let (mut module, mut plan) = raw(READ);
    module.functions[0].foreign.as_mut().unwrap().symbol = "x".repeat(10000);
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits {
        max_analysis_steps: 1000,
        ..Limits::default()
    };
    assert_eq!(
        zil_analyze::analyze(&module, &limits).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    let target = module.target.clone();
    assert_eq!(
        zil_verify::verify_module(module, plan, &target, &limits).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
}
