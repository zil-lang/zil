//! Actual replacement IR in the source component; no fake Store or foreign proof.
use super::*;
fn request() -> (TypedModule, CandidatePlan) {
    let mut module = crate::place_replacements::fixture::module();
    let op = crate::place_replacements::fixture::instruction;
    let body = &mut module.functions[0].blocks[0].instructions;
    body.insert(
        0,
        op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
    );
    body.extend([
        op(None, InstructionKind::StorageDead(LocalId(1))),
        op(None, InstructionKind::StorageDead(LocalId(0))),
        op(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ]);
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions.push(FunctionPlan {
        function: FunctionId(0),
        holds: vec![
            ValueHold {
                value: ValueId(0),
                roots: vec![HoldRoot::Parameter(0)],
            },
            ValueHold {
                value: ValueId(1),
                roots: vec![],
            },
        ],
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![LocalId(0), LocalId(1)],
            initialized_out: vec![],
        }],
    });
    (module, plan)
}
fn run(module: &TypedModule, plan: &CandidatePlan, limit: usize) -> Check<()> {
    let limits = Limits {
        max_analysis_steps: limit,
        ..Limits::default()
    };
    // Run every ordinary responsibility/descriptor-retirement check as well;
    // its result is not used as a source/permission witness below.
    let mut complete = plan.clone();
    complete.module_fingerprint = module.fingerprint();
    crate::verify_module(module.clone(), complete, &module.target, &limits)
        .map_err(|errors| errors.into_iter().next().unwrap())?;
    let mut budget = Budget::new(&limits);
    budget.function_count = module.functions.len();
    budget.replacements = true;
    // Explicitly exercise the mixed stored-source component used by the real
    // application. This schedules a checker; it supplies no typed Store/proof.
    budget.store_option_results = true;
    budget.stored_replacement_functions.insert(FunctionId(0));
    let graphs = module
        .functions
        .iter()
        .zip(&plan.functions)
        .map(|(f, p)| check_function(module, f, p, &mut budget, &limits))
        .collect::<Check<Vec<_>>>()?;
    qualifications(module, &limits, None, &mut budget)?;
    for f in &module.functions {
        header(module, f, &mut budget)?;
    }
    super::super::check(module, &graphs, plan, &mut budget)
}
fn both(module: &TypedModule, plan: &CandidatePlan) -> [Check<()>; 2] {
    let mut plan = plan.clone();
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
    let (decoded, transported) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
    [
        run(module, &plan, limits.max_analysis_steps),
        run(&decoded, &transported, limits.max_analysis_steps),
    ]
}
#[test]
fn actual_mutable_endpoint_and_complete_rhs_settle_exact_reservation_raw_wire() {
    let (module, plan) = request();
    for result in both(&module, &plan) {
        result.unwrap();
    }
    for attack in 0..6 {
        let mut changed = module.clone();
        let f = &mut changed.functions[0];
        let reserve = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::ReservePlaceAddress { .. }))
            .unwrap();
        let commit = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }))
            .unwrap();
        match attack {
            0 => {
                f.blocks[0].instructions.remove(reserve);
            }
            1 => {
                let duplicate = f.blocks[0].instructions[commit].clone();
                f.blocks[0].instructions.insert(commit, duplicate);
            }
            2 => {
                f.blocks[0].instructions.remove(commit);
            }
            3 => {
                f.blocks[0].instructions[commit].kind = InstructionKind::AbortPlaceReservation {
                    reservation: ReservationId(71),
                };
                f.blocks[0].instructions.insert(
                    commit + 1,
                    crate::place_replacements::fixture::instruction(
                        None,
                        InstructionKind::CommitPlaceReplace {
                            reservation: ReservationId(71),
                            value: ValueId(1),
                        },
                    ),
                );
            }
            4 => {
                let Type::Address { mutable, .. } = &mut f.values[0] else {
                    panic!()
                };
                *mutable = false;
            }
            _ => {
                f.values[1] = Type::Desc(Box::new(Type::Record {
                    name: "TicketToken".into(),
                    fields: vec![("n".into(), Type::I32)],
                }));
                f.locals[1].ty = f.values[1].clone();
            }
        }
        for result in both(&changed, &plan) {
            let error = result.unwrap_err();
            assert_ne!(
                error.category,
                zil_base::ResultCategory::ResourceLimit,
                "{attack}: {error:?}"
            );
        }
    }
}
