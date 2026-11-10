//! Test-only logical snapshots. Content owns its descendants: snapshots do not
//! clone State Arcs or change the solver's COW/alias observations.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub(super) result: Vec<Content>,
    pub(super) effects: Vec<Rows>,
    pub(super) store: BTreeSet<usize>,
    pub(super) metadata: Vec<String>,
}
#[derive(Default, Debug)]
pub(super) struct Trace {
    pub(super) rounds: Vec<Snapshot>,
    pub(super) final_state: Option<Snapshot>,
    pub(super) store_history: Vec<BTreeSet<usize>>,
    pub(super) solves: Vec<usize>,
    pub(super) reads: BTreeMap<usize, BTreeSet<usize>>,
}
thread_local! {
    static TRACE:RefCell<Option<Trace>>=const {RefCell::new(None)};
    static FULL:std::cell::Cell<bool>=const {std::cell::Cell::new(false)};
}
pub(super) struct Mode(bool);
impl Mode {
    pub(super) fn enter(full: bool) -> Self {
        Self(FULL.with(|v| v.replace(full)))
    }
}
impl Drop for Mode {
    fn drop(&mut self) {
        FULL.with(|v| v.set(self.0));
    }
}
pub(super) fn full() -> bool {
    FULL.with(|v| v.get())
}
pub(super) fn start() {
    TRACE.with(|v| *v.borrow_mut() = Some(Trace::default()));
}
pub(super) fn finish() -> Trace {
    TRACE.with(|v| v.borrow_mut().take().expect("trace was started"))
}
pub(super) fn metadata(b: &Budget) -> Vec<String> {
    vec![
        format!(
            "{:?}",
            [
                b.static_utf8,
                b.pod_stores,
                b.inline_store_addresses,
                b.inline_store_package_addresses,
                b.sums,
                b.tasks,
                b.task_origin_roots,
                b.borrowed_task_ports,
                b.registrations,
                b.async_registrations,
                b.protocol_contracts,
                b.protocol_checked,
                b.replacements,
                b.borrowed_function_pointers,
                b.invocation_callbacks,
                b.owning_address_captures,
                b.head_descriptions,
                b.stored_enum_addresses,
                b.task_borrowed_results,
                b.store_option_results,
                b.qualified_package_returns,
                b.byte_queries,
                b.shared_byte_borrow,
                b.ordinary_error_places,
                b.byte_query_presence_checked,
                b.stored_borrows,
                b.stored_package_instance,
                b.stored_slot_updates,
                b.stored_exclusive_shapes,
                b.stored_constructor_sites,
                b.stored_exclusive_loans,
                b.generic_mutex,
                b.pure_nominal,
                b.stored_context
            ]
        ),
        format!(
            "{:?}",
            (
                &b.inline_store_roots,
                &b.array_snapshot_roots,
                &b.array_outer_functions,
                &b.sum_functions,
                &b.direct_sum_capture_closures,
                b.function_count,
                b.current_function,
                &b.private_slot_functions,
                &b.scoped_functions,
                &b.shared_thread_functions
            )
        ),
        format!(
            "{:?}",
            (
                &b.task_workers,
                &b.suspend_functions,
                &b.task_exports,
                &b.task_ownership,
                &b.guard_return_sources,
                &b.byte_query_functions,
                &b.package_return_calls,
                &b.store_option_sources,
                &b.task_stored_sources
            )
        ),
        format!(
            "pod-carriers={:?}",
            b.pod_carriers.as_ref().map(|cache| cache.snapshot())
        ),
        // Heads are immutable input to this solver and expose no Debug API.
        format!(
            "heads={} memo={:?}",
            b.owned_byte_heads.is_some(),
            b.addressed_copy_sums
        ),
    ]
}
fn snapshot(
    result: &[Content],
    effects: &[Rows],
    store: &RefCell<BTreeSet<usize>>,
    b: &Budget,
) -> Snapshot {
    Snapshot {
        result: result.to_vec(),
        effects: effects.to_vec(),
        store: store.borrow().clone(),
        metadata: metadata(b),
    }
}
pub(super) fn round(
    result: &[Content],
    effects: &[Rows],
    store: &RefCell<BTreeSet<usize>>,
    b: &Budget,
) {
    TRACE.with(|v| {
        if let Some(trace) = v.borrow_mut().as_mut() {
            trace.rounds.push(snapshot(result, effects, store, b));
        }
    });
}
pub(super) fn final_state(
    result: &[Content],
    effects: &[Rows],
    store: &RefCell<BTreeSet<usize>>,
    b: &Budget,
) {
    TRACE.with(|v| {
        if let Some(trace) = v.borrow_mut().as_mut() {
            trace.final_state = Some(snapshot(result, effects, store, b));
        }
    });
}
pub(super) fn store(store: &BTreeSet<usize>) {
    TRACE.with(|v| {
        if let Some(trace) = v.borrow_mut().as_mut() {
            if trace.store_history.last() != Some(store) {
                trace.store_history.push(store.clone());
            }
        }
    });
}
pub(super) fn solve(index: usize) {
    TRACE.with(|v| {
        if let Some(trace) = v.borrow_mut().as_mut() {
            trace.solves.push(index);
        }
    });
}
pub(super) fn observed(caller: usize, reads: &BTreeSet<usize>) {
    TRACE.with(|v| {
        if let Some(trace) = v.borrow_mut().as_mut() {
            trace.reads.entry(caller).or_default().extend(reads);
        }
    });
}
