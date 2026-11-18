//! Source resource/closure interactions; native traces test actual cleanup.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{
    CoreExprKind, CoreStatementKind, SemanticDefinitionBody, SemanticDefinitionKind,
    SemanticOriginalBlock, SemanticOriginalFunctionBody, UnaryOp,
};
use zil_driver::{CompileOptions, CompileReport, Stage, compile_source};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-resource-closure-cleanup-{}-{}",
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
fn source_report(text: &str, stage: Stage) -> CompileReport {
    compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "closures.zil", text),
        &CompileOptions {
            stop_after: stage,
            ..CompileOptions::default()
        },
    )
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}
fn native(text: &str, expected: &str) -> (String, String) {
    let temp = Temp::new();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "closures.zil", text),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(temp.0.join("plain")),
            emit_dir: Some(temp.0.join("ir")),
            ..CompileOptions::default()
        },
    );
    success(&report);
    for optimized in [false, true] {
        let binary = if optimized {
            let path = temp.0.join("optimized");
            let output = Command::new("clang")
                .args(["-x", "ir", "-O2"])
                .arg(&report.artifacts["llvm"])
                .arg("-o")
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            path
        } else {
            report.artifacts["executable"].clone()
        };
        let output = Command::new(binary).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
    }
    let typed = fs::read_to_string(&report.artifacts["typed"]).unwrap();
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(
        !llvm.contains("@malloc"),
        "concrete environment introduced implicit allocation"
    );
    (typed, llvm)
}

fn swap_capture_roles(body: &mut SemanticOriginalBlock) -> usize {
    let mut changes = 0;
    for statement in &mut body.statements {
        let expression = match &mut statement.kind {
            CoreStatementKind::Binding { initializer: Some(value), .. }
            | CoreStatementKind::Expression(value)
            | CoreStatementKind::Return(Some(value)) => value,
            _ => continue,
        };
        match &mut expression.kind {
            CoreExprKind::Closure { body, .. } => changes += swap_capture_roles(body),
            CoreExprKind::Call { arguments, .. } => {
                for argument in arguments {
                    if let CoreExprKind::Unary { op: UnaryOp::Give, operand } = &mut argument.kind
                        && let CoreExprKind::Name(name) = &mut operand.kind
                        && name == "transferred"
                    {
                        *name = "observed".into();
                        changes += 1;
                    }
                }
            }
            CoreExprKind::Member { object, .. } => {
                if let CoreExprKind::Name(name) = &mut object.kind
                    && name == "observed"
                {
                    *name = "transferred".into();
                    changes += 1;
                }
            }
            _ => {}
        }
    }
    changes
}

fn verify_exact_capture_roles(text: &str) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "closures.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand_with_map_semantics(
        &source, &ast, &limits, &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    ).unwrap().core;
    let mut module = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();

    // The original closure occurs in its creator and in its own declaration.
    // Swap both lexical uses in both copies, retaining the actual capture IR.
    let mut edited_bodies = 0;
    for definition in &mut module.semantic.as_mut().unwrap().definitions {
        if matches!(definition.kind, SemanticDefinitionKind::Function | SemanticDefinitionKind::Closure)
            && let SemanticDefinitionBody::FunctionWithBody {
                body: SemanticOriginalFunctionBody::Defined(body), ..
            } = &mut definition.body
        {
            let changes = swap_capture_roles(body);
            if changes != 0 {
                assert_eq!(changes, 2);
                edited_bodies += 1;
            }
        }
    }
    assert_eq!(edited_bodies, 2);
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let raw = zil_verify::verify_module(module, plan, &target, &limits).unwrap_err();
    let wire = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err();
    assert_eq!(raw, wire);
    assert!(raw.iter().any(|diagnostic| {
        diagnostic.code == "E-VERIFY-SEMANTIC"
            && diagnostic.message == "logical capture does not select its exact original free-value binding"
    }), "{raw:#?}");
}

#[test]
fn resource_closure_resource_and_unrelated_copy_closure_coexist_native() {
    native(
        include_str!("../../../examples/callback-capture-modes/coexist.zil"),
        "A",
    );
}

#[test]
fn resource_closure_resource_context_keeps_i64_array_literals() {
    let text = "resource Block{cells:[i64;2]} fn main()->i32{let block=Block{cells:[3,4]};let identity=(n:i64)=>n;if identity(block.cells[1])==4{return 0}return 1}";
    success(&source_report(text, Stage::Verify));
}

