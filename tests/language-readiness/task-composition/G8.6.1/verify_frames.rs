//! Independent kernel controls derived from real Source, without importing
//! the separately authored Analyze continuation qualification.
use super::*;
use zil_ir::{CallKind, InstructionKind as I, Type};
use zil_verify::{TaskOwnerAction as A, TaskOwnerSlot as S};

const RESOURCE: &str = "extern \"C\" fn putchar(code:i32)->i32;resource Token{code:i32}impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}fn consume(give token:Token){}";
const PAIR: &str = "enum Packet{Empty,Pair(manages Token,manages Token)}fn worker(give a:Token,give b:Token)->i32{let packet=Packet.Pair(give a,give b);task_yield();match packet{Packet.Empty()=>{return 0},Packet.Pair(left,right)=>{return left.code+right.code}}}";

const VIEW_SAME: &str = "enum Packet{Item(manages Token)}fn worker(give token:Token)->i32{let packet=Packet.Item(give token);var seen=0;match packet{Packet.Item(ref payload)=>{seen=payload.code}}task_yield();return seen}";
const VIEW_ACROSS: &str = "enum Packet{Item(manages Token)}fn worker(give token:Token)->i32{let packet=Packet.Item(give token);match packet{Packet.Item(ref payload)=>{task_yield();return payload.code}}}";
const VIEW_UNKNOWN: &str = "enum Packet{Left(manages Token),Right(manages Token)}fn worker(flag:bool,permit:bool,give token:Token)->i32{let packet=if flag{Packet.Left(give token)}else{Packet.Right(give token)};let spare=permit;match packet{Packet.Left(ref left)=>{task_yield();return left.code},Packet.Right(ref right)=>{task_yield();return right.code}}}";

fn candidate(text: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let text = format!("{RESOURCE}{text}fn main(){{}}");
    let typed = zil_types::check(&expand(&text, &limits).unwrap().core, &target, &limits).unwrap();
    // Preserve Yield's Unit SSA definition and initialization effect using a
    // Unit constant plus Direct header (and a direct child's ordinary Call)
    // obtains untrusted init/Hold rows from the existing ordinary Sum path.
    // Verify sees the original actual
    // Source Suspend worker and independently constructs its frame plan.
    let mut ordinary = typed.clone();
    let worker = ordinary
        .functions
        .iter_mut()
        .find(|f| f.name == "worker")
        .unwrap();
    assert_eq!(worker.call_kind, CallKind::Suspend);
    worker.call_kind = CallKind::Direct;
    for block in &mut worker.blocks {
        for instruction in &mut block.instructions {
            if matches!(instruction.kind, I::TaskYield) {
                assert!(instruction.result.is_some());
                instruction.kind = I::Constant(zil_ir::Constant::Unit);
            } else if let I::SuspendCall {
                function,
                arguments,
            } = &instruction.kind
            {
                instruction.kind = I::Call {
                    function: *function,
                    arguments: arguments.clone(),
                };
            }
        }
    }
    let mut plan = zil_analyze::analyze(&ordinary, &limits).unwrap();
    zil_verify::verify_module(ordinary, plan.clone(), &target, &limits).unwrap();
    plan.module_fingerprint = typed.fingerprint();
    (typed, plan)
}

