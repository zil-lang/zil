//! Real Copy aggregate programs, LLVM bounds checks, and forged raw controls.
#[path = "../../../.task/G4.4.6.1-inputs/nominal_controls.rs"]
mod g4461_nominal_controls;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{InstructionKind, Type};
static NEXT: AtomicU64 = AtomicU64::new(0);

#[path = "../../../tests/language-readiness/terminal/340/helper_retirement_verify.rs"]
mod helper_retirement_verify;
#[path = "../../../tests/language-readiness/terminal/340/helper_retirement.rs"]
mod terminal340_helper_retirement;

#[path = "../../../tests/language-readiness/terminal/340/enum_replacement_verify.rs"]
mod enum_replacement_verify;
#[path = "../../../tests/language-readiness/terminal/340/enum_terminal.rs"]
mod terminal340_enum_terminal;

#[test]
fn terminal340_owner_replacement_types_uses_real_tickets() {
    for text in [
        include_str!("../../../tests/language-readiness/terminal/340/replace-fresh.zil"),
        include_str!("../../../tests/language-readiness/terminal/340/replace-captured-fresh.zil"),
        include_str!("../../../tests/language-readiness/terminal/340/replace-incoming.zil"),
        include_str!(
            "../../../tests/language-readiness/terminal/340/replace-captured-incoming.zil"
        ),
    ] {
        let temp = Temp::new();
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Types,
                emit_dir: Some(temp.0.clone()),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let typed = fs::read_to_string(&report.artifacts["typed"]).unwrap();
        assert!(typed.contains("ReservePlaceAddress"), "{typed}");
        assert!(typed.contains("CommitPlaceReplace"), "{typed}");
        assert!(!typed.contains("WriteAddress"), "{typed}");
    }
}

#[test]
fn terminal340_owner_replacement_analyze_tracks_fresh_and_incoming() {
    for (text, expected) in [
        (
            include_str!("../../../tests/language-readiness/terminal/340/replace-fresh.zil"),
            ResultCategory::Success,
        ),
        (
            include_str!(
                "../../../tests/language-readiness/terminal/340/replace-captured-fresh.zil"
            ),
            ResultCategory::Success,
        ),
        (
            include_str!("../../../tests/language-readiness/terminal/340/replace-incoming.zil"),
            ResultCategory::SafeReject,
        ),
        (
            include_str!(
                "../../../tests/language-readiness/terminal/340/replace-captured-incoming.zil"
            ),
            ResultCategory::SafeReject,
        ),
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Analyze,
                ..CompileOptions::default()
            },
        );
        assert_eq!(report.status, expected, "{}", report.to_json());
    }
}

#[test]
fn terminal340_owner_replacement_preparation_abort_and_pin_neighbors() {
    let failed_rhs =
        include_str!("../../../tests/language-readiness/terminal/340/replace-failure.zil");
    let temp = Temp::new();
    let report = compile_source(
        &source(failed_rhs),
        &CompileOptions {
            stop_after: Stage::Types,
            emit_dir: Some(temp.0.clone()),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let typed = fs::read_to_string(&report.artifacts["typed"]).unwrap();
    assert!(typed.contains("AbortPlaceReservation"), "{typed}");
    let report = compile_source(
        &source(failed_rhs),
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let prefix = "resource R{code:i32}package P{manages owner:R;n:i32}";
    for (helper, rhs, expected) in [
        (
            "fn read(n:&i32)->i32{return *n}",
            "read(&target.n)",
            ResultCategory::Success,
        ),
        (
            "fn write(n:&mut i32)->i32{*n=66;return *n}",
            "write(&mut target.n)",
            ResultCategory::Success,
        ),
        (
            "fn write(p:&mut P)->i32{p.n=66;return p.n}",
            "write(target)",
            ResultCategory::SafeReject,
        ),
    ] {
        let text = format!(
            "{prefix}{helper}fn replace(target:&mut P){{target.owner=R{{code:{rhs}}}}}fn result()->i32{{var packet=P{{owner:R{{code:65}},n:0}};replace(&mut packet);return 42}}"
        );
        let report = compile_source(
            &source(&text),
            &CompileOptions {
                stop_after: Stage::Analyze,
                ..CompileOptions::default()
            },
        );
        assert_eq!(report.status, expected, "{}", report.to_json());
    }
}

#[test]
fn terminal340_owner_replacement_wide_long_names_budget() {
    let prefix = "common_replacement_prefix_".repeat(8);
    let parameter = format!("{prefix}target");
    let mut fields = String::new();
    let mut locals = String::new();
    for i in 0..64 {
        fields.push_str(&format!("{prefix}field{i:04}:i32;"));
        locals.push_str(&format!("let {prefix}local{i:04}={i};"));
    }
    let text = format!(
        "resource R{{code:i32}}package P{{manages owner:R;{fields}}}fn replace({parameter}:&mut P){{{locals}{parameter}.owner=R{{code:66}}}}fn result()->i32{{return 42}}"
    );
    let input = source(&text);
    let options = |work| CompileOptions {
        stop_after: Stage::Types,
        limits: Limits {
            max_analysis_steps: work,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let (mut lo, mut hi) = (1, Limits::default().max_analysis_steps);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let report = compile_source(&input, &options(mid));
        match report.status {
            ResultCategory::Success => hi = mid,
            ResultCategory::ResourceLimit => lo = mid + 1,
            _ => panic!("{}", report.to_json()),
        }
    }
    let mut warm = zil_driver::CompilationSession::default();
    for (work, expected) in [
        (lo, ResultCategory::Success),
        (lo - 1, ResultCategory::ResourceLimit),
        (lo, ResultCategory::Success),
    ] {
        for report in [
            compile_source(&input, &options(work)),
            warm.compile_source(&input, &options(work)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!(
        "340 replacement wide/long names source Types N={lo}; N-1 rejects; fresh/warm retry agrees"
    );
}

#[test]
fn terminal340_owner_replacement_independent_verify_source() {
    for (text, expected) in [
        (
            include_str!("../../../tests/language-readiness/terminal/340/replace-fresh.zil"),
            ResultCategory::Success,
        ),
        (
            include_str!(
                "../../../tests/language-readiness/terminal/340/replace-captured-fresh.zil"
            ),
            ResultCategory::Success,
        ),
        (
            include_str!("../../../tests/language-readiness/terminal/340/replace-incoming.zil"),
            ResultCategory::SafeReject,
        ),
        (
            include_str!(
                "../../../tests/language-readiness/terminal/340/replace-captured-incoming.zil"
            ),
            ResultCategory::SafeReject,
        ),
        (
            include_str!("../../../tests/language-readiness/terminal/340/replace-failure.zil"),
            ResultCategory::Success,
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(report.status, expected, "{}", report.to_json());
    }
}

#[test]
fn terminal340_owner_replacement_preserves_freshness_and_real_cleanup() {
    for text in [
        include_str!("../../../tests/language-readiness/terminal/340/replace-fresh.zil"),
        include_str!("../../../tests/language-readiness/terminal/340/replace-captured-fresh.zil"),
    ] {
        review329_native_output(&[("main", text)], b"AB42\n");
    }
    review329_native_output(
        &[(
            "main",
            include_str!("../../../tests/language-readiness/terminal/340/replace-failure.zil"),
        )],
        b"A42\n",
    );
    review329_native_output(
        &[(
            "main",
            include_str!(
                "../../../tests/language-readiness/terminal/340/replace-prepared-failure.zil"
            ),
        )],
        b"BA42\n",
    );
    let read_old = include_str!("../../../tests/language-readiness/terminal/340/replace-fresh.zil")
        .replace("R{code:66}", "R{code:target.owner.code+1}");
    review329_native_output(&[("main", &read_old)], b"AB42\n");
    for text in [
        include_str!("../../../tests/language-readiness/terminal/340/replace-incoming.zil"),
        include_str!(
            "../../../tests/language-readiness/terminal/340/replace-captured-incoming.zil"
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn terminal340_owner_replacement_error_arm_content_and_raw_neighbors() {
    let text =
        include_str!("../../../tests/language-readiness/terminal/340/replace-error-arms.zil");
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native_output(&[("main", text)], b"ABCD42\n");
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let mut typed = zil_types::check(&core, &target, &limits).unwrap();
    // Make the two raw owning arms identical so a wrong arm cannot be
    // dismissed only because its pointee has a different nominal type.
    fn same_payload(ty: &mut Type) {
        match ty {
            Type::Package { name, fields } => {
                if name.ends_with(":Failure") {
                    *name = name.replace(":Failure", ":P");
                }
                for (_, field) in fields {
                    same_payload(field);
                }
            }
            Type::Address { inner, .. } | Type::Managed(inner) | Type::Desc(inner) => {
                same_payload(inner)
            }
            Type::ErrorUnion { error, result } => {
                same_payload(error);
                same_payload(result);
            }
            _ => {}
        }
    }
    for f in &mut typed.functions {
        same_payload(&mut f.return_type);
        for local in &mut f.locals {
            same_payload(&mut local.ty);
        }
        for ty in &mut f.values {
            same_payload(ty);
        }
    }
    for f in typed
        .functions
        .iter()
        .filter(|f| matches!(f.name.as_str(), "make_ok" | "make_err"))
    {
        let Type::ErrorUnion { error, result } = &f.return_type else {
            panic!("owning ErrorUnion")
        };
        assert_eq!(error, result, "raw counterexample requires equal arm Types");
    }
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    for name in ["work_ok", "work_err"] {
        let mut raw = typed.clone();
        let f = raw.functions.iter_mut().find(|f| f.name == name).unwrap();
        let path = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find_map(|i| match &mut i.kind {
                InstructionKind::BorrowPlaceLocal { path, .. }
                    if matches!(path.first(), Some(zil_ir::PlaceProjection::ErrorArm(_))) =>
                {
                    Some(path)
                }
                _ => None,
            })
            .unwrap();
        let zil_ir::PlaceProjection::ErrorArm(arm) = &mut path[0] else {
            unreachable!()
        };
        *arm = !*arm;
        assert!(zil_analyze::analyze(&raw, &limits).is_err(), "{name}");
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        let errors =
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.category == ResultCategory::SafeReject),
            "{name}: {errors:?}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "{name}"
        );
    }
}

#[test]
fn terminal340_owner_replacement_raw_wire_and_real_parent_pin() {
    let input = source(
        "resource R{code:i32}package P{manages owner:R;n:i32}fn read(n:&i32)->i32{return *n}fn touch(p:&mut P)->i32{p.n=66;return p.n}fn replace(target:&mut P){target.owner=R{code:read(&target.n)}}fn result()->i32{var p=P{owner:R{code:65},n:66};replace(&mut p);return 42}",
    );
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    let touch = typed
        .functions
        .iter()
        .find(|f| f.name == "touch")
        .unwrap()
        .id;
    for mode in 0..5 {
        let mut raw = typed.clone();
        let f = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "replace")
            .unwrap();
        let parent = f
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| match i.kind {
                InstructionKind::ReservePlaceAddress { address, .. } => Some(address),
                _ => None,
            })
            .unwrap();
        if mode == 4 {
            let argument = f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| match &i.kind {
                    InstructionKind::Call { arguments, .. } => Some(arguments[0]),
                    _ => None,
                })
                .unwrap();
            f.values[argument.0 as usize] = f.values[parent.0 as usize].clone();
            for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                if i.result == Some(argument) {
                    i.kind = InstructionKind::ReborrowAddress {
                        address: parent,
                        mutable: true,
                    };
                } else if matches!(i.kind, InstructionKind::Call { .. }) {
                    i.kind = InstructionKind::Call {
                        function: touch,
                        arguments: vec![argument],
                    };
                }
            }
        } else if mode == 3 {
            let Type::Address { mutable, .. } = &mut f.locals[f.parameters[0].0 as usize].ty else {
                panic!("actual parent")
            };
            *mutable = false;
        } else {
            let b = f
                .blocks
                .iter_mut()
                .find(|b| {
                    b.instructions
                        .iter()
                        .any(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }))
                })
                .unwrap();
            let commit = b
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }))
                .unwrap();
            if mode == 0 {
                b.instructions
                    .retain(|i| !matches!(i.kind, InstructionKind::ReservePlaceAddress { .. }));
            } else if mode == 1 {
                let duplicate = b.instructions[commit].clone();
                b.instructions.insert(commit + 1, duplicate);
            } else {
                let InstructionKind::CommitPlaceReplace { reservation, .. } =
                    b.instructions[commit].kind
                else {
                    unreachable!()
                };
                b.instructions[commit].kind =
                    InstructionKind::AbortPlaceReservation { reservation };
                // An aborted preparation does not consume the staged new debt.
            }
        }
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        let errors =
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.category == ResultCategory::SafeReject),
            "mode {mode}: {errors:?}"
        );
        if mode == 4 {
            assert!(
                errors.iter().any(|e| e.code == "E-BORROW-REPLACEMENT-PIN"),
                "{errors:?}"
            );
        }
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "mode {mode}"
        );
    }
    // Preserve all producer claims while changing the actual staged debt from
    // Created to Incoming. Verify must derive the transfer through both calls.
    let text = include_str!("../../../tests/language-readiness/terminal/340/replace-fresh.zil")
        .replace("fn make()", "fn make(give incoming:R)")
        .replace(
            "relay(&mut packet)",
            "replace_value(&mut packet,R{code:66})",
        )
        .replace("let owner=make()", "let owner=make(R{code:67})");
    let text =
        format!("{text}fn replace_value(target:&mut P,give next:R){{target.owner=give next}}");
    let input = source(&text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let mut raw = typed;
    let f = raw.functions.iter_mut().find(|f| f.name == "make").unwrap();
    let incoming = f.parameters[0];
    let new = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .filter(|i| matches!(i.kind, InstructionKind::MakeManaged { .. }))
        .last()
        .unwrap();
    new.kind = InstructionKind::TakeLocal { local: incoming };
    let mut claim = plan;
    claim.module_fingerprint = raw.fingerprint();
    let errors =
        zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
    assert!(
        errors.iter().any(|e| e.message.contains("fresh")),
        "{errors:?}"
    );
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
    let errors = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err();
    assert!(
        errors.iter().any(|e| e.message.contains("fresh")),
        "{errors:?}"
    );
}

#[test]
fn terminal340_captured_owning_base_native() {
    let original = include_str!(
        "../../../tests/language-readiness/terminal/340/captured-owning-base-open.zil"
    );
    let report = compile_source(&source(original), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.diagnostics[0].code,
        "E-CLOSURE-MUTABILITY",
        "{}",
        report.to_json()
    );
    let legal =
        include_str!("../../../tests/language-readiness/terminal/340/captured-owning-base.zil");
    assert_eq!(legal, original.replace("let f=", "var f="));
    review329_native_output(&[("main", legal)], b"A42\n");
}

#[test]
fn terminal340_captured_owning_base_compositions() {
    let prefix = r#"extern "C" fn putchar(code:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}package P{manages r:R;n:i32}enum S{Data((P,i32))}enum F{Bad}"#;
    for (index,body) in [
        "fn result()->i32{var value=S.Data((P{r:R{code:65},n:20},22));let input=&value;let f=()->i32=>{match *input{S.Data((ref p,n))=>{return p.n+n}}};return f()}",
        "fn result()->i32{var value=(P{r:R{code:65},n:20},22);let input=&mut value;var f=()->i32=>{let(ref p,ref mut n)=*input;*n+=p.n;return *n};return f()}",
        "fn make()->F!P{return P{r:R{code:65},n:20}}fn result()->i32{var value=make();let input=&mut value;var f=()->i32=>{match *input{.ok(ref mut p)=>{p.n+=22;return p.n},.err(_)=>{return 0}}};return f()}",
        "fn result()->i32{var value=S.Data((P{r:R{code:65},n:20},22));let input=&mut value;var f=()->i32=>{match *input{S.Data((ref p,ref mut n))=>{*n+=p.n;return *n}}};let n=f();let moved=value;return n}",
        "fn result()->i32{var value=S.Data((P{r:R{code:65},n:20},22));let input=&value;let f=()->&i32=>{match *input{S.Data((ref p,_))=>{return &p.n}}};let n=f();return *n+22}",
        "fn select(input:&S)->&i32{let f=()->&i32=>{match *input{S.Data((ref p,_))=>{return &p.n}}};return f()}fn result()->i32{var value=S.Data((P{r:R{code:65},n:20},22));let n=select(&value);return *n+22}",
        "fn inspect(input:&mut S)->i32{var f=()->i32=>{match *input{S.Data((ref p,ref mut n))=>{*n+=p.n;return *n}}};return f()}fn result()->i32{var value=S.Data((P{r:R{code:65},n:20},22));return inspect(&mut value)}",
    ].iter().enumerate(){
        eprintln!("340 owning-base capture composition {index}");
        review329_native_output(&[("main",&format!("{prefix}{body}"))],b"A42\n");
    }
}

#[test]
fn terminal340_captured_owning_base_authority_neighbors() {
    let prefix = "resource R{n:i32}package P{manages r:R;n:i32}enum S{Data((P,i32))}";
    for body in [
        "fn result()->i32{var value=S.Data((P{r:R{n:1},n:20},22));let input=&mut value;var f=()->i32=>{match *input{S.Data((ref p,ref mut n))=>{*n+=p.n;return *n}}};let moved=value;return f()}",
        "fn result()->i32{var value=S.Data((P{r:R{n:1},n:20},22));let input=&value;var f=()->i32=>{match *input{S.Data((ref p,ref mut n))=>{*n+=p.n;return *n}}};return f()}",
        "fn result()->i32{var value=S.Data((P{r:R{n:1},n:20},22));let input=&mut value;var f=()->i32=>{match *input{S.Data((ref p,ref mut n))=>{*n+=p.n;return *n}}};var g=()->i32=>{match *input{S.Data((ref p,ref mut n))=>{*n+=p.n;return *n}}};return f()+g()}",
    ] {
        let text = format!("{prefix}{body}");
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn terminal340_captured_owning_result_lifetime_neighbors() {
    let prefix = "resource R{n:i32}package P{manages r:R;n:i32}";
    let body = "fn result()->i32{var value=(P{r:R{n:1},n:20},22);let input=&mut value;let f=()->&i32=>{let(_,ref n)=*input;return n};let n=f();BODY}";
    for (tail, expected) in [
        (
            "let observed=*n;let moved=value;return observed+20",
            ResultCategory::Success,
        ),
        ("let moved=value;return *n+20", ResultCategory::SafeReject),
        (
            "let(_,ref mut k)=*input;*k=1;return *n+20",
            ResultCategory::SafeReject,
        ),
        (
            "let observed=*n;let(_,ref mut k)=*input;*k=1;return observed+20",
            ResultCategory::Success,
        ),
        (
            "let(ref mut p,_)=*input;p.n=20;return *n+p.n",
            ResultCategory::Success,
        ),
        (
            "let another=f();let(_,ref mut k)=*input;*k=1;return *n+*another-2",
            ResultCategory::SafeReject,
        ),
    ] {
        let text = format!("{prefix}{}", body.replace("BODY", tail));
        eprintln!("340 derived capture tail {tail}");
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(report.status, expected, "{}", report.to_json());
        if expected == ResultCategory::Success {
            review329_native(&[("main", &text)], 42);
        }
    }
}

#[test]
fn terminal340_captured_owning_result_later_mutating_call_rejects() {
    let prefix = "resource R{n:i32}package P{manages r:R;n:i32}";
    for (tail, expected) in [
        (
            "let n=f();let observed=*n;let another=f();return observed+*another-1",
            ResultCategory::Success,
        ),
        (
            "let n=f();let another=f();return *n+*another-1",
            ResultCategory::SafeReject,
        ),
    ] {
        let text = format!(
            "{prefix}fn result()->i32{{var value=(P{{r:R{{n:1}},n:20}},20);let input=&mut value;var f=()->&i32=>{{let(_,ref mut n)=*input;*n+=1;return &*n}};{tail}}}"
        );
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert_eq!(report.status, expected, "{}", report.to_json());
        if expected == ResultCategory::Success {
            review329_native(&[("main", &text)], 42);
        }
    }
}

#[test]
fn terminal340_captured_owning_result_through_helper() {
    let text = "resource R{n:i32}package P{manages r:R;n:i32}fn relay(p:&i32)->&i32{return p}fn result()->i32{var value=(P{r:R{n:1},n:20},22);let input=&mut value;let f=()->&i32=>{let(_,ref n)=*input;return n};let n=relay(f());let(ref mut p,_)=*input;p.n=20;return *n+p.n}";
    review329_native(&[("main", text)], 42);
    let stale = text.replace(
        "let(ref mut p,_)=*input;p.n=20;return *n+p.n",
        "let(_,ref mut k)=*input;*k=20;return *n+*k",
    );
    let report = compile_source(&source(&stale), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn terminal340_captured_owning_callable_parameter_boundary() {
    let text = "resource R{n:i32}package P{manages r:R;n:i32}fn peek<F:Fn()->&i32>(f:&F)->&i32{return f()}fn result()->i32{var value=(P{r:R{n:1},n:20},22);let input=&value;let f=()->&i32=>{let(_,ref n)=*input;return n};let n=peek(&f);return *n+20}";
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    eprintln!(
        "340 captured callable-parameter boundary {}",
        report.to_json()
    );
}

fn terminal340_captured_slots_source() -> &'static str {
    include_str!("../../../tests/language-readiness/terminal/340/captured-owning-result.zil")
}

#[test]
fn terminal340_captured_owning_result_slots_raw_and_wire() {
    let text = terminal340_captured_slots_source();
    review329_native(&[("main", text)], 42);
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    for forged_mode in 0..3 {
        let mut raw = typed.clone();
        let d = raw
            .closures
            .iter_mut()
            .find(|d| !d.lending_captures.is_empty())
            .unwrap();
        assert_eq!(d.captures.len(), 2);
        assert_eq!(d.lending_captures.len(), 1);
        let id = d.id;
        let selected = d.lending_captures[0] as usize;
        if forged_mode == 2 {
            d.lending_captures[0] = u32::MAX;
        } else if forged_mode == 1 {
            let Type::Address { mutable, .. } = &mut d.captures[selected] else {
                panic!()
            };
            *mutable = true;
        } else {
            let captures = raw
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.blocks)
                .flat_map(|b| &mut b.instructions)
                .find_map(|i| match &mut i.kind {
                    InstructionKind::MakeClosure { closure, captures } if *closure == id => {
                        Some(captures)
                    }
                    _ => None,
                })
                .unwrap();
            captures[selected] = captures[1 - selected];
        }
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "forged capture {forged_mode} Analyze"
        );
        assert!(
            zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).is_err(),
            "forged capture {forged_mode} Verify"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "forged capture {forged_mode} Wire"
        );
    }
}

#[test]
fn terminal340_captured_owning_result_raw_parent_move() {
    let text = "resource R{n:i32}package P{manages r:R;n:i32}fn result()->i32{var value=(P{r:R{n:1},n:20},22);let input=&mut value;let f=()->&i32=>{let(_,ref n)=*input;return n};let n=f();let observed=*n;let moved=value;return observed+20}";
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let mut raw = typed.clone();
    let f = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "result")
        .unwrap();
    let local = zil_ir::LocalId(f.locals.iter().position(|l| l.name == "value").unwrap() as u32);
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind,InstructionKind::TakeLocal{local:l} if l==local))
        })
        .unwrap();
    let take = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind,InstructionKind::TakeLocal{local:l} if l==local))
        .unwrap();
    let read = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ReadAddress { .. }))
        .unwrap();
    assert!(read < take);
    let take = block.instructions.remove(take);
    block.instructions.insert(read, take);
    assert!(zil_analyze::analyze(&raw, &limits).is_err());
    assert!(zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).is_err());
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
}

#[test]
fn terminal340_captured_owning_budget_boundaries() {
    use zil_driver::CompilationSession;
    let input = source(terminal340_captured_slots_source());
    let options = |work| CompileOptions {
        limits: Limits {
            max_analysis_steps: work,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let mut low = 1;
    let mut high = Limits::default().max_analysis_steps;
    while low < high {
        let middle = low + (high - low) / 2;
        match compile_source(&input, &options(middle)).status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            status => panic!("unexpected capture boundary {status:?}"),
        }
    }
    let mut warm = CompilationSession::default();
    for (work, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(work)),
            CompilationSession::default().compile_source(&input, &options(work)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!(
        "340 owning-base capture full-request analysis N={low}, N-1 rejects, fresh/warm agree"
    );
    let limits = Limits::default();
    let target = TargetProfile::default();
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    for gate in 0..3 {
        let run = |work| {
            let limits = Limits {
                max_analysis_steps: work,
                ..Limits::default()
            };
            let result = match gate {
                0 => zil_analyze::analyze(&typed, &limits).map(|_| ()),
                1 => zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits)
                    .map(|_| ()),
                _ => zil_verify::verify_module_bytes(&bytes, &target, &limits).map(|_| ()),
            };
            match result {
                Ok(()) => true,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|e| e.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    false
                }
            }
        };
        let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
        while low < high {
            let middle = low + (high - low) / 2;
            if run(middle) {
                high = middle
            } else {
                low = middle + 1
            }
        }
        assert!(run(low));
        assert!(!run(low - 1));
        assert!(run(low));
        println!("340 owning-base capture raw gate{gate} N={low}, N-1 rejects");
    }
}

#[test]
fn terminal340_error_arm_patterns_native() {
    let text = r#"enum F{Bad}fn number(x:F!i32)->i32{match x{.ok(n)=>{return n},.err(F.Bad())=>{return 0}}}fn make(b:bool)->F!i32{if b{return 42}else{return F.Bad()}}fn result()->i32{return number(make(true))+number(make(false))}"#;
    review329_native(&[("main", text)], 42);
    let generic = r#"enum F{Bad}fn get<T:Copy>(x:F!T,fallback:T)->T{match x{.ok(n)=>{return n},.err(_)=>{return fallback}}}fn make()->F!i32{return 42}fn result()->i32{return get(make(),0)}"#;
    review329_native(&[("main", generic)], 42);
    for body in [
        "if let .ok(n)=x{return n}return fallback",
        "while let .ok(n)=x{return n}return fallback",
    ] {
        let generic = format!(
            "enum F{{Bad}}fn get<T:Copy>(x:F!T,fallback:T)->T{{{body}}}fn make(b:bool)->F!i32{{if b{{return 42}}return F.Bad()}}fn result()->i32{{return get(make(true),0)+get(make(false),0)}}"
        );
        review329_native(&[("main", &generic)], 42);
    }
    let unit = r#"enum F{Bad}fn make()->F!(){return ()}fn result()->i32{match make(){.ok(())=>{return 42},.err(_)=>{return 0}}}"#;
    review329_native(&[("main", unit)], 42);
    let named = r#"enum F{Bad}type Number=i32;fn make()->F!Number{return 42}fn result()->i32{match make(){Number(n)=>{return n},F(F.Bad())=>{return 0}}}"#;
    review329_native(&[("main", named)], 42);
    let tuple = r#"enum F{Bad}type Pair=(i32,i32);fn make()->F!Pair{return (20,22)}fn result()->i32{match make(){Pair((a,b))=>{return a+b},F(_)=>{return 0}}}"#;
    review329_native(&[("main", tuple)], 42);
    review329_native(
        &[
            (
                "main",
                "import lib as m;fn make(b:bool)->m.Left!m.Right{if b{return m.Left{value:20}}return m.Right{value:22}}fn score(x:m.Left!m.Right)->i32{match x{m.L(n)=>{return n.value},m.R(n)=>{return n.value}}}fn result()->i32{return score(make(true))+score(make(false))}",
            ),
            (
                "lib",
                "pub struct Left{pub value:i32}pub struct Right{pub value:i32}pub type L=Left;pub type R=Right;",
            ),
        ],
        42,
    );
    let same = "fn inspect<T:Copy>(x:T!T)->i32{match x{.ok(_)=>{return 42},.err(_)=>{return 0}}}fn concrete(x:i32!i32)->i32{match x{.ok(_)=>{return 42},.err(_)=>{return 0}}}fn result()->i32{return 42}";
    review329_native(&[("main", same)], 42);
}

#[test]
fn terminal340_error_arm_owned_and_borrowed_native() {
    review329_native_output(
        &[(
            "main",
            include_str!("../../../tests/language-readiness/terminal/340/error-arms.zil"),
        )],
        b"AXBX42\n",
    );
    let prefix = r#"extern "C" fn putchar(code:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}package P{manages r:R;n:i32}package Failure{manages r:R;n:i32}"#;
    for (body, expected) in [
        (
            "enum E{Left(P),Right(P)}fn make(b:bool)->E{if b{return E.Left(P{r:R{code:65},n:20})}return E.Right(P{r:R{code:66},n:22})}fn work(b:bool)->i32{match make(b){E.Left(p)=>{let n=p.n;return n},E.Right(p)=>{let n=p.n;return n}}}fn result()->i32{return work(true)+work(false)}",
            "AB42\n",
        ),
        (
            "fn make(b:bool)->Failure!P{if b{return P{r:R{code:65},n:20}}return Failure{r:R{code:66},n:22}}fn work(b:bool)->i32{match make(b){.ok(p)=>{let n=p.n;return n},.err(p)=>{return p.n}}}fn result()->i32{return work(true)+work(false)}",
            "AB42\n",
        ),
        (
            "enum F{Bad}fn make()->F!P{return P{r:R{code:65},n:42}}fn result()->i32{var value=make();match value{.ok(ref p)=>{let n=p.n;return n},.err(_)=>{return 0}}}",
            "A42\n",
        ),
        (
            "enum F{Bad}fn make()->F!P{return P{r:R{code:65},n:20}}fn work(value:&mut(F!P))->i32{match *value{.ok(ref mut p)=>{p.n+=22;return p.n},.err(_)=>{return 0}}}fn result()->i32{var value=make();return work(&mut value)}",
            "A42\n",
        ),
        (
            "enum F{Bad}fn make()->F!P{return P{r:R{code:65},n:42}}fn result()->i32{var value=make();if let .ok(ref p)=value{return p.n}return 0}",
            "A42\n",
        ),
        (
            "enum F{Bad}fn make()->F!i32{return 20}fn work(value:&mut(F!i32))->i32{match *value{.ok(ref mut p)=>{*p+=22;return *p},.err(_)=>{return 0}}}fn result()->i32{var value=make();return work(&mut value)}",
            "42\n",
        ),
    ] {
        eprintln!("340 ErrorUnion owning case {body}");
        let text = format!("{prefix}{body}");
        review329_native_output(&[("main", &text)], expected.as_bytes());
    }
}

#[test]
fn terminal340_error_arm_equal_type_raw_wire_and_native() {
    use zil_ir::{Constant, PlaceProjection};
    let text = "fn make()->i32!bool{return true}fn inspect(value:&(i32!bool))->i32{match *value{.ok(ref n)=>{return 42},.err(_)=>{return 0}}}fn result()->i32{let value=make();return inspect(&value)}";
    let input = source(text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let mut raw = zil_types::check(&core, &target, &limits).unwrap();
    fn substitute(ty: &mut Type) {
        match ty {
            Type::ErrorUnion { result, .. } => **result = Type::I32,
            Type::Address { inner, .. } => {
                if **inner == Type::Bool {
                    **inner = Type::I32
                } else {
                    substitute(inner)
                }
            }
            _ => {}
        }
    }
    for f in &mut raw.functions {
        substitute(&mut f.return_type);
        for local in &mut f.locals {
            substitute(&mut local.ty)
        }
        for ty in &mut f.values {
            substitute(ty)
        }
        for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if matches!(i.kind, InstructionKind::Constant(Constant::Bool(true))) {
                i.kind = InstructionKind::Constant(Constant::I32(7));
                f.values[i.result.unwrap().0 as usize] = Type::I32;
            }
        }
    }
    let entry = raw
        .functions
        .iter()
        .find(|f| f.name == "result")
        .unwrap()
        .id;
    for is_error in [false, true] {
        let mut candidate = raw.clone();
        for i in candidate
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
        {
            if let InstructionKind::MakeErrorUnion { is_error: e, .. } = &mut i.kind {
                *e = is_error
            }
        }
        let plan = zil_analyze::analyze(&candidate, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&candidate, &plan, &limits).unwrap();
        let verified = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
        assert_eq!(verified.module(), &candidate);
        let lir = zil_lower::lower(&verified).unwrap();
        for backend in [
            zil_backend_llvm::NativeBackend::Clang,
            zil_backend_llvm::NativeBackend::LlvmApi,
        ] {
            if backend == zil_backend_llvm::NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [
                zil_backend_llvm::Optimization::O0,
                zil_backend_llvm::Optimization::O2,
            ] {
                let out = Temp::new();
                let mut options = zil_backend_llvm::BuildOptions::new(out.0.join("program"));
                options.backend = backend;
                options.optimization = optimization;
                options.emit.entry = zil_backend_llvm::EntryPoint::ReturnValue(entry);
                let built = zil_backend_llvm::build(&verified, &lir, &options).unwrap();
                let ran = Command::new(built.executable).output().unwrap();
                assert!(ran.status.success(), "{ran:?}");
                assert_eq!(
                    ran.stdout,
                    if is_error {
                        b"0\n".as_slice()
                    } else {
                        b"42\n".as_slice()
                    }
                );
            }
        }
    }
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let mut wrong = raw.clone();
    let projection = wrong
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
        .find_map(|i| match &mut i.kind {
            InstructionKind::BorrowPlaceAddress { path, .. }
                if path == &[PlaceProjection::ErrorArm(false)] =>
            {
                Some(path)
            }
            _ => None,
        })
        .unwrap();
    projection[0] = PlaceProjection::ErrorArm(true);
    assert!(zil_analyze::analyze(&wrong, &limits).is_err());
    let mut claim = plan;
    claim.module_fingerprint = wrong.fingerprint();
    assert!(zil_verify::verify_module(wrong.clone(), claim.clone(), &target, &limits).is_err());
    let wire = zil_ir::wire::encode_module_candidate(&wrong, &claim, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
}

#[test]
fn terminal340_error_arm_budget_fresh_and_warm() {
    use zil_driver::CompilationSession;
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/340/error-arms.zil"
    ));
    let options = |work| CompileOptions {
        limits: Limits {
            max_analysis_steps: work,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
    while low < high {
        let mid = low + (high - low) / 2;
        let r = compile_source(&input, &options(mid));
        match r.status {
            ResultCategory::Success => high = mid,
            ResultCategory::ResourceLimit => low = mid + 1,
            _ => panic!("{}", r.to_json()),
        }
    }
    let mut warm = CompilationSession::default();
    for (n, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(n)),
            CompilationSession::default().compile_source(&input, &options(n)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!("340 direct ErrorUnion patterns N={low}, N-1 rejects, fresh/warm agree");
}

#[test]
fn terminal340_unused_generic_error_ref_origins() {
    let bad = "enum F{Bad}fn unused<T:Copy>(x:F!T)->i32{var value=x;match value{.ok(ref n)=>{value=x;let observed=*n;return 0},.err(_)=>{return 0}}}fn result()->i32{return 42}";
    let good = bad.replace("value=x;let observed=*n;", "let observed=*n;value=x;");
    let rejected = compile_source(&source(bad), &CompileOptions::default());
    let admitted = compile_source(&source(&good), &CompileOptions::default());
    eprintln!(
        "340 unused pattern alias bad={:?}, nearest={:?}",
        rejected.status, admitted.status
    );
    assert_eq!(
        admitted.status,
        ResultCategory::Success,
        "{}",
        admitted.to_json()
    );
    assert_eq!(
        rejected.status,
        ResultCategory::SafeReject,
        "{}",
        rejected.to_json()
    );
    let invoked = good.replace(
        "fn result()->i32{return 42}",
        "fn make()->F!i32{return 42}fn result()->i32{return unused(make())+42}",
    );
    review329_native(&[("main", &invoked)], 42);
}

#[test]
fn terminal340_error_arm_patterns_definition_neighbors() {
    for text in [
        "enum F{Bad}fn make()->F!i32{return 42}fn result()->i32{let .ok(n)=make();return n}",
        "fn bad<T:Copy>(x:i32!T)->T{let .ok(n)=x;return n}fn result()->i32{return 0}",
        "enum F{Bad}fn bad(value:F!i32)->i32{match value{.ok(ref self)=>{return 0},.err(_)=>{return 0}}}fn result()->i32{return 0}",
        "enum F{Bad}fn bad(value:F!i32)->i32{match value{.ok(ref u64)=>{return 0},.err(_)=>{return 0}}}fn result()->i32{return 0}",
        "enum F{Bad}fn bad(value:F!i32)->i32{match value{.ok(ref f32)=>{return 0},.err(_)=>{return 0}}}fn result()->i32{return 0}",
        "fn bad<T:Copy>(x:i32!T,fallback:T)->T{if let .err(n)=x{return n}return fallback}fn result()->i32{return 0}",
        "fn bad<T:Copy>(x:i32!T,fallback:T)->T{while let .err(n)=x{return n}return fallback}fn result()->i32{return 0}",
        "enum F{Bad}fn make()->F!i32{return 42}fn result()->i32{var value=make();match value{.ok(ref n)=>{value=F.Bad();return *n},.err(_)=>{return 0}}}",
        "enum Failure{Bad}fn bad<T:Copy>(x:Failure!T)->i32{match x{Failure(_)=>{return 0},T(_)=>{return 1}}}fn result()->i32{return 0}",
        "fn bad<T:Copy>(x:i32!T)->T{match x{.ok(n)=>{return n}}}fn result()->i32{return 0}",
        "fn bad<T:Copy>(x:i32!T)->T{match x{.ok(n)=>{return n},.err(n)=>{return n}}}fn result()->i32{return 0}",
        "fn bad(x:i32)->i32{match x{.ok(n)=>{return n},.err(_)=>{return 0}}}fn result()->i32{return 0}",
        "fn bad(x:i32!bool)->i32{match x{.ok(_)=>{return 0},.ok(_)=>{return 1},.err(n)=>{return n}}}fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}\n{}",
            report.to_json()
        );
    }
}

#[test]
fn terminal340_fnptr_record_enum_result_inference() {
    let text = "enum E{Data{n:i32},Empty}fn keep(x:E)->E{return x}fn result()->i32{let pass:fn(E)->E=keep;let value=pass(E.Data{n:42});match value{E.Data{n:x}=>{return x},E.Empty()=>{return 0}}}";
    review329_native(&[("main", text)], 42);
    let wrong = text.replace("E.Data{n:x}", "Other.Data{n:x}");
    let report = compile_source(&source(&wrong), &CompileOptions::default());
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );

    let input = source(text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&wire, &target, &limits)
            .unwrap()
            .module(),
        &typed
    );

    let original = include_str!("../../../tests/language-readiness/batch336/component.zil");
    let report = compile_source(&source(original), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}

