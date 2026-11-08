//! A real mixed Package keeps resource responsibility and an external loan.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, Stage, compile_path, compile_source,
};
use zil_ir::Type;

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
const NESTED_QUALIFIED_CHILD_MOVE: &str = include_str!("nested-qualified-child-move.zil");
const NESTED_QUALIFIED_CHILD_REFILL: &str = include_str!("nested-qualified-child-refill.zil");
const NESTED_QUALIFIED_CHILD_BORROW: &str = include_str!("nested-qualified-child-borrow.zil");
const NESTED_QUALIFIED_CHILD_DIRECT_LOCAL: &str =
    include_str!("nested-qualified-child-direct-local.zil");
const NESTED_QUALIFIED_SLOT_EXTRACTION: &str = include_str!("nested-qualified-slot-extraction.zil");
const NESTED_QUALIFIED_SLOT_HELPER: &str =
    include_str!("nested-qualified-slot-helper-exchange.zil");
const MOVE_OUT: &str = include_str!("../stored-borrow-move-out.zil");
const EXCLUSIVE_RECORD_TRANSFER: &str = include_str!("exclusive-record-transfer.zil");
const SHARED_RECORD_COPY: &str = include_str!("shared-record-copy.zil");
const EXCLUSIVE_RECORD_PARAMETER_DUPLICATE: &str =
    include_str!("exclusive-record-parameter-duplicate.zil");
const EXCLUSIVE_RECORD_SHARED_HEAD_COPY: &str =
    include_str!("exclusive-record-shared-head-copy.zil");
const EXCLUSIVE_RECORD_BRANCH_REFILL: &str = include_str!("exclusive-record-branch-refill.zil");
const EXCLUSIVE_RECORD_BRANCH_UNINITIALIZED: &str =
    include_str!("exclusive-record-branch-uninitialized.zil");
const EXCLUSIVE_RECORD_AFTER_MOVE: &str = include_str!("exclusive-record-after-move.zil");
const EXCLUSIVE_PAIR: &str = include_str!("exclusive-pair.zil");
const TWO_EXCLUSIVE_HOLDERS: &str = include_str!("two-exclusive-holders.zil");
const SHARED_HOLDER_BRANCH_REFILL: &str = include_str!("shared-holder-branch-refill.zil");
const SHARED_HOLDER_BRANCH_COPY_HOLE: &str = include_str!("shared-holder-branch-copy-hole.zil");
const EXCLUSIVE_HOLDER_LOOP_RETENTION: &str = include_str!("exclusive-holder-loop-retention.zil");
const EXCLUSIVE_HOLDER_LOOP_TRANSFER: &str = include_str!("exclusive-holder-loop-transfer.zil");
const EXCLUSIVE_HOLDER_LOOP_REFILL: &str = include_str!("exclusive-holder-loop-refill.zil");
const EXCLUSIVE_HOLDER_LOOP_RECONSTRUCT: &str =
    include_str!("exclusive-holder-loop-reconstruct.zil");
const EXCLUSIVE_ADDRESS_SEQUENTIAL_REBORROWS: &str =
    include_str!("exclusive-address-sequential-reborrows.zil");
const EXCLUSIVE_NESTED_HOLDER_TRANSFER: &str = include_str!("exclusive-nested-holder-transfer.zil");

fn stdlib_store_head_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/resource-packages/stdlib-owned-store-package-head.zil")
}

fn cross_module_holder_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/resource-packages/cross-module-holder/main.zil")
}

fn move_out_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/stored-borrow-move-out.zil")
}

fn shared_move_out_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/resource-packages/shared-slot-move-out.zil")
}

fn local_move_out_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/resource-packages/local-stored-slot-extraction.zil")
}

fn source(name: &str, program: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), name, program)
}

fn typed_source(
    name: &str,
    program: &str,
    target: &TargetProfile,
    limits: &Limits,
) -> zil_ir::TypedModule {
    let input = source(name, program);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &input,
        &syntax,
        limits,
        target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    zil_types::check(&core, target, limits).unwrap()
}

#[test]
fn constructor_transfer_blocks_later_ssa_accesses() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let original = typed_source(
        "two-exclusive-holders.zil",
        TWO_EXCLUSIVE_HOLDERS,
        &target,
        &limits,
    );
    let peek = original
        .functions
        .iter()
        .find(|f| f.name == "peek")
        .unwrap()
        .id;
    for mode in ["read", "write", "reborrow", "call"] {
        let mut raw = original.clone();
        let function = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "use_two")
            .unwrap();
        let block = &mut function.blocks[0];
        let transferred = block
            .instructions
            .iter()
            .find_map(|i| match &i.kind {
                zil_ir::InstructionKind::MakeAggregate { fields } => Some(fields[0]),
                _ => None,
            })
            .unwrap();
        let position = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, zil_ir::InstructionKind::WriteAddress { .. }))
            .unwrap();
        let (safe, value) = match block.instructions[position].kind {
            zil_ir::InstructionKind::WriteAddress { address, value } => (address, value),
            _ => unreachable!(),
        };
        let kind = match mode {
            "read" => zil_ir::InstructionKind::ReadAddress { address: safe },
            "write" => zil_ir::InstructionKind::WriteAddress {
                address: safe,
                value,
            },
            "reborrow" => zil_ir::InstructionKind::ReborrowAddress {
                address: safe,
                mutable: true,
            },
            "call" => zil_ir::InstructionKind::Call {
                function: peek,
                arguments: vec![safe],
            },
            _ => unreachable!(),
        };
        let result = if mode == "write" {
            None
        } else {
            let result = zil_ir::ValueId(function.values.len() as u32);
            function.values.push(if mode == "reborrow" {
                function.values[safe.0 as usize].clone()
            } else {
                Type::I32
            });
            Some(result)
        };
        let span = block.instructions[position].span;
        block
            .instructions
            .insert(position, zil_ir::Instruction { result, kind, span });
        // The candidate comes from a fresh, independently verified safe child
        // access; only its operand is changed to the transferred constructor input.
        let mut claim = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap();
        let function = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "use_two")
            .unwrap();
        match &mut function.blocks[0].instructions[position].kind {
            zil_ir::InstructionKind::ReadAddress { address }
            | zil_ir::InstructionKind::WriteAddress { address, .. }
            | zil_ir::InstructionKind::ReborrowAddress { address, .. } => *address = transferred,
            zil_ir::InstructionKind::Call { arguments, .. } => arguments[0] = transferred,
            _ => unreachable!(),
        }
        claim.module_fingerprint = raw.fingerprint();
        let analyze = zil_analyze::analyze(&raw, &limits).unwrap_err();
        assert!(
            format!("{analyze:?}").contains("used after transfer"),
            "{mode}: {analyze:?}"
        );
        let verify =
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
        assert!(
            format!("{verify:?}").contains("used after transfer"),
            "{mode}: {verify:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        let verify = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert!(
            format!("{verify:?}").contains("used after transfer"),
            "{mode}: {verify:?}"
        );
    }
}

