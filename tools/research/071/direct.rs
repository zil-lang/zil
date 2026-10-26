//! Exact whole-row transport using the candidate's prestate expression kernels.
//! No calls to the raw oracle, its extractor, evaluator, or fixed point.
use crate::{
    bdd::Bdd,
    budget::{MAX_RELATIONS, MAX_ROWS, R, Work},
    encoding::{Encoding, Representation},
    extract,
    schema::{QueryStatus, Relations},
    symbolic::{self, Event, Solution, Transfer},
    telemetry::Telemetry,
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Instant,
};
use zil_ir::{InstructionKind, PassingMode, TypedFunction, TypedModule};

fn put(row: &mut [u64], bit: usize, value: bool) {
    let mask = 1u64 << (bit % 64);
    if value {
        row[bit / 64] |= mask;
    } else {
        row[bit / 64] &= !mask;
    }
}

fn insert(
    relations: &mut Relations,
    key: String,
    row: &[u64],
    work: &mut Work,
    total: &mut usize,
) -> R<bool> {
    work.charge(key.len() + row.len() + 2)?;
    if !relations.contains_key(&key) && relations.len() >= MAX_RELATIONS {
        return Err("ResourceLimit: direct result relations".into());
    }
    let set = relations.entry(key).or_default();
    let comparisons = (set.len() + 1).ilog2() as usize + 1;
    work.charge(comparisons.saturating_mul(row.len() + 1))?;
    if set.contains(row) {
        return Ok(false);
    }
    if *total >= MAX_ROWS {
        return Err("ResourceLimit: direct total relation rows".into());
    }
    // Charge row copying, insertion comparisons, and eventual row release.
    work.charge(row.len().saturating_mul(2) + comparisons.saturating_mul(row.len() + 1))?;
    set.insert(row.to_vec());
    *total += 1;
    Ok(true)
}

pub(crate) fn image(transfer: &Transfer, pre: &[u64], bdd: &mut Bdd) -> R<Vec<Vec<u64>>> {
    bdd.work
        .charge(pre.len().saturating_mul(2) + transfer.assignments.len())?;
    let mut row = pre.to_vec();
    let mut unknown = Vec::new();
    for &(bit, rhs, free) in &transfer.assignments {
        // Both expressions see the SAME original row. Sequential updates would
        // incorrectly erase moved tokens or change later operand predicates.
        if bdd.contains(free, pre)? {
            unknown.push(bit);
            put(&mut row, bit, false);
        } else {
            put(&mut row, bit, bdd.contains(rhs, pre)?);
        }
    }
    let count = 1usize
        .checked_shl(unknown.len() as u32)
        .ok_or("ResourceLimit: direct free-bit expansion")?;
    if count > MAX_ROWS {
        return Err("ResourceLimit: direct free-bit expansion".into());
    }
    bdd.work
        .charge(count.saturating_mul(row.len().saturating_mul(2) + unknown.len() + 1))?;
    let mut result = Vec::with_capacity(count);
    for choice in 0..count {
        let mut out = row.clone();
        for (i, &bit) in unknown.iter().enumerate() {
            put(&mut out, bit, choice >> i & 1 != 0);
        }
        result.push(out);
    }
    Ok(result)
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
) -> R<Solution> {
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
        let mut events = Vec::new();
        for p in 0..layout.points.len() {
            bdd.set_context("direct-event", Some(p));
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
        bdd.set_context("direct-initial", None);
        let start = symbolic::initial(m, f, &layout, &encoding, &mut bdd)?;
        let initial = bdd.rows(start, MAX_ROWS)?;
        let entry = layout.starts[f.entry.0 as usize];
        let n = events.len();
        bdd.work.charge(n.saturating_mul(32))?;
        let mut relations = Relations::new();
        for p in 0..n {
            relations.insert(format!("point:{p}"), BTreeSet::new());
        }
        let mut total = 0;
        let mut todo = VecDeque::new();
        for row in initial {
            if insert(
                &mut relations,
                format!("point:{entry}"),
                &row,
                &mut bdd.work,
                &mut total,
            )? {
                bdd.work.charge(row.len() + 1)?;
                todo.push_back((entry, row));
            }
        }
        let mut status = QueryStatus::Complete;
        while let Some((p, pre)) = todo.pop_front() {
            bdd.set_context("direct-fixed-point", Some(p));
            bdd.work.charge(pre.len() + 1)?;
            for transfer in &events[p].transfers {
                let mut valid = true;
                for (label, predicate) in &transfer.aborts {
                    if bdd.contains(*predicate, &pre)? {
                        insert(
                            &mut relations,
                            format!("abort:{p}:{label}"),
                            &pre,
                            &mut bdd.work,
                            &mut total,
                        )?;
                    }
                }
                // Every fault predicate reads the full original prestate,
                // including predicates after an earlier fault became true.
                for (label, predicate) in &transfer.faults {
                    if bdd.contains(*predicate, &pre)? {
                        valid = false;
                        if label == "unsupported-overlapping-incarnation" {
                            status = QueryStatus::UnsupportedMultiplicity;
                            insert(
                                &mut relations,
                                format!("unsupported:{p}:overlapping-incarnation"),
                                &pre,
                                &mut bdd.work,
                                &mut total,
                            )?;
                        } else if label != "projection-excludes-invalid-tag" {
                            insert(
                                &mut relations,
                                format!("fault:{p}:{label}"),
                                &pre,
                                &mut bdd.work,
                                &mut total,
                            )?;
                        }
                    }
                }
                if !valid {
                    continue;
                }
                if bdd.contains(transfer.invariant_violation, &pre)? {
                    return Err(format!(
                        "Mismatch: direct normal image violates token domain/uniqueness at point {p}"
                    ));
                }
                for out in image(transfer, &pre, &mut bdd)? {
                    for (label, guard) in &events[p].terminal {
                        if bdd.contains(*guard, &out)? {
                            insert(
                                &mut relations,
                                format!("exit:{p}:{label}"),
                                &out,
                                &mut bdd.work,
                                &mut total,
                            )?;
                        }
                    }
                    for (target, guard) in &events[p].next {
                        if bdd.contains(*guard, &out)?
                            && insert(
                                &mut relations,
                                format!("point:{target}"),
                                &out,
                                &mut bdd.work,
                                &mut total,
                            )?
                        {
                            bdd.work.charge(out.len().saturating_mul(2) + 1)?;
                            todo.push_back((*target, out.clone()));
                        }
                    }
                }
            }
        }
        diagnostic.sample(&bdd, &events, &[], &BTreeMap::new())?;
        bdd.work.charge(
            events
                .iter()
                .map(|e| {
                    e.transfers
                        .iter()
                        .map(|t| t.assignments.len() * 3 + t.faults.len() + t.aborts.len())
                        .sum::<usize>()
                        + e.next.len()
                        + e.terminal.len()
                })
                .sum(),
        )?;
        drop(events);
        let solve_ns = began.elapsed().as_nanos();
        Ok((relations, n, status, [extract_ns, event_ns, 0, solve_ns]))
    })();
    diagnostic.finish(&bdd, solved.as_ref().err());
    let (relations, events, status, phase_ns) = solved?;
    Ok(Solution {
        encoding,
        layout,
        bdd,
        relations: BTreeMap::new(),
        explicit: Some(relations),
        events,
        status,
        phase_ns,
    })
}
