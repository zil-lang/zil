//! A real mixed Package keeps resource responsibility and an external loan.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, Stage, compile_path, compile_source,
};

const PROGRAM: &str = include_str!("mixed-owned-borrowed-package.zil");
const TWO_LEAF: &str = include_str!("two-leaf-stored-borrow.zil");
const OWNED_STORE_HEAD: &str = include_str!("owned-store-package-head.zil");
const LOCAL_EXCLUSIVE_SLOT: &str = include_str!("local-exclusive-package-slot.zil");
const WHOLE_TARGET_REPLACEMENT: &str = include_str!("whole-package-target-replacement.zil");
const MULTI_LEAF_PACKAGE: &str = include_str!("multi-leaf-mixed-package.zil");
const SLOT_REPLACEMENT: &str = include_str!("mutable-package-slot-replacement.zil");
const SHARED_EXCLUSIVE_REPLACEMENT: &str =
    include_str!("mixed-shared-exclusive-slot-replacement.zil");
const REPLACE_AND_RETURN_CHILD: &str = include_str!("replace-and-return-stored-child.zil");
const LOCAL_SLOT_REPLACEMENT: &str = include_str!("local-package-slot-replacement.zil");
const CONDITIONAL_SLOT_REPLACEMENT: &str = include_str!("conditional-package-slot-replacement.zil");
const NESTED_QUALIFIED_PACKAGE: &str = include_str!("nested-qualified-package.zil");
const NESTED_QUALIFIED_BYTEBUFFER: &str = include_str!("nested-qualified-bytebuffer.zil");
const NESTED_QUALIFIED_FAILURE: &str = include_str!("nested-qualified-failure.zil");

fn stdlib_store_head_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/resource-packages/stdlib-owned-store-package-head.zil")
}

fn cross_module_holder_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/resource-packages/cross-module-holder/main.zil")
}

fn source(name: &str, program: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), name, program)
}

#[test]
fn mixed_package_source_raw_and_wire() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for (name, program) in [
        ("mixed-owned-borrowed-package.zil", PROGRAM),
        ("two-leaf-stored-borrow.zil", TWO_LEAF),
        ("owned-store-package-head.zil", OWNED_STORE_HEAD),
        ("local-exclusive-package-slot.zil", LOCAL_EXCLUSIVE_SLOT),
        (
            "whole-package-target-replacement.zil",
            WHOLE_TARGET_REPLACEMENT,
        ),
        ("multi-leaf-mixed-package.zil", MULTI_LEAF_PACKAGE),
        ("mutable-package-slot-replacement.zil", SLOT_REPLACEMENT),
        (
            "mixed-shared-exclusive-slot-replacement.zil",
            SHARED_EXCLUSIVE_REPLACEMENT,
        ),
        (
            "replace-and-return-stored-child.zil",
            REPLACE_AND_RETURN_CHILD,
        ),
        ("local-package-slot-replacement.zil", LOCAL_SLOT_REPLACEMENT),
        ("nested-qualified-package.zil", NESTED_QUALIFIED_PACKAGE),
        (
            "nested-qualified-bytebuffer.zil",
            NESTED_QUALIFIED_BYTEBUFFER,
        ),
        ("nested-qualified-failure.zil", NESTED_QUALIFIED_FAILURE),
    ] {
        let source = source(name, program);
        let report = compile_source(
            &source,
            &CompileOptions {
                stop_after: Stage::Verify,
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

        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand_with_map_limits_target_context(
            &source,
            &syntax,
            &limits,
            &target,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &Default::default(),
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .core;
        let raw = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    }
}

#[test]
fn relay_uses_current_external_slot_after_identity_helper() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = source("mutable-package-slot-replacement.zil", SLOT_REPLACEMENT);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let mut raw = zil_types::check(&core, &target, &limits).unwrap();
    let observer = raw
        .functions
        .iter()
        .find(|f| f.name == "observe")
        .unwrap()
        .id;
    let relay = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "relay")
        .unwrap();
    let instructions = &mut relay.blocks[0].instructions;
    let call = instructions
        .iter()
        .position(|i| matches!(&i.kind, zil_ir::InstructionKind::Call { function, .. } if *function == observer))
        .unwrap();
    let loaded = instructions[..call]
        .iter()
        .rev()
        .find_map(|i| match i.kind {
            zil_ir::InstructionKind::Load(local) if local == relay.parameters[0] => i.result,
            _ => None,
        })
        .unwrap();
    if let zil_ir::InstructionKind::Call { arguments, .. } = &mut instructions[call].kind {
        arguments[0] = loaded;
    }
    // A raw call may use the typed Load directly. Its SSA leaf snapshot was
    // taken after replacement; the actual external slot is the authority.
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
}

