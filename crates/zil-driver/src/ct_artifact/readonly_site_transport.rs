//! Same-owner scalar transport. These request-local rows describe the successful
//! bind walk, not stored provenance or permission to execute a source body.
use super::*;
use zil_ct::CtExecutionBudget;

#[derive(Clone, Copy)]
struct Row {
    item: usize,
    start: usize,
    count: usize,
    work: u64,
}
pub(super) struct Transport {
    module: usize,
    depth: usize,
    rows: Vec<Row>,
    valid: bool,
    active: Option<std::ops::Range<usize>>,
    live: usize,
}
impl Transport {
    pub(super) fn capture(
        &mut self,
        item: &s::Item,
        start: usize,
        count: usize,
        work: Option<u64>,
    ) {
        self.valid &= work.is_some();
        self.rows.push(Row {
            item: item as *const s::Item as usize,
            start,
            count,
            work: work.unwrap_or(0),
        });
    }
}
impl BoundSites {
    pub(super) fn logical_len(&self) -> usize {
        self.readonly
            .as_ref()
            .filter(|t| t.active.is_some())
            .map_or(self.nodes.len(), |t| t.live)
    }
    pub(super) fn get(&self, address: (bool, usize)) -> Option<SourceSite> {
        let site = *self.nodes.get(&address)?;
        if let Some(active) = self.readonly.as_ref().and_then(|t| t.active.as_ref())
            && !active.contains(&(site.ordinal as usize))
        {
            return None;
        }
        Some(site)
    }
    pub(super) fn remove(&mut self, address: (bool, usize)) {
        if let Some(transport) = self.readonly.as_mut().filter(|t| t.active.is_some()) {
            if let std::collections::btree_map::Entry::Occupied(entry) = self.nodes.entry(address)
                && transport
                    .active
                    .as_ref()
                    .expect("active item")
                    .contains(&(entry.get().ordinal as usize))
            {
                entry.remove();
                transport.live -= 1;
            }
        } else {
            self.nodes.remove(&address);
        }
    }
}

// A probe executes only the transport ledger, against its actual live-byte
// entry state. Failure leaves the real ledger untouched for the ordinary walk.
fn probe(budget: &CtExecutionBudget) -> Option<CtExecutionBudget> {
    let mut probe = CtExecutionBudget::new(budget.remaining());
    probe
        .reserve_temporary_bytes(budget.live_temporary_bytes())
        .ok()?;
    Some(probe)
}
fn replayable(budget: &CtExecutionBudget, probe: &CtExecutionBudget) -> bool {
    budget
        .usage()
        .analysis_work
        .checked_add(probe.usage().analysis_work)
        .is_some()
}
fn replay(budget: &mut CtExecutionBudget, probe: &CtExecutionBudget) -> CompileResult<()> {
    let peak = probe.usage().peak_temporary_bytes;
    budget.reserve_analysis_work(probe.usage().analysis_work)?;
    budget.reserve_temporary_bytes(peak - budget.live_temporary_bytes())?;
    budget.release_temporary_bytes(peak - probe.live_temporary_bytes());
    Ok(())
}
fn row_bytes(row: Row) -> u64 {
    row.count as u64 * std::mem::size_of::<Option<SourceSite>>() as u64
}

impl Materializer<'_, '_> {
    pub(in super::super) fn bind_readonly_source_items(
        &mut self,
        module: &s::Module,
    ) -> CompileResult<()> {
        // Optional bookkeeping is bounded independently of input size. No
        // changed formal fee or allocation failure can reject an ordinary input.
        let size = module.items.len().checked_mul(std::mem::size_of::<Row>());
        let available = self
            .environment
            .budget
            .remaining()
            .max_temporary_bytes
            .saturating_sub(self.environment.budget.live_temporary_bytes());
        let mut rows = Vec::new();
        let capture = self.bound_source_sites.nodes.is_empty()
            && self.bound_source_sites.bytes == 0
            && self.bound_source_sites.readonly.is_none()
            && size.is_some_and(|n| n <= 65536 && n as u64 <= available)
            && rows.try_reserve_exact(module.items.len()).is_ok();
        let mut transport = capture.then_some(Transport {
            module: module as *const s::Module as usize,
            depth: self.max_depth,
            rows,
            valid: true,
            active: None,
            live: 0,
        });
        if let Some(current) = transport.as_mut()
            && self.bind_observed_readonly(module, current)?
        {
            self.bound_source_sites.readonly = transport;
            return Ok(());
        }
        self.bind_source_items_inner(
            module,
            &module.items,
            SourceTreeId::Original,
            transport.as_mut(),
        )?;
        self.bound_source_sites.readonly = transport.filter(|t| t.valid);
        Ok(())
    }

