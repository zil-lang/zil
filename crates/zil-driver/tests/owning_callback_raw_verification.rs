//! Source-derived raw/Wire attacks. Neither kernel receives source admission as a
//! proof; each mutation starts from a real typed and independently valid body.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_ir::*;

fn module() -> TypedModule {
    let source = "resource R{n:i32}package Event{manages r:R;n:i32}fn main(){let f=(first:Event,second:Event)->Event=>{return first};let result=f(Event{r:R{n:1},n:2},Event{r:R{n:3},n:4})}";
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "owning_callback_raw_verification.zil",
        source,
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn rejected(result: Result<impl Sized, Vec<zil_base::Diagnostic>>, label: &str) {
    let diagnostics = match result {
        Ok(_) => panic!("{label}: forged ownership accepted"),
        Err(d) => d,
    };
    assert!(
        diagnostics
            .iter()
            .any(|d| d.category == ResultCategory::SafeReject),
        "{label}: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| !d.code.starts_with("E-VERIFY-PLAN")),
        "{label}: only claimed-plan mismatch: {diagnostics:#?}"
    );
}

fn rejected_code(result: Result<impl Sized, Vec<zil_base::Diagnostic>>, label: &str, code: &str) {
    assert!(
        result
            .as_ref()
            .err()
            .is_some_and(|diagnostics| diagnostics.iter().any(|d| d.code == code)),
        "{label}: expected {code}, got {:?}",
        result.as_ref().err()
    );
    rejected(result, label);
}

fn loan_rejected(result: Result<impl Sized, Vec<zil_base::Diagnostic>>, label: &str) {
    rejected_code(result, label, "E-BORROW-CONFLICT");
}

#[test]
fn both_kernels_reject_duplicate_owning_ssa_and_raw_call_signature_forgery() {
    let original = module();
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(original.clone(), plan.clone(), &original.target, &limits).unwrap();
    let control = wire::encode_module_candidate(&original, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&control, &original.target, &limits).unwrap();
    for attack in [
        "duplicate-arg",
        "missing-arg",
        "unused-signature",
        "forged-result",
    ] {
        let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("review248-{attack}-{}", std::process::id()));
        fs::create_dir_all(&out).unwrap();
        let mut bad = original.clone();
        if attack == "unused-signature" {
            bad.closures[0].parameters[0] = Type::I32;
        } else {
            let function = bad.functions.iter_mut().find(|f| f.name == "main").unwrap();
            let call = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| {
                    matches!(
                        i.kind,
                        InstructionKind::CallClosure { .. }
                            | InstructionKind::CallClosureShared { .. }
                            | InstructionKind::CallClosureMut { .. }
                            | InstructionKind::CallClosureOnce { .. }
                            | InstructionKind::CallClosureBorrowed { .. }
                    )
                })
                .unwrap();
            if attack == "forged-result" {
                call.kind = InstructionKind::Constant(Constant::I32(0));
            } else {
                let arguments = match &mut call.kind {
                    InstructionKind::CallClosure { arguments, .. }
                    | InstructionKind::CallClosureShared { arguments, .. }
                    | InstructionKind::CallClosureMut { arguments, .. }
                    | InstructionKind::CallClosureOnce { arguments, .. }
                    | InstructionKind::CallClosureBorrowed { arguments, .. } => arguments,
                    _ => unreachable!(),
                };
                assert_eq!(arguments.len(), 2);
                if attack == "duplicate-arg" {
                    arguments[1] = arguments[0]
                } else {
                    arguments.pop();
                }
            }
        }
        fs::write(out.join("typed.txt"), format!("{bad:#?}")).unwrap();
        let analyzed = zil_analyze::analyze(&bad, &limits);
        fs::write(out.join("analyze.txt"), format!("{analyzed:#?}")).unwrap();
        rejected(analyzed, attack);
        let mut candidate = plan.clone();
        candidate.module_fingerprint = bad.fingerprint();
        let encoded = wire::encode_module_candidate(&bad, &candidate, &limits).unwrap();
        fs::write(out.join("candidate.wire"), &encoded).unwrap();
        let raw = zil_verify::verify_module(bad.clone(), candidate, &bad.target, &limits);
        let bytes = zil_verify::verify_module_bytes(&encoded, &bad.target, &limits);
        fs::write(out.join("raw.txt"), format!("{raw:#?}")).unwrap();
        fs::write(out.join("wire.txt"), format!("{bytes:#?}")).unwrap();
        rejected(raw, &format!("{attack}/raw"));
        rejected(bytes, &format!("{attack}/wire"));
    }
}