#[test]
fn slot_update_cannot_extend_a_distinct_parameter_lifetime() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let original_source = source("mutable-package-slot-replacement.zil", SLOT_REPLACEMENT);
    let syntax = zil_syntax::parse(&original_source).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &original_source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: original_source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let raw = zil_types::check(&core, &target, &limits).unwrap();
    let mut original_plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let before = "fn replace<life a>(session:&mut Session<'a>,next:&'a mut i32)";
    let after = "fn replace<life a,life b>(session:&mut Session<'a>,next:&'b mut i32)";
    assert_eq!(SLOT_REPLACEMENT.matches(before).count(), 1);
    let distinct_source = source(
        "distinct-package-slot-lifetime.zil",
        &SLOT_REPLACEMENT.replace(before, after),
    );
    let syntax = zil_syntax::parse(&distinct_source).into_result().unwrap();
    let distinct_core = zil_expand::expand_with_map_limits_target_context(
        &distinct_source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: distinct_source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let distinct = zil_types::check(&distinct_core, &target, &limits).unwrap();
    let analyze_error = zil_analyze::analyze(&distinct, &limits).unwrap_err();
    assert!(
        analyze_error
            .iter()
            .any(|d| d.message.contains("declared lifetime relation")),
        "{analyze_error:?}"
    );
    original_plan.module_fingerprint = distinct.fingerprint();
    let verify_error =
        zil_verify::verify_module(distinct.clone(), original_plan.clone(), &target, &limits)
            .unwrap_err();
    assert!(
        verify_error
            .iter()
            .any(|d| d.message.contains("declared lifetime relation")),
        "{verify_error:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&distinct, &original_plan, &limits).unwrap();
    let wire_error = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        wire_error
            .iter()
            .any(|d| d.message.contains("declared lifetime relation")),
        "{wire_error:?}"
    );
}

#[test]
fn stored_exclusive_slot_does_not_leave_its_input_ssa_writable() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = source("mutable-package-slot-replacement.zil", SLOT_REPLACEMENT);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let mut raw = zil_types::check(&core, &target, &limits).unwrap();
    let replace = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "replace")
        .unwrap();
    let block = &mut replace.blocks[0];
    let store = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, zil_ir::InstructionKind::StoreStoredBorrow { .. }))
        .unwrap();
    let (address, span) = match &block.instructions[store] {
        zil_ir::Instruction {
            kind: zil_ir::InstructionKind::StoreStoredBorrow { value, .. },
            span,
            ..
        } => (*value, *span),
        _ => unreachable!(),
    };
    let literal = zil_ir::ValueId(replace.values.len() as u32);
    replace.values.push(zil_ir::Type::I32);
    block.instructions.insert(
        store + 1,
        zil_ir::Instruction {
            result: Some(literal),
            kind: zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(7)),
            span,
        },
    );
    let mut candidate = zil_analyze::analyze(&raw, &limits).unwrap();
    let replace = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "replace")
        .unwrap();
    replace.blocks[0].instructions.insert(
        store + 2,
        zil_ir::Instruction {
            result: None,
            kind: zil_ir::InstructionKind::WriteAddress {
                address,
                value: literal,
            },
            span,
        },
    );
    candidate.module_fingerprint = raw.fingerprint();
    let analyze_error = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        analyze_error
            .iter()
            .any(|d| d.message.contains("simultaneously live access")),
        "{analyze_error:?}"
    );
    let verify_error =
        zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap_err();
    assert!(
        verify_error
            .iter()
            .any(|d| d.message.contains("live incompatible loan")),
        "{verify_error:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    let wire_error = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        wire_error
            .iter()
            .any(|d| d.message.contains("live incompatible loan")),
        "{wire_error:?}"
    );
}