#[test]
fn terminal340_unsigned_native_and_ct() {
    let text = include_str!("../../../tests/language-readiness/terminal/340/unsigned.zil");
    review329_native(&[("main", text)], 42);
    for text in [
        "fn result()->i32{let x:u64=18446744073709551616u64;return 0}",
        "fn result()->i32{let x:u64=-1u64;return 0}",
        "fn result()->i32{let x:u64=1usize;return 0}",
        "fn unused(x:usize)->u64{return x}fn result()->i32{return 0}",
        "fn result()->i32{let x:u64=#(18446744073709551615u64+1u64);return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    for text in [
        "fn result()->i32{let x=18446744073709551615u64;let y=x+1u64;return 0}",
        "fn result()->i32{let x=0u64;let y=x-1u64;return 0}",
        "fn result()->i32{let x=18446744073709551615u64;let y=i64(x);return 0}",
        "fn result()->i32{let x:i64=-1;let y=usize(x);return 0}",
        "fn result()->i32{let x=1usize;let y=x<<64usize;return 0}",
    ] {
        native(text, None);
    }
}

#[test]
fn terminal340_unsigned_target_width_raw_wire_and_type_ct() {
    let text = "fn select()->Type{let high=0usize-%1usize;if high>4294967295usize{return bool}return usize}fn result()->i32{let value:#(select())=7;return 0}";
    let target = TargetProfile {
        id: "terminal340-32".into(),
        triple: "i686-unknown-linux-gnu".into(),
        pointer_width: 32,
        ..TargetProfile::default()
    };
    let report = compile_source(
        &source(text),
        &CompileOptions {
            target: target.clone(),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let wide = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        wide.status,
        ResultCategory::SafeReject,
        "{}",
        wide.to_json()
    );

    let limits = Limits::default();
    let input = source("fn result()->u64{return 18446744073709551615u64}");
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let typed = zil_types::check(&core, &TargetProfile::default(), &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&bytes, &typed.target, &limits)
            .unwrap()
            .module(),
        &typed
    );
    let mut broken = typed.clone();
    broken.functions[0].return_type = Type::Usize;
    let mut claim = plan.clone();
    claim.module_fingerprint = broken.fingerprint();
    assert!(
        zil_verify::verify_module(broken.clone(), claim.clone(), &typed.target, &limits).is_err()
    );
    let bytes = zil_ir::wire::encode_module_candidate(&broken, &claim, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &typed.target, &limits).is_err());
}

#[test]
fn terminal340_unsigned_budget_boundaries() {
    use zil_driver::CompilationSession;
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/340/unsigned.zil"
    ));
    let options = |work| CompileOptions {
        limits: Limits {
            max_analysis_steps: work,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let maximum = Limits::default().max_analysis_steps;
    let mut low = 1;
    let mut high = maximum;
    while low < high {
        let middle = low + (high - low) / 2;
        match compile_source(&input, &options(middle)).status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            status => panic!("unexpected budget result {status:?}"),
        }
    }
    let mut warm = CompilationSession::default();
    for (work, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(work)),
            CompilationSession::default().compile_source(&input, &options(work)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!("340 unsigned full-request analysis N={low}, N-1 rejects, fresh/warm agree");
}

#[test]
fn terminal340_option_budget_boundaries() {
    use zil_driver::CompilationSession;
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/340/option.zil"
    ));
    let options = |work| CompileOptions {
        limits: Limits {
            max_analysis_steps: work,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let maximum = Limits::default().max_analysis_steps;
    let mut low = 1;
    let mut high = maximum;
    while low < high {
        let middle = low + (high - low) / 2;
        match compile_source(&input, &options(middle)).status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            status => panic!("unexpected budget result {status:?}"),
        }
    }
    let mut warm = CompilationSession::default();
    for (work, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(work)),
            CompilationSession::default().compile_source(&input, &options(work)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!("340 Option full-request analysis N={low}, N-1 rejects, fresh/warm agree");
}

#[test]
fn terminal340_option_copy_and_owned_native() {
    for text in [
        "fn result()->i32{let value:i32?=Option.Some(42);match value{Option.Some(n)=>{return n},Option.None()=>{return 0}}}",
        "fn make()->Option<i32>{return Option.Some(42)}fn result()->i32{let value=make();match value{Option.Some(n)=>{return n},Option.None()=>{return 0}}}",
        "fn result()->i32{let value:i32?=Option.None;match value{Option.Some(_)=>{return 0},Option.None()=>{return 42}}}",
        "package P{manages data:ByteBuffer;n:i32}fn result()->i32{let value:P?=Option.Some(P{data:bytes_new(),n:42});match value{Option.Some(p)=>{return p.n},Option.None()=>{return 0}}}",
    ] {
        review329_native(&[("main", text)], 42);
    }
}

#[test]
fn terminal340_option_source_identity_and_negative_neighbors() {
    review329_native(
        &[(
            "main",
            include_str!("../../../tests/language-readiness/terminal/340/option.zil"),
        )],
        42,
    );
    review329_native(
        &[
            (
                "main",
                "import model;fn result()->i32{let value:i32?=model.make();match value{Option.Some(n)=>{return n},Option.None()=>{return 0}}}",
            ),
            (
                "model",
                "pub fn make()->Option<i32>{return Option.Some(42)}",
            ),
        ],
        42,
    );
    for text in [
        "enum Option{None,Some(i32)}fn result()->i32{return 0}",
        "type Option=i32;fn result()->i32{return 0}",
        "fn unused<Option>(x:Option)->Option{return x}fn result()->i32{return 0}",
        "fn result()->i32{let Option=1;return 0}",
        "fn result()->i32{let value:i32?=42;return 0}",
        "fn result()->i32{let value=Option.None;return 0}",
        "fn result()->i32{let value:i32?=Option.Some(true);return 0}",
        "fn result()->i32{let value:i32?=Option.None;match value{Option.Some(n)=>{return n}}}",
        "fn unused(value:i32?)->bool?{return value}fn result()->i32{return 0}",
        "package P{manages data:ByteBuffer;n:i32}fn result()->i32{let value:P?=Option.Some(P{data:bytes_new(),n:42});match value{Option.Some(p)=>{let other=p;return p.n},Option.None()=>{return 0}}}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "unexpected accepted neighbor: {text}\n{}",
            report.to_json()
        );
    }
    let limits = Limits::default();
    let target = TargetProfile::default();
    let input = source("fn unused(value:i32?)->i32{return 0}fn result()->i32{return 42}");
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&bytes, &target, &limits)
            .unwrap()
            .module(),
        &typed
    );
    for malformed in [false, true] {
        let mut raw = typed.clone();
        let function = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "unused")
            .unwrap();
        let parameter = function.parameters[0];
        let Type::Enum { name, variants } = &mut function.locals[parameter.0 as usize].ty else {
            panic!("Option must be a hygienic Enum")
        };
        if malformed {
            variants[1].1.clear()
        } else {
            name.push('0')
        }
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        let errors =
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
        assert!(
            errors.iter().any(|e| e.code == "E-VERIFY-OPTION"),
            "{errors:?}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
    }
}

#[test]
fn terminal340_reference_places_native() {
    review329_native(
        &[(
            "main",
            include_str!("../../../tests/language-readiness/terminal/340/reference.zil"),
        )],
        42,
    );
    for text in [
        "fn result()->i32{let n=42;let ref p=n;return *p}",
        "fn result()->i32{var n=20;let ref mut p=n;*p+=22;return n}",
        "fn result()->i32{var pair=(20,22);let(ref x,ref y)=pair;return *x+*y}",
        "fn result()->i32{let value:i32?=Option.Some(42);match value{Option.Some(ref p)=>{return *p},Option.None()=>{return 0}}}",
        "fn result()->i32{var value:i32?=Option.Some(20);match value{Option.Some(ref mut p)=>{*p+=22;return *p},Option.None()=>{return 0}}}",
        "enum E{Pair(i32,i32),None}fn result()->i32{var value=E.Pair(20,22);match value{E.Pair(ref mut a,ref mut b)=>{*a+=*b;return *a},E.None()=>{return 0}}}",
        "enum Inner{Pair(i32,i32),None}enum Outer{Nest(Inner),None}fn result()->i32{var value=Outer.Nest(Inner.Pair(20,22));match value{Outer.Nest(Inner.Pair(ref mut a,ref mut b))=>{*a+=*b;return *a},Outer.Nest(Inner.None())=>{return 0},Outer.None()=>{return 0}}}",
        "package P{manages data:ByteBuffer;n:i32}fn result()->i32{let value:P?=Option.Some(P{data:bytes_new(),n:42});match value{Option.Some(ref p)=>{return p.n},Option.None()=>{return 0}}}",
        "enum Inner{A(i32),B(i32)}enum Outer{Nest(Inner),None}fn result()->i32{var value=Outer.Nest(Inner.A(20));match value{Outer.Nest(ref mut inner)=>{*inner=Inner.B(42)},Outer.None()=>{return 0}}match value{Outer.Nest(Inner.B(ref p))=>{return *p},Outer.Nest(Inner.A(_))=>{return 1},Outer.None()=>{return 0}}}",
    ] {
        review329_native(&[("main", text)], 42);
    }
}

#[test]
fn terminal340_reference_conflicts_and_raw_tag_neighbors() {
    for text in [
        "fn result()->i32{let n=20;let ref mut p=n;return *p}",
        "fn result()->i32{var value:i32?=Option.Some(42);match value{Option.Some(ref p)=>{value=Option.None;return *p},Option.None()=>{return 0}}}",
        "fn result()->i32{var value:i32?=Option.Some(42);match value{Option.Some(ref mut p)=>{let other=&value;return *p},Option.None()=>{return 0}}}",
        "fn result()->i32{let pair=(20,22);let(ref n,ref n)=pair;return *n}",
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
    review329_native(
        &[(
            "main",
            "fn result()->i32{var value:i32?=Option.Some(42);match value{Option.Some(ref p)=>{let result=*p;value=Option.None;return result},Option.None()=>{return 0}}}",
        )],
        42,
    );
    let input = source(
        "enum E{Data(i32),None}fn read(value:E)->i32{match value{E.Data(ref p)=>{return *p},E.None()=>{return 0}}}fn result()->i32{return read(E.Data(42))}",
    );
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&bytes, &target, &limits)
            .unwrap()
            .module(),
        &typed
    );
    for remove_guard in [false, true] {
        let mut raw = typed.clone();
        let function = raw.functions.iter_mut().find(|f| f.name == "read").unwrap();
        if remove_guard {
            let tag = function
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| {
                    matches!(i.kind, InstructionKind::VariantTagLocal { .. })
                        .then(|| i.result.unwrap())
                })
                .unwrap();
            let guard=function.blocks.iter_mut().flat_map(|b|&mut b.instructions).find(|i|matches!(i.kind,InstructionKind::Binary{left,right,..} if left==tag || right==tag)).unwrap();
            guard.kind = InstructionKind::Constant(zil_ir::Constant::Bool(true));
        } else {
            let borrow = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::BorrowVariantLocal { .. }))
                .unwrap();
            let InstructionKind::BorrowVariantLocal { path, .. } = &mut borrow.kind else {
                unreachable!()
            };
            path[0].index = 1;
        }
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze accepted forged active payload path"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        assert!(
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).is_err(),
            "Verify accepted forged active payload path"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
    }
}

#[test]
fn terminal340_reference_budget_boundaries() {
    use zil_driver::CompilationSession;
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/340/reference.zil"
    ));
    let options = |work| CompileOptions {
        limits: Limits {
            max_analysis_steps: work,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let maximum = Limits::default().max_analysis_steps;
    let mut low = 1;
    let mut high = maximum;
    while low < high {
        let middle = low + (high - low) / 2;
        match compile_source(&input, &options(middle)).status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            status => panic!("unexpected budget result {status:?}"),
        }
    }
    let mut warm = CompilationSession::default();
    for (work, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(work)),
            CompilationSession::default().compile_source(&input, &options(work)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!("340 reference full-request analysis N={low}, N-1 rejects, fresh/warm agree");
}

#[test]
fn terminal340_unsigned_native_wrapper_and_api_traps() {
    use zil_driver::{NativeBackend, Optimization};
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (text, expected) in [
                (
                    "fn result()->u64{return 18446744073709551615u64}",
                    Some("18446744073709551615\n"),
                ),
                (
                    "fn result()->usize{return 18446744073709551615usize}",
                    Some("18446744073709551615\n"),
                ),
                (
                    "fn result()->i32{let x=18446744073709551615u64;let y=x+1u64;return 0}",
                    None,
                ),
                ("fn result()->i32{let x=0u64;let y=x-1u64;return 0}", None),
                (
                    "fn result()->i32{let x=18446744073709551615u64;let y=i64(x);return 0}",
                    None,
                ),
                (
                    "fn result()->i32{let x:i64=-1;let y=usize(x);return 0}",
                    None,
                ),
                (
                    "fn result()->i32{let x=1usize;let y=x<<64usize;return 0}",
                    None,
                ),
            ] {
                let directory = Temp::new();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        entry: Some("result".into()),
                        output: Some(directory.0.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let observed = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                if let Some(expected) = expected {
                    assert!(observed.status.success());
                    assert_eq!(observed.stdout, expected.as_bytes())
                } else {
                    assert!(
                        !observed.status.success(),
                        "{backend:?}/{optimization:?} {text}"
                    )
                }
            }
        }
    }
}

#[test]
fn terminal340_addressed_sums_and_tuple_siblings_native() {
    for text in [
        "fn result()->i32{let(ref mut a,b)=(20,22);*a+=b;return *a}",
        "fn result()->i32{let(ref mut a,b)=(1,42);return b}",
        "enum E{Data(i32),None}fn inspect(value:&E)->i32{match *value{E.Data(ref p)=>{return *p},E.None()=>{return 0}}}fn result()->i32{let value=E.Data(42);return inspect(&value)}",
        "enum E{Pair(i32,i32),None}fn mutate(value:&mut E)->i32{match *value{E.Pair(ref mut a,ref mut b)=>{*a+=*b;return *a},E.None()=>{return 0}}}fn result()->i32{var value=E.Pair(20,22);return mutate(&mut value)}",
        "package P{manages bytes:ByteBuffer;n:i32}fn inspect(value:&Option<P>)->i32{match *value{Option.Some(ref p)=>{return p.n},Option.None()=>{return 0}}}fn result()->i32{let value:P?=Option.Some(P{bytes:bytes_new(),n:42});return inspect(&value)}",
        "package P{manages bytes:ByteBuffer;n:i32}fn inspect(value:&Option<P>)->i32{match *value{Option.Some(_)=>{return 42},Option.None()=>{return 0}}}fn result()->i32{let value:P?=Option.Some(P{bytes:bytes_new(),n:1});return inspect(&value)}",
        "enum E{Data(i32)}fn select(value:&E)->&i32{match *value{E.Data(ref p)=>{return p}}}fn result()->i32{let value=E.Data(42);let p=select(&value);return *p}",
        "enum E{Data(i32)}fn select(value:&E)->&i32{match *value{E.Data(ref p)=>{return p}}}fn result()->i32{var value=E.Data(42);let p=select(&value);let n=*p;value=E.Data(1);return n}",
        "fn result()->i32{var pair=(20,22);let(ref a,b)=pair;let f=()->i32=>*a;let n=f();pair=(1,2);return n+b}",
        "enum E{Data(i32),None}fn worker(n:i32)->i32{return n}fn inspect(value:&mut E)->i32{let body=()->i32=>{*value=E.Data(42);return 0};thread_scope(worker,0,body);match *value{E.Data(ref p)=>{return *p},E.None()=>{return 0}}}fn result()->i32{var value=E.None;return inspect(&mut value)}",
    ] {
        review329_native(&[("main", text)], 42);
    }
}

#[test]
fn terminal340_addressed_sums_raw_and_permission_neighbors() {
    for text in [
        "enum E{Data(i32)}fn select(value:&E)->&i32{match *value{E.Data(ref p)=>{return p}}}fn result()->i32{var value=E.Data(42);let p=select(&value);value=E.Data(1);return *p}",
        "enum E{Data(i32)}fn inspect(value:&E)->i32{match *value{E.Data(ref mut p)=>{return *p}}}fn result()->i32{let value=E.Data(42);return inspect(&value)}",
        "fn result()->i32{var pair=(20,22);let(ref a,b)=pair;let f=()->i32=>*a;pair=(1,2);return f()+b}",
        "fn result()->i32{var pair=(20,22);let(ref mut a,ref mut b)=pair;let whole=&pair;return *a+*b+whole.0}",
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
    let input = source(
        "enum E{Data(i32),Other(i32)}fn inspect(value:&E)->i32{match *value{E.Data(ref p)=>{return *p},E.Other(_)=>{return 0}}}fn result()->i32{let value=E.Data(42);return inspect(&value)}",
    );
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_ok());
    for mode in 0..3 {
        let mut raw = typed.clone();
        let f = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "inspect")
            .unwrap();
        if mode == 0 {
            let block = f
                .blocks
                .iter_mut()
                .find(|b| matches!(b.terminator, zil_ir::Terminator::Branch { .. }))
                .unwrap();
            let zil_ir::Terminator::Branch {
                then_block,
                else_block,
                ..
            } = &mut block.terminator
            else {
                unreachable!()
            };
            *else_block = *then_block;
        } else {
            let i = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::BorrowVariantAddress { .. }))
                .unwrap();
            let InstructionKind::BorrowVariantAddress { path, .. } = &mut i.kind else {
                unreachable!()
            };
            if mode == 1 {
                path[0].variant = 1
            } else {
                path[0].index = 1
            }
        }
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze accepted forged address proof {mode}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        let error =
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
        assert!(
            error.iter().any(|d| d.code == "E-VERIFY-SUM-ADDRESS"),
            "{mode}: {error:?}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
    }
}

#[test]
fn terminal340_addressed_tag_call_epoch_neighbors() {
    for (prelude, call, mode) in [
        ("fn observe()->i32{return 0}", "observe();", 0),
        (
            "fn worker(n:i32)->i32{return n}",
            "let body=()->i32=>0;thread_scope(worker,0,body);",
            1,
        ),
        ("", "let body=()->i32=>0;let shared=&body;shared();", 2),
        (
            "resource Mark{code:i32}impl Drop for Mark{fn drop(self:Self){}}",
            "let owner=Mark{code:0};let body=move()->i32=>owner.code;body();",
            3,
        ),
    ] {
        let text = format!(
            "enum E{{Data(i32),None}}{prelude}fn inspect(value:&E)->i32{{{call}match *value{{E.Data(ref p)=>{{return *p}},E.None()=>{{return 0}}}}}}fn result()->i32{{let value=E.Data(42);return inspect(&value)}}"
        );
        // The current observation is made after the call in this real source.
        review329_native(&[("main", &text)], 42);
        let input = source(&text);
        let ast = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand(&input, &ast).unwrap();
        let limits = Limits::default();
        let target = TargetProfile::default();
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        let mut raw = typed.clone();
        let f = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "inspect")
            .unwrap();
        let (from, position) = f
            .blocks
            .iter()
            .enumerate()
            .find_map(|(block, b)| {
                b.instructions
                    .iter()
                    .position(|i| match mode {
                        0 => matches!(i.kind, InstructionKind::Call { .. }),
                        1 => matches!(i.kind, InstructionKind::WithScopedThread { .. }),
                        2 => matches!(i.kind, InstructionKind::CallClosureBorrowed { .. }),
                        _ => matches!(i.kind, InstructionKind::CallClosureShared { .. }),
                    })
                    .map(|position| (block, position))
            })
            .unwrap_or_else(|| panic!("missing call form {mode}"));
        let call = f.blocks[from].instructions.remove(position);
        let block = f
            .blocks
            .iter_mut()
            .find(|b| {
                b.instructions
                    .iter()
                    .any(|i| matches!(i.kind, InstructionKind::BorrowVariantAddress { .. }))
            })
            .unwrap();
        let before = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::BorrowVariantAddress { .. }))
            .unwrap();
        block.instructions.insert(before, call);
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze accepted stale tag across call form {mode}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        let error =
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
        assert!(
            error.iter().any(|d| d.code == "E-VERIFY-SUM-ADDRESS"),
            "{mode}: {error:?}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
    }
}

#[test]
fn terminal340_addressed_tag_child_mutation_epoch() {
    let text = "enum E{Data(i32),None}fn replace(value:&mut E){*value=E.None}fn inspect(value:&mut E)->i32{match *value{E.Data(ref p)=>{return *p},E.None()=>{return 0}}}fn result()->i32{var value=E.Data(42);return inspect(&mut value)}";
    review329_native(&[("main", text)], 42);
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let mut raw = zil_types::check(&core, &target, &limits).unwrap();
    let mut claim = zil_analyze::analyze(&raw, &limits).unwrap();
    let replace = raw
        .functions
        .iter()
        .find(|f| f.name == "replace")
        .unwrap()
        .id;
    let f = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "inspect")
        .unwrap();
    let address = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let InstructionKind::VariantTagAddress { address, .. } = i.kind {
                Some(address)
            } else {
                None
            }
        })
        .unwrap();
    let child = zil_ir::ValueId(f.values.len() as u32);
    f.values.push(f.values[address.0 as usize].clone());
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::BorrowVariantAddress { .. }))
        })
        .unwrap();
    let position = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::BorrowVariantAddress { .. }))
        .unwrap();
    let span = block.instructions[position].span;
    block.instructions.splice(
        position..position,
        [
            zil_ir::Instruction {
                result: Some(child),
                kind: InstructionKind::ReborrowAddress {
                    address,
                    mutable: true,
                },
                span,
            },
            zil_ir::Instruction {
                result: None,
                kind: InstructionKind::Call {
                    function: replace,
                    arguments: vec![child],
                },
                span,
            },
        ],
    );
    let fp = claim
        .functions
        .iter_mut()
        .find(|p| p.function == f.id)
        .unwrap();
    fp.holds.push(zil_ir::ValueHold {
        value: child,
        roots: vec![zil_ir::HoldRoot::Parameter(0)],
    });
    claim.module_fingerprint = raw.fingerprint();
    assert!(zil_analyze::analyze(&raw, &limits).is_err());
    let errors =
        zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
    assert!(
        errors.iter().any(|d| d.code == "E-VERIFY-SUM-ADDRESS"),
        "{errors:?}"
    );
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
    // The nearest current observation after the legal child mutation succeeds.
    let current = "enum E{Data(i32),None}fn replace(value:&mut E){*value=E.Data(42)}fn inspect(value:&mut E)->i32{replace(&mut *value);match *value{E.Data(ref p)=>{return *p},E.None()=>{return 0}}}fn result()->i32{var value=E.None;return inspect(&mut value)}";
    review329_native(&[("main", current)], 42);
}

#[test]
fn terminal340_addressed_budget_boundaries() {
    use zil_driver::CompilationSession;
    let text = include_str!("../../../tests/language-readiness/terminal/340/addressed.zil");
    review329_native(&[("main", text)], 42);
    let input = source(text);
    let options = |work| CompileOptions {
        limits: Limits {
            max_analysis_steps: work,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let mut low = 1;
    let mut high = Limits::default().max_analysis_steps;
    while low < high {
        let middle = low + (high - low) / 2;
        match compile_source(&input, &options(middle)).status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            status => panic!("unexpected budget result {status:?}"),
        }
    }
    let mut warm = CompilationSession::default();
    for (work, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(work)),
            CompilationSession::default().compile_source(&input, &options(work)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!(
        "340 addressed/inline-field full-request analysis N={low}, N-1 rejects, fresh/warm agree"
    );
}

#[test]
fn terminal340_option_owned_reference_drop_once() {
    for some in [true, false] {
        for binding in ["ref payload", "_"] {
            let result = if binding == "_" { "42" } else { "payload.n" };
            let text = format!(
                r#"extern "C" fn putchar(code:i32)->i32;resource Token{{code:i32}}impl Drop for Token{{fn drop(self:Self){{unsafe{{putchar(.code)}}}}}}package P{{manages token:Token;n:i32}}fn build(some:bool)->P?{{if some{{return Option.Some(P{{token:Token{{code:65}},n:42}})}}return Option.None}}fn work()->i32{{let owner=build({some});match owner{{Option.Some({binding})=>{{return {result}}},Option.None()=>{{return 42}}}}}}fn result()->i32{{return work()}}"#
            );
            review329_native_output(&[("main", &text)], if some { b"A42\n" } else { b"42\n" });
        }
    }
}

#[test]
fn terminal340_inline_field_raw_authority_and_alias_neighbors() {
    let input = source("fn result()->i32{let(ref mut a,b)=(20,22);*a+=b;return *a}");
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    for mode in 0..3 {
        let mut raw = typed.clone();
        let f = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "result")
            .unwrap();
        if mode == 0 {
            let local = f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| {
                    if let InstructionKind::BorrowFieldLocal {
                        local,
                        mutable: true,
                        ..
                    } = i.kind
                    {
                        Some(local)
                    } else {
                        None
                    }
                })
                .unwrap();
            f.locals[local.0 as usize].mutable = false;
        } else {
            let i = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| {
                    matches!(
                        i.kind,
                        InstructionKind::BorrowFieldLocal { mutable: false, .. }
                    )
                })
                .unwrap();
            let InstructionKind::BorrowFieldLocal { path, .. } = &mut i.kind else {
                unreachable!()
            };
            path[0] = if mode == 1 { 0 } else { 2 };
        }
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze accepted forged inline field {mode}"
        );
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        assert!(
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).is_err(),
            "Verify accepted forged inline field {mode}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
    }
}

#[test]
fn terminal340_local_variant_alias_and_nested_parent_neighbors() {
    for first in ["ref a", "ref mut a"] {
        let text = format!(
            "enum E{{Pair(i32,i32)}}fn result()->i32{{var value=E.Pair(20,22);match value{{E.Pair({first},ref mut b)=>{{return *a+*b}}}}}}"
        );
        review329_native(&[("main", &text)], 42);
        let input = source(&text);
        let ast = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand(&input, &ast).unwrap();
        let target = TargetProfile::default();
        let limits = Limits::default();
        let mut raw = zil_types::check(&core, &target, &limits).unwrap();
        let mut claim = zil_analyze::analyze(&raw, &limits).unwrap();
        let i = raw
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
            .filter(|i| matches!(i.kind, InstructionKind::BorrowVariantLocal { .. }))
            .nth(1)
            .unwrap();
        let InstructionKind::BorrowVariantLocal { path, .. } = &mut i.kind else {
            unreachable!()
        };
        path[0].index = 0;
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze accepted local106 alias {first}"
        );
        claim.module_fingerprint = raw.fingerprint();
        assert!(zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).is_err());
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
    }
    for text in [
        "enum E{Data(i32)}fn selected(value:&mut E)->&mut i32{match *value{E.Data(ref mut p)=>{return p}}}fn result()->i32{var value=E.Data(20);let p=selected(&mut value);*p+=22;return *p}",
        "enum Inner{Data(i32),None}enum Outer{Data(Inner),None}fn inspect(value:&mut Outer)->i32{match *value{Outer.Data(ref mut inner)=>{match *inner{Inner.Data(ref mut p)=>{*p+=22;return *p},Inner.None()=>{return 0}}},Outer.None()=>{return 0}}}fn result()->i32{var value=Outer.Data(Inner.Data(20));return inspect(&mut value)}",
    ] {
        review329_native(&[("main", text)], 42)
    }
}

#[test]
fn terminal340_nested_tuple_reference_places() {
    for text in [
        "fn result()->i32{var pair=(20,22);match pair{(ref mut a,ref mut b)=>{*a+=*b;return *a}}}",
        "fn result()->i32{match (20,22){(ref mut a,b)=>{*a+=b;return *a}}}",
        "enum E{Data((i32,i32)),None}fn result()->i32{var value=E.Data((20,22));match value{E.Data((ref mut a,ref mut b))=>{*a+=*b;return *a},E.None()=>{return 0}}}",
        "enum E{Data((i32,i32)),None}fn inspect(value:&mut E)->i32{match *value{E.Data((ref mut a,ref mut b))=>{*a+=*b;return *a},E.None()=>{return 0}}}fn result()->i32{var value=E.Data((20,22));return inspect(&mut value)}",
        "enum E{Data(i32)}fn result()->i32{var holder=(E.Data(20),0);match holder.0{E.Data(ref mut p)=>{*p+=22}}match holder.0{E.Data(n)=>{return n}}}",
        "fn inspect(value:&mut(i32,i32))->i32{match *value{(ref mut a,ref mut b)=>{*a+=*b;return *a}}}fn result()->i32{var pair=(20,22);return inspect(&mut pair)}",
        "enum E{Data(i32)}fn result()->i32{var holder=(20,E.Data(22));let(ref mut a,_)=holder;match holder.1{E.Data(ref b)=>{*a+=*b;return *a}}}",
    ] {
        review329_native(&[("main", text)], 42)
    }
    let text = "enum E{Data((i32,i32)),None}fn inspect(value:&mut E)->i32{match *value{E.Data((ref mut a,ref mut b))=>{*a+=*b;return *a},E.None()=>{return 0}}}fn result()->i32{var value=E.Data((20,22));return inspect(&mut value)}";
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let mut raw = zil_types::check(&core, &target, &limits).unwrap();
    let mut claim = zil_analyze::analyze(&raw, &limits).unwrap();
    let i = raw
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::AddressField { index: 1, .. }))
        .unwrap();
    let InstructionKind::AddressField { index, .. } = &mut i.kind else {
        unreachable!()
    };
    *index = 0;
    assert!(zil_analyze::analyze(&raw, &limits).is_err());
    claim.module_fingerprint = raw.fingerprint();
    assert!(zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).is_err());
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
}

#[test]
fn terminal340_reference_pattern_conditions() {
    for text in [
        "fn result()->i32{var value=Option.Some(20);if let Option.Some(ref mut n)=value{*n+=22}else{return 0}match value{Option.Some(n)=>{return n},Option.None()=>{return 0}}}",
        "fn result()->i32{var value=Option.Some(20);while let Option.Some(ref mut n)=value{*n+=22;break}match value{Option.Some(n)=>{return n},Option.None()=>{return 0}}}",
        "enum E{Data(i32),None}fn work(value:&mut E)->i32{if let E.Data(ref mut n)=*value{*n+=22;return *n}return 0}fn result()->i32{var value=E.Data(20);return work(&mut value)}",
        "fn result()->i32{var pair=(20,22);if let(ref mut a,b)=pair{*a+=b}return pair.0}",
    ] {
        review329_native(&[("main", text)], 42);
    }
    let prefix = r#"extern "C" fn putchar(code:i32)->i32;resource Token{code:i32}impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}package P{manages token:Token;n:i32}fn build()->P?{return Option.Some(P{token:Token{code:65},n:21})}"#;
    for body in [
        "fn result()->i32{if let Option.Some(ref payload)=build(){return payload.n*2}return 0}",
        "fn result()->i32{if let Option.None()=build(){return 0}return 42}",
        "fn result()->i32{while let Option.Some(ref payload)=build(){let n=payload.n;break}return 42}",
    ] {
        review329_native_output(&[("main", &format!("{prefix}{body}"))], b"A42\n");
    }
}

#[test]
fn terminal340_owned_tuple_complete_transfers() {
    review329_native_output(
        &[(
            "main",
            include_str!("../../../tests/language-readiness/terminal/340/owned-tuples.zil"),
        )],
        b"A42\n",
    );
    let prefix = r#"extern "C" fn putchar(code:i32)->i32;resource Token{code:i32}impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}package P{manages token:Token;n:i32}"#;
    for body in [
        "fn result()->i32{let(p,n)=(P{token:Token{code:65},n:20},22);return p.n+n}",
        "fn result()->i32{let pair=(P{token:Token{code:65},n:20},22);return 42}",
        "fn identity(pair:(P,i32))->(P,i32){return pair}fn result()->i32{let(p,n)=identity((P{token:Token{code:65},n:20},22));return p.n+n}",
        "fn result()->i32{let((p,n),m)=((P{token:Token{code:65},n:0},20),22);return n+m}",
        "fn result()->i32{let(_,n)=(Token{code:65},42);return n}",
        "enum Failure{Bad}fn fail()->Failure!i32{return Failure.Bad()}fn work()->Failure!i32{let pair=(P{token:Token{code:65},n:20},fail()?);return 0}fn result()->i32{work();return 42}",
    ] {
        review329_native_output(&[("main", &format!("{prefix}{body}"))], b"A42\n");
    }
}