    // The immutable original is also the receiving tree. Observe its actual
    // complete shape once, then replay the original count/collect/bind ledger.
    // This is private to the non-rewriting executor, not a clone bridge or a
    // persisted SourceSite proof. A failed probe leaves the ordinary path whole.
    fn bind_observed_readonly(
        &mut self,
        module: &s::Module,
        transport: &mut Transport,
    ) -> CompileResult<bool> {
        type Observed = ((bool, usize), SourceSite);
        let planned = (|| {
            let mut meter = probe(&self.environment.budget)?;
            let available = meter
                .remaining()
                .max_temporary_bytes
                .saturating_sub(meter.live_temporary_bytes());
            let maximum = (available as usize).min(1_048_576) / std::mem::size_of::<Observed>();
            let mut nodes = Vec::<Observed>::new();
            let mut rows = Vec::new();
            rows.try_reserve_exact(module.items.len()).ok()?;
            let mut work = 0u64;
            for item in &module.items {
                let start = nodes.len();
                let before = work;
                compiler::source_sites::observe_item(
                    item,
                    module.id,
                    SourceTreeId::Original,
                    self.max_depth,
                    |event| {
                        match event {
                            SiteEvent::Work(amount) => {
                                meter.reserve_analysis_work(amount as u64)?;
                                work = work.checked_add(amount as u64).ok_or_else(|| {
                                    category("source observation work overflow", item.span)
                                })?;
                            }
                            SiteEvent::Node(node) => {
                                if nodes.len() >= maximum {
                                    return Err(category(
                                        "optional source observation bound",
                                        item.span,
                                    ));
                                }
                                if nodes.len() == nodes.capacity() {
                                    let extra = maximum.saturating_sub(nodes.len()).min(256);
                                    nodes.try_reserve_exact(extra).map_err(|_| {
                                        category(
                                            "optional source observation allocation",
                                            item.span,
                                        )
                                    })?;
                                }
                                let ordinal = u32::try_from(nodes.len()).map_err(|_| {
                                    category("source observation ordinal overflow", item.span)
                                })?;
                                nodes.push((
                                    key(node.node),
                                    SourceSite {
                                        ordinal,
                                        ..node.site
                                    },
                                ));
                            }
                        }
                        Ok(())
                    },
                )
                .ok()?;
                rows.push(Row {
                    item: item as *const s::Item as usize,
                    start,
                    count: nodes.len() - start,
                    work: work - before,
                });
            }
            let canonical_bytes =
                nodes.len() as u64 * std::mem::size_of::<(bool, SourceSite)>() as u64;
            meter.reserve_temporary_bytes(canonical_bytes).ok()?;
            // The old collection and binding walks visit exactly this same
            // immutable ordered tree. All operations must fit before grouping
            // positive fees; otherwise the original first failing operation runs.
            meter.reserve_analysis_work(work).ok()?;
            for row in &rows {
                meter.reserve_analysis_work(row.work).ok()?;
                for index in row.start..row.start + row.count {
                    meter
                        .reserve_analysis_work(1 + index.checked_ilog2().unwrap_or(0) as u64)
                        .ok()?;
                    meter.reserve_temporary_bytes(96).ok()?;
                }
            }
            meter.release_temporary_bytes(canonical_bytes);
            if !replayable(&self.environment.budget, &meter) {
                return None;
            }
            // Construct the same current address/site associations once. A
            // repeated address cannot lend an ordinal to another current node.
            let count = nodes.len();
            let nodes: BTreeMap<_, _> = nodes.into_iter().collect();
            if nodes.len() != count {
                return None;
            }
            Some((meter, nodes, rows, count))
        })();
        let Some((meter, nodes, rows, count)) = planned else {
            return Ok(false);
        };
        replay(&mut self.environment.budget, &meter)?;
        self.bound_source_sites.nodes = nodes;
        self.bound_source_sites.bytes = count as u64 * 96;
        transport.rows = rows;
        Ok(true)
    }

