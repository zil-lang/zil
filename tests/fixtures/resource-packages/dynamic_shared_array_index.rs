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