#[test]
fn helper_slot_write_does_not_leave_its_input_ssa_writable() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = source("mutable-package-slot-replacement.zil", SLOT_REPLACEMENT);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let mut raw = zil_types::check(&core, &target, &limits).unwrap();
    let replace_id = raw
        .functions
        .iter()
        .find(|f| f.name == "replace")
        .unwrap()
        .id;
    let relay = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "relay")
        .unwrap();
    let block = &mut relay.blocks[0];
    let call = block
        .instructions
        .iter()
        .position(|i| matches!(&i.kind, zil_ir::InstructionKind::Call { function, .. } if *function == replace_id))
        .unwrap();
    let (address, span) = match &block.instructions[call] {
        zil_ir::Instruction {
            kind: zil_ir::InstructionKind::Call { arguments, .. },
            span,
            ..
        } => (arguments[1], *span),
        _ => unreachable!(),
    };
    let literal = zil_ir::ValueId(relay.values.len() as u32);
    relay.values.push(zil_ir::Type::I32);
    block.instructions.insert(
        call + 1,
        zil_ir::Instruction {
            result: Some(literal),
            kind: zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(7)),
            span,
        },
    );
    let mut candidate = zil_analyze::analyze(&raw, &limits).unwrap();
    let relay = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "relay")
        .unwrap();
    relay.blocks[0].instructions.insert(
        call + 2,
        zil_ir::Instruction {
            result: None,
            kind: zil_ir::InstructionKind::WriteAddress {
                address,
                value: literal,
            },
            span,
        },
    );
    candidate.module_fingerprint = raw.fingerprint();
    let analyze_error = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        analyze_error
            .iter()
            .any(|d| d.message.contains("simultaneously live access")),
        "{analyze_error:?}"
    );
    let verify_error =
        zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap_err();
    assert!(
        verify_error
            .iter()
            .any(|d| d.message.contains("live incompatible loan")),
        "{verify_error:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    let wire_error = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        wire_error
            .iter()
            .any(|d| d.message.contains("live incompatible loan")),
        "{wire_error:?}"
    );
}

