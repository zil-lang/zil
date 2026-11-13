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
    component(module, plan, limit)
}
fn component(module: &TypedModule, plan: &CandidatePlan, limit: usize) -> Check<()> {
    let limits = Limits {
        max_analysis_steps: limit,
        ..Limits::default()
    };
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

#[test]
fn old_head_child_and_weak_description_do_not_survive_commit_raw_wire() {
    let (module, plan) = request();
    for weak in [false, true] {
        let mut changed = module.clone();
        let mut claim = plan.clone();
        let f = &mut changed.functions[0];
        let owner = f.locals[1].ty.clone();
        let Type::Managed(payload) = &owner else {
            panic!()
        };
        let payload = payload.clone();
        f.values.extend([
            Type::Address {
                mutable: true,
                inner: Box::new(owner.clone()),
            },
            Type::Address {
                mutable: false,
                inner: Box::new(owner),
            },
            Type::Desc(payload),
            Type::I32,
        ]);
        let op = crate::place_replacements::fixture::instruction;
        f.blocks[0].instructions.insert(
            2,
            op(
                Some(ValueId(2)),
                InstructionKind::AddressField {
                    address: ValueId(0),
                    index: 0,
                },
            ),
        );
        f.blocks[0].instructions.insert(
            3,
            op(
                Some(ValueId(3)),
                InstructionKind::ReborrowAddress {
                    address: ValueId(2),
                    mutable: false,
                },
            ),
        );
        let commit = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }))
            .unwrap();
        f.blocks[0].instructions.insert(
            if weak { commit } else { commit + 1 },
            op(
                Some(ValueId(4)),
                InstructionKind::DescribeAddress {
                    address: ValueId(3),
                },
            ),
        );
        let after = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }))
            .unwrap()
            + 1;
        let read_at = if weak { after } else { after + 1 };
        f.blocks[0].instructions.insert(
            read_at,
            op(
                Some(ValueId(5)),
                InstructionKind::ReadDesc {
                    desc: ValueId(4),
                    index: 0,
                },
            ),
        );
        claim.functions[0].holds.extend([
            ValueHold {
                value: ValueId(2),
                roots: vec![HoldRoot::Parameter(0)],
            },
            ValueHold {
                value: ValueId(3),
                roots: vec![HoldRoot::Parameter(0)],
            },
            ValueHold {
                value: ValueId(4),
                // The actual Desc keeps its epoch privately; Address values
                // above retain the strong Parameter dependency.
                roots: Vec::new(),
            },
            ValueHold {
                value: ValueId(5),
                roots: vec![],
            },
        ]);
        for result in both(&changed, &claim) {
            let error = result.unwrap_err();
            eprintln!("replacement weak={weak}: {error:?}");
            assert_eq!(error.category, zil_base::ResultCategory::SafeReject);
            assert!(
                error.message.contains("retir")
                    || error.message.contains("overlap")
                    || error.message.contains("loan")
                    || error.message.contains("borrow"),
                "{error:?}"
            );
        }
    }
}

#[test]
fn prepared_actual_sources_have_exact_fresh_component_budgets_raw_wire() {
    let (module, mut plan) = request();
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    let (decoded, transported) =
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap();
    for (name, module, plan) in [
        ("raw", &module, &plan),
        ("decoded Wire", &decoded, &transported),
    ] {
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        component(module, plan, high).unwrap();
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            match component(module, plan, mid) {
                Ok(()) => high = mid,
                Err(error) => {
                    assert_eq!(
                        error.category,
                        zil_base::ResultCategory::ResourceLimit,
                        "{error:?}"
                    );
                    low = mid;
                }
            }
        }
        component(module, plan, high).unwrap();
        assert_eq!(
            component(module, plan, high - 1).unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        component(module, plan, high).unwrap();
        eprintln!("V replacement mixed component {name} N={high}, N-1/fresh checked");
    }
}