#[test]
fn shared_holder_branch_copy_requires_every_slot_filled() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for (name, program, expected) in [
        (
            "shared-holder-branch-refill.zil",
            SHARED_HOLDER_BRANCH_REFILL,
            ResultCategory::Success,
        ),
        (
            "shared-holder-branch-copy-hole.zil",
            SHARED_HOLDER_BRANCH_COPY_HOLE,
            ResultCategory::SafeReject,
        ),
    ] {
        let report = compile_source(
            &source(name, program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(report.status, expected, "{}", report.to_json());
    }
    let mut raw = typed_source(
        "shared-holder-branch-refill.zil",
        SHARED_HOLDER_BRANCH_REFILL,
        &target,
        &limits,
    );
    let mut claim = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let function = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "read_copy")
        .unwrap();
    let mut removed = 0;
    for block in &mut function.blocks {
        block.instructions.retain(|i| {
            if matches!(i.kind, zil_ir::InstructionKind::StoreStoredBorrow { .. }) {
                removed += 1;
                false
            } else {
                true
            }
        });
    }
    assert_eq!(removed, 1);
    claim.module_fingerprint = raw.fingerprint();
    let analyze = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        format!("{analyze:?}").contains("whole carrier copy has a vacant"),
        "{analyze:?}"
    );
    let verify =
        zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
    assert!(
        format!("{verify:?}").contains("whole carrier copy has a vacant"),
        "{verify:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
    let verify = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        format!("{verify:?}").contains("whole carrier copy has a vacant"),
        "{verify:?}"
    );
}

#[test]
fn constructor_retains_a_child_across_saved_address_reload() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut raw = typed_source(
        "two-exclusive-holders.zil",
        TWO_EXCLUSIVE_HOLDERS,
        &target,
        &limits,
    );
    let function = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "use_two")
        .unwrap();
    let block = &mut function.blocks[0];
    let constructors: Vec<_> = block
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(position, i)| match &i.kind {
            zil_ir::InstructionKind::MakeAggregate { fields } => Some((position, fields[0])),
            _ => None,
        })
        .collect();
    assert_eq!(constructors.len(), 2);
    let (position, address) = constructors[0];
    let second = constructors[1].1;
    let saved_child = zil_ir::ValueId(function.values.len() as u32);
    function
        .values
        .push(function.values[address.0 as usize].clone());
    let saved = zil_ir::LocalId(function.locals.len() as u32);
    let span = block.instructions[position].span;
    function.locals.push(zil_ir::LocalDecl {
        id: saved,
        name: "saved_address".into(),
        ty: function.values[address.0 as usize].clone(),
        mutable: false,
        span,
    });
    block.instructions.insert(
        1,
        zil_ir::Instruction {
            result: None,
            kind: zil_ir::InstructionKind::StorageLive(saved),
            span,
        },
    );
    block.instructions.splice(
        position + 1..position + 1,
        [
            zil_ir::Instruction {
                result: Some(saved_child),
                kind: zil_ir::InstructionKind::ReborrowAddress {
                    address,
                    mutable: true,
                },
                span,
            },
            zil_ir::Instruction {
                result: None,
                kind: zil_ir::InstructionKind::Store {
                    local: saved,
                    value: saved_child,
                },
                span,
            },
        ],
    );
    let exit = block
        .instructions
        .iter()
        .rposition(|i| matches!(i.kind, zil_ir::InstructionKind::ScopeExit { .. }))
        .unwrap();
    block.instructions.insert(
        exit,
        zil_ir::Instruction {
            result: None,
            kind: zil_ir::InstructionKind::StorageDead(saved),
            span,
        },
    );
    let mut claim = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap();
    let function = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "use_two")
        .unwrap();
    let load = function.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| i.result == Some(second))
        .unwrap();
    assert!(matches!(
        load.kind,
        zil_ir::InstructionKind::Load(_) | zil_ir::InstructionKind::TakeLocal { .. }
    ));
    load.kind = zil_ir::InstructionKind::Load(saved);
    claim.module_fingerprint = raw.fingerprint();
    let analyze = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        format!("{analyze:?}").contains("simultaneously live access overlaps"),
        "{analyze:?}"
    );
    let verify =
        zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
    assert!(
        format!("{verify:?}").contains("actual target intersects"),
        "{verify:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
    let verify = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        format!("{verify:?}").contains("actual target intersects"),
        "{verify:?}"
    );
}