#[test]
fn helper_replacement_rejects_a_live_child_of_the_same_carrier_ssa() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = source("mutable-package-slot-replacement.zil", SLOT_REPLACEMENT);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let mut raw = zil_types::check(&core, &target, &limits).unwrap();
    let replace_id = raw
        .functions
        .iter()
        .find(|f| f.name == "replace")
        .unwrap()
        .id;
    let relay = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "relay")
        .unwrap();
    let block = &mut relay.blocks[0];
    let call = block
        .instructions
        .iter()
        .position(|i| matches!(&i.kind, zil_ir::InstructionKind::Call { function, .. } if *function == replace_id))
        .unwrap();
    let (carrier, span) = match &block.instructions[call] {
        zil_ir::Instruction {
            kind: zil_ir::InstructionKind::Call { arguments, .. },
            span,
            ..
        } => (arguments[0], *span),
        _ => unreachable!(),
    };
    let child = zil_ir::ValueId(relay.values.len() as u32);
    relay.values.push(zil_ir::Type::Address {
        mutable: true,
        inner: Box::new(zil_ir::Type::I32),
    });
    let literal = zil_ir::ValueId(relay.values.len() as u32);
    relay.values.push(zil_ir::Type::I32);
    block.instructions.insert(
        call,
        zil_ir::Instruction {
            result: Some(child),
            kind: zil_ir::InstructionKind::LoadStoredBorrow {
                storage: carrier,
                path: vec![zil_ir::PlaceProjection::Field(1)],
                mode: zil_ir::StoredBorrowLoad::Reborrow { mutable: true },
            },
            span,
        },
    );
    block.instructions.insert(
        call + 2,
        zil_ir::Instruction {
            result: Some(literal),
            kind: zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(7)),
            span,
        },
    );
    let mut candidate = zil_analyze::analyze(&raw, &limits).unwrap();
    let relay = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "relay")
        .unwrap();
    relay.blocks[0].instructions.insert(
        call + 3,
        zil_ir::Instruction {
            result: None,
            kind: zil_ir::InstructionKind::WriteAddress {
                address: child,
                value: literal,
            },
            span,
        },
    );
    candidate.module_fingerprint = raw.fingerprint();
    let analyze_error = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        analyze_error
            .iter()
            .any(|d| d.message.contains("simultaneously live access")),
        "{analyze_error:?}"
    );
    let verify_error =
        zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap_err();
    assert!(
        verify_error
            .iter()
            .any(|d| d.message.contains("live incompatible loan")),
        "{verify_error:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    let wire_error = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        wire_error
            .iter()
            .any(|d| d.message.contains("live incompatible loan")),
        "{wire_error:?}"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn mixed_package_native_both_optimizations() {
    for (name, program, expected) in [
        (
            "mixed-owned-borrowed-package.zil",
            PROGRAM,
            b"42\n".as_slice(),
        ),
        ("two-leaf-stored-borrow.zil", TWO_LEAF, b"42\n".as_slice()),
        (
            "owned-store-package-head.zil",
            OWNED_STORE_HEAD,
            b"1\n".as_slice(),
        ),
        (
            "local-exclusive-package-slot.zil",
            LOCAL_EXCLUSIVE_SLOT,
            b"0\n".as_slice(),
        ),
        (
            "whole-package-target-replacement.zil",
            WHOLE_TARGET_REPLACEMENT,
            b"0\n".as_slice(),
        ),
        (
            "multi-leaf-mixed-package.zil",
            MULTI_LEAF_PACKAGE,
            b"0\n".as_slice(),
        ),
        (
            "mutable-package-slot-replacement.zil",
            SLOT_REPLACEMENT,
            b"0\n".as_slice(),
        ),
        (
            "mixed-shared-exclusive-slot-replacement.zil",
            SHARED_EXCLUSIVE_REPLACEMENT,
            b"0\n".as_slice(),
        ),
        (
            "replace-and-return-stored-child.zil",
            REPLACE_AND_RETURN_CHILD,
            b"0\n".as_slice(),
        ),
        (
            "local-package-slot-replacement.zil",
            LOCAL_SLOT_REPLACEMENT,
            b"0\n".as_slice(),
        ),
        (
            "nested-qualified-package.zil",
            NESTED_QUALIFIED_PACKAGE,
            b"X0\n".as_slice(),
        ),
        (
            "nested-qualified-bytebuffer.zil",
            NESTED_QUALIFIED_BYTEBUFFER,
            b"0\n".as_slice(),
        ),
        (
            "nested-qualified-failure.zil",
            NESTED_QUALIFIED_FAILURE,
            b"F0\n".as_slice(),
        ),
    ] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let output = std::env::temp_dir().join(format!(
                "zil-mixed-package-{}-{name}-{optimization:?}",
                std::process::id()
            ));
            let report = compile_source(
                &source(name, program),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend: NativeBackend::Clang,
                    optimization,
                    entry: Some("main".into()),
                    output: Some(output.clone()),
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
            let output_result = std::process::Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(output_result.status.success());
            assert_eq!(output_result.stdout, expected);
            std::fs::remove_file(output).unwrap();
        }
    }
    for optimization in [Optimization::O0, Optimization::O2] {
        let output = std::env::temp_dir().join(format!(
            "zil-stdlib-store-head-{}-{optimization:?}",
            std::process::id()
        ));
        let report = compile_path(
            &stdlib_store_head_path(),
            &CompileOptions {
                stop_after: Stage::Build,
                backend: NativeBackend::Clang,
                optimization,
                entry: Some("main".into()),
                output: Some(output.clone()),
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
        let output_result = std::process::Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert!(output_result.status.success());
        assert_eq!(output_result.stdout, b"1\n");
        std::fs::remove_file(output).unwrap();
    }
}

#[test]
fn stdlib_owned_store_head_verifies() {
    let report = compile_path(
        &stdlib_store_head_path(),
        &CompileOptions {
            stop_after: Stage::Verify,
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
}

#[test]
#[cfg(target_os = "linux")]
fn cross_module_holder_replaces_the_actual_exclusive_target() {
    for optimization in [Optimization::O0, Optimization::O2] {
        let output = std::env::temp_dir().join(format!(
            "zil-cross-module-holder-{}-{optimization:?}",
            std::process::id()
        ));
        let report = compile_path(
            &cross_module_holder_path(),
            &CompileOptions {
                stop_after: Stage::Build,
                backend: NativeBackend::Clang,
                optimization,
                entry: Some("main".into()),
                output: Some(output.clone()),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let run = std::process::Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        std::fs::remove_file(output).unwrap();
    }
}

#[test]
fn same_stored_leaf_and_same_exclusive_input_reject() {
    for (before, after) in [
        ("let right=pair.right;", "let right=pair.left;"),
        ("hold(&mut left,&mut right)", "hold(&mut left,&mut left)"),
    ] {
        assert_eq!(TWO_LEAF.matches(before).count(), 1);
        let changed = TWO_LEAF.replacen(before, after, 1);
        let report = compile_source(
            &source("overlapping-stored-borrow.zil", &changed),
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
    assert_eq!(
        SLOT_REPLACEMENT.matches("    observe(session)\n}").count(),
        1
    );
    let replaced_target_write = SLOT_REPLACEMENT.replace(
        "    observe(session)\n}",
        "    observe(session)\n    *next=7\n}",
    );
    let report = compile_source(
        &source(
            "stored-replacement-target-write.zil",
            &replaced_target_write,
        ),
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
    let overlapping_return = REPLACE_AND_RETURN_CHILD.replace(
        "  *child=*child+1\n",
        "  let sibling=session.pointer\n  *child=*child+1\n  *sibling=*sibling+1\n",
    );
    let report = compile_source(
        &source("overlapping-returned-stored-child.zil", &overlapping_return),
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
    let overlapping = LOCAL_EXCLUSIVE_SLOT
        .replace(
            "let pointer=session.pointer",
            "let pointer=session.pointer\n        let second=session.pointer",
        )
        .replace(
            "*pointer=*pointer+1",
            "*pointer=*pointer+1\n        *second=*second+1",
        );
    let report = compile_source(
        &source("overlapping-local-package-slot.zil", &overlapping),
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
    let aliased = MULTI_LEAF_PACKAGE.replace("make(&mut a,&mut b)", "make(&mut a,&mut a)");
    let report = compile_source(
        &source("aliased-multi-leaf-package.zil", &aliased),
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
    for (before, after) in [
        (
            "relay(&mut session,&mut next)",
            "let old_child=session.pointer\n        relay(&mut session,&mut next)\n        *old_child=*old_child+1",
        ),
        (
            "relay(&mut session,&mut next)",
            "relay(&mut session,&mut old)",
        ),
    ] {
        let changed = SLOT_REPLACEMENT.replace(before, after);
        let report = compile_source(
            &source("conflicting-package-slot-replacement.zil", &changed),
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
    let live_child = LOCAL_SLOT_REPLACEMENT.replace(
        "session.pointer=&mut next",
        "let child=session.pointer\n        session.pointer=&mut next\n        *child=*child+1",
    );
    let report = compile_source(
        &source("local-replacement-with-live-child.zil", &live_child),
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
    let report = compile_source(
        &source(
            "conditional-package-slot-replacement.zil",
            CONDITIONAL_SLOT_REPLACEMENT,
        ),
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
