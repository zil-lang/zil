//! Whole shared-array snapshots require every actual root to be initialized.
use super::*;

const PARTIAL_SNAPSHOT: &str = "package Job<life a>{manages bytes:ByteBuffer;items:[&'a i32;2]}fn make<life a>(first:&'a i32,second:&'a i32)->Job<'a>{var bytes=bytes_new();return Job::<'a>{bytes:give bytes,items:[first,second]}}fn main()->i32{var first=7;var second=9;var job=make(&first,&second);let window=&job;let unused=window.items[0];let snapshot=window.items;return *snapshot[1]-9}";

fn taken_snapshot(refill_before: bool) -> zil_ir::TypedModule {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let mut raw = typed_source(
        "array-snapshot-slot-state",
        PARTIAL_SNAPSHOT,
        &target,
        &limits,
    );
    let main = raw.functions.iter_mut().find(|f| f.name == "main").unwrap();
    let (block_index, take_index) = main
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, zil_ir::InstructionKind::LoadStoredBorrow { .. }))
                .map(|i| (b, i))
        })
        .unwrap();
    // Authenticate the existing raw mutable-carrier protocol without opening
    // the source-only mutable Array-field projection boundary.
    let package_pointer = |ty: &Type| {
        matches!(ty, Type::Address { inner, .. }
        if matches!(inner.as_ref(), Type::BorrowedStorage { storage, .. }
            if matches!(storage.as_ref(), Type::Package { .. })))
    };
    for local in &mut main.locals {
        if package_pointer(&local.ty)
            && let Type::Address { mutable, .. } = &mut local.ty
        {
            *mutable = true;
        }
    }
    for ty in &mut main.values {
        if package_pointer(ty)
            && let Type::Address { mutable, .. } = ty
        {
            *mutable = true;
        }
    }
    for instruction in main.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if instruction
            .result
            .is_some_and(|id| package_pointer(&main.values[id.0 as usize]))
        {
            match &mut instruction.kind {
                zil_ir::InstructionKind::BorrowLocal { local } => {
                    instruction.kind = zil_ir::InstructionKind::BorrowLocalMut { local: *local }
                }
                zil_ir::InstructionKind::ReborrowAddress { mutable, .. } => *mutable = true,
                _ => {}
            }
        }
        if let zil_ir::InstructionKind::AddressField { address, .. } = &instruction.kind
            && package_pointer(&main.values[address.0 as usize])
            && let Type::Address { mutable, .. } =
                &mut main.values[instruction.result.unwrap().0 as usize]
        {
            *mutable = true;
        }
    }
    let block = &mut main.blocks[block_index];
    let take = block.instructions[take_index].clone();
    let zil_ir::InstructionKind::LoadStoredBorrow { storage, .. } = take.kind else {
        unreachable!()
    };
    let field_index = block
        .instructions
        .iter()
        .position(|i| i.result == Some(storage))
        .unwrap();
    let zil_ir::InstructionKind::AddressField {
        address: parent,
        index: 1,
    } = block.instructions[field_index].kind
    else {
        panic!("actual Array field")
    };
    main.values[storage.0 as usize] = main.values[parent.0 as usize].clone();
    block.instructions[field_index].kind = zil_ir::InstructionKind::ReborrowAddress {
        address: parent,
        mutable: true,
    };
    let path = vec![
        zil_ir::PlaceProjection::Field(1),
        zil_ir::PlaceProjection::Field(0),
    ];
    block.instructions[take_index].kind = zil_ir::InstructionKind::LoadStoredBorrow {
        storage,
        path: path.clone(),
        mode: zil_ir::StoredBorrowLoad::Take,
    };
    let read_index = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, zil_ir::InstructionKind::ReadAddress { .. }))
        .unwrap();
    assert!(read_index > take_index);
    let refill = zil_ir::Instruction {
        result: None,
        kind: zil_ir::InstructionKind::StoreStoredBorrow {
            storage,
            path,
            value: take.result.unwrap(),
        },
        span: take.span,
    };
    block.instructions.insert(
        if refill_before {
            take_index + 1
        } else {
            read_index + 1
        },
        refill,
    );
    raw
}