#[test]
fn borrowed_owned_callback_cannot_retarget_an_input_loan_to_a_live_sibling() {
    let source = "resource R{n:i32}package Item{manages r:R;n:i32}package Reply{manages r:R;n:i32}fn main()->i32{var left=Item{r:R{n:1},n:2};var right=Item{r:R{n:3},n:4};let view=&left;let f=(item:&mut Item)->Reply=>{item.n+=1;return Reply{r:R{n:5},n:item.n}};let reply=f(&mut right);return view.n+reply.n-7}";
    let original = "let f=(item:&mut Item)->Reply=>{item.n+=1;return Reply{r:R{n:5},n:item.n}}";
    let cases = [
        ("shared", source.to_string()),
        ("cfg-multiple-input-sources", source.replace("let view=&left;", "var third=Item{r:R{n:6},n:7};let view=&left;")
            .replace("let reply=f(&mut right);", "let selected=if view.n==2 {&mut right} else {&mut third};let reply=f(selected);")),
        ("mutable-environment", source.replace(original, "var count=0;var f=move(item:&mut Item)->Reply=>{count+=1;item.n+=count;return Reply{r:R{n:5},n:item.n}}")),
        ("consuming-environment", source.replace(original, "let token=R{n:5};let f=move(item:&mut Item)->Reply=>{item.n+=1;return Reply{r:give token,n:item.n}}")),
        ("borrowed-helper", source.replace("fn main()", "fn dispatch<F:Fn(&mut Item)->Reply>(f:&F,item:&mut Item)->Reply{return f(item)}fn main()").replace("f(&mut right)", "dispatch(&f,&mut right)")),
    ];
    for (label, source) in cases {
        let unit = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "borrowed_callback_input.zil",
            &source,
        );
        let ast = zil_syntax::parse(&unit).into_result().unwrap();
        let core = zil_expand::expand(&unit, &ast).unwrap();
        let limits = Limits::default();
        let original = zil_types::check(&core, &TargetProfile::host_llvm(), &limits).unwrap();
        let plan = zil_analyze::analyze(&original, &limits).unwrap();
        zil_verify::verify_module(original.clone(), plan.clone(), &original.target, &limits)
            .unwrap();
        let control = wire::encode_module_candidate(&original, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&control, &original.target, &limits).unwrap();
        let mut forged = original.clone();
        let caller = forged
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let left = caller
            .locals
            .iter()
            .position(|local| local.name == "left")
            .unwrap();
        let right = caller
            .locals
            .iter()
            .position(|local| local.name == "right")
            .unwrap();
        let mut changed = 0;
        for i in caller
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
        {
            if let InstructionKind::BorrowLocalMut { local } = &mut i.kind
                && local.0 as usize == right
            {
                *local = LocalId(left as u32);
                changed += 1;
            }
        }
        assert_eq!(changed, 1);
        let caller_id = caller.id.0 as usize;
        let analyzed = zil_analyze::analyze(&forged, &limits);
        println!("{label}: Analyze={:?}", analyzed.as_ref().err());
        loan_rejected(analyzed, "retarget input loan");
        let mut claimed = plan;
        // Keep the candidate's coarse provenance truthful. A mismatch with
        // the original right-root claim would only test metadata checking,
        // not Verify's independent actual loan consumer.
        for row in &mut claimed.functions[caller_id].holds {
            for root in &mut row.roots {
                if *root == HoldRoot::Local(LocalId(right as u32)) {
                    *root = HoldRoot::Local(LocalId(left as u32));
                }
            }
            row.roots.sort_unstable();
            row.roots.dedup();
        }
        claimed.module_fingerprint = forged.fingerprint();
        let wire = wire::encode_module_candidate(&forged, &claimed, &limits).unwrap();
        let raw = zil_verify::verify_module(forged.clone(), claimed, &forged.target, &limits);
        println!("{label}: Verify={:?}", raw.as_ref().err());
        loan_rejected(raw, "retarget input loan/raw");
        loan_rejected(
            zil_verify::verify_module_bytes(&wire, &forged.target, &limits),
            "retarget input loan/wire",
        );
    }
}

