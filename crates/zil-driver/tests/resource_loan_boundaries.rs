//! Independent review of root integration delta 64406b9 at exact 088a389.
use std::{
    collections::BTreeMap,
    fs,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, CompileReport, ProjectSources, Stage, compile_project, compile_source,
};
use zil_ir::{
    CandidatePlan, Constant, HoldRoot, Instruction, InstructionKind as I, LocalDecl, LocalId, Type,
    TypedModule,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "resource_loan_boundaries_review_loans.zil",
        text,
    )
}
fn native(build: impl FnOnce(&CompileOptions) -> CompileReport, want: i64) {
    let temp = std::env::temp_dir().join(format!(
        "zil-review045-loans-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&temp).unwrap();
    let report = build(&CompileOptions {
        stop_after: Stage::Build,
        entry: Some("answer".into()),
        output: Some(temp.join("o0")),
        emit_dir: Some(temp.join("ir")),
        ..CompileOptions::default()
    });
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(report.verification_runs > 0);
    let ir = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(!ir.contains("noalias") && !ir.contains("@malloc("));
    let output = Command::new("clang")
        .args(["-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(temp.join("o2"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for path in [temp.join("o0"), temp.join("o2")] {
        let output = Command::new(path).output().unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, format!("{want}\n").as_bytes());
        assert!(output.stderr.is_empty());
    }
    fs::remove_dir_all(temp).unwrap();
}
fn one(text: &str, want: i64) {
    native(|options| compile_source(&source(text), options), want);
}
fn project(main: &str, model: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(10),
        modules: vec![
            SourceUnit::new(FileId(101), ModuleId(10), "main.zil", main),
            SourceUnit::new(FileId(205), ModuleId(30), "model.zil", model),
        ],
        paths: BTreeMap::from([(vec!["model".into()], ModuleId(30))]),
    }
}
fn reject(report: CompileReport, code: &str) {
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn mixed_windows_repeated_calls_and_restored_parents_execute() {
    one(
        "fn answer()->i32{var x=1;let y=2;var f=()=>{x=x+y;x};let a=f();let b=f();return a+b+x+y}",
        15,
    );
    one(
        "fn answer()->i32{var x=1;let y=2;let z=3;var f=()=>{x+=y+z;x};let a=f();let b=f();return a+b+x+y+z}",
        33,
    );
    one(
        "fn answer()->i32{var x=1;var y=2;let z=3;var f=()=>{x+=z;y+=x;x+y};let a=f();let b=f();return a+b+x+y+z}",
        53,
    );
    one(
        "fn answer()->i32{var x=1;var y=2;let z=3;let p=&mut x;let q=&mut y;var f=()=>{*p+=z;*q+=*p;*p+*q};let a=f();let b=f();*p+=1;*q+=1;return a+b+x+y+z}",
        55,
    );
}

#[test]
fn multiple_shared_descendants_and_unrelated_mutable_capture_execute() {
    one(
        "fn answer()->i32{var x=10;var y=0;let p=&mut x;let a=&*p;let b=&*p;var f=()=>{y+=*a+*b;y};let first=f();let second=f();return first+second+x+y}",
        110,
    );
    one(
        "fn answer()->i32{var x=21;var y=0;let p=&mut x;var f=()=>{y+=*p;y};let first=f();let second=f();*p=0;return first+second+y+x}",
        105,
    );
}

const CONFLICTS: &[&str] = &[
    // A relevant sibling cannot borrow the delegated status of an unrelated root.
    "fn answer()->i32{var x=1;let y=2;let p=&mut x;let sibling=&mut *p;var f=()=>{*p+=y;*p};let a=f();return a+*sibling}",
    // A live shared child prevents a parent's exclusive capture.
    "fn answer()->i32{var x=1;let y=2;let p=&mut x;let child=&*p;var f=()=>{*p+=y;*p};let a=f();return a+*child}",
    // Parent use between mixed-environment calls remains suspended.
    "fn answer()->i32{var x=1;var y=2;let z=3;let p=&mut x;let q=&mut y;var f=()=>{*p+=z;*q+=*p;*p+*q};let a=f();*q=40;return a+f()}",
    // The unrelated shared capture remains a real address dependency.
    "fn answer()->i32{var x=1;var y=2;var f=()=>{x+=y;x};let a=f();y=40;return a+f()}",
    // Direct reads do not obtain a vacuous parent delegation from an empty own set.
    "fn answer()->i32{var x=1;let y=2;var f=()=>{x+=y;x};let a=f();let n=x;return a+n+f()}",
    // Two relevant overlapping mutable captures cannot pass an any-parent test.
    "fn answer()->i32{var x=1;let z=2;let p=&mut x;let q=&mut *p;var f=()=>{*p+=z;*q+=z;*p+*q};return f()}",
    // Shared grandchild liveness must remain transitive through a mixed environment.
    "fn answer()->i32{var x=1;let y=2;let p=&mut x;let q=&*p;let r=&*q;var f=()=>{*p+=y;*p};let n=f();return n+*r}",
];

#[test]
fn mixed_root_source_conflicts_still_reject() {
    for (case, text) in CONFLICTS.iter().enumerate() {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.diagnostics.first().map(|d| d.phase),
            Some(if matches!(case, 2 | 3) {
                Phase::Analyze
            } else {
                Phase::Verify
            }),
            "{}",
            report.to_json()
        );
        reject(report, "E-BORROW-CONFLICT");
    }
}

// These seven fixed fixtures have one straight-line i32 entry and address-only
// captures. Give each address definition a distinct initialized scalar root to
// obtain a genuinely legal control. Project its unchanged initialization facts
// and exactly retargeted Hold facts back to the original conflicting module.
// This is fixture construction, not an alternative borrow/permission solver.
fn conflict_candidate(module: &TypedModule) -> CandidatePlan {
    assert!(module.resources.is_empty());
    assert_eq!(module.functions.len(), 2);
    assert_eq!(module.closures.len(), 1);
    let closure = &module.closures[0];
    assert_eq!(closure.kind, zil_ir::CallableKind::FnMut);
    assert_eq!(closure.result, Type::I32);
    assert!(closure.parameters.is_empty() && closure.lending_captures.is_empty());
    assert!(
        closure
            .captures
            .iter()
            .all(|ty| matches!(ty, Type::Address { inner, .. } if inner.as_ref() == &Type::I32))
    );
    let at = module
        .functions
        .iter()
        .position(|f| f.name == "answer")
        .unwrap();
    let original = &module.functions[at];
    assert!(original.parameters.is_empty());
    assert_eq!(original.return_type, Type::I32);
    assert_eq!(original.blocks.len(), 1);
    assert!(original.environment.is_none() && original.foreign.is_none());
    assert!(original.callable_contract.is_none() && original.callable_return_contract.is_none());
    assert!(
        original
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&original.values)
            .all(|ty| matches!(ty, Type::I32 | Type::Closure(_))
                || matches!(ty, Type::Address { inner, .. } if inner.as_ref() == &Type::I32))
    );
    let instructions = &original.blocks[0].instructions;
    assert!(
        instructions.iter().all(|i| matches!(
            i.kind,
            I::Binary { .. }
                | I::BorrowLocal { .. }
                | I::BorrowLocalMut { .. }
                | I::CallClosureMut { .. }
                | I::Constant(Constant::I32(_))
                | I::Load(_)
                | I::MakeClosure { .. }
                | I::ReadAddress { .. }
                | I::ReborrowAddress { .. }
                | I::ScopeEnter { .. }
                | I::ScopeExit { .. }
                | I::StorageDead(_)
                | I::StorageLive(_)
                | I::Store { .. }
                | I::WriteAddress { .. }
        )),
        "unexpected instruction in fixed scalar fixture"
    );
    for local in &original.locals {
        if matches!(&local.ty, Type::Address { inner, .. } if inner.as_ref() == &Type::I32) {
            assert_eq!(
                instructions
                    .iter()
                    .filter(|i| matches!(i.kind, I::Store { local: slot, .. } if slot == local.id))
                    .count(),
                1
            );
        }
    }
    assert_eq!(
        instructions
            .iter()
            .filter(|i| matches!(i.kind, I::ScopeEnter { .. }))
            .count(),
        2
    );
    assert_eq!(
        instructions
            .iter()
            .filter(|i| matches!(i.kind, I::ScopeExit { .. }))
            .count(),
        2
    );
    let (constant_at, initial) = instructions
        .iter()
        .enumerate()
        .find_map(|(at, i)| {
            matches!(i.kind, I::Constant(Constant::I32(_))).then(|| (at, i.result.unwrap()))
        })
        .unwrap();
    let mut reference = module.clone();
    let function = &mut reference.functions[at];
    let mut values = vec![None; original.values.len()];
    let mut locals = vec![None; original.locals.len()];
    let mut roots = BTreeMap::<LocalId, HoldRoot>::new();
    let mut rewritten = Vec::new();
    let mut ended = false;
    for (position, original_instruction) in instructions.iter().enumerate() {
        let mut instruction = original_instruction.clone();
        if let Some(value) = instruction.result
            && let Type::Address { mutable, inner } = &original.values[value.0 as usize]
            && inner.as_ref() == &Type::I32
        {
            assert!(!ended && position > constant_at);
            let origin = match instruction.kind {
                I::BorrowLocal { local } | I::BorrowLocalMut { local } => {
                    assert_eq!(original.locals[local.0 as usize].ty, Type::I32);
                    Some(HoldRoot::Local(local))
                }
                I::Load(local) => locals[local.0 as usize],
                I::ReborrowAddress { address, .. } => values[address.0 as usize],
                _ => panic!("unexpected address producer in fixed fixture: {instruction:?}"),
            }
            .expect("fixed scalar address must have one known physical root");
            assert!(
                matches!(origin, HoldRoot::Local(l) if original.locals[l.0 as usize].ty == Type::I32)
            );
            values[value.0 as usize] = Some(origin);
            let local = LocalId(function.locals.len() as u32);
            function.locals.push(LocalDecl {
                id: local,
                name: format!("independent-scalar-root-{}", value.0),
                ty: Type::I32,
                mutable: true,
                span: instruction.span,
            });
            roots.insert(local, origin);
            for kind in [
                I::StorageLive(local),
                I::Store {
                    local,
                    value: initial,
                },
            ] {
                rewritten.push(Instruction {
                    result: None,
                    kind,
                    span: instruction.span,
                });
            }
            instruction.kind = if *mutable {
                I::BorrowLocalMut { local }
            } else {
                I::BorrowLocal { local }
            };
        }
        if let I::Store { local, value } = original_instruction.kind {
            locals[local.0 as usize] = values[value.0 as usize];
        }
        if matches!(instruction.kind, I::ScopeExit { .. }) && !ended {
            for local in roots.keys().rev() {
                rewritten.push(Instruction {
                    result: None,
                    kind: I::StorageDead(*local),
                    span: instruction.span,
                });
            }
            ended = true;
        }
        rewritten.push(instruction);
    }
    assert!(ended && !roots.is_empty());
    function.blocks[0].instructions = rewritten;
    let limits = Limits::default();
    let mut plan = zil_analyze::analyze(&reference, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&reference, &plan, &limits).unwrap();
    zil_verify::verify_module(reference.clone(), plan.clone(), &reference.target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &reference.target, &limits).unwrap();
    assert!(
        plan.origins.is_empty() && plan.responsibilities.is_empty() && plan.unsupported.is_empty()
    );
    let projected = plan
        .functions
        .iter_mut()
        .find(|p| p.function == original.id)
        .unwrap();
    for block in &mut projected.blocks {
        block
            .initialized_in
            .retain(|l| (l.0 as usize) < original.locals.len());
        block
            .initialized_out
            .retain(|l| (l.0 as usize) < original.locals.len());
    }
    assert_eq!(projected.holds.len(), original.values.len());
    for hold in &mut projected.holds {
        let mut actual = Vec::new();
        for root in &hold.roots {
            match root {
                HoldRoot::Local(local) if roots.contains_key(local) => actual.push(roots[local]),
                _ => actual.push(*root),
            }
        }
        actual.sort_unstable();
        actual.dedup();
        hold.roots = actual;
        assert!(
            hold.roots.iter().all(
                |r| !matches!(r, HoldRoot::Local(l) if (l.0 as usize) >= original.locals.len())
            )
        );
    }
    plan.module_fingerprint = module.fingerprint();
    plan
}