#[test]
fn whole_array_snapshot_requires_every_taken_slot_to_be_refilled() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let good = taken_snapshot(true);
    let plan = zil_analyze::analyze(&good, &limits)
        .expect("a legal take/refill is a valid positive control");
    zil_verify::verify_module(good.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&good, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    println!("take/refill before whole Array snapshot: Analyze/Verify/Wire Success");
    let bad = taken_snapshot(false);
    let analyzed = zil_analyze::analyze(&bad, &limits);
    match &analyzed {
        Ok(fresh) => {
            println!("take then whole Array read then refill: Analyze Success");
            let verified = zil_verify::verify_module(bad.clone(), fresh.clone(), &target, &limits);
            println!(
                "same raw with fresh candidate Verify: {:?}",
                verified.as_ref().map(|_| "Success")
            );
            let wire = zil_ir::wire::encode_module_candidate(&bad, fresh, &limits).unwrap();
            println!(
                "same Wire Verify: {:?}",
                zil_verify::verify_module_bytes(&wire, &target, &limits)
                    .as_ref()
                    .map(|_| "Success")
            );
        }
        Err(errors) => println!("take then whole Array read then refill: Analyze {errors:?}"),
    }
    let mut prior = plan;
    prior.module_fingerprint = bad.fingerprint();
    let verified =
        zil_verify::verify_module(bad.clone(), prior.clone(), &target, &limits).unwrap_err();
    println!("bad snapshot with original candidate Verify: {verified:?}");
    let wire = zil_ir::wire::encode_module_candidate(&bad, &prior, &limits).unwrap();
    let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    println!("bad snapshot with original candidate Wire: {decoded:?}");
    assert!(
        analyzed.is_err(),
        "a complete other leaf cannot hide the taken physical Array slot"
    );
}

