//! Source-derived field places are reconstructed by independent raw/Wire Verify.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
    json_string,
};
use zil_ir::{
    CallableKind, CandidatePlan, CoreModule, Instruction, InstructionKind as I, LocalId, Type,
    TypedFunction, TypedModule, ValueId,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const PREFIX: &str = "resource R{code:i32}package State<F>{manages r:R;callback:F;value:i32}fn pack<F:FnMut(i32)->i32>(callback:F)->State<F>{return State::<F>{r:R{code:84},callback:callback,value:0}}fn make()->impl FnMut(i32)->i32{let r=R{code:65};var n=0;return move(delta:i32)=>{r.code;n+=delta;return n}}";
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let saved = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = saved
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "callback199-raw-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, saved.is_some())
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn candidate(source: &str) -> (CoreModule, TypedModule, CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw199.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    (core, module, plan)
}
fn pair(
    label: &str,
    source: &str,
    module: TypedModule,
    mut plan: CandidatePlan,
    expected: Option<&str>,
) {
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan.clone())
    );
    let case = Case::new(label);
    fs::write(case.0.join("source.zil"), source).unwrap();
    fs::write(case.0.join("module.txt"), format!("{module:#?}")).unwrap();
    fs::write(case.0.join("candidate.txt"), format!("{plan:#?}")).unwrap();
    fs::write(case.0.join("candidate.wire"), &bytes).unwrap();
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
        match expected {
            None => {
                result.unwrap();
                fs::write(case.0.join(format!("{route}.txt")), "Pass\n").unwrap();
            }
            Some(code) => {
                let errors = result.unwrap_err();
                fs::write(
                    case.0.join(format!("{route}.txt")),
                    format!("{errors:#?}\n"),
                )
                .unwrap();
                assert_eq!(errors[0].phase, Phase::Verify, "{label}: {errors:?}");
                assert_eq!(
                    errors[0].category,
                    ResultCategory::SafeReject,
                    "{label}: {errors:?}"
                );
                assert_eq!(errors[0].code, code, "{label}: {errors:?}");
            }
        }
    }
}
fn main_function(module: &mut TypedModule) -> &mut TypedFunction {
    module
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap()
}
fn first_borrow(function: &mut TypedFunction) -> &mut Instruction {
    function
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::BorrowPackageField { .. }))
        .unwrap()
}

#[test]
fn actual_direct_place_has_no_take_and_untrusted_field_mode_changes_reject() {
    let source = format!(
        "{PREFIX}fn main()->i32{{var state=pack(make());return state.callback(1)+state.callback(1)-3}}"
    );
    let (_, original, plan) = candidate(&source);
    let f = original
        .functions
        .iter()
        .find(|f| f.name == "main")
        .unwrap();
    assert_eq!(
        f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .filter(|i| matches!(
                i.kind,
                I::CallClosureBorrowed {
                    kind: CallableKind::FnMut,
                    ..
                }
            ))
            .count(),
        2
    );
    assert!(
        !f.blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, I::TakePackageField { .. }))
    );
    pair(
        "place-control",
        &source,
        original.clone(),
        plan.clone(),
        None,
    );
    for (variant, label, code) in [
        (0, "wrong-field", "E-VERIFY-PACKAGE"),
        (1, "shared-permission", "E-VERIFY-CLOSURE-CALL"),
        (2, "borrowed-once-mode", "E-VERIFY-CLOSURE-CALL"),
        (3, "delete-state-cleanup", "E-VERIFY-RESOURCE"),
    ] {
        let mut module = original.clone();
        let f = main_function(&mut module);
        match variant {
            0 => {
                let I::BorrowPackageField { index, .. } = &mut first_borrow(f).kind else {
                    unreachable!()
                };
                *index = 0;
            }
            1 => {
                let i = first_borrow(f);
                let result = i.result.unwrap();
                let I::BorrowPackageField { mutable, .. } = &mut i.kind else {
                    unreachable!()
                };
                *mutable = false;
                let Type::Address { mutable, .. } = &mut f.values[result.0 as usize] else {
                    unreachable!()
                };
                *mutable = false;
            }
            2 => {
                let i = f
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, I::CallClosureBorrowed { .. }))
                    .unwrap();
                let I::CallClosureBorrowed { kind, .. } = &mut i.kind else {
                    unreachable!()
                };
                *kind = CallableKind::FnOnce;
            }
            _ => {
                let state =
                    LocalId(f.locals.iter().position(|l| l.name == "state").unwrap() as u32);
                let mut removed = 0;
                for b in &mut f.blocks {
                    b.instructions.retain(|i| {
                        let keep = !matches!(i.kind, I::DropPackage { local } if local == state);
                        removed += usize::from(!keep);
                        keep
                    });
                }
                assert_eq!(removed, 1);
            }
        }
        pair(label, &source, module, plan.clone(), Some(code));
    }
}

