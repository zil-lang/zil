//! These mutations begin with real loop-control source. The producer's Flow
//! and cleanup labels cannot replace independent CFG, scope and debt checks.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::{
    CallableExit, CallableTrace, CallableTraces, CandidatePlan, CleanupExitKind, InstructionKind,
    Terminator, TypedModule,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SOURCE: &str = r#"
resource R{code:i32}impl Drop for R{fn drop(self:Self){}}
fn run<F:FnMut(&mut i32)->i32>(f:&mut F,value:&mut i32,n:i32)->i32{
 let outer=R{code:90};var total=0;
 for index in 0..n{
  let local=R{code:index};
  if index==0{continue}
  total+=f(value);
  if index==2{break}
 }
 return total
}
fn main()->i32{var calls=0;var f=move(p:&mut i32)=>{calls+=1;*p+=1;return calls};var value=0;return run(&mut f,&mut value,7)-3+value-2}
"#;

fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "raw195-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn baseline() -> (TypedModule, CandidatePlan) {
    let dir = directory("baseline");
    fs::write(dir.join("source.zil"), SOURCE).unwrap();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "control195.zil", SOURCE);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let module = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
    let candidate = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    zil_verify::verify_module(
        module.clone(),
        candidate.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
    fs::write(dir.join("candidate.wire"), wire).unwrap();
    fs::write(dir.join("typed.txt"), format!("{module:#?}")).unwrap();
    (module, candidate)
}
fn helper(module: &TypedModule) -> usize {
    module
        .functions
        .iter()
        .position(|f| f.callable_contract.is_some())
        .unwrap()
}
fn control_block(module: &TypedModule, kind: CleanupExitKind) -> (usize, usize) {
    let f = helper(module);
    let b = module.functions[f]
        .blocks
        .iter()
        .position(|b| {
            b.instructions.iter().any(
                |i| matches!(i.kind,InstructionKind::ScopeExit{kind:actual,..} if actual==kind),
            )
        })
        .unwrap();
    (f, b)
}
fn reject(
    label: &str,
    module: TypedModule,
    mut candidate: CandidatePlan,
    code: &str,
    analyze_rejects: bool,
) {
    candidate.module_fingerprint = module.fingerprint();
    let bytes =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (module.clone(), candidate.clone())
    );
    let raw = zil_verify::verify_module(
        module.clone(),
        candidate,
        &module.target,
        &Limits::default(),
    )
    .unwrap_err();
    let wire =
        zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default()).unwrap_err();
    let analyzed = zil_analyze::analyze(&module, &Limits::default());
    let dir = directory(label);
    fs::write(dir.join("candidate.wire"), bytes).unwrap();
    fs::write(
        dir.join("verify.txt"),
        format!("raw={raw:#?}\nwire={wire:#?}\n"),
    )
    .unwrap();
    fs::write(dir.join("analyze.txt"), format!("{analyzed:#?}\n")).unwrap();
    for errors in [&raw, &wire] {
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == code),
            "{label}: {errors:?}"
        );
    }
    if analyze_rejects {
        let errors = analyzed.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Analyze && d.category == ResultCategory::SafeReject),
            "{label}: {errors:?}"
        );
    } else {
        analyzed.unwrap();
    }
}

#[test]
fn source_control_edges_have_real_cleanup_and_independent_wire_verification() {
    let (module, _) = baseline();
    let f = &module.functions[helper(&module)];
    assert!(matches!(
        f.callable_contract.as_ref().unwrap().traces,
        CallableTraces::Flow(_)
    ));
    for kind in [CleanupExitKind::Continue, CleanupExitKind::Break] {
        let (_, b) = control_block(&module, kind);
        let block = &f.blocks[b];
        assert!(matches!(block.terminator, Terminator::Jump(_)));
        assert!(
            block
                .instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
        );
        assert!(
            block
                .instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::StorageDead(_)))
        );
    }
    let (_, c) = control_block(&module, CleanupExitKind::Continue);
    let Terminator::Jump(step) = f.blocks[c].terminator else {
        unreachable!()
    };
    assert!(
        f.blocks[step.0 as usize]
            .instructions
            .iter()
            .any(|i| matches!(
                i.kind,
                InstructionKind::Binary {
                    op: zil_ir::BinaryOp::Add,
                    ..
                }
            ))
    );
}

#[test]
fn omitting_scope_exit_after_break_or_continue_does_not_restore_entry_state() {
    let (module, candidate) = baseline();
    for kind in [CleanupExitKind::Break, CleanupExitKind::Continue] {
        let mut changed = module.clone();
        let (f, b) = control_block(&changed, kind);
        let at = changed.functions[f].blocks[b]
            .instructions
            .iter()
            .position(
                |i| matches!(i.kind,InstructionKind::ScopeExit{kind:actual,..} if actual==kind),
            )
            .unwrap();
        changed.functions[f].blocks[b].instructions.remove(at);
        reject(
            &format!("missing-scope-{kind:?}"),
            changed,
            candidate.clone(),
            "E-VERIFY-CLEANUP",
            true,
        );
    }
}

