//! Exact complete-row interning. No raw-oracle extraction or transition calls.
//! The dense and local variants differ only in the existing coordinate codec.
use crate::{
    bdd::Bdd,
    budget::{MAX_RELATIONS, MAX_ROWS, R, Work},
    encoding::{Encoding, Representation},
    extract,
    schema::{QueryStatus, Relations},
    symbolic::{self, Event, Solution},
    telemetry::Telemetry,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Instant,
};
use zil_ir::{InstructionKind, PassingMode, TypedFunction, TypedModule};

type RowId = usize;

#[derive(Default, Debug)]
pub struct Stats {
    pub unique_rows: usize,
    pub unique_row_words: usize,
    pub relation_rows: usize,
    pub relation_row_words: usize,
    pub indexed_relations: usize,
    pub intern_lookups: usize,
    pub intern_hits: usize,
    pub hash_word_visits: usize,
    pub probe_visits: usize,
    pub equality_word_visits: usize,
    pub rehashes: usize,
    pub table_slots: usize,
    pub peak_queue: usize,
    pub expand_ns: u128,
}
impl Stats {
    pub fn json(&self) -> String {
        crate::report::object(vec![
            ("unique_rows", self.unique_rows.to_string()),
            ("unique_row_words", self.unique_row_words.to_string()),
            ("relation_rows", self.relation_rows.to_string()),
            ("relation_row_words", self.relation_row_words.to_string()),
            ("indexed_relations", self.indexed_relations.to_string()),
            ("intern_lookups", self.intern_lookups.to_string()),
            ("intern_hits", self.intern_hits.to_string()),
            ("hash_word_visits", self.hash_word_visits.to_string()),
            ("probe_visits", self.probe_visits.to_string()),
            (
                "equality_word_visits",
                self.equality_word_visits.to_string(),
            ),
            ("rehashes", self.rehashes.to_string()),
            ("table_slots", self.table_slots.to_string()),
            ("peak_queue", self.peak_queue.to_string()),
            ("expand_ns", self.expand_ns.to_string()),
        ])
    }
}

#[derive(Default)]
pub(crate) struct Pool {
    pub(crate) rows: Vec<Vec<u64>>,
    hashes: Vec<u64>,
    table: Vec<Option<RowId>>,
    pub(crate) stats: Stats,
}
impl Pool {
    fn hash(&mut self, row: &[u64], work: &mut Work) -> R<u64> {
        work.charge(row.len() + 1)?;
        self.stats.hash_word_visits += row.len();
        let mut hash = 0xcbf29ce484222325u64;
        for word in row {
            hash = (hash ^ word).wrapping_mul(0x100000001b3);
            hash ^= hash >> 32;
        }
        Ok(hash)
    }
    fn probe(&mut self, row: &[u64], hash: u64, work: &mut Work) -> R<(usize, Option<RowId>)> {
        let mask = self.table.len() - 1;
        let mut slot = hash as usize & mask;
        loop {
            work.charge(1)?;
            self.stats.probe_visits += 1;
            let Some(id) = self.table[slot] else {
                return Ok((slot, None));
            };
            if self.hashes[id] == hash {
                work.charge(1)?;
                let existing = &self.rows[id];
                let mut equal = existing.len() == row.len();
                if equal {
                    for (left, right) in existing.iter().zip(row) {
                        work.charge(1)?;
                        self.stats.equality_word_visits += 1;
                        if left != right {
                            equal = false;
                            break;
                        }
                    }
                }
                if equal {
                    return Ok((slot, Some(id)));
                }
            }
            slot = (slot + 1) & mask;
        }
    }
    fn grow(&mut self, work: &mut Work) -> R<()> {
        let next = self
            .table
            .len()
            .max(8)
            .checked_mul(2)
            .ok_or("ResourceLimit: intern table size")?;
        // All allocation/zeroing and old table release are charged.
        work.charge(next + self.table.len())?;
        let mut table = vec![None; next];
        for (id, hash) in self.hashes.iter().enumerate() {
            work.charge(1)?;
            let mut slot = *hash as usize & (next - 1);
            loop {
                work.charge(1)?;
                self.stats.probe_visits += 1;
                if table[slot].is_none() {
                    table[slot] = Some(id);
                    break;
                }
                slot = (slot + 1) & (next - 1);
            }
        }
        self.table = table;
        self.stats.rehashes += 1;
        self.stats.table_slots = next;
        Ok(())
    }
    pub(crate) fn intern(&mut self, row: &[u64], work: &mut Work) -> R<RowId> {
        let hash = self.hash(row, work)?;
        self.intern_hashed(row, hash, work)
    }
    pub(crate) fn intern_hashed(&mut self, row: &[u64], hash: u64, work: &mut Work) -> R<RowId> {
        work.charge(1)?;
        self.stats.intern_lookups += 1;
        if self.table.is_empty() {
            self.grow(work)?;
        }
        let (mut slot, existing) = self.probe(row, hash, work)?;
        if let Some(id) = existing {
            self.stats.intern_hits += 1;
            return Ok(id);
        }
        if self.rows.len() >= MAX_ROWS {
            return Err("ResourceLimit: intern unique rows".into());
        }
        if (self.rows.len() + 1) * 2 > self.table.len() {
            self.grow(work)?;
            slot = self.probe(row, hash, work)?.0;
        }
        work.charge(row.len() + 3)?;
        let id = self.rows.len();
        self.rows.push(row.to_vec());
        self.hashes.push(hash);
        self.table[slot] = Some(id);
        self.stats.unique_rows += 1;
        self.stats.unique_row_words += row.len();
        Ok(id)
    }
    fn release_work(&self, work: &mut Work) -> R<()> {
        work.charge(self.stats.unique_row_words + self.rows.len() * 2 + self.table.len())
    }
}

