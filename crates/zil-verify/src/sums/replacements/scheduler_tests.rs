//! Frozen complete full-scan check, before ordered dirty scheduling.
use super::*;
pub(super) fn full_scan(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    b: &mut Budget,
) -> Check<Proofs> {
    let actual = actual::Program::reconstruct(module, graphs, b)?;
    b.spend(module.functions.len().saturating_mul(3))?;
    let mut places = Vec::with_capacity(module.functions.len());
    let mut result = Vec::with_capacity(module.functions.len());
    let mut effects = vec![Rows::new(); module.functions.len()];
    let mut whole_must_rows = BTreeSet::new();
    let store_calls = RefCell::new(BTreeSet::new());
    for (f, graph) in module.functions.iter().zip(graphs) {
        let inventory = Inventory::reconstruct(module, f, b)?;
        if whole_must(f, &inventory, b)? {
            b.spend(whole_must_rows.len().saturating_add(1))?;
            whole_must_rows.insert(f.id.0 as usize);
        }
        if f.foreign.is_none() {
            inventory.activation_entries(f, graph, b)?;
        }
        places.push(inventory);
        result.push(content(&f.return_type, None, &mut Vec::new(), true, b)?);
    }
    loop {
        let mut changed = false;
        for (f, graph) in module.functions.iter().zip(graphs) {
            b.spend(1)?;
            if f.foreign.is_some() {
                continue;
            }
            trace::solve(f.id.0 as usize);
            let context = Context {
                actual: &actual,
                places: &places,
                summaries: &effects,
                whole_must: &whole_must_rows,
                written: RefCell::new(Rows::new()),
                store_calls: &store_calls,
                observed: None,
            };
            let (found, _, _) =
                run_with_replacements(module, f, graph, &result, false, b, Some(&context))?;
            let rows = context.written.into_inner();
            changed |= result[f.id.0 as usize].merge(&found, b)?;
            b.spend(rows.len())?;
            for (target, payload) in rows {
                changed |= merge_row(&mut effects[f.id.0 as usize], &target, &payload, b)?;
            }
        }
        trace::round(&result, &effects, &store_calls, b);
        if !changed {
            break;
        }
    }
    let mut rows = Vec::with_capacity(module.functions.len());
    let mut excluded_edges = BTreeMap::new();
    for (f, graph) in module.functions.iter().zip(graphs) {
        if f.foreign.is_some() {
            rows.push(Vec::new());
            continue;
        }
        let context = Context {
            actual: &actual,
            places: &places,
            summaries: &effects,
            whole_must: &whole_must_rows,
            written: RefCell::new(Rows::new()),
            store_calls: &store_calls,
            observed: None,
        };
        let (_, bridges, excluded) =
            run_with_replacements(module, f, graph, &result, true, b, Some(&context))?;
        rows.push(bridges);
        if !excluded.is_empty() {
            b.spend(excluded_edges.len().saturating_add(1))?;
            excluded_edges.insert(f.id.0 as usize, excluded);
        }
    }
    trace::final_state(&result, &effects, &store_calls, b);
    b.spend(effects.len())?;
    let mut replacement_writes = Vec::with_capacity(effects.len());
    let needed = store_calls.into_inner();
    let mut replacement_targets = BTreeMap::new();
    let mut retirement_targets = BTreeMap::new();
    for (entry, effect) in effects.into_iter().enumerate() {
        let mut writes = BTreeSet::new();
        let mut targets = Vec::new();
        let mut retired = Vec::new();
        b.spend(needed.len())?;
        let transfer = needed.contains(&entry);
        b.spend(effect.len())?;
        for (target, _) in effect {
            let root = match target.base {
                actual::Base::Input(p) | actual::Base::InputCapture(p, _) => {
                    Some(resources::Root::Parameter(p))
                }
                actual::Base::Environment(c) => Some(resources::Root::BorrowedCapture(c)),
                _ => None,
            };
            if let Some(root) = root {
                lookup(b, writes.len())?;
                writes.insert(root);
                if transfer {
                    b.spend(1)?;
                    targets.push(target);
                } else {
                    // The exact input/capture ordinal and full route survive
                    // the boundary. Ordinary write roots lose this identity.
                    b.spend(target.steps.len().saturating_add(1))?;
                    retired.push(target);
                }
            }
        }
        replacement_writes.push(writes);
        if !targets.is_empty() {
            b.spend(replacement_targets.len().saturating_add(1))?;
            replacement_targets.insert(entry, targets);
        }
        if !retired.is_empty() {
            b.spend(retirement_targets.len().saturating_add(1))?;
            retirement_targets.insert(entry, retired);
        }
    }
    Ok(Proofs {
        rows,
        excluded_edges,
        replacement_writes,
        replacement_targets,
        retirement_targets,
    })
}