#[test]
fn mixed_root_raw_and_wire_conflicts_reject_with_legal_control_candidates() {
    assert_eq!(CONFLICTS.len(), 7);
    for (case, text) in CONFLICTS.iter().enumerate() {
        let s = source(text);
        let ast = zil_syntax::parse(&s).into_result().unwrap();
        let core = zil_expand::expand(&s, &ast).unwrap();
        let limits = Limits::default();
        let module = zil_types::check(&core, &TargetProfile::default(), &limits).unwrap();
        let plan = conflict_candidate(&module);
        assert_eq!(plan.module_fingerprint, module.fingerprint());
        match (case, zil_analyze::analyze(&module, &limits)) {
            (2 | 3, Err(errors)) => {
                assert!(
                    errors.iter().all(|d| d.code == "E-BORROW-CONFLICT"
                        && d.category == ResultCategory::SafeReject),
                    "{text}\n{errors:?}"
                )
            }
            (2 | 3, Ok(_)) => panic!("caller-write conflict was not rejected by Analyze: {text}"),
            (_, Ok(actual)) => assert_eq!(actual, plan, "transported fixture facts differ: {text}"),
            (_, Err(errors)) => panic!("unexpected Analyze phase change: {text}\n{errors:?}"),
        }
        let original_bytes =
            zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        for result in [
            zil_verify::verify_module(module.clone(), plan, &module.target, &limits),
            zil_verify::verify_module_bytes(&original_bytes, &module.target, &limits),
        ] {
            let errors = result.unwrap_err();
            assert!(
                errors
                    .iter()
                    .all(|d| d.code == "E-BORROW-CONFLICT"
                        && d.category == ResultCategory::SafeReject),
                "{text}\n{errors:?}"
            );
        }
    }
}