fn multi_root_snapshot(refill_before: bool) -> zil_ir::TypedModule {
    let definitions = PARTIAL_SNAPSHOT.split("fn main").next().unwrap();
    let program = format!(
        "{definitions}fn read<life w,life a>(left:&'w Job<'a>,right:&'w Job<'a>,flag:bool)->i32{{let unused=left.items[0];let selected=if flag{{left}}else{{right}};let snapshot=selected.items;return *snapshot[1]}}fn main()->i32{{return 0}}"
    );
    let mut raw = typed_source(
        "array-snapshot-two-roots",
        &program,
        &TargetProfile::host_llvm(),
        &Limits::default(),
    );
    let f = raw.functions.iter_mut().find(|f| f.name == "read").unwrap();
    let left = f.parameters[0];
    let Type::Address { mutable, .. } = &mut f.locals[left.0 as usize].ty else {
        unreachable!()
    };
    *mutable = true;
    let (take_block, take_index) = f
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, zil_ir::InstructionKind::LoadStoredBorrow { .. }))
                .map(|i| (b, i))
        })
        .unwrap();
    let take = f.blocks[take_block].instructions[take_index].clone();
    let zil_ir::InstructionKind::LoadStoredBorrow { storage, .. } = take.kind else {
        unreachable!()
    };
    let field_index = f.blocks[take_block]
        .instructions
        .iter()
        .position(|i| i.result == Some(storage))
        .unwrap();
    let zil_ir::InstructionKind::AddressField {
        address: parent,
        index: 1,
    } = f.blocks[take_block].instructions[field_index].kind
    else {
        panic!("actual Array field")
    };
    let Type::Address { mutable, .. } = &mut f.values[parent.0 as usize] else {
        unreachable!()
    };
    *mutable = true;
    let parent_type = f.values[parent.0 as usize].clone();
    f.values[storage.0 as usize] = parent_type.clone();
    f.blocks[take_block].instructions[field_index].kind =
        zil_ir::InstructionKind::ReborrowAddress {
            address: parent,
            mutable: true,
        };
    let path = vec![
        zil_ir::PlaceProjection::Field(1),
        zil_ir::PlaceProjection::Field(0),
    ];
    f.blocks[take_block].instructions[take_index].kind =
        zil_ir::InstructionKind::LoadStoredBorrow {
            storage,
            path: path.clone(),
            mode: zil_ir::StoredBorrowLoad::Take,
        };
    for block in &mut f.blocks {
        let mut index = 0;
        while index < block.instructions.len() {
            let instruction = &block.instructions[index];
            if matches!(instruction.kind,zil_ir::InstructionKind::Load(local) if local==left)
                && instruction.result != Some(parent)
            {
                let original = instruction.clone();
                let id = zil_ir::ValueId(f.values.len() as u32);
                f.values.push(parent_type.clone());
                block.instructions[index].kind = zil_ir::InstructionKind::ReborrowAddress {
                    address: id,
                    mutable: false,
                };
                block.instructions.insert(
                    index,
                    zil_ir::Instruction {
                        result: Some(id),
                        kind: zil_ir::InstructionKind::Load(left),
                        span: original.span,
                    },
                );
                index += 1;
            }
            index += 1;
        }
    }
    let reload = zil_ir::ValueId(f.values.len() as u32);
    f.values.push(parent_type);
    let load = zil_ir::Instruction {
        result: Some(reload),
        kind: zil_ir::InstructionKind::Load(left),
        span: take.span,
    };
    let refill = zil_ir::Instruction {
        result: None,
        kind: zil_ir::InstructionKind::StoreStoredBorrow {
            storage: reload,
            path,
            value: take.result.unwrap(),
        },
        span: take.span,
    };
    let (block, index) = if refill_before {
        (take_block, take_index + 1)
    } else {
        f.blocks
            .iter()
            .enumerate()
            .find_map(|(b, block)| {
                block
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, zil_ir::InstructionKind::ReadAddress { .. }))
                    .map(|i| (b, i + 1))
            })
            .unwrap()
    };
    f.blocks[block]
        .instructions
        .splice(index..index, [load, refill]);
    raw
}