#[test]
fn stale_field_address_and_retargeted_dead_parent_are_rejected() {
    let source = format!(
        "{PREFIX}fn main()->i32{{var state=pack(make());state.callback(1);state.callback=make();return state.callback(1)-1}}"
    );
    let (_, original, plan) = candidate(&source);
    pair(
        "replacement-control",
        &source,
        original.clone(),
        plan.clone(),
        None,
    );
    let mut module = original;
    let f = main_function(&mut module);
    let mut first = None;
    let mut changed = 0;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let I::CallClosureBorrowed { callee, .. } = &mut i.kind {
            if let Some(old) = first {
                *callee = old;
                changed += 1;
            } else {
                first = Some(*callee);
            }
        }
    }
    assert_eq!(changed, 1);
    pair(
        "stale-replaced-field",
        &source,
        module,
        plan,
        Some("E-BORROW-CONFLICT"),
    );

    let source = format!(
        "{PREFIX}fn main()->i32{{var state=pack(make());var other=pack(make());let moved=other;return state.callback(1)-1}}"
    );
    let (_, original, plan) = candidate(&source);
    pair(
        "parent-control",
        &source,
        original.clone(),
        plan.clone(),
        None,
    );
    let mut module = original;
    let f = main_function(&mut module);
    let other = LocalId(f.locals.iter().position(|l| l.name == "other").unwrap() as u32);
    let I::BorrowPackageField { local, .. } = &mut first_borrow(f).kind else {
        unreachable!()
    };
    *local = other;
    pair(
        "dead-parent",
        &source,
        module,
        plan,
        Some("E-VERIFY-RESOURCE"),
    );
}

#[test]
fn actual_once_field_cannot_be_copied_or_borrowed_as_shared() {
    let source = "resource R{n:i32}package State<F>{manages r:R;callback:F}fn pack<F:FnOnce()->i32>(callback:F)->State<F>{return State::<F>{r:R{n:1},callback:callback}}fn make()->impl FnOnce()->i32{let r=R{n:2};return move()=>{let taken=give r;return taken.n}}fn main()->i32{let state=pack(make());return state.callback()-2}";
    let (_, original, plan) = candidate(source);
    pair("once-control", source, original.clone(), plan.clone(), None);
    let mut copied = original.clone();
    let i = main_function(&mut copied)
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::TakePackageField { .. }))
        .unwrap();
    let I::TakePackageField { local, index } = i.kind else {
        unreachable!()
    };
    i.kind = I::ReadPackageField { local, index };
    pair(
        "once-without-take",
        source,
        copied,
        plan.clone(),
        Some("E-VERIFY-PACKAGE"),
    );
    let mut borrowed = original;
    let f = main_function(&mut borrowed);
    let (block, position) = f
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, I::TakePackageField { .. }))
                .map(|p| (b, p))
        })
        .unwrap();
    let take = &f.blocks[block].instructions[position];
    let I::TakePackageField { local, index } = take.kind else {
        unreachable!()
    };
    let inner = f.values[take.result.unwrap().0 as usize].clone();
    let span = take.span;
    let address = ValueId(f.values.len() as u32);
    f.values.push(Type::Address {
        mutable: false,
        inner: Box::new(inner),
    });
    f.blocks[block].instructions.insert(
        position,
        Instruction {
            result: Some(address),
            kind: I::BorrowPackageField {
                local,
                index,
                mutable: false,
            },
            span,
        },
    );
    let call = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::CallClosureOnce { .. }))
        .unwrap();
    let I::CallClosureOnce { arguments, .. } = &mut call.kind else {
        unreachable!()
    };
    call.kind = I::CallClosureBorrowed {
        callee: address,
        kind: CallableKind::Fn,
        arguments: std::mem::take(arguments),
    };
    pair(
        "once-borrowed-shared",
        source,
        borrowed,
        plan,
        Some("E-VERIFY-CLOSURE-CALL"),
    );
}