    pub(in super::super) fn take_readonly_source_sites(
        &mut self,
        module: &s::Module,
    ) -> CompileResult<bool> {
        let planned = (|| {
            let transport = self.bound_source_sites.readonly.as_ref()?;
            if transport.module != module as *const s::Module as usize
                || transport.depth != self.max_depth
                || transport.rows.len() != module.items.len()
                || transport.active.is_some()
            {
                return None;
            }
            let mut probe = probe(&self.environment.budget)?;
            let mut live = self.bound_source_sites.nodes.len();
            for row in &transport.rows {
                probe.reserve_analysis_work(row.work).ok()?;
                probe.reserve_temporary_bytes(row_bytes(*row)).ok()?;
                probe.reserve_analysis_work(row.work).ok()?;
                for _ in 0..row.count {
                    probe
                        .reserve_analysis_work(1 + live.checked_ilog2().unwrap_or(0) as u64)
                        .ok()?;
                    live = live.checked_sub(1)?;
                    probe.release_temporary_bytes(96);
                }
            }
            (live == 0 && replayable(&self.environment.budget, &probe)).then_some(probe)
        })();
        let Some(probe) = planned else {
            self.bound_source_sites.readonly = None;
            return Ok(false);
        };
        replay(&mut self.environment.budget, &probe)?;
        self.bound_source_sites.bytes = 0;
        self.bound_source_sites
            .readonly
            .as_mut()
            .expect("current bind")
            .active = Some(0..0);
        Ok(true)
    }

    pub(in super::super) fn restore_readonly_source_sites(
        &mut self,
        item: &s::Item,
        ordinal: usize,
    ) -> CompileResult<()> {
        let transport = self
            .bound_source_sites
            .readonly
            .as_ref()
            .expect("current immutable transport");
        let row = transport.rows[ordinal];
        if row.item != item as *const s::Item as usize || transport.depth != self.max_depth {
            return Err(category("moved source shape differs", item.span));
        }
        let planned = (|| {
            let mut probe = probe(&self.environment.budget)?;
            probe.reserve_analysis_work(row.work).ok()?;
            for i in 0..row.count {
                let live = transport.live.checked_add(i)?;
                probe
                    .reserve_analysis_work(1 + live.checked_ilog2().unwrap_or(0) as u64)
                    .ok()?;
                probe.reserve_temporary_bytes(96).ok()?;
            }
            probe.release_temporary_bytes(row_bytes(row));
            replayable(&self.environment.budget, &probe).then_some(probe)
        })();
        if let Some(probe) = planned {
            replay(&mut self.environment.budget, &probe)?;
            let bound = &mut self.bound_source_sites;
            bound.bytes += row.count as u64 * 96;
            let transport = bound
                .readonly
                .as_mut()
                .expect("current immutable transport");
            // Earlier entries normally disappear during scalar cleanup. Keep
            // any leftovers visible and counted exactly as in the old map;
            // only not-yet-restored items are hidden by this prefix.
            transport.active = Some(0..row.start + row.count);
            transport.live += row.count;
            return Ok(());
        }
        // The successful bind proves depth, node order and unchanged addresses.
        // A tight allowance still executes the original interleaved work and
        // byte reservations, so its first failure and partial ledger are exact.
        let bound = &mut self.bound_source_sites;
        let budget = &mut self.environment.budget;
        bound
            .readonly
            .as_mut()
            .expect("current immutable transport")
            .active = Some(0..row.start);
        compiler::source_sites::observe_item(
            item,
            ModuleId(0),
            SourceTreeId::Original,
            self.max_depth,
            |event| {
                match event {
                    SiteEvent::Work(work) => budget.reserve_analysis_work(work as u64)?,
                    SiteEvent::Node(node) => {
                        budget.reserve_analysis_work(
                            1 + bound.logical_len().checked_ilog2().unwrap_or(0) as u64,
                        )?;
                        budget.reserve_temporary_bytes(96)?;
                        bound.bytes += 96;
                        let transport = bound
                            .readonly
                            .as_mut()
                            .expect("current immutable transport");
                        let site = bound
                            .nodes
                            .get(&key(node.node))
                            .expect("exact original bind");
                        transport.active.as_mut().expect("restoring item").end =
                            site.ordinal as usize + 1;
                        transport.live += 1;
                    }
                }
                Ok(())
            },
        )?;
        budget.release_temporary_bytes(row_bytes(row));
        Ok(())
    }
}