#[test]
fn control_cleanup_cannot_leave_storage_live_or_close_the_wrong_scope() {
    let (module, candidate) = baseline();
    for mutation in 0..2 {
        let mut changed = module.clone();
        let (f, b) = control_block(&changed, CleanupExitKind::Continue);
        if mutation == 0 {
            let at = changed.functions[f].blocks[b]
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::StorageDead(_)))
                .unwrap();
            changed.functions[f].blocks[b].instructions.remove(at);
        } else {
            let root = changed.functions[f]
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| {
                    if let InstructionKind::ScopeEnter { scope } = i.kind {
                        Some(scope)
                    } else {
                        None
                    }
                })
                .unwrap();
            let instruction = changed.functions[f].blocks[b]
                .instructions
                .iter_mut()
                .find(|i| matches!(i.kind, InstructionKind::ScopeExit { .. }))
                .unwrap();
            let InstructionKind::ScopeExit { scope, .. } = &mut instruction.kind else {
                unreachable!()
            };
            *scope = root;
        }
        reject(
            &format!("storage-scope-{mutation}"),
            changed,
            candidate.clone(),
            "E-VERIFY-CLEANUP",
            true,
        );
    }
}

#[test]
fn loop_exit_storage_markers_do_not_discharge_a_missing_owned_drop() {
    let (module, candidate) = baseline();
    for kind in [CleanupExitKind::Break, CleanupExitKind::Continue] {
        let mut changed = module.clone();
        let (f, b) = control_block(&changed, kind);
        let at = changed.functions[f].blocks[b]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
            .unwrap();
        changed.functions[f].blocks[b].instructions.remove(at);
        reject(
            &format!("missing-drop-{kind:?}"),
            changed,
            candidate.clone(),
            "E-VERIFY-RESOURCE",
            true,
        );
    }
}

#[test]
fn a_control_jump_cannot_claim_to_be_a_function_return_cleanup() {
    let (mut module, candidate) = baseline();
    let (f, b) = control_block(&module, CleanupExitKind::Break);
    for i in &mut module.functions[f].blocks[b].instructions {
        if let InstructionKind::ScopeExit { kind, .. } = &mut i.kind {
            *kind = CleanupExitKind::Return;
        }
    }
    // Analyze's lexical scopes ignore exit-reason labels; Verify checks the
    // actual Return/Jump relation independently. Do not claim an Analyze fail.
    reject(
        "return-label-on-jump",
        module,
        candidate,
        "E-VERIFY-CLEANUP",
        false,
    );
}

#[test]
fn changed_break_or_continue_targets_invalidate_the_original_flow_claim() {
    let (module, candidate) = baseline();
    let (f, c) = control_block(&module, CleanupExitKind::Continue);
    let (_, b) = control_block(&module, CleanupExitKind::Break);
    for (source, target) in [(c, b), (b, c)] {
        let mut changed = module.clone();
        changed.functions[f].blocks[source].terminator =
            module.functions[f].blocks[target].terminator.clone();
        reject(
            &format!("changed-jump-{source}"),
            changed,
            candidate.clone(),
            "E-VERIFY-CALLABLE-CONTRACT",
            true,
        );
    }
}

#[test]
fn actual_cyclic_control_and_callbacks_cannot_be_replaced_by_a_finite_claim() {
    let (mut module, candidate) = baseline();
    let f = helper(&module);
    module.functions[f]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces = CallableTraces::Finite(vec![CallableTrace {
        calls: vec![],
        exit: CallableExit::Return,
    }]);
    reject(
        "finite-masquerade",
        module,
        candidate,
        "E-VERIFY-CALLABLE-CONTRACT",
        true,
    );
}

#[test]
fn independent_raw_and_wire_control_checks_keep_exact_work_frontiers() {
    let (module, candidate) = baseline();
    let bytes =
        zil_ir::wire::encode_module_candidate(&module, &candidate, &Limits::default()).unwrap();
    for wire in [false, true] {
        let mut attempts = Vec::new();
        let mut attempt = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            let result = if wire {
                zil_verify::verify_module_bytes(&bytes, &module.target, &limits).map(|_| ())
            } else {
                zil_verify::verify_module(
                    module.clone(),
                    candidate.clone(),
                    &module.target,
                    &limits,
                )
                .map(|_| ())
            };
            let errors = match &result {
                Ok(()) => "[]".into(),
                Err(d) => format!(
                    "[{}]",
                    d.iter().map(|d| d.to_json()).collect::<Vec<_>>().join(",")
                ),
            };
            attempts.push(format!(
                "{{\"steps\":{steps},\"success\":{},\"diagnostics\":{errors}}}",
                result.is_ok()
            ));
            if let Err(d) = &result {
                assert!(
                    d.iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit),
                    "{d:?}"
                );
            }
            result.is_ok()
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        assert!(attempt(high));
        assert!(!attempt(0));
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            if attempt(mid) { high = mid } else { low = mid }
        }
        assert!(!attempt(high - 1));
        assert!(attempt(high));
        assert!(attempt(Limits::default().max_analysis_steps));
        let dir = directory("work-frontier");
        fs::write(dir.join("frontier.json"),format!("{{\"wire\":{wire},\"minimum_steps\":{high},\"n_minus_one_rejects\":true,\"fresh_retry\":true,\"observations\":[{}]}}\n",attempts.join(","))).unwrap();
    }
}