#[test]
fn resource_closure_descriptor_reads_each_call_and_target_write_is_shared_native() {
    native(
        include_str!("../../../examples/callback-capture-modes/descriptor.zil"),
        "C",
    );
}

#[test]
fn resource_closure_owned_shared_can_repeat_and_environment_transfer_is_unique_native() {
    let (typed, _) = native(
        include_str!("../../../examples/callback-capture-modes/owned_shared.zil"),
        "A",
    );
    assert!(typed.contains("CallClosureShared"));
}

#[test]
fn resource_closure_owned_mutation_through_descriptor_alias_is_exclusive_native() {
    native(
        include_str!("../../../examples/callback-capture-modes/owned_exclusive.zil"),
        "C",
    );
}

#[test]
fn resource_closure_body_give_owns_at_creation_and_conditional_consume_cleans_rest_native() {
    let (typed, _) = native(
        include_str!("../../../examples/callback-capture-modes/consume.zil"),
        "ABC",
    );
    assert!(typed.contains("TakeCapture"));
    assert!(typed.contains("CallClosureOnce"));
}

#[test]
fn resource_closure_uncalled_owned_environments_clean_in_capture_order_native() {
    native(
        include_str!("../../../examples/callback-capture-modes/unused.zil"),
        "ABC",
    );
}

#[test]
fn resource_closure_early_return_loop_exits_and_temporary_environments_cleanup_native() {
    native(
        include_str!("../../../examples/callback-capture-modes/control.zil"),
        "BACDEF",
    );
}

const PRELUDE: &str = "resource Token{code:i32} fn consume(give token:Token){} ";

#[test]
fn resource_closure_owned_write_requires_mutable_closure_position() {
    let text = format!(
        "{PRELUDE} fn main()->i32{{let token=Token{{code:65}};let f=move ()=>{{token.code+=1}};f();return 0}}"
    );
    let report = source_report(&text, Stage::Types);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "E-SEMANTIC-ORIGINAL"
                && diagnostic.message == "original assignment targets an immutable binder"
                && diagnostic.span.is_some_and(|span| {
                    text.get(span.start as usize..span.end as usize) == Some("f")
                })
        }),
        "{}",
        report.to_json()
    );
}

#[test]
fn resource_closure_descriptor_capture_cannot_acquire_give_authority() {
    let text = format!(
        "{PRELUDE} fn main()->i32{{let token=Token{{code:65}};let alias=token;let f=()=>consume(give alias);return 0}}"
    );
    let report = source_report(&text, Stage::Types);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-GIVE-DESCRIPTOR"),
        "{}",
        report.to_json()
    );
}

#[test]
fn resource_closure_source_move_and_capture_liveness_are_independently_rejected() {
    let bodies = [
        ("let token=Token{code:65};let f=move ()=>token.code;consume(give token);return 0",
            Some(("original give transfers an unavailable or borrowed value", "give token"))),
        ("let token=Token{code:65};let f=()=>consume(give token);f();f();return 0",
            Some(("original place was already transferred", "f()"))),
        ("let token=Token{code:65};let f=()=>token.code;consume(give token);return f()", None),
        ("let token=Token{code:65};let alias=token;let f=()=>consume(give token);return alias.code", None),
        ("let token=Token{code:65};let alias=token;let f=move ()=>token.code+alias.code;return f()", None),
        ("let token=Token{code:65};let alias=token;let f=move ()=>alias.code+token.code;return f()", None),
        ("let token=Token{code:65};let alias=token;let f=move ()=>alias.code;consume(give token);return f()", None),
        ("let token=Token{code:65};let f=move ()=>token.code;let moved=f;return f()",
            Some(("original place was already transferred", "f()"))),
    ];
    for (body, early) in bodies {
        let text = format!("{PRELUDE} fn main()->i32{{{body}}}");
        let report = source_report(&text, Stage::Verify);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        if let Some((message, occurrence)) = early {
            let start = text.rfind(occurrence).unwrap();
            assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
            assert!(report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "E-SEMANTIC-ORIGINAL"
                    && diagnostic.message == message
                    && diagnostic.span.is_some_and(|span| {
                        span.start as usize == start
                            && span.end as usize == start + occurrence.len()
                    })
            }), "{}", report.to_json());
        } else {
            assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
            assert!(report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "E-ANALYZE-IR"
                    && diagnostic.message == "resource closure: head transition invalidates a description required later"
            }), "{}", report.to_json());
        }
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn resource_closure_pure_helper_stays_shared_and_recursive_write_helper_is_exclusive_native() {
    native(
        include_str!("../../../examples/callback-capture-modes/effects.zil"),
        "AC",
    );
}