#[test]
fn borrowed_owned_callback_cannot_duplicate_one_exclusive_input_at_a_call() {
    let source = "resource R{n:i32}package Item{manages r:R;n:i32}package Reply{manages r:R}fn main(){var first=Item{r:R{n:1},n:2};var second=Item{r:R{n:3},n:4};let f=(a:&mut Item,b:&mut Item)->Reply=>{a.n+=1;b.n+=1;return Reply{r:R{n:5}}};let reply=f(&mut first,&mut second)}";
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "exclusive_callback_inputs.zil",
        source,
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let limits = Limits::default();
    let original = zil_types::check(&core, &TargetProfile::host_llvm(), &limits).unwrap();
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(original.clone(), plan.clone(), &original.target, &limits).unwrap();
    let mut forged = original.clone();
    let caller = forged
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let mut changed = 0;
    for instruction in caller.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        let arguments = match &mut instruction.kind {
            InstructionKind::CallClosure { arguments, .. }
            | InstructionKind::CallClosureOnce { arguments, .. }
            | InstructionKind::CallClosureShared { arguments, .. }
            | InstructionKind::CallClosureMut { arguments, .. }
            | InstructionKind::CallClosureBorrowed { arguments, .. } => arguments,
            _ => continue,
        };
        assert_eq!(arguments.len(), 2);
        arguments[1] = arguments[0];
        changed += 1;
    }
    assert_eq!(changed, 1);
    loan_rejected(
        zil_analyze::analyze(&forged, &limits),
        "duplicate mutable callback input",
    );
    let mut claimed = plan;
    claimed.module_fingerprint = forged.fingerprint();
    let wire = wire::encode_module_candidate(&forged, &claimed, &limits).unwrap();
    rejected_code(
        zil_verify::verify_module(forged.clone(), claimed, &forged.target, &limits),
        "duplicate mutable callback input/raw",
        "E-VERIFY-MOVE",
    );
    rejected_code(
        zil_verify::verify_module_bytes(&wire, &forged.target, &limits),
        "duplicate mutable callback input/wire",
        "E-VERIFY-MOVE",
    );
}