#[test]
fn terminal340_owned_tuple_authority_neighbors() {
    let text =
        "resource Token{n:i32}fn result()->i32{let(a,b)=(Token{n:20},Token{n:22});return 42}";
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let raw = zil_types::check(&core, &target, &limits).unwrap();
    let claim = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap();
    for duplicate in [true, false] {
        let mut forged = raw.clone();
        let f = forged
            .functions
            .iter_mut()
            .find(|f| f.name == "result")
            .unwrap();
        let i = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::UnpackTuple { .. }))
            .unwrap();
        let InstructionKind::UnpackTuple { local, fields } = &mut i.kind else {
            unreachable!()
        };
        if duplicate {
            fields[1] = fields[0];
        } else {
            fields[0] = *local;
        }
        assert!(zil_analyze::analyze(&forged, &limits).is_err());
        let mut c = claim.clone();
        c.module_fingerprint = forged.fingerprint();
        assert!(zil_verify::verify_module(forged.clone(), c.clone(), &target, &limits).is_err());
        let bytes = zil_ir::wire::encode_module_candidate(&forged, &c, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
    }
    let bad = "resource Token{n:i32}package P{manages token:Token}fn consume(pair:(P,i32))->i32{let(_,n)=pair;return n}fn result()->i32{let pair=(P{token:Token{n:20}},42);consume(pair);return consume(pair)}";
    let report = compile_source(
        &source(bad),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn terminal340_owned_tuple_budget_boundaries() {
    use zil_driver::CompilationSession;
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/340/owned-tuples.zil"
    ));
    let options = |work| CompileOptions {
        limits: Limits {
            max_analysis_steps: work,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
    while low < high {
        let mid = low + (high - low) / 2;
        let report = compile_source(&input, &options(mid));
        match report.status {
            ResultCategory::Success => high = mid,
            ResultCategory::ResourceLimit => low = mid + 1,
            _ => panic!("unexpected tuple budget result {}", report.to_json()),
        }
    }
    let mut warm = CompilationSession::default();
    for (work, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(work)),
            CompilationSession::default().compile_source(&input, &options(work)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!(
        "340 complete owned Tuple full-request analysis N={low}, N-1 rejects, fresh/warm agree"
    );
}

#[test]
fn terminal340_selective_mixed_patterns_native() {
    review329_native_output(
        &[(
            "main",
            include_str!("../../../tests/language-readiness/terminal/340/mixed.zil"),
        )],
        b"BAC42\n",
    );

    let prefix = r#"extern "C" fn putchar(code:i32)->i32;resource Token{code:i32}impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}package P{manages token:Token;n:i32}"#;
    for(index,(body,expected))in[
        ("fn result()->i32{let(a,b,c)=(Token{code:65},Token{code:66},Token{code:67});return 42}","ABC42\n"),
        ("fn result()->i32{var value=(P{token:Token{code:65},n:20},P{token:Token{code:66},n:22},P{token:Token{code:67},n:0});match value{(ref a,b,_)=>{return a.n+b.n}}}","BAC42\n"),
        ("enum E{Data(P,P,P),None}fn result()->i32{var value=E.Data(P{token:Token{code:65},n:20},P{token:Token{code:66},n:22},P{token:Token{code:67},n:0});match value{E.Data(ref a,b,_)=>{return a.n+b.n},E.None()=>{return 0}}}","BAC42\n"),
        ("enum E{Data((P,P,i32)),None}fn result()->i32{var value=E.Data((P{token:Token{code:65},n:20},P{token:Token{code:66},n:22},0));match value{E.Data((ref a,b,n))=>{return a.n+b.n+n},E.None()=>{return 0}}}","BA42\n"),
        ("enum E{Data(P)}fn result()->i32{var value=(E.Data(P{token:Token{code:65},n:20}),P{token:Token{code:66},n:22});match value{(E.Data(ref a),b)=>{return a.n+b.n}}}","BA42\n"),
        ("fn result()->i32{let(ref a,b)=(P{token:Token{code:65},n:20},P{token:Token{code:66},n:22});return a.n+b.n}","BA42\n"),
        ("enum E{Data(P,P,P),None}fn result()->i32{let value=E.Data(P{token:Token{code:65},n:20},P{token:Token{code:66},n:22},P{token:Token{code:67},n:0});if let E.Data(ref a,b,_)=value{return a.n+b.n}return 0}","BAC42\n"),
        ("enum E{Data(P,P,P),None}fn result()->i32{let value=E.Data(P{token:Token{code:65},n:20},P{token:Token{code:66},n:22},P{token:Token{code:67},n:0});while let E.Data(ref a,b,_)=value{let n=a.n+b.n;break}return 42}","BAC42\n"),
        ("fn inspect(value:&mut(P,i32))->i32{match *value{(ref p,ref mut n)=>{*n+=p.n;return *n}}}fn result()->i32{var value=(P{token:Token{code:65},n:20},22);return inspect(&mut value)}","A42\n"),
        ("enum E{Data(P,P),None}fn result()->i32{var value=E.Data(P{token:Token{code:65},n:20},P{token:Token{code:66},n:22});match value{E.Data(ref a,b)=>{let n=a.n+b.n},E.None()=>{}}match value{E.Data(ref a,_)=>{return a.n+22},E.None()=>{return 0}}}","BA42\n"),
        ("fn result()->i32{match(P{token:Token{code:65},n:20},P{token:Token{code:66},n:22}){(ref a,b)=>{let n=a.n+b.n}}unsafe{putchar(88)}return 42}","BAX42\n"),
        ("fn result()->i32{var value=(P{token:Token{code:65},n:20},P{token:Token{code:66},n:22});match value{(ref a,b)=>{let n=a.n+b.n}}unsafe{putchar(88)}return 42}","BXA42\n"),
        ("enum E{Data(P,P,i32),None}fn result()->i32{var value=E.Data(P{token:Token{code:65},n:20},P{token:Token{code:66},n:22},1);match value{E.Data(ref a,b,0)=>{return 0},E.Data(ref a,b,_)=>{return a.n+b.n},E.None()=>{return 0}}}","BA42\n"),
        ("fn inspect(value:&mut(P,i32))->i32{let(ref p,ref mut n)=*value;*n+=p.n;return *n}fn result()->i32{var value=(P{token:Token{code:65},n:20},22);return inspect(&mut value)}","A42\n"),
        ("fn result()->i32{match(P{token:Token{code:65},n:20},P{token:Token{code:66},n:22}){(a,b)=>{return a.n+b.n}}}","AB42\n"),
        ("enum Failure{Bad}fn make(bad:bool)->Failure!P{if bad{return Failure.Bad()}return P{token:Token{code:65},n:20}}fn result()->i32{var value=(make(false),P{token:Token{code:66},n:22});match value{(ref outcome,b)=>{return b.n+20}}}","BA42\n"),
        ("enum Failure{Bad}fn make(bad:bool)->Failure!P{if bad{return Failure.Bad()}return P{token:Token{code:65},n:20}}fn result()->i32{var value=(make(true),P{token:Token{code:66},n:22});match value{(ref outcome,b)=>{return b.n+20}}}","B42\n"),
        ("fn result()->i32{match(P{token:Token{code:65},n:20},P{token:Token{code:66},n:22}){(ref a,b)=>{let n=a.n+b.n;return n}}}","BA42\n"),
        ("fn accept(p:P)->i32{return p.n}fn result()->i32{let pair=(P{token:Token{code:65},n:20},P{token:Token{code:66},n:22}|>accept);let(p,n)=pair;return p.n+n}","BA42\n"),
        ("fn result()->i32{var i=0;while i<2{match(P{token:Token{code:65},n:20},P{token:Token{code:66},n:22}){(ref a,b)=>{let n=a.n+b.n;i+=1;if i<2{continue}break}}}return 42}","BABA42\n"),
    ].into_iter().enumerate(){
        eprintln!("340 mixed native case {index}");
        review329_native_output(&[("main",&format!("{prefix}{body}"))],expected.as_bytes());
    }
}

#[test]
fn terminal340_selective_patterns_raw_wire_neighbors() {
    use zil_ir::{PlaceProjection, ValueHold, ValueId};
    let text = "resource R{n:i32}package P{manages r:R;n:i32}enum E{Data(P,P,i32),Other(P,P,i32),None}fn result()->i32{var value=E.Data(P{r:R{n:1},n:20},P{r:R{n:2},n:22},0);match value{E.Data(ref a,b,0)=>{return a.n+b.n},E.Data(_,_,_)=>{return 0},E.Other(_,_,_)=>{return 0},E.None()=>{return 0}}}";
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    for mode in 0..7 {
        let mut raw = typed.clone();
        let mut claim = plan.clone();
        let f = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "result")
            .unwrap();
        let (b, pos) = f
            .blocks
            .iter()
            .enumerate()
            .find_map(|(b, block)| {
                block
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, InstructionKind::TakeVariantField { .. }))
                    .map(|i| (b, i))
            })
            .unwrap();
        let original = f.blocks[b].instructions[pos].clone();
        let InstructionKind::TakeVariantField { local, path } = &original.kind else {
            unreachable!()
        };
        let (local, path) = (*local, path.clone());
        if mode < 3 {
            let mut extra = original.clone();
            let id = ValueId(f.values.len() as u32);
            let result_type = if mode == 2 {
                f.locals[local.0 as usize].ty.clone()
            } else if mode == 1 {
                let Type::Package { fields, .. } = &f.values[original.result.unwrap().0 as usize]
                else {
                    unreachable!()
                };
                fields[0].1.clone()
            } else {
                f.values[original.result.unwrap().0 as usize].clone()
            };
            extra.result = Some(id);
            f.values.push(result_type);
            extra.kind = if mode == 2 {
                InstructionKind::TakeLocal { local }
            } else {
                let mut p = path.clone();
                if mode == 1 {
                    p.push(PlaceProjection::Field(0));
                }
                InstructionKind::TakeVariantField { local, path: p }
            };
            f.blocks[b].instructions.insert(pos + 1, extra);
            claim.functions[f.id.0 as usize].holds.push(ValueHold {
                value: id,
                roots: Vec::new(),
            });
        } else if mode == 3 {
            let i=f.blocks.iter_mut().flat_map(|b|&mut b.instructions).find(|i|matches!(&i.kind,InstructionKind::BorrowPlaceLocal{path,..} if matches!(i.result.and_then(|v|f.values.get(v.0 as usize)),Some(Type::Address{inner,..}) if matches!(inner.as_ref(),Type::Package{..})) && path.len()==1)).unwrap();
            let InstructionKind::BorrowPlaceLocal {
                path: ref mut borrowed,
                ..
            } = i.kind
            else {
                unreachable!()
            };
            *borrowed = path.clone();
        } else if mode == 5 {
            let block = f
                .blocks
                .iter_mut()
                .find(|b| matches!(b.terminator, zil_ir::Terminator::Branch { .. }))
                .unwrap();
            let zil_ir::Terminator::Branch {
                then_block,
                else_block,
                ..
            } = &mut block.terminator
            else {
                unreachable!()
            };
            *else_block = *then_block;
        } else {
            let i = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::TakeVariantField { .. }))
                .unwrap();
            let InstructionKind::TakeVariantField { path, .. } = &mut i.kind else {
                unreachable!()
            };
            if mode == 4 {
                path[0] = PlaceProjection::ErrorArm(true);
            } else {
                let PlaceProjection::Variant(p) = &mut path[0] else {
                    unreachable!()
                };
                p.variant = 1;
            }
        }
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze accepted selective forgery {mode}"
        );
        claim.module_fingerprint = raw.fingerprint();
        let errors =
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|e| e.category == ResultCategory::SafeReject
                    || e.category == ResultCategory::Unsupported),
            "{mode}: {errors:?}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "Wire accepted selective forgery {mode}"
        );
    }
    for body in [
        "fn result()->i32{var value=(P{r:R{n:1},n:20},P{r:R{n:2},n:22});match value{(ref a,b)=>{let n=a.n+b.n}}let whole=&value;return 42}",
        "fn consume(value:(P,P))->i32{return 42}fn result()->i32{var value=(P{r:R{n:1},n:20},P{r:R{n:2},n:22});match value{(ref a,b)=>{let n=a.n+b.n}}return consume(value)}",
        "fn bad(value:(P,i32))->R contract{result(create)}{let(p,_)=value;return give p.r}fn result()->i32{return 42}",
        "enum E{Pair(P,P),None}fn bad(value:E)->R contract{result(create)}{match value{E.Pair(ref a,b)=>{return give b.r},E.None()=>{return R{n:0}}}}fn result()->i32{return 42}",
    ] {
        let report = compile_source(
            &source(&format!(
                "resource R{{n:i32}}package P{{manages r:R;n:i32}}{body}"
            )),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn terminal340_selective_mutation_epoch_neighbors() {
    use zil_ir::{HoldRoot, PlaceProjection, ValueHold, ValueId};
    let prefix = r#"extern "C" fn putchar(code:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}package P{manages r:R;n:i32}enum C{Some(i32),None(i32)}fn set_none(value:&mut C){*value=C.None(42)}fn set_some(value:&mut C){*value=C.Some(0)}"#;
    let fresh = "fn result()->i32{var value=(C.Some(0),P{r:R{code:65},n:0});let(ref mut p,_)=value;match *p{C.Some(_)=>{set_none(p)},C.None(_)=>{}}match value{(C.None(n),ref owner)=>{return n+owner.n},(C.Some(_),_)=>{return 0}}}";
    review329_native_output(&[("main", &format!("{prefix}{fresh}"))], b"A42\n");
    let body = "fn work(initial:C)->i32{var value=(initial,P{r:R{code:65},n:0});let(ref mut p,_)=value;set_none(p);match value{(C.None(n),ref owner)=>{return n+owner.n},(C.Some(_),_)=>{return 0}}}fn result()->i32{return work(C.Some(0))}";
    let input = source(&format!("{prefix}{body}"));
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    for mutate in [false, true] {
        let mut raw = typed.clone();
        let mut claim = plan.clone();
        let set_some = raw
            .functions
            .iter()
            .find(|f| f.name == "set_some")
            .unwrap()
            .id;
        let f = raw.functions.iter_mut().find(|f| f.name == "work").unwrap();
        let (b, p) = f
            .blocks
            .iter()
            .enumerate()
            .find_map(|(b, block)| {
                block
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, InstructionKind::Call { .. }))
                    .map(|p| (b, p))
            })
            .unwrap();
        let mut call = f.blocks[b].instructions.remove(p);
        let pointer=f.blocks.iter().flat_map(|b|&b.instructions).find(|i|matches!(&i.kind,InstructionKind::BorrowPlaceLocal{mutable:true,path,..}if path==&[PlaceProjection::Field(0)])).unwrap().clone();
        let(b,p)=f.blocks.iter().enumerate().find_map(|(b,block)|block.instructions.iter().position(|i|matches!(&i.kind,InstructionKind::BorrowPlaceLocal{path,..}if path.len()==2)).map(|p|(b,p))).unwrap();
        let mut grant = pointer.clone();
        let id = ValueId(f.values.len() as u32);
        f.values
            .push(f.values[pointer.result.unwrap().0 as usize].clone());
        grant.result = Some(id);
        let InstructionKind::BorrowPlaceLocal { local, .. } = grant.kind else {
            unreachable!()
        };
        claim.functions[f.id.0 as usize].holds.push(ValueHold {
            value: id,
            roots: vec![HoldRoot::Local(local)],
        });
        f.blocks[b].instructions.insert(p, grant);
        if mutate {
            let InstructionKind::Call {
                function,
                arguments,
            } = &mut call.kind
            else {
                unreachable!()
            };
            *function = set_some;
            arguments[0] = id;
            f.blocks[b].instructions.insert(p + 1, call);
        }
        claim.module_fingerprint = raw.fingerprint();
        let analyzed = zil_analyze::analyze(&raw, &limits);
        let verified = zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits);
        if mutate {
            assert!(
                analyzed.is_err(),
                "Analyze accepted a tag proof older than actual mutation"
            );
            let errors = verified.unwrap_err();
            assert!(
                errors.iter().any(|e| e.code == "E-VERIFY-SUM"),
                "{errors:?}"
            );
        } else {
            assert!(analyzed.is_ok(), "{analyzed:?}");
            assert!(verified.is_ok(), "{verified:?}");
        }
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert_eq!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_ok(),
            !mutate
        );
    }
}

#[test]
fn terminal340_actual_mixed_address_observation_budget() {
    for (lane, text) in [
        (
            "mixed",
            include_str!("../../../tests/language-readiness/terminal/340/mixed-address.zil"),
        ),
        (
            "legacy",
            "enum E{Data(i32),None}fn inspect(value:&E)->i32{match *value{E.Data(ref p)=>{return *p},E.None()=>{return 0}}}fn result()->i32{let value=E.Data(42);return inspect(&value)}",
        ),
    ] {
        let input = source(text);
        let ast = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand(&input, &ast).unwrap();
        let target = TargetProfile::default();
        let limits = Limits::default();
        let mut typed = zil_types::check(&core, &target, &limits).unwrap();
        let f = typed
            .functions
            .iter_mut()
            .find(|f| f.name == "inspect")
            .unwrap();
        let (b, p) = f
            .blocks
            .iter()
            .enumerate()
            .find_map(|(b, block)| {
                block
                    .instructions
                    .iter()
                    .position(|i| match &i.kind {
                        InstructionKind::PlaceTagAddress { path, .. } => path.is_empty(),
                        InstructionKind::VariantTagAddress { path, .. } => path.is_empty(),
                        _ => false,
                    })
                    .map(|p| (b, p))
            })
            .unwrap();
        let tag = f.blocks[b].instructions[p].clone();
        for _ in 0..64 {
            let mut read = tag.clone();
            read.result = Some(zil_ir::ValueId(f.values.len() as u32));
            f.values.push(Type::I32);
            f.blocks[b].instructions.insert(p, read);
        }
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
        for channel in 0..3 {
            let run = |work| {
                let bounded = Limits {
                    max_analysis_steps: work,
                    ..Limits::default()
                };
                match channel {
                    0 => zil_analyze::analyze(&typed, &bounded).map(|_| ()),
                    1 => zil_verify::verify_module(typed.clone(), plan.clone(), &target, &bounded)
                        .map(|_| ()),
                    _ => zil_verify::verify_module_bytes(&bytes, &target, &bounded).map(|_| ()),
                }
            };
            let (mut low, mut high) = (1, limits.max_analysis_steps);
            run(high).unwrap();
            while low < high {
                let mid = low + (high - low) / 2;
                match run(mid) {
                    Ok(()) => high = mid,
                    Err(errors) => {
                        assert_eq!(
                            errors[0].category,
                            ResultCategory::ResourceLimit,
                            "{errors:?}"
                        );
                        low = mid + 1;
                    }
                }
            }
            for (work, success) in [(low, true), (low - 1, false), (low, true)] {
                let result = run(work);
                assert_eq!(result.is_ok(), success, "channel {channel}: {result:?}");
                if let Err(errors) = result {
                    assert_eq!(
                        errors[0].category,
                        ResultCategory::ResourceLimit,
                        "{errors:?}"
                    );
                }
            }
            println!(
                "340 actual mixed address observation64 {lane} channel{channel} N={low},N-1 ResourceLimit,repeated fresh agree"
            );
        }
    }
}

#[test]
fn terminal340_actual_mixed_address_returns_and_captures() {
    let prefix = r#"extern "C" fn putchar(code:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}package P{manages r:R;n:i32}enum S{Data((P,i32))}"#;
    for (body, expected) in [
        (
            "fn select(v:&S)->&i32{match *v{S.Data((_,ref n))=>{return n}}}fn result()->i32{let value=S.Data((P{r:R{code:65},n:20},42));let n=select(&value);return *n}",
            "A42\n",
        ),
        (
            "fn select(v:&mut S)->&mut i32{match *v{S.Data((_,ref mut n))=>{return n}}}fn result()->i32{var value=S.Data((P{r:R{code:65},n:20},30));let n=select(&mut value);*n+=12;return *n}",
            "A42\n",
        ),
        (
            "fn select(v:&S)->&i32{match *v{S.Data((_,ref n))=>{return n}}}fn result()->i32{var value=S.Data((P{r:R{code:65},n:20},42));let n=select(&value);let f=()->i32=>*n;return f()}",
            "A42\n",
        ),
        (
            "enum Pair{Data(P,P)}fn inspect(v:&mut Pair)->i32{match *v{Pair.Data(ref mut a,ref mut b)=>{a.n+=b.n;return a.n}}}fn result()->i32{var value=Pair.Data(P{r:R{code:65},n:20},P{r:R{code:66},n:22});return inspect(&mut value)}",
            "AB42\n",
        ),
        (
            "fn index(n:&mut i32)->i64{*n+=1;return 0}fn result()->i32{var n=0;var value=[20];value[index(&mut n)]+=22;return value[0]+n-1}",
            "42\n",
        ),
    ] {
        review329_native_output(&[("main", &format!("{prefix}{body}"))], expected.as_bytes());
    }
    let body = "fn select(v:&S)->&i32{match *v{S.Data((_,ref n))=>{return n}}}fn result()->i32{var value=S.Data((P{r:R{code:65},n:20},42));let n=select(&value);let moved=value;return *n}";
    let r = compile_source(
        &source(&format!("{prefix}{body}")),
        &CompileOptions::default(),
    );
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
}

#[test]
fn terminal340_actual_mixed_address_native_and_raw() {
    use zil_ir::PlaceProjection;
    let text = include_str!("../../../tests/language-readiness/terminal/340/mixed-address.zil");
    review329_native_output(&[("main", text)], b"A42\n");
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    for case in 0..7 {
        let mut raw = typed.clone();
        let mut claim = plan.clone();
        let name = if case == 1 { "peek" } else { "inspect" };
        let f = raw.functions.iter_mut().find(|f| f.name == name).unwrap();
        if case == 5 || case == 6 {
            let sibling = f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find_map(|i| match &i.kind {
                    InstructionKind::BorrowPlaceAddress {
                        path,
                        mutable: true,
                        ..
                    } if path.len() == 2 => Some(path.clone()),
                    _ => None,
                })
                .unwrap();
            let i=f.blocks.iter_mut().flat_map(|b|&mut b.instructions).find(|i|matches!(&i.kind,InstructionKind::BorrowPlaceAddress{path,mutable:true,..}if path.len()==if case==5{1}else{2})).unwrap();
            let InstructionKind::BorrowPlaceAddress { path, .. } = &mut i.kind else {
                unreachable!()
            };
            *path = if case == 5 {
                sibling
            } else {
                vec![
                    PlaceProjection::Variant(zil_ir::VariantProjection {
                        variant: 0,
                        index: 0,
                    }),
                    PlaceProjection::Field(0),
                    PlaceProjection::Field(1),
                ]
            };
        } else if case == 3 {
            let (b, p) = f
                .blocks
                .iter()
                .enumerate()
                .find_map(|(b, block)| {
                    block
                        .instructions
                        .iter()
                        .position(|i| matches!(i.kind, InstructionKind::Call { .. }))
                        .map(|p| (b, p))
                })
                .unwrap();
            let call = f.blocks[b].instructions.remove(p);
            let (b, p) = f
                .blocks
                .iter()
                .enumerate()
                .find_map(|(b, block)| {
                    block
                        .instructions
                        .iter()
                        .position(|i| matches!(i.kind, InstructionKind::BorrowPlaceAddress { .. }))
                        .map(|p| (b, p))
                })
                .unwrap();
            f.blocks[b].instructions.insert(p, call);
        } else if case == 4 {
            let branch = f
                .blocks
                .iter_mut()
                .find(|b| matches!(b.terminator, zil_ir::Terminator::Branch { .. }))
                .unwrap();
            let zil_ir::Terminator::Branch {
                then_block,
                else_block,
                ..
            } = &mut branch.terminator
            else {
                unreachable!()
            };
            *else_block = *then_block;
        } else {
            let i = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::BorrowPlaceAddress { .. }))
                .unwrap();
            let InstructionKind::BorrowPlaceAddress { path, mutable, .. } = &mut i.kind else {
                unreachable!()
            };
            match case {
                0 => {
                    path[0] = PlaceProjection::Variant(zil_ir::VariantProjection {
                        variant: 1,
                        index: 0,
                    })
                }
                1 => {
                    *mutable = true;
                    let Type::Address { mutable, .. } = &mut f.values[i.result.unwrap().0 as usize]
                    else {
                        unreachable!()
                    };
                    *mutable = true;
                }
                2 => path[0] = PlaceProjection::ErrorArm(false),
                _ => unreachable!(),
            }
        }
        claim.module_fingerprint = raw.fingerprint();
        assert!(
            zil_analyze::analyze(&raw, &limits).is_err(),
            "Analyze accepted mixed actual address forgery {case}"
        );
        assert!(
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).is_err(),
            "Verify accepted mixed actual address forgery {case}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "Wire accepted mixed actual address forgery {case}"
        );
    }
}

#[test]
fn terminal340_actual_mixed_address_error_union_epoch() {
    use zil_ir::{BinaryOp, Instruction, PlaceProjection, ValueId};
    let text = r#"extern "C" fn putchar(code:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}package P{manages r:R;n:i32}enum F{Bad}fn make()->F!i32{return 20}fn set_error(p:&mut(F!i32)){*p=F.Bad()}fn work(initial:F!i32)->F!i32{var value=(initial,P{r:R{code:65},n:0},P{r:R{code:66},n:0});let(ref mut p,moved,ref retained)=value;let first=(*p)?;set_error(p);let second=(*p)?;return second+retained.n+moved.n}fn result()->i32{let outcome=work(make());return 42}"#;
    review329_native_output(&[("main", text)], b"AB42\n");
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let mut raw = zil_types::check(&core, &target, &limits).unwrap();
    let f = raw.functions.iter_mut().find(|f| f.name == "work").unwrap();
    let _local = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match &i.kind {
            InstructionKind::BorrowPlaceLocal {
                local,
                path,
                mutable: true,
            } if path == &[PlaceProjection::Field(0)] => Some(*local),
            _ => None,
        })
        .unwrap();
    let base = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match &i.kind {
            InstructionKind::ReadAddress { address }
                if i.result
                    .is_some_and(|v| matches!(f.values[v.0 as usize], Type::ErrorUnion { .. })) =>
            {
                Some(*address)
            }
            _ => None,
        })
        .unwrap();
    let mut tags = Vec::new();
    let old_blocks = f.blocks.len();
    for block in 0..old_blocks {
        let Some(pos) = f.blocks[block]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::ReadErrorUnion { .. }))
        else {
            continue;
        };
        let mut tail = f.blocks[block].instructions.split_off(pos);
        let projection = tail.remove(0);
        let InstructionKind::ReadErrorUnion { is_error, .. } = projection.kind else {
            unreachable!()
        };
        let tag = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        tags.push(tag);
        let expected = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        let condition = ValueId(f.values.len() as u32);
        f.values.push(Type::Bool);
        let span = projection.span;
        f.blocks[block].instructions.extend([
            Instruction {
                result: Some(tag),
                kind: InstructionKind::PlaceTagAddress {
                    address: base,
                    path: vec![],
                },
                span,
            },
            Instruction {
                result: Some(expected),
                kind: InstructionKind::Constant(zil_ir::Constant::I32(i32::from(is_error))),
                span,
            },
            Instruction {
                result: Some(condition),
                kind: InstructionKind::Binary {
                    op: BinaryOp::Equal,
                    left: tag,
                    right: expected,
                },
                span,
            },
        ]);
        let address = ValueId(f.values.len() as u32);
        f.values.push(Type::Address {
            mutable: false,
            inner: Box::new(f.values[projection.result.unwrap().0 as usize].clone()),
        });
        let mut instructions = vec![
            Instruction {
                result: Some(address),
                kind: InstructionKind::BorrowPlaceAddress {
                    address: base,
                    path: vec![PlaceProjection::ErrorArm(is_error)],
                    mutable: false,
                },
                span,
            },
            Instruction {
                result: projection.result,
                kind: InstructionKind::ReadAddress { address },
                span,
            },
        ];
        instructions.extend(tail);
        let trap = zil_ir::BlockId(f.blocks.len() as u32);
        f.blocks.push(zil_ir::BasicBlock {
            id: trap,
            instructions: vec![],
            terminator: zil_ir::Terminator::Trap,
        });
        let next = zil_ir::BlockId(f.blocks.len() as u32);
        let terminator = std::mem::replace(
            &mut f.blocks[block].terminator,
            zil_ir::Terminator::Branch {
                condition,
                then_block: next,
                else_block: trap,
            },
        );
        f.blocks.push(zil_ir::BasicBlock {
            id: next,
            instructions,
            terminator,
        });
    }
    assert_eq!(tags.len(), 4);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    for stale in [true, false] {
        let mut bad = raw.clone();
        let mut claim = plan.clone();
        let f = bad.functions.iter_mut().find(|f| f.name == "work").unwrap();
        if stale {
            for block in &mut f.blocks {
                for i in &mut block.instructions {
                    if let InstructionKind::Binary { left, .. } = &mut i.kind
                        && (*left == tags[2] || *left == tags[3])
                    {
                        *left = tags[1];
                    }
                }
            }
        } else {
            let mut take = f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::TakeTupleField { .. }))
                .unwrap()
                .clone();
            let id = ValueId(f.values.len() as u32);
            f.values
                .push(f.values[take.result.unwrap().0 as usize].clone());
            take.result = Some(id);
            let block = f
                .blocks
                .iter_mut()
                .find(|b| b.instructions.iter().any(|i| i.result == Some(tags[3])))
                .unwrap();
            block.instructions.push(take);
            claim.functions[f.id.0 as usize]
                .holds
                .push(zil_ir::ValueHold {
                    value: id,
                    roots: vec![],
                });
        }
        claim.module_fingerprint = bad.fingerprint();
        assert!(
            zil_analyze::analyze(&bad, &limits).is_err(),
            "Analyze accepted stale ErrorArm or repeated moved sibling"
        );
        let errors =
            zil_verify::verify_module(bad.clone(), claim.clone(), &target, &limits).unwrap_err();
        assert!(
            errors.iter().any(|e| e.code.starts_with("E-VERIFY-SUM")),
            "{errors:?}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&bad, &claim, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
    }
}

#[test]
fn terminal340_copy_error_union_mutation_epoch() {
    use zil_ir::{BinaryOp, Instruction, PlaceProjection, ValueId};
    let text = r#"extern "C" fn putchar(code:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}package P{manages r:R;n:i32}enum F{Bad}fn make()->F!i32{return 20}fn set_error(p:&mut(F!i32)){*p=F.Bad()}fn work(initial:F!i32)->F!i32{var value=(initial,P{r:R{code:65},n:0},P{r:R{code:66},n:0});let first=initial?;let(ref mut p,moved,ref retained)=value;set_error(p);let second=(*p)?;return second+retained.n+moved.n}fn result()->i32{let outcome=work(make());return 42}"#;
    review329_native_output(&[("main", text)], b"AB42\n");
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let mut raw = zil_types::check(&core, &target, &limits).unwrap();
    let f = raw.functions.iter_mut().find(|f| f.name == "work").unwrap();
    let local = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match &i.kind {
            InstructionKind::BorrowPlaceLocal {
                local,
                path,
                mutable: true,
            } if path == &[PlaceProjection::Field(0)] => Some(*local),
            _ => None,
        })
        .unwrap();
    let mut tags = Vec::new();
    let old_blocks = f.blocks.len();
    for block in 0..old_blocks {
        let Some(pos) = f.blocks[block]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::ReadErrorUnion { .. }))
        else {
            continue;
        };
        let mut tail = f.blocks[block].instructions.split_off(pos);
        let projection = tail.remove(0);
        let InstructionKind::ReadErrorUnion { is_error, .. } = projection.kind else {
            unreachable!()
        };
        let tag = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        tags.push(tag);
        let expected = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        let condition = ValueId(f.values.len() as u32);
        f.values.push(Type::Bool);
        let span = projection.span;
        f.blocks[block].instructions.extend([
            Instruction {
                result: Some(tag),
                kind: InstructionKind::PlaceTagLocal {
                    local,
                    path: vec![PlaceProjection::Field(0)],
                },
                span,
            },
            Instruction {
                result: Some(expected),
                kind: InstructionKind::Constant(zil_ir::Constant::I32(i32::from(is_error))),
                span,
            },
            Instruction {
                result: Some(condition),
                kind: InstructionKind::Binary {
                    op: BinaryOp::Equal,
                    left: tag,
                    right: expected,
                },
                span,
            },
        ]);
        let address = ValueId(f.values.len() as u32);
        f.values.push(Type::Address {
            mutable: false,
            inner: Box::new(f.values[projection.result.unwrap().0 as usize].clone()),
        });
        let mut instructions = vec![
            Instruction {
                result: Some(address),
                kind: InstructionKind::BorrowPlaceLocal {
                    local,
                    path: vec![
                        PlaceProjection::Field(0),
                        PlaceProjection::ErrorArm(is_error),
                    ],
                    mutable: false,
                },
                span,
            },
            Instruction {
                result: projection.result,
                kind: InstructionKind::ReadAddress { address },
                span,
            },
        ];
        instructions.extend(tail);
        let trap = zil_ir::BlockId(f.blocks.len() as u32);
        f.blocks.push(zil_ir::BasicBlock {
            id: trap,
            instructions: vec![],
            terminator: zil_ir::Terminator::Trap,
        });
        let next = zil_ir::BlockId(f.blocks.len() as u32);
        let terminator = std::mem::replace(
            &mut f.blocks[block].terminator,
            zil_ir::Terminator::Branch {
                condition,
                then_block: next,
                else_block: trap,
            },
        );
        f.blocks.push(zil_ir::BasicBlock {
            id: next,
            instructions,
            terminator,
        });
    }
    assert_eq!(tags.len(), 4);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    for stale in [true, false] {
        let mut bad = raw.clone();
        let mut claim = plan.clone();
        let f = bad.functions.iter_mut().find(|f| f.name == "work").unwrap();
        if stale {
            for block in &mut f.blocks {
                for i in &mut block.instructions {
                    if let InstructionKind::Binary { left, .. } = &mut i.kind
                        && (*left == tags[2] || *left == tags[3])
                    {
                        *left = tags[1];
                    }
                }
            }
        } else {
            let mut take = f
                .blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::TakeTupleField { .. }))
                .unwrap()
                .clone();
            let id = ValueId(f.values.len() as u32);
            f.values
                .push(f.values[take.result.unwrap().0 as usize].clone());
            take.result = Some(id);
            let block = f
                .blocks
                .iter_mut()
                .find(|b| b.instructions.iter().any(|i| i.result == Some(tags[3])))
                .unwrap();
            block.instructions.push(take);
            claim.functions[f.id.0 as usize]
                .holds
                .push(zil_ir::ValueHold {
                    value: id,
                    roots: vec![],
                });
        }
        claim.module_fingerprint = bad.fingerprint();
        assert!(
            zil_analyze::analyze(&bad, &limits).is_err(),
            "Analyze accepted stale ErrorArm or repeated moved sibling"
        );
        let errors =
            zil_verify::verify_module(bad.clone(), claim.clone(), &target, &limits).unwrap_err();
        assert!(
            errors.iter().any(|e| e.code == "E-VERIFY-SUM"),
            "{errors:?}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&bad, &claim, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
    }
}

