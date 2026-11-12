//! Complete pre-checkpoint fresh solver/ordered check, with logical trace data.
use super::*;
use crate::place_replacements::fixture;
include!("reference.rs");
use super::super::scheduler_contexts as contexts;
type BridgeFacts = (Option<u8>, FieldFresh, Vec<UnpackedFresh>, Option<bool>);
thread_local! { static HITS:RefCell<Vec<usize>>=const {RefCell::new(Vec::new())}; }
type CaptureRow = Vec<Option<Content>>;
struct CaptureInputs {
    function: usize,
    captures: Vec<Option<CaptureRow>>,
}
thread_local! {
    static CAPTURE_INPUTS: RefCell<Option<Vec<CaptureInputs>>> = const { RefCell::new(None) };
}
pub(super) fn hit(index: usize, inputs: &[Option<State>]) {
    HITS.with(|v| v.borrow_mut().push(index));
    CAPTURE_INPUTS.with(|v| {
        if let Some(snapshots) = v.borrow_mut().as_mut() {
            // Deep Content copies never add State/Content Arc references to
            // the actual checkpoint consumed by the following final replay.
            snapshots.push(CaptureInputs {
                function: index,
                captures: inputs
                    .iter()
                    .map(|state| {
                        state.as_ref().map(|state| {
                            state
                                .captures
                                .iter()
                                .map(|root| root.as_deref().cloned())
                                .collect()
                        })
                    })
                    .collect(),
            });
        }
    });
}
pub(super) fn drain_hits() -> Vec<usize> {
    HITS.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

// Complete dynamic payload plus all comparable immutable-plan fields. The
// opaque partial-place Meta has no Eq API; only its presence is compared.
#[derive(PartialEq, Eq)]
struct EntrySnapshot {
    module: usize,
    function: usize,
    graph: usize,
    mode: Mode,
    inputs: Vec<Option<State>>,
    written: Rows,
    excluded: ExcludedEdges,
    reads: Vec<(usize, usize)>,
    sum_marked: bool,
    definitions: Vec<Option<(usize, usize)>>,
    observers: Vec<Option<(LocalId, Prefix)>>,
    partial: bool,
    place_meta: bool,
    tagged_locals: Option<Vec<usize>>,
    address_replacement_tags: bool,
    owned_creation: bool,
}
fn entry_snapshot(entry: &Entry<'_>) -> EntrySnapshot {
    EntrySnapshot {
        module: entry.draft.module as *const TypedModule as usize,
        function: entry.draft.function as *const TypedFunction as usize,
        graph: entry.draft.graph as *const FunctionGraph as usize,
        mode: entry.draft.mode,
        inputs: entry.draft.inputs.clone(),
        written: entry.draft.written.clone(),
        excluded: entry.draft.excluded.clone(),
        reads: entry.reads.clone(),
        sum_marked: entry.sum_marked,
        definitions: entry.plan.definitions.clone(),
        observers: entry.plan.observers.clone(),
        partial: entry.plan.partial,
        place_meta: entry.plan.place_meta.is_some(),
        tagged_locals: entry.plan.tagged_locals.clone(),
        address_replacement_tags: entry.plan.address_replacement_tags,
        owned_creation: entry.plan.owned_creation,
    }
}
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
                        .map(|r| {
                            r.into_iter()
                                .map(|b| (b.result, b.package, b.unpack, b.task_cleanup))
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
            successors: vec![vec![]; f.blocks.len()],
            predecessors: vec![vec![]; f.blocks.len()],
            reachable: vec![true; f.blocks.len()],
            package_views: packages::CaptureViews::default(),
            role_data_uses: vec![],
        })
        .collect()
}
fn run(module: &TypedModule, old: bool, n: usize) -> (Check<Proof>, trace::Trace, Vec<String>) {
    let _mode = trace::Mode::enter(false);
    trace::start();
    let mut b = budget(module, n);
    let result = if old {
        old_check(module, &graphs(module), &mut b)
    } else {
        check(module, &graphs(module), &mut b)
    }
    .map(Proof::from);
    let meta = trace::metadata(&b);
    let t = trace::finish();
    (result, t, meta)
}
fn compare(module: &TypedModule) -> Proof {
    let (a, ta, ma) = run(module, true, 10_000_000);
    let (b, tb, mb) = run(module, false, 10_000_000);
    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a, b);
    assert_eq!(ma, mb);
    assert_eq!(ta.rounds, tb.rounds);
    assert_eq!(ta.final_state, tb.final_state);
    assert_eq!(ta.store_history, tb.store_history);
    b
}
fn chain(count: usize) -> TypedModule {
    let mut module = fixture::module();
    let leaf = module.functions.pop().unwrap();
    for id in 0..count {
        let mut f = leaf.clone();
        f.id = FunctionId(id as u32);
        f.name = format!("checkpoint_chain{id}");
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
fn checkpoint_complete_ordered_rounds_proofs_and_metadata_match_fresh_solver() {
    HITS.with(|v| v.borrow_mut().clear());
    compare(&chain(8));
    assert!(
        HITS.with(|v| v.borrow().contains(&0)),
        "complete chain must actually consume a guarded final checkpoint"
    );
    compare(&fixture::module());
    // Sum transport alone accepts this source; the later independent resource
    // kernel owns its old-description retirement rejection.
    compare(&contexts::old_descriptor_fixture());
    let proof = compare(&contexts::public_callback_input_fixture());
    assert!(!proof.retired[&0].is_empty());
    assert!(!proof.retired[&1].is_empty());
}
#[test]
fn checkpoint_request_preserves_exact_n_minus_one_and_fresh_retry() {
    let module = chain(3);
    let mut lo = 0;
    let mut hi = 10_000_000;
    assert!(run(&module, false, hi).0.is_ok());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if run(&module, false, mid).0.is_ok() {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let n = lo;
    let (a, ta, ma) = run(&module, false, n);
    let (b, tb, mb) = run(&module, true, 10_000_000);
    let expected = b.unwrap();
    assert_eq!(a.unwrap(), expected);
    assert_eq!(ma, mb);
    assert_eq!(ta.rounds, tb.rounds);
    assert_eq!(ta.final_state, tb.final_state);
    assert_eq!(ta.store_history, tb.store_history);
    assert_eq!(
        run(&module, false, n - 1).0.unwrap_err().category,
        zil_base::ResultCategory::ResourceLimit
    );
    let (fresh, trace, metadata) = run(&module, false, n);
    assert_eq!(fresh.unwrap(), expected);
    assert_eq!(metadata, mb);
    assert_eq!(trace.rounds, tb.rounds);
    assert_eq!(trace.final_state, tb.final_state);
    assert_eq!(trace.store_history, tb.store_history);
    let (fresh, trace, metadata) = run(&module, false, n);
    assert_eq!(fresh.unwrap(), expected);
    assert_eq!(metadata, mb);
    assert_eq!(trace.rounds, tb.rounds);
    assert_eq!(trace.final_state, tb.final_state);
    assert_eq!(trace.store_history, tb.store_history);
    eprintln!("checkpoint complete ordered request N={n}, N-1 ResourceLimit, fresh N Pass");
}
#[test]
fn checkpoint_all_mode_bits_guard_metadata_without_restoring_old_flags() {
    let module = fixture::module();
    let flips: [fn(&mut Budget); 10] = [
        |b| b.pod_stores = !b.pod_stores,
        |b| b.store_option_results = !b.store_option_results,
        |b| b.ordinary_error_places = !b.ordinary_error_places,
        |b| b.task_borrowed_results = !b.task_borrowed_results,
        |b| b.stored_enum_addresses = !b.stored_enum_addresses,
        |b| b.sums = !b.sums,
        |b| b.tasks = !b.tasks,
        |b| b.registrations = !b.registrations,
        |b| b.pure_nominal = !b.pure_nominal,
        |b| b.stored_exclusive_shapes = !b.stored_exclusive_shapes,
    ];
    for flip in flips {
        let mut b = budget(&module, 1000);
        let mode = Mode::read(&mut b).unwrap();
        assert!(mode.matches(&mut b).unwrap());
        flip(&mut b);
        assert!(!mode.matches(&mut b).unwrap());
    }
}
fn saved<'m>(
    module: &'m TypedModule,
    graph: &'m [FunctionGraph],
    index: usize,
    n: usize,
) -> Check<Option<Entry<'m>>> {
    saved_with_written(module, graph, index, n).map(|(entry, _)| entry)
}
fn saved_with_written<'m>(
    module: &'m TypedModule,
    graph: &'m [FunctionGraph],
    index: usize,
    n: usize,
) -> Check<(Option<Entry<'m>>, Rows)> {
    let mut b = budget(module, n);
    let actual = actual::Program::reconstruct(module, graph, &mut b)?;
    let mut places = Vec::new();
    let mut whole = BTreeSet::new();
    for f in &module.functions {
        let inventory = Inventory::reconstruct(module, f, &mut b)?;
        if whole_must(f, &inventory, &mut b)? {
            whole.insert(f.id.0 as usize);
        }
        inventory.activation_entries(f, &graph[f.id.0 as usize], &mut b)?;
        places.push(inventory);
    }
    let result = module
        .functions
        .iter()
        .map(|f| content(&f.return_type, None, &mut Vec::new(), true, &mut b))
        .collect::<Check<Vec<_>>>()?;
    let effects = vec![Rows::new(); module.functions.len()];
    let store = RefCell::new(BTreeSet::new());
    let context = Context {
        actual: &actual,
        places: &places,
        summaries: &effects,
        whole_must: &whole,
        written: RefCell::new(Rows::new()),
        store_calls: &store,
        observed: Some(RefCell::new(BTreeSet::new())),
    };
    let (_, entry) = run_retaining(
        module,
        &module.functions[index],
        &graph[index],
        &result,
        false,
        &mut b,
        Some(&context),
        true,
    )?;
    Ok((entry, context.written.into_inner()))
}
fn stamped<'m>(
    module: &'m TypedModule,
    graph: &'m [FunctionGraph],
    index: usize,
    reads: BTreeSet<usize>,
) -> Cache<'m> {
    let entry = saved(module, graph, index, 10_000_000)
        .unwrap()
        .expect("successful original solver must produce incoming states");
    let mut b = budget(module, 10_000_000);
    let mut cache = Cache::new(module.functions.len(), &mut b).unwrap();
    cache.publish(index, Some(entry), &reads, &mut b).unwrap();
    cache.published(index, &mut b).unwrap();
    cache
}
#[test]
fn checkpoint_versions_reject_same_keys_route_missing_and_source_changes() {
    let module = chain(2);
    let graph = graphs(&module);
    let route = Content::Sum(
        BTreeMap::from([(0, vec![Content::Leaf(BTreeSet::from([Source::Fresh]))])]),
        Some((0, vec![1, 0, 0])),
    );
    let without_route = Content::Sum(
        BTreeMap::from([(0, vec![Content::Leaf(BTreeSet::from([Source::Fresh]))])]),
        None,
    );
    let missing = Content::Missing(Box::new(route.clone()));
    let changed_source = Content::Sum(
        BTreeMap::from([(
            0,
            vec![Content::Leaf(BTreeSet::from([Source::Input(
                0,
                vec![0, 1],
            )]))],
        )]),
        Some((0, vec![1, 0, 0])),
    );
    for changed in [without_route, missing, changed_source] {
        let mut cache = stamped(&module, &graph, 0, BTreeSet::from([1]));
        let mut b = budget(&module, 10_000_000);
        let mut summary = route.clone();
        assert!(summary.merge(&changed, &mut b).unwrap());
        let plain = match &summary {
            Content::Missing(value) => value.as_ref(),
            value => value,
        };
        assert!(matches!(plain,Content::Sum(arms,_) if arms.len()==1));
        // A real complete publication changes the epoch independently of
        // row/tag/key counts. No sampled digest can authenticate these facts.
        cache.published(1, &mut b).unwrap();
        assert!(
            cache
                .take(0, &module, &module.functions[0], &graph[0], &mut b)
                .unwrap()
                .is_none()
        );
    }
    let mut cache = stamped(&module, &graph, 0, BTreeSet::from([1]));
    let mut b = budget(&module, 10_000_000);
    assert!(
        cache
            .take(0, &module, &module.functions[0], &graph[0], &mut b)
            .unwrap()
            .is_some()
    );
}
#[test]
fn checkpoint_actual_self_and_mutual_reads_have_guarded_or_fresh_paths() {
    let mut module = chain(2);
    module.functions[1].blocks[0].instructions = module.functions[0].blocks[0].instructions.clone();
    if let InstructionKind::Call { function, .. } =
        &mut module.functions[1].blocks[0].instructions[2].kind
    {
        *function = FunctionId(0);
    }
    compare(&module);
    let graph = graphs(&module);
    let mut b = budget(&module, 10_000_000);
    let mut first = stamped(&module, &graph, 0, BTreeSet::from([1]));
    first.published(1, &mut b).unwrap();
    assert!(
        first
            .take(0, &module, &module.functions[0], &graph[0], &mut b)
            .unwrap()
            .is_none()
    );
    let mut last = stamped(&module, &graph, 1, BTreeSet::from([0]));
    assert!(
        last.take(1, &module, &module.functions[1], &graph[1], &mut b)
            .unwrap()
            .is_some()
    );
    if let InstructionKind::Call { function, .. } =
        &mut module.functions[0].blocks[0].instructions[2].kind
    {
        *function = FunctionId(0);
    }
    compare(&module);
    let graph = graphs(&module);
    assert!(
        saved(&module, &graph, 0, 10_000_000).unwrap().is_none(),
        "actual self read must never publish a checkpoint invalidated by its own publication"
    );
}
#[test]
fn checkpoint_save_prepaid_boundary_and_actual_object_identity_are_exact() {
    let module = chain(2);
    let graph = graphs(&module);
    let mut lo = 0;
    let mut hi = 10_000_000;
    let expected = entry_snapshot(&saved(&module, &graph, 0, hi).unwrap().unwrap());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if saved(&module, &graph, 0, mid).is_ok() {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    assert_eq!(
        saved(&module, &graph, 0, lo - 1).err().unwrap().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert!(entry_snapshot(&saved(&module, &graph, 0, lo).unwrap().unwrap()) == expected);
    assert!(entry_snapshot(&saved(&module, &graph, 0, lo).unwrap().unwrap()) == expected);
    let mut cache = stamped(&module, &graph, 0, BTreeSet::from([1]));
    let copied = module.clone();
    let copied_graph = graphs(&copied);
    let mut b = budget(&module, 10_000_000);
    assert!(
        cache
            .take(0, &copied, &copied.functions[0], &copied_graph[0], &mut b)
            .unwrap()
            .is_none()
    );
    let mut cache = stamped(&module, &graph, 0, BTreeSet::from([1]));
    b.tasks = !b.tasks;
    assert!(
        cache
            .take(0, &module, &module.functions[0], &graph[0], &mut b)
            .unwrap()
            .is_none()
    );
    eprintln!("full checkpoint save original solve+false replay N={lo}; N-1 no Entry; freshN Pass");
}
#[test]
fn checkpoint_saved_incoming_and_solver_only_written_survive_may_cow() {
    let module = fixture::module();
    let graph = graphs(&module);
    let mut entry = saved(&module, &graph, 0, 10_000_000).unwrap().unwrap();
    assert!(
        !entry.draft.written.is_empty(),
        "solver-only nonlocal Commit effect must actually be retained"
    );
    let incoming = entry.draft.inputs[0].as_ref().unwrap().clone();
    let mut b = budget(&module, 10_000_000);
    let original = entry.written(&mut b).unwrap();
    assert!(!original.is_empty());
    assert!(entry.draft.written.is_empty());
    // May's destructive false replay has already completed; this is still the
    // exact pre-block incoming responsibility, not its staged/committed output.
    assert_eq!(incoming.initialized.get(1, &mut b).unwrap(), 2);
    let mut changed = incoming.clone();
    let before = incoming.locals.clone();
    changed.locals[1].cost(&mut b).unwrap();
    Arc::make_mut(&mut changed.locals[1])
        .returned(&mut b)
        .unwrap();
    assert_eq!(incoming.locals, before);
    assert_ne!(changed.locals, incoming.locals);
}
fn resource(module: &TypedModule, fresh: bool) -> (Check<()>, Vec<String>, trace::Trace) {
    // Existing independently frozen full-scan oracle uses the unchanged fresh
    // run path; literal c6ff ordered/run comparisons above cover that split.
    let _mode = trace::Mode::enter(fresh);
    trace::start();
    let mut b = budget(module, 10_000_000);
    let result = resources::check_module(module, &graphs(module), &mut b);
    let meta = trace::metadata(&b);
    (result, meta, trace::finish())
}
fn rejected(module: &TypedModule, text: &str) {
    let (a, ma, ta) = resource(module, true);
    let (b, mb, tb) = resource(module, false);
    assert_eq!(a, b);
    assert_eq!(ma, mb);
    assert_eq!(ta.store_history, tb.store_history);
    let d = b.unwrap_err();
    assert_eq!(d.category, zil_base::ResultCategory::SafeReject);
    assert!(d.message.contains(text), "{d:?}");
}
#[test]
fn checkpoint_complete_resource_retirement_initialization_and_unpaid_guards_remain() {
    let good = chain(4);
    let (old, old_meta, old_trace) = resource(&good, true);
    let (new, new_meta, new_trace) = resource(&good, false);
    old.unwrap();
    new.unwrap();
    assert_eq!(old_meta, new_meta);
    assert_eq!(old_trace.final_state, new_trace.final_state);
    assert_eq!(old_trace.store_history, new_trace.store_history);
    let mut retired = good.clone();
    let mut leaf = contexts::old_descriptor_fixture().functions.remove(0);
    leaf.id = FunctionId(3);
    retired.functions[3] = leaf;
    rejected(&retired, "retirement");
    let mut missing = good.clone();
    missing.functions[0].parameters.pop();
    rejected(&missing, "give requires a live owned position");
    let mut extra = good;
    let f = &mut extra.functions[0];
    let mut local = f.locals[1].clone();
    local.id = LocalId(2);
    f.locals.push(local);
    f.parameters.push(LocalId(2));
    f.blocks[0].instructions.push(fixture::instruction(
        None,
        InstructionKind::DropManaged { local: LocalId(2) },
    ));
    let (old, old_meta, old_trace) = resource(&extra, true);
    let (new, new_meta, new_trace) = resource(&extra, false);
    old.unwrap();
    new.unwrap();
    assert_eq!(old_meta, new_meta);
    assert_eq!(old_trace.final_state, new_trace.final_state);
    assert_eq!(old_trace.store_history, new_trace.store_history);
    extra.functions[0].blocks[0].instructions.pop();
    rejected(&extra, "outstanding resource responsibility");
}

// A changed dependency invalidates inference through Ordered before final cache
// lookup: begin drops the stale checkpoint and a fresh May solve replaces it.
// This is intentionally distinct from the direct epoch-mismatch control above.
#[test]
fn checkpoint_real_public_helper_and_callback_publications_force_fresh_solves() {
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
    HITS.with(|v| v.borrow_mut().clear());
    let (old, before, old_meta) = run(&module, true, 10_000_000);
    let (new, after, new_meta) = run(&module, false, 10_000_000);
    let proof = new.unwrap();
    assert_eq!(old.unwrap(), proof);
    assert_eq!(old_meta, new_meta);
    assert_eq!(before.rounds, after.rounds);
    assert_eq!(before.final_state, after.final_state);
    assert_eq!(before.store_history, after.store_history);
    assert!(after.reads[&0].contains(&1));
    assert!(after.reads[&0].contains(&2));
    assert!(after.reads[&1].contains(&2));
    assert!(after.rounds.len() >= 3);
    assert_ne!(after.rounds[0].effects[1], after.rounds[1].effects[1]);
    assert!(
        after.solves.iter().filter(|&&index| index == 0).count() >= 3,
        "late public helper publication must run caller again after its first retained solve"
    );
    let state = after.final_state.unwrap();
    assert!(
        state.effects[0]
            .keys()
            .any(|t| matches!(t.base, actual::Base::Input(1)))
    );
    assert!(
        state.effects[0]
            .keys()
            .any(|t| matches!(t.base, actual::Base::InputCapture(0, 0)))
    );
    assert!(!proof.retired[&2].is_empty());
    assert!(
        HITS.with(|v| v.borrow().contains(&0)),
        "only the refreshed caller checkpoint may supply final true replay"
    );
}

#[test]
fn checkpoint_solver_written_replay_is_idempotent_and_its_owned_snapshot_is_independent() {
    let module = fixture::module();
    let graph = graphs(&module);
    let (entry, mut after_may) = saved_with_written(&module, &graph, 0, 10_000_000).unwrap();
    let entry = entry.unwrap();
    assert!(!entry.draft.written.is_empty());
    // At a stable solve the last execution of each final input has already
    // joined its writes. Replaying those same inputs cannot add facts. This
    // fixture therefore must not pretend solver-only and post-May rows differ.
    assert_eq!(entry.draft.written, after_may);
    let target = after_may.keys().next().unwrap().clone();
    let leaf = Content::Leaf(BTreeSet::from([
        Source::Fresh,
        Source::Input(0, vec![1, 0]),
    ]));
    let missing = Content::Missing(Box::new(leaf.clone()));
    let product = Content::Product(vec![Content::Copy, missing.clone()]);
    let sum = Content::Sum(
        BTreeMap::from([(0, vec![product.clone()])]),
        Some((0, vec![1])),
    );
    let mut b = budget(&module, 10_000_000);
    for payload in [leaf, product, sum] {
        let mut rows = Rows::new();
        assert!(merge_row(&mut rows, &target, &payload, &mut b).unwrap());
        let snapshot = rows.clone();
        assert!(!merge_row(&mut rows, &target, &payload, &mut b).unwrap());
        assert_eq!(rows, snapshot);
    }
    let mut again = missing.clone();
    assert!(!again.merge(&missing, &mut b).unwrap());
    assert_eq!(again, missing);
    // Internal snapshot isolation control, not a claim that legal stable
    // replay produces a different write set. The original Context may go away
    // or change; the retained solver history must remain complete and owned.
    let saved = entry.draft.written.clone();
    let changed = Content::Leaf(BTreeSet::from([Source::Input(7, vec![3, 2, 1])]));
    assert!(merge_row(&mut after_may, &target, &changed, &mut b).unwrap());
    assert_ne!(after_may, saved);
    assert_eq!(entry.draft.written, saved);
}

fn checkpoint_element_caller() -> TypedModule {
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
fn checkpoint_element_hit_keeps_nonempty_historical_store_calls_and_targets() {
    let module = checkpoint_element_caller();
    HITS.with(|v| v.borrow_mut().clear());
    let (old, before, old_meta) = run(&module, true, 10_000_000);
    let (new, after, new_meta) = run(&module, false, 10_000_000);
    let proof = new.unwrap();
    assert_eq!(old.unwrap(), proof);
    assert_eq!(old_meta, new_meta);
    assert_eq!(before.rounds, after.rounds);
    assert_eq!(before.final_state, after.final_state);
    assert_eq!(before.store_history, after.store_history);
    assert!(after.store_history.iter().any(|calls| calls.contains(&1)));
    assert!(after.final_state.unwrap().store.contains(&1));
    assert!(!proof.targets[&1].is_empty());
    assert!(
        HITS.with(|v| v.borrow().contains(&0)),
        "actual Element transfer must retain store history through a real caller checkpoint hit"
    );
}

#[test]
fn checkpoint_publish_and_guard_charge_failures_never_return_a_partial_success() {
    const ROOM: usize = 1_000_000;
    let module = chain(2);
    let graph = graphs(&module);
    let reads = BTreeSet::from([1]);
    // Both initial table allocation and replacement of an occupied slot have
    // distinct charged paths. Each failed prefix must preserve the old table.
    for occupied in [false, true] {
        let prepare = || {
            if occupied {
                stamped(&module, &graph, 0, reads.clone())
            } else {
                Cache::new(module.functions.len(), &mut budget(&module, ROOM)).unwrap()
            }
        };
        let entry = || saved(&module, &graph, 0, ROOM).unwrap();
        let mut cache = prepare();
        let mut b = budget(&module, ROOM);
        cache.publish(0, entry(), &reads, &mut b).unwrap();
        let n = ROOM - b.remaining;
        assert!(n > 2);
        let expected = entry_snapshot(cache.entries.as_ref().unwrap()[0].as_ref().unwrap());
        for limit in 0..n {
            let mut cache = prepare();
            let previous_snapshot = cache
                .entries
                .as_ref()
                .map(|entries| entry_snapshot(entries[0].as_ref().unwrap()));
            let previous = cache.entries.as_ref().map(|entries| {
                let old = entries[0].as_ref().unwrap();
                (old.reads.clone(), old.draft.written.clone())
            });
            let versions = cache.versions.clone();
            let error = cache
                .publish(0, entry(), &reads, &mut budget(&module, limit))
                .unwrap_err();
            assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
            assert_eq!(cache.versions, versions);
            assert!(
                cache
                    .entries
                    .as_ref()
                    .map(|entries| { entry_snapshot(entries[0].as_ref().unwrap()) })
                    == previous_snapshot
            );
            match previous {
                None => assert!(cache.entries.is_none()),
                Some((reads, written)) => {
                    let old = cache.entries.as_ref().unwrap()[0].as_ref().unwrap();
                    assert_eq!(old.reads, reads);
                    assert_eq!(old.draft.written, written);
                }
            }
        }
        let mut fresh = prepare();
        fresh
            .publish(0, entry(), &reads, &mut budget(&module, n))
            .unwrap();
        assert!(fresh.entries.as_ref().unwrap()[0].is_some());
        assert!(entry_snapshot(fresh.entries.as_ref().unwrap()[0].as_ref().unwrap()) == expected);
        eprintln!(
            "checkpoint publish occupied={occupied} N={n}; every lower charged prefix rejects without publication"
        );
    }
    let mut full = stamped(&module, &graph, 0, reads.clone());
    let mut b = budget(&module, ROOM);
    let expected = entry_snapshot(
        &full
            .take(0, &module, &module.functions[0], &graph[0], &mut b)
            .unwrap()
            .unwrap(),
    );
    let n = ROOM - b.remaining;
    for limit in 0..n {
        let mut cache = stamped(&module, &graph, 0, reads.clone());
        HITS.with(|v| v.borrow_mut().clear());
        let error = cache
            .take(
                0,
                &module,
                &module.functions[0],
                &graph[0],
                &mut budget(&module, limit),
            )
            .err()
            .unwrap();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        assert!(HITS.with(|v| v.borrow().is_empty()));
        // take consumes its candidate after the first two prepayments. A
        // subsequent guard failure aborts the request, never returns that Entry.
        assert_eq!(cache.entries.as_ref().unwrap()[0].is_some(), limit < 3);
    }
    for _ in 0..2 {
        let mut fresh = stamped(&module, &graph, 0, reads.clone());
        let entry = fresh
            .take(
                0,
                &module,
                &module.functions[0],
                &graph[0],
                &mut budget(&module, n),
            )
            .unwrap();
        assert!(entry.is_some());
        assert!(entry_snapshot(entry.as_ref().unwrap()) == expected);
    }
    eprintln!("checkpoint successful guard N={n}; N-1 no hit, fresh N hit");
}

#[test]
fn checkpoint_input_retention_prepays_every_failure_before_adding_strong_references() {
    const ROOM: usize = 1_000_000;
    let module = fixture::module();
    let graph = graphs(&module);
    let entry = saved(&module, &graph, 0, ROOM).unwrap().unwrap();
    let inputs = &entry.draft.inputs;
    let counts = || {
        inputs
            .iter()
            .flatten()
            .flat_map(|state| state.locals.iter().map(Arc::strong_count))
            .collect::<Vec<_>>()
    };
    let before = counts();
    let mut b = budget(&module, ROOM);
    let retained = retain_inputs(inputs, &mut b).unwrap();
    let n = ROOM - b.remaining;
    drop(retained);
    assert_eq!(counts(), before);
    for limit in 0..n {
        let error = retain_inputs(inputs, &mut budget(&module, limit))
            .err()
            .unwrap();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        assert_eq!(counts(), before);
    }
    for _ in 0..2 {
        let retained = retain_inputs(inputs, &mut budget(&module, n)).unwrap();
        assert_eq!(retained.len(), inputs.len());
        assert!(retained == *inputs);
        drop(retained);
        assert_eq!(counts(), before);
    }
    eprintln!(
        "checkpoint input retention N={n}; all lower charged prefixes preserve Arc counts, fresh N succeeds"
    );
    // Solver-written preservation uses a separate owned deep copy; it cannot
    // publish a partially cloned Rows or alter the actual saved solver rows.
    let rows = &entry.draft.written;
    assert!(!rows.is_empty());
    let snapshot = rows.clone();
    let mut b = budget(&module, ROOM);
    assert_eq!(copy_rows(rows, &mut b).unwrap(), snapshot);
    let n = ROOM - b.remaining;
    for limit in 0..n {
        let error = copy_rows(rows, &mut budget(&module, limit)).unwrap_err();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        assert_eq!(*rows, snapshot);
    }
    assert_eq!(copy_rows(rows, &mut budget(&module, n)).unwrap(), snapshot);
    assert_eq!(copy_rows(rows, &mut budget(&module, n)).unwrap(), snapshot);
    eprintln!(
        "checkpoint solver-written copy N={n}; every failed prefix returns no Rows, fresh N succeeds"
    );
}

fn direct_capture_fixture() -> (TypedModule, Vec<FunctionGraph>) {
    let mut module = fixture::module();
    let payload = Type::OwnedStore {
        element: Box::new(Type::I32),
    };
    let captured = Type::Enum {
        name: "CheckpointCapture".into(),
        variants: vec![
            ("Empty".into(), vec![]),
            (
                "Full".into(),
                vec![Type::Managed(Box::new(payload.clone()))],
            ),
        ],
    };
    let dependency = Type::Enum {
        name: "CheckpointDependency".into(),
        variants: vec![("A".into(), vec![]), ("B".into(), vec![])],
    };
    let mut caller = module.functions.pop().unwrap();
    let mut helper = caller.clone();
    caller.name = "captured_branch".into();
    caller.environment = Some(ClosureId(0));
    caller.parameters = vec![LocalId(0)];
    caller.locals.truncate(1);
    caller.locals[0].name = "condition".into();
    caller.locals[0].ty = Type::Bool;
    caller.values = vec![dependency.clone(), captured.clone(), Type::Bool];
    let mut entry = caller.blocks[0].clone();
    entry.instructions = vec![
        fixture::instruction(
            Some(ValueId(0)),
            InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![],
            },
        ),
        fixture::instruction(Some(ValueId(1)), InstructionKind::TakeCapture { index: 0 }),
        fixture::instruction(Some(ValueId(2)), InstructionKind::Load(LocalId(0))),
    ];
    entry.terminator = Terminator::Branch {
        condition: ValueId(2),
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    let mut left = caller.blocks[0].clone();
    left.id = BlockId(1);
    left.instructions = vec![fixture::instruction(
        None,
        InstructionKind::WriteCapture {
            index: 0,
            value: ValueId(1),
        },
    )];
    left.terminator = Terminator::Return(None);
    let mut right = left.clone();
    right.id = BlockId(2);
    caller.blocks = vec![entry, left, right];
    helper.id = FunctionId(1);
    helper.name = "later_enum_publication".into();
    helper.parameters.clear();
    helper.locals.clear();
    helper.return_type = dependency.clone();
    helper.values = vec![dependency];
    helper.blocks[0].instructions = vec![fixture::instruction(
        Some(ValueId(0)),
        InstructionKind::MakeVariant {
            variant: 0,
            fields: vec![],
        },
    )];
    helper.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    module.functions = vec![caller, helper];
    module.closures = vec![ClosureDefinition {
        storage_life: None,
        id: ClosureId(0),
        function: FunctionId(0),
        captures: vec![captured],
        parameters: vec![Type::Bool],
        result: Type::Unit,
        kind: CallableKind::FnMut,
        lending_captures: vec![],
    }];
    module.resources = vec![ResourceDefinition {
        name: dynamic_store::definition_name(&Type::I32),
        payload,
        teardown: None,
    }];
    let mut graph = graphs(&module);
    graph[0].successors = vec![vec![1, 2], vec![], vec![]];
    graph[0].predecessors = vec![vec![], vec![0], vec![0]];
    (module, graph)
}

#[test]
fn checkpoint_direct_sum_capture_hit_replays_every_branch_and_rejects_missing_refill() {
    for refill in [true, false] {
        let (mut module, graph) = direct_capture_fixture();
        if !refill {
            // Keep the actual call, types, CFG and all incoming states. Only
            // one reachable FnMut return loses its required capture refill.
            module.functions[0].blocks[2].instructions.clear();
        }
        let run = |old| {
            let _mode = trace::Mode::enter(false);
            trace::start();
            drain_hits();
            CAPTURE_INPUTS.with(|v| *v.borrow_mut() = Some(Vec::new()));
            let mut b = budget(&module, 10_000_000);
            let limits = b.limits.clone();
            for (index, f) in module.functions.iter().enumerate() {
                b.current_function = Some(index);
                for ty in std::iter::once(&f.return_type)
                    .chain(f.locals.iter().map(|local| &local.ty))
                    .chain(&f.values)
                {
                    check_type_shape(ty, &module.target, &limits, &mut b).unwrap();
                    dynamic_store::register_actual_carrier_root(&module, ty, &mut b).unwrap();
                }
            }
            b.current_function = None;
            for ty in module.closures.iter().flat_map(|closure| {
                closure
                    .captures
                    .iter()
                    .chain(&closure.parameters)
                    .chain(std::iter::once(&closure.result))
            }) {
                check_type_shape(ty, &module.target, &limits, &mut b).unwrap();
                dynamic_store::register_actual_carrier_root(&module, ty, &mut b).unwrap();
            }
            // The real closure preflight must establish the actual capture
            // qualification; no test writes a capture authority or cache bit.
            closures::check_definitions(&module, &limits, &mut b).unwrap();
            assert!(b.direct_sum_capture_closures.contains(&0));
            let result = if old {
                old_check(&module, &graph, &mut b)
            } else {
                check(&module, &graph, &mut b)
            }
            .map(Proof::from);
            (
                result,
                trace::metadata(&b),
                trace::finish(),
                drain_hits(),
                CAPTURE_INPUTS.with(|v| v.borrow_mut().take().unwrap()),
            )
        };
        let (old, old_meta, before, old_hits, old_inputs) = run(true);
        let (new, new_meta, after, new_hits, new_inputs) = run(false);
        assert!(old_hits.is_empty());
        assert!(old_inputs.is_empty());
        assert!(new_hits.contains(&0));
        assert_eq!(old_meta, new_meta);
        assert_eq!(before.rounds, after.rounds);
        assert_eq!(before.final_state, after.final_state);
        assert_eq!(before.store_history, after.store_history);
        assert_eq!(after.solves, vec![0, 1, 0]);
        assert!(after.reads[&0].contains(&1));
        let hit = new_inputs.iter().find(|hit| hit.function == 0).unwrap();
        let initial = Content::Sum(
            BTreeMap::from([
                (0, vec![]),
                (
                    1,
                    vec![Content::Leaf(BTreeSet::from([Source::Captured(
                        0,
                        vec![1, 1, 0],
                    )]))],
                ),
            ]),
            None,
        );
        let taken = Content::Sum(BTreeMap::new(), None);
        assert_eq!(
            hit.captures,
            vec![
                Some(vec![Some(initial)]),
                Some(vec![Some(taken.clone())]),
                Some(vec![Some(taken)]),
            ],
            "a real hit must retain all three inputs and their exact capture payloads"
        );
        if refill {
            let proof = new.unwrap();
            assert_eq!(old.unwrap(), proof);
            assert_eq!(proof.rows[0].len(), 3);
            assert!(after.final_state.is_some());
        } else {
            let error = new.unwrap_err();
            assert_eq!(old.unwrap_err(), error);
            assert_eq!(error.category, zil_base::ResultCategory::SafeReject);
            assert_eq!(error.code, "E-VERIFY-SUM");
            assert_eq!(
                error.message,
                "Fn/FnMut return leaves an unfilled direct Sum capture"
            );
        }
    }
}