#[test]
fn g8611_verify_whole_enum_local_and_unpack_frame_order() {
    for (name, text, boundaries) in [
        (
            "one",
            "enum Packet{Empty,Item(manages Token)}fn worker(give token:Token)->i32{let packet=Packet.Item(give token);task_yield();match packet{Packet.Empty()=>{return 0},Packet.Item(value)=>{return value.code}}}",
            1,
        ),
        (
            "unit",
            "enum Packet{Empty,Item(manages Token)}fn worker(give token:Token)->i32{consume(give token);let packet=Packet.Empty();task_yield();match packet{Packet.Empty()=>{return 0},Packet.Item(value)=>{return value.code}}}",
            1,
        ),
        ("pair", PAIR, 1),
        (
            "nested",
            "enum Inner{Empty,Item(manages Token)}enum Packet{Empty,Wrap(Inner)}fn worker(give token:Token)->i32{let packet=Packet.Wrap(Inner.Item(give token));task_yield();match packet{Packet.Empty()=>{return 0},Packet.Wrap(inner)=>{task_yield();match inner{Inner.Empty()=>{return 0},Inner.Item(value)=>{return value.code}}}}}",
            2,
        ),
    ] {
        let (typed, plan) = candidate(text);
        let target = typed.target.clone();
        let limits = Limits::default();
        let worker = typed.functions.iter().find(|f| f.name == "worker").unwrap();
        for wire in [false, true] {
            let verified = if wire {
                let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
                zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap()
            } else {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap()
            };
            let frame = verified.task_ownership(worker.id).unwrap();
            assert_eq!(frame.boundaries().len(), boundaries, "{name}");
            for boundary in frame.boundaries() {
                assert_eq!(boundary.may_live().len(), 1, "{name}: {frame:?}");
                let owner = &frame.owners()[boundary.may_live()[0]];
                assert!(matches!(owner.ty(), Type::Enum { .. }), "{name}: {owner:?}");
                assert!(matches!(owner.slot(), S::Local(_) | S::Value(_)));
            }
            for block in &worker.blocks {
                for (position, instruction) in block.instructions.iter().enumerate() {
                    let I::UnpackVariant { local, fields, .. } = &instruction.kind else {
                        continue;
                    };
                    let action = frame.actions_at(block.id, position).unwrap();
                    let root = frame
                        .owners()
                        .iter()
                        .position(|o| o.slot() == S::Local(*local))
                        .unwrap();
                    assert!(matches!(action.before(), [A::Unlink(i)] if *i == root));
                    let actual: Vec<_> = action
                        .after()
                        .iter()
                        .map(|a| match a {
                            A::Link(i) => *i,
                            other => panic!("noncanonical whole Enum destination: {other:?}"),
                        })
                        .collect();
                    let expected: Vec<_> = fields
                        .iter()
                        .rev()
                        .filter_map(|local| {
                            frame
                                .owners()
                                .iter()
                                .position(|o| o.slot() == S::Local(*local))
                        })
                        .collect();
                    assert_eq!(
                        actual, expected,
                        "{name}: actual reverse field initialization"
                    );
                }
            }
        }
    }
}

#[test]
fn g8611_verify_complete_enum_ssa_is_the_live_yield_owner() {
    let (mut typed, mut claim) = candidate(PAIR);
    let worker = typed
        .functions
        .iter_mut()
        .find(|f| f.name == "worker")
        .unwrap();
    let block = worker
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, I::TaskYield))
        })
        .unwrap();
    let at = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::TaskYield))
        .unwrap();
    let yield_instruction = block.instructions.remove(at);
    let make = block
        .instructions
        .iter()
        .position(|i| matches!(&i.kind, I::MakeVariant { fields, .. } if fields.len() == 2))
        .unwrap();
    let root = block.instructions[make].result.unwrap();
    // Move only the real Unit-producing Yield into the already prepared
    // whole SSA interval, before Store publishes the ordinary local.
    block.instructions.insert(make + 1, yield_instruction);
    let worker_id = worker.id;
    claim.module_fingerprint = typed.fingerprint();
    let limits = Limits::default();
    let target = typed.target.clone();
    for wire in [false, true] {
        let verified = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&typed, &claim, &limits).unwrap();
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap()
        } else {
            zil_verify::verify_module(typed.clone(), claim.clone(), &target, &limits).unwrap()
        };
        let frame = verified.task_ownership(worker_id).unwrap();
        assert_eq!(frame.boundaries().len(), 1);
        let live = frame.boundaries()[0].may_live();
        assert_eq!(live.len(), 1, "{frame:?}");
        assert_eq!(frame.owners()[live[0]].slot(), S::Value(root));
        assert_eq!(frame.owner_for_value(root), Some(live[0]));
        assert!(matches!(frame.owners()[live[0]].ty(), Type::Enum { .. }));
    }
}