#[test]
fn terminal340_actual_mixed_address_budget() {
    use zil_driver::CompilationSession;
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/340/mixed-address.zil"
    ));
    let options = |work| CompileOptions {
        limits: Limits {
            max_analysis_steps: work,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
    let report = compile_source(&input, &options(high));
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    while low < high {
        let mid = low + (high - low) / 2;
        let report = compile_source(&input, &options(mid));
        match report.status {
            ResultCategory::Success => high = mid,
            ResultCategory::ResourceLimit => low = mid + 1,
            _ => panic!("{}", report.to_json()),
        }
    }
    let mut warm = CompilationSession::default();
    for (work, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(work)),
            CompilationSession::default().compile_source(&input, &options(work)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!("340 actual mixed address full-request N={low}, N-1 rejects, fresh/warm agree");
}
#[test]
fn terminal340_selective_long_prefix_budget() {
    use zil_driver::CompilationSession;
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/340/mixed-long-prefix.zil"
    ));
    let options = |work| CompileOptions {
        limits: Limits {
            max_analysis_steps: work,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
    let report = compile_source(&input, &options(high));
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    while low < high {
        let mid = low + (high - low) / 2;
        let report = compile_source(&input, &options(mid));
        match report.status {
            ResultCategory::Success => high = mid,
            ResultCategory::ResourceLimit => low = mid + 1,
            _ => panic!("{}", report.to_json()),
        }
    }
    let mut warm = CompilationSession::default();
    for (work, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(work)),
            CompilationSession::default().compile_source(&input, &options(work)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!("340 selective long-prefix full-request N={low}, N-1 rejects, fresh/warm agree");
}

fn review329_project(files: &[(&str, &str)]) -> zil_driver::ProjectSources {
    zil_driver::ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(index, (name, text))| {
                SourceUnit::new(
                    FileId(index as u32),
                    ModuleId(index as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(index, (name, _))| (vec![(*name).to_owned()], ModuleId(index as u32)))
            .collect(),
    }
}

fn review329_native(files: &[(&str, &str)], expected: i64) {
    review329_native_output(files, format!("{expected}\n").as_bytes())
}
fn review329_native_output(files: &[(&str, &str)], expected: &[u8]) {
    review329_native_entry_output(files, "result", expected)
}
fn review329_native_entry_output(files: &[(&str, &str)], entry: &str, expected: &[u8]) {
    use zil_driver::{NativeBackend, Optimization};
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let directory = Temp::new();
            let report = zil_driver::compile_project(
                &review329_project(files),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    entry: Some(entry.into()),
                    output: Some(directory.0.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let codegen = report.native_codegen.as_ref().unwrap();
            assert_eq!(codegen.backend, backend);
            assert!(codegen.fallback.is_none());
            if backend == NativeBackend::LlvmApi {
                assert!(!codegen.object_reports.is_empty());
                assert!(
                    codegen
                        .object_reports
                        .iter()
                        .all(|object| object.contains("\"input_kind\":\"builder\""))
                );
            }
            let observed = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(
                observed.status.success(),
                "{backend:?}/{optimization:?}: {observed:?}"
            );
            assert_eq!(observed.stdout, expected);
            assert!(observed.stderr.is_empty());
        }
    }
}

#[test]
fn review329_field_evaluation_binding_labels_and_i64_context_are_native() {
    for (text, expected) in [
        (
            "enum Pair{Item{a:i32,b:i32}}fn mark(trace:&mut i32,n:i32)->i32{*trace=*trace*10+n;return n}fn result()->i32{var trace=0;let p=Pair.Item{b:mark(&mut trace,2),a:mark(&mut trace,1)};match p{Pair.Item{b:right,a:left}=>{return trace*100+left*10+right}}}",
            2112,
        ),
        (
            "enum Event<T:Copy>{Data{code:T,tail:i32}}fn result()->i64{let e:Event<i64>=Event.Data{tail:2,code:1};match e{Event.Data{tail:y,code:x}=>{return x+i64(y)}}}",
            3,
        ),
        (
            "enum Color{N{value:i32}}enum Packet{P{left:Color,right:i32}}fn result()->i32{let value=100;let p=Packet.P{right:2,left:Color.N{value:7}};match p{Packet.P{right:left,left:Color.N{value:right}}=>{return right*10+left+value}}}",
            172,
        ),
        (
            "enum E{V{x:i32}}fn result()->i32{let x=7;let v=E.V{x};match v{E.V{x:n}=>{return n}}}",
            7,
        ),
        (
            "enum E{V{},U}fn result()->i32{let v=E.V{};match v{E.V{}=>{return 7},E.U()=>{return 0}}}",
            7,
        ),
    ] {
        review329_native(&[("main", text)], expected);
    }
}

#[test]
fn review329_field_sets_forms_and_uninstantiated_copy_aliases_reject() {
    for text in [
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V{x:1};return 0}",
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V{x:1,y:2,z:3};return 0}",
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V{x:1,x:2};return 0}",
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V(1,2);return 0}",
        "enum E{V(i32,i32)}fn result()->i32{let e=E.V{x:1,y:2};return 0}",
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V{x:1,y:2};match e{E.V{x:n,y:n}=>{return n}}}",
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V{x:1,y:2};match e{E.V{x:n}=>{return n}}}",
        "enum E{V{x:i32,y:i32}}fn result()->i32{let e=E.V{x:1,y:2};match e{E.V(x,y)=>{return x+y}}}",
        "enum E{V{},U}fn result()->i32{let e=E.V();return 0}",
        "enum E{V{},U}fn result()->i32{let e=E.U{};return 0}",
        "enum E{V{x:i32}}fn result()->i32{let e=E.V{x:1};match e{E.V{x}=>{return x}}}",
        "type Bad=Token<&i32>;enum Token<T:Copy>{Data{n:i32}}fn result()->i32{return 0}",
        "enum Token<T:Copy>{Data{n:i32}}type Bad=Token<&i32>;fn result()->i32{return 0}",
        "type Borrow=&i32;type Bad=Token<Borrow>;enum Token<T:Copy>{Data{n:i32}}fn result()->i32{return 0}",
        "enum E<T:Copy>{V{x:T}}fn bad<T:Copy>(x:T)->i64{let e:E<i64>=E.V{x:x};return 0}fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
        assert!(!report.diagnostics.is_empty());
        assert!(!report.artifacts.contains_key("executable"));
    }
    let bad = "enum E<T:Copy>{V{x:T}}fn bad<T:Copy>(x:T)->E<T>{return E.V{x:7}}fn result()->i32{return 0}";
    let report = compile_source(&source(bad), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand);
    assert!(report.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED"));
    let report = compile_source(
        &source(
            "enum E<T:Copy>{V{x:T}}fn good<T:Copy>(x:T)->E<T>{return E.V{x:x}}fn result()->i32{return 0}",
        ),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn review329_module_nominals_and_visibility_remain_distinct() {
    let files = [
        (
            "main",
            "import left;import right;fn result()->i32{return left.read(left.make())+right.read(right.make())}",
        ),
        (
            "left",
            "pub enum E{V{x:i32}}pub fn make()->E{return E.V{x:7}}pub fn read(e:E)->i32{match e{E.V{x:n}=>{return n}}}",
        ),
        (
            "right",
            "pub enum E{V{x:i32}}pub fn make()->E{return E.V{x:9}}pub fn read(e:E)->i32{match e{E.V{x:n}=>{return n}}}",
        ),
    ];
    review329_native(&files, 16);
    let mut confused = files;
    confused[0].1 = "import left;import right;fn result()->i32{return right.read(left.make())}";
    let report =
        zil_driver::compile_project(&review329_project(&confused), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    let report = zil_driver::compile_project(
        &review329_project(&[
            (
                "main",
                "import data;fn result()->i32{let x=data.Hidden.V{x:1};return 0}",
            ),
            ("data", "enum Hidden{V{x:i32}}"),
        ]),
        &CompileOptions::default(),
    );
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
            .any(|d| d.code == "E-MODULE-PRIVATE")
    );
}

#[test]
fn review329_ct_quote_keeps_labels_separate_from_hygienic_bindings() {
    review329_native(
        &[(
            "main",
            "enum E{V{x:i32}}fn generated()->Syntax<StmtList>{return quote<StmtList>{let x=7;let e=E.V{x};match e{E.V{x:y}=>{return y}}}}fn result()->i32{let x=100;#emit generated()}",
        )],
        7,
    );
    review329_native(
        &[(
            "main",
            "enum E{V{Type:i32,left:i32}}fn generated()->Syntax<StmtList>{return quote<StmtList>{match (E.V{Type:1,left:2}){E.V{left:Type,Type:left}=>{return left*10+Type}}}}fn result()->i32{let Type=100;#emit generated()}",
        )],
        12,
    );
    review329_native(
        &[(
            "main",
            "enum E{V{left:i32,right:i32}}fn generated()->Syntax<StmtList>{return quote<StmtList>{match (E.V{left:1,right:2}){E.V{right:left,left:right}=>{return right*10+left}}}}fn result()->i32{let left=100;#emit generated()}",
        )],
        12,
    );
}

#[test]
fn review329_shared_record_pun_capture_neighbor() {
    review329_native(
        &[(
            "main",
            "struct P{x:i32}fn generated()->Syntax<StmtList>{return quote<StmtList>{let x=7;let p=P{x};return p.x}}fn result()->i32{let x=100;#emit generated()}",
        )],
        7,
    );
}

#[test]
fn review329_unused_variant_attributes_do_not_disappear() {
    let report = compile_source(
        &source("enum E{#[cfg(true)] V{x:i32}}fn result()->i32{return 0}"),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    assert!(report.diagnostics.iter().any(|d| d.code == "E-UNSUPPORTED"));
}

#[test]
fn review329_symbolic_primitive_spelling_patterns_do_not_gain_scalar_permissions() {
    for text in [
        "enum E<T:Copy>{V{x:T}}fn keep<i64:Copy>(e:E<i64>)->i32{match e{E.V{x:n}=>{return 0}}}fn result()->i32{return 0}",
        "fn scalar()->i64{return 1}fn keep<i64:Copy>(x:i64)->i32{let n=scalar();match n{1=>{return 1},_=>{return 0}}}fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    let mut accepted = Vec::new();
    for text in [
        "enum E<T:Copy>{V{x:T}}fn bad<i64:Copy>(e:E<i64>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        "enum E<T:Copy>{V{x:T}}fn bad<bool:Copy>(e:E<bool>)->i32{match e{E.V{x:true}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        eprintln!("review329 symbolic pattern {text}: {}", report.to_json());
        if report.status == ResultCategory::Success {
            accepted.push(text);
        }
    }
    assert!(
        accepted.is_empty(),
        "symbolic primitive-spelling patterns accepted: {accepted:?}"
    );
}

#[test]
fn record_enum329_quote_shorthand_preserves_value_hygiene() {
    // Reviewer 6c6481d08's legal-category source; author regression, not review.
    native(
        "enum E{V{x:i32}}fn generated()->Syntax<StmtList>{return quote<StmtList>{let x=7;let e=E.V{x};match e{E.V{x:y}=>{return y}}}}fn result()->i32{let x=100;#emit generated()}",
        Some(7),
    );
    native(
        "struct P{x:i32}fn generated()->Syntax<StmtList>{return quote<StmtList>{let x=7;let p=P{x};return p.x}}fn result()->i32{let x=100;#emit generated()}",
        Some(7),
    );
}

#[test]
fn record_enum329_unused_variant_attributes_remain_explicit() {
    let report = compile_source(
        &source("enum E<T>{#[cfg(false)] Row{value:T}}fn main(){}"),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, "E-UNSUPPORTED");
}

#[test]
fn record_enum329_named_source_order_and_exact_values() {
    native(
        r#"
enum Message{Data{left:i64,right:i64,flag:bool},Empty{}}
fn mark(log:&mut i64,digit:i64)->i64{*log=*log*10+digit;return digit}
fn result()->i64{
 var log:i64=0;
 let message=Message.Data{right:mark(&mut log,2),flag:true,left:mark(&mut log,1)};
 if log!=21{return -1}
 match message{
  Message.Data{flag:true,left:left,right:right}=>{if left!=1{return -2}if right!=2{return -3}return log*100+left*10+right},
  Message.Data{left:_,right:_,flag:false}=>{return -4},
  Message.Empty{}=>{return -5}
 }
}
"#,
        Some(2112),
    );
    native(
        r#"
enum E{Data{left:i64,right:i64},Empty{},Unit,Tuple(i64)}
fn read(e:E)->i64{match e{E.Data{right:r,left:l}=>{return l*100+r},E.Empty{}=>{return 1},E.Unit()=>{return 2},E.Tuple(n)=>{return n}}}
fn result()->i64{let left:i64=7;let right:i64=9;return read(E.Data{right,left})+read(E.Empty{})+read(E.Unit)+read(E.Tuple(3))}
"#,
        Some(715),
    );
}

#[test]
fn record_enum329_nested_generic_copy_and_phantom_identity() {
    native(
        r#"
struct P{n:i64}
enum E{Named{p:P},Old(P)}
fn result()->i64{let a=E.Named{p:{n:7}};let b=E.Old(P{n:9});match a{E.Named{p:p}=>{match b{E.Old(q)=>{return p.n*10+q.n},E.Named{p:_}=>{return -1}}},E.Old(_)=>{return -2}}}
"#,
        Some(79),
    );
    native(
        r#"
struct Box<T:Copy>{value:T}
enum Message<T:Copy,P:Copy>{Data{boxed:Box<T>,ok:bool}}
fn make<T:Copy,P:Copy>(value:T)->Message<T,P>{return Message::<T,P>.Data{ok:true,boxed:Box::<T>{value:value}}}
fn read<T:Copy,P:Copy>(message:Message<T,P>)->T{match message{Message.Data{ok:_,boxed:box}=>{return box.value}}}
fn result()->i64{let a=make::<i64,bool>(7);let b=make::<i64,i32>(9);return read::<i64,bool>(a)*10+read::<i64,i32>(b)}
"#,
        Some(79),
    );
    native(
        r#"
enum Inner{Only{x:i64,flag:bool}}
enum Outer{Wrap{inner:Inner,pair:(i64,bool)},Other}
fn result()->i64{let value=Outer.Wrap{pair:(7,true),inner:Inner.Only{flag:false,x:5}};match value{Outer.Wrap{pair:(a,true),inner:Inner.Only{x:b,flag:false}}=>{let read=()=>a*10+b;return read()},Outer.Wrap{inner:_,pair:_}=>{return -1},Outer.Other()=>{return -2}}}
"#,
        Some(75),
    );
}

#[test]
fn record_enum329_labels_shapes_and_unused_definitions_reject() {
    for text in [
        "enum E{V{x:i32,y:bool}}fn main(){let x=E.V{x:1};}",
        "enum E{V{x:i32,y:bool}}fn main(){let x=E.V{x:1,y:true,z:2};}",
        "enum E{V{x:i32,y:bool}}fn main(){let x=E.V{x:1,x:2,y:true};}",
        "enum E{V{x:i32,y:bool}}fn main(){let x=E.V{x:true,y:1};}",
        "enum E{V{x:i32,y:bool}}fn main(){let x=E.V(1,true);}",
        "enum E{V(i32,bool)}fn main(){let x=E.V{x:1,y:true};}",
        "enum E{V{x:i32,y:bool}}fn unused(e:E){match e{E.V{x:x}=>{}}}fn main(){}",
        "enum E{V{x:i32,y:bool}}fn unused(e:E){match e{E.V{x:x,y:y,z:_}=>{}}}fn main(){}",
        "enum E{V{x:i32,y:bool}}fn unused(e:E){match e{E.V{x:a,x:b,y:_}=>{}}}fn main(){}",
        "enum E{V{x:i32,y:bool}}fn unused(e:E){match e{E.V{x:n,y:n}=>{}}}fn main(){}",
        "enum E{V{x:i32,y:bool}}fn unused(e:E){match e{E.V(x,y)=>{}}}fn main(){}",
        "enum E{V(i32,bool)}fn unused(e:E){match e{E.V{x:x,y:y}=>{}}}fn main(){}",
        "enum A{V{x:i32}}enum B{V{x:i32}}fn unused(e:A){match e{B.V{x:_}=>{}}}fn main(){}",
        "enum E<T:Copy>{V{x:T}}fn unused<T>(x:T){let value=E.V{x:x};}fn main(){}",
        "enum E<T:Copy,P:Copy>{V{x:T}}fn unused<T:Copy,P>(e:E<T,P>){}fn main(){}",
        "enum E<T:Copy,P:Copy>{V{x:T}}fn only(e:E<i32,bool>){}fn main(){let e:E<i32,i64>=E.V{x:7};only(e)}",
        "enum E<T:Copy,P:Copy>{V{x:T}}type Bad=E<i32,ByteBuffer>;fn main(){}",
        "enum E<T:Copy>{V{x:T}}type A<T>=E<T>;fn unused<T>(x:T)->A<T>{return A::<T>.V{x:x}}fn main(){}",
        "struct Box<T:Copy>{value:T}enum E<T>{V{value:Box<T>}}fn main(){}",
        "enum E{V{x:i32,x:i32}}fn main(){}",
        "enum E{V{x:&i32}}fn main(){}",
        "enum E{V{x:ByteBuffer}}fn main(){}",
        "enum E{V{x:E}}fn main(){}",
        "enum E<T>{V{x:T}}fn unused<T>(x:T){let value=E.V{x:x};let again=x;}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
        assert!(!report.diagnostics.is_empty(), "{text}");
        assert!(!report.artifacts.contains_key("executable"));
    }
    for text in [
        "enum E<T>{R{r:T}}fn bad<T:Copy>()->E<T>{return E::<T>.R{r:7}}fn main(){}",
        "enum E{V{x:i32=7}}fn main(){}",
        "enum E{V{pub x:i32}}fn main(){}",
        "resource R{x:i32}enum E{V{manages owner:R}}fn main(){}",
        "resource R{x:i32}enum E{V{x:i32},Other(manages R)}fn main(){}",
        "enum E{V{x:i32}}fn unused(e:E){if let E.V{x:x}=e{}}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{text}: {}",
            report.to_json()
        );
        assert!(!report.diagnostics.is_empty());
    }
    for text in [
        "enum E<T:Copy>{V{x:T}}fn unused<T:Copy>(x:T){let value=E.V{x:x};}fn main(){}",
        "enum E<T>{R{r:T}}fn good<T:Copy>(r:T)->E<T>{return E::<T>.R{r:r}}fn main(){}",
        "enum E<T:Copy>{V{x:T}}type A<T>=E<T>;fn unused<T:Copy>(x:T)->A<T>{return A::<T>.V{x:x}}fn main(){}",
        "enum E<T:Copy,P:Copy>{V{x:T}}fn unused<T:Copy,P:Copy>(e:E<T,P>){}fn main(){}",
        "struct Box<T:Copy>{value:T}enum E<T:Copy>{V{value:Box<T>}}fn main(){}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
    }
}

#[test]
fn record_enum329_public_module_alias_and_field_label_dependency() {
    use zil_driver::{CompilationSession, ProjectSources};
    let main = "import model;type Alias=model.Message<i64>;fn main()->i32{let e:Alias=model.Message.Data{value:7,flag:true};match e{Alias.Data{flag:true,value:n}=>{if n!=7{return 1}return 0},Alias.Data{flag:false,value:_}=>{return 2}}}";
    let model = "pub enum Message<T:Copy>{Data{flag:bool,value:T}}";
    let project = |provider: &str| ProjectSources {
        root: ModuleId(0),
        modules: vec![
            source(main),
            SourceUnit::new(FileId(1), ModuleId(1), "model.zil", provider),
        ],
        paths: std::collections::BTreeMap::from([(vec!["model".into()], ModuleId(1))]),
    };
    for optimization in [zil_driver::Optimization::O0, zil_driver::Optimization::O2] {
        let out = Temp::new();
        let report = zil_driver::compile_project(
            &project(model),
            &CompileOptions {
                stop_after: Stage::Build,
                output: Some(out.0.join("program")),
                optimization,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let result = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(0), "{result:?}");
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
    let mut session = CompilationSession::default();
    let options = CompileOptions::default();
    assert_eq!(
        session.compile_project(&project(model), &options).status,
        ResultCategory::Success
    );
    // Same payload types/layout, different source member identity. Neither a
    // warm source cache nor positional Enum IR may retain the removed label.
    let renamed = model.replace("value:T", "renamed:T");
    let fresh = CompilationSession::default().compile_project(&project(&renamed), &options);
    assert_eq!(
        fresh.status,
        ResultCategory::SafeReject,
        "{}",
        fresh.to_json()
    );
    let warm = session.compile_project(&project(&renamed), &options);
    assert_eq!(
        warm.status,
        ResultCategory::SafeReject,
        "{}",
        warm.to_json()
    );
    assert_eq!(session.active_query_count(), 0);
    assert_eq!(
        session.compile_project(&project(model), &options).status,
        ResultCategory::Success
    );
}

#[test]
fn record_enum329_request_budgets_are_exact_cold_warm_and_fresh() {
    use zil_driver::CompilationSession;
    let input = source(
        "enum E<T:Copy,P:Copy>{V{value:T,flag:bool}}fn make<T:Copy,P:Copy>(x:T)->E<T,P>{return E::<T,P>.V{flag:true,value:x}}fn read<T:Copy,P:Copy>(e:E<T,P>)->T{match e{E.V{flag:_,value:n}=>{return n}}}fn main()->i32{return read::<i32,bool>(make::<i32,bool>(7))-7}",
    );
    for ledger in ["work", "nodes", "instances"] {
        let options = |n| {
            let mut options = CompileOptions::default();
            match ledger {
                "work" => options.limits.max_analysis_steps = n,
                "nodes" => options.limits.max_nodes = n,
                _ => options.limits.max_functions = n,
            }
            options
        };
        let mut low = 1;
        let mut high = match ledger {
            "work" => Limits::default().max_analysis_steps,
            "nodes" => Limits::default().max_nodes,
            _ => Limits::default().max_functions,
        };
        while low < high {
            let n = low + (high - low) / 2;
            let report = compile_source(&input, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut warm = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            assert_eq!(warm.compile_source(&input, &options(n)).status, expected);
            assert_eq!(
                CompilationSession::default()
                    .compile_source(&input, &options(n))
                    .status,
                expected
            );
            assert_eq!(warm.active_query_count(), 0);
        }
        println!("329 full request {ledger} N={low}; N-1 ResourceLimit, cold/warm/fresh agree");
    }
}

#[test]
fn primitive_binder329_request_budget_is_shared_cold_warm_and_fresh() {
    use zil_driver::CompilationSession;
    let input = source(
        "enum E<T:Copy>{V{x:T}}fn identity<i64:Copy>(x:i64)->i64{return x}fn read<i64:Copy>(e:E<i64>)->i64{match e{E.V{x:n}=>{return n}}}fn main()->i32{return read::<i32>(E::<i32>.V{x:identity::<i32>(7)})-7}",
    );
    for ledger in ["work", "nodes"] {
        let options = |n| {
            let mut options = CompileOptions::default();
            if ledger == "work" {
                options.limits.max_analysis_steps = n;
            } else {
                options.limits.max_nodes = n;
            }
            options
        };
        let mut low = 1;
        let mut high = if ledger == "work" {
            Limits::default().max_analysis_steps
        } else {
            Limits::default().max_nodes
        };
        while low < high {
            let n = low + (high - low) / 2;
            let report = compile_source(&input, &options(n));
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        assert!(low > 1);
        assert!(
            low < if ledger == "work" {
                Limits::default().max_analysis_steps
            } else {
                Limits::default().max_nodes
            }
        );
        let mut warm = CompilationSession::default();
        for (n, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            let report = warm.compile_source(&input, &options(n));
            assert_eq!(report.status, expected, "{}", report.to_json());
            if expected == ResultCategory::ResourceLimit {
                assert!(!report.diagnostics.is_empty());
            }
            let fresh = CompilationSession::default().compile_source(&input, &options(n));
            assert_eq!(fresh.status, expected, "{}", fresh.to_json());
            assert_eq!(warm.active_query_count(), 0);
        }
        println!(
            "329 primitive binder full request {ledger} N={low}; N-1 ResourceLimit, cold/warm/fresh agree"
        );
    }
}

#[test]
fn record_enum329_raw_and_wire_preserve_typed_enum_integrity() {
    let input = source(
        "enum E{V{number:i64,flag:bool},U}fn result()->i64{let e=E.V{flag:true,number:7};match e{E.V{flag:_,number:n}=>{return n},E.U()=>{return 0}}}",
    );
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&wire, &target, &limits)
            .unwrap()
            .module(),
        &typed
    );
    for attack in 0..3 {
        let mut damaged = typed.clone();
        let function = &mut damaged.functions[0];
        if attack < 2 {
            let op = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::MakeVariant { .. }))
                .unwrap();
            let InstructionKind::MakeVariant { variant, fields } = &mut op.kind else {
                unreachable!()
            };
            if attack == 0 {
                fields.swap(0, 1)
            } else {
                *variant = 99
            }
        } else {
            let op = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::ReadVariant { .. }))
                .unwrap();
            let InstructionKind::ReadVariant { index, .. } = &mut op.kind else {
                unreachable!()
            };
            *index = 99;
        }
        let mut claims = plan.clone();
        claims.module_fingerprint = damaged.fingerprint();
        let raw = zil_verify::verify_module(damaged.clone(), claims.clone(), &target, &limits)
            .unwrap_err();
        let bytes = zil_ir::wire::encode_module_candidate(&damaged, &claims, &limits).unwrap();
        let transported = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err();
        for errors in [raw, transported] {
            assert!(!errors.is_empty());
            assert_eq!(errors[0].phase, Phase::Verify);
            assert!(errors[0].code.starts_with("E-VERIFY-"));
        }
    }
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-aggregate-{}-{}",
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
    SourceUnit::new(FileId(0), ModuleId(0), "aggregate.zil", text)
}
fn native(text: &str, expected: Option<i64>) -> String {
    native_entry(text, "result", expected)
}
fn native_entry(text: &str, entry: &str, expected: Option<i64>) -> String {
    let out = Temp::new();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some(entry.into()),
            output: Some(out.0.join("plain")),
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
    for optimized in [false, true] {
        let executable = if optimized {
            let p = out.0.join("optimized");
            let built = Command::new("clang")
                .args(["-x", "ir", "-O2"])
                .arg(&report.artifacts["llvm"])
                .arg("-o")
                .arg(&p)
                .output()
                .unwrap();
            assert!(
                built.status.success(),
                "{}",
                String::from_utf8_lossy(&built.stderr)
            );
            p
        } else {
            report.artifacts["executable"].clone()
        };
        let observed = Command::new(executable).output().unwrap();
        match expected {
            Some(value) => {
                assert!(observed.status.success(), "{observed:?}");
                assert_eq!(observed.stdout, format!("{value}\n").as_bytes());
                assert!(observed.stderr.is_empty());
            }
            None => assert!(!observed.status.success(), "out-of-bounds access must trap"),
        }
    }
    llvm
}
fn reject(text: &str, code: &str) {
    let r = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.diagnostics[0].code, code, "{}", r.to_json());
    assert!(!r.artifacts.contains_key("executable"));
}

fn large_copy_program(body: &str) -> String {
    let mut ty = "[u8;1]".to_owned();
    let mut initializers = "let a0=[3u8];".to_owned();
    for level in 1..=12 {
        ty = format!("[{ty};2]");
        initializers.push_str(&format!("let a{level}=[a{},a{}];", level - 1, level - 1));
    }
    let first = "[0]".repeat(13);
    format!(
        "type Big={ty}; fn touch(value:&mut Big)->i32{{(*value){first}+=1u8;return 1}} fn inspect(value:Big,marker:i32)->i32{{return i32(value{first})+marker}} fn result()->i32{{{initializers}{body}}}"
    )
}

#[test]
fn large_copy_snapshot_order_nested_index_and_loop_join() {
    let first = "[0]".repeat(13);
    let rest = "[0]".repeat(12);
    let text = large_copy_program(&format!(
        "var original=a12;let saved=original;var pair=(original,touch(&mut original));var index=1;pair.0[index]{rest}=4u8;let before=pair.0;pair.0[0]{rest}=8u8;let observed=inspect(original,touch(&mut original));var branch=original;var i=0;while i<2{{if i==0{{branch=saved}}else{{branch=before}}i+=1}}return i32(saved{first})*10000+i32(before{first})*1000+i32(pair.0{first})*100+i32(branch[index]{rest})*10+observed+i32(original{first});"
    ));
    let llvm = native(&text, Some(33850));
    assert!(llvm.contains("copy.value."));
    assert!(llvm.contains("llvm.memcpy.p0.p0.i64"));
    assert!(!llvm.contains("@malloc"));
}

#[test]
fn large_copy_read_address_snapshot_and_bounds_trap() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "var original=a12;let p=&mut original;let before=*p;(*p){first}=7u8;let after=*p;return i32(before{first})*10+i32(after{first});"
    ));
    native(&text, Some(37));
    let text = large_copy_program(&format!(
        "var index=2;return i32(a12[index]{});",
        "[0]".repeat(12)
    ));
    native(&text, None);
}

#[test]
fn large_copy_record_unit_padding_and_tail_snapshot() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "var record=Envelope{{unit:(),flag:true,body:a12,tail:9}};let before=record;record.body{first}=7u8;record.tail=11;record.flag=false;if before.flag{{return i32(before.body{first})*1000+i32(record.body{first})*100+i32(before.tail)*10+i32(record.tail)}}return 0;"
    ));
    let text = format!("struct Envelope{{unit:();flag:bool;body:Big;tail:i64}}{text}");
    native(&text, Some(3801));
}

#[test]
fn large_copy_current_codegen_budget_is_exact_and_fresh() {
    let src = source(include_str!(
        "../../../tests/language-readiness/codegen-pending/large-array-a18.zil"
    ));
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let candidate = zil_analyze::analyze(&typed, &defaults).unwrap();
    let emit = |steps| {
        let limits = Limits {
            max_analysis_steps: steps,
            ..defaults.clone()
        };
        let verified =
            zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits)?;
        let lir = zil_lower::lower(&verified)?;
        zil_backend_llvm::emit(&verified, &lir)
    };
    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    assert!(emit(high).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        if emit(middle).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    let failed = emit(low - 1).unwrap_err();
    assert_eq!(failed[0].category, ResultCategory::ResourceLimit);
    assert_eq!(failed[0].phase, Phase::Codegen);
    assert!(emit(low).is_ok());
    assert!(emit(low).is_ok());
    eprintln!("324 original a18 Codegen exact work limit: {low}");
}

#[test]
fn large_copy_a18_native_all_backends() {
    // Isolate LLVM failures and cap each compiler process. This remains one
    // existing Cargo integration target and preserves the original input bytes.
    if let Ok(profile) = std::env::var("ZIL_324_LARGE_COPY_CHILD") {
        use zil_driver::{NativeBackend, Optimization};
        let (backend, optimization) = match profile.as_str() {
            "clang-O0" => (NativeBackend::Clang, Optimization::O0),
            "clang-O2" => (NativeBackend::Clang, Optimization::O2),
            "llvm-api-O0" => (NativeBackend::LlvmApi, Optimization::O0),
            "llvm-api-O2" => (NativeBackend::LlvmApi, Optimization::O2),
            _ => panic!("unknown child profile"),
        };
        let out = Temp::new();
        let report = compile_source(
            &source(include_str!(
                "../../../tests/language-readiness/codegen-pending/large-array-a18.zil"
            )),
            &CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                cancellation: zil_driver::CancellationToken::with_timeout(
                    std::time::Duration::from_secs(45),
                ),
                output: Some(out.0.join("program")),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, backend);
        assert!(codegen.fallback.is_none());
        if backend == NativeBackend::LlvmApi {
            let pipeline = format!("\"pipeline\":\"default<{optimization:?}>\"");
            assert!(!codegen.object_reports.is_empty());
            assert!(codegen.object_reports.iter().all(|r| r.contains(&pipeline)));
        } else {
            let flag = if optimization == Optimization::O0 {
                "-O0"
            } else {
                "-O2"
            };
            assert!(
                report
                    .commands
                    .iter()
                    .any(|command| command.iter().any(|arg| arg == flag))
            );
        }
        let actual = Command::new("timeout")
            .args(["-s", "KILL", "5s"])
            .arg(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(actual.status.code(), Some(0), "{actual:?}");
        assert!(actual.stdout.is_empty());
        assert!(actual.stderr.is_empty());
        return;
    }
    for backend in ["clang", "llvm-api"] {
        if backend == "llvm-api" && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in ["O0", "O2"] {
            let actual = Command::new("timeout")
                .args([
                    "-s",
                    "KILL",
                    "55s",
                    "bash",
                    "-c",
                    "ulimit -c 0; ulimit -t 45; ulimit -v 2097152; exec \"$@\"",
                    "--",
                ])
                .arg(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "large_copy_a18_native_all_backends",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(
                    "ZIL_324_LARGE_COPY_CHILD",
                    format!("{backend}-{optimization}"),
                )
                .output()
                .unwrap();
            assert!(
                actual.status.success(),
                "{backend}-{optimization}: {} {}",
                String::from_utf8_lossy(&actual.stdout),
                String::from_utf8_lossy(&actual.stderr)
            );
            eprintln!(
                "324 a18 {backend}-{optimization}: backend matched, requested pipeline, fallback=null, native_exit=0"
            );
        }
    }
}
#[test]
fn large_copy_a18_by_value_abi_native_all_backends() {
    // Isolate LLVM failures and cap each compiler process. This remains one
    // existing Cargo integration target and preserves the original input bytes.
    if let Ok(profile) = std::env::var("ZIL_333_LARGE_COPY_CHILD") {
        use zil_driver::{NativeBackend, Optimization};
        let (backend, optimization) = match profile.as_str() {
            "clang-O0" => (NativeBackend::Clang, Optimization::O0),
            "clang-O2" => (NativeBackend::Clang, Optimization::O2),
            "llvm-api-O0" => (NativeBackend::LlvmApi, Optimization::O0),
            "llvm-api-O2" => (NativeBackend::LlvmApi, Optimization::O2),
            _ => panic!("unknown child profile"),
        };
        let out = Temp::new();
        let case = std::env::var("ZIL_333_CASE").unwrap();
        let input = match case.as_str() {
            "argument" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-array-a18-by-value.zil"
            ),
            "return" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-array-a18-return.zil"
            ),
            "pointer" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-array-a18-function-pointer.zil"
            ),
            _ => panic!("unknown ABI case"),
        };
        let report = compile_source(
            &source(input),
            &CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                cancellation: zil_driver::CancellationToken::with_timeout(
                    std::time::Duration::from_secs(45),
                ),
                output: Some(out.0.join("program")),
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
        assert!(llvm.contains(".lc1("), "versioned definition/call absent");
        assert!(llvm.contains("@zil.copy.bytes"));
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, backend);
        assert!(codegen.fallback.is_none());
        if backend == NativeBackend::LlvmApi {
            let pipeline = format!("\"pipeline\":\"default<{optimization:?}>\"");
            assert!(!codegen.object_reports.is_empty());
            assert!(codegen.object_reports.iter().all(|r| r.contains(&pipeline)));
        } else {
            let flag = if optimization == Optimization::O0 {
                "-O0"
            } else {
                "-O2"
            };
            assert!(
                report
                    .commands
                    .iter()
                    .any(|command| command.iter().any(|arg| arg == flag))
            );
        }
        let actual = Command::new("timeout")
            .args(["-s", "KILL", "5s"])
            .arg(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(actual.status.code(), Some(0), "{actual:?}");
        assert!(actual.stdout.is_empty());
        assert!(actual.stderr.is_empty());
        return;
    }
    for backend in ["clang", "llvm-api"] {
        if backend == "llvm-api" && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in ["O0", "O2"] {
            for case in ["argument", "return", "pointer"] {
                let actual = Command::new("timeout")
                    .args([
                        "-s",
                        "KILL",
                        "55s",
                        "bash",
                        "-c",
                        "ulimit -c 0; ulimit -t 45; ulimit -v 2097152; exec \"$@\"",
                        "--",
                    ])
                    .arg(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "large_copy_a18_by_value_abi_native_all_backends",
                        "--nocapture",
                        "--test-threads=1",
                    ])
                    .env("ZIL_333_CASE", case)
                    .env(
                        "ZIL_333_LARGE_COPY_CHILD",
                        format!("{backend}-{optimization}"),
                    )
                    .output()
                    .unwrap();
                assert!(
                    actual.status.success(),
                    "{backend}-{optimization}: {} {}",
                    String::from_utf8_lossy(&actual.stdout),
                    String::from_utf8_lossy(&actual.stderr)
                );
                eprintln!(
                    "333 a18 ABI {case} {backend}-{optimization}: backend matched, requested pipeline, fallback=null, native_exit=0"
                );
            }
        }
    }
}

#[test]
fn records_nested_update_parameter_and_return() {
    let llvm = native(
        include_str!("../../../examples/aggregates/main.zil"),
        Some(6483),
    );
    assert!(llvm.contains("insertvalue"));
    assert!(llvm.contains("extractvalue"));
    assert!(llvm.contains("{ { i64, i64 }, { i64, i64 }, i64 }"));
}
#[test]
fn arrays_nested_fields_loop_and_copy_return() {
    let llvm = native(
        include_str!("../../../examples/aggregates/arrays.zil"),
        Some(7498),
    );
    assert!(llvm.contains("[3 x { i64, i64 }]"));
    assert!(llvm.contains("icmp ult i64"));
    assert!(llvm.contains("label %zil.trap"));
    let entry = llvm.split("entry:").nth(1).unwrap();
    assert!(
        entry
            .split("bb.0:")
            .next()
            .unwrap()
            .contains("%index.slot.")
    );
}
#[test]
fn empty_unit_bool_tuples_and_nested_arrays() {
    native(
        "struct Empty {} struct Bits { unit: (); flag: bool; } fn id(x: Empty) -> Empty { return x; } fn result() -> i32 { let e = id(Empty {}); let z: [i64; 0] = []; var t = (Bits { unit: (), flag: true }, [3, 7]); t.1[0] = 5; t.0.unit = (); if t.0.flag { return t.1[0] * 10 + t.1[1] }; return 0; }",
        Some(57),
    );
}
#[test]
fn tuple_binding_original_readiness_source() {
    native_entry(
        include_str!("../../../tests/language-readiness/pending/runtime-tuple-binding.zil"),
        "main",
        Some(0),
    );
}

#[test]
fn common313_discard_still_checks_copy_and_evaluates_record_rhs_once() {
    review329_native(
        &[(
            "main",
            "enum Envelope{Value{left:i32,right:i32}}fn mark(trace:&mut i32,n:i32)->i32{*trace=*trace*10+n;return n}fn result()->i32{var trace=0;let(_,_)=(Envelope.Value{right:mark(&mut trace,2),left:mark(&mut trace,1)},());return trace}",
        )],
        21,
    );
    {
        let text = "fn result()->i32{let value=7;let(_,_)=(&value,0);return value}";
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
        assert_eq!(report.diagnostics[0].code, "E-UNSUPPORTED");
        assert_eq!(report.verification_runs, 0);
        assert!(!report.artifacts.contains_key("executable"));
    }
    native_entry(
        "resource Token{value:i32}impl Drop for Token{fn drop(self:Self){}}fn result()->i32{let(_,_)=(Token{value:7},0);return 0}",
        "result",
        Some(0),
    );
}
#[test]
fn review318_tuple_rhs_order_and_ct_traversal() {
    native(
        "fn step(c:&mut i32)->i32{*c+=1;return *c}fn result()->i32{var count=0;let(a,(b,z))=(step(&mut count),(step(&mut count),#(2+3)));return count*1000+a*100+b*10+z}",
        Some(2125),
    );
}
#[test]
fn review318_tuple_defer_capture_and_scoped_initialization() {
    native(
        "fn result()->i32{var(a,b)=(1,2);if true{defer{let(x,y)=(b,1);a+=x+y}}return a*10+b}",
        Some(42),
    );
    native(
        "fn result()->i32{var outer=5;var f=()=>{let(a,b)=(outer,1);outer=a+b;return outer};let first=f();return first*10+f()}",
        Some(67),
    );
    for (text, code) in [
        (
            "fn result()->i32{if true{let(a,b)=(1,2)}return a}",
            "E-UNKNOWN-NAME",
        ),
        (
            "fn result()->i32{let f=()=>{let(a,b)=(missing,1);return a+b};return f()}",
            "E-UNKNOWN-NAME",
        ),
        (
            "fn result()->i32{var(a,b)=(1,2);defer{let(x,y)=(a,b);return x+y}return 0}",
            "E-DEFER-CONTROL",
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
        eprintln!("review318/walker rejection: {}", report.to_json());
    }
}
#[test]
fn review318_original_generic_gap_and_mixed_copy_record_obligations() {
    native(
        include_str!("../../../tests/language-readiness/recovery/review318-generic-tuple-gap.zil"),
        Some(42),
    );
    let valid = "struct Pair<T:Copy,U>{first:T;second:U}fn hidden<T:Copy,U:Copy>(p:Pair<T,U>)->Pair<T,U>{let(x,_)=(p,());return x}fn result()->i32{return 0}";
    let report = compile_source(&source(valid), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    for text in [
        "struct T{n:i32}fn hidden<T>(value:T){let(_,_)=(value,());}fn result()->i32{return 0}",
        "struct Pair<T:Copy,U>{first:T;second:U}fn hidden<T:Copy,U>(p:Pair<T,U>){let(_,_)=(p,());}fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        eprintln!("review318/symbolic obligation: {}", report.to_json());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-GENERIC-ARGUMENT")
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}
#[test]
fn tuple_binding_nested_mutable_typed_discard_and_single_evaluation() {
    let llvm = native(
        "fn pair(count: &mut i32) -> (i32,(i32,i32)) { *count += 1; return (*count,(*count+1,*count+2)); } fn result()->i32 { var count=0; let (a,(b,_)):(i32,(i32,i32))=pair(&mut count); var (c,(d,e))=(a,(b,7)); c+=10; d+=20; e+=30; let ()=(); let (_,)= (9,); return count*1000+a*100+b*10+c+d+e; }",
        Some(1190),
    );
    assert!(llvm.contains("extractvalue"));
    assert!(!llvm.contains("malloc"));
}
#[test]
fn tuple_binding_shadow_closure_and_copy_fields() {
    native(
        "struct P { x:i32; } fn result()->i32 { let x=5; if true { let (x,y)=(x+1,2); let f=()=>{ let (a,b)=(x,y); return a+b; }; var (p,a,flag,u)=(P{x:f()},[3,4],true,()); p.x+=1; a[1]=7; if flag { return p.x*100+a[0]*10+a[1]; } }; return 0; }",
        Some(937),
    );
    native(
        "fn result()->i32 { var (a,b)=(1,2); var f=()=>{ a+=b; return a; }; let first=f(); return first*10+f(); }",
        Some(35),
    );
    native(
        "fn result()->i64 { let (a,b):(i64,i64)=(3000000000,2); return a+b; }",
        Some(3000000002),
    );
}
#[test]
fn tuple_binding_shape_and_permission_neighbors() {
    for (text, code) in [
        (
            "fn result()->i32 { let (a,)=(1,2); return a; }",
            "E-MATCH-ARITY",
        ),
        (
            "fn result()->i32 { let (a,b,c)=(1,2); return a; }",
            "E-MATCH-ARITY",
        ),
        (
            "fn result()->i32 { let (a,(b,))=(1,(2,3)); return a; }",
            "E-MATCH-ARITY",
        ),
        (
            "fn result()->i32 { let (a,b)=1; return a; }",
            "E-MATCH-TYPE",
        ),
        (
            "fn result()->i32 { let ()=(1,2); return 0; }",
            "E-MATCH-ARITY",
        ),
        (
            "fn result()->i32 { let (a,a)=(1,2); return a; }",
            "E-MATCH-BINDING",
        ),
        (
            "fn result()->i32 { let (a,b):(i32,bool)=(1,2); return a; }",
            "E-TYPE-MISMATCH",
        ),
        (
            "fn result()->i32 { let (a,b)=(1,2); a=3; return a; }",
            "E-IMMUTABLE-ASSIGNMENT",
        ),
        (
            "fn result()->i32 { let (self,b)=(1,2); return b; }",
            "E-RECEIVER-RESERVED-NAME",
        ),
        (
            "fn result()->i32 { var (a,b)=(1,2); let f=()=>{a+=b;return a;}; return f(); }",
            "E-CLOSURE-MUTABILITY",
        ),
    ] {
        reject(text, code);
    }
    // Terminal340 gives references to one real temporary for an rvalue RHS.
    // The frozen source controls now succeed; named immutable places remain rejected.
    for text in [
        "fn result()->i32 { let (ref a,b)=(1,2); return b; }",
        "fn result()->i32 { let (ref mut a,b)=(1,2); return b; }",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    for text in [
        "fn result()->i32{let(ref a,b)=(20,22);return *a+b}",
        "fn result()->i32{let(ref mut a,b)=(20,22);*a+=b;return *a}",
    ] {
        review329_native(&[("main", text)], 42);
    }
    for text in [
        "fn result()->i32 { let (1,b)=(1,2); return b; }",
        "fn result()->i32 { var a=1; let (_,_) = (&mut a,2); return a; }",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}
#[test]
fn tuple_binding_existing_ir_wire_and_independent_projection_check() {
    let src = source("fn result()->i32 { let (a,(b,_))=(1,(2,3)); return a+b; }");
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&bytes, &target, &limits)
            .unwrap()
            .module(),
        &typed
    );
    let mut damaged = typed.clone();
    let field = damaged.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find(|i| matches!(i.kind, InstructionKind::ExtractField { .. }))
        .unwrap();
    if let InstructionKind::ExtractField { index, .. } = &mut field.kind {
        *index = 99;
    }
    let mut claims = plan;
    claims.module_fingerprint = damaged.fingerprint();
    let error = zil_verify::verify_module(damaged, claims, &target, &limits).unwrap_err();
    assert_eq!(error[0].phase, Phase::Verify);
    assert!(error[0].code.starts_with("E-VERIFY-"));
}
#[test]
fn tuple_binding_pattern_work_has_exact_fresh_limit() {
    let src = source("fn result()->i32 { let (a,(b,_))=(1,(2,3)); return a+b; }");
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let succeeds = |steps| {
        zil_types::check(
            &core,
            &target,
            &Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            },
        )
    };
    let (mut lo, mut hi) = (1, Limits::default().max_analysis_steps);
    assert!(succeeds(hi).is_ok());
    while lo < hi {
        let middle = lo + (hi - lo) / 2;
        if succeeds(middle).is_ok() {
            hi = middle;
        } else {
            lo = middle + 1;
        }
    }
    let failed = succeeds(lo - 1).unwrap_err();
    assert_eq!(failed[0].category, ResultCategory::ResourceLimit);
    assert!(succeeds(lo).is_ok());
    assert!(succeeds(lo).is_ok());
    eprintln!("tuple binding Types exact analysis limit: {lo}");
}
#[test]
fn record_defaults_context_and_inferred_updates() {
    native(
        "struct Pair { x: i64 = 7; y: i64 = 9; } fn result() -> i64 { let p: Pair = .{ y: 20 }; let q = .{ ...p, x: 4 }; return p.x * 100 + q.x * 10 + q.y; }",
        Some(760),
    );
}
#[test]
fn static_and_value_receiver_methods() {
    native(
        "struct Item { price: i64; quantity: i64; } impl Item { fn make(price: i64, quantity: i64) -> Self => Item { price, quantity }; fn total(self: Self) -> i64 => .price * .quantity; } fn result() -> i64 { let item = Item.make(7, 3); return item.total() + Item.total(Item.make(12, 4)); }",
        Some(69),
    );
}
#[test]
fn malformed_record_fields_and_nominal_mismatch() {
    for (text, code) in [
        (
            "struct P { x: i32; } fn result() -> i32 { let p=P { x:1,x:2 }; return p.x; }",
            "E-FIELD-DUPLICATE",
        ),
        (
            "struct P { x: i32; y:i32; } fn result() -> i32 { let p=P { x:1 }; return p.x; }",
            "E-FIELD-MISSING",
        ),
        (
            "struct P { x: i32; } fn result() -> i32 { let p=P { x:1 }; return p.y; }",
            "E-UNKNOWN-FIELD",
        ),
        (
            "struct P { x: i32; } struct Q { x:i32; } fn f(p:P)->i32{return p.x;} fn result()->i32{return f(Q{x:1});}",
            "E-TYPE-MISMATCH",
        ),
        (
            "struct P { x:P; } fn result()->i32{return 0;}",
            "E-INFINITE-LAYOUT",
        ),
        (
            "struct P { x:i32; } fn result()->i32{let p=P{x:1};p.x=2;return p.x;}",
            "E-IMMUTABLE-ASSIGNMENT",
        ),
        (
            "fn result()->i32{let p:[i32;2]=[1];return p[0];}",
            "E-ARRAY-LENGTH",
        ),
        (
            "fn result()->i32{let p=[1,2];return p[true];}",
            "E-INDEX-TYPE",
        ),
        (
            "fn result()->i32{let p=(1,2);return p.2;}",
            "E-UNKNOWN-FIELD",
        ),
    ] {
        reject(text, code);
    }
}
#[test]
fn signed_and_upper_bounds_reads_and_writes_trap() {
    for source in [
        "fn get(i:i64)->i32 {let values=[3,4];return values[i];} fn result()->i32{return get(-1);}",
        "fn get(i:i32)->i32 {let values=[3,4];return values[i];} fn result()->i32{return get(2);}",
        "fn result()->i32{var values=[3,4];values[2]=9;return values[0];}",
        "fn result()->i32{let values:[i32;0]=[];return values[0];}",
    ] {
        native(source, None);
    }
}
#[test]
fn instance_method_value_is_not_silently_bound() {
    reject(
        "struct P{x:i32;} impl P {fn get(self:Self)->i32=>.x;} fn result()->i32{let p=P{x:1};let f=p.get;return 0;}",
        "E-RECEIVER-METHOD-VALUE",
    );
}
#[test]
fn wire_roundtrip_and_independent_field_forgery_rejection() {
    let src = source(
        "struct P { x:i64; y:i64; } fn result()->i64 { var p=P{x:1,y:2};p.x=3;return p.y; }",
    );
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    let checked = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    assert_eq!(checked.module(), &typed);
    for modification in 0..3 {
        let mut damaged = typed.clone();
        let f = &mut damaged.functions[0];
        if modification == 2 {
            let i = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::MakeAggregate { .. }))
                .unwrap();
            if let InstructionKind::MakeAggregate { fields } = &mut i.kind {
                fields.pop();
            }
        } else {
            let i = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| matches!(i.kind, InstructionKind::ExtractField { .. }))
                .unwrap();
            if modification == 0 {
                if let InstructionKind::ExtractField { index, .. } = &mut i.kind {
                    *index = 99;
                }
            } else {
                f.values[i.result.unwrap().0 as usize] = Type::Bool;
            }
        }
        let mut claims = plan.clone();
        claims.module_fingerprint = damaged.fingerprint();
        let errors = zil_verify::verify_module(damaged, claims, &target, &limits).unwrap_err();
        assert_eq!(errors[0].phase, Phase::Verify);
        assert!(errors[0].code.starts_with("E-VERIFY-"));
    }
}