#[test]
fn raw_owned_argument_does_not_grant_borrowed_inputs_with_a_copy_result() {
    let source = "resource R{n:i32}package Event{manages r:R;n:i32}fn main(){let f=(event:Event,p:&i32)->Event=>{let n=event.n;return event}}";
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "owned_borrowed_copy_result.zil",
        source,
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let limits = Limits::default();
    let original = zil_types::check(&core, &TargetProfile::host_llvm(), &limits).unwrap();
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(original.clone(), plan.clone(), &original.target, &limits).unwrap();
    let mut forged = original.clone();
    let definition = &mut forged.closures[0];
    definition.result = Type::I32;
    let entry_id = definition.function.0 as usize;
    let entry = &mut forged.functions[entry_id];
    entry.return_type = Type::I32;
    let copy = ValueId(entry.values.iter().position(|ty| ty == &Type::I32).unwrap() as u32);
    let mut changed = 0;
    for block in &mut entry.blocks {
        if let Terminator::Return(value) = &mut block.terminator {
            *value = Some(copy);
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    let analyzed = zil_analyze::analyze(&forged, &limits).unwrap_err();
    assert!(
        analyzed
            .iter()
            .any(|d| d.message
                == "borrowed inputs in an owning callback require a closed owned result"),
        "{analyzed:?}"
    );
    let mut claimed = plan;
    claimed.module_fingerprint = forged.fingerprint();
    let bytes = wire::encode_module_candidate(&forged, &claimed, &limits).unwrap();
    for diagnostics in [
        zil_verify::verify_module(forged.clone(), claimed, &forged.target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &forged.target, &limits).unwrap_err(),
    ] {
        assert!(
            diagnostics.iter().any(|d| d.message
                == "borrowed inputs in an owning callback require a closed owned result"),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn raw_pure_owned_helper_does_not_grant_a_borrowed_companion() {
    let source = "resource R{n:i32}package Event{manages r:R;n:i32}fn dispatch<F:FnOnce(Event,&i32)->Event>(f:F,event:Event,p:&i32)->Event{return f(event,p)}fn main(){let n=7;let f=(event:Event,p:&i32)->Event=>{return event};let reply=dispatch(f,Event{r:R{n:3},n:2},&n)}";
    let unit = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "owned_helper_borrowed_companion.zil",
        source,
    );
    let ast = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &ast).unwrap();
    let limits = Limits::default();
    let original = zil_types::check(&core, &TargetProfile::host_llvm(), &limits).unwrap();
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    zil_verify::verify_module(original.clone(), plan.clone(), &original.target, &limits).unwrap();
    let mut forged = original.clone();
    let definition = &mut forged.closures[0];
    assert!(matches!(
        definition.parameters.pop(),
        Some(Type::Address { .. })
    ));
    let entry_id = definition.function.0 as usize;
    let entry = &mut forged.functions[entry_id];
    let removed = entry.parameters.pop().unwrap();
    // An incoming parameter starts live implicitly. Removing that input also
    // removes its final storage end, leaving a genuinely unused local slot.
    for block in &mut entry.blocks {
        block.instructions.retain(|instruction| !matches!(instruction.kind, InstructionKind::StorageDead(local) if local == removed));
    }
    let helper = forged
        .functions
        .iter_mut()
        .find(|f| f.callable_contract.is_some())
        .unwrap();
    let mut changed = 0;
    for instruction in helper.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        let arguments = match &mut instruction.kind {
            InstructionKind::CallClosure { arguments, .. }
            | InstructionKind::CallClosureOnce { arguments, .. }
            | InstructionKind::CallClosureShared { arguments, .. }
            | InstructionKind::CallClosureMut { arguments, .. }
            | InstructionKind::CallClosureBorrowed { arguments, .. } => arguments,
            _ => continue,
        };
        arguments.pop();
        changed += 1;
    }
    assert_eq!(changed, 1);
    let analyzed = zil_analyze::analyze(&forged, &limits).unwrap_err();
    assert!(
        analyzed.iter().any(|d| d.message
            == "borrowed helper companions require a borrowed-input owned-result callback"),
        "{analyzed:?}"
    );
    let mut claimed = plan;
    for block in &mut claimed.functions[entry_id].blocks {
        block.initialized_in.retain(|local| *local != removed);
        block.initialized_out.retain(|local| *local != removed);
    }
    claimed.module_fingerprint = forged.fingerprint();
    let bytes = wire::encode_module_candidate(&forged, &claimed, &limits).unwrap();
    for diagnostics in [
        zil_verify::verify_module(forged.clone(), claimed, &forged.target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &forged.target, &limits).unwrap_err(),
    ] {
        assert!(
            diagnostics.iter().any(|d| d.message
                == "borrowed helper companions require a borrowed-input owned-result callback"),
            "{diagnostics:?}"
        );
    }
}