#[test]
fn loop_slot_refill_cannot_keep_the_transferred_ssa_in_an_address_local() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    // A separate external address makes the unmodified saved-local access legal.
    let program = EXCLUSIVE_HOLDER_LOOP_REFILL
        .replace(
            "pointer: &'a mut i32)",
            "pointer: &'a mut i32, other: &'a mut i32)",
        )
        .replace("var number = 0", "var number = 0\n    var other = 0")
        .replace("refill(&mut number)", "refill(&mut number, &mut other)");
    let mut raw = typed_source("loop-slot-local-transfer.zil", &program, &target, &limits);
    let function = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "refill")
        .unwrap();
    let other = function.parameters[1];
    let (bi, position, transferred) = function
        .blocks
        .iter()
        .enumerate()
        .find_map(|(bi, b)| {
            b.instructions
                .iter()
                .enumerate()
                .find_map(|(ii, i)| match i.kind {
                    zil_ir::InstructionKind::StoreStoredBorrow { value, .. } => {
                        Some((bi, ii, value))
                    }
                    _ => None,
                })
        })
        .unwrap();
    let span = function.blocks[bi].instructions[position].span;
    let address_type = function.values[transferred.0 as usize].clone();
    let saved = zil_ir::LocalId(function.locals.len() as u32);
    function.locals.push(zil_ir::LocalDecl {
        id: saved,
        name: "saved_address".into(),
        ty: address_type.clone(),
        mutable: true,
        span,
    });
    let safe = zil_ir::ValueId(function.values.len() as u32);
    let reloaded = zil_ir::ValueId(safe.0 + 1);
    let read = zil_ir::ValueId(safe.0 + 2);
    function
        .values
        .extend([address_type.clone(), address_type, Type::I32]);
    let block = &mut function.blocks[bi];
    block.instructions.splice(
        position..position,
        [
            zil_ir::Instruction {
                result: Some(safe),
                kind: zil_ir::InstructionKind::Load(other),
                span,
            },
            zil_ir::Instruction {
                result: None,
                kind: zil_ir::InstructionKind::Store {
                    local: saved,
                    value: safe,
                },
                span,
            },
        ],
    );
    block.instructions.splice(
        position + 3..position + 3,
        [
            zil_ir::Instruction {
                result: Some(reloaded),
                kind: zil_ir::InstructionKind::Load(saved),
                span,
            },
            zil_ir::Instruction {
                result: Some(read),
                kind: zil_ir::InstructionKind::ReadAddress { address: reloaded },
                span,
            },
        ],
    );
    function.blocks[function.entry.0 as usize]
        .instructions
        .insert(
            1,
            zil_ir::Instruction {
                result: None,
                kind: zil_ir::InstructionKind::StorageLive(saved),
                span,
            },
        );
    let exit = function
        .blocks
        .iter_mut()
        .find(|b| matches!(b.terminator, zil_ir::Terminator::Return(_)))
        .unwrap();
    let position = exit
        .instructions
        .iter()
        .rposition(|i| matches!(i.kind, zil_ir::InstructionKind::ScopeExit { .. }))
        .unwrap();
    exit.instructions.insert(
        position,
        zil_ir::Instruction {
            result: None,
            kind: zil_ir::InstructionKind::StorageDead(saved),
            span,
        },
    );
    let mut claim = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let function = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "refill")
        .unwrap();
    let store = function.blocks[bi]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, zil_ir::InstructionKind::Store { local, .. } if local == saved))
        .unwrap();
    let zil_ir::InstructionKind::Store { value, .. } = &mut store.kind else {
        unreachable!()
    };
    *value = transferred;
    // The same loop-site slot keeper is refilled, then the saved local is read.
    // Initialization claims remain identical; only the stored SSA operand changes.
    claim.module_fingerprint = raw.fingerprint();
    let analyze = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        format!("{analyze:?}").contains("transferred twice"),
        "{analyze:?}"
    );
    let verify =
        zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
    assert!(
        format!("{verify:?}").contains("transferred twice"),
        "{verify:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
    let verify = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        format!("{verify:?}").contains("transferred twice"),
        "{verify:?}"
    );
}

#[test]
fn constructors_reject_overlapping_parent_and_child_addresses() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for (program, function_name, pair) in [
        (EXCLUSIVE_PAIR, "make", true),
        (TWO_EXCLUSIVE_HOLDERS, "use_two", false),
    ] {
        let mut raw = typed_source("constructor-alias.zil", program, &target, &limits);
        let mut claim = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap();
        let function = raw
            .functions
            .iter_mut()
            .find(|f| f.name == function_name)
            .unwrap();
        let inputs: Vec<_> = function
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .filter_map(|i| match &i.kind {
                zil_ir::InstructionKind::MakeAggregate { fields } => Some(fields.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(inputs.len(), 2);
        if pair {
            let second = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| i.result == Some(inputs[1]))
                .unwrap();
            second.kind = zil_ir::InstructionKind::ReborrowAddress {
                address: inputs[0],
                mutable: true,
            };
        } else {
            let parent = inputs[0];
            let child = zil_ir::ValueId(function.values.len() as u32);
            function
                .values
                .push(function.values[parent.0 as usize].clone());
            let block = function
                .blocks
                .iter_mut()
                .find(|b| {
                    b.instructions
                        .iter()
                        .any(|i| matches!(i.kind, zil_ir::InstructionKind::MakeAggregate { .. }))
                })
                .unwrap();
            let position = block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, zil_ir::InstructionKind::MakeAggregate { .. }))
                .unwrap();
            let span = block.instructions[position].span;
            let zil_ir::InstructionKind::MakeAggregate { fields } =
                &mut block.instructions[position].kind
            else {
                unreachable!()
            };
            fields[0] = child;
            block.instructions.insert(
                position,
                zil_ir::Instruction {
                    result: Some(child),
                    kind: zil_ir::InstructionKind::ReborrowAddress {
                        address: parent,
                        mutable: true,
                    },
                    span,
                },
            );
            // The safe carrier stores the child; its parent remains an unconsumed SSA.
            claim = zil_analyze::analyze(&raw, &limits).unwrap();
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap();
            let function = raw
                .functions
                .iter_mut()
                .find(|f| f.name == function_name)
                .unwrap();
            // This parent has not itself been consumed by either constructor.
            let second = function
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .filter_map(|i| match &mut i.kind {
                    zil_ir::InstructionKind::MakeAggregate { fields } => Some(fields),
                    _ => None,
                })
                .nth(1)
                .unwrap();
            second[0] = parent;
        }
        claim.module_fingerprint = raw.fingerprint();
        let analyze = zil_analyze::analyze(&raw, &limits).unwrap_err();
        assert!(
            format!("{analyze:?}").contains("simultaneously live access overlaps"),
            "pair={pair}: {analyze:?}"
        );
        let verify =
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
        assert!(
            format!("{verify:?}").contains("actual target intersects"),
            "pair={pair}: {verify:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
        let verify = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert!(
            format!("{verify:?}").contains("actual target intersects"),
            "pair={pair}: {verify:?}"
        );
    }
}