#[test]
fn raw_aggregate_shapes_obey_work_and_depth_limits_before_identity_checks() {
    let src = source("fn result()->i32{return 3;}");
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let plan = zil_analyze::analyze(&typed, &defaults).unwrap();
    let mut nested = Type::I32;
    for _ in 0..32 {
        nested = Type::Tuple(vec![nested]);
    }
    for (shape, limits) in [
        (
            nested,
            Limits {
                max_call_depth: 16,
                ..defaults.clone()
            },
        ),
        (
            Type::Array {
                element: Box::new(Type::I32),
                length: u32::MAX,
            },
            Limits {
                max_analysis_steps: 1024,
                ..defaults.clone()
            },
        ),
        (
            Type::Record {
                name: "x".repeat(2048),
                fields: vec![],
            },
            Limits {
                max_analysis_steps: 1024,
                ..defaults.clone()
            },
        ),
    ] {
        let mut damaged = typed.clone();
        damaged.functions[0].values[0] = shape;
        let analyzed = zil_analyze::analyze(&damaged, &limits).unwrap_err();
        assert_eq!(analyzed[0].category, ResultCategory::ResourceLimit);
        // Keep the old identity claim: shape limits must be reached before
        // computing the raw candidate fingerprint or trusting any certificate.
        let checked =
            zil_verify::verify_module(damaged, plan.clone(), &target, &limits).unwrap_err();
        assert_eq!(checked[0].category, ResultCategory::ResourceLimit);
        assert_eq!(checked[0].phase, Phase::Verify);
    }
}

#[test]
fn large_copy_abi_copy_in_and_rhs_order() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "var original=a12;let pointer:fn(Big)->Big=keep;let returned=pointer(original);let observed=pair(original,touch(&mut original));return i32(returned{first})*100+i32(original{first})*10+observed;"
    ));
    let text = format!(
        "fn keep(value:Big)->Big{{var changed=value;changed{first}=9u8;return value}}fn pair(value:Big,marker:i32)->i32{{var changed=value;changed{first}=8u8;return i32(value{first})+marker}}{text}"
    );
    let llvm = native(&text, Some(344));
    assert!(llvm.contains(".lc1(ptr %lc1.result, ptr %arg.0)"));
    assert!(!llvm.contains("define internal void @zil.fn."));
}