#[test]
fn g8611_verify_enum_frame_preserves_input_result_and_nested_storage_fences() {
    for (name, text, category) in [
        (
            "owned input",
            "enum Packet{Item(manages Token)}fn worker(packet:Packet)->i32{task_yield();match packet{Packet.Item(value)=>{return value.code}}}",
            ResultCategory::SafeReject,
        ),
        (
            "borrowed input",
            "enum Packet{Item(manages Token)}fn worker(packet:&Packet)->i32{task_yield();return 0}",
            ResultCategory::SafeReject,
        ),
        (
            "borrowed local",
            "enum Packet{Item(manages Token)}fn worker(give token:Token)->i32{let packet=Packet.Item(give token);let view=&packet;task_yield();return 0}",
            ResultCategory::SafeReject,
        ),
        (
            "Product containing Sum",
            "enum Packet{Item(manages Token)}package Box{packet:Packet}fn worker(give token:Token)->i32{let box=Box{packet:Packet.Item(give token)};task_yield();return 0}",
            ResultCategory::SafeReject,
        ),
        (
            "new non-i32 result",
            "enum Packet{Item(manages Token)}fn worker(give token:Token)->bool{let packet=Packet.Item(give token);task_yield();return true}",
            ResultCategory::Unsupported,
        ),
        (
            "owned Enum result",
            "enum Packet{Item(manages Token)}fn worker(give token:Token)->Packet{task_yield();return Packet.Item(give token)}",
            ResultCategory::SafeReject,
        ),
        (
            "child owned Enum result",
            "enum Packet{Item(manages Token)}fn child(give token:Token)->Packet{return Packet.Item(give token)}fn worker(give token:Token)->i32{let packet=task_await(child,give token);task_yield();match packet{Packet.Item(value)=>{return value.code}}}",
            ResultCategory::SafeReject,
        ),
    ] {
        let (typed, plan) = candidate(text);
        let limits = Limits::default();
        let target = typed.target.clone();
        for wire in [false, true] {
            let errors = if wire {
                let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
                zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
            } else {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits)
                    .unwrap_err()
            };
            assert!(
                errors.iter().any(|d| d.category == category),
                "{name}: {errors:?}"
            );
            eprintln!("G8.6.1.1 closed {name} wire={wire} {}", errors[0].code);
        }
    }
}

#[test]
fn g8611_verify_enum_frame_raw_wire_rejects_repeated_and_missing_debt() {
    let (typed, plan) = candidate(PAIR);
    let limits = Limits::default();
    let target = typed.target.clone();
    for mode in 0..4 {
        let mut raw = typed.clone();
        let worker = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "worker")
            .unwrap();
        match mode {
            0 => {
                let make = worker
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(|i| matches!(&i.kind, I::MakeVariant { fields, .. } if fields.len() == 2))
                    .unwrap();
                let I::MakeVariant { fields, .. } = &mut make.kind else {
                    unreachable!()
                };
                fields[1] = fields[0];
            }
            1 => {
                let block = worker.blocks.iter_mut().find(|b| b.instructions.iter()
                    .any(|i| matches!(&i.kind, I::UnpackVariant { fields, .. } if fields.len() == 2))).unwrap();
                let at = block
                    .instructions
                    .iter()
                    .position(
                        |i| matches!(&i.kind, I::UnpackVariant { fields, .. } if fields.len() == 2),
                    )
                    .unwrap();
                block
                    .instructions
                    .insert(at + 1, block.instructions[at].clone());
            }
            2 => {
                let left = worker.locals.iter().find(|l| l.name == "left").unwrap().id;
                let mut removed = 0;
                for block in &mut worker.blocks {
                    block.instructions.retain(|i| {
                        let drop = matches!(i.kind, I::DropManaged { local } if local == left);
                        removed += usize::from(drop);
                        !drop
                    });
                }
                assert!(removed > 0);
            }
            _ => {
                let unpack = worker
                    .blocks
                    .iter_mut()
                    .flat_map(|b| &mut b.instructions)
                    .find(
                        |i| matches!(&i.kind, I::UnpackVariant { fields, .. } if fields.len() == 2),
                    )
                    .unwrap();
                let I::UnpackVariant { fields, .. } = &mut unpack.kind else {
                    unreachable!()
                };
                fields[1] = fields[0];
            }
        }
        let mut claim = plan.clone();
        claim.module_fingerprint = raw.fingerprint();
        for wire in [false, true] {
            let errors = if wire {
                let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
                zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
            } else {
                zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err()
            };
            let expected = [
                "E-VERIFY-RESOURCE",
                "E-VERIFY-INITIALIZATION",
                "E-VERIFY-RESOURCE",
                "E-VERIFY-SUM",
            ][mode];
            assert!(
                errors.iter().any(|d| d.code == expected),
                "mode={mode} wire={wire}: {errors:?}"
            );
            eprintln!(
                "G8.6.1.1 repeated/missing debt mode={mode} wire={wire} {}",
                errors[0].code
            );
        }
    }
}

