//! Independent generated/handwritten ownership and nominal controls.
use std::{fs, path::PathBuf, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};

fn directory(label: &str) -> PathBuf {
    let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "review239-generated-{label}-{}",
            std::process::id()
        ));
    fs::create_dir_all(&out).unwrap();
    out
}
const RESOURCE: &str = "extern \"C\" fn putchar(c:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code);}}}";
const BODY: &str =
    "let e=E.Pair(R{code:65},R{code:66});match e{E.Pair(a,b)=>{}\nE.Empty()=>{}}return 0";

#[test]
fn generated_manages_and_handwritten_fields_execute_the_same_declared_order() {
    // Predeclared from the two source fields: Pair(a,b) drops a then b => AB,
    // exit 0, no explicit allocation. This assertion precedes machine execution.
    for (label, source) in [
        (
            "handwritten",
            format!("{RESOURCE}enum E{{Empty,Pair(manages R,manages R)}}fn main()->i32{{{BODY}}}"),
        ),
        (
            "generated",
            format!(
                "{RESOURCE}#for(n in [1]){{enum E{{Empty,Pair(manages R,manages R)}}pub fn run()->i32{{{BODY}}}}}fn main()->i32=>run()"
            ),
        ),
    ] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = directory(&format!("{label}-{optimization:?}"));
            fs::write(out.join("source.zil"), &source).unwrap();
            fs::write(out.join("expected.txt"), "stdout=AB\nexit=0\n").unwrap();
            let report = compile_source(
                &SourceUnit::new(FileId(0), ModuleId(0), "generated239.zil", source.as_str()),
                &CompileOptions {
                    stop_after: Stage::Build,
                    optimization,
                    backend: if cfg!(feature = "llvm-api") {
                        NativeBackend::LlvmApi
                    } else {
                        NativeBackend::Clang
                    },
                    output: Some(out.join("program")),
                    emit_dir: Some(out.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(out.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let native = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(out.join("stdout"), &native.stdout).unwrap();
            fs::write(out.join("stderr"), &native.stderr).unwrap();
            fs::write(out.join("exit.txt"), format!("{:?}", native.status.code())).unwrap();
            assert_eq!(native.status.code(), Some(0));
            assert_eq!(native.stdout, b"AB");
            assert!(native.stderr.is_empty());
        }
    }
}

#[test]
fn generation_preserves_private_iteration_identity_and_closed_payload_profiles() {
    for (label, source) in [
        (
            "cross-iteration",
            "resource R{n:i32}#for(n in [1,2]){enum E{V(manages R)}#if(n==1){pub fn make()->E{return E.V(R{n:1})}}else{pub fn consume(e:E){}}}fn main(){consume(make())}",
        ),
        (
            "private-escape",
            "resource R{n:i32}#for(n in [1]){enum E{V(manages R)}}fn main(){let e=E.V(R{n:1})}",
        ),
        (
            "bare-buffer",
            "#for(n in [1]){enum E{V(ByteBuffer)}}fn main(){}",
        ),
        (
            "bare-resource",
            "resource R{n:i32}#for(n in [1]){enum E{V(R)}}fn main(){}",
        ),
        (
            "owned-generic",
            "resource R{n:i32}#for(n in [1]){enum E<T>{V(manages R,T)}}fn main(){}",
        ),
    ] {
        let out = directory(label);
        fs::write(out.join("source.zil"), source).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "generated239-reject.zil", source),
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{label}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| !d.code.starts_with("E-PARSE")),
            "{label}: {}",
            report.to_json()
        );
    }
}

#[test]
fn new_local_observers_cannot_enter_ct_even_when_the_enum_is_copy() {
    use zil_base::{Limits, TargetProfile};
    use zil_ir::*;
    let text = "enum E{Some(i32)}fn main()->i32{let e=E.Some(7);match e{E.Some(n)=>{return n}}}";
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "copy-observer239.zil", text);
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let limits = Limits::default();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let control = CtProgram {
        module: module.clone(),
        plan: plan.clone(),
        entry: FunctionId(0),
        max_steps: 1_000,
        max_call_depth: 32,
        max_live_slots: 128,
    };
    let verified = zil_verify::verify_ct_program(control, &module.target, &limits).unwrap();
    assert_eq!(
        zil_ct::evaluate_program(&verified).unwrap().value,
        zil_ct::CompileTimeValue::Scalar(Constant::I32(7))
    );
    for tag in [true, false] {
        let mut mutated = module.clone();
        let f = &mut mutated.functions[0];
        let local = f.locals.iter().find(|l| l.name == "e").unwrap().id;
        let mut changed = false;
        for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if tag && matches!(i.kind, InstructionKind::VariantTag { .. }) {
                i.kind = InstructionKind::VariantTagLocal {
                    local,
                    path: vec![],
                };
                changed = true;
                break;
            } else if !tag && let InstructionKind::ReadVariant { variant, index, .. } = i.kind {
                i.kind = InstructionKind::ReadVariantLocal {
                    local,
                    path: vec![VariantProjection { variant, index }],
                };
                changed = true;
                break;
            }
        }
        assert!(changed);
        let mut plan = plan.clone();
        plan.module_fingerprint = mutated.fingerprint();
        zil_verify::verify_module(mutated.clone(), plan.clone(), &module.target, &limits).unwrap();
        let raw = CtProgram {
            module: mutated,
            plan,
            entry: FunctionId(0),
            max_steps: 1_000,
            max_call_depth: 32,
            max_live_slots: 128,
        };
        let bytes = zil_ir::wire::encode_ct_program(&raw, &limits).unwrap();
        let out = directory(if tag { "ct-new-tag" } else { "ct-new-read" });
        fs::write(out.join("source-control.zil"), text).unwrap();
        fs::write(out.join("raw.txt"), format!("{raw:#?}")).unwrap();
        fs::write(out.join("candidate.wire"), &bytes).unwrap();
        for result in [
            zil_verify::verify_ct_program(raw, &module.target, &limits),
            zil_verify::verify_ct_program_bytes(&bytes, &module.target, &limits),
        ] {
            let errors = result.expect_err("new runtime local opcode admitted to CT");
            assert!(
                errors.iter().any(|d| d.code == "E-CT-FORBIDDEN"),
                "{errors:?}"
            );
        }
    }
}