#[test]
fn large_copy_abi_special_closure_is_explicitly_unsupported() {
    // Match the ordinary CLI's 8 MiB main-thread stack. The recursive closure
    // metadata check exceeds Rust's smaller default test-worker stack.
    std::thread::Builder::new()
        .stack_size(
            std::env::var("ZIL_333_CAPTURE_STACK_BYTES")
                .ok()
                .map(|n| n.parse().unwrap())
                .unwrap_or(8 * 1024 * 1024),
        )
        .spawn(|| {
            let first = "[0]".repeat(13);
            for (prefix, body) in [
                ("", "let f=(value:Big)->Big=>value;return 0;".to_owned()),
                ("", format!("let f=move()=>i32(a12{first});return f();")),
                (
                    "",
                    "let f=(pointer:fn(Big,i32)->i32)->i32=>0;return 0;".to_owned(),
                ),
                (
                    "",
                    "let pointer=inspect;let f=()=>{let held=pointer;return 0};return f();"
                        .to_owned(),
                ),
                (
                    "resource Holder{call:fn(Big,i32)->i32}",
                    "let holder=Holder{call:inspect};return 0;".to_owned(),
                ),
            ] {
                let text = format!("{prefix}{}", large_copy_program(&body));
                eprintln!(
                    "333 capture probe: Verify API begins; move_capture={}",
                    body.contains("move")
                );
                let verified = compile_source(&source(&text), &CompileOptions::default());
                assert_eq!(
                    verified.status,
                    ResultCategory::Success,
                    "{}",
                    verified.to_json()
                );
                eprintln!("333 capture probe: Verify API completed; Build API begins");
                let report = compile_source(
                    &source(&text),
                    &CompileOptions {
                        entry: Some("result".into()),
                        stop_after: Stage::Build,
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Unsupported,
                    "{}",
                    report.to_json()
                );
                assert!(report.diagnostics.iter().any(|d| d.phase == Phase::Codegen));
                assert!(!report.artifacts.contains_key("llvm"));
                assert!(!report.artifacts.contains_key("executable"));
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn large_copy_abi_codegen_budget_exact_and_fresh() {
    let src = source(include_str!(
        "../../../tests/language-readiness/codegen-pending/large-array-a18-function-pointer.zil"
    ));
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let candidate = zil_analyze::analyze(&typed, &defaults).unwrap();
    let emit = |steps| {
        let limits = Limits {
            max_analysis_steps: steps,
            ..defaults.clone()
        };
        let verified =
            zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits)?;
        let lir = zil_lower::lower(&verified)?;
        zil_backend_llvm::emit(&verified, &lir)
    };
    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    assert!(emit(high).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        if emit(middle).is_ok() {
            high = middle
        } else {
            low = middle + 1
        }
    }
    let errors = emit(low - 1).unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert_eq!(errors[0].phase, Phase::Codegen);
    assert!(emit(low).unwrap().contains(".lc1"));
    assert!(emit(low).unwrap().contains(".lc1"));
    if cfg!(feature = "llvm-api") {
        let out = Temp::new();
        let options = CompileOptions {
            backend: zil_driver::NativeBackend::LlvmApi,
            stop_after: Stage::Build,
            object_cache: Some(out.0.join("objects")),
            output: Some(out.0.join("program")),
            ..CompileOptions::default()
        };
        let cold = compile_source(&src, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        assert!(cold.native_codegen.as_ref().unwrap().object_cache.misses > 0);
        let failed = compile_source(
            &src,
            &CompileOptions {
                limits: Limits {
                    max_analysis_steps: low - 1,
                    ..defaults.clone()
                },
                ..options.clone()
            },
        );
        assert_eq!(
            failed.status,
            ResultCategory::ResourceLimit,
            "{}",
            failed.to_json()
        );
        assert_eq!(failed.phase, Phase::Codegen);
        assert!(!failed.artifacts.contains_key("executable"));
        let warm = compile_source(&src, &options);
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        assert_eq!(warm.native_codegen.as_ref().unwrap().object_cache.misses, 0);
        assert!(warm.native_codegen.as_ref().unwrap().object_cache.hits > 0);
    }
    eprintln!("333 a18 function-pointer Codegen exact work limit: {low}");
}

#[test]
fn large_copy_abi_cgu_cache_and_canonical_function_reference() {
    use zil_driver::{
        CodegenOptions, NativeBackend, Optimization, ProjectSources, compile_project,
    };
    let mut ty = "[u8;1]".to_owned();
    let mut initialization = "let a0=[3u8];".to_owned();
    for n in 1..=12 {
        ty = format!("[{ty};2]");
        initialization.push_str(&format!("let a{n}=[a{},a{}];", n - 1, n - 1));
    }
    let first = "[0]".repeat(13);
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = Temp::new();
            for (round, changed) in [(0, false), (1, false), (2, true)] {
                let body = if changed {
                    format!("var changed=value;changed{first}=9u8;return changed")
                } else {
                    "return value".into()
                };
                let modules=[
              ("main",format!("import model;import factory;fn main()->i32{{{initialization}let p=factory.pointer();let value=p(a12);return i32(value{first})}}")),
              ("model",format!("pub type Big={ty};pub fn identity(value:Big)->Big{{{body}}}")),
              ("factory","import model;pub fn pointer()->fn(model.Big)->model.Big{return model.identity}".into()),
            ];
                let project = ProjectSources {
                    root: ModuleId(0),
                    modules: modules
                        .iter()
                        .enumerate()
                        .map(|(n, (name, text))| {
                            SourceUnit::new(
                                FileId(n as u32),
                                ModuleId(n as u32),
                                format!("{name}.zil"),
                                text.clone(),
                            )
                        })
                        .collect(),
                    paths: [
                        (vec!["model".into()], ModuleId(1)),
                        (vec!["factory".into()], ModuleId(2)),
                    ]
                    .into_iter()
                    .collect(),
                };
                let report = compile_project(
                    &project,
                    &CompileOptions {
                        backend,
                        optimization,
                        incremental_o2: optimization == Optimization::O2,
                        stop_after: Stage::Build,
                        object_cache: (backend == NativeBackend::LlvmApi)
                            .then(|| out.0.join("objects")),
                        codegen: CodegenOptions { units: 3, jobs: 1 },
                        output: Some(out.0.join(format!("program{round}"))),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let actual = Command::new("timeout")
                    .args(["-s", "KILL", "5s"])
                    .arg(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(
                    actual.status.code(),
                    Some(if changed { 9 } else { 3 }),
                    "{actual:?}"
                );
                let codegen = report.native_codegen.as_ref().unwrap();
                assert!(codegen.actual_units >= 2, "{}", report.to_json());
                assert!(codegen.fallback.is_none(), "{}", report.to_json());
                if backend == NativeBackend::LlvmApi {
                    assert!(codegen.object_cache.enabled, "{}", report.to_json());
                    if round == 1 {
                        assert_eq!(codegen.object_cache.hits, codegen.actual_units);
                        assert_eq!(codegen.object_cache.misses, 0);
                    } else {
                        assert!(codegen.object_cache.misses > 0);
                    }
                }
                let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
                assert!(llvm.contains(".lc1"));
            }
        }
    }
}

#[test]
fn large_copy_abi_record_tuple_unit_padding_roundtrip_all_backends() {
    use zil_driver::{NativeBackend, Optimization};
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "let pair=(a12,());var record=Envelope{{unit:(),flag:true,body:a12,tail:9}};let pointer:fn((Big,()),Envelope)->(Envelope,Big)=relay;let kept=pointer(pair,record);record.body{first}=7u8;return i32(kept.0.body{first})*1000+i32(kept.1{first})*100+i32(record.body{first})*10+i32(kept.0.tail);"
    ));
    let text = format!(
        "struct Envelope{{unit:();flag:bool;body:Big;tail:i64}}fn relay(pair:(Big,()),record:Envelope)->(Envelope,Big){{var changed=record;changed.body{first}=9u8;changed.tail=11;return (changed,pair.0)}}{text}"
    );
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = Temp::new();
            let report = compile_source(
                &source(&text),
                &CompileOptions {
                    backend,
                    optimization,
                    entry: Some("result".into()),
                    stop_after: Stage::Build,
                    output: Some(out.0.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            let observed = Command::new("timeout")
                .args(["-s", "KILL", "5s"])
                .arg(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(observed.status.success(), "{observed:?}");
            assert_eq!(observed.stdout, b"9381\n");
            let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
            assert!(llvm.contains(".lc1(ptr %lc1.result, ptr %arg.0, ptr %arg.1)"));
        }
    }
}

#[test]
fn large_copy_abi_unit_erasure_small_signature_and_void_return() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(
        "var observed=0;consume((),a12,2,&mut observed);let scalar=small([4u8]);return observed+i32(scalar[0]);",
    );
    let text = format!(
        "fn consume(unit:(),value:Big,marker:i32,output:&mut i32)->(){{*output=i32(value{first})+marker;return ()}}fn small(value:[u8;1])->[u8;1]{{return value}}{text}"
    );
    let llvm = native(&text, Some(9));
    assert!(llvm.contains(".lc1(ptr %arg.1, i32 %arg.2, ptr %arg.3)"));
    let small = llvm
        .lines()
        .find(|line| line.starts_with("define [1 x i8] @zil.fn."))
        .unwrap();
    assert!(!small.contains(".lc1"));
}

#[test]
fn large_copy_abi_borrow_capture_keeps_pointer_environment() {
    // The captured source variable is Big, but its mutable borrowed environment
    // field is one pointer. LargeCopy guards must inspect the physical value.
    std::thread::Builder::new().stack_size(8*1024*1024).spawn(|| {
        use zil_driver::{NativeBackend,Optimization};
        let first="[0]".repeat(13);
        let text=large_copy_program(&format!("var original=a12;var f=()=>{{original{first}+=1u8;return i32(original{first})}};return f()*10+f();"));
        for backend in [NativeBackend::Clang,NativeBackend::LlvmApi] {
            if backend==NativeBackend::LlvmApi && !cfg!(feature="llvm-api") {continue;}
            for optimization in [Optimization::O0,Optimization::O2] {
                let out=Temp::new();
                let report=compile_source(&source(&text),&CompileOptions {backend,optimization,entry:Some("result".into()),stop_after:Stage::Build,output:Some(out.0.join("program")),..CompileOptions::default()});
                assert_eq!(report.status,ResultCategory::Success,"{}",report.to_json());
                let actual=Command::new("timeout").args(["-s","KILL","5s"]).arg(&report.artifacts["executable"]).output().unwrap();
                assert!(actual.status.success(),"{actual:?}");
                assert_eq!(actual.stdout,b"45\n");
            }
        }
    }).unwrap().join().unwrap();
}

#[test]
fn large_copy_abi_recursive_carrier_symbols_and_old_link_rejection() {
    use zil_driver::{NativeBackend, Optimization};
    let input = include_str!(
        "../../../tests/language-readiness/codegen-pending/large-copy-version-carriers.zil"
    );
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = Temp::new();
            let report = compile_source(
                &source(input),
                &CompileOptions {
                    backend,
                    optimization,
                    stop_after: Stage::Build,
                    output: Some(out.0.join("program")),
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
            for id in 0..=12 {
                let base = format!("@zil.fn.{id}");
                let expected = if id <= 8 {
                    format!("{base}.lc1(")
                } else {
                    format!("{base}(")
                };
                assert!(
                    llvm.lines()
                        .any(|line| line.starts_with("define ") && line.contains(&expected)),
                    "missing {expected}"
                );
            }
            assert!(llvm.contains("ptr @zil.fn.1.lc1()"));
            assert!(llvm.contains("i32 @zil.fn.2.lc1(ptr %arg.0)"));
            let actual = Command::new("timeout")
                .args(["-s", "KILL", "5s"])
                .arg(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(actual.status.code(), Some(0), "{actual:?}");
            let symbols = Command::new("nm")
                .arg("-g")
                .arg(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(symbols.status.success());
            let symbols = String::from_utf8(symbols.stdout).unwrap();
            for id in 0..=8 {
                assert!(
                    symbols
                        .lines()
                        .any(|line| line.ends_with(&format!(" T zil.fn.{id}.lc1")))
                );
                assert!(
                    !symbols
                        .lines()
                        .any(|line| line.ends_with(&format!(" T zil.fn.{id}")))
                );
            }
            if backend == NativeBackend::Clang && optimization == Optimization::O0 {
                let object = out.0.join("definitions.o");
                let built = Command::new("clang")
                    .args(["-x", "ir", "-O0", "-c"])
                    .arg(&report.artifacts["llvm"])
                    .arg("-o")
                    .arg(&object)
                    .output()
                    .unwrap();
                assert!(built.status.success(), "{built:?}");
                for (suffix, success) in [("", false), (".lc1", true)] {
                    let probe = out.0.join("consumer.c");
                    fs::write(&probe,format!("extern void *factory(void) __asm__(\"zil.fn.1{suffix}\");void *consume_factory(void){{return factory();}}\n")).unwrap();
                    let linked = Command::new("clang")
                        .arg(&probe)
                        .arg(&object)
                        .arg("-o")
                        .arg(out.0.join("consumer"))
                        .output()
                        .unwrap();
                    assert_eq!(linked.status.success(), success, "{linked:?}");
                    if !success {
                        let error = String::from_utf8_lossy(&linked.stderr);
                        assert!(
                            error.contains("undefined reference") && error.contains("zil.fn.1"),
                            "{error}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn review329_astra_symbolic_copy_does_not_grant_primitive_operations() {
    for (label, text) in [
        (
            "named_i32",
            "enum E<T:Copy>{V{x:T}}fn bad<i32:Copy>(e:E<i32>)->u8{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "tuple_i32",
            "enum E<T:Copy>{V(T)}fn bad<i32:Copy>(e:E<i32>)->u8{match e{E.V(1)=>{return 1},E.V(_)=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "named_i64",
            "enum E<T:Copy>{V{x:T}}fn bad<i64:Copy>(e:E<i64>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "tuple_i64",
            "enum E<T:Copy>{V(T)}fn bad<i64:Copy>(e:E<i64>)->i32{match e{E.V(1)=>{return 1},E.V(_)=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "named_u8",
            "enum E<T:Copy>{V{x:T}}fn bad<u8:Copy>(e:E<u8>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "tuple_u8",
            "enum E<T:Copy>{V(T)}fn bad<u8:Copy>(e:E<u8>)->i32{match e{E.V(1)=>{return 1},E.V(_)=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "named_u16",
            "enum E<T:Copy>{V{x:T}}fn bad<u16:Copy>(e:E<u16>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "tuple_u16",
            "enum E<T:Copy>{V(T)}fn bad<u16:Copy>(e:E<u16>)->i32{match e{E.V(1)=>{return 1},E.V(_)=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "named_bool",
            "enum E<T:Copy>{V{x:T}}fn bad<bool:Copy>(e:E<bool>)->i32{match e{E.V{x:true}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "tuple_bool",
            "enum E<T:Copy>{V(T)}fn bad<bool:Copy>(e:E<bool>)->i32{match e{E.V(true)=>{return 1},E.V(_)=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "named_T",
            "enum E<T:Copy>{V{x:T}}fn bad<T:Copy>(e:E<T>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "tuple_T",
            "enum E<T:Copy>{V(T)}fn bad<T:Copy>(e:E<T>)->i32{match e{E.V(1)=>{return 1},E.V(_)=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "alias_i64",
            "enum E<T:Copy>{V{x:T}}type Alias<T>=E<T>;fn bad<i64:Copy>(e:Alias<i64>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "nested_i64",
            "enum E<T:Copy>{V{x:(T,bool)}}fn bad<i64:Copy>(e:E<i64>)->i32{match e{E.V{x:(1,_)}=>{return 1},E.V{x:(_,_) }=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "scalar_bool",
            "fn bad<bool:Copy>(b:bool)->i32{match b{true=>{return 1},false=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "bool_construct",
            "enum E<T:Copy>{V{x:T}}fn bad<bool:Copy>()->E<bool>{return E::<bool>.V{x:true}}fn result()->i32{return 0}",
        ),
        (
            "bool_return",
            "fn bad<bool:Copy>()->bool{return true}fn result()->i32{return 0}",
        ),
        (
            "bool_condition",
            "fn bad<bool:Copy>(b:bool)->i32{if b{return 1}return 0}fn result()->i32{return 0}",
        ),
        (
            "i64_conversion",
            "fn bad<i64:Copy>(x:i64)->i32{return i32(x)}fn result()->i32{return 0}",
        ),
        (
            "i64_index",
            "fn bad<i64:Copy>(a:[i32;2],i:i64)->i32{return a[i]}fn result()->i32{return 0}",
        ),
        (
            "enum_shadow_named",
            "enum E{V{x:i32}}fn bad<E:Copy>(e:E)->i32{match e{E.V{x:x}=>{return x}}}fn result()->i32{return 0}",
        ),
        (
            "enum_shadow_tuple",
            "enum E{V(i32)}fn bad<E:Copy>(e:E)->i32{match e{E.V(x)=>{return x}}}fn result()->i32{return 0}",
        ),
        (
            "enum_shadow_construct",
            "enum E{V{x:i32}}fn bad<E:Copy>()->E{return E.V{x:1}}fn result()->i32{return 0}",
        ),
        (
            "struct_shadow_field",
            "struct P{x:i32}fn bad<P:Copy>(p:P)->i32{return p.x}fn result()->i32{return 0}",
        ),
        (
            "negative_slice",
            "fn bad<i64:Copy>(a:[i32;2],i:i64){let s=&a[i..i]}fn result()->i32{return 0}",
        ),
        (
            "negative_while",
            "fn bad<bool:Copy>(b:bool){while b{break}}fn result()->i32{return 0}",
        ),
        (
            "negative_conversion_output",
            "fn bad<i64:Copy>()->i64{return i64(1)}fn result()->i32{return 0}",
        ),
        (
            "negative_ordinary_call",
            "fn consume(x:i64){}fn bad<i64:Copy>(x:i64){consume(x)}fn result()->i32{return 0}",
        ),
        (
            "negative_unrelated_alias",
            "type Alias=i32;fn bad<bool:Copy>()->bool{return true}fn result()->i32{return 0}",
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
        assert_eq!(report.verification_runs, 0, "{label}");
    }
}

#[test]
fn review329_astra_lexical_builtin_controls() {
    for (label, text) in [
        (
            "legal_life_bool_type_i64",
            "fn keep<i64:Copy,life bool>(x:&'bool i64)->&'bool i64{return x}fn result()->i32{let x=1;return *keep(&x)}",
        ),
        (
            "legal_life_alias_type_i64",
            "type A=i32;fn keep<i64:Copy,life A>(x:&'A i64)->&'A i64{return x}fn result()->A{let x=1;return *keep(&x)}",
        ),
        (
            "legal_life_i64_only",
            "fn trigger<bool:Copy>(x:bool)->bool{return x}fn keep<life i64>(x:&'i64 i64)->&'i64 i64{return x}fn result()->i64{let x:i64=1;return *keep(&x)}",
        ),
        (
            "legal_concrete_named",
            "enum E<T:Copy>{V{x:T}}fn good<T:Copy>(unused:T,e:E<i64>)->i32{match e{E.V{x:1}=>{return 1},E.V{x:_}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "legal_dependent_named",
            "enum E<T:Copy>{V{x:T}}fn good<i64:Copy>(e:E<i64>)->i64{match e{E.V{x:x}=>{return x}}}fn result()->i32{return 0}",
        ),
        (
            "legal_bool_concrete",
            "enum E<T:Copy>{V{x:T}}fn good<T:Copy>(unused:T,e:E<bool>)->i32{match e{E.V{x:true}=>{return 1},E.V{x:false}=>{return 0}}}fn result()->i32{return 0}",
        ),
        (
            "legal_callee_i64",
            "fn scalar()->i64{return 1}fn keep<i64:Copy>(x:i64)->i32{let n=scalar();match n{1=>{return 1},_=>{return 0}}}fn result()->i32{return keep(true)}",
        ),
        (
            "legal_declared_i64_field",
            "enum E<T:Copy>{V{x:T,y:i64}}fn good<i64:Copy>(e:E<i64>)->i32{match e{E.V{x:_,y:1}=>{return 1},E.V{x:_,y:_}=>{return 0}}}fn result()->i32{return good(E::<bool>.V{x:true,y:i64(1)})}",
        ),
        (
            "legal_callee_bool",
            "fn scalar()->bool{return true}fn keep<bool:Copy>(x:bool)->i32{if scalar(){return 1}return 0}fn result()->i32{return keep(7)}",
        ),
        (
            "legal_renamed_identity",
            "fn keep<i64:Copy>(x:i64)->i64{return x}fn result()->i32{if keep::<bool>(true){return 1}return 0}",
        ),
        (
            "legal_alias_identity",
            "type A<i64>=i64;fn keep<bool:Copy>(x:A<bool>)->A<bool>{return x}fn result()->i32{return keep(1)}",
        ),
        (
            "legal_renamed_enum_binder",
            "enum E<bool:Copy>{V{x:bool,y:i64}}fn wrap<bool:Copy>(x:bool)->E<bool>{return E::<bool>.V{x:x,y:i64(7)}}fn result()->i64{match wrap(1){E.V{x:_,y:y}=>{return y}}}",
        ),
        (
            "legal_declared_i64_field_unsuffixed",
            "enum E<T:Copy>{V{x:T,y:i64}}fn good<i64:Copy>(e:E<i64>)->i32{match e{E.V{x:_,y:1}=>{return 1},E.V{x:_,y:_}=>{return 0}}}fn result()->i32{return good(E::<bool>.V{x:true,y:1})}",
        ),
        (
            "legal_renamed_enum_binder_unsuffixed",
            "enum E<bool:Copy>{V{x:bool,y:i64}}fn wrap<bool:Copy>(x:bool)->E<bool>{return E::<bool>.V{x:x,y:7}}fn result()->i64{match wrap(1){E.V{x:_,y:y}=>{return y}}}",
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{label}");
    }
}

#[test]
fn large_copy_enum_error_original_scale_all_backends() {
    // Isolate LLVM failures and cap each compiler process. This remains one
    // existing Cargo integration target and preserves the original input bytes.
    if let Ok(profile) = std::env::var("ZIL_338_SUM_CHILD") {
        use zil_driver::{NativeBackend, Optimization};
        let (backend, optimization) = match profile.as_str() {
            "clang-O0" => (NativeBackend::Clang, Optimization::O0),
            "clang-O2" => (NativeBackend::Clang, Optimization::O2),
            "llvm-api-O0" => (NativeBackend::LlvmApi, Optimization::O0),
            "llvm-api-O2" => (NativeBackend::LlvmApi, Optimization::O2),
            _ => panic!("unknown child profile"),
        };
        let out = Temp::new();
        let case = std::env::var("ZIL_338_CASE").unwrap();
        // Explicit manual KnownIncomplete reproduction only. The default test
        // cases below never invoke this API small-stack crash or enlarge it.
        if case == "capture-check" {
            let input = include_str!(
                "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-borrow-capture.zil"
            );
            eprintln!(
                "338 borrowed Sum capture: Verify-only API begins (ordinary test worker stack)"
            );
            let report = compile_source(&source(input), &CompileOptions::default());
            eprintln!(
                "338 borrowed Sum capture: Verify-only API completed: {}",
                report.to_json()
            );
            return;
        }
        let input = match case.as_str() {
            "enum" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-enum-a18.zil"
            ),
            "error" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-error-a18.zil"
            ),
            "empty" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-enum-empty-a18.zil"
            ),
            "enum-indirect" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-enum-fnptr-a18.zil"
            ),
            "error-indirect" => include_str!(
                "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-error-fnptr-a18.zil"
            ),
            _ => panic!("unknown ABI case"),
        };
        let report = compile_source(
            &source(input),
            &CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                cancellation: zil_driver::CancellationToken::with_timeout(
                    std::time::Duration::from_secs(45),
                ),
                output: Some(out.0.join("program")),
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
        assert!(llvm.contains(".lc1("), "versioned definition/call absent");
        assert!(llvm.contains("@zil.copy.bytes"));
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, backend);
        assert!(codegen.fallback.is_none());
        if backend == NativeBackend::LlvmApi {
            let pipeline = format!("\"pipeline\":\"default<{optimization:?}>\"");
            assert!(!codegen.object_reports.is_empty());
            assert!(codegen.object_reports.iter().all(|r| r.contains(&pipeline)));
        } else {
            let flag = if optimization == Optimization::O0 {
                "-O0"
            } else {
                "-O2"
            };
            assert!(
                report
                    .commands
                    .iter()
                    .any(|command| command.iter().any(|arg| arg == flag))
            );
        }
        let actual = Command::new("timeout")
            .args(["-s", "KILL", "5s"])
            .arg(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(actual.status.code(), Some(0), "{actual:?}");
        assert!(actual.stdout.is_empty());
        assert!(actual.stderr.is_empty());
        return;
    }
    for backend in ["clang", "llvm-api"] {
        if backend == "llvm-api" && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in ["O0", "O2"] {
            for case in ["enum", "error", "empty", "enum-indirect", "error-indirect"] {
                let actual = Command::new("timeout")
                    .args([
                        "-s",
                        "KILL",
                        "55s",
                        "bash",
                        "-c",
                        "ulimit -c 0; ulimit -t 45; ulimit -v 2097152; exec \"$@\"",
                        "--",
                    ])
                    .arg(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "large_copy_enum_error_original_scale_all_backends",
                        "--nocapture",
                        "--test-threads=1",
                    ])
                    .env("ZIL_338_CASE", case)
                    .env("ZIL_338_SUM_CHILD", format!("{backend}-{optimization}"))
                    .output()
                    .unwrap();
                assert!(
                    actual.status.success(),
                    "{backend}-{optimization}: {} {}",
                    String::from_utf8_lossy(&actual.stdout),
                    String::from_utf8_lossy(&actual.stderr)
                );
                eprintln!(
                    "338 a18 Sum ABI {case} {backend}-{optimization}: backend matched, requested pipeline, fallback=null, native_exit=0"
                );
            }
        }
    }
}

#[test]
fn large_copy_enum_error_value_order_alignment_and_canonical_small_payloads() {
    let first = "[0]".repeat(13);
    let body = format!(
        "var original=a12;var message:Message=Message.Data{{unit:(),flag:true,n:9,bytes:original}};let saved:Message=message;let score=read(message,empty(&mut message));let pointer:fn(Message)->Message=keep;let returned:Message=pointer(saved);original{first}=4u8;let small:Message=Message.Small{{pair:(true,7),unit:()}};let small_score=read(small,0);match returned{{Message.Data{{unit:_,flag:ok,n:n,bytes:bytes}}=>{{if !ok{{return -1}}return i32(bytes{first})*1000+i32(original{first})*100+i32(n)*10+score+small_score}},Message.Empty()=>{{return -2}},Message.Small{{pair:_,unit:_}}=>{{return -3}}}}"
    );
    let declarations = format!(
        "enum Message{{Data{{unit:(),flag:bool,n:i64,bytes:Big}},Empty,Small{{pair:(bool,i64),unit:()}}}}fn keep(value:Message)->Message{{var changed=value;changed=Message.Empty;return value}}fn empty(value:&mut Message)->i32{{*value=Message.Empty;return 1}}fn read(value:Message,marker:i32)->i32{{match value{{Message.Data{{unit:_,flag:_,n:_,bytes:bytes}}=>{{return i32(bytes{first})+marker}},Message.Empty()=>{{return 0}},Message.Small{{pair:(flag,n),unit:_}}=>{{if flag{{return i32(n)}}return -4}}}}}}"
    );
    let text = format!("{declarations}{}", large_copy_program(&body));
    review329_native(&[("main", &text)], 3501);
}

#[test]
fn large_copy_enum_error_both_arms_propagation_and_failure_cleanup() {
    use zil_driver::{NativeBackend, Optimization};
    let first = "[0]".repeat(13);
    let prefix = large_copy_program("");
    let prefix = &prefix[..prefix.find("fn result").unwrap()];
    let init = (0..=12)
        .map(|n| {
            if n == 0 {
                "let a0=[3u8];".to_owned()
            } else {
                format!("let a{n}=[a{},a{}];", n - 1, n - 1)
            }
        })
        .collect::<String>();
    for ok in [true, false] {
        let text = format!(
            r#"{prefix}extern "C" fn putchar(code:i32)->i32;resource Token{{code:i32}}impl Drop for Token{{fn drop(self:Self){{unsafe{{putchar(.code)}}}}}}struct Fault{{bytes:Big,flag:bool}}fn produce(ok:bool,bytes:Big)->Fault!Big{{if ok{{return bytes}}return Fault{{bytes,flag:true}}}}fn relay(value:Fault!Big)->Fault!Big{{return value}}fn work()->Fault!i32{{{init}let token=Token{{code:65}};var original=a12;let pointer:fn(Fault!Big)->Fault!Big=relay;let saved=pointer(produce({ok},original));original{first}=9u8;let bytes=saved?;return i32(bytes{first})}}fn main()->Fault!i32{{return work()?}}"#
        );
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let out = Temp::new();
                let report = compile_source(
                    &source(&text),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        output: Some(out.0.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                let observed = Command::new("timeout")
                    .args(["-s", "KILL", "5s"])
                    .arg(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(
                    observed.status.code(),
                    Some(if ok { 3 } else { 1 }),
                    "{observed:?}"
                );
                assert_eq!(observed.stdout, b"A");
                assert!(observed.stderr.is_empty());
                let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
                assert!(llvm.contains(".lc1(ptr %lc1.result, ptr %arg.0)"));
                assert!(
                    !llvm.contains("extractvalue { i1, {"),
                    "large error must use memory projections"
                );
            }
        }
    }
}

#[test]
fn large_copy_enum_error_codegen_budget_exact_and_fresh() {
    let src = source(include_str!(
        "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-enum-a18.zil"
    ));
    let ast = zil_syntax::parse(&src).into_result().unwrap();
    let core = zil_expand::expand(&src, &ast).unwrap();
    let target = TargetProfile::default();
    let defaults = Limits::default();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let candidate = zil_analyze::analyze(&typed, &defaults).unwrap();
    let emit = |steps| {
        let limits = Limits {
            max_analysis_steps: steps,
            ..defaults.clone()
        };
        let verified =
            zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits)?;
        let lir = zil_lower::lower(&verified)?;
        zil_backend_llvm::emit(&verified, &lir)
    };
    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    assert!(emit(high).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        if emit(middle).is_ok() {
            high = middle
        } else {
            low = middle + 1
        }
    }
    let errors = emit(low - 1).unwrap_err();
    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
    assert_eq!(errors[0].phase, Phase::Codegen);
    assert!(emit(low).unwrap().contains(".lc1"));
    assert!(emit(low).unwrap().contains(".lc1"));
    if cfg!(feature = "llvm-api") {
        let out = Temp::new();
        let options = CompileOptions {
            backend: zil_driver::NativeBackend::LlvmApi,
            stop_after: Stage::Build,
            object_cache: Some(out.0.join("objects")),
            output: Some(out.0.join("program")),
            ..CompileOptions::default()
        };
        let cold = compile_source(&src, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        assert!(cold.native_codegen.as_ref().unwrap().object_cache.misses > 0);
        let failed = compile_source(
            &src,
            &CompileOptions {
                limits: Limits {
                    max_analysis_steps: low - 1,
                    ..defaults.clone()
                },
                ..options.clone()
            },
        );
        assert_eq!(
            failed.status,
            ResultCategory::ResourceLimit,
            "{}",
            failed.to_json()
        );
        assert_eq!(failed.phase, Phase::Codegen);
        assert!(!failed.artifacts.contains_key("executable"));
        let warm = compile_source(&src, &options);
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        assert_eq!(warm.native_codegen.as_ref().unwrap().object_cache.misses, 0);
        assert!(warm.native_codegen.as_ref().unwrap().object_cache.hits > 0);
    }
    eprintln!("338 a18 enum Codegen exact work limit: {low}");
}

#[test]
fn large_copy_enum_error_function_pointer_carriers_are_versioned() {
    let first = "[0]".repeat(13);
    let text = large_copy_program(&format!(
        "let handle:Handle=wrap();let pointer:Worker=unwrap(handle);let message:Message=pointer(Message.Data(a12));let raw=read_address(&message);match message{{Message.Data(bytes)=>{{return raw+i32(bytes{first})}},Message.Empty()=>{{return -1}}}}"
    ));
    let text = format!(
        "enum Message{{Data(Big),Empty}}type Worker=fn(Message)->Message;enum Handle{{Call(Worker),None}}fn relay(value:Message)->Message{{return value}}fn factory()->Worker{{return relay}}fn wrap()->Handle{{return Handle.Call(factory())}}fn unwrap(handle:Handle)->Worker{{match handle{{Handle.Call(pointer)=>{{return pointer}},Handle.None()=>{{return factory()}}}}}}fn read_address(value:&Message)->i32{{match *value{{Message.Data(bytes)=>{{return i32(bytes{first})}},Message.Empty()=>{{return 0}}}}}}{text}"
    );
    let llvm = native(&text, Some(6));
    for id in 0..=3 {
        assert!(
            llvm.lines()
                .any(|l| l.starts_with("define ") && l.contains(&format!("@zil.fn.{id}.lc1("))),
            "missing versioned {id}"
        );
    }
    assert!(llvm.contains("define i32 @zil.fn.4(ptr %arg.0)"));
    assert!(!llvm.contains("@zil.fn.4.lc1"));
}

#[test]
fn large_copy_enum_error_cgu_cache_and_canonical_function_reference() {
    use zil_driver::{
        CodegenOptions, NativeBackend, Optimization, ProjectSources, compile_project,
    };
    let mut ty = "[u8;1]".to_owned();
    let mut initialization = "let a0=[3u8];".to_owned();
    for n in 1..=12 {
        ty = format!("[{ty};2]");
        initialization.push_str(&format!("let a{n}=[a{},a{}];", n - 1, n - 1));
    }
    let first = "[0]".repeat(13);
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = Temp::new();
            for (round, changed) in [(0, false), (1, false), (2, true)] {
                let body = if changed {
                    format!(
                        "match value{{Message.Data(bytes)=>{{var changed=bytes;changed{first}=9u8;return Message.Data(changed)}},Message.Empty()=>{{return Message.Empty}}}}"
                    )
                } else {
                    "return value".into()
                };
                let modules=[
              ("main",format!("import model;import factory;fn main()->i32{{{initialization}let p=factory.pointer();let value:model.Message=p(model.Message.Data(a12));match value{{model.Message.Data(bytes)=>{{return i32(bytes{first})}},model.Message.Empty()=>{{return 0}}}}}}")),
              ("model",format!("pub type Big={ty};pub enum Message{{Data(Big),Empty}}pub fn identity(value:Message)->Message{{{body}}}")),
              ("factory","import model;pub fn pointer()->fn(model.Message)->model.Message{return model.identity}".into()),
            ];
                let project = ProjectSources {
                    root: ModuleId(0),
                    modules: modules
                        .iter()
                        .enumerate()
                        .map(|(n, (name, text))| {
                            SourceUnit::new(
                                FileId(n as u32),
                                ModuleId(n as u32),
                                format!("{name}.zil"),
                                text.clone(),
                            )
                        })
                        .collect(),
                    paths: [
                        (vec!["model".into()], ModuleId(1)),
                        (vec!["factory".into()], ModuleId(2)),
                    ]
                    .into_iter()
                    .collect(),
                };
                let report = compile_project(
                    &project,
                    &CompileOptions {
                        backend,
                        optimization,
                        incremental_o2: optimization == Optimization::O2,
                        stop_after: Stage::Build,
                        object_cache: (backend == NativeBackend::LlvmApi)
                            .then(|| out.0.join("objects")),
                        codegen: CodegenOptions { units: 3, jobs: 1 },
                        output: Some(out.0.join(format!("program{round}"))),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let actual = Command::new("timeout")
                    .args(["-s", "KILL", "5s"])
                    .arg(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(
                    actual.status.code(),
                    Some(if changed { 9 } else { 3 }),
                    "{actual:?}"
                );
                let codegen = report.native_codegen.as_ref().unwrap();
                assert!(codegen.actual_units >= 2, "{}", report.to_json());
                assert!(codegen.fallback.is_none(), "{}", report.to_json());
                if backend == NativeBackend::LlvmApi {
                    assert!(codegen.object_cache.enabled, "{}", report.to_json());
                    if round == 1 {
                        assert_eq!(codegen.object_cache.hits, codegen.actual_units);
                        assert_eq!(codegen.object_cache.misses, 0);
                    } else {
                        assert!(codegen.object_cache.misses > 0);
                    }
                }
                let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
                assert!(llvm.contains(".lc1"));
            }
        }
    }
}

#[test]
fn large_copy_enum_error_long_source_names_are_prepaid_before_emission() {
    let input = include_str!(
        "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-enum-a12.zil"
    );
    let short = input
        .replace(
            "enum E{Data(Big),Empty}",
            "struct Pad{n:i64,flag:bool}enum E{Data(Big,Pad),Empty}",
        )
        .replace("E.Data(a12)", "E.Data(a12,Pad{n:5,flag:true})")
        .replace("E.Data(a)=>", "E.Data(a,p)=>");
    let suffix = "x".repeat(256);
    let long = short
        .replace("Pad", &format!("Pad{suffix}"))
        .replace("Data", &format!("Data{suffix}"))
        .replace("Empty", &format!("Empty{suffix}"))
        .replace("flag", &format!("flag{suffix}"));
    let mut costs = Vec::new();
    for text in [&short, &long] {
        let src = source(text);
        let ast = zil_syntax::parse(&src).into_result().unwrap();
        let core = zil_expand::expand(&src, &ast).unwrap();
        let limits = Limits::default();
        let target = TargetProfile::default();
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let candidate = zil_analyze::analyze(&typed, &limits).unwrap();
        let emit = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..limits.clone()
            };
            let verified =
                zil_verify::verify_module(typed.clone(), candidate.clone(), &target, &limits)?;
            let lir = zil_lower::lower(&verified)?;
            zil_backend_llvm::emit(&verified, &lir)
        };
        let (mut low, mut high) = (1, limits.max_analysis_steps);
        assert!(emit(high).is_ok());
        while low < high {
            let mid = low + (high - low) / 2;
            if emit(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        let errors = emit(low - 1).unwrap_err();
        assert_eq!(errors[0].phase, Phase::Codegen);
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
        assert!(emit(low).is_ok());
        assert!(emit(low).is_ok());
        costs.push(low);
    }
    assert!(
        costs[1] > costs[0] + 4 * 256,
        "source metadata bytes must be in repeated emission reservations: {costs:?}"
    );
    eprintln!("338 enum source metadata Codegen exact limits short/long: {costs:?}");
}

#[test]
fn large_copy_enum_error_default_stack_api_reliability() {
    const LEGAL: &str = include_str!(
        "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-borrow-capture-var.zil"
    );
    const INVALID: &str = include_str!(
        "../../../tests/language-readiness/codegen-pending/large-copy-enum-error-borrow-capture.zil"
    );
    const DEEP: &[&str] = &[
        include_str!("../../../tests/language-readiness/terminal/344/depth20.zil"),
        include_str!("../../../tests/language-readiness/terminal/344/depth32.zil"),
        include_str!("../../../tests/language-readiness/terminal/344/projection32.zil"),
        include_str!("../../../tests/language-readiness/terminal/344/depth48.zil"),
        include_str!("../../../tests/language-readiness/terminal/344/projection48.zil"),
    ];
    if let Ok(profile) = std::env::var("ZIL_344_STACK_CHILD") {
        // No stack_size override: Linux Rust worker threads use the default
        // 2 MiB. A future abort kills this subprocess, never its parent harness.
        std::thread::spawn(move || {
            if profile == "frontend" {
                for stage in [
                    Stage::Parse,
                    Stage::Core,
                    Stage::Types,
                    Stage::Analyze,
                    Stage::Verify,
                ] {
                    let report = compile_source(
                        &source(LEGAL),
                        &CompileOptions {
                            stop_after: stage,
                            ..CompileOptions::default()
                        },
                    );
                    assert_eq!(
                        report.status,
                        ResultCategory::Success,
                        "{}",
                        report.to_json()
                    );
                    assert_eq!(
                        report.verification_runs,
                        usize::from(stage == Stage::Verify)
                    );
                }
                let rejected = compile_source(&source(INVALID), &CompileOptions::default());
                assert_eq!(
                    rejected.status,
                    ResultCategory::SafeReject,
                    "{}",
                    rejected.to_json()
                );
                assert_eq!(rejected.phase, Phase::Types);
                assert!(
                    rejected
                        .diagnostics
                        .iter()
                        .any(|d| d.code == "E-CLOSURE-MUTABILITY")
                );
                assert_eq!(rejected.verification_runs, 0);
                for input in DEEP {
                    let report = compile_source(&source(input), &CompileOptions::default());
                    assert_eq!(
                        report.status,
                        ResultCategory::Success,
                        "{}",
                        report.to_json()
                    );
                    assert_eq!(report.verification_runs, 1);
                }
                let input = source(LEGAL);
                let ast = zil_syntax::parse(&input).into_result().unwrap();
                let core = zil_expand::expand(&input, &ast).unwrap();
                let defaults = Limits::default();
                let target = TargetProfile::default();
                let constraints = |steps| {
                    zil_types::infer_source_closures(
                        &core,
                        &Limits {
                            max_analysis_steps: steps,
                            ..defaults.clone()
                        },
                    )
                };
                let (mut low, mut high) = (1, defaults.max_analysis_steps);
                assert!(constraints(high).is_ok());
                while low < high {
                    let middle = low + (high - low) / 2;
                    if constraints(middle).is_ok() {
                        high = middle;
                    } else {
                        low = middle + 1;
                    }
                }
                let errors = constraints(low - 1).unwrap_err();
                assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
                assert_eq!(errors[0].phase, Phase::Types);
                assert!(constraints(low).is_ok());
                assert!(constraints(low).is_ok());
                eprintln!("344 source constraint exact work N={low}; N/N-1/fresh checked");
                let typed = zil_types::check(&core, &target, &defaults).unwrap();
                let candidate = zil_analyze::analyze(&typed, &defaults).unwrap();
                let bytes =
                    zil_ir::wire::encode_module_candidate(&typed, &candidate, &defaults).unwrap();
                assert_eq!(
                    zil_verify::verify_module_bytes(&bytes, &target, &defaults)
                        .unwrap()
                        .module(),
                    &typed
                );
                let mut damaged = typed.clone();
                assert!(!damaged.closures[0].captures.is_empty());
                damaged.closures[0].captures[0] = Type::Bool;
                let mut claims = candidate;
                claims.module_fingerprint = damaged.fingerprint();
                let errors =
                    zil_verify::verify_module(damaged, claims, &target, &defaults).unwrap_err();
                assert_eq!(errors[0].phase, Phase::Verify);
            } else {
                use zil_driver::{NativeBackend, Optimization};
                let (backend, optimization) = match profile.as_str() {
                    "clang-O0" => (NativeBackend::Clang, Optimization::O0),
                    "clang-O2" => (NativeBackend::Clang, Optimization::O2),
                    "llvm-api-O0" => (NativeBackend::LlvmApi, Optimization::O0),
                    "llvm-api-O2" => (NativeBackend::LlvmApi, Optimization::O2),
                    _ => panic!("unknown 344 child profile"),
                };
                let directory = Temp::new();
                let report = compile_source(
                    &source(LEGAL),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        entry: Some("result".into()),
                        output: Some(directory.0.join("program")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                if backend == NativeBackend::LlvmApi {
                    assert!(!codegen.object_reports.is_empty());
                    assert!(
                        codegen
                            .object_reports
                            .iter()
                            .all(|r| r.contains("\"input_kind\":\"builder\""))
                    );
                }
                let output = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{output:?}");
                assert_eq!(output.stdout, b"30\n");
                assert!(output.stderr.is_empty());
                eprintln!("344 default-stack {profile}: Verify1, native stdout30, fallback=null");
            }
        })
        .join()
        .unwrap();
        return;
    }
    let mut profiles = vec!["frontend", "clang-O0", "clang-O2"];
    if cfg!(feature = "llvm-api") {
        profiles.extend(["llvm-api-O0", "llvm-api-O2"]);
    }
    for profile in profiles {
        let child = Command::new("timeout")
            .args(["-s", "KILL", "90s"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "large_copy_enum_error_default_stack_api_reliability",
                "--nocapture",
                "--test-threads=1",
            ])
            .env_remove("RUST_MIN_STACK")
            .env("ZIL_344_STACK_CHILD", profile)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{profile}: {} {}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        eprint!("{}", String::from_utf8_lossy(&child.stderr));
    }
}

#[test]
fn ieee_float_source_ct_and_native_agree() {
    let text =
        include_str!("../../../tests/language-readiness/terminal/344/floats/ieee-native.zil");
    review329_native(&[("main", text)], 344);
}

#[test]
fn ieee_float_c_scalar_and_borrowed_pod_abi() {
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/344/floats/c-abi.zil"
    ));
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let verified = zil_verify::verify_module(typed, plan, &target, &limits).unwrap();
    let lir = zil_lower::lower(&verified).unwrap();
    let llvm = zil_backend_llvm::emit(&verified, &lir).unwrap();
    assert!(llvm.contains("declare double @consume(float"), "{llvm}");
    let out = Temp::new();
    let ll = out.0.join("float.ll");
    fs::write(&ll, llvm).unwrap();
    for opt in ["-O0", "-O2"] {
        for wrong in [false, true] {
            let probe = out.0.join("consumer.c");
            let declaration = if wrong {
                "extern double inspect(double,double) __asm__(\"zil.fn.1\");"
            } else {
                "extern double inspect(float,double) __asm__(\"zil.fn.1\");"
            };
            fs::write(&probe,format!("#include <stddef.h>\nstruct Pair{{float a;double b;}};_Static_assert(sizeof(struct Pair)==16,\"size\");_Static_assert(offsetof(struct Pair,b)==8,\"offset\");double consume(float a,double b,const struct Pair*p){{return (double)a+b+(double)p->a+p->b;}}{declaration}extern float rounded(float) __asm__(\"zil.fn.2\");int main(void){{return inspect(1.25,2.5)==7.5 && rounded(16777216.0f)==16777216.0f ? 0:9;}}\n")).unwrap();
            let exe = out.0.join("probe");
            let built = Command::new("clang")
                .arg(opt)
                .arg(&ll)
                .arg(&probe)
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(built.status.success(), "{built:?}");
            let ran = Command::new(&exe).output().unwrap();
            assert_eq!(
                ran.status.code(),
                Some(if wrong { 9 } else { 0 }),
                "{opt}/{wrong}: {ran:?}"
            );
        }
    }
}

#[test]
fn ieee_float_artifact_ct_reification_preserves_ieee_values() {
    review329_native(
        &[(
            "main",
            include_str!("../../../tests/language-readiness/terminal/344/floats/ct-artifact.zil"),
        )],
        344,
    );
}

#[test]
fn ieee_float_aggregate_closure_and_function_pointer_are_native() {
    review329_native(
        &[(
            "main",
            include_str!(
                "../../../tests/language-readiness/terminal/344/floats/aggregate-closure.zil"
            ),
        )],
        344,
    );
}

#[test]
fn ieee_float_rejects_invalid_source_and_checked_ct_conversions() {
    for text in [
        "fn unused()->f32{return 1e40f32}fn result()->i32{return 0}",
        "fn result()->f64{return 1.0f32+2.0f64}",
        "fn result()->f64{return 1.0+2}",
        "fn result()->f32{return 1.0f64}",
        "fn result()->f32{return 1.0f32%2.0f32}",
        "fn result()->f64{return ~1.0}",
        "fn result()->f32{return f32(true)}",
        "fn unused<T:Copy>(x:T)->f32{return 1e40f32}fn result()->i32{return 0}",
        "extern \"C\" fn invalid()->&f32;fn result()->i32{return 0}",
        "fn result()->i32{return #(i32(1.5))}",
        "fn result()->i32{return #(i32(0.0/0.0))}",
        "fn result()->i32{return #(i32(1.0/0.0))}",
        "fn result()->i32{return #(i32(2147483648.0))}",
        "fn result()->i64{return #(i64(9223372036854775808.0))}",
        "fn result()->u8{return #(u8(-1.0))}",
        "fn result()->f32{return #(f32(3.5e38))}",
    ] {
        let out = Temp::new();
        let executable = out.0.join("rejected");
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Build,
                output: Some(executable.clone()),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
        assert!(!executable.exists());
        assert!(report.native_codegen.is_none());
        assert!(!report.diagnostics.is_empty());
    }
}

#[test]
fn ieee_float_raw_and_wire_checks_are_independent() {
    use zil_ir::{BinaryOp, Constant};
    let input = source("fn cast(x:f64)->i32{return i32(x)}fn calc(x:f64)->f64{return x+1.0}");
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&bytes, &target, &limits)
            .unwrap()
            .module(),
        &typed
    );
    for attack in 0..4 {
        let mut damaged = typed.clone();
        match attack {
            0 | 1 => {
                let function = &mut damaged.functions[0];
                let instruction = function
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(i.kind, InstructionKind::ConvertNumber { .. }))
                    .unwrap();
                let InstructionKind::ConvertNumber { value, target } = instruction.kind.clone()
                else {
                    unreachable!()
                };
                if attack == 0 {
                    instruction.kind = InstructionKind::ConvertInteger { value, target };
                } else {
                    instruction.kind = InstructionKind::ConvertNumber {
                        value,
                        target: Type::Bool,
                    };
                    function.values[instruction.result.unwrap().0 as usize] = Type::Bool;
                }
            }
            2 => {
                for instruction in damaged.functions[1]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                {
                    if let InstructionKind::Constant(c) = &mut instruction.kind {
                        *c = Constant::F32(0x3f800000);
                        break;
                    }
                }
            }
            _ => {
                for instruction in damaged.functions[1]
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                {
                    if let InstructionKind::Binary { op, .. } = &mut instruction.kind {
                        *op = BinaryOp::BitAnd;
                        break;
                    }
                }
            }
        }
        let mut claims = plan.clone();
        claims.module_fingerprint = damaged.fingerprint();
        assert!(
            zil_verify::verify_module(damaged.clone(), claims.clone(), &target, &limits).is_err(),
            "attack {attack}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&damaged, &claims, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err(),
            "wire attack {attack}"
        );
    }
    let mut negative_zero = typed.clone();
    let mut positive_zero = typed.clone();
    for (module, bits) in [
        (&mut negative_zero, 0x8000000000000000),
        (&mut positive_zero, 0),
    ] {
        for instruction in module.functions[1]
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
        {
            if let InstructionKind::Constant(c) = &mut instruction.kind {
                *c = Constant::F64(bits);
                break;
            }
        }
    }
    assert_ne!(negative_zero.fingerprint(), positive_zero.fingerprint());
}

#[test]
fn ieee_float_budget_boundary_is_request_local() {
    use zil_driver::CompilationSession;
    let input = source(
        "fn result()->i32{let x:f32=1.401298464324817070923729583289916131280e-45;if x>0.0f32{return 344}return 0}",
    );
    let options = |n| CompileOptions {
        limits: Limits {
            max_analysis_steps: n,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let mut low = 1usize;
    let mut high = 1_000_000usize;
    assert_eq!(
        compile_source(&input, &options(high)).status,
        ResultCategory::Success
    );
    while low < high {
        let middle = low + (high - low) / 2;
        let report = compile_source(&input, &options(middle));
        match report.status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            _ => panic!("{}", report.to_json()),
        }
    }
    let mut warm = CompilationSession::default();
    assert_eq!(
        warm.compile_source(&input, &options(low)).status,
        ResultCategory::Success
    );
    for (n, status) in [
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        assert_eq!(warm.compile_source(&input, &options(n)).status, status);
        assert_eq!(
            CompilationSession::default()
                .compile_source(&input, &options(n))
                .status,
            status
        );
        assert_eq!(warm.active_query_count(), 0);
    }
    println!("344 float source analysis-work exact N={low}; N-1 ResourceLimit, warm/fresh agree");
}

#[test]
fn ieee_float_runtime_conversions_trap_at_o0_and_o2() {
    use std::os::unix::process::ExitStatusExt;
    use zil_driver::{NativeBackend, Optimization};
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            for (target, value) in [
                ("i32", "i32(1.5)"),
                ("i64", "i64(9223372036854775808.0)"),
                ("i32", "i32(1.0/0.0)"),
                ("i32", "i32(0.0/0.0)"),
                ("u8", "u8(-1.0)"),
                ("u64", "u64(18446744073709551616.0)"),
                ("usize", "usize(18446744073709551616.0)"),
                ("i32", "i32(f32(3.5e38))"),
            ] {
                let out = Temp::new();
                let text = format!("fn result()->{target}{{return {value}}}");
                let report = compile_source(
                    &source(&text),
                    &CompileOptions {
                        backend,
                        optimization,
                        stop_after: Stage::Build,
                        entry: Some("result".into()),
                        output: Some(out.0.join("trap")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{text}: {}",
                    report.to_json()
                );
                assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
                let observed = Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert!(
                    observed.status.signal().is_some(),
                    "{backend:?}/{optimization:?}/{value}: {observed:?}"
                );
                assert!(observed.stdout.is_empty());
                let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
                assert!(llvm.contains("@llvm.trap"));
                if value.starts_with("i64(") {
                    let guard = llvm.find("fcmp olt double").unwrap();
                    let cast = llvm.find("fptosi double").unwrap();
                    assert!(guard < cast);
                }
            }
        }
    }
}

#[test]
fn ieee_float_unsigned_boundaries_are_ct_and_native() {
    review329_native(
        &[(
            "main",
            include_str!(
                "../../../tests/language-readiness/terminal/344/floats/unsigned-boundaries.zil"
            ),
        )],
        344,
    );
    for target in ["u64", "usize"] {
        let text = format!("fn result()->{target}{{return #({target}(18446744073709551616.0))}}");
        let report = compile_source(&source(&text), &CompileOptions::default());
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
                .any(|d| d.code == "E-NUMERIC-CONVERSION")
        );
        assert!(report.native_codegen.is_none());
    }
}

#[test]
fn ieee_float_usize_conversions_use_target_width_in_both_ct_vms() {
    let target = TargetProfile {
        id: "float-usize32".into(),
        triple: "i686-unknown-linux-gnu".into(),
        pointer_width: 32,
        ..TargetProfile::host_llvm()
    };
    for text in [
        "fn result()->usize{return #(usize(4294967295.0))}",
        "fn cast(t:Type)->usize{return usize(4294967295.0)}fn result()->usize{return #(cast(i32))}",
        "fn cast(t:Type)->f64{return f64(4294967295usize)}fn result()->f64{return #(cast(i32))}",
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                target: target.clone(),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{text}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    for text in [
        "fn result()->usize{return #(usize(4294967296.0))}",
        "fn result()->usize{return #(usize(4294967295.0f32))}",
        "fn cast(t:Type)->usize{return usize(4294967296.0)}fn result()->usize{return #(cast(i32))}",
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                target: target.clone(),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{text}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-NUMERIC-CONVERSION")
        );
        assert_eq!(report.verification_runs, 0);
        assert!(report.native_codegen.is_none());
    }
}

#[test]
fn anonymous_sum_source_and_native() {
    let text = include_str!("../../../tests/language-readiness/terminal/344/anonymous/basic.zil");
    review329_native(&[("main", text)], 42);
}

#[test]
fn anonymous_sum_aliases_generic_copy_and_full_member_identity() {
    let text = include_str!("../../../tests/language-readiness/terminal/344/anonymous/aliases.zil");
    review329_native(&[("main", text)], 42);
}

#[test]
fn anonymous_sum_ct_type_order_and_value_reification() {
    let text =
        include_str!("../../../tests/language-readiness/terminal/344/anonymous/ct-types.zil");
    review329_native(&[("main", text)], 42);
}

#[test]
fn anonymous_sum_rejects_duplicate_inexact_and_incomplete_source() {
    for text in [
        "fn unused(x:i32|i32){}fn result()->i32{return 0}",
        "type Bad=i32|i32;fn result()->i32{return 0}",
        "type Bad<T>=T|T;fn result()->i32{return 0}",
        "type f32=i32;fn result()->i32{return 0}",
        "type f64=i32;fn result()->i32{return 0}",
        "type Int=i32;type Bad=Int|i32;fn result()->i32{return 0}",
        "fn result()->i64|bool{return 42}",
        "fn result()->i32|bool{return 1u8}",
        "fn bad(x:i32|bool)->i32{match x{i64(n)=>{return 0},bool(b)=>{return 1}}}fn result()->i32{return 0}",
        "fn bad(x:i32|bool)->i32{match x{i32(n)=>{return n}}}fn result()->i32{return 0}",
        "fn bad(x:i32|bool)->i32{match x{i32(n)=>{return n},i32(n)=>{return n},bool(b)=>{return 0}}}fn result()->i32{return 0}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert!(
            matches!(report.status, ResultCategory::SafeReject),
            "{text}: {}",
            report.to_json()
        );
        assert!(report.native_codegen.is_none());
    }
}

#[test]
fn anonymous_sum_raw_canonical_shape_and_wire_are_independently_checked() {
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/basic.zil"
    ));
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    assert_eq!(
        zil_verify::verify_module_bytes(&bytes, &target, &limits)
            .unwrap()
            .module(),
        &typed
    );
    for attack in 0..5 {
        let mut damaged = typed.clone();
        let Type::Enum { name, variants } = &mut damaged.functions[0].return_type else {
            panic!("sum result")
        };
        match attack {
            0 => {
                variants.swap(0, 1);
                for (i, (name, _)) in variants.iter_mut().enumerate() {
                    *name = format!("Member{i}");
                }
            }
            1 => variants[1].1 = variants[0].1.clone(),
            2 => variants[0].0 = "Invented".into(),
            3 => *name = "@anonymous-sum:untrusted".into(),
            _ => {
                let nested = Type::Enum {
                    name: name.clone(),
                    variants: variants.clone(),
                };
                variants[1].1 = vec![nested];
            }
        }
        assert!(
            zil_analyze::analyze(&damaged, &limits).is_err(),
            "analysis attack {attack}"
        );
        let mut claims = plan.clone();
        claims.module_fingerprint = damaged.fingerprint();
        assert!(
            zil_verify::verify_module(damaged.clone(), claims.clone(), &target, &limits).is_err(),
            "raw attack {attack}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&damaged, &claims, &limits).unwrap();
        assert!(
            zil_verify::verify_module_bytes(&wire, &target, &limits).is_err(),
            "Wire attack {attack}"
        );
    }
}

#[test]
fn anonymous_sum_nominal_phantom_arguments_stay_distinct_across_ct_and_runtime() {
    let text = include_str!("../../../tests/language-readiness/terminal/344/anonymous/phantom.zil");
    review329_native(&[("main", text)], 42);
    let swapped = text.replace(
        "type Integer=Phantom<i32>;\ntype Boolean=Phantom<bool>;",
        "type Boolean=Phantom<bool>;\ntype Integer=Phantom<i32>;",
    );
    assert_ne!(text, swapped);
    review329_native(&[("main", &swapped)], 42);
}

#[test]
fn anonymous_sum_imported_member_aliases() {
    review329_native(
        &[
            (
                "main",
                "import lib as m;fn choose(flag:bool)->m.Left|m.Right{if flag{return m.Left{value:20}}return m.Right{value:22}}fn score(x:m.Right|m.Left)->i32{match x{m.L(l)=>{return l.value},m.R(r)=>{return r.value}}}fn result()->i32{return score(choose(true))+score(choose(false))}",
            ),
            (
                "lib",
                "pub struct Left{pub value:i32}pub struct Right{pub value:i32}pub type L=Left;pub type R=Right;",
            ),
        ],
        42,
    );
}

#[test]
fn anonymous_sum_budget_boundary_is_request_local() {
    use zil_driver::CompilationSession;
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct-types.zil"
    ));
    let options = |n| CompileOptions {
        limits: Limits {
            max_analysis_steps: n,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let (mut low, mut high) = (1usize, 1_000_000usize);
    assert_eq!(
        compile_source(&input, &options(high)).status,
        ResultCategory::Success
    );
    while low < high {
        let mid = low + (high - low) / 2;
        let report = compile_source(&input, &options(mid));
        match report.status {
            ResultCategory::Success => high = mid,
            ResultCategory::ResourceLimit => low = mid + 1,
            _ => panic!("{}", report.to_json()),
        }
    }
    let mut warm = CompilationSession::default();
    assert_eq!(
        warm.compile_source(&input, &options(low)).status,
        ResultCategory::Success
    );
    for (n, status) in [
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(n)),
            CompilationSession::default().compile_source(&input, &options(n)),
        ] {
            assert_eq!(report.status, status, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!("344 anonymous source exact N={low}; N-1 ResourceLimit and warm/fresh agree");
}

#[test]
fn anonymous_sum_generic_member_patterns_keep_complete_substitution() {
    review329_native(
        &[(
            "main",
            "fn member<T:Copy>(x:T|bool)->i32{match x{T(value)=>{return 20},bool(b)=>{return 2}}}fn result()->i32{let a:i32|bool=1;let b:(i32,bool)|bool=(1,false);return member::<i32>(a)+member::<(i32,bool)>(b)+member::<i32>(false)}",
        )],
        42,
    );
    let bad = "fn member<T:Copy>(x:T|bool)->i32{match x{T(value)=>{return 1},bool(b)=>{return 2}}}fn result()->i32{return member::<bool>(false)}";
    let report = compile_source(&source(bad), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn anonymous_sum_float_unsigned_nested_generic_enum_layout() {
    review329_native(
        &[(
            "main",
            include_str!("../../../tests/language-readiness/terminal/344/anonymous/float-enum.zil"),
        )],
        42,
    );
}

#[test]
fn anonymous_sum_generic_long_prefix_key_budget_is_request_local() {
    use zil_driver::CompilationSession;
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/generic-key-prefix.zil"
    );
    review329_native(&[("main", text)], 42);
    let input = source(text);
    let options = |n| CompileOptions {
        limits: Limits {
            max_analysis_steps: n,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let (mut low, mut high) = (1usize, Limits::default().max_analysis_steps);
    let report = compile_source(&input, &options(high));
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    while low < high {
        let mid = low + (high - low) / 2;
        let report = compile_source(&input, &options(mid));
        match report.status {
            ResultCategory::Success => high = mid,
            ResultCategory::ResourceLimit => low = mid + 1,
            _ => panic!("{}", report.to_json()),
        }
    }
    let mut warm = CompilationSession::default();
    assert_eq!(
        warm.compile_source(&input, &options(low)).status,
        ResultCategory::Success
    );
    for (n, status) in [
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(n)),
            CompilationSession::default().compile_source(&input, &options(n)),
        ] {
            assert_eq!(report.status, status, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!(
        "344 nested/generic common-prefix key exact N={low}; N-1 ResourceLimit and warm/fresh agree"
    );
}

#[test]
fn anonymous_sum_const_associated_and_borrowed_code_compositions() {
    for text in [
        "fn keep<T:Copy,#N:u64>(value:T)->T{return value}fn result()->i32{let value:i32|bool=42;let kept=keep::<i32|bool,18446744073709551615u64>(value);match kept{i32(n)=>{return n},bool(b)=>{return 0}}}",
        "trait Read{type Item:Copy;fn read(self:&Self)->Self.Item;}struct Box<T:Copy>{value:T}impl<T:Copy> Read for Box<T>{type Item=T;fn read()->Self.Item{return .value}}fn get<T:Read>(value:&T)->T.Item{return value.read()}fn result()->i32{let value:i32|bool=42;let box=Box::<i32|bool>{value:value};let found=get::<Box<i32|bool>>(&box);match found{i32(n)=>{return n},bool(b)=>{return 0}}}",
        "type Handler=fn(&i32)->i32;type Choice=Handler|bool;type Reverse=bool|Handler;fn selected()->Type{if Choice==Reverse{return Choice}return bool}fn read(value:&i32)->i32{return *value}fn result()->i32{let f:Handler=read;let choice:#(selected())=f;match choice{Handler(code)=>{let n=42;return code(&n)},bool(b)=>{return 0}}}",
    ] {
        review329_native(&[("main", text)], 42);
    }
}

#[test]
fn nested_conversions_with_unrelated_generic_methods_keep_default_stack() {
    if std::env::var_os("ZIL_340_METHOD_STACK_CHILD").is_some() {
        std::thread::spawn(|| {
            let prefix="struct P{}trait Echo{fn echo<T:Copy>(self:&Self,x:T)->T{return x}}impl Echo for P{}";
            for (callee, depth) in [("i32",32),("i32",96),("i32",124),("((i32))",32)] {
                let expression=format!("{}0{}",format!("{callee}(").repeat(depth),")".repeat(depth));
                let text=format!("{prefix}fn result()->i32{{return {expression}}}");
                let report=compile_source(&source(&text),&CompileOptions::default());
                assert_eq!(report.status,ResultCategory::Success,"{callee}/{depth}: {}",report.to_json());
                assert_eq!(report.verification_runs,1);
            }
        }).join().unwrap();
        return;
    }
    let child = Command::new("timeout")
        .args(["-s", "KILL", "90s"])
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "nested_conversions_with_unrelated_generic_methods_keep_default_stack",
            "--nocapture",
        ])
        .env("ZIL_340_METHOD_STACK_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
}

#[test]
fn nested_calls_reach_verify_or_resource_limit_on_default_worker_stack() {
    if let Ok(mode) = std::env::var("ZIL_344_NESTED_CHILD") {
        std::thread::spawn(move || {
            let call = match mode.as_str() {
                "generic" => "same",
                "conversion" => "i32",
                "f32" => "f32",
                "f64" => "f64",
                _ => "plain",
            };
            let result = match mode.as_str() { "f32" => "f32", "f64" => "f64", _ => "i32" };
            for depth in [32, 64, 96, 112, 124, 126] {
                let expression = format!("{}0{}", format!("{call}(").repeat(depth), ")".repeat(depth));
                let text = format!("fn same<T:Copy>(x:T)->T{{return x}}fn plain(x:i32)->i32{{return x}}fn result()->{result}{{return {expression}}}");
                let report = compile_source(&source(&text), &CompileOptions::default());
                let expected = if depth == 126 { ResultCategory::ResourceLimit } else { ResultCategory::Success };
                assert_eq!(report.status, expected, "{mode}/{depth}: {}", report.to_json());
                assert_eq!(report.verification_runs, usize::from(depth != 126));
                if depth == 126 { assert_eq!(report.phase, Phase::Parse); }
                if mode == "plain" && depth == 96 {
                    for (nodes, n) in [(true, 2348), (false, 197)] {
                        let mut session = zil_driver::CompilationSession::default();
                        for cap in [n, n - 1, n] {
                            let mut options = CompileOptions::default();
                            if nodes { options.limits.max_nodes = cap; }
                            else { options.limits.max_call_depth = cap; }
                            let bounded = session.compile_source(&source(&text), &options);
                            let expected = if cap == n { ResultCategory::Success } else { ResultCategory::ResourceLimit };
                            assert_eq!(bounded.status, expected, "nodes={nodes},N={n},cap={cap}: {}", bounded.to_json());
                        }
                        let mut options = CompileOptions::default();
                        if nodes { options.limits.max_nodes = n; }
                        else { options.limits.max_call_depth = n; }
                        let fresh = compile_source(&source(&text), &options);
                        assert_eq!(fresh.status, ResultCategory::Success, "{}", fresh.to_json());
                    }
                }
            }
        }).join().unwrap();
        return;
    }
    for mode in ["plain", "generic", "conversion", "f32", "f64"] {
        let child = Command::new("timeout")
            .args(["-s", "KILL", "90s"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "nested_calls_reach_verify_or_resource_limit_on_default_worker_stack",
                "--nocapture",
                "--test-threads=1",
            ])
            .env_remove("RUST_MIN_STACK")
            .env("ZIL_344_NESTED_CHILD", mode)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{mode}: {} {}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
    }
}

#[test]
fn ct3_anonymous_artifact_value_source_bridge() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/artifact-values.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    eprintln!("{}", report.to_json());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native(&[("main", text)], 42);
}
#[test]
fn ct3_enum_match_checks_actual_owner_source() {
    let legal = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct-enum-owner-legal.zil"
    );
    let report = compile_source(&source(legal), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let wrong =
        include_str!("../../../tests/language-readiness/codegen-pending/ct3-wrong-enum-owner.zil");
    let report = compile_source(&source(wrong), &CompileOptions::default());
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
            .any(|d| d.code == "E-CT-SOURCE" && d.message.contains("owner")),
        "{}",
        report.to_json()
    );
}

#[test]
fn ct4_fixed_phantom_context_source_bridge() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-fixed-context.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native_entry_output(&[("main", text)], "main", b"42\n");
    for extra in [
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-joint-context.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-generic-context.zil"
        ),
    ] {
        let report = compile_source(&source(extra), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        review329_native_entry_output(&[("main", extra)], "main", b"42\n");
    }
    let joint = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-joint-context.zil"
    );
    let wrong = joint.replace("Pair::<'a,'b>", "Pair::<'a,'a>");
    let report = compile_source(&source(&wrong), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    let free = text.replace("Phantom::<'s>", "Phantom::<'free>");
    let report = compile_source(&source(&free), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    let escape = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-fixed-context-escape.zil"
    );
    let report = compile_source(&source(escape), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn ct4_mixed_nominal_context_preserves_full_source_ordinals() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-mixed-context.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native_entry_output(&[("main", text)], "main", b"42\n");
    let wrong = text.replace("Phantom::<'s,i32,7>", "Phantom::<'s,i32,8>");
    let report = compile_source(&source(&wrong), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn ct4_phantom_source_work_neighbors_remain_checked_after_cache_reuse() {
    use zil_driver::CompilationSession;
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-fixed-context.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-generic-context.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-mixed-context.zil"
        ),
    ] {
        let input = source(text);
        let options = |n| CompileOptions {
            limits: Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
            ..CompileOptions::default()
        };
        let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
        assert_eq!(
            compile_source(&input, &options(high)).status,
            ResultCategory::Success
        );
        while low < high {
            let middle = (low + high) / 2;
            let report = compile_source(&input, &options(middle));
            match report.status {
                ResultCategory::Success => high = middle,
                ResultCategory::ResourceLimit => low = middle + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut warm = CompilationSession::default();
        assert_eq!(
            warm.compile_source(&input, &options(low)).status,
            ResultCategory::Success
        );
        for (n, status) in [
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
            (low, ResultCategory::Success),
        ] {
            for report in [
                warm.compile_source(&input, &options(n)),
                CompilationSession::default().compile_source(&input, &options(n)),
            ] {
                assert_eq!(report.status, status, "{}", report.to_json());
            }
            assert_eq!(warm.active_query_count(), 0);
        }
        eprintln!("344 CT4 whole-source analysis N={low}/N-1/warm/fresh");
    }
}

#[test]
fn ct4_phantom_source_default_worker_subprocess() {
    const CHILD: &str = "ZIL_CT4_PHANTOM_DEFAULT_CHILD";
    if std::env::var_os(CHILD).is_some() {
        for text in [
            include_str!(
                "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-fixed-context.zil"
            ),
            include_str!(
                "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-joint-context.zil"
            ),
            include_str!(
                "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-generic-context.zil"
            ),
            include_str!(
                "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-mixed-context.zil"
            ),
        ] {
            let report = compile_source(&source(text), &CompileOptions::default());
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
        }
        return;
    }
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "ct4_phantom_source_default_worker_subprocess",
            "--nocapture",
            "--test-threads=1",
        ])
        .env_remove("RUST_MIN_STACK")
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "default worker subprocess: {:?}\n{}\n{}",
        child.status,
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
}

#[test]
fn ct5_invocation_scoped_nominal_metadata_reaches_native() {
    use zil_expand::source_context::{DeclarationPlacement, SourceIdentityContext};
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct4-phantom-invocation-metadata.zil"
    );
    let outcome = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        outcome.status,
        ResultCategory::Success,
        "{}",
        outcome.to_json()
    );
    review329_native_entry_output(&[("main", text)], "main", b"42\n");

    // The same ordinary runtime projection, without CT evaluation, supplies
    // raw candidates for independent index/result checks in both kernels.
    let unit = source(&text.replace("#(make())", "make()"));
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let aliases = Default::default();
    let core = zil_expand::expand_with_map_limits_target_context(
        &unit,
        &syntax,
        &limits,
        &target,
        SourceIdentityContext::ClosedSnapshot {
            root: unit.module_id,
            aliases: &aliases,
            declarations: DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    for wrong_result in [false, true] {
        let mut raw = typed.clone();
        let function = raw
            .functions
            .iter_mut()
            .find(|f| {
                f.blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .any(|i| matches!(i.kind, InstructionKind::ExtractField { .. }))
            })
            .unwrap();
        let instruction = function
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::ExtractField { .. }))
            .unwrap();
        if wrong_result {
            function.values[instruction.result.unwrap().0 as usize] = Type::I64;
        } else if let InstructionKind::ExtractField { index, .. } = &mut instruction.kind {
            *index = u32::MAX;
        }
        assert!(zil_analyze::analyze(&raw, &limits).is_err());
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        assert!(zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).is_err());
        let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
    }
}

#[test]
fn ct3_phantom_callable_metadata_stays_separate_from_data() {
    let function = include_str!(
        "../../../tests/language-readiness/codegen-pending/ct3-phantom-fn-metadata.zil"
    );
    let borrowed = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/phantom-bfp-metadata.zil"
    );
    for (text, expected) in [(function, 7), (borrowed, 42)] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        review329_native(&[("main", text)], expected);
    }
    let wrong = function.replace(
        "let value=#(choose(i32))",
        "let value:Phantom<fn(bool)->i32>=#(choose(i32))",
    );
    let report = compile_source(&source(&wrong), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn ct3_generic_enum_values_retain_complete_owner_and_active_payload() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct-generic-enum-values.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native(&[("main", text)], 42);
}

#[test]
fn ct3_generic_callable_arguments_infer_and_reify_full_type_metadata() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct-generic-callable-arguments.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native(&[("main", text)], 42);
    let wrong = text.replace("=first;", "=second;");
    let report = compile_source(&source(&wrong), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn ct3_nominal_arguments_keep_ordered_type_and_scalar_identity() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct-scalar-nominal-arguments.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native(&[("main", text)], 42);
    for bad in [
        text.replace("fn(i32)->i32,7,true", "fn(i32)->i32,7u64,true"),
        text.replace(
            "Stamp::<fn(i32)->i32,7,true>",
            "Stamp::<fn(i32)->i32,9,true>",
        ),
    ] {
        let report = compile_source(&source(&bad), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn ct3_composite_callable_metadata_does_not_construct_pointer_data() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct-composite-callable-metadata.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native(&[("main", text)], 42);
    let physical = "struct Code{call:fn(i32)->i32}fn identity(n:i32)->i32{return n}fn make(t:Type)->Code{return Code{call:identity}}fn result()->i32{let value=#(make(i32));return value.call(42)}";
    let report = compile_source(&source(physical), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn ct3_callable_metadata_alpha_order_and_free_life_boundaries() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/phantom-bfp-alpha.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native(&[("main", text)], 42);
    for bad in [
        text.replace("value:Phantom<Alpha>", "value:Phantom<Second>"),
        text.replace(
            "fn(&'x i32,&'y i32)->&'x i32",
            "fn(&'free i32,&'y i32)->&'x i32",
        ),
    ] {
        let report = compile_source(&source(&bad), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn ct3_member_tag_is_remapped_by_full_type_across_stage_order() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct-member-order-permutation.zil"
    );
    let reordered = text.replace(
        "struct Z{value:i32}\nstruct A{value:i32}",
        "struct A{value:i32}\nstruct Z{value:i32}",
    );
    for text in [text, reordered.as_str()] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        review329_native(&[("main", text)], 42);
    }
}

#[test]
fn ct5_phantom_context_query_reaches_native() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-phantom-context-query.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native_entry_output(&[("main", text)], "main", b"42\n");
}

#[test]
fn ct5_member_own_life_context_reaches_native() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-member-context.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native_entry_output(&[("main", text)], "main", b"42\n");
    use zil_driver::CompilationSession;
    let input = source(text);
    let options = |n| CompileOptions {
        limits: Limits {
            max_analysis_steps: n,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
    while low < high {
        let middle = (low + high) / 2;
        let report = compile_source(&input, &options(middle));
        match report.status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            _ => panic!("{}", report.to_json()),
        }
    }
    let mut warm = CompilationSession::default();
    assert_eq!(
        warm.compile_source(&input, &options(low)).status,
        ResultCategory::Success
    );
    for (n, expected) in [
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(n)),
            CompilationSession::default().compile_source(&input, &options(n)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    eprintln!("G4.4 member source whole analysis N={low}/N-1/warm/fresh");
    let generated = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-generated-member.zil"
    );
    let report = compile_source(&source(generated), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(report.diagnostics.iter().any(|d| {
        d.message
            .contains("transformed declarations require explicit source lineage")
    }));
    let scalar_generated = generated
        .replace("struct Phantom<life s>{n:i32}\n", "")
        .replace("<life s>", "")
        .replace("&'s i32", "&i32");
    let report = compile_source(&source(&scalar_generated), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native_entry_output(&[("main", &scalar_generated)], "main", b"42\n");
    let host = text
        .replace("struct Reader {", "struct Reader<X:Copy> {")
        .replace("Reader{}", "Reader::<i32>{}");
    let report = compile_source(&source(&host), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("actual enclosing generic context"))
    );
    let mixed = text
        .replace(
            "result<life s>(self:Self,p:",
            "result<T:Copy,life s>(self:Self,marker:T,p:",
        )
        .replace("reader.result(&n)", "reader.result(7,&n)");
    let report = compile_source(&source(&mixed), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message == "generic function specialization")
    );
    let wrong = text.replace("Phantom::<'s>", "Phantom::<'other>");
    let report = compile_source(&source(&wrong), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[test]
fn g442_mixed_ct_scalar_call_actuals() {
    let text = r#"
struct Phantom<life a,T:Copy,#N:i32>{n:i32}
fn choose<T:Copy,#N:i32,life a>(marker:Type)->i32{return N}
fn score(a:i32,b:i32)->i32{if a!=7{return 1}if b!=35{return 2}return 42}
fn result<life s,life t>(p:&'s i32,q:&'t i32)->i32{
    let value:Phantom<'s,i32,7>=#(Phantom::<'s,i32,7>{
        n:score(choose::<i32,7,'s>(i32),choose::<bool,35,'t>(bool))
    });
    return value.n
}

fn main()->i32{let n=0;let m=0;return result(&n,&m)}
"#;
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    review329_native_entry_output(&[("main", text)], "main", b"42\n");
    for (bad, expected) in [
        (
            text.replace("return N}", "return N+false}"),
            "actual expected type",
        ),
        (
            text.replace("choose::<i32,7,'s>", "choose::<i32,7,'other>"),
            "authentic caller",
        ),
        (
            text.replace("choose::<i32,7,'s>", "choose::<i32,7>"),
            "arity differs",
        ),
        (
            text.replace("choose::<i32,7,'s>", "choose::<i32,true,'s>"),
            "scalar actual",
        ),
    ] {
        let report = compile_source(&source(&bad), &CompileOptions::default());
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
                .any(|d| d.message.contains(expected)),
            "{}",
            report.to_json()
        );
    }
    let unused = text.replace("return N}", "return N+false}").replace(
        "n:score(choose::<i32,7,'s>(i32),choose::<bool,35,'t>(bool))",
        "n:42",
    );
    let report = compile_source(&source(&unused), &CompileOptions::default());
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
            .any(|d| d.message.contains("actual expected type"))
    );
}

#[test]
fn g442_mixed_ct_phantom_call_actuals() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-mixed-ct-instance-values.zil"
    );
    let permuted = text
        .replace(
            "fn make<T:Copy,#N:i32,life a>",
            "fn make<life a,#N:i32,T:Copy>",
        )
        .replace("make::<i32,7,'s>", "make::<'s,7,i32>")
        .replace("make::<bool,35,'t>", "make::<'t,35,bool>");
    for input in [text, permuted.as_str()] {
        let report = compile_source(&source(input), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        review329_native_entry_output(&[("main", input)], "main", b"42\n");
    }
    // The concrete call already supplies i32/7. Those actuals cannot prove
    // the generic equation for arbitrary T/N, including an unused definition.
    let minimal = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-mixed-ct-factory.zil"
    );
    for input in [
        minimal.replace("return Phantom::<'a,T,N>", "return Phantom::<'a,i32,N>"),
        minimal.replace("return Phantom::<'a,T,N>", "return Phantom::<'a,T,7>"),
        minimal
            .replace("return Phantom::<'a,T,N>", "return Phantom::<'a,i32,N>")
            .replace(
                "fn main()->i32{let n=0;return result(&n)}",
                "fn main()->i32{return 0}",
            ),
    ] {
        let report = compile_source(&source(&input), &CompileOptions::default());
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
                .any(|d| d.message.contains("actual expected type")),
            "{}",
            report.to_json()
        );
    }
    let wrong_life = minimal
        .replace(
            "fn make<T:Copy,#N:i32,life a>",
            "fn make<life a,life b,T:Copy,#N:i32>",
        )
        .replace("make::<i32,7,'s>", "make::<'s,'s,i32,7>")
        .replace("return Phantom::<'a,T,N>", "return Phantom::<'b,T,N>");
    for input in [
        wrong_life.clone(),
        wrong_life.replace(
            "fn main()->i32{let n=0;return result(&n)}",
            "fn main()->i32{return 0}",
        ),
    ] {
        let report = compile_source(&source(&input), &CompileOptions::default());
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
                .any(|d| d.message.contains("actual expected type")),
            "{}",
            report.to_json()
        );
    }
    let shadowed = minimal.replace(
        "return Phantom::<'a,T,N>",
        "let N=7;return Phantom::<'a,T,N>",
    );
    for input in [
        shadowed.clone(),
        shadowed.replace("let N=7", "var N=7"),
        shadowed.replace(
            "fn main()->i32{let n=0;return result(&n)}",
            "fn main()->i32{return 0}",
        ),
    ] {
        let report = compile_source(&source(&input), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("scalar-generic shadowing")),
            "{}",
            report.to_json()
        );
    }
    // Existing runtime const-generic local value shadowing keeps its ordinary
    // accepted behavior; only the new CT mixed-instance profile is fenced.
    for input in [
        "fn scoped<#N:i32>()->i32{let N:i32=8;return N}fn main()->i32{return scoped::<7>()-8}",
        "fn scoped<#N:i32>()->i32{let N:i32=8;let p=&N;return *p}fn main()->i32{return scoped::<7>()-8}",
        "fn scoped<#N:i32>(N:i32)->i32{return N}fn main()->i32{return scoped::<7>(8)-8}",
    ] {
        let report = compile_source(&source(input), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

fn g44_source_request_boundary(input: &str, ledger: &str) -> usize {
    use zil_driver::CompilationSession;
    let options = |n: usize| {
        let mut options = CompileOptions::default();
        match ledger {
            "work" => options.limits.max_analysis_steps = n,
            "nodes" => options.limits.max_nodes = n,
            "steps" => options.limits.max_ct_steps = n as u64,
            "bytes" => options.limits.max_source_bytes = n,
            _ => unreachable!(),
        }
        options
    };
    let limits = Limits::default();
    let mut low = 1;
    let mut high = match ledger {
        "work" => limits.max_analysis_steps,
        "nodes" => limits.max_nodes,
        "steps" => limits.max_ct_steps as usize,
        "bytes" => limits.max_source_bytes,
        _ => unreachable!(),
    };
    let input = source(input);
    assert_eq!(
        compile_source(&input, &options(high)).status,
        ResultCategory::Success
    );
    while low < high {
        let n = low + (high - low) / 2;
        let report = compile_source(&input, &options(n));
        match report.status {
            ResultCategory::Success => high = n,
            ResultCategory::ResourceLimit => low = n + 1,
            _ => panic!("{ledger}/{n}: {}", report.to_json()),
        }
    }
    let mut warm = CompilationSession::default();
    for (n, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(n)),
            CompilationSession::default().compile_source(&input, &options(n)),
        ] {
            assert_eq!(
                report.status,
                expected,
                "{ledger}/N={low}: {}",
                report.to_json()
            );
            if expected == ResultCategory::Success {
                assert_eq!(report.verification_runs, 1);
            }
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    low
}

#[test]
fn g442_source_budgets_cache_and_comparison_growth() {
    use zil_driver::CompilationSession;
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-mixed-ct-instance-values.zil"
    );
    for ledger in ["work", "nodes", "steps", "bytes"] {
        let n = g44_source_request_boundary(text, ledger);
        eprintln!("G4.4.2 whole Source {ledger} N={n}; N-1/cold/warm/fresh agree");
    }
    let deep = |calls: usize, width: usize| {
        let ty = format!(
            "({})",
            (0..width)
                .map(|i| if i % 2 == 0 { "i32" } else { "bool" })
                .collect::<Vec<_>>()
                .join(",")
        );
        let invocations = [7, 11, 35]
            .iter()
            .take(calls)
            .enumerate()
            .map(|(i, n)| {
                let life = if i % 2 == 0 { "s" } else { "t" };
                format!("make::<{ty},{n},'{life}>(i32)")
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "struct Phantom<life a,T:Copy,#N:i32>{{n:i32}}fn make<T:Copy,#N:i32,life a>(marker:Type)->Phantom<'a,T,N>{{return Phantom::<'a,T,N>{{n:N}}}}fn result<life s,life t>(p:&'s i32,q:&'t i32)->i32{{let value:Phantom<'s,{ty},7>=#(({invocations}).0);return value.n}}fn main()->i32{{let n=0;let m=0;return result(&n,&m)}}"
        )
    };
    let small = g44_source_request_boundary(&deep(2, 2), "work");
    let wide = g44_source_request_boundary(&deep(2, 8), "work");
    let many = g44_source_request_boundary(&deep(3, 8), "work");
    assert!(
        small < wide && wide < many,
        "comparison growth {small}/{wide}/{many}"
    );
    eprintln!(
        "G4.4.2 complete graph/instance work growth: 2x2={small}, 2x8={wide}, 3x8={many}; each exact N-1/fresh/warm checked"
    );
    let directory = Temp::new();
    let mut session = CompilationSession::default();
    let changed = text
        .replace("i32,7,'s", "i32,11,'s")
        .replace("Phantom<'s,i32,7>", "Phantom<'s,i32,11>")
        .replace("left.n!=7", "left.n!=11");
    let reversed = text
        .replace(
            "make::<i32,7,'s>(i32),make::<bool,35,'t>(bool)",
            "make::<bool,35,'t>(bool),make::<i32,7,'s>(i32)",
        )
        .replace(").0", ").saved")
        .replace(").1", ").0")
        .replace(").saved", ").1");
    for (ordinal, input) in [text, text, changed.as_str(), reversed.as_str(), text]
        .into_iter()
        .enumerate()
    {
        let executable = directory.0.join(format!("warm-{ordinal}"));
        let options = CompileOptions {
            stop_after: Stage::Build,
            entry: Some("main".into()),
            output: Some(executable.clone()),
            backend: zil_driver::NativeBackend::Clang,
            semantic_cache: Some(directory.0.join("cache")),
            ..CompileOptions::default()
        };
        let report = session.compile_source(&source(input), &options);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
        let observed = Command::new(executable).output().unwrap();
        assert!(observed.status.success(), "{observed:?}");
        assert_eq!(observed.stdout, b"42\n");
    }
    let store = directory.0.join("cache/semantic-v4");
    let mut candidates = 0;
    for entry in fs::read_dir(&store).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|x| x != "candidate") {
            continue;
        }
        let key =
            zil_base::Fingerprint::from_hex(path.file_stem().unwrap().to_str().unwrap()).unwrap();
        let zil_db::CacheRead::Candidate(payload) =
            zil_db::ArtifactCache::with_limit(&store, Limits::default().max_source_bytes)
                .inspect_candidate(key)
                .unwrap()
        else {
            panic!("missing actual semantic candidate")
        };
        assert_eq!(&payload[..8], b"ZILSEM04");
        let length = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
        let bytes = &payload[12..12 + length];
        let (typed, plan) =
            zil_ir::wire::decode_module_candidate(bytes, &Limits::default()).unwrap();
        zil_verify::verify_module(typed.clone(), plan, &typed.target, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(bytes, &typed.target, &Limits::default()).unwrap();
        candidates += 1;
    }
    assert!(candidates > 0);
    eprintln!(
        "G4.4.2 actual warm/change/reorder/revert native outputs42 and {candidates} semantic candidates raw+Wire reverified"
    );
}

#[test]
fn ct5_joint_subvalues_preserve_their_own_schema_contexts() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-joint-subvalues.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native_entry_output(&[("main", text)], "main", b"42\n");
    let wrong = text.replace("Pair::<'a,'b>", "Pair::<'a,'a>");
    let report = compile_source(&source(&wrong), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    let repeated = text.replace("(Pair::<'a,'b>{n:scalar(i32,20)},Pair::<'b,'a>{n:scalar(i32,0)}).0", "(Pair::<'a,'b>{n:scalar(i32,0)},(Pair::<'b,'a>{n:scalar(i32,0)},Pair::<'a,'b>{n:scalar(i32,20)})).1.1");
    let report = compile_source(&source(&repeated), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    review329_native_entry_output(&[("main", &repeated)], "main", b"42\n");
    use zil_driver::CompilationSession;
    for current in [text, repeated.as_str()] {
        let input = source(current);
        let options = |work| CompileOptions {
            limits: Limits {
                max_analysis_steps: work,
                ..Limits::default()
            },
            ..CompileOptions::default()
        };
        let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
        while low < high {
            let middle = (low + high) / 2;
            let report = compile_source(&input, &options(middle));
            match report.status {
                ResultCategory::Success => high = middle,
                ResultCategory::ResourceLimit => low = middle + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        let mut warm = CompilationSession::default();
        assert_eq!(
            warm.compile_source(&input, &options(low)).status,
            ResultCategory::Success
        );
        for (work, expected) in [
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
            (low, ResultCategory::Success),
        ] {
            for report in [
                warm.compile_source(&input, &options(work)),
                CompilationSession::default().compile_source(&input, &options(work)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
            }
            assert_eq!(warm.active_query_count(), 0);
        }
        eprintln!("G4.4.1 joint CT schema whole Source N={low}/N-1/warm/fresh");
    }
}

#[test]
fn terminal340_owner_replacement_error_arm_actual_mutable_alias() {
    let prefix = "extern \"C\" fn putchar(code:i32)->i32;resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}package P{manages owner:R;n:i32}enum F{Bad}fn make()->F!P{return P{owner:R{code:65},n:42}}";
    let text = format!(
        "{prefix}fn result()->i32{{var value=make();let input=&mut value;match *input{{.ok(ref mut p)=>{{p.owner=R{{code:66}};return p.n+p.owner.code-66}},.err(_)=>{{return 0}}}}}}"
    );
    let unknown =
        include_str!("../../../tests/language-readiness/terminal/340/replace-error-arm-alias.zil");
    let relay = unknown.replace("match *input", "let relay=input;match *relay");
    let read_old = unknown.replace("R{code:66}", "R{code:p.owner.code+1}");
    for text in [text.as_str(), unknown, relay.as_str(), read_old.as_str()] {
        let temp = Temp::new();
        let report = compile_source(
            &source(text),
            &CompileOptions {
                emit_dir: Some(temp.0.clone()),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        review329_native_output(&[("main", text)], b"AB42\n");
    }
}

#[test]
fn terminal340_owner_replacement_error_arm_alias_abort_and_pins() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/340/replace-error-arm-alias-failure.zil"
    );
    let temp = Temp::new();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            emit_dir: Some(temp.0.clone()),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let typed = fs::read_to_string(&report.artifacts["typed"]).unwrap();
    assert!(typed.contains("AbortPlaceReservation"), "{typed}");
    review329_native_output(&[("main", text)], b"BA42\n");
    let text =
        include_str!("../../../tests/language-readiness/terminal/340/replace-error-arm-alias.zil");
    for (case, neighbor) in [
        text.replace("let input=&mut value", "let input=&value"),
        text.replace(
            "p.owner=R{code:66};return p.n+p.owner.code-66",
            "let old=p.owner;p.owner=R{code:66};return old.code",
        ),
        format!(
            "fn write(p:&mut P)->i32{{p.n=0;return 66}}{}",
            text.replace("p.owner=R{code:66}", "p.owner=R{code:write(p)}")
        ),
        format!(
            "fn touch(p:&mut(F!P))->i32{{return 66}}{}",
            text.replace("p.owner=R{code:66}", "p.owner=R{code:touch(input)}")
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let report = compile_source(&source(&neighbor), &CompileOptions::default());
        let (phase, code) = if case == 0 {
            (Phase::Types, "E-ADDRESS-PERMISSION")
        } else if case == 1 {
            // Describing this addressed Sum field remains an earlier producer
            // boundary; do not count this as a new invalidation consumer.
            (Phase::Analyze, "E-ANALYZE-IR")
        } else {
            (Phase::Analyze, "E-BORROW-CONFLICT")
        };
        assert_eq!(report.phase, phase, "{}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{}",
            report.to_json()
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn terminal340_owner_replacement_error_arm_actual_root_and_epoch() {
    let text = "resource R{code:i32}package P{manages owner:R;n:i32}enum F{Bad}fn make(ok:bool)->F!P{if ok{return P{owner:R{code:65},n:42}}return F.Bad()}fn tick()->i32{return 0}fn work(ok:bool)->i32{var value=make(ok);var other=make(true);let spare=&mut other;tick();let input=&mut value;match *input{.ok(ref mut p)=>{p.owner=R{code:66};return p.n},.err(_)=>{return 0}}}fn result()->i32{return work(true)+work(false)}";
    let input = source(text);
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let target = TargetProfile::default();
    let limits = Limits::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    for case in ["other-root", "stale-read", "wrong-arm"] {
        let mut raw = typed.clone();
        let mut claim = plan.clone();
        let tick = raw.functions.iter().find(|f| f.name == "tick").unwrap().id;
        let f = raw.functions.iter_mut().find(|f| f.name == "work").unwrap();
        let (block, pos) = f
            .blocks
            .iter()
            .enumerate()
            .find_map(|(b, block)| {
                block
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, InstructionKind::PlaceTagAddress { .. }))
                    .map(|p| (b, p))
            })
            .unwrap();
        let tag = f.blocks[block].instructions[pos].result.unwrap();
        match case {
            "other-root" => {
                let spare = f
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find_map(|i| match i.kind {
                        InstructionKind::BorrowLocalMut { local }
                            if f.locals[local.0 as usize].name == "other" =>
                        {
                            i.result
                        }
                        _ => None,
                    })
                    .unwrap();
                let InstructionKind::PlaceTagAddress { address, .. } =
                    &mut f.blocks[block].instructions[pos].kind
                else {
                    unreachable!()
                };
                *address = spare;
            }
            "stale-read" => {
                let (b, p) = f.blocks.iter().enumerate().find_map(|(b, block)| block.instructions.iter().position(|i| matches!(i.kind, InstructionKind::Call { function, .. } if function == tick)).map(|p| (b,p))).unwrap();
                let call = f.blocks[b].instructions.remove(p);
                let pos = f.blocks[block]
                    .instructions
                    .iter()
                    .position(|i| i.result == Some(tag))
                    .unwrap();
                f.blocks[block].instructions.insert(pos + 1, call);
            }
            "wrong-arm" => {
                let constant = f
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find_map(|i| match i.kind {
                        InstructionKind::Binary { left, right, .. } if left == tag => Some(right),
                        _ => None,
                    })
                    .unwrap();
                let i = f
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| i.result == Some(constant))
                    .unwrap();
                assert!(matches!(
                    i.kind,
                    InstructionKind::Constant(zil_ir::Constant::I32(0))
                ));
                i.kind = InstructionKind::Constant(zil_ir::Constant::I32(1));
            }
            _ => unreachable!(),
        }
        claim.module_fingerprint = raw.fingerprint();
        let errors = zil_analyze::analyze(&raw, &limits).unwrap_err();
        assert!(
            errors.iter().any(|e| e.code == "E-ANALYZE-IR"),
            "{case}: {errors:?}"
        );
        let errors =
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
        assert!(
            errors.iter().any(|e| e.code.starts_with("E-VERIFY-SUM")),
            "{case}: {errors:?}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        let errors = zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err();
        assert!(
            errors.iter().any(|e| e.code.starts_with("E-VERIFY-SUM")),
            "Wire {case}: {errors:?}"
        );
    }
}

#[test]
fn terminal340_owner_replacement_error_arm_alias_budget() {
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/340/replace-error-arm-alias.zil"
    ));
    // G1.3.6: previous Analyze N46908; actual local retirement/liveness and
    // paid producer eligibility add40. Verify retains its independent bound.
    for (stage, expected_n) in [(Stage::Analyze, 46948), (Stage::Verify, 97818)] {
        let options = |work| CompileOptions {
            stop_after: stage,
            limits: Limits {
                max_analysis_steps: work,
                ..Limits::default()
            },
            ..CompileOptions::default()
        };
        let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            let report = compile_source(&input, &options(mid));
            match report.status {
                ResultCategory::Success => high = mid,
                ResultCategory::ResourceLimit => low = mid + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        assert_eq!(
            low, expected_n,
            "{stage:?}: actual address tag proof work changed"
        );
        let mut warm = zil_driver::CompilationSession::default();
        for (work, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                compile_source(&input, &options(work)),
                warm.compile_source(&input, &options(work)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
                if expected == ResultCategory::ResourceLimit {
                    assert_eq!(
                        report.phase,
                        if stage == Stage::Analyze {
                            Phase::Analyze
                        } else {
                            Phase::Verify
                        }
                    );
                } else if stage == Stage::Verify {
                    assert_eq!(report.verification_runs, 1);
                }
            }
            assert_eq!(warm.active_query_count(), 0);
        }
        println!("G1.3.3 {stage:?} N={low}; N-1/fresh/warm/retry");
    }
}

#[test]
fn terminal340_package_terminal_source_types_and_analyze() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-pending.zil"
    );
    for (name, text, abort) in [
        ("complete", text, false),
        (
            "failed preparation",
            include_str!(
                "../../../tests/language-readiness/terminal/340/replace-package-terminal-failure.zil"
            ),
            true,
        ),
    ] {
        let temp = Temp::new();
        let typed = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Types,
                emit_dir: Some(temp.0.clone()),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            typed.status,
            ResultCategory::Success,
            "{name}: {}",
            typed.to_json()
        );
        let ir = fs::read_to_string(&typed.artifacts["typed"]).unwrap();
        assert!(
            ir.contains("ReservePlaceAddress") && ir.contains("CommitPlaceReplace"),
            "{name}: {ir}"
        );
        assert!(!ir.contains("WriteAddress"), "{name}: {ir}");
        if abort {
            assert!(ir.contains("AbortPlaceReservation"), "{name}: {ir}");
        }
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: Stage::Analyze,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
    }
    let old = include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-old-desc.zil"
    );
    let report = compile_source(
        &source(old),
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("description required later")),
        "{}",
        report.to_json()
    );
    let shared = include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-shared.zil"
    );
    let report = compile_source(
        &source(shared),
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-ADDRESS-PERMISSION")
    );
    let partial = text
        .replace("let target=", "let taken=give packet.owner;let target=")
        .replace("packet.n+packet.owner.code-66", "taken.code");
    let report = compile_source(
        &source(&partial),
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze);
    assert!(
        report.diagnostics.iter().any(|d| d
            .message
            .contains("Product borrow requires every owned child live")),
        "{}",
        report.to_json()
    );
    let prefix = "resource R{code:i32}package P{manages owner:R;n:i32}";
    let nonlocal = format!(
        "{prefix}fn replace(target:&mut P){{*target=P{{owner:R{{code:66}},n:42}}}}fn main()->i32{{var p=P{{owner:R{{code:65}},n:0}};replace(&mut p);return p.n}}"
    );
    let report = compile_source(
        &source(&nonlocal),
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze);
    let pin = format!(
        "{prefix}fn change(p:&mut P)->P{{p.n=9;return P{{owner:R{{code:66}},n:42}}}}fn main()->i32{{var packet=P{{owner:R{{code:65}},n:0}};let target=&mut packet;*target=change(target);return packet.n}}"
    );
    let report = compile_source(
        &source(&pin),
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Analyze);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT"),
        "{}",
        report.to_json()
    );
}

#[test]
fn terminal340_package_terminal_resource_identity_raw_and_wire() {
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-pending.zil"
    ));
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::default();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    for mode in 0..6 {
        let mut raw = typed.clone();
        match mode {
            0 => {
                raw.resources[0].name = "Other".into();
                let Type::Record { name, .. } = &mut raw.resources[0].payload else {
                    unreachable!()
                };
                *name = "Other".into();
            }
            1 => raw.resources.clear(),
            2 => raw.resources.push(raw.resources[0].clone()),
            3 => {
                let f = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
                let mut local = f.locals[0].clone();
                local.id = zil_ir::LocalId(f.locals.len() as u32);
                local.name = "unused_other".into();
                local.ty = Type::Managed(Box::new(Type::Record {
                    name: "Other".into(),
                    fields: vec![("code".into(), Type::I32)],
                }));
                f.locals.push(local);
            }
            4 => {
                let Type::Record { fields, .. } = &mut raw.resources[0].payload else {
                    unreachable!()
                };
                fields[0].1 = Type::I64;
            }
            _ => {
                raw.resources[0].teardown =
                    Some(raw.functions.iter().find(|f| f.name == "main").unwrap().id)
            }
        }
        let result = zil_analyze::analyze(&raw, &limits);
        assert!(
            result.is_err(),
            "Package resource neighbor {mode} was wrongly accepted"
        );
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.code == "E-ANALYZE-IR" && d.message.contains("resource")),
            "neighbor {mode}: {errors:?}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        for errors in [
            zil_verify::verify_module(raw, plan.clone(), &target, &limits).unwrap_err(),
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
        ] {
            assert!(
                errors.iter().any(|d| d.code == "E-VERIFY-RESOURCE"),
                "neighbor {mode}: {errors:?}"
            );
        }
    }
}