#[test]
fn g8611_verify_generic_view_guard_rejection_is_also_ordinary() {
    let (mut typed, mut plan) = candidate(VIEW_ACROSS);
    let worker = typed
        .functions
        .iter_mut()
        .find(|f| f.name == "worker")
        .unwrap();
    worker.call_kind = CallKind::Direct;
    let tag = worker
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, I::PlaceTagLocal { .. })
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    for i in worker.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if matches!(i.kind, I::TaskYield) {
            i.kind = I::Constant(zil_ir::Constant::Unit);
        } else if matches!(i.kind, I::Binary { left, .. } if left == tag) {
            i.kind = I::Constant(zil_ir::Constant::Bool(true));
        }
    }
    // Local raw precision probe only: the existing generic-place contract
    // rejects this same no-observation guard in ordinary Direct as well.
    plan.module_fingerprint = typed.fingerprint();
    let limits = Limits::default();
    let target = typed.target.clone();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
    for errors in [
        zil_verify::verify_module(typed, plan, &target, &limits).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
    ] {
        assert!(
            errors.iter().any(|d| d.code == "E-VERIFY-SUM"),
            "{errors:?}"
        );
    }
}

#[test]
fn g8611_verify_live_payload_view_blocks_prepared_parent_retag() {
    let text = VIEW_UNKNOWN.replace(
        "let spare=permit;",
        "let other=Packet.Right(Token{code:66});let spare=permit;",
    );
    let (mut raw, mut claim) = candidate(&text);
    let limits = Limits::default();
    let target = raw.target.clone();
    zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap();
    let worker = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "worker")
        .unwrap();
    let other = worker.locals.iter().find(|l| l.name == "other").unwrap().id;
    let (block, root) = worker.blocks.iter().enumerate().find_map(|(b, block)|
        block.instructions.iter().find_map(|i| match &i.kind {
            I::BorrowPlaceLocal { local, path, .. } if matches!(path.first(), Some(zil_ir::PlaceProjection::Variant(v)) if v.variant == 0) => Some((b, *local)),
            _ => None,
        })
    ).unwrap();
    worker.locals[root.0 as usize].mutable = true;
    let output = zil_ir::ValueId(worker.values.len() as u32);
    worker
        .values
        .push(worker.locals[root.0 as usize].ty.clone());
    claim.functions[worker.id.0 as usize]
        .holds
        .push(zil_ir::ValueHold {
            value: output,
            roots: vec![],
        });
    let at = worker.blocks[block]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, I::TaskYield))
        .unwrap();
    let span = worker.blocks[block].instructions[at].span;
    worker.blocks[block].instructions.splice(
        at..at,
        [
            zil_ir::Instruction {
                result: Some(output),
                kind: I::TakeLocal { local: other },
                span,
            },
            zil_ir::Instruction {
                result: None,
                kind: I::DropEnum { local: root },
                span,
            },
            zil_ir::Instruction {
                result: None,
                kind: I::Store {
                    local: root,
                    value: output,
                },
                span,
            },
        ],
    );
    claim.module_fingerprint = raw.fingerprint();
    for wire in [false, true] {
        let errors = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
        } else {
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err()
        };
        assert!(
            errors.iter().any(|d| d.code == "E-BORROW-CONFLICT"),
            "{errors:?}"
        );
        eprintln!(
            "G8.6.1.1 prepared retag wire={wire} rejects at old-parent retirement: {}",
            errors[0].code
        );
    }
}

#[test]
fn g8611_verify_enum_frame_reconstructs_actual_success_tag() {
    let text = "enum Packet{Empty,Left(manages Token),Right(manages Token)}fn worker(flag:bool,give token:Token)->i32{let packet=if flag{Packet.Left(give token)}else{Packet.Right(give token)};task_yield();match packet{Packet.Empty()=>{return 0},Packet.Left(left)=>{return left.code},Packet.Right(right)=>{return right.code}}}";
    let (typed, plan) = candidate(text);
    let limits = Limits::default();
    let target = typed.target.clone();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let mut raw = typed;
    let worker = raw
        .functions
        .iter_mut()
        .find(|f| f.name == "worker")
        .unwrap();
    let unpack = worker
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.kind, I::UnpackVariant { variant: 1, .. }))
        .unwrap();
    let I::UnpackVariant { variant, .. } = &mut unpack.kind else {
        unreachable!()
    };
    *variant = 2; // Equal field Types still require the actual Left success edge.
    let mut claim = plan;
    claim.module_fingerprint = raw.fingerprint();
    for wire in [false, true] {
        let errors = if wire {
            let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
        } else {
            zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err()
        };
        assert!(
            errors.iter().any(|d| d.code == "E-VERIFY-SUM"),
            "{errors:?}"
        );
    }
}

