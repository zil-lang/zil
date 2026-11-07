//! Frozen and independent source inputs through the Verify boundary.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{LocalDecl, LocalId, Type};

fn checked(name: &str, text: &str) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(FileId(0), ModuleId(0), name, text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    if name.starts_with("slot_") {
        let main = typed.functions.iter().find(|f| f.name == "main").unwrap();
        assert!(
            main.blocks
                .iter()
                .flat_map(|b| &b.instructions)
                .any(|i| matches!(
                    i.kind,
                    zil_ir::InstructionKind::TakeOwnedArrayElement { .. }
                )),
            "{name} must exercise the internal single slot"
        );
    }
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed, plan, &target, &limits)
        .unwrap_or_else(|errors| panic!("{name}: {errors:?}"));
}

#[test]
fn original_source_bytes_reach_verify() {
    checked(
        "original",
        include_str!("forwarded_callback_types_fixtures/original.zil"),
    );
}

#[test]
fn fnmut_and_loop_forwarding_reach_verify() {
    checked(
        "fnmut",
        include_str!("../../../tests/borrowed-callable-arguments/fnmut.zil"),
    );
    checked(
        "branch_loop",
        include_str!("../../../tests/borrowed-callable-arguments/branch_loop.zil"),
    );
}

#[test]
fn stronger_fn_survives_nested_mutable_call_and_return() {
    checked(
        "fn_to_mut",
        "fn step<F:FnMut(i32)->i32>(f:F)->F{var current=f;current(1);return current}fn relay<F:Fn(i32)->i32>(f:F)->F{return step(f)}fn main()->i32{let f=(n:i32)=>n+1;let current=relay(f);return current(2)-3}",
    );
}

#[test]
fn owning_event_callback_in_ordinary_caller_internal_slot_reaches_verify() {
    checked(
        "owned_event_caller",
        include_str!("forwarded_callback_analysis_fixtures/owned_event_caller.zil"),
    );
}

#[test]
fn owning_event_callback_can_leave_mutable_slot_for_new_local() {
    let prelude = "resource R{n:i32}package Event{manages bytes:ByteBuffer;n:i32}package Reply{manages bytes:ByteBuffer;n:i32}fn keep<F:FnOnce(Event)->Reply>(f:F)->F{return f}";
    for (name, binding) in [
        ("into_let", "let g=keep(f)"),
        ("into_var", "var g=keep(f)"),
        ("slot_into_let", "f=keep(f);let g=keep(f)"),
        ("slot_into_var", "f=keep(f);var g=keep(f)"),
    ] {
        let source = format!(
            "{prelude}fn main()->i32{{let r=R{{n:7}};var f=move(e:Event)->Reply=>{{return Reply{{bytes:give e.bytes,n:e.n+r.n}}}};{binding};let p=g(Event{{bytes:bytes_new(),n:3}});return p.n-10}}"
        );
        checked(name, &source);
    }
}

#[test]
fn unused_uninitialized_same_type_private_slot_is_allowed() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "owned_slot_raw.zil",
        include_str!("forwarded_callback_analysis_fixtures/owned_event_caller.zil"),
    );
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let mut typed = zil_types::check(&core, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let main = typed
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let slot = main
        .locals
        .iter()
        .find(|l| {
            matches!(&l.ty,
                Type::Array { element, length: 1 } if matches!(element.as_ref(), Type::Closure(_))
            )
        })
        .unwrap()
        .ty
        .clone();
    let id = LocalId(main.locals.len() as u32);
    main.locals.push(LocalDecl {
        id,
        name: "unrelated_slot".into(),
        ty: slot,
        mutable: true,
        span: main.span,
    });
    plan.module_fingerprint = typed.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    zil_verify::verify_module(typed, plan, &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
}

#[test]
fn complete_error_union_store_companion_reaches_verify() {
    checked(
        "owned_companion",
        include_str!("../../../tests/borrowed-callable-arguments/owned_companion.zil"),
    );
}

#[test]
fn frozen_six_copy_and_owned_signatures_reach_verify() {
    for (name, source) in [
        (
            "fn-copy",
            include_str!(
                "../../../tests/callable-instance-forwarding/frozen-prep/probes/fn-copy.zil"
            ),
        ),
        (
            "fnmut-copy",
            include_str!(
                "../../../tests/callable-instance-forwarding/frozen-prep/probes/fnmut-copy.zil"
            ),
        ),
        (
            "fnonce-copy",
            include_str!(
                "../../../tests/callable-instance-forwarding/frozen-prep/probes/fnonce-copy.zil"
            ),
        ),
        (
            "fn-owned",
            include_str!(
                "../../../tests/callable-instance-forwarding/frozen-prep/probes/fn-owned.zil"
            ),
        ),
        (
            "fnmut-owned",
            include_str!(
                "../../../tests/callable-instance-forwarding/frozen-prep/probes/fnmut-owned.zil"
            ),
        ),
        (
            "fnonce-owned",
            include_str!(
                "../../../tests/callable-instance-forwarding/frozen-prep/probes/fnonce-owned.zil"
            ),
        ),
    ] {
        checked(name, source);
    }
}

#[test]
fn frozen_and_extended_three_module_sources_reach_verify() {
    use zil_driver::{CompileOptions, ProjectSources, Stage, compile_project};
    for directory in ["frozen-prep/project", "extended"] {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/callable-instance-forwarding")
            .join(directory);
        let names = ["main", "model", "relay"];
        let project = ProjectSources {
            root: ModuleId(0),
            modules: names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let path = root.join(format!("{name}.zil"));
                    SourceUnit::new(
                        FileId(index as u32),
                        ModuleId(index as u32),
                        path.to_string_lossy().into_owned(),
                        std::fs::read_to_string(path).unwrap(),
                    )
                })
                .collect(),
            paths: [
                (vec!["model".into()], ModuleId(1)),
                (vec!["relay".into()], ModuleId(2)),
            ]
            .into_iter()
            .collect(),
        };
        let report = compile_project(
            &project,
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            zil_base::ResultCategory::Success,
            "{directory}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}