#[test]
fn complete_other_root_cannot_mask_a_taken_array_leaf() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let good = multi_root_snapshot(true);
    let prior = zil_analyze::analyze(&good, &limits).expect("multi-root positive take/refill");
    zil_verify::verify_module(good.clone(), prior.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&good, &prior, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    println!("multi-root valid take/refill before Read: A/V/W Success");
    let bad = multi_root_snapshot(false);
    let analyzed = zil_analyze::analyze(&bad, &limits);
    match &analyzed {
        Ok(plan) => {
            println!("multi-root taken leaf with complete other parent: Analyze Success");
            println!(
                "fresh candidate Verify: {:?}",
                zil_verify::verify_module(bad.clone(), plan.clone(), &target, &limits)
                    .as_ref()
                    .map(|_| "Success")
            );
            let wire = zil_ir::wire::encode_module_candidate(&bad, plan, &limits).unwrap();
            println!(
                "fresh Wire: {:?}",
                zil_verify::verify_module_bytes(&wire, &target, &limits)
                    .as_ref()
                    .map(|_| "Success")
            );
        }
        Err(errors) => println!("multi-root taken leaf: Analyze {errors:?}"),
    }
    assert!(
        analyzed.is_err(),
        "each actual parent must have every initialized slot"
    );
    let mut candidate = prior;
    candidate.module_fingerprint = bad.fingerprint();
    println!(
        "prior candidate Verify: {:?}",
        zil_verify::verify_module(bad.clone(), candidate.clone(), &target, &limits)
            .as_ref()
            .map(|_| "Success")
    );
    let wire = zil_ir::wire::encode_module_candidate(&bad, &candidate, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
}

fn local_roots_snapshot(refill_before: bool) -> zil_ir::TypedModule {
    local_snapshot_body(
        "let initial=&left;let unused=initial.items[0];let selected=if flag{&left}else{&right};let snapshot=selected.items;return *snapshot[1]",
        refill_before,
    )
}

fn local_snapshot_body(body: &str, refill_before: bool) -> zil_ir::TypedModule {
    let definitions = PARTIAL_SNAPSHOT.split("fn main").next().unwrap();
    let program = format!(
        "{definitions}fn read<life a>(first:&'a i32,second:&'a i32,flag:bool)->i32{{var left=make(first,first);var right=make(second,second);{body}}}fn main()->i32{{let first=7;let second=9;return read(&first,&second,true)-7}}"
    );
    let mut raw = typed_source(
        "array-snapshot-two-local-roots",
        &program,
        &TargetProfile::host_llvm(),
        &Limits::default(),
    );
    let f = raw.functions.iter_mut().find(|f| f.name == "read").unwrap();
    let (block_index, take_index) = f
        .blocks
        .iter()
        .enumerate()
        .find_map(|(b, block)| {
            block
                .instructions
                .iter()
                .position(|i| matches!(i.kind, zil_ir::InstructionKind::LoadStoredBorrow { .. }))
                .map(|i| (b, i))
        })
        .unwrap();
    let block = &mut f.blocks[block_index];
    let take = block.instructions[take_index].clone();
    let zil_ir::InstructionKind::LoadStoredBorrow { storage, .. } = take.kind else {
        unreachable!()
    };
    let field_index = block
        .instructions
        .iter()
        .position(|i| i.result == Some(storage))
        .unwrap();
    let zil_ir::InstructionKind::AddressField {
        address: parent,
        index: 1,
    } = block.instructions[field_index].kind
    else {
        panic!("actual items field")
    };
    let initial = f.locals.iter().find(|l| l.name == "initial").unwrap().id;
    let Type::Address { mutable, .. } = &mut f.locals[initial.0 as usize].ty else {
        unreachable!()
    };
    *mutable = true;
    let Type::Address { mutable, .. } = &mut f.values[parent.0 as usize] else {
        unreachable!()
    };
    *mutable = true;
    let stored = block
        .instructions
        .iter()
        .find_map(|i| {
            if let zil_ir::InstructionKind::Store { local, value } = i.kind {
                (local == initial).then_some(value)
            } else {
                None
            }
        })
        .unwrap();
    let mut current = stored;
    loop {
        let Type::Address { mutable, .. } = &mut f.values[current.0 as usize] else {
            unreachable!()
        };
        *mutable = true;
        let definition = block
            .instructions
            .iter_mut()
            .find(|i| i.result == Some(current))
            .unwrap();
        match &mut definition.kind {
            zil_ir::InstructionKind::ReborrowAddress { address, mutable } => {
                *mutable = true;
                current = *address;
            }
            zil_ir::InstructionKind::BorrowLocal { local } => {
                let local = *local;
                assert!(f.locals[local.0 as usize].mutable);
                definition.kind = zil_ir::InstructionKind::BorrowLocalMut { local };
                break;
            }
            _ => panic!("actual initial borrow chain"),
        }
    }
    f.values[storage.0 as usize] = f.values[parent.0 as usize].clone();
    block.instructions[field_index].kind = zil_ir::InstructionKind::ReborrowAddress {
        address: parent,
        mutable: true,
    };
    let path = vec![
        zil_ir::PlaceProjection::Field(1),
        zil_ir::PlaceProjection::Field(0),
    ];
    block.instructions[take_index].kind = zil_ir::InstructionKind::LoadStoredBorrow {
        storage,
        path: path.clone(),
        mode: zil_ir::StoredBorrowLoad::Take,
    };
    if refill_before {
        block.instructions.insert(
            take_index + 1,
            zil_ir::Instruction {
                result: None,
                kind: zil_ir::InstructionKind::StoreStoredBorrow {
                    storage,
                    path,
                    value: take.result.unwrap(),
                },
                span: take.span,
            },
        );
    }
    raw
}

#[test]
fn complete_local_parent_cannot_mask_a_vacant_snapshot_candidate() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let good = local_roots_snapshot(true);
    let good_plan = zil_analyze::analyze(&good, &limits).expect("local roots legal take/refill");
    zil_verify::verify_module(good.clone(), good_plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&good, &good_plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    println!("local roots: take/refill before snapshot A/V/W Success");
    let bad = local_roots_snapshot(false);
    let analyzed = zil_analyze::analyze(&bad, &limits).unwrap_err();
    assert!(
        analyzed
            .iter()
            .any(|d| d.message.contains("Array snapshot reads a vacant slot")),
        "{analyzed:?}"
    );
    println!("local roots bad snapshot Analyze {analyzed:?}");
    let mut prior = good_plan;
    prior.module_fingerprint = bad.fingerprint();
    let verified =
        zil_verify::verify_module(bad.clone(), prior.clone(), &target, &limits).unwrap_err();
    assert!(
        verified
            .iter()
            .any(|d| d.message.contains("Array snapshot reads a vacant slot")),
        "{verified:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&bad, &prior, &limits).unwrap();
    let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        decoded
            .iter()
            .any(|d| d.message.contains("Array snapshot reads a vacant slot")),
        "{decoded:?}"
    );
    let saved=std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../out/runs/dynamic-shared-array/old-initialization-reproducer/bad-module-candidate.wire");
    if saved.exists() {
        let original = std::fs::read(saved).unwrap();
        let errors = zil_verify::verify_module_bytes(&original, &target, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("Array snapshot reads a vacant slot")),
            "{errors:?}"
        );
        println!(
            "previously accepted original Wire/fresh candidate now rejected by actual vacancy"
        );
    }
}