#[test]
fn g8611_verify_enum_leaf_drop_entry_uses_the_actual_nominal_payload() {
    let text = format!(
        "resource Other{{code:i32}}impl Drop for Other{{fn drop(self:Self){{unsafe{{putchar(.code)}}}}}}{PAIR}"
    );
    let (typed, plan) = candidate(&text);
    let limits = Limits::default();
    let target = typed.target.clone();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    let token = typed
        .resources
        .iter()
        .position(|r| r.name.ends_with(":Token"))
        .unwrap();
    let other = typed
        .resources
        .iter()
        .position(|r| r.name.ends_with(":Other"))
        .unwrap();
    for mode in 0..3 {
        let mut raw = typed.clone();
        let expected = "E-VERIFY-RESOURCE";
        if mode == 0 {
            raw.resources[token].teardown = raw.resources[other].teardown;
        } else {
            let drop = raw.resources[token].teardown.unwrap();
            if mode == 1 {
                let foreign = raw
                    .functions
                    .iter()
                    .find(|f| f.name == "putchar")
                    .unwrap()
                    .foreign
                    .clone();
                raw.functions[drop.0 as usize].foreign = foreign;
            } else {
                raw.functions[drop.0 as usize].call_kind = CallKind::Suspend;
            }
        }
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
                errors.iter().any(|d| d.code == expected),
                "mode={mode}: {errors:?}"
            );
            eprintln!("G8.6.1.1 actual Drop entry mode={mode} wire={wire} {expected}");
        }
    }
}

#[test]
fn g8611_verify_enum_frame_budget_is_exact_and_fresh() {
    let (typed, plan) = candidate(PAIR);
    let defaults = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &defaults).unwrap();
    let target = typed.target.clone();
    for wire in [false, true] {
        let run = |work| {
            let limits = Limits {
                max_analysis_steps: work,
                ..defaults.clone()
            };
            let result = if wire {
                zil_verify::verify_module_bytes(&bytes, &target, &limits)
            } else {
                zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits)
            };
            match result {
                Ok(v) => Some(v.fingerprint()),
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    None
                }
            }
        };
        let (mut low, mut high) = (1, defaults.max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if run(mid).is_some() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        let fresh = run(low).unwrap();
        for _ in 0..2 {
            assert!(run(low - 1).is_none());
            assert_eq!(run(low), Some(fresh));
        }
        eprintln!(
            "G8.6.1.1 whole Enum independent Verify wire={wire} N={low} N-1=ResourceLimit fresh/retry=Pass"
        );
    }
}

#[test]
fn g8611_verify_payload_views_keep_the_complete_enum_live() {
    for (name, text, boundaries) in [
        ("same poll", VIEW_SAME, 1),
        ("across yield", VIEW_ACROSS, 1),
        ("runtime tag", VIEW_UNKNOWN, 2),
    ] {
        let (typed, plan) = candidate(text);
        let target = typed.target.clone();
        let limits = Limits::default();
        let worker = typed.functions.iter().find(|f| f.name == "worker").unwrap();
        let (root, address) = worker
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .find_map(|i| match &i.kind {
                I::BorrowPlaceLocal { local, path, .. } if !path.is_empty() => {
                    Some((*local, i.result.unwrap()))
                }
                _ => None,
            })
            .unwrap();
        for equivalent_raw_tag in [false, true] {
            let mut raw = typed.clone();
            if equivalent_raw_tag {
                let worker = raw
                    .functions
                    .iter_mut()
                    .find(|f| f.name == "worker")
                    .unwrap();
                let mut changed = 0;
                for i in worker.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
                    if let I::PlaceTagLocal { local, path } = &i.kind {
                        assert!(path.is_empty());
                        i.kind = I::VariantTagLocal {
                            local: *local,
                            path: vec![],
                        };
                        changed += 1;
                    }
                }
                assert!(changed > 0);
            }
            let mut claim = plan.clone();
            claim.module_fingerprint = raw.fingerprint();
            for wire in [false, true] {
                let verified = if wire {
                    let bytes =
                        zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
                    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap()
                } else {
                    zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap()
                };
                let frame = verified.task_ownership(worker.id).unwrap();
                assert_eq!(frame.boundaries().len(), boundaries, "{name}");
                for boundary in frame.boundaries() {
                    let live = boundary.may_live();
                    assert_eq!(live.len(), 1, "{name}: {frame:?}");
                    assert_eq!(frame.owners()[live[0]].slot(), S::Local(root));
                    assert_eq!(
                        frame.owners()[live[0]].ty(),
                        &worker.locals[root.0 as usize].ty
                    );
                }
                assert_eq!(
                    frame.owner_for_value(address),
                    None,
                    "a borrowed address has no second Drop authority"
                );
                eprintln!(
                    "G8.6.1.1 payload {name} equivalent_raw_tag={equivalent_raw_tag} wire={wire} complete owner retained"
                );
            }
        }
    }
}