struct Relation {
    key: String,
    keep_empty: bool,
    members: BTreeSet<RowId>,
}
#[derive(Default)]
pub(crate) struct Indexed {
    keys: BTreeMap<String, usize>,
    relations: Vec<Relation>,
    pub(crate) total: usize,
}
impl Indexed {
    pub(crate) fn register(&mut self, key: String, keep_empty: bool, work: &mut Work) -> R<usize> {
        let comparisons = (self.keys.len() + 1).ilog2() as usize + 1;
        work.charge((key.len() + 1) * comparisons + key.len() + 1)?;
        if let Some(id) = self.keys.get(&key) {
            return Ok(*id);
        }
        if self.relations.len() >= MAX_RELATIONS {
            return Err("ResourceLimit: intern relation plan".into());
        }
        work.charge((key.len() + 1) * comparisons + key.len() + 3)?;
        let id = self.relations.len();
        self.keys.insert(key.clone(), id);
        self.relations.push(Relation {
            key,
            keep_empty,
            members: BTreeSet::new(),
        });
        Ok(id)
    }
    pub(crate) fn insert(&mut self, relation: usize, row: RowId, work: &mut Work) -> R<bool> {
        work.charge(1)?;
        let members = &mut self.relations[relation].members;
        let comparisons = (members.len() + 1).ilog2() as usize + 1;
        work.charge(comparisons)?;
        if members.contains(&row) {
            return Ok(false);
        }
        // Cross-point sharing never relaxes the complete relation-member cap.
        if self.total >= MAX_ROWS {
            return Err("ResourceLimit: intern total relation rows".into());
        }
        work.charge(comparisons + 1)?;
        members.insert(row);
        self.total += 1;
        Ok(true)
    }
    fn expand(self, pool: &Pool, work: &mut Work) -> R<Relations> {
        let mut result = Relations::new();
        work.charge(self.keys.keys().map(|k| k.len() + 1).sum())?;
        drop(self.keys);
        for relation in self.relations {
            work.charge(relation.key.len() + relation.members.len() + 1)?;
            if !relation.keep_empty && relation.members.is_empty() {
                continue;
            }
            let mut rows = BTreeSet::new();
            for id in relation.members {
                let row = &pool.rows[id];
                let comparisons = (rows.len() + 1).ilog2() as usize + 1;
                // Expansion is deliberate: the common checker and serializer
                // receive exactly the same complete representation as direct.
                work.charge((row.len() + 1) * comparisons + row.len() * 2 + 1)?;
                rows.insert(row.clone());
            }
            let comparisons = (result.len() + 1).ilog2() as usize + 1;
            work.charge((relation.key.len() + 1) * comparisons)?;
            result.insert(relation.key, rows);
        }
        Ok(result)
    }
}

