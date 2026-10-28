//! Independently checked storage and access qualifications for Copy field addresses.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_ir::{CallableKind, CandidatePlan, InstructionKind as I, TypedModule};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const MODEL: &str =
    "resource R{n:i32}package P{manages r:R;x:i32;y:i32}fn make()->P{return P{r:R{n:1},x:20,y:1}}";
fn directory(label: &str) -> PathBuf {
    let p = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "fields192-raw-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn candidate(source: &str) -> (TypedModule, CandidatePlan) {
    let u = SourceUnit::new(FileId(0), ModuleId(0), "raw192.zil", source);
    let ast = zil_syntax::parse(&u).into_result().unwrap();
    let core = zil_expand::expand(&u, &ast).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    (module, plan)
}
fn pair(
    label: &str,
    source: &str,
    module: TypedModule,
    mut plan: CandidatePlan,
    code: Option<&str>,
) {
    let limits = Limits::default();
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan.clone())
    );
    let p = directory(label);
    fs::write(p.join("source.zil"), source).unwrap();
    fs::write(p.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(p.join("candidate.txt"), format!("{plan:#?}")).unwrap();
    fs::write(p.join("candidate.bin"), &bytes).unwrap();
    for (route, result) in [
        (
            "raw",
            zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &module.target, &limits),
        ),
    ] {
        match code {
            None => {
                result.unwrap();
                fs::write(p.join(format!("{route}.txt")), "Pass\n").unwrap();
            }
            Some(code) => {
                let errors = result.unwrap_err();
                fs::write(p.join(format!("{route}.txt")), format!("{errors:#?}")).unwrap();
                assert_eq!(
                    errors[0].category,
                    ResultCategory::SafeReject,
                    "{label}: {errors:?}"
                );
                assert_eq!(errors[0].phase, Phase::Verify);
                assert_eq!(errors[0].code, code, "{label}: {errors:?}");
            }
        }
    }
}
#[test]
fn copy_field_shape_permission_parent_mutability_and_hold_are_reconstructed() {
    let source =
        format!("{MODEL}fn main()->i32{{var state=make();let p=&mut state.x;*p+=1;return *p-21}}");
    let (module, plan) = candidate(&source);
    pair(
        "valid-copy-field",
        &source,
        module.clone(),
        plan.clone(),
        None,
    );
    let main = module
        .functions
        .iter()
        .position(|f| f.name == "main")
        .unwrap();
    let (local, value) = module.functions[main]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let I::BorrowPackageField { local, .. } = i.kind {
                Some((local, i.result.unwrap()))
            } else {
                None
            }
        })
        .unwrap();
    let mut changed_plan = plan.clone();
    assert!(
        !changed_plan.functions[main].holds[value.0 as usize]
            .roots
            .is_empty()
    );
    changed_plan.functions[main].holds[value.0 as usize]
        .roots
        .clear();
    pair(
        "erased-parent-hold",
        &source,
        module.clone(),
        changed_plan,
        Some("E-VERIFY-HOLD"),
    );
    for (label, variant, code) in [
        ("bad-field-index", 0, "E-VERIFY-PACKAGE"),
        ("borrow-owned-field", 1, "E-VERIFY-PACKAGE"),
        ("permission-mismatch", 2, "E-VERIFY-TYPE"),
        ("immutable-parent", 3, "E-VERIFY-MUTABILITY"),
    ] {
        let mut changed = module.clone();
        let f = &mut changed.functions[main];
        if variant == 3 {
            f.locals[local.0 as usize].mutable = false;
        } else {
            let i = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, I::BorrowPackageField { .. }))
                .unwrap();
            if let I::BorrowPackageField { index, mutable, .. } = &mut i.kind {
                match variant {
                    0 => *index = u32::MAX,
                    1 => *index = 0,
                    _ => *mutable = false,
                }
            }
        }
        pair(label, &source, changed, plan.clone(), Some(code));
    }
}
#[test]
fn direct_copy_reads_cannot_be_retargeted_inside_an_exclusive_field_window() {
    let source = format!(
        "{MODEL}fn main()->i32{{var state=make();let p=&mut state.x;let other=state.y;*p+=1;return *p+other-22}}"
    );
    let (mut module, plan) = candidate(&source);
    let f = module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let mut changed = 0;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::ReadPackageField { index, .. } = &mut i.kind {
            assert_eq!(*index, 2);
            *index = 1;
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    pair(
        "current-read-overlaps",
        &source,
        module,
        plan,
        Some("E-BORROW-CONFLICT"),
    );
}
#[test]
fn parent_initialization_and_cleanup_cannot_be_skipped_by_copy_field_borrows() {
    let source = format!("{MODEL}fn main()->i32{{var state=make();let p=&state.x;return *p-20}}");
    let (module, plan) = candidate(&source);
    for variant in 0..2 {
        let mut changed = module.clone();
        let f = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let block = f
            .blocks
            .iter_mut()
            .find(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.kind, I::BorrowPackageField { .. }))
            })
            .unwrap();
        let at = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, I::BorrowPackageField { .. }))
            .unwrap();
        if variant == 0 {
            let cleanup = block
                .instructions
                .iter()
                .find(|i| matches!(i.kind, I::DropPackage { .. }))
                .unwrap()
                .clone();
            block.instructions.insert(at, cleanup);
        } else {
            let local = if let I::BorrowPackageField { local, .. } = block.instructions[at].kind {
                local
            } else {
                unreachable!()
            };
            let store = block
                .instructions
                .iter()
                .position(|i| matches!(i.kind,I::Store{local:target,..} if target==local))
                .unwrap();
            block.instructions.remove(store);
        }
        pair(
            if variant == 0 {
                "dropped-parent"
            } else {
                "uninitialized-parent"
            },
            &source,
            changed,
            plan.clone(),
            Some("E-VERIFY-INITIALIZATION"),
        );
    }
}
#[test]
fn owned_package_environment_does_not_become_a_shared_entry_by_relabeling() {
    let source = format!(
        "{MODEL}fn main()->i32{{var state=make();var f=move()=>{{let p=&mut state.x;*p+=1;return *p}};return f()+f()-43}}"
    );
    let (mut module, plan) = candidate(&source);
    assert_eq!(module.closures.len(), 1);
    assert_eq!(module.closures[0].kind, CallableKind::FnMut);
    module.closures[0].kind = CallableKind::Fn;
    let errors = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    let p = directory("shared-view-analysis");
    fs::write(p.join("source.zil"), &source).unwrap();
    fs::write(p.join("analysis.txt"), format!("{errors:#?}")).unwrap();
    assert_eq!(errors[0].phase, Phase::Analyze);
    assert_eq!(errors[0].category, ResultCategory::SafeReject);
    assert!(
        errors[0]
            .message
            .contains("Shared entry cannot change a Package capture"),
        "{errors:?}"
    );
    pair(
        "shared-owned-package-entry",
        &source,
        module,
        plan,
        Some("E-VERIFY-RESOURCE"),
    );
}
fn limit_record(label: &str, low: usize, high: usize, errors: &[Diagnostic]) {
    let p = directory(label);
    fs::write(
        p.join("boundary.json"),
        format!(
            "{{\"status\":\"Pass\",\"rejected\":{low},\"accepted\":{high},\"default_steps\":{}}}\n",
            Limits::default().max_analysis_steps
        ),
    )
    .unwrap();
    fs::write(p.join("rejection.txt"), format!("{errors:#?}")).unwrap();
}
#[test]
fn actual_copy_field_checks_have_analysis_and_verification_work_boundaries() {
    let source = format!(
        "{MODEL}fn main()->i32{{var state=make();let a=&mut state.x;let b=&state.y;*a+=*b;return *a-21}}"
    );
    let (module, plan) = candidate(&source);
    for verify in [false, true] {
        let works = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            if verify {
                zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
                    .map(|_| ())
            } else {
                zil_analyze::analyze(&module, &limits).map(|_| ())
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        assert!(works(high).is_ok());
        while high - low > 1 {
            let mid = low + (high - low) / 2;
            if works(mid).is_ok() {
                high = mid;
            } else {
                low = mid;
            }
        }
        let errors = works(low).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.category == ResultCategory::ResourceLimit),
            "{errors:?}"
        );
        assert!(works(high).is_ok());
        assert!(works(Limits::default().max_analysis_steps).is_ok());
        limit_record(
            if verify {
                "verify-work"
            } else {
                "analyze-work"
            },
            low,
            high,
            &errors,
        );
    }
}