#[test]
fn exclusive_holder_loop_preserves_transfers_and_retention() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for (name, program, expected) in [
        (
            "exclusive-holder-loop-transfer.zil",
            EXCLUSIVE_HOLDER_LOOP_TRANSFER,
            ResultCategory::Success,
        ),
        (
            "exclusive-holder-loop-refill.zil",
            EXCLUSIVE_HOLDER_LOOP_REFILL,
            ResultCategory::Success,
        ),
        (
            "exclusive-holder-loop-reconstruct.zil",
            EXCLUSIVE_HOLDER_LOOP_RECONSTRUCT,
            ResultCategory::Success,
        ),
        (
            "exclusive-holder-loop-retention.zil",
            EXCLUSIVE_HOLDER_LOOP_RETENTION,
            ResultCategory::SafeReject,
        ),
    ] {
        let report = compile_source(
            &source(name, program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(report.status, expected, "{name}: {}", report.to_json());
    }
    let mut raw = typed_source(
        "exclusive-holder-loop-transfer.zil",
        EXCLUSIVE_HOLDER_LOOP_TRANSFER,
        &target,
        &limits,
    );
    let function = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "keep_two")
        .unwrap();
    let pointer = function.parameters[2];
    let left = function
        .locals
        .iter()
        .find(|l| l.name == "left")
        .unwrap()
        .id;
    let right = function
        .locals
        .iter()
        .find(|l| l.name == "right")
        .unwrap()
        .id;
    let input = zil_ir::ValueId(function.values.len() as u32);
    function
        .values
        .push(function.locals[pointer.0 as usize].ty.clone());
    let block = function
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions.iter().any(
                |i| matches!(i.kind, zil_ir::InstructionKind::TakeLocal { local } if local == left),
            )
        })
        .unwrap();
    let position = block
        .instructions
        .iter()
        .position(
            |i| matches!(i.kind, zil_ir::InstructionKind::TakeLocal { local } if local == left),
        )
        .unwrap();
    let span = block.instructions[position].span;
    block.instructions.insert(
        position,
        zil_ir::Instruction {
            result: Some(input),
            kind: zil_ir::InstructionKind::Load(pointer),
            span,
        },
    );
    let mut claim = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap();
    let function = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "keep_two")
        .unwrap();
    let mut changed_constructor = false;
    for block in &mut function.blocks {
        for instruction in &mut block.instructions {
            if matches!(instruction.kind, zil_ir::InstructionKind::TakeLocal { local } if local == left)
                && !changed_constructor
            {
                instruction.kind = zil_ir::InstructionKind::MakeAggregate {
                    fields: vec![input],
                };
                changed_constructor = true;
            }
        }
    }
    assert!(changed_constructor);
    // The second conditional assignment now keeps another iteration's result.
    let stores: Vec<_> = function
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(bi, b)| {
            b.instructions
                .iter()
                .enumerate()
                .filter_map(move |(ii, i)| {
                    matches!(i.kind, zil_ir::InstructionKind::Store { local, .. } if local == left)
                        .then_some((bi, ii))
                })
        })
        .collect();
    assert_eq!(stores.len(), 3);
    let (bi, ii) = stores[2];
    let zil_ir::InstructionKind::Store { local, .. } =
        &mut function.blocks[bi].instructions[ii].kind
    else {
        unreachable!()
    };
    *local = right;
    // Replacing TakeLocal changes this local's initialization facts. Refresh
    // just those claims so Verify must reach its independent actual-loan check.
    let mut predecessors = vec![Vec::new(); function.blocks.len()];
    for (bi, block) in function.blocks.iter().enumerate() {
        let successors = match block.terminator {
            zil_ir::Terminator::Jump(next) => vec![next],
            zil_ir::Terminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![then_block, else_block],
            zil_ir::Terminator::Return(_) => Vec::new(),
            ref other => panic!("unexpected loop terminator {other:?}"),
        };
        for next in successors {
            predecessors[next.0 as usize].push(bi);
        }
    }
    let mut initialized_in = vec![true; function.blocks.len()];
    let mut initialized_out = initialized_in.clone();
    loop {
        let mut changed = false;
        for (bi, block) in function.blocks.iter().enumerate() {
            let incoming = if block.id == function.entry {
                function.parameters.contains(&left)
            } else {
                predecessors[bi].iter().all(|p| initialized_out[*p])
            };
            let mut outgoing = incoming;
            for i in &block.instructions {
                match i.kind {
                    zil_ir::InstructionKind::Store { local, .. } if local == left => {
                        outgoing = true
                    }
                    zil_ir::InstructionKind::TakeLocal { local }
                    | zil_ir::InstructionKind::StorageLive(local)
                    | zil_ir::InstructionKind::StorageDead(local)
                        if local == left =>
                    {
                        outgoing = false
                    }
                    _ => {}
                }
            }
            changed |= initialized_in[bi] != incoming || initialized_out[bi] != outgoing;
            initialized_in[bi] = incoming;
            initialized_out[bi] = outgoing;
        }
        if !changed {
            break;
        }
    }
    for (bi, block) in claim.functions[function.id.0 as usize]
        .blocks
        .iter_mut()
        .enumerate()
    {
        for (set, initialized) in [
            (&mut block.initialized_in, initialized_in[bi]),
            (&mut block.initialized_out, initialized_out[bi]),
        ] {
            set.retain(|local| *local != left);
            if initialized {
                set.push(left);
                set.sort();
            }
        }
    }
    claim.module_fingerprint = raw.fingerprint();
    let analyze = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        format!("{analyze:?}").contains("simultaneously live access overlaps"),
        "{analyze:?}"
    );
    let verify =
        zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err();
    assert!(
        format!("{verify:?}").contains("actual target intersects"),
        "{verify:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
    let verify = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        format!("{verify:?}").contains("actual target intersects"),
        "{verify:?}"
    );
}