fn accept_snapshot_protocol(label: &str, raw: zil_ir::TypedModule) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let plan = zil_analyze::analyze(&raw, &limits).unwrap_or_else(|e| panic!("{label}: {e:?}"));
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits)
        .unwrap_or_else(|e| panic!("{label}: {e:?}"));
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits)
        .unwrap_or_else(|e| panic!("{label}: {e:?}"));
    println!("{label}: Analyze/Verify/Wire Success");
}

#[test]
fn a_hole_does_not_block_other_roots_or_direct_initialized_leaves() {
    for (label, body) in [
        (
            "read complete right while left has hole",
            "let initial=&left;let unused=initial.items[0];let selected=&right;let snapshot=selected.items;return *snapshot[1]",
        ),
        (
            "copy initialized left leaf without whole snapshot",
            "let initial=&left;let unused=initial.items[0];let selected=&left;return *selected.items[1]",
        ),
        (
            "take after complete snapshot",
            "let selected=&left;let snapshot=selected.items;let initial=&left;let unused=initial.items[0];return *snapshot[1]",
        ),
    ] {
        accept_snapshot_protocol(label, local_snapshot_body(body, false));
    }
}

#[test]
fn one_cfg_incoming_hole_cannot_be_hidden_by_the_same_complete_root() {
    let body = "if flag{let initial=&left;let unused=initial.items[0]};let selected=&left;let snapshot=selected.items;return *snapshot[1]";
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let good = local_snapshot_body(body, true);
    let plan = zil_analyze::analyze(&good, &limits).expect("conditional Take/Refill legal control");
    accept_snapshot_protocol("same root conditional Take/Refill", good);
    let bad = local_snapshot_body(body, false);
    let errors = zil_analyze::analyze(&bad, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("Array snapshot reads a vacant slot")),
        "{errors:?}"
    );
    let mut prior = plan;
    prior.module_fingerprint = bad.fingerprint();
    let errors =
        zil_verify::verify_module(bad.clone(), prior.clone(), &target, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("Array snapshot reads a vacant slot")),
        "{errors:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&bad, &prior, &limits).unwrap();
    let errors = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("Array snapshot reads a vacant slot")),
        "{errors:?}"
    );
    println!("same root one CFG incoming hole: Analyze/Verify/Wire actual vacancy rejection");
}

