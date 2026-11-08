//! Dynamic selection must preserve all real leaves, including initialization.
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
    let definitions = PARTIAL_SNAPSHOT.split("fn main").next().unwrap();
    let program = format!(
        "{definitions}fn read<life a>(first:&'a i32,second:&'a i32,flag:bool)->i32{{var left=make(first,first);var right=make(second,second);let initial=&left;let unused=initial.items[0];let selected=if flag{{&left}}else{{&right}};let snapshot=selected.items;return *snapshot[1]}}fn main()->i32{{let first=7;let second=9;return read(&first,&second,true)-7}}"
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
    let analyzed = zil_analyze::analyze(&bad, &limits);
    match &analyzed {
        Ok(fresh) => {
            println!("local roots: one hole plus complete other parent Analyze Success");
            let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../out/runs/dynamic-shared-array/old-initialization-reproducer");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("bad-raw.txt"), format!("{bad:#?}")).unwrap();
            std::fs::write(dir.join("fresh-candidate.txt"), format!("{fresh:#?}")).unwrap();
            std::fs::write(
                dir.join("bad-module-candidate.wire"),
                zil_ir::wire::encode_module_candidate(&bad, fresh, &limits).unwrap(),
            )
            .unwrap();
            println!(
                "saved original initialization reproducer artifacts at {}",
                dir.display()
            );
            println!(
                "fresh candidate Verify {:?}",
                zil_verify::verify_module(bad.clone(), fresh.clone(), &target, &limits)
                    .as_ref()
                    .map(|_| "Success")
            );
            let wire = zil_ir::wire::encode_module_candidate(&bad, fresh, &limits).unwrap();
            println!(
                "fresh candidate Wire {:?}",
                zil_verify::verify_module_bytes(&wire, &target, &limits)
                    .as_ref()
                    .map(|_| "Success")
            );
        }
        Err(errors) => println!("local roots bad snapshot Analyze {errors:?}"),
    }
    assert!(
        analyzed.is_err(),
        "initialized alternative root must not hide an actually vacant source slot"
    );
}