#[test]
fn g8611_verify_payload_views_require_actual_tag_guard_and_live_parent() {
    let (typed, plan) = candidate(VIEW_UNKNOWN);
    let limits = Limits::default();
    let target = typed.target.clone();
    zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
    for mode in 0..4 {
        let mut raw = typed.clone();
        let worker = raw
            .functions
            .iter_mut()
            .find(|f| f.name == "worker")
            .unwrap();
        let (block, borrow, root) = worker.blocks.iter().enumerate().find_map(|(b, block)| {
            block.instructions.iter().enumerate().find_map(|(p, i)| match &i.kind {
                I::BorrowPlaceLocal { local, path, .. } if matches!(path.first(), Some(zil_ir::PlaceProjection::Variant(v)) if v.variant == 0) => Some((b, p, *local)),
                _ => None,
            })
        }).unwrap();
        let mut claim = plan.clone();
        match mode {
            0 => {
                let I::BorrowPlaceLocal { path, .. } =
                    &mut worker.blocks[block].instructions[borrow].kind
                else {
                    unreachable!()
                };
                let zil_ir::PlaceProjection::Variant(v) = &mut path[0] else {
                    unreachable!()
                };
                v.variant = 1; // Same payload Type, wrong actual successful branch.
            }
            1 => {
                let permit = worker
                    .locals
                    .iter()
                    .find(|l| l.name == "permit")
                    .unwrap()
                    .id;
                let unrelated = worker
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find_map(|i| match i.kind {
                        I::Load(local) if local == permit => i.result,
                        _ => None,
                    })
                    .unwrap();
                let tag = worker
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find_map(|i| {
                        matches!(i.kind, I::PlaceTagLocal { local, .. } if local == root)
                            .then_some(i.result)
                            .flatten()
                    })
                    .unwrap();
                let guard = worker
                    .blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find_map(|i| match i.kind {
                        I::Binary { left, .. } if left == tag => i.result,
                        _ => None,
                    })
                    .unwrap();
                let branch = worker
                    .blocks
                    .iter_mut()
                    .find_map(|b| match &mut b.terminator {
                        zil_ir::Terminator::Branch { condition, .. } if *condition == guard => {
                            Some(condition)
                        }
                        _ => None,
                    })
                    .unwrap();
                *branch = unrelated; // CFG edges and init claims stay identical.
            }
            2 | 3 => {
                let at = worker.blocks[block]
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, I::TaskYield))
                    .unwrap();
                let mut instruction = worker.blocks[block].instructions[at].clone();
                if mode == 2 {
                    instruction.result = None;
                    instruction.kind = I::DropEnum { local: root };
                } else {
                    let output = zil_ir::ValueId(worker.values.len() as u32);
                    worker
                        .values
                        .push(worker.locals[root.0 as usize].ty.clone());
                    claim.functions[worker.id.0 as usize]
                        .holds
                        .push(zil_ir::ValueHold {
                            value: output,
                            roots: vec![],
                        });
                    instruction.result = Some(output);
                    instruction.kind = I::TakeLocal { local: root };
                }
                worker.blocks[block].instructions.insert(at, instruction);
            }
            _ => unreachable!(),
        }
        claim.module_fingerprint = raw.fingerprint();
        for wire in [false, true] {
            let errors = if wire {
                let bytes = zil_ir::wire::encode_module_candidate(&raw, &claim, &limits).unwrap();
                zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err()
            } else {
                zil_verify::verify_module(raw.clone(), claim.clone(), &target, &limits).unwrap_err()
            };
            let expected = if mode < 2 {
                "E-VERIFY-SUM"
            } else {
                "E-BORROW-CONFLICT"
            };
            assert!(
                errors.iter().any(|d| d.code == expected),
                "mode={mode} wire={wire}: {errors:?}"
            );
            eprintln!(
                "G8.6.1.1 payload neighbour mode={mode} wire={wire}: {} {}",
                errors[0].code, errors[0].message
            );
        }
    }
}