#[test]
fn whole_snapshot_initialization_costs_grow_with_actual_roots_and_slots() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut boundaries = Vec::new();
    for (roots, length) in [(1, 2), (1, 8), (2, 2), (2, 8)] {
        let items = vec!["first"; length].join(",");
        let selected = if roots == 1 {
            "&left"
        } else {
            "if flag{&left}else{&right}"
        };
        let program = format!(
            "package Job<life a>{{manages bytes:ByteBuffer;items:[&'a i32;{length}]}}fn make<life a>(first:&'a i32)->Job<'a>{{var bytes=bytes_new();return Job::<'a>{{bytes:give bytes,items:[{items}]}}}}fn read<life a>(first:&'a i32,second:&'a i32,flag:bool)->i32{{var left=make(first);var right=make(second);let selected={selected};let snapshot=selected.items;return *snapshot[0]}}fn main()->i32{{let first=7;let second=9;return read(&first,&second,true)-7}}"
        );
        let raw = typed_source("snapshot-initialization-budget", &program, &target, &limits);
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        let at = |n| Limits {
            max_analysis_steps: n,
            ..limits.clone()
        };
        let label = format!("whole Array initialization roots={roots} slots={length}");
        let analyze = exact_array_budget(&format!("{label} Analyze"), |n| {
            array_budget_category(zil_analyze::analyze(&raw, &at(n)))
        });
        let verify = exact_array_budget(&format!("{label} Verify"), |n| {
            array_budget_category(zil_verify::verify_module(
                raw.clone(),
                plan.clone(),
                &target,
                &at(n),
            ))
        });
        let decoded = exact_array_budget(&format!("{label} Wire"), |n| {
            array_budget_category(zil_verify::verify_module_bytes(&wire, &target, &at(n)))
        });
        assert_eq!(verify, decoded);
        boundaries.push([analyze, verify, decoded]);
    }
    assert!(boundaries[1][0] > boundaries[0][0]);
    assert!(boundaries[1][1] > boundaries[0][1]);
    assert!(boundaries[2][0] > boundaries[0][0]);
    assert!(boundaries[2][1] > boundaries[0][1]);
    assert!(boundaries[3][0] > boundaries[2][0]);
    assert!(boundaries[3][1] > boundaries[2][1]);
    assert_eq!(
        boundaries,
        vec![
            [31697, 44433, 44433],
            [67795, 99702, 99702],
            [47566, 71365, 71365],
            [104426, 174010, 174010],
        ]
    );
    println!("whole Array initialization exact boundaries: {boundaries:?}");
}

#[test]
fn a_complete_array_snapshot_survives_its_original_package_cleanup() {
    let definitions = PARTIAL_SNAPSHOT.split("fn main").next().unwrap();
    let program = format!(
        "{definitions}fn snapshot<life a>(first:&'a i32,second:&'a i32)->[&'a i32;2]{{let job=make(first,second);let selected=&job;return selected.items}}fn main()->i32{{let first=7;let second=9;let items=snapshot(&first,&second);return *items[1]-9}}"
    );
    accept_snapshot_protocol(
        "snapshot survives its original manager",
        typed_source(
            "snapshot-owner-cleanup",
            &program,
            &TargetProfile::host_llvm(),
            &Limits::default(),
        ),
    );
    let mut runs = 0;
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = std::env::temp_dir().join(format!(
                "zil-initialized-array-snapshot-{}-{backend:?}-{optimization:?}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let executable = dir.join("program");
            let report = compile_source(
                &source("snapshot-owner-cleanup", &program),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(executable.clone()),
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
            let output = std::process::Command::new(&executable).output().unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "{output:?}"
            );
            std::fs::remove_dir_all(dir).unwrap();
            runs += 1;
        }
    }
    assert_eq!(runs, 2 + 2 * usize::from(cfg!(feature = "llvm-api")));
    println!("snapshot after original Package cleanup: {runs} native executions");
}