#[test]
fn terminal340_package_terminal_native_old_drop_store_and_failure() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-pending.zil"
    );
    let alias = text.replace(
        "packet.n+packet.owner.code-66",
        "target.n+target.owner.code-66",
    );
    let other = text
        .replace(
            "    let target=",
            "    var other=P{owner:R{code:67},n:0};let old=other.owner;\n    let target=",
        )
        .replace("packet.n+packet.owner.code-66", "packet.n+old.code-67");
    for (name, text, expected) in [
        ("whole Package", text, b"AB42\n".as_slice()),
        (
            "parent address survives",
            alias.as_str(),
            b"AB42\n".as_slice(),
        ),
        (
            "different actual same-type local",
            other.as_str(),
            b"ACB42\n".as_slice(),
        ),
        (
            "failed RHS preparation",
            include_str!(
                "../../../tests/language-readiness/terminal/340/replace-package-terminal-failure.zil"
            ),
            b"BA42\n".as_slice(),
        ),
    ] {
        eprintln!("G1.3.4 native case {name}");
        review329_native_entry_output(&[("main", text)], "main", expected);
    }
}

#[test]
fn terminal340_package_terminal_exact_rhs_provenance() {
    let fresh = include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-fresh-rhs.zil"
    );
    let incoming = include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-incoming-rhs.zil"
    );
    let given = incoming.replace("result(create)", "result(give)");
    let reversed = fresh.replace(
        "package P{manages owner:R;n:i32}",
        "package P{n:i32;manages owner:R}",
    );
    let via_sum = fresh.replace("fn fresh", "enum Carrier{Wrap(P)}\nfn fresh")
        .replace("return give packet.owner", "let wrapper=Carrier.Wrap(packet);match wrapper{Carrier.Wrap(child)=>{return give child.owner}}");
    let via_product = fresh.replace("fn fresh", "package Outer{child:P;n:i32}\nfn fresh")
        .replace("return give packet.owner", "let moved=packet;let outer=Outer{child:moved,n:0};let child=outer.child;return give child.owner");
    let bad_sum = incoming.replace("fn incoming", "enum Carrier{Wrap(P)}\nfn incoming")
        .replace("return give packet.owner", "let wrapper=Carrier.Wrap(packet);match wrapper{Carrier.Wrap(child)=>{return give child.owner}}");
    let bad_product = incoming.replace("fn incoming", "package Outer{child:P;n:i32}\nfn incoming")
        .replace("return give packet.owner", "let moved=packet;let outer=Outer{child:moved,n:0};let child=outer.child;return give child.owner");
    for (name, text, expected) in [
        (
            "old incoming to actual fresh",
            fresh,
            ResultCategory::Success,
        ),
        (
            "old created to incoming",
            incoming,
            ResultCategory::SafeReject,
        ),
        (
            "incoming exact give result",
            given.as_str(),
            ResultCategory::Success,
        ),
        (
            "actual owner is second field",
            reversed.as_str(),
            ResultCategory::Success,
        ),
        (
            "fresh leaf through owning Sum",
            via_sum.as_str(),
            ResultCategory::Success,
        ),
        (
            "fresh leaf through owning Product",
            via_product.as_str(),
            ResultCategory::Success,
        ),
        (
            "incoming through owning Sum is not create",
            bad_sum.as_str(),
            ResultCategory::SafeReject,
        ),
        (
            "incoming through owning Product is not create",
            bad_product.as_str(),
            ResultCategory::SafeReject,
        ),
    ] {
        for stage in [Stage::Analyze, Stage::Verify] {
            let report = compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: stage,
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                expected,
                "{name} {stage:?}: {}",
                report.to_json()
            );
            if expected == ResultCategory::SafeReject {
                assert_eq!(report.phase, Phase::Analyze);
                assert!(
                    report.diagnostics.iter().any(|d| d
                        .message
                        .contains("create result forwards an incoming responsibility")),
                    "{}",
                    report.to_json()
                );
            } else if stage == Stage::Verify {
                assert_eq!(report.verification_runs, 1);
            }
        }
    }
}