#[test]
fn exclusive_address_storage_moves_preserve_explicit_reborrows_and_nested_children() {
    for (name, program) in [
        (
            "exclusive-address-sequential-reborrows.zil",
            EXCLUSIVE_ADDRESS_SEQUENTIAL_REBORROWS,
        ),
        (
            "exclusive-nested-holder-transfer.zil",
            EXCLUSIVE_NESTED_HOLDER_TRANSFER,
        ),
    ] {
        let report = compile_source(
            &source(name, program),
            &CompileOptions {
                stop_after: Stage::Verify,
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
    for (name, program) in [
        (
            "exclusive-holder-refill-after-move.zil",
            EXCLUSIVE_HOLDER_LOOP_REFILL.replace(
                "holder.pointer = previous",
                "holder.pointer = previous\n        *previous += 1",
            ),
        ),
        (
            "exclusive-nested-holder-duplicate.zil",
            EXCLUSIVE_NESTED_HOLDER_TRANSFER.replace(
                "return Outer::<'a>",
                "var moved = holder\n    return Outer::<'a>",
            ),
        ),
    ] {
        let report = compile_source(
            &source(name, &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == zil_base::Phase::Analyze && d.message.contains("initial")),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn exclusive_record_transfer_rejects_copy_and_raw_duplicate() {
    let input = source("exclusive-record-transfer.zil", EXCLUSIVE_RECORD_TRANSFER);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let valid = compile_source(
        &input,
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(valid.status, ResultCategory::Success, "{}", valid.to_json());
    let shared = compile_source(
        &source("shared-record-copy.zil", SHARED_RECORD_COPY),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        shared.status,
        ResultCategory::Success,
        "{}",
        shared.to_json()
    );

    let duplicate = EXCLUSIVE_RECORD_TRANSFER.replace(
        "var moved = holder",
        "var moved = holder\n    var duplicate = holder",
    );
    assert_ne!(duplicate, EXCLUSIVE_RECORD_TRANSFER);
    let rejected = compile_source(
        &source("exclusive-record-duplicate.zil", &duplicate),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        rejected.status,
        ResultCategory::SafeReject,
        "{}",
        rejected.to_json()
    );
    for (name, program, status) in [
        (
            "exclusive-record-parameter-duplicate.zil",
            EXCLUSIVE_RECORD_PARAMETER_DUPLICATE,
            ResultCategory::SafeReject,
        ),
        (
            "exclusive-record-shared-head-copy.zil",
            EXCLUSIVE_RECORD_SHARED_HEAD_COPY,
            ResultCategory::Unsupported,
        ),
    ] {
        let rejected = compile_source(
            &source(name, program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(rejected.status, status, "{name}: {}", rejected.to_json());
    }

    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &input,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let typed = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&typed, &limits).unwrap();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();

    let mut forged_load = typed.clone();
    let taken = forged_load
        .functions
        .iter_mut()
        .find_map(|function| {
            let locals = &function.locals;
            function.blocks.iter_mut().find_map(|block| {
                block.instructions.iter_mut().find_map(|instruction| {
                    let zil_ir::InstructionKind::TakeLocal { local } = instruction.kind else {
                        return None;
                    };
                    if matches!(&locals[local.0 as usize].ty,
                    Type::BorrowedStorage { storage, .. }
                    if matches!(storage.as_ref(), Type::Record { .. }))
                    {
                        Some((instruction, local))
                    } else {
                        None
                    }
                })
            })
        })
        .unwrap();
    taken.0.kind = zil_ir::InstructionKind::Load(taken.1);
    let error = zil_analyze::analyze(&forged_load, &limits).unwrap_err();
    assert!(format!("{error:?}").contains("copy of actual exclusive stored address carrier"));
    let mut load_claim = plan.clone();
    load_claim.module_fingerprint = forged_load.fingerprint();
    let error =
        zil_verify::verify_module(forged_load.clone(), load_claim.clone(), &target, &limits)
            .unwrap_err();
    assert!(format!("{error:?}").contains("actual exclusive stored carrier was copied"));
    let wire = zil_ir::wire::encode_module_candidate(&forged_load, &load_claim, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());

    let mut forged_duplicate = typed;
    let block = forged_duplicate
        .functions
        .iter_mut()
        .find_map(|function| {
            let locals = &function.locals;
            function.blocks.iter_mut().find(|block| {
                block.instructions.iter().any(|instruction| {
                    matches!(instruction.kind, zil_ir::InstructionKind::TakeLocal { local }
                if matches!(&locals[local.0 as usize].ty,
                    Type::BorrowedStorage { storage, .. }
                    if matches!(storage.as_ref(), Type::Record { .. })))
                })
            })
        })
        .unwrap();
    let taken_result = block
        .instructions
        .iter()
        .find_map(|instruction| {
            if let zil_ir::InstructionKind::TakeLocal { .. } = instruction.kind {
                instruction.result
            } else {
                None
            }
        })
        .unwrap();
    let store = block
        .instructions
        .iter()
        .position(|instruction| {
            matches!(instruction.kind, zil_ir::InstructionKind::Store { value, .. }
            if value == taken_result)
        })
        .unwrap();
    block
        .instructions
        .insert(store + 1, block.instructions[store].clone());
    let error = zil_analyze::analyze(&forged_duplicate, &limits).unwrap_err();
    assert!(
        format!("{error:?}").contains("transferred twice"),
        "{error:?}"
    );
    let mut duplicate_claim = plan;
    duplicate_claim.module_fingerprint = forged_duplicate.fingerprint();
    let error = zil_verify::verify_module(
        forged_duplicate.clone(),
        duplicate_claim.clone(),
        &target,
        &limits,
    )
    .unwrap_err();
    assert!(format!("{error:?}").contains("transferred"), "{error:?}");
    let wire = zil_ir::wire::encode_module_candidate(&forged_duplicate, &duplicate_claim, &limits)
        .unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
}

#[test]
#[cfg(target_os = "linux")]
fn exclusive_record_transfer_native_matches_moved_target() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/resource-packages/exclusive-record-transfer.zil");
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let output = std::env::temp_dir().join(format!(
                "zil-exclusive-record-transfer-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            let report = compile_path(
                &path,
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
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
            let result = std::process::Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(0), "{result:?}");
            let _ = std::fs::remove_file(output);
        }
    }
}

#[test]
#[cfg(target_os = "linux")]
fn affine_holder_control_flow_native_matches_targets() {
    for name in [
        "shared-holder-branch-refill.zil",
        "exclusive-holder-loop-transfer.zil",
        "exclusive-holder-loop-refill.zil",
        "exclusive-holder-loop-reconstruct.zil",
        "exclusive-address-sequential-reborrows.zil",
        "exclusive-nested-holder-transfer.zil",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/resource-packages")
            .join(name);
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let output = std::env::temp_dir().join(format!(
                    "zil-affine-{name}-{}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                let report = compile_path(
                    &path,
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        entry: Some("main".into()),
                        output: Some(output.clone()),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{name}: {}",
                    report.to_json()
                );
                let result = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(
                    result.status.code(),
                    Some(0),
                    "{name} {backend:?} {optimization:?}: {result:?}"
                );
                assert_eq!(result.stdout, b"0\n", "{name}: {result:?}");
                let _ = std::fs::remove_file(output);
            }
        }
    }
}

#[test]
fn exclusive_record_branch_join_requires_initialized_carrier() {
    let accepted = compile_source(
        &source(
            "exclusive-record-branch-refill.zil",
            EXCLUSIVE_RECORD_BRANCH_REFILL,
        ),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        accepted.status,
        ResultCategory::Success,
        "{}",
        accepted.to_json()
    );
    assert_eq!(accepted.verification_runs, 1);

    for (name, program) in [
        (
            "exclusive-record-branch-uninitialized.zil",
            EXCLUSIVE_RECORD_BRANCH_UNINITIALIZED,
        ),
        (
            "exclusive-record-after-move.zil",
            EXCLUSIVE_RECORD_AFTER_MOVE,
        ),
    ] {
        let rejected = compile_source(
            &source(name, program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            rejected.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            rejected.to_json()
        );
        assert_eq!(rejected.diagnostics[0].code, "E-UNINITIALIZED");
    }
}

#[test]
fn exclusive_pair_raw_constructor_cannot_repeat_one_address() {
    let input = source("exclusive-pair.zil", EXCLUSIVE_PAIR);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &input,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let mut raw = zil_types::check(&core, &target, &limits).unwrap();
    let mut candidate = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap();
    let function = raw
        .functions
        .iter_mut()
        .find(|function| function.name == "make")
        .unwrap();
    let fields = function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find_map(|instruction| {
            if let zil_ir::InstructionKind::MakeAggregate { fields } = &mut instruction.kind {
                Some(fields)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(fields.len(), 2);
    fields[1] = fields[0];
    candidate.module_fingerprint = raw.fingerprint();
    let analyze_error = zil_analyze::analyze(&raw, &limits).unwrap_err();
    let verify_error =
        zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap_err();
    assert!(
        format!("{analyze_error:?}").contains("transferred twice"),
        "{analyze_error:?}"
    );
    assert!(
        format!("{verify_error:?}").contains("transferred twice"),
        "{verify_error:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
}

#[test]
fn independent_exclusive_holder_constructors_reject_reused_address_value() {
    let input = source("two-exclusive-holders.zil", TWO_EXCLUSIVE_HOLDERS);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    let core = zil_expand::expand_with_map_limits_target_context(
        &input,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: input.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    let mut raw = zil_types::check(&core, &target, &limits).unwrap();
    let mut candidate = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap();
    let original_raw = raw.clone();
    let original_candidate = candidate.clone();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();

    // Repeated call reborrows of the same SSA address remain legal before
    // its single transfer into a holder, even in a function with stored moves.
    let mut reborrows = original_raw.clone();
    let peek = reborrows
        .functions
        .iter()
        .find(|f| f.name == "peek")
        .unwrap()
        .id;
    let function = reborrows
        .functions
        .iter_mut()
        .find(|f| f.name == "use_two")
        .unwrap();
    let parameter = function.parameters[0];
    let block = &mut function.blocks[0];
    let position = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, zil_ir::InstructionKind::Load(local) | zil_ir::InstructionKind::TakeLocal { local } if local == parameter))
        .unwrap();
    let address = zil_ir::ValueId(function.values.len() as u32);
    function
        .values
        .push(function.locals[parameter.0 as usize].ty.clone());
    let span = block.instructions[position].span;
    let calls: Vec<_> = (0..2)
        .map(|_| {
            let result = zil_ir::ValueId(function.values.len() as u32);
            function.values.push(Type::I32);
            zil_ir::Instruction {
                result: Some(result),
                kind: zil_ir::InstructionKind::Call {
                    function: peek,
                    arguments: vec![address],
                },
                span,
            }
        })
        .collect();
    block.instructions.splice(
        position..position,
        std::iter::once(zil_ir::Instruction {
            result: Some(address),
            kind: zil_ir::InstructionKind::Load(parameter),
            span,
        })
        .chain(calls),
    );
    let reborrow_claim = zil_analyze::analyze(&reborrows, &limits).unwrap();
    zil_verify::verify_module(reborrows.clone(), reborrow_claim.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&reborrows, &reborrow_claim, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();

    let function = raw
        .functions
        .iter_mut()
        .find(|function| function.name == "use_two")
        .unwrap();
    let addresses: Vec<_> = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| {
            if let zil_ir::InstructionKind::MakeAggregate { fields } = &instruction.kind {
                assert_eq!(fields.len(), 1);
                Some(fields[0])
            } else {
                None
            }
        })
        .collect();
    assert_eq!(addresses.len(), 2);
    assert_ne!(addresses[0], addresses[1]);
    let second = function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .filter_map(|instruction| {
            if let zil_ir::InstructionKind::MakeAggregate { fields } = &mut instruction.kind {
                Some(fields)
            } else {
                None
            }
        })
        .nth(1)
        .unwrap();
    second[0] = addresses[0];
    candidate.module_fingerprint = raw.fingerprint();
    let analyze = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        format!("{analyze:?}").contains("transferred twice"),
        "{analyze:?}"
    );
    let verify =
        zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap_err();
    assert!(
        format!("{verify:?}").contains("transferred twice"),
        "{verify:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    let wire_verify = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        format!("{wire_verify:?}").contains("transferred twice"),
        "{wire_verify:?}"
    );

    let mut two_loads = original_raw;
    let function = two_loads
        .functions
        .iter_mut()
        .find(|function| function.name == "use_two")
        .unwrap();
    // Keep distinct SSA definitions and the ordinary constructor temporaries,
    // but load the same exclusive parameter into both holders.
    let [first, second] = function.parameters.as_slice() else {
        panic!("two parameters expected")
    };
    let (first, second) = (*first, *second);
    let mut input_loads = Vec::new();
    for instruction in function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
    {
        if let zil_ir::InstructionKind::Load(local) | zil_ir::InstructionKind::TakeLocal { local } =
            instruction.kind
            && (local == first || local == second)
        {
            input_loads.push(instruction.result.unwrap());
            instruction.kind = zil_ir::InstructionKind::Load(first);
        }
    }
    assert_eq!(input_loads.len(), 2);
    assert_ne!(input_loads[0], input_loads[1]);
    let mut claim = original_candidate;
    claim.module_fingerprint = two_loads.fingerprint();
    let analyze = zil_analyze::analyze(&two_loads, &limits).unwrap_err();
    assert!(
        format!("{analyze:?}").contains("simultaneously live access overlaps"),
        "{analyze:?}"
    );
    let verify =
        zil_verify::verify_module(two_loads.clone(), claim.clone(), &target, &limits).unwrap_err();
    assert!(
        format!("{verify:?}").contains("actual target intersects"),
        "{verify:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&two_loads, &claim, &limits).unwrap();
    let wire_verify = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        format!("{wire_verify:?}").contains("actual target intersects"),
        "{wire_verify:?}"
    );
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
        (
            "nested-qualified-child-move.zil",
            NESTED_QUALIFIED_CHILD_MOVE,
        ),
        (
            "nested-qualified-child-refill.zil",
            NESTED_QUALIFIED_CHILD_REFILL,
        ),
        (
            "nested-qualified-child-borrow.zil",
            NESTED_QUALIFIED_CHILD_BORROW,
        ),
        (
            "nested-qualified-child-direct-local.zil",
            NESTED_QUALIFIED_CHILD_DIRECT_LOCAL,
        ),
        (
            "nested-qualified-slot-extraction.zil",
            NESTED_QUALIFIED_SLOT_EXTRACTION,
        ),
        (
            "nested-qualified-slot-helper-exchange.zil",
            NESTED_QUALIFIED_SLOT_HELPER,
        ),
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
fn nested_child_transfer_rejects_reusing_removed_field_or_parent() {
    let removed = "var child=give outer.child\n        let pointer=child.pointer";
    let cases = [
        (
            "duplicate child transfer",
            "var child=give outer.child\n        let duplicate=give outer.child\n        let pointer=child.pointer",
        ),
        (
            "parent move with removed child",
            "var child=give outer.child\n        let moved=outer\n        let pointer=child.pointer",
        ),
    ];
    for (name, replacement) in cases {
        let program = NESTED_QUALIFIED_CHILD_MOVE.replacen(removed, replacement, 1);
        assert_ne!(program, NESTED_QUALIFIED_CHILD_MOVE, "{name}");
        let report = compile_source(
            &source(name, &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
    }
    let borrowed = "let pointer=child_ref.pointer";
    let program = NESTED_QUALIFIED_CHILD_BORROW.replacen(
        borrowed,
        "let pointer=child_ref.pointer\n        var child=give outer.child",
        1,
    );
    assert_ne!(program, NESTED_QUALIFIED_CHILD_BORROW);
    let report = compile_source(
        &source("live child borrow blocks transfer", &program),
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
    let direct = "let pointer=outer.child.pointer";
    let program = NESTED_QUALIFIED_CHILD_DIRECT_LOCAL.replacen(
        direct,
        "let pointer=outer.child.pointer\n        var child=give outer.child",
        1,
    );
    assert_ne!(program, NESTED_QUALIFIED_CHILD_DIRECT_LOCAL);
    let report = compile_source(
        &source("direct child slot borrow blocks transfer", &program),
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
fn nested_child_transfer_raw_and_wire_reject_duplicate_consumption() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = source(
        "nested-qualified-child-move.zil",
        NESTED_QUALIFIED_CHILD_MOVE,
    );
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
    let mut candidate = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap();
    let main = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let (block, at) = main
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, zil_ir::InstructionKind::TakePackageField { .. }))
                .map(|at| (b, at))
        })
        .unwrap();
    let mut duplicate = main.blocks[block].instructions[at].clone();
    let result = duplicate.result.unwrap();
    let new_result = zil_ir::ValueId(main.values.len() as u32);
    main.values.push(main.values[result.0 as usize].clone());
    duplicate.result = Some(new_result);
    main.blocks[block].instructions.insert(at + 1, duplicate);
    candidate.module_fingerprint = raw.fingerprint();
    let analyzed = zil_analyze::analyze(&raw, &limits).unwrap_err();
    assert!(
        analyzed
            .iter()
            .any(|d| d.message.contains("field") || d.message.contains("stored")),
        "{analyzed:?}"
    );
    let verified =
        zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap_err();
    assert!(
        verified
            .iter()
            .any(|d| d.message.contains("field") || d.message.contains("stored")),
        "{verified:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    let wire_error = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        wire_error
            .iter()
            .any(|d| d.message.contains("field") || d.message.contains("stored")),
        "{wire_error:?}"
    );
}

#[test]
fn raw_take_forgery_requires_refill_before_helper_exit() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let source = source(
        "replace-and-return-stored-child.zil",
        REPLACE_AND_RETURN_CHILD,
    );
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
    let mut candidate = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap();
    let mut invalid = raw;
    let helper = invalid
        .functions
        .iter_mut()
        .find(|f| f.name == "exchange")
        .unwrap();
    let load = helper
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| {
            matches!(
                i.kind,
                zil_ir::InstructionKind::LoadStoredBorrow {
                    mode: zil_ir::StoredBorrowLoad::Reborrow { mutable: true },
                    ..
                }
            )
        })
        .unwrap();
    let zil_ir::InstructionKind::LoadStoredBorrow { mode, .. } = &mut load.kind else {
        unreachable!()
    };
    *mode = zil_ir::StoredBorrowLoad::Take;
    candidate.module_fingerprint = invalid.fingerprint();
    let analyzed = zil_analyze::analyze(&invalid, &limits).unwrap_err();
    assert!(
        analyzed
            .iter()
            .any(|d| d.message.contains("incomplete actual target facts")),
        "{analyzed:?}"
    );
    let verified = zil_verify::verify_module(invalid.clone(), candidate.clone(), &target, &limits)
        .unwrap_err();
    assert!(
        verified.iter().any(|d| d.message.contains("stored")),
        "{verified:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&invalid, &candidate, &limits).unwrap();
    let wire_error = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        wire_error.iter().any(|d| d.message.contains("stored")),
        "{wire_error:?}"
    );
}

#[test]
fn move_out_refill_source_rejects_holes_and_partial_exits() {
    let positive = compile_path(
        &move_out_path(),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        positive.status,
        ResultCategory::Success,
        "{}",
        positive.to_json()
    );
    let replaced = "let previous=move_out(session.pointer)\n session.pointer=next";
    let cases = [
        ("second extraction", MOVE_OUT.replacen(replaced,
            "let previous=move_out(session.pointer)\n let again=move_out(session.pointer)\n session.pointer=next", 1)),
        ("read vacant slot", MOVE_OUT.replacen(replaced,
            "let previous=move_out(session.pointer)\n let empty=session.pointer\n session.pointer=next\n *empty=*empty+1", 1)),
        ("error exit without refill", MOVE_OUT.replacen("fn main()->i32{",
            "enum Fault{Stopped}\nfn lose<life a>(session:&mut Session<'a>)->Fault!i32{\n let previous=move_out(session.pointer)\n return Fault.Stopped\n}\nfn main()->i32{", 1)),
        ("partial shell return", MOVE_OUT.replacen("fn main()->i32{",
            "fn partial<life a>(pointer:&'a mut i32)->Session<'a>{\n var session=make(pointer)\n let previous=move_out(session.pointer)\n return session\n}\nfn main()->i32{", 1)),
        ("branch with vacant incoming slot", MOVE_OUT.replacen("fn main()->i32{",
            "fn maybe<life a>(session:&mut Session<'a>,next:&'a mut i32,flag:bool){\n if flag { let previous=move_out(session.pointer) }\n let current=session.pointer\n session.pointer=next\n}\nfn main()->i32{", 1)),
    ];
    for (index, (name, program)) in cases.into_iter().enumerate() {
        assert_ne!(program, MOVE_OUT, "{name}");
        let path = std::env::temp_dir().join(format!(
            "zil-move-out-neighbor-{}-{index}.zil",
            std::process::id()
        ));
        std::fs::write(&path, &program).unwrap();
        let report = compile_path(
            &path,
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        std::fs::remove_file(path).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
    }
}

#[test]
#[cfg(target_os = "linux")]
fn move_out_native_preserves_exclusive_and_shared_targets() {
    for (source, name) in [
        (move_out_path(), "exclusive"),
        (shared_move_out_path(), "shared"),
        (local_move_out_path(), "local-drop"),
    ] {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let output = std::env::temp_dir().join(format!(
                    "zil-move-out-{}-{name}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                let report = compile_path(
                    &source,
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
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
                let run = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert_eq!(run.status.code(), Some(0), "{run:?}");
                assert_eq!(run.stdout, b"0\n", "{run:?}");
                std::fs::remove_file(output).unwrap();
            }
        }
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
            .any(|d| d.message.contains("used after transfer")),
        "{analyze_error:?}"
    );
    let verify_error =
        zil_verify::verify_module(raw.clone(), candidate.clone(), &target, &limits).unwrap_err();
    assert!(
        verify_error
            .iter()
            .any(|d| d.message.contains("used after transfer")),
        "{verify_error:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&raw, &candidate, &limits).unwrap();
    let wire_error = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        wire_error
            .iter()
            .any(|d| d.message.contains("used after transfer")),
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
        (
            "nested-qualified-child-move.zil",
            NESTED_QUALIFIED_CHILD_MOVE,
            b"M0\n".as_slice(),
        ),
        (
            "nested-qualified-child-refill.zil",
            NESTED_QUALIFIED_CHILD_REFILL,
            b"M0\n".as_slice(),
        ),
        (
            "nested-qualified-child-borrow.zil",
            NESTED_QUALIFIED_CHILD_BORROW,
            b"M0\n".as_slice(),
        ),
        (
            "nested-qualified-child-direct-local.zil",
            NESTED_QUALIFIED_CHILD_DIRECT_LOCAL,
            b"M0\n".as_slice(),
        ),
        (
            "nested-qualified-slot-extraction.zil",
            NESTED_QUALIFIED_SLOT_EXTRACTION,
            b"M0\n".as_slice(),
        ),
        (
            "nested-qualified-slot-helper-exchange.zil",
            NESTED_QUALIFIED_SLOT_HELPER,
            b"M0\n".as_slice(),
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

#[path = "borrowed_record.rs"]
mod borrowed_record;