#[test]
fn generic_modes_are_preserved_when_actual_environment_is_fn() {
    for mode in ["FnMut", "FnOnce"] {
        let source = format!(
            "resource R{{n:i32}}package State<F>{{manages r:R;callback:F}}fn pack<F:{mode}()->i32>(callback:F)->State<F>{{return State::<F>{{r:R{{n:1}},callback:callback}}}}fn make()->impl Fn()->i32{{let r=R{{n:2}};return move()=>r.n}}fn run<G:{mode}()->i32>(state:State<G>)->i32{{var current=state;return current.callback()}}fn main()->i32{{return run(pack(make()))-2}}"
        );
        let (_, module, plan) = candidate(&source);
        let kind = if mode == "FnMut" {
            CallableKind::FnMut
        } else {
            CallableKind::FnOnce
        };
        assert_eq!(module.closures[0].kind, CallableKind::Fn);
        let invoked = module
            .functions
            .iter()
            .filter(|f| f.name.starts_with("@module0:helper"))
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .filter(|i| match i.kind {
                I::CallClosureBorrowed { kind: actual, .. } => actual == kind,
                I::CallClosureOnce { .. } => kind == CallableKind::FnOnce,
                _ => false,
            })
            .count();
        assert_eq!(invoked, 1, "{mode}");
        pair(mode, &source, module, plan, None);
    }
}

#[test]
fn direct_field_work_is_bounded_in_all_three_fresh_phases() {
    let source = format!(
        "{PREFIX}fn run<G:FnMut(i32)->i32>(state:State<G>,count:i32)->i32{{var current=state;var i=0;var sum=0;while i<count{{sum+=current.callback(1);i+=1}}return sum}}fn main()->i32{{return run(pack(make()),3)-6}}"
    );
    let (core, module, plan) = candidate(&source);
    let case = Case::new("budgets");
    fs::write(case.0.join("source.zil"), &source).unwrap();
    let mut rows = Vec::new();
    for phase in [Phase::Types, Phase::Analyze, Phase::Verify] {
        let probe = |work| -> Result<(), Vec<Diagnostic>> {
            let limits = Limits {
                max_analysis_steps: work,
                ..Limits::default()
            };
            match phase {
                Phase::Types => {
                    zil_types::check(&core, &module.target, &limits).map(|m| assert_eq!(m, module))
                }
                Phase::Analyze => {
                    zil_analyze::analyze(&module, &limits).map(|p| assert_eq!(p, plan))
                }
                _ => {
                    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
                        .map(|_| ())
                }
            }
        };
        let mut high = Limits::default().max_analysis_steps;
        let mut low = 0;
        probe(high).unwrap();
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match probe(middle) {
                Ok(()) => high = middle,
                Err(errors) => {
                    assert!(
                        errors.iter().any(
                            |d| d.phase == phase && d.category == ResultCategory::ResourceLimit
                        ),
                        "{phase:?}/{middle}: {errors:?}"
                    );
                    low = middle;
                }
            }
        }
        probe(high).unwrap();
        let errors = probe(high - 1).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit),
            "{errors:?}"
        );
        probe(high).unwrap();
        rows.push(format!(
            "{{\"phase\":\"{phase:?}\",\"N\":{high},\"below\":{},\"fresh_retry\":\"Pass\"}}",
            json_string(&format!("{errors:?}"))
        ));
    }
    fs::write(
        case.0.join("boundaries.json"),
        format!("[{}]\n", rows.join(",")),
    )
    .unwrap();
}