const OBSERVABLE: &str = "extern \"C\" fn putchar(code:i32)->i32; resource Token{code:i32} impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}} fn consume(give token:Token){} ";

const SECOND_DESCRIPTION_WRITE: &str = "
fn write_second(first:Token,second:Token,depth:i32)->i32 {
    let alias=second
    if depth>0 { return write_second(first,alias,depth-1) }
    alias.code+=1
    return first.code
}
";

#[test]
fn resource_closure_only_the_written_parameter_requires_exclusive_capture() {
    let source = |call: &str| {
        format!(
            "{OBSERVABLE}{SECOND_DESCRIPTION_WRITE}
            fn main()->i32 {{
                let captured=Token{{code:65}}
                let callback=move ()=>{{
                    let fresh=Token{{code:66}}
                    return {call}
                }}
                if callback()!=65 || callback()!=65 {{ return 1 }}
                return 0
            }}"
        )
    };
    let (typed, _) = native(&source("write_second(captured,fresh,2)"), "CCA");
    assert!(typed.contains("CallClosureShared"));
    let source = source("write_second(fresh,captured,2)");
    let report = source_report(&source, Stage::Types);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "E-SEMANTIC-ORIGINAL"
                && diagnostic.message == "original assignment targets an immutable binder"
                && diagnostic.span.is_some_and(|span| {
                    source.get(span.start as usize..span.end as usize) == Some("callback")
                })
        }),
        "{}",
        report.to_json()
    );
}

#[test]
fn resource_closure_in_recursive_helper_sees_writes_after_its_call_site() {
    let text = format!(
        "{OBSERVABLE}
        fn step(token:Token,depth:i32)->i32 {{
            if depth>0 {{
                let fresh=Token{{code:64}}
                var callback=move ()=>step(fresh,depth-1)
                if callback()!=65 {{ return 9 }}
            }}
            token.code+=1
            return token.code
        }}
        fn main()->i32 {{
            let token=Token{{code:66}}
            if step(token,2)!=67 {{ return 1 }}
            return 0
        }}"
    );
    let (typed, _) = native(&text, "AAC");
    assert!(typed.contains("CallClosureMut"));
}

#[test]
fn resource_closure_mixed_address_and_owner_capture_ends_loan_after_once_native() {
    let text = format!(
        "{OBSERVABLE} fn main()->i32{{var n=7;let token=Token{{code:65}};let once=()=>{{consume(give token);n}};let observed=once();n=8;if observed==7 && n==8{{return 0}}return 1}}"
    );
    native(&text, "A");
    let same_payload = format!(
        "{OBSERVABLE} fn main()->i32{{let transferred=Token{{code:65}};let observed=Token{{code:66}};let work=()=>{{consume(give transferred);observed.code}};if work()==66 && observed.code==66{{return 0}}return 1}}"
    );
    native(&same_payload, "AB");
    // Keep the Raw/Wire contrast at the creator: both exchanged bindings
    // remain valid original bodies through the later call.
    verify_exact_capture_roles(&same_payload.replace(" && observed.code==66", ""));
}

#[test]
fn resource_closure_owned_environment_moves_through_both_conditional_edges_native() {
    let text = format!(
        "{OBSERVABLE} fn choose(flag:bool,code:i32)->i32{{let token=Token{{code}};let read=move ()=>token.code;let picked=if flag{{read}}else{{read}};if picked()!=code{{return 1}}return 0}} fn main()->i32{{if choose(true,65)!=0{{return 1}}if choose(false,66)!=0{{return 2}}return 0}}"
    );
    native(&text, "AB");
}

#[test]
fn resource_closure_shared_owned_entry_cleans_its_fresh_resource_each_call_native() {
    let text = format!(
        "{OBSERVABLE} fn main()->i32{{let token=Token{{code:65}};let read=move ()=>{{let fresh=Token{{code:66}};token.code+fresh.code}};if read()!=131 || read()!=131{{return 1}}return 0}}"
    );
    native(&text, "BBA");
}

#[test]
fn resource_closure_mutating_a_copy_payload_snapshot_keeps_owned_call_shared_native() {
    let text = "extern \"C\" fn putchar(code:i32)->i32; resource Token{code:i32;pair:(i32,i32)} impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}} fn copy_edit(token:Token)->i32{var pair=token.pair;pair.0+=1;return pair.0} fn main()->i32{let token=Token{code:65,pair:(10,20)};let read=move ()=>copy_edit(token);if read()!=11 || read()!=11{return 1}return 0}";
    native(text, "A");
}
