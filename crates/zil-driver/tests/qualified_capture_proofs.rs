//! Producer candidates are deliberately re-fingerprinted after each forgery.
//! Verify must reconstruct responsibility and origins independently.
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{CandidatePlan, InstructionKind, TypedModule};

const SOURCE: &str = r#"
resource R{code:i32}
package Mixed<life a>{manages owned:R;pointer:&'a i32}
fn wrap<life a>(pointer:&'a i32)->Mixed<'a>{return Mixed::<'a>{owned:R{code:7},pointer:pointer}}
fn make<life a>(holder:Mixed<'a>,other:Mixed<'a>)->impl Fn()->i32{return move()=>holder.owned.code+*holder.pointer}
fn main()->i32{let a=35;let b=9;let f=make(wrap(&a),wrap(&b));return f()-42}
"#;

fn candidate(source: &str) -> (TypedModule, CandidatePlan) {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "qualified-capture.zil", source);
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
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
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    (module, plan)
}

fn check(module: &TypedModule, plan: &CandidatePlan, accepted: bool) {
    let mut plan = plan.clone();
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let raw = zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits);
    let bytes = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
    let wire = zil_verify::verify_module_bytes(&bytes, &module.target, &limits);
    if accepted {
        raw.unwrap();
        wire.unwrap();
    } else {
        assert!(raw.is_err(), "forged raw qualified capture was accepted");
        assert!(wire.is_err(), "forged Wire qualified capture was accepted");
    }
}

#[test]
fn whole_qualified_capture_requires_both_cleanup_and_real_slot_origins() {
    let (module, plan) = candidate(SOURCE);
    check(&module, &plan, true);
    let factory = module
        .functions
        .iter()
        .position(|f| f.name == "make")
        .unwrap();
    let contract = module.functions[factory]
        .callable_return_contract
        .as_ref()
        .unwrap();
    assert_eq!(contract.cleanup, [0]);
    assert!(!contract.capture_sources.as_ref().unwrap().is_empty());
    for mutation in 0..4 {
        let mut forged = module.clone();
        let contract = forged.functions[factory]
            .callable_return_contract
            .as_mut()
            .unwrap();
        match mutation {
            0 => contract.cleanup.clear(),
            1 => contract.capture_sources = None,
            2 => contract.capture_sources = Some(Vec::new()),
            3 => {
                // Same nominal type, same abstract life class, different
                // owning argument and actual pointer slot.
                for row in contract.capture_sources.as_mut().unwrap() {
                    for source in &mut row.sources {
                        let zil_ir::StorageSource::Exact(position) = source else {
                            panic!("fixture requires exact source");
                        };
                        position.base = zil_ir::StorageBase::Parameter(1);
                    }
                }
            }
            _ => unreachable!(),
        }
        check(&forged, &plan, false);
    }
}

#[test]
fn qualified_capture_cannot_be_copied_or_lose_its_owner_cleanup() {
    let (module, plan) = candidate(SOURCE);
    check(&module, &plan, true);
    let mut forged = module.clone();
    let definition = forged.closures[0].clone();
    let entry = &mut forged.functions[definition.function.0 as usize];
    let value = zil_ir::ValueId(entry.values.len() as u32);
    entry.values.push(definition.captures[0].clone());
    entry.blocks[entry.entry.0 as usize].instructions.insert(
        2,
        zil_ir::Instruction {
            result: Some(value),
            kind: InstructionKind::ReadCapture { index: 0 },
            span: entry.span,
        },
    );
    let mut copied_plan = plan.clone();
    copied_plan.functions[definition.function.0 as usize]
        .holds
        .push(zil_ir::ValueHold {
            value,
            roots: vec![zil_ir::HoldRoot::Capture(0)],
        });
    check(&forged, &copied_plan, false);

    let mut missing = module.clone();
    let main = missing
        .functions
        .iter_mut()
        .find(|f| f.name == "main")
        .unwrap();
    let mut removed = 0;
    for block in &mut main.blocks {
        block.instructions.retain(|instruction| {
            if matches!(instruction.kind, InstructionKind::DropClosure { .. }) {
                removed += 1;
                false
            } else {
                true
            }
        });
    }
    assert!(
        removed > 0,
        "candidate did not exercise whole environment cleanup"
    );
    check(&missing, &plan, false);
}

#[test]
fn owning_capture_keeps_the_original_parent_loan_until_the_last_read() {
    let (module, plan) = candidate(SOURCE);
    check(&module, &plan, true);
    for before_last_read in [true, false] {
        let mut changed = module.clone();
        let main = changed
            .functions
            .iter()
            .position(|f| f.name == "main")
            .unwrap();
        let function = &mut changed.functions[main];
        let local = function
            .locals
            .iter()
            .position(|local| local.name == "a")
            .unwrap();
        function.locals[local].mutable = true;
        let value = zil_ir::ValueId(function.values.len() as u32);
        function.values.push(zil_ir::Type::I32);
        let block = function
            .blocks
            .iter_mut()
            .find(|block| {
                block
                    .instructions
                    .iter()
                    .any(|i| matches!(i.kind, InstructionKind::CallClosureShared { .. }))
            })
            .unwrap();
        let call = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::CallClosureShared { .. }))
            .unwrap();
        let at = call + usize::from(!before_last_read);
        block.instructions.splice(
            at..at,
            [
                zil_ir::Instruction {
                    result: Some(value),
                    kind: InstructionKind::Constant(zil_ir::Constant::I32(99)),
                    span: function.span,
                },
                zil_ir::Instruction {
                    result: None,
                    kind: InstructionKind::Store {
                        local: zil_ir::LocalId(local as u32),
                        value,
                    },
                    span: function.span,
                },
            ],
        );
        let mut changed_plan = plan.clone();
        changed_plan.functions[main].holds.push(zil_ir::ValueHold {
            value,
            roots: Vec::new(),
        });
        check(&changed, &changed_plan, !before_last_read);
    }
}