#[test]
fn terminal340_package_terminal_desc_closure_carriers_remain_live() {
    let old = include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-old-desc.zil"
    );
    for borrowed in [false, true] {
        let saved = if borrowed {
            "let saved=()->i32=>old.code;let borrowed=&saved;"
        } else {
            "let saved=()->i32=>old.code;"
        };
        let text = old
            .replace("    let target=", &format!("    {saved}let target="))
            .replace(
                "return old.code",
                if borrowed {
                    "return borrowed()"
                } else {
                    "return saved()"
                },
            );
        let report = compile_source(
            &source(&text),
            &CompileOptions {
                stop_after: Stage::Analyze,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "borrowed={borrowed}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Analyze);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("description required later")),
            "borrowed={borrowed}: {}",
            report.to_json()
        );
    }
}

#[test]
fn terminal340_package_terminal_exact_request_budget() {
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-pending.zil"
    ));
    for stage in [Stage::Analyze, Stage::Verify] {
        let options = |work| CompileOptions {
            stop_after: stage,
            limits: Limits {
                max_analysis_steps: work,
                ..Limits::default()
            },
            ..CompileOptions::default()
        };
        let (mut low, mut high) = (1, Limits::default().max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            let report = compile_source(&input, &options(mid));
            match report.status {
                ResultCategory::Success => high = mid,
                ResultCategory::ResourceLimit => low = mid + 1,
                _ => panic!("{}", report.to_json()),
            }
        }
        assert_eq!(
            low,
            if stage == Stage::Analyze {
                // G1.3.6 paid producer exclusion inventory: previous13945+9.
                13_954
            } else {
                // G1.3.7: old28743 + three actual complete-RHS nodes.
                28_746
            },
            "{stage:?}: actual Package work changed"
        );
        let mut warm = zil_driver::CompilationSession::default();
        for (work, expected) in [
            (low, ResultCategory::Success),
            (low - 1, ResultCategory::ResourceLimit),
            (low, ResultCategory::Success),
        ] {
            for report in [
                compile_source(&input, &options(work)),
                warm.compile_source(&input, &options(work)),
            ] {
                assert_eq!(report.status, expected, "{}", report.to_json());
                if expected == ResultCategory::ResourceLimit {
                    assert_eq!(
                        report.phase,
                        if stage == Stage::Analyze {
                            Phase::Analyze
                        } else {
                            Phase::Verify
                        }
                    );
                } else if stage == Stage::Verify {
                    assert_eq!(report.verification_runs, 1);
                }
            }
            assert_eq!(warm.active_query_count(), 0);
        }
        println!("G1.3.4 whole Source {stage:?} N={low}; N-1/fresh/warm/retry");
    }
}

#[test]
fn terminal340_package_terminal_new_provenance_and_temporary_native() {
    let fresh = include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-fresh-rhs.zil"
    );
    let reverse = fresh.replace(
        "package P{manages owner:R;n:i32}",
        "package P{n:i32;manages owner:R}",
    );
    let given = include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-incoming-rhs.zil"
    )
    .replace("result(create)", "result(give)");
    let temporary = include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-failure.zil"
    )
    .replace(
        "assemble(give spare:R,n:i32)->P{return P{owner:R{code:n},n:42}}",
        "assemble(spare:P,n:i32)->P{return spare}",
    )
    .replace(
        "assemble(R{code:66},fail()?)",
        "assemble(P{owner:R{code:66},n:42},fail()?)",
    );
    for (name, text, expected) in [
        (
            "fresh leaf replaces incoming body-local leaf",
            fresh,
            b"AB42\n".as_slice(),
        ),
        (
            "incoming RHS retains give origin",
            given.as_str(),
            b"AB42\n".as_slice(),
        ),
        (
            "owned field declared second",
            reverse.as_str(),
            b"AB42\n".as_slice(),
        ),
        (
            "whole Package temporary abort",
            temporary.as_str(),
            b"BA42\n".as_slice(),
        ),
    ] {
        eprintln!("G1.3.4 new native case {name}");
        review329_native_entry_output(&[("main", text)], "main", expected);
    }
}

#[test]
fn terminal340_package_terminal_independent_verify_and_wire() {
    let target = TargetProfile::default();
    let limits = Limits::default();
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/340/replace-package-terminal-pending.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/340/replace-package-terminal-failure.zil"
        ),
    ] {
        let input = source(text);
        let ast = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand(&input, &ast).unwrap();
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        if !text.contains("enum F") {
            for case in 0..4 {
                let mut raw = typed.clone();
                match case {
                    0 => raw.resources.clear(),
                    1 => {
                        raw.resources[0].name = "Other".into();
                        let Type::Record { name, .. } = &mut raw.resources[0].payload else {
                            unreachable!()
                        };
                        *name = "Other".into();
                    }
                    2 => raw.resources.push(raw.resources[0].clone()),
                    _ => {
                        let drop = raw.resources[0].teardown.expect("real ordinary Drop entry");
                        raw.functions[drop.0 as usize].call_kind = zil_ir::CallKind::Suspend;
                    }
                }
                let mut claim = plan.clone();
                claim.module_fingerprint = raw.fingerprint();
                for wire in [false, true] {
                    let errors = if wire {
                        let bytes =
                            zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
                        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
                    } else {
                        zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits)
                            .unwrap_err()
                    };
                    assert!(
                        errors.iter().any(|e| e.code == "E-VERIFY-RESOURCE"),
                        "metadata{case} wire={wire}: {errors:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn terminal340_package_terminal_actual_description_epoch_and_equal_type_roots() {
    let target = TargetProfile::default();
    let limits = Limits::default();
    for other in [false, true] {
        let described = if other { "other" } else { "packet" };
        let text = format!(
            "resource R{{code:i32}}package P{{manages owner:R;n:i32}}fn main()->i32{{var packet=P{{owner:R{{code:65}},n:0}};var other=P{{owner:R{{code:67}},n:1}};let old={described}.owner;let seen=old.code;let target=&mut packet;*target=P{{owner:R{{code:66}},n:42}};return seen}}"
        );
        let input = source(&text);
        let ast = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand(&input, &ast).unwrap();
        let mut typed = zil_types::check(&core, &target, &limits).unwrap();
        let mut plan = zil_analyze::analyze(&typed, &limits).unwrap();
        zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
        // Preserve the actual Source's read and SSA definitions; move only the
        // real read/store after Commit. Type/layout equality grants no epoch.
        let f = typed
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        let seen = f.locals.iter().find(|l| l.name == "seen").unwrap().id;
        let instructions = &mut f.blocks[0].instructions;
        let start = instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
            .unwrap();
        let end = instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::Store { local, .. } if local == seen))
            .unwrap();
        let moved: Vec<_> = instructions.drain(start..=end).collect();
        let commit = instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }))
            .unwrap();
        instructions.splice(commit + 1..commit + 1, moved);
        plan.module_fingerprint = typed.fingerprint();
        for wire in [false, true] {
            let result = if wire {
                let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
                zil_verify::verify_module_bytes(&bytes, &target, &limits)
            } else {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits)
            };
            if other {
                result.unwrap();
            } else {
                let errors = result.unwrap_err();
                assert!(
                    errors
                        .iter()
                        .any(|e| e.code == "E-VERIFY-RESOURCE"
                            && e.message.contains("live description")),
                    "wire={wire}: {errors:?}"
                );
            }
        }
    }
}

#[test]
fn terminal340_package_terminal_actual_parent_pin_is_not_type_apartness() {
    let input = source(
        "resource R{code:i32}package P{manages owner:R;n:i32}fn prepare(p:&mut P)->P{p.n=9;return P{owner:R{code:66},n:42}}fn main()->i32{var packet=P{owner:R{code:65},n:0};var other=P{owner:R{code:67},n:1};let spare=&mut other;let target=&mut packet;*target=prepare(spare);return packet.n}",
    );
    let target = TargetProfile::default();
    let limits = Limits::default();
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let prepare = typed
        .functions
        .iter()
        .find(|f| f.name == "prepare")
        .unwrap()
        .id;
    let mut raw = typed.clone();
    let f = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let parent = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match i.kind {
            InstructionKind::ReservePlaceAddress { address, .. } => Some(address),
            _ => None,
        })
        .unwrap();
    let argument = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match &i.kind {
            InstructionKind::Call {
                function,
                arguments,
            } if *function == prepare => Some(arguments[0]),
            _ => None,
        })
        .unwrap();
    assert_eq!(f.values[parent.0 as usize], f.values[argument.0 as usize]);
    let actual = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| i.result == Some(argument))
        .unwrap();
    actual.kind = InstructionKind::ReborrowAddress {
        address: parent,
        mutable: true,
    };
    let mut claim = plan;
    // Supply a matching untrusted Hold claim for the changed real child; the
    // pin proof must reject even after coarse provenance has been corrected.
    let row = &mut claim.functions[f.id.0 as usize].holds;
    row[argument.0 as usize].roots = row[parent.0 as usize].roots.clone();
    claim.module_fingerprint = raw.fingerprint();
    for wire in [false, true] {
        let errors = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
        } else {
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err()
        };
        assert!(
            errors.iter().any(|e| e.code == "E-BORROW-REPLACEMENT-PIN"),
            "wire={wire}: {errors:?}"
        );
    }
}

#[test]
fn terminal340_package_terminal_verify_budgets_are_exact_and_fresh() {
    let input = source(include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-pending.zil"
    ));
    let target = TargetProfile::default();
    let limits = Limits::default();
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    for wire in [false, true] {
        let run = |work| {
            let limits = Limits {
                max_analysis_steps: work,
                ..Limits::default()
            };
            let result = if wire {
                zil_verify::verify_module_bytes(&bytes, &target, &limits)
            } else {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits)
            };
            match result {
                Ok(verified) => Some(verified.fingerprint()),
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|e| e.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    None
                }
            }
        };
        let (mut low, mut high) = (1, limits.max_analysis_steps);
        while low < high {
            let middle = low + (high - low) / 2;
            if run(middle).is_some() {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        let fresh = run(low).unwrap();
        for _ in 0..2 {
            assert!(run(low - 1).is_none());
            assert_eq!(run(low), Some(fresh));
        }
        println!(
            "G1.3.4.1 Verify wire={wire} N={low} N-1=ResourceLimit fresh/retry=Pass fp={fresh}"
        );
    }
    let options = |work| CompileOptions {
        stop_after: Stage::Verify,
        limits: Limits {
            max_analysis_steps: work,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let (mut low, mut high) = (1, limits.max_analysis_steps);
    while low < high {
        let middle = low + (high - low) / 2;
        match compile_source(&input, &options(middle)).status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            status => panic!("unexpected source boundary {status:?}"),
        }
    }
    let mut warm = zil_driver::CompilationSession::default();
    for (work, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        for report in [
            warm.compile_source(&input, &options(work)),
            zil_driver::CompilationSession::default().compile_source(&input, &options(work)),
        ] {
            assert_eq!(report.status, expected, "{}", report.to_json());
        }
        assert_eq!(warm.active_query_count(), 0);
    }
    println!("G1.3.4.1 whole Source->Verify N={low} N-1=ResourceLimit fresh/warm/retry=Pass");
}

#[test]
fn terminal340_package_terminal_actual_tickets_complete_shell_and_rhs_once() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/340/replace-package-terminal-pending.zil"
    )
    .replace(
        "    return packet.n",
        "    *target=P{owner:R{code:67},n:43};\n    return packet.n",
    );
    let input = source(&text);
    let target = TargetProfile::default();
    let limits = Limits::default();
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
    for mode in 0..6 {
        let mut raw = typed.clone();
        let f = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
        let packet = f.locals.iter().find(|l| l.name == "packet").unwrap().id;
        let instructions = &mut f.blocks[0].instructions;
        let reserve = instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::ReservePlaceAddress { .. }))
            .unwrap();
        let commits: Vec<_> = instructions
            .iter()
            .enumerate()
            .filter_map(|(p, i)| {
                matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }).then_some(p)
            })
            .collect();
        assert_eq!(commits.len(), 2, "real producer must issue two tickets");
        let InstructionKind::CommitPlaceReplace { reservation, value } =
            instructions[commits[0]].kind
        else {
            unreachable!()
        };
        let expected = match mode {
            0 => {
                instructions.remove(reserve);
                "E-VERIFY-PLACE-REPLACE"
            }
            1 => {
                instructions.insert(commits[0] + 1, instructions[commits[0]].clone());
                "E-VERIFY-PLACE-REPLACE"
            }
            2 => {
                instructions[commits[0]].kind =
                    InstructionKind::AbortPlaceReservation { reservation };
                "E-VERIFY-RESOURCE"
            }
            3 => {
                let mut drop = instructions[commits[0]].clone();
                drop.kind = InstructionKind::DropPackageField {
                    local: packet,
                    index: 0,
                };
                instructions.insert(commits[0], drop);
                // The real alias is retained through Commit: the ordinary
                // loan check rejects this field cleanup before Content/debt.
                "E-BORROW-CONFLICT"
            }
            4 => {
                let InstructionKind::ReservePlaceAddress { path, .. } =
                    &mut instructions[reserve].kind
                else {
                    unreachable!()
                };
                path.push(zil_ir::PlaceProjection::Field(0));
                "E-VERIFY-PLACE-REPLACE"
            }
            _ => {
                let InstructionKind::CommitPlaceReplace { value: second, .. } =
                    &mut instructions[commits[1]].kind
                else {
                    unreachable!()
                };
                *second = value;
                "E-VERIFY-RESOURCE"
            }
        };
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        for wire in [false, true] {
            let errors = if wire {
                let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
                zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
            } else {
                zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err()
            };
            assert!(
                errors.iter().any(|d| d.code.starts_with(expected)),
                "mode={mode} wire={wire}: {errors:?}"
            );
        }
    }
}

#[test]
fn terminal340_package_terminal_actual_same_type_wrong_root_with_matching_hold() {
    let input = source(
        "resource R{code:i32}package P{manages owner:R;n:i32}fn main()->i32{var packet=P{owner:R{code:65},n:0};var other=P{owner:R{code:67},n:0};let old=other.owner;let target=&mut packet;*target=P{owner:R{code:66},n:42};return old.code+packet.n}",
    );
    let target = TargetProfile::default();
    let limits = Limits::default();
    let ast = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand(&input, &ast).unwrap();
    let mut typed = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let f = typed
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let packet = f.locals.iter().find(|l| l.name == "packet").unwrap().id;
    let other = f.locals.iter().find(|l| l.name == "other").unwrap().id;
    assert_eq!(
        f.locals[packet.0 as usize].ty,
        f.locals[other.0 as usize].ty
    );
    let producer = f.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::BorrowLocalMut { local } if local == packet))
        .unwrap();
    producer.kind = InstructionKind::BorrowLocalMut { local: other };
    // Correct even the untrusted coarse Hold claim. The old description's
    // actual instance, rather than equal nominal/layout Type, must decide.
    for hold in &mut plan.functions[f.id.0 as usize].holds {
        for root in &mut hold.roots {
            if *root == zil_ir::HoldRoot::Local(packet) {
                *root = zil_ir::HoldRoot::Local(other);
            }
        }
        hold.roots.sort();
        hold.roots.dedup();
    }
    plan.module_fingerprint = typed.fingerprint();
    for wire in [false, true] {
        let errors = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
        } else {
            zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap_err()
        };
        assert!(
            errors
                .iter()
                .any(|d| d.code == "E-VERIFY-RESOURCE" && d.message.contains("live description")),
            "wire={wire}: {errors:?}"
        );
    }
}

#[test]
fn terminal340_package_terminal_optional_product_lanes_keep_actual_rhs_origin() {
    for fresh in [true, false] {
        let (old, rhs) = if fresh {
            ("give next", "R{code:66}")
        } else {
            ("R{code:65}", "give next")
        };
        let text = format!(
            "resource R{{code:i32}}package P{{manages owner:R;n:i32}}package Outer{{child:P}}fn make(give next:R)->R contract{{result(give)}}{{let nested=Outer{{child:P{{owner:R{{code:1}},n:0}}}};var packet=P{{owner:{old},n:0}};let target=&mut packet;*target=P{{owner:{rhs},n:42}};let moved=packet;let wrap=Outer{{child:moved}};let child=wrap.child;return give child.owner}}fn main()->i32{{let v=make(R{{code:2}});return v.code}}"
        );
        let input = source(&text);
        let target = TargetProfile::default();
        let limits = Limits::default();
        let ast = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand(&input, &ast).unwrap();
        let mut typed = zil_types::check(&core, &target, &limits).unwrap();
        let mut plan = zil_analyze::analyze(&typed, &limits).unwrap();
        zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
        // The Source's valid give contract supplies the candidate; promote
        // only the untrusted result claim and independently prove or reject it.
        typed
            .functions
            .iter_mut()
            .find(|f| f.name == "make")
            .unwrap()
            .result_mode = zil_ir::PassingMode::Create;
        plan.module_fingerprint = typed.fingerprint();
        for wire in [false, true] {
            let result = if wire {
                let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
                zil_verify::verify_module_bytes(&bytes, &target, &limits)
            } else {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits)
            };
            if fresh {
                result.unwrap();
            } else {
                let errors = result.unwrap_err();
                assert!(
                    errors.iter().any(|d| d.message.contains("fresh")),
                    "{errors:?}"
                );
            }
        }
    }
}

#[test]
fn terminal340_package_terminal_branch_join_requires_one_actual_root() {
    let target = TargetProfile::default();
    let limits = Limits::default();
    for same in [true, false] {
        let right = if same { "x" } else { "y" };
        let text = format!(
            "resource R{{code:i32}}package P{{manages owner:R;n:i32}}fn choose(flag:bool,give incoming:R)->R contract{{result(give)}}{{var x=P{{owner:give incoming,n:0}};var y=P{{owner:R{{code:2}},n:1}};let target=if flag{{&mut x}}else{{&mut {right}}};*target=P{{owner:R{{code:3}},n:3}};return give x.owner}}fn main()->i32{{let r=choose(false,R{{code:1}});return r.code}}"
        );
        let input = source(&text);
        let ast = zil_syntax::parse(&input).into_result().unwrap();
        let core = zil_expand::expand(&input, &ast).unwrap();
        let typed = zil_types::check(&core, &target, &limits).unwrap();
        if same {
            let plan = zil_analyze::analyze(&typed, &limits).unwrap();
            for create in [false, true] {
                let mut raw = typed.clone();
                if create {
                    raw.functions
                        .iter_mut()
                        .find(|f| f.name == "choose")
                        .unwrap()
                        .result_mode = zil_ir::PassingMode::Create;
                }
                let mut claim = plan.clone();
                claim.module_fingerprint = raw.fingerprint();
                zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap();
                let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
                zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
            }
            continue;
        }
        assert!(zil_analyze::analyze(&typed, &limits).is_err());
        // Keep the real Source CFG, both address producers, all SSA values
        // and their honest multi-root Holds. A definite direct-x reservation
        // supplies the candidate without asking Analyze to authorize the
        // distinct raw address-selected replacement that Verify must reject.
        let mut direct = typed.clone();
        let f = direct
            .functions
            .iter_mut()
            .find(|f| f.name == "choose")
            .unwrap();
        let x = f.locals.iter().find(|l| l.name == "x").unwrap().id;
        let reserve = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .find(|i| matches!(i.kind, InstructionKind::ReservePlaceAddress { .. }))
            .unwrap();
        let InstructionKind::ReservePlaceAddress { reservation, .. } = reserve.kind else {
            unreachable!()
        };
        reserve.kind = InstructionKind::ReservePlaceLocal {
            reservation,
            local: x,
            path: vec![],
        };
        let plan = zil_analyze::analyze(&direct, &limits).unwrap();
        zil_verify::verify_module(direct.clone(), plan.clone(), &target, &limits).unwrap();
        let mut outcomes = Vec::new();
        for create in [false, true] {
            let mut raw = typed.clone();
            if create {
                raw.functions
                    .iter_mut()
                    .find(|f| f.name == "choose")
                    .unwrap()
                    .result_mode = zil_ir::PassingMode::Create;
            }
            let mut claim = plan.clone();
            claim.module_fingerprint = raw.fingerprint();
            for wire in [false, true] {
                let result = if wire {
                    let bytes =
                        zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
                    zil_verify::verify_module_bytes(&bytes, &target, &limits)
                } else {
                    zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits)
                };
                println!(
                    "whole Package branch join create={create} wire={wire} accepted={}",
                    result.is_ok()
                );
                outcomes.push((create, wire, result.err()));
            }
        }
        for (create, wire, errors) in outcomes {
            let errors = errors.expect("multi-root whole Package replacement was accepted");
            assert!(
                errors
                    .iter()
                    .any(|d| d.category == ResultCategory::Unsupported
                        && d.message
                            .contains("one independently reconstructed actual target")),
                "create={create} wire={wire}: {errors:?}"
            );
        }
    }
}

#[test]
fn g443_original_static_member_factories_reach_native_full_values() {
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct5-static-member-instance.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct5-static-member-retained.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct5-static-member-host-identity.zil"
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        review329_native_entry_output(&[("main", text)], "main", b"42\n");
    }
}

#[test]
fn g443_member_factories_keep_source_boundaries_and_definition_checks() {
    for text in [
        "struct Maker{fn make()->i32{return 42}}fn main()->i32{return Maker.make()}",
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct5-mixed-ct-factory.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct5-member-context.zil"
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    let valid = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-static-member-instance.zil"
    );
    let bad = valid.replace(
        "return Phantom::<'a,T,N>{n:N}",
        "return Phantom::<'a,i32,42>{n:42}",
    );
    let unused_bad = bad.replace(
        "let value:Phantom<'s,i32,42>=#(Maker.make::<i32,42,'s>(i32));return value.n",
        "return 42",
    );
    assert_ne!(unused_bad, bad);
    for text in [
        bad,
        unused_bad,
        valid.replace("make::<i32,42,'s>", "make::<i32,42>"),
        valid.replace("make::<i32,42,'s>", "make::<i32,true,'s>"),
        valid.replace("make::<i32,42,'s>", "make::<i32,42,'other>"),
        valid.replace("(marker:Type)", "(self:Self,marker:Type)"),
        valid
            .replace("struct Maker{", "struct Maker<X:Copy>{")
            .replace("Maker.make::<", "Maker::<bool>.make::<"),
    ] {
        let report = compile_source(&source(&text), &CompileOptions::default());
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn g443_same_named_hosts_in_distinct_modules_use_actual_imports() {
    let root = "import left;import right;struct Pair<life a>{left:i32;right:i32}fn result<life s>(p:&'s i32)->i32{let pair:Pair<'s>=#(Pair::<'s>{left:left.Maker.make::<i32,7,'s>(i32),right:right.Maker.make::<i32,7,'s>(i32)});if pair.left!=7{return 1}if pair.right!=35{return 2}return pair.left+pair.right}fn main()->i32{let n=0;return result(&n)}";
    let left = "pub struct Maker{pub fn make<T:Copy,#N:i32,life a>(marker:Type)->i32{return N}}";
    let right = "pub struct Maker{pub fn make<T:Copy,#N:i32,life a>(marker:Type)->i32{return N*5}}";
    review329_native_entry_output(
        &[("main", root), ("left", left), ("right", right)],
        "main",
        b"42\n",
    );
    for hidden in [
        left.replacen("pub struct", "struct", 1),
        left.replacen("pub fn", "fn", 1),
    ] {
        let report = zil_driver::compile_project(
            &review329_project(&[("main", root), ("left", &hidden), ("right", right)]),
            &CompileOptions::default(),
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, zil_base::Phase::Ct);
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn g443_member_requests_pay_all_current_ledgers() {
    let input = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-static-member-retained.zil"
    );
    for ledger in ["work", "nodes", "steps", "bytes"] {
        let n = g44_source_request_boundary(input, ledger);
        eprintln!("G4.4.3 real retained-member Source {ledger} N={n}; N-1/cold/warm/fresh agree");
    }
}

#[test]
fn g444_nested_mixed_scalar_source_and_native_values() {
    let scalar = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-nested-mixed-scalar.zil"
    );
    let intermediate = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-nested-mixed-intermediate.zil"
    );
    let reordered = scalar
        .replace(
            "fn forward<T:Copy,#N:i32,life a>",
            "fn forward<life a,#N:i32,T:Copy>",
        )
        .replace("forward::<A,M,'b>", "forward::<'b,M,A>");
    let constants = scalar.replace("make::<T,N,'a>", "make::<T,42,'a>");
    for text in [scalar, intermediate, reordered.as_str(), constants.as_str()] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        review329_native_entry_output(&[("main", text)], "main", b"42\n");
    }
}

#[test]
fn g444_symbolic_forwarding_checks_definitions_before_matching_instances() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-nested-mixed-forwarding.zil"
    );
    let bad_type = text
        .replace(
            "fn forward<T:Copy,#N:i32,life a>",
            "fn forward<T:Copy,U:Copy,#N:i32,life a>",
        )
        .replace("make::<T,N,'a>", "make::<U,N,'a>")
        .replace("forward::<i32,42,'s>", "forward::<i32,i32,42,'s>");
    let bad_const = text
        .replace(
            "fn forward<T:Copy,#N:i32,life a>",
            "fn forward<T:Copy,#N:i32,#M:i32,life a>",
        )
        .replace("make::<T,N,'a>", "make::<T,M,'a>")
        .replace("forward::<i32,42,'s>", "forward::<i32,42,42,'s>");
    let bad_life = text
        .replace(
            "fn forward<T:Copy,#N:i32,life a>",
            "fn forward<T:Copy,#N:i32,life a,life b>",
        )
        .replace("make::<T,N,'a>", "make::<T,N,'b>")
        .replace("forward::<i32,42,'s>", "forward::<i32,42,'s,'s>");
    for bad in [bad_type, bad_const, bad_life] {
        for input in [
            bad.clone(),
            bad.replace(
                "fn main()->i32{let n=0;return result(&n)}",
                "fn main()->i32{return 42}",
            ),
        ] {
            let report = compile_source(&source(&input), &CompileOptions::default());
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
                    .any(|d| d.message.contains("actual expected type")),
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 0);
        }
    }
    let scalar = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-nested-mixed-scalar.zil"
    );
    for (bad, expected) in [
        (
            scalar.replace("fn forward<T:Copy,#N:i32", "fn forward<T:Copy,#N:usize"),
            "scalar argument domain differs",
        ),
        (
            scalar.replace("fn forward<T:Copy,#N:i32", "fn forward<T:Copy,#N:bool"),
            "scalar argument domain differs",
        ),
        (
            scalar.replace("make::<T,N,'a>", "make::<T,N,'missing>"),
            "not this declaration's binder",
        ),
        (
            scalar.replace("make::<T,N,'a>", "make::<T,N>"),
            "actual arity differs",
        ),
    ] {
        let report = compile_source(&source(&bad), &CompileOptions::default());
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
                .any(|d| d.message.contains(expected)),
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
    // G4.4.5 now preserves both authentic contexts and explicitly reconstructs
    // the pure result into the caller's return schema.
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn g445_original_context_record_returns_reach_native_full_values() {
    let reordered = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/p0-review444/good-three-layer-reordered.zil"
    );
    let separated = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-cross-factory-lifetime-separation.zil"
    );
    let two_sources = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-cross-factory-two-source-lives.zil"
    );
    let scalar_fields = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-cross-factory-scalar-fields.zil"
    );
    // The preexisting direct factory must also reify Unit as (), independently
    // of whether the new return relocation is used by this expression.
    let direct = scalar_fields.replace(
        "#(forward::<'s,42,bool>(bool))",
        "#(make::<bool,42,'s>(bool))",
    );
    let report = compile_source(&source(&direct), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let fields = reordered
        .replace("{n:i32}", "{n:i32;m:i32}")
        .replace("{n:N}", "{n:N,m:N+1}")
        .replace("return value.n", "return value.n+value.m");
    for (text, output) in [
        (reordered, b"42\n".as_slice()),
        (separated, b"42\n"),
        (two_sources, b"49\n"),
        (fields.as_str(), b"85\n"),
        (scalar_fields, b"42\n"),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        review329_native_entry_output(&[("main", text)], "main", output);
    }
}

#[test]
fn g445_relocation_keeps_symbolic_life_and_closed_type_identities() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-cross-factory-lifetime-separation.zil"
    );
    let bad = text.replace("make::<U,M,'b,'spare>", "make::<U,M,'spare,'b>");
    for input in [
        bad.clone(),
        bad.replace(
            "fn main()->i32{let n=0;return result(&n,&n)}",
            "fn main()->i32{return 42}",
        ),
    ] {
        let report = compile_source(&source(&input), &CompileOptions::default());
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
                .any(|d| d.message.contains("actual expected type"))
        );
        assert_eq!(report.verification_runs, 0);
    }
    // Closed Type forwarding already worked in G4.4.4; keep its precise
    // nominal identity as a control without claiming a new scoped-Type value.
    let control = "struct First{n:i32}struct Second{n:i32}fn selected<T:Copy,#N:i32,life a>(marker:Type)->Type{return T}fn relay<T:Copy,#N:i32,life a>(marker:Type)->Type{return selected::<T,N,'a>(marker)}fn result<life s>(p:&'s i32)->i32{let value:#(relay::<First,42,'s>(i32))=First{n:42};return value.n}fn main()->i32{let n=0;return result(&n)}";
    let report = compile_source(&source(control), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let report = compile_source(
        &source(&control.replace("=First{n:42}", "=Second{n:42}")),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types);
}

#[test]
fn g445_real_record_relocations_pay_all_request_ledgers() {
    for (shape, input) in [
        (
            "single",
            include_str!(
                "../../../tests/language-readiness/terminal/344/anonymous/ct5-cross-factory-lifetime-separation.zil"
            ),
        ),
        (
            "scalar-vector",
            include_str!(
                "../../../tests/language-readiness/terminal/344/anonymous/ct5-cross-factory-scalar-fields.zil"
            ),
        ),
    ] {
        for ledger in ["work", "nodes", "steps", "bytes"] {
            let n = g44_source_request_boundary(input, ledger);
            eprintln!(
                "G4.4.5 real record relocation {shape}/{ledger} N={n}; N-1/cold/warm/fresh agree"
            );
        }
    }
}

#[test]
fn g444_nested_requests_pay_each_current_ledger() {
    let input = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-nested-mixed-intermediate.zil"
    );
    for ledger in ["work", "nodes", "steps", "bytes"] {
        let n = g44_source_request_boundary(input, ledger);
        eprintln!("G4.4.4 nested Source {ledger} N={n}; N-1/cold/warm/fresh agree");
    }
}

#[test]
fn g443_actual_semantic_cache_rechecks_changed_and_reordered_members() {
    use zil_driver::{CompilationSession, DiskCacheStatus};
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct5-static-member-retained.zil"
    );
    let changed = text
        .replace("Phantom<'s,i32,35>", "Phantom<'s,i32,36>")
        .replace("make::<i32,35,'s>", "make::<i32,36,'s>");
    let make_start = text.find("    fn make<").unwrap();
    let read_start = text.find("    fn read<").unwrap();
    let read_end = read_start + text[read_start..].find("\n    }").unwrap() + "\n    }".len();
    let reordered = format!(
        "{}{}\n{}{}",
        &text[..make_start],
        &text[read_start..read_end],
        text[make_start..read_start].trim_end(),
        &text[read_end..]
    );
    assert_ne!(changed, text);
    assert_ne!(reordered, text);
    let dir = Temp::new();
    let mut session = CompilationSession::default();
    for (ordinal, (input, output)) in [
        (text, b"42\n".as_slice()),
        (text, b"42\n".as_slice()),
        (changed.as_str(), b"43\n".as_slice()),
        (reordered.as_str(), b"42\n".as_slice()),
        (text, b"42\n".as_slice()),
    ]
    .into_iter()
    .enumerate()
    {
        let report = session.compile_project(
            &review329_project(&[("main", input)]),
            &CompileOptions {
                stop_after: Stage::Build,
                entry: Some("main".into()),
                backend: zil_driver::NativeBackend::Clang,
                output: Some(dir.0.join(format!("program-{ordinal}"))),
                semantic_cache: Some(dir.0.join("cache")),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        if matches!(ordinal, 1 | 4) {
            assert_eq!(report.disk_cache, DiskCacheStatus::Hit);
        }
        assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
        let observed = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert!(observed.status.success());
        assert_eq!(observed.stdout, output);
    }
    let store = dir.0.join("cache/semantic-v4");
    let mut candidates = 0;
    for entry in fs::read_dir(&store).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "candidate") {
            continue;
        }
        let key =
            zil_base::Fingerprint::from_hex(path.file_stem().unwrap().to_str().unwrap()).unwrap();
        let zil_db::CacheRead::Candidate(payload) =
            zil_db::ArtifactCache::with_limit(&store, Limits::default().max_source_bytes)
                .inspect_candidate(key)
                .unwrap()
        else {
            panic!("actual cache candidate absent")
        };
        assert_eq!(&payload[..8], b"ZILSEM04");
        let length = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
        let bytes = &payload[12..12 + length];
        let (typed, plan) =
            zil_ir::wire::decode_module_candidate(bytes, &Limits::default()).unwrap();
        zil_verify::verify_module(typed.clone(), plan, &typed.target, &Limits::default()).unwrap();
        zil_verify::verify_module_bytes(bytes, &typed.target, &Limits::default()).unwrap();
        candidates += 1;
    }
    assert_eq!(candidates, 3);
    eprintln!(
        "G4.4.3 actual Project cache: warm/change/reorder/revert native42/43, {candidates} actual candidates raw/Wire reverified"
    );
}

#[path = "../../../tests/language-readiness/terminal/340/whole_helper_review.rs"]
mod whole_helper_review;

#[path = "../../../tests/language-readiness/terminal/340/whole_helper_error_review.rs"]
mod whole_helper_error_review;

#[path = "../../../tests/language-readiness/terminal/344/anonymous/p0-review4452/consumer_tests.rs"]
mod p0_g445_review;

#[path = "../../../tests/language-readiness/terminal/340/nonlocal-replace/controls.rs"]
mod terminal340_nonlocal_replace;

// Independent complete-parent consumers; staged failures remain visible during integration.
#[path = "../../../.task/G10.1.10-inputs/consumers.rs"]
mod g10110_independent_parent_consumers;
#[path = "../../../tests/language-readiness/terminal/344/anonymous/runtime-phase/consumer_tests.rs"]
mod runtime_ct_phase_tests;

#[test]
fn g4453_contextual_type_result_preserves_life_across_factories() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct6-cross-factory-type-result.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    review329_native_entry_output(&[("main", text)], "main", b"42\n");
}

#[test]
fn g4453_pure_composite_fields_keep_every_returned_bit() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct6-cross-factory-pure-composites.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    review329_native_entry_output(&[("main", text)], "main", b"18446744073709551557\n");
}

#[test]
fn g4453_ambient_type_actual_retains_nested_life_graph() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct6-ambient-type-actual.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    review329_native_entry_output(&[("main", text)], "main", b"42\n");
}

#[test]
fn g4453_generic_host_keeps_its_actual_type_binding() {
    let text = include_str!(
        "../../../tests/language-readiness/terminal/344/anonymous/ct6-generic-host-context.zil"
    );
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    review329_native_entry_output(&[("main", text)], "main", b"42\n");
}

#[test]
fn g4453_generic_parameter_subsets_keep_authentic_source_contexts() {
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct6-const-only-factory.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct6-generic-parameter-subsets.zil"
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        review329_native_entry_output(&[("main", text)], "main", b"42\n");
    }
}

#[test]
fn g4453_host_complete_context_and_ordinary_static_member() {
    for text in [
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct6-generic-host-full-context.zil"
        ),
        include_str!(
            "../../../tests/language-readiness/terminal/344/anonymous/ct6-generic-host-member-lives.zil"
        ),
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        review329_native_entry_output(&[("main", text)], "main", b"42\n");
    }
}

#[test]
fn g4453_ordinary_static_member_keeps_effect_and_duplicate_checks() {
    for text in [
        "struct Host{fn make()->i32{let n=42;let address=&n;return n}}fn main()->i32{return #(Host.make())}",
        "struct Host{fn make()->i32{return 42}fn make()->i32{return 7}}fn main()->i32{return #(Host.make())}",
    ] {
        let report = compile_source(&source(text), &CompileOptions::default());
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0, "{}", report.to_json());
    }
}

#[test]
fn g4453_ordinary_static_member_preserves_actual_module_privacy() {
    let main = "import provider;fn main()->i32{return #(provider.Host.make())}";
    let provider = "pub struct Host{fn make()->i32{return 42}}";
    let report = zil_driver::compile_project(
        &review329_project(&[("main", main), ("provider", provider)]),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Ct, "{}", report.to_json());
    assert_eq!(report.verification_runs, 0);
}