enum Fault {
    Hidden,
    Visible(usize),
    Unsupported(usize),
}
struct TransferPlan {
    faults: Vec<Fault>,
    aborts: Vec<usize>,
}
struct EventPlan {
    transfers: Vec<TransferPlan>,
    exits: Vec<usize>,
}
fn plans(events: &[Event], relations: &mut Indexed, work: &mut Work) -> R<Vec<EventPlan>> {
    for p in 0..events.len() {
        let id = relations.register(format!("point:{p}"), true, work)?;
        if id != p {
            return Err("Mismatch: intern point index".into());
        }
    }
    let mut plans = Vec::new();
    for (p, event) in events.iter().enumerate() {
        work.charge(1)?;
        let mut transfers = Vec::new();
        for transfer in &event.transfers {
            let mut faults = Vec::new();
            let mut aborts = Vec::new();
            for (label, _) in &transfer.aborts {
                work.charge(label.len() + 1)?;
                aborts.push(relations.register(format!("abort:{p}:{label}"), false, work)?);
            }
            for (label, _) in &transfer.faults {
                work.charge(label.len() + 1)?;
                faults.push(if label == "projection-excludes-invalid-tag" {
                    Fault::Hidden
                } else if label == "unsupported-overlapping-incarnation" {
                    Fault::Unsupported(relations.register(
                        format!("unsupported:{p}:overlapping-incarnation"),
                        false,
                        work,
                    )?)
                } else {
                    Fault::Visible(relations.register(format!("fault:{p}:{label}"), false, work)?)
                });
            }
            transfers.push(TransferPlan { faults, aborts });
        }
        let mut exits = Vec::new();
        for (label, _) in &event.terminal {
            work.charge(label.len() + 1)?;
            exits.push(relations.register(format!("exit:{p}:{label}"), false, work)?);
        }
        plans.push(EventPlan { transfers, exits });
    }
    Ok(plans)
}