use crate::place_replacements::fixture;
fn budget(module: &TypedModule, n: usize) -> Budget {
    let mut b = Budget::new(&Limits {
        max_analysis_steps: n,
        ..Limits::default()
    });
    b.replacements = true;
    b.sums = true;
    b.pod_stores = true;
    b.head_descriptions = true;
    b.function_count = module.functions.len();
    b.sum_functions = vec![true; module.functions.len()];
    b
}
fn graphs(module: &TypedModule) -> Vec<FunctionGraph> {
    module
        .functions
        .iter()
        .map(|f| FunctionGraph {
            role_data_uses: vec![],
            successors: vec![vec![]; f.blocks.len()],
            predecessors: vec![vec![]; f.blocks.len()],
            reachable: vec![true; f.blocks.len()],
            package_views: packages::CaptureViews::default(),
        })
        .collect()
}
type BridgeFacts = (Option<u8>, FieldFresh, Vec<UnpackedFresh>);
#[derive(Debug, PartialEq, Eq)]
struct Proof {
    rows: Vec<Vec<Vec<BridgeFacts>>>,
    excluded: BTreeMap<usize, ExcludedEdges>,
    writes: Vec<BTreeSet<resources::Root>>,
    targets: BTreeMap<usize, Vec<actual::Target>>,
    retired: BTreeMap<usize, Vec<actual::Target>>,
}
impl From<Proofs> for Proof {
    fn from(p: Proofs) -> Self {
        Self {
            rows: p
                .rows
                .into_iter()
                .map(|f| {
                    f.into_iter()
                        .map(|block| {
                            block
                                .into_iter()
                                .map(|b| (b.result, b.package, b.unpack))
                                .collect()
                        })
                        .collect()
                })
                .collect(),
            excluded: p.excluded_edges,
            writes: p.replacement_writes,
            targets: p.replacement_targets,
            retired: p.retirement_targets,
        }
    }
}
fn run(module: &TypedModule, full: bool, n: usize) -> (Check<Proof>, trace::Trace, Vec<String>) {
    let _mode = trace::Mode::enter(full);
    trace::start();
    let mut b = budget(module, n);
    let result = check(module, &graphs(module), &mut b).map(Proof::from);
    let meta = trace::metadata(&b);
    let trace = trace::finish();
    (result, trace, meta)
}
fn compare(module: &TypedModule) -> (Proof, trace::Trace, trace::Trace) {
    let (a, old, ma) = run(module, true, 10_000_000);
    let (b, new, mb) = run(module, false, 10_000_000);
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a, b);
    assert_eq!(ma, mb);
    assert_eq!(old.final_state, new.final_state);
    assert_eq!(old.store_history, new.store_history);
    assert!(!new.rounds.is_empty());
    assert!(new.rounds.len() <= old.rounds.len());
    for (i, expected) in old.rounds.iter().enumerate() {
        let actual = new
            .rounds
            .get(i)
            .unwrap_or_else(|| new.rounds.last().unwrap());
        assert_eq!(actual, expected, "original round {i} differs");
    }
    (b, old, new)
}
fn chain(count: usize) -> TypedModule {
    let mut module = fixture::module();
    let leaf = module.functions.pop().unwrap();
    for id in 0..count {
        let mut f = leaf.clone();
        f.id = FunctionId(id as u32);
        f.name = format!("chain{id}");
        if id + 1 < count {
            f.blocks[0].instructions = vec![
                fixture::instruction(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
                fixture::instruction(
                    Some(ValueId(1)),
                    InstructionKind::TakeLocal { local: LocalId(1) },
                ),
                fixture::instruction(
                    None,
                    InstructionKind::Call {
                        function: FunctionId((id + 1) as u32),
                        arguments: vec![ValueId(0), ValueId(1)],
                    },
                ),
            ];
        }
        module.functions.push(f);
    }
    module
}
#[test]
fn original_rounds_and_complete_proofs_match_through_reverse_helper_chain() {
    let module = chain(8);
    let (_, old, new) = compare(&module);
    assert!(new.solves.len() < old.solves.len());
    assert!(new.reads[&0].contains(&1));
    println!(
        "V ordered reverse8 solves={} old={}",
        new.solves.len(),
        old.solves.len()
    );
    for fresh in [false, true] {
        compare(&fixture::module_with_relay_and_caller(fresh));
    }
}
#[test]
fn ordered_round_flags_preserve_current_next_self_and_cycle_boundaries() {
    let mut b = Budget::new(&Limits::default());
    let mut o = ordered::Ordered::new(3, &mut b).unwrap();
    assert!(o.run(0, &mut b).unwrap());
    o.observe(0, BTreeSet::from([0, 1]), &mut b).unwrap();
    o.changed(0, &mut b).unwrap();
    assert!(o.run(1, &mut b).unwrap());
    o.observe(1, BTreeSet::from([0]), &mut b).unwrap();
    o.changed(1, &mut b).unwrap();
    assert!(o.run(2, &mut b).unwrap());
    assert!(o.advance(&mut b).unwrap());
    assert!(o.run(0, &mut b).unwrap());
    assert!(!o.run(1, &mut b).unwrap());
    assert!(!o.run(2, &mut b).unwrap());
    assert!(!o.advance(&mut b).unwrap());
    let mut o = ordered::Ordered::new(2, &mut b).unwrap();
    o.observe(1, BTreeSet::from([0]), &mut b).unwrap();
    assert!(o.run(0, &mut b).unwrap());
    o.changed(0, &mut b).unwrap();
    assert!(o.run(1, &mut b).unwrap());
    assert!(!o.advance(&mut b).unwrap());
}
#[test]
fn ordered_summary_requests_keep_exact_fresh_budget_boundaries() {
    let module = chain(8);
    let boundary = |full| {
        let mut lo = 0;
        let mut hi = 10_000_000;
        assert!(run(&module, full, hi).0.is_ok());
        while lo < hi {
            let mid = (lo + hi) / 2;
            if run(&module, full, mid).0.is_ok() {
                hi = mid
            } else {
                lo = mid + 1
            }
        }
        lo
    };
    let n = boundary(false);
    let old = boundary(true);
    assert!(n < old, "{n} >= {old}");
    let a = run(&module, false, n).0.unwrap();
    assert_eq!(a, run(&module, true, old).0.unwrap());
    assert_eq!(
        run(&module, false, n - 1).0.unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert_eq!(a, run(&module, false, n).0.unwrap());
    println!("V ordered full check N={n}, full scan={old}; N-1/fresh checked");
}

#[path = "scheduler_contexts.rs"]
mod contexts;
#[test]
fn actual_public_helper_and_callback_reads_are_registered_before_their_rows_grow() {
    let module = contexts::public_callback_input_fixture();
    let (proofs, _, new) = compare(&module);
    assert!(new.reads[&0].contains(&1));
    assert!(new.reads[&0].contains(&2));
    assert!(new.reads[&1].contains(&2));
    assert!(!proofs.retired[&0].is_empty());
    assert!(!proofs.retired[&1].is_empty());
}
#[test]
fn callback_environment_and_ordinary_input_effects_propagate_in_the_original_rounds() {
    let mut module = contexts::public_callback_input_fixture();
    let address = module.functions[1].locals[1].ty.clone();
    let owner = module.functions[1].locals[2].ty.clone();
    module.closures[0].captures = vec![address.clone()];
    for f in &mut module.functions[..2] {
        let role = &mut f.callable_contract.as_mut().unwrap().roles[0];
        role.hold = vec![0];
        role.writes = vec![0];
    }
    let callback = &mut module.functions[2];
    callback.values = vec![address, Type::I32, owner];
    callback.blocks[0].instructions = vec![
        fixture::instruction(Some(ValueId(0)), InstructionKind::ReadCapture { index: 0 }),
        fixture::instruction(
            Some(ValueId(1)),
            InstructionKind::Constant(Constant::I32(73)),
        ),
        fixture::instruction(
            Some(ValueId(2)),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(1)],
            },
        ),
        fixture::instruction(
            None,
            InstructionKind::ReservePlaceAddress {
                reservation: zil_ir::ReservationId(71),
                address: ValueId(0),
                path: vec![PlaceProjection::Field(0)],
            },
        ),
        fixture::instruction(
            None,
            InstructionKind::CommitPlaceReplace {
                reservation: zil_ir::ReservationId(71),
                value: ValueId(2),
            },
        ),
    ];
    callback.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    let (proofs, _, new) = compare(&module);
    assert!(new.reads[&0].contains(&1));
    assert!(new.reads[&0].contains(&2));
    let final_state = new.final_state.unwrap();
    assert!(
        final_state.effects[0]
            .keys()
            .any(|t| matches!(t.base, actual::Base::Input(1)))
    );
    assert!(
        final_state.effects[0]
            .keys()
            .any(|t| matches!(t.base, actual::Base::InputCapture(0, 0)))
    );
    assert!(!proofs.retired[&2].is_empty());
}
fn element_caller() -> TypedModule {
    let mut module = fixture::module();
    let mut helper = module.functions[0].clone();
    helper.id = FunctionId(1);
    let address = helper.values[0].clone();
    let Type::Address { inner: packet, .. } = &address else {
        unreachable!()
    };
    let store = Type::Desc(Box::new(Type::OwnedStore {
        element: packet.clone(),
    }));
    let caller = &mut module.functions[0];
    caller.locals[0].ty = store.clone();
    caller.values = vec![store, Type::Usize, address, helper.values[1].clone()];
    caller.blocks[0].instructions = vec![
        fixture::instruction(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
        fixture::instruction(
            Some(ValueId(1)),
            InstructionKind::Constant(Constant::Usize(0)),
        ),
        fixture::instruction(
            Some(ValueId(2)),
            InstructionKind::OwnedStoreBorrowMut {
                store: ValueId(0),
                index: ValueId(1),
            },
        ),
        fixture::instruction(
            Some(ValueId(3)),
            InstructionKind::TakeLocal { local: LocalId(1) },
        ),
        fixture::instruction(
            None,
            InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![ValueId(2), ValueId(3)],
            },
        ),
    ];
    module.functions.push(helper);
    module
}
#[test]
fn actual_element_substitution_preserves_historical_store_calls_and_transfer_targets() {
    // This phase input supplies no loan permission or full Store admission.
    // The actual Element recipe must still come from OwnedStoreBorrowMut.
    let module = element_caller();
    let (proofs, old, new) = compare(&module);
    assert!(!new.store_history.is_empty());
    assert_eq!(old.store_history, new.store_history);
    assert!(new.final_state.as_ref().unwrap().store.contains(&1));
    assert!(!proofs.targets[&1].is_empty());
}
#[test]
fn actual_self_and_mutual_calls_keep_complete_round_and_proof_equivalence() {
    for count in [1, 2, 3] {
        let mut module = chain(count);
        let f = &mut module.functions[count - 1];
        f.blocks[0].instructions = vec![
            fixture::instruction(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
            fixture::instruction(
                Some(ValueId(1)),
                InstructionKind::TakeLocal { local: LocalId(1) },
            ),
            fixture::instruction(
                None,
                InstructionKind::Call {
                    function: FunctionId(0),
                    arguments: vec![ValueId(0), ValueId(1)],
                },
            ),
        ];
        let (_, _, new) = compare(&module);
        assert!(new.reads[&(count - 1)].contains(&0));
    }
}
fn resource_result(module: &TypedModule, full: bool) -> (Check<()>, trace::Trace, Vec<String>) {
    let _mode = trace::Mode::enter(full);
    trace::start();
    let mut b = budget(module, 10_000_000);
    let result = resources::check_module(module, &graphs(module), &mut b);
    let meta = trace::metadata(&b);
    let trace = trace::finish();
    (result, trace, meta)
}
fn resource_rejection(module: &TypedModule, text: &str) {
    let (a, old, ma) = resource_result(module, true);
    let (b, new, mb) = resource_result(module, false);
    assert_eq!(a, b);
    assert_eq!(ma, mb);
    assert_eq!(old.store_history, new.store_history);
    let error = a.unwrap_err();
    assert_eq!(error.category, zil_base::ResultCategory::SafeReject);
    assert!(error.message.contains(text), "{error:?}");
}
#[test]
fn resource_retirement_initialization_and_unpaid_errors_remain_real_rejections() {
    let module = fixture::module();
    resource_result(&module, true).0.unwrap();
    resource_result(&module, false).0.unwrap();
    resource_rejection(&contexts::old_descriptor_fixture(), "retirement");
    let mut missing = module.clone();
    missing.functions[0].parameters.pop();
    resource_rejection(&missing, "give requires a live owned position");
    let mut extra = module;
    let f = &mut extra.functions[0];
    let mut local = f.locals[1].clone();
    local.id = LocalId(2);
    f.locals.push(local);
    f.parameters.push(LocalId(2));
    f.blocks[0].instructions.push(fixture::instruction(
        None,
        InstructionKind::DropManaged { local: LocalId(2) },
    ));
    resource_result(&extra, true).0.unwrap();
    resource_result(&extra, false).0.unwrap();
    extra.functions[0].blocks[0].instructions.pop();
    resource_rejection(&extra, "outstanding resource responsibility");
}

#[test]
fn registered_pod_carrier_states_match_in_every_original_summary_round() {
    let mut module = chain(8);
    let payload = Type::OwnedStore {
        element: Box::new(Type::I32),
    };
    let owner = Type::Managed(Box::new(payload.clone()));
    module.resources = vec![ResourceDefinition {
        name: dynamic_store::definition_name(&Type::I32),
        payload,
        teardown: None,
    }];
    for f in &mut module.functions {
        let Type::Address { inner, .. } = &mut f.locals[0].ty else {
            panic!("chain input is an actual mutable Package address")
        };
        let Type::Package { fields, .. } = inner.as_mut() else {
            panic!("chain input has the complete Package")
        };
        fields[0].1 = owner.clone();
        f.locals[1].ty = owner.clone();
        f.values[0] = f.locals[0].ty.clone();
        f.values[1] = owner.clone();
    }
    // A real, unused module-owned slot must remain inventory-only after both
    // solvers finish; it is not a fabricated Cache entry or a temporary Type.
    let mut unused = module.functions[0].locals[1].clone();
    unused.id = LocalId(2);
    unused.name = "unqueried_store".into();
    module.functions[0].locals.push(unused);
    fn field(ty: &Type) -> &Type {
        let Type::Address { inner, .. } = ty else {
            panic!()
        };
        let Type::Package { fields, .. } = inner.as_ref() else {
            panic!()
        };
        &fields[0].1
    }
    let module_key = &module as *const TypedModule as usize;
    let mut inventory = BTreeMap::new();
    for f in &module.functions {
        for ty in [
            field(&f.locals[0].ty),
            &f.locals[1].ty,
            field(&f.values[0]),
            &f.values[1],
        ] {
            inventory.insert(ty as *const Type as usize, false);
        }
    }
    let unused_key = &module.functions[0].locals[2].ty as *const Type as usize;
    inventory.insert(unused_key, false);
    assert_eq!(inventory.len(), module.functions.len() * 4 + 1);
    let seed = &module.functions[0].locals[1].ty;
    let seed_key = seed as *const Type as usize;
    let run = |full| {
        let _mode = trace::Mode::enter(full);
        checkpoint::drain_hits();
        trace::start();
        let mut b = budget(&module, 10_000_000);
        // Use the production raw-root order and complete shape check. No cache
        // is copied between requests, and no success flag is assigned by a test.
        for f in &module.functions {
            for ty in std::iter::once(&f.return_type)
                .chain(f.locals.iter().map(|local| &local.ty))
                .chain(&f.values)
            {
                check_type_shape(ty, &module.target, &b.limits.clone(), &mut b).unwrap();
                dynamic_store::register_actual_carrier_root(&module, ty, &mut b).unwrap();
            }
        }
        assert_eq!(
            b.pod_carriers.as_ref().unwrap().snapshot(),
            (module_key, inventory.clone())
        );
        dynamic_store::check_carrier(&module, seed, None, &mut b).unwrap();
        let mut seeded = inventory.clone();
        seeded.insert(seed_key, true);
        assert_eq!(
            b.pod_carriers.as_ref().unwrap().snapshot(),
            (module_key, seeded)
        );
        let proof = check(&module, &graphs(&module), &mut b)
            .map(Proof::from)
            .unwrap();
        let hits = checkpoint::drain_hits();
        if full {
            assert!(hits.is_empty());
        } else {
            assert!(
                hits.contains(&0),
                "registered actual carriers must participate in a guarded checkpoint hit"
            );
        }
        let after = b.pod_carriers.as_ref().unwrap().snapshot();
        assert_eq!(after.0, module_key);
        assert_eq!(
            after.1.keys().collect::<Vec<_>>(),
            inventory.keys().collect::<Vec<_>>()
        );
        assert!(after.1[&seed_key]);
        assert!(!after.1[&unused_key]);
        assert!(
            after
                .1
                .iter()
                .any(|(key, ready)| *key != seed_key && *ready)
        );
        let metadata = trace::metadata(&b);
        assert!(metadata.contains(&format!("pod-carriers={:?}", Some(after))));
        (proof, trace::finish(), metadata)
    };
    let (old_proof, old, old_metadata) = run(true);
    let (new_proof, new, new_metadata) = run(false);
    assert_eq!(old_proof, new_proof);
    assert_eq!(old_metadata, new_metadata);
    assert_eq!(old.final_state, new.final_state);
    assert_eq!(old.store_history, new.store_history);
    assert!(!new.rounds.is_empty());
    assert!(new.rounds.len() <= old.rounds.len());
    assert!(new.solves.len() < old.solves.len());
    for (i, expected) in old.rounds.iter().enumerate() {
        let actual = new
            .rounds
            .get(i)
            .unwrap_or_else(|| new.rounds.last().unwrap());
        assert_eq!(actual, expected, "registered carrier round {i} differs");
    }
}