const MODEL: &str = "pub enum Value<T>{Empty,Item(T)} pub struct Row{pub value:i64;secret:i64;} pub fn make()->Row{return Row{value:5000000042,secret:7}} pub fn hidden(p:&Row)->Value<i64>{return Value.Item(p.secret)}";

#[test]
fn projected_generic_payloads_compose_across_modules_at_native_width() {
    let p = project(
        "import model\nfn answer()->i64{let row=model.make();let p=&row;let item=model.Value.Item(p.value);if let model.Value.Item(n)=item{return n}return 0}",
        MODEL,
    );
    native(|options| compile_project(&p, options), 5000000042);
    let p = project(
        "import model\nfn answer()->i64{var row:(i64,[i64;2])=(3,[7,5000000042]);let p=&mut row;let item=model.Value.Item(p.1[1]);if let model.Value.Item(n)=item{return n}return 0}",
        MODEL,
    );
    native(|options| compile_project(&p, options), 5000000042);
    let p = project(
        "import model\nfn answer()->i64{let row=model.make();let p=&row;let item=model.hidden(p);if let model.Value.Item(n)=item{return n}return 0}",
        MODEL,
    );
    native(|options| compile_project(&p, options), 7);
}

#[test]
fn projected_generic_hints_grant_neither_private_access_nor_different_payload_type() {
    let p = project(
        "import model\nfn answer()->i64{let row=model.make();let p=&row;let item=model.Value.Item(p.secret);if let model.Value.Item(n)=item{return n}return 0}",
        MODEL,
    );
    reject(
        compile_project(&p, &CompileOptions::default()),
        "E-FIELD-PRIVATE",
    );
    let p = project(
        "import model\nfn answer()->i64{let row=model.make();let p=&row;let item:model.Value<bool>=model.Value.Item(p.value);return 0}",
        MODEL,
    );
    reject(
        compile_project(&p, &CompileOptions::default()),
        "E-GENERIC-ARGUMENT",
    );
    let p = project(
        "import model\nfn answer()->i64{let row=model.make();let p=&row;let pp=&p;let item=model.Value.Item(pp.value);return 0}",
        MODEL,
    );
    let report = compile_project(&p, &CompileOptions::default());
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "nested address hint was silently accepted: {}",
        report.to_json()
    );
    assert!(
        matches!(
            report.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}