#[allow(clippy::too_many_arguments)]
pub fn solve(
    m: &TypedModule,
    f: &TypedFunction,
    limit: usize,
    diagnostic: &mut Telemetry,
    order: &str,
    gc: bool,
    representation: Representation,
) -> R<(Solution, Stats)> {
    let began = Instant::now();
    let mut work = Work::new(limit);
    let layout = extract::extract(m, f, &mut work)
        .inspect_err(|error| diagnostic.extraction_failure(error))?;
    let encoding = Encoding::from_function(&layout, m, f, representation, &mut work)?;
    let extract_ns = began.elapsed().as_nanos();
    let mut bdd = Bdd::new(encoding.physical.bits, limit);
    bdd.work = work;
    let began = Instant::now();
    let permutation = crate::ordering::make(&encoding.physical, order, &mut bdd.work)?;
    bdd.set_order(permutation)?;
    let solved = (|| {
        // This is the unchanged candidate formula compiler used by 071 direct.
        let mut events = Vec::new();
        for p in 0..layout.points.len() {
            bdd.set_context("intern-event", Some(p));
            let (block, ordinal) = layout.points[p];
            if ordinal == f.blocks[block].instructions.len() {
                let (t, terminal, next) =
                    symbolic::terminal(m, f, &layout, &encoding, &mut bdd, p, true)?;
                events.push(Event {
                    transfers: vec![t],
                    next,
                    terminal,
                });
            } else {
                let ins = &f.blocks[block].instructions[ordinal];
                let mut transfers = vec![symbolic::compile_ins(
                    m, f, &layout, &encoding, &mut bdd, p, false, true,
                )?];
                if let InstructionKind::Call { function, .. } = ins.kind
                    && m.functions[function.0 as usize].result_mode == PassingMode::Give
                {
                    transfers.push(symbolic::compile_ins(
                        m, f, &layout, &encoding, &mut bdd, p, true, true,
                    )?);
                }
                events.push(Event {
                    transfers,
                    next: vec![(p + 1, 1)],
                    terminal: vec![],
                });
            }
            symbolic::collect_boundary(&mut bdd, &events, &[], &BTreeMap::new(), gc)?;
            diagnostic.sample(&bdd, &events, &[], &BTreeMap::new())?;
        }
        let event_ns = began.elapsed().as_nanos();
        let began = Instant::now();
        let mut indexed = Indexed::default();
        let plans = plans(&events, &mut indexed, &mut bdd.work)?;
        let plan_ns = began.elapsed().as_nanos();
        let began = Instant::now();
        bdd.set_context("intern-initial", None);
        let start = symbolic::initial(m, f, &layout, &encoding, &mut bdd)?;
        let initial = bdd.rows(start, MAX_ROWS)?;
        let entry = layout.starts[f.entry.0 as usize];
        let n = events.len();
        let mut pool = Pool::default();
        let mut todo = VecDeque::new();
        for row in initial {
            let id = pool.intern(&row, &mut bdd.work)?;
            bdd.work.charge(row.len())?;
            if indexed.insert(entry, id, &mut bdd.work)? {
                bdd.work.charge(2)?;
                todo.push_back((entry, id));
            }
        }
        pool.stats.peak_queue = todo.len();
        let mut status = QueryStatus::Complete;
        while let Some((p, pre_id)) = todo.pop_front() {
            bdd.set_context("intern-fixed-point", Some(p));
            bdd.work.charge(2)?;
            for (transfer, plan) in events[p].transfers.iter().zip(&plans[p].transfers) {
                let pre = &pool.rows[pre_id];
                let mut valid = true;
                for ((_, predicate), relation) in transfer.aborts.iter().zip(&plan.aborts) {
                    if bdd.contains(*predicate, pre)? {
                        indexed.insert(*relation, pre_id, &mut bdd.work)?;
                    }
                }
                for ((_, predicate), fault) in transfer.faults.iter().zip(&plan.faults) {
                    if bdd.contains(*predicate, pre)? {
                        valid = false;
                        match fault {
                            Fault::Hidden => {}
                            Fault::Visible(relation) => {
                                indexed.insert(*relation, pre_id, &mut bdd.work)?;
                            }
                            Fault::Unsupported(relation) => {
                                status = QueryStatus::UnsupportedMultiplicity;
                                indexed.insert(*relation, pre_id, &mut bdd.work)?;
                            }
                        }
                    }
                }
                if !valid {
                    continue;
                }
                if bdd.contains(transfer.invariant_violation, pre)? {
                    return Err(format!(
                        "Mismatch: intern normal image violates token domain/uniqueness at point {p}"
                    ));
                }
                // 071's candidate direct kernel evaluates all RHS/free bits on
                // the same prestate. No oracle code is called here.
                for out in crate::direct::image(transfer, pre, &mut bdd)? {
                    let id = pool.intern(&out, &mut bdd.work)?;
                    for ((_, guard), relation) in events[p].terminal.iter().zip(&plans[p].exits) {
                        if bdd.contains(*guard, &out)? {
                            indexed.insert(*relation, id, &mut bdd.work)?;
                        }
                    }
                    for (target, guard) in &events[p].next {
                        if bdd.contains(*guard, &out)?
                            && indexed.insert(*target, id, &mut bdd.work)?
                        {
                            bdd.work.charge(2)?;
                            todo.push_back((*target, id));
                            pool.stats.peak_queue = pool.stats.peak_queue.max(todo.len());
                        }
                    }
                }
            }
        }
        diagnostic.sample(&bdd, &events, &[], &BTreeMap::new())?;
        pool.stats.relation_rows = indexed.total;
        pool.stats.relation_row_words = indexed.total * encoding.physical.bits.div_ceil(64);
        pool.stats.indexed_relations = indexed.relations.len();
        let expansion_began = Instant::now();
        let relations = indexed.expand(&pool, &mut bdd.work)?;
        pool.stats.expand_ns = expansion_began.elapsed().as_nanos();
        pool.release_work(&mut bdd.work)?;
        bdd.work.charge(
            events
                .iter()
                .map(|e| {
                    e.transfers
                        .iter()
                        .map(|t| t.assignments.len() * 3 + t.faults.len() * 2 + t.aborts.len() * 2)
                        .sum::<usize>()
                        + e.next.len()
                        + e.terminal.len() * 2
                })
                .sum(),
        )?;
        let stats = std::mem::take(&mut pool.stats);
        drop(pool);
        drop(plans);
        drop(events);
        let solve_ns = began.elapsed().as_nanos();
        Ok((
            relations,
            n,
            status,
            [extract_ns, event_ns, plan_ns, solve_ns],
            stats,
        ))
    })();
    diagnostic.finish(&bdd, solved.as_ref().err());
    let (relations, events, status, phase_ns, stats) = solved?;
    Ok((
        Solution {
            encoding,
            layout,
            bdd,
            relations: BTreeMap::new(),
            explicit: Some(relations),
            events,
            status,
            phase_ns,
        },
        stats,
    ))
}
