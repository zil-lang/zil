//! Independent kernel controls derived from real Source, without importing
//! the separately authored Analyze continuation qualification.
use super::*;
use zil_ir::{CallKind, InstructionKind as I, Type};
use zil_verify::{TaskOwnerAction as A, TaskOwnerSlot as S};

const RESOURCE: &str = "extern \"C\" fn putchar(code:i32)->i32;resource Token{code:i32}impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}fn consume(give token:Token){}";
const PAIR: &str = "enum Packet{Empty,Pair(manages Token,manages Token)}fn worker(give a:Token,give b:Token)->i32{let packet=Packet.Pair(give a,give b);task_yield();match packet{Packet.Empty()=>{return 0},Packet.Pair(left,right)=>{return left.code+right.code}}}";

fn candidate(text: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::default();
    let text = format!("{RESOURCE}{text}fn main(){{}}");
    let typed = zil_types::check(&expand(&text, &limits).unwrap().core, &target, &limits).unwrap();
    // Preserve Yield's Unit SSA definition and initialization effect using a
    // Unit constant plus Direct header to obtain untrusted init/Hold rows from the
    // already supported ordinary Sum path. Verify sees the original actual
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
            assert!(
                errors
                    .iter()
                    .any(|d| d.category == ResultCategory::SafeReject),
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
