//! Non-author tests for the frozen238 Product assignment transaction.
//! No producer or kernel implementation is imported as a shared safety oracle.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{
    CandidatePlan, HoldRoot, Instruction, InstructionKind as I, LocalId, Type, TypedModule,
    ValueHold, ValueId,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const MODEL: &str = r#"
extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
package P{manages a:Mark;tag:i32;manages b:Mark}
fn make(code:i32)->P{return P{a:Mark{code:code},tag:code,b:Mark{code:code+1}}}
fn relay(owned:P)->P{return owned}
fn after(owned:P,n:i32)->P{return owned}
fn before(n:i32,owned:P)->P{return owned}
fn sink(owned:P){}
fn tag(p:&P)->i32{return p.tag}
enum Fault{Bad}
fn fail()->Fault!i32{return Fault.Bad}
"#;

fn dir(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "review238-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn unit(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review238.zil", text)
}
fn source(label: &str, body: &str, accept: bool) {
    let out = dir(label);
    let text = format!("{MODEL}\n{body}");
    fs::write(out.join("source.zil"), &text).unwrap();
    let report = compile_source(
        &unit(&text),
        &CompileOptions {
            stop_after: Stage::Verify,
            emit_dir: Some(out.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(out.join("report.json"), report.to_json()).unwrap();
    if accept {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
    } else {
        assert!(
            matches!(report.phase, Phase::Types | Phase::Analyze | Phase::Verify),
            "{label}: rejected before the tested type/ownership boundary: {}",
            report.to_json()
        );
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{label}: {}",
            report.to_json()
        );
    }
    assert!(!report.artifacts.contains_key("executable"));
}
fn native(label: &str, body: &str, expected: &str, exit: i32) {
    let text = format!("{MODEL}\n{body}");
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = dir(label);
        fs::write(out.join("source.zil"), &text).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_source(
            &unit(&text),
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, backend);
        assert!(codegen.fallback.is_none());
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(out.join("stdout"), &output.stdout).unwrap();
        fs::write(out.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}
fn typed(body: &str) -> TypedModule {
    let text = format!("{MODEL}\n{body}");
    let u = unit(&text);
    let syntax = zil_syntax::parse(&u).into_result().unwrap();
    let core = zil_expand::expand(&u, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn reject(result: Result<impl Sized, Vec<Diagnostic>>, label: &str) {
    let errors = match result {
        Ok(_) => panic!("{label}: invalid transaction accepted"),
        Err(e) => e,
    };
    assert!(
        errors.iter().any(|e| matches!(
            e.category,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        )),
        "{label}: {errors:#?}"
    );
    assert!(
        errors
            .iter()
            .all(|e| !e.code.starts_with("E-VERIFY-PLAN") && e.code != "E-VERIFY-HOLD"),
        "{label}: candidate plumbing rejected before executable safety: {errors:#?}"
    );
}
fn raw_wire(label: &str, module: TypedModule, mut plan: CandidatePlan) {
    let out = dir(label);
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    fs::write(out.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(out.join("candidate.txt"), format!("{plan:#?}")).unwrap();
    fs::write(out.join("input.wire"), &bytes).unwrap();
    // Both independent Verify routes run before the optional producer check.
    let raw = zil_verify::verify_module(module.clone(), plan, &module.target, &limits);
    let wire = zil_verify::verify_module_bytes(&bytes, &module.target, &limits);
    fs::write(out.join("raw.txt"), format!("{raw:#?}")).unwrap();
    fs::write(out.join("wire.txt"), format!("{wire:#?}")).unwrap();
    reject(raw, &format!("{label}/raw"));
    reject(wire, &format!("{label}/wire"));
    let analyze = zil_analyze::analyze(&module, &limits);
    fs::write(out.join("analyze.txt"), format!("{analyze:#?}")).unwrap();
    reject(analyze, &format!("{label}/analyze"));
}
fn control(body: &str) -> (TypedModule, CandidatePlan, usize, LocalId) {
    let module = typed(body);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let fi = module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let current = module.functions[fi]
        .locals
        .iter()
        .find(|l| l.name == "current")
        .unwrap()
        .id;
    (module, plan, fi, current)
}
const RAW: &str = "fn main()->i32{var current=make(65);current=make(67);sink(current);return 0}";

#[test]
fn ordinary_and_self_replacement_preserve_shell_age_and_defer_order() {
    native(
        "self-age",
        "fn main()->i32{var current=make(65);defer{emit(81)}let late=Mark{code:76};current=relay(current);return 0}",
        "LQAB",
        0,
    );
    native(
        "independent-age",
        "fn main()->i32{var current=make(65);defer{emit(81)}let late=Mark{code:76};current=make(67);return 0}",
        "ABLQCD",
        0,
    );
    native(
        "no-defer-relay",
        "fn main()->i32{var current=make(65);let late=Mark{code:76};current=relay(current);return 0}",
        "LAB",
        0,
    );
}

#[test]
fn ordinary_static_templates_and_explicit_type_apply_keep_the_same_transaction() {
    native(
        "generic-self",
        "fn bounce<T:Copy>(owned:P,n:T)->P{return owned}fn main()->i32{var current=make(65);defer{emit(81)}let late=Mark{code:76};current=bounce::<i32>(current,7);current=bounce(current,8);return 0}",
        "LQAB",
        0,
    );
    source(
        "generic-other-copy-use",
        "fn bounce<T:Copy>(owned:P,n:T)->P{return owned}fn main()->i32{var current=make(65);current=bounce::<i32>(current,current.tag);return 0}",
        false,
    );
    native(
        "phantom-family-self",
        "package Twin{manages a:Mark;tag:i32;manages b:Mark}package Root<T:StoreElement>{child:P;tag:i32}fn wrapped<T:StoreElement>()->Root<T>{return Root::<T>{child:make(65),tag:7}}fn bounce<T:StoreElement>(owned:Root<T>)->Root<T>{return owned}fn main()->i32{var current=wrapped::<Twin>();defer{emit(81)}let late=Mark{code:76};current=bounce(current);current=bounce::<Twin>(current);return current.tag-7}",
        "LQAB",
        0,
    );
    source(
        "phantom-family-wrong-argument",
        "package Twin{manages a:Mark;tag:i32;manages b:Mark}package Root<T:StoreElement>{child:P;tag:i32}fn wrapped<T:StoreElement>()->Root<T>{return Root::<T>{child:make(65),tag:7}}fn bounce<T:StoreElement>(owned:Root<T>)->Root<T>{return owned}fn main()->i32{var current=wrapped::<Twin>();current=bounce::<P>(current);return 0}",
        false,
    );
}

#[test]
fn rhs_failure_preserves_only_responsibility_not_explicitly_transferred() {
    native(
        "fail-before-self",
        "fn main()->Fault!i32{var current=make(65);defer{emit(81)}let late=Mark{code:76};current=before(fail()?,current);return 0}",
        "LQAB",
        1,
    );
    native(
        "fail-after-self",
        "fn main()->Fault!i32{var current=make(65);defer{emit(81)}let late=Mark{code:76};current=after(current,fail()?);return 0}",
        "ABLQ",
        1,
    );
    native(
        "fail-independent",
        "fn main()->Fault!i32{var current=make(65);defer{emit(81)}let late=Mark{code:76};current=after(make(67),fail()?);return 0}",
        "CDLQAB",
        1,
    );
}

#[test]
fn legal_partial_shell_refill_cleans_only_remaining_old_fields() {
    native(
        "partial-independent",
        "fn main()->i32{var current=make(65);defer{emit(81)}let late=Mark{code:76};let detached=give current.a;current=make(67);return 0}",
        "BALQCD",
        0,
    );
}

#[test]
fn scope_shadow_branch_and_loop_keep_real_binding_and_activation() {
    native(
        "shadow",
        "fn main()->i32{var current=make(65);defer{emit(81)}let late=Mark{code:76};if true{var current=make(67);current=relay(current);}current=relay(current);return 0}",
        "CDLQAB",
        0,
    );
    native(
        "loop",
        "fn main()->i32{var current=make(65);defer{emit(81)}let late=Mark{code:76};var i=0;while i<3{current=relay(current);defer{emit(88)}let each=Mark{code:73};i+=1;if i<3{continue}break}return 0}",
        "IXIXIXLQAB",
        0,
    );
    native(
        "argument-shadow",
        "fn main()->i32{var current=make(65);defer{emit(81)}let late=Mark{code:76};current=before(((current:i32)=>current)(7),current);return 0}",
        "LQAB",
        0,
    );
    native(
        "generated-binding",
        "fn main()->i32{var current=make(65);defer{emit(81)}let late=Mark{code:76};#for(i in 0..2) {var current=make(67+2*i);current=relay(current);}return 0}",
        "EFCDLQAB",
        0,
    );
    for flag in ["true", "false"] {
        native(
            "branch-complete",
            &format!(
                "fn main()->i32{{var current=make(65);defer{{emit(81)}}let late=Mark{{code:76}};if {flag}{{current=make(67)}}else{{current=relay(current)}}return 0}}"
            ),
            if flag == "true" { "ABLQCD" } else { "LQAB" },
            0,
        );
    }
}

#[test]
fn normal_move_and_maybe_moved_shell_cannot_be_resurrected_or_copy_read() {
    for (label, body) in [
        (
            "copy-after-move",
            "fn main()->i32{var current=make(65);let moved=current;let read=current.tag;current=make(67);return read}",
        ),
        (
            "replace-after-move",
            "fn main()->i32{var current=make(65);let moved=current;current=make(67);return 0}",
        ),
        (
            "replace-after-join",
            "fn run(flag:bool)->i32{var current=make(65);if flag{sink(current)}current=make(67);return current.tag}fn main()->i32{return run(true)}",
        ),
        (
            "repeat-loop-move",
            "fn main()->i32{var current=make(65);var i=0;while i<3{sink(current);i+=1;if i<3{continue}}current=make(67);return 0}",
        ),
        (
            "self-on-partial",
            "fn main()->i32{var current=make(65);let detached=give current.a;current=relay(current);return 0}",
        ),
        (
            "reconstruction-is-not-creation",
            "fn origin(owned:P)->Mark contract{result(create)}{var current=owned;current=relay(current);return give current.a}fn main()->i32{return 0}",
        ),
    ] {
        source(label, body, false);
    }
}

#[test]
fn self_argument_cannot_enable_copy_read_through_another_argument() {
    source(
        "direct-copy-other-arg",
        "fn main()->i32{var current=make(65);current=after(current,current.tag);return 0}",
        false,
    );
    source(
        "later-argument-capture",
        "fn main()->i32{var current=make(65);let peek=()=>current.tag;current=after(current,peek());return 0}",
        false,
    );
    source(
        "later-desc-alias",
        "fn main()->i32{var current=make(65);let saved=current.a;current=after(current,saved.code);return 0}",
        false,
    );
}

#[test]
fn whole_product_and_field_windows_stay_live_until_actual_last_use() {
    for (label, body) in [
        (
            "whole-live",
            "fn main()->i32{var current=make(65);let view=&current;current=relay(current);return tag(view)}",
        ),
        (
            "desc-live",
            "fn main()->i32{var current=make(65);let old=current.a;current=make(67);return old.code}",
        ),
        (
            "copy-field-live",
            "fn main()->i32{var current=make(65);let old=&current.tag;current=relay(current);return *old}",
        ),
    ] {
        source(label, body, false);
    }
    source(
        "whole-ended",
        "fn main()->i32{var current=make(65);let view=&current;let snapshot=tag(view);current=relay(current);return snapshot-65}",
        true,
    );
    source(
        "copy-snapshot",
        "fn main()->i32{var current=make(65);let snapshot=current.tag;current=relay(current);return snapshot-65}",
        true,
    );
}

#[test]
fn independent_prepare_is_complete_before_all_old_cleanup_and_all_publication() {
    let (module, _, fi, current) = control(RAW);
    let mut found = false;
    for block in &module.functions[fi].blocks {
        let drops: Vec<_> = block
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(i, op)| {
                matches!(op.kind,I::DropPackageField{local,..}if local==current).then_some(i)
            })
            .collect();
        let stores: Vec<_> = block
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(i, op)| {
                matches!(op.kind,I::StorePackageField{local,..}if local==current).then_some(i)
            })
            .collect();
        if stores.is_empty() {
            continue;
        }
        found = true;
        assert_eq!(
            drops.len(),
            2,
            "two old owning fields must be cleared before publish"
        );
        assert_eq!(stores.len(), 3, "Copy companion is also published");
        assert!(
            drops.iter().all(|d| stores.iter().all(|s| d < s)),
            "new Copy/owning fields published before old cleanup"
        );
        let indices: Vec<_> = drops
            .iter()
            .map(|&i| match block.instructions[i].kind {
                I::DropPackageField { index, .. } => index,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(indices, vec![0, 2], "old fields clear in declaration order");
        assert!(
            !block.instructions[drops[0]..=stores[2]]
                .iter()
                .any(|op| matches!(
                    op.kind,
                    I::Call { .. } | I::CallClosureOnce { .. } | I::RegisterDefer { .. }
                )),
            "callback or registration inside no-failure commit"
        );
    }
    assert!(
        found,
        "whole assignment must use the original shell's field publication"
    );
    assert_eq!(
        module.functions[fi]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .filter(|op| matches!(op.kind,I::Store{local,..}if local==current))
            .count(),
        1,
        "shell constructed exactly once"
    );
}

#[test]
fn raw_duplicate_take_and_wrong_same_type_target_reject_without_prior_analyze() {
    let (mut module, plan, fi, current) = control(
        "fn main()->i32{var current=make(65);current=relay(current);sink(current);return 0}",
    );
    let mut found = false;
    for block in &mut module.functions[fi].blocks {
        let takes: Vec<_> = block
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(i, op)| {
                matches!(op.kind,I::TakePackageField{local,..}if local==current).then_some(i)
            })
            .collect();
        if takes.len() >= 2 {
            let result = block.instructions[takes[1]].result;
            let span = block.instructions[takes[1]].span;
            block.instructions[takes[1]] = Instruction {
                result,
                span,
                kind: I::TakePackageField {
                    local: current,
                    index: 0,
                },
            };
            found = true;
            break;
        }
    }
    assert!(found, "self extraction must take each actual owner once");
    raw_wire("duplicate-self-take", module, plan);
    let (mut module, plan, fi, current) = control(
        "fn main()->i32{var other=make(69);var current=make(65);current=make(67);sink(current);sink(other);return 0}",
    );
    let other = module.functions[fi]
        .locals
        .iter()
        .find(|l| l.name == "other")
        .unwrap()
        .id;
    let store = module.functions[fi]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|op| matches!(op.kind,I::StorePackageField{local,index:0,..}if local==current))
        .unwrap();
    if let I::StorePackageField { local, .. } = &mut store.kind {
        *local = other
    }
    raw_wire("same-type-wrong-target", module, plan);
}

#[test]
fn raw_old_debt_and_incomplete_publication_are_checked_independently() {
    let (mut module, plan, fi, current) = control(RAW);
    for block in &mut module.functions[fi].blocks {
        if let Some(i) = block
            .instructions
            .iter()
            .position(|op| matches!(op.kind,I::DropPackageField{local,index:0}if local==current))
        {
            block.instructions.remove(i);
            break;
        }
    }
    raw_wire("publish-over-old-live-owner", module, plan);
    let (mut module, plan, fi, current) = control(RAW);
    for block in &mut module.functions[fi].blocks {
        if let Some(i) = block.instructions.iter().position(
            |op| matches!(op.kind,I::StorePackageField{local,index:2,..}if local==current),
        ) {
            block.instructions.remove(i);
            break;
        }
    }
    raw_wire("missing-publish-owner", module, plan);
}

#[test]
fn raw_shared_borrow_of_half_extracted_shell_requires_all_children_live() {
    let (mut module, mut plan, fi, current) = control(
        "fn main()->i32{var current=make(65);current=relay(current);sink(current);return 0}",
    );
    let f = &mut module.functions[fi];
    let id = ValueId(f.values.len() as u32);
    f.values.push(Type::Address {
        inner: Box::new(f.locals[current.0 as usize].ty.clone()),
        mutable: false,
    });
    plan.functions[fi].holds.push(ValueHold {
        value: id,
        roots: vec![HoldRoot::Local(current)],
    });
    let mut found = false;
    for block in &mut f.blocks {
        if let Some(i) = block
            .instructions
            .iter()
            .position(|op| matches!(op.kind,I::TakePackageField{local,index:0}if local==current))
        {
            block.instructions.insert(
                i + 1,
                Instruction {
                    result: Some(id),
                    kind: I::BorrowLocal { local: current },
                    span: f.span,
                },
            );
            found = true;
            break;
        }
    }
    assert!(found);
    raw_wire("borrow-half-extracted-shell", module, plan);
}

#[test]
fn raw_field_publication_cannot_reopen_dead_shell_or_read_after_whole_move() {
    let (mut module, plan, fi, current) = control(RAW);
    for block in &mut module.functions[fi].blocks {
        if let Some(i) = block.instructions.iter().position(|op| {
            matches!(op.kind,
            I::StorePackageField { local, .. } if local == current)
        }) {
            block.instructions.insert(
                i,
                Instruction {
                    result: None,
                    kind: I::DropPackage { local: current },
                    span: block.instructions[i].span,
                },
            );
            break;
        }
    }
    raw_wire("publish-after-whole-shell-death", module, plan);
    let (mut module, mut plan, fi, current) = control(RAW);
    let f = &mut module.functions[fi];
    let id = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    plan.functions[fi].holds.push(ValueHold {
        value: id,
        roots: vec![],
    });
    let mut found = false;
    for block in &mut f.blocks {
        if let Some(i) = block.instructions.iter().position(|op| {
            matches!(op.kind,
            I::TakeLocal { local } if local == current)
        }) {
            block.instructions.insert(
                i + 1,
                Instruction {
                    result: Some(id),
                    kind: I::ReadPackageField {
                        local: current,
                        index: 1,
                    },
                    span: f.span,
                },
            );
            found = true;
            break;
        }
    }
    assert!(found, "ordinary whole-name transfer must remain TakeLocal");
    raw_wire("copy-after-normal-whole-move", module, plan);
}

#[test]
fn raw_partial_publication_does_not_become_complete_at_join_or_backedge() {
    for (label, body) in [
        (
            "phi-incomplete",
            "fn main()->i32{var current=make(65);if true{current=make(67)}else{current=make(69)}sink(current);return 0}",
        ),
        (
            "backedge-incomplete",
            "fn main()->i32{var current=make(65);var i=0;while i<2{current=make(67);i+=1;}sink(current);return 0}",
        ),
    ] {
        let (mut module, plan, fi, current) = control(body);
        let mut found = false;
        for block in &mut module.functions[fi].blocks {
            if let Some(i) = block.instructions.iter().position(|op| {
                matches!(op.kind,
                I::StorePackageField { local, index: 0, .. } if local == current)
            }) {
                block.instructions.remove(i);
                found = true;
                break;
            }
        }
        assert!(found);
        raw_wire(label, module, plan);
    }
}
